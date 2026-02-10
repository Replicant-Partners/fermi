use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::{
    MemberType, ObjectShare, ObjectType, Permission, ShareType, Team, TeamMember, TeamRole,
};

// ─── Team CRUD ─────────────────────────────────────────────────────

pub async fn create_team(
    pool: &PgPool,
    name: &str,
    slug: &str,
    description: Option<&str>,
    owner_id: &str,
) -> Result<Team, AuthError> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String)>(
        r#"
        INSERT INTO teams (name, slug, description, owner_id)
        VALUES ($1, $2, $3, $4)
        RETURNING id, name, slug, description, owner_id
        "#,
    )
    .bind(name)
    .bind(slug)
    .bind(description)
    .bind(owner_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    let team = Team {
        id: row.0,
        name: row.1,
        slug: row.2,
        description: row.3,
        owner_id: row.4,
    };

    // Auto-add owner as team member with 'owner' role
    add_team_member(
        pool,
        team.id,
        MemberType::User,
        owner_id,
        TeamRole::Owner,
        owner_id,
    )
    .await?;

    Ok(team)
}

pub async fn get_user_teams(pool: &PgPool, user_id: &str) -> Result<Vec<Team>, AuthError> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String)>(
        r#"
        SELECT t.id, t.name, t.slug, t.description, t.owner_id
        FROM teams t
        JOIN team_members tm ON t.id = tm.team_id
        WHERE tm.member_id = $1 AND tm.member_type = 'user'
        ORDER BY t.name
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| Team {
            id: r.0,
            name: r.1,
            slug: r.2,
            description: r.3,
            owner_id: r.4,
        })
        .collect())
}

pub async fn get_team(pool: &PgPool, team_id: Uuid) -> Result<Team, AuthError> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String)>(
        r#"
        SELECT id, name, slug, description, owner_id
        FROM teams WHERE id = $1
        "#,
    )
    .bind(team_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    .ok_or(AuthError::UserNotFound)?;

    Ok(Team {
        id: row.0,
        name: row.1,
        slug: row.2,
        description: row.3,
        owner_id: row.4,
    })
}

pub async fn delete_team(
    pool: &PgPool,
    team_id: Uuid,
    requester_id: &str,
) -> Result<(), AuthError> {
    let result = sqlx::query("DELETE FROM teams WHERE id = $1 AND owner_id = $2")
        .bind(team_id)
        .bind(requester_id)
        .execute(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AuthError::Forbidden(
            "Only team owner can delete".to_string(),
        ));
    }

    // Clean up membership and shares (no FK cascade)
    let _ = sqlx::query("DELETE FROM team_members WHERE team_id = $1")
        .bind(team_id)
        .execute(pool)
        .await;
    let _ =
        sqlx::query("DELETE FROM object_shares WHERE share_type = 'team' AND share_target = $1")
            .bind(team_id.to_string())
            .execute(pool)
            .await;

    Ok(())
}

// ─── Membership ────────────────────────────────────────────────────

pub async fn add_team_member(
    pool: &PgPool,
    team_id: Uuid,
    member_type: MemberType,
    member_id: &str,
    role: TeamRole,
    invited_by: &str,
) -> Result<(), AuthError> {
    sqlx::query(
        r#"
        INSERT INTO team_members (team_id, member_type, member_id, role, invited_by)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (team_id, member_id) DO UPDATE SET role = $4
        "#,
    )
    .bind(team_id)
    .bind(member_type.as_str())
    .bind(member_id)
    .bind(role.as_str())
    .bind(invited_by)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(())
}

pub async fn remove_team_member(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
) -> Result<(), AuthError> {
    sqlx::query(
        "DELETE FROM team_members WHERE team_id = $1 AND member_id = $2 AND role != 'owner'",
    )
    .bind(team_id)
    .bind(member_id)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(())
}

pub async fn get_team_members(pool: &PgPool, team_id: Uuid) -> Result<Vec<TeamMember>, AuthError> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>(
        r#"
        SELECT team_id, member_type, member_id, role, joined_at
        FROM team_members
        WHERE team_id = $1
        ORDER BY joined_at
        "#,
    )
    .bind(team_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| TeamMember {
            team_id: r.0,
            member_type: MemberType::from_str(&r.1),
            member_id: r.2,
            role: TeamRole::from_str(&r.3),
            joined_at: r.4,
        })
        .collect())
}

pub async fn update_member_role(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
    new_role: TeamRole,
) -> Result<(), AuthError> {
    if new_role == TeamRole::Owner {
        return Err(AuthError::Forbidden(
            "Cannot assign owner role directly".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE team_members SET role = $1 WHERE team_id = $2 AND member_id = $3 AND role != 'owner'",
    )
    .bind(new_role.as_str())
    .bind(team_id)
    .bind(member_id)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(())
}

/// Check if a user has a specific role (or higher) in a team
pub async fn get_member_role(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
) -> Result<Option<TeamRole>, AuthError> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT role FROM team_members WHERE team_id = $1 AND member_id = $2",
    )
    .bind(team_id)
    .bind(member_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(row.map(|r| TeamRole::from_str(&r.0)))
}

// ─── Object Sharing ────────────────────────────────────────────────

pub async fn share_object(
    pool: &PgPool,
    object_type: ObjectType,
    object_id: &str,
    share_type: ShareType,
    share_target: &str,
    permission: Permission,
    granted_by: &str,
) -> Result<ObjectShare, AuthError> {
    let row = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO object_shares (object_type, object_id, share_type, share_target, permission, granted_by)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (object_type, object_id, share_type, share_target)
            DO UPDATE SET permission = $5
        RETURNING id
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .bind(share_type.as_str())
    .bind(share_target)
    .bind(permission.as_str())
    .bind(granted_by)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(ObjectShare {
        id: row.0,
        object_type,
        object_id: object_id.to_string(),
        share_type,
        share_target: share_target.to_string(),
        permission,
        granted_by: granted_by.to_string(),
    })
}

pub async fn list_object_shares(
    pool: &PgPool,
    object_type: ObjectType,
    object_id: &str,
) -> Result<Vec<ObjectShare>, AuthError> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, String, String, String)>(
        r#"
        SELECT id, object_type, object_id, share_type, share_target, permission, granted_by
        FROM object_shares
        WHERE object_type = $1 AND object_id = $2
        ORDER BY created_at
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|r| ObjectShare {
            id: r.0,
            object_type: ObjectType::from_str(&r.1).unwrap_or(ObjectType::Agent),
            object_id: r.2,
            share_type: if r.3 == "team" {
                ShareType::Team
            } else {
                ShareType::User
            },
            share_target: r.4,
            permission: Permission::from_str(&r.5),
            granted_by: r.6,
        })
        .collect())
}

pub async fn revoke_share(pool: &PgPool, share_id: Uuid) -> Result<(), AuthError> {
    let result = sqlx::query("DELETE FROM object_shares WHERE id = $1")
        .bind(share_id)
        .execute(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AuthError::UserNotFound);
    }

    Ok(())
}
