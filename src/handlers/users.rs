//! User discovery handlers — public profiles, search, collaborators.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

// ─── Public profile page ───────────────────────────────────────────

pub async fn user_profile_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/user_profile.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/user_profile.html: {}", e);
            format!("<h1>User Profile</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

// ─── Public profile API ────────────────────────────────────────────

pub async fn get_public_profile_handler(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_row = sqlx::query(
        "SELECT user_id, display_name, avatar_url, bio, created_at
         FROM users WHERE user_id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "User not found".to_string()))?;

    // Stats.
    //
    // Runs come from `agent_execution_rollup` (derived from `episodes`),
    // not from `agents.total_executions` — nothing writes that column, so
    // every public profile advertised 0 executions regardless of how much
    // the owner's agents had actually run. See migrations/192 and
    // src/rollup_trust.rs.
    //
    // LEFT JOIN + COALESCE: agents with no episodes are absent from the
    // view, and they must still be counted in `agent_count`. `SUM` over
    // bigint returns NUMERIC, so cast to ::bigint for the i64 read below.
    let stats_row = sqlx::query(
        "SELECT COUNT(*) as agent_count,
                COALESCE(SUM(COALESCE(r.executions, 0)), 0)::bigint as total_executions
         FROM agents a
         LEFT JOIN agent_execution_rollup r ON r.agent_id = a.agent_id
         WHERE a.user_id = $1 AND a.visibility = 'public'",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Public agents, ranked by measured runs from the rollup view. The
    // previous ORDER BY read `agents.total_executions`, which is never
    // written, so this list was ordered by a constant zero. See
    // migrations/192 and src/rollup_trust.rs.
    let agent_rows = sqlx::query(
        "SELECT a.agent_name, a.display_alias, a.agent_type, a.description,
                COALESCE(r.executions, 0) as total_executions
         FROM agents a
         LEFT JOIN agent_execution_rollup r ON r.agent_id = a.agent_id
         WHERE a.user_id = $1 AND a.visibility = 'public'
         ORDER BY COALESCE(r.executions, 0) DESC LIMIT 20",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agents: Vec<Value> = agent_rows
        .iter()
        .map(|r| {
            json!({
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
                "agent_type": r.try_get::<String, _>("agent_type").unwrap_or_default(),
                "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
                // The view's `executions` is bigint; reading it as i32
                // would fail to decode and fall through to 0, reproducing
                // the zero-runs bug this query was rewritten to fix.
                "total_executions": r.try_get::<i64, _>("total_executions").unwrap_or(0),
            })
        })
        .collect();

    // Rabble stats (creatures, flights, rabbles)
    let rabble_stats = sqlx::query(
        "SELECT
            (SELECT COUNT(*) FROM creatures WHERE owner_id = $1 AND status = 'active') as creature_count,
            (SELECT COUNT(*) FROM creature_flights WHERE owner_id = $1) as flight_count,
            (SELECT COUNT(DISTINCT swarm_id) FROM creature_flights WHERE owner_id = $1 AND swarm_id IS NOT NULL) as rabble_count",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await;

    let (creature_count, flight_count, rabble_count) = match rabble_stats {
        Ok(row) => (
            row.try_get::<i64, _>("creature_count").unwrap_or(0),
            row.try_get::<i64, _>("flight_count").unwrap_or(0),
            row.try_get::<i64, _>("rabble_count").unwrap_or(0),
        ),
        Err(_) => (0, 0, 0),
    };

    Ok(Json(json!({
        "user_id": user_row.try_get::<String, _>("user_id").unwrap_or_default(),
        "display_name": user_row.try_get::<Option<String>, _>("display_name").unwrap_or(None),
        "avatar_url": user_row.try_get::<Option<String>, _>("avatar_url").unwrap_or(None),
        "bio": user_row.try_get::<Option<String>, _>("bio").unwrap_or(None),
        "created_at": user_row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
        "stats": {
            "public_agents": stats_row.try_get::<i64, _>("agent_count").unwrap_or(0),
            "total_executions": stats_row.try_get::<i64, _>("total_executions").unwrap_or(0),
            "creatures": creature_count,
            "flights": flight_count,
            "rabbles": rabble_count,
        },
        "public_agents": agents,
    })))
}

// ─── User search ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    q: Option<String>,
    limit: Option<i64>,
}

pub async fn search_users_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Query(params): Query<SearchParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let term = params.q.unwrap_or_default().trim().to_string();
    if term.len() < 2 {
        return Ok(Json(json!({ "users": [] })));
    }

    let limit = params.limit.unwrap_or(20).min(50);
    let pattern = format!("%{}%", term);

    // Search across multiple fields: display_name, email, user_id, github_username,
    // google_id, AND creature specimen_names (so you can find someone by their creature)
    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.avatar_url, u.bio,
                u.github_username, u.email,
                (SELECT COUNT(*) FROM agents a WHERE a.user_id = u.user_id AND a.visibility = 'public') as public_agent_count,
                (SELECT COUNT(*) FROM creatures c WHERE c.owner_id = u.user_id AND c.status = 'active') as creature_count,
                (SELECT string_agg(c.specimen_name, ', ' ORDER BY c.created_at DESC)
                 FROM creatures c WHERE c.owner_id = u.user_id AND c.specimen_name IS NOT NULL
                 LIMIT 1) as creature_names
         FROM users u
         LEFT JOIN creatures c2 ON c2.owner_id = u.user_id AND c2.specimen_name ILIKE $1
         WHERE u.display_name ILIKE $1
            OR u.email ILIKE $1
            OR u.user_id ILIKE $1
            OR u.github_username ILIKE $1
            OR u.google_id ILIKE $1
            OR c2.specimen_name ILIKE $1
         GROUP BY u.user_id, u.display_name, u.avatar_url, u.bio,
                  u.github_username, u.email
         ORDER BY
            MIN(CASE WHEN u.display_name ILIKE $1 THEN 0
                     WHEN u.github_username ILIKE $1 THEN 1
                     WHEN c2.specimen_name ILIKE $1 THEN 2
                     ELSE 3 END),
            u.display_name
         LIMIT $2",
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        eprintln!("[user_search] Query failed for '{}': {}", term, e);
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Search failed: {}", e))
    })?;

    let users: Vec<Value> = rows
        .iter()
        .map(|r| {
            let bio: Option<String> = r.try_get("bio").unwrap_or(None);
            let bio_snippet = bio.as_deref().map(|b| {
                if b.len() > 100 {
                    format!("{}...", &b[..100])
                } else {
                    b.to_string()
                }
            });
            let display = r
                .try_get::<Option<String>, _>("display_name")
                .unwrap_or(None);
            let github = r
                .try_get::<Option<String>, _>("github_username")
                .unwrap_or(None);
            let email: Option<String> = r.try_get::<Option<String>, _>("email").unwrap_or(None);
            // Show the best available name: display_name > github_username > email prefix > user_id
            let shown_name = display
                .clone()
                .or_else(|| github.clone().map(|g| format!("@{}", g)))
                .or_else(|| {
                    email
                        .as_ref()
                        .map(|e| e.split('@').next().unwrap_or("user").to_string())
                })
                .unwrap_or_else(|| r.try_get::<String, _>("user_id").unwrap_or_default());
            json!({
                "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
                "display_name": shown_name,
                "avatar_url": r.try_get::<Option<String>, _>("avatar_url").unwrap_or(None),
                "bio_snippet": bio_snippet,
                "github_username": github,
                "public_agent_count": r.try_get::<i64, _>("public_agent_count").unwrap_or(0),
                "creature_count": r.try_get::<i64, _>("creature_count").unwrap_or(0),
                "creature_names": r.try_get::<Option<String>, _>("creature_names").unwrap_or(None),
            })
        })
        .collect();

    Ok(Json(json!({ "users": users })))
}

// ─── Collaborators (people who share workspaces) ───────────────────

pub async fn get_collaborators_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.avatar_url, u.bio,
                COUNT(DISTINCT tm2.team_id) as shared_workspaces
         FROM team_members tm1
         JOIN team_members tm2 ON tm2.team_id = tm1.team_id AND tm2.member_id != tm1.member_id
         JOIN users u ON u.user_id = tm2.member_id
         WHERE tm1.member_id = $1 AND tm2.member_type = 'user'
         GROUP BY u.user_id, u.display_name, u.avatar_url, u.bio
         ORDER BY shared_workspaces DESC, u.display_name
         LIMIT 12",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let collaborators: Vec<Value> = rows
        .iter()
        .map(|r| {
            let bio: Option<String> = r.try_get("bio").unwrap_or(None);
            let bio_snippet = bio.as_deref().map(|b| {
                if b.len() > 100 {
                    format!("{}...", &b[..100])
                } else {
                    b.to_string()
                }
            });
            json!({
                "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
                "display_name": r.try_get::<Option<String>, _>("display_name").unwrap_or(None),
                "avatar_url": r.try_get::<Option<String>, _>("avatar_url").unwrap_or(None),
                "bio_snippet": bio_snippet,
                "shared_workspaces": r.try_get::<i64, _>("shared_workspaces").unwrap_or(0),
            })
        })
        .collect();

    Ok(Json(json!({ "collaborators": collaborators })))
}

// ─── Exact email lookup (Spec 24 §3.3) ─────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LookupParams {
    email: Option<String>,
}

/// GET /api/users/lookup?email=alice@example.com
///
/// Spec 24 §3.3: the share UI calls this BEFORE deciding "instant share
/// vs email invite." Exact case-insensitive email match; returns one
/// user or 404. Does not enumerate, does not fuzzy-search — the
/// existing `/api/users/search` covers fuzzy.
///
/// Returns the user's `user_id` (text) — the value that
/// `object_shares.share_target` accepts directly when share_type='user'.
/// Authenticated callers only (`auth_middleware`).
pub async fn lookup_user_by_email_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Query(params): Query<LookupParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let email = params
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "missing email query parameter".to_string(),
        ))?
        .to_lowercase();

    // Strict equality, case-insensitive. We don't ILIKE — that opens an
    // enumeration vector through trailing-wildcard-style probes.
    let row = sqlx::query(
        "SELECT user_id, display_name, avatar_url
         FROM users
         WHERE LOWER(email) = $1
           AND user_id IS NOT NULL
         LIMIT 1",
    )
    .bind(&email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match row {
        None => Err((StatusCode::NOT_FOUND, "no user with that email".into())),
        Some(r) => Ok(Json(json!({
            "user_id":      r.try_get::<String, _>("user_id").unwrap_or_default(),
            "display_name": r.try_get::<Option<String>, _>("display_name").ok().flatten(),
            "avatar_url":   r.try_get::<Option<String>, _>("avatar_url").ok().flatten(),
        }))),
    }
}
