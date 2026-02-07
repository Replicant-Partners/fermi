use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    gemini_api_key: String,
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
struct GeminiGenerationConfig {
    #[serde(rename = "responseModalities")]
    response_modalities: Vec<String>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Deserialize)]
struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
struct GeminiPartResponse {
    #[serde(rename = "inlineData")]
    inline_data: Option<GeminiInlineData>,
}

#[derive(Deserialize)]
struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

#[tokio::main]
async fn main() {
    // Get database URL from environment
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    println!(
        "Database URL: {}",
        database_url.chars().take(30).collect::<String>() + "..."
    );

    // Create database connection pool
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    println!("Connected to database successfully");

    let gemini_api_key = std::env::var("GEMINI_API_KEY")
        .unwrap_or_else(|_| "AIzaSyDgzwrmWLFjPqqrOipsV5ge_2Ad4Ns7iXw".to_string());

    let state = AppState { db, gemini_api_key };

    let app = Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/:agent_id/avatar", get(generate_avatar))
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

async fn index() -> Html<String> {
    let html = std::fs::read_to_string("templates/index.html")
        .unwrap_or_else(|_| "<h1>Error loading page</h1>".to_string());
    Html(html)
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
        "#,
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

async fn generate_avatar(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Use agent_id to deterministically select beast and scene
    let beasts = [
        "fox", "crane", "tiger", "dragon", "owl", "wolf", "bear", "phoenix",
    ];
    let scenes = [
        "misty mountain",
        "moonlit lake",
        "bamboo forest",
        "snowy peak",
        "tranquil garden",
        "coastal cliff",
        "autumn valley",
        "starlit temple",
    ];

    let beast_idx = agent_id.bytes().sum::<u8>() as usize % beasts.len();
    let scene_idx = (agent_id.bytes().map(|b| b as usize).sum::<usize>() / 7) % scenes.len();

    let beast = beasts[beast_idx];
    let scene = scenes[scene_idx];

    let prompt = format!(
        "A {} in {} in the style of Hasui Kawase. Japanese woodblock print aesthetic, \
        serene composition, soft color palette, atmospheric depth, elegant simplicity.",
        beast, scene
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            parts: vec![GeminiPart { text: prompt }],
        }],
        generation_config: GeminiGenerationConfig {
            response_modalities: vec!["IMAGE".to_string()],
        },
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent")
        .header("x-goog-api-key", &state.gemini_api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to call Gemini API: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Gemini API error {}: {}", status, error_text),
        ));
    }

    let gemini_response: GeminiResponse = response.json().await.map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse Gemini response: {}", e),
        )
    })?;

    // Extract the base64 image from the response
    if let Some(candidate) = gemini_response.candidates.first() {
        for part in &candidate.content.parts {
            if let Some(inline_data) = &part.inline_data {
                return Ok(Json(json!({
                    "agent_id": agent_id,
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data
                    }
                })));
            }
        }
    }

    Err((
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "No image generated".to_string(),
    ))
}
