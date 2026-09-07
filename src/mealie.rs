//! Mealie ZIP import.
//!
//! Accepts a Mealie backup archive (`recipes/<slug>/<slug>.json` plus
//! `recipes/<slug>/images/{original,min-original}.webp`) and converts each
//! entry to a [`Meal`](crate::model::Meal) via the shared validation/insert path.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, State};
use axum::http::StatusCode;
use tracing::instrument;
use zip::read::ZipArchive;

use crate::db;
use crate::error::AppError;
use crate::export_import::{ZipImportFailure, ZipImportResult};
use crate::image;
use crate::import::map_multipart_error;
use crate::model::{Meal, NewIngredientLine, NewMeal};
use crate::recipe;
use crate::state::AppState;

const IMPORT_MAX_ARCHIVE_SIZE: u64 = 50 * 1024 * 1024;
const IMPORT_MAX_RECIPES: usize = 500;
const IMPORT_MAX_IMAGE_SIZE: u64 = 20 * 1024 * 1024;

/// `POST /api/import/mealie`.
///
/// Every valid recipe is persisted immediately; duplicates are skipped
/// (case-insensitive name match); validation failures are counted as
/// `failed` with the archive path as source.
#[instrument(skip(state))]
pub async fn import_mealie_zip(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ZipImportResult>), AppError> {
    let zip_bytes = read_zip_file_from_multipart(&mut multipart).await?;

    if zip_bytes.len() as u64 > IMPORT_MAX_ARCHIVE_SIZE {
        return Err(AppError::PayloadTooLarge(format!(
            "archive exceeds maximum size of {} MB",
            IMPORT_MAX_ARCHIVE_SIZE / (1024 * 1024)
        )));
    }

    let cursor = Cursor::new(&zip_bytes[..]);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|e| AppError::BadRequest(format!("invalid zip file: {e}")))?;

    let recipe_paths: Vec<String> = archive
        .file_names()
        .filter(|n| is_mealie_recipe_entry(n))
        .map(str::to_string)
        .collect();

    if recipe_paths.is_empty() {
        return Err(AppError::BadRequest(
            "zip must contain Mealie recipes at recipes/*/*.json".into(),
        ));
    }

    if recipe_paths.len() > IMPORT_MAX_RECIPES {
        return Err(AppError::BadRequest(format!(
            "too many recipes: maximum {IMPORT_MAX_RECIPES} allowed, got {}",
            recipe_paths.len()
        )));
    }

    let image_map = preload_mealie_images(&mut archive)?;

    let mut created: Vec<Meal> = Vec::new();
    let mut skipped: usize = 0;
    let mut failed: Vec<ZipImportFailure> = Vec::new();

    for path in &recipe_paths {
        let source = path.clone();
        let text = match read_archive_entry_to_string(&mut archive, path) {
            Ok(t) => t,
            Err(e) => {
                failed.push(ZipImportFailure {
                    source,
                    reason: format!("failed to read entry: {e}"),
                });
                continue;
            }
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                failed.push(ZipImportFailure {
                    source,
                    reason: format!("invalid recipe JSON: {e}"),
                });
                continue;
            }
        };
        let slug = recipe_slug_from_path(path).unwrap_or_else(|| source.clone());
        match import_single_mealie_recipe(&state, &value, &slug, &image_map).await {
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

/// Ingredient lines from a Mealie recipe value (`recipe_ingredient[].note`,
/// falling back to `original_text`, then structured `quantity`/`unit`/`food`).
pub(crate) fn parse_ingredient_lines(recipe: &serde_json::Value) -> Option<Vec<String>> {
    match recipe.get("recipe_ingredient") {
        Some(serde_json::Value::Array(arr)) => {
            let mut lines = Vec::with_capacity(arr.len());
            for item in arr {
                match item {
                    serde_json::Value::String(s) => {
                        let t = s.trim();
                        if !t.is_empty() {
                            lines.push(t.to_string());
                        }
                    }
                    serde_json::Value::Object(_) => {
                        if let Some(line) = ingredient_text_from_object(item) {
                            lines.push(line);
                        }
                    }
                    _ => {}
                }
            }
            if lines.is_empty() { None } else { Some(lines) }
        }
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(vec![t.to_string()])
            }
        }
        _ => None,
    }
}

/// Best-effort text for one `recipe_ingredient` object.
fn ingredient_text_from_object(item: &serde_json::Value) -> Option<String> {
    for key in ["note", "original_text"] {
        if let Some(s) = item.get(key).and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    // Structured fallback: "<quantity> <unit> <food>".
    let food = match item.get("food") {
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Some(serde_json::Value::Object(map)) => map
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    };
    let food = food?;
    let mut parts = Vec::with_capacity(3);
    if let Some(q) = item.get("quantity") {
        let qs = match q {
            serde_json::Value::Number(n) => n.as_f64().map(|f| {
                if f.fract() == 0.0 {
                    format!("{}", f as i64)
                } else {
                    n.to_string()
                }
            }),
            serde_json::Value::String(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                }
            }
            _ => None,
        };
        if let Some(qs) = qs {
            parts.push(qs);
        }
    }
    if let Some(unit) = unit_text(item.get("unit")) {
        parts.push(unit);
    }
    parts.push(food);
    Some(parts.join(" "))
}

fn unit_text(unit: Option<&serde_json::Value>) -> Option<String> {
    match unit {
        Some(serde_json::Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Some(serde_json::Value::Object(map)) => ["abbreviation", "name"]
            .iter()
            .filter_map(|k| map.get(*k).and_then(|v| v.as_str()))
            .map(str::trim)
            .find(|s| !s.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Instructions from a Mealie recipe value (`recipe_instructions[].text`,
/// joined with a blank line). Python-repr `HowToSection` strings are
/// unwrapped best-effort; missing input yields an empty string (the caller
/// surfaces it as a validation failure).
pub(crate) fn parse_instructions(recipe: &serde_json::Value) -> String {
    match recipe.get("recipe_instructions") {
        Some(serde_json::Value::String(s)) => normalize_step_text(s),
        Some(serde_json::Value::Array(steps)) => {
            let mut out = Vec::with_capacity(steps.len());
            for step in steps {
                if let Some(s) = step.as_str() {
                    let n = normalize_step_text(s);
                    if !n.trim().is_empty() {
                        out.push(n);
                    }
                    continue;
                }
                if let Some(t) = step.get("text").and_then(|v| v.as_str()) {
                    let n = normalize_step_text(t);
                    if !n.trim().is_empty() {
                        out.push(n);
                    }
                }
            }
            out.join("\n\n")
        }
        _ => String::new(),
    }
}

fn normalize_step_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Some(texts) = extract_python_dict_texts(trimmed) {
            if !texts.is_empty() {
                return texts.join("\n\n");
            }
        }
    }
    text.to_string()
}

/// Portions from a Mealie `recipe_yield` value (`"2 serving(s)"`).
pub(crate) fn parse_portions(recipe: &serde_json::Value) -> Option<i32> {
    let v = recipe.get("recipe_yield").or_else(|| recipe.get("yield"))?;
    let s = match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let digits: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let n: i32 = digits.parse().ok()?;
    if n <= 0 { None } else { Some(n) }
}

/// Source URL from a Mealie `org_url` field (validated, else `None`).
pub(crate) fn parse_source_url(recipe: &serde_json::Value) -> Option<String> {
    let s = recipe.get("org_url").and_then(|v| v.as_str())?;
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if db::validate_source_url(Some(t)).is_ok() {
        Some(t.to_string())
    } else {
        None
    }
}

/// Candidate image paths for a recipe slug, best first.
pub(crate) fn mealie_image_candidates(slug: &str) -> [String; 2] {
    [
        format!("recipes/{slug}/images/original.webp"),
        format!("recipes/{slug}/images/min-original.webp"),
    ]
}

/// Best-effort extraction of inner `text` values from a Python-repr
/// `HowToSection` string. Returns `None` when the input is plain text.
pub(crate) fn extract_python_dict_texts(s: &str) -> Option<Vec<String>> {
    if !s.contains("text") {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let key = match find_text_key(s, i) {
            Some(k) => k,
            None => break,
        };
        let mut j = key;
        // Skip whitespace, expect ':', skip whitespace, expect quote.
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b':' {
            i = key + 1;
            continue;
        }
        j += 1;
        while j < bytes.len() && (bytes[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || (bytes[j] != b'\'' && bytes[j] != b'"') {
            i = key + 1;
            continue;
        }
        let quote = bytes[j];
        j += 1;
        let mut raw = String::new();
        let mut closed = false;
        while j < bytes.len() {
            let c = bytes[j];
            if c == b'\\' && j + 1 < bytes.len() {
                raw.push('\\');
                raw.push(bytes[j + 1] as char);
                j += 2;
                continue;
            }
            if c == quote {
                closed = true;
                j += 1;
                break;
            }
            raw.push(c as char);
            j += 1;
        }
        if closed {
            let unescaped = unescape_python_string(&raw);
            if !unescaped.trim().is_empty() {
                out.push(unescaped);
            }
            i = j;
        } else {
            break;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Locate the next `'text'` or `"text"` key at or after `from`.
/// Returns the byte index just past the closing quote of the key.
fn find_text_key(s: &str, from: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i + 5 < bytes.len() {
        let c = bytes[i];
        if c == b'\'' || c == b'"' {
            let quote = c;
            if s[i + 1..].starts_with("text") {
                let end = i + 5;
                if end < bytes.len() && bytes[end] == quote {
                    return Some(end + 1);
                }
            }
        }
        i += 1;
    }
    None
}

/// Unescape Python string escape sequences (`\n`, `\'`, `\xNN`, `\uNNNN`).
fn unescape_python_string(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            out.push('\\');
            break;
        }
        match bytes[i] as char {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '\\' => out.push('\\'),
            '\'' => out.push('\''),
            '"' => out.push('"'),
            '0' => out.push('\0'),
            'x' => {
                let mut done = false;
                if i + 2 < bytes.len() {
                    if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(n) {
                                out.push(ch);
                                i += 2;
                                done = true;
                            }
                        }
                    }
                }
                if !done {
                    out.push('x');
                }
            }
            'u' => {
                let mut done = false;
                if i + 4 < bytes.len() {
                    if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 5]) {
                        if let Ok(n) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(n) {
                                out.push(ch);
                                i += 4;
                                done = true;
                            }
                        }
                    }
                }
                if !done {
                    out.push('u');
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

fn is_mealie_recipe_entry(name: &str) -> bool {
    name.starts_with("recipes/") && name.ends_with(".json")
}

fn recipe_slug_from_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    if parts.next()? != "recipes" {
        return None;
    }
    let slug = parts.next()?;
    if slug.is_empty() {
        return None;
    }
    Some(slug.to_string())
}

fn is_mealie_image_entry(name: &str) -> bool {
    if !name.starts_with("recipes/") || !name.contains("/images/") {
        return false;
    }
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".webp")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
}

fn read_archive_entry_to_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String, String> {
    let mut file = archive.by_name(name).map_err(|e| e.to_string())?;
    let mut s = String::new();
    file.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn preload_mealie_images<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
) -> Result<HashMap<String, Vec<u8>>, AppError> {
    let mut map = HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::BadRequest(format!("zip read error: {e}")))?;
        let name = entry.name().to_string();
        if !is_mealie_image_entry(&name) {
            continue;
        }
        if entry.size() > IMPORT_MAX_IMAGE_SIZE {
            continue;
        }
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| AppError::BadRequest(format!("failed to read {name}: {e}")))?;
        map.insert(name, buf);
    }
    Ok(map)
}

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

async fn import_single_mealie_recipe(
    state: &Arc<AppState>,
    recipe: &serde_json::Value,
    slug: &str,
    image_map: &HashMap<String, Vec<u8>>,
) -> Result<Option<Meal>, String> {
    let name = recipe
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'name' field".to_string())?;
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("'name' field is empty".to_string());
    }

    let normalized = db::normalize_meal_name(trimmed_name);
    let existing = db::list_meals(&state.pool, None)
        .await
        .map_err(|e| format!("database error: {e}"))?;
    if existing
        .iter()
        .any(|m| db::normalize_meal_name(&m.name) == normalized)
    {
        return Ok(None);
    }

    let ingredient_lines = parse_ingredient_lines(recipe)
        .ok_or_else(|| "missing or invalid 'recipe_ingredient' field".to_string())?;
    let ingredients: Vec<NewIngredientLine> = ingredient_lines
        .iter()
        .map(|line| recipe::split_ingredient_line(line))
        .collect();

    let instructions = parse_instructions(recipe);
    let portions = parse_portions(recipe);
    let source_url = parse_source_url(recipe);

    if let Err(e) = db::validate_meal(
        trimmed_name,
        &ingredients,
        &instructions,
        portions,
        source_url.as_deref(),
    ) {
        return Err(format!("validation failed: {e}"));
    }

    let jpeg_bytes: Option<Vec<u8>> = mealie_image_candidates(slug)
        .iter()
        .filter_map(|path| image_map.get(path))
        .find_map(|raw| image::convert_to_jpeg(raw).ok());

    let image_change = match &jpeg_bytes {
        Some(bytes) => db::ImageChange::Set(bytes),
        None => db::ImageChange::Keep,
    };

    let meal = db::insert_meal(
        &state.pool,
        NewMeal {
            name: trimmed_name.to_string(),
            ingredients,
            instructions,
            portions,
            source_url,
        },
        image_change,
    )
    .await
    .map_err(|e| format!("database error: {e}"))?;

    Ok(Some(meal))
}
