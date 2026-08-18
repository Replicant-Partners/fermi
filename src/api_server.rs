// `build_agent_json` assembles one large `json!` literal per agent, and an
// agent card has grown enough declaration fields (accepts, produces,
// prompt_template, fermi_contract, output_contract, capabilities, execution
// stats...) that `serde_json`'s recursive macro expansion exceeds the default
// limit of 128 under `--test`, which adds expansion depth. Non-test builds
// fit, so this surfaces as "cargo check passes but cargo test won't compile".
// Raising the limit is what the compiler itself advises; `fermi-console`'s
// binary carries the same attribute for the same class of reason.
#![recursion_limit = "256"]

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
    auth_middleware, impersonation_guard, optional_auth_middleware, AuthPrincipal, AuthState,
    OAuthConfig,
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgConnectOptions, postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use fermi::gas::GasFees;
use tokio::sync::broadcast;

use agent_bestiary_memory::{
    Agent, EmbeddingGenerator, Episode, MemoryStore, MockEmbeddings, OpenAIEmbeddings,
};
use agent_bestiary_ontology::{GitConfig, WorkspaceGitManager};
use agent_bestiary_projector::{ProjectionCache, ProjectionEngine};

// ─── Rate Limiter ──────────────────────────────────────────────────

use dashmap::DashMap;
use std::time::Instant;

// Handler modules — new handlers go here instead of this file
#[path = "handlers/mod.rs"]
mod handlers;

// v0.11.0: schema trust contract — boot-time drift check against the DB.
// See src/schema_trust.rs for the manifest and the check logic.
//
// Re-exported from the library rather than `#[path]`-included, so the
// contract and the binary share one compiled copy and `cargo test` can
// reach the module. The `pub(crate) use` keeps existing `crate::schema_trust`
// paths (e.g. in handlers/admin.rs) working unchanged.
pub(crate) use fermi::schema_trust;

// Agent economics — measured run counts and cost, derived from `episodes`.
// Re-exported from the library for the same reason `schema_trust` is: one
// compiled copy, reachable from `cargo test`. Handlers use
// `crate::agent_economics::*`; a second copy of an aggregate definition is
// how "successful runs only" creeps into one of them and the platform
// starts under-reporting spend.
pub(crate) use fermi::agent_economics;

// Grounding trust contract — which output fields a given agent could
// possibly have sourced, and enforcement that nulls the ones it could not.
// Re-exported for the same reason as its two siblings: one compiled copy,
// reachable from `cargo test`. Handlers use `crate::grounding_trust::*`.
pub(crate) use fermi::grounding_trust;

// Episode construction moved to `fermi::episodes` (lib) so the in-library
// delegation tools in `agent_backend::tools_legacy` can build episodes
// through the same constructor the HTTP handlers use, instead of keeping a
// second hand-maintained copy that drifts on cost basis, provider
// attribution and failure provenance. The `pub(crate) use` keeps the ~10
// existing `crate::agent_output_to_episode` call sites resolving unchanged.
pub(crate) use fermi::episodes::agent_output_to_episode;

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

/// Routes that dispatch an LLM, and therefore spend real money per call.
///
/// `*` matches exactly one path segment, so `/api/agents/*/execute` matches
/// `/api/agents/abc/execute` and not `/api/agents/abc/execute/stream` — which
/// is listed separately rather than caught by accident.
///
/// ## Why a list and not "all protected routes"
///
/// The strict limiter is 10/min. Applying that to every authenticated route
/// would throttle ordinary dashboard use, the limit would be raised to
/// something harmless, and the protection would be gone — the same way a
/// lint that fires on correct code gets deleted. So it is scoped to the
/// endpoints where one request costs a model call.
///
/// ## Why these and not every credit-spending handler
///
/// 57 call sites across 23 handler files call `charge_gas` and friends, but
/// most are bounded by the wallet: `charge_gas` debits first and fails on an
/// empty balance, so a user cannot spend what they do not have. The
/// unbounded exposure is the LLM bill we incur per dispatch, which is why
/// this list is the *dispatch* entry points rather than everything that
/// touches credits.
const LLM_SPEND_ROUTES: &[&str] = &[
    "/api/agents/*/execute",
    "/api/agents/*/execute/stream",
    "/api/agents/*/eval/run",
    "/api/agents/*/consolidate",
    // NOT `/api/agents/*/dreaming`. That path is a GET returning Loop 1
    // maturity — five COUNT queries and no model call. It was listed here as
    // though it were a dispatch, and since the matcher is path-only (no
    // method), the effect was a 10/min ceiling on a read. The observatory
    // loads it on every agent selection, so an operator clicking through
    // eleven agents in a minute got a 429 from a rate limiter that exists to
    // cap the LLM bill. The dispatch on this loop is `*/consolidate`, above.
    "/api/me/eval/runs/batch",
    "/api/creatures/*/enemy-sensor",
    "/api/creatures/*/genome-profiler",
    "/api/creatures/*/prey-locator",
    "/api/creatures/*/dream",
    "/api/workspaces/*/composition/dream",
    "/api/notebooks/*/execute",
];

/// Does this request path name an LLM-dispatching endpoint?
fn is_llm_spend_route(path: &str) -> bool {
    let segs: Vec<&str> = path.trim_end_matches('/').split('/').collect();
    LLM_SPEND_ROUTES.iter().any(|pattern| {
        let pat: Vec<&str> = pattern.split('/').collect();
        pat.len() == segs.len()
            && pat
                .iter()
                .zip(&segs)
                .all(|(p, s)| *p == "*" || p.eq_ignore_ascii_case(s))
    })
}

/// Rate limit middleware for LLM endpoints (stricter, per-user).
///
/// Was `#[allow(dead_code)]` and referenced nowhere: the limiter was
/// constructed from `RATE_LIMIT_LLM` on every boot and never consulted, so
/// the endpoints that spend money were the only ones with no rate limit at
/// all while the public read-only routes had one. Dead code that looks like
/// a control is worse than absent code, because the control appears in the
/// config and in review.
///
/// Passes non-LLM paths straight through, so it can be layered over the
/// whole protected router without carving hundreds of routes into a
/// sub-router.
async fn llm_rate_limit_middleware(
    State(state): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<Response, (StatusCode, String)> {
    // Everything that is not an LLM dispatch is none of this layer's
    // business. Checked before the key is built so the common path costs a
    // string comparison.
    if !is_llm_spend_route(req.uri().path()) {
        return Ok(next.run(req).await);
    }

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

/// Spec 22 §Security — single-use, time-bounded consent token entry for
/// raw-vector embedding export. Embeddings are invertible, so exporting them
/// is gated behind an explicit acknowledgement that must be presented to the
/// export endpoint.
///
/// Stored in `AppState.export_consent`. In-process map is sufficient at
/// solo-dev / single-instance scale; a multi-instance deployment would
/// promote this to a `consent_tokens` DB table.
#[derive(Clone)]
pub(crate) struct ExportConsentEntry {
    pub agent_id: uuid::Uuid,
    pub user_id: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub consumed: bool,
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
    /// Outbound transactional email (Resend). Cloneable, cheap; the
    /// underlying reqwest::Client is Arc-shared. When not configured
    /// (`RESEND_API_KEY` unset), every send_* call is a logged no-op
    /// so local dev + CI don't require creds — the console's copy-
    /// invite-link affordance is the fallback share path in that
    /// mode.
    pub(crate) email: fermi::email::EmailConfig,
    pub(crate) rate_limits: RateLimitConfig,
    pub(crate) ws_broadcast: broadcast::Sender<WorkspaceEvent>,
    pub(crate) rabble_broadcast: broadcast::Sender<RabbleEvent>,
    pub(crate) creature_broadcast: broadcast::Sender<CreatureEvent>,
    pub(crate) secret_encryptor: Option<Arc<fermi_auth::SecretEncryptor>>,
    /// Spec 22 §Security — issued by POST .../embeddings/export/consent,
    /// consumed by GET .../embeddings/export?format=full. Single-use,
    /// 5-minute TTL. See `ExportConsentEntry`.
    pub(crate) export_consent: Arc<dashmap::DashMap<String, ExportConsentEntry>>,
    /// Spec 21 Phase 4.1: shared broadcast from a single PgListener connection.
    /// Payload: (channel_name, json_payload). SSE handlers subscribe here
    /// instead of opening a raw PgListener connection per client.
    /// Capacity 2048 — a receiver lagging >2048 messages gets RecvError::Lagged
    /// and must re-fetch backfill from the DB (handled in the SSE handler).
    pub(crate) pg_notify: broadcast::Sender<(String, String)>,

    /// Spec 14 §5.6: in-memory store for fitted ConditionalPosteriors.
    /// `fit_conditional` returns a `posterior_id: Uuid` keyed in this map; the
    /// subsequent `predict` / `input_sensitivity` / `compare_scenarios` /
    /// `prob_exceeds` / `optimise_for_target` endpoints look up the posterior
    /// by id. **Session-scoped** — posteriors are lost on server restart
    /// (persistent posterior store is Phase 5).
    pub(crate) posterior_cache:
        Arc<dashmap::DashMap<uuid::Uuid, posterior_reg::ConditionalPosterior>>,

    /// Spec 23 R-1: registry of `Extractor`s used by the BayesOps refit
    /// hook to derive scalar observations from upstream workspace
    /// resolution outcomes. Populated at server boot via
    /// `ExtractorRegistry::with_builtins()`; new extractors can be added
    /// at boot time only (immutable from then on). See
    /// `docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md` §3.4 and
    /// `src/handlers/workspace/refit.rs`.
    pub(crate) extractor_registry: posterior::ExtractorRegistry,

    /// Spec 23 D8 Phase 2: sqlx-backed BrierLookup for the evaluator system.
    /// Wraps a PgPool and resolves Brier scores from `fermi_forecasts` for
    /// the `BrierEvaluator` dimensional evaluator. Populated at server boot.
    pub(crate) brier_lookup: Arc<dyn agent_bestiary_evaluators::BrierLookup + Send + Sync>,
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
        // Positioned, not sorted. 195 declares the `users` columns that
        // production has and no migration creates (`id`, `password_hash`,
        // `password_salt`) plus the auth_provider CHECK that 004b fails to
        // widen. It has to run before the files that depend on them — 004b,
        // 005, 161, 165, 171 — which is here, not at the end of the list.
        // See docs/plans/CI_MIGRATION_RATCHET.md.
        "migrations/195_declare_the_ghost_schema.sql",
        // 004_migrate_users_for_auth.sql was merged into 004b — file does not exist
        "migrations/004b_migrate_users_for_auth.sql",
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
        "migrations/048_fermi_notebooks.sql", // was wrongly listed as 048_voice_assets.sql
        "migrations/049_akp_foundation.sql",  // AKP protocol tables (deferred but idempotent)
        "migrations/048b_voice_assets.sql",
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
        // Must sit between 048 (which creates `fermi_forecasts` with 13
        // columns) and 094 (whose `CREATE TABLE IF NOT EXISTS` declares 28 and
        // is therefore skipped entirely). Without it 094 aborts on an index
        // over the `status` column it believes it created, never reaches its
        // own `fermi_forecast_updates`, and takes 140/149/150/156/174/175/176
        // down with it.
        "migrations/196_reconcile_fermi_forecasts.sql",
        "migrations/094_fermi_forecasting.sql",
        "migrations/094_rabble_follows.sql",
        "migrations/095_saved_locations.sql",
        "migrations/096_performance_indexes.sql",
        "migrations/097_governance.sql",
        "migrations/098_push_subscriptions.sql",
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
        // Phase 5 — two-reviewer consensus for agent_wide interventions
        "migrations/108_intervention_feedback_loop.sql",
        // Repair curated agents wrongly assigned to first user by old
        // migration 006 backfill (now removed).
        "migrations/110_unassign_curated_agents.sql",
        // Reassign curated/system agents to the sys admin so they
        // retain Eval / Intelligence / Manage tab access.
        "migrations/111_restore_admin_ownership_of_curated.sql",
        // Vertical decoupling Step 1: teams.origin column + rabble
        // auto-gen backfill (docs/VERTICAL_HARNESS_SPLIT.md §6).
        "migrations/112_workspace_origin.sql",
        // Composition as first-class: teams.mission + strategist_id
        // + composition_versions table (docs/COMPOSITION_AS_FIRST_CLASS.md §10).
        "migrations/113_composition_as_first_class.sql",
        // Agent valence column on agents table (migration 114)
        "migrations/114_agent_valence_column.sql",
        // Xaman Ek working sessions (migration 115)
        "migrations/115_xaman_sessions.sql",
        // App primitive — registered platform artifacts with workspace templates (Doc 1)
        "migrations/116_apps.sql",
        // Agent output_contract — domain-constrained MoE
        "migrations/117_agent_output_contract.sql",
        // Extend object_shares.object_type to include 'workspace' (Doc 1 §6.2)
        "migrations/118_object_type_workspace.sql",
        // Defensive backstop: ensure teams composition-identity columns
        // exist regardless of whether 113 took. DO-block so PgBouncer
        // can't split it.
        "migrations/119_teams_mission_defensive.sql",
        // composition_versions.rejected_by + rejection_note — code
        // expected these but no migration ever added them.
        "migrations/120_composition_versions_rejection.sql",
        // Fix Xaman Ek ontology snapshot: invalid Mermaid attribute types
        // (TEXT_ARRAY, UUID_ARRAY, VECTOR_1024, PK_FK) that break the
        // knowledge graph renderer.
        "migrations/121_fix_xaman_ek_ontology_mermaid.sql",
        // Backfill ownership for curated/system agents seeded after
        // migration 111 ran (e.g. new SimOps v2 fleet agents). Same
        // logic as 111 — idempotent, assigns earliest admin user.
        "migrations/122_backfill_curated_agent_ownership.sql",
        // Remove test regression-fixture agents (seed_market_research,
        // seed_geopolitical_risk, seed_crypto_sentiment) that were
        // accidentally written to production via SeedData::build().
        "migrations/123_remove_test_seed_agents.sql",
        // Phase 2 observability annotation: provider_used + model_used on
        // episodes, eval_signals, anomaly_events; provider_mix on
        // coherence_evaluations. Enables per-provider observatory filtering
        // and Loop 5 calibration tracking split by provider.
        "migrations/124_observability_provider_annotation.sql",
        // Generalised workspace action protocol: workspace_action_log +
        // workspace_annotations tables. Foundation for isomorphic App actions
        // across companion, CLI, and MCP callers.
        "migrations/125_workspace_action_protocol.sql",
        // agent_versions gains the rest of the capability config (model_ladder,
        // capability_gates, min_tier, output_contract, version_string) so a
        // version snapshot describes behaviour, not just the prompt.
        //
        // NOTE: this file was on disk from v0.10.x but absent from this list
        // until v0.11.9 — CI's `for f in migrations/*.sql` glob applied it while
        // production never did, so the two schemas silently differed by five
        // columns. That divergence is exactly what the migration ledger (Phase 2
        // of docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md) exists to make
        // impossible; this hardcoded list is the proximate cause.
        "migrations/126_agent_version_full_config.sql",
        // Expand xaman_sessions.session_type CHECK to include 'app_design'
        // (new conversational mode for building Apps on ABW via xaman_ek).
        // Originally numbered 124; renumbered to 127 after the topology Phase-2
        // observability migration claimed slot 124 in a parallel session.
        "migrations/127_xaman_sessions_app_design.sql",
        // Doc 12 § Capability 1 — agent version stamp on sosa_observations.
        "migrations/128_sosa_observations_produced_by.sql",
        // Backfill ownership for curated agents seeded after migration 122
        // (e.g. simops_dynamics_runner). Same idempotent pattern as 111/122.
        "migrations/129_backfill_curated_agent_ownership_2.sql",
        // Index sosa_observations.extra for projection_id lookup.
        // Enables ProjectionScoringEvaluator (spec 20) to find prior
        // synthetic observations when real measurements arrive.
        "migrations/130_sosa_projection_index.sql",
        // Creature goal state for Rabble goal-tracking.
        "migrations/131_creature_goals.sql",
        // Forage observation types for kask-app-wild.
        "migrations/132_forage_observations.sql",
        // Named human-agent relationships (dyad social graph).
        "migrations/133_dyad_profiles.sql",
        // Source column on notifications — prevents ABW platform
        // notifications bleeding into the Rabble surface.
        "migrations/134_notifications_source.sql",
        // Spec 22 — Embedding Portability (Phase 1.3): per-row provenance
        // columns on the five vector tables + append-only embedding_provenance
        // sidecar log. Pure additive; safe to re-run.
        "migrations/135_embedding_provenance.sql",
        // NOTE: migrations/136_embedding_provenance_not_null.sql is
        // deliberately absent. Its invariant is correct but its form is not
        // boot-safe (bare ADD CONSTRAINT re-run every boot; 12 top-level
        // statements through PgBouncer). Superseded by migration 184, which
        // applies the same constraints idempotently in one DO block.
        // NOTE: migration 136 (NOT NULL enforcement) intentionally NOT in the
        // boot sequence. It validates "embedding => full provenance" via
        // ALTER TABLE ADD CONSTRAINT, which fails on unstamped pre-Spec-22
        // rows. It MUST be run manually AFTER the backfill binary
        // (scripts/backfill_embedding_provenance.rs) completes:
        //   PGCONNECT_TIMEOUT=30 psql "$DATABASE_URL" \
        //     -v ON_ERROR_STOP=1 \
        //     -f migrations/136_embedding_provenance_not_null.sql
        // Spec 22 Phase 2.1 — closed-model anchor set table (vendor side
        // co-embedded with open reference model for translator fitting if
        // a vendor goes dark).
        "migrations/137_embedding_anchors.sql",
        // Fermi-as-App: missing columns on fermi_forecasts (from prior
        // commit 8c2d5dc but never wired into boot sequence).
        "migrations/138_fermi_forecasts_missing_columns.sql",
        // Fermi-as-App: workspace_id FK on fermi_forecasts. Without this,
        // forecast spawn fails silently and forecasts don't appear as
        // workspaces alongside other ABW apps.
        "migrations/139_fermi_workspace_link.sql",
        // Forecast benchmark infrastructure: commitment anchors,
        // harness snapshots, splits, spacetime trajectory table.
        "migrations/140_forecast_benchmark.sql",
        // SimOps process benchmark: projection commits, process spacetime,
        // sample point config. Two-hook architecture: commit on synthetic
        // write, resolve on real sensor ingest.
        "migrations/141_process_benchmark.sql",
        // Performance: HNSW vector indices for KG hot-path retrieval +
        // partial composite index on episodes (unconsolidated subset).
        "migrations/142_performance_indices.sql",
        // Workspace outputs (typed KV), dependencies (DAG), status lifecycle.
        "migrations/143_workspace_outputs.sql",
        // Schema-drift fix: fermi_forecasts was missing updated_at, which
        // broke publish (INSERT) and edit-save (UPDATE). Same pattern as
        // 107 and 138 — original CREATE TABLE in 048 lacked the column
        // and CREATE TABLE IF NOT EXISTS in 094 was a no-op on existing
        // tables. Idempotent.
        "migrations/144_fermi_forecasts_updated_at.sql",
        // Spec 22 Phase 1b — backfill pre-Spec-22 embedding rows with
        // synthetic provenance. Pure-SQL equivalent of the Rust
        // backfill-embedding-provenance binary, runnable as part of the
        // boot sequence so operators don't have to remember to run it.
        // Idempotent: gated on `embedding_model_id IS NULL` per row so
        // re-runs are no-ops once backfill has been applied.
        "migrations/145_embedding_provenance_backfill.sql",
        // Schema-drift fix: fermi_forecasts.notebook_id was declared
        // NOT NULL in migration 048 but the fermi-as-app workflow creates
        // forecasts without a notebook. Same family as 138 and 144.
        "migrations/146_fermi_forecasts_notebook_nullable.sql",
        // Workspace resolution lifecycle — generalises forecast
        // resolution beyond fermi_forecasts. Single endpoint at
        // POST /api/workspaces/:id/resolve transitions any workspace
        // active → completed/failed, writes outcome to both teams
        // columns AND workspace_outputs, computes Brier, fan-outs to
        // downstream workspaces, and is the BayesOps refit hook
        // insertion point.
        "migrations/147_workspace_resolution.sql",
        // BayesOps refit ledger + pending queue (Phase R-1 of
        // docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md). bayesops_posterior_snapshots
        // is the audit ledger consumed by the spacetime view (R-3);
        // bayesops_pending_fits is the queue consumed by the sparkline
        // accept/dismiss UX (R-2).
        "migrations/148_bayesops_refit_ledger.sql",
        // Spec 23 R-3 Piece 1: lets BayesOps refits tag their
        // fermi_forecast_updates rows so the forecast_spacetime trigger
        // surfaces them with the right revision_trigger value instead of
        // the generic 'evidence_update'.
        "migrations/149_forecast_updates_trigger_kind.sql",
        // ── Spec 24: forecast collaboration & sharing ─────────────────
        //
        // 150: forecast_relationships — generalized inter-forecast
        // cascade primitive. Spec 23 cascade-resolution extension. Also
        // landed via ensure_critical_schema for defensive deploys;
        // double-execution is safe (all statements are IF NOT EXISTS /
        // ALTER ... DROP CONSTRAINT IF EXISTS).
        "migrations/150_forecast_relationships.sql",
        //
        // 151: forecast_invites — unified pending-invite primitive for
        // forecasts, portfolios, and teams. Spec 24 §3.1.1.
        "migrations/151_forecast_invites.sql",
        // 152: extend object_shares.object_type CHECK to include
        // 'portfolio' so portfolios can be shared through the same
        // mechanism as forecasts. Spec 24 §3.1.2.
        "migrations/152_object_shares_portfolio.sql",
        // 154: backfill object_shares rows for forecasts/portfolios that
        // already have team_id set, so the can_access / can_view helpers
        // (Sprint 2.4b) see the team share through the canonical path.
        // Without this, switching handlers from inline team_id checks to
        // object_shares-based ACL would silently deny access to existing
        // team-shared content. ON CONFLICT DO NOTHING → idempotent.
        // Spec 24 §3.2 Wave 2 step 5.
        "migrations/154_forecasts_object_shares_backfill.sql",
        // 153: pending_cascades — operator-gated review queue for
        // cascade propagation. Spec 23-extension. Previously numbered
        // 151 (collided with forecast_invites); renamed to 153 on
        // 2026-06-23 to give a clean monotonic ordering. Also landed
        // via ensure_critical_schema; double-execution is safe.
        "migrations/153_pending_cascades.sql",
        // ── Spec 25: Forecast Relationship Groups ─────────────────────
        //
        // 155: forecast_relationship_groups table + relationship_groups
        // column on fermi_forecasts. Group-tag model replaces the
        // per-relationship ID-list model from mig 150.
        "migrations/155_forecast_relationship_groups.sql",
        // 156: pending_cascades extensions — applied_deltas (for undo),
        // superseded_by (for requeue), group_id, 'undone' status,
        // 'cascade_undo' revision_trigger.
        "migrations/156_pending_cascades_extensions.sql",
        // 157: restore the COMPLETE object_shares.object_type set. MUST stay
        // LAST among constraint-touching migrations — the runner executes
        // files in list order every startup, so the last drop/recreate
        // wins. 152 dropped 'workspace' (its author left "that bug for
        // whoever needs it"), which 500'd kask.bio workspace sharing
        // (POST /api/shares object_type='workspace'). 157 recreates the
        // CHECK with the full union and is the single source of truth for
        // this constraint going forward — add new object types HERE.
        "migrations/157_object_shares_complete_object_type.sql",
        "migrations/158_forecast_sim_probability.sql",
        "migrations/159_pending_cascades_relationship_id_nullable.sql",
        "migrations/160_users_signup_app.sql",
        // 161: backfill users.user_id for legacy / half-provisioned rows
        // that made every account except the INSERT-path original get
        // FK-violations on forecast/portfolio writes and 403s on invite
        // accept. Root cause: sync_user_from_app's UPDATE branch never
        // touched user_id, so rows with a NULL/empty user_id column
        // stayed broken across sign-ins. Paired with the v0.10.3
        // UPDATE-clause fix in oidc.rs. See RELEASE_NOTES_v0.10.3.md.
        "migrations/161_backfill_users_user_id.sql",
        // 162: v0.10.4 substrate. Adds FK NOT VALID from every
        // resource-owner column across all tenant apps (Fermi, Rabble,
        // simOps, SOSA, AR) to users(user_id). Enforces the invariant
        // for new writes without blocking deploy on existing drift.
        // Also heals two recoverable drift classes (empty-string and
        // id::text). See RELEASE_NOTES_v0.10.4.md.
        "migrations/162_rbac_substrate_fk.sql",
        // 163: rbac_orphans view. Single-query cross-table drift audit
        // powering /api/admin/rbac/orphans. Zero rows = RBAC invariant
        // holds across ABW. Extend one SELECT block per new tenant
        // resource table with an owner column.
        "migrations/163_rbac_orphans_view.sql",
        // 164: admin_bypass_events audit table. v0.10.5. Records every
        // platform-admin bypass of a workflow gate (force-publish
        // being the first). RBAC ownership bypass is implicit in the
        // platform-admin role and NOT logged here — only quality-gate
        // overrides land in this table.
        "migrations/164_admin_bypass_events.sql",
        // 165: v0.10.9 root-cause fix. Realigns the fermi_forecasts /
        // _portfolios / _notebooks owner_id FKs from users(id) (the
        // UUID PK, where they'd drifted on this deploy) back to
        // users(user_id) (the TEXT column mig 094 originally declared).
        // Every non-legacy user was hitting FK violation on save
        // because sync_user_from_app's INSERT branch mints a fresh
        // Uuid::new_v4() for user_id, distinct from the row's PK id.
        // Rebases existing owner_id values from id::text → user_id via
        // JOIN so the new FK is satisfied by every existing row.
        // See RELEASE_NOTES_v0.10.9.md.
        "migrations/165_fermi_forecasts_owner_fk_realign.sql",
        // 166: adds agents.updated_at. Four write sites reference this
        // column (publish_agent, archive_agent, restore_agent,
        // update_fork_pricing_handler) and all have been silently
        // 500'ing since the code shipped — nobody hit the codepath
        // cleanly until v0.10.15 unblocked admin force-publish.
        // `agents` was the only publishable substrate on the platform
        // missing an updated_at column; every other one (apps,
        // fermi_forecasts, teams, wallets, …) has it. Backfills
        // existing rows from created_at.
        //
        // Authored for a v0.10.18 hotfix; that version number went to
        // the console updater fix shipped from a parallel session, so
        // it lands here alongside mig-167. Both are schema-drift
        // closures. See RELEASE_NOTES_v0.10.19.md.
        "migrations/166_agents_updated_at.sql",
        // 167: v0.10.19 hotfix. Recreates fermi_leaderboard
        // materialized view with `::float8` casts on MIN/MAX so
        // Rust reads as Option<f64> stop 400'ing with FLOAT4/FLOAT8
        // mismatch. Same substrate rule applied to the four SQL
        // reads in handlers/forecasts.rs (portfolio_stats,
        // my_stats, leaderboard fallback, resolve_forecast). Family
        // originated in Mo's Resolve Forecast dialog. See
        // RELEASE_NOTES_v0.10.19.md.
        "migrations/167_fermi_leaderboard_float8_minmax.sql",
        // 168: v0.10.23 hotfix. GIN index on
        // fermi_forecasts.agents_used. The v0.10.20 legacy-slug
        // audit endpoint timed out client-side because it ran one
        // JSONB-containment COUNT per legacy name (~43 sequential
        // seq-scans of fermi_forecasts). This index makes every
        // `agents_used @>` query O(log n) instead of O(n) — also
        // speeds up eval_brier's Brier lookup and the calibration
        // handler. Combined with the handler rewrite in v0.10.23
        // the audit endpoint returns in milliseconds.
        // (v0.10.21/v0.10.22 were parallel forecast-save work,
        // unrelated to this hotfix.)
        "migrations/168_fermi_forecasts_agents_used_gin.sql",
        // 169: v0.10.25 hotfix. Realigns the 4 mig-049 tables'
        // FKs on agents(agent_id) to ON DELETE CASCADE, matching
        // every other FK on that column across the platform.
        // mig-049 (agent_alignments, pairwise_coherence,
        // knowledge_transfers, agent_interaction_policies)
        // declared them without ON DELETE, defaulting to NO ACTION
        // — which blocks the v0.10.25 test-cruft cleanup path.
        // Semantically these SHOULD cascade: alignments and
        // coherence scores are derived from the agents; when the
        // agent goes away, the derived rows are meaningless.
        "migrations/169_akp_foundation_fks_cascade.sql",
        // v0.10.26: backfill agent_id into fermi_forecasts.agents_used so
        // resolved-forecast brier scores become attributable to agents
        // (Loop 5 join is on agent_id; data was keyed by name only).
        // Idempotent; already applied to prod out-of-band 2026-08-03.
        "migrations/170_backfill_agents_used_agent_id.sql",
        // Agent credential store + abw-system principal (P0 of the
        // credential model, docs/specs/AGENT_CREDENTIAL_MODEL.md).
        "migrations/171_agent_credentials.sql",
        // v0.11.2: orchestra registry — request/approve substrate for
        // domain-constrained MoE membership. Adds
        // `orchestra_membership_requests` table, views
        // `orchestra_fermi_members` and `orchestra_xaman_ek_members`,
        // and reserves `fermi_forecasts.counterfactual_brier` for the
        // manager-effect metric. See RELEASE_NOTES_v0.11.2.md.
        "migrations/172_orchestra_membership.sql",
        // Realign per-agent embedding identity (anthropic/voyage-2 ->
        // openai/text-embedding-3-large) to the active embedder. Metadata
        // only; per-vector provenance untouched. See mig file + P5 notes.
        "migrations/173_embedding_identity_realign.sql",
        // Brier integrity: scored_probability (immutable audit anchor) +
        // resolution_source (structured provenance) on fermi_forecasts,
        // plus a BEFORE UPDATE trigger that freezes the scoring tuple
        // once resolved. Fixes silent corruption of every resolved
        // forecast by the nine unguarded predicted_probability writers.
        "migrations/174_fermi_forecasts_brier_integrity.sql",
        // Loop 5: backfill forecast_spacetime.brier_at_this_point and
        // .loop5_calibration for forecasts that resolved before those
        // columns had any writer. Derived data only — does not touch the
        // mig-174 audit anchors. Idempotent.
        "migrations/175_forecast_spacetime_loop5_backfill.sql",
        // Spec 26 (v0.11.7): collaboration attribution.
        // fermi_forecast_updates.actor_user_id — WHO made a revision
        // (orthogonal to agent_id, which records WHICH agent produced
        // the number). fermi_portfolio_forecasts.added_by — who pulled
        // a forecast into a portfolio. Both were missing, which is why
        // the Teams Activity tab had to guess team activity from
        // updated_at timestamps instead of showing real per-member
        // events. Plus the covering indexes the derived activity feeds
        // and provenance queries need. See
        // docs/specs/SPEC_26_TEAM_COLLABORATION.md.
        "migrations/176_collab_attribution.sql",
        // Clear legacy `mcp_tools` data out of agents.mcp_servers. The
        // old create path wrote the wrong field; harmless while nothing
        // read the column, actively wrong now that it is the source of
        // truth for agent config (resolve_agent_card). Idempotent.
        "migrations/177_agents_mcp_servers_legacy_cleanup.sql",
        // agents.mcp_tools — DB-backed list of tools an agent PUBLISHES
        // over /mcp/agents/:id. Symmetric with mcp_servers (which tools it
        // may CALL). Without it, the ~87% of agents that have no
        // filesystem card published nothing and were reachable only via
        // the catch-all `execute` path.
        "migrations/178_agents_mcp_tools.sql",
        // Spec 30 (v0.11.10): team_members.capabilities. Splits discrete
        // powers over a team's work ('resolve') away from the role ladder
        // that administers the team. Closes the hazard Spec 26 opened: a
        // portfolio team-share grants 'edit' on every forecast inside it,
        // and `resolve` was gated on 'edit' — so sharing a book for
        // collaboration silently delegated the irreversible scoring
        // decision to the whole team. Backfills 'resolve' to owners and
        // admins only; the tightening on members is the point.
        "migrations/179_team_capabilities.sql",
        // SPEC_29 (v0.11.10): orchestra membership as governed state.
        // mig-172 kept the predicate `fermi_contract IS NOT NULL`, so
        // approval was a side effect that *produced* membership rather
        // than the state it derives from — any other writer of that
        // column (notably the unguarded import path) was indistinguishable
        // from an admin approval, and the request/approve audit trail was
        // write-only. Splits capability (the contract, owner-editable)
        // from membership (a grant in orchestra_members, with honest
        // provenance: approved / curated_seed / admin_grant). Backfill is
        // behaviour-preserving and classifies rather than blanket-approves;
        // verified to apply repeatedly without downgrading a real approval.
        "migrations/180_orchestra_members.sql",
        // Integrity reconciliation for the 2026-08-06 production audit
        // (scripts/integrity_audit.sql). Seeds the abw-system principal,
        // declares users.id so a rebuild is faithful, supplies
        // ar_beacons.location_name so migration 163 can finally create the
        // rbac_orphans view, and reconstructs one credit_ledger row lost to
        // a swallowed write. Idempotent; verified to apply twice cleanly.
        "migrations/181_integrity_reconciliation.sql",
        // Stripe checkout idempotency as a database claim rather than a
        // read-then-write race. billing.rs previously treated a DB error as
        // "not yet processed" and wrote its idempotency marker after the
        // deposit, in a swallowed UPDATE — two independent double-credit
        // paths. Backfills processed sessions from credit_ledger history.
        "migrations/182_stripe_session_idempotency.sql",
        // Spec 32 (v0.11.13): driver_annotations. Objections anchored to a
        // specific driver — "your base rate is wrong" — rather than to the
        // forecast, because that is what teams actually argue about. The
        // CHECK enforces that any resolution records who and when, so
        // "accepted" can never be unattributable.
        "migrations/183_driver_annotations.sql",
        // Spec 22 §1c embedding-provenance invariant, in a boot-safe form.
        // Supersedes migration 136 (correct invariant, unusable shape — see
        // the note next to 135). Safe to enable because the 2026-08-06
        // integrity audit measured zero violating rows across all five
        // constrained tables; the migration re-measures before adding each
        // constraint and warns rather than aborting if that ever changes.
        "migrations/184_embedding_provenance_invariant.sql",
        // Hide integration-test cruft (`test_agent_<uuid>`) from the
        // orchestra roster views. Policy is hide-not-delete: the rows are
        // harmless where they sit, they just must never appear in a
        // human-facing list. Rust enumeration is filtered at source by
        // NOT_TEST_CRUFT in the memory store; these views are read
        // directly, so they need the predicate too. Acute for
        // xaman_ek, whose rule is literally "every published agent".
        "migrations/185_hide_test_cruft_from_rosters.sql",
        // SPEC_30 — agents.taxonomy. Taxonomy lived only in on-disk
        // agent_card.json, so it was structurally unavailable to every agent
        // created through the UI or import: no card on disk, nothing to read.
        // All 13 efra agents were permanently "Incertae sedis", and that is
        // the majority shape of third-party agents. Derived ranks are now
        // written at creation by fermi::taxonomy::derive; the curated seeder
        // carries each card's editorial ranks through on every boot.
        "migrations/186_agents_taxonomy.sql",
        // Append-only ledger of the individual quantitative claims agents make
        // (driver multipliers). Without it, a resolved forecast's per-agent
        // inputs are gone — the params write is current-state only — so agent
        // credit is unrecoverable after the fact. Prerequisite for the
        // counterfactual subset re-runs that exact Shapley attribution needs
        // (src/attribution/).
        "migrations/187_forecast_agent_claims.sql",
        // Persisted per-agent Shapley credit + pairwise interaction indices.
        // efficiency_residual and reconstruction_error are validity gates, not
        // metadata: a row with either far from zero describes a forecast that
        // was not actually measured. See
        // docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md
        "migrations/188_forecast_attributions.sql",
        // Admin "view as user" audit substrate. impersonation_sessions is
        // authoritative for liveness — the guard middleware refuses any
        // request whose session row is absent, ended, or expired — so the
        // stateless JWT can be revoked. impersonation_events is the
        // per-request trail, including blocked mutation attempts.
        // See docs/specs/SPEC_33_IMPERSONATION.md.
        "migrations/189_impersonation_audit.sql",
        // High-water mark for the evolution badge. The badge is computed live
        // from outcomes; only the peak is stored, because regression cannot be
        // detected without remembering the best rank previously reached.
        "migrations/190_agent_evolution.sql",
        // The one forecast-keyed table that never declared its reference, so
        // deleting a forecast left its agent schedules behind. Clears the
        // backlog, then lets Postgres enforce it.
        "migrations/191_forecast_schedules_fk.sql",
        // One definition of agent economics, derived from `episodes`. The
        // five `agents.*_executions` / cost counters were never wired to
        // the execution path, so every consumer — marketplace pricing,
        // profiles, rosters, the ecology lens, and a deletion safety
        // guard — read a permanent zero.
        "migrations/192_agent_execution_rollup.sql",
        "migrations/193_route_provenance_outcomes.sql",
        // Episode cost basis: persist the input/output split + rate basis
        // so cost_usd becomes a derived, correctable quantity rather than
        // a figure baked in at write time.
        "migrations/194_episode_cost_basis.sql",
        // Shared correlation id between claims and episodes — the follow-up
        // migration 193 asked for. Replaces the (agent_id, driver, window)
        // heuristic with an exact join, and adds forecast_cost_attribution.
        "migrations/197_claim_episode_correlation.sql",
        // Delegation tree: delegated runs now write their own episode, linked
        // to the caller. Compound cost becomes the sum over the tree, and
        // forecast_cost_attribution descends it.
        "migrations/198_episode_delegation_tree.sql",
        // Retain the agent's own output. `episodes` recorded the question and
        // every property of the run except the answer, which was digested by
        // a per-agent parser and discarded. Without it there is no evidence
        // base for inducing output types, so the port-typing campaign cannot
        // start. See docs/ABW_VERIFICATION_RECONCILIATION.md §7.7.
        "migrations/199_episode_response_retention.sql",
        "migrations/200_coordinator_observation_provenance.sql",
        // `anomaly_events.kind` gains 'grounding', so a field the agent could
        // not have sourced becomes a reportable event rather than a stderr
        // line. Also tags the 13 cached genome profiles written before the
        // contract existed — tagged, not overwritten, because the read path
        // already strips them and the guesses are a calibration signal once
        // real tools land.
        "migrations/200_grounding_anomalies_and_backfill.sql",
        // `semantic_rules.extracted_by` — which agent authored a rule, as
        // opposed to which agent the rule is FOR. Without it the ontologist
        // could not be credited for a single rule it had ever written, so Loop 1
        // for the extractor had no signal to run on.
        "migrations/201_extraction_provenance.sql",
        // Fixes migration 200's legacy tag, which keyed on shape ("has a
        // genome key") rather than history ("has provenance keys") and so
        // mislabelled correctly-grounded profiles as legacy on every reboot.
        // Harmless until PRE_CONTRACT_MARKER began trusting the tag. Also
        // clears the genuine legacy rows so they regenerate under the full
        // contract, retaining the superseded document verbatim.
        "migrations/202_fix_legacy_tag_and_force_regeneration.sql",
        // `semantic_rules.provenance_floor` — how well-grounded the episodes a
        // rule was extracted from actually were, capped at `model_inference`
        // because extraction is judgement. Rules are injected into other
        // agents' prompts, so without a floor a rule extracted from prose is
        // indistinguishable from one extracted from tool output, and the
        // laundering runs outward into the whole ecology.
        "migrations/203_semantic_rule_provenance_floor.sql",
        // Restores `credit_ledger_tx_type_check`, declared by SEVENTEEN
        // migrations and present in none of them: each early attempt ran
        // DROP+ADD as two top-level statements, PgBouncer gave them separate
        // implicit transactions, the ADD failed against existing rows, the DROP
        // stayed committed, and this function logged it and carried on. The net
        // effect of every repair was to delete the constraint. `tx_type` is a
        // bare `&str` at every call site, so this CHECK is the only closed set
        // in the system.
        "migrations/204_restore_credit_ledger_tx_type_check.sql",
        // The assertion layer. `forecast_agent_claims.workspace_id` is NOT NULL,
        // so an agent evaluated outside a workspace had its quantified judgement
        // discarded — measured: 14 judgements, 14 discarded, 0 claims. An
        // assertion is what the agent said (immutable, flat on `episodes`); a
        // claim is that assertion bound to a driver (0..n). Verification is a
        // separate append-only log, because a mutable status column would
        // destroy the previous verdict and a rejected-then-reverified assertion
        // would read as plain "verified".
        "migrations/205_assertion_layer.sql",
        // Migration 202 used `taxonomy_provenance` to tell post-contract
        // profiles from legacy ones — the right discriminator, replacing a shape
        // test that mislabelled correct rows every reboot. But it then treated
        // post-contract as a proxy for CORRECT, and left those rows untouched.
        // `Antaxius beieri` is a bush-cricket profiled as a cerambycid beetle,
        // written under the contract and before `reconcile()` was wired, so
        // `enforce` passed it: the field was present, typed, and declared
        // Sourced. Canonical wins in place, superseded value retained.
        "migrations/206_reconcile_stale_post_contract_taxonomy.sql",
        // Declares `schema_migrations`, the ledger this loop writes to. Also
        // created inline by `ensure_migration_ledger` above, because a migration
        // that records migrations cannot record itself — that copy is the
        // bootstrap and this file is the declaration. Having it here is what lets
        // the schema-consistency lint see these columns; it correctly rejected
        // the first version of this work for referencing a table no migration
        // declared.
        "migrations/207_migration_ledger.sql",
        // Rebuilds `creatures` to reclaim 1,575 dropped column slots. Postgres
        // never releases a dropped column's attnum and the hard 1600 ceiling
        // counts them, so the table could no longer accept any column at all.
        // MUST run after 052/058/065 are guarded — they are, in the same release
        // — or the reclaimed space starts draining again at five slots per boot.
        // Self-limiting: the body is guarded on there being a dropped column, so
        // it is a no-op forever after it succeeds once.
        "migrations/208_rebuild_creatures_reclaim_attnums.sql",
    ];

    // Bootstrap the ledger before anything is recorded into it.
    //
    // Inline rather than a migration file, and the reason is unavoidable: a
    // migration that records migrations cannot record itself. Everything else
    // in this project lives in `migrations/`, and this is the one exception the
    // ordering forces. `IF NOT EXISTS` throughout, so it is as replay-safe as
    // the files it tracks.
    ensure_migration_ledger(db).await;

    for file in &migration_files {
        let started = std::time::Instant::now();
        match std::fs::read_to_string(file) {
            Ok(sql) => {
                println!("Running migration: {}", file);
                let digest = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(sql.as_bytes()))
                };
                match sqlx::raw_sql(&sql).execute(db).await {
                    Ok(_) => {
                        println!("Migration {} completed", file);
                        record_migration_attempt(
                            db,
                            file,
                            &digest,
                            "ok",
                            None,
                            started.elapsed().as_millis() as i32,
                        )
                        .await;
                    }
                    Err(e) => {
                        // Still does not panic. A migration that cannot apply
                        // must not be able to take the service down, because
                        // most failures here are genuinely benign replays of
                        // already-applied DDL.
                        //
                        // What changed is that the failure is now WRITTEN DOWN.
                        // `credit_ledger_tx_type_check` was declared by
                        // seventeen migrations and applied by none: each one
                        // dropped the constraint, failed to re-add it, and left
                        // exactly this line in a boot log nobody reads. A
                        // failure that is only ever printed is a failure nobody
                        // can be asked about.
                        eprintln!("Migration {} warning: {}", file, e);
                        record_migration_attempt(
                            db,
                            file,
                            &digest,
                            "failed",
                            Some(&e.to_string()),
                            started.elapsed().as_millis() as i32,
                        )
                        .await;
                    }
                }
            }
            Err(e) => {
                eprintln!("Could not read migration {}: {}", file, e);
                // A registered file that is missing from the image is a
                // different fault from SQL that failed, and worth its own
                // status: the deploy is not carrying what the code believes it
                // is carrying.
                record_migration_attempt(
                    db,
                    file,
                    "",
                    "unreadable",
                    Some(&e.to_string()),
                    started.elapsed().as_millis() as i32,
                )
                .await;
            }
        }
    }
}

/// Create the migration ledger, so there is somewhere to record the very first
/// run of the migration that declares it.
///
/// `migrations/207_migration_ledger.sql` is the authoritative declaration and
/// carries the reasoning; this is the bootstrap. Both are `IF NOT EXISTS`, so
/// whichever runs first wins and the other is a no-op. If they ever disagree,
/// 207 is right and this is the bug.
async fn ensure_migration_ledger(db: &PgPool) {
    let ddl = "CREATE TABLE IF NOT EXISTS public.schema_migrations (                    filename          TEXT PRIMARY KEY,                    content_sha256    TEXT NOT NULL,                    attempts          INTEGER NOT NULL DEFAULT 0,                    successes         INTEGER NOT NULL DEFAULT 0,                    failures          INTEGER NOT NULL DEFAULT 0,                    consecutive_failures INTEGER NOT NULL DEFAULT 0,                    first_succeeded_at   TIMESTAMPTZ,                    last_attempt_at   TIMESTAMPTZ NOT NULL DEFAULT now(),                    last_status       TEXT NOT NULL,                    last_error        TEXT,                    last_duration_ms  INTEGER                )";
    if let Err(e) = sqlx::raw_sql(ddl).execute(db).await {
        // Recording is best-effort by construction: a ledger that could fail
        // the boot would be a worse problem than the blindness it fixes.
        eprintln!("Could not ensure schema_migrations ledger: {}", e);
    }
}

/// Record one attempt, success or failure.
///
/// Upsert keyed on filename rather than append-per-boot. Migrations replay on
/// every start, so an append-only log would grow without bound and answer no
/// question the counters do not. What the counters do answer, and a print
/// cannot:
///
/// * has this migration EVER succeeded (`first_succeeded_at`)
/// * is it failing RIGHT NOW (`consecutive_failures`)
/// * has the file changed since it last applied (`content_sha256`)
///
/// `first_succeeded_at` is the field the rest of the verification work has been
/// missing. Without a record of when a migration landed, `liveness_trust` cannot
/// tell a write path that is broken from one whose sink was created five minutes
/// ago, and it currently carries a documented exemption saying exactly that.
async fn record_migration_attempt(
    db: &PgPool,
    filename: &str,
    sha: &str,
    status: &str,
    error: Option<&str>,
    duration_ms: i32,
) {
    let ok = status == "ok";
    let res = sqlx::query(
        "INSERT INTO public.schema_migrations              (filename, content_sha256, attempts, successes, failures,               consecutive_failures, first_succeeded_at, last_attempt_at,               last_status, last_error, last_duration_ms)          VALUES ($1, $2, 1, $3, $4, $5,                  CASE WHEN $6 THEN now() END, now(), $7, $8, $9)          ON CONFLICT (filename) DO UPDATE SET              content_sha256 = EXCLUDED.content_sha256,              attempts  = schema_migrations.attempts + 1,              successes = schema_migrations.successes + EXCLUDED.successes,              failures  = schema_migrations.failures + EXCLUDED.failures,              consecutive_failures = CASE WHEN $6 THEN 0                                          ELSE schema_migrations.consecutive_failures + 1 END,              first_succeeded_at = COALESCE(schema_migrations.first_succeeded_at,                                            EXCLUDED.first_succeeded_at),              last_attempt_at = now(),              last_status = EXCLUDED.last_status,              last_error = EXCLUDED.last_error,              last_duration_ms = EXCLUDED.last_duration_ms",
    )
    .bind(filename)
    .bind(sha)
    .bind(if ok { 1i32 } else { 0i32 })
    .bind(if ok { 0i32 } else { 1i32 })
    .bind(if ok { 0i32 } else { 1i32 })
    .bind(ok)
    .bind(status)
    .bind(error)
    .bind(duration_ms)
    .execute(db)
    .await;

    if let Err(e) = res {
        eprintln!("Could not record migration attempt for {}: {}", filename, e);
    }
}

/// Belt-and-suspenders schema ensure. Each ALTER is its own single-statement
/// sqlx::query — bypasses any interaction between raw_sql, DO blocks, and
/// PgBouncer in transaction mode that has eaten multi-statement DDL in the
/// past. Run after `run_migrations`. Idempotent.
async fn ensure_critical_schema(db: &PgPool) {
    // (label, ALTER statement) — keep tight, only columns whose
    // absence causes user-facing 500s on the workspace and dashboard
    // surfaces.
    let alters: &[(&str, &str)] = &[
        ("teams.mission",
         "ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS mission TEXT"),
        ("teams.coordination_strategist_id",
         "ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS coordination_strategist_id UUID"),
        ("teams.strategist_assigned_at",
         "ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS strategist_assigned_at TIMESTAMPTZ"),
        // composition_versions table — migration 113 uses raw_sql which
        // PgBouncer in transaction mode can split; belt-and-suspenders
        // CREATE TABLE IF NOT EXISTS ensures the table always exists.
        ("composition_versions.table",
         "CREATE TABLE IF NOT EXISTS public.composition_versions ( \
              composition_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              workspace_id UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE, \
              version_number INT NOT NULL, \
              mission TEXT, \
              coordination_strategist_id UUID, \
              member_agent_ids UUID[], \
              member_weights JSONB, \
              diff_summary TEXT, \
              proposed_by TEXT, \
              accepted_by TEXT, \
              rejected_by TEXT, \
              rejection_note TEXT, \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW() \
          )"),
        ("composition_versions.rejected_by",
         "ALTER TABLE public.composition_versions ADD COLUMN IF NOT EXISTS rejected_by TEXT"),
        ("composition_versions.rejection_note",
         "ALTER TABLE public.composition_versions ADD COLUMN IF NOT EXISTS rejection_note TEXT"),

        // ── Forecast benchmark tables (migration 140) ─────────────
        // Migration 140 runs through sqlx::raw_sql, which PgBouncer in
        // transaction mode can split at the CREATE OR REPLACE FUNCTION
        // dollar-quoted body. When that happens, the entire script aborts
        // mid-way and the three tables never get created. Symptom: every
        // call to /api/benchmark/anchor-sweep and /api/forecasts/:id/spacetime
        // 500s with `relation "forecast_commitments" does not exist`.
        //
        // Each CREATE TABLE here is a single statement, so PgBouncer can't
        // split it. The trigger/function in 140 is non-essential for the
        // anchor-sweep path (the handler writes directly), so we don't
        // restore the trigger here — only the tables it depends on.
        ("harness_snapshots.table",
         "CREATE TABLE IF NOT EXISTS public.harness_snapshots ( \
              snapshot_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              content_hash TEXT NOT NULL UNIQUE, \
              conductor_card_hash TEXT NOT NULL, \
              routing_weights_hash TEXT, \
              specialist_roster_hash TEXT NOT NULL, \
              bayesops_params_hash TEXT, \
              conductor_version TEXT NOT NULL, \
              specialist_roster JSONB NOT NULL, \
              routing_weights JSONB, \
              bayesops_params JSONB, \
              parent_hash TEXT, \
              surface_changed TEXT, \
              change_rationale TEXT, \
              captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW() \
          )"),
        // fermi_forecast_updates is declared in migration 094 but some
        // deploys (notably the live one) appear to be missing it — the
        // /update-probability endpoint 500s with "relation does not exist".
        // Ensure it here as critical schema so the rate-update path works
        // even when the migration history is incomplete. Schema mirrors
        // 094§143.
        ("fermi_forecast_updates.table",
         "CREATE TABLE IF NOT EXISTS public.fermi_forecast_updates ( \
              id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, \
              forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, \
              previous_probability REAL NOT NULL, \
              new_probability REAL NOT NULL, \
              reason TEXT, \
              agent_id TEXT, \
              evidence_added JSONB, \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              revision_trigger TEXT \
          )"),
        // Migration 150 adds 'cascade' to the legal revision_trigger
        // values. Drop+recreate (PG can't ALTER CHECK in place).
        ("fermi_forecast_updates.drop_old_check",
         "ALTER TABLE public.fermi_forecast_updates \
          DROP CONSTRAINT IF EXISTS fermi_forecast_updates_revision_trigger_check"),
        ("fermi_forecast_updates.add_check",
         "ALTER TABLE public.fermi_forecast_updates \
          ADD CONSTRAINT fermi_forecast_updates_revision_trigger_check \
          CHECK ( \
              revision_trigger IS NULL OR revision_trigger IN ( \
                  'initial', 'evidence_update', 'agent_correction', \
                   'schedule_rerun', 'manual', 'bayesops_refit', 'cascade', 'cascade_undo' \
              ) \
          )"),
        ("fermi_forecast_updates.idx_forecast",
         "CREATE INDEX IF NOT EXISTS idx_forecast_updates_forecast \
          ON public.fermi_forecast_updates(forecast_id)"),
        ("fermi_forecast_updates.idx_time",
         "CREATE INDEX IF NOT EXISTS idx_forecast_updates_time \
          ON public.fermi_forecast_updates(created_at)"),

        // ── 150: forecast_relationships — declarable inter-forecast
        //    dependencies. See src/handlers/relationships.rs.
        ("forecast_relationships.table",
         "CREATE TABLE IF NOT EXISTS public.forecast_relationships ( \
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              kind TEXT NOT NULL, \
              forecast_ids TEXT[] NOT NULL, \
              parameters JSONB NOT NULL DEFAULT '{}'::jsonb, \
              description TEXT, \
              owner_id TEXT NOT NULL, \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              archived_at TIMESTAMPTZ \
          )"),
        ("forecast_relationships.idx_kind",
         "CREATE INDEX IF NOT EXISTS idx_relationships_kind \
          ON public.forecast_relationships(kind)"),
        ("forecast_relationships.idx_owner",
         "CREATE INDEX IF NOT EXISTS idx_relationships_owner \
          ON public.forecast_relationships(owner_id)"),
        ("forecast_relationships.idx_forecast_ids",
         "CREATE INDEX IF NOT EXISTS idx_relationships_forecast_ids \
          ON public.forecast_relationships USING gin (forecast_ids)"),

        // ── 151: pending_cascades — operator-gated cascade queue.
        //    When a forecast resolves (manually OR via upstream
        //    workspace resolution), the server queues a pending_cascade
        //    row for each non-archived relationship the resolved
        //    forecast is part of. The operator reviews and applies
        //    or dismisses from the console queue. Operator-gate rule:
        //    nothing that mutates probabilities fires without a click.
        ("pending_cascades.table",
         "CREATE TABLE IF NOT EXISTS public.pending_cascades ( \
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              relationship_id UUID NOT NULL REFERENCES public.forecast_relationships(id) ON DELETE CASCADE, \
              trigger_forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, \
              trigger_kind TEXT NOT NULL, \
              outcome BOOLEAN, \
              source TEXT NOT NULL DEFAULT 'manual', \
              status TEXT NOT NULL DEFAULT 'pending', \
              owner_id TEXT NOT NULL, \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              decided_at TIMESTAMPTZ, \
              decided_by TEXT, \
              notes TEXT, \
              proposed_snapshot JSONB \
          )"),
        ("pending_cascades.idx_status",
         "CREATE INDEX IF NOT EXISTS idx_pending_cascades_status \
          ON public.pending_cascades(status)"),
        ("pending_cascades.idx_owner",
         "CREATE INDEX IF NOT EXISTS idx_pending_cascades_owner \
          ON public.pending_cascades(owner_id, status, created_at DESC)"),
        ("pending_cascades.idx_trigger",
         "CREATE INDEX IF NOT EXISTS idx_pending_cascades_trigger \
          ON public.pending_cascades(trigger_forecast_id)"),
        ("pending_cascades.idx_relationship",
         "CREATE INDEX IF NOT EXISTS idx_pending_cascades_relationship \
          ON public.pending_cascades(relationship_id)"),
        ("pending_cascades.drop_old_check",
         "ALTER TABLE public.pending_cascades \
          DROP CONSTRAINT IF EXISTS pending_cascades_status_check"),
        ("pending_cascades.add_check",
         "ALTER TABLE public.pending_cascades \
          ADD CONSTRAINT pending_cascades_status_check \
          CHECK (status IN ('pending', 'applied', 'dismissed', 'superseded'))"),

        ("forecast_commitments.table",
         "CREATE TABLE IF NOT EXISTS public.forecast_commitments ( \
              commitment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              forecast_id TEXT REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, \
              revision_id TEXT REFERENCES public.fermi_forecast_updates(id) ON DELETE SET NULL, \
              predicted_probability REAL NOT NULL, \
              fpl_source_hash TEXT, \
              harness_snapshot_id UUID REFERENCES public.harness_snapshots(snapshot_id), \
              commitment_hash TEXT NOT NULL UNIQUE, \
              anchor_method TEXT NOT NULL DEFAULT 'db_timestamp', \
              anchor_ref TEXT, \
              anchor_note TEXT, \
              emitted_at TIMESTAMPTZ NOT NULL, \
              committed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              sosa_projection_id TEXT, \
              CONSTRAINT commitment_has_subject CHECK ( \
                  forecast_id IS NOT NULL OR sosa_projection_id IS NOT NULL \
              ) \
          )"),
        ("forecast_commitments.idx_forecast",
         "CREATE INDEX IF NOT EXISTS idx_forecast_commitments_forecast \
          ON public.forecast_commitments(forecast_id, committed_at DESC)"),
        ("forecast_commitments.idx_hash",
         "CREATE INDEX IF NOT EXISTS idx_forecast_commitments_hash \
          ON public.forecast_commitments(commitment_hash)"),
        ("forecast_splits.table",
         "CREATE TABLE IF NOT EXISTS public.forecast_splits ( \
              forecast_id TEXT PRIMARY KEY REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, \
              split TEXT NOT NULL CHECK (split IN ('held_in','held_out','validation')), \
              split_hash_input TEXT NOT NULL, \
              split_salt TEXT NOT NULL, \
              assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              contamination_status TEXT NOT NULL DEFAULT 'pending' \
                  CHECK (contamination_status IN ('pending','clean','contaminated','exempt')), \
              probe_transcript TEXT, \
              evidence_freeze_cutoff TIMESTAMPTZ \
          )"),
        ("forecast_spacetime.table",
         "CREATE TABLE IF NOT EXISTS public.forecast_spacetime ( \
              spacetime_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, \
              revision_seq INTEGER NOT NULL DEFAULT 0, \
              predicted_probability REAL NOT NULL, \
              previous_probability REAL, \
              revision_trigger TEXT, \
              revision_reason TEXT, \
              triggering_agent TEXT, \
              evidence_delta JSONB, \
              drivers_snapshot JSONB, \
              base_rate_snapshot JSONB, \
              fpl_snapshot TEXT, \
              sobol_snapshot JSONB, \
              harness_snapshot_id UUID REFERENCES public.harness_snapshots(snapshot_id), \
              brier_at_this_point REAL, \
              loop1_signal JSONB, \
              loop3_coherence REAL, \
              loop5_calibration JSONB, \
              committed_at TIMESTAMPTZ, \
              revision_ts TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              UNIQUE (forecast_id, revision_seq) \
          )"),
        ("forecast_spacetime.idx_forecast",
         "CREATE INDEX IF NOT EXISTS idx_spacetime_forecast \
          ON public.forecast_spacetime(forecast_id, revision_seq ASC)"),

        // Trigger function that propagates every insert into
        // fermi_forecast_updates into a new forecast_spacetime row, which
        // is what the Trajectory tab reads. Without this the
        // /update-probability handler succeeds but the trajectory endpoint
        // shows no rate movement and the cockpit's spacetime view stays
        // frozen at the initial probability. Migration 140§201 + 149§49
        // declare the same function — we replicate it here so deploys with
        // an incomplete migrations history still get the propagation wire.
        ("forecast_spacetime.trigger_fn",
         "CREATE OR REPLACE FUNCTION public.fn_forecast_spacetime_on_update() \
          RETURNS TRIGGER LANGUAGE plpgsql AS $$ \
          BEGIN \
              INSERT INTO public.forecast_spacetime ( \
                  forecast_id, revision_seq, predicted_probability, previous_probability, \
                  revision_trigger, revision_reason, triggering_agent, evidence_delta, \
                  fpl_snapshot, revision_ts \
              ) \
              SELECT \
                  NEW.forecast_id, \
                  COALESCE(( \
                      SELECT MAX(revision_seq) + 1 \
                      FROM public.forecast_spacetime \
                      WHERE forecast_id = NEW.forecast_id \
                  ), 1), \
                  NEW.new_probability, \
                  NEW.previous_probability, \
                  COALESCE(NEW.revision_trigger, 'evidence_update'), \
                  NEW.reason, \
                  NEW.agent_id, \
                  NEW.evidence_added, \
                  (SELECT fpl_source FROM public.fermi_forecasts WHERE id = NEW.forecast_id), \
                  NEW.created_at; \
              RETURN NEW; \
          END; \
          $$"),
        // Drop + create as two separate sqlx execute() calls because the
        // sqlx::query layer doesn't run multi-statement strings.
        ("forecast_spacetime.trigger_drop",
         "DROP TRIGGER IF EXISTS trg_forecast_spacetime ON public.fermi_forecast_updates"),
        ("forecast_spacetime.trigger_create",
         "CREATE TRIGGER trg_forecast_spacetime \
              AFTER INSERT ON public.fermi_forecast_updates \
              FOR EACH ROW EXECUTE FUNCTION public.fn_forecast_spacetime_on_update()"),

        // ── 094: resolve_forecast + compute_brier_score ────────────
        //
        // These plpgsql functions live in migration 094 alongside the
        // fermi_forecasts table itself. On Vercel/serverless deploys the
        // multi-statement raw_sql execution of that file has sometimes
        // silently swallowed the function-creation half (PgBouncer
        // transaction-mode eating multi-statement DDL), producing the
        // user-facing error
        //   function resolve_forecast(text, boolean, text, text) does not exist
        // when an operator clicks Resolve on any forecast.
        //
        // Re-declare them here as single-statement CREATE OR REPLACE so
        // each is its own sqlx::query() round-trip — immune to whatever
        // ate them the first time. Idempotent and cheap.
        ("fn.compute_brier_score",
         "CREATE OR REPLACE FUNCTION public.compute_brier_score( \
              predicted REAL, \
              actual BOOLEAN \
          ) RETURNS REAL AS $$ \
          BEGIN \
              RETURN (predicted - (CASE WHEN actual THEN 1.0 ELSE 0.0 END)) ^ 2; \
          END; \
          $$ LANGUAGE plpgsql IMMUTABLE"),
        ("fn.resolve_forecast",
         "CREATE OR REPLACE FUNCTION public.resolve_forecast( \
              p_forecast_id TEXT, \
              p_actual_outcome BOOLEAN, \
              p_resolved_by TEXT, \
              p_resolution_notes TEXT DEFAULT NULL \
          ) RETURNS REAL AS $$ \
          DECLARE \
              v_predicted REAL; \
              v_brier REAL; \
              v_status TEXT; \
          BEGIN \
              SELECT predicted_probability, status \
                INTO v_predicted, v_status \
                FROM public.fermi_forecasts \
               WHERE id = p_forecast_id \
               FOR UPDATE; \
              IF NOT FOUND THEN \
                  RAISE EXCEPTION 'Forecast % not found', p_forecast_id; \
              END IF; \
              IF v_status != 'active' THEN \
                  RAISE EXCEPTION 'Forecast % is not active (status: %)', p_forecast_id, v_status; \
              END IF; \
              v_brier := public.compute_brier_score(v_predicted, p_actual_outcome); \
              UPDATE public.fermi_forecasts SET \
                  actual_outcome = p_actual_outcome, \
                  brier_score = v_brier, \
                  scored_probability = v_predicted, \
                  resolution_source = COALESCE(resolution_source, 'operator'), \
                  status = 'resolved', \
                  resolved_at = NOW(), \
                  resolved_by = p_resolved_by, \
                  resolution_notes = p_resolution_notes, \
                  updated_at = NOW() \
              WHERE id = p_forecast_id; \
              RETURN v_brier; \
          END; \
          $$ LANGUAGE plpgsql"),

        // ── 099: fermi_market_observations + indexes ───────────
        //
        // Append-only Polymarket snapshot log. Every POST
        // /api/polymarket/snapshot (including the console's 5-minute
        // background poll and the operator's manual ↻ Refresh) writes
        // one row here. When the table is missing, every write dies
        // with `relation "fermi_market_observations" does not exist`,
        // .map_err(...).ok() swallows it, and the trajectory view
        // silently shows zero crowd ticks forever — that's how we
        // ended up here in prod. Adding the CREATE TABLE + indexes to
        // ensure_critical_schema makes the observations infrastructure
        // self-heal on every deploy, matching the treatment we give
        // fermi_forecasts / forecast_relationships / pending_cascades.
        ("fermi_market_observations.table",
         "CREATE TABLE IF NOT EXISTS public.fermi_market_observations ( \
              id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, \
              forecast_id TEXT REFERENCES public.fermi_forecasts(id) ON DELETE SET NULL, \
              pm_event_id TEXT NOT NULL, \
              pm_market_id TEXT NOT NULL, \
              pm_condition_id TEXT, \
              pm_slug TEXT, \
              pm_question TEXT NOT NULL, \
              pm_event_title TEXT, \
              market_price REAL NOT NULL CHECK (market_price >= 0 AND market_price <= 1), \
              bid_price REAL CHECK (bid_price IS NULL OR (bid_price >= 0 AND bid_price <= 1)), \
              ask_price REAL CHECK (ask_price IS NULL OR (ask_price >= 0 AND ask_price <= 1)), \
              midpoint_price REAL CHECK (midpoint_price IS NULL OR (midpoint_price >= 0 AND midpoint_price <= 1)), \
              spread REAL, \
              volume_total REAL, \
              volume_24h REAL, \
              liquidity REAL, \
              price_change_1h REAL, \
              price_change_1d REAL, \
              price_change_1w REAL, \
              price_change_1m REAL, \
              pm_end_date TIMESTAMPTZ, \
              pm_active BOOLEAN NOT NULL DEFAULT true, \
              pm_closed BOOLEAN NOT NULL DEFAULT false, \
              pm_resolved BOOLEAN NOT NULL DEFAULT false, \
              pm_outcome TEXT, \
              fermi_probability REAL CHECK (fermi_probability IS NULL OR (fermi_probability >= 0 AND fermi_probability <= 1)), \
              divergence_pp REAL, \
              confidence_signal TEXT CHECK (confidence_signal IS NULL OR confidence_signal IN ('very_high', 'high', 'medium', 'low')), \
              observer_id TEXT NOT NULL REFERENCES public.users(user_id), \
              observation_type TEXT NOT NULL DEFAULT 'search' CHECK (observation_type IN ('search', 'import', 'manual_link', 'refresh', 'scheduled', 'agent_research', 'resolution_check')), \
              tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[], \
              metadata JSONB NOT NULL DEFAULT '{}'::jsonb, \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW() \
          )"),
        ("fermi_market_observations.idx_forecast",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_forecast \
              ON public.fermi_market_observations(forecast_id, created_at) \
            WHERE forecast_id IS NOT NULL"),
        ("fermi_market_observations.idx_pm_market",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_pm_market \
              ON public.fermi_market_observations(pm_market_id, created_at)"),
        ("fermi_market_observations.idx_pm_event",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_pm_event \
              ON public.fermi_market_observations(pm_event_id)"),
        ("fermi_market_observations.idx_observer",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_observer \
              ON public.fermi_market_observations(observer_id, created_at)"),
        ("fermi_market_observations.idx_type",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_type \
              ON public.fermi_market_observations(observation_type)"),
        ("fermi_market_observations.idx_unresolved",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_unresolved \
              ON public.fermi_market_observations(pm_market_id, created_at) \
            WHERE forecast_id IS NOT NULL \
              AND pm_closed = false \
              AND pm_resolved = false"),
        ("fermi_market_observations.idx_resolved",
         "CREATE INDEX IF NOT EXISTS idx_market_obs_resolved \
              ON public.fermi_market_observations(forecast_id, created_at) \
            WHERE pm_resolved = true"),

        // ── 151: forecast_invites (unified invite primitive) ─────────
        //
        // Backs the three Spec 24 collab flows (share forecast, share
        // portfolio, join team). The console's Access tab, teams panel,
        // and Inbox all query this. Same failure class as 099/094: if
        // migration 151 didn't apply, every invite POST 500s and every
        // Inbox render silently shows empty. Bake in the CREATE + three
        // partial indexes so team/invite flows self-heal on deploy.
        ("forecast_invites.table",
         "CREATE TABLE IF NOT EXISTS public.forecast_invites ( \
              id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
              target_type TEXT NOT NULL CHECK (target_type IN ('forecast', 'portfolio', 'team')), \
              target_id TEXT NOT NULL, \
              permission TEXT NOT NULL CHECK (permission IN ('view', 'edit', 'admin', 'owner', 'member', 'viewer')), \
              invitee_user_id TEXT, \
              invitee_email TEXT, \
              token TEXT UNIQUE, \
              inviter_id TEXT NOT NULL, \
              message TEXT, \
              status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'declined', 'revoked', 'expired')), \
              expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '14 days', \
              created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), \
              accepted_at TIMESTAMPTZ, \
              CONSTRAINT forecast_invites_recipient_exactly_one \
                CHECK ((invitee_user_id IS NOT NULL AND invitee_email IS NULL) \
                    OR (invitee_user_id IS NULL AND invitee_email IS NOT NULL)) \
          )"),
        ("forecast_invites.idx_recipient_user",
         "CREATE INDEX IF NOT EXISTS idx_invites_recipient_user \
              ON public.forecast_invites(invitee_user_id) \
            WHERE invitee_user_id IS NOT NULL AND status = 'pending'"),
        ("forecast_invites.idx_recipient_email",
         "CREATE INDEX IF NOT EXISTS idx_invites_recipient_email \
              ON public.forecast_invites(LOWER(invitee_email)) \
            WHERE invitee_email IS NOT NULL AND status = 'pending'"),
        ("forecast_invites.idx_target",
         "CREATE INDEX IF NOT EXISTS idx_invites_target \
              ON public.forecast_invites(target_type, target_id) \
            WHERE status = 'pending'"),

        // v0.10.26: `agents.updated_at` was declared in mig-166 via a
        // DO $$ block, which — exactly per the header comment on this
        // function — PgBouncer in transaction mode ate silently on
        // Ivan's deploy. Symptom is the exact error he hit AGAIN
        // (post-v0.10.18) when trying to force-publish Mario's
        // `key_metrics` agent:
        //
        //   Publish failed: 400 DB error: column "updated_at" of
        //   relation "agents" does not exist
        //
        // Four write sites reference the column (publish_agent,
        // archive_agent, restore_agent, update_fork_pricing_handler);
        // every one 500's until this ALTER lands. Single-statement
        // form so PgBouncer can't split it. Idempotent via IF NOT
        // EXISTS. NOT NULL DEFAULT NOW() so future INSERTs get it
        // automatically. Backfill of existing rows is a follow-up
        // one-liner if any rows end up NULL (default fires on
        // INSERT but not on existing rows added between the
        // migration path and this ensure).
        ("agents.updated_at",
         "ALTER TABLE public.agents \
            ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()"),
        // Belt-and-braces backfill for any rows created between the
        // failed mig-166 attempt and this ensure. NULL updated_at is
        // impossible now (NOT NULL DEFAULT NOW() enforces it on ADD),
        // but PostgreSQL versions handle ADD COLUMN NOT NULL DEFAULT
        // for existing rows by treating the default as retroactive, so
        // this is a no-op on new PG and a fill-in on older PG. Kept
        // in the ensure so re-runs converge.
        ("agents.updated_at.backfill",
         "UPDATE public.agents SET updated_at = created_at WHERE updated_at IS NULL"),

        // v0.11.3: counterfactual_probability. Companion to
        // fermi_forecasts.counterfactual_brier from mig-172. The client
        // (Fermi harness) computes what its naive-average baseline
        // would have said and sends it at forecast creation time; the
        // server just persists. At resolution we compute
        // counterfactual_brier = (counterfactual_probability -
        // actual_outcome::int)^2 alongside the actual brier_score.
        // Team Brier − Counterfactual Brier = manager-effect delta.
        // Nullable — legacy forecasts and non-Fermi forecasts don't
        // populate it.
        ("fermi_forecasts.counterfactual_probability",
         "ALTER TABLE public.fermi_forecasts \
            ADD COLUMN IF NOT EXISTS counterfactual_probability REAL \
              CHECK (counterfactual_probability IS NULL OR (counterfactual_probability >= 0 AND counterfactual_probability <= 1))"),

        // ── mig-174: Brier integrity ───────────────────────────────
        //
        // Migration 174 creates two columns, a plpgsql trigger function
        // with a dollar-quoted body, a trigger, and a view. That is
        // exactly the multi-statement shape PgBouncer transaction mode
        // has eaten before (see the 094 and 140 notes above). If the
        // raw_sql run aborts at the function body, the columns silently
        // never appear and every resolution writes a NULL audit anchor.
        //
        // Re-declare the load-bearing parts as single statements. The
        // audit view is non-essential (nothing reads it at runtime) so
        // it is not restored here.
        ("fermi_forecasts.scored_probability",
         "ALTER TABLE public.fermi_forecasts \
            ADD COLUMN IF NOT EXISTS scored_probability REAL"),
        ("fermi_forecasts.resolution_source",
         "ALTER TABLE public.fermi_forecasts \
            ADD COLUMN IF NOT EXISTS resolution_source TEXT \
              CHECK (resolution_source IS NULL OR resolution_source IN ( \
                  'operator', 'polymarket_oracle', 'polymarket_price_heuristic', \
                  'workspace_upstream', 'backtest_seed', 'unknown'))"),
        ("fn.fermi_forecasts_freeze_resolved",
         "CREATE OR REPLACE FUNCTION public.fn_fermi_forecasts_freeze_resolved() \
          RETURNS TRIGGER LANGUAGE plpgsql AS $$ \
          BEGIN \
              IF OLD.status = 'resolved' AND NEW.status = 'resolved' THEN \
                  IF NEW.scored_probability IS DISTINCT FROM OLD.scored_probability THEN \
                      RAISE WARNING 'fermi_forecasts %: scored_probability is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.scored_probability, NEW.scored_probability; \
                      NEW.scored_probability := OLD.scored_probability; \
                  END IF; \
                  IF NEW.brier_score IS DISTINCT FROM OLD.brier_score THEN \
                      RAISE WARNING 'fermi_forecasts %: brier_score is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.brier_score, NEW.brier_score; \
                      NEW.brier_score := OLD.brier_score; \
                  END IF; \
                  IF NEW.actual_outcome IS DISTINCT FROM OLD.actual_outcome THEN \
                      RAISE WARNING 'fermi_forecasts %: actual_outcome is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.actual_outcome, NEW.actual_outcome; \
                      NEW.actual_outcome := OLD.actual_outcome; \
                  END IF; \
                  IF NEW.predicted_probability IS DISTINCT FROM OLD.predicted_probability THEN \
                      RAISE WARNING 'fermi_forecasts %: predicted_probability is frozen once resolved (attempted % -> %); keeping original. Filter on status = ''active'' in the calling UPDATE.', OLD.id, OLD.predicted_probability, NEW.predicted_probability; \
                      NEW.predicted_probability := OLD.predicted_probability; \
                  END IF; \
              END IF; \
              RETURN NEW; \
          END; \
          $$"),
        // Drop + create as two separate calls: the sqlx::query layer
        // doesn't run multi-statement strings.
        ("fermi_forecasts.freeze_trigger_drop",
         "DROP TRIGGER IF EXISTS trg_fermi_forecasts_freeze_resolved ON public.fermi_forecasts"),
        ("fermi_forecasts.freeze_trigger_create",
         "CREATE TRIGGER trg_fermi_forecasts_freeze_resolved \
              BEFORE UPDATE ON public.fermi_forecasts \
              FOR EACH ROW EXECUTE FUNCTION public.fn_fermi_forecasts_freeze_resolved()"),
    ];

    println!(
        "[ensure_critical_schema] running {} column ensures…",
        alters.len()
    );
    for (label, stmt) in alters {
        match sqlx::query(stmt).execute(db).await {
            Ok(_) => println!("[ensure_critical_schema] ✓ {}", label),
            Err(e) => eprintln!("[ensure_critical_schema] ✗ {} — {}", label, e),
        }
    }

    // Verify post-state and log it so Railway logs show exactly what landed.
    let probe = sqlx::query(
        "SELECT table_name, column_name
         FROM information_schema.columns
         WHERE table_schema = 'public'
           AND ((table_name = 'teams' AND column_name IN ('mission','coordination_strategist_id','strategist_assigned_at'))
             OR (table_name = 'composition_versions' AND column_name IN ('composition_version_id','rejected_by','rejection_note')))
         ORDER BY table_name, column_name",
    )
    .fetch_all(db)
    .await;

    match probe {
        Ok(rows) => {
            use sqlx::Row;
            let names: Vec<String> = rows
                .iter()
                .map(|r| {
                    format!(
                        "{}.{}",
                        r.try_get::<String, _>("table_name").unwrap_or_default(),
                        r.try_get::<String, _>("column_name").unwrap_or_default()
                    )
                })
                .collect();
            println!("[ensure_critical_schema] present: [{}]", names.join(", "));
        }
        Err(e) => eprintln!("[ensure_critical_schema] verification probe failed: {}", e),
    }

    // Also probe the forecast benchmark tables so we can see at-a-glance
    // in Railway logs whether migration 140 (or this ensure block) landed
    // them. Single query, returns each table name that exists.
    let bench_probe = sqlx::query(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name IN (
               'harness_snapshots',
               'forecast_commitments',
               'forecast_splits',
               'forecast_spacetime'
           )
         ORDER BY table_name",
    )
    .fetch_all(db)
    .await;

    match bench_probe {
        Ok(rows) => {
            use sqlx::Row;
            let names: Vec<String> = rows
                .iter()
                .map(|r| r.try_get::<String, _>("table_name").unwrap_or_default())
                .collect();
            println!(
                "[ensure_critical_schema] benchmark tables present: [{}] ({}/4)",
                names.join(", "),
                names.len()
            );
        }
        Err(e) => eprintln!("[ensure_critical_schema] benchmark probe failed: {}", e),
    }
}

/// Install a `tracing` subscriber.
///
/// ## Why this needed adding at all
///
/// Until v0.11.9 the `api-server` binary never initialised one. Every
/// `tracing::info!` / `warn!` / `error!` in the handler tree — including the
/// credit-flow royalty logs in `gas.rs`, the `pg_notify` failure paths, and
/// the embedding-provenance events — emitted into a no-op dispatcher and
/// was silently discarded. Roughly 100 structured log statements produced
/// nothing, which is why diagnosis has leaned entirely on `println!`.
///
/// This is also a hard prerequisite for rule-execution tracing (Phase 6 of
/// `docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md`): instrumentation
/// built on `tracing` writes to `/dev/null` without a subscriber.
///
/// ## Default level
///
/// `RUST_LOG` wins if set. Otherwise `info` for our own crates and `warn`
/// globally — deliberately conservative, because turning on ~100 previously
/// silent statements at `debug` would bury the boot diagnostics that
/// operators currently rely on. Raise per-target via e.g.
/// `RUST_LOG=fermi::handlers::billing=debug`.
fn init_tracing() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,fermi=info,fermi_auth=info,agent_bestiary_memory=info")
    });

    let registry = tracing_subscriber::registry().with(filter);

    // Structured JSON in production (Railway ships stdout to its log
    // aggregator); human-readable elsewhere.
    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        registry
            .with(fmt::layer().json().with_current_span(true))
            .init();
    } else {
        registry.with(fmt::layer().with_target(true)).init();
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "tracing subscriber installed"
    );
}

#[tokio::main]
async fn main() {
    init_tracing();

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
        .max_connections(25) // raised from 10 — headroom for concurrent LLM + API requests
        .min_connections(2) // keep warm connections for fast cold starts
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

    // Run pending migrations on startup (skip with SKIP_MIGRATIONS=1)
    if std::env::var("SKIP_MIGRATIONS").unwrap_or_default() != "1" {
        run_migrations(&db).await;
    } else {
        println!("Skipping migrations (SKIP_MIGRATIONS=1)");
    }

    // Belt-and-suspenders: ensure the columns that have repeatedly failed
    // to land via the multi-statement migration runner (PgBouncer + raw_sql
    // interaction is suspect) actually exist. Each ALTER below is its own
    // single-statement sqlx::query — bypasses any raw_sql / DO-block
    // weirdness. Logs the schema state so we can see in Railway logs
    // whether the columns are present.
    ensure_critical_schema(&db).await;

    // v0.11.0: schema trust contract check. Runs AFTER migrations and
    // ensure_critical_schema so any drift caught here is a genuine
    // contract violation, not just "migration hasn't run yet." Default
    // mode logs LOUDLY and continues; SCHEMA_STRICT=1 aborts boot on
    // drift (production-strict posture once the contract is
    // comprehensive). See src/schema_trust.rs.
    match schema_trust::verify_and_report(&db).await {
        schema_trust::BootDecision::Healthy => {}
        schema_trust::BootDecision::DriftContinueBoot => {
            eprintln!(
                "[main] schema drift detected — continuing boot in warn-only mode. \
                 GET /api/admin/schema-health for the JSON breakdown."
            );
        }
        schema_trust::BootDecision::DriftAbortBoot => {
            eprintln!(
                "[main] aborting boot due to SCHEMA_STRICT=1 + contract violations. \
                 Fix the drift (check migrations and ensure_critical_schema) and redeploy."
            );
            std::process::exit(2);
        }
    }

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
    let embedder: Arc<dyn EmbeddingGenerator> = if let Ok(api_key) = std::env::var("OPENAI_API_KEY")
    {
        // v0.10.26: OpenAI text-embedding-3-large @ 1024 dims (matches
        // the pgvector column + HNSW indices, so no schema migration).
        // Replaces AnthropicEmbeddings, which POSTed to
        // api.anthropic.com/v1/embeddings — an endpoint Anthropic does
        // not serve (404). That silently broke ALL embedding generation
        // (and thus consolidation/dreaming + semantic search) from the
        // Spec-22 portability work until now.
        println!("Using OpenAI embeddings (text-embedding-3-large @ 1024)");
        Arc::new(OpenAIEmbeddings::new(api_key))
    } else {
        // Loud, not silent: mock vectors make memory meaningless, and a
        // silent fallback is exactly how the previous outage hid for 6 weeks.
        eprintln!(
            "\u{26a0} NO OPENAI_API_KEY set \u{2014} falling back to MOCK embeddings. \
                 Consolidation/dreaming and semantic search will be meaningless. \
                 Set OPENAI_API_KEY to enable real embeddings."
        );
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
    seed_agents_to_database(&memory_store, &registry, &embedder).await;

    // SPEC_30 / mig-186 — classify any agent still lacking a taxonomy.
    //
    // The seeder covers agents that have a card on disk, and the create /
    // import handlers classify new ones at birth. Neither reaches agents
    // that were authored through the API *before* the column existed — the
    // 13 efra agents among them — because nothing re-creates them. This is
    // the one-time pass that does, and it is self-healing: it only touches
    // rows where `taxonomy IS NULL`, so it is a no-op on every later boot.
    //
    // Derives from the DB row through `fermi::taxonomy`, the same single
    // implementation the handlers use. Deliberately NOT expressed as SQL in
    // a migration: that would be a third copy of the derivation rules, and
    // the two that already exist are only safe because
    // `tests/taxonomy_parity.rs` holds them to each other.
    backfill_agent_taxonomy(&memory_store).await;
    println!("Agent seeding complete");

    // Seed registered Apps from apps/ directory (idempotent upsert by slug)
    println!("Seeding apps to database...");
    seed_apps_to_database(&db).await;
    println!("App seeding complete");

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

    let email_config = fermi::email::EmailConfig::from_env();
    if email_config.is_configured() {
        println!("Email configured (Resend transactional delivery enabled)");
    } else {
        eprintln!(
            "Note: RESEND_API_KEY not set. Invite emails will be no-ops \
             — operators can still use the copy-link affordance."
        );
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
        email: email_config,
        rate_limits: RateLimitConfig::from_env(),
        ws_broadcast: broadcast::channel::<WorkspaceEvent>(256).0,
        rabble_broadcast: broadcast::channel::<RabbleEvent>(256).0,
        creature_broadcast: broadcast::channel::<CreatureEvent>(512).0,
        secret_encryptor: fermi_auth::SecretEncryptor::from_env().ok().map(Arc::new),
        // Spec 22 §Security — empty at boot, fills as owners request export.
        export_consent: Arc::new(dashmap::DashMap::new()),
        // Spec 21 Phase 4.1 — shared pg_notify broadcast (single Postgres LISTEN
        // connection shared across all SSE subscribers).
        pg_notify: {
            let (tx, _) = broadcast::channel::<(String, String)>(2048);
            // Spawn the shared listener — one Postgres connection total.
            let tx_bg = tx.clone();
            let db_url_bg = database_url.clone();
            tokio::spawn(async move {
                loop {
                    match sqlx::postgres::PgListener::connect(&db_url_bg).await {
                        Ok(mut listener) => {
                            if let Err(e) = listener
                                .listen_all(vec![
                                    "workspace_messages",
                                    "creature_events",
                                    "rabble_events",
                                ])
                                .await
                            {
                                tracing::error!("pg_notify listen_all failed: {e}");
                                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                continue;
                            }
                            tracing::info!("shared pg_notify listener connected");
                            loop {
                                match listener.recv().await {
                                    Ok(notif) => {
                                        let _ = tx_bg.send((
                                            notif.channel().to_string(),
                                            notif.payload().to_string(),
                                        ));
                                    }
                                    Err(e) => {
                                        tracing::warn!("pg_notify recv error: {e}, reconnecting");
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("pg_notify connect failed: {e}");
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            });
            tx
        },
        // Spec 14 §5.6 — empty at boot, fills as agents/operators fit posteriors.
        posterior_cache: Arc::new(dashmap::DashMap::new()),
        // Spec 23 R-1 — built-in extractors (binary_winner_id_match,
        // binary_field_value, scalar_field_value, scalar_difference).
        // New extractors are added by code change + server restart.
        extractor_registry: posterior::ExtractorRegistry::with_builtins(),
        // Spec 23 D8 Phase 2 — sqlx-backed BrierLookup for the evaluator system.
        brier_lookup: Arc::new(crate::handlers::eval_brier::BrierLookupSqlx::new(
            db.clone(),
        )),
    };

    if state.secret_encryptor.is_some() {
        println!("Secrets encryption configured");
    } else {
        eprintln!("Note: SECRETS_ENCRYPTION_KEY not set. User secrets will be disabled.");
    }

    // P0 (credential model): migrate platform provider keys from env into the
    // abw-system credential store. The store is authoritative; env is a
    // one-time bootstrap seed, not the runtime source of truth. Idempotent:
    // only seeds the (abw-system, provider, '*') default when it's absent.
    // See docs/specs/AGENT_CREDENTIAL_MODEL.md.
    if let Some(encryptor) = state.secret_encryptor.as_ref() {
        // SPEC_28: seed EVERY provider the executor can dispatch to, not
        // just openai/anthropic. The executor no longer reads env at
        // runtime, so a platform-service agent on (say) deepseek is funded
        // only if its key reached the abw-system store here.
        for (env_var, provider) in [
            ("OPENAI_API_KEY", "openai"),
            ("ANTHROPIC_API_KEY", "anthropic"),
            ("MISTRAL_API_KEY", "mistral"),
            ("QWEN_API_KEY", "qwen"),
            ("OPENROUTER_API_KEY", "openrouter"),
            ("GLM_API_KEY", "glm"),
            ("DEEPSEEK_API_KEY", "deepseek"),
            ("KIMI_API_KEY", "kimi"),
            ("GEMINI_API_KEY", "gemini"),
        ] {
            if let Ok(key) = std::env::var(env_var) {
                match fermi_auth::bootstrap_agent_credential_if_absent(
                    &state.db,
                    encryptor,
                    "abw-system",
                    provider,
                    &key,
                )
                .await
                {
                    Ok(true) => println!(
                        "Bootstrapped abw-system '{}' credential from {} env var",
                        provider, env_var
                    ),
                    Ok(false) => {} // already in store — store is authoritative
                    Err(e) => eprintln!(
                        "Failed to bootstrap abw-system '{}' credential: {}",
                        provider, e
                    ),
                }
            }
        }
    }

    // Drive the Brier/calibration loop.
    //
    // Until v0.11.4 nothing scheduled resolution: a Polymarket-linked
    // forecast only resolved when an operator clicked "check resolutions"
    // in the console. Markets settled, forecasts stayed `active`, no
    // Brier was computed, and Loop 5 went cold. Paced and bounded; see
    // handlers::polymarket::spawn_resolution_sweeper. Disable with
    // PM_RESOLUTION_SWEEP_SECS=0.
    handlers::polymarket::spawn_resolution_sweeper(state.db.clone(), state.workspace_git.clone());

    // Spec 31: catch forecast state changes that bypassed the commit hook.
    //
    // Two writers still mutate forecasts without committing — the resolution
    // sweeper above and refit_workspace — and both are background tasks
    // holding only a pool. Rather than thread git through their call chains
    // (which fixes today's two gaps and nothing about tomorrow's), detect
    // the drift and fix it. commit_files_as no-ops on an unchanged tree, so
    // this is idempotent with the hooks rather than competing with them.
    // Disable with FERMI_HISTORY_RECONCILE_SECS=0.
    handlers::forecast_git::spawn_history_reconciler(state.db.clone(), state.workspace_git.clone());

    // Loop 1 → Loop 2: read the timeline entries live executions now write.
    //
    // `ObservabilityWorker::scan_agent` had three call sites, all of them
    // either the eval pipeline or a manual observatory endpoint. Nothing ran
    // it on a schedule, so drift and anomaly detection only happened when
    // someone asked. With live traffic producing entries, this is what turns
    // them into `anomaly_events` and therefore into HITL review items.
    // Disable with OBSERVABILITY_SCAN_SECS=0.
    handlers::live_observability::spawn_observability_sweeper(state.clone());

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
        // Fermi Console installer — the URL you send to non-technical
        // testers. See handlers::pages::install_page for the shape.
        // The script sibling is served from the same prefix so the
        // `curl -fsSL <host>/fermi-console/install.sh | bash` snippet
        // on the landing page resolves against a stable path.
        .route("/fermi-console/install", get(handlers::pages::install_page))
        .route(
            "/fermi-console/install.sh",
            get(handlers::pages::install_script),
        )
        // Binary download indirection — install script + in-app updater
        // both hit this so we can swap release backends without
        // touching testers. See handlers::pages::fermi_console_download.
        .route(
            "/fermi-console/download",
            get(handlers::pages::fermi_console_download),
        )
        .route("/apps", get(handlers::pages::apps_catalogue_view))
        .route("/apps/:slug", get(handlers::pages::app_detail_view))
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
        // Spec 24 §3.3 / Sprint 2.3b: invite preview by token. The
        // landing page renders before the recipient signs in; the
        // token is the credential. Auth-required accept variant is
        // on the protected router as /api/invites/by-token/:token/accept.
        .route(
            "/api/invites/by-token/:token",
            get(handlers::invites::get_invite_by_token_handler),
        )
        // Public landing page for invite links. The operator shares
        // /invites/<token> via any channel (email, Slack, WhatsApp);
        // the recipient clicks it, sees the invite details, and
        // accepts (or is prompted to sign in first). Client-side JS
        // in the template hits the /api/invites/by-token/... routes
        // above. Wired here on the public router so it works pre-auth.
        .route("/invites/:token", get(handlers::pages::invite_landing_view))
        // Per-agent MCP endpoints
        .route(
            "/mcp/agents/:agent_id",
            get(handlers::mcp::mcp_agent_manifest),
        )
        .route("/mcp/agents/:agent_id", post(handlers::mcp::mcp_agent_rpc))
        .route("/api/agents", get(handlers::agents::list_agents))
        // Single-agent fetch. Optional auth so anonymous visitors can
        // read published+public agents, while owners and admins see
        // their private/draft agents by direct URL. This is what makes
        // /agent/<name> resolve for third-party agents that aren't yet
        // published — previously the detail page relied on the paginated
        // list which silently excluded them.
        .route(
            "/api/agents/:agent_id",
            get(handlers::agents::get_agent_handler),
        )
        .route(
            "/api/agents/curated",
            get(handlers::agents::list_curated_agents_handler),
        )
        // App registry read endpoints — catalogue is browsable without auth.
        // The handlers accept Option<AuthPrincipal>: an authenticated caller
        // additionally sees their own private/unlisted Apps; an unauthed
        // caller sees only `visibility=public` rows.
        .route("/api/apps", get(handlers::apps::list_apps_handler))
        .route("/api/apps/:slug", get(handlers::apps::get_app_handler))
        .route(
            "/api/apps/:slug/schema",
            get(handlers::apps::get_app_schema_handler),
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
        // v0.9.2 — Agent funding status (marketplace signal).
        //
        // Replaces v0.9.0's per-agent secrets endpoints, which turned out
        // to overlap confusingly with ABW's profile page (the actual
        // source of truth for owner-uploaded keys). This is read-only:
        // owners upload keys on ABW; the console reads back whether an
        // agent is executable. See src/handlers/agent_funding.rs for the
        // full model + auth story.
        .route(
            "/api/agents/:agent_id/funding",
            get(handlers::agent_funding::get_agent_funding_handler),
        )
        // Remote MCP servers (outbound client direction). Reads and a
        // pre-save connection test; WRITES go through the normal
        // PUT /api/agents/:agent_id with an `mcp_servers` field so they
        // inherit RBAC, agent versioning, and the agent_card.updated
        // broadcast. Both are edit-gated: endpoints and credential key
        // names are operational detail, not catalogue metadata.
        .route(
            "/api/agents/:agent_id/mcp-servers",
            get(handlers::agents::get_agent_mcp_servers_handler),
        )
        .route(
            "/api/agents/:agent_id/mcp-servers/test",
            post(handlers::agents::test_agent_mcp_server_handler),
        )
        // Published tools (outbound server direction): what this agent
        // exposes over /mcp/agents/:id, plus the menu of what it could
        // expose. Writes go through PUT /api/agents/:agent_id with an
        // `mcp_tools` field, which validates every name against the
        // dispatch table so phantom tools can't be saved.
        .route(
            "/api/agents/:agent_id/published-tools",
            get(handlers::agents::get_agent_published_tools_handler),
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
            "/api/agents/:agent_id/eval/test-cases/generate-rubrics",
            post(handlers::eval::generate_rubrics_handler),
        )
        .route(
            "/api/me/eval/runs/batch",
            post(handlers::eval::batch_eval_run_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/run",
            post(handlers::eval::trigger_eval_run_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/runs",
            get(handlers::eval::list_eval_runs_handler),
        )
        .route(
            "/api/agents/:agent_id/eval/runs/:run_id/signals",
            get(handlers::eval::list_eval_signals_handler),
        )
        // ─── Phase 4 — Observatory (Plane D) ─────────────────────────
        // Fleet
        .route(
            "/api/observatory/fleet/summary",
            get(handlers::observatory::fleet_summary_handler),
        )
        .route(
            "/api/observatory/fleet/scan",
            post(handlers::observatory::fleet_scan_handler),
        )
        .route(
            "/api/observatory/fleet/agents",
            get(handlers::observatory::fleet_agents_handler),
        )
        // Loop 5a mechanism probe — asks whether the Brier chain moves a
        // signal correctly, which is a different question from whether the
        // resulting score is good. Admin-only: counts span all tenants.
        .route(
            "/api/observatory/loops/brier/mechanism",
            get(handlers::observatory::loop5_mechanism_handler),
        )
        // Loop 1 maturity — has an agent actually dreamt, and did the
        // ontologist build anything? Separates "the cycle ran" from "the agent
        // learned", which every previous surface conflated.
        .route(
            "/api/observatory/loops/dreaming/maturity",
            get(handlers::dreaming_maturity::fleet_dreaming_maturity_handler),
        )
        // Loop 4 — composition proposals derived from Shapley attribution.
        // `composition_versions` has had an accept/reject flow since mig-113
        // but nothing ever generated a proposal, so the loop was structurally
        // complete and permanently empty. GET computes; POST files one for a
        // human to decide on.
        .route(
            "/api/workspaces/:workspace_id/composition/suggestions",
            get(handlers::composition_evolution::composition_suggestions_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/composition/suggestions/materialise",
            post(handlers::composition_evolution::materialise_composition_proposal_handler),
        )
        // Dyads
        .route(
            "/api/observatory/dyads/auto-form",
            post(handlers::observatory::auto_form_dyads_handler),
        )
        .route(
            "/api/observatory/agents/:agent_id/backfill-social",
            post(handlers::observatory::backfill_social_handler),
        )
        .route(
            "/api/observatory/dyads/:dyad_id",
            axum::routing::patch(handlers::observatory::patch_dyad_profile_handler),
        )
        .route(
            "/api/observatory/agents/:agent_id/relationships",
            get(handlers::observatory::agent_relationships_handler),
        )
        // Per-agent
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
        // Per-agent RSI loop health. Replaces the observatory Loops tab's
        // client-side assembly, two rows of which were hardcoded constants
        // rendered under a live status column.
        .route(
            "/api/observatory/agents/:agent_id/loops",
            get(handlers::observatory::agent_loops_handler),
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
        // Phase 5 — two-reviewer consensus for agent_wide interventions
        .route(
            "/api/observatory/hitl/consensus/:request_id",
            post(handlers::observatory::confirm_two_reviewer_handler),
        )
        .route(
            "/api/agents/:agent_id/dependencies",
            get(handlers::agents::get_agent_dependencies_handler),
        )
        .route(
            "/api/agents/:id/calibration",
            get(handlers::agents::get_agent_calibration_handler),
        )
        .route(
            "/api/me/providers",
            get(handlers::agents::list_my_providers_handler),
        )
        .route(
            "/api/me/loop-health",
            get(handlers::agents::loop_health_handler),
        )
        .route(
            "/api/me/apps-health",
            get(handlers::apps::apps_health_handler),
        )
        .route(
            "/api/agents/:agent_id/versions",
            get(handlers::agents::list_agent_versions_handler),
        )
        .route(
            "/api/agents/:agent_id/versions/:version_num",
            get(handlers::agents::get_agent_version_handler),
        )
        // Doc 12 § Capability 4 — version partitioning is exposed as an
        // optional `?partition_by=version` query on the pre-existing
        // /api/agents/:id/calibration route declared above. No second
        // route needed; routing on the same prefix with two different
        // path-param names (`:id` vs `:agent_id`) is rejected by axum.
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
        // CLI login flow (localhost-callback OAuth → long-lived API key)
        .route("/auth/cli", get(handlers::auth::auth_cli_start))
        .route("/auth/cli/finish", get(handlers::auth::auth_cli_finish))
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
        // Ecology — population / habitats / provenance. The structural
        // counterpart to the Observatory's clinical view.
        .route("/ecology", get(handlers::pages::ecology_view))
        .route(
            "/api/ecology/overview",
            get(handlers::ecology::ecology_overview_handler),
        )
        .route(
            "/api/ecology/specimens",
            get(handlers::ecology::ecology_specimens_handler),
        )
        .route(
            "/observatory/hitl",
            get(handlers::pages::observatory_hitl_view),
        )
        // Layers run outermost-last: optional_auth → rate_limit →
        // impersonation_guard → handler. The guard must sit *inside*
        // the auth layer so the principal is already resolved.
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            impersonation_guard,
        ))
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
        // Spec 30: discrete powers over the team's WORK, orthogonal to
        // `role` (which administers the team). Currently just 'resolve' —
        // may take terminal, irreversible actions on the team's forecasts.
        // A single ladder couldn't express "help me work on these but don't
        // close them", which is why a portfolio team-share was silently
        // delegating scoring authority.
        .route(
            "/api/teams/:team_id/members/:member_id/capabilities",
            put(handlers::teams::set_member_capabilities_handler),
        )
        // Spec 24 §3.3 / Sprint 2.3a: invite someone to a team.
        // Permission vocab here is the team role (owner|admin|member|
        // viewer). The legacy POST /api/teams/:id/members stays for
        // tooling that adds members directly without an invite step.
        .route(
            "/api/teams/:team_id/invites",
            post(handlers::invites::invite_to_team_handler)
                .get(handlers::invites::list_team_invites_handler),
        )
        // ── Spec 26: team collaboration surfaces ─────────────────────
        //
        // Members-only, all three. The console's Teams panel used to
        // derive its Shared and Activity tabs client-side by filtering
        // the caller's OWN forecasts — which structurally could not show
        // work a teammate shared with the team. These three endpoints
        // are the server-side truth:
        //
        //   /shared        — inventory: what's shared, by whom, when, how
        //                    (direct vs inherited from a team portfolio)
        //   /activity      — attributed event feed over that surface,
        //                    filterable by ?actor= and ?kind=
        //   /contributions — per-member roll-up: revisions, resolutions,
        //                    authored, shares granted, curations, last
        //                    active. Turns the Roster from a name list
        //                    into a working document.
        .route(
            "/api/teams/:team_id/shared",
            get(handlers::collab::team_shared_handler),
        )
        .route(
            "/api/teams/:team_id/activity",
            get(handlers::collab::team_activity_handler),
        )
        .route(
            "/api/teams/:team_id/contributions",
            get(handlers::collab::team_contributions_handler),
        )
        // Spec 27: the ops board. Detected coordination work — nothing is
        // stored, every op is a condition currently true of the team's
        // shared surface, so the definition of done is the detector going
        // quiet. Safe and cheap to re-poll; re-polling is how ops clear.
        .route(
            "/api/teams/:team_id/ops",
            get(handlers::ops::team_ops_handler),
        )
        // ── Invite inbox + state-transition routes (Spec 24 §3.3) ──────
        //
        // The standalone /api/invites/:id verbs decouple the invite's
        // lifecycle from the target — decline/revoke don't care whether
        // it's a forecast/portfolio/team invite. Accept lands in
        // Sprint 2.3b along with the by-token landing routes.
        .route(
            "/api/me/invites",
            get(handlers::invites::list_my_invites_handler),
        )
        .route(
            "/api/me/invites/sent",
            get(handlers::invites::list_sent_invites_handler),
        )
        .route(
            "/api/invites/:invite_id/decline",
            post(handlers::invites::decline_invite_handler),
        )
        .route(
            "/api/invites/:invite_id",
            delete(handlers::invites::revoke_invite_handler),
        )
        // Sprint 2.3b: accept + by-token. The by-token GET is on the
        // public router (auth optional, since the link is the
        // credential); the accept variants require auth so the caller's
        // identity can be matched against the invite's intended
        // recipient.
        .route(
            "/api/invites/:invite_id/accept",
            post(handlers::invites::accept_invite_handler),
        )
        .route(
            "/api/invites/by-token/:token/accept",
            post(handlers::invites::accept_invite_by_token_handler),
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
        // Spec 22 §UX — Embedding Portability affordance
        .route(
            "/api/agents/:agent_id/embeddings/stats",
            get(handlers::agents::embeddings_stats_handler),
        )
        .route(
            "/api/agents/:agent_id/embeddings/export/consent",
            post(handlers::agents::embeddings_export_consent_handler),
        )
        .route(
            "/api/agents/:agent_id/embeddings/export",
            get(handlers::agents::embeddings_export_handler),
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
        // Consolidation trigger + async job status (Spec 21 Phase 3.1)
        .route(
            "/api/agents/:agent_id/consolidate",
            post(handlers::consolidation::consolidate_agent_handler),
        )
        .route(
            "/api/agents/:agent_id/dreaming",
            get(handlers::dreaming_maturity::agent_dreaming_maturity_handler),
        )
        // Evolution badge — earned progression across four loop-backed
        // dimensions, with a stored high-water mark so regression is visible.
        .route(
            "/api/agents/:agent_id/evolution",
            get(handlers::evolution::agent_evolution_handler),
        )
        .route(
            "/api/agents/:agent_id/consolidation/jobs/:job_id",
            get(handlers::consolidation::get_consolidation_job_handler),
        )
        // ── App registry (Doc 1 — App primitive) ──────────────────────────────
        // Read endpoints (GET /api/apps, GET /api/apps/:slug, GET
        // /api/apps/:slug/schema) are mounted on the *public* router below
        // so the catalogue is browsable without auth. The list handler
        // itself uses Option<AuthPrincipal> to surface caller-owned
        // private/unlisted Apps when a token is present.
        .route("/api/apps", post(handlers::apps::create_app_handler))
        .route(
            "/api/apps/:slug",
            put(handlers::apps::update_app_handler_full),
        )
        .route(
            "/api/apps/:slug/workspaces",
            post(handlers::apps::spawn_workspace_handler),
        )
        .route(
            "/api/apps/:slug/workspaces",
            get(handlers::apps::list_app_workspaces_handler),
        )
        .route(
            "/api/apps/:slug/workspaces/batch",
            post(handlers::apps::batch_spawn_workspaces_handler),
        )
        .route(
            "/api/apps/:slug/publish",
            post(handlers::apps::publish_app_handler),
        )
        .route(
            "/api/apps/:slug/archive",
            post(handlers::apps::archive_app_handler),
        )
        // Batch-reconcile auto_hire across all existing workspaces of an App.
        // Used when auto_hire is edited after workspaces have spawned. The
        // alternative (manual hire per workspace × per added agent) doesn't
        // scale once a fleet exists. Idempotent — safe to re-run.
        .route(
            "/api/apps/:slug/sync-auto-hire",
            post(handlers::apps::sync_auto_hire_handler),
        )
        // ── SimOps direct computation (no LLM — for Compose mode live feedback) ─
        .route(
            "/api/simops/cascade",
            post(handlers::simops::cascade_handler),
        )
        // ── SimOps distributional projection (Digital Twin "Generate distribution") ─
        .route(
            "/api/simops/project",
            post(handlers::simops::project_handler),
        )
        // ── SimOps slot-match binding suggestions (spec 36a A.1.1 + A.1.4) ──────
        .route(
            "/api/simops/cascade/suggest-bindings",
            post(handlers::workspace::actions::suggest_bindings_handler),
        )
        // ── SimOps dynamics (ODE time-series projection) ─────────────────────────
        .route(
            "/api/simops/dynamics",
            post(handlers::simops::dynamics_handler),
        )
        .route(
            "/api/simops/dynamics/models",
            get(handlers::simops::dynamics_list_handler),
        )
        // ── SimOps rheology (instantaneous fluid property calculator) ─────────
        .route(
            "/api/simops/rheology",
            post(handlers::simops::rheology_handler),
        )
        .route(
            "/api/simops/rheology/models",
            get(handlers::simops::rheology_list_handler),
        )
        // ── BayesOps (Spec 14 §5.6) — domain-neutral parameter fitting ─────────
        // Phase 1 (marginal): /fit_marginal
        // Phase 2 (conditional): /fit_conditional → /predict, /input_sensitivity,
        //                        /compare_scenarios, /prob_exceeds, /optimise_for_target
        // Posteriors are cached in-memory (session-scoped). Persistent posterior
        // store is Phase 5. No auth — these endpoints are pure compute.
        .route(
            "/api/bayesops/fit_marginal",
            post(handlers::bayesops::fit_marginal_handler),
        )
        .route(
            "/api/bayesops/fit_conditional",
            post(handlers::bayesops::fit_conditional_handler),
        )
        .route(
            "/api/bayesops/predict",
            post(handlers::bayesops::predict_handler),
        )
        .route(
            "/api/bayesops/input_sensitivity",
            post(handlers::bayesops::input_sensitivity_handler),
        )
        .route(
            "/api/bayesops/compare_scenarios",
            post(handlers::bayesops::compare_scenarios_handler),
        )
        .route(
            "/api/bayesops/prob_exceeds",
            post(handlers::bayesops::prob_exceeds_handler),
        )
        .route(
            "/api/bayesops/optimise_for_target",
            post(handlers::bayesops::optimise_for_target_handler),
        )
        .route(
            "/api/bayesops/posteriors",
            get(handlers::bayesops::list_posteriors_handler),
        )
        .route(
            "/api/bayesops/posteriors/:id",
            delete(handlers::bayesops::evict_posterior_handler),
        )
        // ── R-2: Sparkline UX endpoints (Spec 23 §4.3) ────────────────────────
        // Single round-trip for the editor to render every learnable-driver
        // sparkline in a forecast workspace, plus inline accept/reject.
        .route(
            "/api/workspaces/:workspace_id/bayesops/state",
            get(handlers::bayesops::workspace_bayesops_state_handler),
        )
        .route(
            "/api/bayesops/pending/:pending_id/accept",
            post(handlers::bayesops::accept_pending_handler),
        )
        .route(
            "/api/bayesops/pending/:pending_id/reject",
            post(handlers::bayesops::reject_pending_handler),
        )
        // ── BayesOps refit hook (Spec 23 R-1) ─────────────────────────────────
        // Manual trigger for the same refit_workspace function that fires
        // automatically post-commit from POST /api/workspaces/:id/resolve.
        // Useful for re-fitting after observation arrays are updated without
        // a full resolution, and for the cockpit's "refit now" button.
        .route(
            "/api/workspaces/:workspace_id/refit",
            post(handlers::workspace::refit::refit_workspace_handler),
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
        // ── Workspace-aware cascade (reads process + twin from workspace git) ──
        .route(
            "/api/workspaces/:workspace_id/cascade",
            post(handlers::simops::workspace_cascade_handler),
        )
        // ── SimOps benchmark: process spacetime + sample config ───────────────
        .route(
            "/api/simops/workspaces/:workspace_id/process-spacetime",
            get(handlers::simops_benchmark::process_spacetime_handler),
        )
        .route(
            "/api/simops/workspaces/:workspace_id/sample-config",
            get(handlers::simops_benchmark::get_sample_config_handler)
                .put(handlers::simops_benchmark::put_sample_config_handler),
        )
        // ── Generalised App action protocol ──────────────────────────────────
        // Six action types + list/pending/accept/reject + annotations.
        // Isomorphic across companion action blocks, abw CLI, and MCP tools/call.
        .route(
            "/api/workspaces/:workspace_id/actions",
            get(handlers::workspace::actions::list_actions_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/pending",
            get(handlers::workspace::actions::list_pending_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/mutate_document",
            post(handlers::workspace::actions::mutate_document_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/fork_state",
            post(handlers::workspace::actions::fork_state_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/compare",
            post(handlers::workspace::actions::compare_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/invoke_member",
            post(handlers::workspace::actions::invoke_member_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/annotate_schema",
            post(handlers::workspace::actions::annotate_schema_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/annotate",
            post(handlers::workspace::actions::annotate_handler),
        )
        // kask-wild: foraging observation log action
        .route(
            "/api/workspaces/:workspace_id/actions/log_observation",
            post(handlers::workspace::actions::log_observation_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/:action_id/accept",
            post(handlers::workspace::actions::accept_action_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/:action_id/reject",
            post(handlers::workspace::actions::reject_action_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/actions/migrate_parallelism_to_twin",
            post(handlers::workspace::actions::migrate_parallelism_to_twin_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/annotations",
            get(handlers::workspace::actions::list_annotations_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/annotations/:annotation_id",
            delete(handlers::workspace::actions::resolve_annotation_handler),
        )
        // Workspace outputs (typed KV store for cross-workspace data)
        .route(
            "/api/workspaces/:workspace_id/outputs",
            get(handlers::workspace::outputs::list_outputs_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/outputs/:key",
            put(handlers::workspace::outputs::set_output_handler)
                .get(handlers::workspace::outputs::get_output_handler)
                .delete(handlers::workspace::outputs::delete_output_handler),
        )
        // Workspace dependencies (DAG edges)
        .route(
            "/api/workspaces/:workspace_id/dependencies",
            get(handlers::workspace::outputs::list_dependencies_handler)
                .post(handlers::workspace::outputs::add_dependency_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/dependencies/:upstream_id",
            delete(handlers::workspace::outputs::remove_dependency_handler),
        )
        // Workspace resolution — universal lifecycle endpoint that
        // transitions workspace_status active → completed/failed and
        // is the single entry point BayesOps refits hook into.
        // See docs/fermi/WORKSPACE_RESOLUTION.md for the contract.
        .route(
            "/api/workspaces/:workspace_id/resolve",
            post(handlers::workspace::resolution::resolve_workspace_handler),
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
        // Fork a workspace to a draft App manifest (server-side introspection;
        // UI reviews + edits before POSTing to /api/apps)
        .route(
            "/api/workspaces/:workspace_id/fork-to-app",
            post(handlers::workspace::fork_workspace_to_app_draft_handler),
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
        // Xaman Ek session API
        .route(
            "/api/xaman/sessions",
            get(handlers::xaman::list_xaman_sessions_handler),
        )
        .route(
            "/api/xaman/sessions",
            post(handlers::xaman::create_xaman_session_handler),
        )
        .route(
            "/api/xaman/sessions/:id",
            get(handlers::xaman::get_xaman_session_handler),
        )
        .route(
            "/api/xaman/sessions/:id/message",
            post(handlers::xaman::xaman_session_message_handler),
        )
        .route(
            "/api/xaman/sessions/:id/complete",
            post(handlers::xaman::complete_xaman_session_handler),
        )
        .route(
            "/api/xaman/sessions/:id/create-app",
            post(handlers::xaman::create_app_from_session_handler),
        )
        .route(
            "/api/xaman/sessions/:id",
            delete(handlers::xaman::abandon_xaman_session_handler),
        )
        // Set workspace composition identity (mission + strategist)
        .route(
            "/api/workspaces/:workspace_id/composition/identity",
            post(handlers::workspace::set_composition_identity_handler),
        )
        // Composition version lifecycle (tune-team RSI)
        .route(
            "/api/workspaces/:workspace_id/composition/versions",
            get(handlers::composition::list_composition_versions_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/composition/versions/:version_id/accept",
            post(handlers::composition::accept_composition_version_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/composition/versions/:version_id/reject",
            post(handlers::composition::reject_composition_version_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/composition/propose",
            post(handlers::composition::propose_composition_version_handler),
        )
        .route(
            "/api/workspaces/:workspace_id/composition/dream",
            post(handlers::composition::composition_dream_handler),
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
        // Spec 24 §3.3: exact case-insensitive email lookup for the
        // share-with autocomplete. Returns one user_id or 404 — no
        // enumeration, no fuzzy match (use /search for that).
        .route(
            "/api/users/lookup",
            get(handlers::users::lookup_user_by_email_handler),
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
        // ── Stateless FPL execution (thick client ⌘R, external integrations)
        .route(
            "/api/fpl/execute",
            post(handlers::notebooks::fpl_execute_handler),
        )
        .route(
            "/api/fpl/health",
            get(handlers::notebooks::fpl_health_handler),
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
        // ── Benchmark routes ───────────────────────────────────────────
        .route(
            "/api/forecasts/:forecast_id/spacetime",
            get(handlers::forecast_benchmark::forecast_spacetime_handler),
        )
        // Spec 23 R-3 Piece 2: unified timeline aggregator. Pulls rate
        // revisions, BayesOps fit events, agent runs, system events, and
        // polymarket observations into one chronological event list +
        // separate trace arrays for the line chart.
        .route(
            "/api/forecasts/:forecast_id/timeline",
            get(handlers::forecast_benchmark::forecast_timeline_handler),
        )
        // Phase 2.5 (cascades). Redistribution waterfall for one forecast:
        // where its current probability came from in terms of upstream
        // resolutions that cascaded mass onto (or off) it. Read-only view
        // over fermi_forecast_updates rows with revision_trigger
        // ∈ {'cascade','cascade_undo'}. Drives the Provenance right-tab in
        // the cockpit; see docs/fermi/WORLD_CUP_ROADMAP.md.
        .route(
            "/api/forecasts/:forecast_id/cascade-provenance",
            get(handlers::forecast_benchmark::forecast_cascade_provenance_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/commit",
            post(handlers::forecast_benchmark::commit_forecast_handler),
        )
        .route(
            "/api/benchmark/anchor-sweep",
            post(handlers::forecast_benchmark::anchor_sweep_handler),
        )
        // ── Per-forecast sharing (Spec 24 §3.3) ────────────────────────
        //
        // Distinct from the generic /api/shares which performs zero
        // authorization. These routes pin object_type='forecast' at the
        // route level and gate writes on ownership of the forecast.
        .route(
            "/api/forecasts/:forecast_id/shares",
            get(handlers::shares::list_forecast_shares_handler)
                .post(handlers::shares::create_forecast_share_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/shares/:share_id",
            delete(handlers::shares::revoke_forecast_share_handler),
        )
        // ── Spec 26: collaboration surfaces ─────────────────────────
        //
        // /access answers "who can see this, and how" — direct shares,
        // shares inherited from a containing portfolio, and the
        // flattened effective-viewer list with teams expanded.
        // /activity answers "which teammate did which thing" from
        // derived events (no event-log table; see the module docs).
        .route(
            "/api/forecasts/:forecast_id/access",
            get(handlers::collab::forecast_access_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/activity",
            get(handlers::collab::forecast_activity_handler),
        )
        // ── Spec 31: forecast version history on the git substrate ──────
        //
        // ABW gives every workspace a real git repo and every forecast is
        // its own workspace — the substrate was built and completely idle
        // (zero git_repo_path values, one commit in total). These three
        // routes make it the forecast's version history.
        //
        // /history and /diff are VIEW-gated: if you can read a forecast you
        // can read how it got that way. Provenance isn't a privilege.
        //
        // /revert is EDIT-gated and is the load-bearing piece — shared
        // `edit` is only safe to hand out because any change can be undone
        // (Ward Cunningham: reversibility beats prevention). It writes a
        // forward commit rather than rewriting history, and restores the
        // analysis only — never the lifecycle, since mig-174 freezes the
        // scoring tuple and revert must not be a hole in that.
        .route(
            "/api/forecasts/:forecast_id/history",
            get(handlers::forecast_git::forecast_history_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/history/:sha",
            get(handlers::forecast_git::forecast_diff_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/revert",
            post(handlers::forecast_git::forecast_revert_handler),
        )
        // ── Spec 32: driver annotations ─────────────────────────────
        //
        // "Your base rate for elo_current is wrong, here's why" — anchored
        // to the driver, because disagreement here is almost never about
        // the question, it's about one input.
        //
        // Creating is VIEW-gated on purpose: a view grant exists so people
        // can read and react, and "you may see this but not say it's wrong"
        // would defeat the point of publishing. Annotating mutates no
        // forecast state.
        //
        // Resolving is EDIT-gated — accepting a challenge is a claim about
        // what the forecast now says. Delete is author-only, because
        // letting an editor erase an objection raised against their own
        // work is the one way this could hide disagreement instead of
        // surfacing it; they get 'declined', which stays on the record.
        .route(
            "/api/forecasts/:forecast_id/annotations",
            get(handlers::annotations::list_annotations_handler)
                .post(handlers::annotations::create_annotation_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/annotations/:annotation_id/resolve",
            post(handlers::annotations::resolve_annotation_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/annotations/:annotation_id",
            delete(handlers::annotations::delete_annotation_handler),
        )
        // Spec 24 §3.3 / Sprint 2.3a: invite someone to a forecast.
        // The invitee discovers the invite via /api/me/invites and
        // accepts in Sprint 2.3b. Permission vocab: view|edit|admin.
        .route(
            "/api/forecasts/:forecast_id/invites",
            post(handlers::invites::invite_to_forecast_handler)
                .get(handlers::invites::list_forecast_invites_handler),
        )
        // ── Forecast relationships (legacy — mig 150) ────────────────
        .route(
            "/api/forecast-relationships",
            post(handlers::relationships::legacy::create_relationship_handler)
                .get(handlers::relationships::legacy::list_relationships_handler),
        )
        .route(
            "/api/forecast-relationships/:rel_id",
            delete(handlers::relationships::legacy::delete_relationship_handler),
        )
        .route(
            "/api/forecast-relationships/:rel_id/propagate",
            post(handlers::relationships::legacy::propagate_relationship_handler),
        )
        // ── Relationship groups (Spec 25 §6.1) ────────────────────────
        .route(
            "/api/relationship-groups",
            post(handlers::relationships::groups::create_group_handler)
                .get(handlers::relationships::groups::list_groups_handler),
        )
        .route(
            "/api/relationship-groups/:group_id",
            get(handlers::relationships::groups::get_group_handler)
                .patch(handlers::relationships::groups::patch_group_handler)
                .delete(handlers::relationships::groups::delete_group_handler),
        )
        .route(
            "/api/relationship-groups/:group_id/members",
            get(handlers::relationships::groups::get_group_members_handler),
        )
        // Phase 2.5 Slice B: dry-run propagate for the cascade detail
        // panel's "what if I resolve this member NO?" preview.
        // Defaults dry_run=true; POST body carries trigger_forecast_id
        // + trigger_kind + outcome. See src/handlers/relationships/
        // groups.rs::preview_group_propagation_handler.
        .route(
            "/api/relationship-groups/:group_id/propagate",
            post(handlers::relationships::groups::preview_group_propagation_handler),
        )
        // ── Forecast group membership (Spec 25 §6.2) ────────────
        //
        // GET was previously omitted — the console's cockpit chip strip
        // fires this on every open_forecast to hydrate the chip strip.
        // Without the GET binding the client got HTTP 405 which surfaced
        // as "Failed to load: HTTP 405: ..." in the CASCADES row of the
        // composer header. The handler was already implemented, just
        // never wired.
        .route(
            "/api/forecasts/:forecast_id/groups",
            get(handlers::relationships::membership::get_forecast_groups_handler)
                .put(handlers::relationships::membership::set_forecast_groups_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/groups/:group_id",
            post(handlers::relationships::membership::add_forecast_to_group_handler)
                .delete(handlers::relationships::membership::remove_forecast_from_group_handler),
        )
        // ── Pending cascades — operator-gated cascade queue ──────────
        .route(
            "/api/pending-cascades",
            get(handlers::pending_cascades::list_pending_cascades_handler),
        )
        .route(
            "/api/pending-cascades/:cascade_id/apply",
            post(handlers::relationships::apply::apply_pending_cascade_handler),
        )
        .route(
            "/api/pending-cascades/:cascade_id/dismiss",
            post(handlers::pending_cascades::dismiss_pending_cascade_handler),
        )
        .route(
            "/api/pending-cascades/:cascade_id/undo",
            post(handlers::relationships::undo::undo_pending_cascade_handler),
        )
        .route(
            "/api/pending-cascades/requeue",
            post(handlers::relationships::requeue::requeue_cascade_handler),
        )
        .route(
            "/api/forecasts/:forecast_id/cascade-history",
            get(handlers::pending_cascades::cascade_history_handler),
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
        // ── Per-portfolio sharing (Spec 24 §3.3) ───────────────────────
        .route(
            "/api/portfolios/:portfolio_id/shares",
            get(handlers::shares::list_portfolio_shares_handler)
                .post(handlers::shares::create_portfolio_share_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id/shares/:share_id",
            delete(handlers::shares::revoke_portfolio_share_handler),
        )
        // Spec 26: portfolio collaboration surfaces. /access additionally
        // reports `cascades_to` — how many member forecasts inherit the
        // portfolio's grants — so the consequence of sharing a book is
        // legible before you click.
        .route(
            "/api/portfolios/:portfolio_id/access",
            get(handlers::collab::portfolio_access_handler),
        )
        .route(
            "/api/portfolios/:portfolio_id/activity",
            get(handlers::collab::portfolio_activity_handler),
        )
        // Spec 24 §3.3 / Sprint 2.3a: invite someone to a portfolio.
        .route(
            "/api/portfolios/:portfolio_id/invites",
            post(handlers::invites::invite_to_portfolio_handler)
                .get(handlers::invites::list_portfolio_invites_handler),
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
        // ── Admin "view as user" (SPEC_33) ─────────────────────────────
        // Read-only impersonation for support/debugging. The guard in
        // fermi_auth::middleware enforces the contract; these routes only
        // mint, end, and report. `/end` is intentionally not admin-gated
        // — an impersonated principal is not an admin, so gating it would
        // trap the operator inside the session.
        .route(
            "/api/admin/impersonate",
            post(handlers::impersonation::start_impersonation_handler),
        )
        .route(
            "/api/admin/impersonate/end",
            post(handlers::impersonation::end_impersonation_handler),
        )
        .route(
            "/api/admin/impersonate/sessions",
            get(handlers::impersonation::list_impersonation_sessions_handler),
        )
        // Transparency counterpart: any user can see who viewed their
        // account and why.
        .route(
            "/api/me/impersonation-history",
            get(handlers::impersonation::my_impersonation_history_handler),
        )
        // Platform economics — cost (real USD) vs. revenue (credits) by
        // funding principal. Answers "what do platform-service agents
        // cost me, and do they pay for themselves?"
        .route(
            "/api/admin/economics/platform",
            get(handlers::economics::platform_economics_handler),
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
            "/api/admin/workspaces/:workspace_id/grant",
            post(handlers::admin::admin_grant_workspace_credits_handler),
        )
        .route(
            "/api/admin/agents",
            get(handlers::admin::admin_list_agents_handler),
        )
        .route(
            "/api/admin/agents/:agent_id/flag",
            put(handlers::admin::admin_flag_agent_handler),
        )
        // v0.10.20: audit + rename un-routable legacy agent names
        // (containing `-` or `/`) that predate slug::validate. GET is
        // audit-only; POST with ?apply=true executes the rename in a
        // transaction and backfills fermi_forecasts.agents_used JSONB
        // references. Every rename is logged to admin_bypass_events.
        // See RELEASE_NOTES_v0.10.20.md.
        .route(
            "/api/admin/agents/legacy-slugs",
            get(handlers::admin::admin_legacy_agent_slugs_handler)
                .post(handlers::admin::admin_legacy_agent_slugs_handler),
        )
        // v0.10.25: safety-gated DELETE of orphan test-fixture rows
        // (`test_agent_<uuid>` and similar) with dry-run, prefix
        // filter, grace period, and audit trail. Cascades to every
        // FK on agents(agent_id) — mig-169 aligns the last four
        // (mig-049) so this path can't be blocked. Every deletion
        // is logged to admin_bypass_events with a full row snapshot.
        // See RELEASE_NOTES_v0.10.25.md.
        .route(
            "/api/admin/agents/cleanup-test-cruft",
            get(handlers::admin::admin_cleanup_test_cruft_handler)
                .post(handlers::admin::admin_cleanup_test_cruft_handler),
        )
        // v0.11.2: orchestra registry. Public reads (list orchestras,
        // list members, per-agent memberships). Owner-only writes
        // (submit request, withdraw). Orchestra-admin writes (approve,
        // reject). See handlers/orchestras.rs.
        .route(
            "/api/orchestras",
            get(handlers::orchestras::list_orchestras_handler),
        )
        .route(
            "/api/orchestras/:name/members",
            get(handlers::orchestras::list_orchestra_members_handler),
        )
        // SPEC_29 — revoking a grant is a first-class governance action
        // now that membership is stated rather than inferred from a column.
        .route(
            "/api/orchestras/:name/members/:agent_id",
            axum::routing::delete(handlers::orchestras::revoke_orchestra_member_handler),
        )
        .route(
            "/api/orchestras/:name/requests",
            get(handlers::orchestras::list_orchestra_requests_handler)
                .post(handlers::orchestras::submit_orchestra_request_handler),
        )
        .route(
            "/api/orchestras/:name/requests/:request_id/approve",
            post(handlers::orchestras::approve_orchestra_request_handler),
        )
        .route(
            "/api/orchestras/:name/requests/:request_id/reject",
            post(handlers::orchestras::reject_orchestra_request_handler),
        )
        .route(
            "/api/orchestras/:name/requests/:request_id/withdraw",
            post(handlers::orchestras::withdraw_orchestra_request_handler),
        )
        .route(
            "/api/agents/:agent_id/orchestras",
            get(handlers::orchestras::agent_orchestras_handler),
        )
        // v0.11.3-follow-up: manager-effect readout for strategist
        // orchestras. Aggregate + per-forecast Brier / counterfactual
        // Brier / manager_effect delta.
        .route(
            "/api/orchestras/:name/manager-effect",
            get(handlers::orchestras::orchestra_manager_effect_handler),
        )
        // Admin view over third-party Apps — lists every app across every
        // visibility level (private/unlisted/public) with owner display
        // name + workspace count so external app authors like `efrain_ai`
        // surface in the admin panel.
        .route(
            "/api/admin/apps",
            get(handlers::admin::admin_list_apps_handler),
        )
        .route(
            "/api/admin/apps/:slug/visibility",
            put(handlers::admin::admin_set_app_visibility_handler),
        )
        .route(
            "/api/admin/agent-ownership-audit",
            get(handlers::admin::admin_agent_ownership_audit_handler),
        )
        .route(
            "/api/admin/agent-ownership-reassign",
            post(handlers::admin::admin_agent_ownership_reassign_handler),
        )
        // v0.10.4 substrate: tenant-agnostic RBAC surface. Replaces
        // per-resource `agent-ownership-*` boilerplate. See
        // `handlers::admin_rbac` and RELEASE_NOTES_v0.10.4.md.
        .route(
            "/api/admin/rbac/orphans",
            get(handlers::admin_rbac::admin_rbac_orphans_handler),
        )
        .route(
            "/api/admin/rbac/reassign",
            post(handlers::admin_rbac::admin_rbac_reassign_handler),
        )
        .route(
            "/api/admin/rbac/heal",
            post(handlers::admin_rbac::admin_rbac_heal_handler),
        )
        // v0.10.6: authenticated self-diagnostic. Any signed-in user
        // can hit /api/rbac/self-check to get a definitive answer on
        // whether their JWT sub aligns with users.user_id and, if
        // not, exactly what class of drift they're looking at. See
        // handlers::rbac_self_check for the response shape.
        .route(
            "/api/rbac/self-check",
            get(handlers::rbac_self_check::rbac_self_check_handler),
        )
        // Spec 23 demo cleanup — one-shot wipe of every workspace
        // spawned by the Fermi Forecast App + cascading rows across
        // BayesOps and forecast tables. Requires admin auth and an
        // exact confirmation token. Supports dry_run for sanity checks.
        .route(
            "/api/admin/wipe-fermi-forecasts",
            post(handlers::admin::admin_wipe_fermi_forecasts_handler),
        )
        // One-shot admin recompose over mutex groups. Use to snap
        // displayed probabilities to the current recompose math after
        // a bug fix (or any group-wide drift) without waiting for a
        // per-forecast sim/resolve to trigger the normal recompose
        // path. Optional `?group_id=<id>` scopes to a single group.
        .route(
            "/api/admin/recompose-mutex-groups",
            post(handlers::admin::admin_recompose_mutex_groups_handler),
        )
        // Schema health probe: verifies every table / function / column
        // that ensure_critical_schema is responsible for landing is
        // actually present in the DB. Returns 200 with status='degraded'
        // when anything is missing, so this can be wired into monitoring
        // without hard-failing the console.
        .route(
            "/api/admin/schema-health",
            get(handlers::admin::admin_schema_health_handler),
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
        // Forage module — kask-wild bridge: scout, log observation
        .route(
            "/api/creatures/:creature_id/forage",
            post(handlers::creatures::forage_handler),
        )
        // Creature goals — standing foraging objectives
        .route(
            "/api/creatures/:creature_id/goals",
            get(handlers::creatures::list_goals_handler)
                .post(handlers::creatures::create_goal_handler),
        )
        .route(
            "/api/creatures/:creature_id/goals/:goal_id",
            patch(handlers::creatures::update_goal_handler),
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
        // Layers run outermost-last, so the order below is:
        //   auth_middleware -> impersonation_guard -> llm_rate_limit -> handler
        //
        // The LLM limiter is innermost because it keys on the resolved
        // AuthPrincipal, which `auth_middleware` puts in the extensions. It
        // passes every non-LLM path straight through; see LLM_SPEND_ROUTES.
        //
        // Until now this router had NO rate limiting of any kind, while the
        // public read-only router had one. The endpoints that spend money
        // were the only unprotected ones, because the middleware written for
        // them was `#[allow(dead_code)]` and never layered.
        .layer(middleware::from_fn_with_state(
            state.clone(),
            llm_rate_limit_middleware,
        ))
        // Inside `auth_middleware` (see the public-router note above):
        // enforces the read-only contract and writes the audit trail
        // for any request carrying an impersonated principal.
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            impersonation_guard,
        ))
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

async fn seed_agents_to_database(
    memory_store: &MemoryStore,
    registry: &AgentRegistry,
    embedder: &Arc<dyn EmbeddingGenerator>,
) {
    let cards = match registry.list_cards() {
        Ok(cards) => cards,
        Err(_) => return,
    };

    // Look up the earliest admin user once. All curated/system agents are owned
    // by the admin so the admin has full Eval / Intelligence / Manage views.
    // User-created agents are never seeded here — they go through hire_agent_handler
    // which sets owner_id from the calling principal.
    let admin_user_id: Option<String> = sqlx::query_scalar(
        "SELECT user_id FROM users WHERE role = 'admin' ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(memory_store.pool())
    .await
    .unwrap_or(None);

    if admin_user_id.is_none() {
        eprintln!("seed_agents_to_database: no admin user found — curated agents will have no owner until one is set");
    }

    // ── xaman_ek ontology sync check ───────────────────────────────────────
    // Enforce the maintenance rule: every agent on disk must be named in
    // xaman_ek's system_prompt. Missing agents mean xaman_ek will give
    // incorrect navigation and composition advice.
    let xaman_card = cards.iter().find(|c| c.agent_id == "xaman_ek");
    if let Some(xc) = xaman_card {
        let prompt = xc.system_prompt.as_deref().unwrap_or("");
        let missing: Vec<&str> = cards
            .iter()
            .filter(|c| c.agent_id != "xaman_ek")
            .filter(|c| !prompt.contains(&format!("**{}**", c.agent_id)))
            .map(|c| c.agent_id.as_str())
            .collect();
        if !missing.is_empty() {
            eprintln!(
                "⚠  xaman_ek ONTOLOGY DRIFT: {} agent(s) on disk not registered in xaman_ek's system_prompt.\n   Missing: {}\n   Update agents/curated/xaman_ek/agent_card.json per the maintenance rule.",
                missing.len(),
                missing.join(", ")
            );
        }
    }

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
            mcp_tools: None,
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
            // Curated agents are owned by the admin user so the admin has
            // full management views. The upsert uses ON CONFLICT DO UPDATE,
            // so existing agents with a real owner_id won't be overwritten
            // to NULL — but they will be reassigned to the admin on re-seed.
            // That's intentional: curated agents belong to the platform admin,
            // not to individual users.
            owner_id: admin_user_id.clone(),
            tags: card.metadata.tags.clone(),
            education_budget_credits: 0,
            education_credits_used: 0,
            auto_collect_pct: 0,
            display_alias: None,
            // Card-driven (was hardcoded "anthropic"): keeps the DB row truthful
            // so credential resolution + observability see the real provider.
            llm_provider: card.capabilities.provider.clone(),
            // Track the active embedder (OpenAIEmbeddings), not the stale
            // anthropic/voyage-2 identity. See handlers::agents::default_embedding_*.
            embedding_provider: crate::handlers::agents::default_embedding_provider(),
            embedding_model: crate::handlers::agents::default_embedding_model(),
            embedding_dimension: crate::handlers::agents::default_embedding_dimension(),
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
            valence: card
                .metadata
                .valence
                .as_ref()
                .and_then(|v| serde_json::to_value(v).ok()),
            output_contract: card.capabilities.output_contract.clone(),
            // SPEC_30 / mig-186 — carry the card's taxonomy into the DB so the
            // Ecology lens can group DB rows without reading the filesystem.
            //
            // Editorial ranks (kingdom/family/genus) come from the card,
            // because those are a human's claim about kinship and the card is
            // where a human recorded them. Derived ranks are recomputed from
            // the card's actual structure and overwrite whatever it says, so a
            // stale card cannot assert a class that contradicts its own
            // agent_type — the exact defect SPEC_30 found in 41 cards.
            taxonomy: Some(fermi::taxonomy::merge(
                card.metadata.taxonomy.as_ref(),
                &fermi::taxonomy::derive(&fermi::taxonomy::DeriveInput {
                    agent_name: card.agent_id.clone(),
                    agent_type: card.agent_type.clone(),
                    produces: card.produces.clone(),
                    has_required_deps: !card.dependencies.required.is_empty(),
                    has_instruments: !card.capabilities.mcp_servers.is_empty()
                        || !card.capabilities.mcp_tools.is_empty()
                        || !card.capabilities.skills.is_empty(),
                }),
            )),
        };

        // Log any executable skills this card declares — these are dispatchable
        // by name via ToolRegistry::execute() at runtime.
        let executable_skills = fermi::agent_backend::tools::validate_card_skills(card);
        if !executable_skills.is_empty() {
            println!(
                "  Agent '{}' has {} executable skill(s): {:?}",
                card.agent_id,
                executable_skills.len(),
                executable_skills
            );
        }

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
                        seed_cep_entities(memory_store, embedder, id, &card.agent_id, fc).await;
                    }

                    // SPEC_29 / mig-180 — grant Fermi orchestra membership
                    // for platform-seeded specialists.
                    //
                    // Membership is now stated in `orchestra_members`, not
                    // inferred from `fermi_contract IS NOT NULL`. Without
                    // this the curated specialists (macro_forecaster,
                    // equity_analyst, …) would silently drop off the
                    // roster — and out of Fermi's own injected roster
                    // block, degrading the strategist.
                    //
                    // `source='curated_seed'`, never 'approved': these have
                    // not been through review, and recording them as if
                    // they had would launder exactly the provenance this
                    // table exists to make honest.
                    //
                    // DO NOTHING on conflict so a later real approval
                    // (which upgrades the row to 'approved') is never
                    // downgraded back to 'curated_seed' on the next boot.
                    if let Err(e) = sqlx::query(
                        "INSERT INTO public.orchestra_members \
                         (orchestra_name, agent_id, source) \
                         VALUES ('fermi', $1, 'curated_seed') \
                         ON CONFLICT (orchestra_name, agent_id) DO NOTHING",
                    )
                    .bind(id)
                    .execute(memory_store.pool())
                    .await
                    {
                        eprintln!(
                            "Warning: failed to grant fermi membership for {}: {}",
                            card.agent_id, e
                        );
                    }
                }
            }
            Err(e) => eprintln!("Warning: failed to seed {}: {}", card.agent_id, e),
        }
    }
}

/// Classify any agent whose `taxonomy` is still NULL, from its own DB row.
///
/// Covers the population neither the seeder nor the create handlers reach:
/// agents authored through the API before mig-186 added the column. Only
/// derived ranks — kingdom, family and genus are claims about kinship and
/// stay unset until a human makes them.
///
/// Idempotent by construction: the query selects only unclassified rows, so
/// this is a no-op after the first boot and can never overwrite an editorial
/// decision.
async fn backfill_agent_taxonomy(memory_store: &MemoryStore) {
    let rows = match sqlx::query(
        "SELECT agent_id, agent_name, agent_type, produces, mcp_servers, mcp_tools \
           FROM public.agents \
          WHERE taxonomy IS NULL \
            AND agent_name NOT LIKE 'test\\_agent\\_%'",
    )
    .fetch_all(memory_store.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Note: taxonomy backfill skipped ({})", e);
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    let mut classified = 0usize;
    for row in &rows {
        let agent_id: uuid::Uuid = match row.try_get("agent_id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let agent_name: String = row.try_get("agent_name").unwrap_or_default();
        let agent_type: String = row.try_get("agent_type").unwrap_or_default();
        let produces: Vec<String> = row.try_get("produces").unwrap_or_default();
        let has_instruments = row
            .try_get::<Option<serde_json::Value>, _>("mcp_servers")
            .ok()
            .flatten()
            .map(|v| {
                !matches!(v, serde_json::Value::Null) && v.as_array().is_none_or(|a| !a.is_empty())
            })
            .unwrap_or(false)
            || row
                .try_get::<Option<serde_json::Value>, _>("mcp_tools")
                .ok()
                .flatten()
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false);

        let taxonomy = fermi::taxonomy::derive(&fermi::taxonomy::DeriveInput {
            agent_name,
            agent_type,
            produces,
            // Dependencies aren't a column, so a pre-existing DB agent can't
            // be shown to orchestrate anything. Under-claiming (Instrumenta /
            // Solitaria rather than Composita) is the safe direction: it
            // states less than we know rather than more.
            has_required_deps: false,
            has_instruments,
        });

        if sqlx::query("UPDATE public.agents SET taxonomy = $1 WHERE agent_id = $2")
            .bind(&taxonomy)
            .bind(agent_id)
            .execute(memory_store.pool())
            .await
            .is_ok()
        {
            classified += 1;
        }
    }

    println!(
        "Taxonomy: classified {} previously undescribed agent(s) (derived ranks only; \
         kingdom/family/genus need a human)",
        classified
    );
}

/// Seed App manifests from the `apps/` directory into the `apps` table.
///
/// Each file must be a JSON object matching the `apps` table shape with at
/// minimum: `slug`, `name`, `workspace_template`.
///
/// Behaviour:
/// - INSERT the app if the slug doesn't exist yet.
/// - UPDATE `name`, `tagline`, `workspace_template`, `description`, `metadata`,
///   `homepage_url`, `icon_url`, `composition_slug`, `schema_slug`, `visibility`
///   if the slug already exists — so re-deploys always keep the manifest current.
/// - The `owner_user_id` field in the JSON is advisory; on the server we
///   use "sys" as the platform owner for seeded apps.
/// - Never changes `published_at`, `archived_at`, or `visibility` to something
///   more restrictive than what's already stored.
async fn seed_apps_to_database(db: &sqlx::PgPool) {
    // Locate the apps/ directory relative to the binary's working directory.
    let apps_dirs = ["apps", "../apps", "../../apps"];
    let apps_dir = apps_dirs.iter().find(|d| std::path::Path::new(d).is_dir());
    let apps_dir = match apps_dir {
        Some(d) => *d,
        None => {
            println!("No apps/ directory found — skipping App seeding");
            return;
        }
    };

    let entries = match std::fs::read_dir(apps_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Could not read apps/ directory: {}", e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Could not read {:?}: {}", path, e);
                continue;
            }
        };
        let manifest: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("Could not parse {:?}: {}", path, e);
                continue;
            }
        };

        let slug = match manifest["slug"].as_str() {
            Some(s) => s.to_string(),
            None => {
                eprintln!("App manifest {:?} missing 'slug'", path);
                continue;
            }
        };

        let name = manifest["name"].as_str().unwrap_or(&slug).to_string();
        let tagline = manifest["tagline"].as_str().map(str::to_string);
        let homepage_url = manifest["homepage_url"].as_str().map(str::to_string);
        let icon_url = manifest["icon_url"].as_str().map(str::to_string);
        let composition_slug = manifest["composition_slug"].as_str().map(str::to_string);
        let schema_slug = manifest["schema_slug"].as_str().map(str::to_string);
        let schema_json = manifest
            .get("schema_json")
            .cloned()
            .filter(|v| !v.is_null());
        let description = manifest["description"].as_str().map(str::to_string);
        let visibility = manifest["visibility"]
            .as_str()
            .unwrap_or("public")
            .to_string();
        let workspace_template = manifest["workspace_template"].clone();
        let metadata = manifest["metadata"].clone();

        let result = sqlx::query(
            r#"INSERT INTO apps (
                slug, name, tagline, owner_user_id,
                homepage_url, icon_url,
                composition_slug, schema_slug, schema_json,
                workspace_template, visibility,
                description, metadata
            ) VALUES ($1, $2, $3, 'sys', $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (slug) DO UPDATE SET
                name               = EXCLUDED.name,
                tagline            = EXCLUDED.tagline,
                homepage_url       = EXCLUDED.homepage_url,
                icon_url           = EXCLUDED.icon_url,
                composition_slug   = EXCLUDED.composition_slug,
                schema_slug        = EXCLUDED.schema_slug,
                schema_json        = EXCLUDED.schema_json,
                workspace_template = EXCLUDED.workspace_template,
                description        = EXCLUDED.description,
                metadata           = EXCLUDED.metadata,
                visibility         = CASE
                    WHEN apps.visibility = 'public' THEN 'public'
                    ELSE EXCLUDED.visibility
                END"#,
        )
        .bind(&slug)
        .bind(&name)
        .bind(&tagline)
        .bind(&homepage_url)
        .bind(&icon_url)
        .bind(&composition_slug)
        .bind(&schema_slug)
        .bind(&schema_json)
        .bind(&workspace_template)
        .bind(&visibility)
        .bind(&description)
        .bind(&metadata)
        .execute(db)
        .await;

        match result {
            Ok(_) => println!("Seeded app: {}", slug),
            Err(e) => eprintln!("Warning: failed to seed app {}: {}", slug, e),
        }
    }
}

async fn seed_cep_entities(
    memory_store: &MemoryStore,
    embedder: &Arc<dyn EmbeddingGenerator>,
    agent_uuid: uuid::Uuid,
    agent_name: &str,
    fc: &fermi::agent_backend::agent_card::FermiContract,
) {
    // Idempotency is per fact, keyed on (name, type) — not on a naming
    // convention.
    //
    // This guard used to be `existing.any(|e| e.entity_type.starts_with("cep_"))`,
    // while the loop below writes whatever `entity_type` the card declares. For
    // any card whose seed facts are not `cep_`-prefixed — `field_baseline`,
    // `confederation_coefficient`, `fixture_congestion`, and the rest — the
    // guard could never fire, so **every server boot re-seeded the entire set**.
    //
    // Measured on this deployment before the fix: 15 distinct seed facts stored
    // as 2,475 rows. Exactly 165 identical copies of each, across
    // `football_institution_agent`, `macro_data_agent` and `fixture_context_agent`
    // — one per boot, growing without bound. `weather_oracle` was unaffected
    // only because it happens to declare six genuinely `cep_`-prefixed facts,
    // which tripped the old guard.
    //
    // Keying on (name, type) is idempotent per fact rather than per agent, so a
    // card that gains a new seed fact picks it up on next boot instead of being
    // skipped wholesale.
    let existing = memory_store
        .get_agent_entities(agent_uuid)
        .await
        .unwrap_or_default();
    let existing_facts: std::collections::HashSet<(&str, &str)> = existing
        .iter()
        .map(|e| (e.entity_name.as_str(), e.entity_type.as_str()))
        .collect();

    let pending: Vec<_> = fc
        .seed_facts
        .iter()
        .filter(|sf| !existing_facts.contains(&(sf.name.as_str(), sf.entity_type.as_str())))
        .collect();
    let skipped = fc.seed_facts.len() - pending.len();
    if pending.is_empty() {
        return;
    }

    // Embed seed facts at write time.
    //
    // These used to be stored with `embedding: None`, on the reasoning that
    // "the consolidation worker may later opportunistically embed
    // `entity_name` if needed". It never did, and nothing else does either, so
    // the vectors were never created.
    //
    // That is only survivable for `cep_`-typed rows, which
    // `get_top_k_entities_with_cep` injects unconditionally. Everything else
    // needs a vector to be reachable at all: both retrieval paths filter on
    // `embedding IS NOT NULL`. Seeding without one produces curated knowledge
    // that no agent can ever recall.
    //
    // Embedding here also makes seed data *scale*. Always-injection is fine
    // for a handful of constants but blows the context window at volume;
    // similarity retrieval returns the top-k that matter for the actual query.
    //
    // Cost is bounded: the idempotency filter above runs first, so a steady
    // -state boot embeds nothing. Only genuinely new facts are paid for.
    let texts: Vec<String> = pending
        .iter()
        .map(|sf| format!("{}: {}", sf.name, sf.description))
        .collect();
    let embeddings = match embedder.generate_provenanced_batch(&texts).await {
        Ok(v) if v.len() == pending.len() => Some(v),
        Ok(v) => {
            eprintln!(
                "  Warning: embedder returned {} vectors for {} seed facts on {} — \
                 storing without embeddings",
                v.len(),
                pending.len(),
                agent_name
            );
            None
        }
        Err(e) => {
            // Never block boot on an embedding outage. The rows are still
            // written; the retrievability census will report them as stranded.
            eprintln!(
                "  Warning: could not embed seed facts for {} ({}); storing \
                 without embeddings — they will not be retrievable until \
                 backfilled",
                agent_name, e
            );
            None
        }
    };

    let mut seeded = 0usize;
    for (i, sf) in pending.iter().enumerate() {
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
            embedding: None, // set by store_entity_with_provenance below
            properties: Some(sf.properties.clone()),
        };
        // Stamp source_ref so the row is identifiable as a seed rather than
        // something an episode produced.
        let source_ref = serde_json::json!({
            "kind": "cep_seed",
            "agent_name": agent_name,
        });
        let provenance = embeddings.as_ref().map(|v| &v[i]);
        match memory_store
            .store_entity_with_provenance(entity, provenance, Some(source_ref))
            .await
        {
            Ok(_) => seeded += 1,
            Err(e) => eprintln!(
                "  Warning: failed to seed CEP entity '{}' for {}: {}",
                sf.name, agent_name, e
            ),
        }
    }
    if seeded > 0 {
        println!(
            "  Seeded {} CEP entities for {} ({} already present)",
            seeded, agent_name, skipped
        );
    }
}

// ─── Agent resolution helper ───────────────────────────────────────

// ─── Shared helpers ──────────────────────────────────────────────────

pub(crate) async fn resolve_agent(
    state: &AppState,
    agent_id: &str,
) -> Result<Agent, (StatusCode, String)> {
    // v0.10.15: the URL param is nominally `agent_id`, but historically
    // this resolver only accepted `agent_name`. Scripts/audit tools
    // that address an agent by its actual UUID (e.g. the RBAC-orphan
    // reports emitted by /api/admin/rbac/orphans) got 404s even for
    // agents they'd just seen listed. Try UUID first when the input
    // parses cleanly — no valid `agent_name` satisfies the slug rule
    // AND parses as a UUID (slug rejects `-`), so this is a clean
    // split with zero risk of accidental collision.
    if let Ok(uuid) = uuid::Uuid::parse_str(agent_id) {
        match state.memory_store.get_agent(uuid).await {
            Ok(Some(agent)) => return Ok(agent),
            Ok(None) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    format!("Agent with id '{}' not found", agent_id),
                ));
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Lookup by id failed: {}", e),
                ));
            }
        }
    }

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

/// v0.9.0 (revised in v0.9.2) — Agent-owner API key routing.
///
/// Resolve the secrets a running agent should see, per the marketplace
/// architecture: **agents carry their own funding, not users**. Keys
/// are stored owner-side via ABW's profile page
/// (`https://agent-bestiary.world/profile`), which writes to
/// `user_secrets` with `scope='*'` (global across all the owner's
/// agents). This function reads them back into the executor's
/// ToolContext.
///
/// Return value semantics (revised in v0.9.2 to enable executor
/// tightening — the executor now distinguishes system from owner-owned
/// via `Option<Some(_) vs None>`):
///
///   - **`None`** — the agent is either system-tier (platform-funded via
///     env var) or unidentifiable as owner-owned (no encryptor, no
///     owner_id). The executor treats `None` as "use the platform env
///     fallback" — correct for Fermi, xaman_ek, and other infra agents.
///   - **`Some(HashMap)`** — the agent is owner-owned. The map contains
///     zero or more of the owner's globally-scoped secrets (v0.9.2 no
///     longer collapses empty results into `None` — that's what unlocks
///     the executor to hard-fail with an ABW-aware error instead of
///     silently falling back to the platform key). When empty, the
///     executor emits: *"Agent 'X' is not funded. Owner needs to set
///     ANTHROPIC_API_KEY at https://agent-bestiary.world/profile"*.
///
/// Behavioural difference vs v0.9.0: an owner-owned agent whose owner
/// hasn't uploaded a key USED to silently run on the platform env var
/// (soft fallback). It now hard-fails with the ABW URL. Trade-off
/// documented explicitly because it changes production behaviour: any
/// deployment relying on the soft fallback to keep agents running
/// needs to have owners set their keys on ABW's profile page before
/// upgrading.
///
/// Returns an owned `HashMap<name, plaintext>` because the executor
/// path stores the resolved secrets on `ToolContext` (Arc-shared) and
/// we don't want to leak `state.db` / `state.secret_encryptor` into
/// executor code.
/// P1 (credential model, docs/specs/AGENT_CREDENTIAL_MODEL.md): resolve the
/// effective API key for `provider` that powers `agent`, from the owning
/// principal's credential store. Single credential path:
///
///   - **system-tier** agents → the `abw-system` principal's keys (the
///     platform "system keys", seeded into the store at boot from env).
///   - **owner-owned** agents → the agent's `owner_id`.
///
/// Order: agent-scoped key, then the principal's `*` default (via
/// `agent_credentials`). For owner-owned agents mid-migration we also fall
/// back to the legacy `user_secrets` (`<PROVIDER>_API_KEY`) so already-funded
/// agents keep working until P5. Deliberately does NOT read env vars — env is
/// a one-time bootstrap seed into the store, not a runtime source of truth.
/// One predicate for the platform-funding decision, defined in the lib
/// (`agent_backend::credentials`) so it is testable and so the several
/// places that ask "is this ours?" cannot drift apart again.
pub(crate) use fermi::agent_backend::credentials::is_platform_funded;

/// The principal whose keys pay for this agent: `abw-system` for
/// platform-service agents, the owner otherwise.
pub(crate) fn funding_principal_for(agent: &Agent) -> Option<String> {
    if is_platform_funded(&agent.tier) {
        Some("abw-system".to_string())
    } else {
        agent.owner_id.clone()
    }
}

pub(crate) async fn resolve_credential(
    state: &AppState,
    agent: &Agent,
    provider: &str,
) -> Option<String> {
    let encryptor = state.secret_encryptor.as_ref()?;
    let principal_owned = funding_principal_for(agent)?;
    let principal_id: &str = principal_owned.as_str();

    // Primary: the (principal, provider, scope) store.
    if let Ok(Some(key)) = fermi_auth::resolve_agent_credential(
        &state.db,
        encryptor,
        principal_id,
        provider,
        &agent.agent_name,
    )
    .await
    {
        return Some(key);
    }

    // Legacy fallback (owner-owned only): user_secrets named <PROVIDER>_API_KEY.
    if !is_platform_funded(&agent.tier) {
        if let Some(owner_id) = agent.owner_id.as_deref() {
            let secret_name = format!("{}_API_KEY", provider.to_uppercase());
            if let Ok(secrets) =
                fermi_auth::get_secrets_for_agent(&state.db, encryptor, owner_id, &agent.agent_name)
                    .await
            {
                if let Some(v) = secrets.get(&secret_name) {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

/// SPEC_28 — resolve every provider credential this execution could need,
/// once, up front.
///
/// Pre-resolves the card's declared provider **and** every `model_ladder`
/// rung's provider, so ladder-driven provider switching (and P4 graceful
/// degradation) needs no async credential work mid-execution.
///
/// This is the single entry point every execution path must call. Paths
/// that don't get `ExecutionContext.credentials` populated inherit
/// `unfunded`, which fails loudly rather than quietly billing the
/// platform — the failure mode SPEC_28 exists to eliminate.
pub(crate) async fn build_execution_credentials(
    state: &AppState,
    agent: &Agent,
    card: &fermi::agent_backend::agent_card::AgentCard,
) -> std::sync::Arc<fermi::agent_backend::credentials::ResolvedCredentials> {
    use fermi::agent_backend::credentials::{
        provider_needs_no_key, CredentialSource, ResolvedCredentials,
    };

    // Distinct provider set: declared provider ∪ ladder rung providers.
    let mut providers: Vec<String> = Vec::new();
    let declared = card.capabilities.provider.trim();
    // An empty provider means "anthropic" throughout the executor layer.
    providers.push(if declared.is_empty() {
        "anthropic".to_string()
    } else {
        declared.to_string()
    });
    for rung in &card.capabilities.model_ladder {
        let p = rung.provider.trim();
        if !p.is_empty() && !providers.iter().any(|x| x == p) {
            providers.push(p.to_string());
        }
    }

    let mut builder = ResolvedCredentials::builder();
    // Funding principal — same helper `resolve_credential` uses, so the
    // recorded payer and the actually-charged key cannot disagree.
    let principal = funding_principal_for(agent);
    if let Some(ref p) = principal {
        builder = builder.funding_principal(p.clone());
    }

    for provider in providers {
        if provider_needs_no_key(&provider) {
            continue;
        }
        if let Some(key) = resolve_credential(state, agent, &provider).await {
            // `resolve_credential` tries agent-scope, then the principal
            // default, then legacy user_secrets. It doesn't report which
            // matched; attribute to PrincipalDefault as the common case.
            // Distinguishing the three is a P5.5 telemetry refinement.
            builder = builder.key(provider, key, CredentialSource::PrincipalDefault);
        }
    }

    builder.build_arc()
}

pub(crate) async fn resolve_agent_owner_secrets(
    state: &AppState,
    agent: &Agent,
) -> Option<std::collections::HashMap<String, String>> {
    // Platform-service agents (system + curated) don't have per-owner
    // tool secrets — they're operated by the platform. Same predicate as
    // the credential path so the two views of "is this ours?" agree.
    //
    // Note this map is now only third-party MCP/tool credentials; LLM
    // provider keys travel on ExecutionContext.credentials (SPEC_28).
    if is_platform_funded(&agent.tier) {
        return None;
    }
    // Encryptor / owner_id absence is a config / data issue, not a
    // "this agent is owner-owned but unfunded" signal. Return None so
    // the executor uses env — keeps the platform running even when
    // the secrets primitive isn't configured (dev deploys).
    let encryptor = state.secret_encryptor.as_ref()?;
    let owner_id = agent.owner_id.as_ref()?;
    // Owner-owned agent (we have both encryptor and owner_id): return
    // Some(map) even when empty so the executor knows to hard-fail
    // rather than silently use the platform key. This is the v0.9.2
    // change from v0.9.0's soft-fallback behaviour.
    match fermi_auth::get_secrets_for_agent(&state.db, encryptor, owner_id, &agent.agent_name).await
    {
        Ok(secrets) => Some(secrets),
        Err(e) => {
            // DB failure fetching secrets — log and return Some(empty)
            // so the executor's error names the agent + points at
            // ABW rather than falling back to platform key.
            tracing::warn!(
                agent_name = %agent.agent_name,
                owner_id = %owner_id,
                error = %e,
                "[secrets] failed to load owner secrets",
            );
            Some(std::collections::HashMap::new())
        }
    }
}

/// Cached ABW profile URL used in executor error messages. Configurable
/// via `ABW_PROFILE_URL` env var; defaults to the production URL. This
/// keeps the string in one place so the error message can be updated
/// deploy-side without a code change.
pub(crate) fn abw_profile_url() -> String {
    std::env::var("ABW_PROFILE_URL")
        .unwrap_or_else(|_| "https://agent-bestiary.world/profile".to_string())
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
            // Known limitation, pre-existing and unchanged: DB-sourced
            // cards carry no tool declarations. `resolve_agent_card`
            // prefers the filesystem registry, so agents whose
            // agent_card.json was loaded at boot are unaffected — but an
            // agent that exists only in the DB gets no platform tools and
            // no remote MCP servers.
            //
            // Note `agents.mcp_servers` (JSONB) is NOT the source here:
            // despite the name it is populated from `mcp_tools` at
            // handlers/agents.rs:963 and has never been read back. Wiring
            // it up needs that writer corrected first, or DB cards would
            // silently disagree with file cards.
            mcp_tools: vec![],
            mcp_servers: vec![],
            skills: vec![],
            model: agent.model.clone(),
            temperature: agent.temperature,
            provider: agent.llm_provider.clone(),
            model_ladder: serde_json::from_value(agent.model_ladder.clone()).unwrap_or_default(),
            min_tier: match agent.min_tier.as_str() {
                "standard" => fermi::agent_backend::agent_card::CognitionTier::Standard,
                "premium" => fermi::agent_backend::agent_card::CognitionTier::Premium,
                _ => fermi::agent_backend::agent_card::CognitionTier::Free,
            },
            capability_gates: serde_json::from_value(agent.capability_gates.clone())
                .unwrap_or_default(),
            // min_provider_class is loaded from agent_card.json on disk; the
            // DB row doesn't carry this field yet. Default to CloudStandard,
            // which mirrors the typed default and matches the existing
            // semantics for agents minted before the topology Phase-0 work.
            min_provider_class: fermi::agent_backend::agent_card::MinProviderClass::default(),
            fermi_contract: agent
                .fermi_contract
                .as_ref()
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            output_contract: agent.output_contract.clone(),
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
            // mig-186 — this is the path that makes DB-native agents
            // classifiable at all. Before the column existed, a card
            // reconstructed from a DB row had no taxonomy to carry, so every
            // agent authored through the API was permanently undescribed.
            taxonomy: agent.taxonomy.clone(),
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

    // Bridge remote MCP servers from DB — the DB is the source of truth
    // for agent config.
    //
    // Semantics are deliberately "override, not merge":
    //   - NULL column        -> inherit whatever the file card declares.
    //   - `[]` (or `{}`)     -> explicitly no servers, even if the file
    //                           card declares some. This is what lets the
    //                           UI *remove* a file-declared server.
    //   - non-empty          -> authoritative replacement.
    //
    // A merge would leave no way to express removal and would make
    // precedence ambiguous on name collisions. The UI seeds the DB from
    // the file card on first save, so "override" costs nothing.
    //
    // `interpret_db_column` returns None for NULL, for unparseable data,
    // and for the legacy case where the old create path wrote the card's
    // `mcp_tools` into this column — in all three we keep the filesystem
    // card rather than erasing real servers or inventing endpoint-less
    // ones. See its docs for the full precedence table.
    if let Some(raw) = db_agent.mcp_servers.as_ref() {
        if let Some(servers) = fermi::agent_backend::mcp_client::interpret_db_column(raw) {
            card.capabilities.mcp_servers = servers;
        }
    }

    // Bridge published tools from DB — the server direction, symmetric with
    // mcp_servers above.
    //
    // Without this, `agent_card_from_db` returns `mcp_tools: vec![]` and any
    // agent lacking an `agent_card.json` on disk (~709 DB rows against 95
    // card files) publishes nothing over `/mcp/agents/:id`: `tools/list`
    // shows only the catch-all `execute`, and every typed `tools/call` is
    // rejected as "not declared". The agent's own tool access was never
    // affected — `to_claude_tools_with_card` starts from all builtins — so
    // this gap only ever broke ABW-as-MCP-server.
    //
    // Same precedence as mcp_servers: NULL inherits from the file card,
    // `[]` publishes nothing, non-empty is authoritative.
    if let Some(raw) = db_agent.mcp_tools.as_ref() {
        if !raw.is_null() {
            match serde_json::from_value::<Vec<fermi::agent_backend::agent_card::McpTool>>(
                raw.clone(),
            ) {
                Ok(tools) => card.capabilities.mcp_tools = tools,
                Err(e) => eprintln!(
                    "[mcp] agent '{}' has an unparseable mcp_tools column ({e}); \
                     falling back to the filesystem card",
                    db_agent.agent_name
                ),
            }
        }
    }

    card
}

/// Fold one live human↔agent exchange into the dyad's running relationship
/// state, in the background.
///
/// Call this wherever a real conversation turn completes. It is the live
/// counterpart to the eval pipeline's evaluator-driven path: live turns
/// never reach `agent_timeline_entries` (only `run_eval_cases` writes
/// those), so the background `ObservabilityWorker` social pass would never
/// see them and every real relationship would stay frozen at its defaults.
///
/// Fire-and-forget by design — two cheap queries (`get_dyad_state` +
/// `upsert_dyad_state`) that must never delay or fail a user's response.
pub(crate) fn spawn_dyad_observation(
    state: &AppState,
    agent_db_id: uuid::Uuid,
    dyad_id: String,
    query: &str,
    output: &AgentOutput,
) {
    let obs = agent_bestiary_observability::InteractionObservation {
        succeeded: matches!(output.status, AgentStatus::Success),
        partial: matches!(output.status, AgentStatus::BelowConfidenceThreshold),
        confidence: output.confidence,
        user_chars: query.chars().count(),
        occurred_at: output.timestamp,
    };
    let store = Arc::clone(&state.memory_store);
    tokio::spawn(async move {
        let tracker = agent_bestiary_observability::SocialInteractionTracker::new(store);
        match tracker
            .observe_interaction(agent_db_id, &dyad_id, &obs)
            .await
        {
            Ok(u) => {
                if u.rupture_detected {
                    tracing::warn!(
                        dyad = %dyad_id,
                        max_rapport_drop = u.max_rapport_drop,
                        "dyad rupture detected"
                    );
                }
                tracing::debug!(
                    dyad = %dyad_id,
                    rapport = u.state.rapport,
                    trust = u.state.trust,
                    reciprocity = u.state.reciprocity,
                    episodes = u.state.episode_count,
                    "dyad state updated"
                );
            }
            Err(e) => tracing::warn!(dyad = %dyad_id, error = %e, "dyad observation failed"),
        }
    });
}

/// Stamp a caller-supplied invocation record onto an episode.
///
/// The episode already records the *outcome* of a run — status, failure
/// reason, confidence, tokens, and eventually a Brier score once the
/// forecast it fed resolves. What it never recorded is *how the agent was
/// asked*: whether the query was composed from the agent's own declared
/// contract, from a template its designer wrote, or from a generic fallback
/// because the agent declared nothing to compose against.
///
/// Without that, the two interesting failures are indistinguishable in the
/// data. An agent that returned nothing useful because it was sent the wrong
/// shape of question looks exactly like one that was asked properly and is
/// simply bad at the job. Any adaptation loop learning from outcome alone
/// would blame the agent for the caller's mistake — and would learn to
/// prefer the agents the caller happens to know how to talk to, which is
/// precisely the closed world worth escaping.
///
/// Written as tags as well as context because tags are queryable and already
/// render in the observatory and episode list, so the signal is visible
/// without a bespoke view.
/// Write the SERVER's verdict on whether the caller's prompt matches the
/// interface this agent advertises.
///
/// Split from [`stamp_invocation`] because the two have different
/// provenance: everything there is the caller describing its own intent,
/// which only the caller knows; this is the platform checking a claim
/// against the card, which only the platform can do. Keeping them in one
/// function is what let a caller-asserted value masquerade as a verified
/// one for two releases.
///
/// `claimed` is whatever the caller said, if anything. A disagreement is
/// tagged rather than silently overwritten: a client working from a stale
/// copy of a card is worth knowing about, and it is the only signal that
/// the console and the server have drifted apart.
pub(crate) fn stamp_input_binding(
    episode: &mut Episode,
    verified: &fermi::port_trust::InputBinding,
    claimed: Option<&str>,
) {
    // `declared:query` would read as a `declared` category with a `query`
    // value; keep the whole thing under one namespace. Vocabulary unchanged
    // from v0.16.0 so the tag time series does not split at this deploy.
    let tag = verified.as_tag();
    episode
        .tags
        .push(format!("ibind:{}", tag.replace(':', "-")));

    if verified.is_mismatch() {
        episode.tags.push("ibind:mismatch".to_string());
    }
    if let Some(c) = claimed {
        if c != tag {
            episode.tags.push("ibind:claim-disagreed".to_string());
        }
    }
}

pub(crate) fn stamp_invocation(episode: &mut Episode, invocation: &serde_json::Value) {
    let Some(obj) = invocation.as_object() else {
        return;
    };

    // Bounded, lowercase, no whitespace: these become tag suffixes, and a
    // caller-supplied string must not be able to invent arbitrary tags.
    fn slug(v: Option<&serde_json::Value>) -> Option<String> {
        let s = v?.as_str()?.trim();
        if s.is_empty() || s.len() > 64 {
            return None;
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
        {
            return None;
        }
        Some(s.to_ascii_lowercase())
    }

    if let Some(src) = slug(obj.get("query_source")) {
        episode.tags.push(format!("qsrc:{}", src));
    }
    // NOTE: `input_binding` is deliberately NOT read from here any more.
    //
    // It used to be, and that was the bug. Whether the caller's prompt
    // matches the interface the agent advertises is a property of the card,
    // which the server holds and the caller may not even have seen. Taking
    // it from the request body meant the episode recorded the caller's
    // *claim* about the match and tagged it as a finding — a client could
    // assert `declared:query` against an agent accepting only `gbif_key`
    // and the platform would file it as fact.
    //
    // It is now computed from the resolved card at the execute boundary by
    // `fermi::port_trust::bind_input` and written by
    // [`stamp_input_binding`]. A caller-supplied value is compared against
    // the verified one rather than trusted; see that function.
    if obj
        .get("recomposed_from")
        .and_then(|v| v.as_str())
        .is_some()
    {
        episode.tags.push("recomposed:true".to_string());
    }

    // Why this agent was CHOSEN, alongside how it was ASKED.
    //
    // The asking half above cannot, on its own, separate a router coverage
    // gap from agent incompetence: an agent that underperformed as the
    // generalist fallback is indistinguishable in outcome data from one
    // deliberately selected as the resident domain expert and found wanting.
    // Pooling those two populations teaches a credit model to distrust
    // whichever agents the router reaches for by default — the same closed
    // world, re-entered through the credit model instead of a match arm.
    if let Some(reason) = slug(obj.get("route_reason")) {
        episode.tags.push(format!("route:{}", reason));
    }
    // Only tag the fallback case. `route:default` is already visible above;
    // this is the cheap filter for "routes that carry a real signal".
    if obj.get("route_deliberate").and_then(|v| v.as_bool()) == Some(false) {
        episode.tags.push("route:fallback".to_string());
    }
    // Present only on disagreement, so its existence is the signal: it makes
    // "how often is the strategist overruled, and was overruling it right?"
    // a single query — feedback the decomposition side cannot otherwise get.
    if obj
        .get("route_overrode_suggestion")
        .and_then(|v| v.as_str())
        .is_some()
    {
        episode.tags.push("route:overrode_fermi".to_string());
    }
    // Routing quality is only meaningful per domain: "domain_specialist beats
    // default" in aggregate says nothing about whether the specialist picked
    // for *climate* is the right one. This is the grouping key that lets a
    // measured ranking replace the compile-time specialist table.
    if let Some(domain) = slug(obj.get("route_domain")) {
        episode.tags.push(format!("domain:{}", domain));
    }

    if let Some(ctx) = episode.context.as_object_mut() {
        ctx.insert("invocation".to_string(), invocation.clone());
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
    create_notification_for_surface(pool, user_id, notif_type, title, message, "abw").await;
}

/// Variant that tags the notification with an explicit surface source.
/// Use `source = "rabble"` for creature/swarm/social notifications so they
/// don't bleed into the ABW platform UI and vice versa.
pub(crate) async fn create_notification_for_surface(
    pool: &PgPool,
    user_id: &str,
    notif_type: &str,
    title: &str,
    message: Option<&str>,
    source: &str,
) {
    let _ = sqlx::query(
        "INSERT INTO notifications (user_id, type, title, message, source) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(notif_type)
    .bind(title)
    .bind(message)
    .bind(source)
    .execute(pool)
    .await;
}

#[cfg(test)]
mod failure_provenance_tests {
    use super::*;
    use fermi::agent_backend::executor::{AgentMetadata, ToolInvocation};
    use serde_json::json;

    fn output(
        status: AgentStatus,
        failure_reason: Option<&str>,
        stop: Option<&str>,
    ) -> AgentOutput {
        AgentOutput {
            raw_response: Some("{\"ok\": true}".into()),
            agent_name: "efra_critical_factor".into(),
            agent_type: "research".into(),
            timestamp: chrono::Utc::now(),
            status,
            evidence: vec![],
            confidence: 0.0,
            sources_consulted: vec![],
            execution_time_ms: 41_000,
            tokens_used: Some(120_000),
            // A realistic tool-loop split: long accumulated tool results in,
            // comparatively little prose out. Exercises the measured-split
            // pricing path rather than the assumed one.
            input_tokens: Some(102_000),
            output_tokens: Some(18_000),
            metadata: AgentMetadata {
                model_used: Some("claude-sonnet-4".into()),
                provider: Some("anthropic".into()),
                stop_reason: stop.map(str::to_string),
                failure_reason: failure_reason.map(str::to_string),
                ..Default::default()
            },
            tool_invocations: (1..=15)
                .map(|i| ToolInvocation {
                    tool_name: "web_search".into(),
                    input: json!({}),
                    output: String::new(),
                    duration_ms: 574,
                    iteration: i,
                })
                .collect(),
            loop_iterations: 5,
        }
    }

    #[test]
    fn invocation_provenance_lands_as_queryable_tags_and_context() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        stamp_invocation(
            &mut ep,
            &json!({
                "query_source": "declared_contract",
                "input_binding": "declared:query",
                "declared_label_count": 5,
                "driver": "ai_product_execution"
            }),
        );

        assert!(ep.tags.contains(&"qsrc:declared_contract".to_string()));
        assert_eq!(
            ep.context["invocation"]["declared_label_count"]
                .as_u64()
                .unwrap(),
            5
        );
        // The caller's `input_binding` must NOT become a tag. Whether the
        // prompt matches the interface is a property of the card, which the
        // server holds and the caller may never have read; taking the
        // caller's word for it filed an assertion as a finding. It is now
        // computed at the boundary by `stamp_input_binding`.
        assert!(
            !ep.tags.iter().any(|t| t.starts_with("ibind:")),
            "stamp_invocation must not emit a binding verdict from caller \
             input: {:?}",
            ep.tags
        );
    }

    // ── LLM rate limiting ──────────────────────────────────────────

    #[test]
    fn the_expensive_endpoints_are_rate_limited() {
        for path in [
            "/api/agents/1f3c-abc/execute",
            "/api/agents/1f3c-abc/execute/stream",
            "/api/agents/1f3c-abc/eval/run",
            "/api/agents/1f3c-abc/consolidate",
            "/api/me/eval/runs/batch",
            "/api/creatures/9a/genome-profiler",
            "/api/creatures/9a/enemy-sensor",
            "/api/creatures/9a/prey-locator",
            "/api/creatures/9a/dream",
            "/api/workspaces/w1/composition/dream",
            "/api/notebooks/n1/execute",
        ] {
            assert!(
                is_llm_spend_route(path),
                "{path} dispatches a model and must be rate limited"
            );
        }
    }

    #[test]
    fn ordinary_reads_are_not_throttled() {
        // The strict limiter is 10/min. If it caught routine dashboard
        // traffic the limit would be raised until it was harmless, and the
        // protection would be gone — so over-matching is the failure mode
        // that actually matters here.
        for path in [
            "/api/auth/me",
            "/api/agents",
            "/api/agents/1f3c-abc",
            "/api/agents/1f3c-abc/episodes",
            "/api/agents/1f3c-abc/eval/runs",
            // Loop 1 maturity: a read the observatory issues on every agent
            // selection. It was on the LLM list, which capped the clinical
            // view at ten agents per minute.
            "/api/agents/1f3c-abc/dreaming",
            "/api/observatory/agents/1f3c-abc/loops",
            "/api/forecasts",
            "/api/creatures/9a",
            "/api/workspaces/w1/messages",
            "/api/notebooks/n1",
        ] {
            assert!(
                !is_llm_spend_route(path),
                "{path} is a read and must not be throttled at LLM rates"
            );
        }
    }

    #[test]
    fn a_wildcard_matches_exactly_one_segment() {
        // `/api/agents/*/execute` must not swallow `/execute/stream` by
        // prefix, and must not match a path with the segment missing.
        assert!(!is_llm_spend_route("/api/agents/execute"));
        assert!(!is_llm_spend_route("/api/agents/a/b/execute"));
        // Trailing slash is the same route.
        assert!(is_llm_spend_route("/api/agents/abc/execute/"));
        // And nothing matches the root, which an empty or "/" pattern would.
        assert!(!is_llm_spend_route("/"));
        assert!(!is_llm_spend_route(""));
    }

    #[test]
    fn no_declared_pattern_is_dangerously_broad() {
        for pattern in LLM_SPEND_ROUTES {
            assert!(
                pattern.starts_with("/api/"),
                "{pattern} does not look like an API route"
            );
            let segs: Vec<&str> = pattern.split('/').collect();
            assert!(
                segs.len() >= 4,
                "{pattern} is too short to be specific — a two-segment \
                 pattern would throttle an entire namespace"
            );
            assert!(
                !pattern.ends_with('*'),
                "{pattern} ends in a wildcard, which matches every child \
                 route including cheap reads"
            );
        }
    }

    #[test]
    fn the_binding_verdict_comes_from_the_card_not_the_caller() {
        use fermi::port_trust::bind_input;

        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);

        // An agent that takes only structured input, invoked with prose,
        // while the caller cheerfully claims the interface matched.
        let accepts = vec![
            "species_data".to_string(),
            "taxonomy".to_string(),
            "gbif_key".to_string(),
        ];
        let verified = bind_input(&accepts);
        stamp_input_binding(&mut ep, &verified, Some("declared:query"));

        assert!(
            ep.tags.contains(&"ibind:no_text_input".to_string()),
            "the card says there is no text port; that is the verdict"
        );
        assert!(ep.tags.contains(&"ibind:mismatch".to_string()));
        assert!(
            ep.tags.contains(&"ibind:claim-disagreed".to_string()),
            "a caller whose claim contradicts the card is the only signal \
             that client and server have drifted apart"
        );
    }

    #[test]
    fn a_matching_binding_is_recorded_without_a_dispute() {
        use fermi::port_trust::bind_input;

        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        let verified = bind_input(&["query".to_string()]);
        stamp_input_binding(&mut ep, &verified, Some("declared:query"));

        // Vocabulary unchanged from v0.16.0 so the tag series does not split
        // at this deploy.
        assert!(ep.tags.contains(&"ibind:declared-query".to_string()));
        assert!(!ep.tags.iter().any(|t| t.starts_with("declared:")));
        assert!(!ep.tags.iter().any(|t| t == "ibind:mismatch"));
        assert!(!ep.tags.iter().any(|t| t == "ibind:claim-disagreed"));
    }

    #[test]
    fn route_reason_lands_as_a_queryable_tag() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        stamp_invocation(
            &mut ep,
            &json!({
                "query_source": "declared_contract",
                "route_reason": "domain_specialist",
                "route_deliberate": true,
                "driver": "bucket_probability"
            }),
        );

        assert!(ep.tags.contains(&"route:domain_specialist".to_string()));
        // A deliberate route must NOT also be tagged as a fallback, or the
        // two populations cannot be separated when scoring.
        assert!(!ep.tags.contains(&"route:fallback".to_string()));
        assert_eq!(
            ep.context["invocation"]["route_reason"].as_str(),
            Some("domain_specialist")
        );
    }

    #[test]
    fn a_generalist_fallback_is_tagged_as_such() {
        // The distinction that makes routing measurable: an agent that
        // underperformed as the default is not evidence about the agent.
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        stamp_invocation(
            &mut ep,
            &json!({ "route_reason": "default", "route_deliberate": false }),
        );

        assert!(ep.tags.contains(&"route:default".to_string()));
        assert!(ep.tags.contains(&"route:fallback".to_string()));
    }

    #[test]
    fn overruling_the_strategist_is_tagged_so_the_router_can_be_scored() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        stamp_invocation(
            &mut ep,
            &json!({
                "route_reason": "domain_specialist",
                "route_deliberate": true,
                "route_overrode_suggestion": "macro_forecaster"
            }),
        );

        assert!(ep.tags.contains(&"route:overrode_fermi".to_string()));
        assert_eq!(
            ep.context["invocation"]["route_overrode_suggestion"].as_str(),
            Some("macro_forecaster")
        );
    }

    #[test]
    fn a_hostile_route_reason_cannot_invent_tags() {
        // Same guard the query_source path has: these become tag suffixes.
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        let before = ep.tags.len();
        stamp_invocation(
            &mut ep,
            &json!({ "route_reason": "nice try status:success" }),
        );
        assert_eq!(ep.tags.len(), before, "whitespace must be rejected");
        assert!(!ep.tags.iter().any(|t| t.starts_with("route:")));
    }

    #[test]
    fn a_recomposed_run_is_tagged_so_the_swap_is_findable() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        stamp_invocation(
            &mut ep,
            &json!({ "query_source": "declared_contract", "recomposed_from": "sentiment_analyzer" }),
        );
        assert!(ep.tags.contains(&"recomposed:true".to_string()));
    }

    #[test]
    fn a_caller_cannot_inject_arbitrary_tags() {
        // The invocation record is caller-supplied. It must not be able to
        // forge a status, smuggle whitespace into a tag, or write unbounded
        // strings into an indexed column.
        let out = output(AgentStatus::Failed, Some("real failure"), Some("tool_use"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        let before = ep.tags.len();
        stamp_invocation(
            &mut ep,
            &json!({
                "query_source": "nice try status:success",
                "input_binding": "x".repeat(500),
            }),
        );

        assert_eq!(ep.tags.len(), before, "malformed values must be dropped");
        assert_eq!(
            ep.tags.iter().filter(|t| t.starts_with("status:")).count(),
            1,
            "the real status tag must remain the only one"
        );
        assert!(ep.tags.contains(&"status:error".to_string()));
    }

    #[test]
    fn a_run_with_no_invocation_record_is_unchanged() {
        let out = output(AgentStatus::Success, None, Some("end_turn"));
        let mut ep = agent_output_to_episode(uuid::Uuid::new_v4(), "q", &out);
        let before = ep.tags.clone();
        stamp_invocation(&mut ep, &json!("not an object"));
        assert_eq!(ep.tags, before);
        assert!(ep.context.get("invocation").is_none());
    }
}
