//! Invite lifecycle handlers (Spec 24 §3.3, Sprints 2.3a + 2.3b).
//!
//! Endpoints:
//!
//!   POST   /api/forecasts/:id/invites          (2.3a)
//!   POST   /api/portfolios/:id/invites         (2.3a)
//!   POST   /api/teams/:id/invites              (2.3a)
//!   GET    /api/forecasts/:id/invites          (target-scoped, admin view)
//!   GET    /api/portfolios/:id/invites         (target-scoped, admin view)
//!   GET    /api/teams/:id/invites              (target-scoped, admin view)
//!   GET    /api/me/invites                     (2.3a)
//!   GET    /api/me/invites/sent                (invites the caller has sent)
//!   POST   /api/invites/:id/decline            (2.3a)
//!   DELETE /api/invites/:id                    (2.3a)
//!   POST   /api/invites/:id/accept             (2.3b)
//!   GET    /api/invites/by-token/:token        (2.3b, optional auth)
//!   POST   /api/invites/by-token/:token/accept (2.3b)
//!
//! State machine:
//!
//!   pending ─── decline ──► declined
//!           ─── revoke  ──► revoked
//!           ─── expire  ──► expired   (cron, not implemented yet)
//!           ─── accept  ──► accepted  (materialises grant in
//!                                       object_shares / team_members)
//!
//! Wave-2 ACL on POST (Sprint 2.4, matches `shares.rs`):
//!   • forecast/portfolio: caller must have `can_admin` access.
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
use fermi_auth::visibility::{can_access, can_view};
use fermi_auth::{
    teams, AuthPrincipal, MemberType, ObjectType, Permission, ShareType, TeamRole, Visibility,
};
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

async fn require_admin_of_forecast(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT owner_id::text, visibility FROM fermi_forecasts WHERE id = $1")
            .bind(forecast_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {
        None => Err((StatusCode::NOT_FOUND, "Forecast not found".into())),
        Some((owner_id, visibility)) => {
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                principal,
                ObjectType::Forecast,
                forecast_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if level.has_admin() {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, "Admin access required".into()))
            }
        }
    }
}

async fn require_admin_of_portfolio(
    pool: &PgPool,
    portfolio_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT owner_id::text, visibility FROM fermi_portfolios WHERE id = $1")
            .bind(portfolio_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match row {
        None => Err((StatusCode::NOT_FOUND, "Portfolio not found".into())),
        Some((owner_id, visibility)) => {
            let vis = Visibility::from_legacy(&visibility);
            let level = can_access(
                pool,
                principal,
                ObjectType::Portfolio,
                portfolio_id,
                &owner_id,
                vis,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if level.has_admin() {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, "Admin access required".into()))
            }
        }
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
    let row: (
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
    ) = sqlx::query_as(
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
    require_admin_of_forecast(&state.db, &forecast_id, &principal).await?;
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
    require_admin_of_portfolio(&state.db, &portfolio_id, &principal).await?;
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
        let row_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM forecast_invites WHERE id = $1)")
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
    let (target_type, target_id, inviter_id, status) =
        invite.ok_or((StatusCode::NOT_FOUND, "Invite not found".into()))?;
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

// ─── Sprint 2.3b: accept paths ────────────────────────────────────────

/// All the columns the accept paths need from the invite row. We load
/// once and then dispatch on `target_type`. Status is included so the
/// caller can distinguish 404 (row missing) from 409 (already
/// terminal).
#[derive(Debug)]
struct InviteAcceptRow {
    id: Uuid,
    target_type: String,
    target_id: String,
    permission: String,
    invitee_user_id: Option<String>,
    invitee_email: Option<String>,
    inviter_id: String,
    status: String,
    expired: bool,
}

/// Load an invite row by primary key (the inbox-resolved path).
async fn load_invite_by_id(
    pool: &PgPool,
    invite_id: Uuid,
) -> Result<InviteAcceptRow, (StatusCode, String)> {
    let row: Option<(
        Uuid,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        bool,
    )> = sqlx::query_as(
        "SELECT id, target_type, target_id, permission,
                invitee_user_id, invitee_email, inviter_id, status,
                (expires_at < NOW()) AS expired
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let row = row.ok_or((StatusCode::NOT_FOUND, "Invite not found".into()))?;
    Ok(InviteAcceptRow {
        id: row.0,
        target_type: row.1,
        target_id: row.2,
        permission: row.3,
        invitee_user_id: row.4,
        invitee_email: row.5,
        inviter_id: row.6,
        status: row.7,
        expired: row.8,
    })
}

/// Load an invite by its email-link token. Unknown tokens 404 so the
/// landing page can render a neutral "this invite is no longer valid"
/// without exposing whether the token ever existed.
async fn load_invite_by_token(
    pool: &PgPool,
    token: &str,
) -> Result<InviteAcceptRow, (StatusCode, String)> {
    let row: Option<(
        Uuid,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        bool,
    )> = sqlx::query_as(
        "SELECT id, target_type, target_id, permission,
                invitee_user_id, invitee_email, inviter_id, status,
                (expires_at < NOW()) AS expired
         FROM forecast_invites WHERE token = $1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let row = row.ok_or((StatusCode::NOT_FOUND, "Invite not found".into()))?;
    Ok(InviteAcceptRow {
        id: row.0,
        target_type: row.1,
        target_id: row.2,
        permission: row.3,
        invitee_user_id: row.4,
        invitee_email: row.5,
        inviter_id: row.6,
        status: row.7,
        expired: row.8,
    })
}

/// Authority check for accept: the caller must be the rightful
/// invitee.
///
///   • If `invitee_user_id` is set: caller's `user_id()` must match.
///   • Else (email-only): caller must be an `AuthPrincipal::User`
///     whose `.email` matches `invitee_email` case-insensitively.
///     ApiKey principals fail this branch — they don't carry email.
///
/// The email-claim resolver in Sprint 2.3c will populate
/// `invitee_user_id` on sign-in, so over time more invites flow
/// through the cheap user_id branch.
fn require_caller_is_invitee(
    row: &InviteAcceptRow,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let caller_user_id = principal.user_id();
    if let Some(ref uid) = row.invitee_user_id {
        if uid == &caller_user_id {
            return Ok(());
        }
        return Err((
            StatusCode::FORBIDDEN,
            "This invite was sent to a different user".into(),
        ));
    }
    // Email-only invite — fall back to email match.
    let email = match principal {
        AuthPrincipal::User(u) => u.email.to_lowercase(),
        AuthPrincipal::ApiKey(_) => {
            return Err((
                StatusCode::FORBIDDEN,
                "API-key callers cannot accept email-only invites (no email claim)".into(),
            ))
        }
    };
    match row.invitee_email.as_deref().map(str::to_lowercase) {
        Some(invite_email) if invite_email == email => Ok(()),
        _ => Err((
            StatusCode::FORBIDDEN,
            "This invite was sent to a different email".into(),
        )),
    }
}

/// Materialise the grant for an accepted invite. Dispatches on
/// `target_type` — forecast/portfolio write to `object_shares`, team
/// writes to `team_members`. Both helpers are idempotent
/// (ON CONFLICT DO UPDATE) so a re-attempt is safe; the lifecycle
/// commitment happens at the status-flip below.
async fn materialise_grant(
    pool: &PgPool,
    row: &InviteAcceptRow,
    accepter_user_id: &str,
) -> Result<(), (StatusCode, String)> {
    match row.target_type.as_str() {
        "forecast" | "portfolio" => {
            let object_type = if row.target_type == "forecast" {
                ObjectType::Forecast
            } else {
                ObjectType::Portfolio
            };
            let permission = match row.permission.as_str() {
                "edit" => Permission::Edit,
                "admin" => Permission::Admin,
                _ => Permission::View,
            };
            teams::share_object(
                pool,
                object_type,
                &row.target_id,
                ShareType::User,
                accepter_user_id,
                permission,
                &row.inviter_id,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        "team" => {
            let team_id = Uuid::parse_str(&row.target_id).map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Stored team invite has non-UUID target_id".into(),
                )
            })?;
            let role = TeamRole::from_str(&row.permission);
            teams::add_team_member(
                pool,
                team_id,
                MemberType::User,
                accepter_user_id,
                role,
                &row.inviter_id,
            )
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        }
        other => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Stored invite has unknown target_type '{}'", other),
            ));
        }
    }
    Ok(())
}

/// Core accept logic shared by the by-id and by-token paths.
///
/// Lifecycle commitment: the status-flip is `UPDATE … WHERE id = $1
/// AND status = 'pending'`. If a concurrent accept/decline beats us by
/// one ms, we get 0 rows-affected and return 409 — the grant the
/// other transaction materialised stays; ours becomes a harmless
/// no-op (because the share_object/add_team_member helpers use
/// ON CONFLICT DO UPDATE, the second write is idempotent).
async fn accept_invite_core(
    pool: &PgPool,
    row: InviteAcceptRow,
    principal: &AuthPrincipal,
) -> Result<JsonValue, (StatusCode, String)> {
    // Pre-flight: the invite must be acceptable. Order matters — we
    // surface the most actionable error first.
    if row.status != "pending" {
        return Err((
            StatusCode::CONFLICT,
            format!("Invite is {}, no longer pending", row.status),
        ));
    }
    if row.expired {
        // Auto-transition to 'expired' so the inbox stops showing it.
        // Best-effort: a failure here doesn't block the 409 we owe
        // the caller.
        let _ = sqlx::query(
            "UPDATE forecast_invites SET status = 'expired'
             WHERE id = $1 AND status = 'pending'",
        )
        .bind(row.id)
        .execute(pool)
        .await;
        return Err((StatusCode::CONFLICT, "Invite has expired".into()));
    }

    require_caller_is_invitee(&row, principal)?;

    let accepter_user_id = principal.user_id();
    materialise_grant(pool, &row, &accepter_user_id).await?;

    // Status flip — the source of truth for "did this accept succeed".
    // WHERE status='pending' is the concurrency guard.
    //
    // For email-only invites we back-fill invitee_user_id with the
    // accepter, AND null out invitee_email in the same UPDATE. The
    // forecast_invites_recipient_exactly_one CHECK forbids both
    // columns being non-null, so we have to choose one — and the
    // accept moment is when "this invite is now owned by user X" is
    // the truthful description. Historical email-of-invite info is
    // recoverable from inviter activity / notifications.
    let updated = sqlx::query(
        "UPDATE forecast_invites
            SET status = 'accepted',
                accepted_at = NOW(),
                invitee_user_id = COALESCE(invitee_user_id, $2),
                invitee_email = CASE
                    WHEN invitee_user_id IS NULL THEN NULL
                    ELSE invitee_email
                END
          WHERE id = $1 AND status = 'pending'",
    )
    .bind(row.id)
    .bind(&accepter_user_id)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if updated.rows_affected() == 0 {
        // Someone (or some other request from us) raced us. The grant
        // has already been materialised idempotently, so the caller's
        // intent is satisfied — but the lifecycle row is owned by the
        // first writer. 409 keeps the API contract honest.
        return Err((
            StatusCode::CONFLICT,
            "Invite was accepted, declined, or revoked concurrently".into(),
        ));
    }

    // Notify the inviter (Spec 24 §3.7 — type='invite_accepted').
    // Best-effort: a notification failure must not break the accept.
    let target_label = match row.target_type.as_str() {
        "forecast" => "forecast",
        "portfolio" => "portfolio",
        "team" => "team",
        _ => "invite",
    };
    crate::create_notification(
        pool,
        &row.inviter_id,
        "invite_accepted",
        "Your invite was accepted",
        Some(&format!(
            "Your {} invite is now active for the recipient.",
            target_label
        )),
    )
    .await;

    Ok(json!({
        "status": "accepted",
        "id": row.id,
        "target_type": row.target_type,
        "target_id": row.target_id,
        "permission": row.permission,
    }))
}

// ─── POST /api/invites/:id/accept ──────────────────────────────────────

/// Accept an invite addressed to the calling user (by inbox). The
/// caller must already be authenticated; the invitee identity is
/// `invitee_user_id` on the row.
pub async fn accept_invite_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(invite_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let row = load_invite_by_id(&state.db, invite_id).await?;
    let body = accept_invite_core(&state.db, row, &principal).await?;
    Ok(Json(body))
}

// ─── GET /api/invites/by-token/:token  (optional auth) ─────────────────

/// Public-friendly preview of an invite linked by token. Returns just
/// enough for the landing page to say "X invited you to forecast Y."
/// Auth optional — the link is the only credential. We return 404 for
/// already-terminal invites so the landing page UX is uniform.
pub async fn get_invite_by_token_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let row = load_invite_by_token(&state.db, &token).await?;
    if row.status != "pending" || row.expired {
        return Err((StatusCode::NOT_FOUND, "Invite is no longer valid".into()));
    }

    // Resolve inviter display_name when we can — falls back to the
    // raw inviter_id. The landing page renders "{display_name}
    // invited you to {target_type}".
    let inviter_display: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM users WHERE user_id = $1 LIMIT 1")
            .bind(&row.inviter_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    Ok(Json(json!({
        "id":             row.id,
        "target_type":    row.target_type,
        "target_id":      row.target_id,
        "permission":     row.permission,
        // Email is echoed back so the landing page can prompt
        // "Sign in as alice@example.com to accept." We do NOT
        // include the resolved invitee_user_id (if any) — that's a
        // claim the caller hasn't proven yet.
        "invitee_email":  row.invitee_email,
        "inviter_display_name": inviter_display,
        "expires_at":     // surface via load? we don't have it on the row.
                          // Sprint 2.3b explicit decision: the landing
                          // page treats 'pending' as "still valid";
                          // expiry was already gated above. Skip.
                          serde_json::Value::Null,
    })))
}

// ─── POST /api/invites/by-token/:token/accept ──────────────────────────

/// Accept an invite by its token. Requires auth. The token resolves
/// the invite; identity is verified against `invitee_user_id` (if
/// set) or `invitee_email` (if email-only) the same way the inbox
/// accept path does — so the link cannot be forwarded to a different
/// recipient.
pub async fn accept_invite_by_token_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(token): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let row = load_invite_by_token(&state.db, &token).await?;
    let body = accept_invite_core(&state.db, row, &principal).await?;
    Ok(Json(body))
}

// ─── Target-scoped invite listings ─────────────────────────────────────
//
// The console needs "invites I sent for this forecast/portfolio/team" so
// the operator can see pending outbound invitations with their status.
// Without this the invite flow is fire-and-forget — the toast disappears
// and the invite is invisible until the recipient accepts (materialising
// as a share/member) or declines (silently).
//
// Each endpoint enforces the same admin gate as the corresponding
// POST /invites route: whoever can invite can also see the pending list.

fn map_target_invite_row(
    r: (
        Uuid,
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        String,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
) -> JsonValue {
    // Prefer display_name, then email, then the raw user_id.
    let invitee_display_name = r.10.clone().or_else(|| r.4.clone()).or_else(|| r.3.clone());
    let inviter_display_name =
        r.11.clone()
            .or_else(|| r.12.clone())
            .or_else(|| Some(r.5.clone()));
    json!({
        "id":                     r.0,
        "status":                 r.1,
        "permission":             r.2,
        "invitee_user_id":        r.3,
        "invitee_email":          r.4,
        "inviter_id":             r.5,
        "message":                r.6,
        "target_type":            r.7,
        "expires_at":             r.8.to_rfc3339(),
        "created_at":             r.9.to_rfc3339(),
        "invitee_display_name":   invitee_display_name,
        "inviter_display_name":   inviter_display_name,
    })
}

async fn list_target_invites(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
) -> Result<Vec<JsonValue>, (StatusCode, String)> {
    // LEFT JOIN users twice (invitee + inviter) to enrich the UI.
    // We do a text-based join because forecast_invites.*_id columns are
    // TEXT and users.user_id may be TEXT (Zitadel/API-key) or a UUID
    // rendered as text (email/OIDC). Casting both sides to text keeps
    // this compatible across auth providers.
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            Option<String>, // invitee.display_name
            Option<String>, // inviter.display_name
            Option<String>, // inviter.email
        ),
    >(
        "SELECT fi.id, fi.status, fi.permission,
                fi.invitee_user_id, fi.invitee_email,
                fi.inviter_id, fi.message, fi.target_type,
                fi.expires_at, fi.created_at,
                iu.display_name AS invitee_display_name,
                nu.display_name AS inviter_display_name,
                nu.email AS inviter_email
         FROM forecast_invites fi
         LEFT JOIN users iu ON iu.user_id::text = fi.invitee_user_id
         LEFT JOIN users nu ON nu.user_id::text = fi.inviter_id
         WHERE fi.target_type = $1 AND fi.target_id = $2
         ORDER BY
           CASE fi.status WHEN 'pending' THEN 0 ELSE 1 END,
           fi.created_at DESC",
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(rows.into_iter().map(map_target_invite_row).collect())
}

/// GET /api/forecasts/:id/invites — pending + terminal invites for a forecast.
pub async fn list_forecast_invites_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    require_admin_of_forecast(&state.db, &forecast_id, &principal).await?;
    let invites = list_target_invites(&state.db, "forecast", &forecast_id).await?;
    Ok(Json(json!({ "invites": invites, "count": invites.len() })))
}

/// GET /api/portfolios/:id/invites — pending + terminal invites for a portfolio.
pub async fn list_portfolio_invites_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(portfolio_id): Path<String>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    require_admin_of_portfolio(&state.db, &portfolio_id, &principal).await?;
    let invites = list_target_invites(&state.db, "portfolio", &portfolio_id).await?;
    Ok(Json(json!({ "invites": invites, "count": invites.len() })))
}

/// GET /api/teams/:id/invites — pending + terminal invites for a team.
pub async fn list_team_invites_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<Uuid>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    require_team_invite_authority(&state.db, team_id, &principal).await?;
    let invites = list_target_invites(&state.db, "team", &team_id.to_string()).await?;
    Ok(Json(json!({ "invites": invites, "count": invites.len() })))
}

/// GET /api/me/invites/sent — invites the caller has sent (all statuses).
///
/// Useful for the console operator to inspect their own outbound invite
/// history across every target without having to open each one.
pub async fn list_sent_invites_handler(
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
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
        ),
    >(
        "SELECT fi.id, fi.status, fi.permission,
                fi.invitee_user_id, fi.invitee_email,
                fi.inviter_id, fi.message,
                fi.target_type, fi.target_id,
                fi.expires_at, fi.created_at,
                iu.display_name AS invitee_display_name
         FROM forecast_invites fi
         LEFT JOIN users iu ON iu.user_id::text = fi.invitee_user_id
         WHERE fi.inviter_id = $1
         ORDER BY
           CASE fi.status WHEN 'pending' THEN 0 ELSE 1 END,
           fi.created_at DESC
         LIMIT 200",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let invites: Vec<JsonValue> = rows
        .into_iter()
        .map(|r| {
            let display_name = r.11.clone().or_else(|| r.4.clone()).or_else(|| r.3.clone());
            json!({
                "id":                   r.0,
                "status":               r.1,
                "permission":           r.2,
                "invitee_user_id":      r.3,
                "invitee_email":        r.4,
                "inviter_id":           r.5,
                "message":              r.6,
                "target_type":          r.7,
                "target_id":            r.8,
                "expires_at":           r.9.to_rfc3339(),
                "created_at":           r.10.to_rfc3339(),
                "invitee_display_name": display_name,
            })
        })
        .collect();

    Ok(Json(json!({ "invites": invites, "count": invites.len() })))
}
