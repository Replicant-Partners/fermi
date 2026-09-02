//! Every declared sink must actually be instrumented, and the uninstrumented
//! swallows may only decrease.
//!
//! # Why a scan and not a review
//!
//! `verification_for_agent_ecologies.md` §4.1: a check with nothing waiting on
//! it is indistinguishable from a check that has stopped.
//! [`fermi::write_accounting`] is on the standing clock — nothing stalls if it
//! is never called — so without something that fails the build, the module
//! would be adopted at the sites written the day it landed and nowhere after.
//! That is precisely how `hud_contract::enforce` came to be a thousand lines of
//! safety gate with no production caller.
//!
//! Two checks, doing different jobs:
//!
//! * [`every_declared_sink_is_instrumented_somewhere`] is exact. A `Sink`
//!   variant with no call site is a counter that will read zero for ever, and a
//!   zero from an uninstrumented sink is the same lie as an empty table from an
//!   unscheduled writer.
//! * [`uninstrumented_swallows_may_only_decrease`] is a **ratchet**, not a
//!   clean-tree assertion. An audit found roughly thirty swallowed writes across
//!   the loop sinks; this pass instruments the loop-critical ones. Asserting
//!   zero today would mean either a false claim or a suppression list, and §5.2
//!   is clear about what happens to a check that fires on correct behaviour.
//!   A baseline that may only shrink is the honest instrument, and it is what
//!   §4.1 calls the burn-down ratchet.
//!
//! # What the ratchet can and cannot see
//!
//! It matches **raw SQL writes** against a declared sink table — `INSERT INTO
//! t`, `UPDATE t` — in a swallowed statement. It does **not** see writes that go
//! through a store method (`create_anomaly_event`, `store_episode_with_provenance`),
//! because the table name is not at the call site. Stated plainly rather than
//! left to be discovered: the exact half is
//! [`every_declared_sink_is_instrumented_somewhere`], and this one is a floor.

use std::path::{Path, PathBuf};

use fermi::write_accounting::SINKS;

/// Files whose swallowed writes are counted by the ratchet, per sink table.
///
/// Set from a measured run, not from a target. Lower an entry when you
/// instrument a site; the test tells you when one is stale.
/// Measured after the first instrumentation pass, not guessed. The first run of
/// this test rejected a guessed baseline of 6/2/1 as too generous and named the
/// real figures — which is the ratchet doing its job before anyone relied on it.
/// `episodes` 2 → 1. Three sites instrumented after the ratchet caught them at
/// 4: the workspace attribution in `episode_boundary` (logged with
/// `tracing::error!` and counted nowhere, on the very column added to stop
/// pulses being invisible), the episode-history copy in `workflows::fork`
/// (`.ok()`, so a fork whose history did not copy still reported success), and
/// the outcome annotation in `handlers::forecasts` (`let _ =`, which silently
/// drops an episode out of every future calibration).
///
/// The 1 that remains is a live-database probe in `episode_boundary`'s own
/// tests, whose `UPDATE … AND false` writes nothing by construction.
const BASELINE: &[(&str, usize)] = &[("episodes", 1), ("semantic_rules", 1)];

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

/// Does this line start a write against `table`?
fn writes_table(line: &str, table: &str) -> bool {
    let l = line.to_ascii_lowercase();
    let t = table.to_ascii_lowercase();
    [
        format!("insert into {t}"),
        format!("insert into public.{t}"),
        format!("update {t}"),
        format!("update public.{t}"),
        format!("delete from {t}"),
    ]
    .iter()
    .any(|needle| l.contains(needle.as_str()))
}

/// Is the failure of the statement around `idx` swallowed?
///
/// The shapes the audit found, looked for in the statement's own neighbourhood
/// rather than the whole function, so an unrelated `let _ =` twenty lines away
/// does not count.
///
/// The first version of this function missed **log-and-continue**
/// (`if let Err(e) = … { tracing::warn!(…) }`), which is by a wide margin the
/// commonest shape in this codebase and the one behind every defect the audit
/// found. A scan that cannot see the dominant case is a scan that reports a
/// clean tree — so it is listed first here, and
/// `the_scan_sees_the_shape_that_caused_every_finding` holds it.
fn swallowed_near(lines: &[&str], idx: usize) -> bool {
    let lo = idx.saturating_sub(4);
    let hi = (idx + 30).min(lines.len());
    let window: Vec<&str> = lines[lo..hi].iter().map(|l| l.trim()).collect();

    // log-and-continue
    let logged = window
        .iter()
        .any(|t| t.starts_with("if let Err(") || t.contains("Err(e) =>"))
        && window.iter().any(|t| {
            t.contains("tracing::warn!") || t.contains("tracing::error!") || t.contains("eprintln!")
        });

    logged
        || window.iter().any(|t| {
            t.starts_with("let _ =")
                || t.contains(".ok();")
                || t.contains(".ok()?;")
                || t.contains(".ok().flatten()")
                || t.contains("unwrap_or(")
        })
}

/// Is the statement around `idx` already accounted for?
fn accounted_near(lines: &[&str], idx: usize) -> bool {
    let lo = idx.saturating_sub(30);
    let hi = (idx + 40).min(lines.len());
    lines[lo..hi].iter().any(|l| l.contains("write_accounting"))
}

/// A `Sink` with no call site is a counter that reads zero for ever.
///
/// Exact, and the more important of the two. The failure it prevents is the one
/// the whole audit is about: a declaration that looks like coverage and is not.
#[test]
fn every_declared_sink_is_instrumented_somewhere() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    assert!(
        files.len() > 50,
        "the walker found {} files; a scan over an empty set passes for ever",
        files.len()
    );

    // Only genuine CALL sites count.
    //
    // The first version searched whole files for `Sink::X`, and passed while
    // `kg_context`'s instrumentation was deliberately removed — because
    // `liveness_trust` mentions every variant in its `accounted:` field. That is
    // a declaration, not a writer, and accepting it made this scan assert
    // something cheaper than the property it claims: exactly the proxy
    // assertion the module it guards was written to catch.
    //
    // So a variant only counts when it appears within a few lines of an
    // `observe(` or `record(` call, and the declaration site is excluded
    // outright.
    let call_sites: Vec<String> = files
        .iter()
        .filter(|p| !p.ends_with("write_accounting.rs") && !p.ends_with("liveness_trust.rs"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|body| {
            let lines: Vec<String> = body.lines().map(str::to_string).collect();
            let mut found = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("Sink::") {
                    continue;
                }
                let lo = i.saturating_sub(3);
                let hi = (i + 3).min(lines.len());
                let near_call = lines[lo..hi]
                    .iter()
                    .any(|l| l.contains("observe(") || l.contains("record("));
                if near_call {
                    found.push(line.clone());
                }
            }
            found
        })
        .collect();

    let mut uninstrumented: Vec<String> = Vec::new();
    for spec in SINKS {
        // `Sink::AnomalyEvents` etc. — the variant name as written at a call
        // site. Derived from the table so the two cannot drift apart.
        let variant = spec
            .table
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect::<String>();
        let needle = format!("Sink::{variant}");
        if !call_sites.iter().any(|l| l.contains(&needle)) {
            uninstrumented.push(format!("{} (looked for `{needle}`)", spec.table));
        }
    }

    assert!(
        uninstrumented.is_empty(),
        "\n{} declared sink(s) have no call site:\n  {}\n\n\
         A `Sink` variant nobody calls is a counter that reads zero for ever, \
         and zero from an uninstrumented sink is indistinguishable from zero \
         from a healthy one. Either instrument the writer or remove the \
         variant.\n",
        uninstrumented.len(),
        uninstrumented.join("\n  ")
    );
}

/// The detector must see the shape that caused every finding in the audit.
///
/// `swallowed_near`'s first version matched `let _ =` and `.ok()` and not
/// `if let Err(e) = … { tracing::warn!(…) }`. Every defect the audit turned up
/// — the rejected severity, the foreign-key race, the unbound coherence error —
/// was in the shape it could not see, so the ratchet would have reported a
/// smaller number than the truth and been believed.
#[test]
fn the_scan_sees_the_shape_that_caused_every_finding() {
    // The literal shape of `execution.rs`'s grounding anomaly, before it was
    // instrumented. If the detector cannot see this, the ratchet is decoration.
    let log_and_continue: Vec<&str> = vec![
        "        let res = sqlx::query(\"INSERT INTO anomaly_events (kind) VALUES ($1)\")",
        "            .execute(pool)",
        "            .await;",
        "        if let Err(e) = res {",
        "            tracing::warn!(error = %e, \"failed to record grounding anomaly\");",
        "        }",
    ];
    assert!(
        swallowed_near(&log_and_continue, 0),
        "the detector cannot see log-and-continue, which is the shape behind \
         every defect the audit found"
    );

    // And it must not call a propagating write swallowed, or the ratchet counts
    // correct code and gets deleted for crying wolf (§5.2).
    let propagates: Vec<&str> = vec![
        "        sqlx::query(\"INSERT INTO anomaly_events (kind) VALUES ($1)\")",
        "            .execute(pool)",
        "            .await",
        "            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;",
    ];
    assert!(
        !swallowed_near(&propagates, 0),
        "a write that propagates its error was counted as swallowed"
    );
}

/// The burn-down ratchet.
#[test]
fn uninstrumented_swallows_may_only_decrease() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);

    let mut counts: std::collections::BTreeMap<&'static str, usize> =
        SINKS.iter().map(|s| (s.table, 0usize)).collect();
    let mut sites: std::collections::BTreeMap<&'static str, Vec<String>> = Default::default();

    for path in &files {
        if path.ends_with("write_accounting.rs") {
            continue;
        }
        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = body.lines().collect();
        let rel = path
            .strip_prefix(repo)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        for (i, line) in lines.iter().enumerate() {
            for spec in SINKS {
                if !writes_table(line, spec.table) {
                    continue;
                }
                if accounted_near(&lines, i) || !swallowed_near(&lines, i) {
                    continue;
                }
                *counts.get_mut(spec.table).unwrap() += 1;
                sites
                    .entry(spec.table)
                    .or_default()
                    .push(format!("{rel}:{}", i + 1));
            }
        }
    }

    let baseline = |t: &str| {
        BASELINE
            .iter()
            .find(|(k, _)| *k == t)
            .map(|(_, n)| *n)
            .unwrap_or(0)
    };

    let mut regressions = Vec::new();
    let mut stale = Vec::new();
    for (table, n) in &counts {
        let b = baseline(table);
        if *n > b {
            regressions.push(format!(
                "{table}: {n} uninstrumented swallow(s), baseline {b}\n      {}",
                sites
                    .get(table)
                    .map(|v| v.join("\n      "))
                    .unwrap_or_default()
            ));
        } else if *n < b {
            stale.push(format!("{table}: now {n}, baseline still {b}"));
        }
    }

    assert!(
        regressions.is_empty(),
        "\n{} sink(s) gained an uninstrumented swallowed write:\n  {}\n\n\
         Wrap the write in `fermi::write_accounting::observe(Sink::…, result)`. \
         It is shorter than the `if let Err(e) = … tracing::warn!` it replaces, \
         and the difference is that the failure is counted rather than logged \
         into a file nobody reads.\n",
        regressions.len(),
        regressions.join("\n  ")
    );

    // The same discipline as `KNOWN_SILENT`: a baseline that no longer matches
    // reality is a standing permission nobody re-examines. Lowering it is the
    // point of the exercise, so the test insists on it.
    assert!(
        stale.is_empty(),
        "\nthe baseline is now too generous. Lower it — that is the ratchet \
         working:\n  {}\n",
        stale.join("\n  ")
    );
}
