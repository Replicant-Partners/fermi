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
                if lines[lo..hi].iter().any(|l| reports_a_decision(l)) {
                    found.push(line.clone());
                }
            }
            found
        })
        .collect()
}

/// Does this line call one of `gate_trust`'s reporting entry points?
///
/// # Why this is derived and not a list
///
/// It **was** a list — `decided(`, `decided_ok(`, `decided_about(` — and the
/// comment on it recorded that `decided_about` had been omitted until its first
/// caller appeared, failing as *"this gate records nothing"*, which is the
/// opposite of true.
///
/// It then happened again, identically, the moment `decided_for_episode` was
/// added: `grounding` was reported as recording nothing on the very change that
/// promoted it to `Retention::Recorded`. A list of entry points drifts every time
/// somebody adds one, and it fails in the most misleading available direction —
/// a gate that reports MORE looks like a gate that reports nothing.
///
/// So the shape is derived: `decided` followed by identifier characters and an
/// open paren. Any future `decided_*` is covered without an edit here.
///
/// Deliberately not `contains("decided")`: this repository's comments say the
/// word constantly, and prose counting as coverage is the exact mistake the
/// enclosing scan exists to catch.
fn reports_a_decision(line: &str) -> bool {
    let mut rest = line;
    while let Some(at) = rest.find("decided") {
        let after = &rest[at + "decided".len()..];
        let ident: usize = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .map(char::len_utf8)
            .sum();
        if after[ident..].starts_with('(') {
            return true;
        }
        rest = &rest[at + "decided".len()..];
    }
    false
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

/// Every `Retention::Recorded` gate must record **what it decided about**.
///
/// # The incident
///
/// `gate_decisions` held 42 grounding rows in production and every one of them
/// had `subject = NULL`. The ledger could not say which agent any decision
/// concerned, so `gate_api::GATE_DOORS` handed a reviewer a refusal with no
/// way to find the thing refused, and `gate_decision_reviews` sat at zero rows
/// for the life of the table. The cause was one hardcoded `None` in
/// `decided_for_episode` with the agent slug in scope one line above it.
///
/// Fixing that one writer fixed one gate. This check is why it does not
/// recur: `coherence` and `admission` were in the identical state and nobody
/// had noticed, because their paths are cold — an agent-wide intervention is a
/// rare operator action and the curated corpus does not go through the publish
/// pipeline. Neither had ever written a row, so the defect was invisible and
/// the first row either produced would have been anonymous.
///
/// **A cold defect is still a defect.** A `Counted` gate may be anonymous
/// forever, because it never becomes a row anybody opens. A `Recorded` gate's
/// whole purpose is to be reviewed later, and a row with no subject cannot be.
///
/// # Why the entry points are derived rather than listed
///
/// The same reason [`reports_a_decision`] gives, and the same incident twice
/// over: a hardcoded list of `decided_*` names went stale the moment somebody
/// added one, and failed in the direction that reads as "this gate records
/// nothing". So which entry points carry a subject is read out of
/// `gate_trust.rs`'s own signatures. Add a fifth entry point with a `subject`
/// parameter and it is understood here without an edit.
#[test]
fn every_recorded_gate_names_what_it_decided_about() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));

    let carrying = subject_carrying_entry_points(repo);
    assert!(
        carrying.iter().any(|e| e == "decided_about"),
        "no entry point in gate_trust.rs takes a `subject`, so this check \
         cannot distinguish anything: {carrying:?}"
    );

    let sites = decision_sites_in_src(repo);
    assert!(
        sites.len() > 5,
        "found {} decision sites; a scan over an empty set passes for ever",
        sites.len()
    );

    let problems = anonymous_recorded(&sites, &carrying);
    assert!(
        problems.is_empty(),
        "\n{} Recorded gate decision(s) are written with no subject:\n  {}\n\n\
         A `Recorded` gate becomes a row in `gate_decisions` that a reviewer is \
         asked to judge. With `subject` null they cannot tell whose refusal it \
         is, which is the state all 42 production grounding rows are in and the \
         reason the review door had never been used. Call a `decided_*` entry \
         point that takes a subject. If this decision genuinely has no subject, \
         the gate should not be `Recorded`.\n",
        problems.len(),
        problems.join("\n  ")
    );
}

/// Which `decided_*` entry points take a `subject`, read from their signatures.
fn subject_carrying_entry_points(repo: &Path) -> Vec<String> {
    let body = std::fs::read_to_string(repo.join("src/gate_trust.rs")).expect("gate_trust.rs");
    let mut out = Vec::new();
    let mut rest = body.as_str();
    while let Some(at) = rest.find("pub fn decided") {
        let after = &rest[at + "pub fn ".len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        // The parameter list: from the first `(` to the matching `)`. Depth
        // counted rather than searched for, because `Option<&str>` and
        // `fn() -> bool` both contain parentheses in real signatures.
        if let Some(open) = after.find('(') {
            let mut depth = 0usize;
            let mut end = None;
            for (i, c) in after[open..].char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(end) = end {
                if after[open..end].contains("subject") {
                    out.push(name);
                }
            }
        }
        rest = &rest[at + "pub fn ".len()..];
    }
    out
}

/// Pair each `decided_*(` call with the `Gate::X` it names.
///
/// Over the file **text**, not line by line, because the shape that hid the
/// defect spans lines:
///
/// ```text
/// crate::gate_trust::decided(
///     crate::gate_trust::Gate::Admission,
/// ```
///
/// A line-based scan sees the entry point and the gate on different lines and
/// pairs neither with the other. That is how `admission` kept a subjectless
/// writer while the sibling check in this very file reported it as recorded.
fn decision_sites(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(at) = text[i..].find("decided") {
        let start = i + at;
        let after = &text[start + "decided".len()..];
        let ident: usize = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .map(char::len_utf8)
            .sum();
        if after[ident..].starts_with('(') {
            let entry = format!("decided{}", &after[..ident]);
            // The gate is the first argument in every entry point, so a short
            // window is enough and a long one would reach the NEXT call.
            let span_end = (after.len()).min(ident + 400);
            let span = &after[ident..span_end];
            if let Some(g) = span.find("Gate::") {
                let gate: String = span[g + "Gate::".len()..]
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                if !gate.is_empty() {
                    out.push((entry, gate));
                }
            }
        }
        i = start + "decided".len();
    }
    out
}

/// [`decision_sites`] over every source file, minus the declaration file.
fn decision_sites_in_src(repo: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    rust_sources(&repo.join("src"), &mut files);
    files
        .iter()
        .filter(|p| !p.ends_with("gate_trust.rs"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .flat_map(|body| decision_sites(&body))
        .collect()
}

/// Recorded gates written through an entry point that carries no subject.
fn anonymous_recorded(sites: &[(String, String)], carrying: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for (entry, gate) in sites {
        let Some(spec) = GATES.iter().find(|g| format!("{:?}", g.gate) == *gate) else {
            continue;
        };
        if spec.retention != fermi::gate_trust::Retention::Recorded {
            continue;
        }
        if !carrying.iter().any(|c| c == entry) {
            out.push(format!("{} via `{}` (no subject parameter)", spec.id, entry));
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The pairing scan must survive the shape that hid the defect, and must
/// actually fire on a subjectless Recorded write.
///
/// Both halves matter. The first is the multi-line call: a scan that only reads
/// one line at a time pairs nothing in `publish_pipeline.rs` and therefore
/// reports no problem there, which is exactly how the anonymous `admission`
/// writer survived. The second is the fault itself — with every gate now
/// correct, a check that could not fail on a hand-built bad case would be
/// green for the wrong reason.
#[test]
fn the_pairing_sees_a_multiline_call_and_an_anonymous_recorded_write() {
    // The literal shape from `src/workflows/publish_pipeline.rs`, gate on its
    // own line, entry point on the one above.
    let multiline = "\
    crate::gate_trust::decided(
        crate::gate_trust::Gate::Admission,
        Decision::Refused,
    );";
    assert_eq!(
        decision_sites(multiline),
        vec![("decided".to_string(), "Admission".to_string())],
        "the scan cannot pair an entry point with a gate on a later line, which \
         is the shape it exists to read"
    );

    // A subject-carrying call on the same gate is not a problem.
    let carrying = vec!["decided_about".to_string(), "decided_for_episode".to_string()];
    assert!(
        anonymous_recorded(
            &[("decided_about".into(), "Admission".into())],
            &carrying
        )
        .is_empty(),
        "a Recorded gate recorded WITH a subject must not be reported"
    );

    // The bare one is.
    let found = anonymous_recorded(&[("decided".into(), "Admission".into())], &carrying);
    assert_eq!(
        found.len(),
        1,
        "the check did not fire on a Recorded gate written through an entry \
         point with no subject — the exact production defect: {found:?}"
    );
    assert!(
        found[0].contains("admission"),
        "the finding must name the gate so it can be fixed: {found:?}"
    );

    // And a Counted gate through the same bare entry point is fine, because it
    // never becomes a row anybody opens.
    assert!(
        anonymous_recorded(&[("decided".into(), "RateLimit".into())], &carrying).is_empty(),
        "a Counted gate has no ledger row to be anonymous in; reporting it \
         would make this check noise and it would be switched off"
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

/// The entry-point matcher accepts every real call and no prose.
///
/// Its own falsifier, and it exists because the thing it replaced drifted twice.
/// Both directions matter: a matcher that accepted everything would make the
/// enclosing scan vacuous, and one that accepted too little reports a recording
/// gate as silent.
#[test]
fn the_matcher_sees_every_entry_point_and_no_prose() {
    for call in [
        "    fermi::gate_trust::decided(Gate::Credit, d, None);",
        "        decided_ok(Gate::Attachment, &deliverable);",
        "    gate_trust::decided_about(Gate::OutputSchema, d, r, s);",
        "    fermi::gate_trust::decided_for_episode(Gate::Grounding, d, r, id);",
        // The one that has not been written yet. The point of deriving the
        // shape is that this passes without anyone editing this file.
        "    decided_someday_with_a_new_suffix(Gate::Coherence, d);",
    ] {
        assert!(
            reports_a_decision(call),
            "the matcher missed a real entry point, which reports a recording \
             gate as silent: {call}"
        );
    }

    for prose in [
        "// `decided` is the reporting entry point for a gate.",
        "/// Whether the gate decided anything at all.",
        "    let decided: Vec<_> = batch.iter().map(|d| d.decided_at).collect();",
    ] {
        assert!(
            !reports_a_decision(prose),
            "prose counted as a call site, so a file that mentions the function \
             without calling it would read as covered: {prose}"
        );
    }
}
