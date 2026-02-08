use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    middleware,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use fermi::agent_backend::{
    agent_card::AgentCard,
    executor::{AgentOutput, AgentStatus, ExecutionContext},
    llm_executor::LLMExecutor,
    registry::AgentRegistry,
};
use fermi::ast;
use fermi_auth::{
    api_keys, auth_middleware, build_github_auth_url, build_google_auth_url, create_session_token,
    generate_state, github_exchange_code, github_fetch_user_info, google_exchange_code,
    google_fetch_user_info, sync_user, teams, AuthPrincipal, AuthState, MemberType, OAuthConfig,
    ObjectType, Permission, ShareType, TeamRole,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;
use std::sync::Arc;

use agent_bestiary_memory::{Agent, Episode, ExecutionStatus, MemoryStore};
use agent_bestiary_projector::{ProjectionCache, ProjectionEngine, ProjectionMethod};

#[derive(Clone)]
struct AppState {
    db: PgPool,
    memory_store: Arc<MemoryStore>,
    registry: Arc<AgentRegistry>,
    projection_engine: Arc<ProjectionEngine>,
    projection_cache: Arc<ProjectionCache>,
    gemini_api_key: String,
    jwt_secret: String,
    oauth: OAuthConfig,
}

// Implement From<AppState> for AuthState so middleware can extract it
impl From<AppState> for AuthState {
    fn from(s: AppState) -> Self {
        AuthState {
            jwt_secret: s.jwt_secret,
            db: s.db,
        }
    }
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

/// Run SQL migration files on startup (idempotent — uses IF NOT EXISTS).
async fn run_migrations(db: &PgPool) {
    let migration_files = [
        "migrations/009_add_teams_and_sharing.sql",
        "migrations/010_add_adm_tables_and_dreaming.sql",
    ];

    for file in &migration_files {
        match std::fs::read_to_string(file) {
            Ok(sql) => {
                println!("Running migration: {}", file);
                match sqlx::raw_sql(&sql).execute(db).await {
                    Ok(_) => println!("Migration {} completed", file),
                    Err(e) => {
                        // Don't panic — tables may already exist
                        eprintln!("Migration {} warning: {}", file, e);
                    }
                }
            }
            Err(e) => eprintln!("Could not read migration {}: {}", file, e),
        }
    }
}

#[tokio::main]
async fn main() {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    println!(
        "Database URL: {}",
        database_url.chars().take(30).collect::<String>() + "..."
    );

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    println!("Connected to database successfully");

    // Run pending migrations on startup
    run_migrations(&db).await;

    // Initialize ADM memory store
    let memory_store = Arc::new(
        MemoryStore::new(&database_url)
            .await
            .expect("Failed to initialize MemoryStore"),
    );
    println!("ADM MemoryStore initialized");

    // Initialize agent registry with LLM executor
    let registry = if let Ok(llm_executor) = LLMExecutor::from_env() {
        println!("Using LLM Executor (Claude API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(llm_executor)))
    } else {
        println!("No ANTHROPIC_API_KEY found, using Mock Executor");
        Arc::new(AgentRegistry::new())
    };

    // Load agents from filesystem into registry
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());
    println!("Loading agents from directory: {}", agents_dir);
    println!(
        "Directory exists: {}, is_dir: {}",
        std::path::Path::new(&agents_dir).exists(),
        std::path::Path::new(&agents_dir).is_dir()
    );
    if let Ok(entries) = std::fs::read_dir(&agents_dir) {
        for entry in entries {
            if let Ok(e) = entry {
                println!("  Found entry: {:?}", e.path());
            }
        }
    }
    match registry.load_from_directory(&agents_dir) {
        Ok(count) => println!("Loaded {} agent(s) from {}", count, agents_dir),
        Err(e) => eprintln!("ERROR: failed to load agents from {}: {}", agents_dir, e),
    }

    // Seed filesystem agents into database (idempotent)
    println!("Seeding agents to database...");
    seed_agents_to_database(&memory_store, &registry).await;
    println!("Agent seeding complete");

    // Initialize projection engine + cache
    let projection_engine = Arc::new(ProjectionEngine::new(memory_store.clone()));
    let projection_cache = Arc::new(ProjectionCache::new(300)); // 5 min TTL
    println!("Projection engine initialized");

    let gemini_api_key = std::env::var("GEMINI_API_KEY").unwrap_or_else(|_| {
        eprintln!("Note: GEMINI_API_KEY not set. Avatar generation will be disabled.");
        String::new()
    });

    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        eprintln!(
            "WARNING: JWT_SECRET not set, using insecure default. Set JWT_SECRET in production!"
        );
        "insecure-dev-secret-change-me-in-production".to_string()
    });

    let oauth = OAuthConfig::from_env();
    if oauth.google.is_none() {
        eprintln!("Note: Google OAuth not configured (GOOGLE_CLIENT_ID/SECRET missing)");
    }
    if oauth.github.is_none() {
        eprintln!("Note: GitHub OAuth not configured (GITHUB_CLIENT_ID/SECRET missing)");
    }

    let state = AppState {
        db: db.clone(),
        memory_store,
        registry,
        projection_engine,
        projection_cache,
        gemini_api_key,
        jwt_secret: jwt_secret.clone(),
        oauth,
    };

    let auth_state = AuthState { jwt_secret, db };

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/", get(index))
        .route("/agent/:agent_id", get(agent_detail))
        .route("/agent/:agent_id/ontology", get(ontology_view))
        .route("/api/health", get(health))
        .route("/api/debug/startup", get(debug_startup))
        .route("/api/agents", get(list_agents))
        .route("/api/agents/:agent_id/avatar", get(generate_avatar))
        .route("/api/agents/:agent_id/ontology", get(get_ontology))
        // Ontology evolution (public, read-only)
        .route(
            "/api/agents/:agent_id/ontology/history",
            get(get_ontology_history),
        )
        .route(
            "/api/agents/:agent_id/ontology/snapshots/:snapshot_id",
            get(get_ontology_snapshot),
        )
        .route(
            "/api/agents/:agent_id/ontology/diff",
            get(get_ontology_diff),
        )
        // Projector
        .route("/projector", get(projector_view))
        .route("/agent/:agent_id/projector", get(projector_view))
        .route(
            "/api/agents/:agent_id/projections",
            get(get_agent_projections),
        )
        .route("/api/projections/bestiary", get(get_bestiary_projections))
        .route(
            "/api/agents/:agent_id/projections/temporal",
            get(get_temporal_projections),
        )
        // Auth flow routes
        .route("/auth/google", get(auth_google))
        .route("/auth/github", get(auth_github))
        .route("/auth/callback", get(auth_callback))
        .route("/auth/logout", post(auth_logout));

    // Protected routes (require auth)
    let protected_routes = Router::new()
        .route("/api/auth/me", get(auth_me))
        .route("/api/auth/api-keys", get(list_api_keys))
        .route("/api/auth/api-keys", post(create_api_key))
        .route("/api/auth/api-keys/:key_id", delete(revoke_api_key))
        // Team routes
        .route("/api/teams", post(create_team_handler))
        .route("/api/teams", get(list_teams_handler))
        .route("/api/teams/:team_id", get(get_team_handler))
        .route("/api/teams/:team_id", delete(delete_team_handler))
        .route("/api/teams/:team_id/members", post(add_member_handler))
        .route("/api/teams/:team_id/members", get(list_members_handler))
        .route(
            "/api/teams/:team_id/members/:member_id",
            delete(remove_member_handler),
        )
        .route(
            "/api/teams/:team_id/members/:member_id",
            put(update_member_role_handler),
        )
        // Agent execution
        .route("/api/agents/:agent_id/execute", post(execute_agent_handler))
        // Dreaming budget
        .route(
            "/api/agents/:agent_id/dreaming/budget",
            get(get_dreaming_budget),
        )
        .route(
            "/api/agents/:agent_id/dreaming/budget",
            put(set_dreaming_budget),
        )
        // Sharing routes
        .route("/api/shares", post(share_object_handler))
        .route("/api/shares/:share_id", delete(revoke_share_handler))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
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

// ─── Page routes ───────────────────────────────────────────────────

async fn index() -> Html<String> {
    println!("Index route called");
    let html = match std::fs::read_to_string("templates/index.html") {
        Ok(content) => {
            println!(
                "Successfully loaded templates/index.html ({} bytes)",
                content.len()
            );
            content
        }
        Err(e) => {
            eprintln!("Error loading templates/index.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

async fn agent_detail() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_detail.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_detail.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

async fn ontology_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/ontology.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/ontology.html: {}", e);
            format!(
                "<h1>Knowledge Graph</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

// ─── API routes ────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "Agent Bestiary",
        "description": "A naturalist's catalogue of dreaming agents",
        "version": "1.0.0",
        "api_version": "v1"
    }))
}

async fn debug_startup(State(state): State<AppState>) -> Json<Value> {
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

async fn list_agents(State(state): State<AppState>) -> Json<Value> {
    // Primary: database (filter out test agents)
    if let Ok(db_agents) = state.memory_store.list_agents().await {
        let real_agents: Vec<_> = db_agents
            .into_iter()
            .filter(|a| !a.agent_name.starts_with("test_agent_"))
            .collect();
        if !real_agents.is_empty() {
            let agents: Vec<Value> = real_agents
                .iter()
                .map(|a| {
                    // Merge filesystem card data if available
                    let card = state.registry.get(&a.agent_name).ok();
                    let card_json = card.as_ref().and_then(|c| {
                        let path = format!("agents/curated/{}/agent_card.json", a.agent_name);
                        std::fs::read_to_string(&path).ok()
                            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    });

                    let mut agent_val = json!({
                        "agent_id": a.agent_name,
                        "agent_type": a.agent_type,
                        "version": a.version,
                        "tier": a.tier,
                        "description": a.description,
                        "author": a.author,
                        "model": a.model,
                        "capabilities": {
                            "executor": a.executor_type,
                            "model": a.model,
                            "temperature": a.temperature,
                            "mcp_tools": card.as_ref().map(|c| c.capabilities.mcp_tools.iter().map(|t| json!({"name": t.name, "description": t.description})).collect::<Vec<_>>()).unwrap_or_default(),
                            "skills": card.as_ref().map(|c| c.capabilities.skills.clone()).unwrap_or_default(),
                        },
                        "ontology_stats": {
                            "last_updated": a.last_consolidated_at,
                            "current_commit": a.current_ontology_commit,
                        },
                        "dreaming": {
                            "budget_credits": a.dreaming_budget_credits,
                            "credits_used": a.dreaming_credits_used,
                            "credits_remaining": a.dreaming_budget_credits - a.dreaming_credits_used,
                        },
                        "source": "database",
                    });

                    // Overlay rich fields from filesystem card
                    if let Some(cj) = &card_json {
                        if let Some(obj) = agent_val.as_object_mut() {
                            // Metadata (tags, created date)
                            if let Some(meta) = cj.get("metadata") {
                                obj.insert("metadata".to_string(), meta.clone());
                            }
                            // Performance stats
                            if let Some(perf) = cj.get("performance") {
                                obj.insert("performance".to_string(), perf.clone());
                            }
                            // Usage stats
                            if let Some(usage) = cj.get("usage") {
                                obj.insert("usage".to_string(), usage.clone());
                            }
                            // Wallet
                            if let Some(wallet) = cj.get("wallet") {
                                obj.insert("wallet".to_string(), wallet.clone());
                            }
                            // Ontology stats from card (entities/relationships counts)
                            if let Some(onto) = cj.get("ontology_stats") {
                                let mut merged = obj.get("ontology_stats").cloned().unwrap_or(json!({}));
                                if let (Some(m), Some(o)) = (merged.as_object_mut(), onto.as_object()) {
                                    for (k, v) in o {
                                        if m.get(k).map(|existing| existing.is_null()).unwrap_or(true) {
                                            m.insert(k.clone(), v.clone());
                                        }
                                    }
                                }
                                obj.insert("ontology_stats".to_string(), merged);
                            }
                        }
                    }

                    agent_val
                })
                .collect();
            return Json(json!({ "agents": agents, "total": agents.len() }));
        }
    }

    // Fallback: filesystem
    let agents_dir = "agents/curated";
    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let card_path = path.join("agent_card.json");
                if card_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&card_path) {
                        if let Ok(card) = serde_json::from_str::<Value>(&content) {
                            agents.push(card);
                        }
                    }
                }
            }
        }
    }
    Json(json!({ "agents": agents, "total": agents.len() }))
}

async fn generate_avatar(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if state.gemini_api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Avatar generation disabled (GEMINI_API_KEY not set)".to_string(),
        ));
    }

    let cache_dir = "avatars_cache";
    std::fs::create_dir_all(cache_dir).ok();
    let cache_path = format!("{}/{}.json", cache_dir, agent_id);

    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached_data) = serde_json::from_str::<Value>(&cached) {
            println!("Using cached avatar for {}", agent_id);
            return Ok(Json(cached_data));
        }
    }

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
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to call Gemini API: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Gemini API error {}: {}", status, error_text),
        ));
    }

    let gemini_response: GeminiResponse = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse Gemini response: {}", e),
        )
    })?;

    if let Some(candidate) = gemini_response.candidates.first() {
        for part in &candidate.content.parts {
            if let Some(inline_data) = &part.inline_data {
                let result = json!({
                    "agent_id": agent_id,
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data
                    }
                });

                std::fs::write(&cache_path, serde_json::to_string(&result).unwrap()).ok();
                println!("Cached new avatar for {}", agent_id);

                return Ok(Json(result));
            }
        }
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        "No image generated".to_string(),
    ))
}

async fn get_ontology(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Primary: latest database snapshot
    if let Ok(db_agent) = resolve_agent(&state, &agent_id).await {
        let row = sqlx::query(
            r#"
            SELECT snapshot_id, version, git_commit_sha, github_url,
                   entity_count, fact_count, community_count, rule_count,
                   mermaid_content, dream_synopsis, created_at
            FROM ontology_snapshots
            WHERE agent_id = $1
            ORDER BY version DESC LIMIT 1
            "#,
        )
        .bind(db_agent.agent_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some(row) = row {
            return Ok(Json(json!({
                "ontology_id": format!("{}_ontology", agent_id),
                "agent_id": agent_id,
                "version": row.get::<i32, _>("version"),
                "mermaid_content": row.get::<String, _>("mermaid_content"),
                "git_commit_sha": row.get::<String, _>("git_commit_sha"),
                "github_url": row.get::<Option<String>, _>("github_url"),
                "dream_synopsis": row.get::<Option<String>, _>("dream_synopsis"),
                "entities": [],
                "relationships": [],
                "evolution_commits": row.get::<i32, _>("version"),
                "stats": {
                    "entity_count": row.get::<i32, _>("entity_count"),
                    "fact_count": row.get::<i32, _>("fact_count"),
                    "community_count": row.get::<i32, _>("community_count"),
                    "rule_count": row.get::<i32, _>("rule_count"),
                },
                "source": "database",
            })));
        }
    }

    // Fallback: sample files
    let sample_path = format!("ontologies/samples/{}_ontology.json", agent_id);
    if let Ok(content) = std::fs::read_to_string(&sample_path) {
        if let Ok(ontology) = serde_json::from_str::<Value>(&content) {
            return Ok(Json(ontology));
        }
    }

    Ok(Json(json!({
        "ontology_id": format!("{}_ontology", agent_id),
        "agent_id": agent_id,
        "version": "1.0.0",
        "entities": [],
        "relationships": [],
        "evolution_commits": 0,
        "metadata": {
            "status": "empty",
            "message": "No ontology data available for this agent"
        }
    })))
}

// ─── Projector routes ──────────────────────────────────────────────

async fn projector_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/projector.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/projector.html: {}", e);
            format!(
                "<h1>Embedding Projector</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

#[derive(Debug, Deserialize)]
struct ProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
}

async fn get_agent_projections(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<ProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let dims = params.dimensions.unwrap_or(3);
    let method = parse_projection_method(params.method.as_deref());

    // Check cache
    let cache_key = agent_bestiary_projector::CacheKey {
        agent_id: Some(db_agent.agent_id),
        method: method.name().to_string(),
        dimensions: dims,
    };
    if let Some(cached) = state.projection_cache.get(&cache_key) {
        return Ok(Json(serde_json::to_value(cached).unwrap()));
    }

    let result = state
        .projection_engine
        .project_agent(db_agent.agent_id, &agent_id, &method, dims)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    state.projection_cache.insert(cache_key, result.clone());
    Ok(Json(serde_json::to_value(result).unwrap()))
}

#[derive(Debug, Deserialize)]
struct BestiaryProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
    limit: Option<usize>,
}

async fn get_bestiary_projections(
    State(state): State<AppState>,
    Query(params): Query<BestiaryProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let dims = params.dimensions.unwrap_or(3);
    let limit = params.limit.unwrap_or(5000);
    let method = parse_projection_method(params.method.as_deref());

    let cache_key = agent_bestiary_projector::CacheKey {
        agent_id: None,
        method: method.name().to_string(),
        dimensions: dims,
    };
    if let Some(cached) = state.projection_cache.get(&cache_key) {
        return Ok(Json(serde_json::to_value(cached).unwrap()));
    }

    let result = state
        .projection_engine
        .project_bestiary(&method, dims, limit)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    state.projection_cache.insert(cache_key, result.clone());
    Ok(Json(serde_json::to_value(result).unwrap()))
}

#[derive(Debug, Deserialize)]
struct TemporalProjectionParams {
    method: Option<String>,
    dimensions: Option<u8>,
    keyframes: Option<usize>,
}

async fn get_temporal_projections(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<TemporalProjectionParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let dims = params.dimensions.unwrap_or(3);
    let keyframes = params.keyframes.unwrap_or(10);
    let method = parse_projection_method(params.method.as_deref());

    let result = state
        .projection_engine
        .project_agent_temporal(db_agent.agent_id, &agent_id, &method, dims, keyframes)
        .await
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?;

    Ok(Json(serde_json::to_value(result).unwrap()))
}

fn parse_projection_method(method: Option<&str>) -> ProjectionMethod {
    match method {
        Some("tsne") => ProjectionMethod::Tsne { perplexity: 30.0 },
        _ => ProjectionMethod::Pca,
    }
}

// ─── Auth routes ───────────────────────────────────────────────────

/// Query param to track which provider started the flow
#[derive(Debug, Deserialize)]
struct AuthCallbackQuery {
    code: String,
    state: String,
}

/// Redirect to Google OAuth
async fn auth_google(State(state): State<AppState>) -> Result<Redirect, (StatusCode, String)> {
    let config = state.oauth.google().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Google OAuth not configured".to_string(),
        )
    })?;
    let csrf_state = generate_state();
    // Store provider hint in state: "google:<random>"
    let state_with_provider = format!("google:{}", csrf_state);
    let url = build_google_auth_url(config, &state_with_provider);
    Ok(Redirect::temporary(&url))
}

/// Redirect to GitHub OAuth
async fn auth_github(State(state): State<AppState>) -> Result<Redirect, (StatusCode, String)> {
    let config = state.oauth.github().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GitHub OAuth not configured".to_string(),
        )
    })?;
    let csrf_state = generate_state();
    let state_with_provider = format!("github:{}", csrf_state);
    let url = build_github_auth_url(config, &state_with_provider);
    Ok(Redirect::temporary(&url))
}

/// Handle OAuth callback from Google or GitHub
async fn auth_callback(
    State(state): State<AppState>,
    Query(params): Query<AuthCallbackQuery>,
) -> Result<Response, (StatusCode, String)> {
    let map_err = |e: fermi_auth::AuthError| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());

    // Determine provider from state prefix
    let (provider, _csrf) = params
        .state
        .split_once(':')
        .unwrap_or(("unknown", &params.state));

    let user_info = match provider {
        "google" => {
            let config = state.oauth.google().map_err(|e| map_err(e))?;
            let tokens = google_exchange_code(config, &params.code)
                .await
                .map_err(map_err)?;
            google_fetch_user_info(&tokens.access_token)
                .await
                .map_err(map_err)?
        }
        "github" => {
            let config = state.oauth.github().map_err(|e| map_err(e))?;
            let tokens = github_exchange_code(config, &params.code)
                .await
                .map_err(map_err)?;
            github_fetch_user_info(&tokens.access_token)
                .await
                .map_err(map_err)?
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Unknown OAuth provider".to_string(),
            ));
        }
    };

    // Sync user to database
    let user = sync_user(&state.db, &user_info).await.map_err(map_err)?;

    // Create session JWT
    let token = create_session_token(&user, &state.jwt_secret).map_err(map_err)?;

    // Set cookie and redirect to home
    let cookie = format!(
        "abw_session={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=604800",
        token
    );

    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, cookie)
        .body(axum::body::Body::empty())
        .unwrap())
}

/// Logout — clear session cookie
async fn auth_logout() -> Response {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, "/")
        .header(
            header::SET_COOKIE,
            "abw_session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        )
        .body(axum::body::Body::empty())
        .unwrap()
}

/// Get current authenticated user
async fn auth_me(principal: AuthPrincipal) -> Json<Value> {
    match principal {
        AuthPrincipal::User(user) => Json(json!({
            "user_id": user.user_id,
            "email": user.email,
            "display_name": user.display_name,
            "role": user.role,
            "auth_provider": user.auth_provider,
            "github_username": user.github_username,
        })),
        AuthPrincipal::ApiKey(key) => Json(json!({
            "user_id": key.user_id,
            "auth_type": "api_key",
            "key_name": key.name,
            "scopes": key.scopes,
        })),
    }
}

// ─── API key management ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    scopes: Option<Vec<String>>,
}

async fn create_api_key(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let scopes = body.scopes.unwrap_or_else(|| vec!["read".to_string()]);
    let (plaintext_key, key_info) =
        api_keys::create_api_key(&state.db, &principal.user_id(), &body.name, &scopes)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "key": plaintext_key,
        "key_id": key_info.key_id,
        "name": key_info.name,
        "scopes": key_info.scopes,
        "note": "Save this key — it cannot be retrieved again."
    })))
}

async fn list_api_keys(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let keys = api_keys::list_api_keys(&state.db, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "api_keys": keys })))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(key_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    api_keys::revoke_api_key(&state.db, &principal.user_id(), key_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked" })))
}

// ─── Team management ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CreateTeamRequest {
    name: String,
    slug: String,
    description: Option<String>,
}

async fn create_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let team = teams::create_team(
        &state.db,
        &body.name,
        &body.slug,
        body.description.as_deref(),
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(team))))
}

async fn list_teams_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_teams = teams::get_user_teams(&state.db, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "teams": user_teams })))
}

async fn get_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify membership
    let role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if role.is_none() && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not a team member".to_string()));
    }

    let team = teams::get_team(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    let members = teams::get_team_members(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "team": team,
        "members": members,
    })))
}

async fn delete_team_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    teams::delete_team(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| match e {
            fermi_auth::AuthError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    Ok(Json(json!({ "status": "deleted" })))
}

// ─── Team membership ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AddMemberRequest {
    member_id: String,
    member_type: Option<String>,
    role: Option<String>,
}

async fn add_member_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
    Json(body): Json<AddMemberRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    // Check requester has invite permission (admin or owner)
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    if !requester_role.can_invite() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins and owners can invite members".to_string(),
        ));
    }

    let member_type = match body.member_type.as_deref() {
        Some("agent") => MemberType::Agent,
        _ => MemberType::User,
    };

    let role = match body.role.as_deref() {
        Some("admin") => TeamRole::Admin,
        Some("viewer") => TeamRole::Viewer,
        _ => TeamRole::Member,
    };

    teams::add_team_member(
        &state.db,
        team_id,
        member_type,
        &body.member_id,
        role,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!({ "status": "added" }))))
}

async fn list_members_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(team_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify membership
    let role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if role.is_none() && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not a team member".to_string()));
    }

    let members = teams::get_team_members(&state.db, team_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "members": members })))
}

async fn remove_member_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((team_id, member_id)): Path<(uuid::Uuid, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    // Members can remove themselves; admins/owners can remove others
    if member_id != principal.user_id() && !requester_role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins can remove other members".to_string(),
        ));
    }

    teams::remove_team_member(&state.db, team_id, &member_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "removed" })))
}

#[derive(Debug, Deserialize)]
struct UpdateRoleRequest {
    role: String,
}

async fn update_member_role_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((team_id, member_id)): Path<(uuid::Uuid, String)>,
    Json(body): Json<UpdateRoleRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let requester_role = teams::get_member_role(&state.db, team_id, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a team member".to_string()))?;

    if !requester_role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only admins and owners can change roles".to_string(),
        ));
    }

    let new_role = TeamRole::from_str(&body.role);

    teams::update_member_role(&state.db, team_id, &member_id, new_role)
        .await
        .map_err(|e| match e {
            fermi_auth::AuthError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })?;

    Ok(Json(json!({ "status": "updated" })))
}

// ─── Object sharing ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ShareObjectRequest {
    object_type: String,
    object_id: String,
    share_type: String,
    share_target: String,
    permission: Option<String>,
}

async fn share_object_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<ShareObjectRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let object_type = ObjectType::from_str(&body.object_type)
        .ok_or((StatusCode::BAD_REQUEST, "Invalid object_type".to_string()))?;

    let share_type = match body.share_type.as_str() {
        "team" => ShareType::Team,
        "user" => ShareType::User,
        _ => return Err((StatusCode::BAD_REQUEST, "Invalid share_type".to_string())),
    };

    let permission = match body.permission.as_deref() {
        Some("edit") => Permission::Edit,
        Some("admin") => Permission::Admin,
        _ => Permission::View,
    };

    let share = teams::share_object(
        &state.db,
        object_type,
        &body.object_id,
        share_type,
        &body.share_target,
        permission,
        &principal.user_id(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(json!(share))))
}

async fn revoke_share_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(share_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    teams::revoke_share(&state.db, share_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked" })))
}

// ─── Agent seeding (filesystem → database) ─────────────────────────

async fn seed_agents_to_database(memory_store: &MemoryStore, registry: &AgentRegistry) {
    let cards = match registry.list_cards() {
        Ok(cards) => cards,
        Err(_) => return,
    };

    for card in &cards {
        let agent = Agent {
            agent_id: uuid::Uuid::new_v4(),
            agent_name: card.agent_id.clone(),
            agent_type: card.agent_type.clone(),
            version: card.version.clone(),
            tier: format!("{:?}", card.tier).to_lowercase(),
            executor_type: format!("{:?}", card.capabilities.executor).to_lowercase(),
            model: card.capabilities.model.clone(),
            temperature: card.capabilities.temperature,
            mcp_servers: None,
            description: Some(card.metadata.description.clone()),
            author: card.metadata.author.clone(),
            current_ontology_commit: None,
            current_ontology_snapshot_id: None,
            last_consolidated_at: None,
            dreaming_budget_credits: 10, // default budget
            dreaming_credits_used: 0,
            dreaming_budget_reset_at: None,
        };

        match memory_store.upsert_agent(agent).await {
            Ok(id) => println!("Seeded agent {} → {}", card.agent_id, id),
            Err(e) => eprintln!("Warning: failed to seed {}: {}", card.agent_id, e),
        }
    }
}

// ─── Agent resolution helper ───────────────────────────────────────

async fn resolve_agent(state: &AppState, agent_id: &str) -> Result<Agent, (StatusCode, String)> {
    state
        .memory_store
        .get_agent_by_name(agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                format!("Agent '{}' not found: {}", agent_id, e),
            )
        })
}

// ─── Agent execution ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    query: String,
}

async fn execute_agent_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // 1. Resolve agent in both registry and database
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let card = state.registry.get(&agent_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("Agent not in registry: {}", e),
        )
    })?;

    // 2. Build execution context
    let agent_stmt = ast::AgentStmt {
        name: agent_id.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: body.query.clone(),
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
    };

    // 3. Execute
    let output = state
        .registry
        .execute_agent(&agent_stmt, &context)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Execution failed: {}", e),
            )
        })?;

    // 4. Record stats in registry
    let _ = state.registry.record_execution(&agent_id, &output);

    // 5. Store as ADM episode
    let episode = agent_output_to_episode(db_agent.agent_id, &body.query, &output);
    let episode_id = state
        .memory_store
        .store_episode(episode)
        .await
        .map_err(|e| {
            eprintln!("Warning: failed to store episode: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // 6. Return result
    Ok(Json(json!({
        "agent_id": agent_id,
        "episode_id": episode_id,
        "status": format!("{:?}", output.status),
        "confidence": output.confidence,
        "execution_time_ms": output.execution_time_ms,
        "tokens_used": output.tokens_used,
        "evidence": output.evidence.iter().map(|e| json!({
            "id": e.id,
            "source": e.source,
            "summary": e.summary,
            "key_findings": e.key_findings,
            "relevance": e.relevance,
        })).collect::<Vec<_>>(),
        "metadata": {
            "model_used": output.metadata.model_used,
            "reasoning": output.metadata.reasoning,
        }
    })))
}

fn agent_output_to_episode(agent_db_id: uuid::Uuid, query: &str, output: &AgentOutput) -> Episode {
    Episode {
        episode_id: uuid::Uuid::new_v4(),
        agent_id: agent_db_id,
        timestamp_ref: output.timestamp,
        query: query.to_string(),
        context: json!({
            "evidence": output.evidence.iter().map(|e| json!({
                "id": e.id,
                "source": e.source,
                "summary": e.summary,
                "key_findings": e.key_findings,
                "relevance": e.relevance,
            })).collect::<Vec<_>>(),
            "sources_consulted": output.sources_consulted,
            "model_used": output.metadata.model_used,
            "reasoning": output.metadata.reasoning,
        }),
        execution_status: match output.status {
            AgentStatus::Success => ExecutionStatus::Success,
            AgentStatus::Failed | AgentStatus::Timeout => ExecutionStatus::Failure,
            AgentStatus::BelowConfidenceThreshold => ExecutionStatus::Partial,
        },
        error_details: match output.status {
            AgentStatus::Failed => Some("Execution failed".to_string()),
            AgentStatus::Timeout => Some("Execution timed out".to_string()),
            _ => None,
        },
        execution_time_ms: output.execution_time_ms as i64,
        tokens_used: output.tokens_used.map(|t| t as i32),
        cost_usd: output.tokens_used.map(|t| {
            rust_decimal::Decimal::from_f64_retain((t as f64 / 1_000_000.0) * 3.0)
                .unwrap_or_default()
        }),
        embedding: None,
        consolidated: false,
    }
}

// ─── Ontology API (database-backed) ────────────────────────────────

async fn get_ontology_history(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let rows = sqlx::query(
        r#"
        SELECT snapshot_id, version, git_commit_sha, entity_count, fact_count,
               community_count, rule_count, dream_synopsis, created_at
        FROM ontology_snapshots
        WHERE agent_id = $1
        ORDER BY version DESC
        "#,
    )
    .bind(db_agent.agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let snapshots: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "snapshot_id": r.get::<uuid::Uuid, _>("snapshot_id"),
                "version": r.get::<i32, _>("version"),
                "git_commit_sha": r.get::<String, _>("git_commit_sha"),
                "entity_count": r.get::<i32, _>("entity_count"),
                "fact_count": r.get::<i32, _>("fact_count"),
                "community_count": r.get::<i32, _>("community_count"),
                "rule_count": r.get::<i32, _>("rule_count"),
                "dream_synopsis": r.get::<Option<String>, _>("dream_synopsis"),
                "created_at": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_uuid": db_agent.agent_id,
        "snapshots": snapshots,
        "total": snapshots.len(),
    })))
}

async fn get_ontology_snapshot(
    State(state): State<AppState>,
    Path((agent_id, snapshot_id)): Path<(String, uuid::Uuid)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    let row = sqlx::query(
        r#"
        SELECT snapshot_id, version, git_commit_sha, github_url,
               entity_count, fact_count, community_count, rule_count,
               mermaid_content, dream_synopsis, consolidation_stats, created_at
        FROM ontology_snapshots
        WHERE snapshot_id = $1
        "#,
    )
    .bind(snapshot_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Snapshot not found".to_string()))?;

    Ok(Json(json!({
        "snapshot_id": row.get::<uuid::Uuid, _>("snapshot_id"),
        "agent_id": agent_id,
        "version": row.get::<i32, _>("version"),
        "git_commit_sha": row.get::<String, _>("git_commit_sha"),
        "github_url": row.get::<Option<String>, _>("github_url"),
        "mermaid_content": row.get::<String, _>("mermaid_content"),
        "dream_synopsis": row.get::<Option<String>, _>("dream_synopsis"),
        "consolidation_stats": row.get::<Option<Value>, _>("consolidation_stats"),
        "stats": {
            "entity_count": row.get::<i32, _>("entity_count"),
            "fact_count": row.get::<i32, _>("fact_count"),
            "community_count": row.get::<i32, _>("community_count"),
            "rule_count": row.get::<i32, _>("rule_count"),
        },
        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
    })))
}

#[derive(Debug, Deserialize)]
struct DiffParams {
    from: uuid::Uuid,
    to: uuid::Uuid,
}

async fn get_ontology_diff(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<DiffParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let _db_agent = resolve_agent(&state, &agent_id).await?;

    // Fetch both snapshots
    let from_row = sqlx::query(
        "SELECT version, mermaid_content, entity_count, fact_count, rule_count, created_at FROM ontology_snapshots WHERE snapshot_id = $1",
    )
    .bind(params.from)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Source snapshot not found".to_string()))?;

    let to_row = sqlx::query(
        "SELECT version, mermaid_content, entity_count, fact_count, rule_count, created_at FROM ontology_snapshots WHERE snapshot_id = $1",
    )
    .bind(params.to)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Target snapshot not found".to_string()))?;

    let from_content: String = from_row.get("mermaid_content");
    let to_content: String = to_row.get("mermaid_content");

    // Line-based diff
    let from_lines: std::collections::HashSet<&str> = from_content.lines().collect();
    let to_lines: std::collections::HashSet<&str> = to_content.lines().collect();

    let added: Vec<&str> = to_lines.difference(&from_lines).copied().collect();
    let removed: Vec<&str> = from_lines.difference(&to_lines).copied().collect();

    Ok(Json(json!({
        "agent_id": agent_id,
        "from": {
            "snapshot_id": params.from,
            "version": from_row.get::<i32, _>("version"),
            "entity_count": from_row.get::<i32, _>("entity_count"),
            "fact_count": from_row.get::<i32, _>("fact_count"),
            "rule_count": from_row.get::<i32, _>("rule_count"),
        },
        "to": {
            "snapshot_id": params.to,
            "version": to_row.get::<i32, _>("version"),
            "entity_count": to_row.get::<i32, _>("entity_count"),
            "fact_count": to_row.get::<i32, _>("fact_count"),
            "rule_count": to_row.get::<i32, _>("rule_count"),
        },
        "diff": {
            "lines_added": added.len(),
            "lines_removed": removed.len(),
            "added": added,
            "removed": removed,
        },
        "deltas": {
            "entity_count": to_row.get::<i32, _>("entity_count") - from_row.get::<i32, _>("entity_count"),
            "fact_count": to_row.get::<i32, _>("fact_count") - from_row.get::<i32, _>("fact_count"),
            "rule_count": to_row.get::<i32, _>("rule_count") - from_row.get::<i32, _>("rule_count"),
        }
    })))
}

// ─── Dreaming budget ───────────────────────────────────────────────

async fn get_dreaming_budget(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_uuid": db_agent.agent_id,
        "budget_credits": db_agent.dreaming_budget_credits,
        "credits_used": db_agent.dreaming_credits_used,
        "credits_remaining": db_agent.dreaming_budget_credits - db_agent.dreaming_credits_used,
        "budget_reset_at": db_agent.dreaming_budget_reset_at,
        "last_consolidated_at": db_agent.last_consolidated_at,
    })))
}

#[derive(Debug, Deserialize)]
struct SetBudgetRequest {
    budget_credits: i32,
}

async fn set_dreaming_budget(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<SetBudgetRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    sqlx::query(
        "UPDATE agents SET dreaming_budget_credits = $1, dreaming_credits_used = 0, dreaming_budget_reset_at = NOW() WHERE agent_id = $2",
    )
    .bind(body.budget_credits)
    .bind(db_agent.agent_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "updated",
        "agent_id": agent_id,
        "budget_credits": body.budget_credits,
    })))
}
