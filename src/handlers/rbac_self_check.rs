//! GET /api/rbac/self-check — v0.10.6 diagnostic.
//!
//! Authenticated one-shot health probe that answers exactly the
//! question the RBAC substrate is designed to guarantee:
//!
//!   "For this caller, does principal.user_id() align with the row
//!    in users.user_id that the JWT sub points at?"
//!
//! When it fails, the response body tells the operator exactly what
//! kind of drift they're looking at and what to do about it. The
//! error class landed multiple times before v0.10.3 as
//! "Backend save failed: your users row and session don't line up".
//! Instead of asking the operator to guess whether that's a stale
//! JWT or a stale deploy, they hit this endpoint and get the
//! definitive answer.
//!
//! # Response shape
//!
//! ```json
//! {
//!   "ok": true,
//!   "principal_user_id":  "<jwt sub>",
//!   "principal_email":    "<jwt email>",
//!   "principal_auth_provider": "google" | "github" | ...,
//!   "server_commit":      "<git sha the server is running>",
//!   "server_version":     "0.10.x",
//!   "users_row": {
//!     "found":            true | false,
//!     "user_id":          "<users.user_id>",
//!     "email":            "<users.email>",
//!     "id":               "<users.id — the PK UUID>",
//!     "auth_provider":    "google" | "github" | ...
//!   },
//!   "invariant_holds":    true | false,
//!   "diagnosis":          "aligned" | "stale_jwt" | "users_row_missing"
//!                         | "email_match_wrong_user_id" | ...,
//!   "remediation":        "human-readable next step"
//! }
//! ```

use axum::{extract::State, http::StatusCode, Json};
use fermi_auth::AuthPrincipal;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

pub async fn rbac_self_check_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let principal_user_id = principal.user_id();

    // Server identity — same commit sha /api/health returns. Lets
    // the console warn if it's talking to a stale backend without
    // parsing HTML or scraping.
    let commit = std::env::var("RAILWAY_GIT_COMMIT_SHA")
        .or_else(|_| std::env::var("GIT_SHA"))
        .or_else(|_| std::env::var("SOURCE_VERSION"))
        .ok()
        .or_else(|| option_env!("GIT_SHA").map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let commit_short: String = commit.chars().take(12).collect();
    let server_version = env!("CARGO_PKG_VERSION").to_string();

    let (principal_email, principal_auth_provider) = match &principal {
        AuthPrincipal::User(u) => (
            Some(u.email.clone()),
            Some(format!("{:?}", u.auth_provider).to_lowercase()),
        ),
        AuthPrincipal::ApiKey(_) => (None, Some("api_key".to_string())),
    };

    // Lookup: does users.user_id = principal.user_id() find a row?
    let row_by_uid = sqlx::query(
        "SELECT id, user_id, email, auth_provider \
         FROM users WHERE user_id = $1 LIMIT 1",
    )
    .bind(&principal_user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Fallback lookup by email — reveals whether we're looking at
    // stale-JWT drift (row exists but under a different user_id)
    // vs users-row-missing (no row at all for this email).
    let row_by_email = if let Some(ref email) = principal_email {
        sqlx::query(
            "SELECT id, user_id, email, auth_provider \
             FROM users WHERE lower(email) = lower($1) LIMIT 1",
        )
        .bind(email)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        None
    };

    // Determine the diagnosis in a single ladder.
    let (invariant_holds, diagnosis, remediation, users_row_json) = match (row_by_uid, row_by_email)
    {
        (Some(row), _) => {
            // Happy path: users.user_id = principal.user_id().
            let users_row = users_row_to_json(&row, true);
            (
                true,
                "aligned",
                "No action needed. The RBAC invariant holds for this session.",
                users_row,
            )
        }
        (None, Some(email_row)) => {
            // Email row exists but its user_id doesn't match the
            // session's sub. Two sub-cases:
            //   * users.user_id is NULL / '' → mig 161 was supposed
            //     to backfill this. Server is likely pre-mig-161.
            //   * users.user_id is set to a different value than the
            //     session sub → stale JWT (session was minted before
            //     v0.10.3's sync_user_from_app UPDATE-clause backfill).
            let db_user_id: Option<String> = email_row.try_get("user_id").ok().flatten();
            let is_null_or_empty = db_user_id.as_deref().map(|s| s.is_empty()).unwrap_or(true);
            let users_row = users_row_to_json(&email_row, false);
            if is_null_or_empty {
                (
                    false,
                    "users_row_needs_backfill",
                    "Deployed server is missing migration 161 (v0.10.3 substrate). \
                     Redeploy the backend from main; migration 161 will heal this row \
                     on startup. Alternatively an admin can run \
                     `POST /api/admin/rbac/heal` (v0.10.4).",
                    users_row,
                )
            } else {
                (
                    false,
                    "stale_jwt",
                    "Your session was minted before v0.10.3's user_id backfill. \
                     Sign out of the console and sign back in — the new JWT will \
                     carry the healed user_id. If sign-out doesn't help, contact \
                     support with this response body.",
                    users_row,
                )
            }
        }
        (None, None) => {
            // No row at all for the session's user_id or its email.
            // This shouldn't happen post-v0.10.3 (sync_user_from_app
            // is called on every successful OIDC callback). If it
            // does, either the OIDC callback is failing silently or
            // the row was manually deleted.
            (
                false,
                "users_row_missing",
                "No users row for this session's user_id or email. \
                 Sign out, sign back in — sync_user_from_app should \
                 recreate the row on the next callback. If not, the \
                 OIDC flow is failing silently; check server logs.",
                json!({ "found": false }),
            )
        }
    };

    Ok(Json(json!({
        "ok":                     invariant_holds,
        "invariant_holds":        invariant_holds,
        "diagnosis":              diagnosis,
        "remediation":            remediation,
        "principal_user_id":      principal_user_id,
        "principal_email":        principal_email,
        "principal_auth_provider": principal_auth_provider,
        "server_commit":          commit_short,
        "server_version":         server_version,
        "users_row":              users_row_json,
    })))
}

fn users_row_to_json(row: &sqlx::postgres::PgRow, matched_by_user_id: bool) -> Value {
    let id: Option<uuid::Uuid> = row.try_get("id").ok();
    let user_id: Option<String> = row.try_get("user_id").ok().flatten();
    let email: Option<String> = row.try_get("email").ok();
    let auth_provider: Option<String> = row.try_get("auth_provider").ok().flatten();
    json!({
        "found":            true,
        "matched_by":       if matched_by_user_id { "user_id" } else { "email" },
        "id":               id.map(|u| u.to_string()),
        "user_id":          user_id,
        "email":            email,
        "auth_provider":    auth_provider,
    })
}
