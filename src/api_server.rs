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
        // Dyads
        .route(
            "/api/observatory/dyads/auto-form",
            post(handlers::observatory::auto_form_dyads_handler),
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
        // Spec 24 §3.3 / Sprint 2.3a: invite someone to a team.
        // Permission vocab here is the team role (owner|admin|member|
        // viewer). The legacy POST /api/teams/:id/members stays for
        // tooling that adds members directly without an invite step.
        .route(
            "/api/teams/:team_id/invites",
            post(handlers::invites::invite_to_team_handler)
                .get(handlers::invites::list_team_invites_handler),
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
        // ── Forecast group membership (Spec 25 §6.2) ────────────────
        .route(
            "/api/forecasts/:forecast_id/groups",
            put(handlers::relationships::membership::set_forecast_groups_handler),
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
            valence: card
                .metadata
                .valence
                .as_ref()
                .and_then(|v| serde_json::to_value(v).ok()),
            output_contract: card.capabilities.output_contract.clone(),
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
                        seed_cep_entities(memory_store, id, &card.agent_id, fc).await;
                    }
                }
            }
            Err(e) => eprintln!("Warning: failed to seed {}: {}", card.agent_id, e),
        }
    }
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
    agent_uuid: uuid::Uuid,
    agent_name: &str,
    fc: &fermi::agent_backend::agent_card::FermiContract,
) {
    // Check if CEP entities already exist to stay idempotent across restarts.
    let existing = memory_store
        .get_agent_entities(agent_uuid)
        .await
        .unwrap_or_default();
    let has_cep = existing.iter().any(|e| e.entity_type.starts_with("cep_"));
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
        // CEP seed entities have NULL embedding by design (no vector available
        // at seed time; the consolidation worker may later opportunistically
        // embed `entity_name` if needed). Stamp source_ref so the row is
        // identifiable as a CEP seed.
        let source_ref = serde_json::json!({
            "kind": "cep_seed",
            "agent_name": agent_name,
        });
        match memory_store
            .store_entity_with_provenance(entity, None, Some(source_ref))
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
        // Phase 2: tag execution provenance so the observatory can filter
        // by provider and per-provider calibration can work (Loop 5).
        model_used: output.metadata.model_used.clone(),
        provider_used: output.metadata.model_used.as_deref().map(|m| {
            if m.starts_with("claude") {
                "anthropic".to_string()
            } else if m.starts_with("gpt") || m.starts_with("o1") || m.starts_with("o3") {
                "openai".to_string()
            } else if m.starts_with("mistral") || m.starts_with("open-mistral") {
                "mistral".to_string()
            } else if m.starts_with("qwen") {
                "qwen".to_string()
            } else if m.starts_with("deepseek") {
                "deepseek".to_string()
            } else if m.contains("openrouter") || m.contains("/") {
                "openrouter".to_string()
            } else {
                "ollama".to_string()
            }
        }),
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
