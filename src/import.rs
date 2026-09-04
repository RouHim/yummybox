use base64::Engine;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Multipart, Query, State};
use axum::http::StatusCode;
use tracing::instrument;

use crate::db;
use crate::error::AppError;
use crate::image;
use crate::model::{BulkImportFailure, BulkImportRequest, BulkImportResult, Meal, NewMeal};
use crate::recipe;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Recipe import handlers
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportFromUrlRequest {
    pub url: String,
    /// Optional user-supplied image URL that takes precedence over the
    /// recipe's own image. Server-side download, best-effort.
    pub image_url: Option<String>,
}

#[instrument(skip(_state))]
pub(crate) async fn import_from_url(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImportFromUrlRequest>,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let trimmed_url = req.url.trim().to_string();
    if trimmed_url.is_empty() {
        return Err(AppError::BadRequest("url must not be empty".into()));
    }
    db::validate_source_url(Some(&trimmed_url))?;
    let mut draft = recipe::fetch_and_parse(&trimmed_url).await?;
    // Preserve the originate URL so the frontend can store/display it (normalized).
    draft.source_url = Some(trimmed_url);
    // User-supplied image URL takes precedence over the recipe's own image.
    // Best-effort: failure falls back to whatever fetch_and_parse returned.
    if let Some(image_url) = &req.image_url {
        if let Ok(jpeg) = recipe::download_image_from_url(image_url).await {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
            draft.image_base64 = Some(b64);
        }
    }
    Ok(Json(draft))
}
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportFromPasteRequest {
    pub content: String,
    /// Optional user-supplied image URL. Paste mode has no recipe image,
    /// so this is the only source for image data. Best-effort download.
    pub image_url: Option<String>,
}

#[instrument(skip(_state))]
pub(crate) async fn import_from_paste(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImportFromPasteRequest>,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let mut draft = recipe::parse_recipe(&req.content)?;
    // Paste mode has no recipe image; user-supplied URL is the only source.
    // Best-effort: failure means the draft stays with no image.
    if let Some(image_url) = &req.image_url {
        if let Ok(jpeg) = recipe::download_image_from_url(image_url).await {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
            draft.image_base64 = Some(b64);
        }
    }
    Ok(Json(draft))
}

// ---------------------------------------------------------------------------
// Image-from-URL handler
// ---------------------------------------------------------------------------

/// Request body for loading an image from a URL.
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageFromUrlRequest {
    pub url: String,
}

/// Response containing the base64-encoded JPEG.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageFromUrlResponse {
    pub image_base64: String,
}

/// Download an image from a URL, convert to JPEG (q82, max 3840px), and
/// return the bytes as a base64 string. All failures return structured errors.
#[instrument(skip(_state))]
pub(crate) async fn load_image_from_url(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImageFromUrlRequest>,
) -> Result<Json<ImageFromUrlResponse>, AppError> {
    let jpeg = recipe::download_image_from_url(&req.url).await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
    Ok(Json(ImageFromUrlResponse { image_base64: b64 }))
}

/// Map a multipart error to an `AppError`. Body-size-limit violations are
/// reported as 413 (they can surface from `next_field()`, `field.bytes()`, or
/// `field.text()` alike); everything else is a 400 with the given context.
pub(crate) fn map_multipart_error(
    e: axum::extract::multipart::MultipartError,
    what: &str,
) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::PayloadTooLarge(format!(
            "request body exceeds {} MB limit",
            crate::MAX_BODY_BYTES / (1024 * 1024)
        ))
    } else {
        AppError::BadRequest(format!("{what}: {e}"))
    }
}

#[instrument(skip(_state))]
pub(crate) async fn import_from_llm(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let mut model: Option<String> = None;
    let mut hint: Option<String> = None;
    let mut images: Vec<(Vec<u8>, Option<String>)> = Vec::new(); // (bytes, content_type) in multipart order
    let mut base_url: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| map_multipart_error(e, "invalid multipart data"))?
    {
        match field.name() {
            Some("model") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read model field"))?;
                model = Some(text);
            }
            Some("hint") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read hint field"))?;
                hint = Some(text);
            }
            Some("image") => {
                let content_type = field.content_type().map(String::from);
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read image field"))?;
                // A 0-byte file (e.g. picked in the AI-import dialog) is
                // tolerated and skipped, as in the pre-multipart code; the
                // all-empty case is rejected by the llm_images/hint check
                // below. generate_meal rejects empty images hard.
                if data.is_empty() {
                    continue;
                }
                images.push((data.to_vec(), content_type));
            }
            Some("base_url") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read base_url field"))?;
                base_url = Some(text);
            }
            Some("api_key") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read api_key field"))?;
                api_key = Some(text);
            }
            _ => {}
        }
    }

    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'model' field".into()))?;
    let hint = hint.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let base_url = base_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let api_key = api_key
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    // Only non-empty images count toward the cap — empty picker entries were
    // skipped during collection, so this runs on the post-filter list.
    if images.len() > MAX_GENERATE_IMAGES {
        return Err(AppError::BadRequest(format!(
            "maximum {MAX_GENERATE_IMAGES} images allowed"
        )));
    }

    let mut llm_images: Vec<crate::llm_import::LlmImage> = Vec::with_capacity(images.len());
    for (bytes, content_type) in images {
        // bytes are guaranteed non-empty here — empties were skipped in the
        // image arm during collection.
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(AppError::PayloadTooLarge(
                "image exceeds 20 MB limit".into(),
            ));
        }
        llm_images.push(crate::llm_import::LlmImage {
            bytes,
            content_type: content_type.unwrap_or_else(|| "image/jpeg".to_string()),
        });
    }
    if llm_images.is_empty() && hint.is_none() {
        return Err(AppError::BadRequest(
            "at least one of image or hint is required".into(),
        ));
    }

    if let Some(h) = &hint {
        if h.chars().count() > MAX_INGREDIENTS_CHARS {
            return Err(AppError::BadRequest(
                "hint must be at most 20000 characters".into(),
            ));
        }
    }

    // Preserve bare-URL hint as source_url for later display/edit.
    // (hint was trimmed above; trim again here so detection, fetch, and
    // store all see the same whitespace-free value.)
    let hint_original_for_source = hint
        .as_deref()
        .map(|h| h.trim().to_string())
        .filter(|s| recipe::is_bare_url(s));

    let hint = expand_hint_if_bare_url(hint).await?;

    let skip_image_download = !llm_images.is_empty();

    let mut draft = crate::llm_import::import_via_llm(
        &model,
        hint.as_deref(),
        llm_images,
        base_url.as_deref(),
        api_key.as_deref(),
        skip_image_download,
    )
    .await?;
    if let Some(url) = hint_original_for_source {
        // Best-effort: only preserve hints that pass source_url validation,
        // so overlong/malformed URLs don't fail later at save time.
        if db::validate_source_url(Some(&url)).is_ok() {
            draft.source_url = Some(url);
        }
    }
    Ok(Json(draft))
}

/// Maximum size of a single uploaded image (20 MB).
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Maximum number of ingredient photos accepted in one generation request,
/// for both the /import/generate route and the AI-import route (/import/llm).
const MAX_GENERATE_IMAGES: usize = 5;

/// Maximum length of the ingredients text field.
const MAX_INGREDIENTS_CHARS: usize = 20000;

/// Generate a recipe on the fly from an ingredient list and/or photos.
/// The LLM result is returned as a draft; nothing is persisted here.
#[instrument(skip(_state))]
pub(crate) async fn generate_meal(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let mut model: Option<String> = None;
    let mut ingredients: Option<String> = None;
    let mut images: Vec<crate::llm_import::LlmImage> = Vec::new();
    let mut base_url: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| map_multipart_error(e, "invalid multipart data"))?
    {
        match field.name() {
            Some("model") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read model field"))?;
                model = Some(text);
            }
            Some("ingredients") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read ingredients field"))?;
                ingredients = Some(text);
            }
            Some("image") => {
                let content_type = field.content_type().map(String::from);
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read image field"))?;
                if data.is_empty() {
                    return Err(AppError::BadRequest("image field is empty".into()));
                }
                let content_type = match content_type {
                    Some(ct) => ct,
                    None => {
                        return Err(AppError::BadRequest(
                            "unsupported image content type: missing Content-Type header".into(),
                        ));
                    }
                };
                if !matches!(
                    content_type.as_str(),
                    "image/jpeg" | "image/png" | "image/webp" | "image/gif"
                ) {
                    return Err(AppError::BadRequest(format!(
                        "unsupported image content type: {content_type}"
                    )));
                }
                images.push(crate::llm_import::LlmImage {
                    bytes: data.to_vec(),
                    content_type,
                });
            }
            Some("base_url") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read base_url field"))?;
                base_url = Some(text);
            }
            Some("api_key") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read api_key field"))?;
                api_key = Some(text);
            }
            _ => {}
        }
    }

    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'model' field".into()))?;
    let ingredients = ingredients
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let base_url = base_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let api_key = api_key
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if ingredients.is_none() && images.is_empty() {
        return Err(AppError::BadRequest(
            "at least one of ingredients or an image is required".into(),
        ));
    }

    if let Some(ing) = &ingredients {
        if ing.chars().count() > MAX_INGREDIENTS_CHARS {
            return Err(AppError::BadRequest(
                "ingredients must be at most 20000 characters".into(),
            ));
        }
    }

    if images.len() > MAX_GENERATE_IMAGES {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_GENERATE_IMAGES} images may be uploaded"
        )));
    }
    for img in &images {
        if img.bytes.len() > MAX_IMAGE_BYTES {
            return Err(AppError::PayloadTooLarge(
                "image exceeds 20 MB limit".into(),
            ));
        }
    }

    let draft = crate::llm_import::generate_meal_via_llm(
        &model,
        ingredients.as_deref(),
        images,
        base_url.as_deref(),
        api_key.as_deref(),
    )
    .await?;
    Ok(Json(draft))
}
/// If the hint is a bare URL, fetch the page server-side and expand to
/// readable text so the LLM can extract a recipe from it.
async fn expand_hint_if_bare_url(hint: Option<String>) -> Result<Option<String>, AppError> {
    let Some(h) = hint else {
        return Ok(None);
    };
    if !recipe::is_bare_url(&h) {
        return Ok(Some(h));
    }
    let html = recipe::fetch_page_html(&h).await?;
    let text = recipe::extract_readable_text(&html);
    if text.trim().is_empty() {
        return Err(AppError::BadRequest(
            "URL returned no extractable text".into(),
        ));
    }
    let mut prompt = format!("Recipe from {h}:\n{text}");
    let image_urls = recipe::extract_image_urls_from_html(&html, &h);
    if !image_urls.is_empty() {
        prompt.push_str(&format!(
            "\n\nCandidate dish image URLs found on the page:\n{}",
            image_urls.join("\n")
        ));
    }
    Ok(Some(prompt))
}
// ---------------------------------------------------------------------------
// Polish instructions handler
// ---------------------------------------------------------------------------

#[instrument(skip(_state))]
pub(crate) async fn polish_instructions(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut model: Option<String> = None;
    let mut name: Option<String> = None;
    let mut ingredients_json: Option<String> = None;
    let mut instructions: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| map_multipart_error(e, "invalid multipart data"))?
    {
        match field.name() {
            Some("model") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read model field"))?;
                model = Some(text);
            }
            Some("name") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read name field"))?;
                name = Some(text);
            }
            Some("ingredients") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read ingredients field"))?;
                ingredients_json = Some(text);
            }
            Some("instructions") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read instructions field"))?;
                instructions = Some(text);
            }
            Some("base_url") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read base_url field"))?;
                base_url = Some(text);
            }
            Some("api_key") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| map_multipart_error(e, "failed to read api_key field"))?;
                api_key = Some(text);
            }
            _ => {}
        }
    }

    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'model' field".into()))?;
    let name = name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'name' field".into()))?;
    let ingredients_json = ingredients_json
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'ingredients' field".into()))?;
    let instructions = instructions
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'instructions' field".into()))?;
    let base_url = base_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let api_key = api_key
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let ingredients: Vec<crate::model::NewIngredientLine> = serde_json::from_str(&ingredients_json)
        .map_err(|e| AppError::BadRequest(format!("invalid ingredients JSON: {e}")))?;

    let polished = crate::llm_import::polish_instructions(
        &model,
        &name,
        &ingredients,
        &instructions,
        base_url.as_deref(),
        api_key.as_deref(),
    )
    .await?;
    Ok(Json(serde_json::json!({ "instructions": polished })))
}

// ---------------------------------------------------------------------------
// Bulk URL import handler
// ---------------------------------------------------------------------------

/// Maximum number of URLs accepted in a single bulk import request.
const BULK_IMPORT_MAX_URLS: usize = 50;

#[instrument(skip(state))]
pub(crate) async fn import_bulk(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkImportRequest>,
) -> Result<(StatusCode, Json<BulkImportResult>), AppError> {
    let urls: Vec<&str> = req
        .urls
        .iter()
        .map(|u| u.trim())
        .filter(|u| !u.is_empty())
        .collect();

    if urls.len() > BULK_IMPORT_MAX_URLS {
        return Err(AppError::BadRequest(format!(
            "maximum {} URLs allowed",
            BULK_IMPORT_MAX_URLS
        )));
    }

    let mut created: Vec<Meal> = Vec::new();
    let mut failed: Vec<BulkImportFailure> = Vec::new();

    for url in &urls {
        match process_single_url(&state.pool, url).await {
            Ok(meal) => created.push(meal),
            Err(reason) => failed.push(BulkImportFailure {
                url: url.to_string(),
                reason,
            }),
        }
    }

    Ok((StatusCode::OK, Json(BulkImportResult { created, failed })))
}

/// Process a single URL: fetch, parse, validate, insert. Returns the created
/// [`Meal`] on success or a human-readable failure reason on error.
async fn process_single_url(pool: &sqlx::SqlitePool, url: &str) -> Result<Meal, String> {
    let draft = recipe::fetch_and_parse(url).await.map_err(|e| {
        tracing::warn!(url = %url, error = %e, "bulk import: fetch_and_parse failed");
        classify_fetch_error(&e)
    })?;

    // Best-effort metadata: downgrade an invalid source_url to None instead
    // of failing the bulk item (consistent with the ZIP importer).
    let source_url = if db::validate_source_url(Some(url)).is_ok() {
        Some(url.to_string())
    } else {
        None
    };
    let new_meal = NewMeal {
        name: draft.name,
        ingredients: draft.ingredients,
        instructions: draft.instructions,
        portions: draft.portions,
        source_url,
    };

    if db::meal_name_exists(pool, &new_meal.name, None)
        .await
        .map_err(|e| format!("database error: {e}"))?
    {
        return Err("duplicate".to_string());
    }

    // Decode and convert the image (if present) into owned JPEG bytes so the
    // borrow lives long enough for ImageChange::Set.
    let jpeg_bytes: Option<Vec<u8>> = if let Some(b64) = &draft.image_base64 {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .map_err(|e| format!("image decode error: {e}"))?;
        // Re-encode via convert_to_jpeg as a safety net (ensures consistent
        // encoding even if the upstream downloader changes).
        let jpeg =
            image::convert_to_jpeg(&bytes).map_err(|e| format!("image conversion error: {e}"))?;
        Some(jpeg)
    } else {
        None
    };

    let image_change = match &jpeg_bytes {
        Some(bytes) => db::ImageChange::Set(bytes),
        None => db::ImageChange::Keep,
    };

    db::insert_meal(pool, new_meal, image_change)
        .await
        .map_err(|e| {
            tracing::warn!(url = %url, error = %e, "bulk import: insert_meal failed");
            classify_insert_error(&e)
        })
}

/// Map an [`AppError`] from `fetch_and_parse` to a user-facing reason string.
pub(crate) fn classify_fetch_error(err: &AppError) -> String {
    match err {
        AppError::NotFound => "no recipe found".into(),
        AppError::BadRequest(msg) if msg.starts_with("fetch returned HTTP ") => {
            // Surface the HTTP status so users can distinguish 404 from 500 etc.
            msg.clone()
        }
        _ => format!("fetch failed: {err}"),
    }
}

/// Map an [`AppError`] from `insert_meal` / `validate_meal` to a user-facing reason string.
pub(crate) fn classify_insert_error(err: &AppError) -> String {
    match err {
        AppError::Validation(_) => "validation failed".into(),
        _ => err.to_string(),
    }
}

// ---------------------------------------------------------------------------
// LLM info handlers
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
pub(crate) struct ModelsQuery {
    pub(crate) provider: String,
    pub(crate) base_url: Option<String>,
    pub(crate) api_key: Option<String>,
}
#[instrument(skip(_state))]
pub(crate) async fn llm_providers(
    State(_state): State<Arc<AppState>>,
) -> Json<crate::llm_import::LlmProvidersResponse> {
    Json(crate::llm_import::LlmProvidersResponse {
        providers: crate::llm_import::list_providers(),
    })
}

#[instrument(skip(_state))]
pub(crate) async fn llm_models(
    State(_state): State<Arc<AppState>>,
    Query(q): Query<ModelsQuery>,
) -> Result<Json<crate::llm_import::LlmModelsResponse>, AppError> {
    let models =
        crate::llm_import::list_models(&q.provider, q.base_url.as_deref(), q.api_key.as_deref())
            .await?;
    Ok(Json(crate::llm_import::LlmModelsResponse { models }))
}
