use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::{AuthPrincipal, ObjectType, Permission, Visibility};

/// Result of an access check — either denied or a specific permission level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    Denied,
    Granted(Permission),
}

impl AccessLevel {
    pub fn is_denied(&self) -> bool {
        matches!(self, AccessLevel::Denied)
    }

    pub fn has_view(&self) -> bool {
        matches!(self, AccessLevel::Granted(_))
    }

    pub fn has_edit(&self) -> bool {
        matches!(
            self,
            AccessLevel::Granted(Permission::Edit) | AccessLevel::Granted(Permission::Admin)
        )
    }

    pub fn has_admin(&self) -> bool {
        matches!(self, AccessLevel::Granted(Permission::Admin))
    }
}

/// Check what permission level a principal has on an object.
///
/// Priority chain:
/// 1. System admins → Admin
/// 2. Owner → Admin
/// 3. Public visibility → View
/// 4. Direct user share in object_shares → share's permission
/// 5. Team share (via team_members membership) → share's permission
/// 6. Deny
pub async fn can_access(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<AccessLevel, AuthError> {
    let user_id = principal.user_id();

    // 1. System admins → Admin
    if principal.can_admin() {
        return Ok(AccessLevel::Granted(Permission::Admin));
    }

    // 2. Owner → Admin
    if user_id == owner_id {
        return Ok(AccessLevel::Granted(Permission::Admin));
    }

    // 3. Public visibility → View
    if visibility == Visibility::Public {
        return Ok(AccessLevel::Granted(Permission::View));
    }

    // 4. Direct user share
    let direct = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT permission FROM object_shares
        WHERE object_type = $1 AND object_id = $2
          AND share_type = 'user' AND share_target = $3
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if let Some(row) = direct {
        return Ok(AccessLevel::Granted(Permission::from_str(&row.0)));
    }

    // 5. Team share — find highest permission across all teams the user belongs to
    let team_perm = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT os.permission
        FROM object_shares os
        JOIN team_members tm ON os.share_target = tm.team_id::text
        WHERE os.object_type = $1 AND os.object_id = $2
          AND os.share_type = 'team'
          AND tm.member_id = $3
        ORDER BY CASE os.permission
            WHEN 'admin' THEN 3
            WHEN 'edit'  THEN 2
            WHEN 'view'  THEN 1
        END DESC
        LIMIT 1
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if let Some(row) = team_perm {
        return Ok(AccessLevel::Granted(Permission::from_str(&row.0)));
    }

    // 6. Deny
    Ok(AccessLevel::Denied)
}

/// Convenience: can the principal view this object?
pub async fn can_view(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<bool, AuthError> {
    let level = can_access(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
    )
    .await?;
    Ok(level.has_view())
}

/// Convenience: can the principal edit this object?
pub async fn can_edit(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<bool, AuthError> {
    let level = can_access(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
    )
    .await?;
    Ok(level.has_edit())
}

/// Check access for unauthenticated users — only public objects are visible.
pub fn can_access_anonymous(visibility: Visibility) -> AccessLevel {
    if visibility == Visibility::Public {
        AccessLevel::Granted(Permission::View)
    } else {
        AccessLevel::Denied
    }
}

/// Check if a principal is a member of a specific team (any role).
pub async fn is_team_member(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
) -> Result<bool, AuthError> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND member_id = $2",
    )
    .bind(team_id)
    .bind(member_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(row.0 > 0)
}
