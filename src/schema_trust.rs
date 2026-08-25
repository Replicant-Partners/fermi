//! # Schema trust contract
//!
//! v0.11.0 — the substrate that would have caught every schema/wire drift
//! bug from v0.10.15 → v0.10.29 at deploy time instead of at first user
//! click. Sixth consecutive "the code assumed X but the DB shipped Y"
//! hotfix is when we build the invariant.
//!
//! ## What it does
//!
//! At boot, after `run_migrations()` and `ensure_critical_schema()`, we
//! probe the DB for every schema object the Rust code assumes exists:
//!
//!   * Every table in [`SCHEMA_TABLES`] is present *and is a table*.
//!   * Every materialized view in [`SCHEMA_MATVIEWS`] is present *and is
//!     a materialized view*.
//!   * Every column in [`SCHEMA_COLUMNS`] is present on its relation.
//!   * Every function in [`SCHEMA_FUNCTIONS`] is present AND has the
//!     declared argument-type signature AND the declared return type.
//!     (Return type catches the v0.10.19 REAL-vs-FLOAT8 class.)
//!
//! Any missing/mismatched object is logged LOUDLY to stderr.
//!
//! ## Why `pg_catalog` and not `information_schema`
//!
//! This is load-bearing, not stylistic. **`information_schema` omits
//! materialized views entirely** — they appear in neither
//! `information_schema.tables` nor `information_schema.columns`.
//!
//! v0.11.0 shipped with `fermi_leaderboard` (a MATERIALIZED VIEW, see
//! `migrations/094_fermi_forecasting.sql`) declared in [`SCHEMA_TABLES`]
//! and probed via `information_schema.tables`. The consequence: the
//! contract reported that table permanently missing, [`verify`] could
//! *never* return healthy, and `SCHEMA_STRICT=1` would have aborted
//! every boot — so it was never enabled anywhere. The drift detector
//! was itself an always-failing guard, and because the verdict was only
//! ever written to stderr, nothing ever noticed.
//!
//! Probing `pg_catalog.pg_class` / `pg_catalog.pg_attribute` covers every
//! relation kind uniformly. Relation *kind* is now part of the contract
//! too, so "matview silently replaced by a table" is detectable drift
//! rather than an invisible pass.
//!
//! ## Behaviour
//!
//! Two modes, controlled by `SCHEMA_STRICT` env var:
//!
//!   * **`SCHEMA_STRICT` unset / `SCHEMA_STRICT=0`** — default. Log every
//!     drift to stderr with a WARNING banner. Continue booting. Suitable
//!     for gradual rollout: the deploy proceeds, but the operator sees
//!     the drift in Railway logs immediately.
//!
//!   * **`SCHEMA_STRICT=1`** — abort boot on any drift. The intended
//!     production posture once the contract is comprehensive enough
//!     that a false positive is rarer than a real drift.
//!
//! ## Why hand-declared and not auto-generated
//!
//! The existing pre-commit lint (`scripts/lint-schema-consistency.py`)
//! parses migration files at author time — it catches drift at commit,
//! before deploy. But it's opt-in on the developer machine, only fires
//! on qualified refs (`table.col` or `alias.col` with a JOIN mapping),
//! and doesn't run in CI.
//!
//! The boot check here is the last line of defense: it runs against
//! the *actual production DB* at *actual boot time*, so it catches:
//!
//!   * Migrations that didn't run (PgBouncer ate them — v0.10.27).
//!   * Columns removed from a table between deploys.
//!   * Function signatures changed by an ops-team hotfix.
//!
//! The hand-declared manifest is the tradeoff: high signal on the
//! columns we actually depend on, low maintenance burden compared to
//! generating from every `sqlx::query!` in the codebase.
//!
//! ## Extending
//!
//! When a new column/table/function becomes production-critical, add
//! it here. Rule of thumb: **if the Rust code would 500 on its
//! absence, it belongs in the contract.**

use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════
// The contract
// ═══════════════════════════════════════════════════════════════════

/// Ordinary (or partitioned) tables the Rust code assumes exist.
///
/// Materialized views do **not** belong here — see [`SCHEMA_MATVIEWS`].
pub const SCHEMA_TABLES: &[&str] = &[
    // Core identity
    "users",
    "api_keys",
    // Agent lifecycle
    "agents",
    "agent_versions",
    "workspace_agents",
    "admin_bypass_events",
    // The record of what the platform refused. Its absence was the audit's
    // §2.2, and the asymmetry is the reason it is declared next to its own
    // inverse: the BYPASS of the admission gate has been audited since it
    // existed, and the refusal was not recorded anywhere at all.
    "gate_decisions",
    // Fermi forecasting
    "fermi_forecasts",
    "fermi_forecast_updates",
    "fermi_market_observations",
    "fermi_portfolios",
    "fermi_portfolio_forecasts",
    "fermi_notebooks",
    // Relationships and pending cascades
    "forecast_relationships",
    "forecast_relationship_groups",
    "forecast_invites",
    "pending_cascades",
    "forecast_commitments",
    "forecast_splits",
    "forecast_spacetime",
    // Combinatorial credit assignment (mig-187, mig-188). Declared here as
    // well as in SCHEMA_COLUMNS: a column entry whose relation is undeclared
    // produces a check that can never pass and never says why, which is what
    // `every_column_belongs_to_a_declared_relation` exists to prevent.
    // See docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md
    "forecast_agent_claims",
    "forecast_attributions",
    "forecast_agent_credit",
    "forecast_agent_interactions",
    // Workspaces / teams
    "teams",
    "team_members",
    "object_shares",
    // Compositions
    "composition_versions",
    // Apps
    "apps",
    // Memory (mig-010 core)
    "episodes",
    "entities",
    "facts",
    "semantic_rules",
    "communities",
    "ontology_snapshots",
    "consolidation_jobs",
    "consolidation_locks",
    // Observability (mig-103/104/105/106)
    "episode_corrections",
    "eval_signals",
    "eval_runs",
    "eval_test_cases",
    "agent_timeline_entries",
    "dyad_state",
    // mig-133. The observatory's social graph names dyads from here and
    // upserts on rename, so a drift makes "Save name" 503 rather than fail loud.
    "dyad_profiles",
    "anomaly_events",
    "agent_observability_state",
    "hitl_actions",
    // mig-016. Read by the Loop 3a row on the loop-health panel.
    // (`workspace_agents`, which Loops 3a and 4 join against, is already
    // declared above with the workspace relations.)
    "coherence_evaluations",
    // mig-210. Their columns were declared in SCHEMA_COLUMNS without the
    // relations, which `every_column_belongs_to_a_declared_relation` exists to
    // catch: a column entry whose table is undeclared produces a check that can
    // never pass and never says why.
    "workspace_intentions",
    "workspace_intention_signals",
    // Harness / benchmark
    "harness_snapshots",
    // v0.11.2 — orchestra registry
    "orchestra_membership_requests",
    // v0.11.9 — Stripe idempotency claims (mig 182). Load-bearing for money:
    // billing.rs FAILS CLOSED if this table is unreachable, so its absence
    // silently stops all credit purchases rather than double-crediting.
    // Exactly the kind of object that belongs in the contract.
    "stripe_sessions_processed",
];

/// Materialized views the Rust code assumes exist.
///
/// Declared separately from [`SCHEMA_TABLES`] because `pg_class.relkind`
/// distinguishes them (`'m'` vs `'r'`/`'p'`), and because conflating the
/// two is exactly the bug that made the v0.11.0 contract unsatisfiable.
/// See the module header.
pub const SCHEMA_MATVIEWS: &[&str] = &[
    // migrations/094_fermi_forecasting.sql:178, rebuilt by
    // migrations/167_fermi_leaderboard_float8_minmax.sql:77.
    // Refreshed via refresh_fermi_leaderboard() — see SCHEMA_FUNCTIONS.
    "fermi_leaderboard",
];

/// Plain views the Rust code assumes exist.
///
/// Declared separately from [`SCHEMA_TABLES`] and [`SCHEMA_MATVIEWS`] for
/// the same reason those are separate from each other: `relkind` tells them
/// apart, and conflating kinds is what made the v0.11.0 contract
/// unsatisfiable.
///
/// A view here is a *derived* definition — no refresh step, so it cannot go
/// stale. That is the whole reason to prefer one over a matview or a
/// denormalised column; see [`crate::rollup_trust`].
pub const SCHEMA_VIEWS: &[&str] = &[
    // migrations/192_agent_execution_rollup.sql. THE source of truth for
    // agent run counts, cost and latency — the five `agents.*` counters it
    // replaces are never written by any code path. Six user-facing
    // surfaces read this view; if it disappears they all 500, which is
    // strictly better than the silent zeros they used to serve.
    "agent_execution_rollup",
];

/// `pg_class.relkind` values acceptable for a [`SCHEMA_TABLES`] entry:
/// ordinary table or partitioned table.
pub const TABLE_KINDS: &[&str] = &["r", "p"];

/// `pg_class.relkind` values acceptable for a [`SCHEMA_MATVIEWS`] entry.
pub const MATVIEW_KINDS: &[&str] = &["m"];

/// `pg_class.relkind` values acceptable for a [`SCHEMA_VIEWS`] entry.
pub const VIEW_KINDS: &[&str] = &["v"];

/// Human-readable rendering of a `pg_class.relkind` code, for drift
/// reports that an operator has to read at 3am.
pub fn describe_relkind(k: &str) -> &'static str {
    match k {
        "r" => "ordinary table",
        "p" => "partitioned table",
        "m" => "materialized view",
        "v" => "view",
        "f" => "foreign table",
        "i" => "index",
        "S" => "sequence",
        "c" => "composite type",
        "t" => "TOAST table",
        _ => "unknown relkind",
    }
}

/// Columns the Rust code depends on. Rule of thumb: any column whose
/// absence would 500 a user-facing request. Grouped by table for
/// readability.
///
/// **This list is not exhaustive** — it's populated in priority order
/// starting with the columns whose absence has actually caused
/// production incidents (v0.10.15 → v0.10.29). Extend when a new
/// column becomes load-bearing.
pub const SCHEMA_COLUMNS: &[(&str, &str)] = &[
    // ── observability sweeper (live Loop 1 → Loop 2) ──────────────
    // `sweep_observability_once` joins these two to find agents with
    // unscanned timeline entries. Declared because the sweep's only error
    // handling is a log line: a renamed column would silently stop drift and
    // anomaly detection platform-wide rather than failing loudly. An earlier
    // draft did exactly that, guessing `last_scanned_at`.
    ("agent_observability_state", "agent_id"),
    ("agent_observability_state", "last_scan_completed_at"),
    ("agent_timeline_entries", "agent_id"),
    ("agent_timeline_entries", "created_at"),
    ("agent_timeline_entries", "persona_version"),
    ("agent_timeline_entries", "anomaly_flags"),
    ("agent_timeline_entries", "dim_scores"),
    // `anomaly_events` has `created_at`, not `detected_at`. The Loop 2 health
    // panel queried the latter, the query errored, and the panel reported Loop 2
    // as "unmeasured" — which reads as "no data yet" rather than "this page is
    // broken". Declared so a rename fails in CI instead of on the dashboard.
    ("anomaly_events", "agent_id"),
    ("anomaly_events", "created_at"),
    ("anomaly_events", "requires_review"),
    ("anomaly_events", "resolved_at"),
    ("anomaly_events", "kind"),
    ("anomaly_events", "severity"),
    // ── Loop 3 Stage 0 (mig-210) ───────────────────────────────────
    ("workspace_intentions", "workspace_id"),
    ("workspace_intentions", "agent_id"),
    ("workspace_intentions", "status"),
    ("workspace_intentions", "targets"),
    ("workspace_intentions", "depends_on"),
    ("workspace_intentions", "embedding"),
    ("workspace_intention_signals", "workspace_id"),
    ("workspace_intention_signals", "relation_type"),
    // ── Loop health panel (GET /api/observatory/agents/:id/loops) ───────
    //
    // The `detected_at` incident above was not specific to Loop 2 — that row
    // just happened to be the one that got a column wrong. All six loop rows
    // degrade to "unmeasured" on a bad column, and "unmeasured" is designed to
    // read as "nothing here yet", so every one of them can fail silently in the
    // same way. Declaring what each row reads makes that class of failure a CI
    // error rather than a plausible-looking dashboard.
    //
    // Loop 1a — the two halves of individual learning.
    ("eval_runs", "agent_id"),
    ("eval_signals", "agent_id"),
    ("eval_signals", "dimension"),
    ("eval_signals", "score"),
    ("eval_signals", "rationale"),
    ("eval_signals", "evaluator_name"),
    ("consolidation_jobs", "agent_id"),
    ("consolidation_jobs", "status"),
    ("entities", "agent_id"),
    ("facts", "agent_id"),
    ("semantic_rules", "agent_id"),
    // ── Extractor read-back (`extractor_self_knowledge`) ────────────────────
    //
    // The query that hands the extractor its own learned rules ends in `.ok()?`
    // — a column error returns `None`, which is indistinguishable from "this
    // agent has learned nothing yet". A rename would therefore switch Loop 1's
    // read-back off in silence, and the only symptom would be the extractor
    // quietly ceasing to improve. Declared so that fails in CI instead.
    ("semantic_rules", "rule_content"),
    ("semantic_rules", "rule_description"),
    ("semantic_rules", "confidence_score"),
    ("semantic_rules", "verification_status"),
    ("semantic_rules", "is_active"),
    ("semantic_rules", "invalidated_at"),
    // ── Extraction-utility signal ─────────────────────────────────────
    //
    // `extracted_by` (mig-201) is who WROTE a rule, as against `agent_id` who it
    // is FOR. `application_count` / `last_validated_at` are the retrieval
    // counters — present since mig-010 and, until now, never written by anything.
    // Together they are the extractor's only quality signal, so a drift in any of
    // the three silently returns Loop 1 for the ontologist to having no signal at
    // all, which is the state this work exists to leave behind.
    ("semantic_rules", "extracted_by"),
    ("semantic_rules", "application_count"),
    ("semantic_rules", "last_validated_at"),
    // The signal's destination. `confidence` in particular: it is what stops a
    // 6-rule score being read with the authority of a 600-rule one.
    ("eval_signals", "confidence"),
    ("eval_signals", "evaluator_version"),
    ("eval_signals", "evaluator_tier"),
    ("eval_signals", "created_at"),
    ("episodes", "agent_id"),
    ("episodes", "consolidated"),
    // Loop 3a — workspace coherence, scoped to the agent's workspaces via
    // `workspace_agents` (already declared below with the workspace columns).
    ("coherence_evaluations", "eval_id"),
    ("coherence_evaluations", "workspace_id"),
    ("coherence_evaluations", "global_score"),
    ("coherence_evaluations", "created_at"),
    // Loop 4 — composition evolution. `proposed_by`/`accepted_by` are what
    // separate a strategist proposal that a human accepted (the loop closing)
    // from a human editing a team (not the loop at all).
    ("composition_versions", "proposed_by"),
    ("composition_versions", "accepted_by"),
    // ── Social graph (GET /api/observatory/agents/:id/relationships) ────
    //
    // The dyad cards resolve the human half through `users` and upsert the
    // operator's label into `dyad_profiles`. A drift here does not error — the
    // lookup just returns nothing and every card falls back to "unknown user",
    // which looks like missing data rather than a broken join.
    ("users", "github_username"),
    ("dyad_profiles", "dyad_id"),
    ("dyad_profiles", "agent_id"),
    ("dyad_profiles", "human_id"),
    ("dyad_profiles", "display_name"),
    ("dyad_profiles", "notes"),
    ("dyad_profiles", "auto_formed"),
    ("dyad_profiles", "formed_at"),
    ("dyad_profiles", "total_interactions"),
    ("dyad_profiles", "first_interaction_at"),
    ("dyad_profiles", "last_interaction_at"),
    // ── agents ─────────────────────────────────────────────────────
    // Every one of these has been referenced in a bug in the last
    // month. The trust contract exists to keep them present.
    ("agents", "agent_id"),
    ("agents", "agent_name"),
    ("agents", "agent_type"),
    ("agents", "tier"),
    ("agents", "status"),
    ("agents", "visibility"),
    ("agents", "user_id"), // was assumed as `owner_id` in v0.10.15/16
    ("agents", "created_at"),
    ("agents", "updated_at"), // v0.10.18/v0.10.27 — mig-166 got eaten by PgBouncer
    // Present, correctly typed, and permanently zero — nothing writes it.
    // Declared here because `Agent`'s SELECT list names it, so its absence
    // WOULD 500 the row mapper. Its *emptiness* is a separate contract:
    // see `crate::rollup_trust`, which exists because this check passes on
    // a column that lies.
    ("agents", "total_executions"),
    ("agents", "description"),
    ("agents", "system_prompt"),
    ("agents", "tags"),
    ("agents", "fork_pricing"),
    ("agents", "forked_from"),
    ("agents", "fork_count"),
    // ── agent_execution_rollup (view, migrations/192) ───────────────
    // Six user-facing surfaces read these. `pg_attribute` covers views as
    // well as tables, so the column contract applies unchanged — which
    // means renaming a column in the view definition is caught at boot
    // rather than at the next page load.
    ("agent_execution_rollup", "agent_id"),
    ("agent_execution_rollup", "executions"),
    ("agent_execution_rollup", "successful"),
    ("agent_execution_rollup", "failed"),
    ("agent_execution_rollup", "cost_usd"),
    ("agent_execution_rollup", "tokens_used"),
    ("agent_execution_rollup", "avg_execution_time_ms"),
    ("agent_execution_rollup", "episodes_missing_cost"),
    // ── episodes (mig-199) ─────────────────────────────────────────
    // The agent's own output, retained verbatim. Declared here for the
    // same reason `agent_execution_rollup`'s columns are: a new source of
    // truth with no existence guarantee just relocates the problem it was
    // built to solve. If this column goes missing, output-type induction
    // silently reads `None` for every row — a corpus that looks empty
    // rather than absent, which is the failure this contract exists to
    // make loud.
    ("episodes", "response_text"),
    // ── fermi_forecasts ────────────────────────────────────────────
    ("fermi_forecasts", "id"),
    ("fermi_forecasts", "owner_id"), // realigned TEXT via mig-165
    ("fermi_forecasts", "question_text"),
    ("fermi_forecasts", "predicted_probability"),
    ("fermi_forecasts", "confidence_interval_low"),
    ("fermi_forecasts", "confidence_interval_high"),
    ("fermi_forecasts", "brier_score"), // REAL — see v0.10.19 float8 cast
    ("fermi_forecasts", "actual_outcome"),
    // mig-174: the immutable audit anchor Brier is reproducible from.
    // `predicted_probability` stays mutable post-resolution; this does
    // not. Any consumer auditing or recomputing a score MUST read this.
    ("fermi_forecasts", "scored_probability"),
    // mig-174: structured resolution provenance. Distinguishes operator
    // vs. real oracle vs. price-heuristic vs. synthetic backtest rows,
    // which `resolved_by` alone could not.
    ("fermi_forecasts", "resolution_source"),
    ("fermi_forecasts", "status"),
    ("fermi_forecasts", "resolved_at"),
    ("fermi_forecasts", "agents_used"), // JSONB, GIN-indexed post mig-168
    ("fermi_forecasts", "visibility"),
    ("fermi_forecasts", "team_id"),
    ("fermi_forecasts", "tags"),
    ("fermi_forecasts", "created_at"),
    ("fermi_forecasts", "updated_at"),
    // v0.11.2 — manager-effect placeholder (Team Brier − Counterfactual Brier)
    ("fermi_forecasts", "counterfactual_brier"),
    // ── forecast_agent_claims (mig-187) ────────────────────────────
    // Append-only ledger of each agent's individual quantitative claim.
    // Load-bearing for per-agent credit: the params write it shadows is
    // current-state only, so if these columns go missing the claims are
    // lost silently and cannot be reconstructed after the fact.
    ("forecast_agent_claims", "claim_id"),
    ("forecast_agent_claims", "workspace_id"),
    ("forecast_agent_claims", "agent_id"),
    ("forecast_agent_claims", "agent_name"),
    ("forecast_agent_claims", "driver"),
    ("forecast_agent_claims", "p50"),
    ("forecast_agent_claims", "neutral_value"),
    ("forecast_agent_claims", "claimed_at"),
    // ── forecast attribution (mig-188) ───────────────────────────
    // Per-agent Shapley credit and its validity gates. `efficiency_residual`
    // and `reconstruction_error` are load-bearing: a consumer that reads
    // shapley_value without filtering on them can act on credit derived from
    // Monte Carlo noise, or from a reconstruction of a forecast that never
    // existed. Losing those columns must fail loudly, not degrade quietly.
    ("forecast_attributions", "forecast_id"),
    ("forecast_attributions", "neutralisation"),
    ("forecast_attributions", "seed"),
    ("forecast_attributions", "p_baseline"),
    ("forecast_attributions", "p_full"),
    ("forecast_attributions", "team_improvement"),
    ("forecast_attributions", "efficiency_residual"),
    ("forecast_attributions", "reconstruction_error"),
    ("forecast_agent_credit", "forecast_id"),
    ("forecast_agent_credit", "neutralisation"),
    ("forecast_agent_credit", "agent_id"),
    ("forecast_agent_credit", "agent_name"),
    ("forecast_agent_credit", "shapley_value"),
    ("forecast_agent_interactions", "forecast_id"),
    ("forecast_agent_interactions", "agent_a"),
    ("forecast_agent_interactions", "agent_b"),
    ("forecast_agent_interactions", "interaction_index"),
    // ── users ──────────────────────────────────────────────────────
    ("users", "id"),
    ("users", "user_id"), // TEXT, the substrate identity (v0.10.9 realign)
    ("users", "email"),
    ("users", "display_name"),
    ("users", "auth_provider"),
    // ── admin_bypass_events (mig-164) ─────────────────────────────
    ("admin_bypass_events", "event_id"),
    ("admin_bypass_events", "admin_user_id"),
    ("admin_bypass_events", "target_type"),
    ("admin_bypass_events", "target_id"),
    ("admin_bypass_events", "action"),
    ("admin_bypass_events", "details"),
    ("admin_bypass_events", "created_at"),
    // ── apps ───────────────────────────────────────────────────────
    ("apps", "id"),
    ("apps", "slug"),
    ("apps", "name"),
    ("apps", "owner_user_id"),
    ("apps", "visibility"),
    ("apps", "workspace_template"),
    ("apps", "created_at"),
    ("apps", "updated_at"),
    // ── teams / workspaces ─────────────────────────────────────────
    ("teams", "id"),
    ("teams", "name"),
    ("teams", "slug"),
    ("teams", "owner_id"),
    ("teams", "origin"),
    ("teams", "mission"),
    ("teams", "coordination_strategist_id"),
    ("teams", "strategist_assigned_at"),
    // ── workspace_agents (mig-015) ─────────────────────────────────
    ("workspace_agents", "workspace_id"),
    ("workspace_agents", "agent_id"),
    ("workspace_agents", "added_by"),
    ("workspace_agents", "added_at"),
    ("workspace_agents", "relationship"),
    // ── composition_versions ──────────────────────────────────────
    // ── composition_versions ───────────────────────
    ("composition_versions", "composition_version_id"),
    ("composition_versions", "workspace_id"),
    ("composition_versions", "rejected_by"),
    ("composition_versions", "rejection_note"),
    // ── orchestra_membership_requests (mig-172) ─────────────
    ("orchestra_membership_requests", "request_id"),
    ("orchestra_membership_requests", "orchestra_name"),
    ("orchestra_membership_requests", "agent_id"),
    ("orchestra_membership_requests", "requested_by"),
    ("orchestra_membership_requests", "proposed_contract"),
    ("orchestra_membership_requests", "status"),
    ("orchestra_membership_requests", "reviewed_by"),
    ("orchestra_membership_requests", "review_note"),
];

/// Functions the Rust code depends on. Now includes return type so we
/// catch signature drift on both directions (v0.10.19 was
/// `resolve_forecast()` returning REAL where code assumed FLOAT8).
///
/// Signature format is the **comma-separated input argument type list**,
/// space-normalised and lowercased — e.g. `"text, boolean"`, or `""` for a
/// zero-argument function. Return type matches
/// `pg_catalog.format_type(prorettype)`.
///
/// Do **not** switch the probe to `pg_get_function_identity_arguments()`:
/// it includes parameter *names* (`"p_forecast_id text, ..."`), which can
/// never match a type-only declaration. That mistake made
/// `resolve_forecast` report permanent signature drift — the same
/// always-fails-guard class as the matview bug. See [`verify`].
pub const SCHEMA_FUNCTIONS: &[(&str, &str, &str)] = &[
    // (name, args, return_type)
    ("compute_brier_score", "real, boolean", "real"),
    // v0.10.19 witness — return type declared explicitly so any future
    // change to the SQL function surfaces as a contract violation
    // rather than a runtime decoding panic.
    ("resolve_forecast", "text, boolean, text, text", "real"),
    ("fn_forecast_spacetime_on_update", "", "trigger"),
    // v0.10.19 companion: leaderboard refresh needs the view to exist,
    // captured via SCHEMA_TABLES; the refresh function is here.
    ("refresh_fermi_leaderboard", "", "void"),
];

/// CHECK and FK constraints the platform's correctness depends on.
///
/// # Why constraints need their own list
///
/// `SCHEMA_COLUMNS` asks whether a column exists. A constraint is the other
/// half of the same question and nothing was asking it, which allowed the
/// following to be true for the entire life of the project:
///
/// `credit_ledger_tx_type_check` is declared by **seventeen** migrations — 027,
/// 030, 032, 035, 042, 045, 049, 050, 051, 052, 057, 059, 061, 063, 064, 075
/// and 099 — and **does not exist in production**. Three of those migrations
/// exist for no other purpose than to fix it
/// (`*_fix_tx_type_constraint.sql`), which is what repeatedly repairing
/// something without ever checking the repair looks like from the outside.
///
/// The mechanism is specific and worth keeping. Each early migration ran
/// `DROP CONSTRAINT IF EXISTS` and `ADD CONSTRAINT` as two top-level
/// statements. Through PgBouncer in transaction-pooling mode those are two
/// separate implicit transactions, so when the `ADD` failed — and it failed,
/// because rows already violated the new list — the `DROP` stayed committed.
/// `run_migrations` logs a migration failure with `eprintln!` and continues, so
/// **the net effect of each attempted fix was to delete the constraint.**
/// Migration 075 finally wrapped the pair in a DO block, making it atomic and
/// therefore correct; by then 22 of the 43 live `tx_type` values were absent
/// from its list, so its `ADD` can never succeed. It is now permanently a
/// no-op that logs a warning nobody reads.
///
/// `tx_type` is a bare `&str` parameter at every call site in
/// `fermi-auth/src/credits.rs` — there is no enum and no closed set in Rust —
/// so this constraint was the *only* thing standing between a typo and a
/// silently mis-categorised row on the credit ledger.
///
/// # Why an explicit list rather than parsing the migrations
///
/// "Every constraint any migration ever declared must exist" is wrong: a later
/// migration may legitimately drop one, and `ADD CONSTRAINT IF NOT EXISTS`
/// makes the name hard to extract without false positives (a first pass
/// "found" constraints named `if` and `validates`). Same reasoning as
/// `SCHEMA_COLUMNS`, and the same rule of thumb: list a constraint when its
/// absence would let bad data in unnoticed. Not exhaustive; extend when a
/// constraint becomes load-bearing.
pub const SCHEMA_CONSTRAINTS: &[(&str, &str, &str)] = &[(
    "credit_ledger",
    "credit_ledger_tx_type_check",
    "The only closed set of transaction types anywhere in the system. \
     `credit_charge` and friends take `tx_type: &str`, so without this a \
     misspelled type is accepted, lands on the ledger, and is invisible to \
     every report that groups by it — the money still moves, it is just filed \
     under a category nobody queries.",
)];

// ═══════════════════════════════════════════════════════════════════
// Verdict
// ═══════════════════════════════════════════════════════════════════

/// Result of a single schema check. Serializes cleanly for the
/// `/api/admin/schema-health` endpoint AND is inspectable at boot.
#[derive(Debug, Default)]
pub struct SchemaVerdict {
    pub missing_tables: Vec<&'static str>,
    /// Materialized views that don't exist at all.
    pub missing_matviews: Vec<&'static str>,
    /// Plain views that don't exist at all.
    pub missing_views: Vec<&'static str>,
    /// Relations that exist under the contracted name but with the wrong
    /// `relkind` — e.g. a materialized view replaced by a plain table.
    /// `(name, expected_kind, found_kind(s))`.
    pub relation_kind_mismatches: Vec<(&'static str, &'static str, String)>,
    pub missing_columns: Vec<(&'static str, &'static str)>,
    /// Functions that don't exist at all (name not found).
    pub missing_functions: Vec<(&'static str, &'static str, &'static str)>,
    /// Functions found but with drifted argument signature. Third
    /// element is the actual signatures the DB has, joined with `|`.
    pub function_sig_mismatches: Vec<(&'static str, &'static str, String)>,
    /// Functions found but with drifted return type. Third element is
    /// the actual return type.
    pub function_return_mismatches: Vec<(&'static str, &'static str, String)>,
}

impl SchemaVerdict {
    /// True if every check passed.
    pub fn is_healthy(&self) -> bool {
        self.missing_tables.is_empty()
            && self.missing_matviews.is_empty()
            && self.missing_views.is_empty()
            && self.relation_kind_mismatches.is_empty()
            && self.missing_columns.is_empty()
            && self.missing_functions.is_empty()
            && self.function_sig_mismatches.is_empty()
            && self.function_return_mismatches.is_empty()
    }

    pub fn total_issues(&self) -> usize {
        self.missing_tables.len()
            + self.missing_matviews.len()
            + self.missing_views.len()
            + self.relation_kind_mismatches.len()
            + self.missing_columns.len()
            + self.missing_functions.len()
            + self.function_sig_mismatches.len()
            + self.function_return_mismatches.len()
    }

    /// Serialize to the JSON shape the `/api/admin/schema-health`
    /// endpoint returns. Preserves backwards compatibility with the
    /// pre-v0.11.0 response body.
    pub fn to_health_json(&self) -> Value {
        let kind_drift = |name: &&'static str| -> Option<String> {
            self.relation_kind_mismatches
                .iter()
                .find(|(n, _, _)| n == name)
                .map(|(_, _, found)| found.clone())
        };

        let tables: Vec<Value> = SCHEMA_TABLES
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "present": !self.missing_tables.contains(name),
                    "kind_drift": kind_drift(name),
                })
            })
            .collect();

        let matviews: Vec<Value> = SCHEMA_MATVIEWS
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "present": !self.missing_matviews.contains(name),
                    "kind_drift": kind_drift(name),
                })
            })
            .collect();

        let views: Vec<Value> = SCHEMA_VIEWS
            .iter()
            .map(|name| {
                json!({
                    "name": name,
                    "present": !self.missing_views.contains(name),
                    "kind_drift": kind_drift(name),
                })
            })
            .collect();

        let columns: Vec<Value> = SCHEMA_COLUMNS
            .iter()
            .map(|(t, c)| {
                json!({
                    "table": t,
                    "column": c,
                    "present": !self.missing_columns.contains(&(*t, *c)),
                })
            })
            .collect();

        let functions: Vec<Value> = SCHEMA_FUNCTIONS
            .iter()
            .map(|(name, sig, ret)| {
                let missing = self.missing_functions.iter().any(|(n, _, _)| n == name);
                let sig_drift = self
                    .function_sig_mismatches
                    .iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, _, found)| found.clone());
                let ret_drift = self
                    .function_return_mismatches
                    .iter()
                    .find(|(n, _, _)| n == name)
                    .map(|(_, _, found)| found.clone());

                json!({
                    "name":            name,
                    "signature":       sig,
                    "return_type":     ret,
                    "present":         !missing,
                    "signature_drift": sig_drift,
                    "return_drift":    ret_drift,
                })
            })
            .collect();

        let status = if self.is_healthy() {
            "healthy"
        } else {
            "degraded"
        };

        json!({
            "status":     status,
            "checked_at": chrono::Utc::now().to_rfc3339(),
            "tables":     tables,
            "matviews":   matviews,
            "views":      views,
            "columns":    columns,
            "functions":  functions,
            "summary": {
                "tables":    { "total": SCHEMA_TABLES.len(),    "missing": self.missing_tables.len() },
                "matviews":  { "total": SCHEMA_MATVIEWS.len(),  "missing": self.missing_matviews.len() },
                "views":     { "total": SCHEMA_VIEWS.len(),     "missing": self.missing_views.len() },
                "relation_kind_drift": self.relation_kind_mismatches.len(),
                "columns":   { "total": SCHEMA_COLUMNS.len(),   "missing": self.missing_columns.len() },
                "functions": {
                    "total":               SCHEMA_FUNCTIONS.len(),
                    "missing":             self.missing_functions.len(),
                    "signature_drift":     self.function_sig_mismatches.len(),
                    "return_type_drift":   self.function_return_mismatches.len(),
                },
                "total_issues": self.total_issues(),
            },
        })
    }
}

// ═══════════════════════════════════════════════════════════════════
// The check
// ═══════════════════════════════════════════════════════════════════

/// Probe the DB for every entry in the contract. Returns a verdict.
///
/// Single round trip per axis (tables, columns, functions) via
/// `ANY($1)` — even a bloated contract stays well under any timeout.
pub async fn verify(db: &PgPool) -> Result<SchemaVerdict, sqlx::Error> {
    let mut verdict = SchemaVerdict::default();

    // ── Relations: tables + materialized views ────────────────────
    //
    // `pg_class`, NOT `information_schema.tables` — the latter omits
    // materialized views, which made the v0.11.0 contract permanently
    // unsatisfiable. See the module header.
    let all_relations: Vec<&str> = SCHEMA_TABLES
        .iter()
        .chain(SCHEMA_MATVIEWS.iter())
        .chain(SCHEMA_VIEWS.iter())
        .copied()
        .collect();

    let present_relations: Vec<(String, String)> = sqlx::query(
        "SELECT c.relname, c.relkind::text AS relkind \
           FROM pg_catalog.pg_class c \
           JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = 'public' \
            AND c.relname = ANY($1)",
    )
    .bind(all_relations.as_slice())
    .fetch_all(db)
    .await?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("relname").ok()?,
            r.try_get::<String, _>("relkind").ok()?,
        ))
    })
    .collect();

    let found_kinds = |name: &str| -> Vec<&str> {
        present_relations
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, k)| k.as_str())
            .collect()
    };

    // Which verdict bucket an absent relation lands in. Three kinds now,
    // so a bool no longer distinguishes them.
    enum Slot {
        Table,
        Matview,
        View,
    }

    for (contract, want_kinds, want_label, slot) in [
        (SCHEMA_TABLES, TABLE_KINDS, "table", Slot::Table),
        (
            SCHEMA_MATVIEWS,
            MATVIEW_KINDS,
            "materialized view",
            Slot::Matview,
        ),
        (SCHEMA_VIEWS, VIEW_KINDS, "view", Slot::View),
    ] {
        for &name in contract {
            let found = found_kinds(name);

            if found.is_empty() {
                match slot {
                    Slot::Table => verdict.missing_tables.push(name),
                    Slot::Matview => verdict.missing_matviews.push(name),
                    Slot::View => verdict.missing_views.push(name),
                }
            } else if !found.iter().any(|k| want_kinds.contains(k)) {
                // Present under the contracted name, wrong kind. This is
                // drift, not absence — report it as such so the operator
                // isn't sent looking for a missing migration.
                let found_desc = found
                    .iter()
                    .map(|k| describe_relkind(k))
                    .collect::<Vec<_>>()
                    .join(" | ");
                verdict
                    .relation_kind_mismatches
                    .push((name, want_label, found_desc));
            }
        }
    }

    // ── Columns ───────────────────────────────────────────────────
    //
    // `pg_attribute` rather than `information_schema.columns`, for the
    // same reason as above: matview columns are absent from
    // `information_schema`. `attnum > 0` skips system columns;
    // `attisdropped` skips logically-dropped ones.
    let table_names: Vec<String> = SCHEMA_COLUMNS.iter().map(|(t, _)| t.to_string()).collect();
    let col_names: Vec<String> = SCHEMA_COLUMNS.iter().map(|(_, c)| c.to_string()).collect();

    let present_columns: HashSet<(String, String)> = sqlx::query(
        "SELECT c.relname AS table_name, a.attname AS column_name \
           FROM pg_catalog.pg_attribute a \
           JOIN pg_catalog.pg_class c     ON c.oid = a.attrelid \
           JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
          WHERE n.nspname = 'public' \
            AND a.attnum > 0 \
            AND NOT a.attisdropped \
            AND c.relname = ANY($1) \
            AND a.attname = ANY($2)",
    )
    .bind(&table_names)
    .bind(&col_names)
    .fetch_all(db)
    .await?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("table_name").ok()?,
            r.try_get::<String, _>("column_name").ok()?,
        ))
    })
    .collect();

    for &(t, c) in SCHEMA_COLUMNS {
        if !present_columns.contains(&(t.to_string(), c.to_string())) {
            verdict.missing_columns.push((t, c));
        }
    }

    // ── Functions (name + signature + return type) ────────────────
    // pg_proc joined with pg_namespace + pg_type to get name,
    // argument-identity-list, and formatted return type. Case-insensitive
    // + space-normalized signature match so `text, boolean` == `TEXT,
    // BOOLEAN` == `text,boolean`.
    let fn_names: Vec<String> = SCHEMA_FUNCTIONS
        .iter()
        .map(|(n, _, _)| n.to_string())
        .collect();

    // `args` is built from `proargtypes` rather than
    // `pg_get_function_identity_arguments(oid)` because the latter includes
    // parameter NAMES, so a type-only contract entry could never match it.
    // v0.11.0 used it and `resolve_forecast` reported permanent signature
    // drift as a result. `proargtypes` covers IN arguments in declaration
    // order and yields '' for a zero-arg function — exactly the contract's
    // format.
    let present_functions: Vec<(String, String, String)> = sqlx::query(
        "SELECT p.proname, \
                COALESCE(( \
                    SELECT string_agg(pg_catalog.format_type(t.oid, NULL), ', ' \
                                      ORDER BY t.ord) \
                      FROM unnest(p.proargtypes) WITH ORDINALITY AS t(oid, ord) \
                ), '') AS args, \
                pg_catalog.format_type(p.prorettype, NULL) AS ret \
           FROM pg_catalog.pg_proc p \
           JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace \
          WHERE n.nspname = 'public' \
            AND p.proname = ANY($1)",
    )
    .bind(&fn_names)
    .fetch_all(db)
    .await?
    .into_iter()
    .filter_map(|r| {
        Some((
            r.try_get::<String, _>("proname").ok()?,
            r.try_get::<String, _>("args").ok()?,
            r.try_get::<String, _>("ret").ok()?,
        ))
    })
    .collect();

    let normalise = |s: &str| s.replace(' ', "").to_lowercase();

    for &(name, want_sig, want_ret) in SCHEMA_FUNCTIONS {
        let matches: Vec<&(String, String, String)> = present_functions
            .iter()
            .filter(|(n, _, _)| n == name)
            .collect();

        if matches.is_empty() {
            verdict.missing_functions.push((name, want_sig, want_ret));
            continue;
        }

        let want_sig_norm = normalise(want_sig);
        let want_ret_norm = normalise(want_ret);

        // Any overload with matching signature counts as "present".
        let sig_match = matches
            .iter()
            .any(|(_, sig, _)| normalise(sig) == want_sig_norm);
        if !sig_match {
            let found_sigs = matches
                .iter()
                .map(|(_, s, _)| s.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            verdict
                .function_sig_mismatches
                .push((name, want_sig, found_sigs));
            continue; // signature mismatch dominates; return-type check is meaningless
        }

        // Among matching-signature overloads, does the return type match?
        let ret_match = matches
            .iter()
            .filter(|(_, sig, _)| normalise(sig) == want_sig_norm)
            .any(|(_, _, ret)| normalise(ret) == want_ret_norm);
        if !ret_match {
            let found_rets = matches
                .iter()
                .filter(|(_, sig, _)| normalise(sig) == want_sig_norm)
                .map(|(_, _, r)| r.clone())
                .collect::<Vec<_>>()
                .join(" | ");
            verdict
                .function_return_mismatches
                .push((name, want_ret, found_rets));
        }
    }

    Ok(verdict)
}

// ═══════════════════════════════════════════════════════════════════
// Boot-time enforcement
// ═══════════════════════════════════════════════════════════════════

/// Decision the boot check hands back to `main()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootDecision {
    /// Schema is healthy — serve traffic.
    Healthy,
    /// Drift detected but running in default (non-strict) mode. Continue
    /// booting so the operator can diagnose from Railway logs.
    DriftContinueBoot,
    /// Drift detected and `SCHEMA_STRICT=1` — refuse to serve.
    DriftAbortBoot,
}

/// Emit the verdict to stderr with a loud banner and return the
/// decision `main()` should act on.
///
/// The banner is intentionally visually distinctive (`═` bars, ANSI
/// bold) so it survives being interleaved with the usual startup
/// noise in Railway logs. Every drift item gets its own line so the
/// operator can `grep '[schema_trust]'` and see the whole picture.
pub fn emit_boot_report(verdict: &SchemaVerdict, strict: bool) -> BootDecision {
    if verdict.is_healthy() {
        eprintln!(
            "[schema_trust] ✓ contract verified — {} tables, {} matviews, {} columns, {} functions all present",
            SCHEMA_TABLES.len(),
            SCHEMA_MATVIEWS.len(),
            SCHEMA_COLUMNS.len(),
            SCHEMA_FUNCTIONS.len()
        );
        return BootDecision::Healthy;
    }

    // Banner
    eprintln!();
    eprintln!("╔══════════════════════════════════════════════════════════════╗");
    eprintln!(
        "║ [schema_trust] DRIFT DETECTED — {} issue(s) against contract",
        verdict.total_issues()
    );
    eprintln!("╚══════════════════════════════════════════════════════════════╝");

    for t in &verdict.missing_tables {
        eprintln!("[schema_trust]   ✗ missing table:  public.{}", t);
    }
    for m in &verdict.missing_matviews {
        eprintln!("[schema_trust]   ✗ missing matview: public.{}", m);
    }
    for (name, want, found) in &verdict.relation_kind_mismatches {
        eprintln!(
            "[schema_trust]   ✗ relation kind drift: public.{} — want {}, found {}",
            name, want, found
        );
    }
    for (t, c) in &verdict.missing_columns {
        eprintln!("[schema_trust]   ✗ missing column: public.{}.{}", t, c);
    }
    for (name, sig, ret) in &verdict.missing_functions {
        eprintln!(
            "[schema_trust]   ✗ missing function: {}({}) -> {}",
            name, sig, ret
        );
    }
    for (name, want_sig, found) in &verdict.function_sig_mismatches {
        eprintln!(
            "[schema_trust]   ✗ signature drift: {}({}) — found: {}",
            name, want_sig, found
        );
    }
    for (name, want_ret, found) in &verdict.function_return_mismatches {
        eprintln!(
            "[schema_trust]   ✗ return-type drift: {} — want {}, found {}",
            name, want_ret, found
        );
    }
    eprintln!();

    if strict {
        eprintln!("[schema_trust] SCHEMA_STRICT=1 — refusing to serve traffic. Fix the drift and redeploy.");
        eprintln!();
        BootDecision::DriftAbortBoot
    } else {
        eprintln!("[schema_trust] SCHEMA_STRICT unset — continuing boot in warn-only mode.");
        eprintln!("[schema_trust] Set SCHEMA_STRICT=1 to refuse traffic on future drift.");
        eprintln!();
        BootDecision::DriftContinueBoot
    }
}

/// Convenience: verify against the DB and log the verdict in one call.
/// Returns the decision for `main()`. Never panics.
///
/// ## Probe failure is fail-closed under strict mode
///
/// v0.11.0 treated "the probe itself errored" as `DriftContinueBoot`
/// unconditionally, on the reasoning that we shouldn't refuse boot
/// because the *check* couldn't run. That reasoning is wrong under
/// `SCHEMA_STRICT=1`: a revoked `pg_catalog` grant, a connection
/// failure, or a statement timeout would silently disable the contract
/// while the operator believed it was being enforced. A guard that can
/// be turned off by an error is not a guard.
///
/// So: non-strict mode still continues (warn-only is warn-only), but
/// strict mode refuses to serve if it cannot *prove* the schema is
/// sound.
pub async fn verify_and_report(db: &PgPool) -> BootDecision {
    let strict = std::env::var("SCHEMA_STRICT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    match verify(db).await {
        Ok(verdict) => emit_boot_report(&verdict, strict),
        Err(e) => {
            eprintln!("[schema_trust] ⚠ probe itself failed: {}", e);
            if strict {
                eprintln!(
                    "[schema_trust] SCHEMA_STRICT=1 and the contract could not be verified — \
                     refusing to serve traffic rather than assuming the schema is sound."
                );
                BootDecision::DriftAbortBoot
            } else {
                eprintln!(
                    "[schema_trust] continuing boot without contract check (warn-only mode)."
                );
                BootDecision::DriftContinueBoot
            }
        }
    }
}
