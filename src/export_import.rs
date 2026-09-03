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
use crate::import::map_multipart_error;
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
    let filename = format!("yummybox-export-{today}.zip");

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
pub(crate) fn build_recipe_json(meal: &Meal, images: &[(i64, Vec<u8>)]) -> serde_json::Value {
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

    if let Some(p) = meal.portions {
        obj.insert("recipeYield".into(), Value::String(p.to_string()));
    }

    if let Some(url) = &meal.source_url {
        let trimmed = url.trim();
        if !trimmed.is_empty() {
            obj.insert("url".into(), Value::String(trimmed.to_string()));
        }
    }

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
pub(crate) fn build_export_zip(
    json_str: &str,
    images: &[(i64, Vec<u8>)],
) -> Result<Vec<u8>, AppError> {
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
        .map_err(|e| map_multipart_error(e, "invalid multipart data"))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| map_multipart_error(e, "failed to read file"))?;
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
pub(crate) fn preload_images<R: Read + std::io::Seek>(
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
    let normalized = db::normalize_meal_name(trimmed_name);
    let existing = db::list_meals(&state.pool, None)
        .await
        .map_err(|e| format!("database error: {e}"))?;
    if existing
        .iter()
        .any(|m| db::normalize_meal_name(&m.name) == normalized)
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

    let instructions = extract_instructions(recipe);

    let portions = recipe
        .get("recipeYield")
        .or_else(|| recipe.get("yield"))
        .and_then(|v| {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => return None,
            };
            let num_str: String = s
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            num_str.parse::<i32>().ok()
        });

    // --- source URL (optional, e.g. original recipe link) --------------------
    fn extract_url_string(v: &serde_json::Value) -> Option<String> {
        match v {
            serde_json::Value::String(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            serde_json::Value::Object(map) => {
                for key in ["@id", "url", "href"] {
                    if let Some(serde_json::Value::String(s)) = map.get(key) {
                        let t = s.trim();
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                    }
                }
                None
            }
            serde_json::Value::Array(arr) => {
                for elem in arr {
                    if let Some(s) = extract_url_string(elem) {
                        return Some(s);
                    }
                }
                None
            }
            _ => None,
        }
    }
    let source_url = recipe
        .get("url")
        .or_else(|| recipe.get("mainEntityOfPage"))
        .or_else(|| recipe.get("isBasedOnUrl"))
        .and_then(extract_url_string)
        .filter(|s| !s.is_empty());
    // --- validate -----------------------------------------------------------
    if let Err(e) = db::validate_meal(
        trimmed_name,
        &ingredients,
        &instructions,
        portions,
        source_url.as_deref(),
    ) {
        let msg = e.to_string();
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
        portions,
        source_url,
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
pub(crate) fn extract_ingredient_strings(recipe: &serde_json::Value) -> Option<Vec<String>> {
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
pub(crate) fn extract_instructions(recipe: &serde_json::Value) -> String {
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
