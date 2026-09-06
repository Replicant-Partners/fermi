//! Nothing but the boundary writes an episode.
//!
//! # Why this test was rewritten
//!
//! It used to be a list. Three handlers persisted an episode, the trust
//! machinery was wired into some of them and not others, and this file named
//! the three and asserted each ran all six checks:
//!
//! | | grounding | route | queue | reserve | ledger |
//! |---|---|---|---|---|---|
//! | `execution` | yes | yes | yes | yes | yes |
//! | `execution_stream` | yes | yes | yes | **no** | **no** |
//! | `workspace::messages` | **no** | **no** | **no** | **no** | **no** |
//!
//! The consequences of that third row were all visible and none were
//! attributable: nine of ten contracted agents had never graded a field,
//! `route:` was stamped on 0 of 3,581 episodes, the verification queue held no
//! rows for want of a writer, and six of twelve delegation edges pointed at
//! parents that were never written.
//!
//! Then the list turned out to be wrong. **Fifteen** call sites persisted an
//! episode, not three. Seven of the twelve missing ones came from a genuine
//! agent invocation — `coordination_note`, `plan_solicitation`, the delegation
//! hop, the swarm coordinator, the observation analyst, the workspace
//! strategist, the rabble dispatch, the dream narrator, the eval runner — and
//! ran none of the six checks. Loop 3's own mechanisms were among the
//! ungoverned ones.
//!
//! So the weakness this test was written to fix was the weakness it had: a scan
//! is only as good as its list, and the list was hand-kept. The paper's
//! sentence applies to the test as much as to the code —
//!
//! > a contract that applies on one route and not another is not a contract,
//! > it is a convention.
//!
//! — and the remedy is not a longer list. `fermi::episode_boundary` is the one
//! place an episode is written, and this file asserts that nothing else writes
//! one. A new handler cannot forget the boundary because there is nothing else
//! to call, and this test is what keeps that true.
//!
//! # Why a source scan rather than a runtime check
//!
//! The failure is *absence*, and absence has no runtime signal — the handler
//! runs perfectly, returns a good answer, and simply records less than its
//! sibling. Nothing goes red. That is the standing-clock problem from §4.1 of
//! `docs/papers/verification_for_agent_ecologies.md`: a check with nothing
//! downstream to notice its absence needs a louder hiding place, and a test
//! that reads the source is the loudest cheap one.
//!
//! What is different now is that the scan is over a **closed population** — one
//! grep for a method name, across the whole crate — rather than over a list
//! someone has to remember to extend. `EXEMPT` is still hand-kept, but an
//! exemption is a line a human wrote on purpose; a missing entry was nobody's
//! decision at all.

use std::fs;
use std::path::{Path, PathBuf};

/// The module allowed to write an episode.
const BOUNDARY: &str = "src/episode_boundary.rs";

/// The store methods that put an episode row in the database.
///
/// Matched as substrings, so `store_episode_with_provenance` also trips
/// `store_episode`. That is deliberate: a new overload named
/// `store_episode_fast` should trip this test on the day it is written, not on
/// the day someone remembers to add it here.
const RAW_WRITES: &[&str] = &["store_episode"];

/// Files that may write an episode directly, and why.
///
/// Every entry here is a record with **no agent invocation behind it** — the
/// platform observing itself, a human's rejection, or a client's import. The
/// six checks are all checks on *an agent's answer*: there is no document to
/// enforce a contract over, no route by which an agent was reached, and no
/// claim for a human to settle. Running the boundary on these would file gate
/// rows for pulses that never happened, which is the mirror image of the defect
/// it exists to fix — a control that reports activity it did not have.
///
/// The reason is the point. "It did not apply" is precisely what was silently
/// assumed three times, and then twelve more.
const EXEMPT: &[(&str, &str)] = &[
    (
        "src/coordination_note.rs",
        "the strategist's observation about a member, written into that member's \
         memory. The member did not run: there is no document it produced, so \
         there is nothing to enforce a contract over and no claim of its own to \
         queue. Attributed to the member because that is whose dreaming \
         material it becomes.",
    ),
    (
        "src/agent_backend/simops_tools.rs",
        "a synthetic actuation plan, `Provenance::AutoPass` with the embedding \
         deliberately NULL. The plan is the platform's own arithmetic over a \
         process model, not an answer from a model, so grading it would grade \
         our own code and call the result the agent's.",
    ),
    (
        "src/handlers/consolidation.rs",
        "the dream pipeline's own record of a role it ran, `response_text: \
         None`. There is literally no document. The narrator's episode on the \
         same path DOES go through the boundary, which is the line between the \
         two: one is an agent's answer, the other is the pipeline's note that \
         it asked.",
    ),
    (
        "src/handlers/composition.rs",
        "a rejection a person made, `Provenance::HumanCorrected` at authority \
         1.0. Enforcing an agent's field contract over a human's refusal would \
         grade the human, and a gate row for it would count a refusal the gate \
         did not make.",
    ),
    (
        "src/handlers/agents.rs",
        "the client-import path, and the only caller of \
         `store_episode_with_untrusted_provenance`. The vector and its claimed \
         model identity come from outside and are stamped \
         `provenance_trusted=false` on the row for exactly that reason (Spec 22 \
         §1.6); the episode is a thing being ingested, not a thing being \
         produced here.",
    ),
];

/// Every step the boundary owes, and what goes wrong when it is missing.
///
/// This half survives from the old test unchanged in substance. What changed is
/// that it is asserted against **one** file, so the assertion is about whether
/// the boundary is whole rather than about whether fifteen call sites agree.
const STEPS: &[(&str, &str)] = &[
    (
        "reserve_episode",
        "the episode id is handed to children before its row exists, so a run \
         that fails part-way orphans everything it spawned",
    ),
    (
        "grounding_trust::enforce",
        "the agent's field contract is never applied, so a declared contract \
         grades nothing",
    ),
    (
        "graded_fields",
        "the claimed values are never collected, so nothing downstream can name \
         what the agent actually asserted",
    ),
    (
        "decided_for_episode",
        "the gate decides and writes no ledger row, so a pulse's trace shows \
         `not_recorded` for a check that ran",
    ),
    (
        "stamp_grounding",
        "the episode carries no grounding tag, so no surface can tell a checked \
         document from an unchecked one",
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
        "grounding_anomaly::spawn_raise",
        "a violation is enforced and never reported, so Loop 2 has no input and \
         `anomaly_events` stays empty while the platform is finding things",
    ),
];

/// Every `.rs` file under `src/`, with its project-relative path.
fn sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .filter_map(Result::ok);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let body = fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                out.push((path, body));
            }
        }
    }
    let mut out = Vec::new();
    walk(Path::new("src"), &mut out);
    assert!(
        out.len() > 50,
        "the scan found only {} source files, which means it is walking the \
         wrong directory and would pass no matter what the code did",
        out.len()
    );
    out
}

/// Lines that are code rather than prose.
///
/// The boundary's own comments discuss `store_episode` by name — they have to,
/// since explaining why it is deprecated is half the reason the module exists —
/// and a scan that could not tell a sentence from a call would force the
/// explanation out of the codebase to keep the test green.
fn code_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with("//") && !l.starts_with("*") && !l.starts_with("/*"))
}

#[test]
fn only_the_boundary_writes_an_episode() {
    let mut offenders: Vec<String> = Vec::new();

    for (path, body) in sources() {
        let rel = path.to_string_lossy().replace('\\', "/");
        if rel == BOUNDARY || EXEMPT.iter().any(|(p, _)| *p == rel) {
            continue;
        }
        for line in code_lines(&body) {
            if let Some(call) = RAW_WRITES.iter().find(|c| line.contains(**c)) {
                offenders.push(format!("{rel} calls `{call}` directly"));
                break;
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "an episode is being written outside the boundary:\n\n  {}\n\n\
         Route it through `fermi::episode_boundary` — `persist` if the agent \
         was invoked and answered in one breath, `Pulse::open` + \
         `persist_opened` if the episode id is minted before the invocation and \
         especially if it reaches a `ToolContext`. If no agent ran at all, add \
         the file to `EXEMPT` with the reason. A contract that applies on one \
         route and not another is not a contract, it is a convention — and the \
         list of routes was wrong by twelve the last time it was hand-kept.",
        offenders.join("\n  ")
    );
}

#[test]
fn the_boundary_runs_every_step() {
    let body = fs::read_to_string(BOUNDARY).unwrap_or_else(|e| {
        panic!("{BOUNDARY} is the whole point of this test and cannot be read: {e}")
    });

    let missing: Vec<String> = STEPS
        .iter()
        .filter(|(call, _)| !body.contains(*call))
        .map(|(call, consequence)| format!("`{call}` — {consequence}"))
        .collect();

    assert!(
        missing.is_empty(),
        "{BOUNDARY} is missing part of the boundary:\n\n  {}\n\n\
         Every path in the system now goes through this file, so a step deleted \
         here is a step deleted everywhere at once. That is the trade the \
         consolidation makes and this assertion is the other half of it.",
        missing.join("\n  ")
    );
}

/// A caller that grades contracted fields and cannot queue them.
///
/// `Write.db` is an `Option`, and `None` means: enforced, graded, stamped,
/// warned, and never queued. It compiles. `close` logs the loss by name, which
/// is better than silence and is still a log line nobody reads.
///
/// One deliberate `None` exists and is argued at its call site — the eval
/// harness, whose claims answer frozen fixture queries. A human ruling on a
/// stale fixture would be recorded as the agent having fabricated, and the
/// claimed value is retained precisely as evidence for which model fabricates
/// what, so fixture-derived verdicts poison the one question the queue exists
/// to answer.
#[test]
fn every_boundary_caller_can_reach_the_verification_queue() {
    /// Files allowed to pass `db: None`, and why. Same discipline as `EXEMPT`.
    const NO_QUEUE: &[(&str, &str)] = &[(
        "src/handlers/eval.rs",
        "fixture runs. The claims answer frozen queries, so a human verdict on \
         a stale case would be filed as the agent fabricating — and nothing \
         dedupes an `assertion_id` across runs, so a re-run of the suite queues \
         every fixture sentence again.",
    )];

    let mut silent: Vec<String> = Vec::new();

    for (path, body) in sources() {
        let rel = path.to_string_lossy().replace('\\', "/");
        if rel == BOUNDARY {
            continue;
        }
        // Only files that actually construct a `Write` are in the population.
        if !body.contains("episode_boundary::Write") {
            continue;
        }
        let declares_none = code_lines(&body).any(|l| l.contains("db: None"));
        let excused = NO_QUEUE.iter().any(|(p, _)| *p == rel);
        if declares_none && !excused {
            silent.push(format!(
                "{rel} passes `db: None` to the boundary, so every contracted \
                 field it grades is graded and never queued"
            ));
        }
        if excused && !declares_none {
            silent.push(format!(
                "{rel} is excused from the verification queue and no longer \
                 passes `db: None` — a stale excuse silently permits the next one"
            ));
        }
    }

    assert!(silent.is_empty(), "{}", silent.join("\n  "));
}

/// The contract is looked up by name, so the boundary must be handed a name.
///
/// `resolve_agent` accepts either an agent name or a UUID in the `:agent_id`
/// path segment — deliberately, since v0.10.15, so that audit tools addressing
/// an agent by its real id stop 404ing. Field contracts are declared against
/// the name. So handing the path segment to the boundary means every
/// UUID-addressed call is answered "no contract found", for every contracted
/// agent, and the failure is invisible: `grade` returns an empty report, which
/// is exactly what a genuinely uncontracted agent returns.
///
/// It was live on both general execute routes and was found twice, by having to
/// name the argument rather than by anything going red. `Absent must look
/// different from bad` is the rule it breaks — here the two are the same value.
#[test]
fn the_boundary_is_never_handed_a_path_parameter_as_an_agent_name() {
    let mut wrong: Vec<String> = Vec::new();

    for (path, body) in sources() {
        let rel = path.to_string_lossy().replace('\\', "/");
        // Only handlers that take the ambiguous segment are in the population.
        if !body.contains("Path(agent_id)") {
            continue;
        }
        for line in code_lines(&body) {
            let grades = line.contains(".grade(") || line.contains("agent_slug:");
            // `&agent_id` and nothing longer: `&agent_id_clone` is the same
            // value under another name and must trip too, while
            // `db_agent.agent_id` is a UUID field and would be a type error
            // rather than this bug.
            if grades && line.contains("&agent_id") && !line.contains("db_agent") {
                wrong.push(format!("{rel}: {}", line.trim()));
            }
        }
        // The multi-line call form, where the argument is on its own line.
        let lines: Vec<&str> = code_lines(&body).collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains(".grade(") {
                continue;
            }
            for arg in lines.iter().skip(i + 1).take(3) {
                let a = arg.trim().trim_end_matches(',');
                if a == "&agent_id" || a == "&agent_id_clone" {
                    wrong.push(format!("{rel}: grade(.., {a}, ..)"));
                }
            }
        }
    }

    assert!(
        wrong.is_empty(),
        "the boundary is being handed the `:agent_id` path segment as an agent \
         name:\n\n  {}\n\nThat segment may be a UUID. Pass \
         `db_agent.agent_name`, which is the name the contract was declared \
         against. Nothing goes red when this is wrong — the agent is simply \
         reported as having no contract, which is indistinguishable from an \
         agent that has none.",
        wrong.join("\n  ")
    );
}

/// The exemption list is only useful if it explains itself.
#[test]
fn every_exemption_carries_a_reason() {
    for (path, why) in EXEMPT {
        assert!(
            why.len() > 80,
            "{path} is exempt with no real reason. \"It did not apply\" is what \
             was silently assumed three times, and then twelve more."
        );
        assert!(
            std::path::Path::new(path).exists(),
            "{path} is exempt and is not there. A renamed or deleted file leaves \
             a permanent hole in the ban: the next writer at that path inherits \
             an exemption nobody granted it."
        );
    }
}

/// A file listed as exempt that no longer writes an episode is a hole, not a
/// tidy-up.
///
/// The exemption's cost is that it names a whole file, so once the raw write is
/// gone, any *future* episode write in that file passes unnoticed. Stated as a
/// test because the reasonable-looking response — leaving a harmless stale
/// entry — is exactly how the ban decays.
#[test]
fn every_exemption_is_still_needed() {
    let mut stale: Vec<&str> = Vec::new();
    for (path, _) in EXEMPT {
        let body = fs::read_to_string(path).unwrap_or_default();
        if !code_lines(&body).any(|l| RAW_WRITES.iter().any(|c| l.contains(c))) {
            stale.push(path);
        }
    }
    assert!(
        stale.is_empty(),
        "these files are exempt from the ban and no longer write an episode: \
         {stale:?}. Remove the entry. While it stands, the file is outside the \
         scan and the next episode write in it is invisible."
    );
}

/// The scan must be able to fail.
///
/// A source scan that cannot distinguish a call from a comment, or that reads a
/// directory it is not in, passes forever and reports safety it never checked.
#[test]
fn the_scan_can_actually_fail() {
    let real = "        .store_episode_with_provenance(episode, None, None)";
    assert!(
        code_lines(real).any(|l| RAW_WRITES.iter().any(|c| l.contains(c))),
        "the scan does not see a real raw write and would pass on any code"
    );

    let discussed = "// `store_episode` is deprecated for a reason that bites here";
    assert!(
        !code_lines(discussed).any(|l| RAW_WRITES.iter().any(|c| l.contains(c))),
        "the scan counts a comment as a call, which would force the \
         explanation out of the codebase to keep this test green"
    );

    let doc = "/// Stored through `store_episode_with_provenance`.";
    assert!(
        !code_lines(doc).any(|l| RAW_WRITES.iter().any(|c| l.contains(c))),
        "the scan counts a doc comment as a call"
    );

    // And the substring match is the part that has to hold for a method nobody
    // has written yet.
    let future = "        .store_episode_fast(episode)";
    assert!(
        code_lines(future).any(|l| RAW_WRITES.iter().any(|c| l.contains(c))),
        "a new store method would not trip the ban, which puts this test back \
         to being as good as a hand-kept list"
    );
}
