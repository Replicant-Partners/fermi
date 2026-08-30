//! The trace reads the verification log as a log, and offers one settle UI.
//!
//! # The two defects this holds shut
//!
//! `assertion_verifications` is append-only, and migration 205 says what that
//! means in the table comment itself:
//!
//! > current state is the latest row per `assertion_id`, **derived rather than
//! > stored**, so a rejected-then-reverified assertion reads as exactly that
//! > instead of as "verified".
//!
//! `/api/episodes/:id/trace` serves `routed[]` as that whole log, ordered
//! `created_at DESC`. The client filtered it flat. Measured on episode
//! `386a6248-8663-417b-8b0d-82b277a4afb1` — the one run where the curation loop
//! closed end to end, so the reference example for every surface:
//!
//! | assertion | rows | latest |
//! |---|---|---|
//! | `assessment` | 3 | `human_endorsed` |
//! | `squad_value` | 2 | `human_endorsed` |
//!
//! Five rows, two claims, **both settled**. The page rendered five rows,
//! announced "2 of 5 still awaits a verdict", and offered a settle form on each
//! of the two `pending_human_check` rows underneath the endorsements that had
//! already settled them. Three of the five rows carried no `evidence.path` —
//! settlements are written without one — so they rendered as the literal word
//! `claim` followed by the reviewer's own UUID, naming nothing.
//!
//! The count could never fall, either, because settling a claim **adds** a row.
//! The same `held` figure drove the artifact's `held for review` reading and the
//! loops block, so a closed loop was indistinguishable from a stalled one.
//!
//! Second defect, same screen. Two settle UIs existed 120 lines apart, and one
//! hand-copied its verdicts:
//!
//! | | verdicts | source |
//! |---|---|---|
//! | the claim grid | `Cite it` · `Wrong` | hardcoded, 2 of 3 |
//! | "Routed for verification" | `Sourced` · `Endorse` · `Reject` | served |
//!
//! `Cite it` **is** `Sourced` is `human_sourced`. `Wrong` **is** `Reject` is
//! `rejected`. `human_endorsed` was reachable from one block and not the other,
//! for no reason other than the copy being short — which is exactly what
//! `settleForm`'s own comment warns about, written 120 lines from where it
//! happened:
//!
//! > `settleable_verdicts` is SERVED, never hardcoded here: copying it would be
//! > copying a declaration, and inventing a parallel list is how the two drift.
//!
//! # Why a source scan
//!
//! Both failures render. Neither errors. A flat read of an append-only log
//! produces a plausible screen with confident wrong numbers on it, and a second
//! button that posts the same verdict under a different word is indistinguishable
//! from a feature. There is nothing to catch at runtime, which is the
//! standing-clock problem from §4.1 of
//! `docs/papers/verification_for_agent_ecologies.md`.

use std::fs;

const TRACE: &str = "templates/trace.html";

fn trace() -> String {
    fs::read_to_string(TRACE).unwrap_or_else(|e| panic!("cannot read {TRACE}: {e}"))
}

/// Lines that are code rather than prose.
///
/// The file discusses both defects at length by name — it has to, since the
/// reason the fold exists is the reason it is written down — and a scan that
/// could not tell a sentence from a statement would force the explanation out
/// of the codebase to stay green.
fn code_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with("//") && !l.starts_with("*") && !l.starts_with("/*"))
}

/// `routed[]` is a log. Current state is one entry per assertion, derived.
#[test]
fn the_log_is_never_filtered_flat_for_pending_rows() {
    let src = trace();
    let flat: Vec<&str> = code_lines(&src)
        .filter(|l| l.contains("routed") && l.contains("pending_"))
        .collect();

    assert!(
        flat.is_empty(),
        "the trace filters the verification log directly for pending rows:\n\n  {}\n\n\
         `routed[]` is the whole append-only history. A claim queued once and \
         settled twice keeps its `pending_` row forever, so this reports claims \
         as awaiting a verdict that a person has already settled — and the \
         figure cannot fall, because settling ADDS a row. Fold the log to one \
         entry per `assertion_id` first (the newest row is the state, the rest \
         are history) and filter that.",
        flat.join("\n  ")
    );
}

/// The fold has to actually be there, or the test above passes on a page that
/// simply stopped reporting.
#[test]
fn the_log_is_folded_to_one_entry_per_assertion() {
    let src = trace();
    for needle in [
        // the fold itself
        "CLAIMS[id] = { earlier: 0 }",
        // the path recovered from the queue row, since a settlement carries none
        "if (ev.path && !c.path) c.path = ev.path",
        // and the join the claim grid settles through
        "ASSERTION_BY_PATH[p] = id",
    ] {
        assert!(
            src.contains(needle),
            "the trace no longer folds the verification log — `{needle}` is gone. \
             Without the fold, `held` and the claim rows are counting log rows \
             rather than claims, and the numbers are confidently wrong rather \
             than missing."
        );
    }
}

/// Zero claims awaiting a verdict has two opposite causes.
///
/// Nothing was ever queued, or everything queued was settled. The loops block
/// showed neither, so the one run where curation closed read exactly like a run
/// where it never started — which is the reading that would have hidden the
/// whole point of the screen.
#[test]
fn a_settled_queue_is_distinguishable_from_an_empty_one() {
    let src = trace();
    assert!(
        code_lines(&src).any(|l| l.contains("const settled = Object.keys(CLAIMS)")),
        "the trace counts held claims and not settled ones, so an artifact whose \
         claims were all settled reports the same emptiness as one that was \
         never queued. Absent must look different from done."
    );
}

/// One act, one place, one list — and the list is served.
#[test]
fn there_is_exactly_one_settle_ui_and_it_reads_its_verdicts_from_the_platform() {
    let src = trace();

    // A verdict written as a literal into markup is a copied declaration.
    let hardcoded: Vec<&str> = code_lines(&src)
        .filter(|l| l.contains("data-verdict=\""))
        .filter(|l| !l.contains("${esc(v)}"))
        .collect();
    assert!(
        hardcoded.is_empty(),
        "a verdict is hardcoded into the trace's markup:\n\n  {}\n\n\
         The settleable verdicts are served by `/api/verification-queue`. A copy \
         here drifts from them, and the last copy was short by exactly one \
         verdict — `human_endorsed` was unreachable from the claim grid for no \
         reason but that.",
        hardcoded.join("\n  ")
    );

    // The button labels have one definition.
    assert_eq!(
        src.matches("const label = {").count(),
        1,
        "two label maps means two vocabularies for one act. `Cite it` and \
         `Sourced` were the same verdict under different words, on one screen."
    );

    // And there is one form producing them.
    assert_eq!(
        src.matches("function settleForm(").count(),
        1,
        "more than one settle form. Every settlement posts to the same endpoint \
         against the same `assertion_id`; a second form is a second rendering of \
         rows the page already shows."
    );
}

/// The scan must be able to fail.
#[test]
fn the_scan_can_actually_fail() {
    let flat =
        r#"    const held = (d.routed || []).filter(r => r.verdict.startsWith("pending_"));"#;
    assert!(
        code_lines(flat).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan does not recognise the defect it was written for"
    );

    // The file's own prose names it repeatedly and must not count.
    let prose = "// `routed[]` is the whole log, so filtering it for pending_ double-counts";
    assert!(
        !code_lines(prose).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan counts a comment as code, which would force the explanation out \
         of the file to keep this test green"
    );

    // The folded read must NOT trip it — otherwise the only way to pass is to
    // stop reporting, which is worse than the bug.
    let folded = r#"    const held = Object.keys(CLAIMS).filter(id => pending(CLAIMS[id]));"#;
    assert!(
        !code_lines(folded).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan rejects the correct implementation, so the cheapest way to go \
         green is to delete the count"
    );

    let literal = r#"      <button class="act" data-verdict="human_sourced">Cite it</button>"#;
    assert!(
        code_lines(literal).any(|l| l.contains("data-verdict=\"") && !l.contains("${esc(v)}")),
        "the scan does not see a hardcoded verdict"
    );
    let served = r#"      `<button class="act" data-verdict="${esc(v)}" title="${esc(v)}">`"#;
    assert!(
        !code_lines(served).any(|l| l.contains("data-verdict=\"") && !l.contains("${esc(v)}")),
        "the scan rejects the served form, which is the only correct one"
    );
}
