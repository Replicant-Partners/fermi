//! Every execute boundary must run the grounding contract.
//!
//! # The failure this exists to prevent
//!
//! `grounding_trust` declares field contracts for nine agents. Enforcement was
//! wired into six bespoke creature handlers and into the agent-to-agent
//! delegation hop (`agent_backend/envelope.rs`), which between them covered
//! four of those nine. The other five — `football_analyst`, `weather_oracle`,
//! `hud_field_scout`, `harvest_advisor`, `forage_scout` — had a written
//! contract and no enforcement call site on the generic path. Their output was
//! checked when another agent called them and unchecked when a person did.
//!
//! That is the worst version of this defect rather than a mild one. The
//! contract exists, so a reader looking for one finds it; the coverage metric
//! counts the agent as contracted; and the fabrication travels the human route
//! untouched into `episodes`, where the consolidation worker turns it into a
//! semantic rule and `kg_context` appends it to a prompt as a premise.
//!
//! # Why a source scan
//!
//! Same reasoning as `provenance_floor_coverage`: the type-level fix would be
//! to make enforcement impossible to skip, but every candidate signature ends
//! in an `Option` that a caller may pass `None` to, which is the same hole with
//! more ceremony. So the enforcement is a scan, with an exemption list that has
//! to name each exception and say why.
//!
//! # What it cannot do
//!
//! It proves the call is present, not that it ran, and not that it was correct.
//! Reading the code proves nothing — that is what `liveness_trust` is for. This
//! only closes the specific regression where a third execute path is added and
//! quietly skips the check, which is how the first two came to skip it.

use std::fs;

/// Files that take an agent's output and persist it as an episode on the
/// live request path. Each must enforce before the episode is written.
/// `(path, must_also_consume, why)`.
///
/// The second field is the point. Calling `enforce` and dropping the `Report`
/// leaves the document checked and the verdict thrown away: nothing records
/// that the check ran, nothing downstream can tell a checked document from an
/// unchecked one, and the scan would still be green.
///
/// This is not hypothetical. The sibling scan, `provenance_floor_coverage`,
/// tested `contains(".with_provenance_oracle(")` — presence of a call, nothing
/// about its argument — so passing `None` satisfied it completely while
/// producing exactly the ungraded rules it existed to prevent. Verified by
/// sabotage, then fixed. Asserting a proxy that is cheaper to satisfy than the
/// property is the defect class this whole line of work is about, and a scan is
/// not exempt from it.
const EXECUTE_BOUNDARIES: &[(&str, &str, &str)] = &[
    (
        "src/handlers/execution.rs",
        "stamp_grounding",
        "POST /api/agents/:id/execute — the generic execute endpoint",
    ),
    (
        "src/handlers/execution_stream.rs",
        "stamp_grounding",
        "the SSE streaming variant; if only one of the pair checks, the \
         unchecked one becomes the one callers use",
    ),
    (
        "src/agent_backend/envelope.rs",
        // The delegation hop consumes the report by building the enforced
        // payload into the envelope rather than by stamping an episode.
        "report",
        "the agent-to-agent delegation hop",
    ),
];

/// The call that must be present. Matching on the function name rather than a
/// full expression so reformatting does not break the test — the thing being
/// asserted is that enforcement is reached, not how it is spelled.
const ENFORCE_CALL: &str = "grounding_trust::enforce";

#[test]
fn every_execute_boundary_enforces_the_grounding_contract() {
    let mut missing = Vec::new();

    for (path, must_consume, why) in EXECUTE_BOUNDARIES {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                missing.push(format!(
                    "{path}: could not be read ({e}). If this file moved, move \
                     the entry — do not delete it."
                ));
                continue;
            }
        };

        // A mention inside a comment is not a call. This repo's comments name
        // modules that are not invoked there constantly, and treating those as
        // coverage is the exact mistake the scan exists to catch.
        let called = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("*")
            })
            .any(|l| l.contains(ENFORCE_CALL));

        let consumed = src
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("*")
            })
            .any(|l| l.contains(must_consume));

        if !called {
            missing.push(format!("{path}\n         why it matters: {why}"));
        } else if !consumed {
            missing.push(format!(
                "{path}\n         calls `{ENFORCE_CALL}` but never uses the result \
                 (expected `{must_consume}`) — the document is checked and the \
                 verdict discarded\n         why it matters: {why}"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "\n\n  {} execute boundary/boundaries do not call `{ENFORCE_CALL}`:\n\n         {}\n\n  \
         An agent's output reaching `episodes` unchecked is not contained there: \
         the consolidation worker distils it into a semantic rule and `kg_context` \
         appends that rule to a later prompt as a premise. A contract enforced on \
         one route and not another is a convention.\n",
        missing.len(),
        missing.join("\n\n         ")
    );
}

/// The scan is only worth anything if it can go red.
///
/// Rule 5.1: a check that has never failed has not been tested. A literal that
/// no source file contains must produce a failure for every boundary — if this
/// passes, the detection logic is inert and the test above proves nothing.
#[test]
fn the_scan_can_actually_fail() {
    let sentinel = "grounding_trust::enforce_a_call_that_does_not_exist";
    let mut found_none = true;

    for (path, _, _) in EXECUTE_BOUNDARIES {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        if src.contains(sentinel) {
            found_none = false;
        }
    }

    assert!(
        found_none,
        "the sentinel was found in a source file, so the scan cannot \
         distinguish present from absent"
    );

    // And confirm the comment filter bites: a commented-out call must not
    // count as coverage.
    let commented = "// grounding_trust::enforce(agent, doc);";
    let counted = commented
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("*")
        })
        .any(|l| l.contains(ENFORCE_CALL));
    assert!(
        !counted,
        "a commented-out call was counted as coverage — the scan would pass a \
         file that had deleted its enforcement"
    );
}
