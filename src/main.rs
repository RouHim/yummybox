mod bring;
mod data_dir;
mod db;
mod error;
mod image;
mod jsonld;
mod llm_import;

mod model;
mod recipe;
mod routes;
mod seed;
mod state;
mod static_assets;

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

enum Subcommand {
    Serve,
    Seed,
    Unknown,
}

fn parse_subcommand(args: &[String]) -> Subcommand {
    match args.get(1).map(String::as_str) {
        Some("seed") => Subcommand::Seed,
        Some(_) => Subcommand::Unknown,
        None => Subcommand::Serve,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    match parse_subcommand(&std::env::args().collect::<Vec<_>>()) {
        Subcommand::Seed => return run_seed().await,
        Subcommand::Unknown => {
            eprintln!("usage: mealme [seed]");
            std::process::exit(2);
        }
        Subcommand::Serve => {}
    }
    let env_value = std::env::var("MEALME_DATA_DIR").ok();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let data_dir = data_dir::resolve_data_dir(env_value.as_deref(), &cwd);
    data_dir::ensure_data_dir(&data_dir).map_err(|e| {
        tracing::error!("failed to initialize data directory: {e}");
        e
    })?;
    let db_path = data_dir::db_path_in(&data_dir);
    info!("using data directory at {}", data_dir.display());
    info!("using database at {}", db_path.display());

    let pool = db::init_db(&db_path).await.map_err(|e| {
        tracing::error!("failed to initialize database: {e}");
        e
    })?;

    let state = Arc::new(AppState { pool });
    let api = Router::new()
        .route("/meals", get(routes::list_meals).post(routes::create_meal))
        .route(
            "/meals/{id}",
            get(routes::get_meal)
                .put(routes::update_meal)
                .delete(routes::delete_meal),
        )
        .route("/meals/{id}/image", get(routes::get_meal_image))
        .route("/import/url", post(routes::import_from_url))
        .route("/import/llm", post(routes::import_from_llm))
        .route("/llm/providers", get(routes::llm_providers))
        .route("/llm/models", get(routes::llm_models))
        .route("/import/paste", post(routes::import_from_paste))
        .route("/import/bulk", post(routes::import_bulk))
        .route("/import/image-url", post(routes::load_image_from_url))
        .route("/plans", get(routes::get_plans).post(routes::create_plan))
        .route(
            "/plans/{year}/{week}",
            put(routes::update_plan).delete(routes::delete_plan),
        )
        .route("/bring/items", post(routes::add_bring_item))
        .route("/bring/status", get(routes::get_bring_status))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
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

async fn run_seed() -> Result<(), Box<dyn std::error::Error>> {
    use crate::seed::{SeedOutcome, run};

    let env_value = std::env::var("MEALME_DATA_DIR").ok();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let data_dir = data_dir::resolve_data_dir(env_value.as_deref(), &cwd);
    if let Err(e) = data_dir::ensure_data_dir(&data_dir) {
        eprintln!("error: failed to initialize data directory: {e}");
        std::process::exit(1);
    }
    let db_path = data_dir::db_path_in(&data_dir);
    tracing::info!("using data directory at {}", data_dir.display());

    let pool = match db::init_db(&db_path).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to initialize database: {e}");
            std::process::exit(1);
        }
    };

    match run(&pool).await {
        Ok(SeedOutcome::Inserted(n)) => {
            println!("seeded {n} meals into {}", db_path.display());
            Ok(())
        }
        Ok(SeedOutcome::Skipped) => {
            println!("meals already exist in {}; seed skipped", db_path.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("error: seeding failed: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_no_args_when_parse_subcommand_then_serve() {
        let args: Vec<String> = vec!["mealme".into()];
        match parse_subcommand(&args) {
            Subcommand::Serve => {}
            _ => panic!("expected Serve"),
        }
    }

    #[test]
    fn given_seed_arg_when_parse_subcommand_then_seed() {
        let args: Vec<String> = vec!["mealme".into(), "seed".into()];
        match parse_subcommand(&args) {
            Subcommand::Seed => {}
            _ => panic!("expected Seed"),
        }
    }

    #[test]
    fn given_known_other_arg_when_parse_subcommand_then_unknown() {
        let args: Vec<String> = vec!["mealme".into(), "foo".into()];
        match parse_subcommand(&args) {
            Subcommand::Unknown => {}
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn given_extra_args_with_seed_when_parse_subcommand_then_seed() {
        let args: Vec<String> = vec![
            "mealme".into(),
            "seed".into(),
            "--extra".into(),
            "val".into(),
        ];
        match parse_subcommand(&args) {
            Subcommand::Seed => {}
            _ => panic!("expected Seed"),
        }
    }
}
