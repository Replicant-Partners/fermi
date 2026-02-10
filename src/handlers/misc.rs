//! Miscellaneous handlers — waitlist, health, debug.

use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
// ─── Waitlist ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WaitlistRequest {
    email: String,
    #[serde(default = "default_waitlist_source")]
    source: String,
}

pub fn default_waitlist_source() -> String {
    "landing".to_string()
}

pub async fn waitlist_handler(
    State(state): State<AppState>,
    Json(req): Json<WaitlistRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let email = req.email.trim().to_lowercase();
    if !email.contains('@') || email.len() < 5 {
        return Err((StatusCode::BAD_REQUEST, "Invalid email".to_string()));
    }

    sqlx::query(
        "INSERT INTO waitlist (email, source) VALUES ($1, $2) ON CONFLICT (email) DO NOTHING",
    )
    .bind(&email)
    .bind(&req.source)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e)))?;

    Ok(Json(
        json!({ "status": "ok", "message": "You're on the list!" }),
    ))
}

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "Agent Bestiary",
        "description": "A naturalist's catalogue of dreaming agents",
        "version": "1.0.0",
        "api_version": "v1",
        "embeddings": {
            "provider": state.embedder.provider_name(),
            "dimension": state.embedder.dimension(),
        }
    }))
}

pub async fn debug_startup(State(state): State<AppState>) -> Json<Value> {
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());
    let dir_exists = std::path::Path::new(&agents_dir).exists();
    let dir_is_dir = std::path::Path::new(&agents_dir).is_dir();
    let dir_entries: Vec<String> = std::fs::read_dir(&agents_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| format!("{:?}", e.path()))
                .collect()
        })
        .unwrap_or_default();

    let registry_count = state.registry.list_cards().map(|c| c.len()).unwrap_or(0);

    let db_agent_count = state
        .memory_store
        .list_agents()
        .await
        .map(|a| a.len())
        .unwrap_or(0);

    let db_non_test: Vec<String> = state
        .memory_store
        .list_agents()
        .await
        .map(|agents| {
            agents
                .iter()
                .filter(|a| !a.agent_name.starts_with("test_"))
                .map(|a| a.agent_name.clone())
                .collect()
        })
        .unwrap_or_default();

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    Json(json!({
        "cwd": cwd,
        "agents_dir": agents_dir,
        "dir_exists": dir_exists,
        "dir_is_dir": dir_is_dir,
        "dir_entries": dir_entries,
        "registry_agent_count": registry_count,
        "db_agent_count": db_agent_count,
        "db_non_test_agents": db_non_test,
    }))
}
