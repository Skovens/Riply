use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use crate::api::ApiResponse;

#[derive(Debug)]
pub enum WebError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
    ServiceUnavailable(String),
}

impl WebError {
    pub fn internal(msg: impl std::fmt::Display) -> Self {
        WebError::Internal(msg.to_string())
    }

    pub fn not_found(msg: impl std::fmt::Display) -> Self {
        WebError::NotFound(msg.to_string())
    }

    pub fn bad_request(msg: impl std::fmt::Display) -> Self {
        WebError::BadRequest(msg.to_string())
    }

    pub fn service_unavailable(msg: impl std::fmt::Display) -> Self {
        WebError::ServiceUnavailable(msg.to_string())
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            WebError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            WebError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            WebError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
            WebError::ServiceUnavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
        };
        let body = Json(ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(message),
        });
        (status, body).into_response()
    }
}
