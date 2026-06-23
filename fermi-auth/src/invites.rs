//! Invite-side helpers used by the auth flows (Spec 24 §3.8.1).
//!
//! The accept handlers and inbox queries live in
//! `src/handlers/invites.rs` (binary crate). What lives here is
//! anything called from the OIDC/SIWE sign-in path inside this
//! library crate — specifically, the email-claim resolver that
//! binds pre-existing email-only invites to a newly-signed-in
//! user_id.
//!
//! Why a library-side helper at all: `oidc::sync_user` is in this
//! crate and the cleanest hook point is right after the user row is
//! found-or-upserted there. The binary crate (which owns the
//! handler-side invite module) depends on this crate; the dependency
//! can't reverse without a refactor.

use sqlx::PgPool;

use crate::error::AuthError;

/// Bind every pending invite addressed to `email` to the new
/// `user_id`. Called from the OIDC and SIWE sign-in callbacks the
/// first time we learn a (user_id, email) pair.
///
/// Semantics:
///   • Match is `LOWER(invitee_email) = LOWER($email)`. We stored
///     emails lower-cased on invite creation (see
///     `src/handlers/invites.rs::create_invite_row`), but matching
///     defensively here lets us tolerate any pre-existing rows with
///     mixed case from manual SQL insertion.
///   • Only `status='pending'` rows are touched. Already-accepted,
///     declined, revoked, or expired invites are immutable history.
///   • `invitee_user_id` is set to the supplied `user_id`; the
///     `invitee_email` column is nulled in the same UPDATE so the
///     `forecast_invites_recipient_exactly_one` CHECK invariant is
///     preserved (the same dance the accept-handler does — see Spec
///     24 §3.8.1 and the regression we fixed in Sprint 2.3b).
///   • Returns the count of rows back-filled. Callers (sync_user)
///     log this but do not surface it to the user — they'll see the
///     newly-discoverable invites in their inbox on next render.
///
/// Idempotency: calling this twice for the same (user_id, email)
/// is safe — the second call's WHERE matches zero rows because
/// invitee_user_id is now set and invitee_email is now null.
///
/// Not-an-API-surface: do NOT expose this on an HTTP route. The
/// claim is implicit in sign-in; an exposed endpoint would let any
/// authenticated caller claim invites against email addresses they
/// don't own. The auth flows are the only callers because only
/// they can attest "this user owns this email."
pub async fn claim_pending_for_email(
    pool: &PgPool,
    user_id: &str,
    email: &str,
) -> Result<u64, AuthError> {
    if user_id.is_empty() || email.is_empty() {
        // Defensive: neither column should be empty at the OIDC/SIWE
        // hook point. Bail rather than UPDATE on a wildcard match.
        return Ok(0);
    }
    let result = sqlx::query(
        "UPDATE forecast_invites
            SET invitee_user_id = $1,
                invitee_email   = NULL
          WHERE LOWER(invitee_email) = LOWER($2)
            AND status = 'pending'
            AND invitee_user_id IS NULL",
    )
    .bind(user_id)
    .bind(email)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
    Ok(result.rows_affected())
}
