use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::multipart::Field;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tracing::instrument;

use crate::bring;
use crate::db;
use crate::error::AppError;
use crate::image;
use crate::jsonld;
use crate::model::{Meal, MealPatch, NewMeal, NewPlanRequest, Plan, PlanPatch, PlanSummaryItem};
use crate::state::AppState;

/// Returns `true` when the request's `Accept` header contains
/// `application/ld+json` (simple substring match — no q-value parsing).
fn wants_jsonld(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("application/ld+json"))
        .unwrap_or(false)
}

/// Derive a base URL from the `Host` header, defaulting the scheme to `http`.
fn base_url(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| format!("http://{h}"))
}

/// Build an `application/ld+json` response with the correct Content-Type header.
fn jsonld_response(value: serde_json::Value) -> Response {
    let mut resp = Json(value).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/ld+json"),
    );
    resp
}

// ---------------------------------------------------------------------------
// Shared multipart parser for meal create / update
// ---------------------------------------------------------------------------

struct ParsedMealForm {
    name: String,
    ingredients_json: String,
    instructions: String,
    portions: Option<i32>,
    image_data: Option<Vec<u8>>,
    image_action: Option<String>,
}

async fn parse_meal_multipart(mut multipart: Multipart) -> Result<ParsedMealForm, AppError> {
    let mut name: Option<String> = None;
    let mut ingredients_raw: Option<String> = None;
    let mut instructions: Option<String> = None;
    let mut portions: Option<i32> = None;
    let mut image_data: Option<Vec<u8>> = None;
    let mut image_action: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| crate::import::map_multipart_error(e, "invalid multipart data"))?
    {
        match field.name() {
            Some("name") => {
                name = Some(read_text_field(field, "name").await?);
            }
            Some("ingredients") => {
                ingredients_raw = Some(read_text_field(field, "ingredients").await?);
            }
            Some("instructions") => {
                instructions = Some(read_text_field(field, "instructions").await?);
            }
            Some("portions") => {
                let text = read_text_field(field, "portions").await?;
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    portions =
                        Some(trimmed.parse::<i32>().map_err(|_| {
                            AppError::BadRequest("portions must be an integer".into())
                        })?);
                }
            }
            Some("image") => {
                if image_data.is_some() {
                    return Err(AppError::BadRequest(
                        "only one image may be uploaded".into(),
                    ));
                }
                image_data = Some(read_bytes_field(field, "image").await?);
            }
            Some("image_action") => {
                image_action = Some(read_text_field(field, "image_action").await?);
            }
            _ => {} // ignore unknown fields
        }
    }

    let name = name.ok_or_else(|| AppError::BadRequest("missing 'name' field".into()))?;
    let ingredients_json = ingredients_raw
        .ok_or_else(|| AppError::BadRequest("missing 'ingredients' field".into()))?;
    let instructions =
        instructions.ok_or_else(|| AppError::BadRequest("missing 'instructions' field".into()))?;

    Ok(ParsedMealForm {
        name,
        ingredients_json,
        instructions,
        portions,
        image_data,
        image_action,
    })
}

/// Read a multipart text field, mapping read errors to an `AppError`
/// (body-size-limit violations are reported as 413, like the import routes).
async fn read_text_field(field: Field<'_>, field_name: &str) -> Result<String, AppError> {
    field.text().await.map_err(|e| {
        crate::import::map_multipart_error(e, &format!("failed to read {field_name} field"))
    })
}

/// Read a multipart binary field, mapping read errors to an `AppError`
/// (body-size-limit violations are reported as 413, like the import routes).
async fn read_bytes_field(field: Field<'_>, field_name: &str) -> Result<Vec<u8>, AppError> {
    let data = field.bytes().await.map_err(|e| {
        crate::import::map_multipart_error(e, &format!("failed to read {field_name} field"))
    })?;
    Ok(data.to_vec())
}

#[instrument(skip(state))]
pub async fn list_meals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let search = params.get("search").map(String::as_str);
    let meals = db::list_meals(&state.pool, search).await?;
    if wants_jsonld(&headers) {
        let base = base_url(&headers);
        Ok(jsonld_response(jsonld::meals_to_graph(
            &meals,
            base.as_deref(),
        )))
    } else {
        Ok(Json(meals).into_response())
    }
}

#[instrument(skip(state))]
pub async fn get_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let meal = db::find_meal(&state.pool, id).await?;
    if wants_jsonld(&headers) {
        let base = base_url(&headers);
        Ok(jsonld_response(jsonld::meal_to_recipe(
            &meal,
            base.as_deref(),
        )))
    } else {
        Ok(Json(meal).into_response())
    }
}

#[instrument(skip(state))]
pub async fn create_meal(
    State(state): State<Arc<AppState>>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<Meal>), AppError> {
    let parsed = parse_meal_multipart(multipart).await?;

    let ingredients: Vec<crate::model::NewIngredientLine> =
        serde_json::from_str(&parsed.ingredients_json)
            .map_err(|e| AppError::BadRequest(format!("invalid ingredients JSON: {e}")))?;

    let jpeg_bytes;
    let image = match parsed.image_data {
        Some(bytes) => {
            jpeg_bytes = image::convert_to_jpeg(&bytes)?;
            db::ImageChange::Set(&jpeg_bytes)
        }
        None => db::ImageChange::Keep,
    };

    let new = NewMeal {
        name: parsed.name,
        ingredients,
        instructions: parsed.instructions,
        portions: parsed.portions,
    };
    if db::meal_name_exists(&state.pool, &new.name, None).await? {
        return Err(AppError::DuplicateName);
    }
    let meal = db::insert_meal(&state.pool, new, image).await?;
    Ok((StatusCode::CREATED, Json(meal)))
}

#[instrument(skip(state))]
pub async fn update_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> Result<Json<Meal>, AppError> {
    let parsed = parse_meal_multipart(multipart).await?;

    let ingredients: Vec<crate::model::NewIngredientLine> =
        serde_json::from_str(&parsed.ingredients_json)
            .map_err(|e| AppError::BadRequest(format!("invalid ingredients JSON: {e}")))?;

    // Validate image_action if present
    if let Some(action) = &parsed.image_action {
        if action != "remove" {
            return Err(AppError::BadRequest("image_action must be 'remove'".into()));
        }
    }

    let jpeg_bytes;
    let image = match (parsed.image_data, parsed.image_action.as_deref()) {
        (Some(bytes), _) => {
            jpeg_bytes = image::convert_to_jpeg(&bytes)?;
            db::ImageChange::Set(&jpeg_bytes)
        }
        (None, Some("remove")) => db::ImageChange::Clear,
        _ => db::ImageChange::Keep,
    };

    let patch = MealPatch {
        name: parsed.name,
        ingredients,
        instructions: parsed.instructions,
        portions: parsed.portions,
    };
    if db::meal_name_exists(&state.pool, &patch.name, Some(id)).await? {
        return Err(AppError::DuplicateName);
    }
    let meal = db::update_meal(&state.pool, id, patch, image).await?;
    Ok(Json(meal))
}

#[instrument(skip(state))]
pub async fn delete_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    db::delete_meal(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Image handler
// ---------------------------------------------------------------------------

#[instrument(skip(state))]
pub async fn get_meal_image(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    match db::find_meal_image(&state.pool, id).await? {
        Some((bytes, content_type)) => {
            Ok(([(header::CONTENT_TYPE, content_type)], bytes).into_response())
        }
        None => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}
// ---------------------------------------------------------------------------
// Plan handlers
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
#[serde(untagged)]
pub enum PlansResponse {
    Single(Plan),
    List(Vec<PlanSummaryItem>),
}

#[instrument(skip(state))]
pub async fn create_plan(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NewPlanRequest>,
) -> Result<(StatusCode, Json<Plan>), AppError> {
    let plan = crate::plan::create_or_replace_plan(&state.pool, payload).await?;
    Ok((StatusCode::CREATED, Json(plan)))
}
#[instrument(skip(state))]
pub async fn get_plans(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<PlansResponse>, AppError> {
    let year_str = params
        .get("year")
        .ok_or_else(|| AppError::BadRequest("year is required".into()))?;
    let year: i32 = year_str
        .parse()
        .map_err(|_| AppError::BadRequest("year must be an integer".into()))?;

    if let Some(week_str) = params.get("week") {
        let week: i32 = week_str
            .parse()
            .map_err(|_| AppError::BadRequest("week must be an integer".into()))?;
        let plan = crate::plan::get_plan(&state.pool, year, week).await?;
        Ok(Json(PlansResponse::Single(plan)))
    } else {
        let plans = crate::plan::list_plans_for_year(&state.pool, year).await?;
        Ok(Json(PlansResponse::List(plans)))
    }
}

#[instrument(skip(state))]
pub async fn update_plan(
    State(state): State<Arc<AppState>>,
    Path((year, week)): Path<(i32, i32)>,
    Json(payload): Json<PlanPatch>,
) -> Result<Json<Plan>, AppError> {
    let plan = crate::plan::update_plan_meals(&state.pool, year, week, payload).await?;
    Ok(Json(plan))
}

#[instrument(skip(state))]
pub async fn delete_plan(
    State(state): State<Arc<AppState>>,
    Path((year, week)): Path<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    crate::plan::delete_plan(&state.pool, year, week).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Bring! shopping list handler
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BringItemRequest {
    pub name: String,
    pub spec: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BringStatusResponse {
    pub configured: bool,
    pub connected: bool,
    pub error: Option<String>,
}

impl From<bring::BringStatus> for BringStatusResponse {
    fn from(status: bring::BringStatus) -> Self {
        match status {
            bring::BringStatus::NotConfigured => BringStatusResponse {
                configured: false,
                connected: false,
                error: None,
            },
            bring::BringStatus::Connected { .. } => BringStatusResponse {
                configured: true,
                connected: true,
                error: None,
            },
            bring::BringStatus::Error(msg) => BringStatusResponse {
                configured: true,
                connected: false,
                error: Some(msg),
            },
        }
    }
}

#[instrument(skip(_state))]
pub async fn add_bring_item(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<BringItemRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    bring::push_item_to_bring(&req.name, req.spec.as_deref()).await?;
    Ok(Json(serde_json::json!({"sent": true})))
}

#[instrument(skip(_state))]
pub async fn get_bring_status(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<BringStatusResponse>, AppError> {
    Ok(Json(BringStatusResponse::from(
        bring::check_bring_status().await,
    )))
}

/// Returns the deployed version for the footer (`/api/version`).
/// Precedence: compile-time `YUMMYBOX_VERSION` (set by CI/Docker/build.rs from `git describe`) → `CARGO_PKG_VERSION` fallback.
/// Stateless — never acquires the DB lock.
#[instrument(skip(_state))]
pub async fn get_version(State(_state): State<Arc<AppState>>) -> Json<crate::model::AppVersion> {
    Json(crate::model::AppVersion {
        version: option_env!("YUMMYBOX_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    })
}
