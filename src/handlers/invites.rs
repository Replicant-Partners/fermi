//! Invite lifecycle handlers (Spec 24 §3.3, Sprint 2.3a).
//!
//! Endpoints:
//!
//!   POST   /api/forecasts/:id/invites
//!   POST   /api/portfolios/:id/invites
//!   POST   /api/teams/:id/invites
//!   GET    /api/me/invites
//!   POST   /api/invites/:id/decline
//!   DELETE /api/invites/:id
//!
//! State machine (subset shipped here — accept lands in Sprint 2.3b):
//!
//!   pending ─── decline ──► declined
//!           ─── revoke  ──► revoked
//!           ─── expire  ──► expired   (cron, not implemented yet)
//!           ─── accept  ──► accepted  (Sprint 2.3b — writes into
//!                                       object_shares / team_members)
//!
//! Wave-1 ACL on POST (matches `shares.rs`):
//!   • forecast/portfolio: caller must own the target row.
//!   • team: caller must be team owner or admin (TeamRole::can_invite).
//!
//! The `permission` column on `forecast_invites` accepts both share
//! permissions (view/edit/admin for forecast/portfolio targets) and
//! team roles (owner/admin/member/viewer for team targets). We
//! validate the *value* against the *target type* before the INSERT
//! — the table's CHECK is permissive across both vocabularies.
//!
//! Schema-drift caveat: `forecast_invites.inviter_id` and
//! `invitee_user_id` are TEXT and carry the principal's
//! `user_id()` directly. `users.user_id` is text in prod, so this
//! round-trips for direct user-id invites. Email invites resolve to a
//! user_id later via the email-claim hook (Sprint 2.3c).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{teams, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;

// ─── Request shape ─────────────────────────────────────────────────────

/// Wire-format for `POST /api/<target>/:id/invites`.
///
/// Exactly one of `invitee_user_id` / `invitee_email` must be non-null —
/// enforced both server-side and by the
/// `forecast_invites_recipient_exactly_one` DB CHECK. The wire keeps the
/// fields flat (rather than a nested `invitee` object) because serde
/// gives a much cleaner error message on missing/extra fields when the
/// alternatives are sibling Option<String> values.
#[derive(Debug, Deserialize)]
pub struct InviteRequest {
    #[serde(default)]
    pub invitee_user_id: Option<String>,
    #[serde(default)]
    pub invitee_email: Option<String>,
    /// `"view"|"edit"|"admin"` for forecast/portfolio invites.
    /// `"owner"|"admin"|"member"|"viewer"` for team invites.
    pub permission: String,
    #[serde(default)]
    pub message: Option<String>,
}

// ─── Token generation ──────────────────────────────────────────────────

/// 32-char hex token for shareable invite links. Matches the
/// single-use-token pattern at src/handlers/agents.rs:1333-1339 so
/// callers familiar with that flow recognise the shape. We only mint
/// for email invites — user-id invites are discoverable in the
/// invitee's `/api/me/invites` inbox so the token is unused there.
fn mint_invite_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

// ─── ACL helpers — same shapes as src/handlers/shares.rs ───────────────

async fn require_owner_of_forecast(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT owner_id::text FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match owner {
        None => Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some(oid) if oid == principal.user_id() => Ok(()),
        Some(_) => Err((StatusCode::FORBIDDEN, "Not your forecast".into())),
    }
}

async fn require_owner_of_portfolio(
    pool: &PgPool,
    portfolio_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT owner_id::text FROM fermi_portfolios WHERE id = $1",
    )
    .bind(portfolio_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match owner {
        None => Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
        Some(oid) if oid == principal.user_id() => Ok(()),
        Some(_) => Err((StatusCode::FORBIDDEN, "Not your portfolio".into())),
    }
}

/// Team invite gate — caller must be team owner OR admin
/// (`TeamRole::can_invite`). Mirrors `add_member_handler` (teams.rs:206).
async fn require_team_invite_authority(
    pool: &PgPool,
    team_id: Uuid,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let role = teams::get_member_role(pool, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((
            StatusCode::FORBIDDEN,
            "You are not a member of this team".into(),
        ))?;
    if role.can_invite() {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            format!(
                "Role '{}' cannot invite — must be owner or admin",
                role.as_str()
            ),
        ))
    }
}

// ─── Common invite-creation logic ──────────────────────────────────────

/// Validate the request body, parse exactly-one-of-recipient,
/// validate the permission value against the target type, and INSERT
/// the row. Returns the resulting row as JSON for the response.
async fn create_invite_row(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    inviter_id: &str,
    body: &InviteRequest,
) -> Result<JsonValue, (StatusCode, String)> {
    // Recipient: exactly one of user_id / email, trimmed and non-empty.
    let user_id = body
        .invitee_user_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let email = body
        .invitee_email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase);
    match (user_id, &email) {
        (Some(_), Some(_)) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "invitee_user_id and invitee_email are mutually exclusive".into(),
            ))
        }
        (None, None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                "exactly one of invitee_user_id or invitee_email is required".into(),
            ))
        }
        _ => {}
    }

    // Permission must match the target vocabulary.
    let permission = body.permission.trim();
    let permission_ok = match target_type {
        "forecast" | "portfolio" => matches!(permission, "view" | "edit" | "admin"),
        "team" => matches!(permission, "owner" | "admin" | "member" | "viewer"),
        _ => false, // unreachable — caller controls target_type
    };
    if !permission_ok {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "permission '{}' is not valid for target_type '{}'",
                permission, target_type
            ),
        ));
    }

    // Mint a token for email invites only. User-id invites surface in
    // the inbox and don't need a shareable link.
    let token = email.as_ref().map(|_| mint_invite_token());

    // INSERT and return the row. The DB enforces
    // forecast_invites_recipient_exactly_one as a backstop in case the
    // Rust-side check above ever drifts.
    let row: (Uuid, String, String, String, Option<String>, Option<String>,
             Option<String>, String, Option<String>, String,
             chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as(
            "INSERT INTO forecast_invites
                (target_type, target_id, permission, invitee_user_id, invitee_email,
                 token, inviter_id, message)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             RETURNING id, target_type, target_id, permission,
                       invitee_user_id, invitee_email, token, inviter_id,
                       message, status, expires_at, created_at",
        )
        .bind(target_type)
        .bind(target_id)
        .bind(permission)
        .bind(user_id)
        .bind(email.as_deref())
        .bind(token.as_deref())
        .bind(inviter_id)
        .bind(body.message.as_deref())
        .fetch_one(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Notification on creation (Spec 24 §3.7). Fire only when the
    // recipient is a known user — for email-only invites we can't
    // notify until the email-claim resolver in Sprint 2.3c.
    if let Some(uid) = row.4.as_deref() {
        let notif_type = match target_type {
            "forecast" => "forecast_shared",
            "portfolio" => "portfolio_shared",
            "team" => "team_invite",
            _ => "system",
        };
        crate::create_notification(
            pool,
            uid,
            notif_type,
            "You have a new invite",
            Some(&format!(
                "Open your inbox to accept the invite to this {}.",
                target_type
            )),
        )
        .await;
    }

    Ok(json!({
        "id":              row.0,
        "target_type":     row.1,
        "target_id":       row.2,
        "permission":      row.3,
        "invitee_user_id": row.4,
        "invitee_email":   row.5,
        "token":           row.6,
        "inviter_id":      row.7,
        "message":         row.8,
        "status":          row.9,
        "expires_at":      row.10.to_rfc3339(),
        "created_at":      row.11.to_rfc3339(),
    }))
}

// ─── POST /api/forecasts/:id/invites ───────────────────────────────────

pub async fn invite_to_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(body): Json<InviteRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    require_owner_of_forecast(&state.db, &forecast_id, &principal).await?;
    let invite = create_invite_row(
        &state.db,
        "forecast",
        &forecast_id,
        &principal.user_id(),
        &body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(invite)))
}

// ─── POST /api/portfolios/:id/invites ──────────────────────────────────

pub async fn invite_to_portfolio_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
    Json(body): Json<InviteRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    require_owner_of_portfolio(&state.db, &portfolio_id, &principal).await?;
    let invite = create_invite_row(
        &state.db,
        "portfolio",
        &portfolio_id,
        &principal.user_id(),
        &body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(invite)))
}

// ─── POST /api/teams/:id/invites ───────────────────────────────────────

pub async fn invite_to_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
    Json(body): Json<InviteRequest>,
) -> Result<(StatusCode, Json<JsonValue>), (StatusCode, String)> {
    require_team_invite_authority(&state.db, team_id, &principal).await?;
    // target_id is the team UUID as text — matches the convention
    // can_access uses for share_target on team-shared objects
    // (fermi-auth/src/visibility.rs:93).
    let invite = create_invite_row(
        &state.db,
        "team",
        &team_id.to_string(),
        &principal.user_id(),
        &body,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(invite)))
}

// ─── GET /api/me/invites ───────────────────────────────────────────────

/// List pending invites addressed to the calling user, by user_id only.
///
/// Email-claim is Sprint 2.3c's job — once it lands, the OIDC/SIWE
/// callback UPDATEs invitee_user_id on every matching pending row, so
/// this query (keyed by invitee_user_id) automatically picks them up.
/// Until then, this endpoint shows only invites that were created
/// directly against a known user_id.
pub async fn list_my_invites_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT id, target_type, target_id, permission,
                invitee_user_id, invitee_email, token, inviter_id,
                message, status, expires_at, created_at
         FROM forecast_invites
         WHERE invitee_user_id = $1 AND status = 'pending'
         ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let invites: Vec<JsonValue> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id":              r.0,
                "target_type":     r.1,
                "target_id":       r.2,
                "permission":      r.3,
                "invitee_user_id": r.4,
                "invitee_email":   r.5,
                // We deliberately do NOT return the token to the
                // invitee — they don't need it (they already see the
                // row in their inbox), and surfacing it would make
                // log/screenshot leakage more likely.
                "inviter_id":      r.7,
                "message":         r.8,
                "status":          r.9,
                "expires_at":      r.10.to_rfc3339(),
                "created_at":      r.11.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "invites": invites,
        "count": invites.len(),
    })))
}

// ─── POST /api/invites/:id/decline ─────────────────────────────────────

/// The invitee declines an invite. Only valid if the caller is the
/// invitee AND the invite is still pending.
pub async fn decline_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();
    // Atomically transition pending → declined for this specific
    // invitee. If the row is for someone else, or already in a
    // terminal state, the WHERE matches nothing and we 404/409 below.
    let updated = sqlx::query(
        "UPDATE forecast_invites
            SET status = 'declined'
          WHERE id = $1
            AND status = 'pending'
            AND invitee_user_id = $2",
    )
    .bind(invite_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if updated.rows_affected() == 0 {
        // Disambiguate: does the row even exist?
        let row_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM forecast_invites WHERE id = $1)",
        )
        .bind(invite_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if !row_exists {
            return Err((StatusCode::NOT_FOUND, "Invite not found".into()));
        }
        // The row exists but isn't pending for us — either not ours or
        // already terminal. 409 is the honest answer.
        return Err((
            StatusCode::CONFLICT,
            "Invite is not pending for this user".into(),
        ));
    }
    Ok(Json(json!({ "status": "declined", "id": invite_id })))
}

// ─── DELETE /api/invites/:id ───────────────────────────────────────────

/// The inviter (or, for team invites, any team owner/admin) revokes a
/// pending invite. We don't physically DELETE — we transition to
/// `revoked` so the lifecycle history is preserved. Sprint 2.3b's
/// accept path also sets a terminal status, never deletes. The cron
/// `expire` path will do the same.
pub async fn revoke_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Look up the row so we can apply the correct authority gate.
    let invite: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT target_type, target_id, inviter_id, status
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let (target_type, target_id, inviter_id, status) = invite
        .ok_or((StatusCode::NOT_FOUND, "Invite not found".into()))?;
    if status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Invite already {} — cannot revoke", status),
        ));
    }

    // Authority gate.
    if inviter_id == user_id {
        // Inviter can always revoke their own.
    } else if target_type == "team" {
        // For team invites, team admins/owners can also revoke.
        let team_uuid = Uuid::parse_str(&target_id).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Stored team invite has non-UUID target_id".into(),
            )
        })?;
        require_team_invite_authority(&state.db, team_uuid, &principal).await?;
    } else {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the inviter can revoke this invite".into(),
        ));
    }

    // Atomic transition. WHERE status='pending' guards against a
    // concurrent accept/decline that snuck in between the SELECT and
    // the UPDATE — better to 409 than to overwrite a terminal state.
    let updated = sqlx::query(
        "UPDATE forecast_invites SET status = 'revoked'
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(invite_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if updated.rows_affected() == 0 {
        return Err((
            StatusCode::CONFLICT,
            "Invite was modified concurrently — refresh and retry".into(),
        ));
    }
    Ok(Json(json!({ "status": "revoked", "id": invite_id })))
}
