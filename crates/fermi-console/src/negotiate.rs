//! Query composition by contract, not by agent id.
//!
//! # Why this exists
//!
//! The console has to ask a research agent for evidence about one driver of
//! a forecast. The question is *how* to ask, and the console used to answer
//! it with a hardcoded match on the agent's identifier:
//!
//! ```text
//! match (domain, agent_id) {
//!     (_, "sentiment_analyzer") => "Analyze sentiment around '{driver}' …",
//!     (_, "equity_analyst")     => "… PULL FROM FMP API: …",
//!     (_, "biotech_analyst")    => "… [BASE RATE] phase + historical POS …",
//!     _                         => generic,
//! }
//! ```
//!
//! That is a closed world, and it fails in three separate ways.
//!
//! **It locks out every agent the console has never heard of.** An agent
//! designed by someone else — admitted to the orchestra, approved, listed in
//! the picker — can only ever receive the generic fallback. It is
//! structurally second-class no matter how precisely its card describes what
//! it does. Since the entire point of the orchestra is to compose fleets
//! across heterogeneous providers and designers, a mechanism that can only
//! ask well-formed questions of agents enumerated at compile time optimises
//! for the patterns already known and forecloses the ones worth discovering.
//!
//! **It contradicts the declarations it duplicates.** Every Fermi orchestra
//! member declares `fermi_contract.finding_labels` — in fact
//! `fermi_contract IS NOT NULL` *is* the membership predicate, and
//! `validate_fermi_contract` rejects a request that omits the labels. So the
//! labels always exist, are admin-reviewed, and are authoritative. The
//! hardcoded templates were a third copy that had drifted:
//! `sentiment_analyzer` declares
//! `[BASE RATE, SENTIMENT SCORE, INDICATOR, CONTRARIAN, MULTIPLIER]`, while
//! the console asked it for five numbered prose sections with no labels at
//! all — so the reply could not be parsed into findings, and the driver it
//! was supposed to update got nothing back it could use.
//!
//! **It duplicates what the agent already knows.** The console's
//! `equity_analyst` arm said "PULL FROM FMP API"; that agent's own system
//! prompt already says it has FMP access and MUST ground its analysis in it.
//! The `biotech_analyst` arm restated a tool-usage order and base-rate
//! anchoring that its system prompt spells out at length. The caller was
//! describing the agent's expertise back to it, which is both redundant and
//! a standing invitation to drift.
//!
//! # What replaces it
//!
//! The coordinator supplies the *task* — the forecast question, which driver,
//! the current estimate, the rationale. The agent supplies the *shape* — how
//! it wants to be asked and what it will return. Composition is a ladder over
//! what the agent declares, with no branch anywhere on who the agent is:
//!
//! 1. [`AgentContract::prompt_template`] — the designer wrote the prompt
//!    themselves. Highest authority; interpolate the task into it.
//! 2. [`AgentContract::finding_labels`] — the declared output contract. Ask
//!    for exactly those labels, bounded by the declared multiplier range.
//! 3. Neither declared — a generic request, the honest floor.
//!
//! A new agent design becomes first-class by declaring a contract, which it
//! must do anyway to be admitted. No console release is involved. Which
//! rung each run used is reported in [`ComposedQuery::source`], so
//! "declaration quality vs. outcome" is answerable from data rather than
//! folklore — the input the adaptation loop needs.
//!
//! Lives in the lib target because the binary's `#[cfg(test)]` modules are
//! unrunnable (see the crate docs on rustc's stack overflow when expanding
//! the GPUI element tree under `--test`).

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// What an agent declares about how it expects to be invoked.
///
/// Read from the agent's card. Every field is optional because the whole
/// point is to degrade gracefully for an agent that declares little, rather
/// than to require a shape the console can enumerate.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentContract {
    /// Labels the agent uses in `key_findings`, from
    /// `fermi_contract.finding_labels`.
    pub finding_labels: Vec<String>,
    /// Valid `[min, max]` for multiplier suggestions, from
    /// `fermi_contract.multiplier_range`.
    pub multiplier_range: Option<[f64; 2]>,
    /// A prompt the designer authored for this agent, from the card's
    /// top-level `prompt_template`.
    pub prompt_template: Option<String>,
    /// Semantic input labels the agent accepts, from the card's `accepts`.
    /// Not used for composition; carried so callers can report a mismatch
    /// rather than silently sending an input nothing consumes.
    pub accepts: Vec<String>,
    /// Question domains this agent claims competence in, from
    /// `metadata.domains`, falling back to `metadata.tags`.
    ///
    /// This is how an agent becomes ROUTABLE by declaration rather than by
    /// being enumerated in a compile-time table. `routing::domain_specialist`
    /// is a `match` over four domains, so a weather agent — admitted to the
    /// orchestra, declaring a full contract, and holding purpose-built tools —
    /// was still unreachable for climate questions, and every driver fell to
    /// the generalist. Two production forecasts returned their own
    /// climatological base rate as a result: London 32C at 0.3% against a
    /// market of 13.5%, and Chicago at 23.2% against 0.5%.
    ///
    /// Fewer declared domains is treated as MORE specialised, so a generalist
    /// claiming everything cannot crowd out a specialist claiming one thing.
    pub domains: Vec<String>,
    /// Whether `metadata.domains` was actually present on the card.
    ///
    /// The distinction matters because tags are written for SEARCH, not
    /// routing, so the tag fallback is a heuristic and an explicit declaration
    /// is not. An explicit declaration outranks a tag match, and an explicitly
    /// EMPTY `domains: []` means "I serve no question domain directly" — which
    /// is how a composition's internal members opt out of being routed to.
    ///
    /// Without that opt-out, `weather_calibrator` (tagged "weather") won the
    /// climate route ahead of `weather_oracle`, and would have been handed a
    /// raw driver it is explicitly built not to research.
    pub domains_explicit: bool,
}

impl AgentContract {
    /// Read a contract out of an agent card.
    ///
    /// Tolerates the three shapes the same declaration arrives in:
    /// `capabilities.fermi_contract` (on-disk curated cards),
    /// top-level `fermi_contract` (`/api/agents`, and the
    /// `/api/orchestras/{name}/members` roster rows).
    pub fn from_card(card: &JsonValue) -> Self {
        let fc = card
            .get("capabilities")
            .and_then(|c| c.get("fermi_contract"))
            .or_else(|| card.get("fermi_contract"))
            .filter(|v| !v.is_null());

        let finding_labels = fc
            .and_then(|f| f.get("finding_labels"))
            .and_then(|l| l.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        // Only a well-ordered pair is a range. A reversed or partial one is
        // a defect in the card, and quoting it back at the agent as a bound
        // would turn that defect into a wrong instruction.
        let multiplier_range = fc
            .and_then(|f| f.get("multiplier_range"))
            .and_then(|r| r.as_array())
            .and_then(|a| match a.as_slice() {
                [lo, hi] => match (lo.as_f64(), hi.as_f64()) {
                    (Some(lo), Some(hi)) if lo < hi => Some([lo, hi]),
                    _ => None,
                },
                _ => None,
            });

        let prompt_template = card
            .get("prompt_template")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        // `metadata.domains` is the explicit declaration. `metadata.tags` is
        // the fallback so existing cards gain some routability without an edit,
        // at the cost of precision — tags are written for search, not routing.
        let str_list = |v: Option<&JsonValue>| -> Vec<String> {
            v.and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str())
                        .map(|s| s.trim().to_ascii_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let meta = card.get("metadata");
        let declared = meta.and_then(|m| m.get("domains"));
        let domains_explicit = declared.is_some_and(|v| v.is_array());
        let mut domains = if domains_explicit {
            str_list(declared)
        } else {
            str_list(meta.and_then(|m| m.get("tags")))
        };
        domains.sort();
        domains.dedup();

        let accepts = card
            .get("accepts")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        Self {
            finding_labels,
            multiplier_range,
            prompt_template,
            accepts,
            domains,
            domains_explicit,
        }
    }

    /// Whether this contract says anything usable about how to ask.
    pub fn is_declared(&self) -> bool {
        self.prompt_template.is_some() || !self.finding_labels.is_empty()
    }

    /// Does this agent claim competence in `domain`?
    ///
    /// Matched both ways so `"climate"` finds an agent declaring `"climate"`
    /// and a domain of `"sports_nba"` finds one declaring `"nba"`. Underscores
    /// and hyphens are equivalent, since cards use both.
    pub fn claims_domain(&self, domain: &str) -> bool {
        let d = domain.trim().to_ascii_lowercase().replace('-', "_");
        if d.is_empty() || d == "general" {
            return false;
        }
        self.domains.iter().any(|t| {
            let t = t.replace('-', "_");
            t == d || d.split('_').any(|part| part == t) || t.split('_').any(|part| part == d)
        })
    }

    /// How narrow this agent's claim is. Fewer domains = more specialised, and
    /// wins ties, so a generalist tagging itself with twenty subjects cannot
    /// displace a specialist that claims one.
    pub fn specialisation(&self) -> usize {
        self.domains.len()
    }
}

/// Index contracts by agent identifier from any endpoint that returns cards.
///
/// Handles `{ "agents": [...] }` (`/api/agents`), `{ "members": [...] }`
/// (`/api/orchestras/{name}/members`) and a bare array. Indexes under both
/// `agent_id` and `agent_name` because the two endpoints disagree about
/// which identifier they expose, and a caller holds whichever it was given.
pub fn contracts_from_response(body: &JsonValue) -> HashMap<String, AgentContract> {
    let rows = body
        .get("agents")
        .and_then(|v| v.as_array())
        .or_else(|| body.get("members").and_then(|v| v.as_array()))
        .or_else(|| body.as_array());

    let mut out = HashMap::new();
    for row in rows.into_iter().flatten() {
        let contract = AgentContract::from_card(row);
        if !contract.is_declared() {
            continue;
        }
        for key in ["agent_id", "agent_name"] {
            if let Some(id) = row.get(key).and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    out.insert(id.to_string(), contract.clone());
                }
            }
        }
    }
    out
}

/// The work the coordinator needs done. Provider- and designer-agnostic:
/// nothing here refers to any particular agent.
#[derive(Debug, Clone, PartialEq)]
pub struct ResearchTask {
    /// The forecast question, verbatim.
    pub question: String,
    /// Human-readable driver name (`display_name`, falling back to the id).
    pub driver_display: String,
    /// The driver's FPL identifier.
    pub driver_name: String,
    /// The driver's rationale — why it is in the model at all.
    pub rationale: String,
    pub p5: f64,
    pub p50: f64,
    pub p95: f64,
}

/// Which rung of the ladder produced a query.
///
/// Recorded so the adaptation loop can correlate declaration quality with
/// outcome: if `Undeclared` runs fail more often, that is evidence about the
/// cards, not a mystery about the agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySource {
    /// The agent's own `prompt_template`.
    AgentTemplate,
    /// Composed from the agent's declared `finding_labels`.
    DeclaredContract,
    /// The agent declared nothing to compose from.
    Undeclared,
    /// A human wrote or edited this query. Never overridden.
    UserAuthored,
}

impl QuerySource {
    pub fn as_str(self) -> &'static str {
        match self {
            QuerySource::AgentTemplate => "agent_template",
            QuerySource::DeclaredContract => "declared_contract",
            QuerySource::Undeclared => "undeclared",
            QuerySource::UserAuthored => "user_authored",
        }
    }
}

/// Input labels that denote free-form natural-language text.
///
/// Deliberately narrow. A false positive here means staying quiet about a
/// real interface mismatch; a false negative means crying wolf at a
/// correctly-declared agent, which is how a check earns the right to be
/// ignored. Vocabulary varies by designer — `weather_oracle` says
/// `forecast-question`, `macro_data_agent` says `factor-x1-query` — so match
/// on the shape of the word, not an enumeration of known labels.
fn is_text_input(label: &str) -> bool {
    let l = label.to_ascii_lowercase();
    l.contains("query")
        || l.contains("question")
        || l.contains("prompt")
        || matches!(l.as_str(), "content" | "topic" | "narrative" | "text")
}

/// How a free-text research prompt maps onto what the agent says it accepts.
///
/// The pipeline audit found thirteen stages binding an agent to an interface
/// it never declared (`rabble_curator` handing `ar_beacon` a
/// `creature-record` when `ar_beacon` accepts `description`/`location`). The
/// console's research path can do the same thing: it sends free text to
/// whichever agent routing picked, having never checked that the agent
/// advertises a free-text input at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputBinding {
    /// The agent declares a question-shaped input. Carries the agent's own
    /// label for it, so a report speaks the designer's vocabulary.
    Declared(String),
    /// The agent declares inputs, none of which take free text. Sending a
    /// research prompt binds it to an interface it never advertised.
    NoTextInput(Vec<String>),
    /// The agent declares no inputs at all. Not a mismatch — an absence.
    Undeclared,
}

impl InputBinding {
    /// Stable label for logs and telemetry.
    pub fn as_str(&self) -> String {
        match self {
            InputBinding::Declared(label) => format!("declared:{}", label),
            InputBinding::NoTextInput(_) => "no_text_input".to_string(),
            InputBinding::Undeclared => "undeclared".to_string(),
        }
    }

    /// True only for a genuine mismatch — the agent declared its inputs and
    /// none of them is text. An agent that declared nothing has not
    /// contradicted anything.
    pub fn is_mismatch(&self) -> bool {
        matches!(self, InputBinding::NoTextInput(_))
    }
}

/// Resolve which declared input a free-text prompt is being sent as.
///
/// Prefers the canonical `query` when present so the common case reports a
/// stable label rather than whichever synonym happened to sort first.
pub fn bind_input(contract: Option<&AgentContract>) -> InputBinding {
    let accepts: &[String] = match contract {
        Some(c) if !c.accepts.is_empty() => &c.accepts,
        _ => return InputBinding::Undeclared,
    };

    if let Some(exact) = accepts.iter().find(|a| a.eq_ignore_ascii_case("query")) {
        return InputBinding::Declared(exact.clone());
    }
    if let Some(shaped) = accepts.iter().find(|a| is_text_input(a)) {
        return InputBinding::Declared(shaped.clone());
    }
    InputBinding::NoTextInput(accepts.to_vec())
}

/// The record of how one invocation came to be asked the way it was.
///
/// Travels with the run to the server, which stamps it onto the episode. The
/// point is the join: outcome is already recorded per episode (status,
/// failure reason, confidence, and eventually a Brier score once the forecast
/// resolves), so recording *how the agent was asked* alongside it turns
/// "which agents and compositions actually work" into a query rather than an
/// opinion. Without it, a run that failed because the caller sent it the
/// wrong shape of question is indistinguishable from one that failed on
/// merit — and adaptation driven by that signal would learn the wrong thing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvocationProvenance {
    /// Which rung of the composition ladder produced the query.
    pub query_source: String,
    /// How the prompt mapped onto the agent's declared inputs.
    pub input_binding: String,
    /// Number of finding labels the agent declared. The crude proxy for
    /// "how much did this agent tell us about itself", which is the axis we
    /// want to correlate against outcome.
    pub declared_label_count: usize,
    /// Set when a stale pre-fill was discarded: the agent it was written for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recomposed_from: Option<String>,
    /// The driver this run was researching, for joining back to the forecast.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,

    // ── Why this agent, not another ───────────────────────────────────────
    //
    // The fields above record how the agent was ASKED. These record how it
    // was CHOSEN. Both halves are needed for the same reason: an agent that
    // underperformed as the generalist fallback looks identical, in outcome
    // data, to one deliberately selected as the resident domain expert and
    // found wanting. Without this, a credit model cannot separate the
    // router's coverage gaps from the agent's competence, and will learn to
    // distrust whichever agents the router happens to reach for by default.
    /// `RouteReason::slug()` — why the router picked this agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_reason: Option<String>,
    /// Whether the reason reflects a positive signal rather than a fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_deliberate: Option<bool>,
    /// The agent Fermi suggested, when it differed from the one used.
    ///
    /// Present only on disagreement, which makes "how often is the strategist
    /// overruled, and was overruling it right?" a single query. That is the
    /// feedback the decomposition side currently has no way to receive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_overrode_suggestion: Option<String>,
    /// The question's detected domain at routing time.
    ///
    /// Recorded because routing quality is meaningless in aggregate and only
    /// meaningful per domain: `domain_specialist` beating `default` overall
    /// says nothing about whether the specialist chosen for *climate* is the
    /// right one. This is the grouping key that lets a measured ranking
    /// replace the compile-time `domain_specialist` table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_domain: Option<String>,
}

impl InvocationProvenance {
    pub fn new(
        composed: &ComposedQuery,
        binding: &InputBinding,
        contract: Option<&AgentContract>,
        driver: Option<&str>,
    ) -> Self {
        Self {
            query_source: composed.source.as_str().to_string(),
            input_binding: binding.as_str(),
            declared_label_count: contract.map(|c| c.finding_labels.len()).unwrap_or(0),
            recomposed_from: composed.recomposed_from.clone(),
            driver: driver.map(str::to_string),
            route_reason: None,
            route_deliberate: None,
            route_overrode_suggestion: None,
            route_domain: None,
        }
    }

    /// Record why the router chose this agent.
    ///
    /// `reason_slug` should be `RouteReason::slug()`. `suggested` is Fermi's
    /// pick; it is retained only when it differs from `agent_used`, so the
    /// presence of the field is itself the "strategist was overruled" signal.
    ///
    /// Kept as a builder rather than a `new` parameter so the routing crate
    /// and the composition crate stay independently testable — `negotiate`
    /// does not depend on `routing`'s enum.
    pub fn with_route(
        mut self,
        reason_slug: &str,
        deliberate: bool,
        agent_used: &str,
        suggested: Option<&str>,
        domain: &str,
    ) -> Self {
        self.route_reason = Some(reason_slug.to_string());
        self.route_deliberate = Some(deliberate);
        self.route_overrode_suggestion = suggested
            .map(str::trim)
            .filter(|s| !s.is_empty() && *s != agent_used)
            .map(str::to_string);
        self.route_domain = Some(domain.trim().to_string()).filter(|d| !d.is_empty());
        self
    }

    pub fn to_json(&self) -> JsonValue {
        serde_json::to_value(self).unwrap_or(JsonValue::Null)
    }
}

/// A query, plus how it came to be.
#[derive(Debug, Clone, PartialEq)]
pub struct ComposedQuery {
    pub text: String,
    pub source: QuerySource,
    /// Set when a stale pre-fill was discarded: the agent it had been
    /// composed for. Callers should surface this — silently swapping a
    /// prompt is how the original bug stayed invisible.
    pub recomposed_from: Option<String>,
}

/// A pre-filled query the console generated, and who it was generated for.
///
/// The console writes its suggestion into the same text field the user types
/// into, so "machine suggestion" and "human intent" are indistinguishable by
/// inspection. Remembering what we wrote, and for whom, is what makes them
/// distinguishable again.
#[derive(Debug, Clone, PartialEq)]
pub struct Prefill {
    pub text: String,
    pub agent_id: String,
}

fn fmt_estimate(task: &ResearchTask) -> String {
    format!(
        "Current estimate: p5={:.2}, p50={:.2}, p95={:.2}",
        task.p5, task.p50, task.p95
    )
}

/// The `[MULTIPLIER]` request line.
///
/// Always asked for, even by an agent whose card omits the label. That is
/// not a presumption about the agent's expertise — it is the one output the
/// console structurally cannot use the answer without, since a driver update
/// *is* a multiplier. The exact format matches the one the orchestra's
/// system prompts already mandate.
fn multiplier_line(contract: Option<&AgentContract>) -> String {
    let bound = match contract.and_then(|c| c.multiplier_range) {
        Some([lo, hi]) => format!(" Keep p50 within [{:.2}, {:.2}].", lo, hi),
        None => String::new(),
    };
    format!(
        "[MULTIPLIER] Suggested p50: X.XX (p5: X.XX, p95: X.XX) — one-sentence rationale.{}",
        bound
    )
}

/// Interpolate a task into an agent-authored template.
///
/// Placeholders are replaced wherever they appear; unknown text is left
/// exactly as written, because the designer's prose is not ours to edit.
fn render_template(template: &str, task: &ResearchTask) -> String {
    let subs: [(&str, String); 7] = [
        ("{question}", task.question.clone()),
        ("{driver}", task.driver_display.clone()),
        ("{driver_name}", task.driver_name.clone()),
        ("{rationale}", task.rationale.clone()),
        ("{p5}", format!("{:.2}", task.p5)),
        ("{p50}", format!("{:.2}", task.p50)),
        ("{p95}", format!("{:.2}", task.p95)),
    ];
    let mut out = template.to_string();
    for (needle, value) in subs {
        out = out.replace(needle, &value);
    }
    out
}

/// Compose a research query for `agent_id` from what that agent declares.
///
/// There is deliberately no branch on `agent_id` anywhere in this function
/// or the ones it calls. `agent_id` is carried for reporting only. If you
/// find yourself adding a match on it, the fix belongs in the agent's card.
pub fn compose_query(task: &ResearchTask, contract: Option<&AgentContract>) -> ComposedQuery {
    // Rung 1: the designer wrote the prompt.
    if let Some(template) = contract.and_then(|c| c.prompt_template.as_deref()) {
        return ComposedQuery {
            text: render_template(template, task),
            source: QuerySource::AgentTemplate,
            recomposed_from: None,
        };
    }

    let mut out = format!(
        "For the forecast: \"{}\"\n\nResearch the '{}' driver.\n{}\n",
        task.question,
        task.driver_display,
        fmt_estimate(task)
    );

    let declared: Vec<&String> = contract
        .map(|c| c.finding_labels.iter().collect())
        .unwrap_or_default();

    // Rung 2: ask for the labels the agent declared — and only list them.
    // What each label means is defined by the agent's own system prompt;
    // restating it here is what let the two drift apart.
    let source = if declared.is_empty() {
        out.push_str(
            "\nPROVIDE:\n\
             1. Key data points relevant to this driver (with sources and dates)\n\
             2. Historical base rate or comparable precedent\n\
             3. Your assessment of how this driver should move\n\
             4. Confidence (0.0-1.0) in your assessment\n",
        );
        out.push_str(&format!("\n{}\n", multiplier_line(contract)));
        QuerySource::Undeclared
    } else {
        out.push_str("\nReturn your findings using the labels your card declares:\n");
        for label in &declared {
            if label.eq_ignore_ascii_case("MULTIPLIER") {
                continue;
            }
            out.push_str(&format!("[{}]\n", label));
        }
        out.push_str(&multiplier_line(contract));
        out.push('\n');
        QuerySource::DeclaredContract
    };

    if !task.rationale.trim().is_empty() {
        out.push_str(&format!("\nContext: {}\n", task.rationale.trim()));
    }
    out.push_str("\nBe specific and quantitative — named sources, dates, figures.");

    ComposedQuery {
        text: out,
        source,
        recomposed_from: None,
    }
}

/// Decide what to actually send, given what is in the input box.
///
/// This is the guard the manual agent picker never had. The auto-assign path
/// has always re-composed when the chosen agent differed from the suggested
/// one ("a query written for a macro forecaster asks a football analyst the
/// wrong questions"); the picker composed once, when it opened, for the
/// *recommended* agent — then handed that text to whichever agent the user
/// clicked. A tester trying a third-party agent takes the recommendation's
/// prompt every time, which is how a thesis engine came to be asked for a
/// sentiment classification.
///
/// Rules, in order:
///
/// 1. Empty box → compose fresh for `chosen_agent`.
/// 2. Box still holds our own pre-fill, but it was written for a *different*
///    agent → re-compose, and report what was discarded.
/// 3. Box holds our pre-fill for this same agent → send it.
/// 4. Anything else → a human wrote it. Send verbatim, never overridden,
///    even if the agent changed. Authorship outranks our inference.
pub fn resolve_query(
    box_text: &str,
    prefill: Option<&Prefill>,
    chosen_agent: &str,
    task: &ResearchTask,
    contract: Option<&AgentContract>,
) -> ComposedQuery {
    if box_text.trim().is_empty() {
        return compose_query(task, contract);
    }

    let is_our_prefill = prefill.is_some_and(|p| normalise(&p.text) == normalise(box_text));

    if is_our_prefill {
        let generated_for = prefill.map(|p| p.agent_id.as_str()).unwrap_or_default();
        if generated_for != chosen_agent {
            let mut fresh = compose_query(task, contract);
            fresh.recomposed_from = Some(generated_for.to_string());
            return fresh;
        }
        return ComposedQuery {
            text: box_text.to_string(),
            source: prefill_source(prefill, contract),
            recomposed_from: None,
        };
    }

    ComposedQuery {
        text: box_text.to_string(),
        source: QuerySource::UserAuthored,
        recomposed_from: None,
    }
}

/// Which rung an untouched pre-fill came from. Cheaper than storing it, and
/// it cannot disagree with what `compose_query` would do today.
fn prefill_source(prefill: Option<&Prefill>, contract: Option<&AgentContract>) -> QuerySource {
    let _ = prefill;
    match contract {
        Some(c) if c.prompt_template.is_some() => QuerySource::AgentTemplate,
        Some(c) if !c.finding_labels.is_empty() => QuerySource::DeclaredContract,
        _ => QuerySource::Undeclared,
    }
}

/// Compare ignoring whitespace shape.
///
/// The console flattens newlines out of a suggestion before putting it in a
/// single-line input (`replace('\n', " ")`), so a byte comparison against
/// the composed text would call every untouched pre-fill "user-edited" and
/// disable the guard entirely.
fn normalise(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn task() -> ResearchTask {
        ResearchTask {
            question: "Will GOOG hit 450?".into(),
            driver_display: "AI Product Execution".into(),
            driver_name: "ai_product_execution".into(),
            rationale: "Gemini shipping cadence versus expectations.".into(),
            p5: 0.8,
            p50: 1.05,
            p95: 1.3,
        }
    }

    /// The real `sentiment_analyzer` declaration, as shipped.
    fn sentiment_card() -> JsonValue {
        json!({
            "agent_id": "sentiment_analyzer",
            "accepts": ["content", "query", "topic", "narrative"],
            "capabilities": {
                "fermi_contract": {
                    "finding_labels": ["BASE RATE", "SENTIMENT SCORE", "INDICATOR", "CONTRARIAN", "MULTIPLIER"],
                    "multiplier_range": [0.3, 2.5]
                }
            }
        })
    }

    /// A third-party agent nobody hardcoded: the case the old match
    /// structurally could not serve.
    fn third_party_card() -> JsonValue {
        json!({
            "agent_name": "efra_critical_factor",
            "fermi_contract": {
                "finding_labels": ["BASE RATE", "CRITICAL FACTOR", "EPS IMPACT", "SCENARIO", "MULTIPLIER"],
                "multiplier_range": [0.5, 2.0]
            }
        })
    }

    // ── Reading declarations ──────────────────────────────────────

    #[test]
    fn reads_a_contract_nested_under_capabilities() {
        let c = AgentContract::from_card(&sentiment_card());
        assert_eq!(c.finding_labels.len(), 5);
        assert_eq!(c.multiplier_range, Some([0.3, 2.5]));
        assert_eq!(c.accepts.len(), 4);
        assert!(c.is_declared());
    }

    #[test]
    fn reads_a_contract_at_the_top_level() {
        let c = AgentContract::from_card(&third_party_card());
        assert!(c.finding_labels.contains(&"CRITICAL FACTOR".to_string()));
        assert_eq!(c.multiplier_range, Some([0.5, 2.0]));
    }

    #[test]
    fn an_agent_with_no_contract_declares_nothing() {
        let c = AgentContract::from_card(&json!({ "agent_id": "x" }));
        assert!(!c.is_declared());
        assert!(c.finding_labels.is_empty());
    }

    #[test]
    fn a_null_contract_is_not_a_declaration() {
        // `/api/agents` emits `fermi_contract: null` for non-members.
        let c = AgentContract::from_card(&json!({ "agent_id": "x", "fermi_contract": null }));
        assert!(!c.is_declared());
    }

    #[test]
    fn a_reversed_multiplier_range_is_rejected_rather_than_quoted_back() {
        let c = AgentContract::from_card(&json!({
            "fermi_contract": { "finding_labels": ["MULTIPLIER"], "multiplier_range": [2.5, 0.3] }
        }));
        assert_eq!(
            c.multiplier_range, None,
            "a defective range must not become an instruction"
        );
    }

    #[test]
    fn blank_labels_are_dropped() {
        let c = AgentContract::from_card(&json!({
            "fermi_contract": { "finding_labels": ["BASE RATE", "", "  ", "MULTIPLIER"] }
        }));
        assert_eq!(c.finding_labels, vec!["BASE RATE", "MULTIPLIER"]);
    }

    #[test]
    fn indexes_contracts_from_both_response_shapes() {
        let agents = json!({ "agents": [sentiment_card(), json!({"agent_id": "plain"})] });
        let members = json!({ "members": [third_party_card()] });

        let a = contracts_from_response(&agents);
        assert!(a.contains_key("sentiment_analyzer"));
        assert!(
            !a.contains_key("plain"),
            "an undeclared agent needs no entry"
        );

        let m = contracts_from_response(&members);
        assert!(m.contains_key("efra_critical_factor"));
    }

    // ── Composition ───────────────────────────────────────────────

    #[test]
    fn composes_from_the_declared_labels() {
        let c = AgentContract::from_card(&sentiment_card());
        let q = compose_query(&task(), Some(&c));

        assert_eq!(q.source, QuerySource::DeclaredContract);
        for label in [
            "[BASE RATE]",
            "[SENTIMENT SCORE]",
            "[INDICATOR]",
            "[CONTRARIAN]",
        ] {
            assert!(q.text.contains(label), "missing {label} in:\n{}", q.text);
        }
        assert!(q.text.contains("Will GOOG hit 450?"));
        assert!(q.text.contains("AI Product Execution"));
        assert!(q.text.contains("p50=1.05"));
        assert!(q.text.contains("Gemini shipping cadence"));
    }

    #[test]
    fn asks_for_the_multiplier_within_the_declared_range() {
        let c = AgentContract::from_card(&sentiment_card());
        let q = compose_query(&task(), Some(&c));
        assert!(q
            .text
            .contains("[MULTIPLIER] Suggested p50: X.XX (p5: X.XX, p95: X.XX)"));
        assert!(q.text.contains("within [0.30, 2.50]"), "{}", q.text);
    }

    #[test]
    fn the_multiplier_label_is_not_listed_twice() {
        let c = AgentContract::from_card(&sentiment_card());
        let q = compose_query(&task(), Some(&c));
        assert_eq!(
            q.text.matches("[MULTIPLIER]").count(),
            1,
            "declared MULTIPLIER must merge with the protocol line:\n{}",
            q.text
        );
    }

    /// The whole point. A third-party agent gets a prompt shaped to its own
    /// declaration, with no console change and no entry in any table.
    #[test]
    fn a_third_party_agent_gets_a_contract_shaped_prompt() {
        let c = AgentContract::from_card(&third_party_card());
        let q = compose_query(&task(), Some(&c));

        assert_eq!(q.source, QuerySource::DeclaredContract);
        assert!(q.text.contains("[CRITICAL FACTOR]"));
        assert!(q.text.contains("[EPS IMPACT]"));
        assert!(q.text.contains("[SCENARIO]"));
        assert!(q.text.contains("within [0.50, 2.00]"));
        // And nothing about sentiment, which is what it used to be sent.
        assert!(
            !q.text.to_lowercase().contains("sentiment"),
            "inherited another agent's task:\n{}",
            q.text
        );
    }

    #[test]
    fn an_agent_authored_template_wins_and_is_interpolated() {
        let card = json!({
            "agent_id": "bespoke",
            "prompt_template": "Q: {question}\nDriver {driver_name} ({driver}) at {p50}, band {p5}-{p95}.\nWhy: {rationale}",
            "fermi_contract": { "finding_labels": ["BASE RATE", "MULTIPLIER"] }
        });
        let c = AgentContract::from_card(&card);
        let q = compose_query(&task(), Some(&c));

        assert_eq!(q.source, QuerySource::AgentTemplate);
        assert!(q.text.contains("Q: Will GOOG hit 450?"));
        assert!(q.text.contains(
            "Driver ai_product_execution (AI Product Execution) at 1.05, band 0.80-1.30"
        ));
        assert!(q.text.contains("Why: Gemini shipping cadence"));
        // The designer's template is authoritative: we don't append our own
        // label list on top of it.
        assert!(!q.text.contains("[BASE RATE]"));
    }

    #[test]
    fn an_undeclared_agent_still_gets_a_usable_request() {
        let q = compose_query(&task(), None);
        assert_eq!(q.source, QuerySource::Undeclared);
        assert!(q.text.contains("Will GOOG hit 450?"));
        assert!(q.text.contains("PROVIDE:"));
        // Still asks for the multiplier — the console cannot use a reply
        // without one, whatever the agent failed to declare.
        assert!(q.text.contains("[MULTIPLIER]"));
    }

    #[test]
    fn no_rationale_means_no_empty_context_line() {
        let mut t = task();
        t.rationale = "   ".into();
        let q = compose_query(&t, None);
        assert!(!q.text.contains("Context:"), "{}", q.text);
    }

    /// Guards the invariant the module exists to hold. Two agents with
    /// identical declarations must get identical prompts; identity must not
    /// enter into it.
    #[test]
    fn composition_depends_on_the_declaration_not_the_identity() {
        let shape = json!({
            "fermi_contract": { "finding_labels": ["BASE RATE", "MULTIPLIER"], "multiplier_range": [0.5, 2.0] }
        });
        let mut a = shape.clone();
        a["agent_id"] = json!("well_known_curated_agent");
        let mut b = shape.clone();
        b["agent_id"] = json!("nobody_has_ever_heard_of_this_one");

        let qa = compose_query(&task(), Some(&AgentContract::from_card(&a)));
        let qb = compose_query(&task(), Some(&AgentContract::from_card(&b)));
        assert_eq!(qa.text, qb.text);
    }

    // ── The stale-prefill guard ───────────────────────────────────

    /// The reported bug, reproduced. The picker composed for the recommended
    /// agent, the user clicked a different one, and the prompt rode along.
    #[test]
    fn a_stale_prefill_is_recomposed_for_the_agent_actually_chosen() {
        let recommended = AgentContract::from_card(&sentiment_card());
        let prefill_text = compose_query(&task(), Some(&recommended)).text;
        let prefill = Prefill {
            text: prefill_text.clone(),
            agent_id: "sentiment_analyzer".into(),
        };

        let chosen = AgentContract::from_card(&third_party_card());
        let q = resolve_query(
            &prefill_text,
            Some(&prefill),
            "efra_critical_factor",
            &task(),
            Some(&chosen),
        );

        assert_eq!(q.recomposed_from.as_deref(), Some("sentiment_analyzer"));
        assert!(q.text.contains("[CRITICAL FACTOR]"));
        assert!(
            !q.text.contains("[SENTIMENT SCORE]"),
            "still carrying the other agent's contract:\n{}",
            q.text
        );
    }

    /// The console flattens newlines before putting a suggestion in the
    /// single-line input. If that counted as an edit, the guard above would
    /// never fire in production.
    #[test]
    fn newline_flattening_does_not_count_as_a_user_edit() {
        let recommended = AgentContract::from_card(&sentiment_card());
        let composed = compose_query(&task(), Some(&recommended)).text;
        let flattened = composed.replace('\n', " ").replace("  ", " ");
        let prefill = Prefill {
            text: composed,
            agent_id: "sentiment_analyzer".into(),
        };

        let chosen = AgentContract::from_card(&third_party_card());
        let q = resolve_query(
            &flattened,
            Some(&prefill),
            "efra_critical_factor",
            &task(),
            Some(&chosen),
        );
        assert_eq!(
            q.recomposed_from.as_deref(),
            Some("sentiment_analyzer"),
            "whitespace reshaping must not disable the guard"
        );
    }

    #[test]
    fn a_hand_written_query_is_never_overridden() {
        let prefill = Prefill {
            text: "machine suggestion".into(),
            agent_id: "sentiment_analyzer".into(),
        };
        let mine = "Just tell me the Gemini ship dates and skip the rest.";
        let chosen = AgentContract::from_card(&third_party_card());
        let q = resolve_query(
            mine,
            Some(&prefill),
            "efra_critical_factor",
            &task(),
            Some(&chosen),
        );

        assert_eq!(q.source, QuerySource::UserAuthored);
        assert_eq!(q.text, mine);
        assert_eq!(q.recomposed_from, None);
    }

    #[test]
    fn an_unedited_prefill_for_the_same_agent_is_sent_as_is() {
        let c = AgentContract::from_card(&sentiment_card());
        let text = compose_query(&task(), Some(&c)).text;
        let prefill = Prefill {
            text: text.clone(),
            agent_id: "sentiment_analyzer".into(),
        };
        let q = resolve_query(
            &text,
            Some(&prefill),
            "sentiment_analyzer",
            &task(),
            Some(&c),
        );

        assert_eq!(q.text, text);
        assert_eq!(q.recomposed_from, None);
        assert_eq!(q.source, QuerySource::DeclaredContract);
    }

    #[test]
    fn an_empty_box_composes_fresh() {
        let c = AgentContract::from_card(&third_party_card());
        let q = resolve_query("   ", None, "efra_critical_factor", &task(), Some(&c));
        assert_eq!(q.source, QuerySource::DeclaredContract);
        assert!(q.text.contains("[EPS IMPACT]"));
    }

    #[test]
    fn source_labels_are_stable_for_telemetry() {
        assert_eq!(QuerySource::AgentTemplate.as_str(), "agent_template");
        assert_eq!(QuerySource::DeclaredContract.as_str(), "declared_contract");
        assert_eq!(QuerySource::Undeclared.as_str(), "undeclared");
        assert_eq!(QuerySource::UserAuthored.as_str(), "user_authored");
    }

    // ── Input binding ────────────────────────────────────────

    fn accepting(labels: &[&str]) -> AgentContract {
        AgentContract {
            accepts: labels.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn prefers_the_canonical_query_label() {
        // sentiment_analyzer declares both `content` and `query`.
        let b = bind_input(Some(&accepting(&["content", "query", "topic"])));
        assert_eq!(b, InputBinding::Declared("query".into()));
        assert_eq!(b.as_str(), "declared:query");
        assert!(!b.is_mismatch());
    }

    /// Every Fermi orchestra member on disk declares *something*
    /// question-shaped, but they disagree about what to call it. A check that
    /// only recognised `query` would flag four correct cards.
    #[test]
    fn recognises_each_designers_own_word_for_a_question() {
        for (labels, expected) in [
            (
                vec!["forecast-question", "market-question", "evidence-set"],
                "forecast-question",
            ),
            (
                vec![
                    "country-code",
                    "country-list",
                    "indicator-request",
                    "factor-x1-query",
                ],
                "factor-x1-query",
            ),
            (
                vec![
                    "country-code",
                    "fixture-id",
                    "venue-list",
                    "factor-x6-query",
                ],
                "factor-x6-query",
            ),
            (
                vec!["country-code", "confederation-query"],
                "confederation-query",
            ),
        ] {
            let b = bind_input(Some(&accepting(&labels)));
            assert_eq!(
                b,
                InputBinding::Declared(expected.into()),
                "misread {labels:?}"
            );
            assert!(!b.is_mismatch(), "false positive on {labels:?}");
        }
    }

    #[test]
    fn flags_an_agent_that_takes_no_free_text() {
        // The `ar_beacon` shape from the pipeline audit.
        let b = bind_input(Some(&accepting(&[
            "description",
            "location",
            "gps-coordinates",
        ])));
        assert!(b.is_mismatch());
        assert_eq!(b.as_str(), "no_text_input");
        match b {
            InputBinding::NoTextInput(declared) => assert_eq!(declared.len(), 3),
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    #[test]
    fn declaring_nothing_is_an_absence_not_a_mismatch() {
        assert_eq!(bind_input(None), InputBinding::Undeclared);
        assert_eq!(bind_input(Some(&accepting(&[]))), InputBinding::Undeclared);
        assert!(
            !bind_input(None).is_mismatch(),
            "silence must not read as contradiction"
        );
    }

    // ── Provenance record ───────────────────────────────────

    #[test]
    fn provenance_captures_the_full_join_key() {
        let c = AgentContract::from_card(&sentiment_card());
        let composed = compose_query(&task(), Some(&c));
        let binding = bind_input(Some(&c));
        let p =
            InvocationProvenance::new(&composed, &binding, Some(&c), Some("ai_product_execution"));

        assert_eq!(p.query_source, "declared_contract");
        assert_eq!(p.input_binding, "declared:query");
        assert_eq!(p.declared_label_count, 5);
        assert_eq!(p.driver.as_deref(), Some("ai_product_execution"));
        assert_eq!(p.recomposed_from, None);

        let j = p.to_json();
        assert_eq!(j["query_source"], "declared_contract");
        assert_eq!(j["declared_label_count"], 5);
        // Absent optionals must not appear as nulls on the wire.
        assert!(j.get("recomposed_from").is_none());
    }

    #[test]
    fn provenance_records_an_undeclared_agent_as_such() {
        let composed = compose_query(&task(), None);
        let p = InvocationProvenance::new(&composed, &bind_input(None), None, None);
        assert_eq!(p.query_source, "undeclared");
        assert_eq!(p.input_binding, "undeclared");
        assert_eq!(p.declared_label_count, 0);
    }

    #[test]
    fn provenance_carries_a_recomposition_so_the_swap_is_auditable() {
        let recommended = AgentContract::from_card(&sentiment_card());
        let text = compose_query(&task(), Some(&recommended)).text;
        let prefill = Prefill {
            text: text.clone(),
            agent_id: "sentiment_analyzer".into(),
        };
        let chosen = AgentContract::from_card(&third_party_card());

        let resolved = resolve_query(
            &text,
            Some(&prefill),
            "efra_critical_factor",
            &task(),
            Some(&chosen),
        );
        let p = InvocationProvenance::new(
            &resolved,
            &bind_input(Some(&chosen)),
            Some(&chosen),
            Some("d"),
        );

        assert_eq!(p.recomposed_from.as_deref(), Some("sentiment_analyzer"));
        assert_eq!(p.to_json()["recomposed_from"], "sentiment_analyzer");
    }

    // ── Routing provenance ──────────────────────────────────────────────
    //
    // These cover the CHOOSING half of the record. The asking half is
    // covered above; both are needed before a credit model can tell a
    // router coverage gap apart from agent incompetence.

    fn bare_provenance() -> InvocationProvenance {
        let card = third_party_card();
        let contract = AgentContract::from_card(&card);
        InvocationProvenance::new(
            &ComposedQuery {
                text: "q".into(),
                source: QuerySource::DeclaredContract,
                recomposed_from: None,
            },
            &bind_input(Some(&contract)),
            Some(&contract),
            Some("some_driver"),
        )
    }

    #[test]
    fn route_fields_are_absent_until_recorded() {
        // Omission is deliberate: a run that never went through the router
        // must not be indistinguishable from one routed for an unknown
        // reason. `skip_serializing_if` keeps the two populations separable.
        let p = bare_provenance();
        assert!(p.route_reason.is_none());
        let j = p.to_json();
        assert!(j.get("route_reason").is_none());
        assert!(j.get("route_deliberate").is_none());
        assert!(j.get("route_overrode_suggestion").is_none());
    }

    #[test]
    fn a_deliberate_route_records_reason_and_no_override() {
        // Fermi suggested this agent and it was used, so there is no
        // disagreement to record.
        let p = bare_provenance().with_route(
            "domain_specialist",
            true,
            "weather_oracle",
            Some("weather_oracle"),
            "climate",
        );
        assert_eq!(p.route_reason.as_deref(), Some("domain_specialist"));
        assert_eq!(p.route_deliberate, Some(true));
        assert!(
            p.route_overrode_suggestion.is_none(),
            "agreement must not be recorded as an override"
        );
    }

    #[test]
    fn overruling_the_strategist_is_recorded_by_presence() {
        // The router displaced Fermi's generalist pick. The field's existence
        // is the signal, so "how often is the strategist overruled, and was
        // overruling it right?" becomes a single query joined to outcome.
        let p = bare_provenance().with_route(
            "domain_specialist",
            true,
            "weather_oracle",
            Some("macro_forecaster"),
            "climate",
        );
        assert_eq!(
            p.route_overrode_suggestion.as_deref(),
            Some("macro_forecaster")
        );
        assert_eq!(p.to_json()["route_overrode_suggestion"], "macro_forecaster");
    }

    #[test]
    fn a_fallback_route_is_marked_as_not_deliberate() {
        // The distinction that makes the whole record worth keeping: nothing
        // matched, so the outcome says more about router coverage than about
        // this agent.
        let p = bare_provenance().with_route("default", false, "macro_forecaster", None, "general");
        assert_eq!(p.route_deliberate, Some(false));
        assert_eq!(p.route_reason.as_deref(), Some("default"));
    }

    #[test]
    fn an_empty_suggestion_is_not_an_override() {
        for suggested in [Some(""), Some("   "), None] {
            let p = bare_provenance().with_route(
                "keyword",
                true,
                "nba_analyst",
                suggested,
                "sports_nba",
            );
            assert!(
                p.route_overrode_suggestion.is_none(),
                "blank suggestion {suggested:?} must not read as an override"
            );
        }
    }
}
