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
    "anomaly_events",
    "agent_observability_state",
    "hitl_actions",
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

/// `pg_class.relkind` values acceptable for a [`SCHEMA_TABLES`] entry:
/// ordinary table or partitioned table.
pub const TABLE_KINDS: &[&str] = &["r", "p"];

/// `pg_class.relkind` values acceptable for a [`SCHEMA_MATVIEWS`] entry.
pub const MATVIEW_KINDS: &[&str] = &["m"];

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
    ("agents", "total_executions"),
    ("agents", "description"),
    ("agents", "system_prompt"),
    ("agents", "tags"),
    ("agents", "fork_pricing"),
    ("agents", "forked_from"),
    ("agents", "fork_count"),
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
            && self.relation_kind_mismatches.is_empty()
            && self.missing_columns.is_empty()
            && self.missing_functions.is_empty()
            && self.function_sig_mismatches.is_empty()
            && self.function_return_mismatches.is_empty()
    }

    pub fn total_issues(&self) -> usize {
        self.missing_tables.len()
            + self.missing_matviews.len()
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
            "columns":    columns,
            "functions":  functions,
            "summary": {
                "tables":    { "total": SCHEMA_TABLES.len(),    "missing": self.missing_tables.len() },
                "matviews":  { "total": SCHEMA_MATVIEWS.len(),  "missing": self.missing_matviews.len() },
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

    for (contract, want_kinds, want_label, is_matview) in [
        (SCHEMA_TABLES, TABLE_KINDS, "table", false),
        (SCHEMA_MATVIEWS, MATVIEW_KINDS, "materialized view", true),
    ] {
        for &name in contract {
            let found = found_kinds(name);

            if found.is_empty() {
                if is_matview {
                    verdict.missing_matviews.push(name);
                } else {
                    verdict.missing_tables.push(name);
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
