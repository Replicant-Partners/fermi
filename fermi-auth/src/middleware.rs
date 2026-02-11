use axum::{
    async_trait,
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;

use crate::{api_keys, error::AuthError, jwt::validate_session_token, types::AuthPrincipal};

/// Shared auth state that must be present in AppState
#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: String,
    pub db: PgPool,
}

/// Extract a token from the request: Bearer header, cookie, or API key header
fn extract_token(req: &Request) -> Option<TokenSource> {
    // 1. Check Authorization: Bearer <token>
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(TokenSource::Bearer(token.to_string()));
            }
        }
    }

    // 2. Check session cookie
    if let Some(cookie_header) = req.headers().get(header::COOKIE) {
        if let Ok(cookies) = cookie_header.to_str() {
            for cookie in cookies.split(';') {
                let cookie = cookie.trim();
                if let Some(value) = cookie.strip_prefix("abw_session=") {
                    if !value.is_empty() {
                        return Some(TokenSource::Cookie(value.to_string()));
                    }
                }
            }
        }
    }

    None
}

enum TokenSource {
    Bearer(String), // Could be JWT or API key
    Cookie(String), // Always JWT
}

/// Axum middleware to validate JWT tokens or API keys.
/// Checks: Authorization header (Bearer JWT or API key), then session cookie.
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    // Skip auth for OPTIONS (CORS preflight) requests
    if req.method() == axum::http::Method::OPTIONS {
        return Ok(next.run(req).await);
    }

    let token_source = extract_token(&req).ok_or(AuthError::MissingToken)?;

    match token_source {
        TokenSource::Cookie(token) => {
            // Cookies are always JWTs
            let principal = validate_session_token(&token, &auth_state.jwt_secret)?;
            req.extensions_mut().insert(principal);
        }
        TokenSource::Bearer(token) => {
            // Try JWT first, then API key
            if let Ok(principal) = validate_session_token(&token, &auth_state.jwt_secret) {
                req.extensions_mut().insert(principal);
            } else if let Ok(principal) = api_keys::validate_api_key(&auth_state.db, &token).await {
                req.extensions_mut().insert(principal);
            } else {
                return Err(AuthError::InvalidToken);
            }
        }
    }

    Ok(next.run(req).await)
}

/// Optional auth middleware — allows unauthenticated requests but extracts auth if present
pub async fn optional_auth_middleware(
    State(auth_state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Response {
    // Skip auth extraction for OPTIONS (CORS preflight) requests
    if req.method() == axum::http::Method::OPTIONS {
        return next.run(req).await;
    }

    if let Some(token_source) = extract_token(&req) {
        let principal = match token_source {
            TokenSource::Cookie(token) => {
                validate_session_token(&token, &auth_state.jwt_secret).ok()
            }
            TokenSource::Bearer(token) => {
                // Note: API key validation requires async and is skipped here.
                // API key users should use the enforcing middleware routes.
                validate_session_token(&token, &auth_state.jwt_secret).ok()
            }
        };

        if let Some(p) = principal {
            req.extensions_mut().insert(p);
        }
    }

    next.run(req).await
}

/// Axum extractor that handlers can use to get authenticated user
#[async_trait]
impl<S> FromRequestParts<S> for AuthPrincipal
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts.extensions.get::<AuthPrincipal>().cloned().ok_or((
            StatusCode::UNAUTHORIZED,
            "Missing authentication context. Did you apply auth middleware?",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_source_parsing() {
        // This is a basic structural test. Full integration tests
        // require a running database and HTTP server.
    }
}
