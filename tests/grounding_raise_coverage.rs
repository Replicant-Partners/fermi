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
         path already raised for the same output. Tracked as finding 8 — the \
         gap is `delegate_to_agent`, which has no gate at all.",
    ),
    (
        "src/agent_backend/tool_executor.rs",
        "enforces on a cached genome profile read inside the tool loop, where \
         no `MemoryStore` is in scope. The generating path in \
         `creatures::agent_modules` raises for the same agent and contract.",
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

/// Does this file run the grounding contract as a control?
fn enforces(body: &str) -> bool {
    code_lines(body).any(|l| l.contains("grounding_trust::enforce("))
}

/// Does it tell Loop 2 about what it found?
fn raises(body: &str) -> bool {
    code_lines(body).any(|l| l.contains("grounding_anomaly::"))
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

    // The repair must clear it, or the check fires on correct code.
    let repaired = format!(
        "{silent}    fermi::grounding_anomaly::spawn_raise(store, agent_id, None, report);\n"
    );
    assert!(raises(&repaired));

    // And prose must not satisfy it. The whole `NO_RAISE` exemption discipline
    // rests on `raises` reading code and not comments: a file whose only
    // mention of the raise is a note explaining why it does not raise would
    // otherwise excuse itself.
    assert!(!raises(
        "    // fermi::grounding_anomaly::spawn_raise would go here, but there \
         is no store in scope"
    ));
    assert!(!enforces(
        "    // grounding_trust::enforce( is discussed here and not called"
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

    assert!(
        enforcing.len() >= 7,
        "only {} file(s) appear to enforce grounding, which is fewer than the \
         audit counted (7). Either the scan stopped matching, or call sites were \
         removed — both need a look before this passes.",
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
        assert!(
            code_lines(&body).any(|l| l.contains("grounding_trust::enforce("))
                || body.contains("Gate::Grounding"),
            "{path} is exempted from raising and no longer enforces grounding. \
             Remove the entry."
        );
    }
}
