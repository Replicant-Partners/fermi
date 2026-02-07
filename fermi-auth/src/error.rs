use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing authorization token")]
    MissingToken,

    #[error("Invalid or expired token")]
    InvalidToken,

    #[error("Token signature verification failed")]
    InvalidSignature,

    #[error("Authentication configuration error")]
    ConfigError,

    #[error("Insufficient permissions: {0}")]
    Forbidden(String),

    #[error("User not found")]
    UserNotFound,

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Invalid Ethereum address")]
    InvalidAddress,

    #[error("Invalid Ethereum address format")]
    InvalidEthereumAddress,

    #[error("Invalid domain")]
    InvalidDomain,

    #[error("Invalid SIWE message format")]
    InvalidMessage,

    #[error("Domain mismatch")]
    DomainMismatch,

    #[error("Nonce not found")]
    NonceNotFound,

    #[error("Nonce already used (replay attack?)")]
    NonceAlreadyUsed,

    #[error("Nonce expired")]
    NonceExpired,

    #[error("Message expired")]
    MessageExpired,

    #[error("SIWE verification failed")]
    VerificationFailed,

    #[error("OAuth error: {0}")]
    OAuthError(String),
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::MissingToken => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::ConfigError => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AuthError::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            AuthError::UserNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AuthError::DatabaseError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error occurred".to_string(),
            ),
            AuthError::InvalidAddress => (StatusCode::BAD_REQUEST, self.to_string()),
            AuthError::InvalidEthereumAddress => (StatusCode::BAD_REQUEST, self.to_string()),
            AuthError::InvalidDomain => (StatusCode::BAD_REQUEST, self.to_string()),
            AuthError::InvalidMessage => (StatusCode::BAD_REQUEST, self.to_string()),
            AuthError::DomainMismatch => (StatusCode::FORBIDDEN, self.to_string()),
            AuthError::NonceNotFound => (StatusCode::BAD_REQUEST, self.to_string()),
            AuthError::NonceAlreadyUsed => (StatusCode::FORBIDDEN, self.to_string()),
            AuthError::NonceExpired => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::MessageExpired => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::VerificationFailed => (StatusCode::UNAUTHORIZED, self.to_string()),
            AuthError::OAuthError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AuthError {
    fn from(err: sqlx::Error) -> Self {
        AuthError::DatabaseError(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AuthError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        AuthError::InvalidToken
    }
}
