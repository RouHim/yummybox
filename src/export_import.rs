use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use zip::CompressionMethod;
use zip::read::ZipArchive;
use zip::write::ZipWriter;

use crate::db;
use crate::error::AppError;
use crate::image;
use crate::model::{Meal, NewIngredientLine, NewMeal};
use crate::recipe;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const IMPORT_MAX_ARCHIVE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
const IMPORT_MAX_RECIPES: usize = 500;
const IMPORT_MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024; // 20 MB per image

// ---------------------------------------------------------------------------
// Import result types
// ---------------------------------------------------------------------------

/// A single failed recipe entry in a zip import response.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipImportFailure {
    pub source: String,
    pub reason: String,
}

/// Response body for `POST /api/import/zip`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZipImportResult {
    pub created: Vec<Meal>,
    pub skipped: usize,
    pub failed: Vec<ZipImportFailure>,
}

// ===========================================================================
// Export handler
// ===========================================================================

/// `GET /api/export/meals.zip`
///
/// Builds a ZIP archive containing `recipes.json` (JSON-LD `@graph` of
/// schema.org `Recipe` objects) and one `images/<id>.jpg` per meal that has
/// an image.  Meals without images omit the `image` property and produce no
/// `images/` entry.
#[instrument(skip(state))]
pub async fn export_meals_zip(State(state): State<Arc<AppState>>) -> Result<Response, AppError> {
    let meals = db::list_meals(&state.pool, None).await?;

    // Pre-fetch all images (must happen before ZIP building, which is sync)
    let mut images: Vec<(i64, Vec<u8>)> = Vec::new();
    for meal in &meals {
        if meal.has_image {
            if let Some((bytes, _)) = db::find_meal_image(&state.pool, meal.id).await? {
                images.push((meal.id, bytes));
            }
        }
    }

    // Build JSON-LD graph — one Recipe per meal, image paths relative
    let recipes: Vec<serde_json::Value> = meals
        .iter()
        .map(|meal| build_recipe_json(meal, &images))
        .collect();

    let jsonld = serde_json::json!({
        "@context": "https://schema.org",
        "@graph": recipes,
    });

    let json_str = serde_json::to_string_pretty(&jsonld)
        .map_err(|e| AppError::Internal(format!("JSON serialization error: {e}")))?;

    // Build ZIP in memory
    let zip_bytes = build_export_zip(&json_str, &images)?;

    let today = Utc::now().format("%Y-%m-%d");
    let filename = format!("mealme-export-{today}.zip");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(zip_bytes))
        .map_err(|e| AppError::Internal(format!("failed to build response: {e}")))
}

/// Build a schema.org `Recipe` JSON-LD object for one meal.
///
/// Image references use the relative path `images/<id>.jpg` when the meal's
/// image is included in the export; otherwise the `image` key is omitted.
fn build_recipe_json(meal: &Meal, images: &[(i64, Vec<u8>)]) -> serde_json::Value {
    use serde_json::{Map, Value};

    let mut obj = Map::new();

    obj.insert(
        "@context".into(),
        Value::String("https://schema.org".into()),
    );
    obj.insert("@type".into(), Value::String("Recipe".into()));
    obj.insert("name".into(), Value::String(meal.name.clone()));

    let ingredients: Vec<Value> = meal
        .ingredients
        .iter()
        .map(|i| {
            let line = match &i.quantity {
                Some(q) if !q.trim().is_empty() => format!("{} {}", q.trim(), i.name),
                _ => i.name.clone(),
            };
            Value::String(line)
        })
        .collect();
    obj.insert("recipeIngredient".into(), Value::Array(ingredients));

    obj.insert(
        "recipeInstructions".into(),
        Value::String(meal.instructions.clone()),
    );
    obj.insert(
        "datePublished".into(),
        Value::String(meal.created_at.to_rfc3339()),
    );
    obj.insert(
        "dateModified".into(),
        Value::String(meal.updated_at.to_rfc3339()),
    );

    // Relative image path (omitted when meal has no image)
    if images.iter().any(|(id, _)| *id == meal.id) {
        obj.insert(
            "image".into(),
            Value::String(format!("images/{}.jpg", meal.id)),
        );
    }

    Value::Object(obj)
}

/// Serialize the JSON-LD string and image blobs into a ZIP archive in memory.
fn build_export_zip(json_str: &str, images: &[(i64, Vec<u8>)]) -> Result<Vec<u8>, AppError> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let options = zip::write::FileOptions::<()>::default()
            .compression_method(CompressionMethod::Deflated);

        zip.start_file("recipes.json", options)
            .map_err(|e| AppError::Internal(format!("ZIP error: {e}")))?;
        zip.write_all(json_str.as_bytes())
            .map_err(|e| AppError::Internal(format!("ZIP write error: {e}")))?;

        for (id, bytes) in images {
            let img_path = format!("images/{id}.jpg");
            zip.start_file(img_path, options)
                .map_err(|e| AppError::Internal(format!("ZIP error: {e}")))?;
            zip.write_all(bytes)
                .map_err(|e| AppError::Internal(format!("ZIP write error: {e}")))?;
        }

        zip.finish()
            .map_err(|e| AppError::Internal(format!("ZIP finalization error: {e}")))?;
    }
    Ok(buf.into_inner())
}

// ===========================================================================
// Import handler
// ===========================================================================

/// `POST /api/import/zip`
///
/// Accepts a multipart form with a single `file` field containing a ZIP
/// archive of schema.org Recipe JSON-LD + optional images.
///
/// Every valid recipe is persisted immediately; duplicates are skipped
/// (case-insensitive name match); validation failures are counted as
/// `failed` with the source and reason.
#[instrument(skip(state))]
pub async fn import_meals_zip(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ZipImportResult>), AppError> {
    // Extract the file field from multipart
    let zip_bytes = read_zip_file_from_multipart(&mut multipart).await?;

    // Validate archive size
    if zip_bytes.len() as u64 > IMPORT_MAX_ARCHIVE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "archive exceeds maximum size of {} MB",
            IMPORT_MAX_ARCHIVE_SIZE / (1024 * 1024)
        )));
    }

    // Parse the ZIP and extract recipes.json + image map
    let cursor = Cursor::new(&zip_bytes[..]);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("invalid zip file: {e}")))?;

    // Read recipes.json
    let recipes_json = read_zip_entry_to_string(&mut archive, "recipes.json")?;

    // Pre-load all image entries into memory (avoids borrow issues with the archive)
    let image_map = preload_images(&mut archive)?;

    // Parse the JSON-LD graph
    let graph: serde_json::Value = serde_json::from_str(&recipes_json)
        .map_err(|e| AppError::BadRequest(format!("invalid recipes.json: {e}")))?;

    let recipes = graph
        .get("@graph")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::BadRequest("recipes.json must be an object with an @graph array".into())
        })?;

    if recipes.len() > IMPORT_MAX_RECIPES {
        return Err(AppError::BadRequest(format!(
            "too many recipes: maximum {IMPORT_MAX_RECIPES} allowed, got {}",
            recipes.len()
        )));
    }

    let mut created: Vec<Meal> = Vec::new();
    let mut skipped: usize = 0;
    let mut failed: Vec<ZipImportFailure> = Vec::new();

    for (idx, recipe) in recipes.iter().enumerate() {
        let source = format!("index {idx} in @graph");
        match import_single_recipe(&state, recipe, &image_map).await {
            Ok(Some(meal)) => created.push(meal),
            Ok(None) => skipped += 1,
            Err(reason) => failed.push(ZipImportFailure { source, reason }),
        }
    }

    Ok((
        StatusCode::OK,
        Json(ZipImportResult {
            created,
            skipped,
            failed,
        }),
    ))
}

// ---------------------------------------------------------------------------
// Multipart / ZIP helpers
// ---------------------------------------------------------------------------

/// Extract the first `file` field from a multipart stream as raw bytes.
async fn read_zip_file_from_multipart(multipart: &mut Multipart) -> Result<Vec<u8>, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart data: {e}")))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("failed to read file: {e}")))?;
            return Ok(data.to_vec());
        }
    }
    Err(AppError::BadRequest("missing 'file' field".into()))
}

/// Read a named entry from a ZIP archive as a UTF-8 string, or return a 400.
fn read_zip_entry_to_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, AppError> {
    let mut file = archive
        .by_name(name)
        .map_err(|_| AppError::BadRequest(format!("zip must contain a {name} entry")))?;
    let mut s = String::new();
    file.read_to_string(&mut s)
        .map_err(|e| AppError::BadRequest(format!("failed to read {name}: {e}")))?;
    Ok(s)
}

/// Pre-load every `images/*` entry from the archive into a map.
///
/// Entries exceeding [`IMPORT_MAX_IMAGE_SIZE`] are silently skipped (the
/// caller treats them as best-effort — meal is created without the image).
fn preload_images<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<HashMap<String, Vec<u8>>, AppError> {
    let mut map = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::BadRequest(format!("zip read error: {e}")))?;
        let name = entry.name().to_string();
        if name.starts_with("images/") {
            if entry.size() > IMPORT_MAX_IMAGE_SIZE {
                continue; // best-effort: skip oversized images
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buf)
                .map_err(|e| AppError::BadRequest(format!("failed to read {name}: {e}")))?;
            map.insert(name, buf);
        }
    }
    Ok(map)
}

// ---------------------------------------------------------------------------
// Single-recipe import
// ---------------------------------------------------------------------------

/// Try to import one schema.org `Recipe` JSON value.
///
/// Returns:
/// - `Ok(Some(meal))` — created successfully
/// - `Ok(None)` — skipped (duplicate name)
/// - `Err(reason)` — validation or DB failure
async fn import_single_recipe(
    state: &Arc<AppState>,
    recipe: &serde_json::Value,
    image_map: &HashMap<String, Vec<u8>>,
) -> Result<Option<Meal>, String> {
    // --- name ---------------------------------------------------------------
    let name = recipe
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'name' field".to_string())?;

    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("'name' field is empty".to_string());
    }

    // --- duplicate check ----------------------------------------------------
    let normalized = db::normalize_ingredient_name(trimmed_name);
    let existing = db::list_meals(&state.pool, None)
        .await
        .map_err(|e| format!("database error: {e}"))?;
    if existing
        .iter()
        .any(|m| db::normalize_ingredient_name(&m.name) == normalized)
    {
        return Ok(None); // skipped
    }

    // --- ingredients --------------------------------------------------------
    let ingredient_lines = extract_ingredient_strings(recipe)
        .ok_or_else(|| "missing or invalid 'recipeIngredient' field".to_string())?;

    let ingredients: Vec<NewIngredientLine> = ingredient_lines
        .iter()
        .map(|line| recipe::split_ingredient_line(line))
        .collect();

    // --- instructions -------------------------------------------------------
    let instructions = extract_instructions(recipe);

    // --- validate -----------------------------------------------------------
    if let Err(e) = db::validate_meal(trimmed_name, &ingredients, &instructions) {
        let msg = e.to_string();
        // Strip the "Validation error: " prefix if present (it's not, but be safe)
        return Err(format!("validation failed: {msg}"));
    }

    // --- image (best-effort) ------------------------------------------------
    let jpeg_bytes: Option<Vec<u8>> = recipe
        .get("image")
        .and_then(|v| v.as_str())
        .and_then(|img_path| image_map.get(img_path))
        .and_then(|raw| image::convert_to_jpeg(raw).ok());

    let image_change = match &jpeg_bytes {
        Some(bytes) => db::ImageChange::Set(bytes),
        None => db::ImageChange::Keep,
    };

    // --- insert -------------------------------------------------------------
    let new_meal = NewMeal {
        name: trimmed_name.to_string(),
        ingredients,
        instructions,
    };

    let meal = db::insert_meal(&state.pool, new_meal, image_change)
        .await
        .map_err(|e| format!("database error: {e}"))?;

    Ok(Some(meal))
}

/// Extract ingredient strings from a `Recipe` JSON value.
///
/// `recipeIngredient` may be a string array (standard) or a single string
/// (tolerated — treated as a one-element array).
fn extract_ingredient_strings(recipe: &serde_json::Value) -> Option<Vec<String>> {
    match recipe.get("recipeIngredient") {
        Some(serde_json::Value::Array(arr)) => {
            let lines: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
            if lines.is_empty() { None } else { Some(lines) }
        }
        Some(serde_json::Value::String(s)) => Some(vec![s.clone()]),
        _ => None,
    }
}

/// Extract instructions from a `Recipe` JSON value.
///
/// `recipeInstructions` may be:
/// - a plain string
/// - an array of `HowToStep` objects (each with a `text` field)
/// - missing → returns empty string
fn extract_instructions(recipe: &serde_json::Value) -> String {
    match recipe.get("recipeInstructions") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(steps)) => {
            let texts: Vec<&str> = steps
                .iter()
                .filter_map(|step| step.get("text").and_then(|t| t.as_str()))
                .collect();
            texts.join("\n")
        }
        _ => String::new(),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Meal;
    use chrono::{TimeZone, Utc};
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::write::ZipWriter;

    // ------------------------------------------------------------------
    // build_recipe_json
    // ------------------------------------------------------------------

    fn sample_meal(id: i64, name: &str, has_image: bool) -> Meal {
        Meal {
            id,
            name: name.into(),
            ingredients: vec![
                crate::model::IngredientQuantity {
                    name: "flour".into(),
                    quantity: Some("2 cups".into()),
                },
                crate::model::IngredientQuantity {
                    name: "sugar".into(),
                    quantity: None,
                },
            ],
            instructions: "Mix and bake.".into(),
            last_planned_at: None,
            created_at: Utc.with_ymd_and_hms(2026, 1, 15, 12, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 1, 16, 12, 0, 0).unwrap(),
            has_image,
        }
    }

    #[test]
    fn build_recipe_json_with_image_includes_relative_path() {
        let meal = sample_meal(42, "Test Meal", true);
        let images = vec![(42, vec![1, 2, 3])];
        let recipe = build_recipe_json(&meal, &images);

        assert_eq!(recipe["@context"], "https://schema.org");
        assert_eq!(recipe["@type"], "Recipe");
        assert_eq!(recipe["name"], "Test Meal");
        assert_eq!(recipe["image"], "images/42.jpg");

        let ingredients = recipe["recipeIngredient"].as_array().unwrap();
        assert_eq!(ingredients.len(), 2);
        assert_eq!(ingredients[0], "2 cups flour");
        assert_eq!(ingredients[1], "sugar");
    }

    #[test]
    fn build_recipe_json_without_image_omits_image_key() {
        let meal = sample_meal(7, "No Image", false);
        let images: Vec<(i64, Vec<u8>)> = vec![];
        let recipe = build_recipe_json(&meal, &images);

        assert!(recipe.get("image").is_none());
    }

    // ------------------------------------------------------------------
    // build_export_zip
    // ------------------------------------------------------------------

    #[test]
    fn build_export_zip_empty() {
        let zip_bytes = build_export_zip(
            &serde_json::to_string_pretty(&serde_json::json!({
                "@context": "https://schema.org",
                "@graph": []
            }))
            .unwrap(),
            &[],
        )
        .unwrap();

        assert!(!zip_bytes.is_empty());

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 1);

        let mut file = archive.by_name("recipes.json").unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        assert!(contents.contains("\"@graph\": []"));
    }

    #[test]
    fn build_export_zip_with_images() {
        let images = vec![(1, vec![10, 20, 30]), (3, vec![40, 50, 60])];
        let zip_bytes = build_export_zip(
            &serde_json::to_string_pretty(&serde_json::json!({
                "@context": "https://schema.org",
                "@graph": []
            }))
            .unwrap(),
            &images,
        )
        .unwrap();

        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 3); // recipes.json + 2 images

        // Verify image entries exist and match
        for (id, expected_bytes) in &images {
            let img_path = format!("images/{id}.jpg");
            let mut file = archive.by_name(&img_path).unwrap();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            assert_eq!(&contents, expected_bytes);
        }
    }

    // ------------------------------------------------------------------
    // extract_ingredient_strings
    // ------------------------------------------------------------------

    #[test]
    fn extract_ingredients_from_array() {
        let recipe = serde_json::json!({
            "recipeIngredient": ["2 cups flour", "1 tsp salt"]
        });
        let lines = extract_ingredient_strings(&recipe).unwrap();
        assert_eq!(lines, vec!["2 cups flour", "1 tsp salt"]);
    }

    #[test]
    fn extract_ingredients_from_single_string() {
        let recipe = serde_json::json!({
            "recipeIngredient": "just one thing"
        });
        let lines = extract_ingredient_strings(&recipe).unwrap();
        assert_eq!(lines, vec!["just one thing"]);
    }

    #[test]
    fn extract_ingredients_missing_returns_none() {
        let recipe = serde_json::json!({});
        assert!(extract_ingredient_strings(&recipe).is_none());
    }

    // ------------------------------------------------------------------
    // extract_instructions
    // ------------------------------------------------------------------

    #[test]
    fn extract_instructions_from_string() {
        let recipe = serde_json::json!({
            "recipeInstructions": "Step 1. Step 2."
        });
        assert_eq!(extract_instructions(&recipe), "Step 1. Step 2.");
    }

    #[test]
    fn extract_instructions_from_how_to_steps() {
        let recipe = serde_json::json!({
            "recipeInstructions": [
                {"@type": "HowToStep", "text": "Mix ingredients"},
                {"@type": "HowToStep", "text": "Bake at 350F"}
            ]
        });
        assert_eq!(
            extract_instructions(&recipe),
            "Mix ingredients\nBake at 350F"
        );
    }

    #[test]
    fn extract_instructions_missing_returns_empty() {
        let recipe = serde_json::json!({});
        assert_eq!(extract_instructions(&recipe), "");
    }

    // ------------------------------------------------------------------
    // preload_images
    // ------------------------------------------------------------------

    #[test]
    fn preload_images_ignores_non_image_entries() {
        let zip_bytes = make_test_zip(&[
            ("recipes.json", b"{}" as &[u8]),
            ("images/1.jpg", b"fake-jpeg"),
            ("other.txt", b"ignored"),
        ]);
        let cursor = Cursor::new(zip_bytes);
        let mut archive = ZipArchive::new(cursor).unwrap();
        let map = preload_images(&mut archive).unwrap();

        assert_eq!(map.len(), 1);
        assert_eq!(map.get("images/1.jpg").unwrap(), b"fake-jpeg");
    }

    /// Build a minimal in-memory ZIP for tests.
    fn make_test_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut buf);
            let options = zip::write::FileOptions::<()>::default()
                .compression_method(CompressionMethod::Stored);
            for (name, data) in entries {
                zip.start_file(*name, options).unwrap();
                zip.write_all(data).unwrap();
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    // ------------------------------------------------------------------
    // Route-level integration tests (async)
    // ------------------------------------------------------------------

    use crate::db::init_db;
    use ::image::ImageEncoder as _;
    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::{Method, Request, StatusCode};
    use axum::routing::{get, post};
    use serde_json::json;
    use std::sync::Arc;
    use tower::ServiceExt;
    struct RouteTestCtx {
        app: Router,
        state: Arc<AppState>,
        _dir: tempfile::TempDir,
    }

    async fn route_setup() -> RouteTestCtx {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = init_db(&db_path).await.expect("init_db");
        let state = Arc::new(AppState { pool });
        let app = Router::new()
            .route("/export/meals.zip", get(export_meals_zip))
            .route("/import/zip", post(import_meals_zip))
            .with_state(Arc::clone(&state));
        RouteTestCtx {
            app,
            state,
            _dir: dir,
        }
    }

    /// Helper: create a meal via direct DB insert for test setup.
    async fn insert_test_meal(
        pool: &sqlx::SqlitePool,
        name: &str,
        ingredients: &[(&str, Option<&str>)],
        instructions: &str,
        image: Option<&[u8]>,
    ) -> crate::model::Meal {
        let lines: Vec<crate::model::NewIngredientLine> = ingredients
            .iter()
            .map(|(n, q)| crate::model::NewIngredientLine {
                name: n.to_string(),
                quantity: q.map(|s| s.to_string()),
            })
            .collect();
        let image_change = match image {
            Some(bytes) => crate::db::ImageChange::Set(bytes),
            None => crate::db::ImageChange::Keep,
        };
        crate::db::insert_meal(
            pool,
            crate::model::NewMeal {
                name: name.into(),
                ingredients: lines,
                instructions: instructions.into(),
            },
            image_change,
        )
        .await
        .expect("insert_test_meal")
    }

    // --- export tests -------------------------------------------------------

    #[tokio::test]
    async fn export_empty_db_produces_valid_zip_with_empty_graph() {
        let ctx = route_setup().await;
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/export/meals.zip")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/zip"
        );
        let cd = response
            .headers()
            .get("content-disposition")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cd.starts_with("attachment; filename=\"mealme-export-"));
        assert!(cd.ends_with(".zip\""));

        let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let cursor = Cursor::new(body.as_ref());
        let mut archive = ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 1);

        let mut file = archive.by_name("recipes.json").unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(parsed["@context"], "https://schema.org");
        assert!(parsed["@graph"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn export_includes_images_and_recipe_data() {
        let ctx = route_setup().await;

        // Create a meal with an image
        let jpeg = make_test_jpeg();
        insert_test_meal(
            &ctx.state.pool,
            "Pasta",
            &[("pasta", Some("200g")), ("tomato", Some("3"))],
            "Boil and serve.",
            Some(&jpeg),
        )
        .await;

        // Create a meal without an image
        insert_test_meal(
            &ctx.state.pool,
            "Salad",
            &[("lettuce", None)],
            "Toss.",
            None,
        )
        .await;

        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/export/meals.zip")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let cursor = Cursor::new(body.as_ref());
        let mut archive = ZipArchive::new(cursor).unwrap();

        // recipes.json + 1 image
        assert_eq!(archive.len(), 2);

        let mut file = archive.by_name("recipes.json").unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let graph = parsed["@graph"].as_array().unwrap();
        assert_eq!(graph.len(), 2);

        // Find the Pasta recipe (has image) and Salad recipe (no image)
        let pasta = graph.iter().find(|r| r["name"] == "Pasta").unwrap();
        let salad = graph.iter().find(|r| r["name"] == "Salad").unwrap();

        assert_eq!(pasta["@type"], "Recipe");
        assert_eq!(pasta["recipeIngredient"].as_array().unwrap().len(), 2);
        assert_eq!(pasta["recipeInstructions"], "Boil and serve.");
        assert!(pasta["image"].as_str().unwrap().starts_with("images/"));

        assert!(salad.get("image").is_none());
    }

    // --- import tests -------------------------------------------------------

    /// Build a multipart body containing a single file field with zip bytes.
    fn build_zip_multipart(zip_bytes: &[u8]) -> (Vec<u8>, String) {
        let boundary = "ziptestboundary";
        let mut body = Vec::new();

        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"export.zip\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/zip\r\n\r\n");
        body.extend_from_slice(zip_bytes);
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");

        let content_type = format!("multipart/form-data; boundary={boundary}");
        (body, content_type)
    }

    #[tokio::test]
    async fn import_valid_zip_creates_meals() {
        let ctx = route_setup().await;

        let recipes_json = json!({
            "@context": "https://schema.org",
            "@graph": [
                {
                    "@context": "https://schema.org",
                    "@type": "Recipe",
                    "name": "Pancakes",
                    "recipeIngredient": ["1 cup flour", "1 egg"],
                    "recipeInstructions": "Mix and fry."
                },
                {
                    "@context": "https://schema.org",
                    "@type": "Recipe",
                    "name": "Omelette",
                    "recipeIngredient": ["2 eggs", "salt"],
                    "recipeInstructions": "Whisk and cook."
                }
            ]
        });

        let zip_bytes = make_test_zip(&[(
            "recipes.json",
            serde_json::to_string_pretty(&recipes_json)
                .unwrap()
                .as_bytes(),
        )]);

        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = to_bytes(response.into_body(), 10 * 1024).await.unwrap();
        let result: ZipImportResult = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(result.created.len(), 2);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed.len(), 0);
    }

    #[tokio::test]
    async fn import_skips_duplicate_names() {
        let ctx = route_setup().await;

        // Pre-create "Pizza"
        insert_test_meal(&ctx.state.pool, "Pizza", &[("dough", None)], "Bake.", None).await;

        let recipes_json = json!({
            "@context": "https://schema.org",
            "@graph": [
                {
                    "@context": "https://schema.org",
                    "@type": "Recipe",
                    "name": "Pizza",
                    "recipeIngredient": ["dough", "cheese"],
                    "recipeInstructions": "Bake."
                },
                {
                    "@context": "https://schema.org",
                    "@type": "Recipe",
                    "name": "Salad",
                    "recipeIngredient": ["lettuce"],
                    "recipeInstructions": "Toss."
                }
            ]
        });

        let zip_bytes = make_test_zip(&[(
            "recipes.json",
            serde_json::to_string_pretty(&recipes_json)
                .unwrap()
                .as_bytes(),
        )]);

        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = to_bytes(response.into_body(), 10 * 1024).await.unwrap();
        let result: ZipImportResult = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(result.created.len(), 1);
        assert_eq!(result.created[0].name, "Salad");
        assert_eq!(result.skipped, 1);
        assert_eq!(result.failed.len(), 0);
    }

    #[tokio::test]
    async fn import_missing_name_counts_as_failed() {
        let ctx = route_setup().await;

        let recipes_json = json!({
            "@context": "https://schema.org",
            "@graph": [
                {
                    "@context": "https://schema.org",
                    "@type": "Recipe",
                    "recipeIngredient": ["something"],
                    "recipeInstructions": "Do it."
                }
            ]
        });

        let zip_bytes = make_test_zip(&[(
            "recipes.json",
            serde_json::to_string_pretty(&recipes_json)
                .unwrap()
                .as_bytes(),
        )]);

        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = to_bytes(response.into_body(), 10 * 1024).await.unwrap();
        let result: ZipImportResult = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(result.created.len(), 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].source, "index 0 in @graph");
        assert!(result.failed[0].reason.contains("name"));
    }

    #[tokio::test]
    async fn import_rejects_malformed_json() {
        let ctx = route_setup().await;

        let zip_bytes = make_test_zip(&[("recipes.json", b"not json at all")]);
        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body_bytes = to_bytes(response.into_body(), 4096).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(err["error"].as_str().unwrap().contains("recipes.json"));
    }

    #[tokio::test]
    async fn import_rejects_zip_missing_recipes_json() {
        let ctx = route_setup().await;

        let zip_bytes = make_test_zip(&[("other.txt", b"not recipes")]);
        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn import_rejects_too_many_recipes() {
        let ctx = route_setup().await;

        let mut recipes: Vec<serde_json::Value> = Vec::new();
        for i in 0..501 {
            recipes.push(json!({
                "@context": "https://schema.org",
                "@type": "Recipe",
                "name": format!("Recipe {i}"),
                "recipeIngredient": ["item"],
                "recipeInstructions": "Do it."
            }));
        }

        let recipes_json = json!({
            "@context": "https://schema.org",
            "@graph": recipes
        });

        let zip_bytes = make_test_zip(&[(
            "recipes.json",
            serde_json::to_string_pretty(&recipes_json)
                .unwrap()
                .as_bytes(),
        )]);

        let (body, content_type) = build_zip_multipart(&zip_bytes);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/zip")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// Generate a minimal valid JPEG for test image data.
    fn make_test_jpeg() -> Vec<u8> {
        let img = ::image::RgbImage::from_pixel(4, 4, ::image::Rgb([128, 128, 128]));
        let mut buf = Cursor::new(Vec::new());
        let encoder = ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 82);
        encoder
            .write_image(img.as_raw(), 4, 4, ::image::ExtendedColorType::Rgb8)
            .unwrap();
        buf.into_inner()
    }
}
