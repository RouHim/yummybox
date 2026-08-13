use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::model::{IngredientQuantity, Meal, MealPatch, NewIngredientLine, NewMeal};
// ---------------------------------------------------------------------------
// Row structs for query_as
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
pub(crate) struct MealRow {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) instructions: String,
    pub(crate) last_planned_at: Option<DateTime<Utc>>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
    pub(crate) has_image: bool,
    pub(crate) portions: Option<i32>,
}

/// Convert a [`MealRow`] into a [`Meal`] (without ingredients — those are
/// hydrated separately by the caller).
pub(crate) fn map_meal_row(row: MealRow) -> Meal {
    Meal {
        id: row.id,
        name: row.name,
        instructions: row.instructions,
        ingredients: Vec::new(),
        last_planned_at: row.last_planned_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        has_image: row.has_image,
        portions: row.portions,
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
    portions: Option<i32>,
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
    validate_portions(portions)?;
    Ok(())
}

fn validate_portions(portions: Option<i32>) -> Result<(), AppError> {
    match portions {
        Some(p) if p <= 0 => Err(AppError::Validation("portions must be positive".into())),
        Some(p) if p > 10_000 => Err(AppError::Validation(
            "portions must be at most 10000".into(),
        )),
        _ => Ok(()),
    }
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
    if meals.is_empty() {
        return Ok(());
    }

    let ids: Vec<i64> = meals.iter().map(|m| m.id).collect();

    // One query with dynamic IN clause instead of N per-meal queries
    let mut builder = sqlx::QueryBuilder::new(
        "SELECT mi.meal_id, i.name, mi.quantity
         FROM meal_ingredients mi
         JOIN ingredients i ON i.id = mi.ingredient_id
         WHERE mi.meal_id IN (",
    );
    let mut separated = builder.separated(", ");
    for id in &ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(") ORDER BY i.name");

    let rows = builder.build().fetch_all(pool).await?;

    let mut map: HashMap<i64, Vec<IngredientQuantity>> = HashMap::new();
    for row in &rows {
        let meal_id: i64 = row.get(0);
        map.entry(meal_id).or_default().push(IngredientQuantity {
            name: row.get(1),
            quantity: row.get(2),
        });
    }

    for meal in meals.iter_mut() {
        meal.ingredients = map.remove(&meal.id).unwrap_or_default();
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
                "SELECT DISTINCT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image, m.portions
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
                "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image, m.portions
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
        "SELECT m.id, m.name, m.instructions, m.last_planned_at, m.created_at, m.updated_at, (m.image IS NOT NULL) AS has_image, m.portions
         FROM meals m WHERE m.id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let mut meal = map_meal_row(row);
    let mut conn = pool.acquire().await?;
    meal.ingredients = get_meal_ingredients(&mut conn, meal.id).await?;
    Ok(meal)
}

pub async fn insert_meal(
    pool: &SqlitePool,
    new: NewMeal,
    image: ImageChange<'_>,
) -> Result<Meal, AppError> {
    validate_meal(&new.name, &new.ingredients, &new.instructions, new.portions)?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    let trimmed_name = new.name.trim();
    let id: (i64,) = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO meals (name, instructions, portions, last_planned_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
    )
    .bind(trimmed_name)
    .bind(&new.instructions)
    .bind(new.portions)
    .bind(None::<String>)
    .bind(now)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    set_meal_ingredients(&mut tx, id.0, &new.ingredients).await?;

    if let ImageChange::Set(jpeg_bytes) = image {
        set_meal_image(&mut tx, id.0, jpeg_bytes).await?;
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
    validate_meal(
        &patch.name,
        &patch.ingredients,
        &patch.instructions,
        patch.portions,
    )?;
    let now = Utc::now();

    let mut tx = pool.begin().await?;
    let trimmed_name = patch.name.trim();
    let affected =
        sqlx::query("UPDATE meals SET name = ?1, instructions = ?2, portions = ?3, updated_at = ?4 WHERE id = ?5")
            .bind(trimmed_name)
            .bind(&patch.instructions)
            .bind(patch.portions)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound);
    }

    set_meal_ingredients(&mut tx, id, &patch.ingredients).await?;

    match image {
        ImageChange::Set(jpeg_bytes) => {
            set_meal_image(&mut tx, id, jpeg_bytes).await?;
        }
        ImageChange::Clear => {
            clear_meal_image(&mut tx, id).await?;
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
