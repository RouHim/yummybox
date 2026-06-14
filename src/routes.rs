use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use tracing::instrument;

use crate::db;
use crate::error::AppError;
use crate::model::{Meal, MealPatch, NewMeal};
use crate::state::AppState;

#[instrument(skip(state))]
pub async fn list_meals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<Meal>>, AppError> {
    let conn = state.conn.lock().await;
    let search = params.get("search").map(String::as_str);
    let meals = db::list_meals(&conn, search)?;
    Ok(Json(meals))
}

pub async fn get_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Meal>, AppError> {
    let conn = state.conn.lock().await;
    let meal = db::find_meal(&conn, id)?;
    Ok(Json(meal))
}

#[instrument(skip(state))]
pub async fn create_meal(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NewMeal>,
) -> Result<(StatusCode, Json<Meal>), AppError> {
    let conn = state.conn.lock().await;
    let meal = db::insert_meal(&conn, payload)?;
    Ok((StatusCode::CREATED, Json(meal)))
}

#[instrument(skip(state))]
pub async fn update_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(payload): Json<MealPatch>,
) -> Result<Json<Meal>, AppError> {
    let conn = state.conn.lock().await;
    let meal = db::update_meal(&conn, id, payload)?;
    Ok(Json(meal))
}

#[instrument(skip(state))]
pub async fn delete_meal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let conn = state.conn.lock().await;
    db::delete_meal(&conn, id)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::{Method, Request, StatusCode};
    use axum::routing::get;
    use serde_json::json;
    use tokio::sync::Mutex;
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
        let conn = init_db(&db_path).expect("init_db");
        let state = Arc::new(AppState {
            conn: Mutex::new(conn),
        });
        let app = Router::new()
            .route("/meals", get(list_meals).post(create_meal))
            .route(
                "/meals/:id",
                get(get_meal).put(update_meal).delete(delete_meal),
            )
            .with_state(state);
        TestCtx { app, _dir: dir }
    }

    async fn create_meal_helper(ctx: &TestCtx, name: &str, ingredients: &str) -> Meal {
        let body = serde_json::to_vec(&json!({"name": name, "ingredients": ingredients})).unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

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
        let body =
            serde_json::to_vec(&json!({"name": "Pasta", "ingredients": "noodles, sauce"})).unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let meal: Meal = serde_json::from_slice(&body).unwrap();
        assert_eq!(meal.name, "Pasta");
        assert_eq!(meal.ingredients, "noodles, sauce");
        assert!(meal.id > 0);
    }

    #[tokio::test]
    async fn given_empty_name_when_post_meals_then_returns_400_with_error() {
        let ctx = setup().await;
        let body = serde_json::to_vec(&json!({"name": "", "ingredients": "x"})).unwrap();
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/meals")
                    .header("content-type", "application/json")
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
        let meal = create_meal_helper(&ctx, "Original", "stuff").await;
        let body =
            serde_json::to_vec(&json!({"name": "Updated", "ingredients": "new stuff"})).unwrap();
        let response = ctx
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri(format!("/meals/{}", meal.id))
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let updated: Meal = serde_json::from_slice(&body).unwrap();
        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.ingredients, "new stuff");
        assert_eq!(updated.id, meal.id);
    }

    #[tokio::test]
    async fn given_missing_meal_when_put_meal_then_returns_404() {
        let ctx = setup().await;
        let body = serde_json::to_vec(&json!({"name": "X", "ingredients": "Y"})).unwrap();
        let response = ctx
            .app
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/meals/999")
                    .header("content-type", "application/json")
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
        let meal = create_meal_helper(&ctx, "ToDelete", "x").await;
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

        // Verify it's gone
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
        let _ = create_meal_helper(&ctx, "Test", "stuff").await;
        let _ = create_meal_helper(&ctx, "Other", "test ingredient").await;

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
}
