mod db;
mod error;
mod model;
mod routes;
mod state;
mod static_assets;

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let db_path = std::env::var("MEALME_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")).join("meals.db"));
    info!("using database at {}", db_path.display());

    let conn = db::init_db(&db_path).map_err(|e| {
        tracing::error!("failed to initialize database: {e}");
        e
    })?;

    let state = Arc::new(AppState {
        conn: tokio::sync::Mutex::new(conn),
    });

    let api = Router::new()
        .route("/meals", get(routes::list_meals).post(routes::create_meal))
        .route(
            "/meals/:id",
            get(routes::get_meal)
                .put(routes::update_meal)
                .delete(routes::delete_meal),
        )
        .with_state(state);

    let app = Router::new()
        .nest("/api", api)
        .fallback(static_assets::spa_fallback);

    let port: u16 = std::env::var("MEALME_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(11341);
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("listening on http://{addr}");

    axum::serve(listener, app).await?;

    Ok(())
}
