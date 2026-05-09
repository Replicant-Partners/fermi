use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, patch, post, put},
    Router,
};
use fermi::agent_backend::{
    agent_card::{
        AgentCapabilities, AgentCard, AgentDependencies, AgentMetadata as CardMetadata,
        AgentPerformance, AgentTier, AgentUsage, OntologyStats, UsageWindow,
    },
    executor::{AgentExecutor, AgentOutput, AgentStatus},
    llm_executor::LLMExecutor,
    multi_model_executor::MultiModelExecutor,
    registry::AgentRegistry,
};
use fermi::ast;
use fermi_auth::{
    auth_middleware, optional_auth_middleware, AuthPrincipal, AuthState, OAuthConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgConnectOptions, postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use fermi::gas::GasFees;
use tokio::sync::broadcast;

use agent_bestiary_memory::{
    Agent, AnthropicEmbeddings, EmbeddingGenerator, Episode, ExecutionStatus, MemoryStore,
    MockEmbeddings,
};
use agent_bestiary_ontology::{GitConfig, WorkspaceGitManager};
use agent_bestiary_projector::{ProjectionCache, ProjectionEngine};

// ─── Rate Limiter ──────────────────────────────────────────────────

use dashmap::DashMap;
use std::time::Instant;

// Handler modules — new handlers go here instead of this file
#[path = "handlers/mod.rs"]
mod handlers;

#[derive(Clone)]
struct RateLimiter {
    /// Map of key -> list of request timestamps (sliding window)
    windows: Arc<DashMap<String, Vec<Instant>>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            windows: Arc::new(DashMap::new()),
            max_requests,
            window_secs,
        }
    }

    /// Returns Ok(remaining) or Err(retry_after_secs)
    fn check(&self, key: &str) -> Result<u32, u64> {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(self.window_secs);

        let mut entry = self.windows.entry(key.to_string()).or_default();
        // Prune old entries
        entry.retain(|t| *t > cutoff);

        if entry.len() >= self.max_requests as usize {
            // Find earliest entry to compute retry-after
            let earliest = entry.iter().min().copied().unwrap_or(now);
            let retry_after = self
                .window_secs
                .saturating_sub(now.duration_since(earliest).as_secs());
            Err(retry_after.max(1))
        } else {
            entry.push(now);
            let remaining = self.max_requests - entry.len() as u32;
            Ok(remaining)
        }
    }

    /// Periodically clean up old entries (call from a background task)
    fn cleanup(&self) {
        let now = Instant::now();
        let cutoff = now - std::time::Duration::from_secs(self.window_secs * 2);
        self.windows.retain(|_, v| {
            v.retain(|t| *t > cutoff);
            !v.is_empty()
        });
    }
}

// ─── Observability APIs (Sprint N) ─────────────────────────────────

#[derive(Clone)]
struct RateLimitConfig {
    public: RateLimiter, // 300 req/min per IP
    authed: RateLimiter, // 300 req/min per user
    llm: RateLimiter,    // 10 req/min per user (execute, generate)
}

impl RateLimitConfig {
    fn from_env() -> Self {
        let public_rpm: u32 = std::env::var("RATE_LIMIT_PUBLIC")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let auth_rpm: u32 = std::env::var("RATE_LIMIT_AUTH")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);
        let llm_rpm: u32 = std::env::var("RATE_LIMIT_LLM")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10);

        Self {
            public: RateLimiter::new(public_rpm, 60),
            authed: RateLimiter::new(auth_rpm, 60),
            llm: RateLimiter::new(llm_rpm, 60),
        }
    }
}

/// Rate limit middleware — checks public rate limit by remote IP
async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, (StatusCode, String)> {
    // Extract IP from request (peer addr or X-Forwarded-For)
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    match state.rate_limits.public.check(&ip) {
        Ok(remaining) => {
            let mut response = next.run(req).await;
            response
                .headers_mut()
                .insert("x-ratelimit-remaining", HeaderValue::from(remaining));
            Ok(response)
        }
        Err(retry_after) => Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("Rate limit exceeded. Retry after {} seconds.", retry_after),
        )),
    }
}

/// Rate limit middleware for LLM endpoints (stricter, per-user)
#[allow(dead_code)]
async fn llm_rate_limit_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, (StatusCode, String)> {
    // Try to extract user_id from auth principal (Extension)
    let key = req
        .extensions()
        .get::<AuthPrincipal>()
        .map(|p| format!("user:{}", p.user_id()))
        .unwrap_or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.split(',').next())
                .map(|s| format!("ip:{}", s.trim()))
                .unwrap_or_else(|| "unknown".to_string())
        });

    match state.rate_limits.llm.check(&key) {
        Ok(remaining) => {
            let mut response = next.run(req).await;
            response
                .headers_mut()
                .insert("x-ratelimit-remaining", HeaderValue::from(remaining));
            Ok(response)
        }
        Err(retry_after) => Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "LLM rate limit exceeded ({}/min). Retry after {} seconds.",
                state.rate_limits.llm.max_requests, retry_after
            ),
        )),
    }
}

/// Stripe configuration — pricing tiers for credit purchases.
#[derive(Clone, Default)]
struct StripeConfig {
    secret_key: String,
    webhook_secret: String,
    publishable_key: String,
}

impl StripeConfig {
    fn from_env() -> Self {
        Self {
            secret_key: std::env::var("STRIPE_SECRET_KEY").unwrap_or_default(),
            webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_default(),
            publishable_key: std::env::var("STRIPE_PUBLISHABLE_KEY").unwrap_or_default(),
        }
    }

    fn is_configured(&self) -> bool {
        !self.secret_key.is_empty()
    }

    fn client(&self) -> stripe::Client {
        stripe::Client::new(&self.secret_key)
    }
}

/// Credit pricing tiers
pub(crate) struct CreditTier {
    pub(crate) credits: i32,
    pub(crate) price_cents: i64,
    pub(crate) label: &'static str,
    pub(crate) discount_pct: i32,
}

pub(crate) const CREDIT_TIERS: &[CreditTier] = &[
    CreditTier {
        credits: 250,
        price_cents: 500,
        label: "Starter",
        discount_pct: 0,
    },
    CreditTier {
        credits: 750,
        price_cents: 1200,
        label: "Explorer",
        discount_pct: 20,
    },
    CreditTier {
        credits: 2000,
        price_cents: 2500,
        label: "Keeper",
        discount_pct: 38,
    },
    CreditTier {
        credits: 5000,
        price_cents: 5000,
        label: "Breeder",
        discount_pct: 50,
    },
];

/// Workspace chat event — broadcast to SSE subscribers.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct WorkspaceEvent {
    pub workspace_id: uuid::Uuid,
    pub message: serde_json::Value,
}

#[derive(Clone, Debug)]
pub(crate) struct RabbleEvent {
    pub swarm_id: uuid::Uuid,
    pub message: serde_json::Value,
}

/// Creature lifecycle event — broadcast to per-creature SSE subscribers.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct CreatureEvent {
    pub creature_id: uuid::Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) db: PgPool,
    pub(crate) memory_store: Arc<MemoryStore>,
    pub(crate) registry: Arc<AgentRegistry>,
    pub(crate) projection_engine: Arc<ProjectionEngine>,
    pub(crate) projection_cache: Arc<ProjectionCache>,
    pub(crate) embedder: Arc<dyn EmbeddingGenerator>,
    pub(crate) workspace_git: Arc<WorkspaceGitManager>,
    pub(crate) gemini_api_key: String,
    pub(crate) jwt_secret: String,
    pub(crate) oauth: OAuthConfig,
    pub(crate) gas_fees: GasFees,
    pub(crate) stripe: StripeConfig,
    pub(crate) rate_limits: RateLimitConfig,
    pub(crate) ws_broadcast: broadcast::Sender<WorkspaceEvent>,
    pub(crate) rabble_broadcast: broadcast::Sender<RabbleEvent>,
    pub(crate) creature_broadcast: broadcast::Sender<CreatureEvent>,
    pub(crate) secret_encryptor: Option<Arc<fermi_auth::SecretEncryptor>>,
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
pub(crate) struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(rename = "generationConfig")]
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
pub(crate) struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
pub(crate) struct GeminiPart {
    text: String,
}

#[derive(Serialize)]
pub(crate) struct GeminiGenerationConfig {
    #[serde(rename = "responseModalities")]
    response_modalities: Vec<String>,
}

#[derive(Deserialize)]
pub(crate) struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
pub(crate) struct GeminiCandidate {
    content: GeminiContentResponse,
}

#[derive(Deserialize)]
pub(crate) struct GeminiContentResponse {
    parts: Vec<GeminiPartResponse>,
}

#[derive(Deserialize)]
pub(crate) struct GeminiPartResponse {
    #[serde(rename = "inlineData")]
    inline_data: Option<GeminiInlineData>,
}

#[derive(Deserialize)]
pub(crate) struct GeminiInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

/// Run SQL migration files on startup (idempotent — uses IF NOT EXISTS).
async fn run_migrations(db: &PgPool) {
    let migration_files = [
        "migrations/004_add_users_table.sql",
        "migrations/004_migrate_users_for_auth.sql",
        "migrations/005_add_api_keys.sql",
        "migrations/006_add_user_id_to_agents.sql",
        "migrations/007_add_user_id_to_memory.sql",
        "migrations/008_add_siwe_nonces.sql",
        "migrations/009_add_teams_and_sharing.sql",
        "migrations/010_add_adm_tables_and_dreaming.sql",
        "migrations/011_agent_crud_and_education.sql",
        "migrations/012_credit_ledger.sql",
        "migrations/013_workspace_fields.sql",
        "migrations/014_workspace_messages.sql",
        "migrations/015_workspace_agents.sql",
        "migrations/016_coherence_evaluations.sql",
        "migrations/017_workspace_git.sql",
        "migrations/018_agent_aliases.sql",
        "migrations/019_agent_provider_fields.sql",
        "migrations/020_stripe_and_profile.sql",
        "migrations/021_notifications.sql",
        "migrations/022_sample_queries.sql",
        "migrations/023_waitlist.sql",
        "migrations/024_agent_versions.sql",
        "migrations/025_agent_lifecycle.sql",
        "migrations/026_fork_royalty_tx_type.sql",
        "migrations/027_eval_framework.sql",
        "migrations/028_episode_tags.sql",
        "migrations/029_fix_message_type_and_profile.sql",
        "migrations/030_shopping_marketplace.sql",
        "migrations/031_waitlist_status.sql",
        "migrations/032_fix_tx_type_constraint.sql",
        "migrations/033_backfill_team_owners.sql",
        "migrations/034_xaman_ek_system_ontology.sql",
        "migrations/035_fix_tx_type_constraint.sql",
        "migrations/036_workspace_workflow.sql",
        "migrations/037_agent_valence_and_workflow_template.sql",
        "migrations/038_prompt_template.sql",
        "migrations/039_user_secrets.sql",
        "migrations/040_agent_requires_secrets.sql",
        "migrations/041_ar_beacons.sql",
        "migrations/042_rabble_creatures.sql",
        "migrations/043_seed_starter_creatures.sql",
        "migrations/044_rabble_messages.sql",
        "migrations/045_rabble_funding.sql",
        "migrations/046_rabble_visibility.sql",
        "migrations/047_flight_path_samples.sql",
        "migrations/048_voice_assets.sql",
        "migrations/050_fix_tx_type_constraint_rabble.sql",
        "migrations/051_swarm_telemetry.sql",
        "migrations/052_sosa_observations.sql",
        "migrations/053_creature_image_storage.sql",
        "migrations/054_creature_management.sql",
        "migrations/055_contacts.sql",
        "migrations/056_devices.sql",
        "migrations/057_rabble_workspaces.sql",
        "migrations/058_creature_presence.sql",
        "migrations/059_agent_wallet_admin.sql",
        "migrations/060_fix_object_shares_rabble.sql",
        "migrations/061_swarm_algorithms.sql",
        "migrations/062_anchor_creature.sql",
        "migrations/063_sub_flocks.sql",
        "migrations/064_creature_animation_layers.sql",
        "migrations/065_creature_visibility.sql",
        "migrations/066_wallet_balance_split.sql",
        "migrations/067_flight_environment.sql",
        "migrations/068_flight_data_source.sql",
        "migrations/069_one_active_flight.sql",
        "migrations/070_cleanup_stale_flights.sql",
        "migrations/071_add_flight_plan_tx_type.sql",
        "migrations/072_perch_model.sql",
        "migrations/073_walk_in_budget.sql",
        "migrations/074_creature_tethers.sql",
        "migrations/075_fix_tx_type_constraint.sql",
        "migrations/076_drop_tx_type_constraint.sql",
        "migrations/077_expand_message_type_constraint.sql",
        "migrations/078_creature_versioned_state.sql",
        "migrations/079_conditions_presence.sql",
        "migrations/080_drop_redundant_creature_columns.sql",
        "migrations/081_fix_visibility_contacts.sql",
        "migrations/082_rabble_radius.sql",
        "migrations/083_genome_profile_cache.sql",
        "migrations/084_drop_creature_state_rabble_fk.sql",
        "migrations/085_rename_creature_states.sql",
        "migrations/086_creature_flights_metadata.sql",
        "migrations/087_creature_favourites.sql",
        "migrations/088_backfill_creature_versions.sql",
        "migrations/089_dashboard_spatial_queries.sql",
        "migrations/090_social_layer.sql",
        "migrations/093_users_user_id_unique.sql",
        "migrations/091_swarm_participants.sql",
        "migrations/092_fix_social_layer.sql",
        "migrations/094_fermi_forecasting.sql",
        "migrations/099_polymarket_observations.sql",
        "migrations/100_cognition_tier.sql",
        "migrations/101_model_ladder.sql",
        "migrations/102_cognition_tier_nullable.sql",
        "migrations/103_observability_foundations.sql",
        "migrations/104_cep_kg_columns.sql",
        "migrations/105_cep_fermi_contract.sql",
        "migrations/106_model_params.sql",
        "migrations/107_fermi_tables_catchup.sql",
        "migrations/109_forecast_agent_schedules.sql",
        // Social Agent Observability tables (Phases 2–4) — were shipped
        // in commit b234722 alongside the handler/store code but never
        // registered, so the eval_signals / timeline / dyads / anomaly /
        // hitl_actions tables don't exist in prod and every observatory
        // query 500s. Adding here in dependency order.
        "migrations/104_evaluator_signals.sql",
        "migrations/105_longitudinal_observability.sql",
        "migrations/106_hitl_actions.sql",
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
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .test_before_acquire(true)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                // Reset connection state — clears any aborted transaction from PgBouncer
                sqlx::Executor::execute(conn, "DISCARD ALL")
                    .await
                    .map(|_| ())
            })
        })
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database");

    println!("Connected to database successfully");

    // Run pending migrations on startup
    run_migrations(&db).await;

    // Initialize ADM memory store — reuse the same pool (single pool to Neon)
    let memory_store = Arc::new(MemoryStore::from_pool(db.clone()));
    println!("ADM MemoryStore initialized (shared pool)");

    // Initialize agent registry — prefer MultiModelExecutor, fall back to LLMExecutor, then Mock
    let registry = if let Ok(multi) = MultiModelExecutor::from_env() {
        println!("Using Multi-Model Executor (Claude + additional providers)");
        Arc::new(AgentRegistry::with_executor(Arc::new(multi)))
    } else if let Ok(llm_executor) = LLMExecutor::from_env() {
        println!("Using LLM Executor (Claude API only)");
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

    let stripe_config = StripeConfig::from_env();
    if stripe_config.is_configured() {
        println!("Stripe configured (credit purchases enabled)");
    } else {
        eprintln!("Note: STRIPE_SECRET_KEY not set. Credit purchases will be disabled.");
    }

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
        stripe: stripe_config,
        rate_limits: RateLimitConfig::from_env(),
        ws_broadcast: broadcast::channel::<WorkspaceEvent>(256).0,
        rabble_broadcast: broadcast::channel::<RabbleEvent>(256).0,
        creature_broadcast: broadcast::channel::<CreatureEvent>(512).0,
        secret_encryptor: fermi_auth::SecretEncryptor::from_env().ok().map(Arc::new),
    };

    if state.secret_encryptor.is_some() {
        println!("Secrets encryption configured");
    } else {
        eprintln!("Note: SECRETS_ENCRYPTION_KEY not set. User secrets will be disabled.");
    }

    // Spawn rate limiter cleanup task (every 5 min)
    let rl_clone = state.rate_limits.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            rl_clone.public.cleanup();
            rl_clone.authed.cleanup();
            rl_clone.llm.cleanup();
        }
    });

    let auth_state = AuthState { jwt_secret, db };

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/", get(handlers::pages::landing))
        .route("/aspiration", get(handlers::pages::aspiration))
        .route("/catalogue", get(handlers::pages::catalogue))
        .route("/docs", get(handlers::pages::docs_view))
        .route("/docs/:slug", get(handlers::pages::docs_view))
        .route("/agent/:agent_id", get(handlers::pages::agent_detail))
        .route(
            "/agent/:agent_id/ontology",
            get(handlers::pages::ontology_view),
        )
        .route("/api/health", get(handlers::misc::health))
        .route("/api/debug/startup", get(handlers::misc::debug_startup))
        .route("/api/geocode", get(handlers::misc::geocode_search_handler))
        // Public forecast discovery (no auth required)
        .route(
            "/api/forecasts/public",
            get(handlers::forecasts::public_forecasts_handler),
        )
        // Per-agent MCP endpoints
        .route(
            "/mcp/agents/:agent_id",
            get(handlers::mcp::mcp_agent_manifest),
        )
        .route("/mcp/agents/:agent_id", post(handlers::mcp::mcp_agent_rpc))
        .route("/api/agents", get(handlers::agents::list_agents))
        .route(
            "/api/agents/curated",
            get(handlers::agents::list_curated_agents_handler),
        )
        .route("/api/waitlist", post(handlers::misc::waitlist_handler))
        .route(
            "/api/models/catalogue",
            get(handlers::agents::model_catalogue_handler),
        )
        .route(
            "/api/agents/:agent_id/avatar",
            get(handlers::agents::get_cached_avatar),
        )
        .route(
            "/api/agents/:agent_id/episodes",
            get(handlers::consolidation::get_agent_episodes_handler),
        )
        .route(
            "/api/episodes/:episode_id",
            get(handlers::metrics::get_episode_detail_handler),
        )
        .route(
            "/api/metrics/platform",
            get(handlers::metrics::platform_metrics_handler),
        )
        .route(
            "/api/agents/:agent_id/metrics",
            get(handlers::metrics::agent_metrics_handler),
        )
        // Eval framework
        .route(
            "/api/agents/:agent_id/eval/test-cases",
            get(handlers::eval::list_eval_test_cases_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/test-cases",
            post(handlers::eval::create_eval_test_case_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/test-cases/:test_case_id",
            put(handlers::eval::update_eval_test_case_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/test-cases/:test_case_id",
            delete(handlers::eval::delete_eval_test_case_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/run",
            post(handlers::eval::trigger_eval_run_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/runs",
            get(handlers::eval::list_eval_runs_handler),
        )
        // ─── Phase 4 — Observatory (Plane D) ─────────────────────────
        // See docs/architecture/OBSERVABILITY_IMPL.md
        .route(
            "/api/observatory/agents/:agent_id/timeline",
            get(handlers::observatory::get_agent_timeline_handler),
        )
        .route(
            "/api/observatory/agents/:agent_id/dyads",
            get(handlers::observatory::list_agent_dyads_handler),
        )
        .route(
            "/api/observatory/agents/:agent_id/anomalies",
            get(handlers::observatory::list_agent_anomalies_handler),
        )
        .route(
            "/api/observatory/agents/:agent_id/scan",
            post(handlers::observatory::trigger_agent_scan_handler),
        )
        .route(
            "/api/observatory/hitl",
            get(handlers::observatory::list_hitl_queue_handler),
        )
        .route(
            "/api/observatory/hitl/:event_id/action",
            post(handlers::observatory::record_hitl_action_handler),
        )
        .route(
            "/api/agents/:agent_id/dependencies",
            get(handlers::agents::get_agent_dependencies_handler),
        )
        .route(
            "/api/agents/:agent_id/versions",
            get(handlers::agents::list_agent_versions_handler),
        )
        .route(
            "/api/agents/:agent_id/versions/:version_num",
            get(handlers::agents::get_agent_version_handler),
        )
        .route(
            "/api/agents/:agent_id/ontology",
            get(handlers::ontology::get_ontology),
        )
        // Ontology evolution (public, read-only)
        .route(
            "/api/agents/:agent_id/ontology/history",
            get(handlers::ontology::get_ontology_history),
        )
        .route(
            "/api/agents/:agent_id/ontology/snapshots/:snapshot_id",
            get(handlers::ontology::get_ontology_snapshot),
        )
        .route(
            "/api/agents/:agent_id/ontology/diff",
            get(handlers::ontology::get_ontology_diff),
        )
        // Knowledge graph
        .route(
            "/api/agents/:agent_id/kg",
            get(handlers::kg::kg_overview_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/entities",
            get(handlers::kg::list_entities_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/entities/:entity_id",
            get(handlers::kg::get_entity_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/entities/:entity_id/facts",
            get(handlers::kg::get_entity_facts_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/facts",
            get(handlers::kg::list_facts_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/rules",
            get(handlers::kg::list_rules_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/rules/:rule_id",
            get(handlers::kg::get_rule_handler),
        )
        .route(
            "/api/agents/:agent_id/kg/communities",
            get(handlers::kg::list_communities_handler),
        )
        // Agent Wallet Admin
        .route(
            "/api/agents/:agent_id/wallet",
            get(handlers::agent_wallet::get_agent_wallet_handler),
        )
        .route(
            "/api/agents/:agent_id/earnings",
            get(handlers::agent_wallet::get_agent_earnings_handler),
        )
        .route(
            "/api/agents/:agent_id/collect",
            post(handlers::agent_wallet::collect_handler),
        )
        .route(
            "/api/agents/:agent_id/allocate",
            post(handlers::agent_wallet::allocate_handler),
        )
        .route(
            "/api/agents/:agent_id/auto-collect",
            put(handlers::agent_wallet::set_auto_collect_handler),
        )
        // Projector
        .route("/projector", get(handlers::pages::projector_view))
        .route(
            "/agent/:agent_id/projector",
            get(handlers::pages::projector_view),
        )
        .route(
            "/api/agents/:agent_id/projections",
            get(handlers::ontology::get_agent_projections),
        )
        .route(
            "/api/projections/bestiary",
            get(handlers::ontology::get_bestiary_projections),
        )
        .route(
            "/api/agents/:agent_id/projections/temporal",
            get(handlers::ontology::get_temporal_projections),
        )
        // AR Beacons (public, read-only)
        .route(
            "/api/beacons/nearby",
            get(handlers::beacons::nearby_beacons_handler),
        )
        .route(
            "/api/beacons/:beacon_id",
            get(handlers::beacons::get_beacon_handler),
        )
        .route(
            "/api/beacons/:beacon_id/asset",
            get(handlers::beacons::beacon_asset_handler),
        )
        .route(
            "/api/grid-maps/:map_id",
            get(handlers::beacons::get_grid_map_handler),
        )
        // Rabble.world creatures (public read)
        .route(
            "/api/creatures",
            get(handlers::creatures::list_creatures_handler),
        )
        .route(
            "/api/creatures/:creature_id",
            get(handlers::creatures::get_creature_handler),
        )
        .route(
            "/api/creatures/:creature_id/flights",
            get(handlers::creatures::creature_flights_handler),
        )
        .route(
            "/api/creatures/:creature_id/versions",
            get(handlers::creatures::creature_versions_handler),
        )
        .route(
            "/api/creatures/:creature_id/image",
            get(handlers::creatures::creature_image_handler),
        )
        .route(
            "/api/creatures/:creature_id/animation/:layer_name",
            get(handlers::creatures::creature_animation_layer_handler),
        )
        .route(
            "/api/creatures/:creature_id/animation-status",
            get(handlers::creatures::creature_animation_status_handler),
        )
        .route(
            "/api/creatures/:creature_id/flight-path/:flight_id",
            get(handlers::creatures::creature_flight_path_handler),
        )
        .route(
            "/api/creatures/:creature_id/cognition",
            get(handlers::creatures::creature_cognition_handler),
        )
        .route("/api/swarms", get(handlers::creatures::list_swarms_handler))
        .route(
            "/api/swarms/:swarm_id",
            get(handlers::creatures::get_swarm_handler),
        )
        // Rabble QR (public, no auth)
        .route(
            "/api/rabble/:id/qr",
            get(handlers::qr_codes::rabble_qr_handler),
        )
        // User profiles (public, no auth)
        .route("/user/:user_id", get(handlers::users::user_profile_view))
        .route(
            "/api/users/:user_id",
            get(handlers::users::get_public_profile_handler),
        )
        // Page routes (serve HTML templates)
        .route("/dashboard", get(handlers::pages::dashboard_view))
        .route("/agents/new", get(handlers::pages::agent_create_view))
        .route(
            "/workspace/:workspace_id",
            get(handlers::pages::workspace_view),
        )
        // Auth flow routes
        .route("/auth/google", get(handlers::auth::auth_google))
        .route("/auth/github", get(handlers::auth::auth_github))
        .route("/auth/callback", get(handlers::auth::auth_callback))
        .route("/auth/logout", post(handlers::auth::auth_logout))
        // Stripe webhook (no auth — Stripe calls this directly)
        .route(
            "/webhooks/stripe",
            post(handlers::billing::stripe_webhook_handler),
        )
        // SIWE wallet auth
        .route(
            "/auth/siwe/challenge",
            post(handlers::auth::siwe_challenge_handler),
        )
        .route(
            "/auth/siwe/verify",
            post(handlers::auth::siwe_verify_handler),
        )
        // Profile + Settings pages
        .route("/profile", get(handlers::profile::profile_view))
        .route("/settings", get(handlers::pages::settings_view))
        .route("/marketplace", get(handlers::pages::marketplace_view))
        .route("/admin", get(handlers::pages::admin_view))
        // Phase 4 — Observatory pages
        .route("/observatory", get(handlers::pages::observatory_view))
        .route(
            "/observatory/hitl",
            get(handlers::pages::observatory_hitl_view),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            optional_auth_middleware,
        ));

    // Protected routes (require auth)
    let protected_routes = Router::new()
        .route("/api/auth/me", get(handlers::auth::auth_me))
        .route("/api/auth/api-keys", get(handlers::auth::list_api_keys))
        .route("/api/auth/api-keys", post(handlers::auth::create_api_key))
        .route(
            "/api/auth/api-keys/:key_id",
            delete(handlers::auth::revoke_api_key),
        )
        // Team routes
        .route("/api/teams", post(handlers::teams::create_team_handler))
        .route("/api/teams", get(handlers::teams::list_teams_handler))
        .route(
            "/api/teams/:team_id",
            get(handlers::teams::get_team_handler),
        )
        .route(
            "/api/teams/:team_id",
            delete(handlers::teams::delete_team_handler),
        )
        .route(
            "/api/teams/:team_id/members",
            post(handlers::teams::add_member_handler),
        )
        .route(
            "/api/teams/:team_id/members",
            get(handlers::teams::list_members_handler),
        )
        .route(
            "/api/teams/:team_id/members/:member_id",
            delete(handlers::teams::remove_member_handler),
        )
        .route(
            "/api/teams/:team_id/members/:member_id",
            put(handlers::teams::update_member_role_handler),
        )
        // Agent creation wizard helpers
        .route(
            "/api/ontology-templates",
            get(handlers::wizard::list_ontology_templates_handler),
        )
        .route(
            "/api/agents/generate-ontology",
            post(handlers::wizard::generate_ontology_handler),
        )
        .route(
            "/api/agents/generate-prompt",
            post(handlers::wizard::generate_prompt_handler),
        )
        .route(
            "/api/agents/creation-guide",
            get(handlers::wizard::creation_guide_handler),
        )
        .route(
            "/api/tags/popular",
            get(handlers::wizard::popular_tags_handler),
        )
        // Agent CRUD
        .route("/api/agents", post(handlers::agents::create_agent_handler))
        .route(
            "/api/agents/import",
            post(handlers::agents::import_agent_handler),
        )
        .route(
            "/api/agents/mine",
            get(handlers::agents::list_my_agents_handler),
        )
        .route(
            "/api/agents/:agent_id",
            put(handlers::agents::update_agent_handler),
        )
        .route(
            "/api/agents/:agent_id",
            delete(handlers::agents::delete_agent_handler),
        )
        // Agent lifecycle (fork, publish, archive, restore)
        .route(
            "/api/agents/:agent_id/fork",
            post(handlers::lifecycle::fork_agent_handler),
        )
        .route(
            "/api/agents/:agent_id/fork-pricing",
            put(handlers::lifecycle::update_fork_pricing_handler),
        )
        .route(
            "/api/agents/:agent_id/publish-checks",
            get(handlers::lifecycle::publish_checks_handler),
        )
        .route(
            "/api/agents/:agent_id/publish",
            post(handlers::lifecycle::publish_agent_handler),
        )
        .route(
            "/api/agents/:agent_id/archive",
            post(handlers::lifecycle::archive_agent_handler),
        )
        .route(
            "/api/agents/:agent_id/restore",
            post(handlers::lifecycle::restore_agent_handler),
        )
        .route(
            "/api/agents/:agent_id/versions/:version_num/restore",
            post(handlers::agents::restore_agent_version_handler),
        )
        // Agent avatar generation (credit-gated)
        .route(
            "/api/agents/:agent_id/avatar/generate",
            post(handlers::agents::generate_avatar),
        )
        // Custom embeddings import
        .route(
            "/api/agents/:agent_id/embeddings/import",
            post(handlers::agents::import_embeddings_handler),
        )
        // Agent execution
        .route(
            "/api/agents/:agent_id/execute",
            post(handlers::execution::execute_agent_handler),
        )
        // Streaming execution (SSE — real-time progress events)
        .route(
            "/api/agents/:agent_id/execute/stream",
            post(handlers::execution_stream::execute_agent_stream_handler),
        )
        // Dreaming budget
        .route(
            "/api/agents/:agent_id/dreaming/budget",
            get(handlers::consolidation::get_dreaming_budget),
        )
        .route(
            "/api/agents/:agent_id/dreaming/budget",
            put(handlers::consolidation::set_dreaming_budget),
        )
        .route(
            "/api/agents/:agent_id/dreaming/topup",
            post(handlers::consolidation::topup_dreaming_budget_handler),
        )
        // Consolidation trigger
        .route(
            "/api/agents/:agent_id/consolidate",
            post(handlers::consolidation::consolidate_agent_handler),
        )
        // Workspace routes
        .route(
            "/api/workspaces",
            get(handlers::workspace::list_workspaces_handler),
        )
        .route(
            "/api/workspaces/:workspace_id",
            get(handlers::workspace::get_workspace_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/agents",
            get(handlers::workspace::list_workspace_agents_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/agents",
            post(handlers::workspace::create_workspace_agent_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/budget",
            post(handlers::workspace::fund_workspace_handler),
        )
        // Workspace chat
        .route(
            "/api/workspaces/:workspace_id/messages",
            post(handlers::workspace::post_workspace_message_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/messages",
            get(handlers::workspace::get_workspace_messages_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/messages/poll",
            get(handlers::workspace::poll_workspace_messages_handler),
        )
        // SSE stream (replaces polling for browser clients)
        .route(
            "/api/workspaces/:workspace_id/messages/stream",
            get(handlers::workspace::workspace_messages_stream_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/workflow",
            get(handlers::workspace::get_workspace_workflow_handler),
        )
        // Workspace agent hire/add
        .route(
            "/api/workspaces/:workspace_id/hire",
            post(handlers::workspace::hire_agent_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/add",
            post(handlers::workspace::add_agent_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/agents/:agent_id",
            delete(handlers::workspace::remove_workspace_agent_handler),
        )
        // Wallet / credits
        .route("/api/wallet", get(handlers::wallet::get_wallet_handler))
        .route(
            "/api/wallet/transactions",
            get(handlers::wallet::get_transactions_handler),
        )
        .route(
            "/api/wallet/transfer",
            post(handlers::wallet::transfer_credits_handler),
        )
        // Billing (Stripe)
        .route(
            "/api/billing/tiers",
            get(handlers::billing::billing_tiers_handler),
        )
        .route(
            "/api/billing/checkout",
            post(handlers::billing::billing_checkout_handler),
        )
        .route(
            "/api/billing/dev-topup",
            post(handlers::billing::billing_dev_topup_handler),
        )
        // User discovery
        .route(
            "/api/users/search",
            get(handlers::users::search_users_handler),
        )
        .route(
            "/api/users/collaborators",
            get(handlers::users::get_collaborators_handler),
        )
        // Personal workspace (menagerie)
        .route(
            "/api/me/workspace",
            get(handlers::rabble_workspace::get_personal_workspace_handler),
        )
        // Profile
        .route("/api/profile", get(handlers::profile::get_profile_handler))
        .route(
            "/api/profile",
            put(handlers::profile::update_profile_handler),
        )
        // Notifications
        .route(
            "/api/notifications",
            get(handlers::profile::list_notifications_handler),
        )
        .route(
            "/api/notifications/:id/read",
            put(handlers::profile::mark_notification_read_handler),
        )
        .route(
            "/api/notifications/read-all",
            put(handlers::profile::mark_all_notifications_read_handler),
        )
        // User secrets (connections)
        .route(
            "/api/secrets",
            post(handlers::profile::create_secret_handler),
        )
        .route("/api/secrets", get(handlers::profile::list_secrets_handler))
        .route(
            "/api/secrets/audit",
            get(handlers::profile::secret_audit_handler),
        )
        .route(
            "/api/secrets/:name",
            delete(handlers::profile::delete_secret_handler),
        )
        // Coherence evaluation
        .route(
            "/api/workspaces/:workspace_id/coherence/evaluate",
            post(handlers::workspace::evaluate_coherence_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/coherence",
            get(handlers::workspace::get_coherence_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/coherence/history",
            get(handlers::workspace::get_coherence_history_handler),
        )
        // Workspace ontology (merged)
        .route(
            "/api/workspaces/:workspace_id/ontology",
            get(handlers::workspace::get_workspace_ontology_handler),
        )
        // Workspace git / files
        .route(
            "/api/workspaces/:workspace_id/files",
            get(handlers::workspace::list_workspace_files_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/files/*path",
            get(handlers::workspace::read_workspace_file_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/files/*path",
            put(handlers::workspace::write_workspace_file_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/files-raw/*path",
            get(handlers::workspace::read_workspace_file_raw_handler),
        )
        // File upload (multipart, 6MB limit to allow multipart overhead)
        .route(
            "/api/workspaces/:workspace_id/upload",
            post(handlers::workspace::upload_workspace_file_handler)
                .layer(axum::extract::DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route(
            "/api/workspaces/:workspace_id/git/log",
            get(handlers::workspace::workspace_git_log_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/git/diff",
            get(handlers::workspace::workspace_git_diff_handler),
        )
        // Fermi Notebook routes
        .route(
            "/api/notebooks",
            get(handlers::notebooks::list_notebooks_handler),
        )
        .route(
            "/api/notebooks",
            post(handlers::notebooks::create_notebook_handler),
        )
        .route(
            "/api/notebooks/:notebook_id",
            get(handlers::notebooks::get_notebook_handler),
        )
        .route(
            "/api/notebooks/:notebook_id",
            put(handlers::notebooks::update_notebook_handler),
        )
        .route(
            "/api/notebooks/:notebook_id",
            delete(handlers::notebooks::delete_notebook_handler),
        )
        .route(
            "/api/notebooks/:notebook_id/execute",
            post(handlers::notebooks::execute_notebook_handler),
        )
        // ── Forecast routes ────────────────────────────────────────────
        .route(
            "/api/forecasts",
            post(handlers::forecasts::create_forecast_handler),
        )
        .route(
            "/api/forecasts",
            get(handlers::forecasts::list_forecasts_handler),
        )
        .route(
            "/api/forecasts/my-stats",
            get(handlers::forecasts::my_stats_handler),
        )
        .route(
            "/api/forecasts/:forecast_id",
            get(handlers::forecasts::get_forecast_handler),
        )
        .route(
            "/api/forecasts/:forecast_id",
            put(handlers::forecasts::update_forecast_handler),
        )
        .route(
            "/api/forecasts/:forecast_id",
            delete(handlers::forecasts::delete_forecast_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/resolve",
            post(handlers::forecasts::resolve_forecast_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/void",
            post(handlers::forecasts::void_forecast_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/update-probability",
            post(handlers::forecasts::update_probability_handler),
        )
        // ── Schedule routes ────────────────────────────────────────────
        .route(
            "/api/forecasts/:forecast_id/schedules",
            get(handlers::forecasts::list_forecast_schedules_handler)
                .put(handlers::forecasts::upsert_forecast_schedule_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/schedules/:schedule_id",
            delete(handlers::forecasts::delete_forecast_schedule_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/schedules/:schedule_id/run",
            post(handlers::forecasts::record_schedule_run_handler),
        )
        // ── Portfolio routes ───────────────────────────────────────────
        .route(
            "/api/portfolios",
            post(handlers::forecasts::create_portfolio_handler),
        )
        .route(
            "/api/portfolios",
            get(handlers::forecasts::list_portfolios_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id/stats",
            get(handlers::forecasts::portfolio_stats_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id",
            delete(handlers::forecasts::delete_portfolio_handler)
                .patch(handlers::forecasts::patch_portfolio_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id/forecasts",
            get(handlers::forecasts::list_portfolio_forecasts_handler)
                .post(handlers::forecasts::add_forecast_to_portfolio_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id/forecasts/:forecast_id",
            delete(handlers::forecasts::remove_forecast_from_portfolio_handler),
        )
        // ── Leaderboard routes ─────────────────────────────────────────
        .route(
            "/api/leaderboard",
            get(handlers::forecasts::leaderboard_handler),
        )
        // Sharing routes
        .route("/api/shares", post(handlers::teams::share_object_handler))
        .route(
            "/api/shares/:share_id",
            delete(handlers::teams::revoke_share_handler),
        )
        // ── Polymarket integration routes ──────────────────────────────
        .route(
            "/api/polymarket/search",
            post(handlers::polymarket::search_handler),
        )
        .route(
            "/api/polymarket/snapshot",
            post(handlers::polymarket::snapshot_handler),
        )
        .route(
            "/api/polymarket/link",
            post(handlers::polymarket::link_handler),
        )
        .route(
            "/api/polymarket/import",
            post(handlers::polymarket::import_handler),
        )
        .route(
            "/api/polymarket/observations",
            get(handlers::polymarket::observations_handler),
        )
        .route(
            "/api/polymarket/check-resolutions",
            post(handlers::polymarket::check_resolutions_handler),
        )
        // Admin routes (handlers check can_admin())
        .route(
            "/api/admin/stats",
            get(handlers::admin::admin_stats_handler),
        )
        .route(
            "/api/admin/users",
            get(handlers::admin::admin_list_users_handler),
        )
        .route(
            "/api/admin/users/:user_id/grant",
            post(handlers::admin::admin_grant_credits_handler),
        )
        .route(
            "/api/admin/agents",
            get(handlers::admin::admin_list_agents_handler),
        )
        .route(
            "/api/admin/agents/:agent_id/flag",
            put(handlers::admin::admin_flag_agent_handler),
        )
        .route(
            "/api/admin/waitlist",
            get(handlers::admin::admin_list_waitlist_handler)
                .post(handlers::admin::admin_add_waitlist_handler),
        )
        .route(
            "/api/admin/waitlist/invite",
            post(handlers::admin::admin_invite_waitlist_handler),
        )
        .route(
            "/api/admin/waitlist/:entry_id",
            delete(handlers::admin::admin_delete_waitlist_handler),
        )
        // Admin Rabble routes
        .route(
            "/api/admin/creatures",
            get(handlers::admin::admin_list_creatures_handler),
        )
        .route(
            "/api/admin/creatures/:creature_id/flag",
            put(handlers::admin::admin_flag_creature_handler),
        )
        .route(
            "/api/admin/swarms",
            get(handlers::admin::admin_list_swarms_handler),
        )
        .route(
            "/api/admin/swarms/:swarm_id/status",
            put(handlers::admin::admin_update_swarm_status_handler),
        )
        // Marketplace routes
        .route(
            "/api/marketplace/match",
            post(handlers::marketplace::marketplace_match_handler),
        )
        .route(
            "/api/marketplace/listings",
            get(handlers::marketplace::list_marketplace_listings_handler),
        )
        .route(
            "/api/marketplace/listings",
            post(handlers::marketplace::create_marketplace_listing_handler),
        )
        .route(
            "/api/marketplace/history",
            get(handlers::marketplace::marketplace_history_handler),
        )
        .route(
            "/api/shopping/profile",
            get(handlers::marketplace::get_shopping_profiles_handler),
        )
        .route(
            "/api/shopping/profile/:listing_id/listing",
            put(handlers::marketplace::update_listing_handler),
        )
        // Rabble.world (authenticated)
        .route(
            "/api/collections",
            get(handlers::creatures::list_collections_handler)
                .post(handlers::creatures::create_collection_handler),
        )
        .route(
            "/api/collections/:collection_id",
            put(handlers::creatures::update_collection_handler),
        )
        .route(
            "/api/flights",
            post(handlers::creatures::record_flight_handler),
        )
        .route(
            "/api/flights/:flight_id/end",
            put(handlers::creatures::end_flight_handler),
        )
        .route(
            "/api/flights/:flight_id/telemetry",
            post(handlers::creatures::append_telemetry_handler),
        )
        .route(
            "/api/flights/:flight_id/export",
            get(handlers::creatures::export_flight_handler),
        )
        .route(
            "/api/flights/import",
            post(handlers::creatures::import_flight_handler),
        )
        // Creature SSE stream (real-time push of lifecycle events)
        .route(
            "/api/creatures/:creature_id/stream",
            get(handlers::streams::creature_stream_handler),
        )
        // Perch + Fly model (replaces plan_flight)
        .route(
            "/api/creatures/:creature_id/perch",
            post(handlers::creatures::perch_handler),
        )
        .route(
            "/api/creatures/:creature_id/host",
            post(handlers::creatures::host_rabble_handler),
        )
        .route(
            "/api/creatures/:creature_id/fly",
            post(handlers::creatures::fly_handler),
        )
        // Tethering — link creature to live GPS/sensor
        .route(
            "/api/creatures/:creature_id/tether",
            post(handlers::creatures::tether_handler).delete(handlers::creatures::untether_handler),
        )
        // Enemy sensor — enable/disable/check natural predators
        .route(
            "/api/creatures/:creature_id/enemy-sensor",
            post(handlers::creatures::enemy_sensor_handler),
        )
        // Genome profiler — enable/disable/check phylogenetic context
        .route(
            "/api/creatures/:creature_id/genome-profiler",
            post(handlers::creatures::genome_profiler_handler),
        )
        // Prey locator — premium hunting: scan + stalk with flight plan
        .route(
            "/api/creatures/:creature_id/prey-locator",
            post(handlers::creatures::prey_locator_handler),
        )
        .route(
            "/api/creatures/:creature_id/telemetry",
            post(handlers::creatures::push_telemetry_handler),
        )
        .route(
            "/api/creatures/:creature_id/track",
            get(handlers::creatures::get_track_handler),
        )
        // Creature minting
        .route(
            "/api/creatures/mint",
            post(handlers::creatures::mint_creature_handler),
        )
        .route(
            "/api/my/rabbles",
            get(handlers::creatures::my_rabbles_handler),
        )
        .route(
            "/api/swarms/create",
            post(handlers::creatures::create_swarm_handler),
        )
        .route(
            "/api/swarms/:swarm_id/join",
            post(handlers::creatures::join_swarm_handler),
        )
        .route(
            "/api/swarms/:swarm_id",
            patch(handlers::creatures::update_swarm_handler),
        )
        // Activity feed
        .route("/api/feed", get(handlers::creatures::feed_handler))
        // Creature favourites
        // Dashboard endpoints
        .route(
            "/api/dashboard/my-rabbles",
            get(handlers::dashboard::my_rabbles_handler),
        )
        .route(
            "/api/dashboard/nearby",
            get(handlers::dashboard::nearby_rabbles_handler),
        )
        .route(
            "/api/dashboard/creatures",
            get(handlers::dashboard::creatures_handler),
        )
        .route(
            "/api/dashboard/boundary-violations",
            get(handlers::dashboard::boundary_violations_handler),
        )
        .route(
            "/api/dashboard/nearby-creatures",
            get(handlers::dashboard::nearby_creatures_handler),
        )
        // Saved locations (favourite places, drop pins)
        .route(
            "/api/locations",
            get(handlers::dashboard::list_locations_handler)
                .post(handlers::dashboard::save_location_handler),
        )
        .route(
            "/api/locations/:id",
            patch(handlers::dashboard::update_location_handler)
                .delete(handlers::dashboard::delete_location_handler),
        )
        .route(
            "/api/creatures/:creature_id/favourite",
            post(handlers::creatures::favourite_creature_handler)
                .delete(handlers::creatures::unfavourite_creature_handler),
        )
        // Rabble.world art generation
        .route(
            "/api/creatures/:creature_id/generate-art",
            post(handlers::creatures::generate_art_handler),
        )
        .route(
            "/api/creatures/generate-art-batch",
            post(handlers::creatures::generate_art_batch_handler),
        )
        .route(
            "/api/creatures/:creature_id/sosa-opt-in",
            put(handlers::creatures::sosa_opt_in_handler),
        )
        // Creature CRUD (authenticated)
        .route(
            "/api/creatures/:creature_id/update",
            put(handlers::creatures::update_creature_handler),
        )
        .route(
            "/api/creatures/:creature_id/status",
            put(handlers::creatures::update_creature_status_handler),
        )
        .route(
            "/api/creatures/:creature_id/presence",
            put(handlers::creatures::update_creature_presence_handler),
        )
        .route(
            "/api/creatures/:creature_id/transfer",
            post(handlers::creatures::transfer_creature_handler),
        )
        // Wing animation (authenticated)
        .route(
            "/api/creatures/:creature_id/animate",
            post(handlers::creatures::animate_creature_handler),
        )
        // Creature level + dream (authenticated)
        .route(
            "/api/creatures/:creature_id/level",
            get(handlers::creatures::creature_level_handler),
        )
        .route(
            "/api/creatures/:creature_id/dream",
            post(handlers::creatures::creature_dream_handler),
        )
        .route(
            "/api/creatures/:creature_id/activity",
            get(handlers::creatures::creature_activity_handler),
        )
        // Creature visibility (authenticated)
        .route(
            "/api/creatures/:creature_id/visibility",
            put(handlers::creatures::update_creature_visibility_handler),
        )
        // Visible flights (authenticated — needs contact lookup)
        .route(
            "/api/flights/visible",
            get(handlers::creatures::list_visible_flights_handler),
        )
        // Device pairing (authenticated)
        .route(
            "/api/creatures/:creature_id/devices",
            get(handlers::creatures::list_devices_handler)
                .post(handlers::creatures::pair_device_handler),
        )
        .route(
            "/api/devices/:device_id",
            put(handlers::creatures::update_device_handler)
                .delete(handlers::creatures::unpair_device_handler),
        )
        .route(
            "/api/devices/:device_id/location",
            post(handlers::creatures::report_device_location_handler),
        )
        // Contacts (authenticated)
        .route(
            "/api/contacts",
            get(handlers::social::list_contacts_handler)
                .post(handlers::social::add_contact_handler),
        )
        .route(
            "/api/contacts/:contact_id",
            put(handlers::social::update_contact_handler)
                .delete(handlers::social::remove_contact_handler),
        )
        // Creature friendships (creature-to-creature, symmetric)
        .route(
            "/api/creature-friendships",
            post(handlers::social::send_friendship_request_handler),
        )
        .route(
            "/api/creature-friendships/pending",
            get(handlers::social::pending_friendships_handler),
        )
        .route(
            "/api/creature-friendships/:id/accept",
            post(handlers::social::accept_friendship_handler),
        )
        .route(
            "/api/creature-friendships/:id/decline",
            post(handlers::social::decline_friendship_handler),
        )
        .route(
            "/api/creature-friendships/:id",
            delete(handlers::social::remove_friendship_handler),
        )
        .route(
            "/api/creatures/:creature_id/friends",
            get(handlers::social::list_creature_friends_handler),
        )
        // Creature invites ("come fly with me" — creature-to-creature)
        .route(
            "/api/creature-invites",
            post(handlers::social::send_creature_invite_handler),
        )
        .route(
            "/api/creature-invites/pending",
            get(handlers::social::list_pending_creature_invites_handler),
        )
        .route(
            "/api/creature-invites/:id/accept",
            post(handlers::social::accept_creature_invite_handler),
        )
        .route(
            "/api/creature-invites/:id/decline",
            post(handlers::social::decline_creature_invite_handler),
        )
        // Rabble recap ("you met these creatures")
        .route(
            "/api/rabble/:id/recap/:creature_id",
            get(handlers::social::rabble_recap_handler),
        )
        .route(
            "/api/rabble/:id/co-presence",
            post(handlers::social::record_co_presence_handler),
        )
        // Rabble follows (Decision D3 — active notifications)
        .route(
            "/api/rabbles/:id/follow",
            post(handlers::social::follow_rabble_handler)
                .put(handlers::social::update_follow_handler)
                .delete(handlers::social::unfollow_rabble_handler),
        )
        .route(
            "/api/my/following",
            get(handlers::social::list_following_handler),
        )
        // Social visibility
        .route(
            "/api/users/social-visibility",
            put(handlers::social::update_social_visibility_handler),
        )
        // Activity feed (SSE + paginated)
        .route(
            "/api/feed/events",
            get(handlers::social::activity_feed_handler),
        )
        .route(
            "/api/feed/stream",
            get(handlers::social::activity_feed_stream_handler),
        )
        // Rabble chat (authenticated)
        .route(
            "/api/rabble/:id/messages",
            get(handlers::rabble_chat::get_rabble_messages)
                .post(handlers::rabble_chat::post_rabble_message),
        )
        .route(
            "/api/rabble/:id/stream",
            get(handlers::rabble_chat::rabble_stream),
        )
        // Rabble invite/members (private rabbles)
        .route(
            "/api/rabble/:id/invite",
            post(handlers::rabble_chat::invite_to_rabble),
        )
        .route(
            "/api/rabble/:id/invite/:user_id",
            delete(handlers::rabble_chat::revoke_rabble_invite),
        )
        .route(
            "/api/rabble/:id/members",
            get(handlers::rabble_chat::list_rabble_members),
        )
        .route(
            "/api/rabble/join/:qr_token",
            get(handlers::qr_codes::resolve_qr_token_handler)
                .post(handlers::creatures::join_by_qr_token_handler),
        )
        // Reynolds flocking
        .route(
            "/api/rabble/:id/flock",
            post(handlers::rabble_workspace::flock_tick_handler),
        )
        .route(
            "/api/rabble/:id/flock-history",
            get(handlers::rabble_workspace::flock_history_handler),
        )
        // Anchor creature management
        .route(
            "/api/rabble/:id/end",
            post(handlers::creatures::end_rabble_handler),
        )
        .route(
            "/api/rabble/:id/leave",
            post(handlers::creatures::leave_rabble_handler),
        )
        .route(
            "/api/rabble/:id/eject",
            post(handlers::governance::eject_creature_handler),
        )
        .route(
            "/api/rabble/:id/eject/:creature_id",
            delete(handlers::governance::lift_ejection_handler),
        )
        // Governance — Block + Report
        .route(
            "/api/creatures/:creature_id/block",
            post(handlers::governance::block_creature_handler),
        )
        .route(
            "/api/creatures/:creature_id/block/:blocked_creature_id",
            delete(handlers::governance::unblock_creature_handler),
        )
        .route(
            "/api/users/block",
            post(handlers::governance::block_user_handler),
        )
        .route(
            "/api/users/block/:blocked_user_id",
            delete(handlers::governance::unblock_user_handler),
        )
        .route(
            "/api/my/blocks",
            get(handlers::governance::list_blocks_handler),
        )
        .route(
            "/api/reports",
            post(handlers::governance::create_report_handler),
        )
        // Push notifications
        .route(
            "/api/push/vapid-key",
            get(handlers::push::get_vapid_key_handler),
        )
        .route(
            "/api/push/subscribe",
            post(handlers::push::subscribe_handler).delete(handlers::push::unsubscribe_handler),
        )
        .route(
            "/api/push/proximity",
            post(handlers::push::proximity_check_handler),
        )
        .route(
            "/api/rabble/:id/transfer-anchor",
            post(handlers::rabble_workspace::transfer_anchor_handler),
        )
        .route(
            "/api/rabble/:id/update-anchor-position",
            post(handlers::rabble_workspace::update_anchor_position_handler),
        )
        // Batch join + attraction
        .route(
            "/api/swarms/:swarm_id/join-batch",
            post(handlers::rabble_workspace::join_batch_handler),
        )
        .route(
            "/api/rabble/:id/attraction-leaderboard",
            get(handlers::rabble_workspace::attraction_leaderboard_handler),
        )
        // Swarm telemetry
        .route(
            "/api/swarm/sessions",
            post(handlers::swarm_telemetry::create_session_handler)
                .get(handlers::swarm_telemetry::list_sessions_handler),
        )
        .route(
            "/api/swarm/sessions/:session_id",
            get(handlers::swarm_telemetry::get_session_handler),
        )
        .route(
            "/api/swarm/sessions/:session_id/end",
            put(handlers::swarm_telemetry::end_session_handler),
        )
        .route(
            "/api/swarm/sessions/:session_id/telemetry",
            post(handlers::swarm_telemetry::ingest_telemetry_handler)
                .get(handlers::swarm_telemetry::query_telemetry_handler),
        )
        .route(
            "/api/swarm/sessions/:session_id/summary",
            get(handlers::swarm_telemetry::session_summary_handler),
        )
        .route(
            "/api/swarm/sessions/:session_id/experience",
            get(handlers::swarm_telemetry::experience_export_handler),
        )
        // Swarm algorithm marketplace
        .route(
            "/api/swarm-algorithms",
            get(handlers::swarm_algorithms::list_algorithms_handler),
        )
        .route(
            "/api/swarm-algorithms/activate",
            post(handlers::swarm_algorithms::activate_algorithm_handler),
        )
        .route(
            "/api/swarm-algorithms/activations/:swarm_id",
            get(handlers::swarm_algorithms::list_activations_handler),
        )
        .route(
            "/api/swarm-algorithms/:algorithm_id",
            get(handlers::swarm_algorithms::get_algorithm_handler),
        )
        // Universal SOSA observations
        .route(
            "/api/observe/platforms",
            post(handlers::observations::create_platform_handler)
                .get(handlers::observations::list_platforms_handler),
        )
        .route(
            "/api/observe/sessions",
            post(handlers::observations::create_observation_session_handler)
                .get(handlers::observations::list_observation_sessions_handler),
        )
        .route(
            "/api/observe/sessions/:session_id/end",
            put(handlers::observations::end_observation_session_handler),
        )
        .route(
            "/api/observe/sessions/:session_id/observations",
            post(handlers::observations::ingest_observations_handler)
                .get(handlers::observations::query_observations_handler),
        )
        .route(
            "/api/observe/sessions/:session_id/summary",
            get(handlers::observations::observation_summary_handler),
        )
        .route(
            "/api/observe/sessions/:session_id/experience",
            get(handlers::observations::observation_experience_handler),
        )
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ));

    let rabble_router = Router::new().fallback(rabble_spa_fallback);

    // Configure CORS to allow credentials from fermi.systems and kask.bio
    let cors = CorsLayer::new()
        .allow_origin([
            "https://fermi.systems".parse::<HeaderValue>().unwrap(),
            "https://kask.bio".parse::<HeaderValue>().unwrap(),
            "https://www.kask.bio".parse::<HeaderValue>().unwrap(),
            "http://localhost:5173".parse::<HeaderValue>().unwrap(), // Dev
            "http://localhost:3000".parse::<HeaderValue>().unwrap(), // Dev
        ])
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::PATCH,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::COOKIE])
        .allow_credentials(true);

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .nest_service("/static", ServeDir::new("static"))
        .route("/rabble/", get(|| async { Redirect::permanent("/rabble") }))
        .nest("/rabble", rabble_router)
        .fallback(host_aware_fallback)
        .layer(cors)
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

// ─── Host-Aware Fallback ──────────────────────────────────────────

/// Fallback handler: serves Rabble SPA for rabble.world requests, ABW 404 otherwise.
async fn host_aware_fallback(req: axum::extract::Request) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if host.starts_with("rabble.world") {
        rabble_spa_fallback(req.uri().clone()).await.into_response()
    } else {
        fallback_404().await.into_response()
    }
}

// ─── Rabble SPA Fallback ───────────────────────────────────────────

async fn rabble_spa_fallback(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path();

    // If it's a file with an extension, try to serve it from static/rabble
    if path.contains('.') {
        let file_path = format!(
            "static/rabble{}",
            path.strip_prefix("/rabble").unwrap_or(path)
        );
        if let Ok(content) = std::fs::read(&file_path) {
            let mime_type = if path.ends_with(".js") {
                "application/javascript"
            } else if path.ends_with(".wasm") {
                "application/wasm"
            } else if path.ends_with(".json") {
                "application/json"
            } else if path.ends_with(".png") {
                "image/png"
            } else if path.ends_with(".ttf") {
                "font/ttf"
            } else if path.ends_with(".otf") {
                "font/otf"
            } else {
                "application/octet-stream"
            };

            return Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime_type)
                .header(
                    "Permissions-Policy",
                    "geolocation=(self), camera=(self), microphone=()",
                )
                .body(axum::body::Body::from(content))
                .unwrap();
        }
    }

    // Otherwise, serve index.html for SPA routing
    if let Ok(html) = std::fs::read_to_string("static/rabble/index.html") {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .header(
                "Permissions-Policy",
                "geolocation=(self), camera=(self), microphone=()",
            )
            .body(axum::body::Body::from(html))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Rabble app not found"))
            .unwrap()
    }
}

// ─── Fallback (404) ────────────────────────────────────────────────

async fn fallback_404() -> (StatusCode, Html<String>) {
    let html = std::fs::read_to_string("templates/404.html")
        .unwrap_or_else(|_| "<h1>404 — Not Found</h1>".to_string());
    (StatusCode::NOT_FOUND, Html(html))
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
            system_prompt: card.system_prompt.clone(),
            visibility: "public".to_string(),
            owner_id: None,
            tags: card.metadata.tags.clone(),
            education_budget_credits: 0,
            education_credits_used: 0,
            auto_collect_pct: 0,
            display_alias: None,
            llm_provider: "anthropic".to_string(),
            embedding_provider: "anthropic".to_string(),
            embedding_model: "voyage-2".to_string(),
            embedding_dimension: 1024,
            sample_queries: card.metadata.sample_queries.clone(),
            status: "published".to_string(),
            fork_pricing: None,
            forked_from: None,
            fork_count: 0,
            accepts: card.accepts.clone(),
            produces: card.produces.clone(),
            workflow_template: card
                .workflow_template
                .as_ref()
                .and_then(|t| serde_json::to_value(t).ok()),
            prompt_template: card.prompt_template.clone(),
            requires_secrets: if card.requires_secrets.is_empty() {
                None
            } else {
                serde_json::to_value(&card.requires_secrets).ok()
            },
            model_ladder: serde_json::to_value(&card.capabilities.model_ladder)
                .unwrap_or(serde_json::Value::Array(vec![])),
            min_tier: format!("{:?}", card.capabilities.min_tier).to_lowercase(),
            capability_gates: serde_json::to_value(&card.capabilities.capability_gates)
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            persona_version: 1,
            fermi_contract: card
                .capabilities
                .fermi_contract
                .as_ref()
                .and_then(|fc| serde_json::to_value(fc).ok()),
            model_params: card.capabilities.model_params.clone(),
        };

        match memory_store.upsert_agent(agent).await {
            Ok(id) => {
                println!("Seeded agent {} → {}", card.agent_id, id);
                // Seed eval test cases from sample_queries
                if !card.metadata.sample_queries.is_empty() {
                    match memory_store
                        .seed_eval_test_cases_from_samples(id, &card.metadata.sample_queries)
                        .await
                    {
                        Ok(n) if n > 0 => {
                            println!("  Seeded {} eval test cases for {}", n, card.agent_id)
                        }
                        _ => {}
                    }
                }
                // Seed CEP knowledge-graph entities from fermi_contract.seed_facts (idempotent)
                if let Some(fc) = &card.capabilities.fermi_contract {
                    if !fc.seed_facts.is_empty() {
                        seed_cep_entities(memory_store, id, &card.agent_id, fc).await;
                    }
                }
            }
            Err(e) => eprintln!("Warning: failed to seed {}: {}", card.agent_id, e),
        }
    }
}

async fn seed_cep_entities(
    memory_store: &MemoryStore,
    agent_uuid: uuid::Uuid,
    agent_name: &str,
    fc: &fermi::agent_backend::agent_card::FermiContract,
) {
    // Check if CEP entities already exist to stay idempotent across restarts.
    let existing = memory_store
        .get_agent_entities(agent_uuid)
        .await
        .unwrap_or_default();
    let has_cep = existing
        .iter()
        .any(|e| e.entity_type.starts_with("cep_"));
    if has_cep {
        return;
    }

    let mut seeded = 0usize;
    for sf in &fc.seed_facts {
        let entity = agent_bestiary_memory::Entity {
            entity_id: uuid::Uuid::new_v4(),
            agent_id: agent_uuid,
            entity_name: sf.name.clone(),
            entity_type: sf.entity_type.clone(),
            summary: Some(sf.description.clone()),
            t_valid: chrono::Utc::now(),
            t_invalid: None,
            source_episodes: vec![],
            extraction_confidence: sf.confidence,
            embedding: None,
            properties: Some(sf.properties.clone()),
        };
        match memory_store.store_entity(entity).await {
            Ok(_) => seeded += 1,
            Err(e) => eprintln!(
                "  Warning: failed to seed CEP entity '{}' for {}: {}",
                sf.name, agent_name, e
            ),
        }
    }
    if seeded > 0 {
        println!("  Seeded {} CEP entities for {}", seeded, agent_name);
    }
}

// ─── Agent resolution helper ───────────────────────────────────────

// ─── Shared helpers ──────────────────────────────────────────────────

pub(crate) async fn resolve_agent(
    state: &AppState,
    agent_id: &str,
) -> Result<Agent, (StatusCode, String)> {
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

/// Build an AgentCard from a DB Agent record (for agents not in the filesystem registry)
pub(crate) fn agent_card_from_db(agent: &Agent) -> AgentCard {
    AgentCard {
        agent_id: agent.agent_name.clone(),
        agent_type: agent.agent_type.clone(),
        version: agent.version.clone(),
        tier: if agent.owner_id.is_some() {
            AgentTier::Community
        } else {
            AgentTier::Curated
        },
        capabilities: AgentCapabilities {
            executor: match agent.executor_type.as_str() {
                "mcp" => ast::ExecutorType::MCP,
                "manual" => ast::ExecutorType::Manual,
                "skill" => ast::ExecutorType::Skill,
                _ => ast::ExecutorType::LLM,
            },
            mcp_tools: vec![],
            skills: vec![],
            model: agent.model.clone(),
            temperature: agent.temperature,
            provider: agent.llm_provider.clone(),
            model_ladder: serde_json::from_value(agent.model_ladder.clone())
                .unwrap_or_default(),
            min_tier: match agent.min_tier.as_str() {
                "standard" => fermi::agent_backend::agent_card::CognitionTier::Standard,
                "premium" => fermi::agent_backend::agent_card::CognitionTier::Premium,
                _ => fermi::agent_backend::agent_card::CognitionTier::Free,
            },
            capability_gates: serde_json::from_value(agent.capability_gates.clone())
                .unwrap_or_default(),
            fermi_contract: agent
                .fermi_contract
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            model_params: agent.model_params.clone(),
        },
        performance: AgentPerformance {
            forecasts_contributed: 0,
            avg_brier_impact: 0.0,
            avg_confidence: 0.0,
            accuracy_rate: 0.0,
            total_queries: 0,
        },
        usage: AgentUsage {
            total_executions: agent.total_executions as u32,
            successful_executions: agent.successful_executions as u32,
            failed_executions: agent.failed_executions as u32,
            total_tokens_used: 0,
            total_cost_usd: 0.0,
            avg_execution_time_ms: agent.avg_execution_time_ms as u64,
            last_30_days: UsageWindow {
                executions: 0,
                tokens: 0,
                cost_usd: 0.0,
            },
        },
        wallet: None,
        ontology_stats: OntologyStats {
            entities: 0,
            relationships: 0,
            last_updated: chrono::Utc::now(),
            evolution_commits: 0,
        },
        metadata: CardMetadata {
            created: chrono::Utc::now().to_rfc3339(),
            author: agent.author.clone(),
            description: agent.description.clone().unwrap_or_default(),
            tags: agent.tags.clone(),
            sample_queries: agent.sample_queries.clone(),
            valence: None,
        },
        system_prompt: agent.system_prompt.clone(),
        dependencies: AgentDependencies::default(),
        accepts: agent.accepts.clone(),
        produces: agent.produces.clone(),
        workflow_template: agent
            .workflow_template
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        prompt_template: agent.prompt_template.clone(),
        requires_secrets: agent
            .requires_secrets
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
    }
}

/// Resolve an AgentCard: try registry first, fall back to building from DB agent.
/// Always overrides capabilities.provider from the DB agent's llm_provider.
pub(crate) fn resolve_agent_card(state: &AppState, db_agent: &Agent) -> AgentCard {
    let mut card = match state.registry.get(&db_agent.agent_name) {
        Ok(c) => c,
        Err(_) => agent_card_from_db(db_agent),
    };
    // Bridge DB llm_provider → card capabilities.provider
    card.capabilities.provider = db_agent.llm_provider.clone();
    // Also bridge model/temperature from DB (may have been updated via API)
    card.capabilities.model = db_agent.model.clone();
    card.capabilities.temperature = db_agent.temperature;
    // Bridge system prompt from DB
    card.system_prompt = db_agent.system_prompt.clone();
    // Bridge valence from DB (may have been updated via API)
    if !db_agent.accepts.is_empty() {
        card.accepts = db_agent.accepts.clone();
    }
    if !db_agent.produces.is_empty() {
        card.produces = db_agent.produces.clone();
    }
    card
}

pub(crate) fn agent_output_to_episode(
    agent_db_id: uuid::Uuid,
    query: &str,
    output: &AgentOutput,
) -> Episode {
    // Generate tags from execution metadata
    let mut tags = Vec::new();

    // Status tag
    match output.status {
        AgentStatus::Success => tags.push("status:success".to_string()),
        AgentStatus::Failed => tags.push("status:error".to_string()),
        AgentStatus::Timeout => tags.push("status:timeout".to_string()),
        AgentStatus::BelowConfidenceThreshold => tags.push("status:low-confidence".to_string()),
    }

    // Tool usage tags
    let mut tool_names: Vec<String> = output
        .tool_invocations
        .iter()
        .map(|t| t.tool_name.clone())
        .collect();
    tool_names.sort();
    tool_names.dedup();
    for name in &tool_names {
        tags.push(format!("tool:{}", name));
    }

    // Iteration count tag
    match output.loop_iterations {
        0 | 1 => tags.push("iterations:1".to_string()),
        2..=4 => tags.push("iterations:2+".to_string()),
        _ => tags.push("iterations:5+".to_string()),
    }

    // Cost tier tag (based on token count)
    match output.tokens_used {
        None | Some(0) => tags.push("cost:free".to_string()),
        Some(t) if t < 500 => tags.push("cost:low".to_string()),
        Some(t) if t < 5000 => tags.push("cost:medium".to_string()),
        _ => tags.push("cost:high".to_string()),
    }

    // Model tag
    if let Some(ref model) = output.metadata.model_used {
        let short = if model.contains("sonnet") {
            "claude-sonnet"
        } else if model.contains("haiku") {
            "claude-haiku"
        } else if model.contains("opus") {
            "claude-opus"
        } else if model.contains("mistral") {
            "mistral"
        } else if model.contains("qwen") {
            "qwen"
        } else {
            model.as_str()
        };
        tags.push(format!("model:{}", short));
    }

    // Confidence tag
    let conf = output.confidence;
    if conf >= 0.7 {
        tags.push("confidence:high".to_string());
    } else if conf >= 0.4 {
        tags.push("confidence:medium".to_string());
    } else if conf > 0.0 {
        tags.push("confidence:low".to_string());
    }

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
            "loop_iterations": output.loop_iterations,
            "tool_invocations": output.tool_invocations.iter().map(|t| json!({
                "tool_name": t.tool_name,
                "input": t.input,
                "output": t.output,
                "duration_ms": t.duration_ms,
                "iteration": t.iteration,
            })).collect::<Vec<_>>(),
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
        tags,
        provenance: agent_bestiary_memory::Provenance::AutoPass,
        authority_weight: 0.5,
        dyad_id: None,
        persona_version_at_write: None,
    }
}

// ─── Ontology API (database-backed) ────────────────────────────────

pub(crate) async fn create_notification(
    pool: &PgPool,
    user_id: &str,
    notif_type: &str,
    title: &str,
    message: Option<&str>,
) {
    let _ = sqlx::query(
        "INSERT INTO notifications (user_id, type, title, message) VALUES ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(notif_type)
    .bind(title)
    .bind(message)
    .execute(pool)
    .await;
}
