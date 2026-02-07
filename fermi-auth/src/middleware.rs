use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{error::AuthError, jwt::validate_jwt, types::AuthPrincipal};

/// Axum middleware to validate JWT tokens from Authorization header
pub async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, AuthError> {
    // Extract Authorization header
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or(AuthError::MissingToken)?;

    // Check for Bearer token
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidToken)?;

    // Try JWT validation first
    if let Ok(principal) = validate_jwt(token).await {
        req.extensions_mut().insert(principal);
        return Ok(next.run(req).await);
    }

    // TODO: Fallback to API key validation
    // if let Ok(principal) = validate_api_key(token).await {
    //     req.extensions_mut().insert(principal);
    //     return Ok(next.run(req).await);
    // }

    Err(AuthError::InvalidToken)
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

/// Optional auth middleware - allows unauthenticated requests but extracts auth if present
pub async fn optional_auth_middleware(mut req: Request, next: Next) -> Response {
    // Try to extract token but don't fail if missing
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                // Try to validate, but ignore errors
                if let Ok(principal) = validate_jwt(token).await {
                    req.extensions_mut().insert(principal);
                }
            }
        }
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn protected_handler(auth: AuthPrincipal) -> String {
        format!("Hello, {}!", auth.user_id())
    }

    async fn optional_handler(auth: Option<AuthPrincipal>) -> String {
        match auth {
            Some(principal) => format!("Hello, {}!", principal.user_id()),
            None => "Hello, anonymous!".to_string(),
        }
    }

    #[tokio::test]
    async fn test_missing_token() {
        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn(auth_middleware));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
