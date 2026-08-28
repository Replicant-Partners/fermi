//! # Every agent-selection site routes on the same ladder, or the shortest one wins
//!
//! `routing::select_agent_for_driver_declared` is the ladder. Its top rung is
//! `declared` — the agent that claims the question's domain on its own card,
//! resolved from the roster rather than from a compile-time `match` over four
//! domains. That rung exists because two production weather forecasts fell
//! through to the generalist and came back as their own climatological base
//! rate.
//!
//! `routing::select_agent_for_driver` is the same function with `declared`
//! hardcoded to `None`. It is a convenience, and it is indistinguishable at the
//! call site from routing with the rung.
//!
//! ## The failure this guards
//!
//! Only decomposition resolved `declared`. The three other selection sites in
//! `cockpit.rs` — the research panel, the picker's "Recommended" card, and the
//! URL-ingest analyst fallback — called the convenience arity. So the console
//! shipped two routers that shared a function and disagreed in exactly the case
//! the top rung exists for.
//!
//! Observed, in one session on a San Francisco temperature forecast:
//!
//! ```text
//! 12:03:01  Staged 4 agents on 4 drivers: weather_oracle (2), …
//! 12:03:01  ⚠ weather_oracle declares the 'climate' domain …
//! 12:04:18  🔬 Research panel for 'synoptic_pattern_aug29'
//!             — recommended: entity_investigator
//! 12:05:18  entity_investigator: "This is not an entity investigation."
//! ```
//!
//! Decomposition put `weather_oracle` on `synoptic_pattern_aug29`. The panel,
//! on the same driver of the same forecast one minute later, recommended a
//! corporate-ownership investigator, which was accepted, ran, billed, and
//! replied that it had been asked the wrong question. The console had warned
//! two minutes earlier that `weather_oracle` declares `climate`; the panel's
//! router could not see it.
//!
//! ## Why this is a source scan
//!
//! The property is "four call sites in one 29k-line GPUI file all pass the same
//! input". Those sites need a `CockpitState`, a window, an async executor and a
//! live roster; `cockpit.rs` has no test harness and adding one is a much
//! larger change than the defect warrants. Scanning the source is the
//! established pattern here — see `tests/execute_path_parity.rs` and
//! `tests/gate_trust_coverage.rs`, both of which caught real gaps this way.
//!
//! A source scan is a weaker instrument than a behavioural test, and is chosen
//! knowingly. What it can do is stop the sites silently diverging again, which
//! is the failure that actually happened: the divergence compiled, every
//! existing test passed, and the only symptom was a wrong agent on a driver.

use std::path::Path;

const COCKPIT: &str = "crates/fermi-console/src/cockpit.rs";

fn read(rel: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// Whether byte offset `at` in `src` falls inside a `//` comment.
///
/// The prose in this file names both arities on purpose, and a scan that
/// counted doc comments as call sites would fail on its own explanation.
fn in_comment(src: &str, at: usize) -> bool {
    let line_start = src[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
    src[line_start..at].contains("//")
}

/// Every call to a function whose name starts with `name`, as source text,
/// balanced from the opening paren to its match.
///
/// Returns the full call expression so the assertions below can read the
/// arguments, not merely count occurrences.
fn calls_of(src: &str, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = src[from..].find(name) {
        let start = from + rel;
        from = start + name.len();
        if in_comment(src, start) {
            continue;
        }
        // Skip `foo_declared` when asked for `foo`, and `use` items.
        let after = &src[start + name.len()..];
        if !after.starts_with('(') {
            continue;
        }
        let open = start + name.len();
        let mut depth = 0usize;
        let mut end = None;
        for (i, c) in src[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(end) = end {
            out.push(src[start..end].to_string());
        }
    }
    out
}

// ─── the parity ────────────────────────────────────────────────────────

/// **The one that stops drift.** No selection site may use the arity that
/// hardcodes `declared: None`.
///
/// The two arities are one character apart at the call site and differ in
/// whether the router can see the roster at all. Banning the short one here
/// means adding a fifth selection site cannot quietly re-open the gap: the
/// author has to pass something for `declared`, which means resolving it.
#[test]
fn no_selection_site_drops_the_declared_rung() {
    let src = read(COCKPIT);
    let short = calls_of(&src, "select_agent_for_driver");
    let long = calls_of(&src, "select_agent_for_driver_declared");
    let bare: Vec<&String> = short
        .iter()
        .filter(|c| !c.starts_with("select_agent_for_driver_declared"))
        .collect();

    let n = bare.len();
    assert!(
        bare.is_empty(),
        "{n} call site(s) in {COCKPIT} use `select_agent_for_driver`, which \
         hardcodes `declared: None`. Resolve the claimant with \
         `self.declared_specialist_for(&domain)` and call \
         `select_agent_for_driver_declared`. A router that cannot see the \
         roster does not disagree loudly — it recommends a corporate-ownership \
         investigator for a marine-layer driver and looks confident doing it. \
         Offenders: {bare:#?}"
    );
    assert!(
        !long.is_empty(),
        "no selection site found in {COCKPIT} at all — this test has lost its \
         subject and is passing vacuously"
    );
}

/// A resolved claimant, not a literal `None`, reaches the router.
///
/// Calling the wider arity and passing `None` is the same defect wearing the
/// longer name, and it is what a mechanical fix to the test above would
/// produce.
#[test]
fn every_selection_site_passes_a_resolved_claimant() {
    let src = read(COCKPIT);
    for call in calls_of(&src, "select_agent_for_driver_declared") {
        // Argument 5 of 6. `suggested` (argument 4) is legitimately `None` at
        // three of the four sites, so "contains None" is too coarse — the
        // claimant argument is the one immediately before the predicate.
        let declared_arg = call
            .rsplit_once("&|a|")
            .map(|(before, _)| before)
            .and_then(|before| before.trim_end().trim_end_matches(',').rsplit(',').next())
            .unwrap_or("")
            .trim()
            .to_string();
        assert!(
            declared_arg.contains("as_deref") || declared_arg.contains("declared"),
            "a selection site passes `{declared_arg}` as the declared \
             specialist. That is `select_agent_for_driver` with extra steps. \
             Resolve it: `self.declared_specialist_for(&domain)`. Call: \
             {call}"
        );
    }
}

/// Selection gates on admission, never on mere executability.
///
/// `agent_is_routable` answers "can anything execute this id". `agent_is_
/// assignable` answers "may this id be bound to a driver" — it additionally
/// refuses an agent that declares no free-text port. v0.21.1 fixed six
/// selection predicates and missed the picker's "Recommended" card, which is
/// the one an operator actually reads, so the picker could name a
/// recommendation that clicking it would refuse.
#[test]
fn selection_gates_on_admission_not_executability() {
    let src = read(COCKPIT);
    // Both arities, so this holds independently of the test above rather than
    // only after it has been satisfied.
    for call in calls_of(&src, "select_agent_for_driver") {
        assert!(
            !call.contains("agent_is_routable"),
            "a selection site gates on `agent_is_routable`. Routability is a \
             question about the server; admission is the question about this \
             assignment, and it is the one the manual path enforces. Two doors \
             with different locks is the same as one door with none. Call: \
             {call}"
        );
    }
}
