//! HTTP surface assembly.
//!
//! One route table serves both production and the integration tests, so the
//! two never drift apart: a route added here is reachable from every test
//! suite and a route that exists in production cannot be untested by
//! construction. Mounting (the `/api` nest and the SPA fallback) stays in
//! `main`, because only production serves the frontend.

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};

use crate::export_import;
use crate::import;
use crate::mealie;
use crate::routes;
use crate::state::AppState;

/// The full `/api` route table.
pub fn api(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/meals", get(routes::list_meals).post(routes::create_meal))
        .route(
            "/meals/{id}",
            get(routes::get_meal)
                .put(routes::update_meal)
                .delete(routes::delete_meal),
        )
        .route("/meals/{id}/image", get(routes::get_meal_image))
        .route("/import/url", post(import::import_from_url))
        .route("/import/llm", post(import::import_from_llm))
        .route("/import/generate", post(import::generate_meal))
        .route("/llm/providers", get(import::llm_providers))
        .route("/llm/models", get(import::llm_models))
        .route("/llm/polish", post(import::polish_instructions))
        .route("/import/paste", post(import::import_from_paste))
        .route("/import/bulk", post(import::import_bulk))
        .route("/import/image-url", post(import::load_image_from_url))
        .route("/export/meals.zip", get(export_import::export_meals_zip))
        .route("/import/mealie", post(mealie::import_mealie_zip))
        .route("/import/zip", post(export_import::import_meals_zip))
        .route("/plans", get(routes::get_plans).post(routes::create_plan))
        .route(
            "/plans/{year}/{week}",
            put(routes::update_plan).delete(routes::delete_plan),
        )
        .route("/bring/items", post(routes::add_bring_item))
        .route("/bring/status", get(routes::get_bring_status))
        .route(
            "/settings",
            get(routes::get_settings).patch(routes::patch_settings),
        )
        .route("/version", get(routes::get_version))
        .layer(DefaultBodyLimit::max(crate::MAX_BODY_BYTES))
        .with_state(state)
}
