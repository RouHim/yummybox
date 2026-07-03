#![allow(clippy::explicit_auto_deref)]

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rand::RngExt;
use rand::distr::weighted::WeightedIndex;
use sqlx::Row;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::model::{
    IngredientQuantity, IngredientSummaryEntry, Meal, MealPatch, NewIngredientLine, NewMeal,
    NewPlanRequest, NumericTotal, Plan, PlanPatch, PlanSummaryItem,
};

// ---------------------------------------------------------------------------
// Private row structs for query_as
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct MealRow {
    id: i64,
    name: String,
    instructions: String,
    last_planned_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    has_image: bool,
}

/// Convert a [`MealRow`] into a [`Meal`] (without ingredients — those are
/// loaded separately by [`get_meal_ingredients`] or [`hydrate_meals`]).
fn map_meal_row(row: MealRow) -> Meal {
    Meal {
        id: row.id,
        name: row.name,
        instructions: row.instructions,
        ingredients: Vec::new(),
        last_planned_at: row.last_planned_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        has_image: row.has_image,
    }
}

#[derive(sqlx::FromRow)]
struct IngredientRow {
    id: i64,
    name: String,
}

// ---------------------------------------------------------------------------
// Database initialisation
// ---------------------------------------------------------------------------

pub async fn init_db(path: &Path) -> Result<SqlitePool, AppError> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sqlx::sqlite::SqliteSynchronous::Normal),
        )
        .await?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Internal(format!("migration error: {e}")))?;
    Ok(pool)
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

pub fn normalize_ingredient_name(name: &str) -> String {
    name.split_whitespace()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn normalize_meal_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

pub fn validate_meal(
    name: &str,
    ingredients: &[NewIngredientLine],
    instructions: &str,
) -> Result<(), AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("name must not be empty".into()));
    }
    if name.len() > 200 {
        return Err(AppError::Validation(format!(
            "name must be at most 200 characters, got {}",
            name.len()
        )));
    }
    let instructions_trim = instructions.trim();
    if instructions_trim.is_empty() {
        return Err(AppError::Validation(
            "instructions must not be empty".into(),
        ));
    }
    if instructions_trim.len() > 20000 {
        return Err(AppError::Validation(format!(
            "instructions must be at most 20000 characters, got {}",
            instructions_trim.len()
        )));
    }
    if ingredients.is_empty() {
        return Err(AppError::Validation(
            "at least one ingredient line is required".into(),
        ));
    }
    if ingredients.len() > 100 {
        return Err(AppError::Validation(format!(
            "at most 100 ingredient lines allowed, got {}",
            ingredients.len()
        )));
    }
    for line in ingredients {
        let norm = normalize_ingredient_name(&line.name);
        if norm.is_empty() {
            return Err(AppError::Validation(
                "ingredient name must not be blank".into(),
            ));
        }
        if norm.len() > 100 {
            return Err(AppError::Validation(format!(
                "ingredient name must be at most 100 characters, got {}",
                norm.len()
            )));
        }
        if let Some(ref qty) = line.quantity {
            if qty.len() > 50 {
                return Err(AppError::Validation(format!(
                    "ingredient quantity must be at most 50 characters, got {}",
                    qty.len()
                )));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Ingredient helpers (all take &mut SqliteConnection for use within txs)
// ---------------------------------------------------------------------------

pub async fn upsert_ingredients(
    conn: &mut sqlx::SqliteConnection,
    names: &[String],
) -> Result<Vec<(i64, String)>, AppError> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::with_capacity(names.len());
    for name in names {
        // Case-insensitive lookup first — preserve first-seen casing
        let existing = sqlx::query_as::<_, IngredientRow>(
            "SELECT id, name FROM ingredients WHERE name = ?1 COLLATE NOCASE",
        )
        .bind(name.as_str())
        .fetch_optional(&mut *conn)
        .await?;

        if let Some(row) = existing {
            result.push((row.id, row.name));
        } else {
            sqlx::query("INSERT INTO ingredients (name) VALUES (?1)")
                .bind(name.as_str())
                .execute(&mut *conn)
                .await?;
            let row = sqlx::query_as::<_, IngredientRow>(
                "SELECT id, name FROM ingredients WHERE name = ?1 COLLATE NOCASE",
            )
            .bind(name.as_str())
            .fetch_one(&mut *conn)
            .await?;
            result.push((row.id, row.name));
        }
    }
    Ok(result)
}

pub async fn set_meal_ingredients(
    conn: &mut sqlx::SqliteConnection,
    meal_id: i64,
    lines: &[NewIngredientLine],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM meal_ingredients WHERE meal_id = ?1")
        .bind(meal_id)
        .execute(&mut *conn)
        .await?;

    let mut names: Vec<String> = Vec::with_capacity(lines.len());
    let mut qty_for: Vec<Option<String>> = Vec::with_capacity(lines.len());
    for line in lines {
        let norm = normalize_ingredient_name(&line.name);
        if norm.is_empty() {
            continue;
        }
        names.push(norm);
        qty_for.push(line.quantity.clone());
    }
    if names.is_empty() {
        return Ok(());
    }
    let inserted = upsert_ingredients(&mut *conn, &names).await?;
    for ((ing_id, _name), qty) in inserted.iter().zip(qty_for.iter()) {
        let qty_val = qty.as_deref();
        sqlx::query(
            "INSERT INTO meal_ingredients (meal_id, ingredient_id, quantity) VALUES (?1, ?2, ?3)",
        )
        .bind(meal_id)
        .bind(ing_id)
        .bind(qty_val)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn get_meal_ingredients(
    conn: &mut sqlx::SqliteConnection,
    meal_id: i64,
) -> Result<Vec<IngredientQuantity>, AppError> {
    let rows = sqlx::query_as::<_, IngredientQuantity>(
        "SELECT i.name, mi.quantity
         FROM meal_ingredients mi
         JOIN ingredients i ON i.id = mi.ingredient_id
         WHERE mi.meal_id = ?1
         ORDER BY i.name",
    )
    .bind(meal_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

pub async fn hydrate_meals(pool: &SqlitePool, meals: &mut [Meal]) -> Result<(), AppError> {
    let mut conn = pool.acquire().await?;
    for meal in meals.iter_mut() {
        meal.ingredients = get_meal_ingredients(&mut *conn, meal.id).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Meal CRUD
// ---------------------------------------------------------------------------

pub async fn list_meals(pool: &SqlitePool, search: Option<&str>) -> Result<Vec<Meal>, AppError> {
    let search_term = search.map(str::trim).filter(|s| !s.is_empty());

    let meal_rows: Vec<MealRow> = match &search_term {
        Some(term) => {
            let pattern = format!("%{}%", term.to_lowercase());
            sqlx::query_as::<_, MealRow>(
                "SELECT DISTINCT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image
                 FROM meals m
                 LEFT JOIN meal_ingredients mi ON mi.meal_id = m.id
                 LEFT JOIN ingredients i ON i.id = mi.ingredient_id
                 WHERE LOWER(m.name) LIKE ?1 OR LOWER(i.name) LIKE ?1
                 ORDER BY m.updated_at DESC, m.id DESC",
            )
            .bind(&pattern)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, MealRow>(
                "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image
                 FROM meals m
                 ORDER BY m.updated_at DESC, m.id DESC",
            )
            .fetch_all(pool)
            .await?
        }
    };

    let mut meals: Vec<Meal> = meal_rows.into_iter().map(map_meal_row).collect();

    hydrate_meals(pool, &mut meals).await?;
    Ok(meals)
}

pub async fn find_meal(pool: &SqlitePool, id: i64) -> Result<Meal, AppError> {
    let row = sqlx::query_as::<_, MealRow>(
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image
         FROM meals m WHERE m.id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let mut meal = map_meal_row(row);
    let mut conn = pool.acquire().await?;
    meal.ingredients = get_meal_ingredients(&mut *conn, meal.id).await?;
    Ok(meal)
}

pub async fn insert_meal(
    pool: &SqlitePool,
    new: NewMeal,
    image: ImageChange<'_>,
) -> Result<Meal, AppError> {
    validate_meal(&new.name, &new.ingredients, &new.instructions)?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    let trimmed_name = new.name.trim();
    let id: (i64,) = sqlx::query_as(
        "INSERT INTO meals (name, instructions, last_planned_at, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, ?4)
         RETURNING id",
    )
    .bind(trimmed_name)
    .bind(&new.instructions)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;

    set_meal_ingredients(&mut *tx, id.0, &new.ingredients).await?;

    if let ImageChange::Set(jpeg_bytes) = image {
        set_meal_image(&mut *tx, id.0, jpeg_bytes).await?;
    }

    tx.commit().await?;

    find_meal(pool, id.0).await
}
pub async fn update_meal(
    pool: &SqlitePool,
    id: i64,
    patch: MealPatch,
    image: ImageChange<'_>,
) -> Result<Meal, AppError> {
    validate_meal(&patch.name, &patch.ingredients, &patch.instructions)?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    let trimmed_name = patch.name.trim();
    let affected =
        sqlx::query("UPDATE meals SET name = ?1, instructions = ?2, updated_at = ?3 WHERE id = ?4")
            .bind(trimmed_name)
            .bind(&patch.instructions)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    set_meal_ingredients(&mut *tx, id, &patch.ingredients).await?;

    match image {
        ImageChange::Set(jpeg_bytes) => {
            set_meal_image(&mut *tx, id, jpeg_bytes).await?;
        }
        ImageChange::Clear => {
            clear_meal_image(&mut *tx, id).await?;
        }
        ImageChange::Keep => {}
    }

    tx.commit().await?;

    find_meal(pool, id).await
}

pub async fn delete_meal(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let affected = sqlx::query("DELETE FROM meals WHERE id = ?1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    sqlx::query(
        "DELETE FROM ingredients WHERE id NOT IN (SELECT DISTINCT ingredient_id FROM meal_ingredients)",
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn meals_count(pool: &SqlitePool) -> Result<i64, AppError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM meals")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Check whether a meal with the given name already exists (case-insensitive,
/// whitespace-collapsed comparison). When `exclude_id` is `Some(id)`, that
/// meal is ignored — used during update to allow renaming to the same name
/// with different casing.
pub async fn meal_name_exists(
    pool: &SqlitePool,
    name: &str,
    exclude_id: Option<i64>,
) -> Result<bool, AppError> {
    let target = normalize_meal_name(name);
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM meals WHERE ?1 IS NULL OR id != ?1")
            .bind(exclude_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.iter().any(|(_, n)| normalize_meal_name(n) == target))
}

// ---------------------------------------------------------------------------
// Image helpers
// ---------------------------------------------------------------------------

/// Describes what to do with a meal's image during create or update.
pub enum ImageChange<'a> {
    /// Leave the image as-is (create with no image, or update keeping existing).
    Keep,
    /// Set or replace the image with these already-converted JPEG bytes.
    Set(&'a [u8]),
    /// Remove the image (update only).
    Clear,
}

/// Set the image BLOB and content-type for a meal within an active transaction.
pub async fn set_meal_image(
    conn: &mut sqlx::SqliteConnection,
    id: i64,
    jpeg_bytes: &[u8],
) -> Result<(), AppError> {
    let now = Utc::now();
    let affected = sqlx::query(
        "UPDATE meals SET image = ?1, image_content_type = ?2, updated_at = ?3 WHERE id = ?4",
    )
    .bind(jpeg_bytes)
    .bind(crate::image::JPEG_CONTENT_TYPE)
    .bind(now)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Clear the image BLOB and content-type for a meal within an active transaction.
pub async fn clear_meal_image(conn: &mut sqlx::SqliteConnection, id: i64) -> Result<(), AppError> {
    let now = Utc::now();
    let affected = sqlx::query(
        "UPDATE meals SET image = NULL, image_content_type = NULL, updated_at = ?1 WHERE id = ?2",
    )
    .bind(now)
    .bind(id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Fetch the image bytes and content-type for a meal.
/// Returns `None` when the meal has no image or doesn't exist.
pub async fn find_meal_image(
    pool: &SqlitePool,
    id: i64,
) -> Result<Option<(Vec<u8>, String)>, AppError> {
    let row = sqlx::query(
        "SELECT image, image_content_type FROM meals WHERE id = ?1 AND image IS NOT NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    match row {
        Some(r) => {
            let bytes: Vec<u8> = r.get(0);
            let ct: String = r.get(1);
            Ok(Some((bytes, ct)))
        }
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Week math (simple calendar weeks, week 1 = week containing Jan 1, Monday start)
// ---------------------------------------------------------------------------

pub fn week_monday_of_jan1(year: i32) -> NaiveDate {
    let jan1 = NaiveDate::from_ymd_opt(year, 1, 1).unwrap();
    let days_since_monday = jan1.weekday().num_days_from_monday();
    jan1 - chrono::Days::new(days_since_monday as u64)
}

pub fn week_monday_sunday(year: i32, week: i32) -> (NaiveDate, NaiveDate) {
    let mon_of_jan1 = week_monday_of_jan1(year);
    let mon = mon_of_jan1 + chrono::Days::new((7 * (week - 1)) as u64);
    let sun = mon + chrono::Days::new(6);
    (mon, sun)
}

pub fn weeks_in_year(year: i32) -> i32 {
    let (_mon, sun) = week_monday_sunday(year, 52);
    if sun.year() == year {
        let dec31 = NaiveDate::from_ymd_opt(year, 12, 31).unwrap();
        if dec31 >= sun + chrono::Days::new(1) {
            53
        } else {
            52
        }
    } else {
        53
    }
}

#[allow(dead_code)]
pub fn week_of_date(d: NaiveDate) -> (i32, i32) {
    let year = d.year();
    let mon_of_jan1 = week_monday_of_jan1(year);
    let days = (d - mon_of_jan1).num_days();
    if days < 0 {
        week_of_date(d + chrono::Days::new(7))
    } else {
        let week = (days / 7) as i32 + 1;
        (year, week)
    }
}

// ---------------------------------------------------------------------------
// Numeric quantity parsing
// ---------------------------------------------------------------------------

pub fn parse_numeric_quantity(raw: &str) -> Option<(f64, String)> {
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

pub const NEVER_PLANNED_WEIGHT: f64 = 31_536_000.0; // ~1 year in seconds

pub async fn select_meals_weighted(
    conn: &mut sqlx::SqliteConnection,
    count: usize,
) -> Result<Vec<Meal>, AppError> {
    let meal_rows = sqlx::query_as::<_, MealRow>(
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image
         FROM meals m
         ORDER BY m.updated_at DESC, m.id DESC",
    )
    .fetch_all(&mut *conn)
    .await?;

    if meal_rows.is_empty() {
        return Ok(Vec::new());
    }

    let mut meals: Vec<Meal> = meal_rows.into_iter().map(map_meal_row).collect();

    for meal in &mut meals {
        meal.ingredients = get_meal_ingredients(&mut *conn, meal.id).await?;
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

pub async fn aggregate_plan_ingredients(
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

    #[allow(clippy::type_complexity)]
    let mut groups: HashMap<String, (Vec<(f64, String)>, Vec<String>)> = HashMap::new();
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

pub async fn get_plan_meals(pool: &SqlitePool, plan_id: i64) -> Result<Vec<Meal>, AppError> {
    let meal_rows = sqlx::query_as::<_, MealRow>(
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image
         FROM plan_meals pm
         JOIN meals m ON m.id = pm.meal_id
         WHERE pm.plan_id = ?1
         ORDER BY pm.meal_id",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await?;

    let mut meals: Vec<Meal> = meal_rows.into_iter().map(map_meal_row).collect();

    hydrate_meals(pool, &mut meals).await?;
    Ok(meals)
}

pub async fn create_or_replace_plan(
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
    let selected = select_meals_weighted(&mut *tx, req.meal_count as usize).await?;
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

pub async fn get_plan(pool: &SqlitePool, year: i32, week: i32) -> Result<Plan, AppError> {
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

pub async fn list_plans_for_year(
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

pub async fn update_plan_meals(
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

pub async fn delete_plan(pool: &SqlitePool, year: i32, week: i32) -> Result<(), AppError> {
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
    use std::time::Duration;

    use super::*;

    async fn setup_db() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = init_db(&db_path).await.expect("init_db");
        (pool, dir)
    }

    async fn insert_test_meal(
        pool: &SqlitePool,
        name: &str,
        ingredients: &[(&str, Option<&str>)],
    ) -> Meal {
        insert_meal(
            pool,
            NewMeal {
                name: name.into(),
                ingredients: ingredients
                    .iter()
                    .map(|(n, q)| NewIngredientLine {
                        name: n.to_string(),
                        quantity: q.map(String::from),
                    })
                    .collect(),
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await
        .expect("insert_test_meal")
    }

    // -----------------------------------------------------------------------
    // meals_count
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_empty_db_when_meals_count_then_returns_zero() {
        let (pool, _dir) = setup_db().await;
        assert_eq!(meals_count(&pool).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn given_one_meal_inserted_when_meals_count_then_returns_one() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "Test", &[("salt", None)]).await;
        assert_eq!(meals_count(&pool).await.unwrap(), 1);
    }

    // -----------------------------------------------------------------------
    // pool resilience: cancelled begin
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_cancelled_begin_when_pool_reused_then_no_worker_crash() {
        use std::future::poll_fn;
        use std::pin::pin;
        use std::task::Poll;

        let (pool, _dir) = setup_db().await;

        // Poll the begin future once to start the BEGIN command, then drop it.
        // In sqlx 0.8.x this could leave the worker in a corrupted state
        // (Transaction guard not yet constructed, so Drop couldn't roll back).
        // In sqlx 0.9.0 the guard is created before begin(), so Drop safely rolls back.
        // Poll once then drop — cancellation exercises Transaction::Drop rollback.
        {
            let mut begin_fut = pin!(pool.begin());
            let _ = poll_fn(|cx| {
                let _ = begin_fut.as_mut().poll(cx);
                Poll::Ready(())
            })
            .await;
        } // begin_fut dropped here — cancels the pending BEGIN
        // Pool should still be usable — no WorkerCrashed
        let count = meals_count(&pool)
            .await
            .expect("pool should survive cancelled begin");
        assert_eq!(count, 0);
    }

    // -----------------------------------------------------------------------
    // normalize_ingredient_name
    // -----------------------------------------------------------------------

    #[test]
    fn given_name_with_mixed_case_and_whitespace_when_normalize_then_preserves_case_and_collapses_internal_whitespace()
     {
        assert_eq!(normalize_ingredient_name(" Salt "), "Salt");
        assert_eq!(
            normalize_ingredient_name("  Black   Pepper  "),
            "Black Pepper"
        );
    }

    #[test]
    fn given_name_with_only_whitespace_when_normalize_then_returns_empty_string() {
        assert_eq!(normalize_ingredient_name("   "), "");
    }

    // -----------------------------------------------------------------------
    // normalize_meal_name
    // -----------------------------------------------------------------------

    #[test]
    fn given_name_with_mixed_case_and_whitespace_when_normalize_then_lowercases_and_collapses() {
        assert_eq!(normalize_meal_name("Pancakes"), "pancakes");
        assert_eq!(normalize_meal_name("  PANCAKES  "), "pancakes");
        assert_eq!(normalize_meal_name("  Pan   Cakes  "), "pan cakes");
    }

    #[test]
    fn given_name_empty_or_whitespace_when_normalize_then_returns_empty() {
        assert_eq!(normalize_meal_name(""), "");
        assert_eq!(normalize_meal_name("   "), "");
    }

    #[test]
    fn given_unicode_ingredient_name_when_normalize_then_preserves_case_and_collapses_whitespace() {
        assert_eq!(
            normalize_ingredient_name("  Thüringer   Rostbratwurst  "),
            "Thüringer Rostbratwurst"
        );
        assert_eq!(normalize_ingredient_name("grüne Kresse"), "grüne Kresse");
    }

    // -----------------------------------------------------------------------
    // meal_name_exists
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_existing_meal_when_check_duplicate_name_case_insensitive_then_returns_true() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "Risotto", &[("rice", None)]).await;
        assert!(meal_name_exists(&pool, "RISOTTO", None).await.unwrap());
        assert!(meal_name_exists(&pool, "  risotto  ", None).await.unwrap());
    }

    #[tokio::test]
    async fn given_no_meals_when_check_duplicate_name_then_returns_false() {
        let (pool, _dir) = setup_db().await;
        assert!(!meal_name_exists(&pool, "Anything", None).await.unwrap());
    }

    #[tokio::test]
    async fn given_exclude_id_when_check_own_name_then_returns_false() {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "Pasta", &[("noodles", None)]).await;
        // Excluding the meal itself → not a duplicate
        assert!(
            !meal_name_exists(&pool, "pasta", Some(meal.id))
                .await
                .unwrap()
        );
        // Excluding a different meal → still a duplicate
        assert!(
            meal_name_exists(&pool, "pasta", Some(meal.id + 999))
                .await
                .unwrap()
        );
    }

    // -----------------------------------------------------------------------
    // validate_meal
    // -----------------------------------------------------------------------

    #[test]
    fn given_no_ingredient_lines_when_insert_meal_then_validation_error() {
        let result = validate_meal("x", &[], "valid instructions");
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("ingredient")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_ingredient_line_with_empty_trimmed_name_when_insert_meal_then_validation_error() {
        let result = validate_meal(
            "x",
            &[NewIngredientLine {
                name: "   ".into(),
                quantity: None,
            }],
            "valid instructions",
        );
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_ingredient_name_above_100_chars_when_insert_meal_then_validation_error() {
        let long_name = "a".repeat(101);
        let result = validate_meal(
            "x",
            &[NewIngredientLine {
                name: long_name,
                quantity: None,
            }],
            "valid instructions",
        );
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_ingredient_quantity_above_50_chars_when_insert_meal_then_validation_error() {
        let long_qty = "a".repeat(51);
        let result = validate_meal(
            "x",
            &[NewIngredientLine {
                name: "valid".into(),
                quantity: Some(long_qty),
            }],
            "valid instructions",
        );
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("quantity")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_above_100_ingredient_lines_when_insert_meal_then_validation_error() {
        let lines: Vec<NewIngredientLine> = (0..101)
            .map(|i| NewIngredientLine {
                name: format!("ingredient {i}"),
                quantity: None,
            })
            .collect();
        let result = validate_meal("x", &lines, "valid instructions");
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("100")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_empty_instructions_when_validate_meal_then_error() {
        let result = validate_meal(
            "x",
            &[NewIngredientLine {
                name: "salt".into(),
                quantity: None,
            }],
            "",
        );
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("instructions")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn given_instructions_above_20000_chars_when_validate_meal_then_error() {
        let long_instructions = "a".repeat(20001);
        let result = validate_meal(
            "x",
            &[NewIngredientLine {
                name: "salt".into(),
                quantity: None,
            }],
            &long_instructions,
        );
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("20000")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_name_at_exactly_200_chars_when_insert_meal_then_succeeds() {
        let (pool, _dir) = setup_db().await;
        let name = "a".repeat(200);
        let result = insert_meal(
            &pool,
            NewMeal {
                name: name.clone(),
                ingredients: vec![NewIngredientLine {
                    name: "x".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        let meal = result.expect("should succeed");
        assert_eq!(meal.name, name.as_str());
    }

    #[tokio::test]
    async fn given_name_at_201_chars_when_insert_meal_then_returns_validation_error() {
        let (pool, _dir) = setup_db().await;
        let name = "a".repeat(201);
        let result = insert_meal(
            &pool,
            NewMeal {
                name,
                ingredients: vec![NewIngredientLine {
                    name: "x".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_empty_name_when_insert_meal_then_returns_validation_error() {
        let (pool, _dir) = setup_db().await;
        let result = insert_meal(
            &pool,
            NewMeal {
                name: "".into(),
                ingredients: vec![NewIngredientLine {
                    name: "x".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_whitespace_only_name_when_insert_meal_then_returns_validation_error() {
        let (pool, _dir) = setup_db().await;
        let result = insert_meal(
            &pool,
            NewMeal {
                name: "   ".into(),
                ingredients: vec![NewIngredientLine {
                    name: "x".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        match &result {
            Err(AppError::Validation(msg)) => assert!(msg.contains("name")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // upsert_ingredients
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_new_ingredient_names_when_upsert_then_returns_inserted_rows_in_input_order() {
        let (pool, _dir) = setup_db().await;
        let names: Vec<String> = vec!["salt".into(), "pepper".into()];
        let mut conn = pool.acquire().await.unwrap();
        let result = upsert_ingredients(&mut *conn, &names)
            .await
            .expect("upsert");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "salt");
        assert_eq!(result[1].1, "pepper");
    }

    #[tokio::test]
    async fn given_existing_ingredient_names_when_upsert_then_returns_existing_ids_no_duplicates() {
        let (pool, _dir) = setup_db().await;
        let names: Vec<String> = vec!["salt".into()];
        let mut conn = pool.acquire().await.unwrap();
        let first = upsert_ingredients(&mut *conn, &names)
            .await
            .expect("upsert 1");
        let second = upsert_ingredients(&mut *conn, &names)
            .await
            .expect("upsert 2");
        assert_eq!(first[0].0, second[0].0);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ingredients")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn given_empty_input_when_upsert_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = upsert_ingredients(&mut *conn, &[]).await.expect("upsert");
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // set_meal_ingredients / get_meal_ingredients
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meal_with_existing_ingredients_when_set_meal_ingredients_then_replaces_with_new_set()
     {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "Test", &[("old", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        set_meal_ingredients(
            &mut *conn,
            meal.id,
            &[NewIngredientLine {
                name: "new".into(),
                quantity: None,
            }],
        )
        .await
        .expect("set");

        let ings = get_meal_ingredients(&mut *conn, meal.id)
            .await
            .expect("get");
        assert_eq!(ings.len(), 1);
        assert_eq!(ings[0].name, "new");
    }

    #[tokio::test]
    async fn given_ingredient_line_with_no_quantity_when_set_meal_ingredients_then_stores_null_quantity()
     {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        set_meal_ingredients(
            &mut *conn,
            meal.id,
            &[NewIngredientLine {
                name: "salt".into(),
                quantity: None,
            }],
        )
        .await
        .expect("set");

        let ings = get_meal_ingredients(&mut *conn, meal.id)
            .await
            .expect("get");
        assert_eq!(ings[0].quantity, None);
    }

    #[tokio::test]
    async fn given_ingredient_line_with_blank_name_when_set_meal_ingredients_then_skips_that_line()
    {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        set_meal_ingredients(
            &mut *conn,
            meal.id,
            &[
                NewIngredientLine {
                    name: "   ".into(),
                    quantity: None,
                },
                NewIngredientLine {
                    name: "kept".into(),
                    quantity: None,
                },
            ],
        )
        .await
        .expect("set");

        let ings = get_meal_ingredients(&mut *conn, meal.id)
            .await
            .expect("get");
        assert_eq!(ings.len(), 1);
        assert_eq!(ings[0].name, "kept");
    }

    #[tokio::test]
    async fn given_meal_with_ingredients_when_get_meal_ingredients_then_returns_ingredient_quantities_sorted_by_name()
     {
        let (pool, _dir) = setup_db().await;
        let meal =
            insert_test_meal(&pool, "Test", &[("zucchini", None), ("apple", Some("2"))]).await;
        let mut conn = pool.acquire().await.unwrap();
        let ings = get_meal_ingredients(&mut *conn, meal.id)
            .await
            .expect("get");
        assert_eq!(ings[0].name, "apple");
        assert_eq!(ings[1].name, "zucchini");
        assert_eq!(ings[0].quantity.as_deref(), Some("2"));
        assert_eq!(ings[1].quantity, None);
    }

    #[tokio::test]
    async fn given_meal_with_no_ingredients_when_get_meal_ingredients_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "Test", &[("x", None)]).await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("DELETE FROM meal_ingredients WHERE meal_id = ?1")
            .bind(meal.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let ings = get_meal_ingredients(&mut *conn, meal.id)
            .await
            .expect("get");
        assert!(ings.is_empty());
    }

    #[tokio::test]
    async fn given_same_ingredient_different_case_when_insert_across_meals_then_deduplicates_preserving_first_casing()
     {
        let (pool, _dir) = setup_db().await;
        // First meal uses "Thüringer Rostbratwurst" (imported casing)
        let meal_a = insert_test_meal(
            &pool,
            "Meal A",
            &[("Thüringer Rostbratwurst", Some("200 g"))],
        )
        .await;
        // Second meal uses lowercase variant — must resolve to the SAME ingredient row
        let meal_b = insert_test_meal(
            &pool,
            "Meal B",
            &[("thüringer rostbratwurst", Some("100 g"))],
        )
        .await;

        let mut conn = pool.acquire().await.unwrap();
        let ings_a = get_meal_ingredients(&mut *conn, meal_a.id)
            .await
            .expect("get");
        let ings_b = get_meal_ingredients(&mut *conn, meal_b.id)
            .await
            .expect("get");

        // First-seen casing "Thüringer Rostbratwurst" is stored, not lowercased
        assert_eq!(ings_a[0].name, "Thüringer Rostbratwurst");
        // Second meal resolves to the SAME ingredient row (same casing as first-seen)
        assert_eq!(ings_b[0].name, "Thüringer Rostbratwurst");

        // Only one ingredient row exists in the table (case-insensitive dedup)
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ingredients WHERE name = 'Thüringer Rostbratwurst' COLLATE NOCASE",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    // -----------------------------------------------------------------------
    // hydrate_meals
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meals_with_ingredients_when_hydrate_then_attaches_ingredient_lists_to_each() {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("salt", Some("1g"))]).await;
        let m2 = insert_test_meal(&pool, "B", &[("salt", Some("2g"))]).await;
        let mut meals = vec![
            Meal {
                ingredients: Vec::new(),
                ..m1.clone()
            },
            Meal {
                ingredients: Vec::new(),
                ..m2.clone()
            },
        ];
        hydrate_meals(&pool, &mut meals).await.expect("hydrate");
        assert_eq!(meals[0].ingredients.len(), 1);
        assert_eq!(meals[0].ingredients[0].name, "salt");
        assert_eq!(meals[1].ingredients.len(), 1);
        assert_eq!(meals[1].ingredients[0].name, "salt");
    }

    #[tokio::test]
    async fn given_meals_with_no_ingredients_when_hydrate_then_attaches_empty_lists() {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "A", &[("x", None)]).await;
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("DELETE FROM meal_ingredients WHERE meal_id = ?1")
            .bind(meal.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        let mut meals = vec![Meal {
            ingredients: Vec::new(),
            ..meal.clone()
        }];
        hydrate_meals(&pool, &mut meals).await.expect("hydrate");
        assert!(meals[0].ingredients.is_empty());
    }

    // -----------------------------------------------------------------------
    // list_meals
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_empty_db_when_list_meals_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let meals = list_meals(&pool, None).await.expect("list_meals");
        assert!(meals.is_empty());
    }

    #[tokio::test]
    async fn given_two_meals_when_list_meals_then_returns_both_ordered_by_updated_at_desc() {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "First", &[("a", None)]).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let m2 = insert_test_meal(&pool, "Second", &[("b", None)]).await;

        let meals = list_meals(&pool, None).await.expect("list_meals");
        assert_eq!(meals.len(), 2);
        assert_eq!(meals[0].id, m2.id);
        assert_eq!(meals[1].id, m1.id);
    }

    #[tokio::test]
    async fn given_search_term_matches_name_then_returns_only_matching() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "Chicken Soup", &[("broth", None)]).await;
        insert_test_meal(&pool, "Beef Stew", &[("meat", None)]).await;
        let meals = list_meals(&pool, Some("chicken"))
            .await
            .expect("list_meals");
        assert_eq!(meals.len(), 1);
        assert_eq!(meals[0].name, "Chicken Soup");
    }

    #[tokio::test]
    async fn given_search_term_matches_ingredients_then_returns_only_matching() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "Chicken Soup", &[("broth", None)]).await;
        insert_test_meal(&pool, "Beef Stew", &[("meat", None)]).await;
        let meals = list_meals(&pool, Some("meat")).await.expect("list_meals");
        assert_eq!(meals.len(), 1);
        assert_eq!(meals[0].name, "Beef Stew");
    }

    #[tokio::test]
    async fn given_search_term_matching_ingredient_name_when_list_meals_then_returns_meals_with_that_ingredient()
     {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("tomato", None)]).await;
        insert_test_meal(&pool, "B", &[("onion", None)]).await;
        let meals = list_meals(&pool, Some("tomato")).await.expect("list_meals");
        assert_eq!(meals.len(), 1);
        assert_eq!(meals[0].name, "A");
    }

    #[tokio::test]
    async fn given_search_term_is_whitespace_then_returns_all() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("x", None)]).await;
        insert_test_meal(&pool, "B", &[("y", None)]).await;
        let meals = list_meals(&pool, Some("   ")).await.expect("list_meals");
        assert_eq!(meals.len(), 2);
    }

    #[tokio::test]
    async fn given_search_term_matches_neither_then_returns_empty() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("x", None)]).await;
        let meals = list_meals(&pool, Some("zzz")).await.expect("list_meals");
        assert!(meals.is_empty());
    }

    // -----------------------------------------------------------------------
    // find_meal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meal_exists_when_find_meal_then_returns_meal() {
        let (pool, _dir) = setup_db().await;
        let inserted = insert_test_meal(&pool, "Test", &[("stuff", None)]).await;
        let found = find_meal(&pool, inserted.id).await.expect("find_meal");
        assert_eq!(found.id, inserted.id);
        assert_eq!(found.name, inserted.name);
        assert_eq!(found.ingredients.len(), 1);
        assert_eq!(found.ingredients[0].name, "stuff");
    }

    #[tokio::test]
    async fn given_meal_exists_when_find_meal_with_wrong_id_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "Test", &[("stuff", None)]).await;
        let result = find_meal(&pool, 999).await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // insert_meal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_valid_meal_when_insert_meal_then_persists_with_ingredients() {
        let (pool, _dir) = setup_db().await;
        let result = insert_meal(
            &pool,
            NewMeal {
                name: "Test".into(),
                ingredients: vec![NewIngredientLine {
                    name: "Salt".into(),
                    quantity: Some("200g".into()),
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        let meal = result.expect("insert_meal");
        assert!(meal.id > 0);
        assert_eq!(meal.ingredients.len(), 1);
        assert_eq!(meal.ingredients[0].name, "Salt");
        assert_eq!(meal.ingredients[0].quantity.as_deref(), Some("200g"));
    }

    // -----------------------------------------------------------------------
    // update_meal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meal_exists_when_update_meal_then_preserves_id_and_advances_updated_at() {
        let (pool, _dir) = setup_db().await;
        let original = insert_test_meal(&pool, "Old Name", &[("Old", None)]).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        let updated = update_meal(
            &pool,
            original.id,
            MealPatch {
                name: "New Name".into(),
                ingredients: vec![NewIngredientLine {
                    name: "New".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await
        .expect("update_meal");
        assert_eq!(updated.id, original.id);
        assert_eq!(updated.name, "New Name");
        assert_eq!(updated.ingredients.len(), 1);
        assert_eq!(updated.ingredients[0].name, "New");
        assert!(updated.updated_at > original.updated_at);
    }

    #[tokio::test]
    async fn given_meal_when_update_meal_then_last_planned_at_is_unchanged() {
        let (pool, _dir) = setup_db().await;
        let original = insert_test_meal(&pool, "X", &[("y", None)]).await;
        let ts = "2025-01-01T00:00:00Z";
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
            .bind(ts)
            .bind(original.id)
            .execute(&mut *conn)
            .await
            .unwrap();

        let updated = update_meal(
            &pool,
            original.id,
            MealPatch {
                name: "X2".into(),
                ingredients: vec![NewIngredientLine {
                    name: "z".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await
        .expect("update_meal");
        assert_eq!(
            updated.last_planned_at,
            Some(
                DateTime::parse_from_rfc3339(ts)
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
    }

    #[tokio::test]
    async fn given_meal_missing_when_update_meal_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let result = update_meal(
            &pool,
            999,
            MealPatch {
                name: "X".into(),
                ingredients: vec![NewIngredientLine {
                    name: "y".into(),
                    quantity: None,
                }],
                instructions: "test".into(),
            },
            ImageChange::Keep,
        )
        .await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // delete_meal
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meal_exists_when_delete_meal_then_subsequent_find_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "To Delete", &[("stuff", None)]).await;
        delete_meal(&pool, meal.id).await.expect("delete_meal");
        let result = find_meal(&pool, meal.id).await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound after delete, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_meal_missing_when_delete_meal_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let result = delete_meal(&pool, 999).await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_meal_only_uses_ingredient_when_delete_meal_then_ingredient_row_is_deleted_as_orphan()
     {
        let (pool, _dir) = setup_db().await;
        let meal = insert_test_meal(&pool, "X", &[("unique_ing", None)]).await;
        delete_meal(&pool, meal.id).await.expect("delete");
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ingredients WHERE name = 'unique_ing'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn given_meal_shares_ingredient_with_others_when_delete_meal_then_ingredient_row_remains()
    {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("shared", None)]).await;
        let _m2 = insert_test_meal(&pool, "B", &[("shared", None)]).await;
        delete_meal(&pool, m1.id).await.expect("delete");
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM ingredients WHERE name = 'shared'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    // -----------------------------------------------------------------------
    // Week math
    // -----------------------------------------------------------------------

    #[test]
    fn given_jan_1_2026_when_week_of_date_then_returns_2026_week_1() {
        let d = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let (year, week) = week_of_date(d);
        assert_eq!(year, 2026);
        assert_eq!(week, 1);
    }

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

    // -----------------------------------------------------------------------
    // select_meals_weighted
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_3_unplanned_and_3_recent_meals_when_select_3_weighted_over_100_trials_then_unplanned_chosen_at_least_twice_as_often()
     {
        let (pool, _dir) = setup_db().await;

        for i in 1..=3 {
            let m = insert_test_meal(&pool, &format!("new{i}"), &[("x", None)]).await;
            let mut conn = pool.acquire().await.unwrap();
            sqlx::query("UPDATE meals SET last_planned_at = NULL WHERE id = ?1")
                .bind(m.id)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        let recent = Utc::now();
        for i in 1..=3 {
            let m = insert_test_meal(&pool, &format!("recent{i}"), &[("x", None)]).await;
            let mut conn = pool.acquire().await.unwrap();
            sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
                .bind(recent)
                .bind(m.id)
                .execute(&mut *conn)
                .await
                .unwrap();
        }

        let mut unplanned_picks = 0usize;
        let mut recent_picks = 0usize;

        for _ in 0..100 {
            let mut conn = pool.acquire().await.unwrap();
            for i in 1..=3 {
                sqlx::query("UPDATE meals SET last_planned_at = NULL WHERE name = ?1")
                    .bind(format!("new{i}"))
                    .execute(&mut *conn)
                    .await
                    .unwrap();
            }
            for i in 1..=3 {
                sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE name = ?2")
                    .bind(recent)
                    .bind(format!("recent{i}"))
                    .execute(&mut *conn)
                    .await
                    .unwrap();
            }

            let selected = select_meals_weighted(&mut *conn, 3).await.expect("select");
            for meal in &selected {
                if meal.name.starts_with("new") {
                    unplanned_picks += 1;
                } else {
                    recent_picks += 1;
                }
            }
        }

        assert!(
            unplanned_picks >= 2 * recent_picks,
            "unplanned_picks={unplanned_picks}, recent_picks={recent_picks}"
        );
    }

    #[tokio::test]
    async fn given_3_meals_when_select_5_weighted_then_returns_all_3_meals() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("x", None)]).await;
        insert_test_meal(&pool, "B", &[("x", None)]).await;
        insert_test_meal(&pool, "C", &[("x", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        let selected = select_meals_weighted(&mut *conn, 5).await.expect("select");
        assert_eq!(selected.len(), 3);
    }

    #[tokio::test]
    async fn given_empty_meals_table_when_select_weighted_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = select_meals_weighted(&mut *conn, 3).await.expect("select");
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // aggregate_plan_ingredients
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_plan_with_two_meals_sharing_salt_200g_and_100g_when_aggregate_then_salt_numeric_total_is_300_with_unit_g()
     {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
        let m2 = insert_test_meal(&pool, "B", &[("salt", Some("100g"))]).await;

        let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_id: i64 = row.get(0);
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&pool)
            .await
            .unwrap();

        let summary = aggregate_plan_ingredients(&pool, plan_id)
            .await
            .expect("aggregate");
        let salt = summary
            .iter()
            .find(|e| e.name == "salt")
            .expect("salt entry");
        let nt = salt.numeric_total.as_ref().expect("numeric_total");
        assert!((nt.value - 300.0).abs() < 0.001);
        assert_eq!(nt.unit.as_deref(), Some("g"));
    }

    #[tokio::test]
    async fn given_plan_with_salt_200g_and_a_pinch_when_aggregate_then_numeric_total_is_200_with_unit_g_and_non_numeric_has_a_pinch()
     {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
        let m2 = insert_test_meal(&pool, "B", &[("salt", Some("a pinch"))]).await;

        let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_id: i64 = row.get(0);
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&pool)
            .await
            .unwrap();

        let summary = aggregate_plan_ingredients(&pool, plan_id)
            .await
            .expect("aggregate");
        let salt = summary
            .iter()
            .find(|e| e.name == "salt")
            .expect("salt entry");
        let nt = salt.numeric_total.as_ref().expect("numeric_total");
        assert!((nt.value - 200.0).abs() < 0.001);
        assert_eq!(nt.unit.as_deref(), Some("g"));
        assert_eq!(salt.non_numeric, vec!["a pinch"]);
    }

    #[tokio::test]
    async fn given_plan_with_salt_200g_and_cups_1_5_when_aggregate_then_numeric_total_is_201_5_with_null_unit()
     {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
        let m2 = insert_test_meal(&pool, "B", &[("salt", Some("1.5 cups"))]).await;

        let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_id: i64 = row.get(0);
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&pool)
            .await
            .unwrap();

        let summary = aggregate_plan_ingredients(&pool, plan_id)
            .await
            .expect("aggregate");
        let salt = summary
            .iter()
            .find(|e| e.name == "salt")
            .expect("salt entry");
        let nt = salt.numeric_total.as_ref().expect("numeric_total");
        assert!((nt.value - 201.5).abs() < 0.001);
        assert_eq!(nt.unit, None);
    }

    #[tokio::test]
    async fn given_plan_with_no_meals_when_aggregate_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_id: i64 = row.get(0);
        let summary = aggregate_plan_ingredients(&pool, plan_id)
            .await
            .expect("aggregate");
        assert!(summary.is_empty());
    }

    // -----------------------------------------------------------------------
    // Plan CRUD
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_plan_with_meals_when_get_plan_meals_then_returns_hydrated_meals_in_id_order() {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "A", &[("x", None)]).await;
        let m2 = insert_test_meal(&pool, "B", &[("y", None)]).await;

        let row = sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_id: i64 = row.get(0);
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&pool)
            .await
            .unwrap();

        let meals = get_plan_meals(&pool, plan_id)
            .await
            .expect("get_plan_meals");
        assert_eq!(meals.len(), 2);
        assert!(!meals[0].ingredients.is_empty());
    }

    #[tokio::test]
    async fn given_empty_meals_table_when_create_or_replace_plan_then_returns_bad_request() {
        let (pool, _dir) = setup_db().await;
        let result = create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 1,
                meal_count: 3,
            },
        )
        .await;
        match &result {
            Err(AppError::BadRequest(_)) => {}
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_existing_plan_when_create_or_replace_plan_then_replaces_meals_and_updates_last_planned_for_new_set_only()
     {
        let (pool, _dir) = setup_db().await;

        insert_test_meal(&pool, "M1", &[("x", None)]).await;
        insert_test_meal(&pool, "M2", &[("x", None)]).await;
        insert_test_meal(&pool, "M3", &[("x", None)]).await;

        let plan1 = create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 1,
                meal_count: 2,
            },
        )
        .await
        .expect("create plan 1");

        let _old_meal_ids: std::collections::HashSet<i64> =
            plan1.meals.iter().map(|m| m.id).collect();

        let plan2 = create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 1,
                meal_count: 2,
            },
        )
        .await
        .expect("create plan 2");

        assert_eq!(plan2.meals.len(), 2);
        assert!(!plan2.ingredient_summary.is_empty());
    }

    #[tokio::test]
    async fn given_invalid_year_or_week_when_create_or_replace_plan_then_returns_bad_request() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "X", &[("y", None)]).await;
        let result = create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 99,
                meal_count: 1,
            },
        )
        .await;
        match &result {
            Err(AppError::BadRequest(_)) => {}
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_plan_exists_when_get_plan_then_returns_full_plan_with_meals_and_summary() {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("salt", Some("200g"))]).await;
        insert_test_meal(&pool, "B", &[("salt", Some("100g"))]).await;

        let _created = create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 1,
                meal_count: 2,
            },
        )
        .await
        .expect("create");

        let plan = get_plan(&pool, 2026, 1).await.expect("get_plan");
        assert_eq!(plan.meals.len(), 2);
        assert!(!plan.ingredient_summary.is_empty());
    }

    #[tokio::test]
    async fn given_plan_missing_when_get_plan_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let result = get_plan(&pool, 2026, 99).await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_3_plans_for_year_when_list_plans_for_year_then_returns_3_summary_items_sorted_by_week()
     {
        let (pool, _dir) = setup_db().await;
        insert_test_meal(&pool, "A", &[("x", None)]).await;
        insert_test_meal(&pool, "B", &[("x", None)]).await;

        create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 3,
                meal_count: 1,
            },
        )
        .await
        .expect("create");
        create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 1,
                meal_count: 1,
            },
        )
        .await
        .expect("create");
        create_or_replace_plan(
            &pool,
            NewPlanRequest {
                year: 2026,
                week_number: 2,
                meal_count: 1,
            },
        )
        .await
        .expect("create");

        let list = list_plans_for_year(&pool, 2026).await.expect("list");
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].week_number, 1);
        assert_eq!(list[1].week_number, 2);
        assert_eq!(list[2].week_number, 3);
    }

    #[tokio::test]
    async fn given_no_plans_for_year_when_list_plans_for_year_then_returns_empty_vec() {
        let (pool, _dir) = setup_db().await;
        let list = list_plans_for_year(&pool, 2026).await.expect("list");
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn given_existing_plan_when_update_plan_meals_then_returns_plan_with_new_meal_list_and_does_not_touch_any_last_planned_at()
     {
        let (pool, _dir) = setup_db().await;

        let m1 = insert_test_meal(&pool, "M1", &[("x", None)]).await;
        let m2 = insert_test_meal(&pool, "M2", &[("x", None)]).await;
        let m3 = insert_test_meal(&pool, "M3", &[("x", None)]).await;

        let ts = "2025-06-01T00:00:00Z";
        let mut conn = pool.acquire().await.unwrap();
        for id in &[m1.id, m2.id, m3.id] {
            sqlx::query("UPDATE meals SET last_planned_at = ?1 WHERE id = ?2")
                .bind(ts)
                .bind(*id)
                .execute(&mut *conn)
                .await
                .unwrap();
        }

        sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let plan = update_plan_meals(
            &pool,
            2026,
            1,
            PlanPatch {
                meal_ids: vec![m1.id, m3.id],
            },
        )
        .await
        .expect("update");

        assert_eq!(plan.meals.len(), 2);
        let meal_ids: Vec<i64> = plan.meals.iter().map(|m| m.id).collect();
        assert!(meal_ids.contains(&m1.id));
        assert!(meal_ids.contains(&m3.id));

        let m1_fresh = find_meal(&pool, m1.id).await.unwrap();
        let m3_fresh = find_meal(&pool, m3.id).await.unwrap();
        let expected = Some(
            DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&Utc),
        );
        assert_eq!(m1_fresh.last_planned_at, expected);
        assert_eq!(m3_fresh.last_planned_at, expected);
    }

    #[tokio::test]
    async fn given_plan_missing_when_update_plan_meals_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let m = insert_test_meal(&pool, "X", &[("y", None)]).await;
        let result = update_plan_meals(
            &pool,
            2026,
            99,
            PlanPatch {
                meal_ids: vec![m.id],
            },
        )
        .await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_meal_id_not_in_meals_table_when_update_plan_meals_then_returns_not_found() {
        let (pool, _dir) = setup_db().await;
        let m = insert_test_meal(&pool, "X", &[("y", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        let result = update_plan_meals(
            &pool,
            2026,
            1,
            PlanPatch {
                meal_ids: vec![99999],
            },
        )
        .await;
        match &result {
            Err(AppError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_existing_plan_when_delete_plan_then_returns_ok_and_plan_meals_rows_cascade_away()
    {
        let (pool, _dir) = setup_db().await;
        let m = insert_test_meal(&pool, "A", &[("x", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        delete_plan(&pool, 2026, 1).await.expect("delete");

        let pm_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM plan_meals WHERE plan_id = ?1")
                .bind(plan_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pm_count, 0);

        let _meal = find_meal(&pool, m.id).await.expect("meal should exist");
    }

    #[tokio::test]
    async fn given_meal_in_plan_when_delete_meal_then_plan_meals_row_removed_and_plan_remains() {
        let (pool, _dir) = setup_db().await;
        let m1 = insert_test_meal(&pool, "M1", &[("x", None)]).await;
        let m2 = insert_test_meal(&pool, "M2", &[("x", None)]).await;

        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(
            "INSERT INTO week_plans (year, week_number, created_at) VALUES (2026, 1, '2025-01-01T00:00:00Z')",
        )
        .execute(&mut *conn)
        .await
        .unwrap();
        let plan_id: i64 = sqlx::query_scalar("SELECT last_insert_rowid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m1.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plan_meals (plan_id, meal_id) VALUES (?1, ?2)")
            .bind(plan_id)
            .bind(m2.id)
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        delete_meal(&pool, m1.id).await.expect("delete");

        let wp_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM week_plans WHERE id = ?1")
            .bind(plan_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(wp_count, 1);

        let rows = sqlx::query("SELECT meal_id FROM plan_meals WHERE plan_id = ?1")
            .bind(plan_id)
            .fetch_all(&pool)
            .await
            .unwrap();
        let pm_meals: Vec<i64> = rows.iter().map(|r| r.get(0)).collect();
        assert_eq!(pm_meals, vec![m2.id]);
    }
}
