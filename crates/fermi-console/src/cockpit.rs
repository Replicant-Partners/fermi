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
use std::collections::HashMap;
use std::sync::Arc;

use fermi::agent_backend::{
    llm_executor::LLMExecutor, registry::AgentRegistry, AgentOutput, ExecutionContext,
};
use fermi::ast::{
    AgentStmt, BaseRate, Distribution, DriverStmt, DriverType, EvidenceStmt, Expression,
    GeneratedBy, ModelStmt, Program, QuestionStmt, Schedule, SimulateStmt, Statement,
};

use crate::api::client::{AgentExecutionResult, ApiClient, CreateForecastRequest};
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
    pub evidence_source_input: Entity<TextInput>,
    pub evidence_summary_input: Entity<TextInput>,

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
    pub registry: Arc<AgentRegistry>,
    pub cached_fpl: String,
    pub inside_view_explanation: String,
    pub forecast_confidence: f64, // 0.0-1.0 overall confidence in the inside view
    /// User-set confidence per driver (driver_name → 0.0-1.0). Overrides computed confidence.
    pub driver_confidence: HashMap<String, f64>,
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
        Self {
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
            evidence_source_input,
            evidence_summary_input,
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
            registry,
            cached_fpl: String::new(),
            inside_view_explanation: String::new(),
            forecast_confidence: 0.5,
            driver_confidence: HashMap::new(),
        }
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
               \"drivers\": [{{\"name\": \"snake_case\", \"display_name\": \"Human Name\", \"type\": \"continuous\"|\"binary\", \"p5\": 0.8, \"p50\": 1.0, \"p95\": 1.3, \"unit\": \"multiplier\", \"rationale\": \"...\"}}],\n\
               \"evidence\": [{{\"source\": \"...\", \"summary\": \"...\", \"key_findings\": [\"...\"], \"relevance\": 0.0-1.0}}],\n\
               \"model_expression\": \"base_rate * driver_a * driver_b\",\n\
               \"confidence\": 0.0-1.0,\n\
               \"reasoning\": \"your analysis\"\n\
             }}\n\
             All continuous drivers MUST be probability multipliers near 1.0 (1.2 = +20%, 0.7 = -30%). \
             Include 3-6 independent drivers. Start from a real base rate with reference class.",
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
    fn process_macro_forecaster_result(&mut self, result: &JsonValue) {
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

        // Update agent status
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == "macro_forecaster")
        {
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
        }

        // Try to parse structured JSON from the agent's reasoning
        let reasoning = result
            .get("metadata")
            .and_then(|m| m.get("reasoning"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

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
                        }
                    } else {
                        let p5 = drv.get("p5").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let p50 = drv.get("p50").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let p95 = drv.get("p95").and_then(|v| v.as_f64()).unwrap_or(0.0);
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
                        }
                    };

                    // Replace scaffold driver if it exists, otherwise add
                    self.program.add_driver(driver_stmt);
                    log::info!(
                        "[composer] DRIVER ADDED from agent: {}",
                        sanitize_name(name)
                    );

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

        // ── Suggest agent assignments for new drivers ─────────────
        // Now that the decomposition is ready, suggest which research
        // agents would be good for each driver. User confirms via picker.
        let available = self.discover_research_agents();
        let driver_names: Vec<String> = self
            .program
            .drivers()
            .iter()
            .map(|d| d.name.clone())
            .collect();

        if !driver_names.is_empty() && !available.is_empty() {
            self.messages.push(AssistantMessage {
                node: "question".into(),
                kind: MessageKind::Suggestion,
                text: format!(
                    "Decomposition ready with {} drivers. Click '+ agent' on each driver to assign research agents with specific queries.",
                    driver_names.len()
                ),
            });

            // Suggest agents for drivers using data-driven skill/tag matching.
            // Each agent card has skills and tags — we match those against
            // driver name + rationale keywords. This works for ANY agent
            // (including future domain agents like biotech_analyst) without
            // hardcoding agent names.
            let cards = self.registry.list_cards().unwrap_or_default();
            let orchestra_cards: Vec<_> = cards
                .iter()
                .filter(|c| {
                    c.metadata.tags.iter().any(|t| t == "fermi-orchestra") && c.agent_id != "fermi"
                    // fermi is the orchestrator, not a research agent
                })
                .collect();

            for driver_name in &driver_names {
                let driver = self.program.driver(driver_name);
                let rationale = driver.and_then(|d| d.rationale.as_deref()).unwrap_or("");
                // Build a search string from driver name + rationale
                let search_text = format!(
                    "{} {}",
                    driver_name.replace('_', " ").to_lowercase(),
                    rationale.to_lowercase()
                );

                // Score each orchestra agent by how well its skills/tags match
                let mut best_match: Option<(&str, usize)> = None;
                for card in &orchestra_cards {
                    let mut score = 0usize;
                    // Check skills
                    for skill in &card.capabilities.skills {
                        let skill_words: Vec<&str> = skill.split('-').collect();
                        for word in &skill_words {
                            if word.len() > 2 && search_text.contains(word) {
                                score += 2;
                            }
                        }
                    }
                    // Check tags
                    for tag in &card.metadata.tags {
                        if tag == "fermi-orchestra" {
                            continue;
                        }
                        let tag_words: Vec<&str> = tag.split('-').collect();
                        for word in &tag_words {
                            if word.len() > 2 && search_text.contains(word) {
                                score += 1;
                            }
                        }
                    }
                    // Check description keywords
                    let desc_lower = card.metadata.description.to_lowercase();
                    let driver_words: Vec<&str> = search_text.split_whitespace().collect();
                    for word in &driver_words {
                        if word.len() > 3 && desc_lower.contains(word) {
                            score += 1;
                        }
                    }

                    if score > 0 {
                        if best_match.is_none() || score > best_match.unwrap().1 {
                            best_match = Some((&card.agent_id, score));
                        }
                    }
                }

                if let Some((agent_id, _score)) = best_match {
                    self.messages.push(AssistantMessage {
                        node: format!("driver:{}", driver_name),
                        kind: MessageKind::Suggestion,
                        text: format!(
                            "💡 Consider assigning '{}' to research '{}'",
                            agent_id, driver_name
                        ),
                    });
                }
            }
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
                log::info!("[composer] {} → ABW API execution (via tokio)", base_id);
                let handle = tokio::spawn(async move {
                    api_clone.execute_agent(&base_id_clone, &q_clone).await
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
                        // Route to the right processor based on agent type
                        log::info!("[composer] Routing {} to processor (base_id={})", tracking_id, base_id);
                        if base_id == "macro_forecaster" {
                            log::info!("[composer] → process_macro_forecaster_result");
                            state.process_macro_forecaster_result(&result_json);
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
                                    state.process_macro_forecaster_result(&patched);
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
                                    state.process_macro_forecaster_result(&patched);
                                } else {
                                    log::info!("[composer] → process_fermi_recommendation (no decomposition found, even from narrative)");
                                    state.process_fermi_recommendation(&result_json, cx);
                                }
                            }
                        } else {
                            // Other agents: add evidence to AST
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
                    input.set_text(query, cx);
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

        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == agent_id)
        {
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
            if let Some(run) = self
                .agent_runs
                .iter_mut()
                .find(|r| r.agent_name == agent_id)
            {
                run.evidence_count = count;
            }
        }
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

    /// Open the agent picker for a specific driver.
    pub fn open_agent_picker(&mut self, driver_name: &str, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.agent_search_query.clear();
        self.focused_node = FocusedNode::AgentPicker(driver_name.to_string());
        self.right_tab = RightTab::Edit;

        // Ask Fermi for agent recommendation in the background
        let driver = self.program.driver(driver_name);
        let driver_rationale = driver
            .and_then(|d| d.rationale.as_deref())
            .unwrap_or("")
            .to_string();
        let driver_type = driver
            .map(|d| format!("{:?}", d.driver_type))
            .unwrap_or_else(|| "unknown".into());
        let question = self
            .program
            .question()
            .map(|q| q.text.clone())
            .unwrap_or_default();

        // Build agent list dynamically from the registry
        let available_agents: Vec<String> = self
            .registry
            .list_cards()
            .unwrap_or_default()
            .iter()
            .filter(|c| {
                c.metadata.tags.iter().any(|t| t == "fermi-orchestra") && c.agent_id != "fermi"
            })
            .map(|c| {
                format!(
                    "- {}: {}",
                    c.agent_id,
                    c.metadata.description.chars().take(100).collect::<String>()
                )
            })
            .collect();
        let agent_list = if available_agents.is_empty() {
            "- macro_forecaster, market_research, sentiment_analyzer, entity_investigator"
                .to_string()
        } else {
            available_agents.join("\n")
        };

        let query = format!(
            "I'm building a forecast for: \"{}\"\n\n\
             I need to assign a research agent to the driver '{}' (type: {}).\n\
             Driver rationale: {}\n\n\
             Available agents:\n{}\n\n\
             IMPORTANT: Choose the most domain-specific agent. For NBA/basketball, use nba_analyst. \
             For biotech/pharma, use biotech_analyst. Only use general agents (market_research, \
             sentiment_analyzer, entity_investigator) when no domain agent fits.\n\n\
             Which agent is best for this driver? Suggest a specific research query.\n\
             Respond with JSON: {{\"recommended_agent\": \"...\", \"reasoning\": \"...\", \"suggested_query\": \"...\"}}",
            question, driver_name, driver_type, driver_rationale, agent_list
        );

        let dn = driver_name.to_string();
        self.fire_agent("fermi", &query, cx);
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
            node: format!("driver:{}", dn),
            kind: MessageKind::Info,
            text: format!("🦊 Fermi is analyzing which agent is best for '{}'…", dn),
        });

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

        self.focused_node = FocusedNode::Driver(driver_name.to_string());
        self.populate_editor_from_driver(driver_name, cx);
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
            let d_p5 = driver
                .distribution
                .as_ref()
                .map(|d| match d {
                    Distribution::Triangular { p5, .. } => expr_to_f64(p5),
                    _ => 0.0,
                })
                .unwrap_or(0.0);
            let d_p50 = driver
                .distribution
                .as_ref()
                .map(|d| match d {
                    Distribution::Triangular { p50, .. } => expr_to_f64(p50),
                    _ => 0.0,
                })
                .unwrap_or(0.0);
            let d_p95 = driver
                .distribution
                .as_ref()
                .map(|d| match d {
                    Distribution::Triangular { p95, .. } => expr_to_f64(p95),
                    _ => 0.0,
                })
                .unwrap_or(0.0);
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
        cx.notify();
    }

    /// Delete a driver from the program.
    /// Add manual evidence to the currently focused driver.
    pub fn add_manual_evidence(&mut self, cx: &mut Context<Self>) {
        let driver_name = match &self.focused_node {
            FocusedNode::Driver(n) => n.clone(),
            _ => return,
        };

        let source = self.evidence_source_input.read(cx).text().to_string();
        let summary = self.evidence_summary_input.read(cx).text().to_string();

        if source.trim().is_empty() && summary.trim().is_empty() {
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
            text: format!("Manual evidence added to '{}'", driver_name),
        });
        cx.notify();
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
        self.cached_fpl = generate_fpl_text(&self.program);

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
                    let p5 = self.editor_p5.read(cx).text().parse::<f64>().unwrap_or(0.0);
                    let p50 = self
                        .editor_p50
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(0.0);
                    let p95 = self
                        .editor_p95
                        .read(cx)
                        .text()
                        .parse::<f64>()
                        .unwrap_or(0.0);
                    let unit = self.editor_unit.read(cx).text().to_string();
                    driver.distribution = Some(Distribution::Triangular {
                        p5: Expression::Number(p5),
                        p50: Expression::Number(p50),
                        p95: Expression::Number(p95),
                    });
                    driver.unit = if unit.is_empty() { None } else { Some(unit) };
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
        self.save_focused_driver(cx);
        self.sim_running = true;
        self.sim_error = None;

        // Generate FPL and cache it
        self.cached_fpl = generate_fpl_text(&self.program);

        if self.program.drivers().is_empty() {
            self.sim_error = Some("No drivers defined. Add drivers first.".into());
            self.sim_running = false;
            cx.notify();
            return;
        }

        // Parse the generated FPL and execute
        let fpl = self.cached_fpl.clone();
        let start = std::time::Instant::now();

        let tokens = match ::fermi::lexer::Lexer::new(&fpl).tokenize() {
            Ok(t) => t,
            Err(e) => {
                self.sim_error = Some(format!("FPL tokenization error: {:?}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        let parsed = match ::fermi::parser::Parser::new(tokens).parse() {
            Ok(p) => p,
            Err(e) => {
                self.sim_error = Some(format!("FPL parse error: {}", e));
                self.sim_running = false;
                cx.notify();
                return;
            }
        };

        let mut executor = ::fermi::executor::Executor::new(10_000);
        match executor.execute(&parsed) {
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
                    histogram: histogram_data.iter().map(|(_, c)| *c as u32).collect(),
                });
                self.sim_running = false;

                // ── Update inside view from simulation ────────────
                // Update inside view from simulation
                // ── Normalize simulation output to probability ────
                // The simulation produces a raw model output. For probability
                // forecasts (has base_rate), we normalize deterministically:
                //
                // 1. Compute baseline: run model with all drivers at p50
                // 2. Compute ratio: sim_mean / baseline
                // 3. P = base_rate × ratio, clamped to [0.01, 0.99]
                //
                // This uses the executor deterministically — no LLM involved.
                let base_rate = self
                    .program
                    .question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency)
                    .unwrap_or(0.0);

                if base_rate > 0.0 {
                    // Compute baseline (all drivers at p50/median)
                    let mut fixed_drivers = std::collections::HashMap::new();
                    for driver in self.program.drivers() {
                        match driver.driver_type {
                            DriverType::Continuous => {
                                if let Some(Distribution::Triangular { ref p50, .. }) =
                                    driver.distribution
                                {
                                    fixed_drivers.insert(driver.name.clone(), expr_to_f64(p50));
                                }
                            }
                            DriverType::Binary => {
                                // Binary at expected value: probability * impact + (1-p) * 1.0
                                let p = driver.probability.unwrap_or(0.5);
                                let m = driver.impact_multiplier.unwrap_or(1.0);
                                fixed_drivers.insert(driver.name.clone(), p * m + (1.0 - p) * 1.0);
                            }
                            _ => {}
                        }
                    }

                    let mut baseline_executor =
                        ::fermi::executor::Executor::with_fixed_drivers(1, fixed_drivers);
                    let baseline = baseline_executor.execute(&parsed);
                    let baseline_mean = baseline.as_ref().map(|r| r.mean).unwrap_or(results.mean);

                    // Normalize: P = base_rate × (sim_mean / baseline_mean)
                    let ratio = if baseline_mean.abs() > 0.001 {
                        results.mean / baseline_mean
                    } else {
                        1.0
                    };
                    self.predicted_probability = (base_rate * ratio).clamp(0.01, 0.99);

                    self.predicted_probability = (base_rate * ratio).clamp(0.01, 0.99);

                    // Build narrative explanation
                    let direction = if ratio > 1.05 {
                        "increases"
                    } else if ratio < 0.95 {
                        "decreases"
                    } else {
                        "confirms"
                    };
                    let strength = if (ratio - 1.0).abs() > 0.3 {
                        "significantly"
                    } else if (ratio - 1.0).abs() > 0.1 {
                        "moderately"
                    } else {
                        "slightly"
                    };

                    // Find the most influential drivers from the sensitivity analysis
                    let top_drivers: Vec<String> = self
                        .program
                        .drivers()
                        .iter()
                        .take(3)
                        .map(|d| d.display_name.as_deref().unwrap_or(&d.name).to_string())
                        .collect();

                    self.inside_view_explanation = format!(
                        "Starting from a {:.1}% base rate, our model {} {} the probability to {:.1}%. \
                         The key factors are: {}.",
                        base_rate * 100.0,
                        strength,
                        direction,
                        self.predicted_probability * 100.0,
                        top_drivers.join(", "),
                    );
                } else {
                    // No base rate — use raw mean or clamp
                    if results.mean >= 0.0 && results.mean <= 1.0 {
                        self.predicted_probability = results.mean;
                    } else {
                        self.predicted_probability = 0.5;
                    }
                    self.inside_view_explanation = format!(
                        "Raw model output: {:.2} (no base rate for normalization)",
                        results.mean
                    );
                }
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
        self.cached_fpl = generate_fpl_text(&self.program);
    }

    // ═══════════════════════════════════════════════════════════════
    // Publish + Version
    // ═══════════════════════════════════════════════════════════════

    pub fn load_forecast(&mut self, path: &str, cx: &mut Context<Self>) {
        self.messages.clear();
        let state_path = path.replace(".fpl", ".state.json");

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
        cx.notify();
    }
    pub fn save_forecast(&mut self, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.cached_fpl = generate_fpl_text(&self.program);

        // Create version snapshot
        self.current_version += 1;
        self.versions.push(ForecastVersion {
            version: self.current_version,
            timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M").to_string(),
            fpl_text: self.cached_fpl.clone(),
            probability: self.predicted_probability,
            change_summary: if self.current_version == 1 {
                "Initial forecast".into()
            } else {
                format!("v{} update", self.current_version)
            },
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
                );
                match std::fs::write(&wiki_path, &wiki) {
                    Ok(_) => log::info!("[composer] Saved evidence wiki to {}", wiki_path),
                    Err(e) => log::warn!("[composer] Failed to save evidence wiki: {}", e),
                }

                // Save state.json (versions, probability, sim results)
                let state_path = format!("forecasts/{}.state.json", filename);
                let state_json = serde_json::json!({
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
                });
                match std::fs::write(
                    &state_path,
                    serde_json::to_string_pretty(&state_json).unwrap_or_default(),
                ) {
                    Ok(_) => log::info!("[composer] Saved state to {}", state_path),
                    Err(e) => log::warn!("[composer] Failed to save state: {}", e),
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

    pub fn publish_forecast(&mut self, cx: &mut Context<Self>) {
        self.save_focused_driver(cx);
        self.cached_fpl = generate_fpl_text(&self.program);

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

        self.publish_status = Some("Publishing…".into());
        cx.notify();

        let req = CreateForecastRequest {
            question_text: question,
            predicted_probability: self.predicted_probability,
            domain: None,
            resolution_criteria: self.program.question().and_then(|q| q.resolution_criteria.clone()),
            target_date: self.program.question().and_then(|q| q.target_date.clone()),
            confidence_interval_low: self.sim_results.as_ref().map(|s| s.p5),
            confidence_interval_high: self.sim_results.as_ref().map(|s| s.p95),
            fpl_source: Some(self.cached_fpl.clone()),
            simulation_results: self.sim_results.as_ref().map(|s| {
                serde_json::json!({ "mean": s.mean, "median": s.median, "p5": s.p5, "p95": s.p95 })
            }),
            drivers: None,
            evidence: None,
            visibility: Some("private".into()),
            tags: None,
            portfolio_id: None,
            status: Some("active".into()),
        };

        let api = self.api.clone();
        cx.spawn(
            async move |this, cx| match api.create_forecast(&req).await {
                Ok(resp) => {
                    let fid = resp
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    this.update(cx, |state, cx| {
                        state.forecast_id = Some(fid.clone());
                        state.publish_status =
                            Some(format!("Published v{}", state.current_version));
                        state.messages.push(AssistantMessage {
                            node: "publish".into(),
                            kind: MessageKind::Info,
                            text: format!(
                                "Forecast published as v{} (ID: {})",
                                state.current_version, fid
                            ),
                        });
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |state, cx| {
                        state.publish_status = Some(format!("Failed: {}", e));
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }
}

// ═══════════════════════════════════════════════════════════════════
// Render — the FPL program tree drives the UI
// ═══════════════════════════════════════════════════════════════════

impl Render for CockpitState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let driver_names: Vec<String> = self
            .program
            .drivers()
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let focused = self.focused_node.clone();

        div()
            .flex()
            .size_full()
            .bg(rgb(theme::BG))
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
                    .child(render_forecast_index(self))
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
                                    .child(format!("Drivers ({})", driver_names.len())),
                            )
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
                                render_driver_card(
                                    i,
                                    driver,
                                    is_focused,
                                    &assigned_agents,
                                    &self.agent_runs,
                                    &self.messages,
                                    &self.program.evidence_items(),
                                    uc,
                                    cx,
                                    &n,
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
                            }),
                    )
                    // Assistant messages (always visible below tabs)
                    .child(render_assistant_panel(&self.messages)),
            )
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
            div()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    div()
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
                            .min_w(px(0.0))
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
                    el.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::GREEN))
                            .child(state.publish_status.as_deref().unwrap_or("").to_string()),
                    )
                }),
        )
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .text_size(px(10.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("Ctrl+Enter research")
                .child("Ctrl+R simulate")
                .child("Ctrl+P publish")
                .child("Ctrl+N new")
                .child("Ctrl+O import")
                .child("Ctrl+S save")
                .child("Ctrl+E tabs"),
        )
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
    cx: &mut Context<CockpitState>,
    name: &str,
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

    let has_evidence_gap =
        assigned_agents.is_empty() && evidence_items.iter().all(|e| !e.id.contains(name));
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
                        .child(
                            driver
                                .display_name
                                .as_deref()
                                .unwrap_or(&driver.name)
                                .to_string(),
                        ),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(type_color))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(2.0))
                        .bg(rgb(theme::BG))
                        .child(type_label),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_DIM))
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
                            let chart_w = 120u32;
                            let chart_h = 24u32;
                            let rgb_buf = crate::charts::render_distribution_sparkline(
                                v5, v50, v95, chart_w, chart_h,
                            );
                            let render_img =
                                crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                            el.child(
                                gpui::img(gpui::ImageSource::Render(render_img))
                                    .w(gpui::px(chart_w as f32))
                                    .h(gpui::px(chart_h as f32)),
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
                }),
        )
        // Driver confidence dots (based on evidence coverage or user override)
        .child({
            let ev_count = assigned_agents
                .iter()
                .flat_map(|a| agent_runs.iter().filter(move |r| r.agent_name == *a))
                .map(|r| r.evidence_count)
                .sum::<usize>();
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
                                                if latest.len() > 100 {
                                                    format!("{}…", &latest[..97])
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
    // Get fermi-orchestra agents from the registry
    let available_agents: Vec<(String, String, String, Vec<String>)> = state
        .registry
        .list_cards()
        .unwrap_or_default()
        .iter()
        .filter(|card| card.metadata.tags.iter().any(|t| t == "fermi-orchestra"))
        .map(|card| {
            (
                card.agent_id.clone(),
                card.metadata.description.clone(),
                card.capabilities.model.clone(),
                card.capabilities.skills.clone(),
            )
        })
        .collect();

    let dn = driver_name.to_string();

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(16.0))
        .border_b_1()
        .border_color(rgb(theme::FG_FAINT))
        // Header
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(rgb(theme::BLUE))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("Assign Agent → {}", driver_name)),
                )
                .child({
                    let dn2 = dn.clone();
                    div()
                        .id("close-agent-picker")
                        .text_size(px(12.0))
                        .text_color(rgb(theme::FG_DIM))
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG)))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            this.focused_node = FocusedNode::Driver(dn2.clone());
                            this.populate_editor_from_driver(&dn2, cx);
                            cx.notify();
                        }))
                        .child("✕ Cancel")
                }),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .child("Recommended research agents:"),
        )
        // Custom query input
        .child(state.agent_query_input.clone())
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgb(theme::FG_FAINT))
                .child("Define what this agent should research for this driver, or leave blank for default."),
        )
        // Agent list
        .children(
            available_agents
                .iter()
                .map(|(agent_id, description, model, skills)| {
                    let aid = agent_id.clone();
                    let dn3 = dn.clone();
                    div()
                        .id(ElementId::Name(format!("pick-agent-{}", agent_id).into()))
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG))
                        .border_1()
                        .border_color(rgb(theme::FG_FAINT))
                        .cursor_pointer()
                        .hover(|s| s.border_color(rgb(theme::BLUE)).bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(move |this, _event, _window, cx| {
                            // Card click does nothing — use schedule buttons below
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(agent_id.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(model.clone()),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .min_w(px(0.0))
                                .child(description.clone()),
                        )
                        .when(!skills.is_empty(), |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(4.0))
                                    .children(skills.iter().take(4).map(|s| {
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::CYAN))
                                            .px(px(4.0))
                                            .py(px(1.0))
                                            .rounded(px(2.0))
                                            .bg(rgb(theme::BG_ACTIVE))
                                            .child(s.clone())
                                    })),
                            )
                        })
                        // Schedule buttons
                        .child({
                            let aid_once = agent_id.clone();
                            let aid_daily = agent_id.clone();
                            let aid_weekly = agent_id.clone();
                            let dn_once = dn.clone();
                            let dn_daily = dn.clone();
                            let dn_weekly = dn.clone();
                            div()
                                .flex()
                                .gap(px(4.0))
                                .mt(px(4.0))
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("sched-once-{}", agent_id).into()))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::CYAN))
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(theme::BG_ACTIVE))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.assign_agent_to_driver(&dn_once, &aid_once, Schedule::Once, cx);
                                        }))
                                        .child("Run once"),
                                )
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("sched-daily-{}", agent_id).into()))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::GREEN))
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(theme::BG_ACTIVE))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.assign_agent_to_driver(&dn_daily, &aid_daily,
                                                Schedule::Every { interval: 1, unit: fermi::ast::TimeUnit::Day }, cx);
                                        }))
                                        .child("Daily"),
                                )
                                .child(
                                    div()
                                        .id(ElementId::Name(format!("sched-weekly-{}", agent_id).into()))
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::GOLD))
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(px(3.0))
                                        .bg(rgb(theme::BG_ACTIVE))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.assign_agent_to_driver(&dn_weekly, &aid_weekly,
                                                Schedule::Every { interval: 1, unit: fermi::ast::TimeUnit::Week }, cx);
                                        }))
                                        .child("Weekly"),
                                )
                        })
                }),
        )
        .when(available_agents.is_empty(), |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::FG_DIM))
                    .child("No research agents found in registry."),
            )
        })
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
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::FG_FAINT))
                                            .child(ev.source.clone()),
                                    )
                                    .when(ev.relevance.is_some(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(rgb(theme::CYAN))
                                                .child(format!(
                                                    "{:.0}%",
                                                    ev.relevance.unwrap_or(0.0) * 100.0
                                                )),
                                        )
                                    }),
                            )
                            .when(ev.summary.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG))
                                        .child(ev.summary.as_deref().unwrap_or("").to_string()),
                                )
                            })
                            .when(!ev.key_findings.is_empty(), |el| {
                                el.children(ev.key_findings.iter().take(3).map(|f| {
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgb(theme::FG_DIM))
                                        .child(format!("• {}", f))
                                }))
                            })
                    })),
            )
        })
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
    div()
        .flex()
        .flex_col()
        .max_h(px(200.0))
        .overflow_hidden()
        .border_t_1()
        .border_color(rgb(theme::FG_FAINT))
        .p(px(12.0))
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::CYAN))
                .font_weight(FontWeight::SEMIBOLD)
                .child("FPL Assistant"),
        )
        .children(messages.iter().rev().take(15).map(|msg| {
            let (icon, color) = match msg.kind {
                MessageKind::Suggestion => ("💡", theme::CYAN),
                MessageKind::Warning => ("⚠", theme::GOLD),
                MessageKind::Info => ("ℹ", theme::FG_DIM),
                MessageKind::Error => ("✗", theme::RED),
                MessageKind::Tip => ("🦊", theme::GREEN),
            };
            div()
                .flex()
                .gap(px(6.0))
                .py(px(3.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .w(px(16.0))
                        .child(icon.to_string()),
                )
                .child(
                    div()
                        .flex_grow()
                        .min_w(px(0.0))
                        .text_size(px(11.0))
                        .text_color(rgb(color))
                        .child(msg.text.clone()),
                )
        }))
}

/// Forecast Index — the key visualization section shown above drivers.
/// Displays inside/outside divergence, simulation stats, histogram,
/// index comparison chart, and evidence treemap.
fn render_forecast_index(state: &CockpitState) -> impl IntoElement {
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
            let br = state.program.question().unwrap().base_rate.as_ref().unwrap();
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
                                            .child(format!("Outside {:.1}%", outside * 100.0)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(theme::CYAN))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!("Inside {:.1}%", inside * 100.0)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(rgb(div_color))
                                            .child(format!(
                                                "{}pp divergence",
                                                if divergence > 0.0 {
                                                    format!("+{:.0}", divergence)
                                                } else {
                                                    format!("{:.0}", divergence)
                                                }
                                            )),
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
                            .h(px(8.0))
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
                                    .h(px(8.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::GOLD)),
                            )
                            // Inside view marker (cyan)
                            .child(
                                div()
                                    .absolute()
                                    .top(px(0.0))
                                    .left(gpui::px(inside_x - 2.0))
                                    .w(px(4.0))
                                    .h(px(8.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::CYAN)),
                            )
                            // Fill between the two markers
                            .child(
                                div()
                                    .absolute()
                                    .top(px(2.0))
                                    .left(gpui::px(outside_x.min(inside_x)))
                                    .w(gpui::px((outside_x - inside_x).abs()))
                                    .h(px(4.0))
                                    .bg(rgb(div_color)),
                            ),
                    )
                    // 0% and 100% scale labels
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .w(gpui::px(bar_w))
                            .child(div().text_size(px(8.0)).text_color(rgb(theme::FG_FAINT)).child("0%"))
                            .child(div().text_size(px(8.0)).text_color(rgb(theme::FG_FAINT)).child("50%"))
                            .child(div().text_size(px(8.0)).text_color(rgb(theme::FG_FAINT)).child("100%")),
                    ),
            )
        })
        // ── Simulation results ────────────────────────────────────
        .when(state.sim_running, |el| {
            el.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::GOLD))
                    .child("⟳ Running Monte Carlo simulation…"),
            )
        })
        .when(state.sim_error.is_some(), |el| {
            el.child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::RED))
                    .child(format!("✗ {}", state.sim_error.as_deref().unwrap_or(""))),
            )
        })
        .when(has_sim, |el| {
            let sim = state.sim_results.as_ref().unwrap();
            el
                // Stats row
                .child(
                    div()
                        .px(px(12.0))
                        .flex()
                        .gap(px(16.0))
                        .text_size(px(11.0))
                        .child(render_stat("mean", sim.mean, theme::FG))
                        .child(render_stat("p5", sim.p5, theme::FG_DIM))
                        .child(render_stat("p50", sim.median, theme::CYAN))
                        .child(render_stat("p95", sim.p95, theme::FG_DIM))
                        .child(render_stat("σ", sim.std_dev, theme::FG_FAINT))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child(format!(
                                    "{}k iters · {}ms",
                                    sim.iterations / 1000,
                                    sim.execution_time_ms
                                )),
                        ),
                )
                // Histogram
                .when(!sim.histogram.is_empty(), |el| {
                    el.child(
                        div().px(px(12.0)).child(render_histogram(&sim.histogram)),
                    )
                })
                // Index comparison chart
                .when(state.versions.len() > 0, |el| {
                    let base_rate = state
                        .program
                        .question()
                        .and_then(|q| q.base_rate.as_ref())
                        .map(|br| br.historical_frequency * 100.0)
                        .unwrap_or(50.0);

                    let history: Vec<crate::charts::IndexPoint> = state
                        .versions
                        .iter()
                        .map(|v| crate::charts::IndexPoint {
                            label: format!("v{}", v.version),
                            inside_view: v.probability * 100.0,
                            outside_view: base_rate,
                        })
                        .collect();

                    let chart_w = 400u32;
                    let chart_h = 100u32;
                    let rgb_buf = crate::charts::render_index_chart(
                        &history,
                        history.len().saturating_sub(1),
                        chart_w,
                        chart_h,
                    );
                    let render_img =
                        crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);

                    el.child(
                        div()
                            .px(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child("Inside (blue) vs Outside (gold) over versions"),
                            )
                            .child(
                                gpui::img(gpui::ImageSource::Render(render_img))
                                    .w(gpui::px(chart_w as f32))
                                    .h(gpui::px(chart_h as f32)),
                            ),
                    )
                })
        })
        // ── Evidence Treemap — always show if drivers exist ───────
        .when(has_drivers, |el| {
            let drivers_viz: Vec<crate::charts::DriverViz> = state
                .program
                .drivers()
                .iter()
                .map(|d| {
                    let display = d.display_name.as_deref().unwrap_or(&d.name);
                    let impact = match d.driver_type {
                        DriverType::Continuous => {
                            if let Some(Distribution::Triangular {
                                ref p5, ref p95, ..
                            }) = d.distribution
                            {
                                (expr_to_f64(p95) - expr_to_f64(p5)).abs().max(0.1)
                            } else {
                                1.0
                            }
                        }
                        DriverType::Binary => {
                            d.probability.unwrap_or(0.5)
                                * d.impact_multiplier.unwrap_or(1.0)
                                * 10.0
                        }
                        _ => 1.0,
                    };
                    let evidence_count = state
                        .program
                        .evidence_items()
                        .iter()
                        .filter(|e| {
                            e.id.contains(&d.name)
                                || state
                                    .program
                                    .agents()
                                    .iter()
                                    .filter(|a| a.driver_refs.contains(&d.name))
                                    .any(|a| evidence_matches_agent(e, &a.name))
                        })
                        .count();
                    let quality = if evidence_count > 2 {
                        0.8
                    } else if evidence_count > 0 {
                        0.5
                    } else {
                        0.2
                    };
                    crate::charts::DriverViz {
                        name: display.to_string(),
                        impact,
                        quality,
                        evidence: state
                            .program
                            .evidence_items()
                            .iter()
                            .filter(|e| e.id.contains(&d.name))
                            .filter_map(|e| {
                                e.summary.as_ref().map(|s| s.chars().take(40).collect())
                            })
                            .take(3)
                            .collect(),
                    }
                })
                .collect();

            if !drivers_viz.is_empty() {
                let chart_w = 400u32;
                let chart_h = 140u32;
                let rgb_buf = crate::charts::render_treemap(&drivers_viz, chart_w, chart_h);
                let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                el.child(
                    div()
                        .px(px(12.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child("Driver Impact (size) × Evidence Quality (color: green=strong, gold=partial, red=none)"),
                        )
                        .child(
                            gpui::img(gpui::ImageSource::Render(render_img))
                                .w(gpui::px(chart_w as f32))
                                .h(gpui::px(chart_h as f32)),
                        ),
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
                        .py(px(4.0))
                        .text_size(px(10.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Ctrl+R to run Monte Carlo simulation"),
                )
            },
        )
}

fn render_simulation_section(_state: &CockpitState) -> impl IntoElement {
    // Simulation results now rendered in render_forecast_index above drivers
    div()
}

fn render_stat(label: &str, value: f64, color: u32) -> impl IntoElement {
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
                .child(format!("{:.1}", value)),
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
                        this.cached_fpl = generate_fpl_text(&this.program);
                    }
                    cx.notify();
                }))
                .child(label.to_string())
        }))
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
                }),
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
                        .bg(rgb(0x2A2210))
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
        // ── Forecast Index Charts (same as left panel) ────────────
        .when(
            state.sim_results.is_some() || !state.program.drivers().is_empty(),
            |el| {
                let mut chart_children: Vec<gpui::AnyElement> = Vec::new();

                // Histogram
                if let Some(ref sim) = state.sim_results {
                    if !sim.histogram.is_empty() {
                        let chart_w = 500u32;
                        let chart_h = 100u32;
                        let rgb_buf =
                            crate::charts::render_histogram_chart(&sim.histogram, chart_w, chart_h);
                        let render_img =
                            crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
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
                                            "Simulation Distribution ({}k iterations)",
                                            sim.iterations / 1000
                                        )),
                                )
                                .child(
                                    gpui::img(gpui::ImageSource::Render(render_img))
                                        .w(gpui::px(chart_w as f32))
                                        .h(gpui::px(chart_h as f32)),
                                )
                                .into_any_element(),
                        );
                    }
                }

                // Index comparison chart
                if state.versions.len() > 1 {
                    let base_rate = state
                        .program
                        .question()
                        .and_then(|q| q.base_rate.as_ref())
                        .map(|br| br.historical_frequency * 100.0)
                        .unwrap_or(50.0);
                    let history: Vec<crate::charts::IndexPoint> = state
                        .versions
                        .iter()
                        .map(|v| crate::charts::IndexPoint {
                            label: format!("v{}", v.version),
                            inside_view: v.probability * 100.0,
                            outside_view: base_rate,
                        })
                        .collect();
                    let chart_w = 500u32;
                    let chart_h = 80u32;
                    let rgb_buf = crate::charts::render_index_chart(
                        &history,
                        history.len().saturating_sub(1),
                        chart_w,
                        chart_h,
                    );
                    let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                    chart_children.push(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_FAINT))
                                    .child("Inside (cyan) vs Outside (gold) over versions"),
                            )
                            .child(
                                gpui::img(gpui::ImageSource::Render(render_img))
                                    .w(gpui::px(chart_w as f32))
                                    .h(gpui::px(chart_h as f32)),
                            )
                            .into_any_element(),
                    );
                }

                // Evidence treemap
                if !drivers.is_empty() {
                    let drivers_viz: Vec<crate::charts::DriverViz> = drivers
                        .iter()
                        .map(|d| {
                            let display = d.display_name.as_deref().unwrap_or(&d.name);
                            let impact = match d.driver_type {
                                DriverType::Continuous => {
                                    if let Some(Distribution::Triangular {
                                        ref p5, ref p95, ..
                                    }) = d.distribution
                                    {
                                        (expr_to_f64(p95) - expr_to_f64(p5)).abs().max(0.1)
                                    } else {
                                        1.0
                                    }
                                }
                                DriverType::Binary => {
                                    d.probability.unwrap_or(0.5)
                                        * d.impact_multiplier.unwrap_or(1.0)
                                        * 10.0
                                }
                                _ => 1.0,
                            };
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
                            let quality = if ev_count > 2 {
                                0.8
                            } else if ev_count > 0 {
                                0.5
                            } else {
                                0.2
                            };
                            crate::charts::DriverViz {
                                name: display.to_string(),
                                impact,
                                quality,
                                evidence: evidence
                                    .iter()
                                    .filter(|e| e.id.contains(&d.name))
                                    .filter_map(|e| {
                                        e.summary.as_ref().map(|s| s.chars().take(40).collect())
                                    })
                                    .take(3)
                                    .collect(),
                            }
                        })
                        .collect();

                    if !drivers_viz.is_empty() {
                        let chart_w = 500u32;
                        let chart_h = 160u32;
                        let rgb_buf = crate::charts::render_treemap(&drivers_viz, chart_w, chart_h);
                        let render_img =
                            crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                        chart_children.push(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child("Driver Impact (size) × Evidence Quality (color)"),
                                )
                                .child(
                                    gpui::img(gpui::ImageSource::Render(render_img))
                                        .w(gpui::px(chart_w as f32))
                                        .h(gpui::px(chart_h as f32)),
                                )
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

            // Evidence quality badge
            let ev_count = driver_ev.len();
            let (quality_label, quality_color) = if ev_count >= 3 {
                ("Strong", theme::GREEN)
            } else if ev_count >= 1 {
                ("Partial", theme::GOLD)
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
                    let summary = ev.summary.as_deref().unwrap_or("");
                    // Show full summary up to 800 chars (was 300)
                    let display_summary = if summary.len() > 800 {
                        format!("{}…", &summary[..797])
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
                                }),
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
                                    let display = if s.len() > 500 {
                                        format!("{}…", &s[..497])
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

                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(4.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG_DIM))
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
            md.push_str(&format!("### Evidence ({})\n\n", driver_evidence.len()));
            for ev in &driver_evidence {
                let relevance_pct = ev.relevance.unwrap_or(0.0) * 100.0;
                let date_str = ev
                    .date
                    .as_deref()
                    .map(|d| format!(" · {}", d))
                    .unwrap_or_default();
                md.push_str(&format!(
                    "#### {} — relevance {:.0}%{}\n\n",
                    ev.source, relevance_pct, date_str
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
            md.push_str(&format!(
                "### {} — relevance {:.0}%\n\n",
                ev.source,
                ev.relevance.unwrap_or(0.0) * 100.0
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

fn detect_domain(question: &str) -> String {
    let q = question.to_lowercase();
    if q.contains("stock")
        || q.contains("price")
        || q.contains("market")
        || q.contains("revenue")
        || q.contains("valuation")
    {
        "finance".into()
    } else if q.contains("election")
        || q.contains("vote")
        || q.contains("president")
        || q.contains("congress")
    {
        "politics".into()
    } else if q.contains("ai")
        || q.contains("tech")
        || q.contains("software")
        || q.contains("launch")
    {
        "technology".into()
    } else if q.contains("climate") || q.contains("temperature") || q.contains("carbon") {
        "climate".into()
    } else {
        "general".into()
    }
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
    // Known agent base names
    let known = [
        "macro_forecaster",
        "market_research",
        "sentiment_analyzer",
        "entity_investigator",
        "monte_carlo_sim",
        "fermi",
    ];
    for base in &known {
        if compound_name.starts_with(base) {
            return base;
        }
    }
    compound_name
}

/// Check if an evidence item is linked to an agent (by base name match).
fn evidence_matches_agent(evidence: &EvidenceStmt, agent_name: &str) -> bool {
    let base = base_agent_name(agent_name);
    evidence.source.contains(base) || evidence.id.contains(agent_name)
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
