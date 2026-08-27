//! # Every execute route checks, or the unchecked one becomes the one callers use
//!
//! That sentence is already in `handlers/execution_stream.rs`, written about
//! input binding and grounding. The schema was the third check it did not
//! cover, and the asymmetry it left was the mirror image of the one
//! `envelope.rs` was written to fix: there the composition path was
//! unprotected, and after the envelope landed it was the *public* path that
//! was, because `execute_agent_handler` never read `output_contract` at all.
//!
//! ## Why this is a source scan
//!
//! The property is "three routes all do a thing", which is cross-file and
//! cannot be reached from a unit test: two of the routes are axum handlers
//! needing an `AppState`, a database, credits and a live model. Asserting it
//! over the source is the established pattern here —
//! `tests/gate_trust_coverage.rs` scans for gate call sites for exactly this
//! reason, and it is how that file caught a real gap in `decided_about`.
//!
//! A source scan is a weaker instrument than a behavioural test and is chosen
//! knowingly. What it can do is stop the routes silently diverging, which is
//! the failure that actually happens: someone adds a fourth execute path, or
//! deletes the check from one of three, and every existing test still passes.

use std::path::Path;

/// The routes that hand an agent's output back to a caller, and the files
/// that together implement each.
///
/// A route is a file *set*, not a file: the delegation route builds the
/// envelope in `envelope.rs` and records the conformance signal at its call
/// site in `tools_legacy.rs`. Modelling it as one file was the first version
/// of this test and it failed — correctly reporting that `envelope.rs` writes
/// no signal, which is true and not the point. The property is per route.
///
/// Named individually rather than globbed, so adding a fourth execute path is
/// a deliberate edit here rather than something a glob quietly absorbs or
/// quietly misses.
const EXECUTE_ROUTES: &[(&str, &[&str])] = &[
    (
        "agent-to-agent delegation (envelope::build)",
        &[
            "src/agent_backend/envelope.rs",
            "src/agent_backend/tools_legacy.rs",
        ],
    ),
    (
        "POST /api/agents/:id/execute",
        &["src/handlers/execution.rs"],
    ),
    (
        "POST /api/agents/:id/execute/stream",
        &["src/handlers/execution_stream.rs"],
    ),
];

/// Does any file implementing this route contain `needle`?
fn route_has(files: &[&str], needle: &str) -> bool {
    files.iter().any(|f| read(f).contains(needle))
}

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

// ─── the parity ────────────────────────────────────────────────────────

/// Every route validates the document against the type its producer declared.
#[test]
fn every_execute_route_validates_against_the_declared_schema() {
    for (what, files) in EXECUTE_ROUTES {
        assert!(
            route_has(files, "schema_validate::validate"),
            "{what} does not validate against the declared schema. A contract \
             enforced on some routes and not others is not a contract, it is a \
             convention — and callers migrate to whichever route does not \
             check. Files: {files:?}"
        );
    }
}

/// Every route reports to the gate, so a refusal on any of them is counted.
#[test]
fn every_execute_route_reports_to_the_output_schema_gate() {
    for (what, files) in EXECUTE_ROUTES {
        assert!(
            route_has(files, "Gate::OutputSchema"),
            "{what} validates and tells nobody. `gate_trust`'s own premise is \
             that a refusal nobody counted is the state it exists to make \
             impossible. Files: {files:?}"
        );
    }
}

/// **The one that stops drift.** No route may hand-roll the status→decision
/// mapping; all three call `envelope::decision_for`.
///
/// Three inline `match` statements would agree today and diverge the first
/// time anyone adds an `unverified_*` variant — and the divergence would be
/// silent, because each site would still compile and still report *something*.
/// The dangerous direction is specific: an inline match whose catch-all was
/// written as `_ => Approved` turns the majority case (no schema declared,
/// 90 of 101 cards) into a pass.
#[test]
fn no_route_hand_rolls_the_decision_mapping() {
    // The owner of the mapping defines it rather than calling it, which is the
    // one legitimate exception.
    assert!(
        read("src/agent_backend/envelope.rs").contains("pub fn decision_for"),
        "envelope.rs is supposed to own the status -> decision mapping"
    );
    for (what, files) in EXECUTE_ROUTES {
        if files.contains(&"src/agent_backend/envelope.rs") {
            continue;
        }
        assert!(
            route_has(files, "envelope::decision_for"),
            "{what} does not use `envelope::decision_for`. If it maps statuses \
             to decisions itself, the two definitions agree until someone adds \
             a status — and an inline `_ => Approved` would turn \
             `unverified_no_schema`, the majority case, into a pass. \
             Files: {files:?}"
        );
    }
}

/// Every route feeds the trend, so `loop4.conformed` counts real traffic and
/// not only the delegation hops that happen to be typed on both ends.
#[test]
fn every_execute_route_feeds_the_conformance_trend() {
    for (what, files) in EXECUTE_ROUTES {
        assert!(
            route_has(files, "schema_conformance::"),
            "{what} does not record a conformance signal, so loop4 \
             under-reports by however much traffic uses this route. \
             Files: {files:?}"
        );
    }
}

// ─── the ordering that makes the check fair ────────────────────────────

/// Grounding runs before validation on both HTTP routes.
///
/// The rule is stated in `envelope::build`: enforce, then verify what remains.
/// A schema pinning an unsourceable field to `null` would otherwise reject a
/// document grounding was about to clean, and the agent would be blamed for
/// something the platform then fixed.
///
/// Checked by position because that is what "ordering" means, and because
/// getting it backwards produces a plausible-looking `invalid` verdict rather
/// than an error anyone would notice.
#[test]
fn grounding_is_enforced_before_the_schema_is_checked() {
    for file in [
        "src/handlers/execution.rs",
        "src/handlers/execution_stream.rs",
    ] {
        let src = read(file);
        let enforce = src
            .find("grounding_trust::enforce")
            .unwrap_or_else(|| panic!("{file} does not enforce grounding at all"));
        let validate = src
            .find("schema_validate::validate")
            .unwrap_or_else(|| panic!("{file} does not validate"));
        assert!(
            enforce < validate,
            "{file} validates before enforcing. A field grounding is about to \
             null would be reported as contradicting the schema, blaming the \
             agent for something the platform then fixes."
        );
    }
}

// ─── the status vocabulary is one vocabulary ───────────────────────────

/// The `unverified_*` tokens are identical across routes.
///
/// They are strings crossing an API boundary — `envelope.validation.status` on
/// one route and the `validation.status` field of the execute response on
/// another — so a consumer switching routes must not meet a different
/// spelling. This is the `gbif_verified` / `tool_verified` class of bug: two
/// sites naming what should be one value.
#[test]
fn the_status_vocabulary_is_shared_across_routes() {
    const STATUSES: &[&str] = &[
        "unverified_no_schema",
        "unverified_no_payload",
        "unverified_unsupported_schema",
    ];
    for (what, files) in EXECUTE_ROUTES {
        for status in STATUSES {
            assert!(
                route_has(files, status),
                "{what} never emits `{status}`, so a caller cannot tell the \
                 three kinds of `not checked` apart on this route while it can \
                 on the others. Files: {files:?}"
            );
        }
    }
}

/// And no route emits a fourth spelling of "fine".
///
/// The words that would read as a pass without being one. `valid` is the
/// single permitted positive token; anything else means someone invented a
/// synonym, and a synonym for `valid` is how `unverified` becomes `verified`
/// in a consumer's head.
#[test]
fn no_route_invents_a_synonym_for_valid() {
    for (what, files) in EXECUTE_ROUTES {
        for bad in [
            "\"schema_ok\"",
            "\"conforms\"",
            "\"passed\"",
            "\"verified\"",
            "\"schema_valid\"",
        ] {
            assert!(
                !route_has(files, bad),
                "{what} emits {bad}, a second word for `valid`. One positive \
                 token, or a consumer has to know which routes use which. \
                 Files: {files:?}"
            );
        }
    }
}

// ─── the public response carries it ────────────────────────────────────

/// A validation performed and not reported is a check the caller cannot act
/// on. The delegation route puts it in `envelope.validation`; the POST route
/// must put it in the response body, or a script calling the API has strictly
/// less information than an agent calling the same agent.
#[test]
fn the_public_response_reports_the_verdict_to_the_caller() {
    let src = read("src/handlers/execution.rs");
    assert!(
        src.contains("\"validation\""),
        "the execute response does not carry a `validation` object, so a \
         caller is told nothing about whether the document it just received \
         matched the type the agent declared"
    );
}
