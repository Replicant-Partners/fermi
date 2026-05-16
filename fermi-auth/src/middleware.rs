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

/// Extract a token from the request: Bearer header → session cookie → ?token=
/// query parameter (cross-origin SSE fallback).
fn extract_token(req: &Request) -> Option<TokenSource> {
    // 1. Authorization: Bearer <token> — primary path for SDK/API clients.
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(TokenSource::Bearer(token.to_string()));
            }
        }
    }

    // 2. Session cookie — primary path for browser sessions on the same
    //    origin (and Lax-eligible cross-site requests).
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

    // 3. ?token=<jwt> query parameter. Only auth path available to
    //    cross-origin EventSource clients: the SSE spec forbids
    //    EventSource from sending custom headers, and our SameSite=Lax
    //    cookie is blocked on cross-origin connect requests. Treated
    //    as a Bearer JWT (same validation as the Authorization path).
    //
    //    Ordered last so a fresh same-origin cookie always wins over
    //    a stale token someone might paste into a URL.
    if let Some(query) = req.uri().query() {
        for pair in query.split('&') {
            let mut it = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (it.next(), it.next()) {
                if k == "token" && !v.is_empty() {
                    let decoded = percent_encoding::percent_decode_str(v)
                        .decode_utf8()
                        .map(|s| s.into_owned())
                        .unwrap_or_else(|_| v.to_string());
                    if !decoded.is_empty() {
                        return Some(TokenSource::Bearer(decoded));
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
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    fn build(uri: &str, headers: &[(&str, &str)]) -> Request {
        let mut builder = HttpRequest::builder().uri(uri);
        for (k, v) in headers {
            builder = builder.header(*k, *v);
        }
        builder.body(Body::empty()).unwrap()
    }

    fn unwrap_bearer(t: Option<TokenSource>) -> String {
        match t {
            Some(TokenSource::Bearer(s)) => s,
            other => panic!("expected Bearer, got {:?}", debug(&other)),
        }
    }

    fn unwrap_cookie(t: Option<TokenSource>) -> String {
        match t {
            Some(TokenSource::Cookie(s)) => s,
            other => panic!("expected Cookie, got {:?}", debug(&other)),
        }
    }

    fn debug(t: &Option<TokenSource>) -> &'static str {
        match t {
            Some(TokenSource::Bearer(_)) => "Bearer",
            Some(TokenSource::Cookie(_)) => "Cookie",
            None => "None",
        }
    }

    #[test]
    fn extract_token_prefers_authorization_header() {
        // Header beats cookie beats query — verify both fallbacks are
        // ignored when the header is present.
        let req = build(
            "/api/x?token=query-token",
            &[
                ("authorization", "Bearer header-token"),
                ("cookie", "abw_session=cookie-token"),
            ],
        );
        assert_eq!(unwrap_bearer(extract_token(&req)), "header-token");
    }

    #[test]
    fn extract_token_falls_back_to_cookie() {
        // No header — cookie wins over query.
        let req = build(
            "/api/x?token=query-token",
            &[("cookie", "abw_session=cookie-token")],
        );
        assert_eq!(unwrap_cookie(extract_token(&req)), "cookie-token");
    }

    #[test]
    fn extract_token_reads_query_param_when_no_header_no_cookie() {
        // The cross-origin SSE case: EventSource can't set headers and
        // SameSite=Lax cookies are blocked on cross-origin connects.
        let req = build("/api/x?token=query-token", &[]);
        assert_eq!(unwrap_bearer(extract_token(&req)), "query-token");
    }

    #[test]
    fn extract_token_query_param_handles_url_encoding() {
        // kask.bio uses encodeURIComponent on the token before appending
        // to the URL. JWTs only contain [A-Za-z0-9_-.] so the dot is the
        // realistic encoded char; we also accept arbitrary %xx for safety.
        let raw = "abc.def%2Fghi"; // %2F = '/'
        let uri = format!("/api/x?token={}", raw);
        let req = build(&uri, &[]);
        assert_eq!(unwrap_bearer(extract_token(&req)), "abc.def/ghi");
    }

    #[test]
    fn extract_token_query_param_with_other_params() {
        // ?token= can sit alongside other query params in any order.
        let req = build("/api/x?foo=bar&token=t&baz=qux", &[]);
        assert_eq!(unwrap_bearer(extract_token(&req)), "t");

        let req = build("/api/x?token=t&foo=bar", &[]);
        assert_eq!(unwrap_bearer(extract_token(&req)), "t");
    }

    #[test]
    fn extract_token_empty_query_param_is_ignored() {
        // ?token= with no value falls through to None — never let an
        // empty string become a Bearer source.
        let req = build("/api/x?token=", &[]);
        assert!(extract_token(&req).is_none());
    }

    #[test]
    fn extract_token_returns_none_when_all_absent() {
        let req = build("/api/x", &[]);
        assert!(extract_token(&req).is_none());
    }
}
