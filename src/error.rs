use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
#[allow(dead_code)]
pub enum AppError {
    #[error("not found")]
    NotFound,

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    PayloadTooLarge(String),

    #[error("{0}")]
    UnprocessableEntity(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg.clone()),
            AppError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::Database(_err) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::BadRequest(format!("invalid JSON: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn assert_response(err: AppError, expected_status: StatusCode, expected_contains: &str) {
        let response: Response = err.into_response();
        assert_eq!(response.status(), expected_status);
        let body_bytes = to_bytes(response.into_body(), 1024)
            .await
            .expect("should read body");
        let body_str = String::from_utf8(body_bytes.to_vec()).expect("valid utf8");
        assert!(
            body_str.contains("error"),
            "body should contain 'error' key, got: {body_str}"
        );
        assert!(
            body_str.contains(expected_contains),
            "body should contain '{expected_contains}', got: {body_str}"
        );
    }

    #[tokio::test]
    async fn given_not_found_when_into_response_then_returns_404() {
        assert_response(AppError::NotFound, StatusCode::NOT_FOUND, "not found").await;
    }

    #[tokio::test]
    async fn given_validation_when_into_response_then_returns_400() {
        assert_response(
            AppError::Validation("name must not be empty".into()),
            StatusCode::BAD_REQUEST,
            "name must not be empty",
        )
        .await;
    }

    #[tokio::test]
    async fn given_bad_request_when_into_response_then_returns_400() {
        assert_response(
            AppError::BadRequest("malformed body".into()),
            StatusCode::BAD_REQUEST,
            "malformed body",
        )
        .await;
    }

    #[tokio::test]
    async fn given_database_error_when_into_response_then_returns_500() {
        // Construct AppError::Database directly from a sqlx error.
        let err = sqlx::Error::ColumnNotFound("nonexistent".into());
        let app_err = AppError::Database(err);
        assert_response(app_err, StatusCode::INTERNAL_SERVER_ERROR, "database error").await;
    }

    #[tokio::test]
    async fn given_internal_when_into_response_then_returns_500() {
        assert_response(
            AppError::Internal("something broke".into()),
            StatusCode::INTERNAL_SERVER_ERROR,
            "something broke",
        )
        .await;
    }

    #[tokio::test]
    async fn given_payload_too_large_when_into_response_then_returns_413() {
        assert_response(
            AppError::PayloadTooLarge("image exceeds 20 MB limit".into()),
            StatusCode::PAYLOAD_TOO_LARGE,
            "image exceeds 20 MB limit",
        )
        .await;
    }

    #[tokio::test]
    async fn given_unprocessable_entity_when_into_response_then_returns_422() {
        assert_response(
            AppError::UnprocessableEntity("could not parse a recipe from input".into()),
            StatusCode::UNPROCESSABLE_ENTITY,
            "could not parse a recipe from input",
        )
        .await;
    }

    #[tokio::test]
    async fn given_serde_json_error_when_converting_then_returns_bad_request() {
        let serde_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let app_err: AppError = serde_err.into();
        assert_response(app_err, StatusCode::BAD_REQUEST, "invalid JSON").await;
    }
}
