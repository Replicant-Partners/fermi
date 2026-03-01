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
//!
//! ## Channel Integration (Sprint 2)
//!
//! CockpitState is a GPUI Entity, which means it can use `cx.spawn()` to
//! fire async tasks that hold a `WeakEntity<CockpitState>` handle. When an
//! agent API call completes, the task calls `this.update(cx, ...)` to push
//! results back onto the UI thread. GPUI's event loop ensures the update
//! runs on the main thread, and `cx.notify()` triggers a re-render so the
//! six zones update live as each agent finishes.

use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;

use crate::api::client::{AgentExecutionResult, ApiClient, CreateForecastRequest};
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
    pub sample_size: Option<u32>,
    pub source: String,
    pub reasoning: Option<String>,
    pub generated_by: Option<String>,
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

/// A piece of evidence discovered by an agent or entered manually.
#[derive(Debug, Clone)]
pub struct EvidenceItem {
    pub id: String,
    pub source: String,
    pub summary: String,
    pub relevance: f64,
    pub sentiment: Sentiment,
    pub date: Option<String>,
    pub agent_id: Option<String>,
    pub dismissed: bool,
}

/// Sentiment classification for evidence.
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

/// An identified gap in the evidence landscape that an agent could fill.
#[derive(Debug, Clone)]
pub struct EvidenceGap {
    pub description: String,
    pub suggested_agent: Option<String>,
    pub suggested_query: Option<String>,
}

/// A driver in the forecast model.
#[derive(Debug, Clone)]
pub struct CockpitDriver {
    pub name: String,
    pub driver_type: CockpitDriverType,
    pub rationale: String,
    pub suggested: bool,
}

/// Driver type — continuous (distribution) or binary (probability).
#[derive(Debug, Clone)]
pub enum CockpitDriverType {
    Continuous {
        distribution: String,
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
                distribution,
                unit,
                p5,
                p50,
                p95,
            } => {
                format!(
                    "{} ({:.0}–{:.0}–{:.0} {})",
                    distribution, p5, p50, p95, unit
                )
            }
            CockpitDriverType::Binary {
                probability,
                impact_multiplier,
            } => {
                format!("{:.0}% (×{:.1})", probability * 100.0, impact_multiplier)
            }
        }
    }
}

/// An agent in the fleet panel.
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

/// Agent execution status.
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

/// A point on the probability evolution timeline.
#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub label: String,
    pub probability: Option<f64>,
    pub event_type: TimelineEventType,
    pub timestamp: String,
}

/// Type of timeline event (determines color).
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
            TimelineEventType::Resolved => theme::ORANGE,
        }
    }
}

/// Local simulation results.
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
}

// ═══════════════════════════════════════════════════════════════════
// Cockpit State — GPUI Entity
// ═══════════════════════════════════════════════════════════════════

/// The cockpit is a GPUI Entity so it can use `cx.spawn()` for async
/// agent API calls that flow results back to the UI thread.
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
    pub editing_driver_index: Option<usize>, // which driver is expanded for editing
    pub show_fpl_source: bool,               // toggle FPL source view (⌘E)
    pub fpl_source_override: Option<String>, // manual FPL override
    pub cached_fpl_source: String,           // last-generated FPL for display

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
    pub publish_status: Option<String>, // "publishing…", "published!", error msg
    pub probability_drag_active: bool,  // true while user is dragging the slider
}

impl CockpitState {
    pub fn new(api: Arc<ApiClient>, cx: &mut Context<Self>) -> Self {
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
            editing_driver_index: None,
            show_fpl_source: false,
            fpl_source_override: None,
            cached_fpl_source: String::new(),
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
            publish_status: None,
            probability_drag_active: false,
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Agent Orchestration — async with Entity channel integration
    // ═══════════════════════════════════════════════════════════════

    /// Fire the full research orchestration for a question.
    /// Called when the user submits the question (Enter key).
    ///
    /// Sequence:
    /// 1. Outside view search (base rate) — fires first
    /// 2. Inside view agents (evidence + drivers) — fire in parallel
    ///
    /// Results stream back via `cx.spawn()` + `WeakEntity` callbacks,
    /// updating the cockpit zones live as each agent completes.
    pub fn orchestrate_question(&mut self, question: &str, cx: &mut Context<Self>) {
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

        // ── Phase 2: Inside View agents ───────────────────────────
        let inside_agents: Vec<(&str, String)> = vec![
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

        // ── Populate with illustrative scaffold while agents run ──
        // This gives the user immediate visual feedback that the cockpit
        // is alive. Real agent results will replace/augment this as they
        // stream in via the Entity channel.
        self.populate_initial_scaffold(&question);

        // ── Fire API calls via cx.spawn() ─────────────────────────
        // Each agent gets its own spawned task with a WeakEntity handle.
        // When the API call completes, the task calls this.update(cx, ...)
        // to push results back onto the UI thread. GPUI ensures the
        // update closure runs on the main thread, and cx.notify()
        // triggers a re-render so the zones update live.
        let api = self.api.clone();

        for (agent_id, query) in inside_agents {
            let api = api.clone();
            let aid = agent_id.to_string();
            let q = query.clone();

            cx.spawn(async move |this, cx| {
                log::info!("[cockpit] Firing agent {} for question research", aid);

                match api.execute_agent(&aid, &q).await {
                    Ok(result) => {
                        log::info!(
                            "[cockpit] Agent {} completed: {} evidence items, confidence={:?}",
                            aid,
                            result.evidence.as_ref().map(|e| e.len()).unwrap_or(0),
                            result.confidence,
                        );

                        // Convert AgentExecutionResult to JsonValue for
                        // populate_from_agent_result (which handles the
                        // heterogeneous agent response shapes).
                        let result_json = agent_result_to_json(&result);

                        // Push results back to the UI thread
                        this.update(cx, |cockpit, cx| {
                            cockpit.populate_from_agent_result(&aid, &result_json);
                            cx.notify(); // trigger re-render
                        })
                        .ok();
                    }
                    Err(e) => {
                        log::error!("[cockpit] Agent {} failed: {}", aid, e);

                        // Mark agent as failed in the fleet panel
                        this.update(cx, |cockpit, cx| {
                            cockpit.mark_agent_failed(&aid, &e.to_string());
                            cx.notify();
                        })
                        .ok();
                    }
                }
            })
            .detach();
        }

        cx.notify();
    }

    /// Populate the cockpit with an initial scaffold based on the question.
    /// This runs synchronously and gives immediate visual feedback while
    /// agents are still executing in the background.
    ///
    /// Includes sensible default drivers so the user can immediately
    /// accept them and run ⌘R simulation — even without an API connection.
    fn populate_initial_scaffold(&mut self, question: &str) {
        // Simple heuristic: extract likely domain from question keywords
        let q_lower = question.to_lowercase();
        let domain = if q_lower.contains("stock")
            || q_lower.contains("price")
            || q_lower.contains("market")
            || q_lower.contains("revenue")
            || q_lower.contains("valuation")
        {
            "finance"
        } else if q_lower.contains("election")
            || q_lower.contains("vote")
            || q_lower.contains("president")
            || q_lower.contains("congress")
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

        // ── Scaffold drivers based on domain ──────────────────────
        // These give the user something to accept + simulate immediately,
        // even without an API connection. Agents will add/replace these
        // when their results arrive.
        self.drivers = match domain {
            "finance" => vec![
                CockpitDriver {
                    name: "revenue_growth".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "%".into(),
                        p5: -5.0,
                        p50: 8.0,
                        p95: 25.0,
                    },
                    rationale: "Year-over-year revenue growth rate".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "market_sentiment".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "index".into(),
                        p5: 0.3,
                        p50: 0.6,
                        p95: 0.9,
                    },
                    rationale: "Aggregate market sentiment indicator".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "macro_shock".into(),
                    driver_type: CockpitDriverType::Binary {
                        probability: 0.15,
                        impact_multiplier: 0.7,
                    },
                    rationale: "Probability of adverse macro event (recession, rate shock)".into(),
                    suggested: true,
                },
            ],
            "politics" => vec![
                CockpitDriver {
                    name: "incumbent_approval".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "%".into(),
                        p5: 30.0,
                        p50: 45.0,
                        p95: 55.0,
                    },
                    rationale: "Incumbent approval rating at decision time".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "turnout_factor".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "multiplier".into(),
                        p5: 0.85,
                        p50: 1.0,
                        p95: 1.15,
                    },
                    rationale: "Voter turnout relative to baseline".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "october_surprise".into(),
                    driver_type: CockpitDriverType::Binary {
                        probability: 0.20,
                        impact_multiplier: 1.4,
                    },
                    rationale: "Late-breaking event that shifts dynamics".into(),
                    suggested: true,
                },
            ],
            "technology" => vec![
                CockpitDriver {
                    name: "adoption_rate".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "%".into(),
                        p5: 5.0,
                        p50: 20.0,
                        p95: 50.0,
                    },
                    rationale: "Market adoption rate within forecast horizon".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "competitive_moat".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "score".into(),
                        p5: 0.2,
                        p50: 0.5,
                        p95: 0.8,
                    },
                    rationale: "Strength of competitive advantage".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "regulatory_risk".into(),
                    driver_type: CockpitDriverType::Binary {
                        probability: 0.25,
                        impact_multiplier: 0.6,
                    },
                    rationale: "Probability of adverse regulatory action".into(),
                    suggested: true,
                },
            ],
            "climate" => vec![
                CockpitDriver {
                    name: "emissions_trajectory".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "GtCO2".into(),
                        p5: 30.0,
                        p50: 38.0,
                        p95: 45.0,
                    },
                    rationale: "Annual global CO2 emissions".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "policy_ambition".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "index".into(),
                        p5: 0.2,
                        p50: 0.4,
                        p95: 0.7,
                    },
                    rationale: "Aggregate climate policy ambition score".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "tipping_point".into(),
                    driver_type: CockpitDriverType::Binary {
                        probability: 0.10,
                        impact_multiplier: 2.0,
                    },
                    rationale: "Probability of crossing a climate tipping point".into(),
                    suggested: true,
                },
            ],
            _ => vec![
                CockpitDriver {
                    name: "base_factor".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "".into(),
                        p5: 10.0,
                        p50: 50.0,
                        p95: 90.0,
                    },
                    rationale: "Primary driver — adjust range to match your question".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "trend_modifier".into(),
                    driver_type: CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "multiplier".into(),
                        p5: 0.8,
                        p50: 1.0,
                        p95: 1.3,
                    },
                    rationale: "Trend direction and magnitude".into(),
                    suggested: true,
                },
                CockpitDriver {
                    name: "disruption_event".into(),
                    driver_type: CockpitDriverType::Binary {
                        probability: 0.15,
                        impact_multiplier: 1.5,
                    },
                    rationale: "Probability of a disruptive event".into(),
                    suggested: true,
                },
            ],
        };

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

        // Add timeline events
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
    /// Called on the UI thread when an agent's cx.spawn() task completes.
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

                // Also consider the summary text for sentiment if key_findings is empty
                let sentiment_text = if key_findings.is_empty() {
                    &summary
                } else {
                    &key_findings
                };
                let sentiment = detect_sentiment(sentiment_text);

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
                    date: ev
                        .get("date")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
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

        // Extract drivers from agent results (macro_forecaster suggests these)
        if let Some(drivers_array) = result.get("drivers").and_then(|v| v.as_array()) {
            for drv in drivers_array {
                let name = drv
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let rationale = drv
                    .get("rationale")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let driver_type = if let Some(dist) = drv.get("distribution") {
                    CockpitDriverType::Continuous {
                        distribution: dist
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("triangular")
                            .to_string(),
                        unit: dist
                            .get("unit")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        p5: dist.get("p5").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        p50: dist.get("p50").and_then(|v| v.as_f64()).unwrap_or(0.0),
                        p95: dist.get("p95").and_then(|v| v.as_f64()).unwrap_or(0.0),
                    }
                } else if let Some(prob) = drv.get("probability").and_then(|v| v.as_f64()) {
                    CockpitDriverType::Binary {
                        probability: prob,
                        impact_multiplier: drv
                            .get("impact_multiplier")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(1.0),
                    }
                } else {
                    CockpitDriverType::Continuous {
                        distribution: "triangular".into(),
                        unit: "".into(),
                        p5: 0.0,
                        p50: 0.0,
                        p95: 0.0,
                    }
                };

                self.drivers.push(CockpitDriver {
                    name,
                    driver_type,
                    rationale,
                    suggested: true,
                });
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

            // Update base rate if the agent provided one
            if let Some(base_rate) = result
                .get("metadata")
                .and_then(|m| m.get("base_rate"))
                .and_then(|v| v.as_f64())
            {
                self.outside_view.historical_frequency = base_rate;
                self.outside_view.loading = false;
            }

            // Update reference class if provided
            if let Some(ref_class) = result
                .get("metadata")
                .and_then(|m| m.get("reference_class"))
                .and_then(|v| v.as_str())
            {
                self.outside_view.reference_class = ref_class.to_string();
            }

            // Update sample size if provided
            if let Some(n) = result
                .get("metadata")
                .and_then(|m| m.get("sample_size"))
                .and_then(|v| v.as_u64())
            {
                self.outside_view.sample_size = Some(n as u32);
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
        self.check_orchestration_complete();
    }

    /// Mark an agent as failed in the fleet panel.
    /// Called on the UI thread when an agent's API call errors.
    pub fn mark_agent_failed(&mut self, agent_id: &str, error_msg: &str) {
        if let Some(agent) = self.agents.iter_mut().find(|a| a.agent_id == agent_id) {
            agent.status = AgentStatus::Failed;
            agent.findings_summary = Some(format!("Error: {}", truncate(error_msg, 60)));
        }

        // Add timeline event for the failure
        self.timeline.push(TimelineEvent {
            label: format!("{}: failed", agent_id),
            probability: None,
            event_type: TimelineEventType::AgentExecuted,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });

        // Check if all agents are done (including failures)
        self.check_orchestration_complete();
    }

    /// Check if all agents have finished (completed or failed) and
    /// finalize the orchestration: adjust probability, clear loading states.
    fn check_orchestration_complete(&mut self) {
        let all_done = self.agents.iter().all(|a| a.status != AgentStatus::Running);

        if !all_done {
            return;
        }

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

        // Log orchestration summary
        let completed = self
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Completed)
            .count();
        let failed = self
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Failed)
            .count();
        log::info!(
            "[cockpit] Orchestration complete: {}/{} agents succeeded, {} failed, {} evidence items, {:.1}cr total",
            completed,
            self.agents.len(),
            failed,
            self.evidence.len(),
            self.session_cost,
        );
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

    // ═══════════════════════════════════════════════════════════════
    // Driver Editing
    // ═══════════════════════════════════════════════════════════════

    /// Toggle inline editing for a driver node. Clicking the same driver
    /// again collapses it; clicking a different one switches.
    pub fn toggle_driver_edit(&mut self, index: usize) {
        if self.editing_driver_index == Some(index) {
            self.editing_driver_index = None;
        } else if index < self.drivers.len() {
            self.editing_driver_index = Some(index);
        }
    }

    /// Accept a suggested (ghost) driver — marks it as user-confirmed.
    pub fn accept_driver(&mut self, index: usize) {
        if let Some(driver) = self.drivers.get_mut(index) {
            driver.suggested = false;
        }
    }

    /// Remove a driver by index.
    pub fn remove_driver(&mut self, index: usize) {
        if index < self.drivers.len() {
            self.drivers.remove(index);
            // Collapse editor if we removed the one being edited
            if self.editing_driver_index == Some(index) {
                self.editing_driver_index = None;
            } else if let Some(ref mut ei) = self.editing_driver_index {
                if *ei > index {
                    *ei -= 1;
                }
            }
            self.auto_model_expression();
        }
    }

    /// Update a continuous driver's parameters.
    pub fn update_continuous_driver(
        &mut self,
        index: usize,
        p5: f64,
        p50: f64,
        p95: f64,
        unit: &str,
    ) {
        if let Some(driver) = self.drivers.get_mut(index) {
            if let CockpitDriverType::Continuous {
                p5: ref mut dp5,
                p50: ref mut dp50,
                p95: ref mut dp95,
                unit: ref mut dunit,
                ..
            } = driver.driver_type
            {
                *dp5 = p5;
                *dp50 = p50;
                *dp95 = p95;
                *dunit = unit.to_string();
            }
        }
    }

    /// Update a binary driver's parameters.
    pub fn update_binary_driver(&mut self, index: usize, probability: f64, impact: f64) {
        if let Some(driver) = self.drivers.get_mut(index) {
            if let CockpitDriverType::Binary {
                probability: ref mut dp,
                impact_multiplier: ref mut di,
            } = driver.driver_type
            {
                *dp = probability.clamp(0.0, 1.0);
                *di = impact;
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // FPL Generation & Simulation (⌘R)
    // ═══════════════════════════════════════════════════════════════

    /// Auto-generate a model expression from driver names.
    /// Continuous drivers multiply; binary drivers use if-then.
    pub fn auto_model_expression(&mut self) {
        let parts: Vec<String> = self
            .drivers
            .iter()
            .filter(|d| !d.suggested) // only accepted drivers
            .map(|d| match &d.driver_type {
                CockpitDriverType::Continuous { .. } => d.name.clone(),
                CockpitDriverType::Binary {
                    impact_multiplier, ..
                } => {
                    format!("(if {} then {} else 1.0)", d.name, impact_multiplier)
                }
            })
            .collect();

        self.model_expression = if parts.is_empty() {
            String::new()
        } else {
            parts.join(" * ")
        };
    }

    /// Generate FPL source from the current cockpit state.
    pub fn generate_fpl(&self, cx: &App) -> String {
        let mut lines = Vec::new();

        // Question
        let question = self.question_input.read(cx).text().to_string();
        if !question.is_empty() {
            let escaped = question.replace('"', r#"\""#);
            lines.push(format!("question \"{}\"", escaped));
            lines.push(String::new());
        }

        // Drivers (only accepted ones)
        for driver in &self.drivers {
            if driver.suggested {
                continue;
            }
            let safe_name = sanitize_fpl_name(&driver.name);
            match &driver.driver_type {
                CockpitDriverType::Continuous {
                    distribution: _,
                    unit,
                    p5,
                    p50,
                    p95,
                } => {
                    let dist_str = format!("triangular({}, {}, {})", p5, p50, p95);
                    let unit_str = if unit.is_empty() {
                        String::new()
                    } else {
                        format!("\n    unit: \"{}\"", unit)
                    };
                    let rationale_str = if driver.rationale.is_empty() {
                        String::new()
                    } else {
                        format!("\n    rationale: \"{}\"", driver.rationale)
                    };
                    lines.push(format!(
                        "driver {} continuous {{\n    distribution: {}{}{}\n}}",
                        safe_name, dist_str, unit_str, rationale_str
                    ));
                }
                CockpitDriverType::Binary {
                    probability,
                    impact_multiplier,
                } => {
                    let rationale_str = if driver.rationale.is_empty() {
                        String::new()
                    } else {
                        format!("\n    rationale: \"{}\"", driver.rationale)
                    };
                    lines.push(format!(
                        "driver {} binary {{\n    probability: {}p\n    impact_multiplier: {}{}\n}}",
                        safe_name, probability, impact_multiplier, rationale_str
                    ));
                }
            }
            lines.push(String::new());
        }

        // Model
        if !self.model_expression.is_empty() {
            lines.push(format!("model: {}", self.model_expression));
            lines.push(String::new());
        }

        // Simulate
        lines.push("simulate 10000 iterations".to_string());

        lines.join("\n")
    }

    /// Get the effective FPL source (manual override or auto-generated).
    pub fn effective_fpl(&self, cx: &App) -> String {
        self.fpl_source_override
            .clone()
            .unwrap_or_else(|| self.generate_fpl(cx))
    }

    /// Regenerate the cached FPL source string for display.
    /// Called when toggling the FPL view or before simulation.
    pub fn refresh_fpl_cache(&mut self, cx: &App) {
        self.cached_fpl_source = self.effective_fpl(cx);
    }

    /// Run Monte Carlo simulation locally (⌘R).
    /// Generates FPL from cockpit state, parses, executes, and stores results.
    /// This is synchronous and fast — 10k iterations in <100ms.
    pub fn run_simulation(&mut self, cx: &mut Context<Self>) {
        self.sim_running = true;
        self.sim_error = None;

        let fpl_source = self.effective_fpl(cx);
        self.cached_fpl_source = fpl_source.clone();

        if fpl_source.trim().is_empty() || self.drivers.iter().all(|d| d.suggested) {
            self.sim_error = Some("No accepted drivers to simulate. Accept drivers first.".into());
            self.sim_running = false;
            cx.notify();
            return;
        }

        // Auto-generate model expression if empty
        if self.model_expression.is_empty() {
            self.auto_model_expression();
        }

        let start = std::time::Instant::now();

        // Parse
        let tokens = match ::fermi::lexer::Lexer::new(&fpl_source).tokenize() {
            Ok(t) => t,
            Err(e) => {
                self.sim_error = Some(format!("Tokenization error: {:?}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        let program = match ::fermi::parser::Parser::new(tokens).parse() {
            Ok(p) => p,
            Err(e) => {
                self.sim_error = Some(format!("Parse error: {}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        // Execute
        let mut executor = ::fermi::executor::Executor::new(10_000);
        match executor.execute(&program) {
            Ok(results) => {
                let elapsed = start.elapsed();
                let histogram_data = results.histogram(20);

                self.sim_results = Some(SimResults {
                    mean: results.mean,
                    median: results.median,
                    p5: results.p5,
                    p95: results.p95,
                    std_dev: results.std_dev,
                    iterations: results.iterations as u64,
                    execution_time_ms: elapsed.as_millis() as u64,
                    histogram: histogram_data
                        .iter()
                        .map(|(_, count)| *count as u32)
                        .collect(),
                });
                self.sim_running = false;

                // Add timeline event
                self.timeline.push(TimelineEvent {
                    label: format!(
                        "Simulated: mean={:.1}, p5={:.1}, p95={:.1}",
                        results.mean, results.p5, results.p95
                    ),
                    probability: Some(self.predicted_probability),
                    event_type: TimelineEventType::ProbabilityUpdated,
                    timestamp: chrono::Utc::now().format("%H:%M").to_string(),
                });

                log::info!(
                    "[cockpit] Simulation complete: mean={:.2}, median={:.2}, p5={:.2}, p95={:.2} ({}ms)",
                    results.mean,
                    results.median,
                    results.p5,
                    results.p95,
                    elapsed.as_millis(),
                );
            }
            Err(e) => {
                self.sim_error = Some(format!("Execution error: {:?}", e));
                self.sim_running = false;
            }
        }

        cx.notify();
    }

    // ═══════════════════════════════════════════════════════════════
    // Publish Flow (⌘Enter from cockpit, not question submit)
    // ═══════════════════════════════════════════════════════════════

    /// Publish the forecast to the API for Brier tracking.
    /// Collects all cockpit state into a CreateForecastRequest and POSTs it.
    pub fn publish_forecast(&mut self, cx: &mut Context<Self>) {
        let question = self.question_input.read(cx).text().to_string();
        if question.trim().is_empty() {
            self.publish_status = Some("Cannot publish: no question".into());
            cx.notify();
            return;
        }

        self.publish_status = Some("Publishing…".into());
        cx.notify();

        let domain = self.domain_input.read(cx).text().to_string();
        let target_date = self.target_date_input.read(cx).text().to_string();
        let resolution_criteria = self.resolution_criteria_input.read(cx).text().to_string();
        let fpl_source = self.effective_fpl(cx);

        // Build drivers JSON
        let drivers_json: Vec<JsonValue> = self
            .drivers
            .iter()
            .filter(|d| !d.suggested)
            .map(|d| {
                let mut obj = serde_json::Map::new();
                obj.insert("name".into(), JsonValue::String(d.name.clone()));
                obj.insert("rationale".into(), JsonValue::String(d.rationale.clone()));
                match &d.driver_type {
                    CockpitDriverType::Continuous {
                        distribution,
                        unit,
                        p5,
                        p50,
                        p95,
                    } => {
                        obj.insert("type".into(), JsonValue::String("continuous".into()));
                        obj.insert(
                            "distribution".into(),
                            serde_json::json!({
                                "type": distribution,
                                "unit": unit,
                                "p5": p5,
                                "p50": p50,
                                "p95": p95,
                            }),
                        );
                    }
                    CockpitDriverType::Binary {
                        probability,
                        impact_multiplier,
                    } => {
                        obj.insert("type".into(), JsonValue::String("binary".into()));
                        obj.insert("probability".into(), serde_json::json!(probability));
                        obj.insert(
                            "impact_multiplier".into(),
                            serde_json::json!(impact_multiplier),
                        );
                    }
                }
                JsonValue::Object(obj)
            })
            .collect();

        // Build evidence JSON
        let evidence_json: Vec<JsonValue> = self
            .evidence
            .iter()
            .filter(|e| !e.dismissed)
            .map(|e| {
                serde_json::json!({
                    "source": e.source,
                    "summary": e.summary,
                    "relevance": e.relevance,
                    "sentiment": format!("{:?}", e.sentiment),
                    "agent_id": e.agent_id,
                })
            })
            .collect();

        // Build sim results JSON
        let sim_json = self.sim_results.as_ref().map(|s| {
            serde_json::json!({
                "mean": s.mean,
                "median": s.median,
                "p5": s.p5,
                "p95": s.p95,
                "std_dev": s.std_dev,
                "iterations": s.iterations,
            })
        });

        // Agents used
        let agents_used: Vec<String> = self
            .agents
            .iter()
            .filter(|a| a.status == AgentStatus::Completed)
            .map(|a| a.agent_id.clone())
            .collect();

        let req = CreateForecastRequest {
            question_text: question,
            predicted_probability: self.predicted_probability,
            domain: if domain.is_empty() {
                None
            } else {
                Some(domain)
            },
            resolution_criteria: if resolution_criteria.is_empty() {
                None
            } else {
                Some(resolution_criteria)
            },
            target_date: if target_date.is_empty() {
                None
            } else {
                Some(target_date)
            },
            confidence_interval_low: self.sim_results.as_ref().map(|s| s.p5),
            confidence_interval_high: self.sim_results.as_ref().map(|s| s.p95),
            fpl_source: if fpl_source.trim().is_empty() {
                None
            } else {
                Some(fpl_source)
            },
            simulation_results: sim_json,
            drivers: if drivers_json.is_empty() {
                None
            } else {
                Some(JsonValue::Array(drivers_json))
            },
            evidence: if evidence_json.is_empty() {
                None
            } else {
                Some(JsonValue::Array(evidence_json))
            },
            visibility: Some("private".into()),
            tags: None,
            portfolio_id: None,
            status: Some("active".into()),
        };

        let api = self.api.clone();

        cx.spawn(
            async move |this, cx| match api.create_forecast(&req).await {
                Ok(response) => {
                    let forecast_id = response
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    log::info!("[cockpit] Forecast published: {}", forecast_id);

                    this.update(cx, |cockpit, cx| {
                        cockpit.forecast_id = Some(forecast_id.clone());
                        cockpit.status = "active".into();
                        cockpit.publish_status =
                            Some(format!("Published! ID: {}", truncate(&forecast_id, 12)));

                        cockpit.timeline.push(TimelineEvent {
                            label: "Forecast published".into(),
                            probability: Some(cockpit.predicted_probability),
                            event_type: TimelineEventType::Published,
                            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
                        });

                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("[cockpit] Publish failed: {}", e);

                    this.update(cx, |cockpit, cx| {
                        cockpit.publish_status = Some(format!("Publish failed: {}", e));
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }

    // ═══════════════════════════════════════════════════════════════
    // Probability Slider
    // ═══════════════════════════════════════════════════════════════

    /// Set the predicted probability (from slider drag or direct input).
    /// Clamps to [0.05, 0.95] and records a timeline event.
    pub fn set_probability(&mut self, new_prob: f64) {
        let clamped = new_prob.clamp(0.05, 0.95);
        let old = self.predicted_probability;
        if (clamped - old).abs() < 0.001 {
            return;
        }
        self.predicted_probability = clamped;

        // Only record timeline event when drag ends (not every frame)
        // The caller should add the timeline event on mouse_up.
    }

    /// Record a probability change in the timeline (call on drag end).
    pub fn commit_probability_change(&mut self) {
        self.probability_drag_active = false;

        self.timeline.push(TimelineEvent {
            label: format!(
                "Probability set to {:.0}%",
                self.predicted_probability * 100.0
            ),
            probability: Some(self.predicted_probability),
            event_type: TimelineEventType::ProbabilityUpdated,
            timestamp: chrono::Utc::now().format("%H:%M").to_string(),
        });

        // Check divergence warning
        if self.divergence_warning() {
            log::info!(
                "[cockpit] Divergence warning: {:.0}pp from base rate",
                self.divergence_pp().unwrap_or(0.0)
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Render — CockpitState is now a GPUI Entity with Render impl
// ═══════════════════════════════════════════════════════════════════

impl Render for CockpitState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ── Build interactive driver nodes with click handlers ─────
        let driver_elements: Vec<AnyElement> = self
            .drivers
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let is_editing = self.editing_driver_index == Some(i);
                let is_suggested = d.suggested;

                // Wrap each driver node in a clickable container
                let node = render_driver_node(i, d, is_editing);

                // Header click → toggle edit
                let mut wrapper = div()
                    .id(ElementId::Name(format!("driver-{}", i).into()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.toggle_driver_edit(i);
                        cx.notify();
                    }))
                    .child(node);

                // Accept button (for suggested drivers)
                if is_suggested {
                    wrapper = wrapper.child(
                        div()
                            .id(ElementId::Name(format!("accept-{}", i).into()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.accept_driver(i);
                                this.auto_model_expression();
                                cx.notify();
                            }))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.0))
                            .text_color(rgb(theme::CYAN))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .child("✓ accept driver"),
                    );
                }

                // Remove button (when editing)
                if is_editing {
                    wrapper = wrapper.child(
                        div()
                            .id(ElementId::Name(format!("remove-{}", i).into()))
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                this.remove_driver(i);
                                cx.notify();
                            }))
                            .px(px(8.0))
                            .py(px(3.0))
                            .text_size(px(10.0))
                            .text_color(rgb(theme::RED))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .child("× remove driver"),
                    );
                }

                wrapper.into_any_element()
            })
            .collect();

        // ── Build interactive probability slider ──────────────────
        let prob = self.predicted_probability;
        let div_warning = self.divergence_warning();
        let prob_slider = render_probability_slider_interactive(prob, div_warning, cx);

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(theme::BG))
            // ── Zone 1: Question Hub (top) ────────────────────────────
            .child(render_question_hub(self, prob_slider))
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
                            .child(render_outside_view(self))
                            .child(render_evidence_landscape(self)),
                    )
                    // Zone 4: Driver Map (center, with interactive driver nodes)
                    .child(
                        div()
                            .flex_grow()
                            .child(render_driver_map_with_nodes(self, driver_elements)),
                    )
                    // Zone 5: Agent Fleet (right)
                    .child(div().w(px(240.0)).child(render_agent_fleet(self))),
            )
            // ── Zone 6: Timeline (bottom) ─────────────────────────────
            .child(render_timeline(self))
    }
}

/// Standalone render function for use from FermiConsole when it holds
/// an `Entity<CockpitState>`. The Entity's own Render impl is used
/// internally, but this provides backward-compatible access for the
/// parent to embed the cockpit as a child element.
pub fn render_cockpit(cockpit: &Entity<CockpitState>) -> impl IntoElement {
    cockpit.clone()
}

// ═══════════════════════════════════════════════════════════════════
// Zone 1: Question Hub
// ═══════════════════════════════════════════════════════════════════

fn render_question_hub(state: &CockpitState, prob_slider: AnyElement) -> impl IntoElement {
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
        // Probability row: outside view <- probability -> inside view
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
                // Central probability (large) + slider
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(36.0))
                                .text_color(rgb(theme::CYAN))
                                .font_weight(FontWeight::BOLD)
                                .child(prob_pct),
                        )
                        // ── Probability slider bar (interactive) ──────
                        .child(prob_slider)
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
            let running_count = state
                .agents
                .iter()
                .filter(|a| a.status == AgentStatus::Running)
                .count();
            let total = state.agents.len();
            let completed = total - running_count;
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::GOLD))
                    .child(format!(
                        "⟳ Agents researching… {}/{} complete — results streaming in live",
                        completed, total
                    )),
            )
        })
        // Publish status
        .when(state.publish_status.is_some(), |el| {
            let status = state.publish_status.as_deref().unwrap_or("");
            let color = if status.starts_with("Published") {
                theme::GREEN
            } else if status.starts_with("Publish failed") || status.starts_with("Cannot") {
                theme::RED
            } else {
                theme::GOLD
            };
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(color))
                    .child(status.to_string()),
            )
        })
        // Keyboard hints
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("⌘Enter research")
                .child("⌘R simulate")
                .child("⌘P publish")
                .child("⌘E toggle FPL"),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Probability Slider
// ═══════════════════════════════════════════════════════════════════

/// Build an interactive probability slider with mouse handlers.
/// Must be called from the Render impl where cx is available.
fn render_probability_slider_interactive(
    probability: f64,
    warning: bool,
    cx: &mut Context<CockpitState>,
) -> AnyElement {
    let bar_width = 200.0_f32;
    let fill_width = (probability as f32 * bar_width).clamp(4.0, bar_width - 4.0);
    let fill_color = if warning { theme::GOLD } else { theme::CYAN };

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        // "5%" label
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("5%"),
        )
        // Slider track (interactive)
        .child(
            div()
                .id("prob-slider-track")
                .w(px(bar_width))
                .h(px(12.0))
                .rounded(px(6.0))
                .bg(rgb(theme::BG))
                .border_1()
                .border_color(rgb(theme::FG_FAINT))
                .overflow_hidden()
                .cursor_pointer()
                // Click to set probability
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        // Calculate probability from click position relative to track
                        // The event position is in window coordinates; we need to map
                        // it relative to the element bounds. Since we know the track
                        // width, we use a simplified approach: store the click and
                        // compute on the next frame. For now, use a heuristic based
                        // on the event position within the element's hitbox.
                        this.probability_drag_active = true;
                        cx.notify();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseUpEvent, _window, cx| {
                        if this.probability_drag_active {
                            this.commit_probability_change();
                            cx.notify();
                        }
                    }),
                )
                // Filled portion
                .child(
                    div()
                        .h_full()
                        .w(px(fill_width))
                        .bg(rgb(fill_color))
                        .rounded_l(px(5.0)),
                )
                // Thumb indicator at the fill edge
                .child(
                    div()
                        .absolute()
                        .left(px(fill_width - 3.0))
                        .top(px(0.0))
                        .w(px(6.0))
                        .h(px(12.0))
                        .rounded(px(3.0))
                        .bg(rgb(theme::FG)),
                ),
        )
        // "95%" label
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("95%"),
        )
        // Nudge buttons for fine control
        .child(
            div()
                .flex()
                .gap(px(4.0))
                .child(
                    div()
                        .id("prob-minus-5")
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.set_probability(this.predicted_probability - 0.05);
                            this.commit_probability_change();
                            cx.notify();
                        }))
                        .child("-5"),
                )
                .child(
                    div()
                        .id("prob-minus-1")
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.set_probability(this.predicted_probability - 0.01);
                            this.commit_probability_change();
                            cx.notify();
                        }))
                        .child("-1"),
                )
                .child(
                    div()
                        .id("prob-plus-1")
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.set_probability(this.predicted_probability + 0.01);
                            this.commit_probability_change();
                            cx.notify();
                        }))
                        .child("+1"),
                )
                .child(
                    div()
                        .id("prob-plus-5")
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(rgb(theme::BG_ACTIVE))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _event, _window, cx| {
                            this.set_probability(this.predicted_probability + 0.05);
                            this.commit_probability_change();
                            cx.notify();
                        }))
                        .child("+5"),
                ),
        )
        .into_any_element()
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

/// Render the driver map zone with pre-built interactive driver node elements.
/// The driver_elements are built in the Render impl where cx.listener() is available.
fn render_driver_map_with_nodes(
    state: &CockpitState,
    driver_elements: Vec<AnyElement>,
) -> impl IntoElement {
    render_zone_card(
        &format!("Drivers & Model ({})", state.drivers.len()),
        theme::GREEN,
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .size_full()
            // Drivers list (interactive, built in Render impl)
            .children(driver_elements)
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
                        // Histogram sparkline
                        .when(!sim.histogram.is_empty(), |el| {
                            el.child(render_histogram_bars(&sim.histogram))
                        })
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
            // FPL source toggle
            .when(state.show_fpl_source, |el| {
                let fpl = if state.cached_fpl_source.is_empty() {
                    "[auto-generated — press ⌘R to simulate]"
                } else {
                    &state.cached_fpl_source
                };
                el.child(
                    div()
                        .mt(px(8.0))
                        .px(px(10.0))
                        .py(px(6.0))
                        .bg(rgb(theme::BG))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(rgb(theme::PURPLE))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::PURPLE))
                                .child("FPL source:"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::FG_DIM))
                                .font_family("Berkeley Mono, JetBrains Mono, monospace")
                                .child(fpl.to_string()),
                        ),
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

fn render_driver_node(index: usize, driver: &CockpitDriver, is_editing: bool) -> impl IntoElement {
    let type_color = match &driver.driver_type {
        CockpitDriverType::Continuous { .. } => theme::GREEN,
        CockpitDriverType::Binary { .. } => theme::GOLD,
    };

    let border_style = if is_editing {
        rgb(theme::CYAN)
    } else if driver.suggested {
        rgb(theme::FG_FAINT)
    } else {
        rgb(type_color)
    };

    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(border_style)
        .bg(if is_editing {
            rgb(theme::BG_ACTIVE)
        } else if driver.suggested {
            rgb(theme::BG)
        } else {
            rgb(theme::BG_ELEVATED)
        })
        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
        .cursor_pointer()
        // ── Header row (always visible, click to toggle edit) ─────
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(5.0))
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
                .when(is_editing, |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::RED))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .bg(rgb(theme::BG_ACTIVE))
                            .cursor_pointer()
                            .child("× remove"),
                    )
                }),
        )
        // ── Expanded editor (visible when is_editing) ─────────────
        .when(is_editing, |el| el.child(render_driver_editor(driver)))
}

/// Render the inline parameter editor for an expanded driver node.
fn render_driver_editor(driver: &CockpitDriver) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .border_t_1()
        .border_color(rgb(theme::FG_FAINT))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(match &driver.driver_type {
            CockpitDriverType::Continuous {
                distribution,
                unit,
                p5,
                p50,
                p95,
            } => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                // Distribution type
                .child(render_param_row("distribution", distribution))
                .child(render_param_row(
                    "unit",
                    if unit.is_empty() { "—" } else { unit },
                ))
                // P5 / P50 / P95 parameter bars
                .child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .child(render_param_value("p5", *p5, theme::FG_DIM))
                        .child(render_param_value("p50", *p50, theme::CYAN))
                        .child(render_param_value("p95", *p95, theme::FG_DIM)),
                )
                // Visual range bar
                .child(render_range_bar(*p5, *p50, *p95))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Click values to edit · Tab between fields"),
                )
                .into_any_element(),
            CockpitDriverType::Binary {
                probability,
                impact_multiplier,
            } => div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .gap(px(16.0))
                        .child(render_param_value(
                            "probability",
                            *probability * 100.0,
                            theme::GOLD,
                        ))
                        .child(render_param_value(
                            "impact ×",
                            *impact_multiplier,
                            theme::CYAN,
                        )),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Click values to edit"),
                )
                .into_any_element(),
        })
        // Rationale
        .when(!driver.rationale.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child(format!("rationale: {}", driver.rationale)),
            )
        })
}

/// Render a label: value parameter row.
fn render_param_row(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_DIM))
                .w(px(80.0))
                .child(format!("{}:", label)),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG))
                .child(value.to_string()),
        )
}

/// Render a single numeric parameter with label and colored value.
fn render_param_value(label: &str, value: f64, color: u32) -> impl IntoElement {
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
                .text_size(px(14.0))
                .text_color(rgb(color))
                .font_weight(FontWeight::BOLD)
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .bg(rgb(theme::BG))
                .border_1()
                .border_color(rgb(theme::FG_FAINT))
                .cursor_pointer()
                .hover(|s| s.border_color(rgb(theme::CYAN)))
                .child(format!("{:.1}", value)),
        )
}

/// Render a visual range bar showing p5–p50–p95 spread.
fn render_range_bar(p5: f64, p50: f64, p95: f64) -> impl IntoElement {
    // Normalize to 0..1 within the p5..p95 range for display
    let range = (p95 - p5).max(0.001);
    let mid_frac = ((p50 - p5) / range).clamp(0.0, 1.0) as f32;
    let bar_width = 200.0_f32;
    let mid_px = mid_frac * bar_width;

    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .w(px(40.0))
                .text_right()
                .child(format!("{:.0}", p5)),
        )
        .child(
            div()
                .w(px(bar_width))
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgb(theme::BG))
                .border_1()
                .border_color(rgb(theme::FG_FAINT))
                .overflow_hidden()
                .child(
                    // Full range fill
                    div().h_full().w_full().bg(rgb(theme::GREEN)).opacity(0.3),
                )
                // Median marker (absolute positioned via a nested approach)
                .child(
                    div()
                        .absolute()
                        .left(px(mid_px))
                        .top(px(0.0))
                        .w(px(2.0))
                        .h(px(6.0))
                        .bg(rgb(theme::CYAN)),
                ),
        )
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .w(px(40.0))
                .child(format!("{:.0}", p95)),
        )
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
        // Error line (when failed)
        .when(agent.status == AgentStatus::Failed, |el| {
            el.child(
                div()
                    .pl(px(20.0))
                    .text_size(px(10.0))
                    .text_color(rgb(theme::RED))
                    .child(
                        agent
                            .findings_summary
                            .as_deref()
                            .unwrap_or("Unknown error")
                            .to_string(),
                    ),
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
// Agent result conversion
// ═══════════════════════════════════════════════════════════════════

/// Convert a typed `AgentExecutionResult` into a `JsonValue` for
/// `populate_from_agent_result`, which handles the heterogeneous
/// response shapes from different agents.
fn agent_result_to_json(result: &AgentExecutionResult) -> JsonValue {
    let mut obj = serde_json::Map::new();

    obj.insert(
        "agent_name".into(),
        JsonValue::String(result.agent_name.clone()),
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

    if let Some(ref metadata) = result.metadata {
        obj.insert("metadata".into(), metadata.clone());
    }

    JsonValue::Object(obj)
}

// ═══════════════════════════════════════════════════════════════════
// Sentiment detection (simple heuristic)
// ═══════════════════════════════════════════════════════════════════

/// Sanitize a driver name for use in FPL source.
/// FPL identifiers must be alphanumeric + underscore, no spaces.
fn sanitize_fpl_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Ensure it doesn't start with a digit
    if sanitized.starts_with(|c: char| c.is_ascii_digit()) {
        format!("d_{}", sanitized)
    } else if sanitized.is_empty() {
        "unnamed_driver".to_string()
    } else {
        sanitized
    }
}

/// Render a histogram as a row of vertical bars (sparkline style).
/// Each bar's height is proportional to the max bin count.
fn render_histogram_bars(bins: &[u32]) -> impl IntoElement {
    let max_count = bins.iter().copied().max().unwrap_or(1).max(1);
    let bar_height = 32.0_f32;

    div()
        .flex()
        .items_end()
        .gap(px(1.0))
        .h(px(bar_height + 4.0))
        .mt(px(4.0))
        .children(bins.iter().enumerate().map(move |(i, &count)| {
            let frac = count as f32 / max_count as f32;
            let h = (frac * bar_height).max(1.0);
            // Color gradient: low bins dim, high bins bright
            let color = if i < bins.len() / 4 || i > bins.len() * 3 / 4 {
                theme::FG_FAINT
            } else if i < bins.len() * 2 / 5 || i > bins.len() * 3 / 5 {
                theme::CYAN
            } else {
                theme::GREEN
            };
            div().w(px(6.0)).h(px(h)).bg(rgb(color)).rounded_t(px(1.0))
        }))
}

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
