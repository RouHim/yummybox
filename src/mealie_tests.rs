// Mealie import tests: pure parsers (unit) + route integration.

use serde_json::json;

use crate::mealie::*;

// --- parse_ingredient_lines ---

#[test]
fn given_mealie_notes_when_parse_ingredients_then_returns_notes() {
    let recipe = json!({
        "recipe_ingredient": [
            {"note": "250 g Reis", "original_text": null, "food": null},
            {"note": "Salz & Pfeffer", "original_text": null, "food": null}
        ]
    });
    assert_eq!(
        parse_ingredient_lines(&recipe).unwrap(),
        vec!["250 g Reis", "Salz & Pfeffer"]
    );
}

#[test]
fn given_blank_notes_when_parse_ingredients_then_skips_blanks() {
    let recipe = json!({
        "recipe_ingredient": [
            {"note": "  ", "original_text": "1 egg"},
            {"note": "", "original_text": null}
        ]
    });
    assert_eq!(parse_ingredient_lines(&recipe).unwrap(), vec!["1 egg"]);
}

#[test]
fn given_empty_ingredients_when_parse_then_returns_none() {
    let recipe = json!({"recipe_ingredient": []});
    assert!(parse_ingredient_lines(&recipe).is_none());
    let missing = json!({});
    assert!(parse_ingredient_lines(&missing).is_none());
}

// --- parse_instructions ---

#[test]
fn given_plain_steps_when_parse_instructions_then_joins_with_blank_line() {
    let recipe = json!({
        "recipe_instructions": [
            {"text": "Chop onions."},
            {"text": "Fry until golden."}
        ]
    });
    assert_eq!(
        parse_instructions(&recipe),
        "Chop onions.\n\nFry until golden."
    );
}

#[test]
fn given_python_dict_string_when_parse_then_extracts_inner_text() {
    let text = "{'@type': 'HowToSection', 'name': 'Gratin', 'itemListElement': \
        {'@type': 'HowToStep', 'name': 'Step', 'text': 'Grate cheese.\\n\\nBake 20 min.'}}";
    let recipe = json!({"recipe_instructions": [{"text": text}]});
    assert_eq!(parse_instructions(&recipe), "Grate cheese.\n\nBake 20 min.");
}

#[test]
fn given_python_dict_with_escapes_when_parse_then_unescapes() {
    let text = "{'text': 'Line\\xa0with\\'quote\\' and\\nnewline'}";
    let recipe = json!({"recipe_instructions": [{"text": text}]});
    let out = parse_instructions(&recipe);
    assert!(out.contains("Line"), "got: {out}");
    assert!(out.contains("with'quote'"), "got: {out}");
    assert!(out.contains('\n'), "got: {out:?}");
}

#[test]
fn given_python_dict_with_umlauts_when_parse_then_preserves_utf8() {
    let text = "{'text': 'Grüße mit Käse und Bärlauch'}";
    let recipe = json!({"recipe_instructions": [{"text": text}]});
    let out = parse_instructions(&recipe);
    assert!(out.contains("Grüße mit Käse und Bärlauch"), "got: {out}");
}

#[test]
fn given_python_dict_with_empty_text_when_parse_then_returns_empty() {
    let recipe = json!({"recipe_instructions": [{"text": "{'text': ''}"}]});
    assert_eq!(parse_instructions(&recipe), "");
}

#[test]
fn given_empty_instructions_when_parse_then_returns_empty() {
    let recipe = json!({"recipe_instructions": []});
    assert_eq!(parse_instructions(&recipe), "");
}

// --- parse_portions ---

#[test]
fn given_servings_string_when_parse_portions_then_extracts_number() {
    assert_eq!(
        parse_portions(&json!({"recipe_yield": "2 serving(s)"})),
        Some(2)
    );
    assert_eq!(
        parse_portions(&json!({"recipe_yield": "4 serving(s)"})),
        Some(4)
    );
}

#[test]
fn given_empty_yield_when_parse_portions_then_returns_none() {
    assert_eq!(parse_portions(&json!({"recipe_yield": ""})), None);
    assert_eq!(parse_portions(&json!({})), None);
}

// --- parse_source_url ---

#[test]
fn given_org_url_when_parse_source_url_then_returns_trimmed() {
    let recipe = json!({"org_url": "https://example.com/recipe/1"});
    assert_eq!(
        parse_source_url(&recipe).as_deref(),
        Some("https://example.com/recipe/1")
    );
}

#[test]
fn given_invalid_org_url_when_parse_source_url_then_returns_none() {
    let recipe = json!({"org_url": "not a url"});
    assert!(parse_source_url(&recipe).is_none());
    assert!(parse_source_url(&json!({})).is_none());
}

// --- image candidates ---

#[test]
fn given_slug_when_image_candidates_then_prefers_original() {
    let candidates = mealie_image_candidates("my-slug");
    assert_eq!(candidates[0], "recipes/my-slug/images/original.webp");
    assert_eq!(candidates[1], "recipes/my-slug/images/min-original.webp");
    assert!(candidates.contains(&"recipes/my-slug/images/original.jpg".to_string()));
    assert!(candidates.contains(&"recipes/my-slug/images/min-original.jpg".to_string()));
    assert!(candidates.contains(&"recipes/my-slug/images/original.jpeg".to_string()));
    assert!(candidates.contains(&"recipes/my-slug/images/min-original.jpeg".to_string()));
    assert!(candidates.contains(&"recipes/my-slug/images/original.png".to_string()));
    assert!(candidates.contains(&"recipes/my-slug/images/min-original.png".to_string()));
}

// ------------------------------------------------------------------
// Route-level integration tests (async)
// ------------------------------------------------------------------

use std::io::{Cursor, Write};

use ::image::ImageEncoder as _;
use axum::Router;
use axum::body::to_bytes;
use axum::http::{Method, Request, StatusCode};
use axum::routing::post;
use std::sync::Arc;
use tower::ServiceExt;
use zip::CompressionMethod;
use zip::write::ZipWriter;

use crate::db::init_db;
use crate::mealie::import_mealie_zip;
use crate::state::AppState;

struct MealieRouteCtx {
    app: Router,
    state: Arc<AppState>,
    _dir: tempfile::TempDir,
}

async fn mealie_route_setup() -> MealieRouteCtx {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let pool = init_db(&db_path).await.expect("init_db");
    let state = Arc::new(AppState { pool });
    let app = Router::new()
        .route("/import/mealie", post(import_mealie_zip))
        .layer(axum::extract::DefaultBodyLimit::max(crate::MAX_BODY_BYTES))
        .with_state(Arc::clone(&state));
    MealieRouteCtx {
        app,
        state,
        _dir: dir,
    }
}

fn mealie_recipe_json(name: &str, slug: &str) -> serde_json::Value {
    json!({
        "name": name,
        "slug": slug,
        "recipe_yield": "2 serving(s)",
        "org_url": "https://example.com/recipe",
        "recipe_ingredient": [
            {"note": "250 g Reis", "original_text": null, "food": null, "unit": null, "quantity": 1.0},
            {"note": "Salz & Pfeffer", "original_text": null, "food": null, "unit": null, "quantity": 1.0}
        ],
        "recipe_instructions": [
            {"text": "Cook rice."},
            {"text": "Season and serve."}
        ]
    })
}

fn make_mealie_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buf);
        let options =
            zip::write::FileOptions::<()>::default().compression_method(CompressionMethod::Stored);
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }
    buf.into_inner()
}

fn make_test_jpeg_bytes() -> Vec<u8> {
    let img = ::image::RgbImage::from_pixel(4, 4, ::image::Rgb([90, 120, 200]));
    let mut buf = Cursor::new(Vec::new());
    ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 82)
        .write_image(img.as_raw(), 4, 4, ::image::ExtendedColorType::Rgb8)
        .unwrap();
    buf.into_inner()
}

fn build_mealie_multipart(zip_bytes: &[u8]) -> (Vec<u8>, String) {
    let boundary = "mealietestboundary";
    let mut body = Vec::new();
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"mealie.zip\"\r\n",
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

async fn post_mealie_zip(ctx: &MealieRouteCtx, zip_bytes: &[u8]) -> axum::response::Response {
    let (body, content_type) = build_mealie_multipart(zip_bytes);
    ctx.app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/mealie")
                .header("content-type", &content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn given_valid_mealie_zip_when_import_then_creates_meals() {
    let ctx = mealie_route_setup().await;
    let a = serde_json::to_vec(&mealie_recipe_json("Curry Reis", "curry-reis")).unwrap();
    let b = serde_json::to_vec(&mealie_recipe_json("Bratkartoffeln", "bratkartoffeln")).unwrap();
    let zip = make_mealie_zip(&[
        ("recipes/curry-reis/curry-reis.json", &a),
        ("recipes/bratkartoffeln/bratkartoffeln.json", &b),
    ]);
    let response = post_mealie_zip(&ctx, &zip).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let result: crate::export_import::ZipImportResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.created.len(), 2);
    assert_eq!(result.skipped, 0);
    assert!(result.failed.is_empty());
    let meal = result
        .created
        .iter()
        .find(|m| m.name == "Curry Reis")
        .unwrap();
    assert_eq!(meal.portions, Some(2));
    assert_eq!(
        meal.source_url.as_deref(),
        Some("https://example.com/recipe")
    );
    assert_eq!(meal.ingredients.len(), 2);
    assert!(meal.instructions.contains("Cook rice."));
}

#[tokio::test]
async fn given_mealie_zip_with_image_when_import_then_meal_has_image() {
    let ctx = mealie_route_setup().await;
    let a = serde_json::to_vec(&mealie_recipe_json("Foto Curry", "foto-curry")).unwrap();
    let jpeg = make_test_jpeg_bytes();
    let zip = make_mealie_zip(&[
        ("recipes/foto-curry/foto-curry.json", &a),
        ("recipes/foto-curry/images/original.webp", &jpeg),
    ]);
    let response = post_mealie_zip(&ctx, &zip).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let result: crate::export_import::ZipImportResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.created.len(), 1);
    assert!(result.created[0].has_image);
}

#[tokio::test]
async fn given_duplicate_name_when_import_mealie_then_skips() {
    let ctx = mealie_route_setup().await;
    crate::db::insert_meal(
        &ctx.state.pool,
        crate::model::NewMeal {
            name: "Curry Reis".into(),
            ingredients: vec![crate::model::NewIngredientLine {
                name: "rice".into(),
                quantity: None,
            }],
            instructions: "Cook.".into(),
            portions: None,
            source_url: None,
        },
        crate::db::ImageChange::Keep,
    )
    .await
    .unwrap();
    let a = serde_json::to_vec(&mealie_recipe_json("curry reis", "curry-reis")).unwrap();
    let zip = make_mealie_zip(&[("recipes/curry-reis/curry-reis.json", &a)]);
    let response = post_mealie_zip(&ctx, &zip).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let result: crate::export_import::ZipImportResult = serde_json::from_slice(&body).unwrap();
    assert!(result.created.is_empty());
    assert_eq!(result.skipped, 1);
    assert!(result.failed.is_empty());
}

#[tokio::test]
async fn given_zip_without_mealie_recipes_when_import_then_bad_request() {
    let ctx = mealie_route_setup().await;
    let zip = make_mealie_zip(&[("recipes.json", b"{}")]);
    let response = post_mealie_zip(&ctx, &zip).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn given_recipe_without_instructions_when_import_then_fails_honestly() {
    let ctx = mealie_route_setup().await;
    let good = serde_json::to_vec(&mealie_recipe_json("Gut", "gut")).unwrap();
    let bad = serde_json::to_vec(&json!({
        "name": "Leer",
        "slug": "leer",
        "recipe_yield": "2 serving(s)",
        "recipe_ingredient": [{"note": "1 egg"}],
        "recipe_instructions": []
    }))
    .unwrap();
    let zip = make_mealie_zip(&[
        ("recipes/gut/gut.json", &good),
        ("recipes/leer/leer.json", &bad),
    ]);
    let response = post_mealie_zip(&ctx, &zip).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 10 * 1024 * 1024)
        .await
        .unwrap();
    let result: crate::export_import::ZipImportResult = serde_json::from_slice(&body).unwrap();
    assert_eq!(result.created.len(), 1);
    assert_eq!(result.failed.len(), 1);
    assert!(result.failed[0].source.contains("leer"));
}
