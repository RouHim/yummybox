// Import/export (ZIP) tests. Kept in a separate flat module so
// src/export_import.rs stays focused on production code.

use std::io::{Cursor, Read, Write};

use chrono::{TimeZone, Utc};
use zip::CompressionMethod;
use zip::read::ZipArchive;
use zip::write::ZipWriter;

use crate::export_import::*;
use crate::model::Meal;
use crate::state::AppState;

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
        portions: None,
        source_url: None,
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
        .layer(axum::extract::DefaultBodyLimit::max(crate::MAX_BODY_BYTES))
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
            portions: None,
            source_url: None,
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
    assert!(cd.starts_with("attachment; filename=\"yummybox-export-"));
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
async fn given_body_over_50mb_when_import_zip_then_413() {
    let ctx = route_setup().await;
    // 53 MB of a single file field trips the 50 MiB body limit
    // mid-field-read, which must surface as 413.
    let oversized = vec![0u8; 53_000_000];
    let (body, content_type) = build_zip_multipart(&oversized);
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
                "name": "pizza",
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
