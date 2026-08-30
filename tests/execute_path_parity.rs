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
///
/// The cost of that choice has since been measured on the sibling scan.
/// `execute_boundary_parity.rs` named three files too, and the real number of
/// writers that persist an episode turned out to be fifteen, seven of them from
/// a genuine agent invocation — so twelve paths were outside a list whose whole
/// purpose was to have no outside. A hand-kept list is only sound where the
/// population is genuinely closed. It is closed here, because a route is a
/// thing the router exposes and adding one means editing `api_server.rs`; it
/// was not closed there, and the remedy was not a longer list but one function
/// — `episode_boundary` — and a ban on the raw write.
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

/// The source with comments dropped.
///
/// Needed by the data-flow check below and by nothing above it: those look for
/// call sites that no comment in this repository happens to spell, whereas
/// `graded.enforced` and `graded.claimed` are both named in the prose that
/// explains which one to use — so a file could satisfy the check by describing
/// it.
fn code(src: &str) -> String {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with('*')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `n` code lines leading up to and including each occurrence of `needle`.
///
/// A window rather than the whole file, because the question is which document
/// this particular call was handed. A file-wide `contains("graded.enforced")`
/// would pass a handler that bound the enforced document, used it for something
/// else, and validated a different one.
fn leading_up_to(src: &str, needle: &str, n: usize) -> String {
    let code = code(src);
    let lines: Vec<&str> = code.lines().collect();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if l.contains(needle) {
            for w in lines.iter().take(i + 1).skip(i.saturating_sub(n)) {
                out.push_str(w);
                out.push('\n');
            }
        }
    }
    out
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

/// The two HTTP routes, whose relationship to grounding is now a data
/// dependency rather than a line order.
const GROUNDED_ROUTES: &[&str] = &[
    "src/handlers/execution.rs",
    "src/handlers/execution_stream.rs",
];

/// A second document, extracted a second time from the same response. Whatever
/// the intent, its fields have not been through the contract.
const SECOND_EXTRACTION: &[&str] = &["extract_json", "envelope::build"];

/// Grounding runs before validation on both HTTP routes.
///
/// The rule is stated in `envelope::build`: enforce, then verify what remains.
/// A schema pinning an unsourceable field to `null` would otherwise reject a
/// document grounding was about to clean, and the agent would be blamed for
/// something the platform then fixed.
///
/// This was checked by position, on the grounds that position is what
/// "ordering" means. It no longer is. Enforcement moved into
/// `episode_boundary::Pulse::grade` and the enforced document comes back as
/// `Graded::enforced`, so a handler cannot validate before enforcing: the
/// document it validates does not exist until enforcement has produced it. The
/// ordering is held by the type.
///
/// What that leaves open is which document. `Graded` carries two — `claimed`,
/// kept because it is the evidence for every later verification, and `enforced`
/// — and they differ in exactly the fields grounding nulled. Validating
/// `claimed` reinstates the original defect with the original consequence and no
/// line-ordering mistake to see: the schema rejects a field the platform had
/// already removed, `invalid` is reported, and the agent is blamed for something
/// the platform fixed. So the assertion is about the argument, not the order.
#[test]
fn the_schema_is_checked_against_the_enforced_document() {
    for file in GROUNDED_ROUTES {
        let src = read(file);
        assert!(
            code(&src).contains(".grade("),
            "{file} never grades, so nothing on this route applies the field \
             contract and there is no enforced document for the schema to be \
             checked against"
        );

        let at_validation = leading_up_to(&src, "schema_validate::validate", 12);
        assert!(!at_validation.is_empty(), "{file} does not validate");
        assert!(
            at_validation.contains("graded.enforced"),
            "{file} validates a document that did not come from \
             `graded.enforced`. The schema then sees fields grounding had \
             already nulled, reports `invalid`, and the agent is blamed for \
             something the platform fixed — which is the failure the old \
             enforce-before-validate ordering existed to prevent, reachable \
             again now that the ordering is held by the type instead."
        );
        assert!(
            !at_validation.contains("graded.claimed"),
            "{file} has the pre-enforcement document in hand where it validates. \
             `claimed` exists to be retained as evidence, not to be checked \
             against a schema; the two differ in precisely the fields the \
             contract removed."
        );

        for second in SECOND_EXTRACTION {
            assert!(
                !code(&src).contains(second),
                "{file} reaches `{second}`, which parses the raw response a \
                 second time. Two documents from one response can disagree, \
                 nothing says which one the verdict describes, and the one that \
                 has not been through the contract is the one that is cheaper to \
                 reach for."
            );
        }
    }
}

/// `Graded::enforced` is a document enforcement produced, not a name for the
/// one it was given.
///
/// The test above trusts the boundary for the whole of its meaning: it asserts
/// which field is validated and cannot see whether that field differs from
/// `claimed`. If `enforce` ran on the wrong copy — or if the copy that becomes
/// `enforced` were taken afterwards — both fields would hold the same document
/// and every route would validate an unchecked one while passing.
///
/// Still positional, because in this one function it still is an ordering, and
/// getting it backwards is silent in both directions: the schema sees an
/// unenforced document, and `claimed` records the nulls the platform wrote
/// rather than what the agent said, so the evidence for every later
/// verification becomes a record of the agent having claimed nothing.
#[test]
fn the_boundary_enforces_before_it_hands_a_document_back() {
    let src = code(&read("src/episode_boundary.rs"));
    let copy = src
        .find("let mut enforced")
        .expect("episode_boundary no longer takes a copy to enforce on");
    let enforce = src
        .find("grounding_trust::enforce")
        .expect("episode_boundary no longer enforces the field contract");
    assert!(
        copy < enforce,
        "the document handed back as `enforced` is copied after enforcement ran, \
         so `claimed` and `enforced` are the same bytes: every route validates an \
         unchecked document, and the claims retained as evidence are the nulls \
         the platform wrote."
    );
}

/// The data-flow check can go red.
///
/// Its predecessor was two `find` calls compared with `<`, which is
/// self-evidently able to fail. This one is a windowed match on a field name and
/// is not, so it gets a falsifier: the failure mode of a scan like this is
/// passing everything, and a route validating the wrong document looks exactly
/// like a route validating the right one from every surface except this test.
#[test]
fn the_enforced_document_check_can_actually_fail() {
    let wrong = "\
    let graded = pulse.grade(&agent_id, output.raw_response.as_deref());
    let doc = graded.claimed.as_ref();
    let r = fermi::schema_validate::validate(schema, doc);
";
    let window = leading_up_to(wrong, "schema_validate::validate", 12);
    assert!(
        window.contains("graded.claimed") && !window.contains("graded.enforced"),
        "a route validating the pre-enforcement document reads as correct, so \
         the check passes whatever it is given"
    );

    let right = "\
    let graded = pulse.grade(&agent_id, output.raw_response.as_deref());
    let doc = graded.enforced.as_ref();
    let r = fermi::schema_validate::validate(schema, doc);
";
    let window = leading_up_to(right, "schema_validate::validate", 12);
    assert!(
        window.contains("graded.enforced") && !window.contains("graded.claimed"),
        "the check fires on a correct route, which is how a check gets deleted"
    );

    // And prose must not satisfy it. Both handlers explain at length which
    // document to validate and why, so the commented form of the right answer
    // is present on every route that could get it wrong.
    let described = "\
    // Validated against `graded.enforced`, never `graded.claimed`.
    let r = fermi::schema_validate::validate(schema, doc);
";
    assert!(
        !leading_up_to(described, "schema_validate::validate", 12).contains("graded.enforced"),
        "a comment naming the enforced document counted as validating against \
         it, so a route could satisfy this test by describing what it does not do"
    );

    // The window must not reach past the call it is about.
    let distant = format!(
        "    let doc = graded.enforced.as_ref();\n{}    let r = fermi::schema_validate::validate(schema, other);\n",
        "    let _ = 0;\n".repeat(20)
    );
    assert!(
        !leading_up_to(&distant, "schema_validate::validate", 12).contains("graded.enforced"),
        "the window is unbounded, so an enforced document bound anywhere in a \
         seven-hundred-line handler counts as the one that was validated"
    );
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
