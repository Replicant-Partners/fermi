use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    db: PgPool,
}

#[tokio::main]
async fn main() {
    // Get database URL from environment
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    println!("Database URL: {}", database_url.chars().take(30).collect::<String>() + "...");

    // Create database connection pool
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    println!("Connected to database successfully");

    let state = AppState { db };

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/agents", get(list_agents))
        .with_state(state);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "agent-bestiary",
        "description": "Active Dreaming Memory backend for AI agents",
        "version": "1.0.0",
        "api_version": "v1"
    }))
}

async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    // Query agents from database - using SELECT * to get all columns
    let result = sqlx::query(
        r#"
        SELECT *
        FROM agents
        ORDER BY created_at DESC
        LIMIT 100
        "#
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let agents: Vec<Value> = rows
                .iter()
                .map(|row| {
                    // Get column names and values dynamically
                    let mut agent = serde_json::Map::new();

                    // Try common column names
                    if let Ok(val) = row.try_get::<String, _>("agent_id") {
                        agent.insert("agent_id".to_string(), json!(val));
                    }
                    if let Ok(val) = row.try_get::<String, _>("agent_name") {
                        agent.insert("agent_name".to_string(), json!(val));
                    }
                    if let Ok(val) = row.try_get::<chrono::NaiveDateTime, _>("created_at") {
                        agent.insert("created_at".to_string(), json!(val));
                    }
                    if let Ok(val) = row.try_get::<chrono::NaiveDateTime, _>("updated_at") {
                        agent.insert("updated_at".to_string(), json!(val));
                    }

                    json!(agent)
                })
                .collect();

            Json(json!({
                "agents": agents,
                "total": agents.len()
            }))
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            Json(json!({
                "agents": [],
                "total": 0,
                "error": format!("Database error: {}", e)
            }))
        }
    }
}
