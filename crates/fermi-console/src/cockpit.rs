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
    pub forecast_confidence: f64,  // 0.0-1.0 overall confidence in the inside view
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
        // Fermi is the meta-forecaster — it creates the decomposition with
        // probability-compatible drivers. The model output should be in [0,1].
        // Specialized agents research evidence per driver afterward.
        let structured_query = format!(
            "You are Fermi, the meta-forecasting agent. Decompose this forecast question into a \
             probabilistic model where the output is a PROBABILITY between 0 and 1.\n\n\
             Question: \"{}\"\n\n\
             IMPORTANT: All continuous drivers must be probability multipliers (values near 1.0). \
             The model should be: base_rate × multiplier_1 × multiplier_2 × ... \
             For example, a driver that increases probability by 20% has p50=1.2. \
             A driver that decreases probability by 30% has p50=0.7.\n\n\
             Binary drivers use probability (0-1) and impact_multiplier.\n\n\
             Provide a JSON response with:\n\
             - \"base_rate\": {{\"reference_class\": \"...\", \"historical_frequency\": 0.0-1.0, \"sample_size\": N, \"reasoning\": \"...\"}}\n\
             - \"drivers\": [{{\"name\": \"snake_case\", \"display_name\": \"Human Name\", \"type\": \"continuous\"|\"binary\", \
               \"p5\": 0.8, \"p50\": 1.0, \"p95\": 1.3, \"unit\": \"multiplier\", \"rationale\": \"...\"}}]\n\
             - \"evidence\": [{{\"source\": \"...\", \"summary\": \"...\", \"key_findings\": [...], \"relevance\": 0.0-1.0}}]\n\
             - \"model_expression\": \"base_rate * driver_a * driver_b * (if event then impact else 1.0)\"\n\
             - \"confidence\": 0.0-1.0\n\
             - \"reasoning\": \"your analysis\"\n\n\
             The model_expression MUST start with the base_rate value and multiply by driver adjustments.\n\
             Return ONLY valid JSON.",
            question
        );

        self.agent_runs.push(AgentExecution {
            agent_name: "fermi".into(),
            status: AgentRunStatus::Running,
            evidence_count: 0,
            confidence: None,
            error: None,
            credits_charged: None,
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
        log::info!("[composer] Processing result keys: {:?}", 
            result.as_object().map(|o| o.keys().collect::<Vec<_>>()));
        if let Some(evidence) = result.get("evidence").and_then(|v| v.as_array()) {
            for (i, ev) in evidence.iter().enumerate() {
                let summary_len = ev.get("summary").and_then(|v| v.as_str()).map(|s| s.len()).unwrap_or(0);
                let findings_count = ev.get("key_findings").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
                log::info!("[composer] Evidence[{}]: summary_len={}, findings_count={}", i, summary_len, findings_count);
                if let Some(findings) = ev.get("key_findings").and_then(|v| v.as_array()) {
                    for (j, f) in findings.iter().enumerate().take(3) {
                        log::info!("[composer]   finding[{}]: {}", j, f.as_str().unwrap_or("?").chars().take(100).collect::<String>());
                    }
                }
            }
        }
        let reasoning = result.get("metadata").and_then(|m| m.get("reasoning")).and_then(|v| v.as_str()).unwrap_or("");
        log::info!("[composer] Reasoning length: {}, starts with: {}", reasoning.len(), reasoning.chars().take(100).collect::<String>());

        // Update agent status
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_name == "macro_forecaster")
        {
            run.status = AgentRunStatus::Completed;
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
            .strip_prefix("```json").or_else(|| reasoning.trim().strip_prefix("```"))
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
            log::info!("[composer] Agent returned text (not JSON) - using as evidence. First 80 chars: {}", 
                &reasoning.chars().take(80).collect::<String>());
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
                log::info!("[composer] BASE RATE SET: {:.0}% ref_class={}", freq * 100.0, ref_class);

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
                let template_names: Vec<String> = self.program.drivers()
                    .iter().map(|d| d.name.clone()).collect();
                for name in &template_names {
                    self.program.remove_driver(&name);
                }
                // Also clear the template model — agent may suggest a new one
                let cleared_count = template_names.len();
                log::info!("[composer] Cleared {} template drivers, replacing with agent suggestions", cleared_count);
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
                    log::info!("[composer] DRIVER ADDED from agent: {}", sanitize_name(name));

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
            let model_parts: Vec<String> = new_drivers.iter().map(|d| {
                match d.driver_type {
                    DriverType::Binary => {
                        let m = d.impact_multiplier.unwrap_or(1.3);
                        format!("(if {} then {} else 1.0)", d.name, m)
                    }
                    _ => d.name.clone(),
                }
            }).collect();
            if !model_parts.is_empty() {
                // Try to use agent's suggested model expression if it references our drivers
                let agent_model = data.get("model_expression").and_then(|v| v.as_str()).unwrap_or("");
                let use_agent_model = !agent_model.is_empty() && 
                    new_drivers.iter().any(|d| agent_model.contains(&d.name));
                
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
                self.program.statements.retain(|s| !matches!(s, Statement::Model(_)));
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
        let driver_names: Vec<String> = self.program.drivers()
            .iter().map(|d| d.name.clone()).collect();

        if !driver_names.is_empty() && !available.is_empty() {
            self.messages.push(AssistantMessage {
                node: "question".into(),
                kind: MessageKind::Suggestion,
                text: format!(
                    "Decomposition ready with {} drivers. Click '+ agent' on each driver to assign research agents with specific queries.",
                    driver_names.len()
                ),
            });

            // Suggest specific agents for drivers based on keywords
            for driver_name in &driver_names {
                let dn_lower = driver_name.to_lowercase();
                let suggested = if dn_lower.contains("sentiment") || dn_lower.contains("opinion") || dn_lower.contains("perception") {
                    Some("sentiment_analyzer")
                } else if dn_lower.contains("market") || dn_lower.contains("competition") || dn_lower.contains("betting") {
                    Some("market_research")
                } else if dn_lower.contains("entity") || dn_lower.contains("company") || dn_lower.contains("ownership") {
                    Some("entity_investigator")
                } else {
                    None
                };

                if let Some(agent) = suggested {
                    self.messages.push(AssistantMessage {
                        node: format!("driver:{}", driver_name),
                        kind: MessageKind::Suggestion,
                        text: format!("💡 Consider assigning '{}' to research '{}'", agent, driver_name),
                    });
                }
            }
        }
    }


    /// Discover research-relevant agents from the local registry.
    /// Filters by agent_type and tags to find agents suitable for forecasting.
    fn discover_research_agents(&self) -> Vec<(String, String)> {
        let cards = self.registry.list_cards().unwrap_or_default();
        cards.iter()
            .filter(|card| {
                // Only agents tagged for the Fermi forecasting orchestra
                card.metadata.tags.iter().any(|t| t == "fermi-orchestra")
            })
            .map(|card| (card.agent_id.clone(), card.metadata.description.clone()))
            .collect()
    }

    /// Fire a single agent in the background. Results flow back via cx.spawn.
    fn fire_agent(&self, agent_id: &str, query: &str, cx: &mut Context<Self>) {
        // agent_id may be compound (market_research_song_quality)
        // Registry knows the base name (market_research)
        let base_id = base_agent_name(agent_id).to_string();
        let tracking_id = agent_id.to_string();
        let registry = self.registry.clone();
        let q = query.to_string();
        

        cx.spawn(async move |this, cx| {
            log::info!("[composer] Firing {} (registry: {}) ", tracking_id, base_id);

            let card = match registry.get(&base_id) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("[composer] {} not found: {}", base_id, e);
                    this.update(cx, |state, cx| {
                        state.mark_agent_failed(&tracking_id, &format!("Not in registry: {}", e));
                        cx.notify();
                    }).ok();
                    return;
                }
            };

            let agent_stmt = AgentStmt {
                name: base_id.clone(),
                agent_type: Some("research".into()),
                query: q,
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

            match registry.execute_agent(&agent_stmt, &context).await {
                Ok(output) => {
                    log::info!("[composer] {} completed: {} evidence, confidence={:.2}",
                        tracking_id, output.evidence.len(), output.confidence);

                    let result_json = serde_json::json!({
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
                    });

                    let findings: Vec<String> = output.evidence.iter()
                        .flat_map(|e| e.key_findings.iter().cloned())
                        .take(5)
                        .collect();

                    this.update(cx, |state, cx| {
                        // macro_forecaster gets special processing (base rate + drivers)
                        // macro_forecaster gets special processing (base rate + drivers)
                        if base_id == "macro_forecaster" {
                            state.process_macro_forecaster_result(&result_json);
                        } else if base_id == "fermi" || base_id == "macro_forecaster" {
                            // Check if this is a decomposition (has base_rate/drivers)
                            // or a recommendation (has recommended_agent)
                            let reasoning = result_json.get("metadata")
                                .and_then(|m| m.get("reasoning"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let clean = reasoning.trim()
                                .strip_prefix("```json").or_else(|| reasoning.trim().strip_prefix("```"))
                                .and_then(|s| s.strip_suffix("```"))
                                .unwrap_or(reasoning).trim();
                            let has_base_rate = clean.contains("base_rate") && (clean.contains("drivers") || clean.contains("historical_frequency"));

                            if has_base_rate {
                                state.process_macro_forecaster_result(&result_json);
                            } else {
                                state.process_fermi_recommendation(&result_json, cx);
                            }
                            state.process_fermi_recommendation(&result_json, cx);
                        } else {
                            // Other agents: add evidence to AST
                            state.process_agent_evidence(&tracking_id, &result_json);
                        }

                        if !findings.is_empty() {
                            state.messages.push(AssistantMessage {
                                node: format!("agent:{}", tracking_id),
                                kind: MessageKind::Tip,
                                text: format!("🦊 {} findings:\n{}",
                                    tracking_id,
                                    findings.iter()
                                        .map(|f| format!("• {}", f))
                                        .collect::<Vec<_>>()
                                        .join("\n")),
                            });
                        }

                        state.messages.push(AssistantMessage {
                            node: format!("agent:{}", tracking_id),
                            kind: MessageKind::Info,
                            text: format!("✓ {} complete", tracking_id),
                        });

                        // Check if all agents done
                        let all_done = state.agent_runs.iter()
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
                    }).ok();
                }
                Err(e) => {
                    log::error!("[composer] {} failed: {}", tracking_id, e);
                    this.update(cx, |state, cx| {
                        state.mark_agent_failed(&tracking_id, &format!("{}", e));
                        cx.notify();
                    }).ok();
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
            .strip_prefix("```json").or_else(|| reasoning.trim().strip_prefix("```"))
            .and_then(|s| s.strip_suffix("```"))
            .unwrap_or(reasoning)
            .trim();

        if let Ok(rec) = serde_json::from_str::<JsonValue>(clean) {
            let agent = rec.get("recommended_agent").and_then(|v| v.as_str()).unwrap_or("market_research");
            let reason = rec.get("reasoning").and_then(|v| v.as_str()).unwrap_or("");
            let query = rec.get("suggested_query").and_then(|v| v.as_str()).unwrap_or("").to_string();

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
        if let Some(run) = self.agent_runs.iter_mut().find(|r| r.agent_name == agent_id) {
            run.status = AgentRunStatus::Completed;
            run.confidence = result.get("confidence").and_then(|v| v.as_f64());
            run.credits_charged = result.get("credits_charged").and_then(|v| v.as_f64());
            if let Some(c) = run.credits_charged { self.session_cost += c; }
        }

        if let Some(evidence_arr) = result.get("evidence").and_then(|v| v.as_array()) {
            let mut count = 0;
            for ev in evidence_arr {
                let source = ev.get("source").and_then(|v| v.as_str()).unwrap_or(agent_id);
                let summary = ev.get("summary").and_then(|v| v.as_str());
                let relevance = ev.get("relevance").and_then(|v| v.as_f64());
                let key_findings: Vec<String> = ev.get("key_findings")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
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
            if let Some(run) = self.agent_runs.iter_mut().find(|r| r.agent_name == agent_id) {
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
        let question = self.program.question()
            .map(|q| q.text.clone())
            .unwrap_or_default();

        let query = format!(
            "I'm building a forecast for: \"{}\"\n\n\
             I need to assign a research agent to the driver '{}' (type: {}).\n\
             Driver rationale: {}\n\n\
             Available agents: macro_forecaster, market_research, sentiment_analyzer, entity_investigator.\n\n\
             Which agent is best for this driver? Suggest a specific research query.\n\
             Respond with JSON: {{\"recommended_agent\": \"...\", \"reasoning\": \"...\", \"suggested_query\": \"...\"}}",
            question, driver_name, driver_type, driver_rationale
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
        });

        self.messages.push(AssistantMessage {
            node: format!("driver:{}", driver_name),
            kind: MessageKind::Info,
            text: format!("Agent '{}' assigned to '{}' (schedule: {}) — researching now.",
                agent_id, driver_name, schedule_label),
        });

        self.fire_agent(agent_id, &query, cx);

        self.focused_node = FocusedNode::Driver(driver_name.to_string());
        self.populate_editor_from_driver(driver_name, cx);
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

        let ev_id = format!("manual_{}_{}", sanitize_name(&driver_name),
            self.program.evidence_items().len());

        self.program.add_evidence(EvidenceStmt {
            id: ev_id,
            source: if source.is_empty() { "Manual entry".into() } else { source },
            summary: if summary.is_empty() { None } else { Some(summary) },
            url: None,
            relevance: Some(0.7),
            date: Some(chrono::Utc::now().format("%Y-%m-%d").to_string()),
            strength: Some(0.7),
            key_findings: vec![],
        });

        // Clear inputs
        self.evidence_source_input.update(cx, |input, cx| input.set_text("", cx));
        self.evidence_summary_input.update(cx, |input, cx| input.set_text("", cx));

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
        let question = self.program.question()
            .map(|q| q.text.clone())
            .unwrap_or_default();
        if question.is_empty() { return; }

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
            (format!("event_{}", idx), make_binary_driver(
                &format!("event_{}", idx), &format!("Event {}", idx),
                0.5, 1.3, "Describe this event and its impact",
            ))
        } else {
            (format!("driver_{}", idx), make_continuous_driver(
                &format!("driver_{}", idx), &format!("Driver {}", idx),
                "", 0.0, 0.0, 0.0, "Describe this driver and set your estimates",
            ))
        };
        self.program.add_driver(driver);
        self.focus_driver(&name, cx);
        self.messages.push(AssistantMessage {
            node: format!("driver:{}", name),
            kind: MessageKind::Suggestion,
            text: format!("New driver '{}' added. Set your estimates in the editor.", name),
        });
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

        // Fermi validates the saved driver
        if let Some(driver) = self.program.driver(&name) {
            match driver.driver_type {
                DriverType::Continuous => {
                    if let Some(Distribution::Triangular { ref p5, ref p50, ref p95 }) = driver.distribution {
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
                let base_rate = self.program.question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency)
                    .unwrap_or(0.0);

                if base_rate > 0.0 {
                    // Compute baseline (all drivers at p50/median)
                    let mut fixed_drivers = std::collections::HashMap::new();
                    for driver in self.program.drivers() {
                        match driver.driver_type {
                            DriverType::Continuous => {
                                if let Some(Distribution::Triangular { ref p50, .. }) = driver.distribution {
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

                    let mut baseline_executor = ::fermi::executor::Executor::with_fixed_drivers(
                        1, fixed_drivers
                    );
                    let baseline = baseline_executor.execute(&parsed);
                    let baseline_mean = baseline.as_ref()
                        .map(|r| r.mean)
                        .unwrap_or(results.mean);

                    // Normalize: P = base_rate × (sim_mean / baseline_mean)
                    let ratio = if baseline_mean.abs() > 0.001 {
                        results.mean / baseline_mean
                    } else {
                        1.0
                    };
                    self.predicted_probability = (base_rate * ratio).clamp(0.01, 0.99);

                    self.predicted_probability = (base_rate * ratio).clamp(0.01, 0.99);

                    // Build narrative explanation
                    let direction = if ratio > 1.05 { "increases" }
                        else if ratio < 0.95 { "decreases" }
                        else { "confirms" };
                    let strength = if (ratio - 1.0).abs() > 0.3 { "significantly" }
                        else if (ratio - 1.0).abs() > 0.1 { "moderately" }
                        else { "slightly" };

                    // Find the most influential drivers from the sensitivity analysis
                    let top_drivers: Vec<String> = self.program.drivers().iter()
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
                let base_rate = self.program.question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency)
                    .unwrap_or(0.0);
                let divergence = (results.mean - base_rate) * 100.0;
                let div_direction = if divergence > 0.0 { "above" } else { "below" };

                // Build driver contribution summary

                // ── Run sensitivity analysis ──────────────────────
                let sensitivity = ::fermi::sensitivity::full_sensitivity_analysis(
                    &parsed, 1000  // fewer iterations for speed
                );

                let sensitivity_summary = if let Ok(ref sa) = sensitivity {
                    let top = sa.top_drivers(5);
                    let parts: Vec<String> = top.iter().map(|ds| {
                        format!("{} ({:.0}% influence)",
                            ds.driver_name,
                            ds.total_order_index * 100.0)
                    }).collect();
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
                        let top_names: Vec<String> = top.iter()
                            .map(|ds| format!("{} ({:.0}%)", ds.driver_name, ds.total_order_index * 100.0))
                            .collect();
                        self.inside_view_explanation = format!(
                            "{} Most influential: {}.",
                            self.inside_view_explanation,
                            top_names.join(", "),
                        );
                    }

                // ── Compute forecast confidence (Tetlock methodology) ──
                let total_drivers = self.program.drivers().len() as f64;
                let evidenced_drivers = self.program.drivers().iter().filter(|d| {
                    self.program.evidence_items().iter().any(|e| {
                        e.id.contains(&d.name) ||
                        self.program.agents().iter()
                            .filter(|a| a.driver_refs.contains(&d.name))
                            .any(|a| evidence_matches_agent(e, &a.name))
                    })
                }).count() as f64;
                let evidence_ratio = if total_drivers > 0.0 { evidenced_drivers / total_drivers } else { 0.0 };
                let divergence_penalty = if divergence.abs() > 30.0 { 0.7 }
                    else if divergence.abs() > 15.0 { 0.85 }
                    else { 1.0 };
                let base_confidence = 0.3 + (evidence_ratio * 0.5);
                self.forecast_confidence = (base_confidence * divergence_penalty).clamp(0.1, 0.95);

                }
                // Build driver contribution summary
                let driver_summary: Vec<String> = self.program.drivers().iter().map(|d| {
                    let display = d.display_name.as_deref().unwrap_or(&d.name);
                    // Include sensitivity if available
                    let influence = sensitivity.as_ref().ok()
                        .and_then(|sa| sa.get_driver_sensitivity(&d.name))
                        .map(|ds| format!(" [{:.0}%]", ds.total_order_index * 100.0))
                        .unwrap_or_default();
                    match d.driver_type {
                        DriverType::Continuous => {
                            if let Some(Distribution::Triangular { ref p50, .. }) = d.distribution {
                                format!("{} (p50={:.2}){}", display, expr_to_f64(p50), influence)
                            } else {
                                format!("{}{}", display, influence)
                            }
                        }
                        DriverType::Binary => {
                            format!("{} ({:.0}%){}", display, d.probability.unwrap_or(0.0) * 100.0, influence)
                        }
                        _ => format!("{}{}", display, influence),
                    }
                }).collect();

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
                if let Some(prob) = state_json.get("predicted_probability").and_then(|v| v.as_f64()) {
                    self.predicted_probability = prob.clamp(0.01, 0.99);
                }
                // Restore version history
                if let Some(versions) = state_json.get("versions").and_then(|v| v.as_array()) {
                    self.versions = versions.iter().filter_map(|v| {
                        Some(ForecastVersion {
                            version: v.get("version")?.as_u64()? as u32,
                            timestamp: v.get("timestamp")?.as_str()?.to_string(),
                            fpl_text: String::new(),
                            probability: v.get("probability")?.as_f64()?,
                            change_summary: v.get("change_summary")?.as_str()?.to_string(),
                        })
                    }).collect();
                    self.current_version = self.versions.last()
                        .map(|v| v.version).unwrap_or(0);
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
                            reference_class: br.get("reference_class")
                                .and_then(|v| v.as_str()).unwrap_or("").to_string(),
                            historical_frequency: br.get("historical_frequency")
                                .and_then(|v| v.as_f64()).unwrap_or(0.0),
                            sample_size: br.get("sample_size")
                                .and_then(|v| v.as_u64()).map(|n| n as usize),
                            source: br.get("source")
                                .and_then(|v| v.as_str()).unwrap_or("restored").to_string(),
                            reasoning: br.get("reasoning")
                                .and_then(|v| v.as_str()).map(|s| s.to_string()),
                            generated_by: GeneratedBy::Agent("fermi".into()),
                        });
                    }
                }
                // Restore inside view explanation
                if let Some(expl) = state_json.get("inside_view_explanation").and_then(|v| v.as_str()) {
                    self.inside_view_explanation = expl.to_string();
                }
                // Restore confidence
                if let Some(conf) = state_json.get("forecast_confidence").and_then(|v| v.as_f64()) {
                    self.forecast_confidence = conf;
                }
                // Restore agents into AST
                if let Some(agent_arr) = state_json.get("agents").and_then(|v| v.as_array()) {
                    for ag in agent_arr {
                        let name = ag.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if !name.is_empty() && self.program.agent(&name).is_none() {
                            let driver_refs: Vec<String> = ag.get("driver_refs")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                .unwrap_or_default();
                            self.program.add_agent(AgentStmt {
                                name,
                                agent_type: ag.get("agent_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                query: ag.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string(),
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
                        let name = ag.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if !name.is_empty() && !self.agent_runs.iter().any(|r| r.agent_name == name) {
                            let ev_count = self.program.evidence_items().iter()
                                .filter(|e| evidence_matches_agent(e, &name))
                                .count();
                            self.agent_runs.push(AgentExecution {
                                agent_name: name,
                                status: if ev_count > 0 { AgentRunStatus::Completed } else { AgentRunStatus::Idle },
                                evidence_count: ev_count,
                                confidence: None,
                                error: None,
                                credits_charged: None,
                            });
                        }
                    }
                }
                // Restore evidence into AST (supplement what FPL parsing got)
                log::info!("[load] Restoring evidence from state.json");
                if let Some(ev_arr) = state_json.get("evidence").and_then(|v| v.as_array()) {
                    for ev in ev_arr {
                        let id = ev.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if !id.is_empty() && self.program.evidence_items().iter().all(|e| e.id != id) {
                            self.program.add_evidence(EvidenceStmt {
                                id,
                                source: ev.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                                summary: ev.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                url: None,
                                relevance: ev.get("relevance").and_then(|v| v.as_f64()),
                                date: ev.get("date").and_then(|v| v.as_str()).map(|s| s.to_string()),
                                strength: ev.get("relevance").and_then(|v| v.as_f64()),
                                key_findings: ev.get("key_findings")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
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
                    text: format!("Restored v{} — {:.2}% (confidence: {:.0}%)",
                        self.current_version,
                        self.predicted_probability * 100.0,
                        self.forecast_confidence * 100.0),
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
        let filename = self.program.question()
            .map(|q| sanitize_name(&q.text))
            .unwrap_or_else(|| "forecast".into());
        let path = format!("forecasts/{}.fpl", filename);

        // Ensure directory exists
        let _ = std::fs::create_dir_all("forecasts");

        match std::fs::write(&path, &self.cached_fpl) {
            Ok(_) => {
                log::info!("[composer] Saved FPL to {}", path);
                log::info!("[composer] Evidence in AST: {}, Drivers: {}", self.program.evidence_items().len(), self.program.drivers().len());
                self.messages.push(AssistantMessage {
                    node: "save".into(),
                    kind: MessageKind::Info,
                    text: format!("Saved v{} to {}", self.current_version, path),
                });
                self.publish_status = Some(format!("Saved v{}", self.current_version));

                // Also save evidence wiki
                let wiki_path = format!("forecasts/{}.evidence.md", filename);
                let wiki = generate_evidence_wiki(&self.program, self.current_version, self.predicted_probability, &self.inside_view_explanation, self.forecast_confidence);
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
                });
                match std::fs::write(&state_path, serde_json::to_string_pretty(&state_json).unwrap_or_default()) {
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
                                render_driver_card(
                                    i,
                                    driver,
                                    is_focused,
                                    &assigned_agents,
                                    &self.agent_runs,
                                    &self.messages,
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
                            // Simulation results
                            .child(render_simulation_section(self))
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
                                RightTab::Wiki => render_wiki_tab(self).into_any_element(),
                            })
                    )
                    // Assistant messages (always visible below tabs)
                    .child(render_assistant_panel(&self.messages))
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
                    let conf_label = if state.forecast_confidence > 0.7 { "High" }
                        else if state.forecast_confidence > 0.4 { "Medium" }
                        else { "Low" };
                    let conf_color = if state.forecast_confidence > 0.7 { theme::GREEN }
                        else if state.forecast_confidence > 0.4 { theme::GOLD }
                        else { theme::RED };
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(conf_color))
                            .child(format!("Confidence: {} ({:.0}%)", conf_label, state.forecast_confidence * 100.0)),
                    )
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
                                    .child(
                                        "Researching… Fermi is decomposing your forecast",
                                    ),
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
                .child("Ctrl+N new").child("Ctrl+O import").child("Ctrl+S save").child("Ctrl+E tabs"),
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

    let border_color = if is_focused {
        theme::CYAN
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
            if let Some(Distribution::Triangular { ref p5, ref p50, ref p95 }) = driver.distribution {
                let v5 = expr_to_f64(p5);
                let v50 = expr_to_f64(p50);
                let v95 = expr_to_f64(p95);
                if v95 > v5 {
                    let chart_w = 120u32;
                    let chart_h = 24u32;
                    let rgb_buf = crate::charts::render_distribution_sparkline(v5, v50, v95, chart_w, chart_h);
                    let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                    el.child(
                        gpui::img(gpui::ImageSource::Render(render_img))
                            .w(gpui::px(chart_w as f32))
                            .h(gpui::px(chart_h as f32)),
                    )
                } else { el }
            } else { el }
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
        // Driver confidence dots (based on evidence coverage)
        .child({
            let ev_count = assigned_agents.iter()
                .flat_map(|a| agent_runs.iter().filter(move |r| r.agent_name == *a))
                .map(|r| r.evidence_count)
                .sum::<usize>();
            let (conf_label, conf_color) = if ev_count >= 3 {
                ("Evidence: ●●● High", theme::GREEN)
            } else if ev_count >= 1 {
                ("Evidence: ●●○ Medium", theme::GOLD)
            } else {
                ("Evidence: ●○○ Low", theme::RED)
            };
            div()
                .text_size(px(9.0))
                .text_color(rgb(conf_color))
                .px(px(4.0))
                .child(conf_label)
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

                    let (status_icon, status_text, status_color, bg_color) = match status {
                        Some(AgentRunStatus::Running) => ("⟳", "researching…", theme::GOLD, 0x2A2D3A),
                        Some(AgentRunStatus::Completed) => ("✓", &format!("{} findings", ev_count) as &str, theme::GREEN, theme::BG),
                        Some(AgentRunStatus::Failed) => ("✗", "failed", theme::RED, 0x3D1F1F),
                        _ => ("○", "idle", theme::FG_DIM, theme::BG),
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
                                        .child(status_text.to_string()),
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
        .filter(|card| {
            card.metadata.tags.iter().any(|t| t == "fermi-orchestra")
        })
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
                .child(div().flex_grow().min_w(px(0.0)).child(state.editor_rationale.clone())),
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
                        .child(div().flex_grow().min_w(px(0.0)).child(state.editor_rationale.clone())),
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

fn render_simulation_section(state: &CockpitState) -> impl IntoElement {
    div()
        .px(px(16.0))
        .py(px(8.0))
        .border_t_1()
        .border_color(rgb(theme::FG_FAINT))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .when(state.sim_running, |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::GOLD))
                    .child("⟳ Simulating…"),
            )
        })
        .when(state.sim_error.is_some(), |el| {
            el.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme::RED))
                    .child(format!("✗ {}", state.sim_error.as_deref().unwrap_or(""))),
            )
        })
        .when(state.sim_results.is_some(), |el| {
            let sim = state.sim_results.as_ref().unwrap();
            el.child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .text_size(px(11.0))
                    .child(render_stat("mean", sim.mean, theme::FG))
                    .child(render_stat("p5", sim.p5, theme::FG_DIM))
                    .child(render_stat("p50", sim.median, theme::CYAN))
                    .child(render_stat("p95", sim.p95, theme::FG_DIM))
                    .child(render_stat("σ", sim.std_dev, theme::FG_FAINT)),
            )
            .when(!sim.histogram.is_empty(), |el| {
                el.child(render_histogram(&sim.histogram))
            })
            // Index comparison chart (inside view vs outside view over versions)
            .when(state.versions.len() > 0, |el| {
                let base_rate = state.program.question()
                    .and_then(|q| q.base_rate.as_ref())
                    .map(|br| br.historical_frequency * 100.0)
                    .unwrap_or(50.0);

                let history: Vec<crate::charts::IndexPoint> = state.versions.iter()
                    .map(|v| crate::charts::IndexPoint {
                        label: format!("v{}", v.version),
                        inside_view: v.probability * 100.0,
                        outside_view: base_rate,
                    })
                    .collect();

                let chart_w = 400u32;
                let chart_h = 120u32;
                let rgb_buf = crate::charts::render_index_chart(
                    &history,
                    history.len().saturating_sub(1),
                    chart_w,
                    chart_h,
                );
                let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);

                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .mt(px(4.0))
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(theme::FG_FAINT))
                                .child("Inside View (blue) vs Outside View (gold)"),
                        )
                        .child(
                            gpui::img(gpui::ImageSource::Render(render_img))
                                .w(gpui::px(chart_w as f32))
                                .h(gpui::px(chart_h as f32)),
                        ),
                )
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
            )
            // Evidence treemap — drivers sized by impact
            .child({
                let drivers_viz: Vec<crate::charts::DriverViz> = state.program.drivers().iter().map(|d| {
                    let display = d.display_name.as_deref().unwrap_or(&d.name);
                    let impact = match d.driver_type {
                        DriverType::Continuous => {
                            if let Some(Distribution::Triangular { ref p5, ref p95, .. }) = d.distribution {
                                (expr_to_f64(p95) - expr_to_f64(p5)).abs().max(0.1)
                            } else { 1.0 }
                        }
                        DriverType::Binary => {
                            d.probability.unwrap_or(0.5) * d.impact_multiplier.unwrap_or(1.0) * 10.0
                        }
                        _ => 1.0,
                    };
                    let evidence_count = state.program.evidence_items().iter()
                        .filter(|e| e.id.contains(&d.name) || 
                            state.program.agents().iter()
                                .filter(|a| a.driver_refs.contains(&d.name))
                                .any(|a| evidence_matches_agent(e, &a.name)))
                        .count();
                    let quality = if evidence_count > 2 { 0.8 } 
                        else if evidence_count > 0 { 0.5 } 
                        else { 0.2 };
                    crate::charts::DriverViz {
                        name: display.to_string(),
                        impact,
                        quality,
                        evidence: state.program.evidence_items().iter()
                            .filter(|e| e.id.contains(&d.name))
                            .filter_map(|e| e.summary.as_ref().map(|s| s.chars().take(40).collect()))
                            .take(2)
                            .collect(),
                    }
                }).collect();

                if !drivers_viz.is_empty() {
                    let chart_w = 400u32;
                    let chart_h = 120u32;
                    let rgb_buf = crate::charts::render_treemap(&drivers_viz, chart_w, chart_h);
                    let render_img = crate::charts::rgb_to_render_image(&rgb_buf, chart_w, chart_h);
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .mt(px(4.0))
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
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            })
        })
        .when(
            state.sim_results.is_none() && !state.sim_running && state.sim_error.is_none(),
            |el| {
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgb(theme::FG_FAINT))
                        .child("Ctrl+R to run Monte Carlo simulation"),
                )
            },
        )
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
        .child(format!("{} drivers", state.program.drivers().len()))
        .child(format!("{} evidence", state.program.evidence_items().len()))
        .child(format!("{} agents", state.agent_runs.len()))
        .when(state.session_cost > 0.0, |el| {
            el.child(format!("{:.1} credits", state.session_cost))
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
                .font_weight(if is_active { FontWeight::BOLD } else { FontWeight::NORMAL })
                .text_color(if is_active { rgb(theme::CYAN) } else { rgb(theme::FG_DIM) })
                .border_b_2()
                .border_color(if is_active { rgb(theme::CYAN) } else { rgb(theme::BG_ELEVATED) })
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

    div()
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(theme::FG_DIM))
                .font_family("Ubuntu Mono, DejaVu Sans Mono, monospace")
                .min_w(px(0.0))
                .child(if fpl.is_empty() { "# Empty program".to_string() } else { fpl }),
        )
}

fn render_wiki_tab(state: &CockpitState) -> impl IntoElement {
    let drivers = state.program.drivers();
    let evidence = state.program.evidence_items();
    let agents = state.program.agents();

    div()
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .min_w(px(0.0))
        // Base rate section
        .when(state.program.question().and_then(|q| q.base_rate.as_ref()).is_some(), |el| {
            let br = state.program.question().unwrap().base_rate.as_ref().unwrap();
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .pb(px(8.0))
                    .border_b_1()
                    .border_color(rgb(theme::FG_FAINT))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme::GOLD))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Outside View (Base Rate)"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::FG))
                            .min_w(px(0.0))
                            .child(format!("{:.2}% — {}", br.historical_frequency * 100.0, br.reference_class)),
                    )
                    .when(br.reasoning.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .min_w(px(0.0))
                                .child(br.reasoning.as_deref().unwrap_or("").to_string()),
                        )
                    }),
            )
        })
        // Per-driver evidence sections
        .children(drivers.iter().map(|driver| {
            let display = driver.display_name.as_deref().unwrap_or(&driver.name);
            let driver_agents: Vec<&str> = agents.iter()
                .filter(|a| a.driver_refs.contains(&driver.name))
                .map(|a| a.name.as_str())
                .collect();
            let driver_ev: Vec<_> = evidence.iter()
                .filter(|e| driver_agents.iter().any(|a| evidence_matches_agent(e, a)) || e.id.contains(&driver.name))
                .collect();

            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .pb(px(8.0))
                .border_b_1()
                .border_color(rgb(theme::FG_FAINT))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::GREEN))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(display.to_string()),
                )
                .when(driver.rationale.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_DIM))
                            .min_w(px(0.0))
                            .child(driver.rationale.as_deref().unwrap_or("").to_string()),
                    )
                })
                // Agent assignments
                .when(!driver_agents.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .children(driver_agents.iter().map(|a| {
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::BLUE))
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(rgb(theme::BG))
                                    .child(a.to_string())
                            })),
                    )
                })
                // Evidence items
                .children(driver_ev.iter().map(|ev| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG))
                        .mt(px(4.0))
                        .child(
                            div()
                                .flex()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(theme::FG_FAINT))
                                        .child(ev.source.clone()),
                                )
                                .when(ev.relevance.is_some(), |el| {
                                    el.child(
                                        div()
                                            .text_size(px(9.0))
                                            .text_color(rgb(theme::CYAN))
                                            .child(format!("{:.0}%", ev.relevance.unwrap_or(0.0) * 100.0)),
                                    )
                                }),
                        )
                        .when(ev.summary.is_some(), |el| {
                            let summary = ev.summary.as_deref().unwrap_or("");
                            let display = if summary.len() > 300 {
                                format!("{}…", &summary[..300])
                            } else {
                                summary.to_string()
                            };
                            el.child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme::FG))
                                    .min_w(px(0.0))
                                    .child(display),
                            )
                        })
                        .when(!ev.key_findings.is_empty(), |el| {
                            el.children(ev.key_findings.iter().take(4).map(|f| {
                                div()
                                    .text_size(px(9.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .min_w(px(0.0))
                                    .child(format!("• {}", f))
                            }))
                        })
                }))
                // No evidence yet
                .when(driver_ev.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(theme::FG_FAINT))
                            .child("No evidence yet — assign an agent to research this driver"),
                    )
                })
        }))
        // Unlinked evidence
        .when(!evidence.is_empty(), |el| {
            let all_agent_names: Vec<String> = agents.iter()
                .filter(|a| !a.driver_refs.is_empty())
                .map(|a| a.name.clone())
                .collect();
            let unlinked: Vec<_> = evidence.iter()
                .filter(|e| !all_agent_names.iter().any(|a| evidence_matches_agent(e, a)))
                .collect();
            if unlinked.is_empty() {
                el
            } else {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .pt(px(8.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgb(theme::FG_DIM))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("General Evidence"),
                        )
                        .children(unlinked.iter().map(|ev| {
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(theme::FG_DIM))
                                .min_w(px(0.0))
                                .child(format!("{}: {}",
                                    ev.source,
                                    ev.summary.as_deref().unwrap_or("").chars().take(200).collect::<String>()
                                ))
                        })),
                )
            }
        })
        // Version history
        .when(!state.versions.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(rgb(theme::FG_FAINT))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme::PURPLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Version History ({})", state.versions.len())),
                    )
                    .children(state.versions.iter().rev().map(|v| {
                        let prob_change = if v.version > 1 {
                            state.versions.iter()
                                .find(|prev| prev.version == v.version - 1)
                                .map(|prev| {
                                    let delta = (v.probability - prev.probability) * 100.0;
                                    let sign = if delta > 0.0 { "+" } else { "" };
                                    let color = if delta > 0.0 { theme::GREEN } else if delta < 0.0 { theme::RED } else { theme::FG_DIM };
                                    (format!("{}{}pp", sign, delta as i64), color)
                                })
                        } else { None };

                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .py(px(3.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::FG_DIM))
                                    .w(px(24.0))
                                    .child(format!("v{}", v.version)),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::CYAN))
                                    .w(px(50.0))
                                    .child(format!("{:.2}%", v.probability * 100.0)),
                            )
                            .when(prob_change.is_some(), |el| {
                                let (text, color) = prob_change.unwrap();
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(rgb(color))
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
fn generate_evidence_wiki(program: &Program, version: u32, probability: f64, explanation: &str, confidence: f64) -> String {
    let mut md = String::new();
    let question = program.question().map(|q| q.text.as_str()).unwrap_or("Untitled Forecast");
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    md.push_str(&format!("# Evidence Log: {}\n\n", question));
    md.push_str(&format!("**Version:** v{} | **Probability:** {:.1}% | **Updated:** {}\n\n", version, probability * 100.0, timestamp));
    md.push_str("---\n\n");
    // Inside view section
    if !explanation.is_empty() {
        md.push_str("## Inside View\n\n");
        md.push_str(&format!("**Probability:** {:.2}%\n\n", probability * 100.0));
        md.push_str(&format!("{}\n\n", explanation));
        let conf_label = if confidence > 0.7 { "High" } else if confidence > 0.4 { "Medium" } else { "Low" };
        md.push_str(&format!("**Confidence:** {} ({:.0}%)\n\n", conf_label, confidence * 100.0));
        md.push_str("---\n\n");
    }

    // Base rate section
    if let Some(br) = program.question().and_then(|q| q.base_rate.as_ref()) {
        md.push_str("## Outside View (Base Rate)\n\n");
        md.push_str(&format!("- **Reference class:** {}\n", br.reference_class));
        md.push_str(&format!("- **Historical frequency:** {:.2}%\n", br.historical_frequency * 100.0));
        if let Some(n) = br.sample_size {
            md.push_str(&format!("- **Sample size:** n={}\n", n));
        }
        md.push_str(&format!("- **Source:** {}\n", br.source));
        if let Some(ref r) = br.reasoning {
            md.push_str(&format!("\n> {}\n", r));
        }
        md.push_str("\n---\n\n");
    }

    // Drivers with their evidence
    let drivers = program.drivers();
    let evidence_items = program.evidence_items();
    let agents = program.agents();

    for driver in &drivers {
        let display = driver.display_name.as_deref().unwrap_or(&driver.name);
        let type_label = match driver.driver_type {
            DriverType::Continuous => "continuous",
            DriverType::Binary => "binary",
            _ => "discrete",
        };

        md.push_str(&format!("## {} `{}`\n\n", display, type_label));

        // Driver parameters
        match driver.driver_type {
            DriverType::Continuous => {
                if let Some(ref dist) = driver.distribution {
                    if let Distribution::Triangular { ref p5, ref p50, ref p95 } = dist {
                        let unit = driver.unit.as_deref().unwrap_or("");
                        md.push_str(&format!("| p5 | p50 | p95 | unit |\n|---|---|---|---|\n| {} | {} | {} | {} |\n\n",
                            expr_to_f64(p5), expr_to_f64(p50), expr_to_f64(p95), unit));
                    }
                }
            }
            DriverType::Binary => {
                md.push_str(&format!("- **Probability:** {:.0}%\n", driver.probability.unwrap_or(0.0) * 100.0));
                md.push_str(&format!("- **Impact multiplier:** {:.1}x\n\n", driver.impact_multiplier.unwrap_or(1.0)));
            }
            _ => {}
        }

        if let Some(ref rationale) = driver.rationale {
            md.push_str(&format!("> {}\n\n", rationale));
        }

        // Agents assigned to this driver
        let driver_agents: Vec<_> = agents.iter()
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
                md.push_str(&format!("- **{}** (schedule: {})\n", agent.name, schedule));
                md.push_str(&format!("  - Query: _{}_\n", agent.query.chars().take(100).collect::<String>()));
            }
            md.push_str("\n");
        }

        // Evidence linked to this driver (from its agents)
        let driver_evidence: Vec<_> = evidence_items.iter()
            .filter(|e| driver_agents.iter().any(|a| evidence_matches_agent(e, &a.name)) || e.id.contains(&driver.name))
            .collect();

        if !driver_evidence.is_empty() {
            md.push_str("### Evidence\n\n");
            for ev in &driver_evidence {
                md.push_str(&format!("#### {} (relevance: {:.0}%)\n\n",
                    ev.source,
                    ev.relevance.unwrap_or(0.0) * 100.0));
                if let Some(ref summary) = ev.summary {
                    // Truncate very long summaries (like raw JSON dumps)
                    let clean = if summary.len() > 500 {
                        format!("{}...", &summary[..500])
                    } else {
                        summary.clone()
                    };
                    md.push_str(&format!("{}\n\n", clean));
                }
                if !ev.key_findings.is_empty() {
                    md.push_str("**Key findings:**\n\n");
                    for f in &ev.key_findings {
                        md.push_str(&format!("- {}\n", f));
                    }
                    md.push_str("\n");
                }
                if let Some(ref date) = ev.date {
                    md.push_str(&format!("_Collected: {}_\n\n", date));
                }
            }
        }

        // Also show unlinked evidence that mentions this driver
        let unlinked: Vec<_> = evidence_items.iter()
            .filter(|e| {
                !driver_evidence.iter().any(|de| de.id == e.id) &&
                (e.summary.as_deref().unwrap_or("").to_lowercase().contains(&driver.name.to_lowercase()) ||
                 e.key_findings.iter().any(|f| f.to_lowercase().contains(&driver.name.to_lowercase())))
            })
            .collect();

        if !unlinked.is_empty() {
            md.push_str("### Related Evidence\n\n");
            for ev in &unlinked {
                md.push_str(&format!("- **{}**: {}\n",
                    ev.source,
                    ev.summary.as_deref().unwrap_or("").chars().take(200).collect::<String>()));
            }
            md.push_str("\n");
        }

        md.push_str("---\n\n");
    }

    // Unassigned evidence (not linked to any driver)
    let all_driver_agents: Vec<String> = agents.iter()
        .filter(|a| !a.driver_refs.is_empty())
        .map(|a| a.name.clone())
        .collect();
    let unassigned: Vec<_> = evidence_items.iter()
        .filter(|e| !all_driver_agents.iter().any(|a| evidence_matches_agent(e, a)))
        .collect();

    if !unassigned.is_empty() {
        md.push_str("## General Evidence\n\n");
        for ev in &unassigned {
            md.push_str(&format!("### {} (relevance: {:.0}%)\n\n",
                ev.source,
                ev.relevance.unwrap_or(0.0) * 100.0));
            if let Some(ref summary) = ev.summary {
                let clean = if summary.len() > 500 {
                    format!("{}...", &summary[..500])
                } else {
                    summary.clone()
                };
                md.push_str(&format!("{}\n\n", clean));
            }
            if !ev.key_findings.is_empty() {
                for f in &ev.key_findings {
                    md.push_str(&format!("- {}\n", f));
                }
                md.push_str("\n");
            }
        }
    }

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
                make_continuous_driver("fundamentals", "Fundamentals Strength", "multiplier",
                    0.7, 1.0, 1.4, "How strong are the fundamentals relative to expectations? 1.0 = neutral"),
                make_continuous_driver("market_conditions", "Market Conditions", "multiplier",
                    0.6, 1.0, 1.5, "Favorable (>1) or unfavorable (<1) market environment"),
                make_continuous_driver("momentum", "Momentum Factor", "multiplier",
                    0.8, 1.0, 1.3, "Recent trend direction and strength. 1.0 = no trend"),
                make_binary_driver("catalyst_event", "Major Catalyst/Risk",
                    0.20, 1.4, "Probability of a significant event that shifts the outcome"),
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
                make_continuous_driver("feasibility", "Technical Feasibility", "multiplier",
                    0.5, 1.0, 1.3, "How feasible is the technical achievement? 1.0 = expected"),
                make_continuous_driver("adoption", "Adoption Likelihood", "multiplier",
                    0.6, 1.0, 1.5, "Market readiness and adoption potential. 1.0 = baseline"),
                make_binary_driver("regulatory_block", "Regulatory Blocker",
                    0.25, 0.5, "Probability of regulatory action that halves the outcome"),
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
                make_continuous_driver("strength_factor", "Strength of Case", "multiplier",
                    0.5, 1.0, 1.5, "How strong is the case for this outcome? 1.0 = neutral"),
                make_continuous_driver("conditions", "Favorable Conditions", "multiplier",
                    0.7, 1.0, 1.3, "Are conditions favorable (>1) or unfavorable (<1)?"),
                make_binary_driver("disruption", "Disruption Event",
                    0.15, 1.5, "Probability of a disruptive event that amplifies the outcome"),
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
fn base_agent_name(compound_name: &str) -> &str {
    // Known agent base names
    let known = ["macro_forecaster", "market_research", "sentiment_analyzer", 
                  "entity_investigator", "monte_carlo_sim", "fermi"];
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
