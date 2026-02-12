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

    // Stats
    let stats_row = sqlx::query(
        "SELECT COUNT(*) as agent_count,
                COALESCE(SUM(total_executions), 0) as total_executions
         FROM agents WHERE user_id = $1 AND visibility = 'public'",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Public agents
    let agent_rows = sqlx::query(
        "SELECT agent_name, display_alias, agent_type, description, total_executions
         FROM agents WHERE user_id = $1 AND visibility = 'public'
         ORDER BY total_executions DESC LIMIT 20",
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
                "total_executions": r.try_get::<i32, _>("total_executions").unwrap_or(0),
            })
        })
        .collect();

    // Rabble stats (creatures, flights, rabbles)
    let rabble_stats = sqlx::query(
        "SELECT
            (SELECT COUNT(*) FROM creatures WHERE owner_id = $1 AND status = 'active') as creature_count,
            (SELECT COUNT(*) FROM flights WHERE owner_id = $1) as flight_count,
            (SELECT COUNT(DISTINCT s.swarm_id) FROM swarm_participants sp
             JOIN swarm_events s ON s.swarm_id = sp.swarm_id
             WHERE sp.user_id = $1) as rabble_count",
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
    let pattern = format!("{}%", term);

    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.avatar_url, u.bio,
                COUNT(a.agent_id) FILTER (WHERE a.visibility = 'public') as public_agent_count
         FROM users u
         LEFT JOIN agents a ON a.user_id = u.user_id
         WHERE u.display_name ILIKE $1 OR u.email ILIKE $1
         GROUP BY u.user_id
         ORDER BY u.display_name
         LIMIT $2",
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

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
            json!({
                "user_id": r.try_get::<String, _>("user_id").unwrap_or_default(),
                "display_name": r.try_get::<Option<String>, _>("display_name").unwrap_or(None),
                "avatar_url": r.try_get::<Option<String>, _>("avatar_url").unwrap_or(None),
                "bio_snippet": bio_snippet,
                "public_agent_count": r.try_get::<i64, _>("public_agent_count").unwrap_or(0),
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
