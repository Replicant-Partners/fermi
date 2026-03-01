//! Forecast Composer — question builder, driver editor, simulation runner
//!
//! The composer is the core authoring experience in the Fermi Console.
//! It lets you:
//! 1. Define a question (what are you forecasting?)
//! 2. Add drivers (continuous distributions, binary probabilities)
//! 3. Define a model expression (how drivers combine)
//! 4. Run Monte Carlo simulation locally (instant, no server needed)
//! 5. View results (mean, percentiles, histogram)
//! 6. Publish to the API for Brier scoring
//!
//! All simulation runs locally using the fermi crate's executor.
//! No network calls needed until you publish.

use gpui::prelude::*;
use gpui::*;

use crate::theme;

// ═══════════════════════════════════════════════════════════════════
// Driver types (mirror the FPL AST but UI-friendly)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum DriverKind {
    Continuous {
        distribution: DistributionKind,
        unit: String,
    },
    Binary {
        probability: f64,
        impact_multiplier: f64,
    },
}

#[derive(Debug, Clone)]
pub enum DistributionKind {
    Triangular { p5: f64, p50: f64, p95: f64 },
    Normal { mean: f64, stddev: f64 },
    Uniform { low: f64, high: f64 },
    Lognormal { median: f64, sigma: f64 },
}

impl DistributionKind {
    fn label(&self) -> &'static str {
        match self {
            DistributionKind::Triangular { .. } => "Triangular",
            DistributionKind::Normal { .. } => "Normal",
            DistributionKind::Uniform { .. } => "Uniform",
            DistributionKind::Lognormal { .. } => "Lognormal",
        }
    }

    fn summary(&self) -> String {
        match self {
            DistributionKind::Triangular { p5, p50, p95 } => {
                format!("p5={:.1}, p50={:.1}, p95={:.1}", p5, p50, p95)
            }
            DistributionKind::Normal { mean, stddev } => {
                format!("μ={:.1}, σ={:.1}", mean, stddev)
            }
            DistributionKind::Uniform { low, high } => {
                format!("low={:.1}, high={:.1}", low, high)
            }
            DistributionKind::Lognormal { median, sigma } => {
                format!("median={:.1}, σ={:.1}", median, sigma)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Driver {
    pub name: String,
    pub kind: DriverKind,
    pub rationale: String,
}

impl Driver {
    fn new_continuous(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: DriverKind::Continuous {
                distribution: DistributionKind::Triangular {
                    p5: 0.0,
                    p50: 50.0,
                    p95: 100.0,
                },
                unit: String::new(),
            },
            rationale: String::new(),
        }
    }

    fn new_binary(name: &str) -> Self {
        Self {
            name: name.to_string(),
            kind: DriverKind::Binary {
                probability: 0.5,
                impact_multiplier: 1.3,
            },
            rationale: String::new(),
        }
    }

    fn kind_label(&self) -> &'static str {
        match &self.kind {
            DriverKind::Continuous { .. } => "continuous",
            DriverKind::Binary { .. } => "binary",
        }
    }

    fn summary(&self) -> String {
        match &self.kind {
            DriverKind::Continuous { distribution, unit } => {
                let u = if unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", unit)
                };
                format!("{}{}", distribution.summary(), u)
            }
            DriverKind::Binary {
                probability,
                impact_multiplier,
            } => format!("{:.0}% (×{:.1})", probability * 100.0, impact_multiplier),
        }
    }

    /// Generate FPL source for this driver.
    fn to_fpl(&self) -> String {
        match &self.kind {
            DriverKind::Continuous { distribution, unit } => {
                let dist_str = match distribution {
                    DistributionKind::Triangular { p5, p50, p95 } => {
                        format!("triangular({}, {}, {})", p5, p50, p95)
                    }
                    DistributionKind::Normal { mean, stddev } => {
                        format!("normal({}, {})", mean, stddev)
                    }
                    DistributionKind::Uniform { low, high } => {
                        format!("uniform({}, {})", low, high)
                    }
                    DistributionKind::Lognormal { median, sigma } => {
                        format!("lognormal({}, {})", median, sigma)
                    }
                };
                let unit_str = if unit.is_empty() {
                    String::new()
                } else {
                    format!("\n    unit: \"{}\"", unit)
                };
                let rationale_str = if self.rationale.is_empty() {
                    String::new()
                } else {
                    format!("\n    rationale: \"{}\"", self.rationale)
                };
                format!(
                    "driver {} continuous {{\n    distribution: {}{}{}\n}}",
                    self.name, dist_str, unit_str, rationale_str
                )
            }
            DriverKind::Binary {
                probability,
                impact_multiplier,
            } => {
                let rationale_str = if self.rationale.is_empty() {
                    String::new()
                } else {
                    format!("\n    rationale: \"{}\"", self.rationale)
                };
                format!(
                    "driver {} binary {{\n    probability: {}p\n    impact_multiplier: {}{}\n}}",
                    self.name, probability, impact_multiplier, rationale_str
                )
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Simulation results (from local Monte Carlo)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct SimulationResults {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub p5: f64,
    pub p25: f64,
    pub p75: f64,
    pub p95: f64,
    pub min: f64,
    pub max: f64,
    pub iterations: usize,
    pub histogram: Vec<(f64, usize)>,
    pub execution_time_ms: u64,
}

// ═══════════════════════════════════════════════════════════════════
// Composer state
// ═══════════════════════════════════════════════════════════════════

pub struct ComposerState {
    // Question
    pub question: String,
    pub domain: String,
    pub resolution_criteria: String,
    pub target_date: String,

    // Drivers
    pub drivers: Vec<Driver>,

    // Model expression (free text — maps to FPL model: expression)
    pub model_expression: String,

    // Simulation
    pub iterations: usize,
    pub results: Option<SimulationResults>,
    pub sim_running: bool,
    pub sim_error: Option<String>,

    // Publishing
    pub predicted_probability: f64,
    pub visibility: String, // "private", "shared", "public"
    pub publish_status: Option<String>,

    // FPL source (generated or hand-edited)
    pub show_fpl_source: bool,
    pub fpl_source_override: Option<String>,
}

impl ComposerState {
    pub fn new() -> Self {
        Self {
            question: String::new(),
            domain: String::new(),
            resolution_criteria: String::new(),
            target_date: String::new(),
            drivers: Vec::new(),
            model_expression: String::new(),
            iterations: 10_000,
            results: None,
            sim_running: false,
            sim_error: None,
            predicted_probability: 0.5,
            visibility: "private".to_string(),
            publish_status: None,
            show_fpl_source: false,
            fpl_source_override: None,
        }
    }

    /// Generate FPL source from the current composer state.
    pub fn generate_fpl(&self) -> String {
        let mut lines = Vec::new();

        // Question
        lines.push(format!("question \"{}\"", self.question));
        lines.push(String::new());

        // Drivers
        for driver in &self.drivers {
            lines.push(driver.to_fpl());
            lines.push(String::new());
        }

        // Model
        if !self.model_expression.is_empty() {
            lines.push(format!("model: {}", self.model_expression));
            lines.push(String::new());
        }

        // Simulate
        lines.push(format!("simulate {} iterations", self.iterations));

        lines.join("\n")
    }

    /// Get the effective FPL source (override or generated).
    pub fn effective_fpl(&self) -> String {
        self.fpl_source_override
            .clone()
            .unwrap_or_else(|| self.generate_fpl())
    }

    /// Run Monte Carlo simulation locally using the fermi executor.
    ///
    /// This is synchronous and fast — 10k iterations in <100ms.
    /// Called from a background thread via cx.background_executor().
    pub fn run_simulation(&self) -> Result<SimulationResults, String> {
        let fpl_source = self.effective_fpl();

        if fpl_source.trim().is_empty() {
            return Err("No FPL source to simulate".into());
        }

        let start = std::time::Instant::now();

        // Parse
        let lexer = ::fermi::lexer::Lexer::new(&fpl_source);
        let tokens = lexer
            .tokenize()
            .map_err(|e| format!("Tokenization error: {:?}", e))?;

        let parser = ::fermi::parser::Parser::new(tokens);
        let program = parser.parse().map_err(|e| format!("Parse error: {}", e))?;

        // Execute
        let mut executor = ::fermi::executor::Executor::new(self.iterations);
        let results = executor
            .execute(&program)
            .map_err(|e| format!("Execution error: {:?}", e))?;

        let elapsed = start.elapsed();

        // Build histogram (20 bins)
        let histogram = results.histogram(20);

        Ok(SimulationResults {
            mean: results.mean,
            median: results.median,
            std_dev: results.std_dev,
            p5: results.p5,
            p25: results.p25,
            p75: results.p75,
            p95: results.p95,
            min: results.min,
            max: results.max,
            iterations: results.iterations,
            histogram,
            execution_time_ms: elapsed.as_millis() as u64,
        })
    }

    pub fn add_continuous_driver(&mut self) {
        let idx = self.drivers.len() + 1;
        self.drivers
            .push(Driver::new_continuous(&format!("driver_{}", idx)));
    }

    pub fn add_binary_driver(&mut self) {
        let idx = self.drivers.len() + 1;
        self.drivers
            .push(Driver::new_binary(&format!("event_{}", idx)));
    }

    pub fn remove_driver(&mut self, index: usize) {
        if index < self.drivers.len() {
            self.drivers.remove(index);
        }
    }

    /// Auto-generate a model expression from driver names.
    /// Simple multiplication of all continuous drivers, with binary if-then.
    pub fn auto_model_expression(&mut self) {
        let parts: Vec<String> = self
            .drivers
            .iter()
            .map(|d| match &d.kind {
                DriverKind::Continuous { .. } => d.name.clone(),
                DriverKind::Binary {
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
}

// ═══════════════════════════════════════════════════════════════════
// Render functions (called from FermiConsole)
// ═══════════════════════════════════════════════════════════════════

/// Render the full composer panel.
pub fn render_composer(state: &ComposerState) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .p(px(24.0))
        .gap(px(16.0))
        .child(
            // Header
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(22.0))
                        .text_color(theme::fg())
                        .font_weight(FontWeight::BOLD)
                        .child("Forecast Composer"),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child("⌘R to simulate"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child("⌘Enter to publish"),
                        ),
                ),
        )
        // Question section
        .child(render_question_section(state))
        // Drivers section
        .child(render_drivers_section(state))
        // Model expression section
        .child(render_model_section(state))
        // Simulation results (if available)
        .when(state.results.is_some(), |el: Div| {
            el.child(render_results_section(state.results.as_ref().unwrap()))
        })
        // Simulation error
        .when(state.sim_error.is_some(), |el: Div| {
            el.child(render_error_section(state.sim_error.as_ref().unwrap()))
        })
        // FPL source toggle
        .when(state.show_fpl_source, |el: Div| {
            el.child(render_fpl_source_section(state))
        })
        // Empty state hint
        .when(
            state.question.is_empty() && state.drivers.is_empty(),
            |el: Div| el.child(render_empty_hint()),
        )
}

fn render_question_section(state: &ComposerState) -> impl IntoElement {
    render_card(
        "Question",
        theme::CYAN,
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(render_field_display(
                "What are you forecasting?",
                if state.question.is_empty() {
                    "Enter your question…"
                } else {
                    &state.question
                },
                state.question.is_empty(),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(render_field_display(
                        "Domain",
                        if state.domain.is_empty() {
                            "e.g. tech, economics"
                        } else {
                            &state.domain
                        },
                        state.domain.is_empty(),
                    ))
                    .child(render_field_display(
                        "Target Date",
                        if state.target_date.is_empty() {
                            "YYYY-MM-DD"
                        } else {
                            &state.target_date
                        },
                        state.target_date.is_empty(),
                    )),
            )
            .child(render_field_display(
                "Resolution Criteria",
                if state.resolution_criteria.is_empty() {
                    "How will this be resolved?"
                } else {
                    &state.resolution_criteria
                },
                state.resolution_criteria.is_empty(),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("Predicted probability:"),
                    )
                    .child(
                        div()
                            .text_size(px(18.0))
                            .text_color(theme::cyan())
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{:.0}%", state.predicted_probability * 100.0)),
                    ),
            ),
    )
}

fn render_drivers_section(state: &ComposerState) -> impl IntoElement {
    render_card(
        &format!("Drivers ({})", state.drivers.len()),
        theme::GREEN,
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .when(state.drivers.is_empty(), |el: Div| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::fg_dim())
                        .py(px(8.0))
                        .child("No drivers yet — add continuous or binary drivers below"),
                )
            })
            .children(
                state
                    .drivers
                    .iter()
                    .enumerate()
                    .map(|(i, d)| render_driver_row(i, d)),
            )
            .child(
                // Add driver buttons
                div()
                    .flex()
                    .gap(px(8.0))
                    .mt(px(8.0))
                    .child(render_button("+ Continuous", theme::GREEN))
                    .child(render_button("+ Binary", theme::GOLD)),
            ),
    )
}

fn render_driver_row(index: usize, driver: &Driver) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(4.0))
        .hover(|style| style.bg(theme::bg_hover()))
        .child(
            // Index
            div()
                .w(px(24.0))
                .text_size(px(11.0))
                .text_color(theme::fg_faint())
                .child(format!("{}.", index + 1)),
        )
        .child(
            // Name
            div()
                .w(px(120.0))
                .text_size(px(13.0))
                .text_color(theme::fg())
                .font_weight(FontWeight::SEMIBOLD)
                .child(driver.name.clone()),
        )
        .child(
            // Kind badge
            div()
                .text_size(px(10.0))
                .text_color(match &driver.kind {
                    DriverKind::Continuous { .. } => theme::green(),
                    DriverKind::Binary { .. } => theme::gold(),
                })
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .bg(theme::bg_active())
                .child(driver.kind_label()),
        )
        .child(
            // Summary
            div()
                .flex_grow()
                .text_size(px(12.0))
                .text_color(theme::fg_dim())
                .child(driver.summary()),
        )
        .child(
            // Rationale (truncated)
            div()
                .w(px(200.0))
                .text_size(px(11.0))
                .text_color(theme::fg_faint())
                .child(if driver.rationale.is_empty() {
                    "—".to_string()
                } else {
                    crate::truncate(&driver.rationale, 30)
                }),
        )
}

fn render_model_section(state: &ComposerState) -> impl IntoElement {
    render_card(
        "Model Expression",
        theme::ORANGE,
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child("How do drivers combine to produce the forecast?"),
            )
            .child(
                div()
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .rounded(px(4.0))
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::fg_faint())
                    .text_size(px(13.0))
                    .font_family("Ubuntu Mono, DejaVu Sans Mono, Liberation Mono, monospace")
                    .text_color(if state.model_expression.is_empty() {
                        theme::fg_faint()
                    } else {
                        theme::fg()
                    })
                    .child(if state.model_expression.is_empty() {
                        "e.g. market_size * growth_rate * (if contract then 1.3 else 1.0)"
                            .to_string()
                    } else {
                        state.model_expression.clone()
                    }),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(render_button("Auto-generate", theme::FG_DIM))
                    .child(
                        div().flex().items_center().gap(px(8.0)).child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child(format!("{} iterations", state.iterations)),
                        ),
                    ),
            ),
    )
}

fn render_results_section(results: &SimulationResults) -> impl IntoElement {
    render_card(
        "Simulation Results",
        theme::CYAN,
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            // Stats row
            .child(
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(render_stat("Mean", &format!("{:.2}", results.mean)))
                    .child(render_stat("Median", &format!("{:.2}", results.median)))
                    .child(render_stat("Std Dev", &format!("{:.2}", results.std_dev)))
                    .child(render_stat("P5", &format!("{:.2}", results.p5)))
                    .child(render_stat("P95", &format!("{:.2}", results.p95)))
                    .child(render_stat("Min", &format!("{:.2}", results.min)))
                    .child(render_stat("Max", &format!("{:.2}", results.max))),
            )
            // Histogram (text-based bar chart)
            .child(render_histogram(&results.histogram))
            // Footer
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(format!(
                        "{} iterations in {}ms",
                        results.iterations, results.execution_time_ms
                    )),
            ),
    )
}

fn render_histogram(histogram: &[(f64, usize)]) -> impl IntoElement {
    if histogram.is_empty() {
        return div().child("No histogram data").into_any_element();
    }

    let max_count = histogram.iter().map(|(_, c)| *c).max().unwrap_or(1);

    let bars: Vec<_> = histogram
        .iter()
        .map(|(bin_start, count)| {
            let bar_width = if max_count > 0 {
                (*count as f64 / max_count as f64) * 300.0
            } else {
                0.0
            };
            let label = format!("{:.1}", bin_start);
            let count_str = count.to_string();

            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .w(px(60.0))
                        .text_size(px(10.0))
                        .text_color(theme::fg_dim())
                        .child(label),
                )
                .child(
                    div()
                        .h(px(12.0))
                        .w(px(bar_width as f32))
                        .bg(theme::cyan())
                        .rounded(px(2.0)),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_faint())
                        .child(count_str),
                )
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::fg_dim())
                .mb(px(4.0))
                .child("Distribution"),
        )
        .children(bars)
        .into_any_element()
}

fn render_error_section(error: &str) -> impl IntoElement {
    div()
        .bg(rgb(0x3D1F1F))
        .rounded(px(8.0))
        .border_1()
        .border_color(theme::red())
        .p(px(16.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(theme::red())
                        .child("✗"),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme::red())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Simulation Error"),
                ),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(0xFFAAAA))
                .mt(px(8.0))
                .font_family("Ubuntu Mono, DejaVu Sans Mono, Liberation Mono, monospace")
                .child(error.to_string()),
        )
}

fn render_fpl_source_section(state: &ComposerState) -> impl IntoElement {
    let fpl = state.effective_fpl();

    render_card(
        "FPL Source",
        theme::FG_DIM,
        div()
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(4.0))
            .bg(theme::bg())
            .border_1()
            .border_color(theme::fg_faint())
            .text_size(px(12.0))
            .font_family("Ubuntu Mono, DejaVu Sans Mono, Liberation Mono, monospace")
            .text_color(theme::fg_dim())
            .child(if fpl.is_empty() {
                "# Empty — add a question and drivers".to_string()
            } else {
                fpl
            }),
    )
}

fn render_empty_hint() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .py(px(40.0))
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(36.0))
                .text_color(theme::fg_faint())
                .child("✎"),
        )
        .child(
            div()
                .text_size(px(16.0))
                .text_color(theme::fg_dim())
                .child("Create a new forecast"),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(theme::fg_faint())
                .max_w(px(400.0))
                .text_center()
                .child(
                    "Start by entering a question, then add drivers with probability \
                     distributions. The model expression defines how drivers combine. \
                     Run a Monte Carlo simulation locally — instant, no server needed.",
                ),
        )
        .child(
            div()
                .flex()
                .gap(px(16.0))
                .mt(px(8.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child("⌘E toggle FPL source"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child("⌘R run simulation"),
                ),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Reusable UI components
// ═══════════════════════════════════════════════════════════════════

fn render_card(title: &str, accent: u32, content: impl IntoElement) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .bg(theme::bg_elevated())
        .rounded(px(8.0))
        .border_1()
        .border_color(theme::fg_faint())
        .child(
            div()
                .px(px(16.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(theme::fg_faint())
                .text_size(px(13.0))
                .text_color(rgb(accent))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
        .child(div().p(px(16.0)).child(content))
}

fn render_field_display(label: &str, value: &str, is_placeholder: bool) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .flex_grow()
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::fg_dim())
                .child(label.to_string()),
        )
        .child(
            div()
                .w_full()
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .bg(theme::bg())
                .border_1()
                .border_color(theme::fg_faint())
                .text_size(px(13.0))
                .text_color(if is_placeholder {
                    theme::fg_faint()
                } else {
                    theme::fg()
                })
                .child(value.to_string()),
        )
}

fn render_stat(label: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(2.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme::fg_dim())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(14.0))
                .text_color(theme::fg())
                .font_weight(FontWeight::BOLD)
                .child(value.to_string()),
        )
}

fn render_button(label: &str, color: u32) -> impl IntoElement {
    div()
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(4.0))
        .bg(theme::bg_active())
        .text_size(px(12.0))
        .text_color(rgb(color))
        .cursor_pointer()
        .hover(|style| style.bg(theme::bg_hover()))
        .child(label.to_string())
}
