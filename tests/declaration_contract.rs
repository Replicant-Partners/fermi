//! Every declared field names its consumer, or admits it has none.
//!
//! # The defect class this exists to close
//!
//! The recurring failure in this codebase is not a wrong computation. It is **a
//! declared thing that nothing consults**: present, well-typed, internally
//! consistent, compared against nothing. A partial list, all found by accident:
//!
//! | declaration | state when found |
//! |---|---|
//! | `DriverStmt.constraints` | `let constraints = Vec::new();` — not even `mut` |
//! | `AssertionKind::Probability` | the variant existed; nothing ever constructed one |
//! | `Assertion.target_hint` | 0 of 3 construction sites populated it, 0 readers |
//! | `BaseRate.generated_by` | required, parsed, emitted, persisted, rendered nowhere |
//! | `forecast_spacetime.triggering_agent` | column since mig-140; writer passed `None` |
//! | `DriverStmt.applies_to` | dropped by the console's FPL emitter |
//! | `Density::from_samples` | written and tested; zero non-test callers |
//!
//! Every one is the shadow side of a good decision. This platform is
//! deliberately declaration-driven rather than driven by hardcoded Rust tables,
//! and that is the right choice — an agent becomes routable for a new domain by
//! editing its card, not by shipping a console release. But it has one specific
//! failure mode: **adding a declaration is cheap, adding its reader is separate
//! work, and nothing fails in between.** A hardcoded table breaks loudly when
//! incomplete. A declaration just sits there looking correct.
//!
//! # How this test works
//!
//! Two mechanisms, and the first is the one that matters.
//!
//! 1. **Exhaustive destructuring.** Each registry is checked against a `let`
//!    pattern that names every field with no `..` rest pattern. Adding a field
//!    to `DriverStmt` therefore *fails to compile here* until somebody writes
//!    down who reads it. The compiler enforces completeness; no list can drift.
//!
//! 2. **A reason, not a checkbox.** An `Orphan` entry must explain what closing
//!    the gap would take. `CROSS_CHECK_EXEMPTIONS` in `grounding_trust` is the
//!    model: the point is not that everything is consumed, it is that nothing
//!    is *silently* unconsumed.
//!
//! # What this test is not
//!
//! It does not prove a consumer exists. A text search for `.field` was tried
//! first and produced false results in both directions — `source`, `query` and
//! `kind` collide across unrelated structs, and one filter accidentally hid
//! `src/assertions.rs` from itself and reported a consumed field as orphaned.
//! Shipping a detector whose measurements cannot be trusted would be the same
//! mistake one level up. So this forces a human statement per field and makes
//! the statement reviewable in a diff. That is a weaker claim, honestly made.

use fermi::ast::{BaseRate, DriverStmt, GeneratedBy};

/// What reads a declared field.
#[derive(Debug, Clone, Copy)]
enum Consumer {
    /// Something reads it and acts on it. Names what, with enough specificity
    /// that a reviewer can check the claim.
    ReadBy(&'static str),
    /// Nothing reads it. Says why that is a defect and what closing it needs.
    Orphan(&'static str),
    /// Read by something real, but never written on the live path — the same
    /// disconnect with the arrow reversed, and just as silent.
    NeverPopulated(&'static str),
}

impl Consumer {
    fn justification(&self) -> &'static str {
        match self {
            Consumer::ReadBy(s) | Consumer::Orphan(s) | Consumer::NeverPopulated(s) => s,
        }
    }
}

/// Every field of [`DriverStmt`], and what consumes it.
const DRIVER_FIELDS: &[(&str, Consumer)] = &[
    (
        "name",
        Consumer::ReadBy(
            "executor binds it into the evaluation context; semantic.rs resolves \
         model-expression symbols against it; the console keys every panel on it",
        ),
    ),
    (
        "display_name",
        Consumer::ReadBy("render_driver_card and the driver list use it in preference to `name`"),
    ),
    (
        "description",
        Consumer::ReadBy(
            "rendered on the driver card and carried into the agent research prompt \
         by negotiate::compose_query",
        ),
    ),
    (
        "driver_type",
        Consumer::ReadBy(
            "executor selects the sampling path; plot::curve::driver_summary and \
         driver_samples branch on it",
        ),
    ),
    (
        "distribution",
        Consumer::ReadBy("executor::sample_distribution; plot::curve draws it via sample_literal"),
    ),
    (
        "probability",
        Consumer::ReadBy("executor samples binary drivers from it; driver_summary prints it"),
    ),
    (
        "impact_multiplier",
        Consumer::ReadBy(
            "executor applies it when a binary driver fires; driver_summary prints it",
        ),
    ),
    (
        "values",
        Consumer::ReadBy(
            "executor::sample_categorical; plot::curve::driver_samples draws the \
         discrete shape from it",
        ),
    ),
    (
        "weights",
        Consumer::ReadBy("executor::sample_categorical; plot::curve reproduces the declared split"),
    ),
    (
        "unit",
        Consumer::ReadBy(
            "driver_summary appends it; semantic.rs reads it when deciding whether a \
         driver is ratio-valued",
        ),
    ),
    (
        "applies_to",
        Consumer::ReadBy(
            "semantic::check_driver_spaces rejects a product mixing probability and \
         quantity ratios. NOTE the read is analysis-only: the console never sets \
         it, offers no control for it, and its FPL emitter drops it — so every \
         driver the console creates is born undeclared and triggers the warning \
         the console then displays.",
        ),
    ),
    (
        "rationale",
        Consumer::ReadBy(
            "rendered on the driver card; semantic.rs treats its absence together \
         with empty evidence_refs as an unsupported driver",
        ),
    ),
    (
        "constraints",
        Consumer::ReadBy(
            "parse_driver checks the declared distribution against them at both ends \
         (fixed in adc786bc, where the field was `let constraints = Vec::new()` \
         and not even `mut`); semantic.rs re-checks at analysis time",
        ),
    ),
    (
        "evidence_refs",
        Consumer::NeverPopulated(
            "READ by semantic.rs (unsupported-driver check) and the CLI report. \
         WRITTEN nowhere on the console path: all five construction sites pass \
         vec![], and process_agent_evidence never pushes the id of the evidence \
         it just attached. So the structural driver-to-evidence link does not \
         exist and the only association is the `{agent}_{driver}` naming \
         convention. Closing it: push ev.id onto the driver the agent was bound \
         to, at the same point the suggestion is staged.",
        ),
    ),
    (
        "learnable",
        Consumer::ReadBy(
            "executor resolves the driver to a fitted posterior instead of the static \
         prior; refit.rs decides which drivers it owns; the console renders the \
         learnable badge",
        ),
    ),
    (
        "feeds_from",
        Consumer::ReadBy(
            "refit.rs::collect_observations walks workspace_dependencies with the \
         named extractor, and auto_accept_threshold_pp overrides the impact gate",
        ),
    ),
];

/// Every field of [`BaseRate`], and what consumes it.
///
/// This is the Tetlock outside view. The operator requirement is "I should know
/// why the base rate is the base rate", and the fields that answer *why* are
/// exactly the ones with the weakest consumers.
const BASE_RATE_FIELDS: &[(&str, Consumer)] = &[
    (
        "reference_class",
        Consumer::ReadBy(
            "semantic.rs rejects an empty one; calibration::critique_base_rate runs \
         the circularity check against it; rendered in the outside-view panel \
         and the markdown report",
        ),
    ),
    (
        "historical_frequency",
        Consumer::ReadBy(
            "executor uses it as the outside-view anchor and computes divergence \
         against it; calibration::base_rate_agreement compares it to the \
         climatology the platform measured; semantic.rs range-checks it",
        ),
    ),
    (
        "sample_size",
        Consumer::ReadBy(
            "calibration::wilson_interval widths the frequency by it; rendered as \
         'n=' in the outside-view panel",
        ),
    ),
    (
        "source",
        Consumer::ReadBy(
            "semantic.rs rejects an empty one; rendered in render_outside_view and \
         the markdown report",
        ),
    ),
    (
        "reasoning",
        Consumer::ReadBy("rendered in both outside-view panels and the markdown report"),
    ),
    (
        "generated_by",
        Consumer::Orphan(
            "Parsed, REQUIRED by the parser, emitted by the FPL writer, persisted \
         into forecasts.metadata — and read by nothing that a human or a check \
         ever sees. Its only two reads are serialisation: the metadata PATCH and \
         generate_fpl_text. No render function, no report, no validation rule. \
         \
         That is load-bearing, not cosmetic. apply_base_rate_only hardcodes \
         source and generated_by to \"fermi\" even when update_outside_rate \
         routed the work to a declared specialist, and the local state.json \
         restore overwrites it with Agent(\"fermi\") on every reload. So the one \
         field that would expose a false provenance claim is the one field never \
         rendered, and the reference forecast's honest `generated_by: \
         weather_oracle` does not survive an open/close cycle. \
         \
         Closing it: render it beside `source` in render_outside_view, stop \
         hardcoding it in apply_base_rate_only, and stop overwriting it on \
         restore.",
        ),
    ),
];

/// Compile-time completeness for [`DriverStmt`].
///
/// No `..` rest pattern. Adding a field to `DriverStmt` breaks this function,
/// and the only way to fix it is to name the new field's consumer below.
fn driver_field_names(d: &DriverStmt) -> Vec<&'static str> {
    let DriverStmt {
        name: _,
        display_name: _,
        description: _,
        driver_type: _,
        distribution: _,
        probability: _,
        impact_multiplier: _,
        values: _,
        weights: _,
        unit: _,
        applies_to: _,
        rationale: _,
        constraints: _,
        evidence_refs: _,
        learnable: _,
        feeds_from: _,
    } = d;

    vec![
        "name",
        "display_name",
        "description",
        "driver_type",
        "distribution",
        "probability",
        "impact_multiplier",
        "values",
        "weights",
        "unit",
        "applies_to",
        "rationale",
        "constraints",
        "evidence_refs",
        "learnable",
        "feeds_from",
    ]
}

/// Compile-time completeness for [`BaseRate`].
fn base_rate_field_names(b: &BaseRate) -> Vec<&'static str> {
    let BaseRate {
        reference_class: _,
        historical_frequency: _,
        sample_size: _,
        source: _,
        reasoning: _,
        generated_by: _,
    } = b;

    vec![
        "reference_class",
        "historical_frequency",
        "sample_size",
        "source",
        "reasoning",
        "generated_by",
    ]
}

fn sample_driver() -> DriverStmt {
    DriverStmt {
        name: "d".into(),
        display_name: None,
        description: None,
        driver_type: fermi::ast::DriverType::Continuous,
        distribution: None,
        probability: None,
        impact_multiplier: None,
        values: None,
        weights: None,
        unit: None,
        applies_to: None,
        rationale: None,
        constraints: vec![],
        evidence_refs: vec![],
        learnable: false,
        feeds_from: None,
    }
}

fn sample_base_rate() -> BaseRate {
    BaseRate {
        reference_class: "c".into(),
        historical_frequency: 0.1,
        sample_size: None,
        source: "s".into(),
        reasoning: None,
        generated_by: GeneratedBy::Human,
    }
}

fn check(registry: &[(&str, Consumer)], declared: &[&'static str], what: &str) {
    let listed: Vec<&str> = registry.iter().map(|(f, _)| *f).collect();

    for field in declared {
        assert!(
            listed.contains(field),
            "{what}.{field} is declared and this registry does not say what reads \
             it. Add an entry naming the consumer, or an Orphan entry saying why \
             there isn't one and what closing the gap would take. Silence here is \
             how every defect in this file's header got shipped."
        );
    }
    for field in &listed {
        assert!(
            declared.contains(field),
            "{what}.{field} is in the registry but no longer on the struct — \
             delete the entry so the list cannot drift into fiction"
        );
    }
}

#[test]
fn every_driver_field_names_its_consumer() {
    check(
        DRIVER_FIELDS,
        &driver_field_names(&sample_driver()),
        "DriverStmt",
    );
}

#[test]
fn every_base_rate_field_names_its_consumer() {
    check(
        BASE_RATE_FIELDS,
        &base_rate_field_names(&sample_base_rate()),
        "BaseRate",
    );
}

/// A justification has to say something.
///
/// Without this, the registry degrades into a checkbox list: "TODO", "n/a",
/// "see code". The threshold is deliberately crude — length is a poor proxy for
/// content, and it is enough to stop the specific failure of an entry added to
/// silence the test.
#[test]
fn no_entry_is_a_checkbox() {
    for (registry, what) in [
        (DRIVER_FIELDS, "DriverStmt"),
        (BASE_RATE_FIELDS, "BaseRate"),
    ] {
        for (field, consumer) in registry {
            let j = consumer.justification();
            assert!(
                j.len() >= 40,
                "{what}.{field}: {consumer:?} is too short to be a reason. Name \
                 the function that reads it, or say what closing the gap needs."
            );
            assert!(
                !j.to_lowercase().contains("todo"),
                "{what}.{field}: a TODO is not a consumer"
            );
        }
    }
}

/// An orphan must say what closing the gap would take, not merely that it exists.
///
/// The distinction `CROSS_CHECK_EXEMPTIONS` draws: an admission with a route out
/// is a backlog item, and an admission without one is a shrug that will still be
/// here in a year.
#[test]
fn every_orphan_names_a_route_out() {
    let mut orphans = 0;
    for (registry, what) in [
        (DRIVER_FIELDS, "DriverStmt"),
        (BASE_RATE_FIELDS, "BaseRate"),
    ] {
        for (field, consumer) in registry {
            if let Consumer::Orphan(why) | Consumer::NeverPopulated(why) = consumer {
                orphans += 1;
                let lower = why.to_lowercase();
                assert!(
                    lower.contains("closing it") || lower.contains("closing the gap"),
                    "{what}.{field} admits it has no consumer but does not say what \
                     would fix it. Write 'Closing it: ...' so this is a backlog \
                     item rather than a shrug."
                );
            }
        }
    }
    assert!(
        orphans > 0,
        "no orphans recorded — either the codebase became perfect, or somebody \
         quietly reclassified one. Both deserve a look at the diff."
    );
}
