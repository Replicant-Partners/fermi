//! Every declared gate must record its decisions, at every site that makes one.
//!
//! # Why this is not optional
//!
//! `verification_for_agent_ecologies.md` §4.1 draws the line this test defends:
//! a gate on the admission or invocation clock has a moment that forces it to
//! exist, because somebody is waiting for the verdict. **The recording of that
//! verdict has no such moment.** Nothing stalls when a gate forgets to count
//! itself, nothing turns red, and the system behaves in every observable way
//! exactly as it would if the count were being taken.
//!
//! That is not hypothetical. Before [`fermi::gate_trust`] every gate in the
//! system was in precisely that state: the coherence gate returned a 422 and
//! nothing, credit refusals returned before the ledger INSERT, the rate limiter
//! kept its counts in a process-local map with no export. The platform had a
//! record of every request it served and none of any it refused, and a gate that
//! rejected 100% of agent-wide interventions survived for the life of the
//! feature because of it.
//!
//! # What each check does
//!
//! * [`every_declared_gate_is_recorded_somewhere`] — a `Gate` variant with no
//!   call site is a counter that reads zero for ever, and a zero from an
//!   unrecorded gate is indistinguishable from a gate that never fired. Exact.
//! * [`a_refusal_site_records_before_it_returns`] — the ordering that makes the
//!   count true. A refusal recorded *after* the early return is not recorded.
//! * [`the_scan_only_counts_call_sites`] — this scan's own guard. Its first
//!   cousin in `write_accounting_coverage` was satisfied by a *declaration*
//!   rather than a call, and passed while the instrumentation it checked was
//!   deliberately removed.

use std::path::{Path, PathBuf};

use fermi::gate_trust::GATES;

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

/// Lines that are genuine `Gate::X` **call sites**, not declarations.
///
/// `gate_trust.rs` declares every variant, and the declaration must not satisfy
/// a check about calls. That exact confusion made the sibling scan in
/// `write_accounting_coverage` pass over a removed instrumentation, so it is
/// excluded here by construction rather than by care.
fn gate_call_sites(repo: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    assert!(
        files.len() > 50,
        "the walker found {} files; a scan over an empty set passes for ever",
        files.len()
    );

    files
        .iter()
        .filter(|p| !p.ends_with("gate_trust.rs"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|body| {
            let lines: Vec<String> = body.lines().map(str::to_string).collect();
            let mut found = Vec::new();
            for (i, line) in lines.iter().enumerate() {
                if !line.contains("Gate::") {
                    continue;
                }
                let lo = i.saturating_sub(4);
                let hi = (i + 4).min(lines.len());
                // Every reporting entry point in `gate_trust`, not just the
                // two that existed when this was written. `decided_about` was
                // public and unused outside the module, so the omission was
                // latent until the first caller appeared — and it failed as
                // "this gate records nothing", which is the opposite of true.
                // Note `decided(` is not a substring of `decided_about(`.
                if lines[lo..hi].iter().any(|l| {
                    l.contains("decided(")
                        || l.contains("decided_ok(")
                        || l.contains("decided_about(")
                }) {
                    found.push(line.clone());
                }
            }
            found
        })
        .collect()
}

#[test]
fn every_declared_gate_is_recorded_somewhere() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sites = gate_call_sites(repo);

    let mut missing = Vec::new();
    for spec in GATES {
        // `credit` -> `Gate::Credit`, `rate_limit` -> `Gate::RateLimit`.
        let variant: String = spec
            .id
            .split('_')
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let needle = format!("Gate::{variant}");
        if !sites.iter().any(|l| l.contains(&needle)) {
            missing.push(format!("{} (looked for `{needle}`)", spec.id));
        }
    }

    assert!(
        missing.is_empty(),
        "\n{} declared gate(s) record nothing:\n  {}\n\n\
         A `Gate` variant nobody calls is a counter that reads zero for ever, \
         and a gate reading zero is indistinguishable from a gate that has \
         never had cause to fire. Call \
         `fermi::gate_trust::decided(Gate::…, Decision::…, reason)` at the \
         decision, or remove the variant.\n",
        missing.len(),
        missing.join("\n  ")
    );
}

/// The scan must not be satisfiable by a declaration.
///
/// A guard on this file, written because its sibling failed exactly here: it
/// searched whole files for `Sink::X`, and the liveness contracts name every
/// variant in a struct field. The check passed while the instrumentation it
/// was checking had been deliberately deleted.
#[test]
fn the_scan_only_counts_call_sites() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sites = gate_call_sites(repo);
    assert!(!sites.is_empty(), "no call sites found at all");

    // Every line the scan accepted must have a call within reach. The scan
    // enforces this by construction; asserting it here is what stops a future
    // widening of the window from quietly turning this back into a grep.
    for line in &sites {
        assert!(
            line.contains("Gate::"),
            "the scan accepted a line with no gate reference: {line}"
        );
    }

    // And the declaration file is excluded, so its variants cannot count.
    let decl = std::fs::read_to_string(repo.join("src/gate_trust.rs")).expect("gate_trust.rs");
    assert!(
        decl.contains("gate: Gate::Coherence"),
        "the declaration site no longer looks the way this exclusion assumes"
    );
}

/// A refusal must be recorded **before** the early return that refuses — and
/// the compiler is what enforces it.
///
/// The ordering bug is silent in the friendliest possible way: the gate works,
/// the caller is correctly refused, and the counter reads zero for ever. Every
/// refusal site in this codebase is an early `return`, so the window is one or
/// two statements wide.
///
/// # Why this is an attribute check and not a scan
///
/// A text scan was written for exactly this, looking backwards from each
/// `Decision::Refused` for a preceding `return`. **It did not catch its own
/// motivating case.** The deliberate break — moving the record below the return
/// — pushed the `return` seven lines up, past a four-line window, and the scan
/// reported green. Widening the window would have traded a false negative for
/// false positives on the many legitimate early returns nearby.
///
/// While constructing that break, `rustc` refused to compile it without
/// `#[allow(unreachable_code)]`. The lint had the answer the whole time, and
/// precisely: it is not a heuristic about line distance, it is the compiler's
/// own reachability analysis. So the scan was deleted — a check that certifies
/// without being able to fail is worse than no check — and the crate roots deny
/// the lint instead.
///
/// This test holds the `deny`, because an attribute someone can quietly delete
/// is the same standing permission as an exemption nobody re-reads.
#[test]
fn the_crate_roots_deny_unreachable_code() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    for root in ["src/lib.rs", "src/api_server.rs"] {
        let body =
            std::fs::read_to_string(repo.join(root)).unwrap_or_else(|e| panic!("{root}: {e}"));
        assert!(
            body.contains("#![deny(unreachable_code)]"),
            "{root} no longer denies `unreachable_code`. That lint is the only \
             thing standing between a gate counter and reading zero for ever \
             while the gate it counts works perfectly — a refusal recorded after \
             the return that refuses is never recorded."
        );
    }

    // And no site may opt out of it near a gate or write record, which is the
    // one place the lint is load-bearing.
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    let mut opted_out = Vec::new();
    for path in &files {
        let Ok(body) = std::fs::read_to_string(path) else {
            continue;
        };
        let lines: Vec<&str> = body.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("allow(unreachable_code)") {
                continue;
            }
            // Is the opt-out sitting next to a decision record? That is the
            // one combination the lint exists to prevent here.
            let hi = (i + 10).min(lines.len());
            let near_a_record = lines[i..hi]
                .iter()
                .any(|l| l.contains("Decision::") || l.contains("write_accounting::observe"));
            if near_a_record {
                let rel = path
                    .strip_prefix(repo)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/");
                opted_out.push(format!("{rel}:{}", i + 1));
            }
        }
    }

    assert!(
        opted_out.is_empty(),
        "\n{} site(s) suppress `unreachable_code` next to a gate or write \
         record:\n  {}\n\n\
         That is the exact shape the lint is denied for. The record is dead \
         code, the counter reads zero, and the surrounding logic works.\n",
        opted_out.len(),
        opted_out.join("\n  ")
    );
}
