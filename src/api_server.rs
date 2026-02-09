use axum::{
    extract::{Extension, Path, Query, State},
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
    credit_charge, credit_get_balance, credit_get_transactions, credit_grant, generate_state,
    get_or_create_wallet, github_exchange_code, github_fetch_user_info, google_exchange_code,
    google_fetch_user_info, optional_auth_middleware, sync_user, teams, AuthPrincipal, AuthState,
    CreditTransaction, MemberType, OAuthConfig, ObjectType, Permission, ShareType, TeamRole,
    Wallet,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgConnectOptions, postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

#[path = "gas.rs"]
mod gas;
use gas::{charge_gas, GasFees};

use agent_bestiary_memory::{
    Agent, AgentUpdate, AnthropicEmbeddings, CoherenceEvaluation, ConsolidationLock,
    ConsolidationWorker, EmbeddingGenerator, Episode, ExecutionStatus, MemoryStore, MockEmbeddings,
    WorkspaceMessage,
};
use agent_bestiary_ontology::{GitConfig, WorkspaceGitManager};
use agent_bestiary_projector::{ProjectionCache, ProjectionEngine, ProjectionMethod};
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    memory_store: Arc<MemoryStore>,
    registry: Arc<AgentRegistry>,
    projection_engine: Arc<ProjectionEngine>,
    projection_cache: Arc<ProjectionCache>,
    embedder: Arc<dyn EmbeddingGenerator>,
    workspace_git: Arc<WorkspaceGitManager>,
    gemini_api_key: String,
    jwt_secret: String,
    oauth: OAuthConfig,
    gas_fees: GasFees,
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

    // Neon uses PgBouncer in transaction mode — disable prepared statement
    // cache to avoid "prepared statement does not exist" errors
    let connect_options = PgConnectOptions::from_str(&database_url)
        .expect("Invalid DATABASE_URL")
        .statement_cache_capacity(0);

    let db = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(connect_options)
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

    // Initialize embedding generator
    let embedder: Arc<dyn EmbeddingGenerator> =
        if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
            println!("Using Anthropic embeddings (voyage-2)");
            Arc::new(AnthropicEmbeddings::new(api_key))
        } else {
            println!("No ANTHROPIC_API_KEY, using mock embeddings");
            Arc::new(MockEmbeddings::new(1024))
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

    // Workspace git manager — per-workspace repos for version control
    let git_config = GitConfig {
        base_path: std::env::var("GIT_REPOS_PATH").unwrap_or_else(|_| "./repos".to_string()),
        author_name: "Fermi Workspace".to_string(),
        author_email: "workspace@agent-bestiary.world".to_string(),
        branch: "main".to_string(),
        github_org: std::env::var("GITHUB_ORG").ok(),
        github_token: std::env::var("GIT_GITHUB_TOKEN").ok(),
        auto_push: std::env::var("GIT_AUTO_PUSH")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false),
        remote_name: "origin".to_string(),
    };
    let workspace_git = Arc::new(
        WorkspaceGitManager::new(git_config).expect("Failed to initialize WorkspaceGitManager"),
    );

    let state = AppState {
        db: db.clone(),
        memory_store,
        registry,
        projection_engine,
        projection_cache,
        embedder,
        workspace_git,
        gemini_api_key,
        jwt_secret: jwt_secret.clone(),
        oauth,
        gas_fees: GasFees::from_env(),
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
        .route("/api/models/catalogue", get(model_catalogue_handler))
        .route("/api/agents/:agent_id/avatar", get(get_cached_avatar))
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
        // Page routes (serve HTML templates)
        .route("/dashboard", get(dashboard_view))
        .route("/agents/new", get(agent_create_view))
        .route("/workspace/:workspace_id", get(workspace_view))
        // Auth flow routes
        .route("/auth/google", get(auth_google))
        .route("/auth/github", get(auth_github))
        .route("/auth/callback", get(auth_callback))
        .route("/auth/logout", post(auth_logout))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            optional_auth_middleware,
        ));

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
        // Agent creation wizard helpers
        .route(
            "/api/ontology-templates",
            get(list_ontology_templates_handler),
        )
        .route(
            "/api/agents/generate-ontology",
            post(generate_ontology_handler),
        )
        .route("/api/agents/generate-prompt", post(generate_prompt_handler))
        .route("/api/agents/creation-guide", get(creation_guide_handler))
        .route("/api/tags/popular", get(popular_tags_handler))
        // Agent CRUD
        .route("/api/agents", post(create_agent_handler))
        .route("/api/agents/import", post(import_agent_handler))
        .route("/api/agents/mine", get(list_my_agents_handler))
        .route("/api/agents/:agent_id", put(update_agent_handler))
        .route("/api/agents/:agent_id", delete(delete_agent_handler))
        // Agent avatar generation (credit-gated)
        .route(
            "/api/agents/:agent_id/avatar/generate",
            post(generate_avatar),
        )
        // Custom embeddings import
        .route(
            "/api/agents/:agent_id/embeddings/import",
            post(import_embeddings_handler),
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
        // Consolidation trigger
        .route(
            "/api/agents/:agent_id/consolidate",
            post(consolidate_agent_handler),
        )
        // Workspace routes
        .route("/api/workspaces", get(list_workspaces_handler))
        .route("/api/workspaces/:workspace_id", get(get_workspace_handler))
        .route(
            "/api/workspaces/:workspace_id/agents",
            get(list_workspace_agents_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/agents",
            post(create_workspace_agent_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/budget",
            post(fund_workspace_handler),
        )
        // Workspace chat
        .route(
            "/api/workspaces/:workspace_id/messages",
            post(post_workspace_message_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/messages",
            get(get_workspace_messages_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/messages/poll",
            get(poll_workspace_messages_handler),
        )
        // Workspace agent hire/add
        .route(
            "/api/workspaces/:workspace_id/hire",
            post(hire_agent_handler),
        )
        .route("/api/workspaces/:workspace_id/add", post(add_agent_handler))
        .route(
            "/api/workspaces/:workspace_id/agents/:agent_id",
            delete(remove_workspace_agent_handler),
        )
        // Wallet / credits
        .route("/api/wallet", get(get_wallet_handler))
        .route("/api/wallet/transactions", get(get_transactions_handler))
        // Coherence evaluation
        .route(
            "/api/workspaces/:workspace_id/coherence/evaluate",
            post(evaluate_coherence_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/coherence",
            get(get_coherence_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/coherence/history",
            get(get_coherence_history_handler),
        )
        // Workspace ontology (merged)
        .route(
            "/api/workspaces/:workspace_id/ontology",
            get(get_workspace_ontology_handler),
        )
        // Workspace git / files
        .route(
            "/api/workspaces/:workspace_id/files",
            get(list_workspace_files_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/files/*path",
            get(read_workspace_file_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/files/*path",
            put(write_workspace_file_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/git/log",
            get(workspace_git_log_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/git/diff",
            get(workspace_git_diff_handler),
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

async fn list_agents(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
) -> Json<Value> {
    let caller_id = caller.map(|Extension(p)| p.user_id());

    // Primary: database (filter out test agents + apply visibility)
    if let Ok(db_agents) = state.memory_store.list_agents().await {
        let real_agents: Vec<_> = db_agents
            .into_iter()
            .filter(|a| !a.agent_name.starts_with("test_agent_"))
            .filter(|a| {
                // Public agents visible to everyone
                if a.visibility == "public" {
                    return true;
                }
                // Private/shared agents only visible to owner
                if let Some(ref uid) = caller_id {
                    if a.owner_id.as_deref() == Some(uid.as_str()) {
                        return true;
                    }
                }
                false
            })
            .collect();
        if !real_agents.is_empty() {
            let agents: Vec<Value> = real_agents
                .iter()
                .map(|a| {
                    // Merge filesystem card data if available
                    let card = state.registry.get(&a.agent_name).ok();
                    let card_json = card.as_ref().and_then(|_c| {
                        let path = format!("agents/curated/{}/agent_card.json", a.agent_name);
                        std::fs::read_to_string(&path).ok()
                            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    });

                    let mut agent_val = json!({
                        "agent_id": a.agent_name,
                        "display_alias": a.display_alias,
                        "agent_type": a.agent_type,
                        "version": a.version,
                        "tier": a.tier,
                        "description": a.description,
                        "author": a.author,
                        "model": a.model,
                        "tags": a.tags,
                        "visibility": a.visibility,
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
                        "execution_stats": {
                            "total_executions": a.total_executions,
                            "successful_executions": a.successful_executions,
                            "failed_executions": a.failed_executions,
                            "total_cost_usd": a.total_cost_usd,
                            "avg_execution_time_ms": a.avg_execution_time_ms,
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

/// Public endpoint: serves cached avatar only (no generation)
async fn get_cached_avatar(
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let cache_path = format!("avatars_cache/{}.json", agent_id);
    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached_data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(cached_data));
        }
    }
    Err((
        StatusCode::NOT_FOUND,
        "No cached avatar. Use POST /api/agents/:id/avatar/generate to create one.".to_string(),
    ))
}

/// Protected endpoint: generates avatar via Gemini, charges credits
async fn generate_avatar(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Check cache first (free)
    let cache_dir = "avatars_cache";
    std::fs::create_dir_all(cache_dir).ok();
    let cache_path = format!("{}/{}.json", cache_dir, agent_id);
    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached_data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(cached_data));
        }
    }

    // Charge credits for generation
    let user_id = principal.user_id();
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.avatar_generate,
        "avatar_generate",
        &format!("Avatar generation for {}", agent_id),
        Some(&agent_id),
    )
    .await?;

    if state.gemini_api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Avatar generation disabled (GEMINI_API_KEY not set)".to_string(),
        ));
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

async fn dashboard_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/dashboard.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/dashboard.html: {}", e);
            format!("<h1>Dashboard</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

async fn agent_create_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_create.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_create.html: {}", e);
            format!("<h1>Create Agent</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

async fn workspace_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/workspace.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/workspace.html: {}", e);
            format!("<h1>Workspace</h1><p>Error loading template: {}</p>", e)
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

    // Ensure wallet exists; grant onboarding credits if new
    if let Ok(wallet) = get_or_create_wallet(&state.db, "user", &user.user_id).await {
        if wallet.total_deposited == 0 && wallet.balance == 0 {
            let _ =
                credit_grant(&state.db, wallet.wallet_id, 100, "Welcome onboarding grant").await;
        }
    }

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
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_cost_usd: None,
            avg_execution_time_ms: 0,
            dreaming_budget_credits: 10, // default budget
            dreaming_credits_used: 0,
            dreaming_budget_reset_at: None,
            system_prompt: None,
            visibility: "public".to_string(),
            owner_id: None,
            tags: card.metadata.tags.clone(),
            education_budget_credits: 0,
            education_credits_used: 0,
            display_alias: None,
            llm_provider: "anthropic".to_string(),
            embedding_provider: "anthropic".to_string(),
            embedding_model: "voyage-2".to_string(),
            embedding_dimension: 1024,
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

// ─── Agent CRUD handlers ───────────────────────────────────────────

#[derive(Deserialize)]
struct CreateAgentRequest {
    agent_name: String,
    #[serde(default = "default_agent_type")]
    agent_type: String,
    description: Option<String>,
    system_prompt: Option<String>,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default = "default_temperature")]
    temperature: f64,
    #[serde(default = "default_executor")]
    executor_type: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_visibility")]
    visibility: String,
    #[serde(default)]
    education_budget_credits: i32,
    display_alias: Option<String>,
    #[serde(default = "default_llm_provider")]
    llm_provider: String,
    #[serde(default = "default_embedding_provider")]
    embedding_provider: String,
    #[serde(default = "default_embedding_model")]
    embedding_model: String,
    #[serde(default = "default_embedding_dimension")]
    embedding_dimension: i32,
}

fn default_agent_type() -> String {
    "research".to_string()
}
fn default_model() -> String {
    "claude-3-haiku-20240307".to_string()
}
fn default_temperature() -> f64 {
    0.3
}
fn default_executor() -> String {
    "llm".to_string()
}
fn default_visibility() -> String {
    "private".to_string()
}
fn default_llm_provider() -> String {
    "anthropic".to_string()
}
fn default_embedding_provider() -> String {
    "anthropic".to_string()
}
fn default_embedding_model() -> String {
    "voyage-2".to_string()
}
fn default_embedding_dimension() -> i32 {
    1024
}

async fn create_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: req.agent_name.clone(),
        agent_type: req.agent_type,
        version: "1.0.0".to_string(),
        tier: "community".to_string(),
        executor_type: req.executor_type,
        model: req.model,
        temperature: req.temperature,
        mcp_servers: None,
        description: req.description,
        author: user_id.clone(),
        system_prompt: req.system_prompt,
        visibility: req.visibility,
        owner_id: Some(user_id.clone()),
        tags: req.tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: req.education_budget_credits,
        education_credits_used: 0,
        display_alias: req.display_alias,
        llm_provider: req.llm_provider,
        embedding_provider: req.embedding_provider,
        embedding_model: req.embedding_model,
        embedding_dimension: req.embedding_dimension,
    };

    // If education budget requested, debit from user's wallet
    if req.education_budget_credits > 0 {
        let wallet = get_or_create_wallet(&state.db, "user", &user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Wallet error: {}", e),
                )
            })?;
        credit_charge(
            &state.db,
            wallet.wallet_id,
            req.education_budget_credits,
            "education_alloc",
            &format!("Education budget for agent {}", req.agent_name),
            None,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Insufficient credits: {}", e),
            )
        })?;
    }

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": req.agent_name,
        "message": "Agent created successfully"
    })))
}

// ─── Model catalogue endpoint ──────────────────────────────────────

async fn model_catalogue_handler(State(_state): State<AppState>) -> Json<Value> {
    let check_env = |key: &str| -> bool { std::env::var(key).is_ok() };

    Json(json!({
        "providers": [
            {
                "id": "anthropic",
                "name": "Anthropic",
                "models": [
                    {"id": "claude-3-haiku-20240307", "name": "Haiku", "speed": "fast", "cost_tier": "low", "description": "Fast, efficient"},
                    {"id": "claude-sonnet-4-5-20250929", "name": "Sonnet 4.5", "speed": "balanced", "cost_tier": "medium", "description": "Balanced"},
                    {"id": "claude-opus-4-6", "name": "Opus 4.6", "speed": "slow", "cost_tier": "high", "description": "Most capable"}
                ],
                "env_var": "ANTHROPIC_API_KEY",
                "available": check_env("ANTHROPIC_API_KEY")
            },
            {
                "id": "mistral",
                "name": "Mistral",
                "models": [
                    {"id": "mistral-large-latest", "name": "Mistral Large", "speed": "balanced", "cost_tier": "medium", "description": "Most capable Mistral model"},
                    {"id": "mistral-medium-latest", "name": "Mistral Medium", "speed": "fast", "cost_tier": "low", "description": "Balanced Mistral model"},
                    {"id": "open-mistral-nemo", "name": "Mistral Nemo", "speed": "fast", "cost_tier": "low", "description": "Lightweight open model"}
                ],
                "env_var": "MISTRAL_API_KEY",
                "available": check_env("MISTRAL_API_KEY")
            },
            {
                "id": "openrouter",
                "name": "OpenRouter",
                "models": [
                    {"id": "anthropic/claude-3-opus", "name": "Claude 3 Opus (via OR)", "speed": "slow", "cost_tier": "high", "description": "Anthropic via OpenRouter"},
                    {"id": "meta-llama/llama-3.1-70b-instruct", "name": "Llama 3.1 70B", "speed": "fast", "cost_tier": "low", "description": "Meta open model"},
                    {"id": "google/gemini-pro-1.5", "name": "Gemini Pro 1.5", "speed": "balanced", "cost_tier": "medium", "description": "Google via OpenRouter"},
                    {"id": "mistralai/mixtral-8x22b-instruct", "name": "Mixtral 8x22B", "speed": "fast", "cost_tier": "low", "description": "Mistral MoE via OpenRouter"}
                ],
                "env_var": "OPENROUTER_API_KEY",
                "available": check_env("OPENROUTER_API_KEY")
            },
            {
                "id": "qwen",
                "name": "Qwen",
                "models": [
                    {"id": "qwen-max", "name": "Qwen Max", "speed": "slow", "cost_tier": "medium", "description": "Most capable Qwen model"},
                    {"id": "qwen-plus", "name": "Qwen Plus", "speed": "balanced", "cost_tier": "low", "description": "Balanced Qwen model"},
                    {"id": "qwen-turbo", "name": "Qwen Turbo", "speed": "fast", "cost_tier": "low", "description": "Fast Qwen model"}
                ],
                "env_var": "QWEN_API_KEY",
                "available": check_env("QWEN_API_KEY")
            }
        ],
        "embedding_providers": [
            {"id": "anthropic", "name": "Voyage-2 (Anthropic)", "model": "voyage-2", "dimension": 1024, "env_var": "ANTHROPIC_API_KEY", "available": check_env("ANTHROPIC_API_KEY")},
            {"id": "openai", "name": "text-embedding-3-large (OpenAI)", "model": "text-embedding-3-large", "dimension": 1024, "env_var": "OPENAI_API_KEY", "available": check_env("OPENAI_API_KEY")},
            {"id": "mistral", "name": "mistral-embed (Mistral)", "model": "mistral-embed", "dimension": 1024, "env_var": "MISTRAL_API_KEY", "available": check_env("MISTRAL_API_KEY")},
            {"id": "qwen", "name": "text-embedding-v3 (Qwen)", "model": "text-embedding-v3", "dimension": 1024, "env_var": "QWEN_API_KEY", "available": check_env("QWEN_API_KEY")}
        ]
    }))
}

// ─── Import agent endpoint ─────────────────────────────────────────

#[derive(Deserialize)]
struct ImportAgentRequest {
    agent_card_json: Value,
}

async fn import_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ImportAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let card = &req.agent_card_json;

    // Extract fields from agent_card.json format
    let agent_name = card
        .get("agent_id")
        .or_else(|| card.get("agent_name"))
        .and_then(|v| v.as_str())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Missing agent_id or agent_name in card".to_string(),
        ))?
        .to_string();

    let agent_type = card
        .get("agent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("research")
        .to_string();

    let caps = card.get("capabilities");
    let model = caps
        .and_then(|c| c.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("claude-3-haiku-20240307")
        .to_string();

    let temperature = caps
        .and_then(|c| c.get("temperature"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);

    let executor_type = caps
        .and_then(|c| c.get("executor"))
        .and_then(|v| v.as_str())
        .unwrap_or("llm")
        .to_string();

    let meta = card.get("metadata");
    let description = meta
        .and_then(|m| m.get("description"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tags: Vec<String> = meta
        .and_then(|m| m.get("tags"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let system_prompt = card
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: agent_name.clone(),
        agent_type,
        version: card
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string(),
        tier: "community".to_string(),
        executor_type,
        model,
        temperature,
        mcp_servers: caps.and_then(|c| c.get("mcp_tools")).cloned(),
        description,
        author: user_id.clone(),
        system_prompt,
        visibility: "private".to_string(),
        owner_id: Some(user_id),
        tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: 0,
        education_credits_used: 0,
        display_alias: None,
        llm_provider: "anthropic".to_string(),
        embedding_provider: "anthropic".to_string(),
        embedding_model: "voyage-2".to_string(),
        embedding_dimension: 1024,
    };

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to import agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": agent_name,
        "message": "Agent imported successfully"
    })))
}

// ─── Custom embeddings import endpoint ─────────────────────────────

#[derive(Deserialize)]
struct ImportEmbeddingsRequest {
    episodes: Vec<ImportedEpisode>,
}

#[derive(Deserialize)]
struct ImportedEpisode {
    query: String,
    summary: Option<String>,
    embedding: Vec<f32>,
}

async fn import_embeddings_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
    Json(req): Json<ImportEmbeddingsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Load agent to verify ownership and get embedding dimension
    let agent = state
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    if agent.owner_id.as_deref() != Some(&user_id) {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".to_string()));
    }

    if req.episodes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No episodes provided".to_string()));
    }

    // Validate embedding dimensions
    for (i, ep) in req.episodes.iter().enumerate() {
        if ep.embedding.len() as i32 != agent.embedding_dimension {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Episode {}: expected {} dimensions, got {}. Embeddings must match agent's embedding model ({}).",
                    i, agent.embedding_dimension, ep.embedding.len(), agent.embedding_model
                ),
            ));
        }
    }

    // Charge gas
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.embedding_import,
        "embedding_import",
        &format!(
            "Import {} episodes with embeddings for agent {}",
            req.episodes.len(),
            agent.agent_name
        ),
        Some(&agent_id.to_string()),
    )
    .await?;

    // Create episodes with provided embeddings
    let mut imported = 0;
    for ep in &req.episodes {
        let episode = Episode {
            episode_id: uuid::Uuid::new_v4(),
            agent_id,
            timestamp_ref: chrono::Utc::now(),
            query: ep.query.clone(),
            context: serde_json::json!({
                "source": "import",
                "summary": ep.summary
            }),
            execution_status: agent_bestiary_memory::ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            embedding: Some(ep.embedding.clone()),
            consolidated: false,
        };

        state
            .memory_store
            .store_episode(episode)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to store episode: {}", e),
                )
            })?;
        imported += 1;
    }

    Ok(Json(json!({
        "imported": imported,
        "agent_id": agent_id,
        "message": format!("Imported {} episodes with embeddings", imported)
    })))
}

async fn list_my_agents_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let agents = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list agents: {}", e),
            )
        })?;

    let agent_list: Vec<Value> = agents
        .iter()
        .map(|a| {
            json!({
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "display_alias": a.display_alias,
                "agent_type": a.agent_type,
                "description": a.description,
                "visibility": a.visibility,
                "tags": a.tags,
                "model": a.model,
                "total_executions": a.total_executions,
                "education_budget_credits": a.education_budget_credits,
                "education_credits_used": a.education_credits_used,
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agent_list })))
}

async fn update_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(updates): Json<AgentUpdate>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Owner check
    if db_agent.owner_id.as_deref() != Some(&principal.user_id()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    state
        .memory_store
        .update_agent(db_agent.agent_id, &updates)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update agent: {}", e),
            )
        })?;

    Ok(Json(json!({ "message": "Agent updated successfully" })))
}

async fn delete_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Owner check
    if db_agent.owner_id.as_deref() != Some(&principal.user_id()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    state
        .memory_store
        .delete_agent(db_agent.agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete agent: {}", e),
            )
        })?;

    Ok(Json(json!({ "message": "Agent deleted successfully" })))
}

// ─── Wallet / Credits handlers ─────────────────────────────────────

async fn get_wallet_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    Ok(Json(json!({
        "wallet_id": wallet.wallet_id,
        "balance": wallet.balance,
        "total_deposited": wallet.total_deposited,
        "total_spent": wallet.total_spent,
        "created_at": wallet.created_at,
    })))
}

#[derive(Deserialize)]
struct TransactionsQuery {
    #[serde(default = "default_tx_limit")]
    limit: i64,
}

fn default_tx_limit() -> i64 {
    50
}

async fn get_transactions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(params): Query<TransactionsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    let txs = credit_get_transactions(&state.db, wallet.wallet_id, params.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Transaction query error: {}", e),
            )
        })?;

    let tx_list: Vec<Value> = txs
        .iter()
        .map(|t| {
            json!({
                "tx_id": t.tx_id,
                "amount": t.amount,
                "balance_after": t.balance_after,
                "tx_type": t.tx_type,
                "description": t.description,
                "related_id": t.related_id,
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "wallet_id": wallet.wallet_id,
        "balance": wallet.balance,
        "transactions": tx_list,
    })))
}

// ─── Workspace handlers ────────────────────────────────────────────

async fn list_workspaces_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let user_teams = teams::get_user_teams(&state.db, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Enrich with budget info from DB
    let mut workspaces = Vec::new();
    for team in &user_teams {
        let budget_row =
            sqlx::query("SELECT workspace_budget, workspace_spent FROM teams WHERE id = $1")
                .bind(team.id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let (budget, spent) = match budget_row {
            Some(row) => (
                row.try_get::<i32, _>("workspace_budget").unwrap_or(0),
                row.try_get::<i32, _>("workspace_spent").unwrap_or(0),
            ),
            None => (0, 0),
        };

        workspaces.push(json!({
            "id": team.id,
            "name": team.name,
            "slug": team.slug,
            "description": team.description,
            "workspace_budget": budget,
            "workspace_spent": spent,
            "workspace_remaining": budget - spent,
        }));
    }

    Ok(Json(json!({ "workspaces": workspaces })))
}

async fn get_workspace_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let team = teams::get_team(&state.db, ws_uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Get budget
    let budget_row =
        sqlx::query("SELECT workspace_budget, workspace_spent FROM teams WHERE id = $1")
            .bind(ws_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (budget, spent) = match budget_row {
        Some(row) => (
            row.try_get::<i32, _>("workspace_budget").unwrap_or(0),
            row.try_get::<i32, _>("workspace_spent").unwrap_or(0),
        ),
        None => (0, 0),
    };

    // Get workspace agents
    let ws_id_str = ws_uuid.to_string();
    let agents = state
        .memory_store
        .list_agents_for_owner(&ws_id_str)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent_list: Vec<Value> = agents
        .iter()
        .map(|a| {
            json!({
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "description": a.description,
                "total_executions": a.total_executions,
            })
        })
        .collect();

    // Get members
    let members = teams::get_team_members(&state.db, ws_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "id": team.id,
        "name": team.name,
        "slug": team.slug,
        "description": team.description,
        "workspace_budget": budget,
        "workspace_spent": spent,
        "workspace_remaining": budget - spent,
        "agents": agent_list,
        "members": members.iter().map(|m| json!({
            "member_id": m.member_id,
            "role": format!("{:?}", m.role),
        })).collect::<Vec<_>>(),
    })))
}

async fn list_workspace_agents_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Query workspace_agents junction table joined with agents
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.agent_type, a.description, a.total_executions,
                a.display_alias, a.model,
                wa.relationship, wa.added_by, wa.added_at
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1
         ORDER BY wa.added_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent_list: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "agent_id": r.get::<uuid::Uuid, _>("agent_id"),
                "agent_name": r.get::<String, _>("agent_name"),
                "display_alias": r.get::<Option<String>, _>("display_alias"),
                "agent_type": r.get::<String, _>("agent_type"),
                "model": r.get::<String, _>("model"),
                "description": r.get::<Option<String>, _>("description"),
                "total_executions": r.get::<i32, _>("total_executions"),
                "relationship": r.get::<String, _>("relationship"),
                "added_by": r.get::<String, _>("added_by"),
                "added_at": r.get::<chrono::DateTime<chrono::Utc>, _>("added_at"),
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agent_list })))
}

async fn create_workspace_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify the user is a member of this workspace
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &principal.user_id())
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                "Not a member of this workspace".to_string(),
            )
        })?;

    // Create agent owned by workspace
    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: req.agent_name.clone(),
        agent_type: req.agent_type,
        version: "1.0.0".to_string(),
        tier: "community".to_string(),
        executor_type: req.executor_type,
        model: req.model,
        temperature: req.temperature,
        mcp_servers: None,
        description: req.description,
        author: principal.user_id(),
        system_prompt: req.system_prompt,
        visibility: "shared".to_string(), // workspace agents are shared by default
        owner_id: Some(workspace_id),
        tags: req.tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: req.education_budget_credits,
        education_credits_used: 0,
        display_alias: None,
        llm_provider: "anthropic".to_string(),
        embedding_provider: "anthropic".to_string(),
        embedding_model: "voyage-2".to_string(),
        embedding_dimension: 1024,
    };

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": req.agent_name,
        "workspace_id": ws_uuid,
        "message": "Workspace agent created successfully"
    })))
}

#[derive(Deserialize)]
struct FundWorkspaceRequest {
    amount: i32,
}

async fn fund_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<FundWorkspaceRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    if req.amount <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Amount must be positive".to_string(),
        ));
    }

    // Verify owner
    let team = teams::get_team(&state.db, ws_uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    if team.owner_id != principal.user_id() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only workspace owner can fund it".to_string(),
        ));
    }

    // Charge user's wallet
    let user_wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    credit_charge(
        &state.db,
        user_wallet.wallet_id,
        req.amount,
        "transfer_out",
        &format!("Fund workspace {}", team.name),
        Some(&workspace_id),
    )
    .await
    .map_err(|e| (StatusCode::PAYMENT_REQUIRED, e.to_string()))?;

    // Credit workspace budget in teams table (display)
    sqlx::query("UPDATE teams SET workspace_budget = workspace_budget + $1 WHERE id = $2")
        .bind(req.amount)
        .bind(ws_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Also credit the workspace wallet (used for gas charges)
    let ws_wallet = get_or_create_wallet(&state.db, "workspace", &workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sqlx::query("UPDATE wallets SET balance = balance + $1, total_deposited = total_deposited + $1 WHERE wallet_id = $2")
        .bind(req.amount)
        .bind(ws_wallet.wallet_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-commit budget log to workspace git repo
    let wg = state.workspace_git.clone();
    let db_clone = state.db.clone();
    let uid = principal.user_id();
    let amt = req.amount;
    let team_name = team.name.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let entry = format!(
                "## {} — Funded {} credits\n\nBy: {}\nWorkspace: {}\n\n---\n",
                chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
                amt,
                uid,
                team_name,
            );
            // Append to budget_log or create it
            let existing = wg
                .read_file(&slug, "context/budget_log.md")
                .unwrap_or_default();
            let updated = format!("{}{}", existing, entry);
            let _ = wg.commit_file(
                &slug,
                "context/budget_log.md",
                &updated,
                &format!("Fund workspace: +{} credits", amt),
            );
        }
    });

    Ok(Json(json!({
        "message": "Workspace funded successfully",
        "amount": req.amount,
        "workspace_id": ws_uuid,
    })))
}

// ─── Workspace Gas Helper ──────────────────────────────────────────

/// Charge gas from workspace wallet and sync workspace_spent on teams table.
async fn charge_workspace_gas(
    pool: &PgPool,
    ws_uuid: uuid::Uuid,
    workspace_id: &str,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<i32, (StatusCode, String)> {
    let ws_wallet = get_or_create_wallet(pool, "workspace", workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let charged = charge_gas(
        pool,
        ws_wallet.wallet_id,
        amount,
        tx_type,
        description,
        related_id,
    )
    .await?;
    // Keep teams.workspace_spent in sync for display
    let _ = sqlx::query("UPDATE teams SET workspace_spent = workspace_spent + $1 WHERE id = $2")
        .bind(charged)
        .bind(ws_uuid)
        .execute(pool)
        .await;
    Ok(charged)
}

// ─── Workspace Chat ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PostMessageRequest {
    content: String,
    #[serde(default)]
    message_type: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
}

/// Parse @agent_name mentions from message content.
/// Returns (target_agent_name, query_text) if found.
fn parse_at_mention(content: &str) -> Option<(String, String)> {
    // Match @word_chars at start or after whitespace
    let re = regex::Regex::new(r"@([a-zA-Z0-9_-]+)").ok()?;
    let m = re.find(content)?;
    let agent_name = re.captures(content)?.get(1)?.as_str().to_string();
    // Query is everything except the @mention
    let query = format!("{}{}", &content[..m.start()], &content[m.end()..])
        .trim()
        .to_string();
    if query.is_empty() {
        return None;
    }
    Some((agent_name, query))
}

/// Load workspace context files from the git repo's context/ directory.
async fn load_workspace_context(workspace_git: &WorkspaceGitManager, slug: &str) -> String {
    let files = match workspace_git.list_files(slug, Some("context")) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut context_parts = Vec::new();
    for file in &files {
        if file.is_dir {
            continue;
        }
        if let Ok(content) = workspace_git.read_file(slug, &file.path) {
            context_parts.push(format!("--- {} ---\n{}", file.path, content));
        }
    }
    if context_parts.is_empty() {
        String::new()
    } else {
        format!("[Workspace Context]\n{}", context_parts.join("\n\n"))
    }
}

async fn post_workspace_message_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<PostMessageRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Verify membership
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Charge message gas
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.message_send,
        "gas_fee",
        "Chat message",
        None,
    )
    .await?;

    // Detect @agent_name invocation
    let at_mention = parse_at_mention(&req.content);
    let is_invocation =
        req.message_type.as_deref() == Some("agent_invocation") || at_mention.is_some();

    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "user".to_string(),
        sender_id: user_id.clone(),
        sender_name: Some(user_id.clone()),
        content: req.content.clone(),
        message_type: if is_invocation {
            "agent_invocation".to_string()
        } else {
            "chat".to_string()
        },
        metadata: req.metadata.clone().unwrap_or(json!({})),
        created_at: chrono::Utc::now(),
    };

    let msg_id = state
        .memory_store
        .store_workspace_message(&msg)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // If @agent invocation, spawn background execution
    if is_invocation {
        // Extract target agent and query
        let (target_agent, query) = if let Some((name, q)) = at_mention {
            (name, q)
        } else if let Some(meta) = &req.metadata {
            let name = meta
                .get("target_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let q = meta
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or(&req.content)
                .to_string();
            (name, q)
        } else {
            ("".to_string(), req.content.clone())
        };

        if !target_agent.is_empty() {
            // Verify agent is in workspace
            let agent_in_ws = sqlx::query(
                "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
                 JOIN agents a ON a.agent_id = wa.agent_id
                 WHERE wa.workspace_id = $1 AND a.agent_name = $2",
            )
            .bind(ws_uuid)
            .bind(&target_agent)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if let Some(agent_row) = agent_in_ws {
                let agent_name: String = agent_row.get("agent_name");
                let agent_display: Option<String> = agent_row.get("display_alias");
                let display = agent_display.unwrap_or_else(|| agent_name.clone());

                // Clone what we need for the background task
                let state2 = state.clone();
                let ws_id = workspace_id.clone();
                let ws_uuid2 = ws_uuid;
                let query2 = query.clone();
                let agent_name2 = agent_name.clone();
                let display2 = display.clone();
                let user_id2 = user_id.clone();

                tokio::spawn(async move {
                    // Load workspace context
                    let slug = get_workspace_slug(&state2.db, ws_uuid2)
                        .await
                        .unwrap_or_default();
                    let ws_context = load_workspace_context(&state2.workspace_git, &slug).await;

                    // Build augmented query with workspace context
                    let augmented_query = if ws_context.is_empty() {
                        query2.clone()
                    } else {
                        format!("{}\n\n{}", ws_context, query2)
                    };

                    // Resolve and execute
                    let result = async {
                        let db_agent = resolve_agent(&state2, &agent_name2).await?;
                        let card = state2.registry.get(&agent_name2).map_err(|e| {
                            (
                                StatusCode::NOT_FOUND,
                                format!("Agent not in registry: {}", e),
                            )
                        })?;

                        let agent_stmt = ast::AgentStmt {
                            name: agent_name2.clone(),
                            agent_type: Some(card.agent_type.clone()),
                            query: augmented_query.clone(),
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
                        let output = state2
                            .registry
                            .execute_agent(&agent_stmt, &context)
                            .await
                            .map_err(|e| {
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    format!("Execution failed: {}", e),
                                )
                            })?;

                        // Record stats
                        let _ = state2.registry.record_execution(&agent_name2, &output);

                        // Store episode
                        let mut episode =
                            agent_output_to_episode(db_agent.agent_id, &query2, &output);
                        let embed_text = format!(
                            "{} {}",
                            query2,
                            output.metadata.reasoning.as_deref().unwrap_or("")
                        );
                        if let Ok(embedding) = state2.embedder.generate(&embed_text).await {
                            episode.embedding = Some(embedding);
                        }
                        let _ = state2.memory_store.store_episode(episode).await;

                        // Charge execution gas from workspace wallet
                        let tokens = output.tokens_used.unwrap_or(0) as i32;
                        let (exec_fee, gas_fee) = state2.gas_fees.execution_fee(tokens);
                        let total = exec_fee + gas_fee;
                        let _ = charge_workspace_gas(
                            &state2.db,
                            ws_uuid2,
                            &ws_id,
                            total,
                            "execution_fee",
                            &format!("@{} execution ({}tk)", agent_name2, tokens),
                            None,
                        )
                        .await;

                        // Auto-commit ontology snapshot to workspace repo
                        if !slug.is_empty() {
                            if let Ok(snapshot) = sqlx::query(
                                "SELECT version, mermaid_content, dream_synopsis FROM ontology_snapshots
                                 WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1"
                            )
                            .bind(db_agent.agent_id)
                            .fetch_optional(&state2.db)
                            .await
                            {
                                if let Some(snap) = snapshot {
                                    let version: i32 = snap.get("version");
                                    let mermaid: Option<String> = snap.get("mermaid_content");
                                    let synopsis: Option<String> = snap.get("dream_synopsis");
                                    let content = format!(
                                        "# Ontology Snapshot v{}\n\n{}\n\n{}",
                                        version,
                                        synopsis.as_deref().unwrap_or(""),
                                        mermaid.as_deref().unwrap_or("(no diagram)")
                                    );
                                    let path = format!("ontology/{}/snapshot_v{}.md", agent_name2, version);
                                    let _ = state2.workspace_git.commit_file(
                                        &slug, &path, &content,
                                        &format!("Ontology snapshot v{} for {}", version, agent_name2),
                                    );
                                }
                            }
                        }

                        Ok::<_, (StatusCode, String)>(output)
                    }
                    .await;

                    // Post result message
                    let (content, metadata, msg_type) = match result {
                        Ok(output) => {
                            let evidence_summary = output
                                .evidence
                                .iter()
                                .map(|e| {
                                    format!("- {}", e.summary.as_deref().unwrap_or("(no summary)"))
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let reasoning = output
                                .metadata
                                .reasoning
                                .as_deref()
                                .unwrap_or("No reasoning provided");
                            let content = format!(
                                "{}\n\n{}",
                                reasoning,
                                if evidence_summary.is_empty() {
                                    String::new()
                                } else {
                                    format!("**Evidence:**\n{}", evidence_summary)
                                }
                            );
                            let meta = json!({
                                "agent_name": agent_name2,
                                "confidence": output.confidence,
                                "execution_time_ms": output.execution_time_ms,
                                "tokens_used": output.tokens_used,
                                "status": format!("{:?}", output.status),
                                "evidence_count": output.evidence.len(),
                            });
                            (content, meta, "execution_result".to_string())
                        }
                        Err((_status, err_msg)) => (
                            format!("Execution failed: {}", err_msg),
                            json!({"agent_name": agent_name2, "error": true}),
                            "execution_result".to_string(),
                        ),
                    };

                    let result_msg = WorkspaceMessage {
                        message_id: uuid::Uuid::new_v4(),
                        workspace_id: ws_uuid2,
                        sender_type: "agent".to_string(),
                        sender_id: agent_name2.clone(),
                        sender_name: Some(display2),
                        content,
                        message_type: msg_type,
                        metadata,
                        created_at: chrono::Utc::now(),
                    };
                    let _ = state2
                        .memory_store
                        .store_workspace_message(&result_msg)
                        .await;
                });
            } else {
                // Agent not in workspace — post system error
                let err_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "system".to_string(),
                    sender_name: Some("System".to_string()),
                    content: format!("Agent '{}' is not in this workspace. Use Hire or Add to bring them in first.", target_agent),
                    message_type: "system_event".to_string(),
                    metadata: json!({}),
                    created_at: chrono::Utc::now(),
                };
                let _ = state.memory_store.store_workspace_message(&err_msg).await;
            }
        }
    }

    // Auto-evaluate coherence every N messages (background, best-effort)
    let auto_eval_interval: i64 = std::env::var("COHERENCE_AUTO_EVAL_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let store = state.memory_store.clone();
    tokio::spawn(async move {
        // Check last eval time
        let since = match store.get_latest_coherence(ws_uuid).await {
            Ok(Some(e)) => e.created_at,
            _ => chrono::DateTime::<chrono::Utc>::MIN_UTC,
        };
        let count = store
            .count_workspace_messages_since(ws_uuid, since)
            .await
            .unwrap_or(0);
        if count >= auto_eval_interval {
            // Run coherence evaluation (no gas charge for auto-eval)
            let messages = match store.get_workspace_messages(ws_uuid, 50, None).await {
                Ok(m) => m,
                Err(_) => return,
            };
            if messages.is_empty() {
                return;
            }
            let conv_id = ConversationId(ws_uuid);
            let coherence_msgs: Vec<CoherenceMessage> = messages
                .iter()
                .rev()
                .map(|m| {
                    let pid = ParticipantId(
                        uuid::Uuid::parse_str(&m.sender_id)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    );
                    CoherenceMessage::new(pid, &m.content)
                })
                .collect();

            let observer = ConversationObserver::new(conv_id);
            let mut system = observer.observe(&coherence_msgs);
            let engine = SettlingEngine::with_defaults();
            engine.settle(&mut system);
            let snapshot = system.snapshot();

            let principle_scores =
                serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));
            let health_indicators = json!({
                "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
                "converged": snapshot.global_coherence.converged,
                "accepted_count": snapshot.global_coherence.accepted_count,
                "rejected_count": snapshot.global_coherence.rejected_count,
            });

            let eval = CoherenceEvaluation {
                eval_id: uuid::Uuid::new_v4(),
                workspace_id: ws_uuid,
                global_score: snapshot.global_coherence.score,
                quality_label: snapshot.global_coherence.quality_label().to_string(),
                principle_scores: principle_scores.clone(),
                health_indicators: health_indicators.clone(),
                utterance_count: snapshot.utterance_stats.total as i32,
                message_window: Some(json!({
                    "message_count": messages.len(),
                    "auto": true,
                })),
                created_at: chrono::Utc::now(),
            };

            if let Ok(eval_id) = store.store_coherence_evaluation(&eval).await {
                let update_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "coherence_evaluator".to_string(),
                    sender_name: Some("Coherence Evaluator".to_string()),
                    content: format!(
                        "Coherence: {:.0}% ({}) | {} utterances",
                        eval.global_score * 100.0,
                        eval.quality_label,
                        eval.utterance_count,
                    ),
                    message_type: "coherence_update".to_string(),
                    metadata: json!({
                        "eval_id": eval_id,
                        "global_score": eval.global_score,
                        "quality_label": eval.quality_label,
                        "auto": true,
                    }),
                    created_at: chrono::Utc::now(),
                };
                let _ = store.store_workspace_message(&update_msg).await;
            }
        }
    });

    Ok(Json(json!({
        "message_id": msg_id,
        "sender_type": msg.sender_type,
        "sender_id": msg.sender_id,
        "content": msg.content,
        "message_type": msg.message_type,
        "created_at": msg.created_at,
    })))
}

#[derive(Debug, Deserialize)]
struct MessageQuery {
    limit: Option<i64>,
    before: Option<String>,
}

async fn get_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<MessageQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let limit = params.limit.unwrap_or(50).min(200);
    let before = params.before.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, limit, before)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

#[derive(Debug, Deserialize)]
struct PollQuery {
    since: String,
}

async fn poll_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<PollQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let since = chrono::DateTime::parse_from_rfc3339(&params.since)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid timestamp format".to_string(),
            )
        })?
        .with_timezone(&chrono::Utc);

    let messages = state
        .memory_store
        .get_workspace_messages_since(ws_uuid, since)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

// ─── Workspace Hire / Add ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct HireAddRequest {
    agent_id: uuid::Uuid,
}

/// Post a system message to workspace chat (helper)
async fn post_system_message(store: &MemoryStore, workspace_id: uuid::Uuid, content: &str) {
    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id,
        sender_type: "system".to_string(),
        sender_id: "system".to_string(),
        sender_name: None,
        content: content.to_string(),
        message_type: "system_event".to_string(),
        metadata: json!({}),
        created_at: chrono::Utc::now(),
    };
    let _ = store.store_workspace_message(&msg).await;
}

async fn hire_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be admin+ to hire
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;
    if !role.can_invite() {
        return Err((
            StatusCode::FORBIDDEN,
            "Admin role required to hire agents".to_string(),
        ));
    }

    // Resolve agent
    let agent = state
        .memory_store
        .get_agent(req.agent_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Agent not found: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // Must not own the agent (use /add for your own)
    if agent.owner_id.as_deref() == Some(&user_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use /add for your own agents".to_string(),
        ));
    }

    // Agent must be public (or shared with caller — future)
    if agent.visibility != "public" {
        return Err((StatusCode::FORBIDDEN, "Agent is not public".to_string()));
    }

    // Charge hire gas from workspace wallet
    let agent_id_str = req.agent_id.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_hire,
        "gas_fee",
        &format!("Hire agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'hired') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(req.agent_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state.memory_store,
        ws_uuid,
        &format!("{} hired {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "hired",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Hired agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    Ok(Json(json!({
        "message": "Agent hired successfully",
        "agent_name": agent.agent_name,
        "relationship": "hired",
        "gas_charged": state.gas_fees.agent_hire,
    })))
}

async fn add_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be workspace member
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Resolve agent — must own it
    let agent = state
        .memory_store
        .get_agent(req.agent_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Agent not found: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    if agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "You don't own this agent. Use /hire instead.".to_string(),
        ));
    }

    // Charge add gas from workspace wallet
    let agent_id_str = req.agent_id.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_add,
        "gas_fee",
        &format!("Add agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'owned') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(req.agent_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state.memory_store,
        ws_uuid,
        &format!("{} added {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "owned",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Added agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    Ok(Json(json!({
        "message": "Agent added successfully",
        "agent_name": agent.agent_name,
        "relationship": "owned",
        "gas_charged": state.gas_fees.agent_add,
    })))
}

async fn remove_workspace_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;
    let agent_uuid: uuid::Uuid = agent_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    // Must be admin+ or the person who added
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let row = sqlx::query(
        "SELECT added_by FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(ws_uuid)
    .bind(agent_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = row.ok_or((StatusCode::NOT_FOUND, "Agent not in workspace".to_string()))?;
    let added_by: String = row.try_get("added_by").unwrap_or_default();

    if added_by != user_id && !role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Must be admin or the person who added".to_string(),
        ));
    }

    sqlx::query("DELETE FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
        .bind(ws_uuid)
        .bind(agent_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state.memory_store,
        ws_uuid,
        &format!("{} removed an agent from the workspace", user_id),
    )
    .await;

    Ok(Json(json!({ "message": "Agent removed from workspace" })))
}

// ─── Agent execution ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ExecuteRequest {
    query: String,
}

async fn execute_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();

    // 0. Check caller has credits
    let wallet = get_or_create_wallet(&state.db, "user", &caller_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    if wallet.balance <= 0 {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "Insufficient credits".to_string(),
        ));
    }

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

    // 5. Store as ADM episode (with embedding)
    let mut episode = agent_output_to_episode(db_agent.agent_id, &body.query, &output);

    // Generate embedding from query + output summary
    let embed_text = format!(
        "{} {}",
        body.query,
        output.metadata.reasoning.as_deref().unwrap_or("")
    );
    match state.embedder.generate(&embed_text).await {
        Ok(embedding) => episode.embedding = Some(embedding),
        Err(e) => eprintln!("Warning: embedding generation failed: {}", e),
    }

    let episode_id = state
        .memory_store
        .store_episode(episode)
        .await
        .map_err(|e| {
            eprintln!("Warning: failed to store episode: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // 6. Charge credits: execution fee (1 credit per 1000 tokens, min 1) + 10% gas fee
    let tokens = output.tokens_used.unwrap_or(0) as i32;
    let execution_fee = std::cmp::max(1, tokens / 1000);
    let gas_fee = std::cmp::max(1, execution_fee / 10);

    // Charge execution fee
    let ep_id_str = episode_id.to_string();
    if let Err(e) = credit_charge(
        &state.db,
        wallet.wallet_id,
        execution_fee,
        "execution_fee",
        &format!("Execute {} ({}tk)", agent_id, tokens),
        Some(ep_id_str.as_str()),
    )
    .await
    {
        eprintln!("Warning: failed to charge execution fee: {}", e);
    }

    // Charge gas fee
    if let Err(e) = credit_charge(
        &state.db,
        wallet.wallet_id,
        gas_fee,
        "gas_fee",
        &format!("Gas fee for {}", agent_id),
        Some(ep_id_str.as_str()),
    )
    .await
    {
        eprintln!("Warning: failed to charge gas fee: {}", e);
    }

    let total_charged = execution_fee + gas_fee;

    // 7. Return result
    Ok(Json(json!({
        "agent_id": agent_id,
        "episode_id": episode_id,
        "status": format!("{:?}", output.status),
        "confidence": output.confidence,
        "execution_time_ms": output.execution_time_ms,
        "tokens_used": output.tokens_used,
        "credits_charged": total_charged,
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

// ─── Consolidation trigger ─────────────────────────────────────────

async fn consolidate_agent_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Check dreaming budget
    let remaining = db_agent.dreaming_budget_credits - db_agent.dreaming_credits_used;
    if remaining <= 0 {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "No dreaming credits remaining (used {}/{})",
                db_agent.dreaming_credits_used, db_agent.dreaming_budget_credits
            ),
        ));
    }

    // Check for unconsolidated episodes first (avoids spending a credit on empty runs)
    let episodes = state
        .memory_store
        .get_unconsolidated_episodes(db_agent.agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch episodes: {}", e),
            )
        })?;

    if episodes.is_empty() {
        return Ok(Json(json!({
            "status": "completed",
            "agent_id": agent_id,
            "result": {
                "episodes_processed": 0,
                "clusters_identified": 0,
                "rules_extracted": 0,
                "message": "No unconsolidated episodes found"
            },
            "dreaming_credits_remaining": remaining,
        })));
    }

    // Create consolidation worker and run
    let pool = Arc::new(state.db.clone());
    let lock = Arc::new(ConsolidationLock::new(
        pool,
        format!("api-{}", uuid::Uuid::new_v4()),
    ));
    let worker = ConsolidationWorker::new(
        state.memory_store.clone(),
        lock,
        state.embedder.clone(),
        "api-trigger".to_string(),
    );

    let result = worker
        .consolidate_agent(db_agent.agent_id, 0.5, 2)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Consolidation failed: {}", e),
            )
        })?;

    // Debit dreaming credit
    sqlx::query(
        "UPDATE agents SET dreaming_credits_used = dreaming_credits_used + 1, last_consolidated_at = NOW() WHERE agent_id = $1",
    )
    .bind(db_agent.agent_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "completed",
        "agent_id": agent_id,
        "result": {
            "episodes_processed": result.episodes_processed,
            "clusters_identified": result.clusters_identified,
            "rules_extracted": result.rules_extracted,
            "rules_verified": result.rules_verified,
            "rules_rejected": result.rules_rejected,
            "entities_created": result.entities_created,
            "facts_created": result.facts_created,
        },
        "dreaming_credits_remaining": remaining - 1,
    })))
}

// ─── Coherence Evaluation ────────────────────────────────────────────

/// Run TEC coherence evaluation on recent workspace messages.
async fn evaluate_coherence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Verify membership
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Charge coherence evaluation gas (2 credits)
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        2, // coherence evaluation cost
        "gas_fee",
        "Coherence evaluation",
        None,
    )
    .await?;

    // Fetch recent messages (last 50)
    let messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, 50, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No messages in workspace to evaluate".to_string(),
        ));
    }

    // Convert workspace messages to coherence-core Messages
    let conv_id = ConversationId(ws_uuid);
    let coherence_messages: Vec<CoherenceMessage> = messages
        .iter()
        .rev() // messages come DESC, observer expects chronological
        .map(|m| {
            let pid = ParticipantId(
                uuid::Uuid::parse_str(&m.sender_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            );
            CoherenceMessage::new(pid, &m.content)
        })
        .collect();

    // Run observation pipeline: classify utterances + detect relations
    let observer = ConversationObserver::new(conv_id);
    let mut system = observer.observe(&coherence_messages);

    // Run settling engine
    let engine = SettlingEngine::with_defaults();
    let _result = engine.settle(&mut system);

    // Extract snapshot
    let snapshot = system.snapshot();

    // Build principle scores JSON
    let principle_scores: serde_json::Value =
        serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));

    // Build health indicators
    let health_indicators = json!({
        "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
        "converged": snapshot.global_coherence.converged,
        "accepted_count": snapshot.global_coherence.accepted_count,
        "rejected_count": snapshot.global_coherence.rejected_count,
        "evidence_density": snapshot.utterance_stats.evidence_density(),
        "explanation_density": snapshot.utterance_stats.explanation_density(),
    });

    // Store evaluation
    let eval = CoherenceEvaluation {
        eval_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        global_score: snapshot.global_coherence.score,
        quality_label: snapshot.global_coherence.quality_label().to_string(),
        principle_scores: principle_scores.clone(),
        health_indicators: health_indicators.clone(),
        utterance_count: snapshot.utterance_stats.total as i32,
        message_window: Some(json!({
            "message_count": messages.len(),
            "from": messages.last().map(|m| m.created_at),
            "to": messages.first().map(|m| m.created_at),
        })),
        created_at: chrono::Utc::now(),
    };

    let eval_id = state
        .memory_store
        .store_coherence_evaluation(&eval)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Post coherence update to workspace chat
    let update_msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "system".to_string(),
        sender_id: "coherence_evaluator".to_string(),
        sender_name: Some("Coherence Evaluator".to_string()),
        content: format!(
            "Coherence: {:.0}% ({}) | {} utterances | {}",
            eval.global_score * 100.0,
            eval.quality_label,
            eval.utterance_count,
            snapshot.feedback_action,
        ),
        message_type: "coherence_update".to_string(),
        metadata: json!({
            "eval_id": eval_id,
            "global_score": eval.global_score,
            "quality_label": eval.quality_label,
            "principle_scores": principle_scores,
            "health_indicators": health_indicators,
        }),
        created_at: chrono::Utc::now(),
    };

    let _ = state
        .memory_store
        .store_workspace_message(&update_msg)
        .await;

    Ok(Json(json!({
        "eval_id": eval_id,
        "global_score": eval.global_score,
        "quality_label": eval.quality_label,
        "principle_scores": principle_scores,
        "health_indicators": health_indicators,
        "utterance_count": eval.utterance_count,
        "message_window": eval.message_window,
    })))
}

/// Get latest coherence evaluation for a workspace.
async fn get_coherence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let eval = state
        .memory_store
        .get_latest_coherence(ws_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match eval {
        Some(e) => Ok(Json(json!({
            "eval_id": e.eval_id,
            "global_score": e.global_score,
            "quality_label": e.quality_label,
            "principle_scores": e.principle_scores,
            "health_indicators": e.health_indicators,
            "utterance_count": e.utterance_count,
            "message_window": e.message_window,
            "created_at": e.created_at,
        }))),
        None => Ok(Json(
            json!({ "eval_id": null, "message": "No evaluations yet" }),
        )),
    }
}

/// Get coherence evaluation history for a workspace.
async fn get_coherence_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let limit = params.limit.unwrap_or(20).min(100);

    let evals = state
        .memory_store
        .get_coherence_history(ws_uuid, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = evals
        .iter()
        .map(|e| {
            json!({
                "eval_id": e.eval_id,
                "global_score": e.global_score,
                "quality_label": e.quality_label,
                "principle_scores": e.principle_scores,
                "health_indicators": e.health_indicators,
                "utterance_count": e.utterance_count,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "evaluations": items })))
}

#[derive(Debug, Deserialize)]
struct HistoryQuery {
    limit: Option<i64>,
}

/// Merge ontology snapshots from all agents in a workspace into a combined view
async fn get_workspace_ontology_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Get all agents in workspace with their latest ontology snapshots
    let rows = sqlx::query(
        "SELECT a.agent_name, a.display_alias, os.version, os.mermaid_content, os.dream_synopsis, os.entity_count, os.fact_count, os.created_at
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         LEFT JOIN LATERAL (
            SELECT * FROM ontology_snapshots
            WHERE agent_id = a.agent_id
            ORDER BY created_at DESC LIMIT 1
         ) os ON true
         WHERE wa.workspace_id = $1"
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent_ontologies = Vec::new();
    let mut merged_mermaid_parts = Vec::new();
    let mut total_entities = 0i32;
    let mut total_facts = 0i32;

    for row in &rows {
        let agent_name: String = row.get("agent_name");
        let display_alias: Option<String> = row.get("display_alias");
        let version: Option<i32> = row.get("version");
        let mermaid: Option<String> = row.get("mermaid_content");
        let synopsis: Option<String> = row.get("dream_synopsis");
        let entities: Option<i32> = row.get("entity_count");
        let facts: Option<i32> = row.get("fact_count");

        total_entities += entities.unwrap_or(0);
        total_facts += facts.unwrap_or(0);

        if let Some(ref m) = mermaid {
            // Extract relationship lines from mermaid (skip the erDiagram header)
            let lines: Vec<&str> = m
                .lines()
                .filter(|l| {
                    !l.trim().is_empty()
                        && !l.trim().starts_with("erDiagram")
                        && !l.trim().starts_with("%%")
                })
                .collect();
            if !lines.is_empty() {
                merged_mermaid_parts.push(format!("    %% {} %%", agent_name));
                merged_mermaid_parts.extend(lines.iter().map(|l| l.to_string()));
            }
        }

        agent_ontologies.push(json!({
            "agent_name": agent_name,
            "display_alias": display_alias,
            "version": version,
            "entity_count": entities,
            "fact_count": facts,
            "dream_synopsis": synopsis,
            "has_ontology": mermaid.is_some(),
        }));
    }

    let merged_mermaid = if merged_mermaid_parts.is_empty() {
        None
    } else {
        Some(format!("erDiagram\n{}", merged_mermaid_parts.join("\n")))
    };

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "agent_count": rows.len(),
        "total_entities": total_entities,
        "total_facts": total_facts,
        "merged_mermaid": merged_mermaid,
        "agent_ontologies": agent_ontologies,
    })))
}

// ---------------------------------------------------------------------------
// Workspace Git / Files handlers
// ---------------------------------------------------------------------------

/// Helper: get workspace slug from UUID
async fn get_workspace_slug(
    pool: &PgPool,
    ws_uuid: uuid::Uuid,
) -> Result<String, (StatusCode, String)> {
    let row = sqlx::query("SELECT slug FROM teams WHERE id = $1")
        .bind(ws_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Workspace not found".to_string()))?;
    Ok(row.get::<String, _>("slug"))
}

#[derive(Debug, Deserialize)]
struct FilesQuery {
    path: Option<String>,
}

async fn list_workspace_files_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let files = state
        .workspace_git
        .list_files(&slug, query.path.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = files
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "name": f.name,
                "is_dir": f.is_dir,
                "size": f.size,
            })
        })
        .collect();

    Ok(Json(json!({ "files": items })))
}

async fn read_workspace_file_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path((workspace_id, file_path)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let content = state
        .workspace_git
        .read_file(&slug, &file_path)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(json!({
        "path": file_path,
        "content": content,
    })))
}

#[derive(Debug, Deserialize)]
struct WriteFileBody {
    content: String,
    message: Option<String>,
}

async fn write_workspace_file_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, file_path)): Path<(String, String)>,
    Json(body): Json<WriteFileBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    // Charge gas for file write
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.file_write,
        "file_write",
        &format!("Write file: {}", file_path),
        None,
    )
    .await?;

    let commit_msg = body
        .message
        .unwrap_or_else(|| format!("{} updated {}", principal.user_id(), file_path));

    let commit = state
        .workspace_git
        .commit_file(&slug, &file_path, &body.content, &commit_msg)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update git tracking columns
    let _ = sqlx::query(
        "UPDATE teams SET git_latest_commit = $1, git_commit_count = git_commit_count + 1 WHERE id = $2",
    )
    .bind(&commit.sha)
    .bind(ws_uuid)
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "path": file_path,
        "commit": {
            "sha": commit.sha,
            "message": commit.message,
            "timestamp": commit.timestamp,
        },
    })))
}

#[derive(Debug, Deserialize)]
struct GitLogQuery {
    limit: Option<usize>,
}

async fn workspace_git_log_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitLogQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;
    let limit = query.limit.unwrap_or(20);

    let log = state
        .workspace_git
        .get_log(&slug, limit)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = log
        .iter()
        .map(|c| {
            json!({
                "sha": c.sha,
                "message": c.message,
                "timestamp": c.timestamp,
                "author": c.author,
            })
        })
        .collect();

    Ok(Json(json!({ "commits": items })))
}

#[derive(Debug, Deserialize)]
struct GitDiffQuery {
    from: String,
    to: String,
}

async fn workspace_git_diff_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let diff = state
        .workspace_git
        .diff_commits(&slug, &query.from, &query.to)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "from": query.from,
        "to": query.to,
        "diff": diff,
    })))
}

// ---------------------------------------------------------------------------
// Agent creation wizard helpers
// ---------------------------------------------------------------------------

async fn list_ontology_templates_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let seeds_dir = std::path::Path::new("agents/templates/ontology_seeds");
    if !seeds_dir.exists() {
        return Ok(Json(json!({ "templates": [] })));
    }

    let mut templates = Vec::new();
    let entries = std::fs::read_dir(seeds_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                templates.push(val);
            }
        }
    }

    Ok(Json(json!({ "templates": templates })))
}

#[derive(Debug, Deserialize)]
struct GenerateOntologyRequest {
    domain_description: String,
}

async fn generate_ontology_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<GenerateOntologyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Charge 2 credits
    let wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        2,
        "ontology_generation",
        "Generate seed ontology",
        None,
    )
    .await?;

    // Call Claude to generate a Mermaid ontology
    let prompt = format!(
        r#"Generate a Mermaid ER diagram for the following domain. Use erDiagram syntax with entities and relationships. Keep it focused: 5-8 entities, clear relationship labels.

Domain: {}

Return ONLY the Mermaid diagram starting with "erDiagram", no markdown fences, no explanation."#,
        req.domain_description
    );

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-5-20250929",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mermaid = body["content"][0]["text"]
        .as_str()
        .unwrap_or("erDiagram\n    ENTITY_A ||--o{ ENTITY_B : relates_to\n")
        .to_string();

    Ok(Json(json!({
        "mermaid": mermaid,
        "domain": req.domain_description,
    })))
}

#[derive(Debug, Deserialize)]
struct GeneratePromptRequest {
    agent_type: String,
    description: String,
    ontology: Option<String>,
}

async fn generate_prompt_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<GeneratePromptRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Charge 1 credit
    let wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "prompt_generation",
        "Generate system prompt",
        None,
    )
    .await?;

    let ontology_ctx = req.ontology.as_deref().unwrap_or("(none provided)");
    let prompt = format!(
        r#"Generate a system prompt for a Fermi forecasting agent with these characteristics:

Type: {}
Description: {}
Ontology (Mermaid ER): {}

The system prompt should:
1. Define the agent's role and expertise clearly
2. Specify how it should approach research queries
3. Include confidence scoring guidelines (0.0-1.0)
4. List key evidence categories it should look for
5. Be 150-300 words

Return ONLY the system prompt text, no markdown, no explanation."#,
        req.agent_type, req.description, ontology_ctx
    );

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-5-20250929",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let system_prompt = body["content"][0]["text"]
        .as_str()
        .unwrap_or("You are a specialist forecasting agent.")
        .to_string();

    Ok(Json(json!({
        "system_prompt": system_prompt,
    })))
}

async fn creation_guide_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Return structured tips from the prompt engineering guide
    Ok(Json(json!({
        "tips": [
            {
                "step": "identity",
                "title": "Naming",
                "content": "Use lowercase with underscores (e.g., market_research). The name becomes the agent's system identifier."
            },
            {
                "step": "identity",
                "title": "Type Selection",
                "content": "Research agents gather information. Risk agents assess threats. Sentiment agents track opinions. Forecasting agents predict outcomes."
            },
            {
                "step": "ontology",
                "title": "Seed Ontology",
                "content": "A seed ontology gives your agent initial structure. It defines entities and relationships the agent will track. The ontology evolves as the agent learns."
            },
            {
                "step": "capabilities",
                "title": "Temperature",
                "content": "Lower (0.1-0.3) for factual extraction and analysis. Higher (0.5-0.8) for creative or exploratory tasks. Default 0.3 works well for most agents."
            },
            {
                "step": "capabilities",
                "title": "System Prompt",
                "content": "Be specific about the agent's expertise, output format, and confidence scoring. Include domain terminology. The system prompt is the most important configuration."
            },
            {
                "step": "economics",
                "title": "Education Budget",
                "content": "Credits allocated for the ADM learning cycle. Each consolidation cycle costs 3 credits. More cycles = deeper learning. Start with 0 and add later."
            },
            {
                "step": "economics",
                "title": "How Optimization Works",
                "content": "Execute queries -> episodic memory stored -> consolidation extracts patterns -> semantic rules formed -> ontology evolves -> agent improves over time."
            }
        ]
    })))
}

async fn popular_tags_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT UNNEST(tags) as tag, COUNT(*) as cnt FROM agents WHERE tags IS NOT NULL GROUP BY tag ORDER BY cnt DESC LIMIT 20"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tags: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "tag": r.get::<String, _>("tag"),
                "count": r.get::<i64, _>("cnt"),
            })
        })
        .collect();

    Ok(Json(json!({ "tags": tags })))
}
