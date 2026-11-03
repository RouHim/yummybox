use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use zip::CompressionMethod;
use zip::write::ZipWriter;

use crate::db;
use crate::error::AppError;
use crate::intake;
use crate::jsonld;
use crate::model::{Meal, NewIngredientLine, NewMeal};
use crate::recipe;
use crate::state::AppState;
use crate::zip_intake;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

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
        if meal.has_image
            && let Some((bytes, _)) = db::find_meal_image(&state.pool, meal.id).await?
        {
            images.push((meal.id, bytes));
        }
    }

    // Build JSON-LD graph — one Recipe per meal, image paths relative to the
    // archive. A meal whose image blob is missing from the export omits the
    // image, so the JSON never points at a file that is not in the archive.
    let recipes: Vec<serde_json::Value> = meals
        .iter()
        .map(|meal| {
            let image = if images.iter().any(|(id, _)| *id == meal.id) {
                jsonld::ImageRef::Relative("images")
            } else {
                jsonld::ImageRef::Omit
            };
            jsonld::meal_to_recipe(meal, image)
        })
        .collect();

    let jsonld = serde_json::json!({
        "@context": jsonld::SCHEMA_CONTEXT,
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
    let mut archive = zip_intake::open(&mut multipart).await?;

    // Read recipes.json
    let recipes_json = zip_intake::read_string(&mut archive, "recipes.json")?;

    // Pre-load all image entries into memory (avoids borrow issues with the archive)
    let image_map = zip_intake::read_images(&mut archive, &|name| name.starts_with("images/"))?;

    // Parse the JSON-LD graph
    let graph: serde_json::Value = serde_json::from_str(&recipes_json)
        .map_err(|e| AppError::BadRequest(format!("invalid recipes.json: {e}")))?;

    let recipes = graph
        .get("@graph")
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            AppError::BadRequest("recipes.json must be an object with an @graph array".into())
        })?;

    if recipes.len() > zip_intake::MAX_RECIPES {
        return Err(AppError::BadRequest(format!(
            "too many recipes: maximum {} allowed, got {}",
            zip_intake::MAX_RECIPES,
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

    // Duplicates are skipped before parsing, so a malformed copy of an
    // existing recipe is still reported as skipped rather than failed.
    if intake::name_taken(&state.pool, trimmed_name)
        .await
        .map_err(|e| format!("database error: {e}"))?
    {
        return Ok(None);
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
    fn collect_url_strings(v: &serde_json::Value, out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) => {
                let t = s.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
            serde_json::Value::Object(map) => {
                for key in ["@id", "url", "href"] {
                    if let Some(sub) = map.get(key) {
                        collect_url_strings(sub, out);
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for elem in arr {
                    collect_url_strings(elem, out);
                }
            }
            _ => {}
        }
    }
    // Optional link is best-effort: keep the first candidate that passes
    // validation, else None — so an invalid `url` doesn't shadow a valid
    // `mainEntityOfPage` fallback (consistent with portions/image handling).
    let source_url = ["url", "mainEntityOfPage", "isBasedOnUrl"]
        .iter()
        .flat_map(|key| {
            recipe
                .get(*key)
                .map(|v| {
                    let mut out = Vec::new();
                    collect_url_strings(v, &mut out);
                    out
                })
                .unwrap_or_default()
        })
        .filter(|s| !s.is_empty())
        .find(|s| db::validate_source_url(Some(s)).is_ok());
    // --- image (best-effort) ------------------------------------------------
    let image_bytes = recipe
        .get("image")
        .and_then(|v| v.as_str())
        .and_then(|img_path| image_map.get(&img_path.to_ascii_lowercase()));
    let image = match image_bytes {
        Some(raw) => intake::ImageInput::TryAny(&[raw]),
        None => intake::ImageInput::Keep,
    };

    let new_meal = NewMeal {
        name: trimmed_name.to_string(),
        ingredients,
        instructions,
        portions,
        source_url,
    };
    match intake::insert(&state.pool, new_meal, image).await {
        Ok(intake::IntakeOutcome::Created(meal)) => Ok(Some(meal)),
        Ok(intake::IntakeOutcome::Duplicate) => Ok(None),
        Err(e) => Err(intake::failure_reason(&e)),
    }
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
