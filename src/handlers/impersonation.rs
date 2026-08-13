//! Admin "view as user" — read-only impersonation for support and
//! debugging.
//!
//! # Why this exists
//!
//! `can_admin()` short-circuits RBAC (`fermi_auth::rbac::require`) and
//! visibility (`fermi_auth::visibility::can_access`). An admin therefore
//! cannot reproduce a user-visible bug: the 404 the user is reporting is
//! a 200 for the admin, by construction. This mints a short-lived token
//! that resolves to the target user's identity so the platform behaves
//! exactly as it does for them.
//!
//! # The contract
//!
//! | Property | Rule |
//! |---|---|
//! | Effective identity | the target — `principal.user_id()` returns them |
//! | Privileges | the target's, **never** the admin's (`AuthPrincipal::can_admin`) |
//! | Methods | GET/HEAD/OPTIONS only (`read_only` mode) |
//! | Reachability | credential, key-minting, and money paths denied outright |
//! | Lifetime | 30 minutes, revocable, one indexed liveness check per request |
//! | Audit | mandatory written reason; every request recorded; visible to the target |
//!
//! Enforcement lives in `fermi_auth::middleware::impersonation_guard`,
//! not here — this module only mints, ends, and reports.
//!
//! # Eligibility
//!
//! Refused targets: yourself, any admin, and non-login service
//! principals such as `abw-system`. The last two matter most —
//! impersonating a privileged or system principal would convert a
//! diagnostic tool into a privilege-escalation path, since the effective
//! role is what governs access.
//!
//! Managing service principals (`abw-system` and friends) is deliberately
//! *not* a use case for this feature. That belongs to the RBAC substrate:
//! own the resources as the service principal, then grant a human team
//! `Permission::Admin` via `object_shares`. That is attributable per
//! person and revocable; a shared service identity is neither.

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use fermi_auth::{
    create_impersonation_token, AuthPrincipal, ImpersonationMode, User, UserRole,
    IMPERSONATION_COOKIE, IMPERSONATION_DURATION_SECS,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

use super::admin::require_admin;

/// Service principals that may never be impersonated.
///
/// `abw-system` owns the platform's provider credentials
/// (`agent_credentials`) and carries `role = 'admin'` in `users`.
/// Impersonating it would hand the session admin rights *and* the
/// platform's key material.
const NON_IMPERSONATABLE_PRINCIPALS: &[&str] = &["abw-system"];

/// Forces the operator to articulate why. Short strings like "test" are
/// worse than useless in an audit log — they create the appearance of
/// oversight without the substance.
const MIN_REASON_LEN: usize = 10;

#[derive(Deserialize)]
pub struct StartImpersonationRequest {
    /// Target account. Either is accepted; `user_id` wins if both given.
    #[serde(default)]
    pub target_user_id: Option<String>,
    #[serde(default)]
    pub target_email: Option<String>,
    /// Mandatory written justification, recorded on the session.
    pub reason: String,
}

/// `POST /api/admin/impersonate`
///
/// Admin-only. Returns the token and sets `abw_impersonation`, leaving
/// the admin's own `abw_session` cookie untouched so exiting is simply
/// dropping the new cookie.
pub async fn start_impersonation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<StartImpersonationRequest>,
) -> Result<Response, (StatusCode, String)> {
    require_admin(&principal)?;

    // No chaining. Nested impersonation makes "who did this" ambiguous
    // in exactly the situation the audit trail exists to resolve. The
    // guard already denies `/api/admin/*`, so this is defence in depth.
    if principal.is_impersonating() {
        return Err((
            StatusCode::FORBIDDEN,
            "Already in a view-as session. Exit it before starting another.".into(),
        ));
    }

    let reason = req.reason.trim().to_string();
    if reason.len() < MIN_REASON_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "A reason of at least {} characters is required — it is recorded \
                 in the audit log and shown to the user.",
                MIN_REASON_LEN
            ),
        ));
    }

    let admin_id = principal.user_id();
    let admin = load_user(&state, &admin_id)
        .await?
        .ok_or((StatusCode::UNAUTHORIZED, "Admin account not found".into()))?;

    // Resolve the target by id or email.
    let target = match (&req.target_user_id, &req.target_email) {
        (Some(id), _) if !id.trim().is_empty() => load_user(&state, id.trim()).await?,
        (_, Some(email)) if !email.trim().is_empty() => {
            load_user_by_email(&state, email.trim()).await?
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Provide target_user_id or target_email".into(),
            ))
        }
    };
    let target = target.ok_or((StatusCode::NOT_FOUND, "Target user not found".into()))?;

    // ─── Eligibility ────────────────────────────────────────────────
    if target.user_id == admin.user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "You are already yourself — impersonating your own account does nothing.".into(),
        ));
    }
    if NON_IMPERSONATABLE_PRINCIPALS.contains(&target.user_id.as_str()) {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "`{}` is a system principal and cannot be impersonated. Administer \
                 its resources by granting your team admin rights on them instead.",
                target.user_id
            ),
        ));
    }
    // Impersonating an admin would *retain* admin rights (the effective
    // role governs access), defeating the isolation this feature relies
    // on and producing an unattributable admin session.
    if target.role == UserRole::Admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Cannot view as another admin: the session would carry admin rights \
             under a second identity."
                .into(),
        ));
    }

    // ─── Mint ───────────────────────────────────────────────────────
    let session_id = uuid::Uuid::new_v4();
    let mode = ImpersonationMode::ReadOnly;
    let ttl = IMPERSONATION_DURATION_SECS;

    // Insert the audit row *before* issuing the token: the guard treats
    // "no live session row" as "refuse", so a failure here fails closed.
    sqlx::query(
        "INSERT INTO impersonation_sessions
             (session_id, admin_user_id, target_user_id, reason, mode, expires_at)
         VALUES ($1, $2, $3, $4, $5, NOW() + ($6 || ' seconds')::interval)",
    )
    .bind(session_id)
    .bind(&admin.user_id)
    .bind(&target.user_id)
    .bind(&reason)
    .bind(mode.as_str())
    .bind(ttl.to_string())
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (token, exp) =
        create_impersonation_token(&target, &admin, session_id, mode, ttl, &state.jwt_secret)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let body = json!({
        "session_id": session_id,
        "token": token,
        "mode": mode.as_str(),
        "expires_at_unix": exp,
        "expires_in_secs": ttl,
        "target": {
            "user_id": target.user_id,
            "email": target.email,
            "display_name": target.display_name,
            "role": target.role,
        },
        "note": "Read-only. Credential, API-key and billing paths are blocked. \
                 Every request is logged and visible to the user.",
    });

    // SameSite=Lax + HttpOnly, matching the session cookie in
    // handlers::auth. Max-Age mirrors the JWT so the browser drops it at
    // the same moment the token stops validating.
    let cookie = format!(
        "{}={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={}",
        IMPERSONATION_COOKIE, token, ttl
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::SET_COOKIE, cookie)
        .body(axum::body::Body::from(body.to_string()))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// `POST /api/admin/impersonate/end`
///
/// Ends the caller's view-as session and clears the cookie.
///
/// Deliberately **not** admin-gated. The impersonated principal is (by
/// design) not an admin, so requiring `can_admin()` would trap the admin
/// inside the session until it expired. Ending a session is a
/// de-escalation and holding the token is sufficient authority to do it.
/// The route is exempted from the guard's deny-list for the same reason.
pub async fn end_impersonation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Response, (StatusCode, String)> {
    let ended = if let Some(imp) = principal.impersonation() {
        sqlx::query(
            "UPDATE impersonation_sessions
                SET ended_at = NOW(), end_reason = 'exited'
              WHERE session_id = $1 AND ended_at IS NULL",
        )
        .bind(imp.session_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .rows_affected()
            > 0
    } else {
        // Not impersonating. Still clear the cookie so a stale or
        // malformed one can always be cleaned up by calling this.
        false
    };

    let body = json!({
        "ended": ended,
        "message": if ended {
            "View-as session ended. You are yourself again."
        } else {
            "No active view-as session; cookie cleared."
        },
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::SET_COOKIE,
            format!(
                "{}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
                IMPERSONATION_COOKIE
            ),
        )
        .body(axum::body::Body::from(body.to_string()))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct SessionsQuery {
    /// Filter to sessions targeting one user.
    #[serde(default)]
    pub target_user_id: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
}

/// `GET /api/admin/impersonate/sessions` — the audit surface.
pub async fn list_impersonation_sessions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(q): Query<SessionsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    require_admin(&principal)?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);

    let rows = sqlx::query(
        "SELECT s.session_id, s.admin_user_id, s.target_user_id, s.reason, s.mode,
                s.created_at, s.expires_at, s.ended_at, s.end_reason,
                COALESCE(au.display_name, au.email, s.admin_user_id)  AS admin_display,
                COALESCE(tu.display_name, tu.email, s.target_user_id) AS target_display,
                (s.ended_at IS NULL AND s.expires_at > NOW())         AS is_live,
                (SELECT COUNT(*) FROM impersonation_events e
                  WHERE e.session_id = s.session_id)                  AS request_count,
                (SELECT COUNT(*) FROM impersonation_events e
                  WHERE e.session_id = s.session_id AND e.blocked)    AS blocked_count
           FROM impersonation_sessions s
           LEFT JOIN users au ON au.user_id = s.admin_user_id
           LEFT JOIN users tu ON tu.user_id = s.target_user_id
          WHERE ($1::text IS NULL OR s.target_user_id = $1)
          ORDER BY s.created_at DESC
          LIMIT $2",
    )
    .bind(&q.target_user_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions: Vec<Value> = rows.iter().map(session_row_to_json).collect();
    Ok(Json(json!({
        "count": sessions.len(),
        "sessions": sessions,
    })))
}

/// `GET /api/me/impersonation-history`
///
/// The transparency counterpart: any user can see who has viewed their
/// account and why. A privileged capability the affected party cannot
/// inspect is indistinguishable from a backdoor — this is what makes
/// the feature defensible rather than merely convenient.
pub async fn my_impersonation_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Uses the *effective* identity: while viewing as someone, this
    // correctly shows their history, not the admin's.
    let user_id = principal.user_id();

    let rows = sqlx::query(
        "SELECT s.session_id, s.admin_user_id, s.target_user_id, s.reason, s.mode,
                s.created_at, s.expires_at, s.ended_at, s.end_reason,
                COALESCE(au.display_name, au.email, s.admin_user_id)  AS admin_display,
                COALESCE(tu.display_name, tu.email, s.target_user_id) AS target_display,
                (s.ended_at IS NULL AND s.expires_at > NOW())         AS is_live,
                (SELECT COUNT(*) FROM impersonation_events e
                  WHERE e.session_id = s.session_id)                  AS request_count,
                (SELECT COUNT(*) FROM impersonation_events e
                  WHERE e.session_id = s.session_id AND e.blocked)    AS blocked_count
           FROM impersonation_sessions s
           LEFT JOIN users au ON au.user_id = s.admin_user_id
           LEFT JOIN users tu ON tu.user_id = s.target_user_id
          WHERE s.target_user_id = $1
          ORDER BY s.created_at DESC
          LIMIT 100",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions: Vec<Value> = rows.iter().map(session_row_to_json).collect();
    Ok(Json(json!({
        "user_id": user_id,
        "count": sessions.len(),
        "sessions": sessions,
    })))
}

fn session_row_to_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "session_id": r.try_get::<uuid::Uuid, _>("session_id").ok(),
        "admin_user_id": r.try_get::<String, _>("admin_user_id").ok(),
        "admin_display": r.try_get::<Option<String>, _>("admin_display").ok().flatten(),
        "target_user_id": r.try_get::<String, _>("target_user_id").ok(),
        "target_display": r.try_get::<Option<String>, _>("target_display").ok().flatten(),
        "reason": r.try_get::<String, _>("reason").ok(),
        "mode": r.try_get::<String, _>("mode").ok(),
        "created_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
        "expires_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("expires_at").ok(),
        "ended_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ended_at").ok().flatten(),
        "end_reason": r.try_get::<Option<String>, _>("end_reason").ok().flatten(),
        "is_live": r.try_get::<Option<bool>, _>("is_live").ok().flatten().unwrap_or(false),
        "request_count": r.try_get::<i64, _>("request_count").ok(),
        "blocked_count": r.try_get::<i64, _>("blocked_count").ok(),
    })
}

// ─── User loading ───────────────────────────────────────────────────

const USER_COLUMNS: &str = "user_id, email, display_name, role, auth_provider, \
                            github_username, google_id, ethereum_address, ens_name";

async fn load_user(state: &AppState, user_id: &str) -> Result<Option<User>, (StatusCode, String)> {
    let row = sqlx::query(&format!(
        "SELECT {} FROM users WHERE user_id = $1 LIMIT 1",
        USER_COLUMNS
    ))
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(row.as_ref().map(row_to_user))
}

async fn load_user_by_email(
    state: &AppState,
    email: &str,
) -> Result<Option<User>, (StatusCode, String)> {
    let row = sqlx::query(&format!(
        "SELECT {} FROM users WHERE LOWER(email) = LOWER($1) LIMIT 1",
        USER_COLUMNS
    ))
    .bind(email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(row.as_ref().map(row_to_user))
}

fn row_to_user(r: &sqlx::postgres::PgRow) -> User {
    User {
        user_id: r.try_get("user_id").unwrap_or_default(),
        email: r.try_get("email").unwrap_or_default(),
        display_name: r.try_get("display_name").ok().flatten(),
        role: parse_role(r.try_get::<Option<String>, _>("role").ok().flatten()),
        auth_provider: parse_provider(
            r.try_get::<Option<String>, _>("auth_provider")
                .ok()
                .flatten(),
        ),
        github_username: r.try_get("github_username").ok().flatten(),
        google_id: r.try_get("google_id").ok().flatten(),
        ethereum_address: r.try_get("ethereum_address").ok().flatten(),
        ens_name: r.try_get("ens_name").ok().flatten(),
    }
}

/// Unknown roles resolve to the least privileged value. A role string we
/// don't recognise must never widen what a session can reach.
fn parse_role(role: Option<String>) -> UserRole {
    match role.as_deref() {
        Some("admin") => UserRole::Admin,
        Some("developer") => UserRole::Developer,
        _ => UserRole::Viewer,
    }
}

fn parse_provider(provider: Option<String>) -> fermi_auth::AuthProvider {
    use fermi_auth::AuthProvider;
    match provider.as_deref() {
        Some("github") => AuthProvider::GitHub,
        Some("google") => AuthProvider::Google,
        Some("ethereum") => AuthProvider::Ethereum,
        _ => AuthProvider::Email,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unknown role strings must fall to `Viewer`. If a future role were
    /// to default to `Admin`, impersonating an account carrying it would
    /// silently produce an admin session.
    #[test]
    fn unknown_roles_default_to_viewer() {
        assert_eq!(parse_role(Some("admin".into())), UserRole::Admin);
        assert_eq!(parse_role(Some("developer".into())), UserRole::Developer);
        assert_eq!(parse_role(Some("viewer".into())), UserRole::Viewer);
        assert_eq!(parse_role(Some("superuser".into())), UserRole::Viewer);
        assert_eq!(parse_role(None), UserRole::Viewer);
    }

    #[test]
    fn abw_system_is_not_impersonatable() {
        assert!(NON_IMPERSONATABLE_PRINCIPALS.contains(&"abw-system"));
    }

    #[test]
    fn reason_minimum_is_long_enough_to_be_meaningful() {
        assert!("test".len() < MIN_REASON_LEN);
        assert!("debugging forecast 404".len() >= MIN_REASON_LEN);
    }
}
