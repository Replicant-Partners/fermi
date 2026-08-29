//! Every path that persists an episode must run the same boundary.
//!
//! # Why this test exists
//!
//! Three handlers persist an episode, and the trust machinery was wired into
//! some of them and not others — **three separate times, discovered one at a
//! time, each by reading a screen that looked wrong**:
//!
//! | | grounding | route | queue | reserve | ledger |
//! |---|---|---|---|---|---|
//! | `execution` | yes | yes | yes | yes | yes |
//! | `execution_stream` | yes | yes | yes | **no** | **no** |
//! | `workspace::messages` | **no** | **no** | **no** | **no** | **no** |
//!
//! The workspace path is where multi-agent work actually happens, and it ran
//! none of it. The consequences were all visible and none were attributable:
//! nine of ten contracted agents had never graded a field, the `route:` tag was
//! stamped zero times in 3,581 episodes, the verification queue was empty so
//! curation had nothing to attach a verdict to, and six of twelve delegation
//! edges pointed at parents that were never written.
//!
//! `docs/papers/verification_for_agent_ecologies.md` names the class:
//!
//! > a contract that applies on one route and not another is not a contract,
//! > it is a convention.
//!
//! It was written about `/execute` versus the delegation hop. There was a third
//! door, and then a fourth thing missing from the second door. A convention is
//! exactly what this was, and a scan is what turns it back into a contract.
//!
//! # Why a source scan rather than a runtime check
//!
//! The failure is *absence*, and absence has no runtime signal — the handler
//! runs perfectly, returns a good answer, and simply records less than its
//! sibling. Nothing goes red. That is the standing-clock problem from §4.1: a
//! check with nothing downstream to notice its absence needs a louder hiding
//! place, and a test that reads the source is the loudest cheap one.
//!
//! Precedent: `tests/episode_lineage_coverage.rs` scans call sites for the same
//! reason.
//!
//! # Adding a path
//!
//! If you add a handler that persists an episode, add it here. If a boundary
//! call genuinely does not apply to it, add it to `EXEMPT` **with a reason** —
//! the reason is the point, because "it did not apply" is precisely what was
//! silently assumed three times.

use std::fs;

/// Every handler that persists an episode of its own.
const PATHS: &[&str] = &[
    "src/handlers/execution.rs",
    "src/handlers/execution_stream.rs",
    "src/handlers/workspace/messages.rs",
];

/// What each of them owes, and what goes wrong when it is missing.
const BOUNDARY: &[(&str, &str)] = &[
    (
        "grounding_trust::enforce",
        "the agent's field contract is never applied, so a declared contract \
         grades nothing on this route",
    ),
    (
        "stamp_grounding",
        "the episode carries no grounding tag, so no surface can tell a checked \
         document from an unchecked one",
    ),
    (
        "decided_for_episode",
        "the gate decides and writes no ledger row, so the artifact's belt shows \
         `not_recorded` for a check that ran",
    ),
    (
        "route_trust::stamp",
        "`route:` is never written, so `route_outcomes`, `domain_agent_ranking` \
         and `declaration_quality_outcomes` stay empty and Loop 4 cannot turn",
    ),
    (
        "verification_queue::enqueue",
        "no claim is ever queued, so a human has nothing to attach a verdict to \
         and curation cannot start",
    ),
    (
        "reserve_episode",
        "the episode id is handed to children before its row exists, so a run \
         that fails part-way orphans everything it spawned",
    ),
];

/// Deliberate omissions. A path here must say why, in prose, on this line.
const EXEMPT: &[(&str, &str, &str)] = &[];

#[test]
fn every_execute_path_runs_the_whole_boundary() {
    let mut missing: Vec<String> = Vec::new();

    for path in PATHS {
        let src = fs::read_to_string(path).unwrap_or_else(|e| {
            panic!("{path} is listed as an execute path and cannot be read: {e}")
        });

        for (call, consequence) in BOUNDARY {
            if src.contains(call) {
                continue;
            }
            if EXEMPT.iter().any(|(p, c, _why)| p == path && c == call) {
                continue;
            }
            missing.push(format!("{path} never calls `{call}` — {consequence}"));
        }
    }

    assert!(
        missing.is_empty(),
        "an execute path is missing part of the boundary:\n\n  {}\n\n\
         A contract that applies on one route and not another is not a contract, \
         it is a convention. If the omission is deliberate, add it to `EXEMPT` \
         with the reason rather than deleting the assertion.",
        missing.join("\n  ")
    );
}

/// The exemption list is only useful if it explains itself.
#[test]
fn every_exemption_carries_a_reason() {
    for (path, call, why) in EXEMPT {
        assert!(
            why.len() > 30,
            "{path} is exempt from `{call}` with no real reason. \
             \"It did not apply\" is what was silently assumed three times."
        );
    }
}

/// The scan is worthless if it is pointed at files that do not exist.
#[test]
fn the_listed_paths_exist() {
    for path in PATHS {
        assert!(
            std::path::Path::new(path).exists(),
            "{path} is listed as an execute path and is not there. A renamed \
             handler silently empties this test."
        );
    }
}
