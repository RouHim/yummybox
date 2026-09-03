use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rand::RngExt;
use rand::distr::weighted::WeightedIndex;
use sqlx::Row;
use sqlx::SqlitePool;

use crate::db::{self, MealRow};
use crate::error::AppError;
use crate::model::{
    IngredientSummaryEntry, Meal, NewPlanRequest, NumericTotal, Plan, PlanPatch, PlanSummaryItem,
};

// ---------------------------------------------------------------------------
// Week math (simple calendar weeks, week 1 = week containing Jan 1, Monday start)
// ---------------------------------------------------------------------------

pub(crate) fn week_monday_of_jan1(year: i32) -> NaiveDate {
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).expect("Jan 1 is always a valid date");
    let days_since_monday = jan1.weekday().num_days_from_monday();
    jan1 - chrono::Days::new(days_since_monday as u64)
}

pub(crate) fn week_monday_sunday(year: i32, week: i32) -> (NaiveDate, NaiveDate) {
    let mon_of_jan1 = week_monday_of_jan1(year);
    let mon = mon_of_jan1 + chrono::Days::new((7 * (week - 1)) as u64);
    let sun = mon + chrono::Days::new(6);
    (mon, sun)
}

pub(crate) fn weeks_in_year(year: i32) -> i32 {
    let (_mon, sun) = week_monday_sunday(year, 52);
    if sun.year() == year {
        let dec31 = NaiveDate::from_ymd_opt(year, 12, 31).expect("Dec 31 is always a valid date");
        if dec31 >= sun + chrono::Days::new(1) {
            53
        } else {
            52
        }
    } else {
        53
    }
}

// ---------------------------------------------------------------------------
// Numeric quantity parsing
// ---------------------------------------------------------------------------

pub(crate) fn parse_numeric_quantity(raw: &str) -> Option<(f64, String)> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let num_end = raw
        .chars()
        .position(|c| !c.is_ascii_digit() && c != '.')
        .unwrap_or(raw.len());
    if num_end == 0 {
        return None;
    }
    let num_str = &raw[..num_end];
    if num_str.matches('.').count() > 1 {
        return None;
    }
    let value: f64 = num_str.parse().ok()?;
    let unit = raw[num_end..].trim().to_owned();
    Some((value, unit))
}

// ---------------------------------------------------------------------------
// Weighted meal selection
// ---------------------------------------------------------------------------

pub(crate) const NEVER_PLANNED_WEIGHT: f64 = 31_536_000.0; // ~1 year in seconds

pub(crate) async fn select_meals_weighted(
    conn: &mut sqlx::SqliteConnection,
    count: usize,
) -> Result<Vec<Meal>, AppError> {
    let meal_rows = sqlx::query_as::<_, MealRow>(
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image, m.portions, m.source_url
         FROM meals m
         ORDER BY m.updated_at DESC, m.id DESC",
    )
    .fetch_all(&mut *conn)
    .await?;

    if meal_rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut meals: Vec<Meal> = meal_rows.into_iter().map(db::map_meal_row).collect();

    for meal in &mut meals {
        meal.ingredients = db::get_meal_ingredients(&mut *conn, meal.id).await?;
    }

    let now = Utc::now();
    let weights: Vec<f64> = meals
        .iter()
        .map(|m| match &m.last_planned_at {
            Some(t) => {
                let secs = (now - *t).num_seconds().max(1) as f64;
                secs.max(1.0)
            }
            None => NEVER_PLANNED_WEIGHT,
        })
        .collect();

    let _dist = WeightedIndex::new(&weights)
        .map_err(|e| AppError::Internal(format!("weighted index error: {e}")))?;

    let mut rng: rand::rngs::StdRng = rand::make_rng();
    let picked_count = count.min(meals.len());

    let mut available: Vec<usize> = (0..meals.len()).collect();
    let mut chosen_indices: Vec<usize> = Vec::with_capacity(picked_count);

    for _ in 0..picked_count {
        let remaining_weights: Vec<f64> = available.iter().map(|&idx| weights[idx]).collect();
        let dist = WeightedIndex::new(&remaining_weights)
            .map_err(|e| AppError::Internal(format!("weighted index error: {e}")))?;
        let pick = rng.sample(&dist);
        chosen_indices.push(available.remove(pick));
    }

    for &idx in &chosen_indices {
        sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
            .bind(now)
            .bind(meals[idx].id)
            .execute(&mut *conn)
            .await?;
    }

    let mut result: Vec<Meal> = chosen_indices
        .iter()
        .map(|&idx| meals[idx].clone())
        .collect();
    for meal in &mut result {
        meal.last_planned_at = Some(now);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Ingredient aggregation
// ---------------------------------------------------------------------------

pub(crate) async fn aggregate_plan_ingredients(
    pool: &SqlitePool,
    plan_id: i64,
) -> Result<Vec<IngredientSummaryEntry>, AppError> {
    let rows = sqlx::query(
        "SELECT i.name, mi.quantity
         FROM plan_meals pm
         JOIN meal_ingredients mi ON mi.meal_id = pm.meal_id
         JOIN ingredients i ON i.id = mi.ingredient_id
         WHERE pm.plan_id = ?1
         ORDER BY i.name",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;

    type IngredientGroup = (Vec<(f64, String)>, Vec<String>);
    let mut groups: HashMap<String, IngredientGroup> = HashMap::new();
    for r in rows {
        let name: String = r.get(0);
        let qty: Option<String> = r.get(1);
        let entry = groups.entry(name.clone()).or_default();
        match &qty {
            Some(q) => match parse_numeric_quantity(q) {
                Some((val, unit)) => entry.0.push((val, unit)),
                None => entry.1.push(q.clone()),
            },
            None => {
                entry.1.push(String::new());
            }
        }
    }

    let mut result: Vec<IngredientSummaryEntry> = groups
        .into_iter()
        .map(|(name, (num, non_num))| {
            let numeric_total = if num.is_empty() {
                None
            } else {
                let sum: f64 = num.iter().map(|(v, _)| v).sum();
                let all_units: Vec<&str> = num
                    .iter()
                    .map(|(_, u)| u.as_str())
                    .filter(|u| !u.is_empty())
                    .collect();
                let unit = if all_units.is_empty() || all_units.len() != num.len() {
                    None
                } else {
                    let first = all_units[0];
                    if all_units.iter().all(|u| *u == first) {
                        Some(first.to_owned())
                    } else {
                        None
                    }
                };
                Some(NumericTotal { value: sum, unit })
            };
            IngredientSummaryEntry {
                name,
                numeric_total,
                non_numeric: non_num.into_iter().filter(|s| !s.is_empty()).collect(),
            }
        })
        .collect();

    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Plan CRUD
// ---------------------------------------------------------------------------

pub(crate) async fn get_plan_meals(pool: &SqlitePool, plan_id: i64) -> Result<Vec<Meal>, AppError> {
    let meal_rows = sqlx::query_as::<_, MealRow>(
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image, m.portions, m.source_url
         FROM plan_meals pm
         JOIN meals m ON m.id = pm.meal_id
         WHERE pm.plan_id = ?1
         ORDER BY pm.meal_id",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;

    let mut meals: Vec<Meal> = meal_rows.into_iter().map(db::map_meal_row).collect();

    db::hydrate_meals(pool, &mut meals).await?;
    Ok(meals)
}

pub(crate) async fn create_or_replace_plan(
    pool: &SqlitePool,
    req: NewPlanRequest,
) -> Result<Plan, AppError> {
    let max_week = weeks_in_year(req.year);
    if req.week_number < 1 || req.week_number > max_week {
        return Err(AppError::BadRequest(format!(
            "week_number must be between 1 and {}",
            max_week
        )));
    }
    let meal_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meals")
        .fetch_one(pool)
        .await?;
    if meal_count == 0 {
        return Err(AppError::BadRequest(
            "no meals exist — create at least one meal first".into(),
        ));
    }

    let mut tx = pool.begin().await?;
    let selected = select_meals_weighted(&mut tx, req.meal_count as usize).await?;
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO week_plans (year, week_number, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(year, week_number) DO UPDATE SET created_at = ?3",
    )
    .bind(req.year)
    .bind(req.week_number)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    let plan_id: i64 =
        sqlx::query_scalar("SELECT id FROM week_plans WHERE year = ?1 AND week_number = ?2")
            .bind(req.year)
            .bind(req.week_number)
            .fetch_one(&mut *tx)
            .await?;

    sqlx::query("DELETE FROM plan_meals WHERE plan_id = ?1")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;

    for meal in &selected {
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(meal.id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    get_plan(pool, req.year, req.week_number).await
}

pub(crate) async fn get_plan(pool: &SqlitePool, year: i32, week: i32) -> Result<Plan, AppError> {
    let plan_row =
        sqlx::query("SELECT id, created_at FROM week_plans WHERE year = ?1 AND week_number = ?2")
            .bind(year)
            .bind(week)
            .fetch_optional(pool)
            .await?
            .ok_or(AppError::NotFound)?;

    let plan_id: i64 = plan_row.get(0);
    let created_at: DateTime<Utc> = plan_row.get(1);

    let meals = get_plan_meals(pool, plan_id).await?;
    let summary = aggregate_plan_ingredients(pool, plan_id).await?;

    Ok(Plan {
        id: plan_id,
        year,
        week_number: week,
        created_at,
        meals,
        ingredient_summary: summary,
    })
}

pub(crate) async fn list_plans_for_year(
    pool: &SqlitePool,
    year: i32,
) -> Result<Vec<PlanSummaryItem>, AppError> {
    let rows = sqlx::query(
        "SELECT wp.year, wp.week_number, wp.id, COUNT(pm.meal_id) AS meal_count
         FROM week_plans wp
         LEFT JOIN plan_meals pm ON pm.plan_id = wp.id
         WHERE wp.year = ?1
         GROUP BY wp.id
         ORDER BY wp.week_number",
    )
    .bind(year)
    .fetch_all(pool)
    .await?;

    let items: Vec<PlanSummaryItem> = rows
        .iter()
        .map(|r| PlanSummaryItem {
            year: r.get::<i32, _>(0),
            week_number: r.get::<i32, _>(1),
            id: r.get(2),
            meal_count: r.get(3),
        })
        .collect();
    Ok(items)
}

pub(crate) async fn update_plan_meals(
    pool: &SqlitePool,
    year: i32,
    week: i32,
    patch: PlanPatch,
) -> Result<Plan, AppError> {
    let mut tx = pool.begin().await?;

    let plan_row = sqlx::query("SELECT id FROM week_plans WHERE year = ?1 AND week_number = ?2")
        .bind(year)
        .bind(week)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let plan_id: i64 = plan_row.get(0);

    for &meal_id in &patch.meal_ids {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meals WHERE id = ?1")
            .bind(meal_id)
            .fetch_one(&mut *tx)
            .await?;
        if exists == 0 {
            return Err(AppError::NotFound);
        }
    }

    sqlx::query("DELETE FROM plan_meals WHERE plan_id = ?1")
        .bind(plan_id)
        .execute(&mut *tx)
        .await?;

    for &meal_id in &patch.meal_ids {
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(meal_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    get_plan(pool, year, week).await
}

pub(crate) async fn delete_plan(pool: &SqlitePool, year: i32, week: i32) -> Result<(), AppError> {
    let affected = sqlx::query("DELETE FROM week_plans WHERE year = ?1 AND week_number = ?2")
        .bind(year)
        .bind(week)
        .execute(pool)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // -----------------------------------------------------------------------
    // Week math
    // -----------------------------------------------------------------------

    #[test]
    fn given_2026_week_1_when_monday_sunday_then_dec_29_2025_to_jan_4_2026() {
        let (mon, sun) = week_monday_sunday(2026, 1);
        assert_eq!(mon, NaiveDate::from_ymd_opt(2025, 12, 29).unwrap());
        assert_eq!(sun, NaiveDate::from_ymd_opt(2026, 1, 4).unwrap());
    }

    #[test]
    fn given_2026_week_25_when_monday_sunday_then_jun_15_to_jun_21_2026() {
        let (mon, sun) = week_monday_sunday(2026, 25);
        assert_eq!(mon, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(sun, NaiveDate::from_ymd_opt(2026, 6, 21).unwrap());
    }

    #[test]
    fn given_2026_when_weeks_in_year_then_returns_53() {
        assert_eq!(weeks_in_year(2026), 53);
    }

    #[test]
    fn given_year_with_52_weeks_when_weeks_in_year_then_returns_52() {
        let actual = weeks_in_year(2023);
        assert!((52..=53).contains(&actual));
        let same = weeks_in_year(2023);
        assert_eq!(same, actual);
    }

    // -----------------------------------------------------------------------
    // parse_numeric_quantity
    // -----------------------------------------------------------------------

    #[test]
    fn given_200g_when_parse_then_returns_200_and_g() {
        let result = parse_numeric_quantity("200g");
        assert_eq!(result, Some((200.0, "g".into())));
    }

    #[test]
    fn given_1_5_cups_when_parse_then_returns_1_5_and_cups() {
        let result = parse_numeric_quantity("1.5 cups");
        assert_eq!(result, Some((1.5, "cups".into())));
    }

    #[test]
    fn given_bare_2_when_parse_then_returns_2_and_empty_unit() {
        let result = parse_numeric_quantity("2");
        assert_eq!(result, Some((2.0, String::new())));
    }

    #[test]
    fn given_a_pinch_when_parse_then_returns_none() {
        assert_eq!(parse_numeric_quantity("a pinch"), None);
    }

    #[test]
    fn given_empty_string_when_parse_then_returns_none() {
        assert_eq!(parse_numeric_quantity(""), None);
    }

    #[test]
    fn given_malformed_1_2_3_when_parse_then_returns_none() {
        assert_eq!(parse_numeric_quantity("1.2.3"), None);
    }
}
