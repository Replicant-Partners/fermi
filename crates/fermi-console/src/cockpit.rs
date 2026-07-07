//! FPL Composer — Visual editor driven by the FPL Program tree.
//!
//! The FPL program IS the forecast. The Composer renders each node of the
//! program tree as a visual element. The FPL Assistant provides contextual
//! guidance at each node — it doesn't have an open chat, the program
//! structure itself guides the workflow:
//!
//!   - Question has no base_rate? → Assistant researches reference class
//!   - Driver has <SPECIFY> placeholders? → Assistant helps estimate
//!   - Driver has no agents? → Assistant suggests monitoring
//!   - Distribution looks wrong? → Assistant warns + suggests
//!   - All drivers populated? → Validation pipeline runs
//!   - Validation passes? → Simulation runs
//!
//! Agents are INSIDE drivers (the `agents: [...]` block in FPL).
//! Evidence is INSIDE drivers (the `evidence: [...]` block in FPL).
//! The program is always viewable as FPL text (Ctrl+E).

use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use tokio::sync::mpsc;

// ─── Cockpit-scoped actions ────────────────────────────────────────────────────

gpui::actions!(fermi_console, [NavigateDriverUp, NavigateDriverDown]);

use fermi::agent_backend::{
    llm_executor::LLMExecutor, registry::AgentRegistry, AgentOutput, ExecutionContext,
};
use fermi::ast::{
    AgentStmt, BaseRate, Distribution, DriverStmt, DriverType, EvidenceStmt, Expression,
    GeneratedBy, ModelStmt, Program, QuestionStmt, Schedule, SimulateStmt, Statement,
};

use crate::api::client::{
    AgentExecutionResult, ApiClient, CreateForecastRequest, ForecastSchedule, Invite,
    InviteRequest, ShareEntry, ShareRequest, Team, UpsertScheduleRequest,
};
use crate::text_input::TextInput;
use crate::theme;

// ═══════════════════════════════════════════════════════════════════
// Cockpit State — the FPL Program is the source of truth
// ═══════════════════════════════════════════════════════════════════

/// Which node of the FPL tree the user is currently focused on.
#[derive(Debug, Clone, PartialEq)]
pub enum FocusedNode {
    /// Top-level question — entering/editing the forecast question
    Question,
    /// A specific driver by name — editing its parameters
    Driver(String),
    /// Picking an agent to assign to a driver
    AgentPicker(String),
    /// The model expression
    Model,
    /// Simulation config / results
    Simulation,
    /// Viewing the raw FPL source
    FplSource,
}

/// Which tab is active in the right panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RightTab {
    Edit,
    Fpl,
    Wiki,
    Schedules,
    /// Spec 23 R-3: chronological event view of the forecast — rate +
    /// market traces, plus dots for every BayesOps fit, agent run,
    /// upstream resolution, and market poll. The "spacetime" view from
    /// the spec, scoped to this forecast.
    Trajectory,
    /// Spec 24 §3.5.2: who can see/edit this forecast. Lists object_shares
    /// rows and lets the owner add (by user/email) or revoke collaborators.
    Access,
}

/// A live event from an SSE agent execution stream.
/// Sent from the tokio background task to the GPUI render loop.
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Agent execution has started
    Started { agent_id: String },
    /// A key finding was extracted during execution
    Finding { agent_id: String, text: String },
    /// Agent execution completed — full result attached
    Complete { agent_id: String, result: JsonValue },
    /// Agent execution failed
    Failed { agent_id: String, error: String },
}

/// Status of an agent execution within a driver context.
#[derive(Debug, Clone)]
pub struct AgentExecution {
    pub agent_name: String,
    pub status: AgentRunStatus,
    pub evidence_count: usize,
    pub confidence: Option<f64>,
    pub error: Option<String>,
    pub credits_charged: Option<f64>,
    /// When the agent started executing (epoch seconds).
    pub started_at: Option<u64>,
    /// When the agent finished executing (epoch seconds).
    pub completed_at: Option<u64>,
    /// The most recent finding snippet from this agent (truncated).
    pub latest_finding: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentRunStatus {
    Idle,
    Running,
    Completed,
    Failed,
}

/// A pending parameter adjustment suggested by an agent.
/// The user can accept (applies the change) or reject (discards it).
#[derive(Debug, Clone)]
pub struct EvidenceSuggestion {
    pub id: String,
    pub driver_name: String,
    pub agent_name: String,
    pub suggested_p50: f64,
    pub current_p50: f64,
    pub reasoning: String,
    pub evidence_id: String,
}

/// A message from the FPL Assistant — contextual guidance tied to a program node.
#[derive(Debug, Clone)]
pub struct AssistantMessage {
    pub node: String, // which FPL node this relates to (e.g. "question", "driver:market_tam")
    pub kind: MessageKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageKind {
    Suggestion, // "Consider adding a regulatory monitor agent"
    Warning,    // "p5 > p50 — your distribution is backwards"
    Info,       // "Base rate anchored to 35%"
    Error,      // "Model references undefined driver 'price'"
    Tip,        // "🦊 Fermi tip: Set weekly monitoring for fast-moving processes"
}

/// A version snapshot of the forecast.
#[derive(Debug, Clone)]
pub struct ForecastVersion {
    pub version: u32,
    pub timestamp: String,
    pub fpl_text: String,
    pub probability: f64,
    pub change_summary: String,
}

/// A schedule the FPL declares but the operator hasn't yet pushed to the
/// server. Pre-populates the Schedules tab so the operator can review +
/// batch-save the agent×driver fan-out instead of clicking through each
/// driver one by one to attach a cadence.
#[derive(Debug, Clone)]
pub struct ScheduleDraft {
    pub agent_id: String,
    pub driver_name: String,
    pub query: String,
    pub interval_hours: i32,
}

pub struct CockpitState {
    // ── The FPL Program (source of truth) ─────────────────────────
    pub program: Program,

    // ── UI State ──────────────────────────────────────────────────
    pub focused_node: FocusedNode,
    pub right_tab: RightTab,
    pub predicted_probability: f64,

    // ── Text Inputs ───────────────────────────────────────────────
    pub question_input: Entity<TextInput>,
    // Driver editor fields (shared, populated from focused driver)
    pub editor_name: Entity<TextInput>,
    pub editor_p5: Entity<TextInput>,
    pub editor_p50: Entity<TextInput>,
    pub editor_p95: Entity<TextInput>,
    pub editor_unit: Entity<TextInput>,
    pub editor_prob: Entity<TextInput>,
    pub editor_impact: Entity<TextInput>,
    pub editor_rationale: Entity<TextInput>,
    pub agent_query_input: Entity<TextInput>,
    pub editor_confidence: Entity<TextInput>,
    pub driver_research_input: Entity<TextInput>,
    pub evidence_source_input: Entity<TextInput>,
    pub evidence_summary_input: Entity<TextInput>,

    // ── Evidence Affordances ──────────────────────────────────────
    /// Pending p50 adjustment suggestions from agents, awaiting user accept/reject.
    pub pending_suggestions: Vec<EvidenceSuggestion>,
    /// Evidence IDs that are collapsed in the UI (default: all collapsed).
    pub collapsed_evidence: HashSet<String>,

    // ── Polymarket Link (live crowd price for linked forecasts) ───
    /// Polymarket event ID linked to this forecast (if any).
    pub pm_event_id: Option<String>,
    /// Polymarket market ID linked to this forecast.
    pub pm_market_id: Option<String>,
    /// The PM question text (may differ from Fermi question).
    pub pm_question: Option<String>,
    /// Latest crowd-implied probability from Polymarket (0.0–1.0).
    pub pm_market_price: Option<f64>,
    /// 24-hour trading volume (USD).
    pub pm_volume_24h: Option<f64>,
    /// Market liquidity depth (USD).
    pub pm_liquidity: Option<f64>,
    /// Confidence signal based on volume + spread.
    pub pm_confidence: Option<String>,
    /// 1-week price change (percentage points).
    pub pm_price_change_1w: Option<f64>,
    /// Polymarket URL for this event.
    pub pm_url: Option<String>,
    /// Whether PM data is currently being fetched.
    pub pm_loading: bool,

    // ── Agent Execution State (runtime, not in AST) ───────────────
    pub agent_runs: Vec<AgentExecution>,
    pub orchestration_running: bool,
    pub session_cost: f64,

    // ── Assistant Messages ─────────────────────────────────────────
    pub messages: Vec<AssistantMessage>,

    // ── Agent Picker State ─────────────────────────────────────────
    pub agent_search_query: String,

    // ── Simulation Results (runtime, not in AST) ──────────────────
    pub sim_results: Option<SimResults>,
    pub sim_running: bool,
    pub sim_error: Option<String>,

    // ── Versioning ────────────────────────────────────────────────
    pub versions: Vec<ForecastVersion>,
    pub current_version: u32,

    // ── Meta ──────────────────────────────────────────────────────
    pub forecast_id: Option<String>,
    pub publish_status: Option<String>,
    pub api: Arc<ApiClient>,

    // ── Server lifecycle reconciliation ───────────────────────────
    // The authoritative status of this forecast, pulled from the server
    // (not inferred by the user). When it's `resolved`/`void` the cockpit
    // locks: no re-sims, no new snapshots. This is the source of truth that
    // frees the operator from having to remember "this team is eliminated".
    pub forecast_status: Option<String>,
    pub forecast_outcome: Option<bool>,
    pub resolution_note: Option<String>,
    pub reconciling: bool,

    /// Per-driver Sobol total-order index (0..1) from the last simulation's
    /// `full_sensitivity_analysis`. The sensitivity bars render from THIS
    /// (true variance-based influence) when present, instead of the driver's
    /// raw p95−p5 spread (which is ~uniform for a factor model).
    pub driver_sensitivity: std::collections::HashMap<String, f64>,

    /// True while a just-run sim's raw mean is being recomposed server-side
    /// (mutex-group eliminations priced back in). The displayed value is
    /// provisional until this clears; saving is blocked so a save can never
    /// disagree with the value the sim settles on.
    pub recomposing: bool,

    // ── Access / sharing (Spec 24 §3.5.2) ─────────────────────
    /// Current object_shares rows for this forecast (Access tab).
    pub shares: Vec<ShareEntry>,
    pub shares_loading: bool,
    /// Target input for "add collaborator" (email or user_id).
    pub share_input: Entity<TextInput>,
    /// Permission to grant on add: view | edit | admin (cycle chip).
    pub share_permission: String,
    pub share_add_loading: bool,
    pub share_error: Option<String>,
    /// Loaded-shares marker so we only auto-fetch once per forecast_id.
    pub shares_loaded_for: Option<String>,
    /// Pending / recent invites for this forecast. Loaded alongside
    /// `shares` so the operator can see outbound invitations (Alice —
    /// pending) sitting alongside materialised shares. Powers the
    /// "Sent invites" section of the Access tab.
    pub forecast_invites: Vec<Invite>,
    pub forecast_invites_loading: bool,
    /// Invite IDs with a revoke in flight (disables the button).
    pub forecast_invite_revoke_in_flight: HashSet<String>,
    /// Collaboration teams the operator belongs to — candidate share
    /// targets for the Access tab's "Share with a team" section.
    /// Filtered client-side to the fermi_forecast vertical and further
    /// to exclude auto-created workspace-prior teams. Loaded lazily on
    /// first Access-tab open per cockpit lifetime.
    pub share_teams: Vec<Team>,
    pub share_teams_loading: bool,
    /// Team IDs currently being shared with (button disabled until
    /// the API call returns) so double-clicks don't create dupes.
    pub share_team_in_flight: HashSet<String>,
    /// Share targets collected in the commit sheet (target, permission),
    /// applied right after the forecast row is created/updated on publish.
    pub pending_publish_shares: Vec<(String, String)>,
    pub registry: Arc<AgentRegistry>,
    pub cached_fpl: String,
    pub inside_view_explanation: String,
    pub forecast_confidence: f64, // 0.0-1.0 overall confidence in the inside view
    /// User-set confidence per driver (driver_name → 0.0-1.0). Overrides computed confidence.
    pub driver_confidence: HashMap<String, f64>,
    /// Which version is selected for viewing/diff (None = current)
    pub selected_version: Option<u32>,
    /// Receiver for live SSE events from background agent tasks.
    /// Drained on every render frame for progressive UI updates.
    pub sse_rx: std_mpsc::Receiver<SseEvent>,
    /// Sender cloned into each fire_agent background task.
    pub sse_tx: std_mpsc::Sender<SseEvent>,

    // ── Agent Schedules (persisted via API) ───────────────────────
    pub schedules: Vec<ForecastSchedule>,
    pub schedules_loading: bool,

    /// Agent completion notifications queued for the parent (FermiConsole) to
    /// drain and display as toasts. Each entry is an agent display name.
    pub pending_toasts: Vec<String>,

    // ── ABW Workspace Integration ─────────────────────────────────
    /// Workspace ID (UUID) backing this forecast in ABW.
    /// Spawned from the `fermi_forecast` app on first orchestration.
    pub workspace_id: Option<String>,

    /// The workspace's `params` output as a flat key→Value map. Loaded
    /// on workspace mount and passed into the Executor via set_params /
    /// set_json_params before each simulation. Carries:
    ///   - per-team scalar bindings (elo_current, gdp_per_capita_log, …)
    ///     written by the spawn script
    ///   - `<driver>_fitted` JSON written by the BayesOps refit-accept
    ///     handler so the next sim picks up the fitted posterior
    ///
    /// Without this, Ctrl+R uses an empty param context and the WC
    /// team_prior's distribution expressions fail with "Undefined
    /// variable: gdp_per_capita_log" (silently 0.0-substituted today,
    /// which is why every team gets nearly the same rate).
    pub workspace_params: serde_json::Map<String, serde_json::Value>,

    // ── BayesOps R-2: Sparkline UX ─────────────────────────────────
    /// Per-driver pending-fit state, keyed by driver name. Populated by
    /// `load_bayesops_state()` from `/api/workspaces/:id/bayesops/state`.
    /// The render_driver_card logic checks this first; if a driver has a
    /// pending fit, the badge renders the `PendingReview` state with
    /// inline accept/dismiss buttons, taking precedence over the
    /// post-sim `LearnableBadgeStatus::Fitted` / `PriorFallback`.
    ///
    /// Loaded on workspace mount, after every refit, and on
    /// `bayesops_fit_pending` workspace event.
    pub bayesops_pending: std::collections::HashMap<String, PendingFitState>,

    /// Pending in-flight accept/reject calls per driver name — prevents
    /// double-click double-submit. Cleared on response.
    pub bayesops_decisions_in_flight: std::collections::HashSet<String>,

    // ── R-3 Trajectory view ────────────────────────────────────────
    /// Cached response from GET /api/forecasts/:id/timeline. None when
    /// the tab hasn't been opened yet or the load failed. Refreshed when
    /// the user clicks the Trajectory tab.
    pub timeline_data: Option<JsonValue>,
    /// True while a timeline fetch is in flight.
    pub timeline_loading: bool,
    /// Error message from the last timeline fetch, if any.
    pub timeline_error: Option<String>,

    // ── Polymarket Price History ──────────────────────────────────
    /// Time-series of crowd prices, sampled at `pm_poll_interval`.
    /// Each entry is (timestamp_epoch_secs, price 0.0–1.0).
    pub pm_price_history: Vec<(u64, f64)>,
    /// Index of the histogram bin currently under the cursor in the Wiki
    /// tab's interactive distribution. `None` when the cursor is outside
    /// the histogram. Drives the tooltip that surfaces the outcome value,
    /// bin count, CDF percentile, and distance-to-each-anchor at the
    /// hovered position. Reset to `None` on forecast switch via
    /// load_forecast.
    pub hovered_histogram_bin: Option<usize>,
    /// Index of the version currently under the cursor in the Wiki tab's
    /// interactive index chart (inside/outside/crowd over time). `None`
    /// when the cursor is outside the chart. Drives the crosshair +
    /// tooltip showing the three anchor values and pairwise deltas at
    /// that point in history. Reset on forecast switch.
    pub hovered_index_version: Option<usize>,
    /// Index of the trajectory event the operator is hovering over
    /// (in the chart). Drives both the chart's highlighted dot and the
    /// matching event row in the list below — eye-trace correlation
    /// between the worm chart and the bullet list. Reset on forecast
    /// switch.
    pub hovered_trajectory_event: Option<usize>,
    /// Polling interval for PM price updates. None = no polling.
    pub pm_poll_interval: Option<std::time::Duration>,

    // ── PM refresh telemetry ──────────────────────────────────
    //
    // Written by refresh_pm_price_now() and the polling loop so the
    // Polymarket panel can render "updated 3 s ago" / "refreshing…" /
    // "failed: Bad Gateway" — without this the operator has no way to
    // tell whether the crowd number they see is live, stale-and-being-
    // refreshed, or silently failing every poll.
    /// Unix epoch seconds when the last successful PM snapshot landed.
    pub pm_last_refresh_at: Option<u64>,
    /// Error message from the most recent failed snapshot attempt.
    /// Cleared on next success.
    pub pm_last_refresh_error: Option<String>,
    /// True while a snapshot request is in flight (from either the
    /// interval poll or the manual ↻ Refresh now button).
    pub pm_refresh_in_flight: bool,
}

#[derive(Debug, Clone)]
pub struct SimResults {
    pub mean: f64,
    pub median: f64,
    pub p5: f64,
    pub p95: f64,
    pub std_dev: f64,
    pub iterations: u64,
    pub execution_time_ms: u64,
    pub histogram: Vec<u32>,
    /// Starting outcome value for each bin in `histogram`. Same length as
    /// `histogram`. Lets the interactive histogram (Wiki tab) show the
    /// outcome range under the cursor without having to re-execute the
    /// simulation. Empty for loaded forecasts whose state.json predates
    /// this field — those keep the static rendering.
    pub bin_starts: Vec<f64>,
    pub bin_width: f64,
    /// Per-driver record of how each `learnable: true` driver's distribution
    /// was resolved this run. Drives the status badge on each driver card.
    /// Empty for forecasts with no learnable drivers (the common case until
    /// BayesOps wiring is live for a given driver). See
    /// `fermi::executor::LearnableDriverResolution` for the upstream shape;
    /// we keep a compact mirror here so SimResults stays cheap to serialize
    /// without dragging the posterior crate into the console.
    pub learnable_drivers: Vec<LearnableDriverBadge>,
}

/// Compact mirror of `fermi::executor::LearnableSource` for the UI layer.
/// Captures just enough to drive the status badge: which driver, whether
/// the run used a fit, and (when fitted) the effective observation count
/// + CI width so the user can see "this prior is now tight because data
/// supports it".
#[derive(Debug, Clone, PartialEq)]
pub struct LearnableDriverBadge {
    pub driver_name: String,
    pub status: LearnableBadgeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LearnableBadgeStatus {
    /// Marked learnable but no fit was available this run — used the static
    /// prior. UX: cold-start badge.
    PriorFallback,
    /// A `FittedDistribution` was found and used. UX: green badge with
    /// observation count.
    Fitted {
        family: String,
        n_eff: f64,
        ci_width: f64,
    },
    /// Spec 23 R-2: the refit hook produced a fit whose impact exceeded
    /// the auto-accept threshold, so it's staged for the forecaster's
    /// decision. The badge renders the impact delta and inline ✓/✗
    /// buttons. Accept writes `params.<driver>_fitted` server-side and
    /// transitions the badge to `Fitted`. Reject drops the staged fit
    /// and the badge returns to its prior state.
    PendingReview {
        pending_id: String,
        n_eff: f64,
        ci_width: f64,
        /// |rate_after - rate_before| × 100 — what the operator sees on
        /// the badge ("+6pp"). `None` when impact couldn't be computed.
        delta_pp: Option<f64>,
        n_observations: i32,
    },
}

/// Snapshot of a server-side pending fit, fetched from
/// `/api/workspaces/:id/bayesops/state`. Lives on `CockpitState` keyed by
/// driver name (not inside SimResults, which is a per-run artefact —
/// pending fits persist across local sim re-runs).
#[derive(Debug, Clone, PartialEq)]
pub struct PendingFitState {
    pub driver_name: String,
    pub pending_id: String,
    pub n_observations: i32,
    pub n_eff: f64,
    pub ci_width: f64,
    pub delta_pp: Option<f64>,
}

// ═══════════════════════════════════════════════════════════════════
// Construction
// ═══════════════════════════════════════════════════════════════════

impl CockpitState {
    pub fn new(api: Arc<ApiClient>, registry: Arc<AgentRegistry>, cx: &mut Context<Self>) -> Self {
        let question_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("What are you forecasting?")
                .with_large(true)
        });
        let editor_name = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("driver_name")
                .with_label("Name")
        });
        let editor_p5 = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("pessimistic")
                .with_label("p5 (low)")
        });
        let editor_p50 = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("likely")
                .with_label("p50 (mid)")
        });
        let editor_p95 = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("optimistic")
                .with_label("p95 (high)")
        });
        let editor_unit = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("USD, %, ratio…")
                .with_label("Unit")
        });
        let editor_prob = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("0.0–1.0")
                .with_label("Probability")
        });
        let editor_impact = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("e.g. 1.3")
                .with_label("Impact ×")
        });
        let editor_rationale = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Why does this driver matter?")
                .with_label("Rationale")
        });
        let agent_query_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("What should this agent research for this driver?")
                .with_label("Agent Query")
        });
        let editor_confidence = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("0–100")
                .with_label("Confidence %")
        });
        let driver_research_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("What do you want to know about this driver? e.g. 'How deep is Bayern's squad compared to other CL contenders?'")
                .with_label("Research Question")
        });

        let evidence_source_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Source (e.g. Bloomberg, analyst report, URL)")
                .with_label("Source")
        });
        let evidence_summary_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("What does this evidence say?")
                .with_label("Summary")
        });
        let share_input = cx.new(|cx| TextInput::new(cx).with_placeholder("email or user id…"));
        let (tx, rx) = std_mpsc::channel::<SseEvent>();
        let s = Self {
            program: Program::empty(),
            focused_node: FocusedNode::Question,
            right_tab: RightTab::Edit,
            predicted_probability: 0.5,
            question_input,
            editor_name,
            editor_p5,
            editor_p50,
            editor_p95,
            editor_unit,
            editor_prob,
            editor_impact,
            editor_rationale,
            agent_query_input,
            editor_confidence,
            driver_research_input,
            evidence_source_input,
            evidence_summary_input,
            pending_suggestions: Vec::new(),
            collapsed_evidence: HashSet::new(),
            pm_event_id: None,
            pm_market_id: None,
            pm_question: None,
            pm_market_price: None,
            pm_volume_24h: None,
            pm_liquidity: None,
            pm_confidence: None,
            pm_price_change_1w: None,
            pm_url: None,
            pm_loading: false,
            agent_runs: Vec::new(),
            orchestration_running: false,
            session_cost: 0.0,
            messages: vec![AssistantMessage {
                node: "question".into(),
                kind: MessageKind::Suggestion,
                text: "Type a forecast question to begin. The FPL Assistant will analyze it and suggest a Fermi decomposition.".into(),
            }],
            agent_search_query: String::new(),
            sim_results: None,
            sim_running: false,
            sim_error: None,
            versions: Vec::new(),
            current_version: 0,
            forecast_id: None,
            publish_status: None,
            api,
            forecast_status: None,
            forecast_outcome: None,
            resolution_note: None,
            reconciling: false,
            driver_sensitivity: std::collections::HashMap::new(),
            recomposing: false,
            shares: Vec::new(),
            shares_loading: false,
            share_input,
            share_permission: "view".into(),
            share_add_loading: false,
            share_error: None,
            shares_loaded_for: None,
            forecast_invites: Vec::new(),
            forecast_invites_loading: false,
            forecast_invite_revoke_in_flight: HashSet::new(),
            share_teams: Vec::new(),
            share_teams_loading: false,
            share_team_in_flight: HashSet::new(),
            pending_publish_shares: Vec::new(),
            registry,
            cached_fpl: String::new(),
            inside_view_explanation: String::new(),
            forecast_confidence: 0.5,
            driver_confidence: HashMap::new(),
            selected_version: None,
            sse_rx: rx,
            sse_tx: tx,
            schedules: Vec::new(),
            schedules_loading: false,
            pending_toasts: Vec::new(),
            workspace_id: None,
            workspace_params: serde_json::Map::new(),
            bayesops_pending: std::collections::HashMap::new(),
            bayesops_decisions_in_flight: std::collections::HashSet::new(),
            timeline_data: None,
            timeline_loading: false,
            timeline_error: None,
            pm_price_history: Vec::new(),
            pm_poll_interval: None,
            pm_last_refresh_at: None,
            pm_last_refresh_error: None,
            pm_refresh_in_flight: false,
            hovered_histogram_bin: None,
            hovered_index_version: None,
            hovered_trajectory_event: None,
        };

        // Start SSE polling timer — periodically triggers re-renders
        // so drain_sse_events() picks up live findings from background agents.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let should_poll = this
                .update(cx, |state, _cx| {
                    state.orchestration_running
                        || state
                            .agent_runs
                            .iter()
                            .any(|r| r.status == AgentRunStatus::Running)
                })
                .unwrap_or(false);
            if should_poll {
                this.update(cx, |_state, cx| {
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();

        // Start Polymarket price polling timer — fetches crowd price
        // at the configured interval when a PM market is linked.
        //
        // The FIRST tick fires after only 1 s (not the full poll
        // interval) so a freshly-loaded forecast doesn't sit with a
        // stale `last_market_price` from the DB for 5 minutes before
        // the operator sees the current crowd number. Subsequent ticks
        // use the configured interval.
        cx.spawn(async move |this, cx| {
            let mut first_tick = true;
            loop {
                let interval = this
                    .update(cx, |state, _cx| state.pm_poll_interval)
                    .ok()
                    .flatten();

                let sleep_ms = if first_tick {
                    // Give the load path a moment to hydrate
                    // pm_event_id/pm_market_id, then fire once.
                    1_000
                } else {
                    interval.map(|d| d.as_millis() as u64).unwrap_or(30_000)
                };
                first_tick = false;
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(sleep_ms))
                    .await;

                // Only poll if we have a linked market and polling is enabled
                let poll_data = this
                    .update(cx, |state, _cx| {
                        if state.pm_poll_interval.is_some() {
                            if let (Some(ref eid), Some(ref mid)) =
                                (state.pm_event_id.clone(), state.pm_market_id.clone())
                            {
                                return Some((
                                    state.api.clone(),
                                    eid.clone(),
                                    mid.clone(),
                                    state.forecast_id.clone(),
                                ));
                            }
                        }
                        None
                    })
                    .ok()
                    .flatten();

                if let Some((api, eid, mid, forecast_id)) = poll_data {
                    let fid = forecast_id.unwrap_or_default();
                    // Signal "refresh in flight" so the UI status chip
                    // shows the same "refreshing…" state as a manual
                    // click. Skip if a manual refresh is already
                    // running to avoid clobbering its outcome.
                    let should_fire = this
                        .update(cx, |state, _cx| {
                            if state.pm_refresh_in_flight {
                                false
                            } else {
                                state.pm_refresh_in_flight = true;
                                true
                            }
                        })
                        .unwrap_or(false);
                    if !should_fire {
                        continue;
                    }
                    let result =
                        tokio::spawn(async move { api.pm_snapshot(&fid, &eid, &mid).await }).await;

                    this.update(cx, |state, cx| {
                        state.pm_refresh_in_flight = false;
                        match &result {
                            Ok(Ok(resp)) => {
                                let price = resp
                                    .get("market_price")
                                    .and_then(|v| v.as_f64())
                                    .or_else(|| {
                                        resp.get("snapshot")
                                            .and_then(|s| s.get("market_price"))
                                            .and_then(|v| v.as_f64())
                                    });
                                let vol_24h = resp.get("volume_24h").and_then(|v| v.as_f64());
                                let liquidity = resp.get("liquidity").and_then(|v| v.as_f64());
                                let price_change_1w =
                                    resp.get("price_change_1w").and_then(|v| v.as_f64());
                                let confidence = resp
                                    .get("confidence_signal")
                                    .and_then(|v| v.as_str())
                                    .map(String::from);
                                if let Some(p) = price {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_secs();
                                    // Only overwrite the history point if
                                    // the value actually changed enough to
                                    // matter (>0.05pp) OR if we haven't
                                    // recorded any history yet — avoids
                                    // spamming the sparkline when the
                                    // market is quiet. The refresh
                                    // timestamp is always updated so the
                                    // UI can show "last checked: now".
                                    let changed = state
                                        .pm_market_price
                                        .map(|prev| (prev - p).abs() > 0.0005)
                                        .unwrap_or(true);
                                    state.pm_market_price = Some(p);
                                    if vol_24h.is_some() {
                                        state.pm_volume_24h = vol_24h;
                                    }
                                    if liquidity.is_some() {
                                        state.pm_liquidity = liquidity;
                                    }
                                    if price_change_1w.is_some() {
                                        state.pm_price_change_1w = price_change_1w;
                                    }
                                    if confidence.is_some() {
                                        state.pm_confidence = confidence;
                                    }
                                    if changed || state.pm_price_history.is_empty() {
                                        state.pm_price_history.push((now, p));
                                        if state.pm_price_history.len() > 500 {
                                            state.pm_price_history.remove(0);
                                        }
                                    }
                                    state.pm_last_refresh_at = Some(now);
                                    state.pm_last_refresh_error = None;
                                    log::info!(
                                        "[pm-poll] Price update: {:.2}% ({} history points)",
                                        p * 100.0,
                                        state.pm_price_history.len()
                                    );
                                } else {
                                    let msg = "snapshot response missing market_price".to_string();
                                    log::warn!("[pm-poll] {}: {}", msg, resp);
                                    state.pm_last_refresh_error = Some(msg);
                                }
                            }
                            Ok(Err(e)) => {
                                let msg = e.to_string();
                                log::warn!("[pm-poll] Snapshot API error: {}", msg);
                                state.pm_last_refresh_error = Some(msg);
                            }
                            Err(e) => {
                                let msg = format!("task join error: {}", e);
                                log::warn!("[pm-poll] {}", msg);
                                state.pm_last_refresh_error = Some(msg);
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();

        s
    }

    /// Set the PM polling interval. Call after linking a Polymarket market.
    pub fn set_pm_poll_interval(&mut self, interval: std::time::Duration, cx: &mut Context<Self>) {
        self.pm_poll_interval = Some(interval);
        // Seed initial price into history if we have one
        if let Some(price) = self.pm_market_price {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if self.pm_price_history.is_empty() {
                self.pm_price_history.push((now, price));
            }
        }
        cx.notify();
    }

    /// Immediately fetch a fresh Polymarket snapshot for the linked
    /// market and update the cockpit's crowd fields. Called from the
    /// forecast load path so the operator doesn't see the stale
    /// `last_market_price` cached in `metadata.polymarket` for the full
    /// poll interval (5 min) before the first background tick fires.
    ///
    /// Silent no-op if there's no linked market on the current
    /// forecast, or if `pm_snapshot` errors (background poll will retry
    /// on its normal cadence).
    pub fn refresh_pm_price_now(&mut self, cx: &mut Context<Self>) {
        let (Some(eid), Some(mid)) = (self.pm_event_id.clone(), self.pm_market_id.clone()) else {
            return;
        };
        // Guard against double-clicks / overlapping polls firing the
        // manual refresh; the button/status chip reads this flag.
        if self.pm_refresh_in_flight {
            return;
        }
        self.pm_refresh_in_flight = true;
        self.pm_last_refresh_error = None;
        cx.notify();
        let api = self.api.clone();
        let fid = self.forecast_id.clone().unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let result = tokio::spawn(async move { api.pm_snapshot(&fid, &eid, &mid).await }).await;
            this.update(cx, |state, cx| {
                state.pm_refresh_in_flight = false;
                match result {
                    Ok(Ok(resp)) => {
                        let price =
                            resp.get("market_price")
                                .and_then(|v| v.as_f64())
                                .or_else(|| {
                                    resp.get("snapshot")
                                        .and_then(|s| s.get("market_price"))
                                        .and_then(|v| v.as_f64())
                                });
                        let vol_24h = resp.get("volume_24h").and_then(|v| v.as_f64());
                        let liquidity = resp.get("liquidity").and_then(|v| v.as_f64());
                        let price_change_1w = resp.get("price_change_1w").and_then(|v| v.as_f64());
                        let confidence = resp
                            .get("confidence_signal")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                        if let Some(p) = price {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let prev = state.pm_market_price;
                            state.pm_market_price = Some(p);
                            if vol_24h.is_some() {
                                state.pm_volume_24h = vol_24h;
                            }
                            if liquidity.is_some() {
                                state.pm_liquidity = liquidity;
                            }
                            if price_change_1w.is_some() {
                                state.pm_price_change_1w = price_change_1w;
                            }
                            if confidence.is_some() {
                                state.pm_confidence = confidence;
                            }
                            state.pm_price_history.push((now, p));
                            if state.pm_price_history.len() > 500 {
                                state.pm_price_history.remove(0);
                            }
                            state.pm_last_refresh_at = Some(now);
                            state.pm_last_refresh_error = None;
                            log::info!(
                                "[pm-refresh] Snapshot: {:.2}% (was {:.2}%)",
                                p * 100.0,
                                prev.map(|q| q * 100.0).unwrap_or(0.0)
                            );
                        } else {
                            let msg = "snapshot response missing market_price".to_string();
                            log::warn!("[pm-refresh] {}: {}", msg, resp);
                            state.pm_last_refresh_error = Some(msg);
                        }
                    }
                    Ok(Err(e)) => {
                        let msg = e.to_string();
                        log::warn!("[pm-refresh] Snapshot API error: {}", msg);
                        state.pm_last_refresh_error = Some(msg);
                    }
                    Err(e) => {
                        let msg = format!("task join error: {}", e);
                        log::warn!("[pm-refresh] {}", msg);
                        state.pm_last_refresh_error = Some(msg);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Stop PM polling.
    pub fn stop_pm_poll(&mut self, cx: &mut Context<Self>) {
        self.pm_poll_interval = None;
        cx.notify();
    }

    /// Drain pending SSE events from background agent tasks.
    /// Called at the start of every render frame for live updates.
    /// Each event updates the UI immediately — findings pop in one by one.
    fn drain_sse_events(&mut self) {
        let mut changed = false;
        while let Ok(event) = self.sse_rx.try_recv() {
            match event {
                SseEvent::Started { ref agent_id } => {
                    log::info!("[sse] {} started", agent_id);
                    // Don't spam the banner with multiple "started" messages
                    // when 5 agents fire in parallel
                    if !self.messages.iter().rev().take(3).any(|m| {
                        m.kind == MessageKind::Info && m.text.contains("started researching")
                    }) {
                        self.messages.push(AssistantMessage {
                            node: format!("agent:{}", agent_id),
                            kind: MessageKind::Info,
                            text: format!("⟳ {} started researching…", agent_id),
                        });
                    }
                    changed = true;
                }
                SseEvent::Finding {
                    ref agent_id,
                    ref text,
                } => {
                    // Skip raw JSON fragments — only show human-readable findings
                    let is_json_fragment = text.trim_start().starts_with('{')
                        || text.trim_start().starts_with('"')
                        || text.trim_start().starts_with('[')
                        || text.contains("\"p5\":")
                        || text.contains("\"type\":")
                        || text.contains("\"name\":")
                        || text.len() < 10;
                    if is_json_fragment {
                        continue;
                    }

                    log::info!(
                        "[sse] {} finding: {}",
                        agent_id,
                        text.chars().take(60).collect::<String>()
                    );
                    // Update the agent's latest_finding for the speech bubble
                    if let Some(run) = self.agent_runs.iter_mut().find(|r| {
                        r.agent_name == *agent_id
                            || base_agent_name(&r.agent_name) == agent_id.as_str()
                            || r.agent_name.starts_with(agent_id.as_str())
                    }) {
                        run.latest_finding = Some(text.chars().take(120).collect());
                    }
                    self.messages.push(AssistantMessage {
                        node: format!("agent:{}", agent_id),
                        kind: MessageKind::Tip,
                        text: format!("🔍 {}", text),
                    });
                    changed = true;
                }
                SseEvent::Complete { .. } | SseEvent::Failed { .. } => {
                    // Complete/Failed are handled in fire_agent's result processing
                    // (they carry the full result JSON which needs the routing logic)
                }
            }
        }
        let _ = changed; // cx.notify() called by render
    }

    // ═══════════════════════════════════════════════════════════════
    // Phase 1: Question → Assistant generates decomposition template
    // ═══════════════════════════════════════════════════════════════

    /// Called when user submits a question (Ctrl+Enter).
    /// The Assistant analyzes the question and generates an FPL template
    /// with driver scaffolding. Then fires agents to research.
    pub fn orchestrate_question(&mut self, question: &str, cx: &mut Context<Self>) {
        let question = question.trim().to_string();
        if question.is_empty() {
            return;
        }

        self.orchestration_running = true;
        self.messages.clear();
        self.agent_runs.clear();
        self.sim_results = None;
        self.sim_error = None;
        self.session_cost = 0.0;

        // ── Spawn ABW workspace for this forecast ─────────────────
        // Each forecast gets a workspace from the fermi_forecast app.
        // This gives us: workspace messages (agent audit trail),
        // action log (OODA step tracking), coherence evaluation (Loop 3),
        // and dashboard visibility.
        if self.workspace_id.is_none() {
            let api = self.api.clone();
            let q = question.clone();
            cx.spawn(async move |this, cx| {
                let ws_name = format!(
                    "Fermi — {}",
                    q.chars().take(60).collect::<String>()
                );
                let desc = Some(q.as_str());
                match api.spawn_forecast_workspace(&ws_name, desc).await {
                    Ok(resp) => {
                        let ws_id = resp
                            .get("workspace_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        if let Some(ref id) = ws_id {
                            log::info!("[workspace] Spawned forecast workspace: {}", id);
                        }
                        this.update(cx, |state, cx| {
                            state.workspace_id = ws_id;
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::warn!("[workspace] Failed to spawn workspace: {} — forecast will operate without workspace backing", e);
                    }
                }
            })
            .detach();
        }

        // ── Build the FPL program template ────────────────────────
        // The Assistant analyzes the domain and generates a decomposition.
        // For now this is heuristic; with an LLM co-pilot it would be
        // a real analysis. The key point: the PROGRAM STRUCTURE guides
        // what happens next.
        let domain = detect_domain(&question);

        self.program = Program::empty();

        // Question node
        self.program.set_question(QuestionStmt {
            text: question.clone(),
            base_rate: None, // Agent will fill this
            target_date: None,
            resolution_criteria: None,
        });

        // Generate domain-appropriate driver decomposition
        // Each driver has agents embedded — this is the FPL way
        let (drivers, model_expr) = generate_decomposition(&question, &domain);
        for driver in &drivers {
            self.program.add_driver(driver.clone());
        }

        // Model
        if let Some(expr) = model_expr {
            self.program.set_model(ModelStmt { expression: expr });
        }

        // Simulate
        self.program.set_simulate(SimulateStmt {
            iterations: 10_000,
            target: None,
        });

        // ── Assistant messages ─────────────────────────────────────
        self.messages.push(AssistantMessage {
            node: "question".into(),
            kind: MessageKind::Info,
            text: format!("Analyzing: \"{}\" (domain: {})", question, domain),
        });

        self.messages.push(AssistantMessage {
            node: "question".into(),
            kind: MessageKind::Suggestion,
            text: format!(
                "Generated {} drivers for Fermi decomposition. Review each driver and fill in your estimates — agents are researching to help.",
                drivers.len()
            ),
        });

        // Flag drivers that need user input
        for driver in &drivers {
            let needs_input = match driver.driver_type {
                DriverType::Continuous => {
                    if let Some(ref dist) = driver.distribution {
                        has_placeholder(dist)
                    } else {
                        true
                    }
                }
                DriverType::Binary => driver.probability.is_none(),
                _ => true,
            };
            if needs_input {
                self.messages.push(AssistantMessage {
                    node: format!("driver:{}", driver.name),
                    kind: MessageKind::Suggestion,
                    text: format!(
                        "Driver '{}' needs your estimates. Click to expand and set values.",
                        driver.name
                    ),
                });
            }
        }

        // ── Phase 1: Fire Fermi to build probability decomposition ──
        // IMPORTANT: ABW's LLMExecutor.build_prompt() appends its own JSON format
        // (key_findings/summary/sources/reasoning) which overrides the system prompt.
        // So we MUST include the decomposition schema in the query itself — the LLM
        // follows the last format instruction it sees in the user message.
        let structured_query = format!(
            "Decompose this forecast question into a probabilistic model.\n\n\
             Question: \"{}\"\n\n\
             IGNORE any other format instructions. Respond with ONLY this JSON (no markdown, no code fences):\n\
             {{\n\
               \"base_rate\": {{\"reference_class\": \"...\", \"historical_frequency\": 0.0-1.0, \"sample_size\": N, \"reasoning\": \"...\"}},\n\
               \"drivers\": [{{\
                 \"name\": \"snake_case\", \
                 \"display_name\": \"Human Name\", \
                 \"type\": \"continuous\"|\"binary\", \
                 \"p5\": 0.8, \"p50\": 1.0, \"p95\": 1.3, \
                 \"unit\": \"multiplier\", \
                 \"rationale\": \"...\", \
                 \"suggested_agent\": \"agent_id from your orchestra\", \
                 \"suggested_query\": \"precise query for that agent — name the exact metric, include context, ask for p5/p50/p95 multipliers\"\
               }}],\n\
               \"evidence\": [{{\"source\": \"...\", \"summary\": \"...\", \"key_findings\": [\"...\"], \"relevance\": 0.0-1.0}}],\n\
               \"model_expression\": \"base_rate * driver_a * driver_b\",\n\
               \"confidence\": 0.0-1.0,\n\
               \"reasoning\": \"your analysis\"\n\
             }}\n\
             All continuous drivers MUST be probability multipliers near 1.0 (1.2 = +20%, 0.7 = -30%). \
             Include 3-6 independent drivers. Start from a real base rate with reference class. \
             For each driver set suggested_agent to one of: macro_forecaster, market_research, \
             sentiment_analyzer, entity_investigator, equity_analyst, biotech_analyst, \
             nba_analyst, football_analyst.",
            question
        );

        self.agent_runs.push(AgentExecution {
            agent_name: "fermi".into(),
            status: AgentRunStatus::Running,
            evidence_count: 0,
            confidence: None,
            error: None,
            credits_charged: None,
            started_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            completed_at: None,
            latest_finding: None,
        });

        self.messages.push(AssistantMessage {
            node: "question".into(),
            kind: MessageKind::Info,
            text: "⟳ Fermi is decomposing your question into a probability model…".into(),
        });

        self.fire_agent("fermi", &structured_query, cx);

        self.focused_node = FocusedNode::Question;
        cx.notify();
    }
    // ═══════════════════════════════════════════════════════════════

    /// Process the macro_forecaster's structured response.
    /// This is the main co-authoring step — the agent returns a complete
    /// decomposition with estimates that populate the FPL program.
    fn process_macro_forecaster_result(&mut self, result: &JsonValue, cx: &mut Context<Self>) {
        // DEBUG: log what we received
        log::info!(
            "[composer] Processing result keys: {:?}",
            result.as_object().map(|o| o.keys().collect::<Vec<_>>())
        );
        if let Some(evidence) = result.get("evidence").and_then(|v| v.as_array()) {
            for (i, ev) in evidence.iter().enumerate() {
                let summary_len = ev
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                let findings_count = ev
                    .get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                log::info!(
                    "[composer] Evidence[{}]: summary_len={}, findings_count={}",
                    i,
                    summary_len,
                    findings_count
                );
                if let Some(findings) = ev.get("key_findings").and_then(|v| v.as_array()) {
                    for (j, f) in findings.iter().enumerate().take(3) {
                        log::info!(
                            "[composer]   finding[{}]: {}",
                            j,
                            f.as_str()
                                .unwrap_or("?")
                                .chars()
                                .take(100)
                                .collect::<String>()
                        );
                    }
                }
            }
        }
        let reasoning = result
            .get("metadata")
            .and_then(|m| m.get("reasoning"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        log::info!(
            "[composer] Reasoning length: {}, starts with: {}",
            reasoning.len(),
            reasoning.chars().take(100).collect::<String>()
        );

        // Update agent status — matches either "fermi" (orchestration path) or
        // "macro_forecaster" (direct call path)
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == "macro_forecaster" || r.agent_name == "fermi")
        {
            let completed_name = run.agent_name.clone();
            run.status = AgentRunStatus::Completed;
            run.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            run.confidence = result.get("confidence").and_then(|v| v.as_f64());
            run.credits_charged = result.get("credits_charged").and_then(|v| v.as_f64());
            if let Some(c) = run.credits_charged {
                self.session_cost += c;
            }
            self.pending_toasts
                .push(format!("✓ {} finished", completed_name));
        }

        // Try to parse structured JSON from the agent's reasoning
        let reasoning = result
            .get("metadata")
            .and_then(|m| m.get("reasoning"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Fermi suggestions: driver_name -> (suggested_agent, suggested_query)
        // Populated during driver parsing; consumed by the auto-assign block below.
        let mut fermi_suggestions: HashMap<String, (String, String)> = HashMap::new();

        // Try parsing reasoning as JSON (works when agent returns structured output)
        // Strip markdown code fences if present (agent often wraps JSON in ```json ... ```)
        let clean_reasoning = reasoning
            .trim()
            .strip_prefix("```json")
            .or_else(|| reasoning.trim().strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(reasoning)
            .trim();

        let structured: Option<JsonValue> = if !clean_reasoning.is_empty() {
            serde_json::from_str(clean_reasoning).ok()
        } else {
            None
        };

        if structured.is_some() {
            log::info!("[composer] Parsed structured JSON from reasoning");
        } else {
            log::info!(
                "[composer] Agent returned text (not JSON) - using as evidence. First 80 chars: {}",
                &reasoning.chars().take(80).collect::<String>()
            );
        }

        // ── Base Rate ─────────────────────────────────────────────
        if let Some(ref data) = structured {
            if let Some(br) = data.get("base_rate") {
                let freq = br
                    .get("historical_frequency")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.35);
                let ref_class = br
                    .get("reference_class")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general predictions");
                let sample_size = br
                    .get("sample_size")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize);
                let br_reasoning = br
                    .get("reasoning")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if let Some(q) = self.program.question_mut() {
                    q.base_rate = Some(BaseRate {
                        reference_class: ref_class.to_string(),
                        historical_frequency: freq,
                        sample_size,
                        source: "macro_forecaster".into(),
                        reasoning: br_reasoning,
                        generated_by: GeneratedBy::Agent("macro_forecaster".into()),
                    });
                }
                self.predicted_probability = freq;
                log::info!(
                    "[composer] BASE RATE SET: {:.0}% ref_class={}",
                    freq * 100.0,
                    ref_class
                );

                self.messages.push(AssistantMessage {
                    node: "question".into(),
                    kind: MessageKind::Info,
                    text: format!(
                        "Base rate: {:.0}% from reference class \"{}\"{}",
                        freq * 100.0,
                        ref_class,
                        sample_size
                            .map(|n| format!(" (n={})", n))
                            .unwrap_or_default()
                    ),
                });
            }

            // ── Drivers from structured response ──────────────────
            if let Some(drivers_arr) = data.get("drivers").and_then(|v| v.as_array()) {
                // Clear template drivers — agent provides real ones
                let template_names: Vec<String> = self
                    .program
                    .drivers()
                    .iter()
                    .map(|d| d.name.clone())
                    .collect();
                for name in &template_names {
                    self.program.remove_driver(&name);
                }
                // Also clear the template model — agent may suggest a new one
                let cleared_count = template_names.len();
                log::info!(
                    "[composer] Cleared {} template drivers, replacing with agent suggestions",
                    cleared_count
                );
                for drv in drivers_arr {
                    let name = drv
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let drv_type = drv
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("continuous");
                    let rationale = drv.get("rationale").and_then(|v| v.as_str()).unwrap_or("");

                    let driver_stmt = if drv_type == "binary" {
                        DriverStmt {
                            name: sanitize_name(name),
                            display_name: Some(name.to_string()),
                            description: Some(rationale.to_string()),
                            driver_type: DriverType::Binary,
                            distribution: None,
                            probability: drv
                                .get("probability")
                                .and_then(|v| v.as_f64())
                                .or(Some(0.5)),
                            impact_multiplier: drv
                                .get("impact_multiplier")
                                .and_then(|v| v.as_f64())
                                .or(Some(1.3)),
                            values: None,
                            weights: None,
                            unit: None,
                            rationale: Some(rationale.to_string()),
                            constraints: vec![],
                            evidence_refs: vec![],
                            learnable: false,
                            feeds_from: None,
                        }
                    } else {
                        let p5 = drv.get("p5").and_then(|v| v.as_f64()).unwrap_or(0.8);
                        let p50 = drv.get("p50").and_then(|v| v.as_f64()).unwrap_or(1.0);
                        let p95 = drv.get("p95").and_then(|v| v.as_f64()).unwrap_or(1.2);
                        let unit = drv.get("unit").and_then(|v| v.as_str()).unwrap_or("");

                        DriverStmt {
                            name: sanitize_name(name),
                            display_name: Some(name.to_string()),
                            description: Some(rationale.to_string()),
                            driver_type: DriverType::Continuous,
                            distribution: Some(Distribution::Triangular {
                                p5: Expression::Number(p5),
                                p50: Expression::Number(p50),
                                p95: Expression::Number(p95),
                            }),
                            probability: None,
                            impact_multiplier: None,
                            values: None,
                            weights: None,
                            unit: if unit.is_empty() {
                                None
                            } else {
                                Some(unit.to_string())
                            },
                            rationale: Some(rationale.to_string()),
                            constraints: vec![],
                            evidence_refs: vec![],
                            learnable: false,
                            feeds_from: None,
                        }
                    };

                    // Replace scaffold driver if it exists, otherwise add
                    self.program.add_driver(driver_stmt);
                    log::info!(
                        "[composer] DRIVER ADDED from agent: {}",
                        sanitize_name(name)
                    );

                    // Capture fermi's agent suggestion for this driver
                    if let (Some(agent), Some(query)) = (
                        drv.get("suggested_agent").and_then(|v| v.as_str()),
                        drv.get("suggested_query").and_then(|v| v.as_str()),
                    ) {
                        if !agent.is_empty() && !query.is_empty() {
                            fermi_suggestions.insert(
                                sanitize_name(name),
                                (agent.to_string(), query.to_string()),
                            );
                            log::info!(
                                "[composer] Fermi suggests {} → {}",
                                sanitize_name(name),
                                agent
                            );
                        }
                    }

                    self.messages.push(AssistantMessage {
                        node: format!("driver:{}", sanitize_name(name)),
                        kind: MessageKind::Info,
                        text: format!("Agent suggested driver '{}': {}", name, rationale),
                    });
                }
            }

            // ── Model expression ──────────────────────────────────
            // Regenerate model from the new driver names
            let new_drivers = self.program.drivers();
            let model_parts: Vec<String> = new_drivers
                .iter()
                .map(|d| match d.driver_type {
                    DriverType::Binary => {
                        let m = d.impact_multiplier.unwrap_or(1.3);
                        format!("(if {} then {} else 1.0)", d.name, m)
                    }
                    _ => d.name.clone(),
                })
                .collect();
            if !model_parts.is_empty() {
                // Try to use agent's suggested model expression if it references our drivers
                let agent_model = data
                    .get("model_expression")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let use_agent_model = !agent_model.is_empty()
                    && new_drivers.iter().any(|d| agent_model.contains(&d.name));

                let model_text = if use_agent_model {
                    agent_model.to_string()
                } else {
                    model_parts.join(" * ")
                };

                self.messages.push(AssistantMessage {
                    node: "model".into(),
                    kind: MessageKind::Info,
                    text: format!("Model: {}", model_text),
                });

                // Note: we don't set the AST model here because it needs to be
                // parsed as an Expression. The generate_fpl_text auto-generates
                // the model from driver names if no ModelStmt exists.
                // Clear the old template model so it gets regenerated
                self.program
                    .statements
                    .retain(|s| !matches!(s, Statement::Model(_)));
            }
        }

        // ── Evidence from the agent's evidence array ──────────────
        if let Some(evidence_arr) = result.get("evidence").and_then(|v| v.as_array()) {
            let mut count = 0;
            for ev in evidence_arr {
                let source = ev
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("macro_forecaster");
                let summary = ev.get("summary").and_then(|v| v.as_str());
                let relevance = ev.get("relevance").and_then(|v| v.as_f64());
                let key_findings: Vec<String> = ev
                    .get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                let ev_id = ev
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("macro_forecaster_ev_{}", count));

                self.program.add_evidence(EvidenceStmt {
                    id: ev_id,
                    source: source.to_string(),
                    summary: summary.map(|s| s.to_string()),
                    url: None,
                    relevance,
                    date: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
                    strength: relevance,
                    key_findings,
                });
                count += 1;
            }

            if let Some(run) = self
                .agent_runs
                .iter_mut()
                .find(|r| r.agent_name == "macro_forecaster")
            {
                run.evidence_count = count;
            }

            if count > 0 {
                self.messages.push(AssistantMessage {
                    node: "evidence".into(),
                    kind: MessageKind::Info,
                    text: format!("macro_forecaster found {} evidence items.", count),
                });
            }
        }

        // ── Validation hints ──────────────────────────────────────
        self.run_validation_hints();
        self.orchestration_running = false;

        // ── Log decomposition to workspace ────────────────────────
        if let Some(ref ws_id) = self.workspace_id {
            let api = self.api.clone();
            let ws = ws_id.clone();
            let driver_names: Vec<String> = self
                .program
                .drivers()
                .iter()
                .map(|d| d.display_name.as_deref().unwrap_or(&d.name).to_string())
                .collect();
            let question = self
                .program
                .question()
                .map(|q| q.text.clone())
                .unwrap_or_default();
            let content = format!(
                "**Decomposition complete** for: \"{}\"\n\n{} drivers identified:\n{}",
                question.chars().take(100).collect::<String>(),
                driver_names.len(),
                driver_names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| format!("{}. {}", i + 1, n))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            let meta = serde_json::json!({
                "cost_class": "event_append",
                "fermi_action": "decompose_question",
                "driver_count": driver_names.len(),
            });
            tokio::spawn(async move {
                let _ = api
                    .post_workspace_message(
                        &ws,
                        "agent",
                        "fermi",
                        Some("Fermi Decomposer"),
                        &content,
                        "execution_result",
                        Some(&meta),
                    )
                    .await;
            });
        }

        // ── Auto-assign + auto-fire agents for all drivers ────────
        // ONLY on initial decomposition — skip if drivers already have agents.
        // This prevents the cascade: agent completes → process_macro_forecaster_result
        // → auto-assigns MORE agents → rate limit → failure loop.
        let drivers_already_have_agents = self
            .program
            .agents()
            .iter()
            .any(|a| !a.driver_refs.is_empty() && a.name != "fermi");
        if drivers_already_have_agents {
            return;
        }

        let question_text = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        let domain = detect_domain(&question_text);
        let driver_names: Vec<String> = self
            .program
            .drivers()
            .iter()
            .map(|d| d.name.clone())
            .collect();

        if !driver_names.is_empty() {
            // Domain-specific primary agent (used as default when no better match)
            let domain_agent = match domain.as_str() {
                "sports_nba" | "basketball" => "nba_analyst",
                "biotech" | "pharma" | "clinical" => "biotech_analyst",
                "sports_football" | "sports_nfl" | "sports_other" => "football_analyst",
                "finance" | "stocks" => "macro_forecaster",
                "politics" | "geopolitics" => "macro_forecaster",
                "technology" => "market_research",
                "climate" => "macro_forecaster",
                _ => "macro_forecaster",
            };

            let mut assigned_count = 0;

            for driver_name in &driver_names {
                let driver = self.program.driver(driver_name);
                let rationale = driver
                    .and_then(|d| d.rationale.as_deref())
                    .unwrap_or("")
                    .to_string();
                let driver_display = driver
                    .and_then(|d| d.display_name.as_deref())
                    .unwrap_or(driver_name)
                    .to_string();

                // Extract current driver parameters for the query
                let (p5, p50, p95) = driver
                    .and_then(|d| d.distribution.as_ref())
                    .map(|dist| match dist {
                        Distribution::Triangular { p5, p50, p95 } => {
                            (expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95))
                        }
                        _ => (0.8, 1.0, 1.2),
                    })
                    .unwrap_or((0.8, 1.0, 1.2));

                // ── Per-driver agent selection ────────────────────
                // Use fermi's suggestion if present and the agent exists in the
                // registry; fall back to keyword heuristics otherwise.
                let (agent_to_use, query) = if let Some((suggested_agent, suggested_query)) =
                    fermi_suggestions.get(driver_name)
                {
                    let agent = if self.registry.get(suggested_agent.as_str()).is_ok() {
                        log::info!(
                            "[composer] Using fermi suggestion for {}: {}",
                            driver_name,
                            suggested_agent
                        );
                        suggested_agent.as_str()
                    } else {
                        log::warn!(
                                "[composer] Fermi suggested {} for {} but agent not in registry — falling back",
                                suggested_agent,
                                driver_name
                            );
                        domain_agent
                    };
                    let agent = if self.registry.get(agent).is_ok() {
                        agent
                    } else {
                        "macro_forecaster"
                    };
                    (agent.to_string(), suggested_query.clone())
                } else {
                    // Keyword heuristics — fermi didn't provide a suggestion
                    let dl = driver_name.to_lowercase();
                    let rl = rationale.to_lowercase();
                    let combined = format!("{} {}", dl, rl);

                    let heuristic_agent = if combined.contains("sentiment")
                        || combined.contains("opinion")
                        || combined.contains("perception")
                        || combined.contains("buzz")
                        || combined.contains("narrative")
                        || combined.contains("social media")
                        || combined.contains("public opinion")
                    {
                        "sentiment_analyzer"
                    } else if combined.contains("entity")
                        || combined.contains("ownership")
                        || combined.contains("leadership")
                        || combined.contains("management")
                        || combined.contains("regulatory")
                        || combined.contains("legal")
                        || combined.contains("compliance")
                        || combined.contains("investigation")
                        || combined.contains("regime")
                        || combined.contains("government")
                        || combined.contains("military")
                        || combined.contains("security apparatus")
                        || combined.contains("cohesion")
                        || combined.contains("supreme leader")
                        || combined.contains("succession")
                    {
                        "entity_investigator"
                    } else if combined.contains("protest")
                        || combined.contains("revolution")
                        || combined.contains("uprising")
                        || combined.contains("momentum")
                        || combined.contains("unrest")
                        || combined.contains("civil")
                        || combined.contains("dissent")
                        || combined.contains("demonstration")
                    {
                        "sentiment_analyzer"
                    } else if combined.contains("market")
                        || combined.contains("competition")
                        || combined.contains("competitor")
                        || combined.contains("partnership")
                        || combined.contains("revenue")
                        || combined.contains("pricing")
                        || combined.contains("demand")
                        || combined.contains("adoption")
                        || combined.contains("customer")
                        || combined.contains("commercial")
                        || combined.contains("sales")
                    {
                        "market_research"
                    } else if combined.contains("macro")
                        || combined.contains("economic")
                        || combined.contains("gdp")
                        || combined.contains("inflation")
                        || combined.contains("interest rate")
                        || combined.contains("fed")
                        || combined.contains("recession")
                        || combined.contains("valuation")
                        || combined.contains("currency")
                        || combined.contains("sanction")
                        || combined.contains("crisis")
                        || combined.contains("trade")
                        || combined.contains("fiscal")
                        || combined.contains("monetary")
                    {
                        "macro_forecaster"
                    } else if combined.contains("policy")
                        || combined.contains("geopolit")
                        || combined.contains("diplomat")
                        || combined.contains("interven")
                        || combined.contains("external")
                        || combined.contains("foreign")
                        || combined.contains("international")
                        || combined.contains("alliance")
                        || combined.contains("nuclear")
                    {
                        "macro_forecaster"
                    } else if combined.contains("clinical")
                        || combined.contains("trial")
                        || combined.contains("fda")
                        || combined.contains("drug")
                        || combined.contains("pipeline")
                        || combined.contains("approval")
                    {
                        "biotech_analyst"
                    } else if combined.contains("stock")
                        || combined.contains("equity")
                        || combined.contains("eps")
                        || combined.contains("p/e")
                        || combined.contains("earnings")
                        || combined.contains("share price")
                        || combined.contains("shareholder")
                    {
                        "equity_analyst"
                    } else if combined.contains("energy")
                        || combined.contains("oil")
                        || combined.contains("gas")
                        || combined.contains("renewable")
                        || combined.contains("solar")
                        || combined.contains("wind power")
                        || combined.contains("carbon")
                        || combined.contains("emission")
                    {
                        "energy_advisor"
                    } else if combined.contains("nba")
                        || combined.contains("basketball")
                        || combined.contains("elo")
                        || combined.contains("home court")
                        || (combined.contains("injury")
                            && (domain.contains("nba") || domain.contains("basketball")))
                    {
                        "nba_analyst"
                    } else {
                        let has_domain = self.registry.get(domain_agent).is_ok();
                        if has_domain {
                            domain_agent
                        } else {
                            "macro_forecaster"
                        }
                    };

                    let agent = if self.registry.get(heuristic_agent).is_ok() {
                        heuristic_agent
                    } else {
                        "macro_forecaster"
                    };
                    let q = formulate_research_query(
                        &question_text,
                        &driver_display,
                        &rationale,
                        agent,
                        &domain,
                        p5,
                        p50,
                        p95,
                    );
                    (agent.to_string(), q)
                };

                // Create compound agent name for this driver
                let compound_name = format!("{}_{}", agent_to_use, sanitize_name(driver_name));
                let compound_for_fire = compound_name.clone();

                // Add agent to AST
                self.program.add_agent(AgentStmt {
                    name: compound_name.clone(),
                    agent_type: Some("research".into()),
                    query: query.clone(),
                    executor: Some(fermi::ast::ExecutorType::LLM),
                    schedule: Some(Schedule::Once),
                    driver_refs: vec![driver_name.clone()],
                    depends_on: vec![],
                    confidence_threshold: None,
                });

                // Track execution
                self.agent_runs.push(AgentExecution {
                    agent_name: compound_name,
                    status: AgentRunStatus::Running,
                    evidence_count: 0,
                    confidence: None,
                    error: None,
                    credits_charged: None,
                    started_at: Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs(),
                    ),
                    completed_at: None,
                    latest_finding: None,
                });

                // Fire agent with compound name so tracking matches agent_runs
                self.fire_agent(&compound_for_fire, &query, cx);
                assigned_count += 1;
            }

            // Summarize what was assigned
            let mut agent_counts: HashMap<String, usize> = HashMap::new();
            for a in self
                .program
                .agents()
                .iter()
                .filter(|a| a.name != "fermi" && !a.driver_refs.is_empty())
            {
                let base = base_agent_name(&a.name).to_string();
                *agent_counts.entry(base).or_insert(0) += 1;
            }
            let summary: Vec<String> = agent_counts
                .iter()
                .map(|(agent, count)| format!("{} ({})", agent, count))
                .collect();

            self.messages.push(AssistantMessage {
                node: "question".into(),
                kind: MessageKind::Info,
                text: format!(
                    "🔬 Auto-assigned {} agents to {} drivers: {}. Evidence streaming in…",
                    assigned_count,
                    driver_names.len(),
                    summary.join(", ")
                ),
            });
        }
    }

    /// Discover research-relevant agents from the local registry.
    /// Filters by agent_type and tags to find agents suitable for forecasting.
    fn discover_research_agents(&self) -> Vec<(String, String)> {
        let cards = self.registry.list_cards().unwrap_or_default();
        cards
            .iter()
            .filter(|card| {
                // Only agents tagged for the Fermi forecasting orchestra
                card.metadata.tags.iter().any(|t| t == "fermi-orchestra")
            })
            .map(|card| (card.agent_id.clone(), card.metadata.description.clone()))
            .collect()
    }

    /// Fire a single agent in the background. Results flow back via cx.spawn.
    ///
    /// Execution path:
    ///   1. ABW API (primary) — user authenticated via OAuth, ABW handles LLM costs
    ///   2. Local registry (dev fallback) — only if ANTHROPIC_API_KEY was set at startup
    ///   3. Fail with "Sign in to run agents" if neither is available
    fn fire_agent(&self, agent_id: &str, query: &str, cx: &mut Context<Self>) {
        // agent_id may be compound (market_research_song_quality)
        // Registry knows the base name (market_research)
        let base_id = base_agent_name(agent_id).to_string();
        let tracking_id = agent_id.to_string();
        let api = self.api.clone();
        let registry = self.registry.clone();
        let q = query.to_string();
        let sse_tx = self.sse_tx.clone();

        cx.spawn(async move |this, cx| {
            log::info!("[composer] Firing {} (base: {})", tracking_id, base_id);

            // ── Determine execution path ──────────────────────────
            let use_api = api.is_authenticated().await;

            // ── Execute via tokio::spawn to ensure proper reactor context ──
            // GPUI's async executor doesn't drive tokio's I/O reactor,
            // so reqwest hangs on long-running HTTP calls. We spawn onto
            // the tokio runtime (entered in main()) for the actual network I/O.
            let api_clone = api.clone();
            let base_id_clone = base_id.clone();
            let q_clone = q.clone();
            let registry_clone = registry.clone();

            let result_json: Result<JsonValue, String> = if use_api {
                // ── ABW SSE streaming path ────────────────────────
                // Uses POST /api/agents/:id/execute/stream for real-time
                // progress events instead of waiting 25-30s for full response.
                log::info!("[composer] {} → ABW SSE stream", base_id);

                let base_url = api.base_url().await;
                let api_key = api.api_key().unwrap_or_default();
                let sse_url = format!(
                    "{}/api/agents/{}/execute/stream",
                    base_url, base_id_clone
                );
                let body = serde_json::json!({ "query": q_clone }).to_string();

                // Channel for SSE line events from HTTP stream → event processor
                let (tx, mut rx) = mpsc::channel::<(String, String)>(32);
                let sse_tx_clone = sse_tx.clone();
                let tracking_for_sse = tracking_id.clone();

                // Spawn the HTTP streaming request on tokio runtime
                tokio::spawn(async move {
                    let client = reqwest::Client::new();
                    let resp = client
                        .post(&sse_url)
                        .header("Authorization", format!("Bearer {}", api_key))
                        .header("Content-Type", "application/json")
                        .header("Accept", "text/event-stream")
                        .body(body)
                        .timeout(std::time::Duration::from_secs(120))
                        .send()
                        .await;

                    match resp {
                        Ok(response) => {
                            if !response.status().is_success() {
                                let status = response.status().as_u16();
                                let body = response.text().await.unwrap_or_default();
                                let _ = tx.send(("error".into(), format!("HTTP {}: {}", status, body))).await;
                                return;
                            }
                            // Read SSE stream line by line
                            use futures::StreamExt;
                            let mut stream = response.bytes_stream();
                            let mut buffer = String::new();
                            let mut current_event = String::new();

                            while let Some(chunk) = stream.next().await {
                                match chunk {
                                    Ok(bytes) => {
                                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                                        // Process complete lines
                                        while let Some(newline_pos) = buffer.find('\n') {
                                            let line = buffer[..newline_pos].trim_end().to_string();
                                            buffer = buffer[newline_pos + 1..].to_string();

                                            if line.starts_with("event: ") {
                                                current_event = line[7..].to_string();
                                            } else if line.starts_with("data: ") {
                                                let data = line[6..].to_string();
                                                let evt = if current_event.is_empty() {
                                                    "message".to_string()
                                                } else {
                                                    current_event.clone()
                                                };
                                                if tx.send((evt, data)).await.is_err() {
                                                    return; // receiver dropped
                                                }
                                            } else if line.is_empty() {
                                                current_event.clear();
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let _ = tx.send(("error".into(), format!("Stream: {}", e))).await;
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(("error".into(), format!("Connection: {}", e))).await;
                        }
                    }
                });

                // Receive SSE events and update UI progressively
                let mut final_result: Option<JsonValue> = None;
                let mut stream_error: Option<String> = None;

                // Process SSE events — push live updates through the GPUI channel
                while let Some((event_type, data)) = rx.recv().await {
                    match event_type.as_str() {
                        "started" => {
                            log::info!("[composer] {} SSE: started", tracking_id);
                            let _ = sse_tx_clone.send(SseEvent::Started {
                                agent_id: tracking_for_sse.clone(),
                            });
                        }
                        "progress" => {
                            if let Ok(d) = serde_json::from_str::<JsonValue>(&data) {
                                let msg = d.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                let elapsed = d.get("elapsed_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                                log::info!("[composer] {} SSE: {} ({}ms)", tracking_id, msg, elapsed);
                            }
                        }
                        "evidence" => {
                            if let Ok(d) = serde_json::from_str::<JsonValue>(&data) {
                                if let Some(finding) = d.get("finding").and_then(|v| v.as_str()) {
                                    if !finding.is_empty() {
                                        log::info!(
                                            "[composer] {} SSE evidence: {}",
                                            tracking_id,
                                            finding.chars().take(80).collect::<String>()
                                        );
                                        let _ = sse_tx_clone.send(SseEvent::Finding {
                                            agent_id: tracking_for_sse.clone(),
                                            text: finding.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                        "complete" => {
                            log::info!("[composer] {} SSE: complete", tracking_id);
                            if let Ok(result) = serde_json::from_str::<JsonValue>(&data) {
                                final_result = Some(result);
                            } else {
                                stream_error = Some("Failed to parse complete event".into());
                            }
                            break;
                        }
                        "error" => {
                            log::error!("[composer] {} SSE error: {}", tracking_id, data);
                            stream_error = Some(data);
                            break;
                        }
                        _ => {} // keepalive, unknown events
                    }
                }

                // If SSE stream failed, fall back to non-streaming
                if let Some(result) = final_result {
                    Ok(result)
                } else if let Some(err) = stream_error {
                    // Try non-streaming fallback
                    log::warn!("[composer] {} SSE failed, trying non-streaming: {}", tracking_id, err);
                    let api_fb = api.clone();
                    let bid = base_id.clone();
                    let qfb = q.clone();
                    let handle = tokio::spawn(async move {
                        api_fb.execute_agent(&bid, &qfb).await
                    });
                    match handle.await {
                        Ok(Ok(api_result)) => {
                            let evidence = api_result.evidence.unwrap_or_default();
                            let metadata = api_result.metadata.unwrap_or_else(|| serde_json::json!({}));
                            Ok(serde_json::json!({
                                "agent_id": api_result.agent_id,
                                "status": api_result.status,
                                "confidence": api_result.confidence,
                                "execution_time_ms": api_result.execution_time_ms,
                                "tokens_used": api_result.tokens_used,
                                "credits_charged": api_result.credits_charged,
                                "evidence": evidence,
                                "metadata": metadata,
                            }))
                        }
                        Ok(Err(e)) => Err(format!("ABW API: {}", e)),
                        Err(e) => Err(format!("Agent task panicked: {}", e)),
                    }
                } else {
                    Err("SSE stream ended without complete event".into())
                }
            } else {
                // ── Fallback: local registry (dev mode with ANTHROPIC_API_KEY) ──
                let executor_name = registry.executor_arc().name().to_string();
                if executor_name == "mock" {
                    Err("Sign in to ABW to run agents. Click Dashboard → Sign In with Google or GitHub.".to_string())
                } else {
                    log::info!("[composer] {} → local executor ({})", base_id, executor_name);
                    let card = match registry.get(&base_id) {
                        Ok(c) => c,
                        Err(e) => {
                            return {
                                this.update(cx, |state, cx| {
                                    state.mark_agent_failed(
                                        &tracking_id,
                                        &format!("Not in registry: {}", e),
                                    );
                                    cx.notify();
                                })
                                .ok();
                            };
                        }
                    };

                    let agent_stmt = AgentStmt {
                        name: base_id.clone(),
                        agent_type: Some("research".into()),
                        query: q.clone(),
                        executor: Some(fermi::ast::ExecutorType::LLM),
                        schedule: None,
                        driver_refs: vec![],
                        depends_on: vec![],
                        confidence_threshold: None,
                    };

                    let program = Program {
                        statements: vec![Statement::Agent(agent_stmt.clone())],
                    };

                    let context = ExecutionContext {
                        program,
                        agent_card: card.clone(),
                        creature_id: None,
                        cognition_tier: None,
                    };

                    // Also use tokio::spawn for local execution (it uses reqwest too)
                    let handle = tokio::spawn(async move {
                        registry_clone.execute_agent(&agent_stmt, &context).await
                    });
                    match handle.await {
                        Ok(Ok(output)) => Ok(serde_json::json!({
                            "agent_id": output.agent_name,
                            "status": format!("{:?}", output.status),
                            "confidence": output.confidence,
                            "execution_time_ms": output.execution_time_ms,
                            "tokens_used": output.tokens_used,
                            "evidence": output.evidence.iter().map(|e| serde_json::json!({
                                "id": e.id,
                                "source": e.source,
                                "summary": e.summary.clone().unwrap_or_default(),
                                "key_findings": e.key_findings,
                                "relevance": e.relevance.unwrap_or(0.0),
                            })).collect::<Vec<_>>(),
                            "metadata": {
                                "model_used": output.metadata.model_used,
                                "reasoning": output.metadata.reasoning,
                            }
                        })),
                        Ok(Err(e)) => Err(format!("Local executor: {}", e)),
                        Err(e) => Err(format!("Agent task panicked: {}", e)),
                    }
                }
            };

            // ── Process result (same for both paths) ──────────────
            match result_json {
                Ok(result_json) => {
                    log::info!("[composer] {} completed", tracking_id);

                    // Debug: log the response shape so we can diagnose routing
                    let evidence_count = result_json.get("evidence")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let has_metadata = result_json.get("metadata").is_some();
                    let reasoning_len = result_json.get("metadata")
                        .and_then(|m| m.get("reasoning"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.len())
                        .unwrap_or(0);
                    let reasoning_preview = result_json.get("metadata")
                        .and_then(|m| m.get("reasoning"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.chars().take(200).collect::<String>())
                        .unwrap_or_else(|| "NO REASONING".into());
                    log::info!(
                        "[composer] {} response: evidence={}, has_metadata={}, reasoning_len={}, preview: {}",
                        tracking_id, evidence_count, has_metadata, reasoning_len, reasoning_preview
                    );

                    // Also log metadata keys if present
                    if let Some(meta) = result_json.get("metadata") {
                        if let Some(obj) = meta.as_object() {
                            let keys: Vec<&String> = obj.keys().collect();
                            log::info!("[composer] {} metadata keys: {:?}", tracking_id, keys);
                        } else if let Some(s) = meta.as_str() {
                            log::info!("[composer] {} metadata is a string (len {}): {}...",
                                tracking_id, s.len(), s.chars().take(200).collect::<String>());
                        }
                    }

                    // Extract findings for the tip message
                    let findings: Vec<String> = result_json
                        .get("evidence")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .flat_map(|e| {
                                    e.get("key_findings")
                                        .and_then(|v| v.as_array())
                                        .into_iter()
                                        .flatten()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                })
                                .take(5)
                                .collect()
                        })
                        .unwrap_or_default();

                    this.update(cx, |state, cx| {
                        // Route to the right processor based on agent type.
                        // If a driver-bound agent (compound name like "macro_forecaster_driver_x"),
                        // always process evidence for that driver regardless of agent type.
                        let is_driver_bound = tracking_id != base_id; // compound name means it's bound to a driver
                        log::info!("[composer] Routing {} to processor (base_id={}, driver_bound={})", tracking_id, base_id, is_driver_bound);

                        if is_driver_bound {
                            // Driver-bound agent: add evidence to the driver first
                            log::info!("[composer] → process_agent_evidence({}) [driver-bound]", tracking_id);
                            state.process_agent_evidence(&tracking_id, &result_json);
                        }

                        if base_id == "macro_forecaster" && !is_driver_bound {
                            log::info!("[composer] → process_macro_forecaster_result");
                            state.process_macro_forecaster_result(&result_json, cx);
                        } else if base_id == "fermi" {
                            // Check multiple locations for the structured JSON decomposition:
                            // 1. metadata.reasoning (local executor path)
                            // 2. evidence[0].summary (ABW sometimes puts it here)
                            // 3. The raw reasoning text may BE the JSON (new prompt enforces this)
                            let reasoning = result_json
                                .get("metadata")
                                .and_then(|m| m.get("reasoning"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            // Also check evidence summary as a fallback source
                            let evidence_summary = result_json
                                .get("evidence")
                                .and_then(|v| v.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|e| e.get("summary"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");

                            // Try to find structured JSON in either location
                            let candidates = [reasoning, evidence_summary];
                            let mut found_decomposition = false;

                            for candidate in &candidates {
                                let clean = candidate
                                    .trim()
                                    .strip_prefix("```json")
                                    .or_else(|| candidate.trim().strip_prefix("```"))
                                    .and_then(|s| s.strip_suffix("```"))
                                    .unwrap_or(candidate)
                                    .trim();

                                let has_base_rate = clean.contains("base_rate")
                                    && (clean.contains("drivers")
                                        || clean.contains("historical_frequency"));

                                log::info!(
                                    "[composer] fermi candidate (len={}): has_base_rate={}, preview={}",
                                    clean.len(), has_base_rate,
                                    clean.chars().take(120).collect::<String>()
                                );

                                if has_base_rate {
                                    // If the JSON is in evidence summary, we need to
                                    // inject it into metadata.reasoning for the processor
                                    let mut patched = result_json.clone();
                                    if let Some(meta) = patched.get_mut("metadata") {
                                        if let Some(obj) = meta.as_object_mut() {
                                            obj.insert(
                                                "reasoning".to_string(),
                                                serde_json::json!(clean),
                                            );
                                        }
                                    } else {
                                        patched["metadata"] = serde_json::json!({
                                            "reasoning": clean
                                        });
                                    }
                                    log::info!("[composer] → process_macro_forecaster_result (decomposition found)");
                                    state.process_macro_forecaster_result(&patched, cx);
                                    found_decomposition = true;
                                    break;
                                }
                            }

                            if !found_decomposition {
                                // Last resort: parse narrative text for base rate + drivers
                                // The LLM consistently mentions "base rate of X%" and driver
                                // descriptions even when it doesn't return JSON.
                                let narrative = result_json
                                    .get("metadata")
                                    .and_then(|m| m.get("reasoning"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("");

                                if let Some(synthetic) = parse_narrative_decomposition(narrative) {
                                    log::info!(
                                        "[composer] → process_macro_forecaster_result (parsed from narrative: base_rate={:.1}%, {} drivers)",
                                        synthetic.get("base_rate")
                                            .and_then(|b| b.get("historical_frequency"))
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0) * 100.0,
                                        synthetic.get("drivers")
                                            .and_then(|d| d.as_array())
                                            .map(|a| a.len())
                                            .unwrap_or(0)
                                    );
                                    // Wrap in the same shape process_macro_forecaster_result expects
                                    let patched = serde_json::json!({
                                        "evidence": result_json.get("evidence").cloned().unwrap_or(serde_json::json!([])),
                                        "confidence": result_json.get("confidence").cloned().unwrap_or(serde_json::json!(0.5)),
                                        "metadata": {
                                            "reasoning": synthetic.to_string(),
                                            "model_used": result_json.get("metadata")
                                                .and_then(|m| m.get("model_used"))
                                                .cloned()
                                                .unwrap_or(serde_json::json!("unknown"))
                                        }
                                    });
                                    state.process_macro_forecaster_result(&patched, cx);
                                } else {
                                    log::info!("[composer] → process_fermi_recommendation (no decomposition found, even from narrative)");
                                    state.process_fermi_recommendation(&result_json, cx);
                                }
                            }
                        } else if !is_driver_bound {
                            // Other agents (not driver-bound): add evidence to AST
                            log::info!("[composer] → process_agent_evidence({})", tracking_id);
                            state.process_agent_evidence(&tracking_id, &result_json);
                        }

                        if !findings.is_empty() {
                            state.messages.push(AssistantMessage {
                                node: format!("agent:{}", tracking_id),
                                kind: MessageKind::Tip,
                                text: format!(
                                    "🦊 {} findings:\n{}",
                                    tracking_id,
                                    findings
                                        .iter()
                                        .map(|f| format!("• {}", f))
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                ),
                            });
                        }

                        state.messages.push(AssistantMessage {
                            node: format!("agent:{}", tracking_id),
                            kind: MessageKind::Info,
                            text: format!(
                                "✓ {} complete{}",
                                tracking_id,
                                if use_api { " (via ABW)" } else { " (local)" }
                            ),
                        });

                        // Check if all agents done
                        let all_done = state
                            .agent_runs
                            .iter()
                            .all(|r| r.status != AgentRunStatus::Running);
                        if all_done {
                            state.orchestration_running = false;
                            state.messages.push(AssistantMessage {
                                node: "question".into(),
                                kind: MessageKind::Suggestion,
                                text: "All agents complete. Review drivers and evidence, then Ctrl+R to simulate.".into(),
                            });
                        }

                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("[composer] {} failed: {}", tracking_id, e);
                    this.update(cx, |state, cx| {
                        state.mark_agent_failed(&tracking_id, &e);
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Process evidence from a non-macro_forecaster agent.
    /// Process Fermi meta-agent recommendation.
    fn process_fermi_recommendation(&mut self, result: &JsonValue, cx: &mut Context<Self>) {
        if let Some(run) = self.agent_runs.iter_mut().find(|r| r.agent_name == "fermi") {
            run.status = AgentRunStatus::Completed;
        }

        // Try to parse the recommendation from reasoning
        let reasoning = result
            .get("metadata")
            .and_then(|m| m.get("reasoning"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let clean = reasoning
            .trim()
            .strip_prefix("```json")
            .or_else(|| reasoning.trim().strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(reasoning)
            .trim();

        if let Ok(rec) = serde_json::from_str::<JsonValue>(clean) {
            let agent = rec
                .get("recommended_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("market_research");
            let reason = rec.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");
            let query = rec
                .get("suggested_query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            self.messages.push(AssistantMessage {
                node: "agent_picker".into(),
                kind: MessageKind::Tip,
                text: format!("🦊 Fermi recommends **{}**: {}", agent, reason),
            });

            if !query.is_empty() {
                // Pre-fill the query input with Fermi's suggestion
                self.agent_query_input.update(cx, |input, cx| {
                    input.set_text(query.replace('\n', " ").replace("  ", " "), cx);
                });
            }
        } else {
            // Fallback: show raw reasoning as guidance
            let summary: String = reasoning.chars().take(300).collect();
            if !summary.is_empty() {
                self.messages.push(AssistantMessage {
                    node: "agent_picker".into(),
                    kind: MessageKind::Tip,
                    text: format!("🦊 Fermi: {}", summary),
                });
            }
        }
    }

    fn process_agent_evidence(&mut self, agent_id: &str, result: &JsonValue) {
        // Extract the first key finding before borrowing agent_runs mutably
        let first_finding: Option<String> = result
            .get("evidence")
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|ev| {
                ev.get("key_findings")
                    .and_then(|v| v.as_array())
                    .and_then(|f| f.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(120).collect())
            })
            .or_else(|| {
                result
                    .get("evidence")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|ev| ev.get("summary"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.chars().take(120).collect())
            });

        // Match by exact name OR by base agent name (compound names like
        // market_research_satellite_deployment match base "market_research")
        if let Some(run) = self.agent_runs.iter_mut().find(|r| {
            r.agent_name == agent_id
                || base_agent_name(&r.agent_name) == agent_id
                || r.agent_name.starts_with(agent_id)
        }) {
            let completed_name = run.agent_name.clone();
            run.status = AgentRunStatus::Completed;
            run.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            run.confidence = result.get("confidence").and_then(|v| v.as_f64());
            run.credits_charged = result.get("credits_charged").and_then(|v| v.as_f64());
            if let Some(c) = run.credits_charged {
                self.session_cost += c;
            }
            run.latest_finding = first_finding;
            self.pending_toasts
                .push(format!("✓ {} finished research", completed_name));
        }

        if let Some(evidence_arr) = result.get("evidence").and_then(|v| v.as_array()) {
            let mut count = 0;
            for ev in evidence_arr {
                let source = ev
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or(agent_id);
                let summary = ev.get("summary").and_then(|v| v.as_str());
                let relevance = ev.get("relevance").and_then(|v| v.as_f64());
                let key_findings: Vec<String> = ev
                    .get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                self.program.add_evidence(EvidenceStmt {
                    id: format!("{}_{}", agent_id, count),
                    source: source.to_string(),
                    summary: summary.map(|s| s.to_string()),
                    url: None,
                    relevance,
                    date: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
                    strength: relevance,
                    key_findings,
                });
                count += 1;
            }
            if let Some(run) = self.agent_runs.iter_mut().find(|r| {
                r.agent_name == agent_id
                    || base_agent_name(&r.agent_name) == agent_id
                    || r.agent_name.starts_with(agent_id)
            }) {
                run.evidence_count = count;
            }

            // ── Extract parameter suggestions from evidence ───────
            // Agents include "Suggested p50: X.XX" in their findings.
            // Parse these into pending suggestions for user accept/reject.
            for ev in evidence_arr {
                let summary_text = ev.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let findings_list: Vec<&str> = ev
                    .get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                let all_text = format!("{} {}", summary_text, findings_list.join(" "));

                if let Some(suggested) = extract_suggested_p50(&all_text) {
                    let driver_name = self
                        .program
                        .agents()
                        .iter()
                        .find(|a| {
                            a.name == agent_id
                                || base_agent_name(&a.name) == agent_id
                                || a.name.starts_with(agent_id)
                        })
                        .and_then(|a| a.driver_refs.first().cloned());

                    if let Some(dn) = driver_name {
                        // Read the driver's current center across all the
                        // distribution shapes our templates use (not just
                        // Triangular). Two layers:
                        //
                        //   1. If the driver's distribution is parameterized
                        //      (Triangular(Identifier, Identifier, Identifier))
                        //      read the current p50 from workspace_params.
                        //      That's the source of truth for parameterized
                        //      WC drivers — the AST literal would just be
                        //      "socio_p50" the identifier.
                        //
                        //   2. Otherwise fall back to the AST literal (legacy
                        //      hand-authored FPLs) or 1.0 if even that isn't
                        //      a Number expression.
                        let driver_dist = self
                            .program
                            .driver(&dn)
                            .and_then(|d| d.distribution.as_ref());

                        let current_p50 = match driver_dist {
                            Some(Distribution::Triangular {
                                p50: Expression::Identifier(p50_name),
                                ..
                            }) => self
                                .workspace_params
                                .get(p50_name)
                                .and_then(|v| v.as_f64())
                                .unwrap_or(1.0),
                            Some(d) => distribution_center_or_default(d),
                            None => 1.0,
                        };

                        // Only create suggestion if meaningfully different (>1% change)
                        if (suggested - current_p50).abs() / current_p50.max(0.01) > 0.01 {
                            let sug_id =
                                format!("sug_{}_{}", agent_id, self.pending_suggestions.len());
                            let ev_id = format!("{}_{}", agent_id, count.saturating_sub(1));
                            self.pending_suggestions.push(EvidenceSuggestion {
                                id: sug_id,
                                driver_name: dn.clone(),
                                agent_name: agent_id.to_string(),
                                suggested_p50: suggested,
                                current_p50,
                                reasoning: all_text.chars().take(200).collect(),
                                evidence_id: ev_id,
                            });

                            self.messages.push(AssistantMessage {
                                node: format!("driver:{}", dn),
                                kind: MessageKind::Suggestion,
                                text: format!(
                                    "💡 {} suggests p50 {:.2} → {:.2} ({:+.0}%)",
                                    base_agent_name(agent_id),
                                    current_p50,
                                    suggested,
                                    (suggested / current_p50.max(0.001) - 1.0) * 100.0
                                ),
                            });
                        }
                    }
                }
            }

            // ── Post evidence to workspace (if workspace exists) ──────
            // This bridges agent findings into the ABW workspace message
            // log, making them visible on the dashboard and available for
            // Loop 3 coherence evaluation.
            if let Some(ref ws_id) = self.workspace_id {
                let api = self.api.clone();
                let ws = ws_id.clone();
                let agent = agent_id.to_string();
                let summary = result
                    .get("evidence")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|e| e.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Research complete")
                    .to_string();
                let evidence_json = result
                    .get("evidence")
                    .cloned()
                    .unwrap_or(serde_json::json!([]));
                let meta = serde_json::json!({
                    "cost_class": "event_append",
                    "fermi_action": "add_evidence",
                    "agent_id": agent,
                    "evidence_count": count,
                });
                tokio::spawn(async move {
                    let content = format!(
                        "**{}** completed research ({} evidence items):\n\n{}",
                        base_agent_name(&agent),
                        count,
                        summary.chars().take(500).collect::<String>()
                    );
                    let _ = api
                        .post_workspace_message(
                            &ws,
                            "agent",
                            &agent,
                            Some(base_agent_name(&agent)),
                            &content,
                            "execution_result",
                            Some(&meta),
                        )
                        .await;
                });
            }

            // ── Persist evidence to the server-side forecast row ─────
            //
            // Without this, every agent run only mutates the local AST.
            // Reopening the forecast pulls fpl_source from the server
            // (which has no evidence baked in — the WC team_prior
            // template only declares drivers/agents/params/feeds_from),
            // re-parses into a fresh Program, and all the research
            // disappears.
            //
            // Push the full evidence + agent set to fermi_forecasts as
            // JSONB. The serializer already returns these fields on GET,
            // so open_forecast can read them back. Drivers stay
            // template-defined; we update predicted_probability via the
            // dedicated /update-probability endpoint elsewhere.
            self.push_research_state_to_server();
        }
    }

    /// Push the cockpit's current `program.evidence_items()` and
    /// `program.agents()` lists to the server-side `fermi_forecasts.evidence`
    /// and `agents_used` JSONB columns. Fire-and-forget; failures log but
    /// don't block the UI.
    ///
    /// This is the persistence wire for accumulated agent research. It runs
    /// after every successful agent completion (in `process_agent_evidence`)
    /// so reopening the forecast restores the work, not just the FPL skeleton.
    fn push_research_state_to_server(&self) {
        let Some(ref fid) = self.forecast_id else {
            // No forecast_id yet — operator hasn't published. The legacy
            // local-disk save path (Ctrl+S) covers this case via
            // forecasts/<name>.state.json.
            return;
        };

        // Serialize evidence items by hand because EvidenceStmt doesn't
        // derive Serialize on the upstream ast crate. Same shape the GET
        // serializer would write back so the round-trip is symmetric.
        let evidence_json: Vec<serde_json::Value> = self
            .program
            .evidence_items()
            .iter()
            .map(|ev| {
                serde_json::json!({
                    "id": ev.id,
                    "source": ev.source,
                    "summary": ev.summary,
                    "url": ev.url,
                    "relevance": ev.relevance,
                    "date": ev.date,
                    "strength": ev.strength,
                    "key_findings": ev.key_findings,
                })
            })
            .collect();

        // Same for agents_used. We capture every agent the program
        // currently declares, with the union of driver_refs each owns.
        let agents_used_json: Vec<serde_json::Value> = self
            .program
            .agents()
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "agent_type": a.agent_type,
                    "query": a.query,
                    "driver_refs": a.driver_refs,
                })
            })
            .collect();

        // Build a partial-update body. Only ship evidence + agents_used;
        // everything else stays whatever the server last knew. The PUT
        // handler uses COALESCE per column so absent keys leave the
        // existing values untouched.
        let body = serde_json::json!({
            "evidence": evidence_json,
            "agents_used": agents_used_json,
        });

        let api = self.api.clone();
        let fid = fid.clone();
        let n_evidence = evidence_json.len();
        let n_agents = agents_used_json.len();
        tokio::spawn(async move {
            match api.update_forecast(&fid, &body).await {
                Ok(_) => log::info!(
                    "[research-persist] forecast {} → {} evidence, {} agents",
                    fid,
                    n_evidence,
                    n_agents
                ),
                Err(e) => log::warn!(
                    "[research-persist] update_forecast failed for {}: {} \
                     — research will be lost if you close before publishing",
                    fid,
                    e
                ),
            }
        });
    }

    fn mark_agent_failed(&mut self, agent_name: &str, error: &str) {
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == agent_name)
        {
            run.status = AgentRunStatus::Failed;
            run.completed_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            run.error = Some(error.to_string());
        }
        self.messages.push(AssistantMessage {
            node: format!("agent:{}", agent_name),
            kind: MessageKind::Error,
            text: format!("Agent '{}' failed: {}", agent_name, error),
        });
        self.orchestration_running = false;
    }

    // ═══════════════════════════════════════════════════════════════
    // Validation — the Assistant checks the program for issues
    // ═══════════════════════════════════════════════════════════════

    fn run_validation_hints(&mut self) {
        // Check each driver for issues
        for driver in self.program.drivers() {
            match driver.driver_type {
                DriverType::Continuous => {
                    if let Some(ref dist) = driver.distribution {
                        if let Distribution::Triangular {
                            ref p5,
                            ref p50,
                            ref p95,
                        } = dist
                        {
                            let v5 = expr_to_f64(p5);
                            let v50 = expr_to_f64(p50);
                            let v95 = expr_to_f64(p95);
                            if v5 > v50 || v50 > v95 {
                                self.messages.push(AssistantMessage {
                                    node: format!("driver:{}", driver.name),
                                    kind: MessageKind::Warning,
                                    text: format!(
                                        "Driver '{}': p5 ({}) should be ≤ p50 ({}) ≤ p95 ({}). Your distribution may be backwards.",
                                        driver.name, v5, v50, v95
                                    ),
                                });
                            }
                            if v5 == 0.0 && v50 == 0.0 && v95 == 0.0 {
                                self.messages.push(AssistantMessage {
                                    node: format!("driver:{}", driver.name),
                                    kind: MessageKind::Suggestion,
                                    text: format!(
                                        "Driver '{}' has all zeros — set your estimates.",
                                        driver.name
                                    ),
                                });
                            }
                        }
                    } else {
                        self.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver.name),
                            kind: MessageKind::Warning,
                            text: format!(
                                "Driver '{}' has no distribution specified.",
                                driver.name
                            ),
                        });
                    }
                }
                DriverType::Binary => {
                    if driver.probability.is_none() {
                        self.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver.name),
                            kind: MessageKind::Suggestion,
                            text: format!("Driver '{}' needs a probability estimate.", driver.name),
                        });
                    }
                }
                _ => {}
            }
        }

        // Check if model references all drivers
        if self.program.model().is_none() && !self.program.drivers().is_empty() {
            self.messages.push(AssistantMessage {
                node: "model".into(),
                kind: MessageKind::Suggestion,
                text: "No model expression defined. The model determines how drivers combine."
                    .into(),
            });
        }

        // Check for agents
        let has_agents = self.program.drivers().iter().any(|d| {
            // In the current AST, agents are top-level, not embedded in drivers.
            // Check if any agent references this driver.
            self.program
                .agents()
                .iter()
                .any(|a| a.driver_refs.contains(&d.name))
        });
        if !has_agents && !self.program.drivers().is_empty() {
            self.messages.push(AssistantMessage {
                node: "agents".into(),
                kind: MessageKind::Tip,
                text: "🦊 No monitoring agents configured. Your forecast won't update with new evidence. Consider adding agents to key drivers.".into(),
            });
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Driver Editing
    // ═══════════════════════════════════════════════════════════════

    /// Open the research panel for a specific driver.
    /// Pre-fills the query input with a domain-specific suggested query
    /// based on the driver name, rationale, and forecast domain.
    /// No Fermi LLM call needed — the routing is deterministic.
    pub fn open_agent_picker(&mut self, driver_name: &str, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.agent_search_query.clear();
        self.focused_node = FocusedNode::AgentPicker(driver_name.to_string());
        self.right_tab = RightTab::Edit;

        // Pre-fill the query input with a domain-specific suggested query
        let driver = self.program.driver(driver_name);
        let driver_display = driver
            .and_then(|d| d.display_name.as_deref())
            .unwrap_or(driver_name)
            .to_string();
        let rationale = driver
            .and_then(|d| d.rationale.as_deref())
            .unwrap_or("")
            .to_string();
        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        let domain = detect_domain(&question);

        let (p5, p50, p95) = driver
            .and_then(|d| d.distribution.as_ref())
            .map(|dist| match dist {
                Distribution::Triangular { p5, p50, p95 } => {
                    (expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95))
                }
                _ => (0.8, 1.0, 1.2),
            })
            .unwrap_or((0.8, 1.0, 1.2));

        // Determine the recommended agent for this driver
        let dl = driver_name.to_lowercase();
        let rl = rationale.to_lowercase();
        let combined = format!("{} {}", dl, rl);
        let recommended = if combined.contains("sentiment") || combined.contains("opinion") {
            "sentiment_analyzer"
        } else if combined.contains("regulatory")
            || combined.contains("legal")
            || combined.contains("entity")
        {
            "entity_investigator"
        } else if combined.contains("market")
            || combined.contains("competition")
            || combined.contains("revenue")
            || combined.contains("commercial")
        {
            "market_research"
        } else if combined.contains("clinical")
            || combined.contains("trial")
            || combined.contains("fda")
        {
            "biotech_analyst"
        } else if combined.contains("nba") || combined.contains("basketball") {
            "nba_analyst"
        } else {
            match domain.as_str() {
                "sports_nba" | "basketball" => "nba_analyst",
                "biotech" | "pharma" => "biotech_analyst",
                "sports_football" | "sports_nfl" | "sports_other" => "football_analyst",
                "finance" | "stocks" => "macro_forecaster",
                "technology" => "market_research",
                _ => "macro_forecaster",
            }
        };

        // Generate the suggested query
        let suggested_query = formulate_research_query(
            &question,
            &driver_display,
            &rationale,
            recommended,
            &domain,
            p5,
            p50,
            p95,
        );

        // Pre-fill the query input so the user sees what will be asked
        self.agent_query_input.update(cx, |input, cx| {
            input.set_text(&suggested_query.replace('\n', " ").replace("  ", " "), cx)
        });

        // Clear the driver research input for fresh input
        self.driver_research_input
            .update(cx, |input, cx| input.set_text("", cx));

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!(
                "🔬 Research panel for '{}' — recommended: {} (edit query below to customize)",
                driver_display, recommended
            ),
        });

        cx.notify();
    }
    /// Load persisted schedules from the API and auto-fire any that are overdue.
    pub fn load_schedules(&mut self, cx: &mut Context<Self>) {
        let forecast_id = match &self.forecast_id {
            Some(id) => id.clone(),
            None => return,
        };
        let api = self.api.clone();
        self.schedules_loading = true;
        cx.spawn(async move |this, cx| {
            match api.list_forecast_schedules(&forecast_id).await {
                Ok(schedules) => {
                    let now = chrono::Utc::now();
                    let overdue: Vec<ForecastSchedule> = schedules
                        .iter()
                        .filter(|s| {
                            s.enabled
                                && chrono::DateTime::parse_from_rfc3339(&s.next_run_at)
                                    .map(|t| t.with_timezone(&chrono::Utc) <= now)
                                    .unwrap_or(false)
                        })
                        .cloned()
                        .collect();

                    this.update(cx, |state, cx| {
                        state.schedules = schedules;
                        state.schedules_loading = false;
                        for sched in &overdue {
                            let agent_id = sched.agent_id.clone();
                            let query = sched.query.clone();
                            state.fire_agent(&agent_id, &query, cx);
                            state.messages.push(AssistantMessage {
                                node: format!("driver:{}", sched.driver_name),
                                kind: MessageKind::Info,
                                text: format!(
                                    "⏰ Auto-running {} for '{}' (scheduled every {}h)",
                                    sched.agent_id, sched.driver_name, sched.interval_hours
                                ),
                            });
                        }
                        // Auto-persist any FPL-declared schedules that aren't
                        // yet on the server. Runs after the schedules list is
                        // populated so the dedup check inside
                        // fpl_declared_schedule_drafts works correctly.
                        state.auto_persist_fpl_schedules(cx);
                        cx.notify();
                    })
                    .ok();
                    // Record runs for overdue schedules
                    for sched in overdue {
                        let api2 = api.clone();
                        let fid = forecast_id.clone();
                        let sid = sched.id.clone();
                        tokio::spawn(async move {
                            if let Err(e) = api2.record_schedule_run(&fid, &sid).await {
                                log::warn!("[schedule] record_run failed: {}", e);
                            }
                        });
                    }
                }
                Err(e) => {
                    log::warn!("[schedule] load_schedules failed: {}", e);
                    this.update(cx, |state, cx| {
                        state.schedules_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Walk the program's agents and return one schedule-draft per
    /// `(agent, driver_ref)` pair that declares a recurring `Schedule::Every`
    /// but isn't yet persisted in `self.schedules`. This drives the
    /// Schedules tab's pre-population: when the FPL ships with a full set
    /// of agent blocks (as the WC team-prior template does), the operator
    /// sees every agent×driver pair laid out with its declared cadence and
    /// can batch-persist them instead of clicking through each driver in
    /// the program tree.
    ///
    /// Drafts are NOT auto-persisted. The user explicitly confirms either
    /// per-row ("Save") or in bulk ("Save all from FPL"). This matches the
    /// spec's "operator drives the demo" stance.
    pub fn fpl_declared_schedule_drafts(&self) -> Vec<ScheduleDraft> {
        let mut drafts: Vec<ScheduleDraft> = Vec::new();
        for agent in self.program.agents() {
            // Only recurring schedules pre-populate. Schedule::Once is a
            // one-shot fire from the UI; Schedule::Cron isn't surfaced
            // anywhere else in the UI yet either.
            let Some(Schedule::Every { interval, unit }) = agent.schedule.as_ref() else {
                continue;
            };
            let interval_hours: i32 = match unit {
                fermi::ast::TimeUnit::Minute => 1,
                fermi::ast::TimeUnit::Hour => *interval as i32,
                fermi::ast::TimeUnit::Day => *interval as i32 * 24,
                fermi::ast::TimeUnit::Week => *interval as i32 * 168,
                fermi::ast::TimeUnit::Month => *interval as i32 * 720,
            };
            for driver_name in &agent.driver_refs {
                // Skip if this exact (agent, driver) pair is already
                // persisted on the server — we don't want to nag the user
                // to re-save schedules that exist.
                let already_persisted = self
                    .schedules
                    .iter()
                    .any(|s| s.agent_id == agent.name && s.driver_name == *driver_name);
                if already_persisted {
                    continue;
                }
                drafts.push(ScheduleDraft {
                    agent_id: agent.name.clone(),
                    driver_name: driver_name.clone(),
                    query: agent.query.clone(),
                    interval_hours,
                });
            }
        }
        // Stable order by driver then agent so the list doesn't jump around
        // between renders.
        drafts.sort_by(|a, b| {
            a.driver_name
                .cmp(&b.driver_name)
                .then(a.agent_id.cmp(&b.agent_id))
        });
        drafts
    }

    /// Persist a single schedule draft via the same upsert path the
    /// per-driver 📅 buttons use. On success refreshes `self.schedules` so
    /// the row migrates from the "drafts from FPL" section into the
    /// persisted-schedules list.
    pub fn save_schedule_draft(&mut self, draft: ScheduleDraft, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            self.messages.push(AssistantMessage {
                node: "schedule".into(),
                kind: MessageKind::Warning,
                text: "Publish this forecast first (Ctrl+P) before persisting schedules.".into(),
            });
            cx.notify();
            return;
        };
        let api = self.api.clone();
        let req = UpsertScheduleRequest {
            agent_id: draft.agent_id.clone(),
            driver_name: draft.driver_name.clone(),
            query: draft.query.clone(),
            interval_hours: draft.interval_hours,
        };
        cx.spawn(
            async move |this, cx| match api.upsert_forecast_schedule(&fid, &req).await {
                Ok(_) => {
                    this.update(cx, |state, cx| {
                        state.messages.push(AssistantMessage {
                            node: format!("driver:{}", req.driver_name),
                            kind: MessageKind::Info,
                            text: format!(
                                "Saved {} on '{}' (every {}h).",
                                req.agent_id, req.driver_name, req.interval_hours
                            ),
                        });
                        state.load_schedules(cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("[schedule] save_draft failed: {}", e);
                    this.update(cx, |state, cx| {
                        state.messages.push(AssistantMessage {
                            node: "schedule".into(),
                            kind: MessageKind::Error,
                            text: format!("Failed to save {}: {}", req.agent_id, e),
                        });
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }

    /// Change the cadence of an already-persisted schedule. The
    /// upsert_forecast_schedule endpoint is keyed by
    /// (forecast_id, agent_id, driver_name), so re-upserting with a
    /// different interval_hours updates the row in place. Used by the
    /// Schedules tab's per-row Daily/Weekly buttons so the operator can
    /// adjust cadence without dropping into the Edit panel.
    pub fn change_schedule_interval(
        &mut self,
        schedule_id: &str,
        new_interval_hours: i32,
        cx: &mut Context<Self>,
    ) {
        let Some(sched) = self.schedules.iter().find(|s| s.id == schedule_id).cloned() else {
            return;
        };
        let Some(fid) = self.forecast_id.clone() else {
            return;
        };
        let api = self.api.clone();
        let req = UpsertScheduleRequest {
            agent_id: sched.agent_id.clone(),
            driver_name: sched.driver_name.clone(),
            query: sched.query.clone(),
            interval_hours: new_interval_hours,
        };
        let label = if new_interval_hours >= 168 {
            format!("every {} week", new_interval_hours / 168)
        } else if new_interval_hours >= 24 {
            format!("every {} day", new_interval_hours / 24)
        } else {
            format!("every {}h", new_interval_hours)
        };
        // Retry the upsert up to 3 times with exponential backoff —
        // observed 502s on this endpoint during cadence flips are
        // typically transient (Railway under load / metrics-trigger
        // contention). Without retry the user sees a red 'Failed to
        // update X' and has to click again, which is bad UX for a
        // batch-of-six cadence flip.
        cx.spawn(async move |this, cx| {
            const MAX_ATTEMPTS: u32 = 3;
            let mut last_err: Option<String> = None;
            for attempt in 0..MAX_ATTEMPTS {
                match api.upsert_forecast_schedule(&fid, &req).await {
                    Ok(_) => {
                        this.update(cx, |state, cx| {
                            state.messages.push(AssistantMessage {
                                node: format!("driver:{}", req.driver_name),
                                kind: MessageKind::Info,
                                text: format!(
                                    "Updated {} → {} ({}){}.",
                                    req.agent_id,
                                    req.driver_name,
                                    label,
                                    if attempt > 0 {
                                        format!(" [retry #{}]", attempt)
                                    } else {
                                        String::new()
                                    }
                                ),
                            });
                            state.load_schedules(cx);
                            cx.notify();
                        })
                        .ok();
                        return;
                    }
                    Err(e) => {
                        last_err = Some(e.to_string());
                        log::warn!(
                            "[schedule] change_interval attempt {} failed: {}",
                            attempt + 1,
                            e
                        );
                        // Exponential backoff: 250ms, 750ms, 1750ms.
                        if attempt + 1 < MAX_ATTEMPTS {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                250 * (3u64.pow(attempt)),
                            ))
                            .await;
                        }
                    }
                }
            }
            this.update(cx, |state, cx| {
                state.messages.push(AssistantMessage {
                    node: "schedule".into(),
                    kind: MessageKind::Error,
                    text: format!(
                        "Failed to update {} after {} attempts: {}",
                        req.agent_id,
                        MAX_ATTEMPTS,
                        last_err.unwrap_or_default()
                    ),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Batch-save every FPL-declared schedule draft that isn't yet
    /// persisted. Sequential upserts (not concurrent) so the messages
    /// surface in order and a partial failure doesn't leave the user
    /// uncertain about which saved.
    pub fn save_all_schedule_drafts(&mut self, cx: &mut Context<Self>) {
        let drafts = self.fpl_declared_schedule_drafts();
        if drafts.is_empty() {
            return;
        }
        let Some(fid) = self.forecast_id.clone() else {
            self.messages.push(AssistantMessage {
                node: "schedule".into(),
                kind: MessageKind::Warning,
                text: "Publish this forecast first (Ctrl+P) before persisting schedules.".into(),
            });
            cx.notify();
            return;
        };
        let api = self.api.clone();
        let n = drafts.len();
        cx.spawn(async move |this, cx| {
            let mut saved = 0usize;
            let mut failed: Vec<String> = Vec::new();
            for draft in &drafts {
                let req = UpsertScheduleRequest {
                    agent_id: draft.agent_id.clone(),
                    driver_name: draft.driver_name.clone(),
                    query: draft.query.clone(),
                    interval_hours: draft.interval_hours,
                };
                match api.upsert_forecast_schedule(&fid, &req).await {
                    Ok(_) => saved += 1,
                    Err(e) => failed.push(format!("{}: {}", draft.agent_id, e)),
                }
            }
            this.update(cx, |state, cx| {
                state.messages.push(AssistantMessage {
                    node: "schedule".into(),
                    kind: if failed.is_empty() {
                        MessageKind::Info
                    } else {
                        MessageKind::Warning
                    },
                    text: format!(
                        "Saved {}/{} FPL-declared schedules.{}",
                        saved,
                        n,
                        if failed.is_empty() {
                            String::new()
                        } else {
                            format!(" Failures: {}.", failed.join(", "))
                        }
                    ),
                });
                state.load_schedules(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ─── BayesOps R-2: pending fits ────────────────────────────────────
    //
    // Fetch the server's view of "what fits are pending for this workspace,
    // per driver." The render path consults `self.bayesops_pending` to
    // decide whether to show the inline accept/dismiss affordance on a
    // driver's sparkline badge.
    //
    // Called on workspace-mount, after every refit, and whenever a
    // `bayesops_fit_pending` event lands in the workspace messages.

    /// On workspace mount, after `load_schedules` populates
    /// `self.schedules`, automatically persist any FPL-declared
    /// agent×driver pair that doesn't yet have a server-side schedule
    /// row. Persisted with `interval_hours = 0` (On-demand) so the
    /// operator opts in to autonomous runs explicitly via the cadence
    /// buttons — the FPL's declared cadence becomes a *suggestion* rather
    /// than an obligation.
    ///
    /// Without this auto-persist, the operator's first action on every
    /// new workspace was clicking "Save" six times in the Schedules tab,
    /// which adds zero information (the FPL already declared which
    /// agents own which drivers).
    ///
    /// Idempotent: fpl_declared_schedule_drafts excludes pairs already
    /// persisted, so this is a no-op on the second mount.
    pub fn auto_persist_fpl_schedules(&mut self, cx: &mut Context<Self>) {
        let drafts = self.fpl_declared_schedule_drafts();
        if drafts.is_empty() {
            return;
        }
        let Some(fid) = self.forecast_id.clone() else {
            return;
        };
        let api = self.api.clone();
        let n = drafts.len();
        cx.spawn(async move |this, cx| {
            let mut saved = 0usize;
            for draft in &drafts {
                let req = UpsertScheduleRequest {
                    agent_id: draft.agent_id.clone(),
                    driver_name: draft.driver_name.clone(),
                    query: draft.query.clone(),
                    // On-demand: server stores the row but next_run_at is
                    // a year-3000 sentinel so the overdue check never
                    // fires. Operator picks Daily/Weekly/Monthly per row
                    // when they want autonomous runs.
                    interval_hours: 0,
                };
                if api.upsert_forecast_schedule(&fid, &req).await.is_ok() {
                    saved += 1;
                }
            }
            this.update(cx, |state, cx| {
                if saved > 0 {
                    state.messages.push(AssistantMessage {
                        node: "schedule".into(),
                        kind: MessageKind::Info,
                        text: format!(
                            "Pre-loaded {}/{} agent×driver schedules from FPL (On-demand). \
                             Use the cadence buttons in the Schedules tab to enable autonomous runs.",
                            saved, n
                        ),
                    });
                }
                state.load_schedules(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch the workspace's `params` output from the server and store it
    /// on `self.workspace_params`. Called on workspace mount, after every
    /// BayesOps accept (so a freshly-fitted distribution flows into the
    /// next sim), and after any local set_workspace_output mutation.
    ///
    /// The params object holds two distinct shapes:
    ///   - Scalar bindings written by the spawn script: `elo_current`,
    ///     `gdp_per_capita_log`, etc. The Executor picks these up via
    ///     `set_params`.
    ///   - JSON-typed fitted distributions written by accept-pending:
    ///     `<driver>_fitted`. The Executor picks these up via
    ///     `set_json_params`. Read-side is `fitted_distribution_for`.
    pub fn load_workspace_params(&mut self, cx: &mut Context<Self>) {
        let Some(ref ws_id) = self.workspace_id else {
            return;
        };
        let ws_id = ws_id.clone();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            match api.get_workspace_output(&ws_id, "params").await {
                Ok(resp) => {
                    // The handler returns either {"value": <obj>} or the
                    // raw object directly depending on whether the row
                    // exists. Tolerate both shapes.
                    let value = resp.get("value").cloned().unwrap_or(resp);
                    let map = value.as_object().cloned().unwrap_or_default();
                    let n = map.len();
                    this.update(cx, |state, cx| {
                        state.workspace_params = map;
                        log::info!("[workspace-params] loaded {} keys for {}", n, ws_id);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!(
                        "[workspace-params] load failed for {}: {} \
                         — sim will fall back to empty param context",
                        ws_id,
                        e
                    );
                }
            }
        })
        .detach();
    }

    pub fn load_bayesops_state(&mut self, cx: &mut Context<Self>) {
        let Some(ref ws_id) = self.workspace_id else {
            return;
        };
        let ws_id = ws_id.clone();
        let api = self.api.clone();
        cx.spawn(
            async move |this, cx| match api.workspace_bayesops_state(&ws_id).await {
                Ok(state_value) => {
                    let drivers = state_value
                        .get("drivers")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    let mut pending: std::collections::HashMap<String, PendingFitState> =
                        std::collections::HashMap::new();
                    for d in drivers {
                        let driver_name = d
                            .get("driver_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if driver_name.is_empty() {
                            continue;
                        }
                        if let Some(p) = d.get("pending_fit") {
                            if p.is_null() {
                                continue;
                            }
                            let pending_id = p
                                .get("pending_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if pending_id.is_empty() {
                                continue;
                            }
                            let n_observations =
                                p.get("n_observations")
                                    .and_then(|v| v.as_i64())
                                    .unwrap_or(0) as i32;
                            let n_eff = p.get("n_eff").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let ci_width =
                                p.get("ci_width").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let delta_pp = p.get("delta_pp").and_then(|v| v.as_f64());
                            pending.insert(
                                driver_name.clone(),
                                PendingFitState {
                                    driver_name,
                                    pending_id,
                                    n_observations,
                                    n_eff,
                                    ci_width,
                                    delta_pp,
                                },
                            );
                        }
                    }
                    this.update(cx, |state, cx| {
                        state.bayesops_pending = pending;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("[bayesops] load_bayesops_state failed: {}", e);
                }
            },
        )
        .detach();
    }

    pub fn accept_bayesops_pending(
        &mut self,
        driver_name: &str,
        pending_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.bayesops_decisions_in_flight.contains(driver_name) {
            return; // ignore double-clicks
        }
        self.bayesops_decisions_in_flight
            .insert(driver_name.to_string());
        let driver_name = driver_name.to_string();
        let pending_id = pending_id.to_string();
        let api = self.api.clone();
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!("✓ Accepting fit for '{}'…", driver_name),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = api.accept_pending_fit(&pending_id, None).await;
            this.update(cx, |state, cx| {
                state.bayesops_decisions_in_flight.remove(&driver_name);
                match result {
                    Ok(_) => {
                        state.bayesops_pending.remove(&driver_name);
                        state.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver_name),
                            kind: MessageKind::Info,
                            text: format!(
                                "✓ Fit accepted for '{}'. Run the forecast to apply.",
                                driver_name
                            ),
                        });
                        // Re-fetch workspace params so the freshly-written
                        // `<driver>_fitted` lands in self.workspace_params
                        // and the very next Ctrl+R picks it up via
                        // set_json_params. Without this the user has to
                        // close + reopen to see the fit's effect.
                        state.load_workspace_params(cx);
                    }
                    Err(e) => {
                        state.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver_name),
                            kind: MessageKind::Error,
                            text: format!("Failed to accept fit for '{}': {}", driver_name, e),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn reject_bayesops_pending(
        &mut self,
        driver_name: &str,
        pending_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.bayesops_decisions_in_flight.contains(driver_name) {
            return;
        }
        self.bayesops_decisions_in_flight
            .insert(driver_name.to_string());
        let driver_name = driver_name.to_string();
        let pending_id = pending_id.to_string();
        let api = self.api.clone();
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!("✗ Dismissing fit for '{}'…", driver_name),
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = api.reject_pending_fit(&pending_id, None).await;
            this.update(cx, |state, cx| {
                state.bayesops_decisions_in_flight.remove(&driver_name);
                match result {
                    Ok(_) => {
                        state.bayesops_pending.remove(&driver_name);
                        state.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver_name),
                            kind: MessageKind::Info,
                            text: format!("✗ Fit dismissed for '{}'.", driver_name),
                        });
                    }
                    Err(e) => {
                        state.messages.push(AssistantMessage {
                            node: format!("driver:{}", driver_name),
                            kind: MessageKind::Error,
                            text: format!("Failed to dismiss fit for '{}': {}", driver_name, e),
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ─── R-3 Trajectory: timeline fetch ────────────────────────────────
    //
    // Pulls the unified event stream for this forecast from
    // /api/forecasts/:id/timeline. Called when the user clicks the
    // Trajectory tab; the response is cached in `timeline_data` so
    // re-renders don't re-fetch.

    pub fn load_timeline(&mut self, cx: &mut Context<Self>) {
        let Some(forecast_id) = self.forecast_id.clone() else {
            self.timeline_error =
                Some("Save the forecast first to see its trajectory.".to_string());
            cx.notify();
            return;
        };
        self.timeline_loading = true;
        self.timeline_error = None;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.forecast_timeline(&forecast_id).await;
            this.update(cx, |state, cx| {
                state.timeline_loading = false;
                match result {
                    Ok(data) => {
                        state.timeline_data = Some(data);
                        state.timeline_error = None;
                    }
                    Err(e) => {
                        state.timeline_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Manually trigger a scheduled agent now and bump its next_run_at.
    pub fn run_now_schedule(&mut self, schedule_id: &str, cx: &mut Context<Self>) {
        let sid = schedule_id.to_string();
        let sched = match self.schedules.iter().find(|s| s.id == sid) {
            Some(s) => s.clone(),
            None => return,
        };
        let forecast_id = match &self.forecast_id {
            Some(id) => id.clone(),
            None => return,
        };

        self.fire_agent(&sched.agent_id, &sched.query, cx);
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", sched.driver_name),
            kind: MessageKind::Info,
            text: format!("▶ Running {} for '{}'…", sched.agent_id, sched.driver_name),
        });

        let api = self.api.clone();
        let fid = forecast_id.clone();
        cx.spawn(async move |this, cx| {
            if let Err(e) = api.record_schedule_run(&fid, &sid).await {
                log::warn!("[schedule] record_run failed: {}", e);
            }
            // Reload schedules to reflect updated next_run_at
            this.update(cx, |state, cx| {
                state.load_schedules(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Delete a persisted schedule.
    pub fn delete_schedule(&mut self, schedule_id: &str, cx: &mut Context<Self>) {
        let sid = schedule_id.to_string();
        let forecast_id = match &self.forecast_id {
            Some(id) => id.clone(),
            None => return,
        };
        self.schedules.retain(|s| s.id != sid);
        let api = self.api.clone();
        cx.spawn(async move |_, _| {
            if let Err(e) = api.delete_forecast_schedule(&forecast_id, &sid).await {
                log::warn!("[schedule] delete failed: {}", e);
            }
        })
        .detach();
        cx.notify();
    }

    pub fn assign_agent_to_driver(
        &mut self,
        driver_name: &str,
        agent_id: &str,
        schedule: Schedule,
        cx: &mut Context<Self>,
    ) {
        let question_text = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();

        // Use custom query from input, or generate a default
        let custom_query = self.agent_query_input.read(cx).text().to_string();
        let query = if custom_query.trim().is_empty() {
            format!(
                "Research evidence for the '{}' driver in the forecast: \"{}\"",
                driver_name, question_text
            )
        } else {
            custom_query
        };

        let schedule_label = match &schedule {
            Schedule::Once => "once".to_string(),
            Schedule::Every { interval, unit } => format!("every {} {:?}", interval, unit),
            Schedule::Cron(c) => format!("cron: {}", c),
        };

        // Extract interval before schedule is moved into AST
        let interval_hours: Option<i32> = match &schedule {
            Schedule::Every { interval, unit } => {
                let h = match unit {
                    fermi::ast::TimeUnit::Minute => 1,
                    fermi::ast::TimeUnit::Hour => *interval as i32,
                    fermi::ast::TimeUnit::Day => *interval as i32 * 24,
                    fermi::ast::TimeUnit::Week => *interval as i32 * 168,
                    fermi::ast::TimeUnit::Month => *interval as i32 * 720,
                };
                Some(h)
            }
            _ => None,
        };

        self.program.add_agent(AgentStmt {
            name: format!("{}_{}", agent_id, sanitize_name(driver_name)),
            agent_type: Some("research".into()),
            query: query.clone(),
            executor: Some(fermi::ast::ExecutorType::LLM),
            schedule: Some(schedule),
            driver_refs: vec![driver_name.to_string()],
            depends_on: vec![],
            confidence_threshold: None,
        });

        self.agent_runs.push(AgentExecution {
            agent_name: format!("{}_{}", agent_id, sanitize_name(driver_name)),
            status: AgentRunStatus::Running,
            evidence_count: 0,
            confidence: None,
            error: None,
            credits_charged: None,
            started_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            completed_at: None,
            latest_finding: None,
        });

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!(
                "Agent '{}' assigned to '{}' (schedule: {}) — researching now.",
                agent_id, driver_name, schedule_label
            ),
        });

        self.fire_agent(agent_id, &query, cx);

        // Persist recurring schedules to the backend (Once is fire-and-forget)
        if let (Some(fid), Some(hours)) = (self.forecast_id.clone(), interval_hours) {
            let req = UpsertScheduleRequest {
                agent_id: agent_id.to_string(),
                driver_name: driver_name.to_string(),
                query: query.clone(),
                interval_hours: hours,
            };
            let api = self.api.clone();
            cx.spawn(
                async move |this, cx| match api.upsert_forecast_schedule(&fid, &req).await {
                    Ok(_) => {
                        this.update(cx, |state, cx| state.load_schedules(cx)).ok();
                    }
                    Err(e) => log::warn!("[schedule] upsert failed: {}", e),
                },
            )
            .detach();
        }

        self.focused_node = FocusedNode::Driver(driver_name.to_string());
        self.populate_editor_from_driver(driver_name, cx);
        cx.notify();
    }

    /// Update the schedule for an agent that is ALREADY bound to a driver.
    ///
    /// `base_agent_id` is the registry id (e.g. "football_analyst"), NOT the
    /// bound AST agent name (e.g. "football_analyst_argentina_elo_squad_strength").
    /// The schedule API is keyed on (forecast_id, driver_name, base_agent_id),
    /// so passing the bound name produces a wrong, duplicated entry.
    ///
    /// Does NOT add a new AgentStmt — that's what `assign_agent_to_driver`
    /// is for. This is the right call when the user clicks ▶/📅/📅 on an
    /// already-attached agent.
    ///
    /// Schedule::Once → triggers an immediate re-run via fire_agent + clears
    /// any persisted recurring schedule.
    /// Schedule::Every / Cron → upserts the persisted schedule. Does not
    /// fire immediately — the next scheduled invocation will pick it up.
    pub fn update_schedule_for_assigned_agent(
        &mut self,
        driver_name: &str,
        base_agent_id: &str,
        schedule: Schedule,
        cx: &mut Context<Self>,
    ) {
        let bound_name = format!("{}_{}", base_agent_id, sanitize_name(driver_name));

        // Update the in-memory AST schedule on the bound agent (if it exists).
        // This keeps the FPL source in sync — generate_fpl_text reads from
        // the agents list. If for some reason the bound agent isn't in the
        // AST (e.g. orphaned schedule from a previous session), we skip the
        // AST update silently rather than create a new agent.
        if let Some(a) = self.program.agent_mut(&bound_name) {
            a.schedule = Some(schedule.clone());
        } else {
            log::warn!(
                "[schedule] update_schedule: bound agent '{}' not in AST — skipping AST mutation",
                bound_name
            );
        }

        // Persist or fire based on schedule kind.
        match &schedule {
            Schedule::Once => {
                // Fire immediately. Use the stored query if we can find it,
                // otherwise build a generic one so the button still does
                // something useful.
                let query = self
                    .program
                    .agent(&bound_name)
                    .map(|a| a.query.clone())
                    .unwrap_or_else(|| {
                        let q_text = self
                            .program
                            .question()
                            .map(|q| q.text.clone())
                            .unwrap_or_default();
                        format!(
                            "Research evidence for the '{}' driver in the forecast: \"{}\"",
                            driver_name, q_text
                        )
                    });
                self.fire_agent(base_agent_id, &query, cx);
                self.messages.push(AssistantMessage {
                    node: format!("driver:{}", driver_name),
                    kind: MessageKind::Info,
                    text: format!(
                        "Agent '{}' re-running for '{}' (one-shot).",
                        base_agent_id, driver_name
                    ),
                });
            }
            Schedule::Every { interval, unit } => {
                let interval_hours: i32 = match unit {
                    fermi::ast::TimeUnit::Minute => 1,
                    fermi::ast::TimeUnit::Hour => *interval as i32,
                    fermi::ast::TimeUnit::Day => *interval as i32 * 24,
                    fermi::ast::TimeUnit::Week => *interval as i32 * 168,
                    fermi::ast::TimeUnit::Month => *interval as i32 * 720,
                };
                let query = self
                    .program
                    .agent(&bound_name)
                    .map(|a| a.query.clone())
                    .unwrap_or_default();
                let cadence = format!("every {} {:?}", interval, unit);
                self.messages.push(AssistantMessage {
                    node: format!("driver:{}", driver_name),
                    kind: MessageKind::Info,
                    text: format!(
                        "Agent '{}' on '{}': schedule updated to {}.",
                        base_agent_id, driver_name, cadence
                    ),
                });
                if let Some(fid) = self.forecast_id.clone() {
                    let req = UpsertScheduleRequest {
                        agent_id: base_agent_id.to_string(),
                        driver_name: driver_name.to_string(),
                        query,
                        interval_hours,
                    };
                    let api = self.api.clone();
                    cx.spawn(async move |this, cx| {
                        match api.upsert_forecast_schedule(&fid, &req).await {
                            Ok(_) => {
                                this.update(cx, |state, cx| state.load_schedules(cx)).ok();
                            }
                            Err(e) => log::warn!("[schedule] upsert failed: {}", e),
                        }
                    })
                    .detach();
                } else {
                    log::info!("[schedule] no forecast_id yet — schedule only persisted in AST");
                }
            }
            Schedule::Cron(_) => {
                // Cron schedules go through the same upsert path as Every,
                // but the UI doesn't expose them today. Stub for future.
                log::warn!("[schedule] cron schedules not yet supported in UI");
            }
        }
        cx.notify();
    }

    /// Re-run a previously completed or failed agent using its stored query.
    pub fn retry_agent(&mut self, agent_name: &str, cx: &mut Context<Self>) {
        // Look up the agent in the AST to get its query
        let agent_stmt = self
            .program
            .agents()
            .iter()
            .find(|a| a.name == agent_name)
            .cloned();
        let query = match agent_stmt {
            Some(ref a) => a.query.clone(),
            None => {
                log::warn!("[composer] retry_agent: {} not found in AST", agent_name);
                return;
            }
        };

        // Reset the execution state
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == agent_name)
        {
            run.status = AgentRunStatus::Running;
            run.error = None;
            run.started_at = Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            );
            run.completed_at = None;
            run.latest_finding = None;
        } else {
            // No existing run — create one
            self.agent_runs.push(AgentExecution {
                agent_name: agent_name.to_string(),
                status: AgentRunStatus::Running,
                evidence_count: 0,
                confidence: None,
                error: None,
                credits_charged: None,
                started_at: Some(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                ),
                completed_at: None,
                latest_finding: None,
            });
        }

        self.messages.push(AssistantMessage {
            node: format!("agent:{}", agent_name),
            kind: MessageKind::Info,
            text: format!("⟳ Re-running {}…", agent_name),
        });

        let base_id = base_agent_name(agent_name).to_string();
        self.fire_agent(&base_id, &query, cx);
        cx.notify();
    }

    /// Helper to populate editor fields from a driver name.
    fn populate_editor_from_driver(&self, name: &str, cx: &mut Context<Self>) {
        if let Some(driver) = self.program.driver(name) {
            let d_name = driver.name.clone();
            let d_rationale = driver.rationale.clone().unwrap_or_default();
            let d_unit = driver.unit.clone().unwrap_or_default();
            let d_type = driver.driver_type.clone();
            // For parameterized distributions (Triangular(Identifier(p5_name),
            // Identifier(p50_name), Identifier(p95_name)) — what the
            // post-Option-2 WC template uses) the AST literal is just the
            // identifier name. We have to look up the actual numeric
            // value in self.workspace_params, which gets populated by
            // load_workspace_params on workspace mount + after every
            // Apply / BayesOps accept. For literal-args distributions
            // (legacy hand-authored FPLs) we still read directly via
            // expr_to_f64.
            let resolve = |expr: &Expression| -> f64 {
                match expr {
                    Expression::Number(n) => *n,
                    Expression::Identifier(param_name) => self
                        .workspace_params
                        .get(param_name)
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    _ => 0.0,
                }
            };
            let (d_p5, d_p50, d_p95) = driver
                .distribution
                .as_ref()
                .map(|d| match d {
                    Distribution::Triangular { p5, p50, p95 } => {
                        (resolve(p5), resolve(p50), resolve(p95))
                    }
                    _ => (0.0, 0.0, 0.0),
                })
                .unwrap_or((0.0, 0.0, 0.0));
            let d_prob = driver.probability.unwrap_or(0.5);
            let d_impact = driver.impact_multiplier.unwrap_or(1.3);

            self.editor_name
                .update(cx, |input, cx| input.set_text(d_name, cx));
            self.editor_rationale
                .update(cx, |input, cx| input.set_text(d_rationale, cx));

            match d_type {
                DriverType::Continuous => {
                    self.editor_p5
                        .update(cx, |input, cx| input.set_text(format!("{}", d_p5), cx));
                    self.editor_p50
                        .update(cx, |input, cx| input.set_text(format!("{}", d_p50), cx));
                    self.editor_p95
                        .update(cx, |input, cx| input.set_text(format!("{}", d_p95), cx));
                    self.editor_unit
                        .update(cx, |input, cx| input.set_text(d_unit, cx));
                }
                DriverType::Binary => {
                    self.editor_prob
                        .update(cx, |input, cx| input.set_text(format!("{}", d_prob), cx));
                    self.editor_impact
                        .update(cx, |input, cx| input.set_text(format!("{}", d_impact), cx));
                }
                _ => {}
            }

            // Populate confidence from user-set value (or empty if not set)
            let conf_text = self
                .driver_confidence
                .get(&driver.name)
                .map(|c| format!("{:.0}", c * 100.0))
                .unwrap_or_default();
            self.editor_confidence
                .update(cx, |input, cx| input.set_text(conf_text, cx));
        }
    }

    /// Focus on a driver for editing. Populates the shared editor fields.
    pub fn focus_driver(&mut self, name: &str, cx: &mut Context<Self>) {
        // Don't override if we're in the agent picker for this driver
        // (the click bubbles from the "+ agent" button)
        if let FocusedNode::AgentPicker(ref dn) = self.focused_node {
            if dn == name {
                return;
            }
        }
        self.save_focused_driver(cx); // save previous
        self.focused_node = FocusedNode::Driver(name.to_string());

        self.populate_editor_from_driver(name, cx);

        // Auto-switch to the Edit tab. The previous behaviour required an
        // extra click to see the populated p5/p50/p95 + suggestions + agent
        // controls, which made driver-clicks feel half-wired. The Edit tab
        // is the canonical destination for "I picked a driver, show me what
        // matters about it" — auto-routing here matches operator intent.
        //
        // Exception: if the user is mid-trajectory or mid-Wiki review, we
        // still switch — they explicitly clicked a driver, which is a stronger
        // signal than "stay on the current tab." The prior tab is one click
        // away if they want it back.
        self.right_tab = RightTab::Edit;

        cx.notify();
    }

    /// Delete a driver from the program.
    /// Add manual evidence to the currently focused driver.
    /// Enrich a user's rough research question into a structured agent query.
    /// Takes "How deep is Bayern's squad?" and produces a detailed query with
    /// context from the forecast question, driver rationale, and current params.
    pub fn enrich_driver_query(&self, driver_name: &str, user_question: &str, cx: &App) -> String {
        let driver = self.program.driver(driver_name);
        let driver_display = driver
            .and_then(|d| d.display_name.as_deref())
            .unwrap_or(driver_name);
        let rationale = driver.and_then(|d| d.rationale.as_deref()).unwrap_or("");
        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        let (p5, p50, p95) = driver
            .and_then(|d| d.distribution.as_ref())
            .map(|dist| match dist {
                Distribution::Triangular { p5, p50, p95 } => {
                    (expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95))
                }
                _ => (0.8, 1.0, 1.2),
            })
            .unwrap_or((0.8, 1.0, 1.2));

        format!(
            "For the forecast: \"{question}\"\n\
             Driver: '{driver_display}' (p5={p5:.2}, p50={p50:.2}, p95={p95:.2})\n\
             Context: {rationale}\n\n\
             USER'S SPECIFIC QUESTION: {user_question}\n\n\
             Research this specific question in depth. Provide:\n\
             1. Specific data points and facts that answer the user's question\n\
             2. How this evidence should adjust the driver's p50 multiplier\n\
             3. Sources and dates for your data\n\
             4. Confidence (0.0-1.0) in your findings"
        )
    }

    /// Ingest a URL as evidence — fires an agent to fetch, summarize, and
    /// suggest how it impacts the driver's probability.
    pub fn ingest_url_evidence(&mut self, driver_name: &str, url: &str, cx: &mut Context<Self>) {
        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        let driver = self.program.driver(driver_name);
        let driver_display = driver
            .and_then(|d| d.display_name.as_deref())
            .unwrap_or(driver_name)
            .to_string();
        let rationale = driver
            .and_then(|d| d.rationale.as_deref())
            .unwrap_or("")
            .to_string();
        let (_, p50, _) = driver
            .and_then(|d| d.distribution.as_ref())
            .map(|dist| match dist {
                Distribution::Triangular { p5, p50, p95 } => {
                    (expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95))
                }
                _ => (0.8, 1.0, 1.2),
            })
            .unwrap_or((0.8, 1.0, 1.2));

        let query = format!(
            "For the forecast: \"{question}\"\n\
             Driver: '{driver_display}' (current p50={p50:.2})\n\
             Context: {rationale}\n\n\
             The user has provided this URL as evidence: {url}\n\n\
             TASKS:\n\
             1. Analyze the content at this URL (use your knowledge of what this source typically contains)\n\
             2. Summarize the key findings relevant to the '{driver_display}' driver\n\
             3. Assess how this evidence should adjust the p50 multiplier\n\
             4. Provide a suggested new p50 value with reasoning\n\
             5. Rate the evidence quality (0.0-1.0) based on source reliability and relevance"
        );

        let compound = format!("market_research_{}", sanitize_name(driver_name));
        self.program.add_agent(AgentStmt {
            name: compound.clone(),
            agent_type: Some("research".into()),
            query: query.clone(),
            executor: Some(fermi::ast::ExecutorType::LLM),
            schedule: Some(Schedule::Once),
            driver_refs: vec![driver_name.to_string()],
            depends_on: vec![],
            confidence_threshold: None,
        });
        self.agent_runs.push(AgentExecution {
            agent_name: compound,
            status: AgentRunStatus::Running,
            evidence_count: 0,
            confidence: None,
            error: None,
            credits_charged: None,
            started_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            completed_at: None,
            latest_finding: None,
        });

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!("📎 Analyzing URL for '{}': {}…", driver_display, url),
        });

        self.fire_agent("market_research", &query, cx);
        cx.notify();
    }

    pub fn add_manual_evidence(&mut self, cx: &mut Context<Self>) {
        let driver_name = match &self.focused_node {
            FocusedNode::Driver(n) => n.clone(),
            FocusedNode::AgentPicker(n) => n.clone(),
            _ => return,
        };

        let source = self.evidence_source_input.read(cx).text().to_string();
        let summary = self.evidence_summary_input.read(cx).text().to_string();

        if source.trim().is_empty() && summary.trim().is_empty() {
            return;
        }

        // Auto-detect URLs — if source looks like a URL, trigger agent analysis
        let is_url = source.contains("http://") || source.contains("https://");
        if is_url {
            // Fire agent to analyze the URL and suggest impact
            self.ingest_url_evidence(&driver_name, &source, cx);

            // Also add as manual evidence immediately so user sees it
            let ev_id = format!(
                "url_{}_{}",
                sanitize_name(&driver_name),
                self.program.evidence_items().len()
            );
            self.program.add_evidence(EvidenceStmt {
                id: ev_id,
                source: source.clone(),
                summary: if summary.is_empty() {
                    Some(format!("🔗 {} — agent analyzing…", source))
                } else {
                    Some(summary.clone())
                },
                url: Some(source.clone()),
                relevance: Some(0.5),
                date: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
                strength: Some(0.5),
                key_findings: vec![
                    "Agent analysis pending — results will update this evidence".into()
                ],
            });

            self.evidence_source_input
                .update(cx, |input, cx| input.set_text("", cx));
            self.evidence_summary_input
                .update(cx, |input, cx| input.set_text("", cx));

            self.messages.push(AssistantMessage {
                node: format!("driver:{}", driver_name),
                kind: MessageKind::Info,
                text: format!(
                    "🔗 URL added to '{}' — agent analyzing for impact…",
                    driver_name
                ),
            });
            cx.notify();
            return;
        }

        let ev_id = format!(
            "manual_{}_{}",
            sanitize_name(&driver_name),
            self.program.evidence_items().len()
        );

        self.program.add_evidence(EvidenceStmt {
            id: ev_id,
            source: if source.is_empty() {
                "Manual entry".into()
            } else {
                source
            },
            summary: if summary.is_empty() {
                None
            } else {
                Some(summary)
            },
            url: None,
            relevance: Some(0.7),
            date: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
            strength: Some(0.7),
            key_findings: vec![],
        });

        // Clear inputs
        self.evidence_source_input
            .update(cx, |input, cx| input.set_text("", cx));
        self.evidence_summary_input
            .update(cx, |input, cx| input.set_text("", cx));

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!("✓ Evidence added to '{}'", driver_name),
        });
        cx.notify();
    }

    /// Accept a pending p50 suggestion — applies the value to the driver.
    pub fn accept_suggestion(&mut self, suggestion_id: &str, cx: &mut Context<Self>) {
        let sug = self
            .pending_suggestions
            .iter()
            .find(|s| s.id == suggestion_id)
            .cloned();
        let Some(sug) = sug else {
            return;
        };

        // ── Detect template shape: parameterized vs literal ────────────
        //
        // Two distinct mutation paths depending on how the driver
        // declares its distribution:
        //
        //   Path A (parameterized)  —  triangular(socio_p5, socio_p50, socio_p95)
        //       The distribution's args are Identifier expressions
        //       referencing `param X: real` declarations. The actual
        //       values live in workspace_outputs.params and are bound
        //       per-team at sim time. Mutating the AST literals would
        //       break the parameterization (it'd replace the Identifier
        //       node with a Number, severing the link to the per-team
        //       value, AND the next workspace-params load would wipe
        //       the local change anyway).
        //
        //       Right thing: write the new center to
        //       workspace_outputs.params[<param-name>], proportionally
        //       shift p5 and p95 to preserve spread shape, then
        //       refresh load_workspace_params so the next sim picks it
        //       up. The AST stays clean.
        //
        //   Path B (literal)  —  triangular(0.6, 1.0, 1.4) with Number args
        //       Old-style template, no per-team parameterization. Mutate
        //       the literals in-place (preserved from the previous
        //       implementation for non-WC templates).
        //
        // The WC team_prior post-Option-2 redesign uses Path A
        // exclusively. Generic / hand-authored templates may still use
        // Path B.
        let driver_dist_shape = self
            .program
            .driver(&sug.driver_name)
            .and_then(|d| d.distribution.as_ref())
            .cloned();

        let param_names: Option<(String, String, String)> = match &driver_dist_shape {
            Some(Distribution::Triangular { p5, p50, p95 }) => match (p5, p50, p95) {
                (
                    Expression::Identifier(p5_name),
                    Expression::Identifier(p50_name),
                    Expression::Identifier(p95_name),
                ) => Some((p5_name.clone(), p50_name.clone(), p95_name.clone())),
                _ => None,
            },
            _ => None,
        };

        if let Some((p5_name, p50_name, p95_name)) = param_names {
            // ── Path A: parameterized → write through to workspace params
            self.apply_suggestion_to_workspace_params(sug, &p5_name, &p50_name, &p95_name, cx);
        } else {
            // ── Path B: literal-args distribution, mutate AST directly
            self.apply_suggestion_to_ast_literals(sug, cx);
        }
    }

    /// Path A: parameterized driver — write the suggested center to
    /// workspace_outputs.params via PUT, proportionally shifting p5/p95
    /// to preserve the original spread shape. Refresh
    /// `self.workspace_params` so the next sim picks up the new triple.
    ///
    /// Without proportional shift, accepting a +20% suggestion on
    /// `socio_p50` leaves `socio_p5` and `socio_p95` unchanged — the
    /// distribution becomes asymmetric and the operator's "wider spread =
    /// less confident" intuition breaks. We preserve the half-widths
    /// (p50 − p5) and (p95 − p50) computed from the existing triple,
    /// then shift everything by the delta.
    fn apply_suggestion_to_workspace_params(
        &mut self,
        sug: EvidenceSuggestion,
        p5_name: &str,
        p50_name: &str,
        p95_name: &str,
        cx: &mut Context<Self>,
    ) {
        // Read current triple values from workspace_params (loaded on
        // open). Falls back to a sane default if any are missing — we
        // don't want a partial backfill to dead-end the Apply path.
        let cur_p5 = self
            .workspace_params
            .get(p5_name)
            .and_then(|v| v.as_f64())
            .unwrap_or(sug.current_p50 - 0.20);
        let cur_p50 = self
            .workspace_params
            .get(p50_name)
            .and_then(|v| v.as_f64())
            .unwrap_or(sug.current_p50);
        let cur_p95 = self
            .workspace_params
            .get(p95_name)
            .and_then(|v| v.as_f64())
            .unwrap_or(sug.current_p50 + 0.20);

        let new_p50 = sug.suggested_p50;
        let half_lo = (cur_p50 - cur_p5).max(0.05); // preserve lower half-width
        let half_hi = (cur_p95 - cur_p50).max(0.05); // preserve upper half-width
        let new_p5 = (new_p50 - half_lo).max(0.01);
        let new_p95 = (new_p50 + half_hi).max(new_p5 + 0.05);

        // Update local cache so the next Ctrl+R uses the new triple
        // immediately (cockpit's run_simulation reads self.workspace_params).
        self.workspace_params
            .insert(p5_name.to_string(), serde_json::json!(new_p5));
        self.workspace_params
            .insert(p50_name.to_string(), serde_json::json!(new_p50));
        self.workspace_params
            .insert(p95_name.to_string(), serde_json::json!(new_p95));

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", sug.driver_name),
            kind: MessageKind::Info,
            text: format!(
                "✓ Accepted: {} ({:.2} → {:.2}) — spread preserved [{:.2}, {:.2}] (from {})",
                p50_name,
                cur_p50,
                new_p50,
                new_p5,
                new_p95,
                base_agent_name(&sug.agent_name)
            ),
        });

        // Persist server-side: PUT the merged params object so the next
        // workspace-params reload (or another operator on another
        // machine) gets the same triple. Fire-and-forget; the local
        // cache is already updated.
        if let Some(ref ws_id) = self.workspace_id {
            let api = self.api.clone();
            let ws = ws_id.clone();
            let merged = self.workspace_params.clone();
            let driver_name = sug.driver_name.clone();
            let agent_name = base_agent_name(&sug.agent_name);
            let content = format!(
                "**Param update** on driver `{}`:\n- {}: {:.3} → {:.3}\n- {}: {:.3} → {:.3}\n- {}: {:.3} → {:.3}\n- Source: {} evidence\n- Rationale: {}",
                driver_name,
                p5_name, cur_p5, new_p5,
                p50_name, cur_p50, new_p50,
                p95_name, cur_p95, new_p95,
                agent_name,
                sug.reasoning.chars().take(200).collect::<String>(),
            );
            let meta = serde_json::json!({
                "cost_class": "event_append",
                "fermi_action": "update_params",
                "driver": driver_name,
                "p5_name": p5_name,
                "p50_name": p50_name,
                "p95_name": p95_name,
                "previous_p50": cur_p50,
                "updated_p50": new_p50,
            });
            tokio::spawn(async move {
                let value = serde_json::Value::Object(merged);
                if let Err(e) = api.set_workspace_output(&ws, "params", &value).await {
                    log::warn!(
                        "[apply→params] PUT failed for {}: {} — local cache has the new triple, \
                         next workspace-params reload will revert it",
                        ws,
                        e
                    );
                }
                let _ = api
                    .post_workspace_message(
                        &ws,
                        "user",
                        "fermi_console",
                        Some("Fermi Console"),
                        &content,
                        "system_event",
                        Some(&meta),
                    )
                    .await;
            });
        }

        self.pending_suggestions.retain(|s| s.id != sug.id);

        // If the operator has the affected driver focused in the Edit
        // panel, repopulate its p5/p50/p95 fields with the new values
        // so the panel reflects what just happened. Without this the
        // panel keeps showing the old triple (or — worse — "0.00" from
        // the unresolved Identifier expression) and the user thinks the
        // Apply didn't take.
        if let FocusedNode::Driver(ref focused_name) = self.focused_node.clone() {
            if focused_name == &sug.driver_name {
                self.populate_editor_from_driver(focused_name, cx);
            }
        }

        cx.notify();

        // Auto-fire a sim so the predicted_probability + trajectory
        // update immediately. Without this the operator has to manually
        // Ctrl+R to see the consequence of their accept, which makes the
        // Apply feel unresponsive — especially in batch (apply 4 of 6
        // pending suggestions and the dashboard stays frozen until the
        // last manual sim).
        self.run_simulation(cx);
    }

    /// Path B: legacy distribution-literal mutation. Used by hand-authored
    /// FPLs whose drivers carry Number-typed bounds rather than param
    /// references. Preserves the previous behaviour byte-for-byte so we
    /// don't regress non-WC forecasts.
    fn apply_suggestion_to_ast_literals(
        &mut self,
        sug: EvidenceSuggestion,
        cx: &mut Context<Self>,
    ) {
        if let Some(driver) = self.program.driver_mut(&sug.driver_name) {
            if let Some(ref mut dist) = driver.distribution {
                match dist {
                    Distribution::Triangular { p5, p50, p95 } => {
                        let new_center = sug.suggested_p50;
                        let lo = match p5 {
                            Expression::Number(n) => Some(*n),
                            _ => None,
                        };
                        let hi = match p95 {
                            Expression::Number(n) => Some(*n),
                            _ => None,
                        };
                        let old_center = match p50 {
                            Expression::Number(n) => Some(*n),
                            _ => None,
                        };
                        *p50 = Expression::Number(new_center);
                        if let (Some(lo_v), Some(hi_v), Some(c_v)) = (lo, hi, old_center) {
                            let half_lo = c_v - lo_v;
                            let half_hi = hi_v - c_v;
                            *p5 = Expression::Number(new_center - half_lo);
                            *p95 = Expression::Number(new_center + half_hi);
                        }
                    }
                    Distribution::Normal { mean, .. } => {
                        *mean = Expression::Number(sug.suggested_p50);
                    }
                    Distribution::Lognormal { median, .. } => {
                        *median = Expression::Number(sug.suggested_p50);
                    }
                    Distribution::Uniform { low, high } => {
                        let lo = match low {
                            Expression::Number(n) => Some(*n),
                            _ => None,
                        };
                        let hi = match high {
                            Expression::Number(n) => Some(*n),
                            _ => None,
                        };
                        if let (Some(lo_v), Some(hi_v)) = (lo, hi) {
                            let half = (hi_v - lo_v) / 2.0;
                            *low = Expression::Number(sug.suggested_p50 - half);
                            *high = Expression::Number(sug.suggested_p50 + half);
                        }
                    }
                    Distribution::Beta { .. } => {
                        self.messages.push(AssistantMessage {
                            node: format!("driver:{}", sug.driver_name),
                            kind: MessageKind::Warning,
                            text: "Beta distributions don't support direct p50 override yet."
                                .into(),
                        });
                        return;
                    }
                }
            }
        }
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", sug.driver_name),
            kind: MessageKind::Info,
            text: format!(
                "✓ Accepted: p50 {:.2} → {:.2} (from {})",
                sug.current_p50,
                sug.suggested_p50,
                base_agent_name(&sug.agent_name)
            ),
        });

        if let Some(ref ws_id) = self.workspace_id {
            let api = self.api.clone();
            let ws = ws_id.clone();
            let content = format!(
                "**Distribution update** on driver `{}`:\n- p50: {:.3} → {:.3} ({:+.1}%)\n- Source: {} evidence\n- Rationale: {}",
                sug.driver_name,
                sug.current_p50,
                sug.suggested_p50,
                (sug.suggested_p50 / sug.current_p50.max(0.001) - 1.0) * 100.0,
                base_agent_name(&sug.agent_name),
                sug.reasoning.chars().take(200).collect::<String>(),
            );
            let meta = serde_json::json!({
                "cost_class": "event_append",
                "fermi_action": "update_distribution",
                "driver": sug.driver_name,
                "previous_p50": sug.current_p50,
                "updated_p50": sug.suggested_p50,
            });
            tokio::spawn(async move {
                let _ = api
                    .post_workspace_message(
                        &ws,
                        "user",
                        "fermi_console",
                        Some("Fermi Console"),
                        &content,
                        "system_event",
                        Some(&meta),
                    )
                    .await;
            });
        }

        self.pending_suggestions.retain(|s| s.id != sug.id);

        // Refresh the Edit panel if the affected driver is focused.
        if let FocusedNode::Driver(ref focused_name) = self.focused_node.clone() {
            if focused_name == &sug.driver_name {
                self.populate_editor_from_driver(focused_name, cx);
            }
        }

        cx.notify();

        // Auto-sim — same rationale as the parameterized path.
        self.run_simulation(cx);
    }

    /// Reject a pending p50 suggestion — discards it.
    pub fn reject_suggestion(&mut self, suggestion_id: &str, cx: &mut Context<Self>) {
        if let Some(sug) = self
            .pending_suggestions
            .iter()
            .find(|s| s.id == suggestion_id)
        {
            self.messages.push(AssistantMessage {
                node: format!("driver:{}", sug.driver_name),
                kind: MessageKind::Info,
                text: format!(
                    "✗ Rejected p50 suggestion from {}",
                    base_agent_name(&sug.agent_name)
                ),
            });
        }
        self.pending_suggestions.retain(|s| s.id != suggestion_id);
        cx.notify();
    }

    /// Toggle evidence expand/collapse state.
    pub fn toggle_evidence_collapsed(&mut self, evidence_id: &str) {
        if self.collapsed_evidence.contains(evidence_id) {
            self.collapsed_evidence.remove(evidence_id);
        } else {
            self.collapsed_evidence.insert(evidence_id.to_string());
        }
    }

    /// Update the outside rate (base rate) without resetting drivers.
    /// Fires Fermi to research the current base rate for the question.
    pub fn update_outside_rate(&mut self, cx: &mut Context<Self>) {
        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        if question.is_empty() {
            return;
        }

        let query = format!(
            "What is the base rate for this forecast question? \
             Provide ONLY a JSON object: \
             {{\"reference_class\": \"...\", \"historical_frequency\": 0.0-1.0, \
             \"sample_size\": N, \"reasoning\": \"...\"}}\n\n\
             Question: \"{}\"",
            question
        );

        self.agent_runs.push(AgentExecution {
            agent_name: "fermi_base_rate".into(),
            status: AgentRunStatus::Running,
            evidence_count: 0,
            confidence: None,
            error: None,
            credits_charged: None,
            started_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            ),
            completed_at: None,
            latest_finding: None,
        });

        self.messages.push(AssistantMessage {
            node: "question".into(),
            kind: MessageKind::Info,
            text: "⟳ Updating outside rate…".into(),
        });

        // Fire fermi agent for base rate only
        self.fire_agent("fermi", &query, cx);
    }

    /// Flip the `learnable` flag on a driver. When ON, the driver's static
    /// distribution becomes the BayesOps prior and `params.<name>_fitted` (if
    /// present) overrides at sim time. When OFF, the driver behaves as before:
    /// the distribution is sampled directly.
    ///
    /// This is the user-facing entry point for opting individual drivers into
    /// data-informed parameter fitting. See `docs/fermi/BAYESOPS_CONTRACT.md`.
    pub fn toggle_driver_learnable(&mut self, name: &str, cx: &mut Context<Self>) {
        // Persist any in-flight edits to the focused driver first so the FPL
        // regenerated below reflects the latest p5/p50/p95/etc.
        self.save_focused_driver(cx);

        let new_state = if let Some(driver) = self.program.driver_mut(name) {
            driver.learnable = !driver.learnable;
            driver.learnable
        } else {
            return;
        };

        // Regenerate FPL so the toggle change shows up if the user is also
        // looking at the raw FPL pane — but only if the cached FPL is in
        // the cockpit AST's representable subset. For factor-model loaded
        // forecasts this is a no-op (see regenerate_cached_fpl_if_safe).
        self.regenerate_cached_fpl_if_safe();

        // Surface the change as an assistant message so the user sees what
        // just happened — especially the cold-start hint when turning ON.
        let (kind, text) = if new_state {
            (
                MessageKind::Info,
                format!(
                    "🦊 Driver '{}' is now learnable. \
                     The current distribution is the prior; BayesOps will tighten it \
                     as observations accumulate. Run a simulation to see resolution status.",
                    name
                ),
            )
        } else {
            (
                MessageKind::Info,
                format!(
                    "🦊 Driver '{}' is no longer learnable. \
                     The distribution will be sampled as-is, ignoring any BayesOps fits.",
                    name
                ),
            )
        };
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", name),
            kind,
            text,
        });
        cx.notify();
    }

    pub fn delete_driver(&mut self, name: &str, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.program.remove_driver(name);
        if let FocusedNode::Driver(ref n) = self.focused_node {
            if n == name {
                self.focused_node = FocusedNode::Question;
            }
        }
        self.messages.push(AssistantMessage {
            node: "question".into(),
            kind: MessageKind::Info,
            text: format!("Driver '{}' removed.", name),
        });
        cx.notify();
    }

    /// Add a new continuous driver manually and open it for editing.
    pub fn add_manual_driver(&mut self, binary: bool, cx: &mut Context<Self>) {
        let idx = self.program.drivers().len() + 1;
        let (name, driver) = if binary {
            (
                format!("event_{}", idx),
                make_binary_driver(
                    &format!("event_{}", idx),
                    &format!("Event {}", idx),
                    0.5,
                    1.3,
                    "Describe this event and its impact",
                ),
            )
        } else {
            (
                format!("driver_{}", idx),
                make_continuous_driver(
                    &format!("driver_{}", idx),
                    &format!("Driver {}", idx),
                    "",
                    0.0,
                    0.0,
                    0.0,
                    "Describe this driver and set your estimates",
                ),
            )
        };
        self.program.add_driver(driver);
        self.focus_driver(&name, cx);
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", name),
            kind: MessageKind::Suggestion,
            text: format!(
                "New driver '{}' added. Set your estimates in the editor.",
                name
            ),
        });
    }

    /// Export the evidence wiki as a Markdown file and open it.
    pub fn export_wiki_markdown(&mut self, _cx: &mut Context<Self>) {
        self.save_focused_driver(_cx);
        self.regenerate_cached_fpl_if_safe();

        let filename = self
            .program
            .question()
            .map(|q| sanitize_name(&q.text))
            .unwrap_or_else(|| "forecast".into());

        let wiki = generate_evidence_wiki(
            &self.program,
            self.current_version,
            self.predicted_probability,
            &self.inside_view_explanation,
            self.forecast_confidence,
            self.pm_market_price,
            self.pm_url.as_deref(),
            self.pm_volume_24h,
            self.pm_confidence.as_deref(),
            self.pm_price_change_1w,
            self.sim_results.as_ref(),
            &self.versions,
        );

        let export_path = format!("forecasts/{}.evidence.md", filename);
        let _ = std::fs::create_dir_all("forecasts");

        match std::fs::write(&export_path, &wiki) {
            Ok(_) => {
                log::info!("[composer] Exported wiki to {}", export_path);
                self.messages.push(AssistantMessage {
                    node: "export".into(),
                    kind: MessageKind::Info,
                    text: format!("📄 Exported to {}", export_path),
                });
                // Try to open the file with the system default app
                if let Err(e) = open::that(&export_path) {
                    log::warn!("[composer] Could not open {}: {}", export_path, e);
                    self.messages.push(AssistantMessage {
                        node: "export".into(),
                        kind: MessageKind::Tip,
                        text: format!("File saved to {}. Open it manually to view.", export_path),
                    });
                }
            }
            Err(e) => {
                log::error!("[composer] Failed to export wiki: {}", e);
                self.messages.push(AssistantMessage {
                    node: "export".into(),
                    kind: MessageKind::Error,
                    text: format!("Export failed: {}", e),
                });
            }
        }
        _cx.notify();
    }

    /// Save the currently focused driver's editor values back to the AST.
    pub fn save_focused_driver(&mut self, cx: &App) {
        let name = match &self.focused_node {
            FocusedNode::Driver(n) => n.clone(),
            _ => return,
        };

        let new_name = self.editor_name.read(cx).text().to_string();
        let rationale = self.editor_rationale.read(cx).text().to_string();

        if let Some(driver) = self.program.driver_mut(&name) {
            if !new_name.trim().is_empty() {
                driver.name = sanitize_name(&new_name);
            }
            driver.rationale = if rationale.is_empty() {
                None
            } else {
                Some(rationale)
            };

            match driver.driver_type {
                DriverType::Continuous => {
                    let p5 = self.editor_p5.read(cx).text().parse::<f64>().unwrap_or(0.8);
                    let p50 = self
                        .editor_p50
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(1.0);
                    let p95 = self
                        .editor_p95
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(1.2);
                    let unit = self.editor_unit.read(cx).text().to_string();

                    // Param-indirection bridge (post-Option-2 team-prior
                    // template). Workspace-backed forecasts define each
                    // driver as `triangular(socio_p5, socio_p50, socio_p95)`
                    // — the triple is bound from workspace params, and
                    // run_simulation PRESERVES that FPL verbatim (see
                    // cached_fpl_is_richer_than_ast) instead of regenerating
                    // it from this AST. If we only overwrote the AST with
                    // literals, the edit would never reach the executed FPL
                    // and the sim wouldn't move — the regression behind
                    // "editing distributions only makes minor changes".
                    //
                    // So when the loaded distribution references param names,
                    // KEEP the identifiers in the AST and mirror the edited
                    // values into workspace_params under those names.
                    // run_simulation's set_params then binds them and the
                    // preserved FPL picks them up. The editor was pre-filled
                    // from these same params on focus, so an un-edited save
                    // round-trips the existing values without clobbering.
                    let param_refs: Option<(String, String, String)> = match &driver.distribution {
                        Some(Distribution::Triangular {
                            p5: Expression::Identifier(a),
                            p50: Expression::Identifier(b),
                            p95: Expression::Identifier(c),
                        }) => Some((a.clone(), b.clone(), c.clone())),
                        _ => None,
                    };

                    if param_refs.is_none() {
                        // Literal-args distribution (legacy hand-authored
                        // FPL): regeneration is safe, write literals.
                        driver.distribution = Some(Distribution::Triangular {
                            p5: Expression::Number(p5),
                            p50: Expression::Number(p50),
                            p95: Expression::Number(p95),
                        });
                    }
                    driver.unit = if unit.is_empty() { None } else { Some(unit) };

                    // driver's borrow ends above; apply param mirroring.
                    if let Some((a, b, c)) = param_refs {
                        self.workspace_params.insert(a, serde_json::json!(p5));
                        self.workspace_params.insert(b, serde_json::json!(p50));
                        self.workspace_params.insert(c, serde_json::json!(p95));
                    }
                }
                DriverType::Binary => {
                    let prob = self
                        .editor_prob
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(0.5);
                    let impact = self
                        .editor_impact
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(1.3);
                    driver.probability = Some(prob.clamp(0.0, 1.0));
                    driver.impact_multiplier = Some(impact);
                }
                _ => {}
            }

            // Save user-set confidence for this driver
            let conf_text = self.editor_confidence.read(cx).text().to_string();
            if !conf_text.trim().is_empty() {
                if let Ok(pct) = conf_text.trim().parse::<f64>() {
                    let clamped = (pct / 100.0).clamp(0.0, 1.0);
                    self.driver_confidence.insert(name.clone(), clamped);
                }
            } else {
                // Empty means "use computed" — remove override
                self.driver_confidence.remove(&name);
            }

            // Fermi validates the saved driver
            if let Some(driver) = self.program.driver(&name) {
                match driver.driver_type {
                    DriverType::Continuous => {
                        if let Some(Distribution::Triangular {
                            ref p5,
                            ref p50,
                            ref p95,
                        }) = driver.distribution
                        {
                            let v5 = expr_to_f64(p5);
                            let v50 = expr_to_f64(p50);
                            let v95 = expr_to_f64(p95);
                            if v5 > v50 || v50 > v95 {
                                self.messages.push(AssistantMessage {
                                node: format!("driver:{}", name),
                                kind: MessageKind::Warning,
                                text: format!("🦊 Driver '{}': p5 ({:.2}) should be ≤ p50 ({:.2}) ≤ p95 ({:.2}). Distribution is backwards.", name, v5, v50, v95),
                            });
                            }
                            if v5 == v50 && v50 == v95 {
                                self.messages.push(AssistantMessage {
                                node: format!("driver:{}", name),
                                kind: MessageKind::Suggestion,
                                text: format!("🦊 Driver '{}': all values are equal ({:.2}). This means no uncertainty — is that intended?", name, v50),
                            });
                            }
                        }
                    }
                    DriverType::Binary => {
                        if let Some(p) = driver.probability {
                            if p <= 0.0 || p >= 1.0 {
                                self.messages.push(AssistantMessage {
                                node: format!("driver:{}", name),
                                kind: MessageKind::Warning,
                                text: format!("🦊 Driver '{}': probability {:.0}% is at an extreme. Consider whether this is truly certain.", name, p * 100.0),
                            });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Simulation
    // ═══════════════════════════════════════════════════════════════

    pub fn run_simulation(&mut self, cx: &mut Context<Self>) {
        // Reconcile-derived lock: a resolved/voided forecast is settled —
        // re-running a simulation against it is meaningless and its result
        // can't be saved anyway. Refuse with the authoritative reason.
        if self.is_locked() {
            self.sim_error = Some(format!(
                "Locked: {}. Re-running a simulation is disabled.",
                self.lock_reason()
                    .unwrap_or_else(|| "forecast is resolved".into())
            ));
            self.messages.push(AssistantMessage {
                node: "simulation".into(),
                kind: MessageKind::Warning,
                text: format!(
                    "This forecast is settled ({}). Re-sims are disabled — use ↻ Reconcile if you think this is stale.",
                    self.lock_reason().unwrap_or_else(|| "resolved".into())
                ),
            });
            cx.notify();
            return;
        }
        self.save_focused_driver(cx);
        self.sim_running = true;
        self.sim_error = None;

        // ── Source-of-truth selection for the FPL we run ────────────────
        //
        // The cockpit data model (`self.program`) represents the legacy
        // linear "outside view + drivers" composition. `generate_fpl_text`
        // serializes it back to FPL — but it ONLY emits question +
        // drivers. Factor blocks (`factor X1 {...}`), `estimate ... as:`
        // expressions, and learnable() function calls all get dropped
        // because they're not part of the cockpit AST.
        //
        // That's fine when the user composed the forecast from scratch
        // here. But when we hydrated the cockpit from a workspace-backed
        // forecast (Spec 23 team priors), the cached_fpl is the rich
        // 6-factor program loaded by open_forecast / open_workspace_forecast.
        // Regenerating from `self.program` strips the model and we run a
        // 2-driver-only simulation that has no scoring expression.
        //
        // Heuristic: if cached_fpl contains a factor-model construct, use
        // it as-is. Otherwise it came from this cockpit's composer and is
        // safe to regenerate.
        let preserve_loaded_fpl = Self::cached_fpl_is_richer_than_ast(&self.cached_fpl);
        self.regenerate_cached_fpl_if_safe();

        if self.program.drivers().is_empty() && !preserve_loaded_fpl {
            self.sim_error = Some("No drivers defined. Add drivers first.".into());
            self.sim_running = false;
            cx.notify();
            return;
        }

        // ── Zero-driver guard ─────────────────────────────────────
        // If any continuous driver has p5=p50=p95=0, it will nuke the
        // entire multiplicative model (anything × 0 = 0). Fix these
        // to neutral (0.8/1.0/1.2) and warn the user.
        let driver_names_to_fix: Vec<String> = self
            .program
            .drivers()
            .iter()
            .filter(|d| {
                d.driver_type == DriverType::Continuous
                    && d.distribution
                        .as_ref()
                        .map(|dist| match dist {
                            Distribution::Triangular { p5, p50, p95 } => {
                                expr_to_f64(p5) == 0.0
                                    && expr_to_f64(p50) == 0.0
                                    && expr_to_f64(p95) == 0.0
                            }
                            _ => false,
                        })
                        .unwrap_or(false)
            })
            .map(|d| d.name.clone())
            .collect();

        for name in &driver_names_to_fix {
            if let Some(driver) = self.program.driver_mut(name) {
                driver.distribution = Some(Distribution::Triangular {
                    p5: Expression::Number(0.8),
                    p50: Expression::Number(1.0),
                    p95: Expression::Number(1.2),
                });
            }
            self.messages.push(AssistantMessage {
                node: format!("driver:{}", name),
                kind: MessageKind::Warning,
                text: format!(
                    "⚠ Driver '{}' had all-zero values (p5=p50=p95=0) which would collapse the model. Reset to neutral (0.8/1.0/1.2). Adjust based on evidence.",
                    name
                ),
            });
            log::warn!("[sim] Fixed zero-driver '{}' → neutral 0.8/1.0/1.2", name);
        }

        if !driver_names_to_fix.is_empty() && !preserve_loaded_fpl {
            // Regenerate FPL after fixing the zero-driver. Guarded so we
            // never overwrite a factor-model loaded FPL.
            self.regenerate_cached_fpl_if_safe();
        }

        // ── Debug: log driver state before simulation ─────────────
        log::info!("[sim] === SIMULATION START ===");
        log::info!("[sim] Drivers in AST: {}", self.program.drivers().len());
        for d in self.program.drivers() {
            match d.driver_type {
                DriverType::Continuous => {
                    if let Some(Distribution::Triangular {
                        ref p5,
                        ref p50,
                        ref p95,
                    }) = d.distribution
                    {
                        log::info!(
                            "[sim]   {} (continuous): p5={:.3} p50={:.3} p95={:.3} unit={:?}",
                            d.name,
                            expr_to_f64(p5),
                            expr_to_f64(p50),
                            expr_to_f64(p95),
                            d.unit
                        );
                    } else {
                        log::info!("[sim]   {} (continuous): NO DISTRIBUTION", d.name);
                    }
                }
                DriverType::Binary => {
                    log::info!(
                        "[sim]   {} (binary): prob={:.3} impact={:.2}",
                        d.name,
                        d.probability.unwrap_or(0.0),
                        d.impact_multiplier.unwrap_or(1.0)
                    );
                }
                _ => log::info!("[sim]   {} (discrete)", d.name),
            }
        }
        log::info!("[sim] Has model: {}", self.program.model().is_some());
        log::info!("[sim] Has simulate: {}", self.program.simulate().is_some());
        log::info!(
            "[sim] Base rate: {:?}",
            self.program
                .question()
                .and_then(|q| q.base_rate.as_ref())
                .map(|br| br.historical_frequency)
        );

        // Parse the generated FPL and execute
        let fpl = self.cached_fpl.clone();
        log::info!(
            "[sim] Generated FPL ({} chars):\n{}",
            fpl.chars().count(),
            fpl.chars().take(2000).collect::<String>()
        );
        let start = std::time::Instant::now();

        let tokens = match ::fermi::lexer::Lexer::new(&fpl).tokenize() {
            Ok(t) => {
                log::info!("[sim] Tokenized OK: {} tokens", t.len());
                t
            }
            Err(e) => {
                log::error!("[sim] Tokenization FAILED: {:?}", e);
                self.sim_error = Some(format!("FPL tokenization error: {:?}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        let parsed = match ::fermi::parser::Parser::new(tokens).parse() {
            Ok(p) => {
                log::info!(
                    "[sim] Parsed OK: {} statements, {} drivers, model={}, simulate={}",
                    p.statements.len(),
                    p.drivers().len(),
                    p.model().is_some(),
                    p.simulate().is_some()
                );
                p
            }
            Err(e) => {
                log::error!("[sim] Parse FAILED: {}", e);
                self.sim_error = Some(format!("FPL parse error: {}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        let mut executor = ::fermi::executor::Executor::new(10_000);

        // ── Bind workspace params into the Executor ──────────────────
        //
        // workspace_params is a flat key→Value map pulled from
        // workspace_outputs[ws].params. Two contributors:
        //   - Spawn-time scalar bindings (elo_current=1850, etc.) written
        //     by respawn_aligned.py from the team CSV.
        //   - BayesOps fitted distributions (`<driver>_fitted` → JSON)
        //     written by the accept_pending handler.
        //
        // The Executor takes scalars via set_params (reachable as plain
        // identifiers in distribution-arg expressions) and structured
        // overrides via set_json_params (read by fitted_distribution_for
        // for learnable drivers). Without this wire, the WC team_prior's
        // `normal((elo_current - 1700) / 300, 0.20)` resolves to
        // EvalError(UndefinedVariable("elo_current")) and every team
        // collapses onto the same rate.
        let mut numeric_params: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        let mut json_params: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        for (k, v) in &self.workspace_params {
            // Numbers get bound as scalars. Booleans coerce to 0.0/1.0
            // because the executor's evaluator only knows f64. Strings
            // are skipped — distribution-args don't reference them.
            // Object/Array values go to json_params (BayesOps fits live
            // there).
            match v {
                serde_json::Value::Number(n) => {
                    if let Some(f) = n.as_f64() {
                        numeric_params.insert(k.clone(), f);
                    }
                }
                serde_json::Value::Bool(b) => {
                    numeric_params.insert(k.clone(), if *b { 1.0 } else { 0.0 });
                }
                serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                    json_params.insert(k.clone(), v.clone());
                }
                _ => {} // String / Null — skip, not a numeric/structured param
            }
        }
        let n_numeric = numeric_params.len();
        let n_json = json_params.len();
        executor.set_params(numeric_params);
        executor.set_json_params(json_params);
        log::info!(
            "[sim] Bound {} scalar + {} JSON params from workspace",
            n_numeric,
            n_json
        );

        match executor.execute(&parsed) {
            Ok(results) => {
                let elapsed = start.elapsed();
                log::info!("[sim] Execution OK in {}ms: mean={:.4} median={:.4} p5={:.4} p95={:.4} std={:.4} iters={}",
                    elapsed.as_millis(), results.mean, results.median, results.p5, results.p95, results.std_dev, results.iterations);
                let histogram_data = results.histogram(20);
                // Capture bin_starts + bin_width so the interactive
                // histogram (Wiki tab) can map cursor x → outcome value
                // without re-executing the simulation.
                let bin_starts: Vec<f64> = histogram_data.iter().map(|(start, _)| *start).collect();
                let bin_width = if bin_starts.len() >= 2 {
                    bin_starts[1] - bin_starts[0]
                } else {
                    0.0
                };
                // Mirror the executor's learnable-driver resolution log into
                // a UI-friendly shape. Empty when no driver is `learnable: true`.
                let learnable_drivers: Vec<LearnableDriverBadge> = results
                    .learnable_drivers
                    .iter()
                    .map(|r| {
                        let status = match &r.source {
                            ::fermi::executor::LearnableSource::Fitted { fitted } => {
                                LearnableBadgeStatus::Fitted {
                                    family: match fitted {
                                        ::posterior::FittedDistribution::Beta { .. } => {
                                            "beta".into()
                                        }
                                        ::posterior::FittedDistribution::Normal { .. } => {
                                            "normal".into()
                                        }
                                        ::posterior::FittedDistribution::Lognormal { .. } => {
                                            "lognormal".into()
                                        }
                                        ::posterior::FittedDistribution::Triangular { .. } => {
                                            "triangular".into()
                                        }
                                    },
                                    n_eff: fitted.n_eff(),
                                    ci_width: fitted.ci_width(),
                                }
                            }
                            ::fermi::executor::LearnableSource::PriorFallback => {
                                LearnableBadgeStatus::PriorFallback
                            }
                            ::fermi::executor::LearnableSource::Static => {
                                // Static drivers aren't logged by the executor
                                // today; this arm exists only because the enum
                                // is non-exhaustive. Skip with PriorFallback so
                                // the UI degrades gracefully.
                                LearnableBadgeStatus::PriorFallback
                            }
                        };
                        LearnableDriverBadge {
                            driver_name: r.name.clone(),
                            status,
                        }
                    })
                    .collect();

                self.sim_results = Some(SimResults {
                    mean: results.mean,
                    median: results.median,
                    p5: results.p5,
                    p95: results.p95,
                    std_dev: results.std_dev,
                    iterations: results.iterations as u64,
                    execution_time_ms: elapsed.as_millis() as u64,
                    histogram: histogram_data.iter().map(|(_, c)| *c as u32).collect(),
                    bin_starts,
                    bin_width,
                    learnable_drivers,
                });
                self.sim_running = false;
                // Spec 23 R-2: refresh server-side pending fits after every
                // sim run so the sparkline badges reflect any fits the refit
                // hook may have produced since we last looked.
                if self.workspace_id.is_some() {
                    self.load_bayesops_state(cx);
                }

                // ── Take the simulation result as authoritative ────
                //
                // Design contract (Option 2): the FPL `model:` expression is
                // the quantity being forecast. Whatever it evaluates to IS
                // the output. The cockpit is a Monte Carlo runner + viewer,
                // not a transformer.
                //
                // For binary-question forecasts, the operator's responsibility
                // is to write a model expression that produces a probability:
                //   model: 0.0208 * socio * institutional * dynamic * squad * ...
                // The base_rate (0.0208 here) is part of the model the
                // operator declared. The cockpit doesn't re-multiply it in.
                //
                // For non-probability forecasts (counts, magnitudes, durations)
                // the model expression returns whatever it returns and the
                // distribution is shown to the operator unrescaled. We only
                // clamp into [0,1] when the question carries a base_rate
                // (signalling "this is a probability forecast"); otherwise
                // we accept whatever value the executor produces.
                let is_probability_forecast = self
                    .program
                    .question()
                    .and_then(|q| q.base_rate.as_ref())
                    .is_some();

                if is_probability_forecast {
                    // Lower clamp at 0.01 (1%) so the dashboard never
                    // displays a meaningless 0%; upper clamp at 0.99 to
                    // mirror the legacy behaviour and avoid dashboard
                    // edge cases. Mean below 0.01 is still a real model
                    // signal — log it so calibration work has access.
                    if results.mean < 0.01 || results.mean > 0.99 {
                        log::info!(
                            "[sim] Mean {:.4} clamped to display range [0.01, 0.99]; \
                             raw mean preserved in results.",
                            results.mean
                        );
                    }
                    self.predicted_probability = results.mean.clamp(0.01, 0.99);
                } else {
                    // Non-probability forecast: just take the mean. The
                    // distribution surfaces in the histogram + p5/p95
                    // displays and the operator interprets it in domain
                    // terms (count, magnitude, duration).
                    self.predicted_probability = results.mean;
                }
                log::info!(
                    "[sim] Taken: mean={:.4}  display={:.4}  is_probability={}",
                    results.mean,
                    self.predicted_probability,
                    is_probability_forecast
                );

                // Build narrative explanation. With Option 2 there's no
                // ratio-vs-baseline story — instead we describe the mean +
                // its spread, citing the most influential drivers.
                let top_drivers: Vec<String> = self
                    .program
                    .drivers()
                    .iter()
                    .take(3)
                    .map(|d| d.display_name.as_deref().unwrap_or(&d.name).to_string())
                    .collect();

                self.inside_view_explanation = if is_probability_forecast {
                    let base_rate = self
                        .program
                        .question()
                        .and_then(|q| q.base_rate.as_ref())
                        .map(|br| br.historical_frequency)
                        .unwrap_or(0.0);
                    format!(
                        "Inside view: model evaluates to {:.1}% (p5={:.1}%, p95={:.1}%). \
                         Outside view (base rate): {:.1}%. Key drivers: {}.",
                        self.predicted_probability * 100.0,
                        results.p5 * 100.0,
                        results.p95 * 100.0,
                        base_rate * 100.0,
                        top_drivers.join(", "),
                    )
                } else {
                    format!(
                        "Inside view: model evaluates to {:.3} (p5={:.3}, p95={:.3}). \
                         Key drivers: {}.",
                        results.mean,
                        results.p5,
                        results.p95,
                        top_drivers.join(", "),
                    )
                };
                // ── Fermi interprets the result ───────────────────
                let base_rate = self
                    .program
                    .question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency)
                    .unwrap_or(0.0);
                let divergence = (results.mean - base_rate) * 100.0;
                let div_direction = if divergence > 0.0 { "above" } else { "below" };

                // Build driver contribution summary

                // ── Run sensitivity analysis ──────────────────────
                let sensitivity = ::fermi::sensitivity::full_sensitivity_analysis(
                    &parsed, 1000, // fewer iterations for speed
                );

                let sensitivity_summary = if let Ok(ref sa) = sensitivity {
                    let top = sa.top_drivers(5);
                    let parts: Vec<String> = top
                        .iter()
                        .map(|ds| {
                            format!(
                                "{} ({:.0}% influence)",
                                ds.driver_name,
                                ds.total_order_index * 100.0
                            )
                        })
                        .collect();
                    if parts.is_empty() {
                        "No sensitivity data available.".to_string()
                    } else {
                        format!("Key drivers by influence: {}", parts.join(", "))
                    }
                } else {
                    "Sensitivity analysis unavailable.".to_string()
                };

                // Store per-driver Sobol total-order indices so the
                // sensitivity bars render true influence, not raw spread.
                if let Ok(ref sa) = sensitivity {
                    self.driver_sensitivity.clear();
                    for d in parsed.drivers() {
                        if let Some(ds) = sa.get_driver_sensitivity(&d.name) {
                            self.driver_sensitivity
                                .insert(d.name.clone(), ds.total_order_index);
                        }
                    }
                }

                // Enrich the narrative explanation with sensitivity data
                if let Ok(ref sa) = sensitivity {
                    let top = sa.top_drivers(3);
                    if !top.is_empty() {
                        let top_names: Vec<String> = top
                            .iter()
                            .map(|ds| {
                                format!("{} ({:.0}%)", ds.driver_name, ds.total_order_index * 100.0)
                            })
                            .collect();
                        self.inside_view_explanation = format!(
                            "{} Most influential: {}.",
                            self.inside_view_explanation,
                            top_names.join(", "),
                        );
                    }

                    // ── Compute forecast confidence (Tetlock methodology) ──
                    // Per-driver confidence: use user override if set, else compute from evidence
                    let drivers = self.program.drivers();
                    let total_drivers = drivers.len() as f64;
                    let mut driver_conf_sum = 0.0_f64;
                    for d in &drivers {
                        if let Some(&uc) = self.driver_confidence.get(&d.name) {
                            // User explicitly set confidence for this driver
                            driver_conf_sum += uc;
                        } else {
                            // Compute from evidence coverage
                            let has_evidence = self.program.evidence_items().iter().any(|e| {
                                e.id.contains(&d.name)
                                    || self
                                        .program
                                        .agents()
                                        .iter()
                                        .filter(|a| a.driver_refs.contains(&d.name))
                                        .any(|a| evidence_matches_agent(e, &a.name))
                            });
                            driver_conf_sum += if has_evidence { 0.7 } else { 0.2 };
                        }
                    }
                    let avg_driver_conf = if total_drivers > 0.0 {
                        driver_conf_sum / total_drivers
                    } else {
                        0.3
                    };
                    let divergence_penalty = if divergence.abs() > 30.0 {
                        0.7
                    } else if divergence.abs() > 15.0 {
                        0.85
                    } else {
                        1.0
                    };
                    self.forecast_confidence =
                        (avg_driver_conf * divergence_penalty).clamp(0.1, 0.95);
                }
                // Build driver contribution summary
                let driver_summary: Vec<String> = self
                    .program
                    .drivers()
                    .iter()
                    .map(|d| {
                        let display = d.display_name.as_deref().unwrap_or(&d.name);
                        // Include sensitivity if available
                        let influence = sensitivity
                            .as_ref()
                            .ok()
                            .and_then(|sa| sa.get_driver_sensitivity(&d.name))
                            .map(|ds| format!(" [{:.0}%]", ds.total_order_index * 100.0))
                            .unwrap_or_default();
                        match d.driver_type {
                            DriverType::Continuous => {
                                if let Some(Distribution::Triangular { ref p50, .. }) =
                                    d.distribution
                                {
                                    format!(
                                        "{} (p50={:.2}){}",
                                        display,
                                        expr_to_f64(p50),
                                        influence
                                    )
                                } else {
                                    format!("{}{}", display, influence)
                                }
                            }
                            DriverType::Binary => {
                                format!(
                                    "{} ({:.0}%){}",
                                    display,
                                    d.probability.unwrap_or(0.0) * 100.0,
                                    influence
                                )
                            }
                            _ => format!("{}{}", display, influence),
                        }
                    })
                    .collect();

                self.messages.push(AssistantMessage {
                    node: "simulation".into(),
                    kind: MessageKind::Tip,
                    text: format!(
                        "🦊 Inside view: {:.4} (mean from {} drivers: {}). \
                         This is {:.0}pp {} the outside view base rate of {:.1}%.\n\
                         {}",
                        self.predicted_probability,
                        driver_summary.len(),
                        driver_summary.join(", "),
                        divergence.abs(),
                        div_direction,
                        base_rate * 100.0,
                        sensitivity_summary,
                    ),
                });

                if divergence.abs() > 20.0 {
                    self.messages.push(AssistantMessage {
                        node: "simulation".into(),
                        kind: MessageKind::Warning,
                        text: format!(
                            "⚠ Significant divergence ({:.0}pp) from base rate. \
                             Strong evidence needed to justify this difference.",
                            divergence.abs()
                        ),
                    });
                }
                self.messages.push(AssistantMessage {
                    node: "simulation".into(),
                    kind: MessageKind::Info,
                    text: format!(
                        "Simulation complete: mean={:.1}, p5={:.1}, p95={:.1} ({}ms)",
                        results.mean,
                        results.p5,
                        results.p95,
                        elapsed.as_millis()
                    ),
                });

                // ── Publish outputs to workspace ──────────────────
                // Enables cross-workspace reads (e.g., tournament path
                // reading team prior outputs).
                if let Some(ref ws_id) = self.workspace_id {
                    let api = self.api.clone();
                    let ws = ws_id.clone();
                    let prob = self.predicted_probability;
                    let conf = self.forecast_confidence;
                    let mean = results.mean;
                    let p5 = results.p5;
                    let p95 = results.p95;
                    let std_dev = results.std_dev;
                    // Collect Sobol indices if available
                    let sobol = sensitivity.as_ref().ok().map(|sa| {
                        let top = sa.top_drivers(10);
                        top.iter()
                            .map(|ds| {
                                (
                                    ds.driver_name.clone(),
                                    serde_json::json!({
                                        "first_order": ds.first_order_index,
                                        "total_order": ds.total_order_index,
                                    }),
                                )
                            })
                            .collect::<serde_json::Map<String, serde_json::Value>>()
                    });
                    // Driver scores
                    let driver_scores: serde_json::Map<String, serde_json::Value> = self
                        .program
                        .drivers()
                        .iter()
                        .map(|d| {
                            let val = match d.driver_type {
                                DriverType::Continuous => {
                                    if let Some(Distribution::Triangular { ref p50, .. }) =
                                        d.distribution
                                    {
                                        expr_to_f64(p50)
                                    } else {
                                        1.0
                                    }
                                }
                                DriverType::Binary => d.probability.unwrap_or(0.5),
                                _ => 1.0,
                            };
                            (d.name.clone(), serde_json::json!(val))
                        })
                        .collect();

                    tokio::spawn(async move {
                        // Publish predicted_probability
                        let _ = api
                            .set_workspace_output(
                                &ws,
                                "predicted_probability",
                                &serde_json::json!(prob),
                            )
                            .await;
                        // Publish forecast_confidence
                        let _ = api
                            .set_workspace_output(
                                &ws,
                                "forecast_confidence",
                                &serde_json::json!(conf),
                            )
                            .await;
                        // Publish simulation_results
                        let _ = api
                            .set_workspace_output(
                                &ws,
                                "simulation_results",
                                &serde_json::json!({
                                    "mean": mean, "p5": p5, "p95": p95, "std_dev": std_dev,
                                }),
                            )
                            .await;
                        // Publish driver_scores
                        let _ = api
                            .set_workspace_output(
                                &ws,
                                "driver_scores",
                                &serde_json::json!(driver_scores),
                            )
                            .await;
                        // Publish sobol_indices if available
                        if let Some(si) = sobol {
                            let _ = api
                                .set_workspace_output(&ws, "sobol_indices", &serde_json::json!(si))
                                .await;
                        }
                        log::info!("[workspace] Published outputs to {}", ws);
                    });
                }

                // ── Persist sim result to the server-side forecast ─────
                //
                // POST /api/forecasts/:id/update-probability writes a new
                // fermi_forecast_updates row, which the spacetime trigger
                // (migration 149) propagates into forecast_spacetime — the
                // table the Trajectory tab reads from. Without this every
                // sim run only updated cockpit-local state and the dashboard
                // probability + Trajectory tab stayed frozen at the cold-
                // start prior (2% for every team).
                //
                // Spec 23 §6 step 7: "The next time the editor runs the
                // forecast (manually or on schedule), the executor uses the
                // new posterior. The rate becomes 26%. fermi_forecast_updates
                // records the revision." — this is the wire.
                if let Some(ref fid) = self.forecast_id {
                    let api = self.api.clone();
                    let fid = fid.clone();
                    let new_prob = self.predicted_probability;
                    // The displayed value is provisional until the server
                    // recomposes mutex-group eliminations back in. Mark it so
                    // the headline reads "recomposing…" and saving is blocked
                    // — a save can then never disagree with the settled sim.
                    self.recomposing = true;
                    let reason = format!(
                        "Local Monte Carlo simulation: mean={:.4}, p5={:.4}, p95={:.4} ({} iterations)",
                        results.mean, results.p5, results.p95, results.iterations
                    );
                    cx.spawn(async move |this, cx| {
                        let req = crate::api::client::UpdateProbabilityRequest {
                            new_probability: new_prob,
                            reason: Some(reason),
                            agent_id: None,
                            evidence_added: None,
                        };
                        // Run the HTTP call on the tokio runtime. GPUI's
                        // executor doesn't drive tokio's I/O reactor, so
                        // awaiting reqwest directly inside cx.spawn fires the
                        // request but never resumes after the response — which
                        // left the displayed value stuck on the raw Monte-Carlo
                        // mean after Ctrl+R (the recomposed value only appeared
                        // after a manual save/refetch). Awaiting the tokio
                        // JoinHandle is reactor-free, so GPUI can drive it.
                        let outcome =
                            tokio::spawn(async move { api.update_probability(&fid, &req).await })
                                .await;
                        match outcome {
                            Ok(Ok(resp)) => {
                                // Server recomposes mutex-group eliminations
                                // into the displayed value; adopt it so a
                                // re-sim keeps eliminations priced in instead
                                // of dropping back to the standalone mean.
                                let recomposed = resp
                                    .get("recomposed_probability")
                                    .and_then(|v| v.as_f64());
                                this.update(cx, |state, cx| {
                                    state.recomposing = false;
                                    match recomposed {
                                        Some(p) if (p - new_prob).abs() > 1e-6 => {
                                            log::info!(
                                                "[sim-persist] standalone {:.4} → recomposed {:.4} (eliminations priced in)",
                                                new_prob, p
                                            );
                                            state.predicted_probability = p;
                                            // Explain the change so the jump from
                                            // the raw sim mean isn't a surprise.
                                            state.messages.push(AssistantMessage {
                                                node: "simulation".into(),
                                                kind: MessageKind::Info,
                                                text: format!(
                                                    "Recomposed: standalone {:.1}% → {:.1}% (mutex-group eliminations priced in)",
                                                    new_prob * 100.0,
                                                    p * 100.0
                                                ),
                                            });
                                        }
                                        _ => {}
                                    }
                                    cx.notify();
                                })
                                .ok();
                            }
                            Ok(Err(e)) => {
                                log::warn!("[sim-persist] update_probability failed: {}", e);
                                this.update(cx, |state, cx| {
                                    state.recomposing = false;
                                    cx.notify();
                                })
                                .ok();
                            }
                            Err(e) => {
                                log::warn!("[sim-persist] tokio join error: {}", e);
                                this.update(cx, |state, cx| {
                                    state.recomposing = false;
                                    cx.notify();
                                })
                                .ok();
                            }
                        }
                    })
                    .detach();
                }
            }
            Err(e) => {
                self.sim_error = Some(format!("Execution error: {:?}", e));
                self.sim_running = false;
                self.messages.push(AssistantMessage {
                    node: "simulation".into(),
                    kind: MessageKind::Error,
                    text: format!("Simulation failed: {:?}", e),
                });
            }
        }
        cx.notify();
    }

    // ═══════════════════════════════════════════════════════════════
    // FPL Source View
    // ═══════════════════════════════════════════════════════════════

    pub fn refresh_fpl_cache(&mut self, _cx: &App) {
        self.regenerate_cached_fpl_if_safe();
    }

    /// Rebuild `cached_fpl` from `self.program` ONLY if doing so won't lose
    /// information.
    ///
    /// `generate_fpl_text` is a partial serializer — it covers question +
    /// drivers + a handful of model expression shapes the cockpit AST can
    /// represent, but it does NOT round-trip factor blocks
    /// (`factor X1 {...}`), Cobb-Douglas `estimate ... as:` lines, or
    /// `learnable(mean, sd)` priors. Calling it on a workspace-backed team
    /// prior FPL strips all of those, leaving a 2-driver shell that won't
    /// simulate.
    ///
    /// Guard: if the current cached_fpl already contains a richer construct
    /// than the generator can emit, leave it alone. The user is presumably
    /// looking at the loaded FPL in the FPL tab; they don't want it nuked
    /// because they toggled a learnable flag or saved a version.
    ///
    /// Call sites that need to commit user-edited driver state into the
    /// cached FPL (toggle learnable, save version, export wiki, run sim)
    /// all go through here. The cost: for true factor-model forecasts, the
    /// FPL tab won't reflect cockpit-side driver edits until the user
    /// publishes via the server PUT path (which uses the program AST). This
    /// is the right trade-off until the cockpit AST grows real
    /// factor/estimate/learnable representations.
    fn regenerate_cached_fpl_if_safe(&mut self) {
        if Self::cached_fpl_is_richer_than_ast(&self.cached_fpl) {
            return;
        }
        self.cached_fpl = generate_fpl_text(&self.program);
    }

    /// Returns true when cached_fpl contains constructs that the
    /// cockpit AST + `generate_fpl_text` cannot round-trip.
    ///
    /// Today the emitter only knows about question + driver + evidence +
    /// model + simulate. Anything else (param declarations, agent blocks,
    /// factor / estimate / learnable() expressions) gets silently dropped
    /// if we round-trip through the AST. The team-prior template uses
    /// `param`, `agent`, and `feeds_from` — and previous template revisions
    /// used `factor` / `estimate` / `learnable()` — so the guard checks
    /// for any of them.
    pub(crate) fn cached_fpl_is_richer_than_ast(fpl: &str) -> bool {
        if fpl.is_empty() {
            return false;
        }
        // Use line-start matching where possible to avoid false positives
        // from prose ("the macro factor agent reads..."). agent / param at
        // line start is unambiguously a statement keyword.
        let has_top_level = |kw: &str| fpl.starts_with(kw) || fpl.contains(&format!("\n{}", kw));
        has_top_level("agent ")
            || has_top_level("param ")
            || has_top_level("factor ")
            || has_top_level("estimate ")
            || fpl.contains("learnable(")
            || fpl.contains("feeds_from")
    }

    // ═══════════════════════════════════════════════════════════════
    // Publish + Version
    // ═══════════════════════════════════════════════════════════════

    pub fn load_forecast(&mut self, path: &str, cx: &mut Context<Self>) {
        self.messages.clear();
        let state_path = path.replace(".fpl", ".state.json");

        // Reset all forecast-scoped state before loading the new one. Without
        // this, opening a forecast that has NO polymarket link leaves the
        // pm_* fields populated with the *previous* forecast's PM data —
        // surfacing one forecast's Polymarket event as if it belonged to
        // every forecast.
        //
        // Same reasoning for forecast_id, workspace_id, sim_results, versions,
        // and agent_runs: every one of them was previously restored
        // conditionally with no clear-on-absence branch.
        self.forecast_id = None;
        self.workspace_id = None;
        self.pm_event_id = None;
        self.pm_market_id = None;
        self.pm_question = None;
        self.pm_market_price = None;
        self.pm_volume_24h = None;
        self.pm_liquidity = None;
        self.pm_confidence = None;
        self.pm_price_change_1w = None;
        self.pm_url = None;
        self.pm_price_history.clear();
        self.pm_poll_interval = None;
        self.versions.clear();
        self.current_version = 0;
        self.sim_results = None;
        self.agent_runs.clear();
        self.driver_confidence.clear();
        self.inside_view_explanation.clear();
        self.hovered_histogram_bin = None;
        self.hovered_index_version = None;

        // Try to parse FPL — may fail on old files with bad evidence strings
        let mut fpl_parsed = false;
        if let Ok(fpl_text) = std::fs::read_to_string(path) {
            self.cached_fpl = fpl_text.clone();
            if let Ok(tokens) = ::fermi::lexer::Lexer::new(&fpl_text).tokenize() {
                if let Ok(program) = ::fermi::parser::Parser::new(tokens).parse() {
                    self.program = program;
                    fpl_parsed = true;
                    self.messages.push(AssistantMessage {
                        node: "load".into(),
                        kind: MessageKind::Info,
                        text: format!("Loaded FPL from {}", path),
                    });
                } else {
                    self.messages.push(AssistantMessage {
                        node: "load".into(),
                        kind: MessageKind::Warning,
                        text: "FPL parse had errors — loading state from backup.".into(),
                    });
                }
            }
        }

        // Update question input from whatever we parsed
        if let Some(q) = self.program.question() {
            self.question_input.update(cx, |input, cx| {
                input.set_text(&q.text, cx);
            });
            if let Some(ref br) = q.base_rate {
                self.predicted_probability = br.historical_frequency;
            }
        }

        // Always try state.json — it has evidence, versions, base rate
        if let Ok(state_text) = std::fs::read_to_string(&state_path) {
            if let Ok(state_json) = serde_json::from_str::<JsonValue>(&state_text) {
                // Restore forecast_id (set when forecast was previously published)
                if let Some(fid) = state_json.get("forecast_id").and_then(|v| v.as_str()) {
                    self.forecast_id = Some(fid.to_string());
                }
                // Restore probability
                if let Some(prob) = state_json
                    .get("predicted_probability")
                    .and_then(|v| v.as_f64())
                {
                    self.predicted_probability = prob.clamp(0.01, 0.99);
                }
                // Restore version history
                if let Some(versions) = state_json.get("versions").and_then(|v| v.as_array()) {
                    self.versions = versions
                        .iter()
                        .filter_map(|v| {
                            Some(ForecastVersion {
                                version: v.get("version")?.as_u64()? as u32,
                                timestamp: v.get("timestamp")?.as_str()?.to_string(),
                                fpl_text: String::new(),
                                probability: v.get("probability")?.as_f64()?,
                                change_summary: v.get("change_summary")?.as_str()?.to_string(),
                            })
                        })
                        .collect();
                    self.current_version = self.versions.last().map(|v| v.version).unwrap_or(0);
                }
                // Restore sim results
                if let Some(sim) = state_json.get("sim_results").and_then(|v| v.as_object()) {
                    self.sim_results = Some(SimResults {
                        mean: sim.get("mean").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        median: sim.get("median").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        p5: sim.get("p5").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        p95: sim.get("p95").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        std_dev: sim.get("std_dev").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        iterations: sim.get("iterations").and_then(|v| v.as_u64()).unwrap_or(0),
                        execution_time_ms: 0,
                        histogram: vec![],
                        bin_starts: vec![],
                        bin_width: 0.0,
                        // Loaded forecasts predating learnable drivers won't
                        // have this field in state.json — start empty, the
                        // next live sim fills it in.
                        learnable_drivers: vec![],
                    });
                }
                // Restore base rate into AST
                if let Some(br) = state_json.get("base_rate").and_then(|v| v.as_object()) {
                    if let Some(q) = self.program.question_mut() {
                        q.base_rate = Some(BaseRate {
                            reference_class: br
                                .get("reference_class")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            historical_frequency: br
                                .get("historical_frequency")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0),
                            sample_size: br
                                .get("sample_size")
                                .and_then(|v| v.as_u64())
                                .map(|n| n as usize),
                            source: br
                                .get("source")
                                .and_then(|v| v.as_str())
                                .unwrap_or("restored")
                                .to_string(),
                            reasoning: br
                                .get("reasoning")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            generated_by: GeneratedBy::Agent("fermi".into()),
                        });
                    }
                }
                // Restore inside view explanation
                if let Some(expl) = state_json
                    .get("inside_view_explanation")
                    .and_then(|v| v.as_str())
                {
                    self.inside_view_explanation = expl.to_string();
                }
                // Restore confidence
                if let Some(conf) = state_json
                    .get("forecast_confidence")
                    .and_then(|v| v.as_f64())
                {
                    self.forecast_confidence = conf;
                }
                // Restore per-driver confidence overrides
                if let Some(dc) = state_json
                    .get("driver_confidence")
                    .and_then(|v| v.as_object())
                {
                    for (driver_name, val) in dc {
                        if let Some(c) = val.as_f64() {
                            self.driver_confidence.insert(driver_name.clone(), c);
                        }
                    }
                    log::info!(
                        "[load] Restored {} driver confidence overrides",
                        self.driver_confidence.len()
                    );
                }
                // Restore agents into AST
                if let Some(agent_arr) = state_json.get("agents").and_then(|v| v.as_array()) {
                    for ag in agent_arr {
                        let name = ag
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() && self.program.agent(&name).is_none() {
                            let driver_refs: Vec<String> = ag
                                .get("driver_refs")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                        .collect()
                                })
                                .unwrap_or_default();
                            self.program.add_agent(AgentStmt {
                                name,
                                agent_type: ag
                                    .get("agent_type")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                query: ag
                                    .get("query")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                executor: Some(fermi::ast::ExecutorType::LLM),
                                schedule: Some(Schedule::Once),
                                driver_refs,
                                depends_on: vec![],
                                confidence_threshold: None,
                            });
                        }
                    }
                    log::info!("[load] Restored {} agents", agent_arr.len());
                    // Initialize agent_runs from restored agents
                    for ag in agent_arr {
                        let name = ag
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() && !self.agent_runs.iter().any(|r| r.agent_name == name)
                        {
                            let ev_count = self
                                .program
                                .evidence_items()
                                .iter()
                                .filter(|e| evidence_matches_agent(e, &name))
                                .count();
                            self.agent_runs.push(AgentExecution {
                                agent_name: name,
                                status: if ev_count > 0 {
                                    AgentRunStatus::Completed
                                } else {
                                    AgentRunStatus::Idle
                                },
                                evidence_count: ev_count,
                                confidence: None,
                                error: None,
                                credits_charged: None,
                                started_at: None,
                                completed_at: None,
                                latest_finding: None,
                            });
                        }
                    }
                }
                // Restore evidence into AST (supplement what FPL parsing got)
                log::info!("[load] Restoring evidence from state.json");
                if let Some(ev_arr) = state_json.get("evidence").and_then(|v| v.as_array()) {
                    for ev in ev_arr {
                        let id = ev
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !id.is_empty()
                            && self.program.evidence_items().iter().all(|e| e.id != id)
                        {
                            self.program.add_evidence(EvidenceStmt {
                                id,
                                source: ev
                                    .get("source")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                summary: ev
                                    .get("summary")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                url: None,
                                relevance: ev.get("relevance").and_then(|v| v.as_f64()),
                                date: ev
                                    .get("date")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                                strength: ev.get("relevance").and_then(|v| v.as_f64()),
                                key_findings: ev
                                    .get("key_findings")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            });
                        }
                    }
                }
                let ev_count_log = self.program.evidence_items().len();
                log::info!("[load] After restore: {} evidence in AST", ev_count_log);
                for ev in self.program.evidence_items() {
                    log::info!("[load]   ev id={} source={}", ev.id, ev.source);
                }
                log::info!("[load] Drivers: {}", self.program.drivers().len());
                for d in self.program.drivers() {
                    log::info!("[load]   driver={}", d.name);
                }
                log::info!("[load] Agents: {}", self.program.agents().len());
                for a in self.program.agents() {
                    log::info!("[load]   agent={} refs={:?}", a.name, a.driver_refs);
                }
                // Restore Polymarket link
                if let Some(pm) = state_json.get("polymarket").and_then(|v| v.as_object()) {
                    self.pm_event_id = pm
                        .get("event_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pm_market_id = pm
                        .get("market_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pm_question = pm
                        .get("question")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pm_market_price = pm.get("market_price").and_then(|v| v.as_f64());
                    self.pm_volume_24h = pm.get("volume_24h").and_then(|v| v.as_f64());
                    self.pm_liquidity = pm.get("liquidity").and_then(|v| v.as_f64());
                    self.pm_confidence = pm
                        .get("confidence")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    self.pm_price_change_1w = pm.get("price_change_1w").and_then(|v| v.as_f64());
                    self.pm_url = pm
                        .get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    // Restore price history
                    if let Some(hist) = pm.get("price_history").and_then(|v| v.as_array()) {
                        self.pm_price_history = hist
                            .iter()
                            .filter_map(|h| {
                                let t = h.get("t").and_then(|v| v.as_u64())?;
                                let p = h.get("p").and_then(|v| v.as_f64())?;
                                Some((t, p))
                            })
                            .collect();
                    }
                    if self.pm_event_id.is_some() {
                        log::info!("[load] Restored Polymarket link: event={}, price={:.1}%, {} history points",
                            self.pm_event_id.as_deref().unwrap_or("?"),
                            self.pm_market_price.unwrap_or(0.0) * 100.0,
                            self.pm_price_history.len());
                        // Auto-resume PM polling at 5-minute interval
                        self.pm_poll_interval = Some(std::time::Duration::from_secs(5 * 60));
                        // Fire one immediate snapshot so the crowd
                        // number is live-accurate on restore rather
                        // than the last-saved value.
                        self.refresh_pm_price_now(cx);
                    }
                }
                // Restore workspace_id
                if let Some(ws_id) = state_json.get("workspace_id").and_then(|v| v.as_str()) {
                    self.workspace_id = Some(ws_id.to_string());
                    // Spec 23 R-2: prime the pending-fits state from the server
                    // so sparklines render the right badge immediately on load.
                    self.load_bayesops_state(cx);
                }

                self.messages.push(AssistantMessage {
                    node: "load".into(),
                    kind: MessageKind::Info,
                    text: format!(
                        "Restored v{} — {:.2}% (confidence: {:.0}%)",
                        self.current_version,
                        self.predicted_probability * 100.0,
                        self.forecast_confidence * 100.0
                    ),
                });
            }
        } else if !fpl_parsed {
            self.messages.push(AssistantMessage {
                node: "load".into(),
                kind: MessageKind::Error,
                text: format!("Failed to load {}", path),
            });
        }

        self.focused_node = FocusedNode::Question;
        self.right_tab = RightTab::Wiki;
        // Load persisted schedules if this forecast was previously published
        self.load_schedules(cx);
        cx.notify();
    }
    pub fn save_forecast(&mut self, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.regenerate_cached_fpl_if_safe();

        // Create version snapshot with descriptive change summary
        self.current_version += 1;
        let change_summary = if self.current_version == 1 {
            let driver_count = self.program.drivers().len();
            let ev_count = self.program.evidence_items().len();
            let base_rate = self
                .program
                .question()
                .and_then(|q| q.base_rate.as_ref())
                .map(|br| format!(" base={:.0}%", br.historical_frequency * 100.0))
                .unwrap_or_default();
            format!(
                "Initial: {:.1}%{}, {} drivers, {} evidence",
                self.predicted_probability * 100.0,
                base_rate,
                driver_count,
                ev_count
            )
        } else {
            let prev = self.versions.last();
            let prev_prob = prev
                .map(|v| v.probability)
                .unwrap_or(self.predicted_probability);
            let delta = (self.predicted_probability - prev_prob) * 100.0;
            let delta_str = if delta.abs() < 0.5 {
                "→".to_string()
            } else if delta > 0.0 {
                format!("+{:.0}pp", delta)
            } else {
                format!("{:.0}pp", delta)
            };
            let driver_count = self.program.drivers().len();
            let ev_count = self.program.evidence_items().len();
            let agent_count = self
                .agent_runs
                .iter()
                .filter(|r| r.status == AgentRunStatus::Completed)
                .count();
            let mut parts = vec![format!(
                "{:.1}% ({})",
                self.predicted_probability * 100.0,
                delta_str
            )];
            if driver_count > 0 {
                parts.push(format!("{} drivers", driver_count));
            }
            if ev_count > 0 {
                parts.push(format!("{} evidence", ev_count));
            }
            if agent_count > 0 {
                parts.push(format!("{} agents", agent_count));
            }
            parts.join(", ")
        };
        self.versions.push(ForecastVersion {
            version: self.current_version,
            timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            fpl_text: self.cached_fpl.clone(),
            probability: self.predicted_probability,
            change_summary,
        });

        // Save to disk
        let filename = self
            .program
            .question()
            .map(|q| sanitize_name(&q.text))
            .unwrap_or_else(|| "forecast".into());
        let path = format!("forecasts/{}.fpl", filename);

        // Ensure directory exists
        let _ = std::fs::create_dir_all("forecasts");

        match std::fs::write(&path, &self.cached_fpl) {
            Ok(_) => {
                log::info!("[composer] Saved FPL to {}", path);
                log::info!(
                    "[composer] Evidence in AST: {}, Drivers: {}",
                    self.program.evidence_items().len(),
                    self.program.drivers().len()
                );
                self.messages.push(AssistantMessage {
                    node: "save".into(),
                    kind: MessageKind::Info,
                    text: format!("Saved v{} to {}", self.current_version, path),
                });
                self.publish_status = Some(format!("Saved v{}", self.current_version));

                // Also save evidence wiki
                let wiki_path = format!("forecasts/{}.evidence.md", filename);
                let wiki = generate_evidence_wiki(
                    &self.program,
                    self.current_version,
                    self.predicted_probability,
                    &self.inside_view_explanation,
                    self.forecast_confidence,
                    self.pm_market_price,
                    self.pm_url.as_deref(),
                    self.pm_volume_24h,
                    self.pm_confidence.as_deref(),
                    self.pm_price_change_1w,
                    self.sim_results.as_ref(),
                    &self.versions,
                );
                match std::fs::write(&wiki_path, &wiki) {
                    Ok(_) => log::info!("[composer] Saved evidence wiki to {}", wiki_path),
                    Err(e) => log::warn!("[composer] Failed to save evidence wiki: {}", e),
                }

                // Save state.json (versions, probability, sim results)
                let state_path = format!("forecasts/{}.state.json", filename);
                let state_json = serde_json::json!({
                    "forecast_id": self.forecast_id,
                    "current_version": self.current_version,
                    "predicted_probability": self.predicted_probability,
                    "inside_view_explanation": self.inside_view_explanation,
                    "forecast_confidence": self.forecast_confidence,
                    "versions": self.versions.iter().map(|v| serde_json::json!({
                        "version": v.version,
                        "timestamp": v.timestamp,
                        "probability": v.probability,
                        "change_summary": v.change_summary,
                    })).collect::<Vec<_>>(),
                    "sim_results": self.sim_results.as_ref().map(|s| serde_json::json!({
                        "mean": s.mean,
                        "median": s.median,
                        "p5": s.p5,
                        "p95": s.p95,
                        "std_dev": s.std_dev,
                        "iterations": s.iterations,
                    })),
                    "base_rate": self.program.question()
                        .and_then(|q| q.base_rate.as_ref())
                        .map(|br| serde_json::json!({
                            "reference_class": br.reference_class,
                            "historical_frequency": br.historical_frequency,
                            "sample_size": br.sample_size,
                            "source": br.source,
                            "reasoning": br.reasoning,
                        })),
                    "evidence": self.program.evidence_items().iter().map(|e| serde_json::json!({
                        "id": e.id,
                        "source": e.source,
                        "summary": e.summary,
                        "relevance": e.relevance,
                        "date": e.date,
                        "key_findings": e.key_findings,
                    })).collect::<Vec<_>>(),
                    "agents": self.program.agents().iter().map(|a| serde_json::json!({
                        "name": a.name,
                        "agent_type": a.agent_type,
                        "query": a.query,
                        "schedule": format!("{:?}", a.schedule),
                        "driver_refs": a.driver_refs,
                    })).collect::<Vec<_>>(),
                    "driver_confidence": self.driver_confidence.iter()
                        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                        .collect::<serde_json::Map<String, JsonValue>>(),
                    "polymarket": serde_json::json!({
                        "event_id": self.pm_event_id,
                        "market_id": self.pm_market_id,
                        "question": self.pm_question,
                        "market_price": self.pm_market_price,
                        "volume_24h": self.pm_volume_24h,
                        "liquidity": self.pm_liquidity,
                        "confidence": self.pm_confidence,
                        "price_change_1w": self.pm_price_change_1w,
                        "url": self.pm_url,
                        "price_history": self.pm_price_history.iter()
                            .map(|(ts, p)| serde_json::json!({"t": ts, "p": p}))
                            .collect::<Vec<_>>(),
                    }),
                    "workspace_id": self.workspace_id,
                });
                match std::fs::write(
                    &state_path,
                    serde_json::to_string_pretty(&state_json).unwrap_or_default(),
                ) {
                    Ok(_) => log::info!("[composer] Saved state to {}", state_path),
                    Err(e) => log::warn!("[composer] Failed to save state: {}", e),
                }

                // ── Git auto-commit: atomic version of all three artifacts ──
                // The .fpl, .evidence.md, and .state.json are committed together
                // as one version snapshot. The commit message includes the
                // probability change so `git log` reads as a version history.
                let prev_prob = if self.versions.len() >= 2 {
                    self.versions[self.versions.len() - 2].probability
                } else {
                    self.predicted_probability
                };
                let delta = (self.predicted_probability - prev_prob) * 100.0;
                let delta_str = if delta.abs() < 0.5 {
                    "no change".to_string()
                } else if delta > 0.0 {
                    format!("+{:.0}pp", delta)
                } else {
                    format!("{:.0}pp", delta)
                };
                let commit_msg = format!(
                    "v{}: {:.1}% ({}) — {}",
                    self.current_version,
                    self.predicted_probability * 100.0,
                    delta_str,
                    self.program
                        .question()
                        .map(|q| q.text.chars().take(60).collect::<String>())
                        .unwrap_or_else(|| "forecast".into()),
                );

                // git add all three files + git commit
                // Paths are already relative to repo root (e.g., "forecasts/name.fpl")
                match std::process::Command::new("git")
                    .args(&["add", &path, &wiki_path, &state_path])
                    .output()
                {
                    Ok(_) => {
                        match std::process::Command::new("git")
                            .args(&["commit", "-m", &commit_msg, "--allow-empty"])
                            .output()
                        {
                            Ok(output) => {
                                if output.status.success() {
                                    log::info!("[composer] Git committed: {}", commit_msg);
                                    self.messages.push(AssistantMessage {
                                        node: "save".into(),
                                        kind: MessageKind::Info,
                                        text: format!("📦 Committed: {}", commit_msg),
                                    });
                                } else {
                                    let stderr = String::from_utf8_lossy(&output.stderr);
                                    log::warn!("[composer] Git commit warning: {}", stderr);
                                }
                            }
                            Err(e) => log::warn!("[composer] Git commit failed: {}", e),
                        }
                    }
                    Err(e) => log::warn!("[composer] Git add failed: {}", e),
                }
            }
            Err(e) => {
                log::error!("[composer] Failed to save: {}", e);
                self.messages.push(AssistantMessage {
                    node: "save".into(),
                    kind: MessageKind::Error,
                    text: format!("Save failed: {}", e),
                });
            }
        }
        cx.notify();
    }

    // ── Server lifecycle reconciliation ────────────────────────────────

    /// True when the server has resolved/voided this forecast — the cockpit
    /// must then block re-sims and new snapshots. Driven by the authoritative
    /// `forecast_status`, not by anything the user has to remember.
    pub fn is_locked(&self) -> bool {
        matches!(
            self.forecast_status.as_deref(),
            Some("resolved") | Some("void")
        )
    }

    /// A human-readable reason for the lock, e.g.
    /// "Resolved → No · Auto-resolved via Polymarket …".
    pub fn lock_reason(&self) -> Option<String> {
        if !self.is_locked() {
            return None;
        }
        let verb = match self.forecast_status.as_deref() {
            Some("void") => "Voided".to_string(),
            _ => {
                let outcome = match self.forecast_outcome {
                    Some(true) => " → Yes",
                    Some(false) => " → No",
                    None => "",
                };
                format!("Resolved{}", outcome)
            }
        };
        match &self.resolution_note {
            Some(note) if !note.is_empty() => Some(format!("{} · {}", verb, note)),
            _ => Some(verb),
        }
    }

    /// Re-pull the authoritative lifecycle state from the server and adopt
    /// it. This is the "reconcile server context" action: a stale cockpit
    /// (opened while active, resolved server-side since) snaps to the real
    /// state — status, outcome, the resolved probability — and locks itself.
    pub fn reconcile_forecast(&mut self, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            return;
        };
        self.reconciling = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = tokio::spawn(async move { api.get_forecast(&fid).await }).await;
            this.update(cx, |state, cx| {
                state.reconciling = false;
                if let Ok(Ok(f)) = result {
                    let was_locked = state.is_locked();
                    state.forecast_status = Some(f.status.clone());
                    state.forecast_outcome = f.actual_outcome;
                    state.resolution_note = f.resolution_notes.clone();
                    // When locked, show the authoritative resolved value
                    // rather than a stale local sim mean.
                    if state.is_locked() {
                        state.predicted_probability = f.predicted_probability;
                        if !was_locked {
                            state.pending_toasts.push(
                                "This forecast was resolved on the server — locked for editing"
                                    .to_string(),
                            );
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Access / sharing (Spec 24 §3.5.2) ──────────────────────────────

    /// Fetch the current shares for this forecast into the Access tab.
    /// Also kicks off a parallel invite-list fetch so the operator sees
    /// outbound pending invitations alongside materialised shares.
    pub fn load_shares(&mut self, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            return;
        };
        self.shares_loading = true;
        self.share_error = None;
        self.shares_loaded_for = Some(fid.clone());
        cx.notify();

        let api = self.api.clone();
        let fid_clone = fid.clone();
        cx.spawn(async move |this, cx| {
            let result = api.list_forecast_shares(&fid).await;
            this.update(cx, |state, cx| {
                state.shares_loading = false;
                match result {
                    Ok(resp) => state.shares = resp.shares,
                    Err(e) => state.share_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();

        self.load_forecast_invites(fid_clone, cx);
        // Populate the "Share with a team" section's dropdown.
        // Cheap noop when teams are already loaded.
        self.load_share_teams(cx);
    }

    /// Fetch pending/terminal invites for this forecast into
    /// `forecast_invites`. Called by `load_shares` and after every
    /// send/revoke so the Access tab stays in sync.
    pub fn load_forecast_invites(&mut self, forecast_id: String, cx: &mut Context<Self>) {
        self.forecast_invites_loading = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.list_forecast_invites(&forecast_id).await;
            this.update(cx, |state, cx| {
                state.forecast_invites_loading = false;
                match result {
                    Ok(resp) => {
                        if state.forecast_id.as_deref() == Some(forecast_id.as_str()) {
                            state.forecast_invites = resp.invites;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch forecast invites: {}", e);
                        // Non-fatal — keep existing list.
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch the operator's collaboration teams so the Access tab can
    /// offer them as share targets. We filter locally against the same
    /// workspace-prior heuristic the Teams panel uses — sharing a
    /// forecast with a Team-Prior workspace team makes no sense.
    pub fn load_share_teams(&mut self, cx: &mut Context<Self>) {
        if self.share_teams_loading || !self.share_teams.is_empty() {
            return;
        }
        self.share_teams_loading = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.list_my_teams().await;
            this.update(cx, |state, cx| {
                state.share_teams_loading = false;
                match result {
                    Ok(resp) => {
                        state.share_teams = resp
                            .teams
                            .into_iter()
                            .filter(is_forecast_collaboration_team)
                            .collect();
                    }
                    Err(e) => {
                        log::warn!("[access] load_share_teams failed: {}", e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Share this forecast with a team at the currently-selected
    /// permission. Idempotent server-side (ON CONFLICT DO UPDATE), so
    /// re-clicking a team upgrades/downgrades its role.
    pub fn share_with_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            self.share_error = Some("Publish the forecast first (Ctrl+P).".into());
            cx.notify();
            return;
        };
        if !self.share_team_in_flight.insert(team_id.clone()) {
            return;
        }
        self.share_error = None;
        cx.notify();
        let api = self.api.clone();
        let permission = self.share_permission.clone();
        cx.spawn(async move |this, cx| {
            let body = ShareRequest {
                share_type: "team".into(),
                share_target: team_id.clone(),
                permission: Some(permission),
            };
            let result = api.add_forecast_share(&fid, &body).await;
            this.update(cx, |state, cx| {
                state.share_team_in_flight.remove(&team_id);
                match result {
                    Ok(_) => {
                        // Refresh shares so the new team-share row
                        // appears immediately with the enriched
                        // display name.
                        state.load_shares(cx);
                    }
                    Err(e) => {
                        state.share_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Revoke a pending forecast invite by id, then refresh.
    pub fn revoke_forecast_invite(&mut self, invite_id: String, cx: &mut Context<Self>) {
        if !self
            .forecast_invite_revoke_in_flight
            .insert(invite_id.clone())
        {
            return;
        }
        cx.notify();
        let api = self.api.clone();
        let fid = self.forecast_id.clone();
        cx.spawn(async move |this, cx| {
            let result = api.revoke_invite(&invite_id).await;
            this.update(cx, |state, cx| {
                state.forecast_invite_revoke_in_flight.remove(&invite_id);
                match result {
                    Ok(()) => {
                        if let Some(fid) = fid {
                            state.load_forecast_invites(fid, cx);
                        }
                    }
                    Err(e) => {
                        state.share_error = Some(format!("Revoke failed: {}", e));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Add a collaborator from the share input. Email targets are resolved
    /// via `/api/users/lookup`: a hit becomes an instant user-share, a miss
    /// becomes a pending email invite. Non-email input is treated as a
    /// `user_id` and shared directly.
    pub fn add_share_from_input(&mut self, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            self.share_error = Some("Publish the forecast first (Ctrl+P).".into());
            cx.notify();
            return;
        };
        let raw = self.share_input.read(cx).text().trim().to_string();
        if raw.is_empty() {
            return;
        }
        self.share_add_loading = true;
        self.share_error = None;
        cx.notify();

        let api = self.api.clone();
        let permission = self.share_permission.clone();
        let share_input = self.share_input.clone();
        cx.spawn(async move |this, cx| {
            let is_email = raw.contains('@');
            // Resolve emails to a user_id when an account exists.
            let resolved_user = if is_email {
                api.lookup_user(&raw)
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.user_id)
            } else {
                Some(raw.clone())
            };

            let result: Result<String, String> = match resolved_user {
                Some(user_id) => {
                    // Known principal → direct user share.
                    let body = ShareRequest {
                        share_type: "user".into(),
                        share_target: user_id,
                        permission: Some(permission.clone()),
                    };
                    api.add_forecast_share(&fid, &body)
                        .await
                        .map(|_| "Shared".to_string())
                        .map_err(|e| e.to_string())
                }
                None => {
                    // Unknown email → pending email invite.
                    let body = InviteRequest {
                        invitee_user_id: None,
                        invitee_email: Some(raw.clone()),
                        permission: permission.clone(),
                        message: None,
                    };
                    api.invite_to_forecast(&fid, &body)
                        .await
                        .map(|_| "Invite sent".to_string())
                        .map_err(|e| e.to_string())
                }
            };

            this.update(cx, |state, cx| {
                state.share_add_loading = false;
                match result {
                    Ok(_) => {
                        share_input.update(cx, |inp, cx| inp.set_text("", cx));
                        state.load_shares(cx);
                    }
                    Err(e) => state.share_error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Revoke a share by its object_shares id, then refresh the list.
    pub fn revoke_share(&mut self, share_id: String, cx: &mut Context<Self>) {
        let Some(fid) = self.forecast_id.clone() else {
            return;
        };
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.revoke_forecast_share(&fid, &share_id).await;
            this.update(cx, |state, cx| {
                match result {
                    Ok(()) => state.load_shares(cx),
                    Err(e) => state.share_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Apply commit-sheet share targets to a freshly-published forecast.
    /// Each (target, permission) is resolved the same way the Access tab
    /// resolves a manual add: emails hit `/api/users/lookup` (instant share
    /// on hit, pending email invite on miss); anything else is a user_id
    /// share. Runs in one background task so the Commit click stays snappy.
    pub fn apply_publish_shares(
        &mut self,
        fid: String,
        targets: Vec<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        if targets.is_empty() {
            return;
        }
        let api = self.api.clone();
        let total = targets.len();
        cx.spawn(async move |this, cx| {
            let mut ok = 0usize;
            let mut failures: Vec<String> = Vec::new();
            for (raw, permission) in targets {
                let is_email = raw.contains('@');
                let resolved = if is_email {
                    api.lookup_user(&raw)
                        .await
                        .ok()
                        .flatten()
                        .map(|u| u.user_id)
                } else {
                    Some(raw.clone())
                };
                let res = match resolved {
                    Some(user_id) => {
                        let body = ShareRequest {
                            share_type: "user".into(),
                            share_target: user_id,
                            permission: Some(permission),
                        };
                        api.add_forecast_share(&fid, &body).await.map(|_| ())
                    }
                    None => {
                        let body = InviteRequest {
                            invitee_user_id: None,
                            invitee_email: Some(raw.clone()),
                            permission,
                            message: None,
                        };
                        api.invite_to_forecast(&fid, &body).await.map(|_| ())
                    }
                };
                match res {
                    Ok(()) => ok += 1,
                    Err(e) => {
                        log::error!("[publish-share] {} failed: {}", raw, e);
                        failures.push(format!("{} ({})", raw, e));
                    }
                }
            }
            // Surface the outcome — a silent swallow here is exactly the
            // "I shared and nothing happened" failure mode.
            this.update(cx, |state, cx| {
                if !failures.is_empty() {
                    state.publish_status = Some(format!(
                        "Published, but {}/{} share(s) failed: {}",
                        failures.len(),
                        total,
                        failures.join("; ")
                    ));
                    state.pending_toasts.push(format!(
                        "{} share(s) failed — see publish status",
                        failures.len()
                    ));
                } else if ok > 0 {
                    state.pending_toasts.push(format!("Shared with {}", ok));
                }
                // Refresh the Access tab list so the new grants are visible.
                state.shares_loaded_for = None;
                state.load_shares(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn publish_forecast(&mut self, visibility: String, cx: &mut Context<Self>) {
        // Reconcile-derived lock: the server has settled this forecast, so a
        // new snapshot would be rejected (409) anyway. Block locally with the
        // authoritative reason instead of letting the save silently fail.
        if self.is_locked() {
            self.publish_status = Some(format!(
                "Locked: {}. Saving new snapshots is disabled.",
                self.lock_reason()
                    .unwrap_or_else(|| "forecast is resolved".into())
            ));
            cx.notify();
            return;
        }
        // Don't save mid-recompose: the displayed value is still the raw sim
        // mean and would disagree with the value the sim settles on a moment
        // later. Wait for the recomposed value, then save.
        if self.recomposing {
            self.publish_status =
                Some("Recomposing eliminations — try save again in a moment.".into());
            cx.notify();
            return;
        }
        self.save_focused_driver(cx);
        // Same guard as run_simulation — never regenerate cached_fpl from
        // the AST if the loaded FPL is richer (factor blocks, agents,
        // params, learnable(), feeds_from). Otherwise publishing a
        // workspace-backed forecast would push a stripped 2-driver shell
        // to the server.
        self.regenerate_cached_fpl_if_safe();

        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        if question.is_empty() {
            self.publish_status = Some("No question defined.".into());
            cx.notify();
            return;
        }

        // Snapshot version
        self.current_version += 1;
        self.versions.push(ForecastVersion {
            version: self.current_version,
            timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            fpl_text: self.cached_fpl.clone(),
            probability: self.predicted_probability,
            change_summary: if self.current_version == 1 {
                "Initial forecast".into()
            } else {
                "Updated forecast".into()
            },
        });

        // ── Branch: update an existing forecast vs create a new one ───
        //
        // Without this branch every Save creates a duplicate row. Opening a
        // workspace-backed forecast (e.g. ARG, forecast_id pre-set) and
        // hitting Save produced a second 'Will Argentina win...' forecast
        // with no workspace_id, no base_rate, and probability stuck at 2%
        // — exactly the noisy duplicate the user saw.
        //
        // The decision rule is simple: if we know the forecast_id, PUT
        // updates to that row. Otherwise it's a genuinely-new draft, POST
        // to create. The PUT path uses UpdateForecastRequest which accepts
        // partial updates so we only ship the fields that may have moved.
        self.publish_status = Some("Publishing…".into());
        cx.notify();

        let api = self.api.clone();
        let fpl = self.cached_fpl.clone();
        let prob = self.predicted_probability;
        let res_crit = self
            .program
            .question()
            .and_then(|q| q.resolution_criteria.clone());
        let target_date = self.program.question().and_then(|q| q.target_date.clone());
        let ci_low = self.sim_results.as_ref().map(|s| s.p5);
        let ci_high = self.sim_results.as_ref().map(|s| s.p95);
        let sim_results_json = self.sim_results.as_ref().map(
            |s| serde_json::json!({ "mean": s.mean, "median": s.median, "p5": s.p5, "p95": s.p95 }),
        );
        let existing_fid = self.forecast_id.clone();

        cx.spawn(async move |this, cx| {
            let outcome: Result<(String, bool), String> = if let Some(fid) = existing_fid {
                // PUT — update the existing row, preserve workspace_id /
                // tags / portfolio memberships that the create payload
                // wouldn't carry. Build the JSON payload by hand because
                // the client's update_forecast takes a loose JsonValue —
                // we only ship the fields that may have moved.
                let mut updates = serde_json::Map::new();
                updates.insert("question_text".into(), serde_json::json!(question));
                updates.insert("predicted_probability".into(), serde_json::json!(prob));
                updates.insert("fpl_source".into(), serde_json::json!(fpl));
                updates.insert("visibility".into(), serde_json::json!(visibility));
                updates.insert("status".into(), serde_json::json!("active"));
                if let Some(ref s) = res_crit {
                    updates.insert("resolution_criteria".into(), serde_json::json!(s));
                }
                if let Some(ref s) = target_date {
                    updates.insert("target_date".into(), serde_json::json!(s));
                }
                if let Some(v) = ci_low {
                    updates.insert("confidence_interval_low".into(), serde_json::json!(v));
                }
                if let Some(v) = ci_high {
                    updates.insert("confidence_interval_high".into(), serde_json::json!(v));
                }
                if let Some(ref v) = sim_results_json {
                    updates.insert("simulation_results".into(), v.clone());
                }
                let body = serde_json::Value::Object(updates);
                api.update_forecast(&fid, &body)
                    .await
                    .map(|_| (fid, false))
                    .map_err(|e| e.to_string())
            } else {
                // POST — first-time publish.
                let req = CreateForecastRequest {
                    question_text: question,
                    predicted_probability: prob,
                    domain: None,
                    resolution_criteria: res_crit,
                    target_date,
                    confidence_interval_low: ci_low,
                    confidence_interval_high: ci_high,
                    fpl_source: Some(fpl),
                    simulation_results: sim_results_json,
                    drivers: None,
                    evidence: None,
                    visibility: Some(visibility),
                    tags: None,
                    portfolio_id: None,
                    status: Some("active".into()),
                };
                api.create_forecast(&req)
                    .await
                    .map(|resp| {
                        let fid = resp
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        (fid, true)
                    })
                    .map_err(|e| e.to_string())
            };

            match outcome {
                Ok((fid, created)) => {
                    this.update(cx, |state, cx| {
                        state.forecast_id = Some(fid.clone());
                        state.publish_status = Some(if created {
                            format!("Published v{}", state.current_version)
                        } else {
                            format!("Updated v{}", state.current_version)
                        });
                        state.messages.push(AssistantMessage {
                            node: "publish".into(),
                            kind: MessageKind::Info,
                            text: if created {
                                format!(
                                    "Forecast published as v{} (ID: {})",
                                    state.current_version, fid
                                )
                            } else {
                                format!(
                                    "Forecast updated to v{} (ID: {})",
                                    state.current_version, fid
                                )
                            },
                        });
                        // Load any existing schedules now that we have a forecast_id
                        state.load_schedules(cx);
                        // Apply any share targets collected in the commit sheet
                        // now that the forecast row (and its id) exists.
                        if !state.pending_publish_shares.is_empty() {
                            let targets = std::mem::take(&mut state.pending_publish_shares);
                            state.apply_publish_shares(fid.clone(), targets, cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |state, cx| {
                        state.publish_status = Some(format!("Failed: {}", e));
                        // If the server rejected the write because the
                        // forecast is already resolved, our local view was
                        // stale — reconcile so the cockpit locks and shows
                        // the real settled state instead of a dead error.
                        if e.to_lowercase().contains("resolved") {
                            state.reconcile_forecast(cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }
}

// ═══════════════════════════════════════════════════════════════════
// Render — the FPL program tree drives the UI
// ═══════════════════════════════════════════════════════════════════

impl Render for CockpitState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Drain live SSE events from background agent tasks.
        // This runs every frame — findings pop in one by one.
        self.drain_sse_events();

        let driver_names: Vec<String> = self
            .program
            .drivers()
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let focused = self.focused_node.clone();

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::BG))
            .on_action(cx.listener(|this, _: &NavigateDriverUp, _window, cx| {
                let drivers: Vec<String> = this.program.drivers().iter().map(|d| d.name.clone()).collect();
                if drivers.is_empty() { return; }
                let current_idx = if let FocusedNode::Driver(ref n) = this.focused_node {
                    drivers.iter().position(|d| d == n).unwrap_or(0)
                } else { 0 };
                let prev_idx = if current_idx == 0 { drivers.len() - 1 } else { current_idx - 1 };
                let name = drivers[prev_idx].clone();
                this.focus_driver(&name, cx);
            }))
            .on_action(cx.listener(|this, _: &NavigateDriverDown, _window, cx| {
                let drivers: Vec<String> = this.program.drivers().iter().map(|d| d.name.clone()).collect();
                if drivers.is_empty() { return; }
                let current_idx = if let FocusedNode::Driver(ref n) = this.focused_node {
                    drivers.iter().position(|d| d == n).unwrap_or(0)
                } else { 0 };
                let next_idx = (current_idx + 1) % drivers.len();
                let name = drivers[next_idx].clone();
                this.focus_driver(&name, cx);
            }))
            // ── Fermi Banner (top, always visible) ────────────────
            .child(render_fermi_banner(&self.messages, &self.agent_runs))
            // ── Locked banner: server resolved/voided this forecast ──
            .when(self.is_locked(), |el| el.child(render_locked_banner(self, cx)))
            // ── Main content (left + right panels) ────────────────
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .overflow_hidden()
            // ── Left: Program Tree ────────────────────────────────
            .child(
                div()
                    .id("composer-left-panel")
                    .flex()
                    .flex_col()
                    .w(px(700.0))
                    .h_full()
                    .overflow_y_scroll()
                    // Question + Outside View section
                    .child(render_question_section(self))
                    .child(render_outside_view(self, cx))
                    // Orchestration loading banner
                    .when(self.orchestration_running, |el| {
                        let agent_count = self.agent_runs.len();
                        let done_count = self.agent_runs.iter()
                            .filter(|r| r.status != AgentRunStatus::Running)
                            .count();
                        let running_names: Vec<String> = self.agent_runs.iter()
                            .filter(|r| r.status == AgentRunStatus::Running)
                            .map(|r| base_agent_name(&r.agent_name).to_string())
                            .collect();
                        el.child(
                            div()
                                .mx(px(8.0))
                                .my(px(6.0))
                                .px(px(16.0))
                                .py(px(12.0))
                                .rounded(px(8.0))
                                .bg(rgb(0x1A2332))
                                .border_1()
                                .border_color(rgb(theme::GOLD))
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .text_color(rgb(theme::GOLD))
                                                .child("⟳"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(13.0))
                                                .text_color(rgb(theme::GOLD))
                                                .font_weight(FontWeight::BOLD)
                                                .child("Fermi is decomposing your forecast…"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG_DIM))
                                        .child("Researching base rates, identifying drivers, gathering initial evidence. This takes 20–30 seconds."),
                                )
                                // Progress bar
                                .child(
                                    div()
                                        .h(px(4.0))
                                        .w_full()
                                        .rounded(px(2.0))
                                        .bg(rgb(theme::BG))
                                        .child(
                                            div()
                                                .h(px(4.0))
                                                .rounded(px(2.0))
                                                .bg(rgb(theme::GOLD))
                                                .w(gpui::px(
                                                    if agent_count > 0 {
                                                        (done_count as f32 / agent_count as f32) * 400.0
                                                    } else {
                                                        100.0
                                                    }
                                                )),
                                        ),
                                )
                                .when(!running_names.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child(format!(
                                                "Running: {} ({}/{})",
                                                running_names.join(", "),
                                                done_count,
                                                agent_count,
                                            )),
                                    )
                                }),
                        )
                    })
                    // ── Forecast Index (visualizations) — ABOVE drivers ──
                    // These are the key visuals that justify the 30-credit
                    // decomposition cost. Inside/outside divergence, driver
                    // impact treemap, and simulation histogram are shown
                    // immediately after the base rate so the user sees value.
                    .child(render_forecast_index(self, cx))
                    // ── 8A: Workflow state transition banner ──────────
                    // Shows clear state after research completes:
                    //   - "Ready to simulate" (agents done, no sim yet)
                    //   - "Simulation complete" (sim run, results available)
                    .when({
                        let has_drivers = !self.program.drivers().is_empty();
                        let has_agents = !self.agent_runs.is_empty();
                        let all_done = self.agent_runs.iter().all(|r| {
                            r.status != AgentRunStatus::Running
                        });
                        let no_sim = self.sim_results.is_none();
                        !self.orchestration_running && has_drivers && has_agents && all_done && no_sim
                    }, |el| {
                        el.child(
                            div()
                                .mx(px(8.0))
                                .my(px(6.0))
                                .px(px(16.0))
                                .py(px(10.0))
                                .rounded(px(8.0))
                                .bg(rgb(0x1A2A1A))
                                .border_1()
                                .border_color(rgb(theme::GREEN))
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .text_color(rgb(theme::GREEN))
                                        .child("✓"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(2.0))
                                        .child(
                                            div()
                                                .text_size(px(13.0))
                                                .text_color(rgb(theme::GREEN))
                                                .font_weight(FontWeight::BOLD)
                                                .child("Research complete — ready to simulate"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(rgb(theme::FG_DIM))
                                                .child("Review drivers and evidence above, then press Ctrl+R to run Monte Carlo simulation."),
                                        ),
                                ),
                        )
                    })
                    // ── 8A: Simulation running indicator ──────────────
                    .when(self.sim_running, |el| {
                        el.child(
                            div()
                                .mx(px(8.0))
                                .my(px(6.0))
                                .px(px(16.0))
                                .py(px(10.0))
                                .rounded(px(8.0))
                                .bg(rgb(0x1A2332))
                                .border_1()
                                .border_color(rgb(theme::CYAN))
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .text_color(rgb(theme::CYAN))
                                        .child("⟳"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .text_color(rgb(theme::CYAN))
                                        .font_weight(FontWeight::BOLD)
                                        .child("Running Monte Carlo simulation (10,000 iterations)…"),
                                ),
                        )
                    })
                    // Drivers section (the core of the forecast)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .border_b_1()
                                    .border_color(rgb(theme::FG_FAINT))
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::GREEN))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(if self.orchestration_running && driver_names.is_empty() {
                                        "Drivers (decomposing…)".to_string()
                                    } else {
                                        format!("Drivers ({})", driver_names.len())
                                    }),
                            )
                            // ── 8A: Loading skeleton while Fermi decomposes ──
                            // Pulsing placeholder cards shown while waiting for
                            // the decomposition result (typically 20-30 seconds).
                            .when(self.orchestration_running && driver_names.is_empty(), |el| {
                                el.children((0..4).map(|i| {
                                    div()
                                        .mx(px(8.0))
                                        .my(px(3.0))
                                        .px(px(12.0))
                                        .py(px(12.0))
                                        .rounded(px(6.0))
                                        .border_1()
                                        .border_color(rgb(theme::FG_FAINT))
                                        .bg(rgb(theme::BG_ELEVATED))
                                        .opacity(if i % 2 == 0 { 0.5 } else { 0.35 })
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        // Skeleton name bar
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(
                                                    div()
                                                        .h(px(12.0))
                                                        .w(px(120.0 + (i as f32) * 20.0))
                                                        .rounded(px(3.0))
                                                        .bg(rgb(theme::FG_FAINT)),
                                                )
                                                .child(
                                                    div()
                                                        .h(px(10.0))
                                                        .w(px(60.0))
                                                        .rounded(px(2.0))
                                                        .bg(rgb(theme::FG_FAINT)),
                                                ),
                                        )
                                        // Skeleton parameter bar
                                        .child(
                                            div()
                                                .h(px(8.0))
                                                .w(px(200.0 - (i as f32) * 15.0))
                                                .rounded(px(2.0))
                                                .bg(rgb(theme::FG_FAINT)),
                                        )
                                        // Skeleton confidence dots
                                        .child(
                                            div()
                                                .h(px(8.0))
                                                .w(px(100.0))
                                                .rounded(px(2.0))
                                                .bg(rgb(theme::FG_FAINT)),
                                        )
                                }))
                            })
                            .children(driver_names.iter().enumerate().map(|(i, name)| {
                                let is_focused = focused == FocusedNode::Driver(name.clone())
                                    || focused == FocusedNode::AgentPicker(name.clone());
                                let driver = self.program.driver(name);
                                // Find agents assigned to this driver
                                let assigned_agents: Vec<String> = self
                                    .program
                                    .agents()
                                    .iter()
                                    .filter(|a| a.driver_refs.contains(&name.to_string()))
                                    .map(|a| a.name.clone())
                                    .collect();
                                let n = name.clone();
                                let uc = self.driver_confidence.get(name).copied();
                                let sug_count = self.pending_suggestions.iter()
                                    .filter(|s| s.driver_name == *name)
                                    .count();
                                // Spec 23 R-2: pending fits take precedence over
                                // the per-sim Fitted/PriorFallback badge — they
                                // represent server-side state the user can act on
                                // now, vs. last-sim state. Construct a synthetic
                                // LearnableDriverBadge wrapping the pending state.
                                let pending_badge: Option<LearnableDriverBadge> = self
                                    .bayesops_pending
                                    .get(name)
                                    .map(|p| LearnableDriverBadge {
                                        driver_name: p.driver_name.clone(),
                                        status: LearnableBadgeStatus::PendingReview {
                                            pending_id: p.pending_id.clone(),
                                            n_eff: p.n_eff,
                                            ci_width: p.ci_width,
                                            delta_pp: p.delta_pp,
                                            n_observations: p.n_observations,
                                        },
                                    });
                                let sim_badge = self
                                    .sim_results
                                    .as_ref()
                                    .and_then(|sr| sr.learnable_drivers.iter()
                                        .find(|b| b.driver_name == *name))
                                    .cloned();
                                // Pending wins; otherwise fall back to sim badge.
                                let learnable_status_owned = pending_badge.or(sim_badge);
                                let learnable_status = learnable_status_owned.as_ref();
                                render_driver_card(
                                    i,
                                    driver,
                                    is_focused,
                                    &assigned_agents,
                                    &self.agent_runs,
                                    &self.messages,
                                    &self.program.evidence_items(),
                                    uc,
                                    sug_count,
                                    cx,
                                    &n,
                                    learnable_status,
                                )
                            }))
                            // Add driver buttons
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .child(
                                        div()
                                            .id("add-continuous-btn")
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(theme::BG_ELEVATED))
                                            .border_1()
                                            .border_color(rgb(theme::GREEN))
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::GREEN))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.add_manual_driver(false, cx);
                                            }))
                                            .child("+ Continuous driver"),
                                    )
                                    .child(
                                        div()
                                            .id("add-binary-btn")
                                            .px(px(10.0))
                                            .py(px(4.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(theme::BG_ELEVATED))
                                            .border_1()
                                            .border_color(rgb(theme::GOLD))
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::GOLD))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.add_manual_driver(true, cx);
                                            }))
                                            .child("+ Binary event"),
                                    ),
                            )
                            // Status bar
                            .child(render_status_bar(self)),
                    ),
            )
            // ── Right: Assistant + Editor ──────────────────────────
            // ── Right: Tabbed Panel ───────────────────────────────
            .child(
                div()
                    .id("composer-right-panel")
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .min_w(px(0.0))
                    .h_full()
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_l_1()
                    .border_color(rgb(theme::FG_FAINT))
                    // Tab bar
                    .child(render_tab_bar(self.right_tab, cx))
                    // Tab content (scrollable)
                    .child(
                        div()
                            .id("right-tab-content")
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .overflow_y_scroll()
                            .min_w(px(0.0))
                            .child(match self.right_tab {
                                RightTab::Edit => render_right_panel(self, &focused, cx),
                                RightTab::Fpl => render_fpl_tab(self).into_any_element(),
                                RightTab::Wiki => render_wiki_tab(self, cx).into_any_element(),
                                RightTab::Schedules => {
                                    render_schedules_tab(self, cx).into_any_element()
                                }
                                RightTab::Trajectory => {
                                    render_trajectory_tab(self, cx).into_any_element()
                                }
                                RightTab::Access => {
                                    render_access_tab(self, cx).into_any_element()
                                }
                            }),
                    )
            )
            ) // close main content horizontal div
    }
}

pub fn render_cockpit(cockpit: &Entity<CockpitState>) -> impl IntoElement {
    cockpit.clone()
}

// ═══════════════════════════════════════════════════════════════════
// Section Renderers
// ═══════════════════════════════════════════════════════════════════

fn render_question_section(state: &CockpitState) -> impl IntoElement {
    let prob_pct = format!("{:.2}%", state.predicted_probability * 100.0);

    div()
        .bg(rgb(theme::BG_ELEVATED))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        .px(px(16.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(state.question_input.clone())
        .child(
            // Header row: probability + inside-view explainer + confidence
            // + (researching badge) + (publish status). flex_wrap() lets a
            // long error message or explainer wrap to a second row instead
            // of pushing siblings into degenerate single-character widths.
            // min_w(0) on the parent + on text children allows shrinking.
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(16.0))
                .min_w(px(0.0))
                .child(
                    div()
                        .flex_none()
                        .text_size(px(28.0))
                        .text_color(rgb(theme::CYAN))
                        .font_weight(FontWeight::BOLD)
                        .child(prob_pct),
                )
                .when(!state.inside_view_explanation.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_DIM))
                            // min_w large enough to hold a multi-word phrase,
                            // not 0 — when the parent flex_wrap()s, the text
                            // element otherwise shrinks below per-word width
                            // and GPUI falls back to per-character line breaks
                            // ("s/t/a/r/t/i/n/g" stacked vertically).
                            .min_w(px(220.0))
                            .max_w(px(560.0))
                            .flex_grow()
                            .child(state.inside_view_explanation.clone()),
                    )
                    .when(state.forecast_confidence > 0.0, |el| {
                        let conf_label = if state.forecast_confidence > 0.7 {
                            "High"
                        } else if state.forecast_confidence > 0.4 {
                            "Medium"
                        } else {
                            "Low"
                        };
                        let conf_color = if state.forecast_confidence > 0.7 {
                            theme::GREEN
                        } else if state.forecast_confidence > 0.4 {
                            theme::GOLD
                        } else {
                            theme::RED
                        };
                        el.child(div().text_size(px(10.0)).text_color(rgb(conf_color)).child(
                            format!(
                                "Confidence: {} ({:.0}%)",
                                conf_label,
                                state.forecast_confidence * 100.0
                            ),
                        ))
                    })
                })
                .when(state.orchestration_running, |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x2A2D3A))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(rgb(theme::GOLD))
                                    .child("⟳"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::GOLD))
                                    .child("Researching… Fermi is decomposing your forecast"),
                            ),
                    )
                })
                .when(state.publish_status.is_some(), |el| {
                    // Render publish status with strict width control:
                    // a fixed width (not min/max) so the flex_wrap parent
                    // doesn't squeeze it into a sliver that breaks text
                    // per-character. Long errors get truncated visually;
                    // the full message lives in the assistant messages
                    // panel (the messages.push at the publish call site).
                    let msg = state.publish_status.as_deref().unwrap_or("").to_string();
                    let color = if msg.starts_with("Failed")
                        || msg.contains("error")
                        || msg.contains("Error")
                    {
                        theme::RED
                    } else if msg.contains("Publishing") || msg.contains("…") {
                        theme::GOLD
                    } else {
                        theme::GREEN
                    };
                    // Show only the first ~80 chars on the badge —
                    // multi-paragraph DB errors otherwise blow out the
                    // header layout no matter what wrapping policy we use.
                    let short = if msg.chars().count() > 80 {
                        let truncated: String = msg.chars().take(77).collect();
                        format!("{}…", truncated)
                    } else {
                        msg.clone()
                    };
                    el.child(
                        div()
                            .flex_none()
                            .w(px(360.0))
                            .text_size(px(11.0))
                            .text_color(rgb(color))
                            .child(short),
                    )
                }),
        )
        // ── Three-anchor delta chips ────────────────────────────────
        // Surfaces (inside − outside), (inside − crowd), (outside − crowd)
        // immediately under the big probability number — these deltas are
        // the headline product of the forecasting workflow. Hidden when
        // no comparison anchor exists yet.
        .child(render_delta_chips(state))
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                // ── 8A: Context-sensitive hint for Ctrl+Enter ──
                .child(if state.orchestration_running {
                    "⏳ Researching…".to_string()
                } else {
                    "Ctrl+Enter research".to_string()
                })
                // ── 8A: Context-sensitive hint for Ctrl+R ──
                .child(if state.sim_running {
                    "⏳ Simulating…".to_string()
                } else if state.sim_results.is_some() {
                    "✓ Simulated · Ctrl+R re-run".to_string()
                } else {
                    "Ctrl+R simulate".to_string()
                })
                .child("Ctrl+P publish")
                .child("Ctrl+N new")
                .child("Ctrl+O import")
                .child("Ctrl+S save")
                .child("Ctrl+E tabs"),
        )
}

// ──────────────────────────────────────────────────────────────────────
// Anchor triad — the three named probabilities (Inside / Outside / Crowd)
// and their pairwise deltas. These deltas ARE the core value of the
// forecasting workflow: how does your inside-view model compare to the
// reference-class outside view AND to the prediction-market crowd?
// ──────────────────────────────────────────────────────────────────────

/// The three named probability anchors as percentages (0–100), plus the
/// three pairwise deltas in percentage-points (positive = first > second).
///
/// `outside` and `crowd` are `None` when not available. `delta_*` are also
/// `None` when either side of the comparison is missing.
#[derive(Debug, Clone, Copy, Default)]
struct AnchorTriad {
    inside_pct: f64,
    outside_pct: Option<f64>,
    crowd_pct: Option<f64>,
    /// inside − outside (positive = model above reference class)
    delta_io_pp: Option<f64>,
    /// inside − crowd (positive = contrarian-bullish vs market)
    delta_ic_pp: Option<f64>,
    /// outside − crowd (positive = ref class above market)
    delta_oc_pp: Option<f64>,
}

impl AnchorTriad {
    fn from_state(state: &CockpitState) -> Self {
        let inside_pct = state.predicted_probability * 100.0;
        let outside_pct = state
            .program
            .question()
            .and_then(|q| q.base_rate.as_ref())
            .map(|br| br.historical_frequency * 100.0);
        let crowd_pct = state.pm_market_price.map(|p| p * 100.0);

        let delta_io_pp = outside_pct.map(|o| inside_pct - o);
        let delta_ic_pp = crowd_pct.map(|c| inside_pct - c);
        let delta_oc_pp = match (outside_pct, crowd_pct) {
            (Some(o), Some(c)) => Some(o - c),
            _ => None,
        };

        Self {
            inside_pct,
            outside_pct,
            crowd_pct,
            delta_io_pp,
            delta_ic_pp,
            delta_oc_pp,
        }
    }

    /// True when there's at least one delta to display (so we have a reason
    /// to render the chip strip at all).
    fn has_any_delta(&self) -> bool {
        self.delta_io_pp.is_some() || self.delta_ic_pp.is_some() || self.delta_oc_pp.is_some()
    }
}

/// Color a delta by magnitude:
///   • |Δ| ≤ 3pp  → neutral (convergent — the three views agree)
///   • |Δ| ≤ 10pp → gold (mild divergence — worth noting)
///   • |Δ| > 10pp → red (strong divergence — this is the interesting case)
fn delta_color(delta_pp: f64) -> u32 {
    let m = delta_pp.abs();
    if m <= 3.0 {
        theme::FG_DIM
    } else if m <= 10.0 {
        theme::GOLD
    } else {
        theme::RED
    }
}

/// Render a single delta chip. The label encodes the comparison direction
/// (e.g. "model − crowd") and the value carries sign + magnitude in pp.
fn render_delta_chip(label: &str, delta_pp: f64) -> gpui::Div {
    let sign = if delta_pp > 0.0 { "+" } else { "" };
    let color = delta_color(delta_pp);
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(3.0))
        .rounded(px(3.0))
        .bg(rgb(theme::BG_ELEVATED))
        .border_1()
        .border_color(rgb(color))
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(color))
                .font_weight(FontWeight::BOLD)
                .child(format!("{}{:.1}pp", sign, delta_pp)),
        )
}

/// Render the three-delta chip strip. Returns an empty hidden div when
/// no anchors exist (the chips only appear once there's something to
/// compare).
fn render_delta_chips(state: &CockpitState) -> gpui::Div {
    let t = AnchorTriad::from_state(state);
    if !t.has_any_delta() {
        return div().w(px(0.0)).h(px(0.0));
    }

    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .text_size(px(11.0));

    if let Some(d) = t.delta_io_pp {
        row = row.child(render_delta_chip("model − base", d));
    }
    if let Some(d) = t.delta_ic_pp {
        row = row.child(render_delta_chip("model − crowd", d));
    }
    if let Some(d) = t.delta_oc_pp {
        row = row.child(render_delta_chip("base − crowd", d));
    }
    row
}

// ──────────────────────────────────────────────────────────────────────
// Interactive histogram — "feels like a stock chart"
//
// Renders the simulation distribution as native GPUI bars (one div per
// bin) so we can attach hover handlers. The hovered bin's index is
// stored in state.hovered_histogram_bin and drives a tooltip card
// above the histogram showing:
//   • outcome value at the cursor
//   • bin count + density (probability per unit outcome)
//   • CDF percentile up to that point
//   • signed distance to each anchor (inside / outside / crowd)
//
// Three vertical reference lines are overlaid at the anchor positions
// so the user can see where each named view sits within the distribution.
// ──────────────────────────────────────────────────────────────────────

/// Map a histogram bin index to its center outcome value. When the
/// `bin_starts` field is populated (new sims), uses the exact stored
/// values. Falls back to linear interpolation across `[p5, p95]` for
/// loaded forecasts where bin_starts wasn't persisted.
fn bin_center(sim: &SimResults, idx: usize) -> f64 {
    if idx < sim.bin_starts.len() {
        sim.bin_starts[idx] + sim.bin_width * 0.5
    } else {
        let n = sim.histogram.len().max(1);
        if n == 1 {
            sim.median
        } else {
            sim.p5 + (sim.p95 - sim.p5) * (idx as f64 + 0.5) / n as f64
        }
    }
}

/// Render the simulation distribution as an interactive histogram with
/// per-bar hover, anchor reference lines, and a live tooltip.
///
/// The element is constructed via `cx.listener` per-bar so each bar
/// updates `state.hovered_histogram_bin` independently. Cost is
/// negligible (~20 bins per render). The tooltip is rendered above the
/// bars; bars are stacked horizontally beneath it.
fn render_interactive_histogram(
    state: &CockpitState,
    cx: &mut Context<CockpitState>,
    chart_w: f32,
    chart_h: f32,
) -> gpui::AnyElement {
    let sim_opt = state.sim_results.as_ref();
    let Some(sim) = sim_opt else {
        return div().into_any_element();
    };
    if sim.histogram.is_empty() {
        return div().into_any_element();
    }

    let triad = AnchorTriad::from_state(state);
    let n_bins = sim.histogram.len();
    let bar_gap = 1.0_f32;
    let bar_w = ((chart_w - bar_gap * (n_bins as f32 - 1.0)) / n_bins as f32).max(2.0);

    let max_count = *sim.histogram.iter().max().unwrap_or(&1) as f32;
    let total: u64 = sim.histogram.iter().map(|&c| c as u64).sum();

    // Map an outcome value (0–1 for prob forecasts; arbitrary for others)
    // to an x-offset within the histogram. Returns None if outside the
    // displayed range so the caller can suppress the reference line.
    let outcome_to_x = {
        let p5 = sim.p5;
        let p95 = sim.p95;
        let span = (p95 - p5).max(1e-9);
        let w = chart_w;
        move |outcome_pct: f64| -> Option<f32> {
            // The histogram is built over the simulation output's actual
            // (min, max) range. We approximate using (p5, p95) which is
            // what's reliably stored. Outcome inputs are 0-100 (model %).
            // For prob forecasts the sim output is itself a probability
            // in [0,1], so we compare on the same scale by treating the
            // 0-100 outcome as a 0-1 fraction.
            let val = outcome_pct / 100.0;
            if val < p5 || val > p95 {
                return None;
            }
            Some(((val - p5) / span * w as f64) as f32)
        }
    };

    // Tooltip text for the currently-hovered bin (if any).
    let tooltip_lines: Vec<String> = match state.hovered_histogram_bin {
        Some(idx) if idx < n_bins => {
            let count = sim.histogram[idx];
            let outcome = bin_center(sim, idx) * 100.0;
            let density_pct = if total > 0 {
                count as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            // CDF up to end of this bin
            let cdf_count: u64 = sim.histogram[..=idx].iter().map(|&c| c as u64).sum();
            let cdf_pct = if total > 0 {
                cdf_count as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let mut lines = vec![
                format!("outcome: {:.1}%", outcome),
                format!("count: {} ({:.1}% of sims)", count, density_pct),
                format!("CDF: {:.0}th percentile", cdf_pct),
            ];
            // Signed distance to each anchor (in pp of outcome).
            lines.push(format!(
                "Δ from model: {:+.1}pp",
                outcome - triad.inside_pct
            ));
            if let Some(o) = triad.outside_pct {
                lines.push(format!("Δ from base: {:+.1}pp", outcome - o));
            }
            if let Some(c) = triad.crowd_pct {
                lines.push(format!("Δ from crowd: {:+.1}pp", outcome - c));
            }
            lines
        }
        _ => vec!["hover a bar".to_string()],
    };

    // ── Tooltip card (always present; content changes with hover) ──
    let tooltip = div()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(4.0))
        .bg(rgb(theme::BG_ELEVATED))
        .border_1()
        .border_color(rgb(theme::FG_FAINT))
        .text_size(px(9.0))
        .text_color(rgb(theme::FG_DIM))
        .children(tooltip_lines.into_iter().map(|line| div().child(line)));

    // ── Bars + reference lines layered together ──
    // We use a horizontal flex of bars; the anchor reference lines are
    // overlaid via absolutely-positioned children on the bar container.
    let mut bars_row = div()
        .id("histogram-bars-row")
        .relative()
        .w(px(chart_w))
        .h(px(chart_h))
        .flex()
        .items_end()
        .gap(px(bar_gap));

    for idx in 0..n_bins {
        let count = sim.histogram[idx];
        let bar_h = if max_count > 0.0 {
            (count as f32 / max_count) * chart_h * 0.95
        } else {
            1.0
        };
        let hovered = state.hovered_histogram_bin == Some(idx);
        bars_row = bars_row.child(
            div()
                .id(("hist-bar", idx))
                .w(px(bar_w))
                .h(px(bar_h.max(1.0)))
                .bg(rgb(theme::CYAN))
                .when(hovered, |el| {
                    // Hovered bar pops via a gold outline — keeps the bar
                    // body the same cyan as the rest so the eye lands on
                    // the affordance, not on a flickering color change.
                    el.border_1().border_color(rgb(theme::GOLD))
                })
                .cursor_pointer()
                .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                    if *hovered {
                        if this.hovered_histogram_bin != Some(idx) {
                            this.hovered_histogram_bin = Some(idx);
                            cx.notify();
                        }
                    } else if this.hovered_histogram_bin == Some(idx) {
                        this.hovered_histogram_bin = None;
                        cx.notify();
                    }
                })),
        );
    }

    // Overlay reference lines for each anchor (inside / outside / crowd).
    // Each is a thin absolutely-positioned vertical div spanning the
    // histogram height. Skip silently when outside the [p5, p95] range.
    let mut overlay = div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(chart_w))
        .h(px(chart_h));

    if let Some(x) = outcome_to_x(triad.inside_pct) {
        overlay = overlay.child(
            div()
                .absolute()
                .left(px(x))
                .top(px(0.0))
                .w(px(1.5))
                .h(px(chart_h))
                .bg(rgb(theme::CYAN)),
        );
    }
    if let Some(o) = triad.outside_pct {
        if let Some(x) = outcome_to_x(o) {
            overlay = overlay.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(0.0))
                    .w(px(1.5))
                    .h(px(chart_h))
                    .bg(rgb(theme::GOLD)),
            );
        }
    }
    if let Some(c) = triad.crowd_pct {
        if let Some(x) = outcome_to_x(c) {
            overlay = overlay.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(0.0))
                    .w(px(1.5))
                    .h(px(chart_h))
                    .bg(rgb(theme::PURPLE)),
            );
        }
    }

    bars_row = bars_row.child(overlay);

    // ── Compose: tooltip on top, bars below, legend at bottom ──
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(tooltip)
        .child(bars_row)
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .text_size(px(8.0))
                .text_color(rgb(theme::FG_FAINT))
                .child(div().text_color(rgb(theme::CYAN)).child("│ model"))
                .when(triad.outside_pct.is_some(), |el| {
                    el.child(div().text_color(rgb(theme::GOLD)).child("│ base"))
                })
                .when(triad.crowd_pct.is_some(), |el| {
                    el.child(div().text_color(rgb(theme::PURPLE)).child("│ crowd"))
                })
                .child(div().text_color(rgb(theme::FG_FAINT)).child(format!(
                    "p5–p95: {:.0}% – {:.0}% · {} iters",
                    sim.p5 * 100.0,
                    sim.p95 * 100.0,
                    sim.iterations
                ))),
        )
        .into_any_element()
}

// ──────────────────────────────────────────────────────────────────────
// Interactive index chart — version-over-time comparison of the three
// anchors. Keeps the existing bitmap line rendering (GPUI has no line
// primitive that beats a plotters bitmap for diagonals) but overlays a
// transparent native layer of one column per version so each version
// gets per-point hover with a tooltip showing the three values and
// pairwise deltas at that historical point.
// ──────────────────────────────────────────────────────────────────────

/// Render the interactive index chart with mouseover crosshair + tooltip.
///
/// Bitmap line layer + native hover layer composed in absolute positioning.
/// The hovered version (if any) drives a vertical crosshair + per-version
/// tooltip card.
fn render_interactive_index_chart(
    state: &CockpitState,
    cx: &mut Context<CockpitState>,
    chart_w: f32,
    chart_h: f32,
) -> gpui::AnyElement {
    if state.versions.len() < 2 {
        return div().into_any_element();
    }

    let base_rate = state
        .program
        .question()
        .and_then(|q| q.base_rate.as_ref())
        .map(|br| br.historical_frequency * 100.0)
        .unwrap_or(50.0);
    let crowd_price_pct = state.pm_market_price.map(|p| p * 100.0);

    let history: Vec<crate::charts::IndexPoint> = state
        .versions
        .iter()
        .map(|v| crate::charts::IndexPoint {
            label: format!("v{}", v.version),
            inside_view: v.probability * 100.0,
            outside_view: base_rate,
            crowd_price: crowd_price_pct,
        })
        .collect();

    let n = history.len();
    let chart_w_u = chart_w as u32;
    let chart_h_u = chart_h as u32;
    let rgb_buf =
        crate::charts::render_index_chart(&history, n.saturating_sub(1), chart_w_u, chart_h_u);
    let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w_u, chart_h_u);

    // Per-version hover columns. Each column is centered on the x-pixel
    // of its data point. Columns are equally spaced from 0 to chart_w.
    let col_w = chart_w / n as f32;

    // ── Tooltip card ──
    // Shows three anchor values + pairwise deltas at the hovered version.
    // Stays mounted so layout doesn't jump; content swaps based on hover.
    let tooltip_card = match state.hovered_index_version {
        Some(idx) if idx < n => {
            let v = &state.versions[idx];
            let p = &history[idx];
            let inside = p.inside_view;
            let outside = p.outside_view;
            let crowd = p.crowd_price;

            let mut lines: Vec<String> = Vec::new();
            lines.push(format!(
                "v{} · {}",
                v.version,
                v.timestamp.split('T').next().unwrap_or(&v.timestamp)
            ));
            lines.push(format!(
                "model: {:.1}%   base: {:.1}%{}",
                inside,
                outside,
                crowd
                    .map(|c| format!("   crowd: {:.1}%", c))
                    .unwrap_or_default()
            ));
            lines.push(format!(
                "Δ(model−base): {:+.1}pp{}",
                inside - outside,
                crowd
                    .map(|c| format!(
                        "   Δ(model−crowd): {:+.1}pp   Δ(base−crowd): {:+.1}pp",
                        inside - c,
                        outside - c
                    ))
                    .unwrap_or_default()
            ));
            if !v.change_summary.is_empty() {
                lines.push(format!("note: {}", v.change_summary));
            }

            div()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG_ELEVATED))
                .border_1()
                .border_color(rgb(theme::FG_FAINT))
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_DIM))
                .children(lines.into_iter().map(|line| div().child(line)))
        }
        _ => div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .bg(rgb(theme::BG_ELEVATED))
            .border_1()
            .border_color(rgb(theme::FG_FAINT))
            .text_size(px(9.0))
            .text_color(rgb(theme::FG_FAINT))
            .child("hover a version on the line chart"),
    };

    // ── Chart layer (bitmap) + hover layer (transparent native) ──
    let mut hover_layer = div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .w(px(chart_w))
        .h(px(chart_h));

    for idx in 0..n {
        // Center the column on the data point's x-pixel; same convention
        // plotters uses (i × col_w + col_w / 2). The column width covers
        // the full segment so adjacent points don't fight for hover.
        let col_x = (idx as f32) * col_w;
        let hovered = state.hovered_index_version == Some(idx);
        let mut col = div()
            .id(("idx-col", idx))
            .absolute()
            .left(px(col_x))
            .top(px(0.0))
            .w(px(col_w))
            .h(px(chart_h))
            .cursor_crosshair()
            .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                if *hovered {
                    if this.hovered_index_version != Some(idx) {
                        this.hovered_index_version = Some(idx);
                        cx.notify();
                    }
                } else if this.hovered_index_version == Some(idx) {
                    this.hovered_index_version = None;
                    cx.notify();
                }
            }));
        // Vertical crosshair line — only drawn when this column is the
        // hovered one. Thin gold line at the column's center pixel.
        if hovered {
            col = col.child(
                div()
                    .absolute()
                    .left(px(col_w / 2.0 - 0.5))
                    .top(px(0.0))
                    .w(px(1.0))
                    .h(px(chart_h))
                    .bg(rgb(theme::GOLD)),
            );
        }
        hover_layer = hover_layer.child(col);
    }

    // Composed: tooltip on top, layered chart beneath.
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(tooltip_card)
        .child(
            div()
                .relative()
                .w(px(chart_w))
                .h(px(chart_h))
                .child(
                    gpui::img(gpui::ImageSource::Render(render_img))
                        .w(gpui::px(chart_w))
                        .h(gpui::px(chart_h)),
                )
                .child(hover_layer),
        )
        .child(
            div()
                .flex()
                .gap(px(10.0))
                .text_size(px(8.0))
                .text_color(rgb(theme::FG_FAINT))
                .child(div().text_color(rgb(theme::CYAN)).child("─ model"))
                .child(div().text_color(rgb(theme::GOLD)).child("─ base"))
                .when(crowd_price_pct.is_some(), |el| {
                    el.child(div().text_color(rgb(theme::PURPLE)).child("─ crowd"))
                })
                .child(
                    div()
                        .text_color(rgb(theme::FG_FAINT))
                        .child(format!("{} versions", n)),
                ),
        )
        .into_any_element()
}

/// Outside View — base rate, reference class, reasoning.
/// Populated by the macro_forecaster's research.
fn render_outside_view(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let base_rate = state.program.question().and_then(|q| q.base_rate.as_ref());

    div()
        .mx(px(8.0))
        .my(px(4.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .bg(rgb(theme::BG_ELEVATED))
        .border_1()
        .border_color(rgb(theme::GOLD))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::GOLD))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Outside View (Base Rate)"),
        )
        .when(base_rate.is_some(), |el| {
            let br = base_rate.unwrap();
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .text_color(rgb(theme::GOLD))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{:.2}%", br.historical_frequency * 100.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG))
                                    .child(br.reference_class.clone()),
                            )
                            .when(br.sample_size.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(format!("n={}", br.sample_size.unwrap_or(0))),
                                )
                            }),
                    ),
            )
            .when(br.reasoning.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .child(br.reasoning.as_deref().unwrap_or("").to_string()),
                )
            })
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child(format!("Source: {}", br.source)),
            )
            .child(
                div()
                    .id("update-base-rate")
                    .mt(px(4.0))
                    .px(px(8.0))
                    .py(px(3.0))
                    .rounded(px(3.0))
                    .bg(rgb(theme::BG))
                    .text_size(px(10.0))
                    .text_color(rgb(theme::GOLD))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.update_outside_rate(cx);
                    }))
                    .child("⟳ Update base rate"),
            )
        })
        .when(base_rate.is_none(), |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child(if state.orchestration_running {
                        "Searching for reference class…"
                    } else {
                        "No base rate yet — Ctrl+Enter to research"
                    }),
            )
        })
        // ── Polymarket Crowd Price (when linked) ──────────────────
        .when(state.pm_market_price.is_some(), |el| {
            let pm_price = state.pm_market_price.unwrap_or(0.0);
            let pm_pct = format!("{:.1}%", pm_price * 100.0);
            let fermi_prob = state.predicted_probability;
            let divergence_pp = (fermi_prob - pm_price) * 100.0;
            let div_abs = divergence_pp.abs();
            let (div_sign, div_color) = if divergence_pp > 2.0 {
                ("+", theme::GREEN)
            } else if divergence_pp < -2.0 {
                ("", theme::RED)
            } else {
                ("±", theme::FG_DIM)
            };
            let div_label = if div_abs < 2.0 {
                "Consensus"
            } else if div_abs < 5.0 {
                "Minor"
            } else if div_abs < 15.0 {
                "Moderate"
            } else if div_abs < 30.0 {
                "Significant"
            } else {
                "Extreme"
            };

            let vol_str = state.pm_volume_24h.map(|v| {
                if v >= 1_000_000.0 {
                    format!("${:.1}M", v / 1_000_000.0)
                } else if v >= 1_000.0 {
                    format!("${:.0}K", v / 1_000.0)
                } else {
                    format!("${:.0}", v)
                }
            }).unwrap_or_default();

            let liq_str = state.pm_liquidity.map(|v| {
                if v >= 1_000_000.0 {
                    format!("${:.1}M", v / 1_000_000.0)
                } else if v >= 1_000.0 {
                    format!("${:.0}K", v / 1_000.0)
                } else {
                    format!("${:.0}", v)
                }
            }).unwrap_or_default();

            el.child(
                div()
                    .mt(px(6.0))
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x1A1A2E))
                    .border_1()
                    .border_color(rgb(theme::PURPLE))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    // Header
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Outside View (Prediction Market)"),
                            )
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .child("🔗 Polymarket"),
                            ),
                    )
                    // Price + question
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .text_size(px(22.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .font_weight(FontWeight::BOLD)
                                    .child(pm_pct),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .flex_grow()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::FG))
                                            .child(
                                                state.pm_question.as_deref()
                                                    .unwrap_or("Linked Polymarket market")
                                                    .to_string(),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .when(!vol_str.is_empty(), |el| {
                                                el.child(format!("{} vol/24h", vol_str))
                                            })
                                            .when(!liq_str.is_empty(), |el| {
                                                el.child(format!("{} liquidity", liq_str))
                                            })
                                            .when(state.pm_confidence.is_some(), |el| {
                                                el.child(format!("{} confidence",
                                                    state.pm_confidence.as_deref().unwrap_or("")))
                                            }),
                                    ),
                            ),
                    )
                    // 1-week trend
                    .when(state.pm_price_change_1w.is_some(), |el| {
                        let change = state.pm_price_change_1w.unwrap_or(0.0);
                        let (arrow, trend_color) = if change > 0.005 {
                            ("📈", theme::GREEN)
                        } else if change < -0.005 {
                            ("📉", theme::RED)
                        } else {
                            ("→", theme::FG_DIM)
                        };
                        el.child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(trend_color))
                                .child(format!(
                                    "{} {:+.1}pp this week",
                                    arrow,
                                    change * 100.0
                                )),
                        )
                    })
                    // Divergence box
                    .child(
                        div()
                            .mt(px(4.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(rgb(div_color))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(div_color))
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!(
                                        "DIVERGENCE: {}{:.1}pp {} crowd",
                                        div_sign,
                                        div_abs,
                                        if divergence_pp > 0.0 { "above" } else { "below" }
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .child(format!(
                                        "{} divergence — Your model: {:.1}% · Crowd: {:.1}%",
                                        div_label,
                                        fermi_prob * 100.0,
                                        pm_price * 100.0
                                    )),
                            )
                            .when(div_abs > 15.0, |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::GOLD))
                                        .child("Is this alpha or overconfidence?"),
                                )
                            }),
                    )
                    // Action row
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .mt(px(2.0))
                            .when(state.pm_url.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::PURPLE))
                                        .child(format!(
                                            "🔗 {}",
                                            state.pm_url.as_deref().unwrap_or("")
                                        )),
                                )
                            })
                            .child(
                                div()
                                    .id("use-pm-as-base-rate")
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .bg(rgb(theme::BG_ELEVATED))
                                    .border_1()
                                    .border_color(rgb(theme::PURPLE))
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        // Update base rate only — does NOT change the model's
                                        // inside view probability. The base rate is the outside
                                        // view anchor; the inside view is your model's output.
                                        if let Some(q) = this.program.question_mut() {
                                            q.base_rate = Some(BaseRate {
                                                reference_class: format!(
                                                    "Polymarket crowd-implied probability ({})",
                                                    this.pm_question.as_deref().unwrap_or("linked market")
                                                ),
                                                historical_frequency: pm_price,
                                                sample_size: None,
                                                source: "Polymarket".into(),
                                                reasoning: Some(format!(
                                                    "Crowd price backed by {} volume, {} liquidity. {} confidence.",
                                                    vol_str, liq_str,
                                                    this.pm_confidence.as_deref().unwrap_or("Medium")
                                                )),
                                                generated_by: GeneratedBy::Agent("polymarket".into()),
                                            });
                                        }
                                        // Do NOT overwrite predicted_probability — that's the inside view
                                        this.messages.push(AssistantMessage {
                                            node: "question".into(),
                                            kind: MessageKind::Info,
                                            text: format!(
                                                "Base rate anchored to Polymarket crowd price: {:.1}%. Your model: {:.1}%",
                                                pm_price * 100.0,
                                                this.predicted_probability * 100.0,
                                            ),
                                        });
                                        cx.notify();
                                    }))
                                    .child("Anchor base rate"),
                            ),
                    )
                    // PM poll schedule selector
                    .child({
                        let current_interval = state.pm_poll_interval
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let schedules: Vec<(&str, u64)> = vec![
                            ("Off", 0),
                            ("5 min", 300),
                            ("15 min", 900),
                            ("30 min", 1800),
                            ("1 hr", 3600),
                            ("Daily", 86400),
                        ];
                        // Build the status chip: "refreshing…" while a
                        // request is in flight, "failed: <err>" when
                        // the last attempt errored, "updated 3 s ago"
                        // otherwise. Colour signals status at a glance.
                        let (status_text, status_color) = if state.pm_refresh_in_flight {
                            ("refreshing…".to_string(), rgb(theme::CYAN))
                        } else if let Some(err) = state.pm_last_refresh_error.as_ref() {
                            // Truncate long errors so the chip doesn't blow up.
                            let short = if err.len() > 40 {
                                format!("failed: {}…", &err[..40])
                            } else {
                                format!("failed: {}", err)
                            };
                            (short, rgb(theme::RED))
                        } else if let Some(ts) = state.pm_last_refresh_at {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(ts);
                            let secs = now.saturating_sub(ts);
                            let rel = if secs < 5 {
                                "just now".to_string()
                            } else if secs < 60 {
                                format!("{}s ago", secs)
                            } else if secs < 3600 {
                                format!("{}m ago", secs / 60)
                            } else if secs < 86400 {
                                format!("{}h ago", secs / 3600)
                            } else {
                                format!("{}d ago", secs / 86400)
                            };
                            (format!("updated {}", rel), rgb(theme::GREEN))
                        } else {
                            ("never refreshed".to_string(), rgb(theme::FG_FAINT))
                        };
                        let refresh_disabled = state.pm_refresh_in_flight;

                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .mt(px(4.0))
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child("Crowd price refresh:"),
                            )
                            // Manual "refresh now" — fires an immediate
                            // snapshot regardless of the polling cadence,
                            // so the operator never has to wait 5–60 min
                            // to see a live crowd value after landing on
                            // the forecast. Grays out while a refresh is
                            // in flight so double-clicks can't stack.
                            .child(
                                div()
                                    .id("pm-refresh-now")
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .text_size(px(8.0))
                                    .bg(rgb(theme::BG))
                                    .text_color(if refresh_disabled {
                                        rgb(theme::FG_FAINT)
                                    } else {
                                        rgb(theme::CYAN)
                                    })
                                    .when(!refresh_disabled, |el| {
                                        el.cursor_pointer().hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.refresh_pm_price_now(cx);
                                            }))
                                    })
                                    .child(if refresh_disabled {
                                        "↻ Refreshing…"
                                    } else {
                                        "↻ Refresh now"
                                    }),
                            )
                            // Status chip — the whole point of this
                            // block: gives the operator confidence the
                            // refresh is actually firing (and surfaces
                            // silent server errors when it isn't).
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .text_size(px(8.0))
                                    .text_color(status_color)
                                    .child(status_text),
                            )
                            .children(schedules.into_iter().map(|(label, secs)| {
                                let is_active = if secs == 0 {
                                    current_interval == 0
                                } else {
                                    current_interval == secs
                                };
                                let s = secs;
                                div()
                                    .id(ElementId::Name(format!("pm-poll-{}", secs).into()))
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .text_size(px(8.0))
                                    .cursor_pointer()
                                    .when(is_active, |el| {
                                        el.bg(rgb(theme::PURPLE))
                                            .text_color(rgb(theme::BG_DEEP))
                                            .font_weight(FontWeight::BOLD)
                                    })
                                    .when(!is_active, |el| {
                                        el.bg(rgb(theme::BG))
                                            .text_color(rgb(theme::FG_DIM))
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if s == 0 {
                                            this.stop_pm_poll(cx);
                                        } else {
                                            this.set_pm_poll_interval(
                                                std::time::Duration::from_secs(s),
                                                cx,
                                            );
                                        }
                                    }))
                                    .child(label)
                            }))
                    }),
            )
        })
}

fn render_driver_card(
    _index: usize,
    driver: Option<&DriverStmt>,
    is_focused: bool,
    assigned_agents: &[String],
    agent_runs: &[AgentExecution],
    messages: &[AssistantMessage],
    evidence_items: &[&fermi::ast::EvidenceStmt],
    user_confidence: Option<f64>,
    pending_suggestion_count: usize,
    cx: &mut Context<CockpitState>,
    name: &str,
    learnable_status: Option<&LearnableDriverBadge>,
) -> AnyElement {
    let Some(driver) = driver else {
        return div().child("Unknown driver").into_any_element();
    };

    let type_label = match driver.driver_type {
        DriverType::Continuous => "continuous",
        DriverType::Binary => "binary",
        DriverType::Discrete => "discrete",
    };
    let type_color = match driver.driver_type {
        DriverType::Continuous => theme::GREEN,
        DriverType::Binary => theme::GOLD,
        DriverType::Discrete => theme::PURPLE,
    };

    let summary = match driver.driver_type {
        DriverType::Continuous => {
            if let Some(Distribution::Triangular {
                ref p5,
                ref p50,
                ref p95,
            }) = driver.distribution
            {
                let unit = driver.unit.as_deref().unwrap_or("");
                format!(
                    "{:.1} – {:.1} – {:.1} {}",
                    expr_to_f64(p5),
                    expr_to_f64(p50),
                    expr_to_f64(p95),
                    unit
                )
            } else {
                "no distribution".into()
            }
        }
        DriverType::Binary => {
            let p = driver.probability.unwrap_or(0.0);
            let m = driver.impact_multiplier.unwrap_or(1.0);
            format!("{:.0}% (×{:.1})", p * 100.0, m)
        }
        _ => "—".into(),
    };

    // Count messages for this driver
    let driver_node = format!("driver:{}", driver.name);
    let msg_count = messages.iter().filter(|m| m.node == driver_node).count();

    // Agents bound to this driver
    // (In current AST, agents are top-level with driver_refs)

    // Check if this driver has any evidence (from its assigned agents or matching evidence items)
    let has_driver_evidence = assigned_agents
        .iter()
        .any(|a| evidence_items.iter().any(|e| evidence_matches_agent(e, a)));
    let has_evidence_gap = assigned_agents.is_empty() && !has_driver_evidence;
    let any_agent_running = assigned_agents.iter().any(|a| {
        agent_runs
            .iter()
            .any(|r| r.agent_name == *a && r.status == AgentRunStatus::Running)
    });

    let border_color = if is_focused {
        theme::CYAN
    } else if has_evidence_gap {
        theme::RED
    } else if any_agent_running {
        theme::GOLD
    } else {
        theme::FG_FAINT
    };
    let name_owned = name.to_string();

    div()
        .id(ElementId::Name(format!("driver-card-{}", name).into()))
        .mx(px(8.0))
        .my(px(3.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgb(border_color))
        .bg(if is_focused {
            rgb(theme::BG_ACTIVE)
        } else {
            rgb(theme::BG_ELEVATED)
        })
        .cursor_pointer()
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .on_click(cx.listener(move |this, _event, _window, cx| {
            this.focus_driver(&name_owned, cx);
        }))
        .flex()
        .flex_col()
        .gap(px(4.0))
        // Header: name + type + summary
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgb(theme::FG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .min_w(px(0.0))
                        // overflow_hidden + truncate the long names
                        // ('dynamic_performance' wraps to two lines without
                        // this and breaks the card header layout).
                        .overflow_hidden()
                        .child({
                            let raw = driver.display_name.as_deref().unwrap_or(&driver.name);
                            // Soft cap at 22 chars so 'institutional_capacity'
                            // and 'tactical_efficiency' fit alongside the
                            // type/learnable chips without truncation
                            // ellipsis kicking in.
                            if raw.chars().count() > 22 {
                                format!("{}…", raw.chars().take(21).collect::<String>())
                            } else {
                                raw.to_string()
                            }
                        }),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(type_color))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(2.0))
                        .bg(rgb(theme::BG))
                        .flex_shrink_0()
                        .child(type_label),
                )
                // BayesOps learnable badge — visible only when the driver
                // opted in via `learnable: true`. Four states:
                //   • PendingReview (server-side staged fit): orange chip
                //     with delta + inline ✓ / ✗ buttons (Spec 23 R-2)
                //   • Fitted (last sim used a fit): green chip with n=…
                //   • PriorFallback (last sim used prior, cold start): yellow chip
                //   • None: neutral "learnable" chip (cyan)
                .when(driver.learnable, |el| {
                    match learnable_status {
                        Some(LearnableDriverBadge {
                            status:
                                LearnableBadgeStatus::PendingReview {
                                    pending_id,
                                    delta_pp,
                                    n_observations,
                                    ..
                                },
                            driver_name: pending_driver,
                        }) => {
                            // Compose label: "↻ pending +6pp" or "↻ pending"
                            let badge_text = match delta_pp {
                                Some(d) => format!("↻ pending {:+.1}pp", d),
                                None => format!("↻ pending (n={})", n_observations),
                            };
                            let badge_color = theme::GOLD;
                            let pending_id = pending_id.clone();
                            let pending_id_for_reject = pending_id.clone();
                            let driver_name_accept = pending_driver.clone();
                            let driver_name_reject = pending_driver.clone();
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(badge_color))
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .border_1()
                                    .border_color(rgb(badge_color))
                                    .flex_shrink_0()
                                    .child(badge_text),
                            )
                            // ✓ Accept button
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("bayesops-accept-{}", driver_name_accept).into(),
                                    ))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::GREEN))
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .border_1()
                                    .border_color(rgb(theme::GREEN))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .flex_shrink_0()
                                    .child("✓")
                                    .on_click(cx.listener(move |this, event, _window, cx| {
                                        // Stop the click from focusing the driver card.
                                        let _ = event;
                                        this.accept_bayesops_pending(
                                            &driver_name_accept,
                                            &pending_id,
                                            cx,
                                        );
                                    })),
                            )
                            // ✗ Dismiss button
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("bayesops-reject-{}", driver_name_reject).into(),
                                    ))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::RED))
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .border_1()
                                    .border_color(rgb(theme::RED))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .flex_shrink_0()
                                    .child("✗")
                                    .on_click(cx.listener(move |this, event, _window, cx| {
                                        let _ = event;
                                        this.reject_bayesops_pending(
                                            &driver_name_reject,
                                            &pending_id_for_reject,
                                            cx,
                                        );
                                    })),
                            )
                        }
                        _ => {
                            let (badge_text, badge_color) = match learnable_status {
                                Some(LearnableDriverBadge {
                                    status: LearnableBadgeStatus::Fitted { n_eff, .. },
                                    ..
                                }) => (format!("✓ fit n={:.0}", n_eff), theme::GREEN),
                                Some(LearnableDriverBadge {
                                    status: LearnableBadgeStatus::PriorFallback,
                                    ..
                                }) => ("⏳ prior".to_string(), theme::GOLD),
                                _ => ("◌ learnable".to_string(), theme::CYAN),
                            };
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(badge_color))
                                    .px(px(5.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .border_1()
                                    .border_color(rgb(badge_color))
                                    .flex_shrink_0()
                                    .child(badge_text),
                            )
                        }
                    }
                })
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
                        .min_w(px(0.0))
                        .child(summary),
                )
                // Distribution sparkline for continuous drivers
                .when(driver.driver_type == DriverType::Continuous, |el| {
                    if let Some(Distribution::Triangular {
                        ref p5,
                        ref p50,
                        ref p95,
                    }) = driver.distribution
                    {
                        let v5 = expr_to_f64(p5);
                        let v50 = expr_to_f64(p50);
                        let v95 = expr_to_f64(p95);
                        if v95 > v5 {
                            // Match sparkline bg to card bg so it blends seamlessly
                            let card_bg = if is_focused {
                                plotters::prelude::RGBColor(61, 68, 85) // BG_ACTIVE
                            } else {
                                plotters::prelude::RGBColor(39, 45, 56) // BG_ELEVATED
                            };
                            let chart_w = 120u32;
                            let chart_h = 24u32;
                            let rgb_buf = crate::charts::render_distribution_sparkline_on(
                                v5, v50, v95, chart_w, chart_h, card_bg,
                            );
                            let render_img =
                                crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);

                            let ev_count = evidence_items.len();
                            let spread = v95 - v5;
                            let skew = (v50 - v5) / spread - 0.5;
                            let shape_label = if skew.abs() < 0.08 {
                                "symmetric"
                            } else if skew > 0.0 {
                                "right-skewed"
                            } else {
                                "left-skewed"
                            };
                            let evidence_label = if ev_count == 0 {
                                "no evidence yet".to_string()
                            } else {
                                format!(
                                    "{} evidence item{}",
                                    ev_count,
                                    if ev_count == 1 { "" } else { "s" }
                                )
                            };

                            el.child(
                                gpui::img(gpui::ImageSource::Render(render_img))
                                    .w(gpui::px(chart_w as f32))
                                    .h(gpui::px(chart_h as f32)),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .min_w(px(0.0))
                                    .child(format!("{} spread · {}", shape_label, evidence_label)),
                            )
                        } else {
                            el
                        }
                    } else {
                        el
                    }
                })
                .when(msg_count > 0, |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::GOLD))
                            .child(format!(
                                "{} hint{}",
                                msg_count,
                                if msg_count == 1 { "" } else { "s" }
                            )),
                    )
                })
                .when(pending_suggestion_count > 0, |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::BG_DEEP))
                            .bg(rgb(theme::GOLD))
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .font_weight(FontWeight::BOLD)
                            .child(format!(
                                "💡 {} suggestion{}",
                                pending_suggestion_count,
                                if pending_suggestion_count == 1 {
                                    ""
                                } else {
                                    "s"
                                }
                            )),
                    )
                }),
        )
        // Driver confidence dots (based on evidence coverage or user override)
        .child({
            // Count evidence from agent runs first, then fall back to
            // counting evidence items directly (handles cases where
            // evidence_count wasn't properly updated on the run).
            let ev_count_from_runs: usize = assigned_agents
                .iter()
                .flat_map(|a| agent_runs.iter().filter(move |r| r.agent_name == *a))
                .map(|r| r.evidence_count)
                .sum();
            let ev_count_from_items: usize = assigned_agents
                .iter()
                .map(|a| {
                    evidence_items
                        .iter()
                        .filter(|e| evidence_matches_agent(e, a))
                        .count()
                })
                .sum();
            let ev_count = ev_count_from_runs.max(ev_count_from_items);
            let computed_conf = if ev_count >= 3 {
                0.8
            } else if ev_count >= 1 {
                0.5
            } else {
                0.2
            };
            let effective_conf = user_confidence.unwrap_or(computed_conf);
            let (conf_label, conf_color) = if effective_conf >= 0.7 {
                ("●●● High", theme::GREEN)
            } else if effective_conf >= 0.4 {
                ("●●○ Medium", theme::GOLD)
            } else {
                ("●○○ Low", theme::RED)
            };
            let label_prefix = if user_confidence.is_some() {
                "Confidence"
            } else {
                "Evidence"
            };
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(conf_color))
                        .px(px(4.0))
                        .child(format!(
                            "{}: {} {:.0}%",
                            label_prefix,
                            conf_label,
                            effective_conf * 100.0
                        )),
                )
                .when(
                    user_confidence.is_some()
                        && (user_confidence.unwrap() - computed_conf).abs() > 0.05,
                    |el| {
                        el.child(
                            div()
                                .text_size(px(8.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!("(computed: {:.0}%)", computed_conf * 100.0)),
                        )
                    },
                )
                .when(has_evidence_gap, |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::RED))
                            .px(px(4.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .bg(rgb(0x3D1F1F))
                            .child("⚠ No agents — assign one to research this driver"),
                    )
                })
                .when(
                    !has_evidence_gap && ev_count == 0 && !any_agent_running,
                    |el| {
                        el.child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::GOLD))
                                .px(px(4.0))
                                .child("◌ Awaiting evidence"),
                        )
                    },
                )
        })
        // Rationale (if present)
        .when(driver.rationale.is_some(), |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child(driver.rationale.as_deref().unwrap_or("").to_string()),
            )
        })
        // Assigned agents — visible entities showing live state
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .mt(px(4.0))
                .children(assigned_agents.iter().map(|agent_name| {
                    let run = agent_runs.iter().find(|r| r.agent_name == *agent_name);
                    let status = run.map(|r| &r.status);
                    let ev_count = run.map(|r| r.evidence_count).unwrap_or(0);
                    let confidence = run.and_then(|r| r.confidence);

                    let elapsed_str: String = match (
                        run.and_then(|r| r.started_at),
                        run.and_then(|r| r.completed_at),
                    ) {
                        (Some(start), Some(end)) => {
                            let secs = end.saturating_sub(start);
                            if secs < 60 {
                                format!(" ({secs}s)")
                            } else {
                                format!(" ({}m{}s)", secs / 60, secs % 60)
                            }
                        }
                        (Some(start), None) => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let secs = now.saturating_sub(start);
                            if secs < 60 {
                                format!(" ({secs}s…)")
                            } else {
                                format!(" ({}m{}s…)", secs / 60, secs % 60)
                            }
                        }
                        _ => String::new(),
                    };

                    let (status_icon, status_text, status_color, bg_color) = match status {
                        Some(AgentRunStatus::Running) => (
                            "⟳",
                            format!("researching…{}", elapsed_str),
                            theme::GOLD,
                            0x2A2D3A,
                        ),
                        Some(AgentRunStatus::Completed) => (
                            "✓",
                            format!("{} findings{}", ev_count, elapsed_str),
                            theme::GREEN,
                            theme::BG,
                        ),
                        Some(AgentRunStatus::Failed) => {
                            ("✗", format!("failed{}", elapsed_str), theme::RED, 0x3D1F1F)
                        }
                        _ => ("○", "idle".to_string(), theme::FG_DIM, theme::BG),
                    };

                    // Extract the base agent name (before the _driver suffix)
                    let display_name = agent_name.split('_').take(2).collect::<Vec<_>>().join("_");

                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(rgb(bg_color))
                        .border_1()
                        .border_color(rgb(status_color))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(rgb(status_color))
                                .w(px(18.0))
                                .child(status_icon.to_string()),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_grow()
                                .min_w(px(0.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(display_name),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(status_color))
                                        .child(status_text.clone()),
                                ),
                        )
                        .when(confidence.is_some(), |el| {
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child(format!("{:.0}%", confidence.unwrap_or(0.0) * 100.0)),
                            )
                        })
                        // Credits charged
                        .when(run.and_then(|r| r.credits_charged).is_some(), |el| {
                            let credits = run.and_then(|r| r.credits_charged).unwrap_or(0.0);
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child(format!("⚡{:.1}", credits)),
                            )
                        })
                        // Error details for failed agents
                        .when(matches!(status, Some(AgentRunStatus::Failed)), |el| {
                            let err_msg: String = run
                                .and_then(|r| r.error.as_ref())
                                .map(|e| e.chars().take(120).collect())
                                .unwrap_or_default();
                            if !err_msg.is_empty() {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::RED))
                                        .mt(px(2.0))
                                        .min_w(px(0.0))
                                        .child(format!("⚠ {}", err_msg)),
                                )
                            } else {
                                el
                            }
                        })
                        // Speech bubble — latest finding from this agent
                        .when(
                            ev_count > 0 || run.and_then(|r| r.latest_finding.as_ref()).is_some(),
                            |el| {
                                // Prefer the cached latest_finding (set during execution),
                                // fall back to searching evidence items
                                let latest: String = run
                                    .and_then(|r| r.latest_finding.as_ref())
                                    .cloned()
                                    .or_else(|| {
                                        evidence_items
                                            .iter()
                                            .filter(|e| evidence_matches_agent(*e, agent_name))
                                            .last()
                                            .and_then(|e| e.summary.as_ref())
                                            .map(|s| s.chars().take(100).collect())
                                    })
                                    .unwrap_or_default();
                                if !latest.is_empty() {
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .mt(px(2.0))
                                            .min_w(px(0.0))
                                            .child(format!(
                                                "💬 {}",
                                                if latest.chars().count() > 100 {
                                                    format!(
                                                        "{}…",
                                                        latest.chars().take(97).collect::<String>()
                                                    )
                                                } else {
                                                    latest
                                                }
                                            )),
                                    )
                                } else {
                                    el
                                }
                            },
                        )
                        // Retry / Re-run button for failed or completed agents
                        .when(
                            matches!(
                                status,
                                Some(AgentRunStatus::Failed) | Some(AgentRunStatus::Completed)
                            ),
                            |el| {
                                let an = agent_name.clone();
                                let btn_label = if matches!(status, Some(AgentRunStatus::Failed)) {
                                    "↻ Retry"
                                } else {
                                    "↻ Re-run"
                                };
                                el.child(
                                    div()
                                        .id(ElementId::Name(format!("retry-{}", an).into()))
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(theme::BG_ELEVATED))
                                        .border_1()
                                        .border_color(rgb(theme::GOLD))
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::GOLD))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.retry_agent(&an, cx);
                                        }))
                                        .child(btn_label),
                                )
                            },
                        )
                }))
                // "Assign agent" button
                .child({
                    let driver_name = name.to_string();
                    div()
                        .id(ElementId::Name(format!("assign-agent-{}", name).into()))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG))
                        .border_1()
                        .border_color(rgb(theme::BLUE))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.open_agent_picker(&driver_name, cx);
                        }))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::BLUE))
                                .child("+ Assign Agent"),
                        )
                }),
        )
        .into_any_element()
}

/// Right panel — context-sensitive content based on focused node.
fn render_right_panel(
    state: &CockpitState,
    focused: &FocusedNode,
    cx: &mut Context<CockpitState>,
) -> AnyElement {
    match focused {
        FocusedNode::AgentPicker(driver_name) => {
            render_agent_picker(state, driver_name, cx).into_any_element()
        }
        FocusedNode::Driver(name) => {
            render_driver_editor_and_evidence(state, name, cx).into_any_element()
        }
        _ => {
            // Default: assistant messages
            div()
                .flex()
                .flex_col()
                .p(px(16.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .child("Click a driver on the left to edit, or click '+ agent' to assign research agents."),
                )
                .into_any_element()
        }
    }
}

/// Agent picker panel — shown when assigning an agent to a driver.
fn render_agent_picker(
    state: &CockpitState,
    driver_name: &str,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    // ── Driver context ────────────────────────────────────────────
    let driver = state.program.driver(driver_name);
    let driver_display = driver
        .and_then(|d| d.display_name.as_deref())
        .unwrap_or(driver_name)
        .to_string();
    let rationale = driver
        .and_then(|d| d.rationale.as_deref())
        .unwrap_or("")
        .to_string();
    let (p5, p50, p95) = driver
        .and_then(|d| d.distribution.as_ref())
        .map(|dist| match dist {
            Distribution::Triangular { p5, p50, p95 } => {
                (expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95))
            }
            _ => (0.8, 1.0, 1.2),
        })
        .unwrap_or((0.8, 1.0, 1.2));

    // ── Existing evidence for this driver ─────────────────────────
    let driver_agents: Vec<String> = state
        .program
        .agents()
        .iter()
        .filter(|a| a.driver_refs.contains(&driver_name.to_string()))
        .map(|a| a.name.clone())
        .collect();
    let existing_evidence: Vec<_> = state
        .program
        .evidence_items()
        .into_iter()
        .filter(|e| {
            driver_agents.iter().any(|a| evidence_matches_agent(e, a)) || e.id.contains(driver_name)
        })
        .collect();

    // ── Persisted schedules for this driver ───────────────────────
    let driver_schedules: Vec<ForecastSchedule> = state
        .schedules
        .iter()
        .filter(|s| s.driver_name == driver_name && s.enabled)
        .cloned()
        .collect();

    // ── Recommended agent (domain-first routing) ──────────────────
    let question_text = state
        .program
        .question()
        .map(|q| q.text.clone())
        .unwrap_or_default();
    let domain = detect_domain(&question_text);
    let dl = driver_name.to_lowercase();
    let rl = rationale.to_lowercase();
    let combined = format!("{} {}", dl, rl);

    let recommended = if combined.contains("sentiment")
        || combined.contains("opinion")
        || combined.contains("perception")
    {
        "sentiment_analyzer"
    } else if combined.contains("entity")
        || combined.contains("regulatory")
        || combined.contains("legal")
        || combined.contains("ownership")
    {
        "entity_investigator"
    } else if combined.contains("stock")
        || combined.contains("valuation")
        || combined.contains("earnings")
        || combined.contains("eps")
        || combined.contains("p/e")
        || combined.contains("revenue growth")
        || combined.contains("margin")
        || combined.contains("balance sheet")
        || combined.contains("cash flow")
        || combined.contains("dcf")
        || combined.contains("intrinsic value")
        || combined.contains("share price")
        || combined.contains("dividend")
        || combined.contains("buyback")
        || combined.contains("ticker")
        || combined.contains("ipo")
        || combined.contains("analyst estimate")
    {
        "equity_analyst"
    } else if combined.contains("market")
        || combined.contains("competition")
        || combined.contains("partnership")
        || combined.contains("revenue")
        || combined.contains("commercial")
    {
        "market_research"
    } else if combined.contains("clinical")
        || combined.contains("trial")
        || combined.contains("fda")
        || combined.contains("drug")
    {
        "biotech_analyst"
    } else if combined.contains("nba")
        || combined.contains("basketball")
        || combined.contains("elo")
    {
        "nba_analyst"
    } else {
        match domain.as_str() {
            "sports_nba" | "basketball" => "nba_analyst",
            "biotech" | "pharma" => "biotech_analyst",
            "sports_football" | "sports_nfl" | "sports_other" => "market_research",
            "stocks" | "equity" => "equity_analyst",
            "finance" => "macro_forecaster",
            "technology" => "market_research",
            _ => "macro_forecaster",
        }
    };

    // Pre-fill the query with a domain-specific formulation
    let suggested_query = formulate_research_query(
        &question_text,
        &driver_display,
        &rationale,
        recommended,
        &domain,
        p5,
        p50,
        p95,
    );

    // Get all available agents for the "Other agents" section
    let available_agents: Vec<(String, String, Vec<String>)> = state
        .registry
        .list_cards()
        .unwrap_or_default()
        .iter()
        .filter(|card| {
            card.metadata.tags.iter().any(|t| t == "fermi-orchestra")
                && card.agent_id != "fermi"
                && card.agent_id != recommended
        })
        .map(|card| {
            (
                card.agent_id.clone(),
                card.metadata.description.clone(),
                card.capabilities.skills.clone(),
            )
        })
        .collect();

    let recommended_desc = state
        .registry
        .get(recommended)
        .map(|c| c.metadata.description.clone())
        .unwrap_or_default();

    let dn = driver_name.to_string();

    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        // ── Header with driver context ────────────────────────────
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(theme::CYAN))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("🔬 Research: {}", driver_display)),
                )
                .child({
                    let dn2 = dn.clone();
                    div()
                        .id("close-agent-picker")
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG)))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.focused_node = FocusedNode::Driver(dn2.clone());
                            this.populate_editor_from_driver(&dn2, cx);
                            cx.notify();
                        }))
                        .child("✕")
                }),
        )
        // ── Driver context card ───────────────────────────────────
        .child(
            div()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .when(!rationale.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_DIM))
                            .min_w(px(0.0))
                            .child(rationale.clone()),
                    )
                })
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::CYAN))
                        .font_family("Ubuntu Mono, DejaVu Sans Mono, monospace")
                        .child(format!("p5={:.2}  p50={:.2}  p95={:.2}", p5, p50, p95)),
                )
                .when(!existing_evidence.is_empty(), |el| {
                    // Compute average quality across all evidence for this driver
                    let avg_quality: f64 = if existing_evidence.is_empty() {
                        0.0
                    } else {
                        existing_evidence
                            .iter()
                            .map(|e| score_evidence_quality(e).0)
                            .sum::<f64>()
                            / existing_evidence.len() as f64
                    };
                    let (q_label, q_color) = if avg_quality >= 0.7 {
                        ("High", theme::GREEN)
                    } else if avg_quality >= 0.4 {
                        ("Medium", theme::GOLD)
                    } else {
                        ("Low", theme::RED)
                    };
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::GREEN))
                                    .child(format!(
                                        "✓ {} evidence items collected",
                                        existing_evidence.len()
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(8.0))
                                    .text_color(rgb(q_color))
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG_ELEVATED))
                                    .child(format!("Quality: {} ({:.0}%)", q_label, avg_quality * 100.0)),
                            ),
                    )
                })
                .when(existing_evidence.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::RED))
                            .child("⚠ No evidence yet — research needed"),
                    )
                }),
        )
        // ── Already-assigned agents (auto + manual) ───────────────
        // Every agent attached to this driver gets its own schedule
        // controls here. Previously the ▶ Run Now / 📅 Daily / 📅 Weekly
        // affordance only appeared on the "Recommended" card, which
        // meant auto-assigned agents had no way to be scheduled.
        // Now any agent on the driver — regardless of how it got there
        // — can be run on-demand or persisted as a recurring schedule.
        .when(!driver_agents.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child("Currently assigned to this driver:"),
            )
            .child({
                let mut col = div().flex().flex_col().gap(px(6.0));
                for assigned in &driver_agents {
                    // The AST agent name is the BOUND name
                    // (`<base>_<driver>`). The schedule API and the
                    // registry are keyed on the BASE name. Resolve once
                    // here so the closures below pass the right id.
                    let bound_name = assigned.clone();
                    let base_id =
                        base_agent_id_for_bound(&bound_name, driver_name);

                    let dn_run = dn.clone();
                    let baid_run = base_id.clone();
                    let dn_daily = dn.clone();
                    let baid_daily = base_id.clone();
                    let dn_weekly = dn.clone();
                    let baid_weekly = base_id.clone();

                    // Description from the registry — lookup uses the
                    // BASE agent id so auto-spawned agents still resolve.
                    let desc = state
                        .registry
                        .get(&base_id)
                        .ok()
                        .map(|c| c.metadata.description.clone())
                        .filter(|d| !d.is_empty())
                        .unwrap_or_else(|| "Research agent".into());

                    col = col.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG_ELEVATED))
                            .border_1()
                            .border_color(rgb(theme::FG_FAINT))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::CYAN))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            // Display the BASE agent id, not
                                            // the bound name (which contains
                                            // a redundant driver suffix).
                                            .child(base_id.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_DIM))
                                            .min_w(px(0.0))
                                            .child(desc),
                                    ),
                            )
                            // Same three actions as the Recommended card,
                            // re-issued so auto-assigned agents are
                            // schedulable.
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("assigned-run-{}-{}", driver_name, base_id).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::CYAN))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .border_1()
                                            .border_color(rgb(theme::CYAN))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                                this.update_schedule_for_assigned_agent(
                                                    &dn_run, &baid_run, Schedule::Once, cx,
                                                );
                                            }))
                                            .child("▶ Run Now"),
                                    )
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("assigned-daily-{}-{}", driver_name, base_id).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::GREEN))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .border_1()
                                            .border_color(rgb(theme::GREEN))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                                this.update_schedule_for_assigned_agent(
                                                    &dn_daily,
                                                    &baid_daily,
                                                    Schedule::Every {
                                                        interval: 1,
                                                        unit: fermi::ast::TimeUnit::Day,
                                                    },
                                                    cx,
                                                );
                                            }))
                                            .child("📅 Daily"),
                                    )
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("assigned-weekly-{}-{}", driver_name, base_id).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::GOLD))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .border_1()
                                            .border_color(rgb(theme::GOLD))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                                this.update_schedule_for_assigned_agent(
                                                    &dn_weekly,
                                                    &baid_weekly,
                                                    Schedule::Every {
                                                        interval: 1,
                                                        unit: fermi::ast::TimeUnit::Week,
                                                    },
                                                    cx,
                                                );
                                            }))
                                            .child("📅 Weekly"),
                                    ),
                            ),
                    );
                }
                col
            })
        })
        // ── Recommended agent (highlighted) ───────────────────────
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("Recommended for this driver:"),
        )
        .child({
            let rec_id = recommended.to_string();
            let dn_rec = dn.clone();
            div()
                .px(px(10.0))
                .py(px(10.0))
                .rounded(px(6.0))
                .bg(rgb(0x1A2332))
                .border_1()
                .border_color(rgb(theme::CYAN))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(recommended.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::GREEN))
                                .px(px(4.0))
                                .py(px(1.0))
                                .rounded(px(2.0))
                                .bg(rgb(theme::BG))
                                .child("★ best match"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .min_w(px(0.0))
                        .child(recommended_desc),
                )
                // Query preview
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .mt(px(4.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(rgb(theme::FG_FAINT))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child("Research query (edit below to customize):"),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_DIM))
                                .min_w(px(0.0))
                                .child(
                                    suggested_query
                                        .chars()
                                        .take(200)
                                        .collect::<String>()
                                        + if suggested_query.len() > 200 { "…" } else { "" },
                                ),
                        ),
                )
                // Action buttons — prominent
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .mt(px(4.0))
                        .child(
                            div()
                                .id(ElementId::Name(
                                    format!("research-now-{}", recommended).into(),
                                ))
                                .text_size(px(11.0))
                                .text_color(rgb(theme::BG))
                                .font_weight(FontWeight::BOLD)
                                .px(px(14.0))
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::CYAN))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::GREEN)))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    // If user typed a custom research question, enrich it
                                    let user_q = this.driver_research_input.read(cx).text().to_string();
                                    if !user_q.trim().is_empty() {
                                        let enriched = this.enrich_driver_query(&dn_rec, &user_q, cx);
                                        this.agent_query_input.update(cx, |input, cx| {
                                            input.set_text(&enriched.replace('\n', " ").replace("  ", " "), cx)
                                        });
                                    }
                                    this.assign_agent_to_driver(
                                        &dn_rec,
                                        &rec_id,
                                        Schedule::Once,
                                        cx,
                                    );
                                }))
                                .child("▶ Research Now"),
                        )
                        .child({
                            let rec_id2 = recommended.to_string();
                            let dn_daily = dn.clone();
                            div()
                                .id(ElementId::Name(
                                    format!("research-daily-{}", recommended).into(),
                                ))
                                .text_size(px(10.0))
                                .text_color(rgb(theme::GREEN))
                                .px(px(10.0))
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::BG))
                                .border_1()
                                .border_color(rgb(theme::GREEN))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.assign_agent_to_driver(
                                        &dn_daily,
                                        &rec_id2,
                                        Schedule::Every {
                                            interval: 1,
                                            unit: fermi::ast::TimeUnit::Day,
                                        },
                                        cx,
                                    );
                                }))
                                .child("📅 Daily")
                        })
                        .child({
                            let rec_id3 = recommended.to_string();
                            let dn_weekly = dn.clone();
                            div()
                                .id(ElementId::Name(
                                    format!("research-weekly-{}", recommended).into(),
                                ))
                                .text_size(px(10.0))
                                .text_color(rgb(theme::GOLD))
                                .px(px(10.0))
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::BG))
                                .border_1()
                                .border_color(rgb(theme::GOLD))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.assign_agent_to_driver(
                                        &dn_weekly,
                                        &rec_id3,
                                        Schedule::Every {
                                            interval: 1,
                                            unit: fermi::ast::TimeUnit::Week,
                                        },
                                        cx,
                                    );
                                }))
                                .child("📅 Weekly")
                        }),
                )
        })
        // ── Active schedules for this driver ─────────────────────
        .when(!driver_schedules.is_empty(), |el| {
            el.child({
                let rows: Vec<AnyElement> = driver_schedules
                    .iter()
                    .map(|sched| {
                        let sid_run = sched.id.clone();
                        let sid_del = sched.id.clone();
                        let label = if sched.interval_hours >= 168 {
                            format!("every {} week", sched.interval_hours / 168)
                        } else {
                            format!("every {}h", sched.interval_hours)
                        };
                        let next = sched
                            .next_run_at
                            .get(..16)
                            .unwrap_or(&sched.next_run_at)
                            .replace('T', " ");
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(4.0))
                            .py(px(3.0))
                            .rounded(px(3.0))
                            .bg(rgb(theme::BG_ACTIVE))
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::GREEN))
                                    .child("🔁"),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .flex_grow()
                                    .child(format!(
                                        "{} {} — next: {}",
                                        sched.agent_id, label, next
                                    )),
                            )
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("run-now-{}", sid_run).into(),
                                    ))
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::CYAN))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .bg(rgb(theme::BG))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.run_now_schedule(&sid_run, cx);
                                    }))
                                    .child("▶"),
                            )
                            .child(
                                div()
                                    .id(ElementId::Name(
                                        format!("del-sched-{}", sid_del).into(),
                                    ))
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .rounded(px(3.0))
                                    .bg(rgb(theme::BG))
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(rgb(theme::RED)))
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.delete_schedule(&sid_del, cx);
                                    }))
                                    .child("×"),
                            )
                            .into_any_element()
                    })
                    .collect();

                div()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::FG_FAINT))
                            .font_weight(FontWeight::SEMIBOLD)
                            .mb(px(2.0))
                            .child("SCHEDULED AUTO-RESEARCH"),
                    )
                    .children(rows)
            })
        })
        // ── Driver research question ──────────────────────────────
        // The user types what they want to know — the system enriches
        // it into a structured query with driver context and params.
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG))
                .border_1()
                .border_color(rgb(theme::FG_FAINT))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("What do you want to research?"),
                )
                .child(state.driver_research_input.clone())
                .child(
                    div()
                        .text_size(px(8.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Type your question — the system will structure it for the best agent. Or use the pre-filled query below."),
                ),
        )
        // ── Evidence URL/link input ───────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("📎 Add evidence (URL or text)"),
                )
                .child(state.evidence_source_input.clone())
                .child(state.evidence_summary_input.clone())
                .child({
                    let dn_ev = dn.clone();
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(
                            div()
                                .id("add-evidence-btn")
                                .text_size(px(10.0))
                                .text_color(rgb(theme::GREEN))
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(3.0))
                                .bg(rgb(theme::BG_ACTIVE))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.add_manual_evidence(cx);
                                }))
                                .child("+ Add Evidence"),
                        )
                        .child(
                            div()
                                .id("analyze-url-btn")
                                .text_size(px(10.0))
                                .text_color(rgb(theme::BLUE))
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(3.0))
                                .bg(rgb(theme::BG_ACTIVE))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    let source = this.evidence_source_input.read(cx).text().to_string();
                                    if source.contains("http://") || source.contains("https://") {
                                        this.ingest_url_evidence(&dn_ev, &source, cx);
                                        this.evidence_source_input.update(cx, |input, cx| input.set_text("", cx));
                                    } else {
                                        this.messages.push(AssistantMessage {
                                            node: "evidence".into(),
                                            kind: MessageKind::Warning,
                                            text: "Enter a URL (https://...) in the Source field to analyze".into(),
                                        });
                                        cx.notify();
                                    }
                                }))
                                .child("🔍 Analyze URL"),
                        )
                })
                .child(
                    div()
                        .text_size(px(8.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Paste a URL and click 'Analyze URL' — an agent will summarize it and suggest how it impacts this driver."),
                ),
        )
        // ── Advanced: raw query editor ────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Advanced: edit the full agent query"),
                )
                .child(state.agent_query_input.clone()),
        )
        // ── Other available agents ────────────────────────────────
        .when(!available_agents.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .pt(px(6.0))
                    .border_t_1()
                    .border_color(rgb(theme::FG_FAINT))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_FAINT))
                            .child("Or choose a different agent:"),
                    )
                    .children(available_agents.iter().map(|(agent_id, description, skills)| {
                        let aid = agent_id.clone();
                        let dn3 = dn.clone();
                        div()
                            .id(ElementId::Name(format!("pick-alt-{}", agent_id).into()))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(rgb(theme::FG_FAINT))
                            .cursor_pointer()
                            .hover(|s| s.border_color(rgb(theme::BLUE)).bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.assign_agent_to_driver(&dn3, &aid, Schedule::Once, cx);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_grow()
                                    .min_w(px(0.0))
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::FG))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(agent_id.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_DIM))
                                            .min_w(px(0.0))
                                            .child(
                                                description
                                                    .chars()
                                                    .take(100)
                                                    .collect::<String>(),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme::BLUE))
                                    .px(px(8.0))
                                    .py(px(3.0))
                                    .rounded(px(3.0))
                                    .bg(rgb(theme::BG_ACTIVE))
                                    .child("Run ▶"),
                            )
                    })),
            )
        })
        .when(available_agents.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child("No research agents found in registry."),
            )
        })
}

/// Render the per-driver `learnable` toggle row inside the driver editor.
///
/// Two compositions side by side:
///   1. The toggle pill itself (clickable, shows ON / OFF state).
///   2. The current resolution status from the last sim — "Frozen" /
///      "Cold start (prior)" / "Fitted from N obs (±CI)". This is the
///      live BayesOps signal the user came here to see.
fn render_learnable_toggle(
    state: &CockpitState,
    name: &str,
    cx: &mut Context<CockpitState>,
) -> gpui::Div {
    let driver = state.program.driver(name);
    let learnable = driver.map(|d| d.learnable).unwrap_or(false);

    // Find the most recent resolution for this driver, if any.
    let resolution = state
        .sim_results
        .as_ref()
        .and_then(|sr| sr.learnable_drivers.iter().find(|b| b.driver_name == name));

    let (toggle_label, toggle_color, toggle_bg) = if learnable {
        ("● learnable", theme::CYAN, theme::BG_ELEVATED)
    } else {
        ("○ frozen", theme::FG_FAINT, theme::BG)
    };

    let toggle_name = name.to_string();
    let toggle = div()
        .id(ElementId::Name(format!("learnable-toggle-{}", name).into()))
        .px(px(10.0))
        .py(px(4.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgb(toggle_color))
        .bg(rgb(toggle_bg))
        .text_size(px(11.0))
        .text_color(rgb(toggle_color))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .on_click(cx.listener(move |this, _ev, _w, cx| {
            this.toggle_driver_learnable(&toggle_name, cx);
        }))
        .child(toggle_label);

    let status_chip = match resolution {
        // Driver is learnable and got a BayesOps fit this run — show the
        // tightness signal. This is the magic moment for the user: their
        // hand-typed prior was just replaced by a data-informed posterior.
        Some(LearnableDriverBadge {
            status:
                LearnableBadgeStatus::Fitted {
                    family,
                    n_eff,
                    ci_width,
                },
            ..
        }) => Some((
            format!(
                "✓ fitted · {} · n={:.0} · 90% CI ±{:.2}",
                family,
                n_eff,
                ci_width / 2.0
            ),
            theme::GREEN,
        )),
        // Driver is learnable but no fit was found — cold start. The prior
        // distribution above is being sampled directly.
        Some(LearnableDriverBadge {
            status: LearnableBadgeStatus::PriorFallback,
            ..
        }) => Some((
            "⏳ cold start · using prior · BayesOps has no data yet".into(),
            theme::GOLD,
        )),
        // Spec 23 R-2: a pending fit is staged on the server. This toggle
        // doesn't gate the decision — the per-driver sparkline badge has the
        // accept/dismiss buttons. We just show a heads-up chip here.
        Some(LearnableDriverBadge {
            status:
                LearnableBadgeStatus::PendingReview {
                    delta_pp,
                    n_observations,
                    ..
                },
            ..
        }) => Some((
            match delta_pp {
                Some(d) => format!(
                    "↻ pending fit · Δ{:+.1}pp · n={} · review in sparkline",
                    d, n_observations
                ),
                None => format!("↻ pending fit · n={} · review in sparkline", n_observations),
            },
            theme::GOLD,
        )),
        // Either the driver isn't learnable or the sim hasn't run since the
        // toggle was flipped — no chip.
        None => {
            if learnable {
                Some((
                    "↻ learnable · run sim to see resolution".into(),
                    theme::FG_DIM,
                ))
            } else {
                None
            }
        }
    };

    let mut row = div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .w(px(80.0))
                .child("BayesOps:"),
        )
        .child(toggle);

    if let Some((text, color)) = status_chip {
        row = row.child(div().text_size(px(10.0)).text_color(rgb(color)).child(text));
    }

    row
}

/// Driver editor + evidence panel (shown when a driver is focused).
fn render_driver_editor_and_evidence(
    state: &CockpitState,
    name: &str,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    let driver = state.program.driver(name);
    let is_continuous = driver
        .map(|d| d.driver_type == DriverType::Continuous)
        .unwrap_or(true);

    let rationale_text = driver
        .and_then(|d| d.rationale.as_deref())
        .unwrap_or("")
        .to_string();

    // Evidence for this driver (from agents bound to it)
    let driver_agents: Vec<&str> = state
        .program
        .agents()
        .iter()
        .filter(|a| a.driver_refs.contains(&name.to_string()))
        .map(|a| a.name.as_str())
        .collect();

    let driver_evidence: Vec<&EvidenceStmt> = state
        .program
        .evidence_items()
        .into_iter()
        .filter(|e| {
            // Evidence from agents assigned to this driver
            driver_agents
                .iter()
                .any(|agent_name| evidence_matches_agent(e, agent_name))
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(16.0))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        // Header with close button
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(theme::CYAN))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("Editing: {}", name)),
                )
                .child(
                    div()
                        .id("close-editor")
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG)))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.save_focused_driver(cx);
                            this.focused_node = FocusedNode::Question;
                            cx.notify();
                        }))
                        .child("✕ Close"),
                )
                .child({
                    let del_name = name.to_string();
                    div()
                        .id("delete-driver-btn")
                        .text_size(px(11.0))
                        .text_color(rgb(theme::RED))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.delete_driver(&del_name, cx);
                        }))
                        .child("Delete")
                }),
        )
        // Rationale
        .when(!rationale_text.is_empty(), |el| {
            el.child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child(rationale_text),
            )
        })
        // ── PINNED: pending agent suggestions for this driver ───────
        //
        // Suggestions are the action-required gate that turns research
        // into model change: an agent runs, says 'p50 should move from
        // 1.42 to 1.55', the operator accepts, the driver's prior shifts,
        // and the next sim reflects the change. Without these visible,
        // research is just text in the evidence pane — nothing happens.
        //
        // Pinned at the top (right under the header / rationale) rather
        // than buried under the evidence list. The render is identical
        // to the legacy block at line ~10426, just placed earlier so
        // it's always above the fold.
        .child(render_pinned_suggestions(state, name, cx))
        // Editor fields
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .child(div().w(px(140.0)).child(state.editor_name.clone()))
                .child(
                    div()
                        .flex_grow()
                        .min_w(px(0.0))
                        .child(state.editor_rationale.clone()),
                ),
        )
        .when(is_continuous, |el| {
            el.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().w(px(90.0)).child(state.editor_p5.clone()))
                    .child(div().w(px(90.0)).child(state.editor_p50.clone()))
                    .child(div().w(px(90.0)).child(state.editor_p95.clone()))
                    .child(div().w(px(90.0)).child(state.editor_unit.clone())),
            )
        })
        // Learnable toggle — opt this driver into BayesOps-managed distribution
        // fitting. When ON, the `distribution:` above acts as the prior and
        // BayesOps' fitted posterior (written to `params.<name>_fitted` by the
        // backend) overrides at sim time. Visual: cyan when learnable, grey
        // (no border) when frozen. The toggle is render-aware of the current
        // run's resolution status — see `render_learnable_toggle`.
        .child(render_learnable_toggle(state, name, cx))
        .when(!is_continuous, |el| {
            el.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().w(px(120.0)).child(state.editor_prob.clone()))
                    .child(div().w(px(120.0)).child(state.editor_impact.clone())),
            )
        })
        // Confidence override (user-settable per driver)
        .child({
            let user_conf = state.driver_confidence.get(name).copied();
            // Compute evidence-based confidence for this driver
            let ev_count = state
                .program
                .agents()
                .iter()
                .filter(|a| a.driver_refs.contains(&name.to_string()))
                .flat_map(|a| {
                    state
                        .agent_runs
                        .iter()
                        .filter(move |r| r.agent_name == a.name)
                })
                .map(|r| r.evidence_count)
                .sum::<usize>();
            let computed_conf = if ev_count >= 3 {
                0.8
            } else if ev_count >= 1 {
                0.5
            } else {
                0.2
            };

            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().w(px(90.0)).child(state.editor_confidence.clone()))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!("Computed: {:.0}%", computed_conf * 100.0)),
                        )
                        .when(user_conf.is_some(), |el| {
                            let uc = user_conf.unwrap();
                            let delta = ((uc - computed_conf) * 100.0) as i64;
                            let (sign, color) = if delta > 0 {
                                ("+", theme::GREEN)
                            } else if delta < 0 {
                                ("", theme::RED)
                            } else {
                                ("±", theme::FG_DIM)
                            };
                            el.child(div().text_size(px(9.0)).text_color(rgb(color)).child(
                                format!("Override: {:.0}% ({}{}pp)", uc * 100.0, sign, delta),
                            ))
                        })
                        .when(user_conf.is_none(), |el| {
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child("Leave empty to use computed confidence"),
                            )
                        }),
                )
        })
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("Values save when you close, switch drivers, or simulate (Ctrl+R)."),
        )
        // ── Scheduled research for this driver ────────────────────
        // Every agent attached to this driver — auto-assigned by Fermi
        // during decomposition OR manually added via the picker — gets
        // its own schedule controls here. Previously the ▶ / 📅 / 📅
        // affordance lived only on the picker's "Recommended" card,
        // which meant auto-spawned agents had no path to be scheduled
        // without navigating to the picker first. The driver editor is
        // the natural landing page after Fermi spawns agents for a
        // driver, so the controls live here too.
        .when(!driver_agents.is_empty(), |el| {
            let driver_name_owned = name.to_string();
            // Convert &str borrows to owned Strings for the closures.
            let assigned_agents: Vec<String> =
                driver_agents.iter().map(|s| s.to_string()).collect();
            // Persisted schedules for this driver (so the operator can
            // tell at a glance which agents are already on a recurring
            // schedule vs which are just attached).
            let driver_schedules: std::collections::HashMap<String, ForecastSchedule> = state
                .schedules
                .iter()
                .filter(|s| s.driver_name == name && s.enabled)
                .map(|s| (s.agent_id.clone(), s.clone()))
                .collect();

            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .mt(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgb(theme::FG_FAINT))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::CYAN))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Scheduled research ({})", assigned_agents.len())),
                    )
                    .child({
                        let mut col = div().flex().flex_col().gap(px(6.0));
                        for bound_name in &assigned_agents {
                            // The AST has the BOUND name
                            // (`<base>_<driver>`); the schedule API and the
                            // registry are keyed on the BASE name. Resolve
                            // once here so closures + lookups use the
                            // right id.
                            let bound = bound_name.clone();
                            let base_id = base_agent_id_for_bound(&bound, &driver_name_owned);

                            let dn_run = driver_name_owned.clone();
                            let baid_run = base_id.clone();
                            let dn_daily = driver_name_owned.clone();
                            let baid_daily = base_id.clone();
                            let dn_weekly = driver_name_owned.clone();
                            let baid_weekly = base_id.clone();

                            // Existing persisted schedule keyed on BASE id.
                            let active = driver_schedules.get(&base_id);
                            let active_label = active.map(|s| {
                                if s.interval_hours >= 168 {
                                    "📅 Weekly".to_string()
                                } else if s.interval_hours >= 24 {
                                    "📅 Daily".to_string()
                                } else {
                                    format!("⏱ every {}h", s.interval_hours)
                                }
                            });

                            // Description from registry (BASE id lookup)
                            // — fall back to a generic label for agents
                            // that aren't in the registry.
                            let desc = state
                                .registry
                                .get(&base_id)
                                .ok()
                                .map(|c| c.metadata.description.clone())
                                .filter(|d| !d.is_empty())
                                .unwrap_or_else(|| "Research agent".into());

                            col = col.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.0))
                                    .px(px(10.0))
                                    .py(px(8.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(theme::BG_ELEVATED))
                                    .border_1()
                                    .border_color(rgb(theme::FG_FAINT))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .text_color(rgb(theme::CYAN))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    // Display the BASE id; the
                                                    // bound name carries a
                                                    // redundant driver suffix.
                                                    .child(base_id.clone()),
                                            )
                                            .when(active_label.is_some(), |el| {
                                                el.child(
                                                    div()
                                                        .text_size(px(9.0))
                                                        .text_color(rgb(theme::GREEN))
                                                        .px(px(6.0))
                                                        .py(px(1.0))
                                                        .rounded(px(3.0))
                                                        .bg(rgb(theme::BG))
                                                        .border_1()
                                                        .border_color(rgb(theme::GREEN))
                                                        .child(active_label.unwrap().clone()),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(theme::FG_DIM))
                                                    .min_w(px(0.0))
                                                    .child(desc),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .id(ElementId::Name(
                                                        format!("editor-run-{}-{}", name, base_id)
                                                            .into(),
                                                    ))
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(theme::CYAN))
                                                    .px(px(8.0))
                                                    .py(px(3.0))
                                                    .rounded(px(3.0))
                                                    .bg(rgb(theme::BG))
                                                    .border_1()
                                                    .border_color(rgb(theme::CYAN))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                                    .on_click(cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            this.update_schedule_for_assigned_agent(
                                                                &dn_run,
                                                                &baid_run,
                                                                Schedule::Once,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("▶ Run Now"),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::Name(
                                                        format!(
                                                            "editor-daily-{}-{}",
                                                            name, base_id
                                                        )
                                                        .into(),
                                                    ))
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(theme::GREEN))
                                                    .px(px(8.0))
                                                    .py(px(3.0))
                                                    .rounded(px(3.0))
                                                    .bg(rgb(theme::BG))
                                                    .border_1()
                                                    .border_color(rgb(theme::GREEN))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                                    .on_click(cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            this.update_schedule_for_assigned_agent(
                                                                &dn_daily,
                                                                &baid_daily,
                                                                Schedule::Every {
                                                                    interval: 1,
                                                                    unit:
                                                                        fermi::ast::TimeUnit::Day,
                                                                },
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("📅 Daily"),
                                            )
                                            .child(
                                                div()
                                                    .id(ElementId::Name(
                                                        format!(
                                                            "editor-weekly-{}-{}",
                                                            name, base_id
                                                        )
                                                        .into(),
                                                    ))
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(theme::GOLD))
                                                    .px(px(8.0))
                                                    .py(px(3.0))
                                                    .rounded(px(3.0))
                                                    .bg(rgb(theme::BG))
                                                    .border_1()
                                                    .border_color(rgb(theme::GOLD))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                                    .on_click(cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            this.update_schedule_for_assigned_agent(
                                                                &dn_weekly,
                                                                &baid_weekly,
                                                                Schedule::Every {
                                                                    interval: 1,
                                                                    unit:
                                                                        fermi::ast::TimeUnit::Week,
                                                                },
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                                    .child("📅 Weekly"),
                                            ),
                                    ),
                            );
                        }
                        col
                    }),
            )
        })
        // ── Evidence for this driver ──────────────────────────────
        .when(!driver_evidence.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .mt(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgb(theme::FG_FAINT))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::CYAN))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Evidence ({})", driver_evidence.len())),
                    )
                    .children(driver_evidence.iter().map(|ev| {
                        let is_collapsed = state.collapsed_evidence.contains(&ev.id);
                        let (quality_score, quality_label, quality_color) =
                            score_evidence_quality(ev);
                        let ev_id_toggle = ev.id.clone();
                        let summary_text = ev.summary.as_deref().unwrap_or("").to_string();
                        // Char-aware truncation — `&str[..117]` panics if
                        // byte 117 lands mid-codepoint. Agent output can
                        // contain Unicode (em-dashes, 'Türkiye' etc.).
                        let display_summary = if is_collapsed && summary_text.chars().count() > 120
                        {
                            format!("{}…", summary_text.chars().take(117).collect::<String>())
                        } else {
                            summary_text.clone()
                        };
                        let findings_limit = if is_collapsed { 2 } else { 10 };
                        let total_findings = ev.key_findings.len();

                        div()
                            .id(ElementId::Name(format!("ev-{}", ev.id).into()))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _event, _window, _cx| {
                                this.toggle_evidence_collapsed(&ev_id_toggle);
                            }))
                            // Header row: source + quality + relevance + expand indicator
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .flex_shrink_0()
                                            .child(ev.source.clone()),
                                    )
                                    // Quality indicator
                                    .child(
                                        div()
                                            .text_size(px(8.0))
                                            .text_color(rgb(quality_color))
                                            .px(px(4.0))
                                            .py(px(1.0))
                                            .rounded(px(2.0))
                                            .bg(rgb(theme::BG_ELEVATED))
                                            .flex_shrink_0()
                                            .child(format!(
                                                "{} {:.0}%",
                                                quality_label,
                                                quality_score * 100.0
                                            )),
                                    )
                                    .when(ev.relevance.is_some(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::CYAN))
                                                .flex_shrink_0()
                                                .child(format!(
                                                    "rel {:.0}%",
                                                    ev.relevance.unwrap_or(0.0) * 100.0
                                                )),
                                        )
                                    })
                                    // Expand/collapse indicator
                                    .child(
                                        div()
                                            .flex_grow()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child(if is_collapsed {
                                                "▸ expand"
                                            } else {
                                                "▾ collapse"
                                            }),
                                    ),
                            )
                            // Quality bar (thin colored strip)
                            .child(
                                div()
                                    .h(px(2.0))
                                    .w_full()
                                    .rounded(px(1.0))
                                    .bg(rgb(theme::BG_ELEVATED))
                                    .child(
                                        div()
                                            .h(px(2.0))
                                            .rounded(px(1.0))
                                            .bg(rgb(quality_color))
                                            .w(gpui::px((quality_score * 200.0).min(200.0) as f32)),
                                    ),
                            )
                            // Summary (truncated when collapsed)
                            .when(!display_summary.is_empty(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG))
                                        .mt(px(2.0))
                                        .child(display_summary),
                                )
                            })
                            // Key findings (limited when collapsed)
                            .when(!ev.key_findings.is_empty(), |el| {
                                el.children(ev.key_findings.iter().take(findings_limit).map(|f| {
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::FG_DIM))
                                        .child(format!("• {}", f))
                                }))
                                .when(
                                    is_collapsed && total_findings > findings_limit,
                                    |el2| {
                                        el2.child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::FG_FAINT))
                                                .child(format!(
                                                    "… {} more findings",
                                                    total_findings - findings_limit
                                                )),
                                        )
                                    },
                                )
                            })
                            // Date (shown when expanded)
                            .when(!is_collapsed && ev.date.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(8.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .mt(px(2.0))
                                        .child(format!("📅 {}", ev.date.as_deref().unwrap_or(""))),
                                )
                            })
                            // URL link (shown when expanded and URL exists)
                            .when(!is_collapsed && ev.url.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::BLUE))
                                        .mt(px(2.0))
                                        .child(format!("🔗 {}", ev.url.as_deref().unwrap_or(""))),
                                )
                            })
                    })),
            )
        })
        // (Pending suggestions used to render here; moved to a pinned
        // block at the top of the panel via render_pinned_suggestions —
        // see the call near the start of this function. Keeping this
        // comment so the next sweep knows there isn't a render gap.)
        // ── Add manual evidence ───────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .mt(px(8.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(rgb(theme::FG_FAINT))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Add Evidence"),
                )
                .child(state.evidence_source_input.clone())
                .child(state.evidence_summary_input.clone())
                .child({
                    div()
                        .id("add-evidence-btn")
                        .px(px(12.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::CYAN))
                        .text_color(rgb(theme::BG_DEEP))
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.8))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.add_manual_evidence(cx);
                        }))
                        .child("+ Add Evidence")
                }),
        )
}

fn render_editor_panel(
    state: &CockpitState,
    focused: &FocusedNode,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    match focused {
        FocusedNode::Driver(name) => {
            let driver = state.program.driver(name);
            let is_continuous = driver
                .map(|d| d.driver_type == DriverType::Continuous)
                .unwrap_or(true);

            // Get the driver's rationale for display
            let rationale_text = driver
                .and_then(|d| d.rationale.as_deref())
                .unwrap_or("")
                .to_string();

            div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .p(px(16.0))
                .border_b_1()
                .border_color(rgb(theme::FG_FAINT))
                // Header with close button
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(format!("Editing: {}", name)),
                        )
                        .child(
                            div()
                                .id("close-editor")
                                .text_size(px(12.0))
                                .text_color(rgb(theme::FG_DIM))
                                .px(px(8.0))
                                .py(px(2.0))
                                .rounded(px(4.0))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG)))
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.save_focused_driver(cx);
                                    this.focused_node = FocusedNode::Question;
                                    cx.notify();
                                }))
                                .child("✕ Close"),
                        ),
                )
                // Rationale display (from the driver definition)
                .when(!rationale_text.is_empty(), |el| {
                    el.child(
                        div()
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG_DIM))
                            .child(rationale_text),
                    )
                })
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(div().w(px(140.0)).child(state.editor_name.clone()))
                        .child(
                            div()
                                .flex_grow()
                                .min_w(px(0.0))
                                .child(state.editor_rationale.clone()),
                        ),
                )
                .when(is_continuous, |el| {
                    el.child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(div().w(px(90.0)).child(state.editor_p5.clone()))
                            .child(div().w(px(90.0)).child(state.editor_p50.clone()))
                            .child(div().w(px(90.0)).child(state.editor_p95.clone()))
                            .child(div().w(px(90.0)).child(state.editor_unit.clone())),
                    )
                })
                .when(!is_continuous, |el| {
                    el.child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(div().w(px(120.0)).child(state.editor_prob.clone()))
                            .child(div().w(px(120.0)).child(state.editor_impact.clone())),
                    )
                })
                // Confidence override
                .child({
                    let user_conf = state.driver_confidence.get(name).copied();
                    let ev_count = state
                        .program
                        .agents()
                        .iter()
                        .filter(|a| a.driver_refs.contains(&name.to_string()))
                        .flat_map(|a| {
                            state
                                .agent_runs
                                .iter()
                                .filter(move |r| r.agent_name == a.name)
                        })
                        .map(|r| r.evidence_count)
                        .sum::<usize>();
                    let computed_conf = if ev_count >= 3 {
                        0.8
                    } else if ev_count >= 1 {
                        0.5
                    } else {
                        0.2
                    };

                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().w(px(90.0)).child(state.editor_confidence.clone()))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "Computed: {:.0}%{}",
                                    computed_conf * 100.0,
                                    user_conf
                                        .map(|uc| format!(" → Override: {:.0}%", uc * 100.0))
                                        .unwrap_or_default()
                                )),
                        )
                })
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Values save when you close, switch drivers, or simulate (Ctrl+R)."),
                )
                .into_any_element()
        }
        _ => div()
            .p(px(16.0))
            .border_b_1()
            .border_color(rgb(theme::FG_FAINT))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child("Click a driver on the left to edit its parameters."),
            )
            .into_any_element(),
    }
}

// Note: render_editor_panel is kept but render_right_panel is the new entry point.
// render_editor_panel is used as fallback within render_right_panel.

fn render_assistant_panel(messages: &[AssistantMessage]) -> impl IntoElement {
    // Legacy — kept for compatibility but Fermi banner is now the primary display.
    div()
}

/// Fermi Banner — persistent top strip showing live agent activity.
/// Always visible, shows the most recent messages + latest finding.
/// Replaces the buried "FPL Assistant" panel.
/// Banner shown when the server has resolved/voided this forecast. Makes the
/// settled state — and the fact that re-sims / new snapshots are disabled —
/// explicit, so the operator doesn't have to carry that context. Includes a
/// ↻ Reconcile button to re-pull server state on demand.
fn render_locked_banner(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let reason = state
        .lock_reason()
        .unwrap_or_else(|| "Resolved".to_string());
    let outcome_color = match state.forecast_outcome {
        Some(true) => theme::GREEN,
        Some(false) => theme::RED,
        None => theme::GOLD,
    };
    let pct = format!("{:.1}%", state.predicted_probability * 100.0);
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(16.0))
        .py(px(8.0))
        .bg(rgb(theme::BG_ELEVATED))
        .border_b_1()
        .border_color(rgb(outcome_color))
        .child(
            div()
                .text_size(px(14.0))
                .text_color(rgb(outcome_color))
                .child("🔒"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(outcome_color))
                .child(reason),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child(format!(
                    "settled at {} · re-sims & new snapshots disabled",
                    pct
                )),
        )
        .child(div().flex_grow())
        .child(
            div()
                .id("cockpit-reconcile")
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG_ACTIVE))
                .text_size(px(11.0))
                .text_color(rgb(theme::CYAN))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                .on_click(cx.listener(|this, _, _w, cx| this.reconcile_forecast(cx)))
                .child(if state.reconciling {
                    "↻ Reconciling…"
                } else {
                    "↻ Reconcile"
                }),
        )
}

fn render_fermi_banner(
    messages: &[AssistantMessage],
    agent_runs: &[AgentExecution],
) -> impl IntoElement {
    let running_count = agent_runs
        .iter()
        .filter(|r| r.status == AgentRunStatus::Running)
        .count();
    let running_names: Vec<String> = agent_runs
        .iter()
        .filter(|r| r.status == AgentRunStatus::Running)
        .map(|r| base_agent_name(&r.agent_name).to_string())
        .collect();

    // Get the 2 most recent non-info messages (findings, suggestions, warnings)
    let recent: Vec<&AssistantMessage> = messages
        .iter()
        .rev()
        .filter(|m| m.kind != MessageKind::Info)
        .take(2)
        .collect();

    // Latest finding (from SSE stream or evidence)
    let latest_finding: Option<&AssistantMessage> =
        messages.iter().rev().find(|m| m.kind == MessageKind::Tip);

    // Status indicator
    let (status_icon, status_text, status_color) = if running_count > 0 {
        (
            "⟳",
            format!("Researching: {}", running_names.join(", ")),
            theme::GOLD,
        )
    } else if !messages.is_empty() {
        let last = messages.last().unwrap();
        let (icon, color) = match last.kind {
            MessageKind::Suggestion => ("💡", theme::CYAN),
            MessageKind::Warning => ("⚠", theme::GOLD),
            MessageKind::Info => ("✓", theme::FG_DIM),
            MessageKind::Error => ("✗", theme::RED),
            MessageKind::Tip => ("🦊", theme::GREEN),
        };
        (icon, last.text.chars().take(120).collect(), color)
    } else {
        (
            "🦊",
            "Ready — type a question and press Ctrl+Enter".to_string(),
            theme::FG_DIM,
        )
    };

    div()
        .id("fermi-banner")
        .w_full()
        .px(px(16.0))
        .py(px(6.0))
        .bg(rgb(0x171D2A))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        .flex()
        .flex_col()
        .gap(px(3.0))
        // Top line: Fermi label + status + running agents
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::CYAN))
                        .font_weight(FontWeight::BOLD)
                        .child("🦊 Fermi"),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(status_color))
                        .child(format!("{} {}", status_icon, status_text)),
                ),
        )
        // Bottom line: latest finding or recent suggestion
        .when(!recent.is_empty(), |el| {
            let msg = recent[0];
            let (icon, color) = match msg.kind {
                MessageKind::Suggestion => ("💡", theme::CYAN),
                MessageKind::Warning => ("⚠", theme::GOLD),
                MessageKind::Error => ("✗", theme::RED),
                MessageKind::Tip => ("🔍", theme::GREEN),
                _ => ("ℹ", theme::FG_DIM),
            };
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().text_size(px(9.0)).w(px(14.0)).child(icon.to_string()))
                    .child(
                        div()
                            .flex_grow()
                            .min_w(px(0.0))
                            .text_size(px(10.0))
                            .text_color(rgb(color))
                            .child(msg.text.chars().take(150).collect::<String>()),
                    ),
            )
        })
        // Second recent message if different from first
        .when(recent.len() > 1, |el| {
            let msg = recent[1];
            let (icon, color) = match msg.kind {
                MessageKind::Suggestion => ("💡", theme::CYAN),
                MessageKind::Warning => ("⚠", theme::GOLD),
                MessageKind::Error => ("✗", theme::RED),
                MessageKind::Tip => ("🔍", theme::GREEN),
                _ => ("ℹ", theme::FG_DIM),
            };
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().text_size(px(9.0)).w(px(14.0)).child(icon.to_string()))
                    .child(
                        div()
                            .flex_grow()
                            .min_w(px(0.0))
                            .text_size(px(9.0))
                            .text_color(rgb(color))
                            .child(msg.text.chars().take(120).collect::<String>()),
                    ),
            )
        })
}

/// Forecast Index — the key visualization section shown above drivers.
/// Displays inside/outside divergence, simulation stats, histogram,
/// index comparison chart, and evidence treemap.
fn render_forecast_index(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let has_base_rate = state
        .program
        .question()
        .and_then(|q| q.base_rate.as_ref())
        .is_some();
    let has_sim = state.sim_results.is_some();
    let has_drivers = !state.program.drivers().is_empty();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .mx(px(8.0))
        .my(px(4.0))
        // Only show if we have something to visualize
        .when(!has_base_rate && !has_sim && !has_drivers, |el| {
            el.child(
                div()
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child("Ctrl+Enter to research · Ctrl+R to simulate"),
            )
        })
        // ── Inside vs Outside divergence bar ──────────────────────
        .when(has_base_rate, |el| {
            let br = state
                .program
                .question()
                .unwrap()
                .base_rate
                .as_ref()
                .unwrap();
            let outside = br.historical_frequency;
            let inside = state.predicted_probability;
            let divergence = (inside - outside) * 100.0;
            let div_color = if divergence.abs() > 20.0 {
                theme::RED
            } else if divergence.abs() > 10.0 {
                theme::GOLD
            } else {
                theme::GREEN
            };

            // Visual bar showing both probabilities
            let bar_w = 400.0_f32;
            let outside_x = (outside as f32 * bar_w).clamp(0.0, bar_w);
            let inside_x = (inside as f32 * bar_w).clamp(0.0, bar_w);

            el.child(
                div()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(6.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    // Labels row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::GOLD))
                                            .child(format!("Out {:.1}%", outside * 100.0)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::CYAN))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!("In {:.1}%", inside * 100.0)),
                                    )
                                    .child(
                                        div().text_size(px(10.0)).text_color(rgb(div_color)).child(
                                            format!(
                                                "{}pp",
                                                if divergence > 0.0 {
                                                    format!("+{:.0}", divergence)
                                                } else {
                                                    format!("{:.0}", divergence)
                                                }
                                            ),
                                        ),
                                    ),
                            )
                            .when(state.forecast_confidence > 0.0, |el| {
                                let (label, color) = if state.forecast_confidence > 0.7 {
                                    ("High", theme::GREEN)
                                } else if state.forecast_confidence > 0.4 {
                                    ("Medium", theme::GOLD)
                                } else {
                                    ("Low", theme::RED)
                                };
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(color))
                                        .child(format!("{} confidence", label)),
                                )
                            }),
                    )
                    // Divergence bar
                    .child(
                        div()
                            .h(px(6.0))
                            .w(gpui::px(bar_w))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .relative()
                            // Outside view marker (gold)
                            .child(
                                div()
                                    .absolute()
                                    .top(px(0.0))
                                    .left(gpui::px(outside_x - 2.0))
                                    .w(px(4.0))
                                    .h(px(6.0))
                                    .rounded(px(1.0))
                                    .bg(rgb(theme::GOLD)),
                            )
                            // Inside view marker (cyan)
                            .child(
                                div()
                                    .absolute()
                                    .top(px(0.0))
                                    .left(gpui::px(inside_x - 2.0))
                                    .w(px(4.0))
                                    .h(px(6.0))
                                    .rounded(px(1.0))
                                    .bg(rgb(theme::CYAN)),
                            )
                            // Fill between the two markers
                            .child(
                                div()
                                    .absolute()
                                    .top(px(1.0))
                                    .left(gpui::px(outside_x.min(inside_x)))
                                    .w(gpui::px((outside_x - inside_x).abs()))
                                    .h(px(4.0))
                                    .bg(rgb(div_color)),
                            ),
                    ),
            )
        })
        // ── Simulation results (condensed) ────────────────────────
        .when(state.sim_running, |el| {
            el.child(
                div()
                    .px(px(12.0))
                    .py(px(4.0))
                    .text_size(px(10.0))
                    .text_color(rgb(theme::GOLD))
                    .child("⟳ Simulating…"),
            )
        })
        .when(state.sim_error.is_some(), |el| {
            el.child(
                div()
                    .px(px(12.0))
                    .py(px(4.0))
                    .text_size(px(10.0))
                    .text_color(rgb(theme::RED))
                    .child(format!("✗ {}", state.sim_error.as_deref().unwrap_or(""))),
            )
        })
        .when(has_sim, |el| {
            let sim = state.sim_results.as_ref().unwrap();
            el
                // Stats row (compact)
                .child(
                    div()
                        .px(px(12.0))
                        .flex()
                        .gap(px(10.0))
                        .text_size(px(10.0))
                        .child(render_stat("mean", sim.mean, theme::FG))
                        .child(render_stat("p5", sim.p5, theme::FG_DIM))
                        .child(render_stat("p50", sim.median, theme::CYAN))
                        .child(render_stat("p95", sim.p95, theme::FG_DIM))
                        .child(render_stat("σ", sim.std_dev, theme::FG_FAINT))
                        .child(
                            div()
                                .text_size(px(8.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "{}k·{}ms",
                                    sim.iterations / 1000,
                                    sim.execution_time_ms
                                )),
                        ),
                )
                // Histogram + Index chart SIDE BY SIDE — interactive,
                // matching the Wiki tab. Same renderers, smaller default
                // dimensions for the composer layout.
                .child(
                    div()
                        .px(px(12.0))
                        .flex()
                        .gap(px(8.0))
                        // Histogram (left)
                        .when(!sim.histogram.is_empty(), |el| {
                            let chart_w = 240.0_f32;
                            let chart_h = 70.0_f32;
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(8.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child("Distribution — hover bars"),
                                    )
                                    .child(render_interactive_histogram(
                                        state, cx, chart_w, chart_h,
                                    )),
                            )
                        })
                        // Index chart (right) — only if versions exist
                        .when(state.versions.len() > 0, |el| {
                            let chart_w = 240.0_f32;
                            let chart_h = 70.0_f32;
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(8.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child("Model · Base rate · Crowd — hover versions"),
                                    )
                                    .child(render_interactive_index_chart(
                                        state, cx, chart_w, chart_h,
                                    )),
                            )
                        }),
                )
        })
        // ── Driver Sensitivity Bar Chart (native GPUI) ─────────────
        // Horizontal bars sized by impact spread (p95−p5).
        // Evidence quality shown as a colored dot at bar end.
        // Click a bar to focus that driver.
        .when(has_drivers, |el| {
            // Compute driver impact data
            let drivers_data: Vec<(String, String, f64, usize)> = state
                .program
                .drivers()
                .iter()
                .map(|d| {
                    let display = d.display_name.as_deref().unwrap_or(&d.name).to_string();
                    let name = d.name.clone();
                    // Prefer the Sobol total-order index (true variance-based
                    // influence) from the last sim; fall back to p95−p5 spread
                    // when no sensitivity has been computed yet.
                    let impact = state
                        .driver_sensitivity
                        .get(&d.name)
                        .copied()
                        .map(|s| s.max(0.001))
                        .unwrap_or_else(|| match d.driver_type {
                            DriverType::Continuous => {
                                if let Some(Distribution::Triangular {
                                    ref p5, ref p95, ..
                                }) = d.distribution
                                {
                                    (expr_to_f64(p95) - expr_to_f64(p5)).abs().max(0.01)
                                } else {
                                    0.5
                                }
                            }
                            DriverType::Binary => {
                                d.probability.unwrap_or(0.5) * d.impact_multiplier.unwrap_or(1.0)
                            }
                            _ => 0.5,
                        });
                    let ev_count = state
                        .program
                        .evidence_items()
                        .iter()
                        .filter(|e| {
                            state.program.agents().iter()
                                .filter(|a| a.driver_refs.contains(&d.name))
                                .any(|a| evidence_matches_agent(e, &a.name))
                                || e.id.contains(&d.name)
                        })
                        .count();
                    (name, display, impact, ev_count)
                })
                .collect();

            let max_impact = drivers_data.iter().map(|(_, _, imp, _)| *imp).fold(0.01_f64, f64::max);

            if !drivers_data.is_empty() {
                el.child(
                    div()
                        .px(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(8.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(if state.driver_sensitivity.is_empty() {
                                    "Driver sensitivity (spread)  ·  evidence — run a sim for Sobol influence"
                                } else {
                                    "Driver influence (Sobol total-order)  ·  evidence"
                                }),
                        )
                        .children(drivers_data.iter().map(|(name, display, impact, ev_count)| {
                            let bar_frac = (impact / max_impact).clamp(0.05, 1.0);
                            let bar_width = (bar_frac * 200.0) as f32;
                            let ev_color = if *ev_count >= 3 {
                                theme::GREEN
                            } else if *ev_count >= 1 {
                                theme::GOLD
                            } else {
                                theme::FG_FAINT
                            };
                            let ev_label = if *ev_count > 0 {
                                format!("{}", ev_count)
                            } else {
                                "—".to_string()
                            };
                            let n = name.clone();
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .h(px(14.0))
                                .cursor_pointer()
                                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                    cx.stop_propagation();
                                })
                                // Driver label — fixed width, right-aligned
                                .child(
                                    div()
                                        .w(px(100.0))
                                        .text_size(px(8.0))
                                        .text_color(rgb(theme::FG_DIM))
                                        .overflow_hidden()
                                        .child(display.clone()),
                                )
                                // Bar — single color, width proportional to sensitivity
                                .child(
                                    div()
                                        .h(px(8.0))
                                        .w(px(bar_width))
                                        .rounded(px(2.0))
                                        .bg(rgba(0x5CCFE680)),
                                )
                                // Impact value
                                .child(
                                    div()
                                        .text_size(px(7.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(format!("{:.2}", impact)),
                                )
                                // Evidence dot
                                .child(
                                    div()
                                        .text_size(px(7.0))
                                        .text_color(rgb(ev_color))
                                        .child(ev_label),
                                )
                        })),
                )
            } else {
                el
            }
        })
        // Ctrl+R hint if no sim yet
        .when(
            !has_sim && !state.sim_running && state.sim_error.is_none() && has_drivers,
            |el| {
                el.child(
                    div()
                        .px(px(12.0))
                        .py(px(2.0))
                        .text_size(px(9.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Ctrl+R to simulate"),
                )
            },
        )
}

fn render_simulation_section(_state: &CockpitState) -> impl IntoElement {
    // Simulation results now rendered in render_forecast_index above drivers
    div()
}

/// Render the pinned "Suggested Adjustments" block that lives at the top
/// of the Edit panel — pulled out of render_driver_editor_and_evidence so
/// it stays above the fold instead of being buried under evidence.
///
/// Each suggestion card carries:
///   - the agent name + driver_name
///   - p50 transition (current → suggested) with arrow + percent change
///   - a 150-char excerpt of the agent's reasoning
///   - Accept / Reject buttons
///
/// Accept fires accept_suggestion which:
///   1. Updates workspace_outputs.params (Path A) or AST literals (Path B).
///   2. Refreshes the Edit panel inputs to show new values.
///   3. Auto-runs the simulation.
///
/// Returns an empty element when there are no pending suggestions for the
/// driver — keeps the layout stable when the section is empty.
fn render_pinned_suggestions(
    state: &CockpitState,
    name: &str,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    let driver_suggestions: Vec<&EvidenceSuggestion> = state
        .pending_suggestions
        .iter()
        .filter(|s| s.driver_name == name)
        .collect();

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .when(!driver_suggestions.is_empty(), |el| {
            el.mt(px(4.0))
                .pt(px(8.0))
                .pb(px(8.0))
                .px(px(10.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(rgb(theme::GOLD))
                .bg(rgb(0x1F1A0E))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::GOLD))
                        .font_weight(FontWeight::SEMIBOLD)
                        .mb(px(4.0))
                        .child(format!(
                            "💡 Suggested Adjustments ({}) — review to apply",
                            driver_suggestions.len()
                        )),
                )
                .children(driver_suggestions.iter().map(|sug| {
                    let accept_id = sug.id.clone();
                    let reject_id = sug.id.clone();
                    let delta_pct = (sug.suggested_p50 / sug.current_p50.max(0.001) - 1.0) * 100.0;
                    let (arrow, delta_color) = if delta_pct > 0.0 {
                        ("↑", theme::GREEN)
                    } else {
                        ("↓", theme::RED)
                    };

                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .mt(px(4.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x2A2518))
                        .border_1()
                        .border_color(rgb(theme::GOLD))
                        // Agent name + change summary
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!(
                                            "{} suggests:",
                                            base_agent_name(&sug.agent_name)
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(delta_color))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "p50 {:.2} → {:.2} ({}{:.0}%)",
                                            sug.current_p50,
                                            sug.suggested_p50,
                                            arrow,
                                            delta_pct.abs()
                                        )),
                                ),
                        )
                        // Reasoning excerpt
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .min_w(px(0.0))
                                .child(sug.reasoning.chars().take(150).collect::<String>()),
                        )
                        // Accept / Reject buttons
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .mt(px(4.0))
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("accept-pinned-{}", sug.id).into(),
                                        ))
                                        .px(px(14.0))
                                        .py(px(4.0))
                                        .rounded(px(4.0))
                                        .bg(rgb(theme::GREEN))
                                        .text_color(rgb(theme::BG_DEEP))
                                        .text_size(px(11.0))
                                        .font_weight(FontWeight::BOLD)
                                        .cursor_pointer()
                                        .hover(|s| s.opacity(0.8))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.accept_suggestion(&accept_id, cx);
                                        }))
                                        .child("✓ Apply"),
                                )
                                .child(
                                    div()
                                        .id(ElementId::Name(
                                            format!("reject-pinned-{}", sug.id).into(),
                                        ))
                                        .px(px(14.0))
                                        .py(px(4.0))
                                        .rounded(px(4.0))
                                        .bg(rgb(theme::BG_ELEVATED))
                                        .border_1()
                                        .border_color(rgb(theme::RED))
                                        .text_color(rgb(theme::RED))
                                        .text_size(px(11.0))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.reject_suggestion(&reject_id, cx);
                                        }))
                                        .child("✗ Reject"),
                                ),
                        )
                }))
        })
}

fn render_stat(label: &str, value: f64, color: u32) -> impl IntoElement {
    // Format precision adapts to magnitude. Probability stats are
    // typically in [0.001, 0.99], so {:.1} truncates everything below 5%
    // to "0.0". For non-probability forecasts (counts in the 100s,
    // magnitudes in the millions) the same precision is too tight.
    // Pick the format dynamically:
    //   |v| < 0.001  → "0.000" (don't show "0.0e0" garbage)
    //   |v| < 1.0    → 3 decimals (probabilities: "0.087")
    //   |v| < 100    → 2 decimals (mid-range: "12.34")
    //   else         → integer-style with thousands separators
    let av = value.abs();
    let formatted = if av < 0.001 && av > 0.0 {
        format!("{:.4}", value)
    } else if av < 1.0 {
        format!("{:.3}", value)
    } else if av < 100.0 {
        format!("{:.2}", value)
    } else {
        format!("{:.1}", value)
    };
    div()
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgb(color))
                .font_weight(FontWeight::BOLD)
                .child(formatted),
        )
}

fn render_histogram(bins: &[u32]) -> impl IntoElement {
    let chart_w = 400u32;
    let chart_h = 80u32;
    let rgb_buf = crate::charts::render_histogram_chart(bins, chart_w, chart_h);
    let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);

    gpui::img(gpui::ImageSource::Render(render_img))
        .w(gpui::px(chart_w as f32))
        .h(gpui::px(chart_h as f32))
}

fn render_status_bar(state: &CockpitState) -> impl IntoElement {
    let total_drivers = state.program.drivers().len();
    let total_evidence = state.program.evidence_items().len();
    let running_count = state
        .agent_runs
        .iter()
        .filter(|r| r.status == AgentRunStatus::Running)
        .count();
    let completed_count = state
        .agent_runs
        .iter()
        .filter(|r| r.status == AgentRunStatus::Completed)
        .count();
    let failed_count = state
        .agent_runs
        .iter()
        .filter(|r| r.status == AgentRunStatus::Failed)
        .count();

    // Drivers with no agents and no evidence = gaps
    let gap_count = state
        .program
        .drivers()
        .iter()
        .filter(|d| {
            let has_agent = state
                .program
                .agents()
                .iter()
                .any(|a| a.driver_refs.contains(&d.name));
            let has_ev = state
                .program
                .evidence_items()
                .iter()
                .any(|e| e.id.contains(&d.name));
            !has_agent && !has_ev
        })
        .count();

    div()
        .h(px(32.0))
        .px(px(16.0))
        .border_t_1()
        .border_color(rgb(theme::FG_FAINT))
        .flex()
        .items_center()
        .gap(px(12.0))
        .text_size(px(10.0))
        .text_color(rgb(theme::FG_FAINT))
        .child(format!("{} drivers", total_drivers))
        .child(
            div()
                .text_color(if total_evidence > 0 {
                    rgb(theme::GREEN)
                } else {
                    rgb(theme::FG_FAINT)
                })
                .child(format!("{} evidence", total_evidence)),
        )
        .when(running_count > 0, |el| {
            el.child(
                div()
                    .text_color(rgb(theme::GOLD))
                    .child(format!("⟳ {} running", running_count)),
            )
        })
        .when(completed_count > 0, |el| {
            el.child(
                div()
                    .text_color(rgb(theme::GREEN))
                    .child(format!("✓ {} done", completed_count)),
            )
        })
        .when(failed_count > 0, |el| {
            el.child(
                div()
                    .text_color(rgb(theme::RED))
                    .child(format!("✗ {} failed", failed_count)),
            )
        })
        .when(gap_count > 0, |el| {
            el.child(
                div()
                    .text_color(rgb(theme::RED))
                    .child(format!("⚠ {} gaps", gap_count)),
            )
        })
        .when(state.session_cost > 0.0, |el| {
            el.child(
                div()
                    .text_color(rgb(theme::FG_FAINT))
                    .child(format!("⚡{:.1} credits", state.session_cost)),
            )
        })
        .when(state.current_version > 0, |el| {
            el.child(format!("v{}", state.current_version))
        })
}

fn render_tab_bar(active: RightTab, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let tabs = [
        (RightTab::Edit, "Edit"),
        (RightTab::Fpl, "FPL"),
        (RightTab::Wiki, "Wiki"),
        (RightTab::Schedules, "Schedules"),
        (RightTab::Trajectory, "Trajectory"),
        (RightTab::Access, "Access"),
    ];

    div()
        .flex()
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        .children(tabs.iter().map(|(tab, label)| {
            let t = *tab;
            let is_active = t == active;
            div()
                .id(ElementId::Name(format!("tab-{}", label).into()))
                .px(px(16.0))
                .py(px(8.0))
                .text_size(px(12.0))
                .font_weight(if is_active {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if is_active {
                    rgb(theme::CYAN)
                } else {
                    rgb(theme::FG_DIM)
                })
                .border_b_2()
                .border_color(if is_active {
                    rgb(theme::CYAN)
                } else {
                    rgb(theme::BG_ELEVATED)
                })
                .cursor_pointer()
                .hover(|s| s.text_color(rgb(theme::FG)))
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.right_tab = t;
                    if t == RightTab::Fpl || t == RightTab::Wiki {
                        // Refresh the cached_fpl so the FPL/Wiki view shows
                        // recent driver edits — but only if the AST emitter
                        // can faithfully round-trip the loaded program.
                        // Workspace-backed forecasts (factor/agent/param/
                        // feeds_from blocks) MUST stay as the loaded source;
                        // see regenerate_cached_fpl_if_safe for the long-form
                        // rationale. Without this guard, tabbing to the FPL
                        // or Wiki view of a workspace-backed forecast would
                        // strip its agents+drivers down to a 2-driver shell
                        // and the next Ctrl+R would fail with "Undefined
                        // variable: dynamic_performance".
                        this.regenerate_cached_fpl_if_safe();
                    }
                    if t == RightTab::Trajectory {
                        this.load_timeline(cx);
                    }
                    if t == RightTab::Access {
                        // Lazy-load shares once per forecast_id.
                        if this.shares_loaded_for != this.forecast_id {
                            this.load_shares(cx);
                        }
                    }
                    cx.notify();
                }))
                .child(label.to_string())
        }))
}

/// Spec 24 §3.5.2 — the cockpit "Access" tab. Lists who can see/edit this
/// forecast and lets the owner add (by user_id or email) or revoke
/// collaborators at view/edit/admin granularity.
fn render_access_tab(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let container = div()
        .id("access-tab")
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(16.0))
        .overflow_y_scroll();

    // Gate on a published forecast — shares attach to a forecast row.
    if state.forecast_id.is_none() {
        return container.child(
            div()
                .p(px(8.0))
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child("Publish this forecast first (Ctrl+P) to share it with people or teams."),
        );
    }

    let perm = state.share_permission.clone();

    container
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(theme::CYAN))
                .child("🔗 Access"),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .child(
                    "Add by email (sends an invite if they have no account) or user id. \
                     Share with a whole team from the Teams panel.",
                ),
        )
        // Add-collaborator row
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(div().flex_grow().child(state.share_input.clone()))
                // Permission cycle chip
                .child(
                    div()
                        .id("share-perm")
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .text_size(px(11.0))
                        .text_color(rgb(theme::GOLD))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _, _w, cx| {
                            this.share_permission = match this.share_permission.as_str() {
                                "view" => "edit",
                                "edit" => "admin",
                                _ => "view",
                            }
                            .into();
                            cx.notify();
                        }))
                        .child(perm),
                )
                .child(
                    div()
                        .id("share-add")
                        .px(px(12.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::CYAN))
                        .text_size(px(11.0))
                        .text_color(rgb(theme::BG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.85))
                        .on_click(cx.listener(|this, _, _w, cx| {
                            this.add_share_from_input(cx);
                        }))
                        .child(if state.share_add_loading {
                            "Adding…"
                        } else {
                            "Add"
                        }),
                ),
        )
        // Error line
        .when(state.share_error.is_some(), |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::RED))
                    .child(state.share_error.clone().unwrap_or_default()),
            )
        })
        // Shares list header
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(theme::FG_DIM))
                .child(if state.shares_loading {
                    "Loading shares…".to_string()
                } else {
                    format!("Shared with ({})", state.shares.len())
                }),
        )
        .when(state.shares.is_empty() && !state.shares_loading, |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child("Private — not shared with anyone yet."),
            )
        })
        // Share rows
        .children(state.shares.iter().map(|s| {
            let sid = s.id.clone();
            let icon = if s.share_type == "team" {
                "👥"
            } else {
                "🧑"
            };
            // Prefer server-resolved display name; fall back to raw id.
            let primary_label = s
                .share_target_display_name
                .clone()
                .unwrap_or_else(|| s.share_target.clone());
            let show_subtitle = s.share_target_display_name.is_some();
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(7.0))
                .rounded(px(6.0))
                .bg(rgb(theme::BG_ELEVATED))
                .child(div().text_size(px(12.0)).child(icon))
                .child(
                    div()
                        .flex_grow()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .gap(px(1.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::FG))
                                .child(primary_label),
                        )
                        .when(show_subtitle, |el| {
                            el.child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child(s.share_target.clone()),
                            )
                        }),
                )
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .text_size(px(10.0))
                        .text_color(rgb(theme::GOLD))
                        .child(s.permission.clone()),
                )
                .child(
                    div()
                        .id(ElementId::Name(format!("revoke-{}", sid).into()))
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::RED)))
                        .on_click(cx.listener({
                            let sid = sid.clone();
                            move |this, _, _w, cx| {
                                this.revoke_share(sid.clone(), cx);
                            }
                        }))
                        .child("✕"),
                )
        }))
        // ── Share with a team ───────────────────────────────────
        //
        // Complements the per-user "Add by email" row above: click a
        // team pill and the forecast is shared with the entire team
        // via `object_shares` (share_type='team'). Uses the same
        // permission chip as the user share row — whichever role you've
        // cycled to is what the team gets on click.
        .child(render_team_share_section(state, cx))
        // ── Sent invites (pending + recent terminal) ──────────────────
        //
        // Distinct from shares: an invite is an INTENT to grant access,
        // which materialises as a share row on accept. Surfacing them
        // here means the operator can see "Alice — pending (view)"
        // right after clicking Add, without waiting for the invitee to
        // accept and the row to move from `forecast_invites` into
        // `shares`.
        .child(render_forecast_invites_section(state, cx))
}

fn render_team_share_section(
    state: &CockpitState,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    // Skip already-shared teams so the pill list is only for teams the
    // forecast isn't already shared with. Re-clicking a shared team is
    // still safe (server upsert) but the UI reads cleaner this way.
    let already_shared_team_ids: std::collections::HashSet<String> = state
        .shares
        .iter()
        .filter(|s| s.share_type == "team")
        .map(|s| s.share_target.clone())
        .collect();

    let mut container = div().flex().flex_col().gap(px(6.0)).mt(px(8.0)).child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme::FG_DIM))
                    .child(format!("Share with a team ({})", state.share_teams.len())),
            )
            .when(state.share_teams_loading, |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("loading…"),
                )
            }),
    );

    if state.share_teams.is_empty() && !state.share_teams_loading {
        container = container.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child(
                "No collaboration teams yet. Create one in the Teams panel to share with a group.",
            ),
        );
        return container;
    }

    // Team pills — wrap horizontally so many teams fit without
    // scrolling. Each pill is clickable; grays out while a share
    // request is in flight; disabled visual state when already shared.
    let pills = state.share_teams.iter().map(|t| {
        let tid = t.id.clone();
        let already_shared = already_shared_team_ids.contains(&t.id);
        let in_flight = state.share_team_in_flight.contains(&t.id);
        let interactive = !already_shared && !in_flight;
        let (label, color) = if already_shared {
            (format!("✓ {}", t.name), rgb(theme::GREEN))
        } else if in_flight {
            (format!("… {}", t.name), rgb(theme::FG_FAINT))
        } else {
            (t.name.clone(), rgb(theme::CYAN))
        };
        let pill = div()
            .id(ElementId::Name(format!("team-share-{}", tid).into()))
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(12.0))
            .bg(rgb(theme::BG_ELEVATED))
            .text_size(px(11.0))
            .text_color(color)
            .child(label);
        if interactive {
            pill.cursor_pointer()
                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                .on_click(cx.listener({
                    let tid = tid.clone();
                    move |this, _, _w, cx| {
                        this.share_with_team(tid.clone(), cx);
                    }
                }))
        } else {
            pill
        }
    });

    container.child(div().flex().flex_wrap().gap(px(6.0)).children(pills))
}

fn render_forecast_invites_section(
    state: &CockpitState,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    let pending: Vec<&Invite> = state
        .forecast_invites
        .iter()
        .filter(|i| i.status == "pending")
        .collect();
    let recent_terminal: Vec<&Invite> = state
        .forecast_invites
        .iter()
        .filter(|i| i.status != "pending")
        .take(3)
        .collect();

    let mut container = div().flex().flex_col().gap(px(6.0)).mt(px(8.0)).child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(theme::FG_DIM))
                    .child(format!("Sent invites ({} pending)", pending.len())),
            )
            .when(state.forecast_invites_loading, |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("loading…"),
                )
            }),
    );

    if pending.is_empty() && recent_terminal.is_empty() && !state.forecast_invites_loading {
        container = container.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("No outbound invitations. Add an email above to send one."),
        );
        return container;
    }

    for inv in pending.iter().chain(recent_terminal.iter()) {
        let iid = inv.id.clone();
        let is_pending = inv.status == "pending";
        let in_flight = state.forecast_invite_revoke_in_flight.contains(&inv.id);
        let recipient = inv
            .invitee_display_name
            .clone()
            .or_else(|| inv.invitee_email.clone())
            .or_else(|| inv.invitee_user_id.clone())
            .unwrap_or_else(|| "(unknown)".to_string());
        let status_color = match inv.status.as_str() {
            "pending" => rgb(theme::GOLD),
            "accepted" => rgb(theme::GREEN),
            "declined" => rgb(theme::RED),
            "revoked" => rgb(theme::FG_DIM),
            "expired" => rgb(theme::FG_FAINT),
            _ => rgb(theme::FG_DIM),
        };
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(6.0))
            .bg(rgb(theme::BG_ELEVATED))
            .child(div().text_size(px(12.0)).child("✉"))
            .child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG))
                            .child(recipient),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::FG_FAINT))
                            .child(format!("{} access", inv.permission)),
                    ),
            )
            .child(
                div()
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG_ACTIVE))
                    .text_size(px(10.0))
                    .text_color(status_color)
                    .child(inv.status.clone()),
            );
        if is_pending {
            row = row.child(
                div()
                    .id(ElementId::Name(format!("finv-revoke-{}", iid).into()))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_DIM))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::RED)))
                    .on_click(cx.listener({
                        let iid = iid.clone();
                        move |this, _, _w, cx| {
                            this.revoke_forecast_invite(iid.clone(), cx);
                        }
                    }))
                    .child(if in_flight { "…" } else { "Revoke" }),
            );
        }
        container = container.child(row);
    }
    container
}

fn render_schedules_tab(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let has_forecast_id = state.forecast_id.is_some();
    let schedules = state.schedules.clone();
    let now = chrono::Utc::now();

    let body = if !has_forecast_id {
        div()
            .p(px(20.0))
            .text_size(px(11.0))
            .text_color(rgb(theme::FG_DIM))
            .child(
                "Publish this forecast first (Ctrl+P) to enable scheduled auto-research.\n\
                 Once published, the 📅 Daily and 📅 Weekly buttons in each driver's research panel \
                 will persist schedules to the cloud.",
            )
            .into_any_element()
    } else if schedules.is_empty() {
        // Pre-populate from FPL-declared agent×driver pairs so the
        // operator can review + batch-save the schedule fan-out instead of
        // attaching cadences one driver at a time. WC team_prior fans out
        // to 6 driver×agent pairs (4 agents, 3 of which research one
        // driver each + football_analyst on three). Doing that by hand on
        // 48 workspaces costs serious clicks.
        let drafts = state.fpl_declared_schedule_drafts();
        if drafts.is_empty() {
            div()
                .p(px(20.0))
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child(
                    "No scheduled agents yet.\n\n\
                     Click a driver in the program tree, then 📅 Daily or 📅 Weekly to schedule \
                     recurring research. Overdue schedules auto-fire when this forecast is reopened.",
                )
                .into_any_element()
        } else {
            let n = drafts.len();
            let header = div()
                .px(px(14.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(rgb(theme::FG_FAINT))
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .flex_grow()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(theme::FG))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("{} schedule draft{} declared by FPL", n, if n == 1 { "" } else { "s" })),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .child(
                                    "Each row is an agent×driver pair the FPL ships with a recurring \
                                     cadence but isn't yet persisted on the server. Use 'Save all' to \
                                     batch-persist them, or save individually with the row buttons.",
                                ),
                        ),
                )
                .child(
                    div()
                        .id("save-all-schedules")
                        .px(px(12.0))
                        .py(px(5.0))
                        .rounded(px(5.0))
                        .border_1()
                        .border_color(rgb(theme::CYAN))
                        .text_size(px(11.0))
                        .text_color(rgb(theme::CYAN))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.save_all_schedule_drafts(cx);
                        }))
                        .child(format!("→ Save all {}", n)),
                );

            let rows: Vec<AnyElement> = drafts
                .into_iter()
                .map(|draft| {
                    let label_interval = if draft.interval_hours >= 168 {
                        format!("every {} week", draft.interval_hours / 168)
                    } else if draft.interval_hours >= 24 {
                        format!("every {} day", draft.interval_hours / 24)
                    } else {
                        format!("every {}h", draft.interval_hours)
                    };
                    let row_id = format!(
                        "save-draft-{}-{}",
                        sanitize_name(&draft.agent_id),
                        sanitize_name(&draft.driver_name)
                    );
                    let draft_for_save = draft.clone();
                    div()
                        .px(px(14.0))
                        .py(px(8.0))
                        .border_b_1()
                        .border_color(rgb(theme::FG_FAINT))
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .child(
                            div()
                                .flex_grow()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme::CYAN))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(draft.agent_id.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::FG_DIM))
                                                .child("→"),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(rgb(theme::FG))
                                                .child(draft.driver_name.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::FG_DIM))
                                                .child(label_interval),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_DIM))
                                        .child({
                                            let q = &draft.query;
                                            if q.chars().count() > 110 {
                                                format!(
                                                    "{}…",
                                                    q.chars().take(108).collect::<String>()
                                                )
                                            } else {
                                                q.clone()
                                            }
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .id(SharedString::from(row_id))
                                .px(px(10.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(rgb(theme::GREEN))
                                .text_size(px(10.0))
                                .text_color(rgb(theme::GREEN))
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.save_schedule_draft(draft_for_save.clone(), cx);
                                }))
                                .child("Save"),
                        )
                        .into_any_element()
                })
                .collect();

            div()
                .flex()
                .flex_col()
                .child(header)
                .children(rows)
                .into_any_element()
        }
    } else {
        // Group schedules by driver for clearer display
        let mut by_driver: std::collections::BTreeMap<String, Vec<ForecastSchedule>> =
            std::collections::BTreeMap::new();
        for s in &schedules {
            by_driver
                .entry(s.driver_name.clone())
                .or_default()
                .push(s.clone());
        }

        let groups: Vec<AnyElement> = by_driver
            .into_iter()
            .map(|(driver, scheds)| {
                let rows: Vec<AnyElement> = scheds
                    .into_iter()
                    .map(|sched| {
                        let sid_run = sched.id.clone();
                        let sid_del = sched.id.clone();
                        let label = if sched.interval_hours >= 168 {
                            format!("every {} week", sched.interval_hours / 168)
                        } else if sched.interval_hours >= 24 {
                            format!("every {} day", sched.interval_hours / 24)
                        } else {
                            format!("every {}h", sched.interval_hours)
                        };
                        let next_str = sched
                            .next_run_at
                            .get(..16)
                            .unwrap_or(&sched.next_run_at)
                            .replace('T', " ");
                        let last_str = sched
                            .last_run_at
                            .as_ref()
                            .map(|t| t.get(..16).unwrap_or(t).replace('T', " "))
                            .unwrap_or_else(|| "never".into());
                        let is_overdue = chrono::DateTime::parse_from_rfc3339(&sched.next_run_at)
                            .map(|t| t.with_timezone(&chrono::Utc) <= now)
                            .unwrap_or(false);
                        let next_color = if is_overdue {
                            rgb(theme::GOLD)
                        } else {
                            rgb(theme::FG_DIM)
                        };

                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .px(px(10.0))
                            .py(px(8.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG_ACTIVE))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::CYAN))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(sched.agent_id.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_DIM))
                                            .child(label),
                                    )
                                    .child(div().flex_grow())
                                    // Cadence buttons — On-demand / Daily /
                                    // Weekly / Monthly. Active cadence
                                    // highlighted in gold; clicking a
                                    // different value calls
                                    // change_schedule_interval, which re-
                                    // upserts with the new interval_hours.
                                    //
                                    // interval_hours = 0 is the on-demand
                                    // mode: schedule is saved (so Run Now
                                    // works) but auto-fire is disabled —
                                    // server sets next_run_at to a far-
                                    // future sentinel so the overdue check
                                    // never matches.
                                    .child({
                                        let sid_o = sched.id.clone();
                                        let is_active = sched.interval_hours == 0;
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-o-{}", sid_o).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(if is_active {
                                                rgb(theme::GOLD)
                                            } else {
                                                rgb(theme::FG_DIM)
                                            })
                                            .px(px(6.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.change_schedule_interval(&sid_o, 0, cx);
                                                },
                                            ))
                                            .child("On-demand")
                                    })
                                    .child({
                                        let sid_d = sched.id.clone();
                                        let is_active = sched.interval_hours == 24;
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-d-{}", sid_d).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(if is_active {
                                                rgb(theme::GOLD)
                                            } else {
                                                rgb(theme::FG_DIM)
                                            })
                                            .px(px(6.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.change_schedule_interval(&sid_d, 24, cx);
                                                },
                                            ))
                                            .child("Daily")
                                    })
                                    .child({
                                        let sid_w = sched.id.clone();
                                        let is_active = sched.interval_hours == 168;
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-w-{}", sid_w).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(if is_active {
                                                rgb(theme::GOLD)
                                            } else {
                                                rgb(theme::FG_DIM)
                                            })
                                            .px(px(6.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.change_schedule_interval(&sid_w, 168, cx);
                                                },
                                            ))
                                            .child("Weekly")
                                    })
                                    .child({
                                        let sid_m = sched.id.clone();
                                        let is_active = sched.interval_hours == 720;
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-m-{}", sid_m).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(if is_active {
                                                rgb(theme::GOLD)
                                            } else {
                                                rgb(theme::FG_DIM)
                                            })
                                            .px(px(6.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.change_schedule_interval(&sid_m, 720, cx);
                                                },
                                            ))
                                            .child("Monthly")
                                    })
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-run-{}", sid_run).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::CYAN))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .border_1()
                                            .border_color(rgb(theme::CYAN))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.run_now_schedule(&sid_run, cx);
                                                },
                                            ))
                                            .child("▶ Run Now"),
                                    )
                                    .child(
                                        div()
                                            .id(ElementId::Name(
                                                format!("schedules-tab-del-{}", sid_del).into(),
                                            ))
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .px(px(8.0))
                                            .py(px(3.0))
                                            .rounded(px(3.0))
                                            .bg(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(rgb(theme::RED)))
                                            .on_click(cx.listener(
                                                move |this, _event, _window, cx| {
                                                    this.delete_schedule(&sid_del, cx);
                                                },
                                            ))
                                            .child("× Delete"),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(12.0))
                                    .text_size(px(9.0))
                                    .child(
                                        div()
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child(format!("last: {}", last_str)),
                                    )
                                    .child(div().text_color(next_color).child(format!(
                                        "next: {}{}",
                                        next_str,
                                        if is_overdue { " (overdue)" } else { "" }
                                    ))),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child(
                                        sched.query.chars().take(120).collect::<String>()
                                            + if sched.query.len() > 120 { "…" } else { "" },
                                    ),
                            )
                            .into_any_element()
                    })
                    .collect();

                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Driver: {}", driver)),
                    )
                    .children(rows)
                    .into_any_element()
            })
            .collect();

        div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(14.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG_DIM))
                            .child(format!(
                                "{} active schedule{} • auto-fires on cockpit open when overdue",
                                schedules.len(),
                                if schedules.len() == 1 { "" } else { "s" }
                            )),
                    )
                    .child(
                        div()
                            .id("schedules-tab-refresh")
                            .text_size(px(10.0))
                            .text_color(rgb(theme::CYAN))
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(3.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(rgb(theme::FG_FAINT))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.load_schedules(cx);
                            }))
                            .child("⟳ Refresh"),
                    ),
            )
            .children(groups)
            .into_any_element()
    };

    div().flex().flex_col().size_full().child(body)
}

// ═══════════════════════════════════════════════════════════════════
// R-3 Trajectory tab
// ═══════════════════════════════════════════════════════════════════
//
// Renders the forecast's spacetime as the spec describes (§5): rate trace
// over time + a chronological event list correlating to it. The event
// list is what makes "what made the rate move?" answerable for every
// kind of evidence — BayesOps fits, agent runs, upstream resolutions,
// market polls, system events.
//
// This is the MVP shape: rate summary header, then a vertical event
// list with each event as a coloured card. A future polish pass can
// add the line-chart overlay (the data is already in `rate_series` /
// `market_series` arrays on the response). The event list alone is the
// load-bearing affordance — the user can trace causation by reading
// rows.

fn render_trajectory_tab(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let body = if state.timeline_loading {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(theme::FG_DIM))
            .child("Loading trajectory…")
            .into_any_element()
    } else if let Some(err) = &state.timeline_error {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(theme::RED))
            .child(format!("Failed to load trajectory: {}", err))
            .into_any_element()
    } else if state.timeline_data.is_some() {
        render_trajectory_body(state, cx).into_any_element()
    } else {
        div()
            .flex()
            .items_center()
            .justify_center()
            .size_full()
            .text_color(rgb(theme::FG_DIM))
            .child("Click Trajectory to load this forecast's event history.")
            .into_any_element()
    };

    div().flex().flex_col().size_full().child(body)
}

fn render_trajectory_body(
    state: &CockpitState,
    cx: &mut Context<CockpitState>,
) -> impl IntoElement {
    let data = state.timeline_data.as_ref().unwrap(); // checked by caller

    // Span summary
    let span = data.get("span").cloned().unwrap_or(JsonValue::Null);
    let event_count = span
        .get("event_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let rate_count = span
        .get("rate_revision_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let market_count = span
        .get("market_observation_count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // Rate series for the worm
    let rate_series = data
        .get("rate_series")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let first_rate = rate_series
        .first()
        .and_then(|v| v.get("rate"))
        .and_then(|v| v.as_f64());
    let last_rate = rate_series
        .last()
        .and_then(|v| v.get("rate"))
        .and_then(|v| v.as_f64());
    let net_change_pp = match (first_rate, last_rate) {
        (Some(f), Some(l)) => Some((l - f) * 100.0),
        _ => None,
    };

    // ── Build the worm: convert RFC3339 timestamps to seconds-since-first
    let parse_ts = |s: &str| -> Option<chrono::DateTime<chrono::Utc>> {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&chrono::Utc))
    };

    let events_arr = data
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut all_ts: Vec<chrono::DateTime<chrono::Utc>> = rate_series
        .iter()
        .filter_map(|p| p.get("ts").and_then(|v| v.as_str()).and_then(parse_ts))
        .collect();
    all_ts.extend(
        events_arr
            .iter()
            .filter_map(|e| e.get("ts").and_then(|v| v.as_str()).and_then(parse_ts)),
    );
    let earliest = all_ts.iter().min().cloned();

    let worm_points: Vec<crate::charts::TrajectoryPoint> = rate_series
        .iter()
        .filter_map(|p| {
            let ts = parse_ts(p.get("ts")?.as_str()?)?;
            let rate = p.get("rate")?.as_f64()?;
            let t_secs = (ts - earliest?).num_milliseconds() as f64 / 1000.0;
            Some(crate::charts::TrajectoryPoint {
                t_seconds: t_secs,
                rate_pct: rate * 100.0,
            })
        })
        .collect();

    let rate_at = |ts: chrono::DateTime<chrono::Utc>| -> f64 {
        let target = (ts - earliest.unwrap_or(ts)).num_milliseconds() as f64 / 1000.0;
        worm_points
            .iter()
            .min_by(|a, b| {
                (a.t_seconds - target)
                    .abs()
                    .partial_cmp(&(b.t_seconds - target).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|p| p.rate_pct)
            .unwrap_or(2.08)
    };

    let worm_events: Vec<crate::charts::TrajectoryEvent> = events_arr
        .iter()
        .filter_map(|ev| {
            let ts = parse_ts(ev.get("ts")?.as_str()?)?;
            let kind_str = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let kind = match kind_str {
                "rate_revision" => crate::charts::TrajectoryEventKind::RateRevision,
                "bayesops_fit" => crate::charts::TrajectoryEventKind::BayesOpsFit,
                "agent_run" => crate::charts::TrajectoryEventKind::AgentRun,
                "market_observation" => crate::charts::TrajectoryEventKind::MarketObservation,
                _ => crate::charts::TrajectoryEventKind::AgentRun,
            };
            let rate_pct = ev
                .get("predicted_probability")
                .and_then(|v| v.as_f64())
                .map(|p| p * 100.0)
                .unwrap_or_else(|| rate_at(ts));
            let t_secs = (ts - earliest?).num_milliseconds() as f64 / 1000.0;
            Some(crate::charts::TrajectoryEvent {
                t_seconds: t_secs,
                rate_pct,
                kind,
            })
        })
        .collect();

    // Pull base rate from the program's question, crowd price from the
    // cockpit's PM polling. Both are passed to the chart so it can draw
    // the gold + purple horizontals that frame the worm visually.
    let base_rate_pct = state
        .program
        .question()
        .and_then(|q| q.base_rate.as_ref())
        .map(|br| br.historical_frequency * 100.0);
    let crowd_price_pct = state.pm_market_price.map(|p| p * 100.0);

    // ── Crowd worm: parse `market_series` from the timeline response.
    //
    // Each entry is `{ ts, market_price }` on the same wall-clock axis
    // as the model's rate series. Convert to the chart's chart
    // (seconds-since-earliest, percent) coordinate space — same origin
    // as `worm_points` so the two lines share a time axis exactly.
    //
    // We also append the operator's LIVE `pm_market_price` (from the
    // 5-min PM poll) as a final synthetic point at "now" so the worm
    // extends up to the current moment even if the last recorded
    // observation is stale. Without this, a forecast whose last
    // snapshot was 20 min ago shows the crowd line ending well before
    // the model line — misleading, since the crowd DOES have a
    // current value.
    let market_series_json = data
        .get("market_series")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut crowd_points: Vec<crate::charts::TrajectoryPoint> = market_series_json
        .iter()
        .filter_map(|p| {
            let ts = parse_ts(p.get("ts")?.as_str()?)?;
            let price = p.get("market_price")?.as_f64()?;
            let t_secs = (ts - earliest?).num_milliseconds() as f64 / 1000.0;
            Some(crate::charts::TrajectoryPoint {
                t_seconds: t_secs,
                rate_pct: price * 100.0,
            })
        })
        .collect();
    // Append "now" as a synthetic tip so the crowd worm always reaches
    // the right edge of the chart. Skip when the live price agrees with
    // the last recorded observation (avoids a duplicate point).
    if let (Some(price), Some(base)) = (state.pm_market_price, earliest) {
        let now = chrono::Utc::now();
        let t_secs = (now - base).num_milliseconds() as f64 / 1000.0;
        let live_pct = price * 100.0;
        let is_dupe = crowd_points
            .last()
            .map(|p| (p.rate_pct - live_pct).abs() < 0.05 && (t_secs - p.t_seconds).abs() < 30.0)
            .unwrap_or(false);
        if !is_dupe {
            crowd_points.push(crate::charts::TrajectoryPoint {
                t_seconds: t_secs,
                rate_pct: live_pct,
            });
        }
    }

    let chart_w: u32 = 800;
    let chart_h: u32 = 240;
    let worm_buf = crate::charts::render_trajectory_worm(
        &worm_points,
        &crowd_points,
        &worm_events,
        base_rate_pct,
        crowd_price_pct,
        chart_w,
        chart_h,
    );
    let worm_img = crate::charts::rgb_to_render_image(&worm_buf, chart_w, chart_h);

    // Compute pixel positions of every event so we can put hover divs
    // over them. Same coordinate-space math the chart used internally.
    let event_pixels = crate::charts::trajectory_event_pixel_positions(
        &worm_events,
        &worm_points,
        &crowd_points,
        base_rate_pct,
        crowd_price_pct,
        chart_w,
        chart_h,
    );

    let header = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(16.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        .child(
            div()
                .text_size(px(14.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(theme::FG))
                .child("Trajectory"),
        )
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child(format!("{} events", event_count))
                .child(format!("{} rate revisions", rate_count))
                .child(format!("{} market observations", market_count))
                .child(match (first_rate, last_rate, net_change_pp) {
                    (Some(f), Some(l), Some(d)) => {
                        format!("model {:.1}% → {:.1}% ({:+.1}pp)", f * 100.0, l * 100.0, d)
                    }
                    _ => "no rate revisions yet".into(),
                })
                // Crowd trajectory delta — shows first → last crowd price
                // across the same window as the model row above. If the
                // model moved +8pp but the crowd moved +9pp, they're
                // walking together; if the model moved +8pp and the
                // crowd moved −2pp, we're the ones taking a position.
                .when(!crowd_points.is_empty(), {
                    let first = crowd_points.first().map(|p| p.rate_pct);
                    let last = crowd_points.last().map(|p| p.rate_pct);
                    move |el| match (first, last) {
                        (Some(f), Some(l)) => {
                            el.child(format!("crowd {:.1}% → {:.1}% ({:+.1}pp)", f, l, l - f))
                        }
                        _ => el,
                    }
                })
                .when(crowd_price_pct.is_some(), move |el| {
                    // Live divergence: latest model rate vs latest crowd price.
                    let div_pp = match (last_rate, crowd_price_pct) {
                        (Some(l), Some(c)) => Some(l * 100.0 - c),
                        _ => None,
                    };
                    el.child(format!("vs crowd: {:+.1}pp", div_pp.unwrap_or(0.0)))
                }),
        );

    // ── Worm chart slot with interactive hover overlays ─────────────
    //
    // The chart bitmap is rendered at fixed (chart_w, chart_h). We layer
    // invisible 16×16 hover divs on top, one per event, at the pixel
    // coordinates the chart placed each dot. Hovering a div toggles
    // state.hovered_trajectory_event so:
    //   • the matching row in the event list highlights
    //   • a tooltip card surfaces the event details
    //
    // The chart canvas is wrapped in an absolute-positioned container
    // so the hover divs can position themselves with `left`/`top` in
    // canvas-local coordinates.
    let mut chart_overlay = div()
        .relative()
        .w(px(chart_w as f32))
        .h(px(chart_h as f32))
        .child(
            gpui::img(worm_img)
                .w(px(chart_w as f32))
                .h(px(chart_h as f32)),
        );

    for (i, (x_pix, y_pix)) in event_pixels.iter().enumerate() {
        let hit_size = 16.0_f32;
        let left = (*x_pix as f32) - hit_size / 2.0;
        let top = (*y_pix as f32) - hit_size / 2.0;
        let idx = i;
        chart_overlay = chart_overlay.child(
            div()
                .id(ElementId::Name(format!("traj-hit-{}", i).into()))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(hit_size))
                .h(px(hit_size))
                .cursor_pointer()
                .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                    if *hovered {
                        if this.hovered_trajectory_event != Some(idx) {
                            this.hovered_trajectory_event = Some(idx);
                            cx.notify();
                        }
                    } else if this.hovered_trajectory_event == Some(idx) {
                        this.hovered_trajectory_event = None;
                        cx.notify();
                    }
                })),
        );
    }

    let worm = div()
        .px(px(16.0))
        .py(px(8.0))
        .flex()
        .justify_center()
        .child(chart_overlay);

    // Legend
    let legend = div()
        .px(px(16.0))
        .pb(px(8.0))
        .flex()
        .gap(px(14.0))
        .text_size(px(10.0))
        .text_color(rgb(theme::FG_DIM))
        .child(legend_chip("●", theme::CYAN, "Apply / rate revision"))
        .child(legend_chip("●", theme::GOLD, "BayesOps fit"))
        .child(legend_chip("●", theme::FG_DIM, "Agent run"))
        .child(legend_chip("●", theme::PURPLE, "Polymarket tick"))
        .child(legend_chip("─", theme::GOLD, "Base"))
        .when(crowd_price_pct.is_some(), |el| {
            el.child(legend_chip("─", theme::PURPLE, "Crowd"))
        });

    // Event list — each row carries its index so we can highlight the
    // one matching state.hovered_trajectory_event.
    let hovered = state.hovered_trajectory_event;
    let event_list = if events_arr.is_empty() {
        div()
            .flex()
            .items_center()
            .justify_center()
            .flex_grow()
            .text_color(rgb(theme::FG_DIM))
            .child("No events yet — resolve an upstream workspace or run an agent to start the trajectory.")
            .into_any_element()
    } else {
        div()
            .id("trajectory-event-list")
            .flex()
            .flex_col()
            .flex_grow()
            .overflow_y_scroll()
            .px(px(16.0))
            .py(px(8.0))
            .gap(px(6.0))
            .children(
                events_arr
                    .iter()
                    .enumerate()
                    .map(|(i, ev)| render_trajectory_event_with_hover(ev, i, hovered)),
            )
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header)
        .child(worm)
        .child(legend)
        .child(event_list)
}

/// Small color-swatch + label pair for the trajectory legend.
fn legend_chip(glyph: &str, color: u32, label: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(div().text_color(rgb(color)).child(glyph.to_string()))
        .child(label.to_string())
}

/// Wrap render_trajectory_event with a highlight ring when the chart's
/// hovered_trajectory_event index matches this row. The ring lifts the
/// matching card out of the wall-of-text feel and creates the eye-trace
/// connection between worm dot and bullet entry.
fn render_trajectory_event_with_hover(
    ev: &JsonValue,
    idx: usize,
    hovered: Option<usize>,
) -> AnyElement {
    let base = render_trajectory_event(ev);
    let is_highlighted = hovered == Some(idx);
    if !is_highlighted {
        return base;
    }
    // Wrap in an outer div with a soft glow + heavier border. The inner
    // base card keeps its kind-specific left-border color; we add a
    // gold outer ring to scream "this is the one".
    div()
        .border_2()
        .border_color(rgb(theme::GOLD))
        .rounded(px(6.0))
        .child(base)
        .into_any_element()
}

fn render_trajectory_event(ev: &JsonValue) -> AnyElement {
    let kind = ev.get("kind").and_then(|v| v.as_str()).unwrap_or("event");
    let ts = ev.get("ts").and_then(|v| v.as_str()).unwrap_or("");

    // Per-kind colour + glyph + summary text.
    let (color, glyph, headline, detail) = match kind {
        "rate_revision" => {
            let prob = ev
                .get("predicted_probability")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let prev = ev.get("previous_probability").and_then(|v| v.as_f64());
            let trigger = ev
                .get("revision_trigger")
                .and_then(|v| v.as_str())
                .unwrap_or("update");
            let headline = match prev {
                Some(p) => format!(
                    "Rate {:.1}% → {:.1}% ({:+.1}pp) via {}",
                    p * 100.0,
                    prob * 100.0,
                    (prob - p) * 100.0,
                    trigger
                ),
                None => format!("Rate initialised at {:.1}%", prob * 100.0),
            };
            let detail = ev
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (theme::CYAN, "●", headline, detail)
        }
        "bayesops_fit" => {
            let driver = ev
                .get("driver_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let decision = ev
                .get("decision")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let n = ev
                .get("n_observations")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let delta_pp = ev.get("delta_pp").and_then(|v| v.as_f64());
            let headline = match delta_pp {
                Some(d) => format!(
                    "BayesOps fit · {} · {} (n={}, Δ{:+.1}pp)",
                    driver, decision, n, d
                ),
                None => format!("BayesOps fit · {} · {} (n={})", driver, decision, n),
            };
            let (color, glyph) = match decision {
                "auto_accepted" => (theme::GREEN, "✓"),
                "staged" => (theme::GOLD, "↻"),
                "hard_blocked" => (theme::RED, "⚠"),
                _ => (theme::FG_DIM, "?"),
            };
            // Surface the statistical strength of the fit — n_eff and the
            // CI width are carried in the event but were never shown, so a
            // "staged" vs "auto_accepted" decision had no visible rationale.
            let n_eff = ev.get("n_eff").and_then(|v| v.as_f64());
            let ci_width = ev.get("ci_width").and_then(|v| v.as_f64());
            let detail = match (n_eff, ci_width) {
                (Some(ne), Some(cw)) => {
                    format!("effective n={:.1} · 90% CI ±{:.3}", ne, cw / 2.0)
                }
                (Some(ne), None) => format!("effective n={:.1}", ne),
                (None, Some(cw)) => format!("90% CI ±{:.3}", cw / 2.0),
                _ => String::new(),
            };
            (color, glyph, headline, detail)
        }
        "agent_run" => {
            let sender = ev
                .get("sender_name")
                .and_then(|v| v.as_str())
                .or_else(|| ev.get("sender_id").and_then(|v| v.as_str()))
                .unwrap_or("agent");
            let confidence = ev
                .get("metadata")
                .and_then(|m| m.get("confidence"))
                .and_then(|v| v.as_f64());
            let headline = match confidence {
                Some(c) => format!("Agent run · {} (confidence {:.0}%)", sender, c * 100.0),
                None => format!("Agent run · {}", sender),
            };
            let detail = ev
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(180)
                .collect::<String>();
            (theme::PURPLE, "✎", headline, detail)
        }
        "upstream_resolved" => {
            let outcome = ev
                .get("metadata")
                .and_then(|m| m.get("outcome"))
                .map(|v| v.to_string())
                .unwrap_or_default();
            let headline = "Upstream workspace resolved".to_string();
            (theme::GOLD, "⇪", headline, outcome)
        }
        "bayesops_fit_accepted"
        | "bayesops_fit_pending"
        | "bayesops_fit_failed"
        | "bayesops_fit_decision" => {
            let driver = ev
                .get("metadata")
                .and_then(|m| m.get("driver_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let headline = format!("BayesOps event · {} · {}", kind, driver);
            let detail = ev
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (theme::CYAN, "📊", headline, detail)
        }
        _ => {
            let headline = ev
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or(kind)
                .chars()
                .take(120)
                .collect::<String>();
            (
                theme::FG_DIM,
                "·",
                format!("{} · {}", kind, headline),
                String::new(),
            )
        }
    };

    let ts_short = ts
        .split('T')
        .collect::<Vec<_>>()
        .get(0..2)
        .map(|p| p.join(" "))
        .unwrap_or_else(|| ts.to_string());

    let mut card = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(10.0))
        .py(px(6.0))
        .rounded(px(4.0))
        .border_l_2()
        .border_color(rgb(color))
        .bg(rgb(theme::BG_ELEVATED))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(color))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{} {}", glyph, headline)),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::FG_DIM))
                        .child(ts_short),
                ),
        );

    if !detail.is_empty() {
        card = card.child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .child(detail),
        );
    }

    card.into_any_element()
}

fn render_fpl_tab(state: &CockpitState) -> impl IntoElement {
    let fpl = if state.cached_fpl.is_empty() {
        generate_fpl_text(&state.program)
    } else {
        state.cached_fpl.clone()
    };

    div().p(px(12.0)).flex().flex_col().gap(px(4.0)).child(
        div()
            .text_size(px(11.0))
            .text_color(rgb(theme::FG_DIM))
            .font_family("Ubuntu Mono, DejaVu Sans Mono, monospace")
            .min_w(px(0.0))
            .child(if fpl.is_empty() {
                "# Empty program".to_string()
            } else {
                fpl
            }),
    )
}

fn render_wiki_tab(state: &CockpitState, cx: &mut Context<CockpitState>) -> impl IntoElement {
    let drivers = state.program.drivers();
    let evidence = state.program.evidence_items();
    let agents = state.program.agents();

    let question_text = state
        .program
        .question()
        .map(|q| q.text.clone())
        .unwrap_or_else(|| "Untitled Forecast".into());

    let total_evidence = evidence.len();
    let total_drivers = drivers.len();
    let total_agents = agents.iter().filter(|a| !a.driver_refs.is_empty()).count();

    div()
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .min_w(px(0.0))
        // ── Export button ─────────────────────────────────────────
        .child(
            div().flex().justify_end().child(
                div()
                    .id("export-wiki-md")
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(6.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::CYAN))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.export_wiki_markdown(cx);
                    }))
                    .child("📄 Export Markdown"),
            ),
        )
        // ── Question Header ───────────────────────────────────────
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .pb(px(12.0))
                .border_b_1()
                .border_color(rgb(theme::FG_FAINT))
                .child(
                    div()
                        .text_size(px(16.0))
                        .text_color(rgb(theme::FG))
                        .font_weight(FontWeight::BOLD)
                        .min_w(px(0.0))
                        .child(question_text),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(16.0))
                        .text_size(px(11.0))
                        .child(
                            div()
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(format!("{:.1}%", state.predicted_probability * 100.0)),
                        )
                        .when(state.forecast_confidence > 0.0, |el| {
                            let conf_label = if state.forecast_confidence > 0.7 {
                                "High"
                            } else if state.forecast_confidence > 0.4 {
                                "Medium"
                            } else {
                                "Low"
                            };
                            let conf_color = if state.forecast_confidence > 0.7 {
                                theme::GREEN
                            } else if state.forecast_confidence > 0.4 {
                                theme::GOLD
                            } else {
                                theme::RED
                            };
                            el.child(div().text_color(rgb(conf_color)).child(format!(
                                "Confidence: {} ({:.0}%)",
                                conf_label,
                                state.forecast_confidence * 100.0
                            )))
                        })
                        .child(div().text_color(rgb(theme::FG_FAINT)).child(format!(
                            "{} drivers · {} evidence · {} agents",
                            total_drivers, total_evidence, total_agents
                        ))),
                )
                .when(state.current_version > 0, |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(theme::FG_FAINT))
                            .child(format!(
                                "v{} · Last saved: {}",
                                state.current_version,
                                state
                                    .versions
                                    .last()
                                    .map(|v| v.timestamp.as_str())
                                    .unwrap_or("—")
                            )),
                    )
                })
                // ── Three-anchor delta chips (the core value strip) ──
                .child(render_delta_chips(state)),
        )
        // ── Inside View (always at top) ───────────────────────────
        .when(!state.inside_view_explanation.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(rgb(0x1A2332))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::CYAN))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Inside View"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme::CYAN))
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("{:.1}%", state.predicted_probability * 100.0)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG))
                            .min_w(px(0.0))
                            .child(state.inside_view_explanation.clone()),
                    ),
            )
        })
        // ── Outside View (Base Rate) ──────────────────────────────
        .when(
            state
                .program
                .question()
                .and_then(|q| q.base_rate.as_ref())
                .is_some(),
            |el| {
                let br = state
                    .program
                    .question()
                    .unwrap()
                    .base_rate
                    .as_ref()
                    .unwrap();
                let divergence = (state.predicted_probability - br.historical_frequency) * 100.0;
                let div_color = if divergence.abs() > 20.0 {
                    theme::RED
                } else if divergence.abs() > 10.0 {
                    theme::GOLD
                } else {
                    theme::GREEN
                };
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .px(px(12.0))
                        .py(px(10.0))
                        .rounded(px(6.0))
                        .bg(rgb(theme::BG_ELEVATED))
                        .border_1()
                        .border_color(rgb(theme::GOLD))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme::GOLD))
                                        .font_weight(FontWeight::BOLD)
                                        .child("Outside View (Base Rate)"),
                                )
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(rgb(theme::GOLD))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!("{:.1}%", br.historical_frequency * 100.0)),
                                )
                                .child(div().text_size(px(10.0)).text_color(rgb(div_color)).child(
                                    format!(
                                        "divergence: {}{:.0}pp",
                                        if divergence > 0.0 { "+" } else { "" },
                                        divergence
                                    ),
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::FG))
                                .min_w(px(0.0))
                                .child(format!(
                                    "Reference class: {}{}",
                                    br.reference_class,
                                    br.sample_size
                                        .map(|n| format!(" (n={})", n))
                                        .unwrap_or_default()
                                )),
                        )
                        .when(br.reasoning.is_some(), |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .min_w(px(0.0))
                                    .child(br.reasoning.as_deref().unwrap_or("").to_string()),
                            )
                        }),
                )
            },
        )
        // ── Crowd (Polymarket) — third anchor in the triad ─────────
        // Surfaces the prediction-market crowd price alongside the
        // inside/outside views so the wiki carries the full triad, not
        // just two of three. The deltas live in the header chip strip;
        // this block adds the qualitative context (question, volume,
        // liquidity, confidence band, URL).
        .when(state.pm_market_price.is_some(), |el| {
            let triad = AnchorTriad::from_state(state);
            let pm_price = state.pm_market_price.unwrap_or(0.0);
            // Reuse the same divergence logic the cockpit panel uses for
            // visual consistency: compare crowd to the model (inside view).
            let div_ic = triad.delta_ic_pp.unwrap_or(0.0);
            let div_color = if div_ic.abs() > 10.0 {
                theme::RED
            } else if div_ic.abs() > 3.0 {
                theme::GOLD
            } else {
                theme::FG_DIM
            };
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::PURPLE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .font_weight(FontWeight::BOLD)
                                    .child("Crowd View (Prediction Market)"),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .font_weight(FontWeight::BOLD)
                                    .child(format!("{:.1}%", pm_price * 100.0)),
                            )
                            .child(div().text_size(px(10.0)).text_color(rgb(div_color)).child(
                                format!(
                                    "model − crowd: {}{:.1}pp",
                                    if div_ic > 0.0 { "+" } else { "" },
                                    div_ic
                                ),
                            )),
                    )
                    .when(state.pm_question.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::FG))
                                .min_w(px(0.0))
                                .child(
                                    state
                                        .pm_question
                                        .as_deref()
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                        )
                    })
                    // Volume / liquidity / confidence band on one row.
                    .child({
                        let mut meta_row = div()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_DIM));
                        if let Some(vol) = state.pm_volume_24h {
                            meta_row = meta_row.child(div().child(format!(
                                "vol 24h: ${:.0}",
                                vol
                            )));
                        }
                        if let Some(liq) = state.pm_liquidity {
                            meta_row = meta_row.child(div().child(format!(
                                "liquidity: ${:.0}",
                                liq
                            )));
                        }
                        if let Some(ref conf) = state.pm_confidence {
                            meta_row =
                                meta_row.child(div().child(format!("confidence: {}", conf)));
                        }
                        if let Some(chg) = state.pm_price_change_1w {
                            let sign = if chg > 0.0 { "+" } else { "" };
                            meta_row = meta_row.child(div().child(format!(
                                "Δ1w: {}{:.1}pp",
                                sign,
                                chg * 100.0
                            )));
                        }
                        meta_row
                    })
                    .when(state.pm_url.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "↗ {}",
                                    state.pm_url.as_deref().unwrap_or("")
                                )),
                        )
                    }),
            )
        })
        // ── Forecast Index Charts (same as left panel) ────────────
        .when(
            state.sim_results.is_some() || !state.program.drivers().is_empty(),
            |el| {
                let mut chart_children: Vec<gpui::AnyElement> = Vec::new();

                // Interactive histogram with mouseover + anchor lines.
                // Replaces the static bitmap blit so users can hover any
                // bar to see outcome value, count, CDF percentile, and
                // signed distance to each anchor (model / base / crowd).
                if let Some(ref sim) = state.sim_results {
                    if !sim.histogram.is_empty() {
                        let chart_w = 500.0_f32;
                        let chart_h = 100.0_f32;
                        chart_children.push(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(format!(
                                            "Simulation Distribution ({}k iterations) — hover bars for details",
                                            sim.iterations / 1000
                                        )),
                                )
                                .child(render_interactive_histogram(
                                    state, cx, chart_w, chart_h,
                                ))
                                .into_any_element(),
                        );
                    }
                }

                // Interactive index chart — same bitmap line render as
                // before, plus a transparent hover layer with one column
                // per version that drives a crosshair + tooltip showing
                // the three anchor values + pairwise deltas at that
                // historical point.
                if state.versions.len() > 1 {
                    let chart_w = 500.0_f32;
                    let chart_h = 80.0_f32;
                    chart_children.push(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child("Model (cyan) · Base rate (gold) · Crowd (purple) — hover any version for details"),
                            )
                            .child(render_interactive_index_chart(
                                state, cx, chart_w, chart_h,
                            ))
                            .into_any_element(),
                    );
                }

                // Driver sensitivity — native GPUI bar chart (no bitmap)
                if !drivers.is_empty() {
                    let drivers_data: Vec<(String, f64, usize)> = drivers
                        .iter()
                        .map(|d| {
                            let display = d.display_name.as_deref().unwrap_or(&d.name).to_string();
                            // Sobol total-order index when available, else spread.
                            let impact = state
                                .driver_sensitivity
                                .get(&d.name)
                                .copied()
                                .map(|s| s.max(0.001))
                                .unwrap_or_else(|| match d.driver_type {
                                    DriverType::Continuous => {
                                        if let Some(Distribution::Triangular {
                                            ref p5, ref p95, ..
                                        }) = d.distribution
                                        {
                                            (expr_to_f64(p95) - expr_to_f64(p5)).abs().max(0.01)
                                        } else {
                                            0.5
                                        }
                                    }
                                    DriverType::Binary => {
                                        d.probability.unwrap_or(0.5)
                                            * d.impact_multiplier.unwrap_or(1.0)
                                    }
                                    _ => 0.5,
                                });
                            let ev_count = evidence
                                .iter()
                                .filter(|e| {
                                    e.id.contains(&d.name)
                                        || agents
                                            .iter()
                                            .filter(|a| a.driver_refs.contains(&d.name))
                                            .any(|a| evidence_matches_agent(e, &a.name))
                                })
                                .count();
                            (display, impact, ev_count)
                        })
                        .collect();

                    let max_impact = drivers_data.iter().map(|(_, imp, _)| *imp).fold(0.01_f64, f64::max);

                    if !drivers_data.is_empty() {
                        chart_children.push(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(if state.driver_sensitivity.is_empty() {
                                            "Driver sensitivity (spread) · evidence"
                                        } else {
                                            "Driver influence (Sobol total-order) · evidence"
                                        }),
                                )
                                .children(drivers_data.iter().map(|(display, impact, ev_count)| {
                                    let bar_frac = (impact / max_impact).clamp(0.05, 1.0);
                                    let bar_width = (bar_frac * 280.0) as f32;
                                    let ev_color = if *ev_count >= 3 {
                                        theme::GREEN
                                    } else if *ev_count >= 1 {
                                        theme::GOLD
                                    } else {
                                        theme::FG_FAINT
                                    };
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .h(px(16.0))
                                        .child(
                                            div()
                                                .w(px(140.0))
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::FG_DIM))
                                                .overflow_hidden()
                                                .child(display.clone()),
                                        )
                                        .child(
                                            div()
                                                .h(px(6.0))
                                                .w(px(bar_width))
                                                .rounded(px(1.0))
                                                .bg(rgba(0x5CCFE670)),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(8.0))
                                                .text_color(rgb(theme::FG_FAINT))
                                                .child(format!("{:.2}", impact)),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(8.0))
                                                .text_color(rgb(ev_color))
                                                .child(if *ev_count > 0 { format!("{}", ev_count) } else { "—".to_string() }),
                                        )
                                }))
                                .into_any_element(),
                        );
                    }
                }

                if chart_children.is_empty() {
                    el
                } else {
                    el.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .px(px(8.0))
                            .py(px(10.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG_ELEVATED))
                            .children(chart_children),
                    )
                }
            },
        )
        // ── Per-Driver Evidence Sections ──────────────────────────
        .children(drivers.iter().map(|driver| {
            let display = driver.display_name.as_deref().unwrap_or(&driver.name);
            let driver_agents: Vec<&str> = agents
                .iter()
                .filter(|a| a.driver_refs.contains(&driver.name))
                .map(|a| a.name.as_str())
                .collect();
            let driver_ev: Vec<_> = evidence
                .iter()
                .filter(|e| {
                    driver_agents.iter().any(|a| evidence_matches_agent(e, a))
                        || e.id.contains(&driver.name)
                })
                .collect();

            // Driver distribution summary
            let dist_summary = match driver.driver_type {
                DriverType::Continuous => {
                    if let Some(Distribution::Triangular {
                        ref p5,
                        ref p50,
                        ref p95,
                    }) = driver.distribution
                    {
                        let unit = driver.unit.as_deref().unwrap_or("");
                        format!(
                            "p5={:.2}  p50={:.2}  p95={:.2} {}",
                            expr_to_f64(p5),
                            expr_to_f64(p50),
                            expr_to_f64(p95),
                            unit
                        )
                    } else {
                        String::new()
                    }
                }
                DriverType::Binary => format!(
                    "P={:.0}%  impact=×{:.1}",
                    driver.probability.unwrap_or(0.0) * 100.0,
                    driver.impact_multiplier.unwrap_or(1.0)
                ),
                _ => String::new(),
            };

            // Evidence quality badge (uses score_evidence_quality for real scoring)
            let ev_count = driver_ev.len();
            let avg_quality = if driver_ev.is_empty() {
                0.0
            } else {
                driver_ev
                    .iter()
                    .map(|e| score_evidence_quality(e).0)
                    .sum::<f64>()
                    / driver_ev.len() as f64
            };
            let (quality_label, quality_color) = if avg_quality >= 0.7 {
                ("Strong", theme::GREEN)
            } else if avg_quality >= 0.4 {
                ("Partial", theme::GOLD)
            } else if ev_count > 0 {
                ("Weak", theme::ORANGE)
            } else {
                ("None", theme::RED)
            };

            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(10.0))
                .rounded(px(6.0))
                .bg(rgb(theme::BG_ELEVATED))
                // Driver header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(rgb(theme::GREEN))
                                .font_weight(FontWeight::BOLD)
                                .child(display.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(match driver.driver_type {
                                    DriverType::Continuous => theme::GREEN,
                                    DriverType::Binary => theme::GOLD,
                                    _ => theme::FG_DIM,
                                }))
                                .px(px(4.0))
                                .py(px(1.0))
                                .rounded(px(2.0))
                                .bg(rgb(theme::BG))
                                .child(match driver.driver_type {
                                    DriverType::Continuous => "continuous",
                                    DriverType::Binary => "binary",
                                    _ => "discrete",
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(quality_color))
                                .px(px(4.0))
                                .py(px(1.0))
                                .rounded(px(2.0))
                                .bg(rgb(theme::BG))
                                .child(format!("{} evidence ({})", quality_label, ev_count)),
                        ),
                )
                // Distribution parameters
                .when(!dist_summary.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::CYAN))
                            .font_family("Ubuntu Mono, DejaVu Sans Mono, monospace")
                            .child(dist_summary),
                    )
                })
                // Rationale
                .when(driver.rationale.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG_DIM))
                            .min_w(px(0.0))
                            .child(driver.rationale.as_deref().unwrap_or("").to_string()),
                    )
                })
                // Agent assignments with query info
                .when(!driver_agents.is_empty(), |el| {
                    el.child(div().flex().flex_col().gap(px(2.0)).mt(px(2.0)).children(
                        driver_agents.iter().map(|agent_name| {
                            let agent_stmt = agents.iter().find(|a| a.name == *agent_name);
                            let query_preview = agent_stmt
                                .map(|a| {
                                    if a.query.len() > 80 {
                                        format!("{}…", &a.query[..77])
                                    } else {
                                        a.query.clone()
                                    }
                                })
                                .unwrap_or_default();
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::BLUE))
                                        .px(px(4.0))
                                        .py(px(1.0))
                                        .rounded(px(2.0))
                                        .bg(rgb(theme::BG))
                                        .child(base_agent_name(agent_name).to_string()),
                                )
                                .when(!query_preview.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .flex_grow()
                                            .min_w(px(0.0))
                                            .child(format!("→ {}", query_preview)),
                                    )
                                })
                        }),
                    ))
                })
                // Evidence items — expanded, readable
                .children(driver_ev.iter().map(|ev| {
                    let (eq_score, eq_label, eq_color) = score_evidence_quality(ev);
                    let summary = ev.summary.as_deref().unwrap_or("");
                    // Show full summary up to 800 chars. Char-aware
                    // truncation (not byte slicing) — agent research often
                    // contains multibyte UTF-8 (em-dashes, smart quotes,
                    // 'Türkiye', etc.). `&summary[..797]` on a non-char
                    // boundary panics: "byte index 797 is not a char
                    // boundary; it is inside ‘…’".
                    let display_summary = if summary.chars().count() > 800 {
                        format!("{}…", summary.chars().take(797).collect::<String>())
                    } else {
                        summary.to_string()
                    };

                    // Detect URLs in summary for display
                    let has_url = summary.contains("http://") || summary.contains("https://");

                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG))
                        .mt(px(4.0))
                        .border_l_2()
                        .border_color(rgb(if ev.relevance.unwrap_or(0.0) > 0.7 {
                            theme::GREEN
                        } else if ev.relevance.unwrap_or(0.0) > 0.4 {
                            theme::GOLD
                        } else {
                            theme::FG_FAINT
                        }))
                        // Source + date + relevance header
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(ev.source.clone()),
                                )
                                .when(ev.date.is_some(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child(ev.date.as_deref().unwrap_or("").to_string()),
                                    )
                                })
                                .when(ev.relevance.is_some(), |el| {
                                    let r = ev.relevance.unwrap_or(0.0);
                                    let r_color = if r > 0.7 {
                                        theme::GREEN
                                    } else if r > 0.4 {
                                        theme::GOLD
                                    } else {
                                        theme::FG_FAINT
                                    };
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(r_color))
                                            .px(px(4.0))
                                            .py(px(1.0))
                                            .rounded(px(2.0))
                                            .bg(rgb(theme::BG_ELEVATED))
                                            .child(format!("relevance {:.0}%", r * 100.0)),
                                    )
                                })
                                // Quality badge
                                .child(
                                    div()
                                        .text_size(px(8.0))
                                        .text_color(rgb(eq_color))
                                        .px(px(4.0))
                                        .py(px(1.0))
                                        .rounded(px(2.0))
                                        .bg(rgb(theme::BG_ELEVATED))
                                        .child(format!("{} {:.0}%", eq_label, eq_score * 100.0)),
                                ),
                        )
                        // Quality bar (thin colored strip)
                        .child(
                            div()
                                .h(px(2.0))
                                .w_full()
                                .rounded(px(1.0))
                                .bg(rgb(theme::BG_ELEVATED))
                                .child(
                                    div()
                                        .h(px(2.0))
                                        .rounded(px(1.0))
                                        .bg(rgb(eq_color))
                                        .w(gpui::px((eq_score * 200.0).min(200.0) as f32)),
                                ),
                        )
                        // Summary — full text, readable size
                        .when(!display_summary.is_empty(), |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG))
                                    .min_w(px(0.0))
                                    .child(display_summary),
                            )
                        })
                        // Key findings — each as a proper bullet item
                        .when(!ev.key_findings.is_empty(), |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(3.0))
                                    .mt(px(4.0))
                                    .pt(px(4.0))
                                    .border_t_1()
                                    .border_color(rgb(theme::FG_FAINT))
                                    .child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(format!(
                                                "Key Findings ({})",
                                                ev.key_findings.len()
                                            )),
                                    )
                                    .children(ev.key_findings.iter().take(8).map(|f| {
                                        div()
                                            .flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(theme::CYAN))
                                                    .w(px(12.0))
                                                    .child("▸"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(rgb(theme::FG))
                                                    .flex_grow()
                                                    .min_w(px(0.0))
                                                    .child(f.clone()),
                                            )
                                    })),
                            )
                        })
                        // URL indicator
                        .when(has_url, |el| {
                            // Extract URLs from summary
                            let urls: Vec<&str> = summary
                                .split_whitespace()
                                .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
                                .take(3)
                                .collect();
                            if urls.is_empty() {
                                el
                            } else {
                                el.child(div().flex().flex_col().gap(px(2.0)).mt(px(4.0)).children(
                                    urls.iter().map(|url| {
                                        // Trim trailing punctuation
                                        let clean = url.trim_end_matches(|c: char| {
                                            c == ',' || c == '.' || c == ')' || c == ']' || c == '"'
                                        });
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::BLUE))
                                            .min_w(px(0.0))
                                            .child(format!("🔗 {}", clean))
                                    }),
                                ))
                            }
                        })
                }))
                // No evidence yet
                .when(driver_ev.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x2D1F1F))
                            .mt(px(4.0))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme::RED))
                                    .child("⚠"),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .child(
                                        "No evidence yet — assign an agent to research this driver",
                                    ),
                            ),
                    )
                })
        }))
        // ── Unlinked Evidence ─────────────────────────────────────
        .when(!evidence.is_empty(), |el| {
            let all_agent_names: Vec<String> = agents
                .iter()
                .filter(|a| !a.driver_refs.is_empty())
                .map(|a| a.name.clone())
                .collect();
            let unlinked: Vec<_> = evidence
                .iter()
                .filter(|e| !all_agent_names.iter().any(|a| evidence_matches_agent(e, a)))
                .collect();
            if unlinked.is_empty() {
                el
            } else {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(10.0))
                        .rounded(px(6.0))
                        .bg(rgb(theme::BG_ELEVATED))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(theme::FG_DIM))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(format!("General Evidence ({})", unlinked.len())),
                        )
                        .children(unlinked.iter().map(|ev| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(px(4.0))
                                .bg(rgb(theme::BG))
                                .border_l_2()
                                .border_color(rgb(theme::FG_FAINT))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(ev.source.clone()),
                                )
                                .when(ev.summary.is_some(), |el| {
                                    let s = ev.summary.as_deref().unwrap_or("");
                                    // Char-aware: see comment near
                                    // display_summary above. Same UTF-8
                                    // boundary panic risk on agent output.
                                    let display = if s.chars().count() > 500 {
                                        format!(
                                            "{}…",
                                            s.chars().take(497).collect::<String>()
                                        )
                                    } else {
                                        s.to_string()
                                    };
                                    el.child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::FG_DIM))
                                            .min_w(px(0.0))
                                            .child(display),
                                    )
                                })
                                .when(!ev.key_findings.is_empty(), |el| {
                                    el.children(ev.key_findings.iter().take(4).map(|f| {
                                        div()
                                            .flex()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(theme::FG_FAINT))
                                                    .w(px(12.0))
                                                    .child("▸"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(rgb(theme::FG_DIM))
                                                    .flex_grow()
                                                    .min_w(px(0.0))
                                                    .child(f.clone()),
                                            )
                                    }))
                                })
                        })),
                )
            }
        })
        // ── Version Diff Panel (shown when a version is selected) ─
        .when(state.selected_version.is_some(), |el| {
            let sel_v = state.selected_version.unwrap();
            let sel = state.versions.iter().find(|v| v.version == sel_v);
            let current_prob = state.predicted_probability;

            if let Some(ver) = sel {
                let prob_delta = (current_prob - ver.probability) * 100.0;
                let old_fpl = &ver.fpl_text;

                // ── Semantic diff: parse both FPL versions and compare ──
                let mut changes: Vec<(u32, String)> = Vec::new(); // (color, text)

                // 1. Probability change
                if prob_delta.abs() > 0.1 {
                    let dir = if prob_delta > 0.0 { "▲" } else { "▼" };
                    changes.push((
                        if prob_delta > 0.0 { theme::GREEN } else { theme::RED },
                        format!(
                            "{} Probability: {:.1}% → {:.1}% ({}pp)",
                            dir,
                            ver.probability * 100.0,
                            current_prob * 100.0,
                            if prob_delta > 0.0 { format!("+{:.0}", prob_delta) } else { format!("{:.0}", prob_delta) }
                        ),
                    ));
                } else {
                    changes.push((theme::FG_DIM, "→ Probability unchanged".to_string()));
                }

                // 2. Parse old FPL for driver names
                let old_drivers: Vec<String> = old_fpl
                    .lines()
                    .filter(|l| l.trim().starts_with("driver "))
                    .filter_map(|l| {
                        l.trim()
                            .strip_prefix("driver ")
                            .and_then(|rest| rest.split_whitespace().next())
                            .map(|s| s.to_string())
                    })
                    .collect();
                let current_drivers: Vec<String> = state
                    .program
                    .drivers()
                    .iter()
                    .map(|d| d.name.clone())
                    .collect();

                // Drivers added
                for d in &current_drivers {
                    if !old_drivers.contains(d) {
                        let display = state.program.driver(d)
                            .and_then(|dr| dr.display_name.as_deref())
                            .unwrap_or(d);
                        changes.push((theme::GREEN, format!("+ Driver: {}", display)));
                    }
                }
                // Drivers removed
                for d in &old_drivers {
                    if !current_drivers.contains(d) {
                        changes.push((theme::RED, format!("- Driver: {}", d)));
                    }
                }

                // 3. Evidence count change
                let old_ev_count = old_fpl
                    .lines()
                    .filter(|l| l.trim().starts_with("evidence "))
                    .count();
                let new_ev_count = state.program.evidence_items().len();
                if new_ev_count != old_ev_count {
                    let delta = new_ev_count as i64 - old_ev_count as i64;
                    changes.push((
                        if delta > 0 { theme::GREEN } else { theme::GOLD },
                        format!(
                            "{} Evidence: {} → {} ({})",
                            if delta > 0 { "+" } else { "−" },
                            old_ev_count,
                            new_ev_count,
                            if delta > 0 { format!("+{}", delta) } else { format!("{}", delta) }
                        ),
                    ));
                }

                // 4. Agent count change
                let old_agent_count = old_fpl
                    .lines()
                    .filter(|l| l.trim().starts_with("agent "))
                    .count();
                let new_agent_count = state.program.agents().len();
                if new_agent_count != old_agent_count {
                    let delta = new_agent_count as i64 - old_agent_count as i64;
                    changes.push((
                        theme::BLUE,
                        format!(
                            "{} Agents: {} → {}",
                            if delta > 0 { "+" } else { "−" },
                            old_agent_count,
                            new_agent_count,
                        ),
                    ));
                }

                // 5. Base rate change
                let old_base = old_fpl
                    .lines()
                    .find(|l| l.contains("historical_frequency"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .filter_map(|w| w.trim_end_matches('%').parse::<f64>().ok())
                            .next()
                    });
                let new_base = state
                    .program
                    .question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency);
                if let (Some(ob), Some(nb)) = (old_base, new_base) {
                    if (ob - nb).abs() > 0.001 {
                        changes.push((
                            theme::GOLD,
                            format!("⟳ Base rate: {:.1}% → {:.1}%", ob * 100.0, nb * 100.0),
                        ));
                    }
                }

                // 6. Driver parameter changes (p50 values)
                for d in &current_drivers {
                    if old_drivers.contains(d) {
                        // Find old p50 from FPL text
                        let old_p50: Option<f64> = old_fpl.lines()
                            .skip_while(|l| !l.contains(&format!("driver {}", d)))
                            .skip(1)
                            .take(5)
                            .find(|l| l.contains("distribution"))
                            .and_then(|l| {
                                // Extract middle number from triangular(a, b, c)
                                l.split(',').nth(1).and_then(|s| s.trim().parse().ok())
                            });
                        let new_p50 = state.program.driver(d).and_then(|dr| {
                            dr.distribution.as_ref().map(|dist| match dist {
                                Distribution::Triangular { p50, .. } => expr_to_f64(p50),
                                _ => 0.0,
                            })
                        });
                        if let (Some(op), Some(np)) = (old_p50, new_p50) {
                            if (op - np).abs() > 0.01 {
                                let display = state.program.driver(d)
                                    .and_then(|dr| dr.display_name.as_deref())
                                    .unwrap_or(d);
                                changes.push((
                                    theme::CYAN,
                                    format!("~ {}: p50 {:.2} → {:.2}", display, op, np),
                                ));
                            }
                        }
                    }
                }

                if changes.len() <= 1 {
                    changes.push((theme::FG_FAINT, "No structural changes detected".to_string()));
                }

                let ver_num = ver.version;
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(10.0))
                        .rounded(px(6.0))
                        .bg(rgb(0x1A1E2E))
                        .border_1()
                        .border_color(rgb(theme::PURPLE))
                        // Header with close button
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme::PURPLE))
                                        .font_weight(FontWeight::BOLD)
                                        .child(format!(
                                            "v{} → current: {:.1}% → {:.1}% ({}pp)",
                                            ver.version,
                                            ver.probability * 100.0,
                                            current_prob * 100.0,
                                            if prob_delta > 0.0 {
                                                format!("+{:.0}", prob_delta)
                                            } else {
                                                format!("{:.0}", prob_delta)
                                            }
                                        )),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.0))
                                        // Restore button
                                        .child(
                                            div()
                                                .id(ElementId::Name(
                                                    format!("restore-v{}", ver.version).into(),
                                                ))
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded(px(4.0))
                                                .bg(rgb(theme::BG))
                                                .border_1()
                                                .border_color(rgb(theme::GOLD))
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::GOLD))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    // Restore: parse the old FPL and load it
                                                    let old_fpl_text = this
                                                        .versions
                                                        .iter()
                                                        .find(|v| v.version == ver_num)
                                                        .map(|v| v.fpl_text.clone())
                                                        .unwrap_or_default();
                                                    if !old_fpl_text.is_empty() {
                                                        if let Ok(tokens) =
                                                            ::fermi::lexer::Lexer::new(&old_fpl_text)
                                                                .tokenize()
                                                        {
                                                            if let Ok(program) =
                                                                ::fermi::parser::Parser::new(tokens)
                                                                    .parse()
                                                            {
                                                                this.program = program;
                                                                this.predicted_probability =
                                                                    this.versions
                                                                        .iter()
                                                                        .find(|v| {
                                                                            v.version == ver_num
                                                                        })
                                                                        .map(|v| v.probability)
                                                                        .unwrap_or(0.5);
                                                                this.messages.push(
                                                                    AssistantMessage {
                                                                        node: "version".into(),
                                                                        kind: MessageKind::Info,
                                                                        text: format!(
                                                                        "Restored v{}. Save to create a new version.",
                                                                        ver_num
                                                                    ),
                                                                    },
                                                                );
                                                            }
                                                        }
                                                    }
                                                    this.selected_version = None;
                                                    cx.notify();
                                                }))
                                                .child("↩ Restore"),
                                        )
                                        // Close button
                                        .child(
                                            div()
                                                .id("close-version-diff")
                                                .px(px(8.0))
                                                .py(px(3.0))
                                                .rounded(px(4.0))
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::FG_DIM))
                                                .cursor_pointer()
                                                .hover(|s| s.text_color(rgb(theme::FG)))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.selected_version = None;
                                                    cx.notify();
                                                }))
                                                .child("✕"),
                                        ),
                                ),
                        )
                        // Version metadata
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "{} — {}",
                                    ver.timestamp, ver.change_summary
                                )),
                        )
                        // Semantic changes list
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .mt(px(4.0))
                                .children(changes.iter().map(|(color, text)| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .px(px(6.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(theme::BG))
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(rgb(*color))
                                                .min_w(px(0.0))
                                                .child(text.clone()),
                                        )
                                })),
                        ),
                )
            } else {
                el
            }
        })
        // ── Version History ───────────────────────────────────────
        .when(!state.versions.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme::PURPLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Version History ({})", state.versions.len())),
                    )
                    .children(state.versions.iter().rev().map(|v| {
                        let prob_change = if v.version > 1 {
                            state
                                .versions
                                .iter()
                                .find(|prev| prev.version == v.version - 1)
                                .map(|prev| {
                                    let delta = (v.probability - prev.probability) * 100.0;
                                    let sign = if delta > 0.0 { "+" } else { "" };
                                    let color = if delta > 0.0 {
                                        theme::GREEN
                                    } else if delta < 0.0 {
                                        theme::RED
                                    } else {
                                        theme::FG_DIM
                                    };
                                    (format!("{}{}pp", sign, delta as i64), color)
                                })
                        } else {
                            None
                        };

                        let is_selected = state.selected_version == Some(v.version);
                        let ver_num = v.version;

                        div()
                            .id(ElementId::Name(format!("version-{}", v.version).into()))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(4.0))
                            .px(px(4.0))
                            .rounded(px(3.0))
                            .cursor_pointer()
                            .bg(if is_selected {
                                rgb(theme::BG_ACTIVE)
                            } else {
                                rgb(theme::BG_ELEVATED)
                            })
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.selected_version == Some(ver_num) {
                                    this.selected_version = None;
                                } else {
                                    this.selected_version = Some(ver_num);
                                }
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(if is_selected {
                                        rgb(theme::PURPLE)
                                    } else {
                                        rgb(theme::FG_DIM)
                                    })
                                    .w(px(28.0))
                                    .child(format!("v{}", v.version)),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::CYAN))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .w(px(55.0))
                                    .child(format!("{:.1}%", v.probability * 100.0)),
                            )
                            .when(prob_change.is_some(), |el| {
                                let (text, color) = prob_change.unwrap();
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(color))
                                        .w(px(50.0))
                                        .child(text),
                                )
                            })
                            .child(
                                div()
                                    .flex_grow()
                                    .min_w(px(0.0))
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child(format!("{} — {}", v.timestamp, v.change_summary)),
                            )
                            .when(is_selected, |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::PURPLE))
                                        .child("▾"),
                                )
                            })
                    })),
            )
        })
}

fn render_fpl_source(fpl: &str) -> impl IntoElement {
    div()
        .flex_grow()
        .p(px(12.0))
        .bg(rgb(theme::BG))
        .border_t_1()
        .border_color(rgb(theme::PURPLE))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::PURPLE))
                .font_weight(FontWeight::SEMIBOLD)
                .child("FPL Source"),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .font_family("Ubuntu Mono, DejaVu Sans Mono, monospace")
                .child(if fpl.is_empty() {
                    "# Empty program".to_string()
                } else {
                    fpl.to_string()
                }),
        )
}

// ═══════════════════════════════════════════════════════════════════
// FPL Generation — serialize the AST back to FPL text
// ═══════════════════════════════════════════════════════════════════

fn generate_fpl_text(program: &Program) -> String {
    let mut lines = Vec::new();

    // Question
    if let Some(q) = program.question() {
        let escaped = q.text.replace('"', r#"\""#);
        lines.push(format!("question \"{}\"", escaped));
        // Note: base_rate is stored in the AST and shown in the UI
        // but not output in the FPL text to avoid parser issues
        // with the simplified question syntax
        lines.push(String::new());
    }

    // Drivers
    for driver in program.drivers() {
        let safe_name = sanitize_name(&driver.name);
        match driver.driver_type {
            DriverType::Continuous => {
                lines.push(format!("driver {} continuous {{", safe_name));
                if let Some(Distribution::Triangular {
                    ref p5,
                    ref p50,
                    ref p95,
                }) = driver.distribution
                {
                    lines.push(format!(
                        "    distribution: triangular({}, {}, {})",
                        expr_to_f64(p5),
                        expr_to_f64(p50),
                        expr_to_f64(p95)
                    ));
                }
                if let Some(ref unit) = driver.unit {
                    lines.push(format!("    unit: \"{}\"", unit));
                }
                // Emit `learnable: true` only when set — false is the default,
                // so omit it for cleanliness. The static `distribution:`
                // above doubles as the cold-start prior when learnable is on.
                if driver.learnable {
                    lines.push("    learnable: true".into());
                }
                if let Some(ref rationale) = driver.rationale {
                    lines.push(format!(
                        "    rationale: \"{}\"",
                        rationale.replace('"', r#"\""#)
                    ));
                }
                lines.push("}".into());
            }
            DriverType::Binary => {
                lines.push(format!("driver {} binary {{", safe_name));
                if let Some(p) = driver.probability {
                    lines.push(format!("    probability: {}p", p));
                }
                if let Some(m) = driver.impact_multiplier {
                    lines.push(format!("    impact_multiplier: {}", m));
                }
                if driver.learnable {
                    lines.push("    learnable: true".into());
                }
                if let Some(ref rationale) = driver.rationale {
                    lines.push(format!(
                        "    rationale: \"{}\"",
                        rationale.replace('"', r#"\""#)
                    ));
                }
                lines.push("}".into());
            }
            _ => {}
        }
        lines.push(String::new());
    }

    // Evidence
    for ev in program.evidence_items() {
        lines.push(format!("evidence {} {{", sanitize_name(&ev.id)));
        lines.push(format!("    source: \"{}\"", clean_fpl_string(&ev.source)));
        if let Some(ref summary) = ev.summary {
            lines.push(format!(
                "    summary: \"{}\"",
                summary.replace('"', r#"\""#)
            ));
        }
        if let Some(rel) = ev.relevance {
            lines.push(format!("    relevance: {}p", rel));
        }
        if let Some(ref date) = ev.date {
            lines.push(format!("    date: {}", date));
        }
        lines.push("}".into());
        lines.push(String::new());
    }

    // Model
    if let Some(model) = program.model() {
        lines.push(format!("model: {}", model.expression));
        lines.push(String::new());
    } else if !program.drivers().is_empty() {
        // Auto-generate model from drivers
        let parts: Vec<String> = program
            .drivers()
            .iter()
            .map(|d| match d.driver_type {
                DriverType::Binary => {
                    let m = d.impact_multiplier.unwrap_or(1.3);
                    format!("(if {} then {} else 1.0)", d.name, m)
                }
                _ => d.name.clone(),
            })
            .collect();
        if !parts.is_empty() {
            lines.push(format!("model: {}", parts.join(" * ")));
            lines.push(String::new());
        }
    }

    // Simulate
    if let Some(sim) = program.simulate() {
        lines.push(format!("simulate {} iterations", sim.iterations));
    }

    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════════════
// Domain Detection + Decomposition Template Generation
// ═══════════════════════════════════════════════════════════════════

/// Generate an evidence wiki markdown file organized by driver.
/// Each driver is a heading, with agent evidence entries as logs.
/// This is the SHAREABLE REPORT — no truncation, full evidence, proper formatting.
fn generate_evidence_wiki(
    program: &Program,
    version: u32,
    probability: f64,
    explanation: &str,
    confidence: f64,
    pm_market_price: Option<f64>,
    pm_url: Option<&str>,
    pm_volume_24h: Option<f64>,
    pm_confidence: Option<&str>,
    pm_price_change_1w: Option<f64>,
    sim_results: Option<&SimResults>,
    versions: &[ForecastVersion],
) -> String {
    let mut md = String::new();
    let question = program
        .question()
        .map(|q| q.text.as_str())
        .unwrap_or("Untitled Forecast");
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let drivers = program.drivers();
    let evidence_items = program.evidence_items();
    let agents = program.agents();
    let total_evidence = evidence_items.len();
    let total_drivers = drivers.len();
    let total_agents = agents.iter().filter(|a| !a.driver_refs.is_empty()).count();

    // ── Header ────────────────────────────────────────────────────
    md.push_str(&format!("# {}\n\n", question));
    md.push_str(&format!(
        "**Probability:** {:.1}% · **Version:** v{} · **Updated:** {}\n\n",
        probability * 100.0,
        version,
        timestamp
    ));

    let conf_label = if confidence > 0.7 {
        "High"
    } else if confidence > 0.4 {
        "Medium"
    } else {
        "Low"
    };
    md.push_str(&format!(
        "**Confidence:** {} ({:.0}%) · **Drivers:** {} · **Evidence:** {} · **Agents:** {}\n\n",
        conf_label,
        confidence * 100.0,
        total_drivers,
        total_evidence,
        total_agents
    ));
    md.push_str("---\n\n");

    // ── Polymarket Crowd Price ─────────────────────────────────────
    if let Some(crowd_price) = pm_market_price {
        md.push_str("## Polymarket Crowd Price\n\n");
        let divergence_pp = (probability - crowd_price) * 100.0;
        let edge = if divergence_pp.abs() < 2.0 {
            "Consensus"
        } else if divergence_pp.abs() < 5.0 {
            "Minor divergence"
        } else if divergence_pp.abs() < 15.0 {
            "Moderate divergence — potential edge"
        } else {
            "Significant disagreement — verify assumptions"
        };
        let direction = if divergence_pp >= 0.0 {
            "above"
        } else {
            "below"
        };
        md.push_str(&format!(
            "| Metric | Value |\n|---|---|\n| Crowd price | **{:.1}%** |\n| Fermi estimate | **{:.1}%** |\n| Divergence | {:+.1}pp {} crowd ({}) |\n",
            crowd_price * 100.0,
            probability * 100.0,
            divergence_pp.abs(),
            direction,
            edge,
        ));
        if let Some(vol) = pm_volume_24h {
            let vol_fmt = if vol >= 1_000_000.0 {
                format!("${:.1}M", vol / 1_000_000.0)
            } else if vol >= 1_000.0 {
                format!("${:.0}K", vol / 1_000.0)
            } else {
                format!("${:.0}", vol)
            };
            md.push_str(&format!("| 24h volume | {} |\n", vol_fmt));
        }
        if let Some(conf) = pm_confidence {
            md.push_str(&format!("| Market confidence | {} |\n", conf));
        }
        if let Some(chg) = pm_price_change_1w {
            let arrow = if chg > 0.005 {
                "↑"
            } else if chg < -0.005 {
                "↓"
            } else {
                "→"
            };
            md.push_str(&format!(
                "| 1-week trend | {} {:+.1}pp |\n",
                arrow,
                chg * 100.0
            ));
        }
        if let Some(url) = pm_url {
            md.push_str(&format!("\n[View on Polymarket]({})\n", url));
        }
        md.push_str("\n---\n\n");
    }

    // ── Inside View (FIRST — this is the main analysis) ───────────
    if !explanation.is_empty() {
        md.push_str("## Inside View\n\n");
        md.push_str(&format!("**Probability: {:.1}%**\n\n", probability * 100.0));
        md.push_str(&format!("{}\n\n", explanation));
        md.push_str(&format!(
            "**Forecast Confidence:** {} ({:.0}%)\n\n",
            conf_label,
            confidence * 100.0
        ));

        // Divergence from base rate
        if let Some(br) = program.question().and_then(|q| q.base_rate.as_ref()) {
            let divergence = (probability - br.historical_frequency) * 100.0;
            let direction = if divergence > 0.0 { "above" } else { "below" };
            md.push_str(&format!(
                "**Divergence from base rate:** {:.0}pp {} ({:.1}% vs {:.1}%)\n\n",
                divergence.abs(),
                direction,
                probability * 100.0,
                br.historical_frequency * 100.0
            ));
        }
        md.push_str("---\n\n");
    }

    // ── Outside View (Base Rate) ──────────────────────────────────
    if let Some(br) = program.question().and_then(|q| q.base_rate.as_ref()) {
        md.push_str("## Outside View (Base Rate)\n\n");
        md.push_str(&format!(
            "**{:.1}%** — {}\n\n",
            br.historical_frequency * 100.0,
            br.reference_class
        ));
        if let Some(n) = br.sample_size {
            md.push_str(&format!("- **Sample size:** n={}\n", n));
        }
        md.push_str(&format!("- **Source:** {}\n", br.source));
        if let Some(ref r) = br.reasoning {
            md.push_str(&format!("\n{}\n", r));
        }
        md.push_str("\n---\n\n");
    }

    // ── Simulation distribution (ASCII histogram) ─────────────────
    // Renders a fixed-width text histogram of the simulation output so
    // the visual that lives in the composer/wiki tabs survives a
    // markdown export. Uses ▁▂▃▄▅▆▇█ block characters scaled to the
    // tallest bin, plus a markdown table with bin centers + counts.
    if let Some(sim) = sim_results {
        if !sim.histogram.is_empty() {
            md.push_str("## Simulation Distribution\n\n");
            md.push_str(&format!(
                "**{} iterations** · p5 = {:.1}% · median = {:.1}% · p95 = {:.1}% · σ = {:.3}\n\n",
                sim.iterations,
                sim.p5 * 100.0,
                sim.median * 100.0,
                sim.p95 * 100.0,
                sim.std_dev,
            ));

            // Inline sparkline.
            let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            let max_count = *sim.histogram.iter().max().unwrap_or(&1) as f64;
            let spark: String = sim
                .histogram
                .iter()
                .map(|&c| {
                    let frac = c as f64 / max_count.max(1.0);
                    let idx = (frac * (blocks.len() - 1) as f64).round() as usize;
                    blocks[idx.min(blocks.len() - 1)]
                })
                .collect();
            md.push_str(&format!("```\n{}\n```\n\n", spark));

            // Tabular breakdown with bin center, count, and percent.
            // Only emit if we have bin_starts available (fresh sim);
            // older loaded forecasts skip the table and keep just the
            // sparkline.
            if !sim.bin_starts.is_empty() && sim.bin_starts.len() == sim.histogram.len() {
                let total: u64 = sim.histogram.iter().map(|&c| c as u64).sum();
                md.push_str("| Bin center | Count | % of sims |\n|---|---|---|\n");
                for (i, &c) in sim.histogram.iter().enumerate() {
                    let center = sim.bin_starts[i] + sim.bin_width * 0.5;
                    let pct = if total > 0 {
                        c as f64 / total as f64 * 100.0
                    } else {
                        0.0
                    };
                    md.push_str(&format!(
                        "| {:.1}% | {} | {:.1}% |\n",
                        center * 100.0,
                        c,
                        pct
                    ));
                }
                md.push_str("\n");
            }
            md.push_str("---\n\n");
        }
    }

    // ── Index chart (version history with three anchors) ───────────
    // Versions over time as a markdown table. Each row shows the three
    // anchor values + deltas at that point. The wiki tab has an
    // interactive line chart here; markdown gets the underlying numbers
    // and a per-row sparkline for the model line.
    if versions.len() >= 2 {
        let base_rate_pct = program
            .question()
            .and_then(|q| q.base_rate.as_ref())
            .map(|br| br.historical_frequency * 100.0);
        let crowd_pct = pm_market_price.map(|p| p * 100.0);

        md.push_str("## Forecast Index (version history)\n\n");
        let mut header = String::from("| v | timestamp | model |");
        let mut sep = String::from("|---|---|---|");
        if base_rate_pct.is_some() {
            header.push_str(" base |");
            sep.push_str("---|");
        }
        if crowd_pct.is_some() {
            header.push_str(" crowd |");
            sep.push_str("---|");
        }
        header.push_str(" Δ(model−base) |");
        sep.push_str("---|");
        if crowd_pct.is_some() {
            header.push_str(" Δ(model−crowd) |");
            sep.push_str("---|");
        }
        header.push_str(" note |");
        sep.push_str("---|");
        md.push_str(&format!("{}\n{}\n", header, sep));

        for v in versions {
            let model_pct = v.probability * 100.0;
            let mut row = format!(
                "| v{} | {} | {:.1}% |",
                v.version,
                v.timestamp.split('T').next().unwrap_or(&v.timestamp),
                model_pct
            );
            if let Some(b) = base_rate_pct {
                row.push_str(&format!(" {:.1}% |", b));
            }
            if let Some(c) = crowd_pct {
                row.push_str(&format!(" {:.1}% |", c));
            }
            if let Some(b) = base_rate_pct {
                row.push_str(&format!(" {:+.1}pp |", model_pct - b));
            } else {
                row.push_str(" — |");
            }
            if let Some(c) = crowd_pct {
                row.push_str(&format!(" {:+.1}pp |", model_pct - c));
            }
            // Trim change_summary to fit in the table cell.
            let note = if v.change_summary.len() > 60 {
                format!("{}…", &v.change_summary[..57])
            } else {
                v.change_summary.clone()
            };
            row.push_str(&format!(" {} |", note.replace('|', "\\|")));
            md.push_str(&row);
            md.push('\n');
        }

        // Inline sparkline of the model line so the trend is visible
        // even before reading the table.
        let blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let probs: Vec<f64> = versions.iter().map(|v| v.probability).collect();
        let min = probs.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = probs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let span = (max - min).max(1e-9);
        let spark: String = probs
            .iter()
            .map(|&p| {
                let frac = (p - min) / span;
                let idx = (frac * (blocks.len() - 1) as f64).round() as usize;
                blocks[idx.min(blocks.len() - 1)]
            })
            .collect();
        md.push_str(&format!(
            "\n**Model line:** ```{}``` (range {:.1}% – {:.1}%)\n\n",
            spark,
            min * 100.0,
            max * 100.0
        ));
        md.push_str("---\n\n");
    }

    // ── Drivers with Evidence ─────────────────────────────────────
    for (i, driver) in drivers.iter().enumerate() {
        let display = driver.display_name.as_deref().unwrap_or(&driver.name);
        let type_label = match driver.driver_type {
            DriverType::Continuous => "continuous",
            DriverType::Binary => "binary",
            _ => "discrete",
        };

        md.push_str(&format!("## {}. {} `{}`\n\n", i + 1, display, type_label));

        // Driver parameters
        match driver.driver_type {
            DriverType::Continuous => {
                if let Some(ref dist) = driver.distribution {
                    if let Distribution::Triangular {
                        ref p5,
                        ref p50,
                        ref p95,
                    } = dist
                    {
                        let unit = driver.unit.as_deref().unwrap_or("");
                        md.push_str(&format!(
                            "| p5 | p50 | p95 | unit |\n|---:|---:|---:|---|\n| {:.2} | {:.2} | {:.2} | {} |\n\n",
                            expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95), unit
                        ));
                    }
                }
            }
            DriverType::Binary => {
                md.push_str(&format!(
                    "- **Probability:** {:.0}%\n- **Impact multiplier:** ×{:.1}\n\n",
                    driver.probability.unwrap_or(0.0) * 100.0,
                    driver.impact_multiplier.unwrap_or(1.0)
                ));
            }
            _ => {}
        }

        // Rationale
        if let Some(ref rationale) = driver.rationale {
            md.push_str(&format!("> {}\n\n", rationale));
        }

        // Agents assigned to this driver
        let driver_agents: Vec<_> = agents
            .iter()
            .filter(|a| a.driver_refs.contains(&driver.name))
            .collect();

        if !driver_agents.is_empty() {
            md.push_str("### Assigned Agents\n\n");
            for agent in &driver_agents {
                let schedule = match &agent.schedule {
                    Some(Schedule::Once) => "once",
                    Some(Schedule::Every { interval, unit }) => {
                        &format!("every {} {:?}", interval, unit)
                    }
                    _ => "on-demand",
                };
                md.push_str(&format!(
                    "- **{}** (schedule: {})  \n  Query: _{}_\n",
                    base_agent_name(&agent.name),
                    schedule,
                    agent.query
                ));
            }
            md.push_str("\n");
        }

        // Evidence linked to this driver — FULL, no truncation
        let driver_evidence: Vec<_> = evidence_items
            .iter()
            .filter(|e| {
                driver_agents
                    .iter()
                    .any(|a| evidence_matches_agent(e, &a.name))
                    || e.id.contains(&driver.name)
            })
            .collect();

        if !driver_evidence.is_empty() {
            // Compute average quality for the evidence set
            let avg_q: f64 = driver_evidence
                .iter()
                .map(|e| score_evidence_quality(e).0)
                .sum::<f64>()
                / driver_evidence.len() as f64;
            let q_label = if avg_q >= 0.7 {
                "Strong"
            } else if avg_q >= 0.4 {
                "Partial"
            } else {
                "Weak"
            };
            md.push_str(&format!(
                "### Evidence ({}) — {} quality ({:.0}%)\n\n",
                driver_evidence.len(),
                q_label,
                avg_q * 100.0
            ));
            for ev in &driver_evidence {
                let relevance_pct = ev.relevance.unwrap_or(0.0) * 100.0;
                let (eq_score, eq_label, _) = score_evidence_quality(ev);
                let date_str = ev
                    .date
                    .as_deref()
                    .map(|d| format!(" · {}", d))
                    .unwrap_or_default();
                md.push_str(&format!(
                    "#### {} — relevance {:.0}% · quality {} ({:.0}%){}\n\n",
                    ev.source,
                    relevance_pct,
                    eq_label,
                    eq_score * 100.0,
                    date_str
                ));

                // Full summary — NO truncation
                if let Some(ref summary) = ev.summary {
                    md.push_str(&format!("{}\n\n", summary));
                }

                // All key findings
                if !ev.key_findings.is_empty() {
                    md.push_str("**Key findings:**\n\n");
                    for f in &ev.key_findings {
                        md.push_str(&format!("- {}\n", f));
                    }
                    md.push_str("\n");
                }
            }
        } else {
            md.push_str(
                "_No evidence collected yet. Assign an agent to research this driver._\n\n",
            );
        }

        // Related evidence (mentions this driver but isn't directly linked)
        let related: Vec<_> = evidence_items
            .iter()
            .filter(|e| {
                !driver_evidence.iter().any(|de| de.id == e.id)
                    && (e
                        .summary
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&driver.name.to_lowercase())
                        || e.key_findings
                            .iter()
                            .any(|f| f.to_lowercase().contains(&driver.name.to_lowercase())))
            })
            .collect();

        if !related.is_empty() {
            md.push_str("### Related Evidence\n\n");
            for ev in &related {
                md.push_str(&format!(
                    "- **{}**: {}\n",
                    ev.source,
                    ev.summary.as_deref().unwrap_or("")
                ));
            }
            md.push_str("\n");
        }

        md.push_str("---\n\n");
    }

    // ── General Evidence (not linked to any driver) ───────────────
    let all_driver_agents: Vec<String> = agents
        .iter()
        .filter(|a| !a.driver_refs.is_empty())
        .map(|a| a.name.clone())
        .collect();
    let unassigned: Vec<_> = evidence_items
        .iter()
        .filter(|e| {
            !all_driver_agents
                .iter()
                .any(|a| evidence_matches_agent(e, a))
        })
        .collect();

    if !unassigned.is_empty() {
        md.push_str(&format!("## General Evidence ({})\n\n", unassigned.len()));
        md.push_str("_Evidence not linked to a specific driver._\n\n");
        for ev in &unassigned {
            let (eq_score, eq_label, _) = score_evidence_quality(ev);
            md.push_str(&format!(
                "### {} — relevance {:.0}% · quality {} ({:.0}%)\n\n",
                ev.source,
                ev.relevance.unwrap_or(0.0) * 100.0,
                eq_label,
                eq_score * 100.0
            ));
            // Full summary — NO truncation
            if let Some(ref summary) = ev.summary {
                md.push_str(&format!("{}\n\n", summary));
            }
            if !ev.key_findings.is_empty() {
                md.push_str("**Key findings:**\n\n");
                for f in &ev.key_findings {
                    md.push_str(&format!("- {}\n", f));
                }
                md.push_str("\n");
            }
        }
        md.push_str("---\n\n");
    }

    // ── Appendix: Model & Methodology ─────────────────────────────
    md.push_str("## Methodology\n\n");
    md.push_str("This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:\n\n");
    md.push_str("1. **Outside view** — anchor to a base rate from a relevant reference class\n");
    md.push_str("2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier\n");
    md.push_str("3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions\n");
    md.push_str(&format!(
        "4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]\n\n"
    ));

    // Model expression
    md.push_str("### Model\n\n");
    md.push_str("```\n");
    let model_parts: Vec<String> = drivers
        .iter()
        .map(|d| match d.driver_type {
            DriverType::Binary => {
                format!(
                    "(if {} then {:.1} else 1.0)",
                    d.name,
                    d.impact_multiplier.unwrap_or(1.3)
                )
            }
            _ => d.name.clone(),
        })
        .collect();
    if model_parts.is_empty() {
        md.push_str("# No drivers defined\n");
    } else {
        md.push_str(&format!("model: {}\n", model_parts.join(" * ")));
    }
    md.push_str("```\n\n");

    // Agent roster
    let active_agents: Vec<_> = agents
        .iter()
        .filter(|a| !a.driver_refs.is_empty())
        .collect();
    if !active_agents.is_empty() {
        md.push_str("### Research Agents\n\n");
        md.push_str("| Agent | Driver | Query |\n|---|---|---|\n");
        for a in &active_agents {
            let driver_list = a.driver_refs.join(", ");
            md.push_str(&format!(
                "| {} | {} | {} |\n",
                base_agent_name(&a.name),
                driver_list,
                a.query
            ));
        }
        md.push_str("\n");
    }

    md.push_str(&format!(
        "\n---\n\n_Generated by [Fermi Console](https://agent-bestiary.world) · v{} · {}_\n",
        version, timestamp
    ));

    md
}

/// Formulate a domain-specific research query for an agent+driver combination.
/// The query tells the agent exactly what data points to look for, includes the
/// current driver parameters (p50), and asks for a specific parameter adjustment.
fn formulate_research_query(
    question: &str,
    driver_display: &str,
    rationale: &str,
    agent_id: &str,
    domain: &str,
    p5: f64,
    p50: f64,
    p95: f64,
) -> String {
    let params = format!(
        "Current estimate: p5={:.2}, p50={:.2}, p95={:.2}",
        p5, p50, p95
    );

    match (domain, agent_id) {
        (d, "nba_analyst") if d.contains("nba") || d.contains("basketball") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze the '{driver_display}' driver.\n{params}\n\n\
             PROVIDE SPECIFIC DATA:\n\
             1. Current relevant stats (NetRtg, record, splits, trends)\n\
             2. Historical base rate for this factor (with sample size)\n\
             3. Elo-based adjustment or statistical impact estimate\n\
             4. Suggested p50 multiplier based on your findings\n\
             5. Confidence (0.0-1.0)\n\n\
             Context: {rationale}\n\n\
             Be quantitative — specific numbers, win rates, percentages."
        ),

        (_, "biotech_analyst") => format!(
            "For the forecast: \"{question}\"\n\n\
             Research the '{driver_display}' driver.\n{params}\n\n\
             Return findings using these labels:\n\
             [BASE RATE] phase + historical POS with sample size\n\
             [TRIAL DATA] specific endpoint result (n, p-value, comparator)\n\
             [FDA STATUS] current designation or action with date\n\
             [COMPETITIVE] competitor count, approval status, differentiation\n\
             [MECHANISTIC] biological plausibility with ontology IDs\n\
             [MULTIPLIER] Suggested p50: X.XX (p5: X.XX, p95: X.XX) — rationale\n\n\
             Context: {rationale}\n\
             Confidence (0.0-1.0) in your assessment."
        ),

        (d, "macro_forecaster") if d.contains("finance") || d.contains("stock") => format!(
            "For the forecast: \"{question}\"\n\n\
             Research the '{driver_display}' driver.\n{params}\n\n\
             PROVIDE:\n\
             1. Current value of the key metric for this driver\n\
             2. Historical trend (3-month, 12-month, relevant cycle)\n\
             3. Analyst consensus or market expectations\n\
             4. Comparable precedents with outcomes\n\
             5. Suggested p50 multiplier based on findings\n\n\
             Context: {rationale}\n\
             Be specific — include named sources, dates, dollar figures."
        ),

        (_, "sentiment_analyzer") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze sentiment around '{driver_display}'.\n\n\
             PROVIDE:\n\
             1. Sentiment classification (strongly bearish → strongly bullish)\n\
             2. Key narrative themes in recent coverage\n\
             3. Sentiment trend (improving/stable/deteriorating)\n\
             4. Expert vs public opinion divergence\n\
             5. How sentiment should adjust the probability\n\n\
             Context: {rationale}"
        ),

        (_, "equity_analyst") => format!(
            "For the forecast: \"{question}\"\n\n\
             Analyze the '{driver_display}' driver using live financial data.\n{params}\n\n\
             PULL FROM FMP API:\n\
             1. Company profile (price, market cap, sector, beta, 52-week range)\n\
             2. Income statement (revenue, margins, EPS for last 2-3 years)\n\
             3. Key ratios (P/E, P/B, P/S, EV/EBITDA, ROE, ROIC, debt/equity)\n\
             4. DCF intrinsic value vs current price\n\
             5. Analyst consensus estimates (revenue and EPS, low/avg/high)\n\n\
             THEN PROVIDE:\n\
             - Growth trajectory assessment (accelerating/stable/decelerating)\n\
             - Valuation assessment (undervalued/fair/overvalued with % gap)\n\
             - Suggested p50 multiplier based on financial data\n\
             - Confidence (0.0-1.0) in your assessment\n\n\
             Context: {rationale}\n\n\
             Ground every claim in specific numbers from FMP data."
        ),

        (_, "entity_investigator") => format!(
            "For the forecast: \"{question}\"\n\n\
             Investigate entities relevant to '{driver_display}'.\n\n\
             PROVIDE:\n\
             1. Key decision-makers and their positions\n\
             2. Organizational dynamics (strategy, leadership, M&A)\n\
             3. Financial health or resource position\n\
             4. Relationships and dependencies\n\
             5. How findings should adjust the probability\n\n\
             Context: {rationale}"
        ),

        // General fallback — works for any agent
        _ => format!(
            "For the forecast: \"{question}\"\n\n\
             Research evidence for the '{driver_display}' driver.\n{params}\n\n\
             PROVIDE:\n\
             1. Key data points relevant to this driver (with sources and dates)\n\
             2. Historical base rate or comparable precedent\n\
             3. Suggested p50 multiplier adjustment based on your findings\n\
             4. Confidence (0.0-1.0) in your assessment\n\n\
             Context: {rationale}\n\n\
             Be specific and quantitative — numbers, percentages, named sources."
        ),
    }
}

fn detect_domain(question: &str) -> String {
    let q = question.to_lowercase();

    // Sports — NBA / basketball (check BEFORE general sports)
    if q.contains("nba")
        || q.contains("lakers")
        || q.contains("celtics")
        || q.contains("knicks")
        || q.contains("warriors")
        || q.contains("nuggets")
        || q.contains("bucks")
        || q.contains("76ers")
        || q.contains("basketball")
        || (q.contains("playoff") && (q.contains("game") || q.contains("series")))
    {
        return "sports_nba".into();
    }

    // Sports — football / soccer
    if q.contains("champions league")
        || q.contains("premier league")
        || q.contains("world cup")
        || q.contains("euro 20")
        || q.contains("europa league")
        || q.contains("la liga")
        || q.contains("bundesliga")
        || q.contains("serie a")
        || q.contains("ligue 1")
        || q.contains("uefa")
        || q.contains("fifa")
        || q.contains("bayern")
        || q.contains("barcelona")
        || q.contains("real madrid")
        || q.contains("manchester")
        || q.contains("liverpool")
        || q.contains("arsenal")
        || q.contains("psg")
        || q.contains("juventus")
        || q.contains("inter milan")
        || q.contains("soccer")
        || q.contains("football") && !q.contains("nfl")
    {
        return "sports_football".into();
    }

    // Stocks / equity — specific company financial analysis
    if q.contains("stock price")
        || q.contains("share price")
        || q.contains("earnings per share")
        || q.contains("eps ")
        || q.contains("p/e ratio")
        || q.contains("dcf")
        || q.contains("intrinsic value")
        || q.contains("market cap")
        || q.contains("ipo")
        || q.contains("quarterly earnings")
        || q.contains("revenue beat")
        || q.contains("analyst estimate")
        || q.contains("price target")
        || q.contains("stock split")
        || (q.contains("valuation") && (q.contains("company") || q.contains("stock")))
    {
        return "stocks".into();
    }

    // Sports — NFL
    if q.contains("nfl")
        || q.contains("super bowl")
        || q.contains("touchdown")
        || q.contains("quarterback")
    {
        return "sports_nfl".into();
    }

    // Sports — general / other
    if q.contains("olympics")
        || q.contains("tennis")
        || q.contains("f1")
        || q.contains("formula 1")
        || q.contains("eurovision")
    {
        return "sports_other".into();
    }

    // Biotech / pharma
    if q.contains("fda")
        || q.contains("clinical trial")
        || q.contains("drug")
        || q.contains("pharma")
        || q.contains("biotech")
        || q.contains("approval")
            && (q.contains("drug") || q.contains("therapy") || q.contains("treatment"))
        || q.contains("phase 1")
        || q.contains("phase 2")
        || q.contains("phase 3")
        || q.contains("oncology")
        || q.contains("crispr")
        || q.contains("mrna")
    {
        return "biotech".into();
    }

    // Finance / stocks
    if q.contains("stock")
        || q.contains("share price")
        || q.contains("revenue")
        || q.contains("earnings")
        || q.contains("valuation")
        || q.contains("ipo")
        || q.contains("nasdaq")
        || q.contains("s&p")
        || q.contains("dow")
        || q.contains("market cap")
        || q.contains("dividend")
        || q.contains("quarterly")
    {
        return "finance".into();
    }

    // Politics / geopolitics
    if q.contains("election")
        || q.contains("vote")
        || q.contains("president")
        || q.contains("congress")
        || q.contains("senate")
        || q.contains("parliament")
        || q.contains("referendum")
        || q.contains("war")
        || q.contains("conflict")
        || q.contains("nato")
        || q.contains("sanctions")
        || q.contains("treaty")
    {
        return "politics".into();
    }

    // Technology
    if q.contains(" ai ")
        || q.contains("artificial intelligence")
        || q.contains("software")
        || q.contains("chip")
        || q.contains("semiconductor")
        || q.contains("quantum")
        || q.contains("spacex")
        || q.contains("satellite")
        || q.contains("autonomous")
        || q.contains("robotics")
    {
        return "technology".into();
    }

    // Climate / energy
    if q.contains("climate")
        || q.contains("carbon")
        || q.contains("emission")
        || q.contains("renewable")
        || q.contains("solar")
        || q.contains("wind power")
        || q.contains("nuclear") && q.contains("energy")
        || q.contains("fusion")
    {
        return "climate".into();
    }

    "general".into()
}

/// Generate a Fermi decomposition template for the given domain.
/// Returns (drivers, model_expression).
/// Drivers have <SPECIFY> equivalent (zero values) that the user or agent fills in.
fn generate_decomposition(_question: &str, domain: &str) -> (Vec<DriverStmt>, Option<Expression>) {
    // All templates use probability multipliers (values near 1.0).
    // The model is: driver_a * driver_b * ... producing a relative adjustment.
    // The simulation output gets normalized: P = base_rate × (mean / baseline)
    match domain {
        "finance" => {
            let drivers = vec![
                make_continuous_driver(
                    "fundamentals",
                    "Fundamentals Strength",
                    "multiplier",
                    0.7,
                    1.0,
                    1.4,
                    "How strong are the fundamentals relative to expectations? 1.0 = neutral",
                ),
                make_continuous_driver(
                    "market_conditions",
                    "Market Conditions",
                    "multiplier",
                    0.6,
                    1.0,
                    1.5,
                    "Favorable (>1) or unfavorable (<1) market environment",
                ),
                make_continuous_driver(
                    "momentum",
                    "Momentum Factor",
                    "multiplier",
                    0.8,
                    1.0,
                    1.3,
                    "Recent trend direction and strength. 1.0 = no trend",
                ),
                make_binary_driver(
                    "catalyst_event",
                    "Major Catalyst/Risk",
                    0.20,
                    1.4,
                    "Probability of a significant event that shifts the outcome",
                ),
            ];
            let model = Expression::Multiply(
                Box::new(Expression::Multiply(
                    Box::new(Expression::Multiply(
                        Box::new(Expression::Identifier("fundamentals".into())),
                        Box::new(Expression::Identifier("market_conditions".into())),
                    )),
                    Box::new(Expression::Identifier("momentum".into())),
                )),
                Box::new(Expression::If {
                    condition: Box::new(Expression::Identifier("catalyst_event".into())),
                    then_expr: Box::new(Expression::Number(1.4)),
                    else_expr: Box::new(Expression::Number(1.0)),
                }),
            );
            (drivers, Some(model))
        }
        "technology" => {
            let drivers = vec![
                make_continuous_driver(
                    "feasibility",
                    "Technical Feasibility",
                    "multiplier",
                    0.5,
                    1.0,
                    1.3,
                    "How feasible is the technical achievement? 1.0 = expected",
                ),
                make_continuous_driver(
                    "adoption",
                    "Adoption Likelihood",
                    "multiplier",
                    0.6,
                    1.0,
                    1.5,
                    "Market readiness and adoption potential. 1.0 = baseline",
                ),
                make_binary_driver(
                    "regulatory_block",
                    "Regulatory Blocker",
                    0.25,
                    0.5,
                    "Probability of regulatory action that halves the outcome",
                ),
            ];
            let model = Expression::Multiply(
                Box::new(Expression::Multiply(
                    Box::new(Expression::Identifier("feasibility".into())),
                    Box::new(Expression::Identifier("adoption".into())),
                )),
                Box::new(Expression::If {
                    condition: Box::new(Expression::Identifier("regulatory_block".into())),
                    then_expr: Box::new(Expression::Number(0.5)),
                    else_expr: Box::new(Expression::Number(1.0)),
                }),
            );
            (drivers, Some(model))
        }
        _ => {
            let drivers = vec![
                make_continuous_driver(
                    "strength_factor",
                    "Strength of Case",
                    "multiplier",
                    0.5,
                    1.0,
                    1.5,
                    "How strong is the case for this outcome? 1.0 = neutral",
                ),
                make_continuous_driver(
                    "conditions",
                    "Favorable Conditions",
                    "multiplier",
                    0.7,
                    1.0,
                    1.3,
                    "Are conditions favorable (>1) or unfavorable (<1)?",
                ),
                make_binary_driver(
                    "disruption",
                    "Disruption Event",
                    0.15,
                    1.5,
                    "Probability of a disruptive event that amplifies the outcome",
                ),
            ];
            let model = Expression::Multiply(
                Box::new(Expression::Multiply(
                    Box::new(Expression::Identifier("strength_factor".into())),
                    Box::new(Expression::Identifier("conditions".into())),
                )),
                Box::new(Expression::If {
                    condition: Box::new(Expression::Identifier("disruption".into())),
                    then_expr: Box::new(Expression::Number(1.5)),
                    else_expr: Box::new(Expression::Number(1.0)),
                }),
            );
            (drivers, Some(model))
        }
    }
}

fn make_continuous_driver(
    name: &str,
    display: &str,
    unit: &str,
    p5: f64,
    p50: f64,
    p95: f64,
    rationale: &str,
) -> DriverStmt {
    DriverStmt {
        name: name.to_string(),
        display_name: Some(display.to_string()),
        description: Some(rationale.to_string()),
        driver_type: DriverType::Continuous,
        distribution: Some(Distribution::Triangular {
            p5: Expression::Number(p5),
            p50: Expression::Number(p50),
            p95: Expression::Number(p95),
        }),
        probability: None,
        impact_multiplier: None,
        values: None,
        weights: None,
        unit: if unit.is_empty() {
            None
        } else {
            Some(unit.to_string())
        },
        rationale: Some(rationale.to_string()),
        constraints: vec![],
        evidence_refs: vec![],
        learnable: false,
        feeds_from: None,
    }
}

fn make_binary_driver(
    name: &str,
    display: &str,
    prob: f64,
    impact: f64,
    rationale: &str,
) -> DriverStmt {
    DriverStmt {
        name: name.to_string(),
        display_name: Some(display.to_string()),
        description: Some(rationale.to_string()),
        driver_type: DriverType::Binary,
        distribution: None,
        probability: Some(prob),
        impact_multiplier: Some(impact),
        values: None,
        weights: None,
        unit: None,
        rationale: Some(rationale.to_string()),
        constraints: vec![],
        evidence_refs: vec![],
        learnable: false,
        feeds_from: None,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

/// Clean a string for safe embedding in FPL string literals.
/// Removes/escapes characters that would break the parser.
/// Extract the base agent name from a compound agent_driver name.
/// e.g. "market_research_song_quality" → "market_research"
/// Parse a narrative LLM response to extract base rate and driver decomposition.
///
/// ABW's LLMExecutor.build_prompt() wraps queries with its own JSON format,
/// causing the LLM to return narrative text like "The base rate of 35% from
/// NBA home games..." instead of structured JSON. This function extracts
/// the structured data from that narrative.
///
/// Returns a synthetic JSON value matching the decomposition schema if
/// a base rate can be extracted, or None if the text isn't parseable.
fn parse_narrative_decomposition(text: &str) -> Option<serde_json::Value> {
    if text.len() < 50 {
        return None;
    }

    // ── Extract base rate ─────────────────────────────────────────
    // Look for patterns like:
    //   "base rate of 35%"
    //   "base rate: 0.35"
    //   "35% base rate"
    //   "historical frequency of 42%"
    //   "base rate of 17.1% from last season"
    let text_lower = text.to_lowercase();
    let mut base_rate: Option<f64> = None;
    let mut reference_class = String::new();

    // Pattern 1: "base rate of X%" or "base rate of 0.X"
    for pattern in &[
        "base rate of ",
        "base rate: ",
        "base_rate: ",
        "historical frequency of ",
    ] {
        if let Some(pos) = text_lower.find(pattern) {
            let after = &text[pos + pattern.len()..];
            if let Some(val) = extract_percentage_or_decimal(after) {
                base_rate = Some(val);
                // Try to get context as reference class
                let start = pos.saturating_sub(80);
                let context = &text[start..pos + pattern.len() + 20.min(after.len())];
                reference_class = context.trim().to_string();
                break;
            }
        }
    }

    // Pattern 2: "X% base rate" or "X% from"
    if base_rate.is_none() {
        let words: Vec<&str> = text.split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            if word.ends_with('%') {
                if let Ok(pct) = word.trim_end_matches('%').parse::<f64>() {
                    // Check if next words are "base rate" or if this is near "base rate"
                    let window = words[i.saturating_sub(3)..words.len().min(i + 5)]
                        .join(" ")
                        .to_lowercase();
                    if window.contains("base rate")
                        || window.contains("base_rate")
                        || window.contains("historical")
                        || window.contains("win rate")
                        || window.contains("success rate")
                        || window.contains("approval rate")
                    {
                        base_rate = Some(pct / 100.0);
                        reference_class = window;
                        break;
                    }
                }
            }
        }
    }

    // If no base rate found, can't decompose
    let freq = base_rate?;
    if freq <= 0.0 || freq >= 1.0 {
        // Try to recover — maybe it was given as percentage > 1
        if freq > 1.0 && freq <= 100.0 {
            // was given as 35 not 0.35
        } else {
            return None;
        }
    }
    let freq = if freq > 1.0 { freq / 100.0 } else { freq };

    // ── Extract reasoning (use full narrative) ────────────────────
    let reasoning_text = text.to_string();

    // ── Build drivers by parsing the LLM's actual text ────────────
    // The LLM consistently enumerates drivers in patterns like:
    //   "(1) current form, (2) squad quality, (3) tactical approach"
    //   "drivers: injury impact, competitive pressure, draw luck"
    //   "six independent drivers capture: form, quality, tactics..."
    let mut drivers = Vec::new();
    let mut extracted_names: Vec<String> = Vec::new();

    // Strategy 1: Look for numbered items "(1) phrase" or "(2) phrase"
    {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i < chars.len().saturating_sub(4) {
            let is_paren_num = chars[i] == '('
                && chars
                    .get(i + 1)
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false);
            if is_paren_num {
                if let Some(close) = chars[i..].iter().position(|&c| c == ')') {
                    let after_idx = i + close + 1;
                    let rest: String = chars[after_idx..].iter().collect();
                    let rest = rest.trim_start();
                    let phrase: String = rest
                        .chars()
                        .take_while(|&c| c != ',' && c != ';' && c != '(' && c != '\n')
                        .collect();
                    let phrase = phrase.trim().trim_end_matches('.').trim();
                    if phrase.len() > 2 && phrase.len() < 60 {
                        extracted_names.push(phrase.to_string());
                    }
                }
            }
            i += 1;
        }
    }

    // Strategy 2: Look for "drivers:" or "factors:" followed by comma list
    if extracted_names.len() < 3 {
        let triggers = [
            "drivers:",
            "factors:",
            "drivers capture",
            "adjusted for:",
            "adjustments:",
            "key uncertainties:",
            "key drivers:",
            "independent drivers",
            "five drivers",
            "four drivers",
            "three drivers",
            "six drivers",
        ];
        for trigger in &triggers {
            if let Some(pos) = text_lower.find(trigger) {
                let after = &text[pos + trigger.len()..];
                let chunk: String = after.chars().take(300).collect();
                for part in chunk.split(|c: char| c == ',' || c == ';') {
                    let clean = part
                        .trim()
                        .trim_start_matches("and ")
                        .trim_start_matches('(')
                        .trim_end_matches(')')
                        .trim_end_matches('.')
                        .trim();
                    if clean.len() > 2
                        && clean.len() < 60
                        && !clean.contains("base rate")
                        && !clean.starts_with("The ")
                        && !clean.starts_with("which ")
                    {
                        // Avoid duplicates
                        if !extracted_names
                            .iter()
                            .any(|e| e.to_lowercase() == clean.to_lowercase())
                        {
                            extracted_names.push(clean.to_string());
                        }
                    }
                }
                if extracted_names.len() >= 3 {
                    break;
                }
            }
        }
    }

    // Convert extracted phrases to driver structs
    for phrase in extracted_names.iter().take(6) {
        let snake: String = phrase
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>()
            .join("_");

        if snake.len() < 3 {
            continue;
        }
        if drivers
            .iter()
            .any(|d: &serde_json::Value| d.get("name").and_then(|v| v.as_str()) == Some(&snake))
        {
            continue;
        }

        // Try to find a multiplier value near this phrase
        let multiplier = extract_multiplier_near_keyword(&text_lower, &phrase.to_lowercase());
        let (p5, p50, p95) = match multiplier {
            Some(m) => (
                (m * 0.85 * 100.0).round() / 100.0,
                (m * 100.0).round() / 100.0,
                (m * 1.15 * 100.0).round() / 100.0,
            ),
            None => (0.85, 1.0, 1.15),
        };

        // Title-case display name
        let display_name: String = phrase
            .split_whitespace()
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().to_string() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        drivers.push(serde_json::json!({
            "name": snake,
            "display_name": display_name,
            "type": "continuous",
            "p5": p5,
            "p50": p50,
            "p95": p95,
            "unit": "multiplier",
            "rationale": phrase,
        }));
    }

    // Only add generic fallbacks if we truly found nothing
    if drivers.is_empty() {
        let generic = [
            ("factor_1", "Primary Factor", "Main driver of the outcome"),
            ("factor_2", "Secondary Factor", "Supporting factor"),
            ("factor_3", "Risk Factor", "Key risk or uncertainty"),
        ];
        for (name, display, rationale) in &generic {
            drivers.push(serde_json::json!({
                "name": name,
                "display_name": display,
                "type": "continuous",
                "p5": 0.8,
                "p50": 1.0,
                "p95": 1.2,
                "unit": "multiplier",
                "rationale": rationale,
            }));
        }
    }

    // Build model expression
    let model_parts: Vec<String> = drivers
        .iter()
        .filter_map(|d| {
            d.get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    let model_expr = if model_parts.is_empty() {
        format!("{}", freq)
    } else {
        model_parts.join(" * ")
    };

    Some(serde_json::json!({
        "base_rate": {
            "reference_class": reference_class,
            "historical_frequency": freq,
            "sample_size": null,
            "reasoning": reasoning_text.chars().take(500).collect::<String>(),
        },
        "drivers": drivers,
        "evidence": [{
            "source": "Fermi decomposition (parsed from narrative)",
            "summary": reasoning_text,
            "key_findings": [],
            "relevance": 0.7,
        }],
        "model_expression": model_expr,
        "confidence": 0.5,
        "reasoning": reasoning_text,
    }))
}

/// Extract a percentage (e.g., "35%") or decimal (e.g., "0.35") from the start of a string.
fn extract_percentage_or_decimal(s: &str) -> Option<f64> {
    let s = s.trim();
    // Try "35%" or "35.7%"
    if let Some(pct_pos) = s.find('%') {
        let num_str: String = s[..pct_pos]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Ok(pct) = num_str.parse::<f64>() {
            return Some(pct / 100.0);
        }
    }
    // Try "0.35" or ".35"
    let num_str: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if let Ok(val) = num_str.parse::<f64>() {
        if val > 0.0 && val < 1.0 {
            return Some(val);
        }
        if val >= 1.0 && val <= 100.0 {
            return Some(val / 100.0);
        }
    }
    None
}

/// Try to extract a multiplier value near a keyword in the text.
/// Looks for patterns like "0.85x", "×0.9", "1.2x", "(0.85)" near the keyword.
fn extract_multiplier_near_keyword(text: &str, keyword: &str) -> Option<f64> {
    if let Some(pos) = text.find(keyword) {
        // Search in a window around the keyword
        let start = pos.saturating_sub(40);
        let end = (pos + keyword.len() + 60).min(text.len());
        let window = &text[start..end];

        // Look for multiplier patterns: 0.85, 1.2, ×1.1, (0.9)
        for word in window.split_whitespace() {
            let clean = word
                .trim_matches(|c: char| c == '(' || c == ')' || c == '×' || c == 'x' || c == ',');
            if let Ok(val) = clean.parse::<f64>() {
                if val > 0.3 && val < 3.0 && val != 1.0 {
                    return Some(val);
                }
            }
        }
    }
    None
}

fn base_agent_name(compound_name: &str) -> &str {
    // Known agent base names — covers all curated agents.
    // The compound agent name format is "{base_id}_{driver_name}".
    // We match against known base IDs to extract the base portion.
    let known = [
        "macro_forecaster",
        "market_research",
        "sentiment_analyzer",
        "entity_investigator",
        "monte_carlo_sim",
        "equity_analyst",
        "biotech_analyst",
        "nba_analyst",
        "football_analyst",
        "energy_advisor",
        "comparator",
        "performance_coach",
        "social_media_studio",
        "simops_advisor",
        "simops_optimizer",
        "simops_cascade",
        "simops_narrator_local",
        "valuechain_mapper",
        "ar_cartographer",
        "ar_choreographer",
        "wild_companion",
        "keeper",
        "reynolds_flock",
        "embedding_broker",
        "coherence_evaluator",
        "publish_coach",
        "sensor_advisor",
        "intention_coordinator",
        "rabble_anchor_manager",
        "fermi",
    ];
    // Check longest matches first (some names are prefixes of others)
    let mut best: &str = compound_name;
    let mut best_len = 0;
    for base in &known {
        if compound_name.starts_with(base) && base.len() > best_len {
            best = base;
            best_len = base.len();
        }
    }
    best
}

/// Check if an evidence item is linked to an agent (by base name match).
/// Extract a suggested p50 value from agent output text.
/// Scans for patterns like "Suggested p50: 1.15", "p50 multiplier: 0.95", etc.
fn extract_suggested_p50(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    let patterns = [
        "suggested p50",
        "p50 multiplier",
        "recommended p50",
        "new p50",
        "adjust p50 to",
        "p50 value",
        "p50 of",
        "suggested multiplier",
    ];
    for pattern in &patterns {
        if let Some(pos) = lower.find(pattern) {
            let after = &text[pos + pattern.len()..];
            let num_str: String = after
                .chars()
                .skip_while(|c| !c.is_ascii_digit() && *c != '.' && *c != '-')
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(val) = num_str.parse::<f64>() {
                // Sanity check: p50 multipliers are typically 0.01–50.0
                if val > 0.01 && val < 50.0 {
                    return Some(val);
                }
            }
        }
    }
    None
}

/// Score evidence quality based on specificity, source, findings count, and relevance.
/// Returns (score 0.0–1.0, label, theme color).
fn score_evidence_quality(ev: &EvidenceStmt) -> (f64, &'static str, u32) {
    let text = format!(
        "{} {}",
        ev.summary.as_deref().unwrap_or(""),
        ev.key_findings.join(" ")
    );

    let mut score = 0.0;

    // Specificity: numbers, percentages, dollar signs indicate quantitative evidence
    if text.chars().any(|c| c.is_ascii_digit()) {
        score += 0.15;
    }
    if text.contains('%') {
        score += 0.1;
    }
    if text.contains('$') || text.contains("USD") || text.contains("revenue") {
        score += 0.05;
    }

    // Source quality
    let src = &ev.source;
    if src.contains("http")
        || src.contains("Bloomberg")
        || src.contains("Reuters")
        || src.contains("API")
        || src.contains("ESPN")
        || src.contains("PubMed")
    {
        score += 0.2;
    } else if src == "Manual entry" || src == "Mock Executor" {
        score += 0.05;
    } else {
        score += 0.1;
    }

    // Findings richness (more findings = more comprehensive)
    let n = ev.key_findings.len();
    score += (n.min(5) as f64) * 0.04;

    // Relevance (from the evidence's own relevance field)
    score += ev.relevance.unwrap_or(0.5) * 0.3;

    let score = score.min(1.0);

    let (label, color) = if score >= 0.7 {
        ("●●● High", theme::GREEN)
    } else if score >= 0.4 {
        ("●●○ Med", theme::GOLD)
    } else {
        ("●○○ Low", theme::RED)
    };

    (score, label, color)
}

/// Filter for team dropdowns in the forecast Access tab: only true
/// collaboration teams should appear as share targets, not the ABW
/// workspace wrappers auto-created for every Team-Prior /
/// Tournament-Path forecast (spawn_forecast_workspace creates one per
/// workspace so shares can bind to it — 62+ entries for the WC event).
///
/// Detection is the same shape as main.rs::is_workspace_prior_team —
/// duplicated intentionally so cockpit.rs doesn't need to reach into
/// binary-crate helpers. Kept in sync with that function.
fn is_forecast_collaboration_team(t: &Team) -> bool {
    // Only the fermi vertical — skip rabble/kask/etc.
    let fermi_vertical = match t.origin.as_deref() {
        Some(o) => o == "fermi_forecast",
        None => true, // legacy rows without an origin — assume fermi
    };
    if !fermi_vertical {
        return false;
    }
    // Filter out workspace-prior wrappers by name / slug / description.
    if t.name.starts_with("Team Prior — ")
        || t.name.starts_with("Team Prior - ")
        || t.name.starts_with("Tournament Path — ")
        || t.name.starts_with("Tournament Path - ")
    {
        return false;
    }
    let slug = t.slug.to_ascii_lowercase();
    if let Some(tail) = slug.strip_prefix("fermi-forecast-") {
        if tail.len() >= 6 && tail.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    let desc = t.description.as_deref().unwrap_or("").to_ascii_lowercase();
    if desc.contains("tournament win probability prior") || desc.contains("auto-created workspace")
    {
        return false;
    }
    true
}

fn evidence_matches_agent(evidence: &EvidenceStmt, agent_name: &str) -> bool {
    let base = base_agent_name(agent_name);
    // Evidence IDs are formatted as "{base_agent_id}_{N}" (e.g. "market_research_0")
    // Agent statement names are "{base_agent_id}_{driver_name}" (e.g. "market_research_economic_crisis")
    // Match by:
    // 1. Evidence ID starts with the base agent name followed by "_" (most reliable)
    // 2. Evidence source text contains the base agent name
    // 3. Evidence ID contains the full compound agent name
    evidence.id.starts_with(&format!("{}_", base))
        || evidence.source.contains(base)
        || evidence.id.contains(agent_name)
}

fn clean_fpl_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', "")
        .replace('`', "'")
        .chars()
        .take(500) // truncate very long strings
        .collect()
}

/// Parse a bound AST agent name back to its base registry id.
///
/// `assign_agent_to_driver` constructs the bound name as
/// `<base_agent_id>_<sanitize_name(driver_name)>`. This is the inverse:
/// strip the suffix to recover `<base_agent_id>`. Falls back to the bound
/// name unchanged when the suffix doesn't match (e.g. agents added by a
/// path that doesn't follow the convention).
fn base_agent_id_for_bound(bound_name: &str, driver_name: &str) -> String {
    let suffix = format!("_{}", sanitize_name(driver_name));
    bound_name
        .strip_suffix(&suffix)
        .map(|s| s.to_string())
        .unwrap_or_else(|| bound_name.to_string())
}

fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("d_{}", s)
    } else if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

fn expr_to_f64(expr: &Expression) -> f64 {
    match expr {
        Expression::Number(n) => *n,
        _ => 0.0,
    }
}

/// Best-effort "what's the center of this distribution?" without binding any
/// per-team params. Returns 1.0 for any distribution whose center is an
/// expression (rather than a literal), which is what the WC team_prior
/// uses for socio_capital and dynamic_performance — without a fallback
/// the suggestion-comparison divides by zero and every suggestion either
/// gets filtered out or compared against a meaningless anchor.
///
/// 1.0 is a reasonable default for the team_prior because the
/// multiplicative model is calibrated so each driver lands near 1.0 for an
/// average team. For other templates it's still a usable anchor — the
/// suggestion's reasoning text carries the real story; the numeric anchor
/// is just for display.
fn distribution_center_or_default(dist: &Distribution) -> f64 {
    match dist {
        Distribution::Triangular { p50, .. } => match p50 {
            Expression::Number(n) => *n,
            _ => 1.0,
        },
        Distribution::Normal { mean, .. } => match mean {
            Expression::Number(n) => *n,
            _ => 1.0,
        },
        Distribution::Lognormal { median, .. } => match median {
            Expression::Number(n) => *n,
            _ => 1.0,
        },
        Distribution::Uniform { low, high } => match (low, high) {
            (Expression::Number(l), Expression::Number(h)) => (l + h) / 2.0,
            _ => 1.0,
        },
        Distribution::Beta { .. } => 0.5,
    }
}

fn has_placeholder(dist: &Distribution) -> bool {
    match dist {
        Distribution::Triangular { p5, p50, p95 } => {
            expr_to_f64(p5) == 0.0 && expr_to_f64(p50) == 0.0 && expr_to_f64(p95) == 0.0
        }
        _ => false,
    }
}

fn agent_result_to_json(result: &AgentExecutionResult) -> JsonValue {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "agent_id".into(),
        JsonValue::String(result.agent_id.clone()),
    );
    obj.insert("status".into(), JsonValue::String(result.status.clone()));
    if let Some(ref evidence) = result.evidence {
        obj.insert("evidence".into(), JsonValue::Array(evidence.clone()));
    }
    if let Some(confidence) = result.confidence {
        obj.insert("confidence".into(), serde_json::json!(confidence));
    }
    if let Some(time_ms) = result.execution_time_ms {
        obj.insert("execution_time_ms".into(), serde_json::json!(time_ms));
    }
    if let Some(tokens) = result.tokens_used {
        obj.insert("tokens_used".into(), serde_json::json!(tokens));
    }
    if let Some(credits) = result.credits_charged {
        obj.insert("credits_charged".into(), serde_json::json!(credits));
    }
    if let Some(ref metadata) = result.metadata {
        obj.insert("metadata".into(), metadata.clone());
    }
    JsonValue::Object(obj)
}
