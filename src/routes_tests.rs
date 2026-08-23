// Integration tests for route handlers. Kept in a separate flat module
// so src/routes.rs stays focused on production code.

use std::sync::Arc;

use ::image::ImageEncoder;
use ::image::Rgba;
use ::image::RgbaImage;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode, header};
use axum::routing::{get, post, put};
use serde_json::json;

use tower::ServiceExt;

use crate::db::init_db;
use crate::error::AppError;
use crate::model::{Meal, Plan, PlanSummaryItem};
use crate::routes::{
    add_bring_item, create_meal, create_plan, delete_meal, delete_plan, get_bring_status, get_meal,
    get_meal_image, get_plans, get_version, list_meals, update_meal, update_plan,
};
use crate::state::AppState;

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
        .route("/import/url", post(crate::import::import_from_url))
        .route("/import/paste", post(crate::import::import_from_paste))
        .route("/import/llm", post(crate::import::import_from_llm))
        .route("/import/generate", post(crate::import::generate_meal))
        .route("/import/bulk", post(crate::import::import_bulk))
        .route("/llm/providers", get(crate::import::llm_providers))
        .route("/llm/models", get(crate::import::llm_models))
        .route("/llm/polish", post(crate::import::polish_instructions))
        .route("/plans", get(get_plans).post(create_plan))
        .route("/plans/{year}/{week}", put(update_plan).delete(delete_plan))
        .route("/bring/items", post(add_bring_item))
        .route("/bring/status", get(get_bring_status))
        .route("/version", get(get_version))
        .layer(axum::extract::DefaultBodyLimit::max(crate::MAX_BODY_BYTES))
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
    let (body, content_type) = build_multipart_body(name, &ingredients_json, instructions, None);
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
async fn given_existing_meal_when_post_meal_duplicate_name_then_returns_409() {
    let ctx = setup().await;
    create_meal_helper(&ctx, "Pancakes", &[("flour", None)], "test instructions").await;

    // Same name, different case
    let ings = make_ingredient_lines(&[("flour", None)]);
    let ingredients_json = serde_json::to_string(&ings).unwrap();
    let (body, content_type) =
        build_multipart_body("pancakes", &ingredients_json, "test instructions", None);
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
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let body_bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(
        body["error"].as_str().unwrap().contains("already exists"),
        "expected duplicate error, got: {body}"
    );
}

#[tokio::test]
async fn given_body_over_50mb_when_post_meals_then_413() {
    let ctx = setup().await;
    let boundary = "testboundary123";
    let mut body = Vec::new();
    // name field
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
    body.extend_from_slice(b"Test Meal\r\n");
    // oversized instructions field — 53 MB of a single field trips the
    // 50 MiB body limit mid-field-read, which must surface as 413.
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"instructions\"\r\n\r\n");
    body.extend_from_slice(&vec![b'x'; 53_000_000]);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    let content_type = format!("multipart/form-data; boundary={boundary}");
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
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("request body exceeds 50 MB limit")
    );
}

#[tokio::test]
async fn given_existing_meal_when_put_meal_then_returns_200_with_updated_payload() {
    let ctx = setup().await;
    let meal = create_meal_helper(&ctx, "Original", &[("stuff", None)], "test instructions").await;
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
async fn given_two_meals_when_put_meal_rename_to_other_name_then_returns_409() {
    let ctx = setup().await;
    let tacos = create_meal_helper(&ctx, "Tacos", &[("tortilla", None)], "test instructions").await;
    create_meal_helper(&ctx, "Burritos", &[("tortilla", None)], "test instructions").await;

    let ings = make_ingredient_lines(&[("tortilla", None)]);
    let ingredients_json = serde_json::to_string(&ings).unwrap();
    let (body, content_type) =
        build_multipart_body("Burritos", &ingredients_json, "test instructions", None);
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/meals/{}", tacos.id))
                .header("content-type", &content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn given_meal_when_put_meal_rename_to_own_name_case_variant_then_returns_200() {
    let ctx = setup().await;
    let meal = create_meal_helper(&ctx, "Tacos", &[("tortilla", None)], "test instructions").await;

    let ings = make_ingredient_lines(&[("tortilla", None)]);
    let ingredients_json = serde_json::to_string(&ings).unwrap();
    let (body, content_type) =
        build_multipart_body("tacos", &ingredients_json, "test instructions", None);
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
async fn given_year_query_only_no_week_when_get_plans_then_returns_summary_array_for_that_year() {
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
    let put_body = to_bytes(put_resp.into_body(), 4096).await.unwrap();
    let updated: Plan = serde_json::from_slice(&put_body).unwrap();
    assert_eq!(updated.meals.len(), 1);
    assert_eq!(updated.meals[0].id, m2.id);

    // Verify the replacement persisted: the plan now holds exactly m2.
    let get_plan_resp = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/plans?year=2026&week=1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_plan_resp.status(), StatusCode::OK);
    let get_plan_body = to_bytes(get_plan_resp.into_body(), 4096).await.unwrap();
    let fetched: Plan = serde_json::from_slice(&get_plan_body).unwrap();
    let fetched_ids: Vec<i64> = fetched.meals.iter().map(|m| m.id).collect();
    assert_eq!(fetched_ids, vec![m2.id]);

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
                    serde_json::to_vec(&json!({"content": "<html><body>no recipe</body></html>"}))
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
async fn given_missing_content_field_when_import_from_paste_then_returns_422() {
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
    images: &[&[u8]],
    base_url: Option<&str>,
    api_key: Option<&str>,
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
    for img in images {
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
    if let Some(b) = base_url {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"base_url\"\r\n\r\n");
        body.extend_from_slice(b.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(k) = api_key {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"api_key\"\r\n\r\n");
        body.extend_from_slice(k.as_bytes());
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
    let (body, content_type) = build_llm_multipart(None, None, &[], None, None);
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
    let (body, content_type) = build_llm_multipart(Some("gpt-4o-mini"), None, &[], None, None);
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
async fn given_hint_over_20000_chars_when_import_llm_then_400() {
    let ctx = setup().await;
    let long_hint = "x".repeat(20001);
    let (body, content_type) =
        build_llm_multipart(Some("gpt-4o-mini"), Some(&long_hint), &[], None, None);
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
    let (body, content_type) =
        build_llm_multipart(Some("gpt-4o-mini"), None, &[&oversized], None, None);
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

#[tokio::test]
async fn given_six_images_when_import_llm_then_400_with_limit_message() {
    let ctx = setup().await;
    let (body, content_type) = build_llm_multipart(
        Some("gpt-4o-mini"),
        None,
        &[&[1u8], &[1u8], &[1u8], &[1u8], &[1u8], &[1u8]],
        None,
        None,
    );
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
            .contains("maximum 5 images allowed")
    );
}

#[tokio::test]
async fn given_empty_image_field_when_import_llm_then_skipped() {
    let ctx = setup().await;
    // An empty image field is skipped (0-byte files are tolerated, as in the
    // pre-multipart code), so it must NOT satisfy the image requirement; with
    // no image and no hint the all-empty guard still rejects.
    let (body, content_type) = build_llm_multipart(Some("gpt-4o-mini"), None, &[&[]], None, None);
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
            .contains("at least one of image or hint is required")
    );
}

#[tokio::test]
async fn given_empty_image_field_with_hint_when_import_llm_then_draft_returned() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ctx = setup().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let mock_body = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"test-model","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"extract_recipe","arguments":"{\"name\":\"Front Back Curry\",\"ingredients\":[{\"name\":\"chicken\",\"quantity\":\"200 g\"}],\"instructions\":\"Cook both sides.\",\"portion\":2}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Drain request headers then the body (per Content-Length) so the
        // mock HTTP exchange completes.
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.unwrap();
            buf.push(byte[0]);
        }
        let headers = String::from_utf8_lossy(&buf);
        let content_length = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            stream.read_exact(&mut body).await.unwrap();
        }
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            mock_body.len(),
            mock_body
        );
        let _ = stream.write_all(resp.as_bytes()).await;
    });

    let base_url = format!("http://127.0.0.1:{port}/v1/");
    // A 0-byte file picked alongside a valid hint must still produce a draft
    // (the empty field is skipped rather than rejected).
    let (body, content_type) = build_llm_multipart(
        Some("test-model"),
        Some("flour"),
        &[&[]],
        Some(&base_url),
        Some("test-key"),
    );
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
    assert_eq!(response.status(), StatusCode::OK);
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    let draft: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(draft["name"], "Front Back Curry");
}

#[tokio::test]
async fn given_three_oversized_images_when_import_llm_then_413() {
    let ctx = setup().await;
    let oversized = vec![0u8; 21_000_001];
    let (body, content_type) = build_llm_multipart(
        Some("gpt-4o-mini"),
        None,
        &[&oversized, &oversized, &oversized],
        None,
        None,
    );
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
    // The 63 MB body exceeds the 50 MiB body limit, so the limit fires
    // during the field read, not the per-image check.
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("request body exceeds 50 MB limit")
    );
}

#[tokio::test]
async fn given_two_images_each_over_20mb_when_import_llm_then_413_with_per_image_message() {
    let ctx = setup().await;
    let oversized = vec![0u8; 21_000_001];
    let (body, content_type) = build_llm_multipart(
        Some("gpt-4o-mini"),
        None,
        &[&oversized, &oversized],
        None,
        None,
    );
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
    // 42 MB body is under the 50 MiB body limit; the per-image 20 MB
    // check must be what fires.
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("image exceeds 20 MB limit")
    );
}

#[tokio::test]
async fn given_body_over_50mb_with_images_under_20mb_each_when_import_llm_then_413_with_body_message()
 {
    let ctx = setup().await;
    let image = vec![0u8; 19_000_000];
    let (body, content_type) = build_llm_multipart(
        Some("gpt-4o-mini"),
        None,
        &[&image, &image, &image],
        None,
        None,
    );
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
    // Each image is under the per-image cap; the 57 MB body trips the
    // 50 MiB body limit mid-field-read and must surface as 413.
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    let error: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert!(
        error["error"]
            .as_str()
            .unwrap()
            .contains("request body exceeds 50 MB limit")
    );
}

#[tokio::test]
async fn given_two_images_when_import_llm_then_sent_in_order_and_draft_returned() {
    use base64::Engine;
    use std::sync::Mutex;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ctx = setup().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let captured: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_server = captured.clone();
    let mock_body = r#"{"id":"chatcmpl-test","object":"chat.completion","created":0,"model":"test-model","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"extract_recipe","arguments":"{\"name\":\"Front Back Curry\",\"ingredients\":[{\"name\":\"chicken\",\"quantity\":\"200 g\"}],\"instructions\":\"Cook both sides.\",\"portion\":2}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#;
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Read headers until the blank line, then the body per Content-Length.
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.unwrap();
            buf.push(byte[0]);
        }
        let headers = String::from_utf8_lossy(&buf);
        let content_length = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            stream.read_exact(&mut body).await.unwrap();
        }
        // Collect the user message's image parts in array order.
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let messages = json["messages"].as_array().unwrap();
        let user = messages
            .iter()
            .find(|m| m["role"] == "user")
            .expect("user message present");
        let mut decoded = Vec::new();
        for part in user["content"].as_array().unwrap() {
            if part["type"] == "image_url" {
                let url = part["image_url"]["url"].as_str().unwrap();
                let b64 = url.split("base64,").nth(1).unwrap();
                decoded.push(
                    base64::engine::general_purpose::STANDARD
                        .decode(b64)
                        .unwrap(),
                );
            }
        }
        *captured_server.lock().unwrap() = decoded;
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            mock_body.len(),
            mock_body
        );
        let _ = stream.write_all(resp.as_bytes()).await;
    });

    let base_url = format!("http://127.0.0.1:{port}/v1/");
    let (body, content_type) = build_llm_multipart(
        Some("test-model"),
        None,
        &[&[0x01, 0x02], &[0x03, 0x04]],
        Some(&base_url),
        Some("test-key"),
    );
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
    let status = response.status();
    let resp_body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(status, StatusCode::OK);
    let draft: serde_json::Value = serde_json::from_slice(&resp_body).unwrap();
    assert_eq!(draft["name"], "Front Back Curry");
    assert_eq!(draft["ingredients"][0]["name"], "chicken");
    assert_eq!(draft["portions"], 2);
    assert_eq!(
        *captured.lock().unwrap(),
        vec![vec![0x01, 0x02], vec![0x03, 0x04]],
        "images must reach the LLM in upload order"
    );
}

#[tokio::test]
async fn given_no_fields_when_generate_meal_then_400_missing_model() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart(None, None, &[]);
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("model"));
}
fn build_generate_multipart(
    model: Option<&str>,
    ingredients: Option<&str>,
    images: &[&[u8]],
) -> (Vec<u8>, String) {
    let boundary = "testboundaryGEN";
    let mut body = Vec::new();

    if let Some(m) = model {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(m.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(ing) = ingredients {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"ingredients\"\r\n\r\n");
        body.extend_from_slice(ing.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    for img in images {
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

/// Like build_generate_multipart, but with a single image part carrying a
/// caller-supplied Content-Type (build_generate_multipart always sends
/// image/jpeg).
fn build_generate_multipart_with_image_content_type(
    model: Option<&str>,
    ingredients: Option<&str>,
    image_bytes: &[u8],
    image_content_type: &str,
) -> (Vec<u8>, String) {
    let boundary = "testboundaryGEN";
    let mut body = Vec::new();

    if let Some(m) = model {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(m.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(ing) = ingredients {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"ingredients\"\r\n\r\n");
        body.extend_from_slice(ing.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"image\"; filename=\"photo.svg\"\r\n",
    );
    body.extend_from_slice(format!("Content-Type: {image_content_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(image_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let content_type = format!("multipart/form-data; boundary={boundary}");
    (body, content_type)
}

#[tokio::test]
async fn given_model_without_input_when_generate_meal_then_400() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart(Some("mock-model"), None, &[]);
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("at least one of ingredients or an image")
    );
}

#[tokio::test]
async fn given_six_images_when_generate_meal_then_400() {
    let ctx = setup().await;
    let imgs: Vec<Vec<u8>> = (0..6).map(|i| vec![i as u8; 16]).collect();
    let refs: Vec<&[u8]> = imgs.iter().map(|v| v.as_slice()).collect();
    let (body, content_type) = build_generate_multipart(Some("mock-model"), Some("flour"), &refs);
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("5 images"));
}

#[tokio::test]
async fn given_oversized_image_when_generate_meal_then_413() {
    let ctx = setup().await;
    let oversized = vec![0u8; 21_000_001];
    let (body, content_type) =
        build_generate_multipart(Some("mock-model"), Some("flour"), &[&oversized]);
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn given_long_ingredients_when_generate_meal_then_400() {
    let ctx = setup().await;
    let long = "x".repeat(20001);
    let (body, content_type) = build_generate_multipart(Some("mock-model"), Some(&long), &[]);
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json["error"].as_str().unwrap().contains("20000"));
}

#[tokio::test]
async fn given_empty_image_when_generate_meal_then_400_image_field_empty() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart_with_image_content_type(
        Some("mock-model"),
        Some("flour"),
        b"",
        "image/jpeg",
    );
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("image field is empty")
    );
}

/// Like build_generate_multipart_with_image_content_type, but the image part
/// carries NO Content-Type header at all (simulating a malformed client that
/// omits it), so the None branch of the generate_meal content-type match runs.
fn build_generate_multipart_without_image_content_type(
    model: Option<&str>,
    ingredients: Option<&str>,
    image_bytes: &[u8],
) -> (Vec<u8>, String) {
    let boundary = "testboundaryGEN";
    let mut body = Vec::new();

    if let Some(m) = model {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(m.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(ing) = ingredients {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"ingredients\"\r\n\r\n");
        body.extend_from_slice(ing.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"image\"; filename=\"photo.jpg\"\r\n\r\n",
    );
    body.extend_from_slice(image_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let content_type = format!("multipart/form-data; boundary={boundary}");
    (body, content_type)
}

#[tokio::test]
async fn given_svg_image_when_generate_meal_then_400_unsupported_content_type() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart_with_image_content_type(
        Some("mock-model"),
        Some("flour"),
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        "image/svg+xml",
    );
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("unsupported image content type: image/svg+xml")
    );
}

#[tokio::test]
async fn given_image_without_content_type_when_generate_meal_then_400_missing_header() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart_without_image_content_type(
        Some("mock-model"),
        Some("flour"),
        b"x",
    );
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("unsupported image content type: missing Content-Type header")
    );
}
// ---------------------------------------------------------------
// LLM providers & models route tests
// ---------------------------------------------------------------

#[tokio::test]
async fn given_no_api_keys_when_list_providers_then_ollama_configured() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/providers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let providers = json["providers"].as_array().expect("providers array");
    let ollama = providers
        .iter()
        .find(|p| p["id"].as_str() == Some("ollama"))
        .expect("ollama provider");
    assert_eq!(ollama["configured"], serde_json::Value::Bool(true));
}

#[tokio::test]
async fn list_providers_includes_custom() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/providers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let providers = json["providers"].as_array().expect("providers array");
    let custom = providers
        .iter()
        .find(|p| p["id"].as_str() == Some("custom"))
        .expect("custom provider");
    assert_eq!(
        custom["supportsCustomEndpoint"],
        serde_json::Value::Bool(true)
    );
}

#[tokio::test]
async fn given_unknown_provider_when_list_models_then_400() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/models?provider=nonexistent")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn given_custom_provider_no_base_url_when_list_models_then_400() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/models?provider=custom")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
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
        serde_json::from_slice(&to_bytes(create_resp.into_body(), 4096).await.unwrap()).unwrap();
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
        serde_json::from_slice(&to_bytes(create_resp.into_body(), 4096).await.unwrap()).unwrap();
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

// Serializes tests that mutate the process-global BRING_EMAIL/BRING_PASSWORD
// env vars; a concurrent restore in one test could otherwise land between
// another test's remove_var and its assertion. tokio Mutex: the guard is
// held across awaits.
static BRING_ENV_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tokio::test]
async fn given_missing_bring_credentials_when_send_then_returns_400() {
    let _guard = BRING_ENV_LOCK.lock().await;
    // Ensure env vars are unset during test
    let had_email = std::env::var("BRING_EMAIL").ok();
    let had_password = std::env::var("BRING_PASSWORD").ok();
    unsafe {
        std::env::remove_var("BRING_EMAIL");
        std::env::remove_var("BRING_PASSWORD");
    }

    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/bring/items")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_string(&json!({"name": "Tomatoes", "spec": "400 g"})).unwrap(),
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

    // Restore env vars
    if let Some(v) = had_email {
        unsafe {
            std::env::set_var("BRING_EMAIL", v);
        }
    }
    if let Some(v) = had_password {
        unsafe {
            std::env::set_var("BRING_PASSWORD", v);
        }
    }
}

#[tokio::test]
async fn given_missing_bring_credentials_when_status_then_returns_not_configured() {
    let _guard = BRING_ENV_LOCK.lock().await;
    // Ensure env vars are unset
    let had_email = std::env::var("BRING_EMAIL").ok();
    let had_password = std::env::var("BRING_PASSWORD").ok();
    unsafe {
        std::env::remove_var("BRING_EMAIL");
        std::env::remove_var("BRING_PASSWORD");
    }

    let TestCtx { app, _dir } = setup().await;
    let request = Request::builder()
        .uri("/bring/status")
        .method(Method::GET)
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json,
        json!({"configured": false, "connected": false, "error": null})
    );

    // Restore env vars
    if let Some(v) = had_email {
        unsafe {
            std::env::set_var("BRING_EMAIL", v);
        }
    }
    if let Some(v) = had_password {
        unsafe {
            std::env::set_var("BRING_PASSWORD", v);
        }
    }
}

// -----------------------------------------------------------------------
// Bulk import tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_51_urls_when_bulk_import_then_400() {
    let ctx = setup().await;
    let urls: Vec<String> = (0..51)
        .map(|i| format!("https://example.com/{i}"))
        .collect();
    let body = axum::body::Body::from(serde_json::to_vec(&json!({ "urls": urls })).unwrap());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/import/bulk")
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap();
    let resp = ctx.app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["error"].as_str().unwrap().contains("50"));
}

#[tokio::test]
async fn given_empty_urls_when_bulk_import_then_empty_result() {
    let ctx = setup().await;
    let body =
        axum::body::Body::from(serde_json::to_vec(&json!({ "urls": ["", "  ", "\n"] })).unwrap());
    let req = Request::builder()
        .method(Method::POST)
        .uri("/import/bulk")
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap();
    let resp = ctx.app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["created"].as_array().unwrap().is_empty());
    assert!(body["failed"].as_array().unwrap().is_empty());
}

#[test]
fn given_not_found_error_when_classify_fetch_then_no_recipe_found() {
    let err = AppError::NotFound;
    assert_eq!(crate::import::classify_fetch_error(&err), "no recipe found");
}

// -----------------------------------------------------------------------
// Polish instructions tests
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_missing_model_when_polish_instructions_then_returns_400() {
    let ctx = setup().await;
    let boundary = "testpolishboundary";
    let mut body = Vec::new();
    // name field
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
    body.extend_from_slice(b"Test Meal\r\n");
    // closing boundary — no model field
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/llm/polish")
                .header("content-type", &content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn given_body_over_50mb_when_polish_instructions_then_413() {
    let ctx = setup().await;
    let boundary = "testpolishboundary";
    let mut body = Vec::new();
    // name field
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"name\"\r\n\r\n");
    body.extend_from_slice(b"Test Meal\r\n");
    // oversized instructions field — 53 MB of a single field trips the
    // 50 MiB body limit mid-field-read, which must surface as 413.
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"instructions\"\r\n\r\n");
    body.extend_from_slice(&vec![b'x'; 53_000_000]);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    let content_type = format!("multipart/form-data; boundary={boundary}");
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/llm/polish")
                .header("content-type", &content_type)
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
            .contains("request body exceeds 50 MB limit")
    );
}
#[test]
fn given_other_error_when_classify_fetch_then_includes_detail() {
    let err = AppError::Internal("timeout".into());
    assert_eq!(
        crate::import::classify_fetch_error(&err),
        "fetch failed: internal error: timeout"
    );
}

#[test]
fn given_validation_error_when_classify_insert_then_validation_failed() {
    let err = AppError::Validation("too long".into());
    assert_eq!(
        crate::import::classify_insert_error(&err),
        "validation failed"
    );
}

#[test]
fn given_db_error_when_classify_insert_then_returns_message() {
    let err = AppError::BadRequest("something wrong".into());
    assert_eq!(
        crate::import::classify_insert_error(&err),
        "something wrong"
    );
}

#[tokio::test]
async fn given_running_app_when_get_version_then_returns_cargo_pkg_version() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/version")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}
