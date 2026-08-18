use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::{
    MemberType, ObjectShare, ObjectType, Permission, ShareType, Team, TeamCapability, TeamMember,
    TeamRole,
};

// ─── Coordination strategist ───────────────────────────────────────

/// The agent assigned to coordinate a workspace unless something says otherwise.
///
/// `cohere_and_coordinate`'s own card opens: *"You are Cohere & Coordinate — the
/// default coordination strategist for every workspace on the Agent Bestiary
/// platform."* It was assigned to **1 workspace out of 249**.
pub const DEFAULT_COORDINATION_STRATEGIST: &str = "cohere_and_coordinate";

/// Assign the default coordination strategist to a workspace.
///
/// ## Why this exists
///
/// `teams.coordination_strategist_id` has been read in 40 places — the
/// composition-dreaming path, the Loop 4 accept path, and
/// `record_coordination_observation`'s authorisation gate — and written by
/// none. Nothing on the platform has ever assigned one.
///
/// The consequence is that Loop 3's coordination half and the whole of Loop 4
/// were unreachable by construction, not by defect. Both look up the workspace's
/// strategist and find NULL, so a correctly-implemented, correctly-gated
/// coordination tool refuses in 248 of 249 workspaces. The mechanisms were
/// built, tested, and pointed at a column nobody populated.
///
/// ## Never fails the caller
///
/// Returns the assigned id, or `None` when the strategist agent is not present
/// in this deployment. Workspace creation must not fail because a curated agent
/// is missing — a workspace with no strategist is degraded, one that could not
/// be created is broken.
pub async fn assign_default_strategist(pool: &PgPool, workspace_id: Uuid) -> Option<Uuid> {
    let strategist: Option<Uuid> =
        sqlx::query_scalar("SELECT agent_id FROM agents WHERE agent_name = $1")
            .bind(DEFAULT_COORDINATION_STRATEGIST)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let Some(agent_id) = strategist else {
        eprintln!(
            "WARN: default coordination strategist '{DEFAULT_COORDINATION_STRATEGIST}' not \
             found — workspace {workspace_id} created without one, so Loop 3 coordination \
             and Loop 4 composition are unavailable for it"
        );
        return None;
    };

    // Only when unset: an explicit assignment is more authoritative than this
    // default, and re-running must not clobber it.
    let res = sqlx::query(
        "UPDATE teams
            SET coordination_strategist_id = $2, strategist_assigned_at = NOW()
          WHERE id = $1 AND coordination_strategist_id IS NULL",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .execute(pool)
    .await;

    match res {
        Ok(_) => Some(agent_id),
        Err(e) => {
            eprintln!("WARN: could not assign coordination strategist to {workspace_id}: {e}");
            None
        }
    }
}

// ─── Team CRUD ─────────────────────────────────────────────────────

pub async fn create_team(
    pool: &PgPool,
    name: &str,
    slug: &str,
    description: Option<&str>,
    owner_id: &str,
    origin: &str,
) -> Result<Team, AuthError> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String, Option<String>)>(
        r#"
        INSERT INTO teams (name, slug, description, owner_id, origin)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, slug, description, owner_id, origin
        "#,
    )
    .bind(name)
    .bind(slug)
    .bind(description)
    .bind(owner_id)
    .bind(origin)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    let team = Team {
        id: row.0,
        name: row.1,
        slug: row.2,
        description: row.3,
        owner_id: row.4,
        origin: row.5,
    };

    // Every workspace gets a coordination strategist at creation. Without it
    // Loop 3's coordination half and Loop 4 are unreachable for that workspace
    // — see `assign_default_strategist`.
    assign_default_strategist(pool, team.id).await;

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
    let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String, Option<String>)>(
        r#"
        SELECT t.id, t.name, t.slug, t.description, t.owner_id, t.origin
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
            origin: r.5,
        })
        .collect())
}

pub async fn get_team(pool: &PgPool, team_id: Uuid) -> Result<Team, AuthError> {
    let row = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String, Option<String>)>(
        r#"
        SELECT id, name, slug, description, owner_id, origin
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
        origin: row.5,
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
    // Capabilities come from the role's defaults (Spec 30). Without this a
    // member added today would land with an empty set while one backfilled
    // by migration 179 has 'resolve' — making "who can resolve" depend on
    // when someone joined, which is indefensible.
    //
    // On CONFLICT we also refresh them, so promoting a member to admin
    // through this path grants the admin defaults rather than leaving them
    // with a member's powers under an admin's title.
    let caps: Vec<String> = role
        .default_capabilities()
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();

    sqlx::query(
        r#"
        INSERT INTO team_members (team_id, member_type, member_id, role, invited_by, capabilities)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (team_id, member_id) DO UPDATE
            SET role = $4,
                capabilities = (
                    SELECT ARRAY(SELECT DISTINCT unnest(team_members.capabilities || $6::text[]))
                )
        "#,
    )
    .bind(team_id)
    .bind(member_type.as_str())
    .bind(member_id)
    .bind(role.as_str())
    .bind(invited_by)
    .bind(&caps)
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

// ─── Capabilities (Spec 30) ──────────────────────────────────

/// Read one member's capability grants.
///
/// Unrecognised strings are dropped rather than erroring: the column has
/// no CHECK constraint (see migration 179 for why), so forward-compatible
/// reads are the contract. A newer node writing a capability this binary
/// doesn't know about must not break this one's access checks.
pub async fn get_member_capabilities(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
) -> Result<Vec<TeamCapability>, AuthError> {
    let row = sqlx::query_as::<_, (Vec<String>,)>(
        "SELECT capabilities FROM team_members WHERE team_id = $1 AND member_id = $2",
    )
    .bind(team_id)
    .bind(member_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(row
        .map(|r| {
            r.0.iter()
                .filter_map(|s| TeamCapability::from_str(s))
                .collect()
        })
        .unwrap_or_default())
}

/// Replace a member's capability set.
///
/// Whole-set replacement rather than add/remove verbs: the caller is a UI
/// rendering checkboxes, and a read-modify-write of individual grants
/// across two admins editing at once silently loses one of their changes.
///
/// Owners are exempt from downgrade — the same rule
/// [`update_member_role`] applies. A team that can strip its owner's
/// `resolve` can lock every terminal action out of the team.
pub async fn set_member_capabilities(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
    capabilities: &[TeamCapability],
) -> Result<(), AuthError> {
    let as_text: Vec<String> = capabilities
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();

    let result = sqlx::query(
        "UPDATE team_members SET capabilities = $1
          WHERE team_id = $2 AND member_id = $3 AND role != 'owner'",
    )
    .bind(&as_text)
    .bind(team_id)
    .bind(member_id)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if result.rows_affected() == 0 {
        // Either no such member, or the owner — the caller can't tell them
        // apart and shouldn't need to; both mean "that edit didn't apply".
        return Err(AuthError::Forbidden(
            "Member not found, or is the team owner (owners always hold every capability)"
                .to_string(),
        ));
    }
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

/// Grant access to an object.
///
/// ## Share targets are validated here, deliberately
///
/// `object_shares.share_target` is an unconstrained TEXT column — there is no
/// foreign key, because the column holds either a `users.user_id` or a
/// `teams.id` depending on `share_type`, and a single FK cannot express that.
///
/// Nothing validated it. The 2026-08-06 integrity audit found a share granted
/// to the user `'a'`, created that same morning: a target that does not exist
/// and never will, sitting permanently in the ACL ladder.
///
/// A share pointing at a non-existent principal is not merely inert. It is a
/// silent failure of intent — the granter believes they shared something, and
/// no error ever told them otherwise. Validating at this choke point is what
/// keeps SHARE-001/SHARE-002 at zero instead of accumulating drift that has
/// to be cleaned up later.
pub async fn share_object(
    pool: &PgPool,
    object_type: ObjectType,
    object_id: &str,
    share_type: ShareType,
    share_target: &str,
    permission: Permission,
    granted_by: &str,
) -> Result<ObjectShare, AuthError> {
    // ── Validate the target exists ────────────────────────────────
    let target = share_target.trim();
    if target.is_empty() {
        return Err(AuthError::InvalidInput(
            "share target cannot be empty".to_string(),
        ));
    }

    match share_type {
        ShareType::User => {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE user_id = $1)")
                    .bind(target)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
            if !exists {
                return Err(AuthError::InvalidInput(format!(
                    "cannot share with '{}': no such user",
                    target
                )));
            }
        }
        ShareType::Team => {
            // teams.id is a UUID; a non-UUID target can never match, so reject
            // it as input rather than letting the comparison silently fail.
            let team_id = Uuid::parse_str(target).map_err(|_| {
                AuthError::InvalidInput(format!(
                    "cannot share with team '{}': not a team id",
                    target
                ))
            })?;
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM teams WHERE id = $1)")
                    .bind(team_id)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
            if !exists {
                return Err(AuthError::InvalidInput(format!(
                    "cannot share with team '{}': no such team",
                    target
                )));
            }
        }
    }

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
    .bind(target)
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
        share_target: target.to_string(),
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
