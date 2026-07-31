//! Substrate-level RBAC helper — one entry point for every ownership
//! check across every tenant app in ABW (Fermi, Rabble, simOps, …).
//!
//! # Contract
//!
//! Every handler that operates on an owned resource — read, edit, or
//! delete — routes through [`require`] (or the convenience wrappers
//! [`require_view`] / [`require_edit`] / [`require_admin`]). This
//! guarantees:
//!
//! 1. The system admin bypass is applied once, in one place.
//! 2. Owner detection uses `principal.user_id() == owner_id` and only
//!    that — no hand-rolled variants that might drift from
//!    [`crate::visibility::can_access`]'s semantics.
//! 3. Public visibility grants View to everyone (including anon).
//! 4. Direct user shares in `object_shares` and team shares via
//!    `team_members` are honoured.
//! 5. Rejections come back with the right HTTP shape: 404 when the
//!    caller has no view at all (don't leak existence), 403 when they
//!    have View but not the requested permission.
//!
//! # Extending to a new tenant resource
//!
//! When a new tenant app lands with a new resource table:
//!
//! 1. Add a variant to [`crate::types::ObjectType`].
//! 2. Add a `SELECT` block to the `rbac_orphans` view (mig 163).
//! 3. Add a `FOREIGN KEY … REFERENCES users(user_id) NOT VALID`
//!    constraint on the owner column (mig 162 pattern).
//! 4. Route every handler for that resource through [`require`].
//!
//! No fifth step. If a fifth step is tempting, the pattern needs a
//! design conversation, not a workaround.
//!
//! # Anti-pattern: don't hand-roll ownership checks
//!
//! Anything of the shape
//!
//! ```ignore
//! if agent.owner_id.as_deref() != Some(&user_id) && !principal.can_admin() {
//!     return Err((StatusCode::FORBIDDEN, "Not owner".into()));
//! }
//! ```
//!
//! is a bug in waiting. Use [`require`] instead. The `lint-owner-columns.sh`
//! CI script flags this shape.

use axum::http::StatusCode;
use sqlx::PgPool;

use crate::error::AuthError;
use crate::types::{AuthPrincipal, ObjectType, Permission, Visibility};
use crate::visibility::{can_access, AccessLevel};

/// Result of an RBAC check. `Ok(())` means the caller has at least the
/// requested permission. Errors are `(StatusCode, String)` ready to
/// return from an axum handler.
pub type RbacResult = Result<AccessLevel, (StatusCode, String)>;

/// Require that `principal` has at least `needed` permission on the
/// specified object. Returns the actual granted `AccessLevel` so
/// callers can, for example, log "admin bypass used".
///
/// Error shape:
/// * `404 NOT FOUND` — caller has no view at all. We don't distinguish
///   "doesn't exist" from "you can't see it" to avoid leaking
///   existence of private resources through response codes.
/// * `403 FORBIDDEN` — caller has View but not the higher permission
///   they asked for. Existence is already known (they could have
///   viewed it), so signalling that they lack write is fine.
///
/// # Arguments
///
/// * `owner_id` — the resource's stored owner (as text; `users.user_id`
///   namespace). Passing `""` or a value not in `users.user_id` is
///   fine — the invariant is enforced by the FK in mig 162; this
///   helper doesn't re-check.
/// * `visibility` — the resource's `Visibility` (private/shared/public).
///   Callers that don't have a visibility model can pass
///   [`Visibility::Private`] to mean "owner + shares only".
pub async fn require(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
    needed: Permission,
) -> RbacResult {
    let level = can_access(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
    )
    .await
    .map_err(map_auth_err)?;

    match (level, needed) {
        // Denied — caller has no visibility at all. 404 to avoid
        // leaking existence.
        (AccessLevel::Denied, _) => Err((
            StatusCode::NOT_FOUND,
            format!("{} not found", object_type.as_str()),
        )),

        // Granted at least what was asked — Permission is Ord (View <
        // Edit < Admin), so `granted >= needed` is the right check.
        (AccessLevel::Granted(granted), _) if granted >= needed => Ok(level),

        // Has View but asked for more — 403.
        (AccessLevel::Granted(_), _) => Err((
            StatusCode::FORBIDDEN,
            format!(
                "{} required on this {}",
                needed.as_str(),
                object_type.as_str()
            ),
        )),
    }
}

/// Convenience: require View. Same semantics as [`require`] with
/// `needed = Permission::View`.
pub async fn require_view(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> RbacResult {
    require(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
        Permission::View,
    )
    .await
}

/// Convenience: require Edit. Owner-or-share-with-edit-or-admin.
pub async fn require_edit(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> RbacResult {
    require(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
        Permission::Edit,
    )
    .await
}

/// Convenience: require Admin on the resource. Only the owner or a
/// system admin (or someone with an explicit admin share) qualifies.
pub async fn require_admin_on(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> RbacResult {
    require(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
        Permission::Admin,
    )
    .await
}

/// Require that the caller is a system admin (server-wide, not
/// per-resource). Use this only for platform-admin endpoints; per-resource
/// admin should go through [`require_admin_on`].
pub fn require_platform_admin(principal: &AuthPrincipal) -> Result<(), (StatusCode, String)> {
    if !principal.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Platform-admin access required".into(),
        ));
    }
    Ok(())
}

fn map_auth_err(e: AuthError) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

// ─── Unit tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{User, UserRole};

    fn dev_user(user_id: &str) -> AuthPrincipal {
        AuthPrincipal::User(User {
            user_id: user_id.into(),
            email: format!("{}@test", user_id),
            display_name: Some(user_id.into()),
            role: UserRole::Developer,
            auth_provider: crate::types::AuthProvider::Email,
            github_username: None,
            google_id: None,
            ethereum_address: None,
            ens_name: None,
        })
    }

    fn admin_user() -> AuthPrincipal {
        AuthPrincipal::User(User {
            user_id: "admin-1".into(),
            email: "admin@test".into(),
            display_name: Some("Admin".into()),
            role: UserRole::Admin,
            auth_provider: crate::types::AuthProvider::Email,
            github_username: None,
            google_id: None,
            ethereum_address: None,
            ens_name: None,
        })
    }

    #[test]
    fn platform_admin_gate_allows_admin() {
        let principal = admin_user();
        assert!(require_platform_admin(&principal).is_ok());
    }

    #[test]
    fn platform_admin_gate_rejects_developer() {
        let principal = dev_user("alice");
        let err = require_platform_admin(&principal).unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
    }

    // Full require() tests require a live PgPool + object_shares
    // fixtures — those live in the integration suite (see tests/
    // in fermi/, not here). The unit surface is intentionally thin
    // because the interesting logic delegates to `can_access`, which
    // is exercised by the visibility tests.
}
