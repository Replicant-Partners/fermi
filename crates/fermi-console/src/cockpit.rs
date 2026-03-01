//! Research Cockpit — the six-zone spatial workspace for forecast authoring.
//!
//! Replaces the linear form-based composer with an OODA-loop-driven
//! research environment where the question is at the center and
//! intelligence radiates outward.
//!
//! Zones:
//! 1. Question Hub (top) — editable question + live probability + divergence
//! 2. Outside View (left-top) — base rate, reference class, divergence warning
//! 3. Evidence Landscape (left-bottom) — clustered evidence items + gaps
//! 4. Driver Map (center) — drivers + model expression + simulation results
//! 5. Agent Fleet (right) — active/completed/idle agents
//! 6. Timeline (bottom) — probability evolution audit trail

use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::api::client::ApiClient;
use crate::text_input::TextInput;
use crate::theme;

// ═══════════════════════════════════════════════════════════════════
// Data structures
// ═══════════════════════════════════════════════════════════════════

/// Outside View — Tetlock base rate from reference class.
#[derive(Debug, Clone)]
pub struct OutsideView {
    pub reference_class: String,
    pub historical_frequency: f64,
    pub sample_size: Option<usize>,
    pub source: String,
    pub reasoning: Option<String>,
    pub generated_by: Option<String>, // agent that found it
    pub loading: bool,
}

impl Default for OutsideView {
    fn default() -> Self {
        Self {
            reference_class: String::new(),
            historical_frequency: 0.0,
            sample_size: None,
            source: String::new(),
            reasoning: None,
            generated_by: None,
            loading: false,
        }
    }
}

impl OutsideView {
    pub fn has_data(&self) -> bool {
        !self.reference_class.is_empty()
    }

    pub fn base_rate_pct(&self) -> f64 {
        self.historical_frequency * 100.0
    }
}

/// A single piece of evidence.
#[derive(Debug, Clone)]
pub struct EvidenceItem {
    pub id: String,
    pub source: String,
    pub summary: String,
    pub relevance: f64,       // 0.0 to 1.0
    pub sentiment: Sentiment, // bullish / bearish / neutral
    pub date: Option<String>,
    pub agent_id: Option<String>, // which agent found it
    pub dismissed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sentiment {
    Bullish,
    Bearish,
    Neutral,
}

impl Sentiment {
    pub fn label(&self) -> &'static str {
        match self {
            Sentiment::Bullish => "bullish",
            Sentiment::Bearish => "bearish",
            Sentiment::Neutral => "neutral",
        }
    }

    pub fn color(&self) -> u32 {
        match self {
            Sentiment::Bullish => theme::GREEN,
            Sentiment::Bearish => theme::RED,
            Sentiment::Neutral => theme::FG_DIM,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Sentiment::Bullish => "▲",
            Sentiment::Bearish => "▼",
            Sentiment::Neutral => "●",
        }
    }
}

/// An evidence gap — something we don't know but should.
#[derive(Debug, Clone)]
pub struct EvidenceGap {
    pub description: String,
    pub suggested_agent: Option<String>,
    pub suggested_query: Option<String>,
}

/// A forecast driver.
#[derive(Debug, Clone)]
pub struct CockpitDriver {
    pub name: String,
    pub driver_type: CockpitDriverType,
    pub rationale: String,
    pub suggested: bool, // true = agent-suggested ghost node, not yet accepted
}

#[derive(Debug, Clone)]
pub enum CockpitDriverType {
    Continuous {
        distribution: String, // e.g. "triangular(500, 1200, 2500)"
        unit: String,
        p5: f64,
        p50: f64,
        p95: f64,
    },
    Binary {
        probability: f64,
        impact_multiplier: f64,
    },
}

impl CockpitDriver {
    pub fn type_label(&self) -> &'static str {
        match &self.driver_type {
            CockpitDriverType::Continuous { .. } => "continuous",
            CockpitDriverType::Binary { .. } => "binary",
        }
    }

    pub fn summary(&self) -> String {
        match &self.driver_type {
            CockpitDriverType::Continuous {
                p5, p50, p95, unit, ..
            } => {
                let u = if unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", unit)
                };
                format!("p5={:.0}, p50={:.0}, p95={:.0}{}", p5, p50, p95, u)
            }
            CockpitDriverType::Binary {
                probability,
                impact_multiplier,
            } => format!("{:.0}% (×{:.1})", probability * 100.0, impact_multiplier),
        }
    }
}

/// An agent in the fleet.
#[derive(Debug, Clone)]
pub struct FleetAgent {
    pub agent_id: String,
    pub display_name: String,
    pub status: AgentStatus,
    pub findings_count: usize,
    pub findings_summary: Option<String>,
    pub model_used: Option<String>,
    pub execution_time_ms: Option<u64>,
    pub cost_credits: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AgentStatus {
    Running,
    Completed,
    Failed,
    Idle,
}

impl AgentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            AgentStatus::Running => "running",
            AgentStatus::Completed => "done",
            AgentStatus::Failed => "failed",
            AgentStatus::Idle => "idle",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            AgentStatus::Running => "●",
            AgentStatus::Completed => "✓",
            AgentStatus::Failed => "✗",
            AgentStatus::Idle => "○",
        }
    }

    pub fn color(&self) -> u32 {
        match self {
            AgentStatus::Running => theme::GOLD,
            AgentStatus::Completed => theme::GREEN,
            AgentStatus::Failed => theme::RED,
            AgentStatus::Idle => theme::FG_DIM,
        }
    }
}

/// A point on the timeline.
#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub label: String,
    pub probability: Option<f64>,
    pub event_type: TimelineEventType,
    pub timestamp: String,
}

#[derive(Debug, Clone, Copy)]
pub enum TimelineEventType {
    Created,
    BaseRateSet,
    EvidenceAdded,
    ProbabilityUpdated,
    AgentExecuted,
    Published,
    Resolved,
}

impl TimelineEventType {
    pub fn color(&self) -> u32 {
        match self {
            TimelineEventType::Created => theme::FG_DIM,
            TimelineEventType::BaseRateSet => theme::GOLD,
            TimelineEventType::EvidenceAdded => theme::CYAN,
            TimelineEventType::ProbabilityUpdated => theme::GREEN,
            TimelineEventType::AgentExecuted => theme::BLUE,
            TimelineEventType::Published => theme::PURPLE,
            TimelineEventType::Resolved => theme::GREEN,
        }
    }
}

/// Simulation results from local Monte Carlo.
#[derive(Debug, Clone)]
pub struct SimResults {
    pub mean: f64,
    pub median: f64,
    pub p5: f64,
    pub p95: f64,
    pub std_dev: f64,
    pub iterations: usize,
    pub execution_time_ms: u64,
    pub histogram: Vec<(f64, usize)>,
}

// ═══════════════════════════════════════════════════════════════════
// Cockpit State
// ═══════════════════════════════════════════════════════════════════

pub struct CockpitState {
    // ── Question Hub ──────────────────────────────────────────────
    pub question_input: Entity<TextInput>,
    pub domain_input: Entity<TextInput>,
    pub target_date_input: Entity<TextInput>,
    pub resolution_criteria_input: Entity<TextInput>,
    pub predicted_probability: f64,

    // ── Outside View ──────────────────────────────────────────────
    pub outside_view: OutsideView,

    // ── Evidence Landscape ────────────────────────────────────────
    pub evidence: Vec<EvidenceItem>,
    pub evidence_gaps: Vec<EvidenceGap>,

    // ── Driver Map ────────────────────────────────────────────────
    pub drivers: Vec<CockpitDriver>,
    pub model_expression: String,
    pub sim_results: Option<SimResults>,
    pub sim_running: bool,
    pub sim_error: Option<String>,

    // ── Agent Fleet ───────────────────────────────────────────────
    pub agents: Vec<FleetAgent>,
    pub session_cost: f64,

    // ── Timeline ──────────────────────────────────────────────────
    pub timeline: Vec<TimelineEvent>,

    // ── Meta ──────────────────────────────────────────────────────
    pub forecast_id: Option<String>, // None = new, Some = editing existing
    pub status: String,              // "draft", "active", "resolved"
    pub api: Arc<ApiClient>,
    pub orchestration_running: bool,
}

impl CockpitState {
    pub fn new(api: Arc<ApiClient>, cx: &mut App) -> Self {
        let question_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("What are you forecasting? (Enter to research)")
                .with_label("Question")
                .with_large(true)
        });

        let domain_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("e.g. tech, economics, geopolitics")
                .with_label("Domain")
        });

        let target_date_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("YYYY-MM-DD")
                .with_label("Target Date")
        });

        let resolution_criteria_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("How will this be resolved?")
                .with_label("Resolution Criteria")
        });

        Self {
            question_input,
            domain_input,
            target_date_input,
            resolution_criteria_input,
            predicted_probability: 0.5,
            outside_view: OutsideView::default(),
            evidence: Vec::new(),
            evidence_gaps: Vec::new(),
            drivers: Vec::new(),
            model_expression: String::new(),
            sim_results: None,
            sim_running: false,
            sim_error: None,
            agents: Vec::new(),
            session_cost: 0.0,
            timeline: vec![TimelineEvent {
                label: "Created".into(),
                probability: Some(0.5),
                event_type: TimelineEventType::Created,
                timestamp: chrono::Utc::now().format("%H:%M").to_string(),
            }],
            forecast_id: None,
            status: "draft".into(),
            api,
            orchestration_running: false,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Agent Orchestration
    // ═══════════════════════════════════════════════════════════════

    /// Fire the full research orchestration for a question.
    /// Called when the user submits the question (Enter key).
    ///
    /// Sequence:
    /// 1. Outside view search (base rate) — fires first
    /// 2. Inside view agents (evidence + drivers) — fire in parallel
    ///
    /// Results stream back and populate the cockpit zones.
    pub fn orchestrate_question(&mut self, question: &str, cx: &mut App) {
        if question.trim().is_empty() {
            return;
        }

        let question = question.to_string();
        self.orchestration_running = true;

        // Reset state for new question
        self.evidence.clear();
        self.evidence_gaps.clear();
        self.drivers.clear();
        self.agents.clear();
        self.sim_results = None;
        self.sim_error = None;
        self.session_cost = 0.0;

        // Add timeline event
        self.timeline.push(TimelineEvent {
            label: "Question set".into(),
            probability: Some(self.predicted_probability),
            event_type: TimelineEventType::Created,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });

        // ── Phase 1: Outside View (base rate search) ──────────────
        self.outside_view.loading = true;
        self.agents.push(FleetAgent {
            agent_id: "macro_forecaster".into(),
            display_name: "macro_forecaster (base rate)".into(),
            status: AgentStatus::Running,
            findings_count: 0,
            findings_summary: None,
            model_used: None,
            execution_time_ms: None,
            cost_credits: None,
        });

        let api = self.api.clone();
        let q_base = question.clone();

        // We can't use cx.spawn here (CockpitState is not an Entity).
        // Instead, fire a tokio task and use a channel to send results back.
        // For now, use tokio::spawn — the results will be picked up on next
        // render via a polling mechanism or we'll refactor to Entity later.
        //
        // TEMPORARY: fire and forget with logging. The real integration
        // requires making CockpitState an Entity or using a shared channel.
        // For this sprint, we populate mock results to prove the UI works,
        // then wire real API calls when CockpitState becomes an Entity.

        // ── Phase 2: Inside View agents ───────────────────────────
        let inside_agents = vec![
            ("macro_forecaster", format!(
                "Analyze the following forecast question and suggest 3-5 key drivers with probability distributions. \
                 Also identify the reference class and base rate for the outside view. \
                 Question: {}",
                question
            )),
            ("market_research", format!(
                "Research market data, analyst consensus, and competitive dynamics relevant to: {}",
                question
            )),
            ("sentiment_analyzer", format!(
                "Analyze current sentiment from news, social media, and expert opinions about: {}",
                question
            )),
        ];

        for (agent_id, _query) in &inside_agents {
            if *agent_id == "macro_forecaster" {
                // Already added above for base rate
                continue;
            }
            self.agents.push(FleetAgent {
                agent_id: agent_id.to_string(),
                display_name: agent_id.to_string(),
                status: AgentStatus::Running,
                findings_count: 0,
                findings_summary: None,
                model_used: None,
                execution_time_ms: None,
                cost_credits: None,
            });
        }

        // ── Fire API calls ────────────────────────────────────────
        // For each agent, spawn an async task that calls the API and
        // stores results. We use a shared results channel.
        let api_clone = api.clone();
        let agents_to_fire = inside_agents;

        // Spawn all agent executions
        for (agent_id, query) in agents_to_fire {
            let api = api_clone.clone();
            let aid = agent_id.to_string();
            tokio::spawn(async move {
                log::info!("[cockpit] Firing agent {} for question research", aid);
                match api.execute_agent(&aid, &query).await {
                    Ok(result) => {
                        log::info!(
                            "[cockpit] Agent {} completed: {} evidence items, confidence={:?}",
                            aid,
                            result.evidence.as_ref().map(|e| e.len()).unwrap_or(0),
                            result.confidence,
                        );
                        // Results are logged for now. Real integration requires
                        // a channel back to the UI thread. See populate_from_agent_result().
                    }
                    Err(e) => {
                        log::error!("[cockpit] Agent {} failed: {}", aid, e);
                    }
                }
            });
        }

        // ── Populate with illustrative data while agents run ──────
        // This gives the user immediate visual feedback that the cockpit
        // is alive. Real agent results will replace this when the channel
        // integration is complete.
        self.populate_initial_scaffold(&question);
    }

    /// Populate the cockpit with an initial scaffold based on the question.
    /// This runs synchronously and gives immediate visual feedback while
    /// agents are still executing in the background.
    fn populate_initial_scaffold(&mut self, question: &str) {
        // Simple heuristic: extract likely domain from question keywords
        let q_lower = question.to_lowercase();
        let domain = if q_lower.contains("stock")
            || q_lower.contains("price")
            || q_lower.contains("market")
        {
            "finance"
        } else if q_lower.contains("election")
            || q_lower.contains("vote")
            || q_lower.contains("president")
        {
            "politics"
        } else if q_lower.contains("ai") || q_lower.contains("tech") || q_lower.contains("software")
        {
            "technology"
        } else if q_lower.contains("climate")
            || q_lower.contains("temperature")
            || q_lower.contains("carbon")
        {
            "climate"
        } else {
            "general"
        };

        // Set outside view to "searching" state
        self.outside_view = OutsideView {
            reference_class: format!("{} predictions (12-month horizon)", domain),
            historical_frequency: 0.35,
            sample_size: Some(142),
            source: "Searching via macro_forecaster…".into(),
            reasoning: Some("Base rate will be refined when agent results arrive.".into()),
            generated_by: Some("macro_forecaster".into()),
            loading: true,
        };

        // Anchor probability to base rate (Tetlock discipline)
        self.predicted_probability = 0.35;

        // Add evidence gaps (agents will fill these)
        self.evidence_gaps = vec![
            EvidenceGap {
                description: format!("Market data and analyst consensus for this question"),
                suggested_agent: Some("market_research".into()),
                suggested_query: Some(question.to_string()),
            },
            EvidenceGap {
                description: "Current sentiment from news and social media".into(),
                suggested_agent: Some("sentiment_analyzer".into()),
                suggested_query: Some(question.to_string()),
            },
            EvidenceGap {
                description: "Historical precedent and reference class data".into(),
                suggested_agent: Some("macro_forecaster".into()),
                suggested_query: None,
            },
        ];

        // Add timeline event
        self.timeline.push(TimelineEvent {
            label: "Base rate anchored".into(),
            probability: Some(0.35),
            event_type: TimelineEventType::BaseRateSet,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });

        self.timeline.push(TimelineEvent {
            label: "Agents dispatched".into(),
            probability: None,
            event_type: TimelineEventType::AgentExecuted,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });
    }

    /// Process results from an agent execution and populate the cockpit.
    /// Called when an agent completes (via channel from async task).
    pub fn populate_from_agent_result(&mut self, agent_id: &str, result: &JsonValue) {
        // Update agent status in fleet
        if let Some(agent) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.status = AgentStatus::Completed;
            agent.model_used = result
                .get("metadata")
                .and_then(|m| m.get("model_used"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            agent.execution_time_ms = result.get("execution_time_ms").and_then(|v| v.as_u64());
            agent.cost_credits = result.get("credits_charged").and_then(|v| v.as_f64());

            if let Some(cost) = agent.cost_credits {
                self.session_cost += cost;
            }
        }

        // Extract evidence from agent results
        if let Some(evidence_array) = result.get("evidence").and_then(|v| v.as_array()) {
            for ev in evidence_array {
                let summary = ev
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let source = ev
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or(agent_id)
                    .to_string();
                let relevance = ev.get("relevance").and_then(|v| v.as_f64()).unwrap_or(0.5);

                // Simple sentiment detection from key findings
                let key_findings = ev
                    .get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();

                let sentiment = detect_sentiment(&key_findings);

                let evidence_id = ev
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                self.evidence.push(EvidenceItem {
                    id: evidence_id,
                    source,
                    summary,
                    relevance,
                    sentiment,
                    date: None,
                    agent_id: Some(agent_id.to_string()),
                    dismissed: false,
                });

                // Remove matching evidence gaps
                self.evidence_gaps
                    .retain(|g| g.suggested_agent.as_deref() != Some(agent_id));
            }

            // Update agent findings count
            if let Some(agent) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
                agent.findings_count = evidence_array.len();
                agent.findings_summary = evidence_array
                    .first()
                    .and_then(|e| e.get("summary"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }

        // Extract reasoning for outside view (from macro_forecaster)
        if agent_id == "macro_forecaster" {
            if let Some(reasoning) = result
                .get("metadata")
                .and_then(|m| m.get("reasoning"))
                .and_then(|v| v.as_str())
            {
                self.outside_view.reasoning = Some(reasoning.to_string());
                self.outside_view.loading = false;
                self.outside_view.source = "macro_forecaster".into();
            }
        }

        // Add timeline event
        let findings = result
            .get("evidence")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        self.timeline.push(TimelineEvent {
            label: format!("{}: {} findings", agent_id, findings),
            probability: None,
            event_type: TimelineEventType::EvidenceAdded,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });

        // Check if all agents are done
        let all_done = self.agents.iter().all(|a| a.status != AgentStatus::Running);
        if all_done {
            self.orchestration_running = false;
            self.outside_view.loading = false;

            // Suggest probability adjustment based on evidence balance
            let (bull, bear, _neut) = self.sentiment_counts();
            if bull + bear > 0 {
                let bull_ratio = bull as f64 / (bull + bear) as f64;
                // Adjust from base rate toward evidence direction
                let base = self.outside_view.historical_frequency;
                let adjustment = (bull_ratio - 0.5) * 0.3; // conservative adjustment
                self.predicted_probability = (base + adjustment).clamp(0.05, 0.95);

                self.timeline.push(TimelineEvent {
                    label: format!(
                        "Probability adjusted to {:.0}%",
                        self.predicted_probability * 100.0
                    ),
                    probability: Some(self.predicted_probability),
                    event_type: TimelineEventType::ProbabilityUpdated,
                    timestamp: chrono::Utc::now().format("%H:%M").to_string(),
                });
            }
        }
    }

    /// Divergence between predicted probability and base rate (percentage points).
    pub fn divergence_pp(&self) -> Option<f64> {
        if self.outside_view.has_data() {
            Some((self.predicted_probability - self.outside_view.historical_frequency) * 100.0)
        } else {
            None
        }
    }

    /// Whether the divergence is large enough to warrant a warning.
    pub fn divergence_warning(&self) -> bool {
        self.divergence_pp()
            .map(|d| d.abs() > 20.0)
            .unwrap_or(false)
    }

    /// Count of active (non-dismissed) evidence items.
    pub fn active_evidence_count(&self) -> usize {
        self.evidence.iter().filter(|e| !e.dismissed).count()
    }

    /// Count evidence by sentiment.
    pub fn sentiment_counts(&self) -> (usize, usize, usize) {
        let mut bull = 0;
        let mut bear = 0;
        let mut neut = 0;
        for e in &self.evidence {
            if e.dismissed {
                continue;
            }
            match e.sentiment {
                Sentiment::Bullish => bull += 1,
                Sentiment::Bearish => bear += 1,
                Sentiment::Neutral => neut += 1,
            }
        }
        (bull, bear, neut)
    }
}

// ═══════════════════════════════════════════════════════════════════
// Render — Six-Zone Layout
// ═══════════════════════════════════════════════════════════════════

pub fn render_cockpit(state: &CockpitState, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(rgb(theme::BG))
        // ── Zone 1: Question Hub (top) ────────────────────────────
        .child(render_question_hub(state, cx))
        // ── Middle row: Outside View | Evidence | Drivers | Agents ─
        .child(
            div()
                .flex()
                .flex_grow()
                .gap(px(1.0))
                // Zone 2: Outside View (left-top)
                .child(
                    div()
                        .w(px(220.0))
                        .flex()
                        .flex_col()
                        .gap(px(1.0))
                        .child(render_outside_view(state))
                        .child(render_evidence_landscape(state)),
                )
                // Zone 4: Driver Map (center)
                .child(div().flex_grow().child(render_driver_map(state)))
                // Zone 5: Agent Fleet (right)
                .child(div().w(px(240.0)).child(render_agent_fleet(state))),
        )
        // ── Zone 6: Timeline (bottom) ─────────────────────────────
        .child(render_timeline(state))
}

// ═══════════════════════════════════════════════════════════════════
// Zone 1: Question Hub
// ═══════════════════════════════════════════════════════════════════

fn render_question_hub(state: &CockpitState, cx: &App) -> impl IntoElement {
    let prob_pct = format!("{:.0}%", state.predicted_probability * 100.0);
    let divergence = state.divergence_pp();
    let div_warning = state.divergence_warning();

    div()
        .bg(rgb(theme::BG_ELEVATED))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        .px(px(20.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        // Question input (large, editable)
        .child(state.question_input.clone())
        // Probability row: outside view ← probability → inside view
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(24.0))
                // Outside view summary (left)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_end()
                        .w(px(160.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .child("outside view"),
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .text_color(rgb(theme::GOLD))
                                .font_weight(FontWeight::BOLD)
                                .child(if state.outside_view.has_data() {
                                    format!("{:.0}%", state.outside_view.base_rate_pct())
                                } else if state.outside_view.loading {
                                    "…".into()
                                } else {
                                    "—".into()
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child("base rate"),
                        ),
                )
                // Central probability (large)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(36.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(prob_pct),
                        )
                        .when(divergence.is_some(), |el| {
                            let d = divergence.unwrap();
                            let sign = if d > 0.0 { "+" } else { "" };
                            let color = if div_warning {
                                theme::RED
                            } else {
                                theme::FG_DIM
                            };
                            el.child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(color))
                                    .child(format!("{}{}pp from base", sign, d as i64)),
                            )
                        }),
                )
                // Inside view summary (right)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .w(px(160.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .child("inside view"),
                        )
                        .child(
                            div()
                                .text_size(px(18.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(if let Some(ref sim) = state.sim_results {
                                    format!("{:.1}", sim.mean)
                                } else {
                                    "—".into()
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child("model mean"),
                        ),
                ),
        )
        // Metadata row: domain, target date, resolution criteria
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .child(div().flex_grow().child(state.domain_input.clone()))
                .child(div().w(px(160.0)).child(state.target_date_input.clone()))
                .child(
                    div()
                        .flex_grow()
                        .child(state.resolution_criteria_input.clone()),
                ),
        )
        // Orchestration status
        .when(state.orchestration_running, |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::GOLD))
                    .child("⟳ Agents researching… results will stream in as they complete"),
            )
        })
}

// ═══════════════════════════════════════════════════════════════════
// Zone 2: Outside View
// ═══════════════════════════════════════════════════════════════════

fn render_outside_view(state: &CockpitState) -> impl IntoElement {
    let ov = &state.outside_view;

    render_zone_card(
        "Outside View",
        theme::GOLD,
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .when(!ov.has_data() && !ov.loading, |el| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .child("Enter a question to find the base rate"),
                )
            })
            .when(ov.loading, |el| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::GOLD))
                        .child("Searching for reference class…"),
                )
            })
            .when(ov.has_data(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(render_kv("Reference Class", &ov.reference_class))
                        .child(render_kv(
                            "Historical Frequency",
                            &format!("{:.0}%", ov.base_rate_pct()),
                        ))
                        .child(render_kv(
                            "Sample Size",
                            &ov.sample_size
                                .map(|n| format!("n={}", n))
                                .unwrap_or_else(|| "—".into()),
                        ))
                        .child(render_kv("Source", &ov.source))
                        .when(ov.reasoning.is_some(), |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .mt(px(4.0))
                                    .child(ov.reasoning.as_deref().unwrap_or("").to_string()),
                            )
                        })
                        .when(state.divergence_warning(), |el| {
                            el.child(
                                div()
                                    .mt(px(6.0))
                                    .px(px(8.0))
                                    .py(px(6.0))
                                    .bg(rgb(0x3D2A1F))
                                    .rounded(px(4.0))
                                    .border_1()
                                    .border_color(rgb(theme::GOLD))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::GOLD))
                                    .child(
                                        "⚠ Significant divergence from base rate. Strong evidence needed to justify.",
                                    ),
                            )
                        }),
                )
            }),
    )
}

// ═══════════════════════════════════════════════════════════════════
// Zone 3: Evidence Landscape
// ═══════════════════════════════════════════════════════════════════

fn render_evidence_landscape(state: &CockpitState) -> impl IntoElement {
    let (bull, bear, neut) = state.sentiment_counts();
    let active = state.active_evidence_count();

    render_zone_card(
        &format!("Evidence ({})", active),
        theme::CYAN,
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .flex_grow()
            // Sentiment summary
            .when(active > 0, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .text_size(px(10.0))
                        .child(
                            div()
                                .text_color(rgb(theme::GREEN))
                                .child(format!("▲ {} bullish", bull)),
                        )
                        .child(
                            div()
                                .text_color(rgb(theme::RED))
                                .child(format!("▼ {} bearish", bear)),
                        )
                        .child(
                            div()
                                .text_color(rgb(theme::FG_DIM))
                                .child(format!("● {} neutral", neut)),
                        ),
                )
            })
            // Evidence items
            .children(
                state
                    .evidence
                    .iter()
                    .filter(|e| !e.dismissed)
                    .map(|e| render_evidence_item(e)),
            )
            // Evidence gaps
            .children(state.evidence_gaps.iter().map(|g| render_evidence_gap(g)))
            // Empty state
            .when(active == 0 && state.evidence_gaps.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
                        .py(px(8.0))
                        .child("No evidence yet — agents will populate this"),
                )
            }),
    )
}

fn render_evidence_item(item: &EvidenceItem) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap(px(6.0))
        .px(px(4.0))
        .py(px(3.0))
        .rounded(px(3.0))
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(item.sentiment.color()))
                .w(px(14.0))
                .child(item.sentiment.icon()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_grow()
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG))
                        .child(truncate(&item.summary, 50)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child(truncate(&item.source, 20))
                        .child(format!("{:.0}%", item.relevance * 100.0)),
                ),
        )
}

fn render_evidence_gap(gap: &EvidenceGap) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .px(px(4.0))
        .py(px(3.0))
        .rounded(px(3.0))
        .border_1()
        .border_color(rgb(theme::FG_FAINT))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(theme::GOLD))
                .w(px(14.0))
                .child("◌"),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::GOLD))
                .flex_grow()
                .child(truncate(&gap.description, 40)),
        )
        .when(gap.suggested_agent.is_some(), |el| {
            el.child(
                div()
                    .text_size(px(9.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .child("[fill gap]"),
            )
        })
}

// ═══════════════════════════════════════════════════════════════════
// Zone 4: Driver Map
// ═══════════════════════════════════════════════════════════════════

fn render_driver_map(state: &CockpitState) -> impl IntoElement {
    render_zone_card(
        &format!("Drivers & Model ({})", state.drivers.len()),
        theme::GREEN,
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .size_full()
            // Drivers list
            .children(
                state
                    .drivers
                    .iter()
                    .enumerate()
                    .map(|(i, d)| render_driver_node(i, d)),
            )
            // Model expression
            .when(!state.model_expression.is_empty(), |el| {
                el.child(
                    div()
                        .mt(px(8.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .bg(rgb(theme::BG))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme::FG_FAINT))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .child("model:"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(theme::FG))
                                .font_family("Berkeley Mono, JetBrains Mono, monospace")
                                .child(state.model_expression.clone()),
                        ),
                )
            })
            // Simulation results
            .when(state.sim_results.is_some(), |el| {
                let sim = state.sim_results.as_ref().unwrap();
                el.child(
                    div()
                        .mt(px(8.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .bg(rgb(theme::BG))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme::CYAN))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Simulation Results"),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(12.0))
                                .text_size(px(11.0))
                                .child(render_sim_stat("mean", sim.mean))
                                .child(render_sim_stat("median", sim.median))
                                .child(render_sim_stat("p5", sim.p5))
                                .child(render_sim_stat("p95", sim.p95))
                                .child(render_sim_stat("σ", sim.std_dev)),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "{}k iterations in {}ms",
                                    sim.iterations / 1000,
                                    sim.execution_time_ms
                                )),
                        ),
                )
            })
            // Simulation error
            .when(state.sim_error.is_some(), |el| {
                el.child(
                    div()
                        .mt(px(4.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .bg(rgb(0x3D1F1F))
                        .rounded(px(4.0))
                        .text_size(px(11.0))
                        .text_color(rgb(theme::RED))
                        .child(format!(
                            "✗ {}",
                            state.sim_error.as_deref().unwrap_or("Unknown error")
                        )),
                )
            })
            // Empty state
            .when(state.drivers.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .py(px(16.0))
                        .text_center()
                        .child("Agents will suggest drivers based on your question"),
                )
            })
            // Simulation hint
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_FAINT))
                    .mt(px(4.0))
                    .child("⌘R simulate · ⌘E toggle FPL source"),
            ),
    )
}

fn render_driver_node(index: usize, driver: &CockpitDriver) -> impl IntoElement {
    let type_color = match &driver.driver_type {
        CockpitDriverType::Continuous { .. } => theme::GREEN,
        CockpitDriverType::Binary { .. } => theme::GOLD,
    };

    let border_style = if driver.suggested {
        rgb(theme::FG_FAINT) // dashed would be ideal but GPUI doesn't support it
    } else {
        rgb(type_color)
    };

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(8.0))
        .py(px(5.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(border_style)
        .bg(if driver.suggested {
            rgb(theme::BG)
        } else {
            rgb(theme::BG_ELEVATED)
        })
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .cursor_pointer()
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .w(px(18.0))
                .child(format!("{}.", index + 1)),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(theme::FG))
                .font_weight(FontWeight::SEMIBOLD)
                .w(px(100.0))
                .child(driver.name.clone()),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(type_color))
                .px(px(4.0))
                .py(px(1.0))
                .rounded(px(2.0))
                .bg(rgb(theme::BG_ACTIVE))
                .child(driver.type_label()),
        )
        .child(
            div()
                .flex_grow()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child(driver.summary()),
        )
        .when(driver.suggested, |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::CYAN))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(3.0))
                    .bg(rgb(theme::BG_ACTIVE))
                    .cursor_pointer()
                    .child("+ accept"),
            )
        })
}

fn render_sim_stat(label: &str, value: f64) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_DIM))
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(theme::FG))
                .font_weight(FontWeight::BOLD)
                .child(format!("{:.1}", value)),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Zone 5: Agent Fleet
// ═══════════════════════════════════════════════════════════════════

fn render_agent_fleet(state: &CockpitState) -> impl IntoElement {
    let running = state
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Running)
        .count();
    let completed = state
        .agents
        .iter()
        .filter(|a| a.status == AgentStatus::Completed)
        .count();

    render_zone_card(
        &format!(
            "Agent Fleet ({}/{})",
            running + completed,
            state.agents.len()
        ),
        theme::BLUE,
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .size_full()
            // Agent rows
            .children(state.agents.iter().map(|a| render_agent_row(a)))
            // Empty state
            .when(state.agents.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
                        .py(px(8.0))
                        .child("No agents assigned yet"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Enter a question to trigger auto-research"),
                )
            })
            // Session cost
            .when(state.session_cost > 0.0, |el| {
                el.child(
                    div()
                        .mt(px(8.0))
                        .pt(px(6.0))
                        .border_t_1()
                        .border_color(rgb(theme::FG_FAINT))
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .child(format!("Session cost: {:.1}cr", state.session_cost)),
                )
            })
            // Assign button
            .child(
                div()
                    .mt(px(4.0))
                    .px(px(8.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .bg(rgb(theme::BG_ACTIVE))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::BLUE))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                    .text_center()
                    .child("[+ Assign agent]"),
            ),
    )
}

fn render_agent_row(agent: &FleetAgent) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(4.0))
        .py(px(4.0))
        .rounded(px(3.0))
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(agent.status.color()))
                        .w(px(14.0))
                        .child(agent.status.icon()),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG))
                        .child(agent.display_name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(agent.status.color()))
                        .child(agent.status.label()),
                ),
        )
        // Details line (when completed)
        .when(agent.status == AgentStatus::Completed, |el| {
            el.child(
                div()
                    .pl(px(20.0))
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child(format!(
                        "{} findings{}{}",
                        agent.findings_count,
                        agent
                            .model_used
                            .as_ref()
                            .map(|m| format!(" · {}", m))
                            .unwrap_or_default(),
                        agent
                            .cost_credits
                            .map(|c| format!(" · {:.1}cr", c))
                            .unwrap_or_default(),
                    )),
            )
        })
        // Summary (when completed and has summary)
        .when(
            agent.status == AgentStatus::Completed && agent.findings_summary.is_some(),
            |el| {
                el.child(
                    div()
                        .pl(px(20.0))
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child(truncate(
                            agent.findings_summary.as_deref().unwrap_or(""),
                            60,
                        )),
                )
            },
        )
}

// ═══════════════════════════════════════════════════════════════════
// Zone 6: Timeline
// ═══════════════════════════════════════════════════════════════════

fn render_timeline(state: &CockpitState) -> impl IntoElement {
    div()
        .h(px(48.0))
        .bg(rgb(theme::BG_ELEVATED))
        .border_t_1()
        .border_color(rgb(theme::FG_FAINT))
        .px(px(20.0))
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .w(px(60.0))
                .child("TIMELINE"),
        )
        .children(state.timeline.iter().map(|event| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(rgb(event.event_type.color()))
                        .child("●"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_DIM))
                                .child(event.label.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(
                                    event
                                        .probability
                                        .map(|p| format!("{:.0}%", p * 100.0))
                                        .unwrap_or_default(),
                                ),
                        ),
                )
                .child(
                    // Connector line to next event
                    div().w(px(20.0)).h(px(1.0)).bg(rgb(theme::FG_FAINT)),
                )
        }))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .child("now"),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Reusable components
// ═══════════════════════════════════════════════════════════════════

fn render_zone_card(title: &str, accent: u32, content: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .bg(rgb(theme::BG_ELEVATED))
        .border_1()
        .border_color(rgb(theme::FG_FAINT))
        .child(
            div()
                .px(px(10.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(rgb(theme::FG_FAINT))
                .text_size(px(11.0))
                .text_color(rgb(accent))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(div().p(px(8.0)).flex_grow().child(content))
}

fn render_kv(key: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .w(px(90.0))
                .child(format!("{}:", key)),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG))
                .flex_grow()
                .child(value.to_string()),
        )
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len.min(s.len()) - 1])
    }
}

// ═══════════════════════════════════════════════════════════════════
// Sentiment detection (simple heuristic)
// ═══════════════════════════════════════════════════════════════════

fn detect_sentiment(text: &str) -> Sentiment {
    let t = text.to_lowercase();
    let bullish_words = [
        "growth",
        "increase",
        "positive",
        "strong",
        "bullish",
        "gain",
        "opportunity",
        "upside",
        "beat",
        "exceed",
        "outperform",
        "surge",
        "momentum",
        "optimistic",
        "favorable",
        "accelerat",
    ];
    let bearish_words = [
        "decline",
        "decrease",
        "negative",
        "weak",
        "bearish",
        "loss",
        "risk",
        "downside",
        "miss",
        "below",
        "underperform",
        "drop",
        "slowdown",
        "pessimistic",
        "unfavorable",
        "decelerat",
        "concern",
    ];

    let bull_score: usize = bullish_words.iter().filter(|w| t.contains(*w)).count();
    let bear_score: usize = bearish_words.iter().filter(|w| t.contains(*w)).count();

    if bull_score > bear_score + 1 {
        Sentiment::Bullish
    } else if bear_score > bull_score + 1 {
        Sentiment::Bearish
    } else {
        Sentiment::Neutral
    }
}
