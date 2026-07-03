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

    #[error("{0}")]
    BringAuthFailed(String),

    #[error("{0}")]
    BringNetworkError(String),

    #[error("no Bring! lists found in your account")]
    BringNoLists,

    #[error("{0}")]
    Llm(String, &'static str), // (message, error_code)
    #[error("a meal with this name already exists")]
    DuplicateName,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message, code) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string(), None),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone(), None),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, msg.clone(), None),
            AppError::UnprocessableEntity(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, msg.clone(), None)
            }
            AppError::Database(_err) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string(), None),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone(), None),
            AppError::BringAuthFailed(msg) => (StatusCode::UNAUTHORIZED, msg.clone(), None),
            AppError::BringNetworkError(msg) => (StatusCode::BAD_GATEWAY, msg.clone(), None),
            AppError::BringNoLists => (StatusCode::NOT_FOUND, self.to_string(), None),
            AppError::Llm(msg, code) => {
                let status = match *code {
                    "llm_api_key_missing" => StatusCode::BAD_REQUEST,
                    "llm_parse_failed" => StatusCode::UNPROCESSABLE_ENTITY,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                (status, msg.clone(), Some(*code))
            }
            AppError::DuplicateName => (StatusCode::CONFLICT, self.to_string(), None),
        };
        (status, Json(json!({ "error": message, "code": code }))).into_response()
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
            body_str.contains("\"error\""),
            "body should contain 'error' key, got: {body_str}"
        );
        assert!(
            body_str.contains(expected_contains),
            "body should contain '{expected_contains}', got: {body_str}"
        );
        assert!(
            body_str.contains("\"code\""),
            "body should contain 'code' key, got: {body_str}"
        );
    }

    async fn assert_response_code(
        err: AppError,
        expected_status: StatusCode,
        expected_contains: &str,
        expected_code: &str,
    ) {
        let response: Response = err.into_response();
        assert_eq!(response.status(), expected_status);
        let body_bytes = to_bytes(response.into_body(), 1024)
            .await
            .expect("should read body");
        let body_str = String::from_utf8(body_bytes.to_vec()).expect("valid utf8");
        assert!(
            body_str.contains("\"error\""),
            "body should contain 'error' key, got: {body_str}"
        );
        assert!(
            body_str.contains(expected_contains),
            "body should contain '{expected_contains}', got: {body_str}"
        );
        assert!(
            body_str.contains(&format!("\"code\":\"{expected_code}\"")),
            "body should contain code '{expected_code}', got: {body_str}"
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

    #[tokio::test]
    async fn given_bring_auth_failed_when_into_response_then_returns_401() {
        assert_response(
            AppError::BringAuthFailed(
                "Bring! login failed — check BRING_EMAIL and BRING_PASSWORD".into(),
            ),
            StatusCode::UNAUTHORIZED,
            "Bring! login failed",
        )
        .await;
    }

    #[tokio::test]
    async fn given_bring_network_error_when_into_response_then_returns_502() {
        assert_response(
            AppError::BringNetworkError(
                "Could not reach Bring! — check your network connection".into(),
            ),
            StatusCode::BAD_GATEWAY,
            "Could not reach Bring!",
        )
        .await;
    }

    #[tokio::test]
    async fn given_bring_no_lists_when_into_response_then_returns_404() {
        assert_response(
            AppError::BringNoLists,
            StatusCode::NOT_FOUND,
            "no Bring! lists found",
        )
        .await;
    }

    #[tokio::test]
    async fn given_llm_api_key_missing_when_into_response_then_returns_400() {
        assert_response_code(
            AppError::Llm("API key missing".into(), "llm_api_key_missing"),
            StatusCode::BAD_REQUEST,
            "API key missing",
            "llm_api_key_missing",
        )
        .await;
    }

    #[tokio::test]
    async fn given_llm_timeout_when_into_response_then_returns_500() {
        assert_response_code(
            AppError::Llm("timed out".into(), "llm_timeout"),
            StatusCode::INTERNAL_SERVER_ERROR,
            "timed out",
            "llm_timeout",
        )
        .await;
    }

    #[tokio::test]
    async fn given_llm_parse_failed_when_into_response_then_returns_422() {
        assert_response_code(
            AppError::Llm("parse failed".into(), "llm_parse_failed"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "parse failed",
            "llm_parse_failed",
        )
        .await;
    }

    #[tokio::test]
    async fn given_llm_model_not_found_when_into_response_then_returns_500() {
        assert_response_code(
            AppError::Llm("model not found".into(), "llm_model_not_found"),
            StatusCode::INTERNAL_SERVER_ERROR,
            "model not found",
            "llm_model_not_found",
        )
        .await;
    }

    #[tokio::test]
    async fn given_llm_request_failed_when_into_response_then_returns_500() {
        assert_response_code(
            AppError::Llm("request failed".into(), "llm_request_failed"),
            StatusCode::INTERNAL_SERVER_ERROR,
            "request failed",
            "llm_request_failed",
        )
        .await;
    }

    #[tokio::test]
    async fn given_duplicate_name_error_when_response_then_returns_409() {
        assert_response(
            AppError::DuplicateName,
            StatusCode::CONFLICT,
            "a meal with this name already exists",
        )
        .await;
    }
}
