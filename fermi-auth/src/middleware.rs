use axum::{
    async_trait,
    extract::{FromRequestParts, Request, State},
    http::{header, request::Parts, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;

use crate::{
    api_keys,
    error::AuthError,
    jwt::validate_session_token,
    types::{AuthPrincipal, ImpersonationMode},
};

/// Shared auth state that must be present in AppState
#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: String,
    pub db: PgPool,
}

/// Cookie carrying an admin "view as user" token.
///
/// Deliberately **separate** from `abw_session` rather than replacing
/// it: the admin's real session stays intact underneath, so exiting is
/// just "delete this cookie" and can never strand an admin logged out
/// of their own account.
pub const IMPERSONATION_COOKIE: &str = "abw_impersonation";

/// Read a named cookie's value out of the request headers.
pub fn cookie_value(req: &Request, name: &str) -> Option<String> {
    let prefix = format!("{}=", name);
    let cookies = req.headers().get(header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|c| {
        c.trim()
            .strip_prefix(&prefix)
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
    })
}

/// Extract a token from the request: Bearer header → impersonation
/// cookie → session cookie → ?token= query parameter (cross-origin SSE
/// fallback).
fn extract_token(req: &Request) -> Option<TokenSource> {
    // 1. Authorization: Bearer <token> — primary path for SDK/API clients.
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return Some(TokenSource::Bearer(token.to_string()));
            }
        }
    }

    // 2. Impersonation cookie — ordered ahead of `abw_session` because
    //    when both are present the admin has explicitly entered a
    //    "view as" session and the narrower identity must win. An
    //    explicit Bearer still beats it (above), matching the existing
    //    header-over-cookie precedence.
    //
    //    Distinct variant so the middleware can fall back to the
    //    admin's own session when this token is expired — see
    //    `resolve_principal`.
    if let Some(value) = cookie_value(req, IMPERSONATION_COOKIE) {
        return Some(TokenSource::Impersonation(value));
    }

    // 3. Session cookie — primary path for browser sessions on the same
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

    // 4. ?token=<jwt> query parameter. Only auth path available to
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

/// Everything `resolve_principal` needs, lifted out of the request
/// synchronously so no borrow of it is held across an await.
struct Credentials {
    token_source: Option<TokenSource>,
    /// The `abw_session` cookie, used only to recover from an unusable
    /// impersonation token.
    session_fallback: Option<String>,
}

fn extract_credentials(req: &Request) -> Credentials {
    Credentials {
        token_source: extract_token(req),
        session_fallback: cookie_value(req, "abw_session"),
    }
}

enum TokenSource {
    Bearer(String), // Could be JWT or API key
    Cookie(String), // Always JWT
    /// The `abw_impersonation` cookie. Always a JWT, and always
    /// recoverable: if it fails to validate we fall back to the
    /// underlying session rather than 401-ing.
    Impersonation(String),
}

/// Resolve a request's principal, with one deliberate fallback.
///
/// The impersonation cookie shadows `abw_session` while it is set. Its
/// `Max-Age` matches the JWT's TTL, but the two can drift apart (clock
/// skew, a cookie sent a moment past expiry, a session revoked
/// server-side). Without a fallback the admin would then be 401'd on
/// *every* request — including the one that clears the cookie — despite
/// holding a perfectly valid session of their own. Locking an admin out
/// of their own account is a strictly worse failure than dropping back
/// to being themselves, so an unusable impersonation token degrades to
/// the underlying session.
///
/// The fallback is scoped to this one case: a bad `Bearer` or a bad
/// `abw_session` still fails, so genuine auth errors stay loud.
///
/// Takes **owned** credentials rather than `&Request` on purpose:
/// `axum::body::Body` is `Send` but not `Sync`, so holding a `&Request`
/// across an `.await` would make this future non-`Send` and the
/// middleware would no longer satisfy tower's `Service` bounds. Callers
/// pull what they need out synchronously via [`extract_credentials`].
async fn resolve_principal(
    auth_state: &AuthState,
    credentials: Credentials,
) -> Result<AuthPrincipal, AuthError> {
    let Credentials {
        token_source,
        session_fallback,
    } = credentials;
    let token_source = token_source.ok_or(AuthError::MissingToken)?;

    match token_source {
        TokenSource::Cookie(token) => {
            // Cookies are always JWTs
            validate_session_token(&token, &auth_state.jwt_secret)
        }
        TokenSource::Impersonation(token) => {
            if let Ok(principal) = validate_session_token(&token, &auth_state.jwt_secret) {
                return Ok(principal);
            }
            // Expired or otherwise unusable — fall back to the admin's
            // own session cookie if one is present.
            let session = session_fallback.ok_or(AuthError::InvalidToken)?;
            validate_session_token(&session, &auth_state.jwt_secret)
        }
        TokenSource::Bearer(token) => {
            // Try JWT first, then API key
            if let Ok(principal) = validate_session_token(&token, &auth_state.jwt_secret) {
                Ok(principal)
            } else {
                api_keys::validate_api_key(&auth_state.db, &token).await
            }
        }
    }
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

    let credentials = extract_credentials(&req);
    let principal = resolve_principal(&auth_state, credentials).await?;
    req.extensions_mut().insert(principal);

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

    // v0.10.10 fix. The old code validated JWTs only in the Bearer
    // branch, with a comment saying "API key validation requires
    // async and is skipped here." That comment was stale — this
    // function IS async, and `api_keys::validate_api_key` is an
    // async fn. The consequence of the skip was that API-key
    // callers (every `ferm_...` token) got treated as anonymous
    // on any route wired through `optional_auth_middleware` — which
    // includes the whole public router, notably
    // `GET /api/agents/:agent_id`. Owners hitting their own
    // private drafts got 404 because their AuthPrincipal was never
    // inserted into request extensions, and the handler correctly
    // returned NOT_FOUND for the anon-on-private branch.
    //
    // Shares `resolve_principal` with `auth_middleware` so the two can
    // never drift; the only difference is that failure is silent here.
    let credentials = extract_credentials(&req);
    if let Ok(principal) = resolve_principal(&auth_state, credentials).await {
        req.extensions_mut().insert(principal);
    }

    next.run(req).await
}

// ═══ Impersonation guard ═════════════════════════════════════════════

/// The route that ends a "view as" session.
///
/// Exempt from every rule below. Ending a session is a *de-escalation*,
/// so requiring privilege to do it would be backwards — and since the
/// impersonated principal is (correctly) not an admin, gating this on
/// `can_admin()` would trap the admin inside the session.
pub const IMPERSONATION_EXIT_PATH: &str = "/api/admin/impersonate/end";

/// Paths an impersonated session may never touch, in **any** mode and
/// by **any** method.
///
/// The boundary this draws: "view as" lets you see what the user sees;
/// it must not let you *become* them durably or extract anything that
/// outlives the session. Everything here either mints a long-lived
/// credential, reveals a secret, or moves money.
const IMPERSONATION_DENIED_PREFIXES: &[&str] = &[
    // Provider keys and integration secrets (mig-039 / mig-171).
    "/api/secrets",
    // Minting an API key here would produce a credential outliving the
    // 30-minute session — impersonation laundered into permanent access.
    "/api/auth/api-keys",
    // Identity/credential mutation on the target account.
    "/api/auth/password",
    // Money movement and payout configuration.
    "/api/wallet/transfer",
    "/api/billing",
    "/api/stripe",
    // Per-agent funding + credential surfaces.
    "/api/agent-credentials",
    // The admin surface. `can_admin()` is already false for an
    // impersonated non-admin so these would 403 anyway; denying by path
    // makes it explicit and keeps the rule true even when the target
    // happens to be privileged.
    "/api/admin",
];

fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn denied_prefix(path: &str) -> Option<&'static str> {
    IMPERSONATION_DENIED_PREFIXES
        .iter()
        .find(|p| path == **p || path.starts_with(&format!("{}/", p)))
        .copied()
}

/// Enforce the impersonation contract and record the audit trail.
///
/// Layered *after* `auth_middleware` / `optional_auth_middleware`, so a
/// principal is already in extensions. Non-impersonated traffic takes an
/// early return and pays only an enum check — no DB work.
///
/// Three jobs, in order:
///   1. **Liveness.** The JWT is stateless, so a session that was exited
///      or revoked would otherwise stay usable until it expired. One
///      indexed lookup makes `impersonation_sessions` authoritative.
///   2. **Read-only.** Reject unsafe methods, and the deny-list above.
///   3. **Audit.** Record every request, blocked or not.
pub async fn impersonation_guard(
    State(auth_state): State<AuthState>,
    req: Request,
    next: Next,
) -> Response {
    let Some(imp) = req
        .extensions()
        .get::<AuthPrincipal>()
        .and_then(|p| p.impersonation())
        .map(|i| (i.session_id, i.mode))
    else {
        return next.run(req).await;
    };
    let (session_id, mode) = imp;

    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // The exit route is always reachable.
    if path == IMPERSONATION_EXIT_PATH {
        return next.run(req).await;
    }

    // 1. Liveness.
    if !session_is_live(&auth_state.db, session_id).await {
        log_event(
            &auth_state.db,
            session_id,
            &method,
            &path,
            None,
            true,
            Some("session_not_live"),
        );
        return impersonation_refusal(
            "This view-as session has ended or expired. Start a new one from the admin console.",
        );
    }

    // 2. Read-only contract.
    let violation = if let Some(prefix) = denied_prefix(&path) {
        Some((
            "denied_path",
            format!(
                "`{}` is not reachable while viewing as another user. \
                 Exit the session and act as yourself.",
                prefix
            ),
        ))
    } else if mode == ImpersonationMode::ReadOnly && !is_safe_method(&method) {
        Some((
            "mutation_in_read_only",
            format!(
                "{} is blocked: view-as sessions are read-only. \
                 Exit the session to act as yourself.",
                method
            ),
        ))
    } else {
        None
    };

    if let Some((reason, message)) = violation {
        log_event(
            &auth_state.db,
            session_id,
            &method,
            &path,
            None,
            true,
            Some(reason),
        );
        return impersonation_refusal(&message);
    }

    // 3. Serve and record.
    let response = next.run(req).await;
    log_event(
        &auth_state.db,
        session_id,
        &method,
        &path,
        Some(response.status().as_u16() as i32),
        false,
        None,
    );
    response
}

fn impersonation_refusal(message: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        [("x-abw-impersonation", "blocked")],
        message.to_string(),
    )
        .into_response()
}

/// Is this session still open? Treats any DB error as "not live": if we
/// cannot verify the audit row, we must not serve the request.
async fn session_is_live(db: &PgPool, session_id: uuid::Uuid) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
             SELECT 1 FROM impersonation_sessions
              WHERE session_id = $1 AND ended_at IS NULL AND expires_at > NOW()
         )",
    )
    .bind(session_id)
    .fetch_one(db)
    .await
    .unwrap_or(false)
}

/// Append to the per-request trail.
///
/// Detached: the audit write must never add latency to, or fail, the
/// request it describes. Sessions are rare and minutes-long, so the
/// volume is trivial.
fn log_event(
    db: &PgPool,
    session_id: uuid::Uuid,
    method: &Method,
    path: &str,
    status: Option<i32>,
    blocked: bool,
    block_reason: Option<&str>,
) {
    let db = db.clone();
    let method = method.to_string();
    let path = path.to_string();
    let block_reason = block_reason.map(|s| s.to_string());
    tokio::spawn(async move {
        let result = sqlx::query(
            "INSERT INTO impersonation_events
                 (session_id, method, path, status, blocked, block_reason)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(session_id)
        .bind(&method)
        .bind(&path)
        .bind(status)
        .bind(blocked)
        .bind(&block_reason)
        .execute(&db)
        .await;
        if let Err(e) = result {
            // Matches the crate's existing logging convention (see
            // oidc.rs) — fermi-auth deliberately carries no tracing dep.
            eprintln!(
                "⚠ failed to record impersonation event for session {}: {}",
                session_id, e
            );
        }
    });
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

    fn unwrap_impersonation(t: Option<TokenSource>) -> String {
        match t {
            Some(TokenSource::Impersonation(s)) => s,
            other => panic!("expected Impersonation, got {:?}", debug(&other)),
        }
    }

    fn debug(t: &Option<TokenSource>) -> &'static str {
        match t {
            Some(TokenSource::Bearer(_)) => "Bearer",
            Some(TokenSource::Cookie(_)) => "Cookie",
            Some(TokenSource::Impersonation(_)) => "Impersonation",
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

    // ─── Impersonation guard ────────────────────────────────────────

    /// When an admin is mid-"view as", the narrower identity must win
    /// over their own still-valid session cookie.
    #[test]
    fn impersonation_cookie_beats_session_cookie() {
        let req = build(
            "/api/x",
            &[(
                "cookie",
                "abw_session=admin-token; abw_impersonation=viewas-token",
            )],
        );
        assert_eq!(unwrap_impersonation(extract_token(&req)), "viewas-token");
    }

    /// An explicit Bearer header still wins, preserving the existing
    /// header-over-cookie precedence for API clients.
    #[test]
    fn bearer_still_beats_impersonation_cookie() {
        let req = build(
            "/api/x",
            &[
                ("authorization", "Bearer header-token"),
                ("cookie", "abw_impersonation=viewas-token"),
            ],
        );
        assert_eq!(unwrap_bearer(extract_token(&req)), "header-token");
    }

    /// Absent the impersonation cookie nothing changes for normal users.
    #[test]
    fn session_cookie_used_when_no_impersonation_cookie() {
        let req = build("/api/x", &[("cookie", "abw_session=admin-token")]);
        assert_eq!(unwrap_cookie(extract_token(&req)), "admin-token");
    }

    /// The fallback that stops an expired view-as token locking an admin
    /// out of their own account: the underlying session must still be
    /// reachable from the same request.
    #[test]
    fn session_cookie_remains_recoverable_alongside_impersonation_cookie() {
        let req = build(
            "/api/x",
            &[(
                "cookie",
                "abw_session=admin-token; abw_impersonation=viewas-token",
            )],
        );
        assert_eq!(
            cookie_value(&req, "abw_session").as_deref(),
            Some("admin-token"),
        );
    }

    #[test]
    fn cookie_value_reads_named_cookies_and_ignores_others() {
        let req = build(
            "/api/x",
            &[("cookie", "other=1; abw_impersonation=tok; another=2")],
        );
        assert_eq!(
            cookie_value(&req, IMPERSONATION_COOKIE).as_deref(),
            Some("tok")
        );
        assert_eq!(cookie_value(&req, "abw_session"), None);
        // An empty value is treated as absent, never as a token.
        let empty = build("/api/x", &[("cookie", "abw_impersonation=")]);
        assert_eq!(cookie_value(&empty, IMPERSONATION_COOKIE), None);
        assert!(extract_token(&empty).is_none());
    }

    #[test]
    fn safe_methods_are_reads_only() {
        assert!(is_safe_method(&Method::GET));
        assert!(is_safe_method(&Method::HEAD));
        assert!(is_safe_method(&Method::OPTIONS));
        for m in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!is_safe_method(&m), "{m} must not be treated as safe");
        }
    }

    #[test]
    fn denied_prefixes_cover_credential_and_money_paths() {
        // Exact match and nested paths both denied.
        assert_eq!(denied_prefix("/api/secrets"), Some("/api/secrets"));
        assert_eq!(
            denied_prefix("/api/secrets/OPENAI_API_KEY"),
            Some("/api/secrets")
        );
        assert_eq!(
            denied_prefix("/api/auth/api-keys"),
            Some("/api/auth/api-keys")
        );
        assert_eq!(
            denied_prefix("/api/wallet/transfer"),
            Some("/api/wallet/transfer")
        );
        assert_eq!(denied_prefix("/api/billing/checkout"), Some("/api/billing"));
        assert_eq!(denied_prefix("/api/admin/users"), Some("/api/admin"));
    }

    /// Prefix matching must be segment-aware: `/api/secretsauce` is a
    /// different resource from `/api/secrets` and must not be swept up.
    #[test]
    fn denied_prefix_matching_is_segment_aware() {
        assert_eq!(denied_prefix("/api/secretsauce"), None);
        assert_eq!(denied_prefix("/api/administrators"), None);
        assert_eq!(denied_prefix("/api/wallet"), None);
        assert_eq!(denied_prefix("/api/wallet/transactions"), None);
    }

    /// Reading the target's own data — the entire point of the feature.
    #[test]
    fn ordinary_read_paths_are_not_denied() {
        for p in [
            "/api/auth/me",
            "/api/agents",
            "/api/forecasts",
            "/api/workspace/abc",
            "/api/apps",
        ] {
            assert_eq!(denied_prefix(p), None, "{p} should be reachable");
        }
    }

    /// The exit route sits under `/api/admin`, which is denied wholesale
    /// — the guard must special-case it or an admin gets trapped in the
    /// session until it expires.
    #[test]
    fn exit_path_is_under_a_denied_prefix_and_must_be_exempted() {
        assert_eq!(denied_prefix(IMPERSONATION_EXIT_PATH), Some("/api/admin"));
    }
}
