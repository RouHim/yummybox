use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
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
use crate::recipe;
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
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Meal>), AppError> {
    let mut name: Option<String> = None;
    let mut ingredients_raw: Option<String> = None;
    let mut instructions: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart data: {e}")))?
    {
        match field.name() {
            Some("name") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("failed to read name field: {e}")))?;
                name = Some(text);
            }
            Some("ingredients") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read ingredients field: {e}"))
                })?;
                ingredients_raw = Some(text);
            }
            Some("instructions") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read instructions field: {e}"))
                })?;
                instructions = Some(text);
            }
            Some("image") => {
                if image_bytes.is_some() {
                    return Err(AppError::BadRequest(
                        "only one image may be uploaded".into(),
                    ));
                }
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read image field: {e}"))
                })?;
                image_bytes = Some(data.to_vec());
            }
            _ => {} // ignore unknown fields
        }
    }

    let name = name.ok_or_else(|| AppError::BadRequest("missing 'name' field".into()))?;
    let ingredients_raw = ingredients_raw
        .ok_or_else(|| AppError::BadRequest("missing 'ingredients' field".into()))?;
    let ingredients: Vec<crate::model::NewIngredientLine> = serde_json::from_str(&ingredients_raw)
        .map_err(|e| AppError::BadRequest(format!("invalid ingredients JSON: {e}")))?;
    let instructions =
        instructions.ok_or_else(|| AppError::BadRequest("missing 'instructions' field".into()))?;

    let jpeg_bytes;
    let image = match image_bytes {
        Some(bytes) => {
            jpeg_bytes = image::convert_to_jpeg(&bytes)?;
            db::ImageChange::Set(&jpeg_bytes)
        }
        None => db::ImageChange::Keep,
    };

    let new = NewMeal {
        name,
        ingredients,
        instructions,
    };
    let meal = db::insert_meal(&state.pool, new, image).await?;
    Ok((StatusCode::CREATED, Json(meal)))
}

#[instrument(skip(state))]
pub async fn update_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<Meal>, AppError> {
    let mut name: Option<String> = None;
    let mut ingredients_raw: Option<String> = None;
    let mut instructions: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_action: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart data: {e}")))?
    {
        match field.name() {
            Some("name") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("failed to read name field: {e}")))?;
                name = Some(text);
            }
            Some("ingredients") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read ingredients field: {e}"))
                })?;
                ingredients_raw = Some(text);
            }
            Some("instructions") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read instructions field: {e}"))
                })?;
                instructions = Some(text);
            }
            Some("image") => {
                if image_bytes.is_some() {
                    return Err(AppError::BadRequest(
                        "only one image may be uploaded".into(),
                    ));
                }
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read image field: {e}"))
                })?;
                image_bytes = Some(data.to_vec());
            }
            Some("image_action") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read image_action field: {e}"))
                })?;
                image_action = Some(text);
            }
            _ => {} // ignore unknown fields
        }
    }

    let name = name.ok_or_else(|| AppError::BadRequest("missing 'name' field".into()))?;
    let ingredients_raw = ingredients_raw
        .ok_or_else(|| AppError::BadRequest("missing 'ingredients' field".into()))?;
    let ingredients: Vec<crate::model::NewIngredientLine> = serde_json::from_str(&ingredients_raw)
        .map_err(|e| AppError::BadRequest(format!("invalid ingredients JSON: {e}")))?;
    let instructions =
        instructions.ok_or_else(|| AppError::BadRequest("missing 'instructions' field".into()))?;

    // Validate image_action if present
    if let Some(ref action) = image_action {
        if action != "remove" {
            return Err(AppError::BadRequest("image_action must be 'remove'".into()));
        }
    }

    let jpeg_bytes;
    let image = match (image_bytes, image_action.as_deref()) {
        (Some(bytes), _) => {
            jpeg_bytes = image::convert_to_jpeg(&bytes)?;
            db::ImageChange::Set(&jpeg_bytes)
        }
        (None, Some("remove")) => db::ImageChange::Clear,
        _ => db::ImageChange::Keep,
    };

    let patch = MealPatch {
        name,
        ingredients,
        instructions,
    };
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
// Recipe import handlers
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFromUrlRequest {
    pub url: String,
}

#[instrument(skip(_state))]
pub async fn import_from_url(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImportFromUrlRequest>,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let draft = recipe::fetch_and_parse(&req.url).await?;
    Ok(Json(draft))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFromPasteRequest {
    pub content: String,
}

#[instrument(skip(_state))]
pub async fn import_from_paste(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ImportFromPasteRequest>,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let draft = recipe::parse_recipe(&req.content)?;
    Ok(Json(draft))
}

#[instrument(skip(_state))]
pub async fn import_from_llm(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let mut model: Option<String> = None;
    let mut hint: Option<String> = None;
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut image_content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart data: {e}")))?
    {
        match field.name() {
            Some("model") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read model field: {e}"))
                })?;
                model = Some(text);
            }
            Some("hint") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("failed to read hint field: {e}")))?;
                hint = Some(text);
            }
            Some("image") => {
                if image_bytes.is_some() {
                    return Err(AppError::BadRequest(
                        "only one image may be uploaded".into(),
                    ));
                }
                image_content_type = field.content_type().map(String::from);
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read image field: {e}"))
                })?;
                image_bytes = Some(data.to_vec());
            }
            _ => {}
        }
    }

    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'model' field".into()))?;
    let hint = hint.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    let image_bytes = match image_bytes {
        Some(b) if !b.is_empty() => Some(b),
        _ => None,
    };
    if image_bytes.is_none() && hint.is_none() {
        return Err(AppError::BadRequest(
            "at least one of image or hint is required".into(),
        ));
    }

    if let Some(h) = &hint {
        if h.chars().count() > 5000 {
            return Err(AppError::BadRequest(
                "hint must be at most 5000 characters".into(),
            ));
        }
    }

    const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
    let image = match image_bytes {
        Some(bytes) => {
            if bytes.len() > MAX_IMAGE_BYTES {
                return Err(AppError::PayloadTooLarge(
                    "image exceeds 20 MB limit".into(),
                ));
            }
            Some(crate::llm_import::LlmImage {
                bytes,
                content_type: image_content_type.unwrap_or_else(|| "image/jpeg".to_string()),
            })
        }
        None => None,
    };

    let draft = crate::llm_import::import_via_llm(&model, hint.as_deref(), image).await?;
    Ok(Json(draft))
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
    let plan = db::create_or_replace_plan(&state.pool, payload).await?;
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
        let plan = db::get_plan(&state.pool, year, week).await?;
        Ok(Json(PlansResponse::Single(plan)))
    } else {
        let plans = db::list_plans_for_year(&state.pool, year).await?;
        Ok(Json(PlansResponse::List(plans)))
    }
}

#[instrument(skip(state))]
pub async fn update_plan(
    State(state): State<Arc<AppState>>,
    Path((year, week)): Path<(i32, i32)>,
    Json(payload): Json<PlanPatch>,
) -> Result<Json<Plan>, AppError> {
    let plan = db::update_plan_meals(&state.pool, year, week, payload).await?;
    Ok(Json(plan))
}

#[instrument(skip(state))]
pub async fn delete_plan(
    State(state): State<Arc<AppState>>,
    Path((year, week)): Path<(i32, i32)>,
) -> Result<StatusCode, AppError> {
    db::delete_plan(&state.pool, year, week).await?;
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

#[instrument(skip(_state))]
pub async fn add_bring_item(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<BringItemRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    bring::push_item_to_bring(&req.name, req.spec.as_deref()).await?;
    Ok(Json(serde_json::json!({"sent": true})))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ::image::ImageEncoder;
    use ::image::Rgba;
    use ::image::RgbaImage;
    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::{Method, Request, StatusCode, header};
    use axum::routing::{get, post, put};
    use serde_json::json;

    use tower::ServiceExt;

    use super::*;
    use crate::db::init_db;
    struct TestCtx {
        app: Router,
        _dir: tempfile::TempDir,
    }

    async fn setup() -> TestCtx {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db");
        let pool = init_db(&db_path).await.expect("init_db");
        let state = Arc::new(AppState { pool });
        let app = Router::new()
            .route("/meals", get(list_meals).post(create_meal))
            .route(
                "/meals/{id}",
                get(get_meal).put(update_meal).delete(delete_meal),
            )
            .route("/meals/{id}/image", get(get_meal_image))
            .route("/import/url", post(import_from_url))
            .route("/import/paste", post(import_from_paste))
            .route("/import/llm", post(import_from_llm))
            .route("/plans", get(get_plans).post(create_plan))
            .route("/plans/{year}/{week}", put(update_plan).delete(delete_plan))
            .route("/bring/items", post(add_bring_item))
            .layer(axum::extract::DefaultBodyLimit::max(50 * 1024 * 1024))
            .with_state(state);
        TestCtx { app, _dir: dir }
    }

    fn make_ingredient_lines(ings: &[(&str, Option<&str>)]) -> Vec<serde_json::Value> {
        ings.iter()
            .map(|(n, q)| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), json!(n));
                obj.insert("quantity".into(), json!(q));
                serde_json::Value::Object(obj)
            })
            .collect()
    }
    fn build_multipart_body(
        name: &str,
        ingredients_json: &str,
        instructions_text: &str,
        image_bytes: Option<&[u8]>,
    ) -> (Vec<u8>, String) {
        let boundary = "testboundary123";
        let mut body = Vec::new();

        // name field
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(b"\r\n");

        // ingredients field
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"ingredients\"\r\n\r\n");
        body.extend_from_slice(ingredients_json.as_bytes());
        body.extend_from_slice(b"\r\n");
        // instructions field
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"instructions\"\r\n\r\n");
        body.extend_from_slice(instructions_text.as_bytes());
        body.extend_from_slice(b"\r\n");

        // optional image field
        if let Some(img) = image_bytes {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(
                b"Content-Disposition: form-data; name=\"image\"; filename=\"photo.png\"\r\n",
            );
            body.extend_from_slice(b"Content-Type: image/png\r\n\r\n");
            body.extend_from_slice(img);
            body.extend_from_slice(b"\r\n");
        }

        // closing boundary
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");

        let content_type = format!("multipart/form-data; boundary={boundary}");
        (body, content_type)
    }

    async fn create_meal_helper(
        ctx: &TestCtx,
        name: &str,
        ingredients: &[(&str, Option<&str>)],
        instructions: &str,
    ) -> Meal {
        let ingredients_json = serde_json::to_string(&make_ingredient_lines(ingredients)).unwrap();
        let (body, content_type) =
            build_multipart_body(name, &ingredients_json, instructions, None);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    // -----------------------------------------------------------------------
    // Meal route tests (updated for structured ingredients)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_no_meals_when_get_meals_then_returns_200_and_empty_array() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/meals")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(meals.is_empty());
    }

    #[tokio::test]
    async fn given_valid_payload_when_post_meals_then_returns_201_and_persists() {
        let ctx = setup().await;
        let ings = make_ingredient_lines(&[("noodles", None), ("sauce", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("Pasta", &ingredients_json, "test instructions", None);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meal: Meal = serde_json::from_slice(&body).unwrap();
        assert_eq!(meal.name, "Pasta");
        assert_eq!(meal.ingredients.len(), 2);
        assert!(meal.id > 0);
    }

    #[tokio::test]
    async fn given_empty_name_when_post_meals_then_returns_400_with_error() {
        let ctx = setup().await;
        let ings = make_ingredient_lines(&[("x", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("", &ingredients_json, "test instructions", None);
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("name"));
    }
    #[tokio::test]
    async fn given_existing_meal_when_put_meal_then_returns_200_with_updated_payload() {
        let ctx = setup().await;
        let meal =
            create_meal_helper(&ctx, "Original", &[("stuff", None)], "test instructions").await;
        let ings = make_ingredient_lines(&[("new stuff", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("Updated", &ingredients_json, "test instructions", None);
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/meals/{}", meal.id))
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let updated: Meal = serde_json::from_slice(&body).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.ingredients.len(), 1);
        assert_eq!(updated.ingredients[0].name, "new stuff");
        assert_eq!(updated.id, meal.id);
    }

    #[tokio::test]
    async fn given_missing_meal_when_put_meal_then_returns_404() {
        let ctx = setup().await;
        let ings = make_ingredient_lines(&[("y", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("X", &ingredients_json, "test instructions", None);
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/meals/999")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_existing_meal_when_delete_meal_then_returns_204_and_removes_row() {
        let ctx = setup().await;
        let meal = create_meal_helper(&ctx, "ToDelete", &[("x", None)], "test instructions").await;
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/meals/{}", meal.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);

        let get_resp = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri(format!("/meals/{}", meal.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_missing_meal_when_delete_meal_then_returns_404() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/meals/999")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_search_term_when_get_meals_then_filters_by_name_and_ingredients() {
        let ctx = setup().await;
        let _ = create_meal_helper(&ctx, "Test", &[("stuff", None)], "test instructions").await;
        let _ = create_meal_helper(
            &ctx,
            "Other",
            &[("test ingredient", None)],
            "test instructions",
        )
        .await;

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/meals?search=test")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meals: Vec<Meal> = serde_json::from_slice(&body).unwrap();
        assert_eq!(meals.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Image route tests
    // -----------------------------------------------------------------------
    fn build_test_png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, Rgba([10, 20, 30, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        ::image::codecs::png::PngEncoder::new(&mut buf)
            .write_image(img.as_raw(), w, h, ::image::ExtendedColorType::Rgba8)
            .unwrap();
        buf.into_inner()
    }

    #[tokio::test]
    async fn given_valid_jpeg_when_post_meal_then_persists_and_has_image_true() {
        let ctx = setup().await;
        let png = build_test_png(10, 10);
        let ings = make_ingredient_lines(&[("salt", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) = build_multipart_body(
            "Photo Meal",
            &ingredients_json,
            "test instructions",
            Some(&png),
        );
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meal: Meal = serde_json::from_slice(&resp_body).unwrap();
        assert!(meal.has_image, "meal should have has_image: true");
    }

    #[tokio::test]
    async fn given_png_upload_when_post_meal_then_image_endpoint_returns_jpeg() {
        let ctx = setup().await;
        let png = build_test_png(10, 10);
        let ings = make_ingredient_lines(&[("x", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("Img", &ingredients_json, "test instructions", Some(&png));
        let resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let meal: Meal =
            serde_json::from_slice(&to_bytes(resp.into_body(), 4096).await.unwrap()).unwrap();

        let img_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/meals/{}/image", meal.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(img_resp.status(), StatusCode::OK);
        let ct = img_resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/jpeg");
        let img_body = to_bytes(img_resp.into_body(), 65536).await.unwrap();
        assert_eq!(&img_body[..2], &[0xFF, 0xD8], "should be JPEG");
    }

    #[tokio::test]
    async fn given_text_file_when_post_meal_then_returns_400() {
        let ctx = setup().await;
        let ings = make_ingredient_lines(&[("x", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) = build_multipart_body(
            "Bad",
            &ingredients_json,
            "test instructions",
            Some(b"not an image"),
        );
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_meal_without_image_when_get_image_then_returns_204() {
        let ctx = setup().await;
        let meal = create_meal_helper(&ctx, "NoImg", &[("a", None)], "test instructions").await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri(format!("/meals/{}/image", meal.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn given_missing_meal_when_get_image_then_returns_204() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/meals/999/image")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    // -----------------------------------------------------------------------
    // Plan route tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_no_meals_exist_when_post_plans_then_returns_400() {
        let ctx = setup().await;
        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 3
        }))
        .unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_meals_exist_when_post_plans_then_returns_201_with_plan_and_ingredient_summary() {
        let ctx = setup().await;
        create_meal_helper(&ctx, "A", &[("salt", Some("200g"))], "test instructions").await;
        create_meal_helper(&ctx, "B", &[("salt", Some("100g"))], "test instructions").await;
        create_meal_helper(&ctx, "C", &[("pepper", None)], "test instructions").await;

        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 2
        }))
        .unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let plan: Plan = serde_json::from_slice(&body).unwrap();
        assert_eq!(plan.meals.len(), 2);
        assert!(!plan.ingredient_summary.is_empty());
    }

    #[tokio::test]
    async fn given_plan_exists_when_get_plans_with_year_and_week_then_returns_plan_with_ingredient_summary()
     {
        let ctx = setup().await;
        create_meal_helper(&ctx, "A", &[("salt", Some("200g"))], "test instructions").await;
        create_meal_helper(&ctx, "B", &[("salt", Some("100g"))], "test instructions").await;

        // Create a plan
        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 2
        }))
        .unwrap();
        ctx.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans?year=2026&week=1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let plan: Plan = serde_json::from_slice(&body).unwrap();
        assert_eq!(plan.meals.len(), 2);
        assert!(!plan.ingredient_summary.is_empty());
    }

    #[tokio::test]
    async fn given_plan_missing_when_get_plans_with_year_and_week_then_returns_404() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans?year=2026&week=99")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_year_query_missing_when_get_plans_then_returns_400() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_year_query_invalid_when_get_plans_then_returns_400() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans?year=abc")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_year_query_only_no_week_when_get_plans_then_returns_summary_array_for_that_year()
    {
        let ctx = setup().await;
        create_meal_helper(&ctx, "A", &[("x", None)], "test instructions").await;

        // Create a plan
        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 1
        }))
        .unwrap();
        ctx.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans?year=2026")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let list: Vec<PlanSummaryItem> = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].week_number, 1);
    }

    #[tokio::test]
    async fn given_plan_exists_when_put_plans_with_meal_ids_then_returns_updated_plan_without_touching_last_planned_at()
     {
        let ctx = setup().await;
        let m1 = create_meal_helper(&ctx, "M1", &[("x", None)], "test instructions").await;
        let m2 = create_meal_helper(&ctx, "M2", &[("y", None)], "test instructions").await;

        // Create a plan with m1, m2 via POST
        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 2
        }))
        .unwrap();
        ctx.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Record last_planned_at values after generation
        let get_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/meals/{}", m1.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(get_resp.into_body(), 4096).await.unwrap();
        let m1_after: Meal = serde_json::from_slice(&body).unwrap();
        let lp1 = m1_after.last_planned_at;

        // Replace plan with just m2 via PUT
        let put_body = serde_json::to_vec(&json!({
            "meal_ids": [m2.id]
        }))
        .unwrap();
        let put_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/plans/2026/1")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(put_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put_resp.status(), StatusCode::OK);

        // Verify m1's last_planned_at unchanged
        let get_resp2 = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/meals/{}", m1.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = to_bytes(get_resp2.into_body(), 4096).await.unwrap();
        let m1_final: Meal = serde_json::from_slice(&body2).unwrap();
        assert_eq!(m1_final.last_planned_at, lp1);
    }

    #[tokio::test]
    async fn given_plan_missing_when_put_plans_then_returns_404() {
        let ctx = setup().await;
        let body = serde_json::to_vec(&json!({
            "meal_ids": [1]
        }))
        .unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/plans/2026/99")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_plan_exists_when_delete_plans_then_returns_204_and_subsequent_get_returns_404() {
        let ctx = setup().await;
        create_meal_helper(&ctx, "A", &[("x", None)], "test instructions").await;

        // Create a plan
        let body = serde_json::to_vec(&json!({
            "year": 2026,
            "week_number": 1,
            "meal_count": 1
        }))
        .unwrap();
        ctx.app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/plans")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Delete
        let del_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/plans/2026/1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

        // Verify gone
        let get_resp = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/plans?year=2026&week=1")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn given_plan_missing_when_delete_plans_then_returns_404() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/plans/2026/99")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // -----------------------------------------------------------------------
    // Recipe import tests
    // -----------------------------------------------------------------------

    const PASTE_HTML_WITH_RECIPE: &str = r#"<html><head>
<script type="application/ld+json">
{"@context":"https://schema.org","@type":"Recipe","name":"Test Recipe","description":"A test recipe","recipeIngredient":["2 cups flour","salt"],"recipeInstructions":[{"@type":"HowToStep","text":"Mix ingredients."},{"@type":"HowToStep","text":"Bake for 30 minutes."}]}
</script>
</head><body></body></html>"#;

    const PASTE_HTML_WITH_HTML_INSTRUCTIONS: &str = r#"<html><head>
<script type="application/ld+json">
{"@context":"https://schema.org","@type":"Recipe","name":"HTML Recipe","description":"A recipe with HTML instructions","recipeIngredient":["3 eggs","flour"],"recipeInstructions":[{"@type":"HowToStep","text":"<p dir=ltr>Step 1: crack eggs</p>"},{"@type":"HowToStep","text":"<p dir=ltr>Step 2: mix with flour</p>"}]}
</script>
</head><body></body></html>"#;

    #[tokio::test]
    async fn given_valid_paste_content_when_import_from_paste_then_returns_draft() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/paste")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&json!({"content": PASTE_HTML_WITH_RECIPE})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let draft: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(draft["name"], "Test Recipe");
        assert_eq!(draft["ingredients"].as_array().unwrap().len(), 2);
        assert!(draft["instructions"].as_str().unwrap().contains("Mix"));
        assert!(draft["imageBase64"].is_null());
    }

    #[tokio::test]
    async fn given_paste_with_html_instructions_when_import_from_paste_then_sanitized() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/paste")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&json!({"content": PASTE_HTML_WITH_HTML_INSTRUCTIONS}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let draft: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(draft["name"], "HTML Recipe");
        // dir attribute must be stripped; only whitelisted <p> tag survives
        assert_eq!(
            draft["instructions"].as_str().unwrap(),
            "<p>Step 1: crack eggs</p>\n<p>Step 2: mix with flour</p>"
        );
        assert!(draft["imageBase64"].is_null());
    }

    #[tokio::test]
    async fn given_paste_without_recipe_when_import_from_paste_then_returns_400() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/paste")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(
                            &json!({"content": "<html><body>no recipe</body></html>"}),
                        )
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let error: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(error["error"].as_str().unwrap().contains("Recipe"));
    }

    #[tokio::test]
    async fn given_missing_content_field_when_import_from_paste_then_returns_400() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/paste")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&json!({})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn given_import_draft_when_received_then_not_persisted() {
        let ctx = setup().await;
        // Call import/paste
        let _response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/paste")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&json!({"content": PASTE_HTML_WITH_RECIPE})).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        // Verify no meal was persisted (FR-006 / SC-006)
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .uri("/meals")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meals: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(meals.is_empty(), "import must not persist meals");
    }

    // ---------------------------------------------------------------
    // LLM import route tests
    // ---------------------------------------------------------------

    fn build_llm_multipart(
        model: Option<&str>,
        hint: Option<&str>,
        image_data: Option<&[u8]>,
    ) -> (Vec<u8>, String) {
        let boundary = "testboundaryLLM";
        let mut body = Vec::new();

        if let Some(m) = model {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
            body.extend_from_slice(m.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        if let Some(h) = hint {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(b"Content-Disposition: form-data; name=\"hint\"\r\n\r\n");
            body.extend_from_slice(h.as_bytes());
            body.extend_from_slice(b"\r\n");
        }
        if let Some(img) = image_data {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(
                b"Content-Disposition: form-data; name=\"image\"; filename=\"photo.jpg\"\r\n",
            );
            body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
            body.extend_from_slice(img);
            body.extend_from_slice(b"\r\n");
        }

        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");

        let content_type = format!("multipart/form-data; boundary={boundary}");
        (body, content_type)
    }

    #[tokio::test]
    async fn given_empty_body_when_import_llm_then_400() {
        let ctx = setup().await;
        let (body, content_type) = build_llm_multipart(None, None, None);
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/llm")
                    .header("content-type", content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
        let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains("missing 'model' field")
        );
    }

    #[tokio::test]
    async fn given_model_but_no_image_no_hint_when_import_llm_then_400() {
        let ctx = setup().await;
        let (body, content_type) = build_llm_multipart(Some("gpt-4o-mini"), None, None);
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/llm")
                    .header("content-type", content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_hint_over_5000_chars_when_import_llm_then_400() {
        let ctx = setup().await;
        let long_hint = "x".repeat(5001);
        let (body, content_type) = build_llm_multipart(Some("gpt-4o-mini"), Some(&long_hint), None);
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/llm")
                    .header("content-type", content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn given_image_over_20mb_when_import_llm_then_413() {
        let ctx = setup().await;
        let oversized = vec![0u8; 21_000_001];
        let (body, content_type) = build_llm_multipart(Some("gpt-4o-mini"), None, Some(&oversized));
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/import/llm")
                    .header("content-type", content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
        let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
        assert!(
            error["error"]
                .as_str()
                .unwrap()
                .contains("image exceeds 20 MB limit")
        );
    }

    // -----------------------------------------------------------------------
    // JSON-LD content-negotiation tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn given_meal_when_get_meal_with_jsonld_accept_then_returns_recipe_jsonld() {
        let ctx = setup().await;
        let meal = create_meal_helper(
            &ctx,
            "Pancakes",
            &[("flour", Some("2 cups")), ("egg", Some("1"))],
            "Mix and fry.",
        )
        .await;

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/meals/{}", meal.id))
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/ld+json");

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["@context"], "https://schema.org");
        assert_eq!(json["@type"], "Recipe");
        assert_eq!(json["name"], "Pancakes");
        let ings = json["recipeIngredient"].as_array().unwrap();
        assert_eq!(ings.len(), 2);
        assert!(
            ings.iter().any(|v| v == "2 cups flour"),
            "expected '2 cups flour' in ingredients"
        );
        assert!(
            ings.iter().any(|v| v == "1 egg"),
            "expected '1 egg' in ingredients"
        );
        assert_eq!(json["recipeInstructions"], "Mix and fry.");
        assert!(json["datePublished"].as_str().unwrap().contains("T"));
        assert!(json["dateModified"].as_str().unwrap().contains("T"));
        assert!(!json.as_object().unwrap().contains_key("image"));
    }

    #[tokio::test]
    async fn given_meal_with_image_when_get_meal_jsonld_then_image_is_absolute_url() {
        let ctx = setup().await;
        // Create a meal with an image
        let png = build_test_png(10, 10);
        let ings = make_ingredient_lines(&[("salt", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) = build_multipart_body(
            "Photo Meal",
            &ingredients_json,
            "test instructions",
            Some(&png),
        );
        let create_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let meal: Meal =
            serde_json::from_slice(&to_bytes(create_resp.into_body(), 4096).await.unwrap())
                .unwrap();
        assert!(meal.has_image);

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/meals/{}", meal.id))
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json["image"],
            format!("http://127.0.0.1:11341/api/meals/{}/image", meal.id)
        );
    }

    #[tokio::test]
    async fn given_meal_without_image_when_get_meal_jsonld_then_no_image_field() {
        let ctx = setup().await;
        let meal = create_meal_helper(&ctx, "Plain", &[("x", None)], "test").await;
        assert!(!meal.has_image);

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/meals/{}", meal.id))
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("image"),
            "image field should be absent for meals without an image"
        );
    }

    #[tokio::test]
    async fn given_missing_meal_when_get_meal_jsonld_then_404_json_error() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/meals/99999")
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "404 errors should return application/json, got {ct}"
        );
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "not found");
    }

    #[tokio::test]
    async fn given_meals_when_get_meals_jsonld_then_returns_graph_array() {
        let ctx = setup().await;
        create_meal_helper(&ctx, "A", &[("a", None)], "test").await;
        create_meal_helper(&ctx, "B", &[("b", None)], "test").await;
        create_meal_helper(&ctx, "C", &[("c", None)], "test").await;

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/meals")
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["@context"], "https://schema.org");
        let graph = json["@graph"].as_array().unwrap();
        assert_eq!(graph.len(), 3);
        for node in graph {
            assert_eq!(node["@type"], "Recipe");
            assert!(node.as_object().unwrap().contains_key("@context"));
        }
    }

    #[tokio::test]
    async fn given_no_meals_when_get_meals_jsonld_then_empty_graph() {
        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/meals")
                    .header(header::ACCEPT, "application/ld+json")
                    .header(header::HOST, "127.0.0.1:11341")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["@context"], "https://schema.org");
        let graph = json["@graph"].as_array().unwrap();
        assert!(graph.is_empty());
    }

    #[tokio::test]
    async fn given_meal_when_get_meal_without_jsonld_accept_then_plain_json_unchanged() {
        let ctx = setup().await;
        let meal =
            create_meal_helper(&ctx, "Pasta", &[("noodles", Some("200 g"))], "Boil water.").await;

        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/meals/{}", meal.id))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"));

        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // Default JSON shape: ingredients is [{name, quantity}], has "id"
        assert!(json["id"].is_number());
        assert_eq!(json["name"], "Pasta");
        let ings = json["ingredients"].as_array().unwrap();
        assert_eq!(ings.len(), 1);
        assert_eq!(ings[0]["name"], "noodles");
        assert_eq!(ings[0]["quantity"], "200 g");
    }

    #[tokio::test]
    async fn given_missing_host_when_get_meal_jsonld_then_image_omitted() {
        let ctx = setup().await;
        // Create a meal with an image
        let png = build_test_png(10, 10);
        let ings = make_ingredient_lines(&[("salt", None)]);
        let ingredients_json = serde_json::to_string(&ings).unwrap();
        let (body, content_type) =
            build_multipart_body("Hostless", &ingredients_json, "test", Some(&png));
        let create_resp = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", &content_type)
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let meal: Meal =
            serde_json::from_slice(&to_bytes(create_resp.into_body(), 4096).await.unwrap())
                .unwrap();
        assert!(meal.has_image);

        // Request with Accept: application/ld+json but NO Host header
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/meals/{}", meal.id))
                    .header(header::ACCEPT, "application/ld+json")
                    // deliberately omit Host header
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("image"),
            "image should be omitted when Host header is missing"
        );
    }

    // Bring! integration tests

    #[tokio::test]
    async fn given_missing_bring_credentials_when_send_then_returns_400() {
        // Ensure env vars are unset during test
        unsafe { std::env::remove_var("BRING_EMAIL") };
        unsafe { std::env::remove_var("BRING_PASSWORD") };

        let ctx = setup().await;
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/bring/items")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_string(&json!({"name": "Tomatoes", "spec": "400 g"}))
                            .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            json["error"]
                .as_str()
                .unwrap()
                .contains("BRING_EMAIL and BRING_PASSWORD"),
            "expected credential error, got: {json}"
        );
    }
}
