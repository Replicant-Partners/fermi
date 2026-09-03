//! Every path that enforces grounding must tell Loop 2.
//!
//! # The gap this closes
//!
//! Nine files called [`fermi::grounding_trust::enforce`]. **One** raised an
//! anomaly, and that one carried almost none of the traffic from agents that
//! have contracts:
//!
//! | agent | episodes | reached `/execute` | grounding-stamped |
//! |---|---|---|---|
//! | football_analyst | 208 | 0 | 0 |
//! | prey_locator | 93 | 0 | 0 |
//! | genome_profiler | 65 | 0 | 0 |
//! | enemy_sensor | 62 | 0 | 0 |
//! | weather_oracle | 54 | 27 | 5 |
//!
//! The creature paths *run* the control — they strip the fabricated field
//! before it renders — and then said nothing. A violation on the path where
//! violations are most likely was caught, corrected, and forgotten.
//!
//! `anomaly_events` is Loop 2's only input, so a control that corrects without
//! reporting is a loop that cannot start. This scan is why a tenth call site
//! cannot quietly join them.
//!
//! # Why file granularity
//!
//! Coarse, and deliberately so. A statement-level rule would need to know which
//! `Report` reaches which raise, and a scan that tries to follow data flow with
//! text matching is the kind of check that fires on correct code and gets
//! deleted (§5.2). File granularity catches the failure that actually happened
//! — an entire path with no raise anywhere in it — and the exemption list
//! carries the cases where that is the wrong reading, with reasons.
//!
//! The coarseness is now load-bearing rather than merely tolerable.
//! `episode_boundary` enforces in [`Pulse::grade`] and raises in [`close`],
//! because the raise has to sit below the write — `anomaly_events.episode_id`
//! is a foreign key. A statement-level or same-function rule would report the
//! one path that gets this exactly right as the only one that gets it wrong.
//!
//! # The literal is part of the check
//!
//! Three of the nine call sites collapsed into `episode_boundary`, and while
//! that was happening `envelope.rs` migrated from `enforce` to
//! `enforce_from_output_contract` — the general path, driven by the compiled
//! `grounding` map instead of by `FIELD_CONTRACTS`. It is the same control
//! doing the same stripping, and a scan matching only `enforce(` stopped seeing
//! it. That is the failure mode of every source scan and it is invisible from
//! the outside: the population shrinks, nothing goes red except the count, and
//! a file drops out of the set that has to justify not raising. Both entry
//! points are named in [`ENFORCE_CALLS`], and the falsifier below exercises
//! each of them, so the next spelling has to be added deliberately rather than
//! discovered by a file quietly leaving the scan.
//!
//! [`Pulse::grade`]: fermi::episode_boundary::Pulse::grade
//! [`close`]: fermi::episode_boundary::close

use std::path::{Path, PathBuf};

/// Files that call `enforce` and legitimately do not raise, with the reason.
/// May only shrink.
const NO_RAISE: &[(&str, &str)] = &[
    (
        "src/hud_contract.rs",
        "calls `enforce` to build a display report and has no production caller \
         at all — see finding 6. Wiring a raise into dead code would create the \
         appearance of coverage without any.",
    ),
    (
        "src/agent_backend/envelope.rs",
        "the delegation hop. It strips the payload handed to the CALLING agent \
         and returns the violations inside the envelope, where the caller sees \
         them. Raising here would double-count: the child agent's own execute \
         path reaches `episode_boundary::close` and raises for the same output. \
         Tracked as finding 8 — the gap is `delegate_to_agent`, which has no \
         gate at all. Enforces through `enforce_from_output_contract` rather \
         than `enforce`; the exemption is about the raise, not about which \
         entry point supplies the contract.",
    ),
    // `src/agent_backend/tool_executor.rs` was here, and the entry was stale.
    // It claimed the file "enforces on a cached genome profile read inside the
    // tool loop, where no `MemoryStore` is in scope" — and there is no such
    // call. Its only `enforce` is in `the_genome_profiler_fixture_is_itself_grounded`,
    // inside `#[cfg(test)]`. The production path is the one the reason already
    // named, `creatures::agent_modules`, which raises.
    //
    // Surfaced by teaching the scan to skip test modules: an exemption held in
    // place by a unit test is invisible while the scan counts unit tests, which
    // is precisely how a list that "may only shrink" quietly stops describing
    // anything.
    (
        "src/handlers/loops.rs",
        "the artifact trace re-runs the contract over a RETAINED response to \
         display a historical episode's grade. It is a GET. Raising here would \
         make `anomaly_events` a function of UI traffic: one row per page load, \
         attributed to an episode that ran weeks ago, and Loop 2's count \
         determined by how often someone opens a screen. That is the opposite of \
         what the raise is for. The finding is real and worth having — re-running \
         the contract over retained bytes surfaces 10 violations that were never \
         recorded, because the contract was not wired to those paths when the \
         episodes ran — but its home is the trace payload a reviewer reads, not \
         the exception channel. Whether a historical violation should be \
         backfilled into `anomaly_events` ONCE is a real question and a different \
         one; it needs a de-duplication key this table does not have.",
    ),
];

fn rust_sources(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some("target") | Some("node_modules") | Some(".git")
            ) {
                rust_sources(&p, out);
            }
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

fn code_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines().filter(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && !t.starts_with('*')
    })
}

/// Every way into the contract. A file reaching any of them is running the
/// control and owes Loop 2 a report.
///
/// Two entries, not one, because `enforce_from_output_contract` is the general
/// path and delegates to `enforce` only as a fallback: an agent whose contract
/// is compiled into its card never touches the legacy function. Matching one
/// spelling meant a call site could migrate to the other and leave the scan
/// without going red — which is what `envelope.rs` did.
const ENFORCE_CALLS: &[&str] = &[
    "grounding_trust::enforce(",
    "grounding_trust::enforce_from_output_contract(",
];

/// Does this file run the grounding contract as a control?
/// Everything above the file's first top-level `#[cfg(test)]`.
///
/// A unit test that runs the contract over a fixture is not an enforcement
/// path: it corrects nothing anybody reads, there is no episode to attribute an
/// anomaly to, and no `MemoryStore` in scope to raise with. Counting one made
/// this check demand a `spawn_raise` inside a `#[test]`, whose only satisfying
/// answers are a fake raise or a `NO_RAISE` entry — and an exemption list that
/// accumulates test files stops being a list of production gaps, which is the
/// only thing it is for. "An exemption without a reason is a permanent one"
/// cuts both ways.
///
/// Truncates rather than parses. `mod tests` is conventionally last in this
/// codebase, and the failure mode of being wrong is that production code below
/// a test module stops being scanned — so the floor assertion in the walk is
/// what makes this safe, and it is checked below.
fn production_only(body: &str) -> &str {
    body.find("\n#[cfg(test)]").map_or(body, |i| &body[..i + 1])
}

fn enforces(body: &str) -> bool {
    code_lines(production_only(body)).any(|l| ENFORCE_CALLS.iter().any(|c| l.contains(c)))
}

/// Does it tell Loop 2 about what it found?
fn raises(body: &str) -> bool {
    code_lines(production_only(body)).any(|l| l.contains("grounding_anomaly::"))
}

/// The detector must see an enforcing path that reports nothing.
///
/// Both predicates were inline in the walk below, so nothing in the build had
/// ever put a known-bad file in front of them. That matters more here than
/// almost anywhere: this scan's failure mode is silence, its subject is Loop
/// 2's only input, and the loop it guards has produced zero rows since it was
/// written — so a detector that could not fire and a codebase with nothing to
/// find look identical from every surface.
#[test]
fn the_scan_sees_an_enforcing_path_that_does_not_raise() {
    let silent = "\
    let mut doc = output.clone();
    let report = fermi::grounding_trust::enforce(agent_id, &mut doc);
    if !report.is_clean() {
        tracing::warn!(stripped = report.violations.len(), \"ungrounded fields\");
    }
";
    assert!(
        enforces(silent),
        "the scan cannot recognise an enforcing path"
    );
    assert!(
        !raises(silent),
        "the scan counts a path that logs and returns as one that told Loop 2 \
         — which is the exact shape of every defect the audit found"
    );

    // The general path, silent in the same way. Exercised separately because a
    // scan that recognises only the legacy spelling loses a call site the day
    // it compiles its contract — an edit that is a pure improvement to the
    // contract and a pure loss to this check, so nothing about it looks wrong.
    let silent_general = "\
    let report = crate::grounding_trust::enforce_from_output_contract(name, oc, doc);
    envelope[\"violations\"] = json!(report.violations.len());
";
    assert!(
        enforces(silent_general),
        "the scan does not recognise `enforce_from_output_contract`, so an agent \
         whose contract is compiled into its card runs the control unwatched"
    );
    assert!(!raises(silent_general));

    // Both spellings must be reachable from the constant rather than only from
    // this test's literals, or the falsifier proves a detector the scan does
    // not use.
    assert_eq!(
        ENFORCE_CALLS.len(),
        2,
        "a way into the contract was added or removed without updating the \
         falsifier, so one entry point is now scanned and untested"
    );

    // The repair must clear it, or the check fires on correct code.
    let repaired = format!(
        "{silent}    fermi::grounding_anomaly::spawn_raise(store, agent_id, None, report);\n"
    );
    assert!(raises(&repaired));

    // And the raise must count from a different function in the same file than
    // the enforcement, because that is the shape `episode_boundary` has: the
    // raise sits below the write, in `close`, and the enforcement is in
    // `grade`. A same-function rule would fail the one path that orders these
    // correctly.
    let split = "\
    pub fn grade(&self, slug: &str, doc: &mut Value) -> Report {
        grounding_trust::enforce(slug, doc)
    }

    pub async fn close(&self, report: Report) -> anyhow::Result<Uuid> {
        let stored = self.store.store_episode(ep).await?;
        crate::grounding_anomaly::spawn_raise(store, slug, Some(stored), report);
        Ok(stored)
    }
";
    assert!(
        enforces(split) && raises(split),
        "the scan is not file-granular after all: a module that enforces in one \
         function and raises below the write in another is the correct \
         arrangement and would be reported as the defect"
    );

    // And prose must not satisfy it. The whole `NO_RAISE` exemption discipline
    // rests on `raises` reading code and not comments: a file whose only
    // mention of the raise is a note explaining why it does not raise would
    // otherwise excuse itself.
    assert!(!raises(
        "    // fermi::grounding_anomaly::spawn_raise would go here, but there \
         is no store in scope"
    ));
    // A test module is not an enforcement path, and the truncation must be
    // exact about which side of the boundary a call sits on. Both halves
    // asserted: dropping the second would let `production_only` return "" and
    // silently excuse the whole repository.
    let test_only = "\
pub fn render(doc: &Value) -> String {
    format!(\"{doc}\")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_fixture_is_grounded() {
        let report = crate::grounding_trust::enforce(\"genome_profiler\", &mut doc);
        assert!(report.is_clean());
    }
}
";
    assert!(
        !enforces(test_only),
        "a unit test running the contract over a fixture reads as a production \
         enforcement path, so writing a test for the contract makes this check \
         demand a raise from inside a `#[test]`"
    );
    let prod_and_test = test_only.replace(
        "    format!(\"{doc}\")",
        "    grounding_trust::enforce(slug, doc);",
    );
    assert!(
        enforces(&prod_and_test),
        "the truncation ate production code: a file that enforces above its test \
         module must still be scanned, or every control in the repo can be \
         hidden by putting a `#[cfg(test)]` near the top"
    );

    assert!(!enforces(
        "    // grounding_trust::enforce( is discussed here and not called"
    ));
    assert!(!enforces(
        "    // enforcement is applied via `grounding_trust::enforce_from_output_contract(` upstream"
    ));
}

#[test]
fn every_path_that_enforces_grounding_also_raises() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    assert!(
        files.len() > 50,
        "the walker found {} files; a scan over an empty set passes for ever",
        files.len()
    );

    let mut enforcing = Vec::new();
    let mut silent = Vec::new();

    for path in &files {
        let rel = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if rel == "src/grounding_anomaly.rs" {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };

        if !enforces(&body) {
            continue;
        }
        enforcing.push(rel.clone());

        let excused = NO_RAISE.iter().any(|(p, _)| *p == rel);
        if !raises(&body) && !excused {
            silent.push(rel);
        }
    }

    // The floor is what caught `envelope.rs` dropping out of the scan when it
    // migrated spelling, so it is the one assertion here that has ever fired
    // for a reason nobody predicted. It survives the boundary consolidation
    // unchanged and that is not a coincidence: three handler call sites became
    // one in `episode_boundary`, and `envelope.rs` came back once both entry
    // points were named, so the count is seven on either side of the edit. If
    // it falls, either a control was deleted or a spelling escaped
    // `ENFORCE_CALLS`, and the second is the one that looks like nothing.
    // Seven until the scan learned to skip `#[cfg(test)]`. Six of the seven are
    // real production paths; the seventh was `tool_executor.rs`, counted only
    // for an `enforce` call inside a unit test. Lowered deliberately and in the
    // same edit that removed its exemption — the floor is meant to catch a
    // control going missing, and nothing went missing here.
    assert!(
        enforcing.len() >= 6,
        "only {} file(s) appear to enforce grounding, which is fewer than the \
         audit counted (6). Either the scan stopped matching, or call sites were \
         removed — both need a look before this passes. A call site that changed \
         which `grounding_trust` entry point it uses reads as the second and is \
         the first: see `ENFORCE_CALLS`.",
        enforcing.len()
    );
    println!(
        "  {} file(s) enforce grounding; {} excused from raising",
        enforcing.len(),
        NO_RAISE.len()
    );

    assert!(
        silent.is_empty(),
        "\n{} path(s) enforce the grounding contract and tell Loop 2 nothing:\n  {}\n\n\
         `anomaly_events` is Loop 2's only input, so a control that corrects \
         without reporting is a loop that cannot start. Call \
         `fermi::grounding_anomaly::spawn_raise(store, agent, persisted_episode_id, report)`, \
         or add the file to NO_RAISE with a reason.\n",
        silent.len(),
        silent.join("\n  ")
    );
}

/// An exemption must name a real file and give a reason. The list may only
/// shrink.
#[test]
fn every_no_raise_exemption_is_real_and_reasoned() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (path, why) in NO_RAISE {
        assert!(
            why.len() > 80,
            "{path}: an exemption without a reason is a permanent one"
        );
        let body = std::fs::read_to_string(repo.join(path))
            .unwrap_or_else(|e| panic!("{path} is exempted and unreadable: {e}"));
        // A file that has stopped enforcing does not need an exemption, and a
        // stale one hides the next file that takes its place.
        //
        // Through the same predicate the walk uses, so an entry point the walk
        // recognises and this check does not cannot report a live exemption as
        // dead. That mismatch is not hypothetical: this assertion was the only
        // thing that went red when `envelope.rs` migrated to
        // `enforce_from_output_contract`, and it went red saying the file had
        // stopped enforcing, which was false.
        assert!(
            enforces(&body) || body.contains("Gate::Grounding"),
            "{path} is exempted from raising and no longer enforces grounding. \
             Remove the entry."
        );
    }
}
