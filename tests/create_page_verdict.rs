//! # The create page reads the compiler; it does not imitate it
//!
//! `templates/agent_create.html` used to score itself. `buildReview` held five
//! checks written by hand in JavaScript, and one of them read:
//!
//! ```text
//!   "Typed output contract compiles — would pass the publish gate"
//! ```
//!
//! The page could not know that. `publish_pipeline::run_publish_checks` is what
//! `Gate::Admission` refuses on, and it blocks on far more than a compiled
//! contract: `sample_queries`, `accepts`, `produces`, `valence`, and every
//! `agent_contract::typed_tier_violations` finding. So an author could be told
//! they would pass and then be refused, by the surface that was meant to be
//! helping them.
//!
//! **One producer of a verdict; everything else reads it.** These tests hold the
//! page to that, from both sides — because either side alone is trivially
//! satisfiable:
//!
//! | | |
//! |---|---|
//! | [`the_page_asks_the_platform`] | the verdict is fetched at all |
//! | [`the_verdict_is_read_and_not_derived`] | `can_publish` comes from the payload |
//! | [`both_standards_are_rendered`] | legality and ambition stay separate |
//! | [`no_publish_check_is_hardcoded_in_the_page`] | and none of it is re-implemented |
//!
//! Delete the panel and the first three fail. Re-hardcode the checks and the
//! fourth fails. A guard that only forbade imitation could be satisfied by
//! showing nothing at all.
//!
//! ## The list this scans comes from the thing it is scanning for
//!
//! *A scan is only as good as the list it scans* is the most repeated defect in
//! this repository: `TRUST_MODULES` did not list `port_trust`; a fold check
//! searched a window that no longer reached its target; `inline_js_syntax`
//! scanned only `templates/`. So the check names here are **not** written down.
//! They are asked of `contract_checks` and `typed_tier_violations`, which are
//! the functions the publish gate itself calls, and a check added there is
//! covered here the day it lands.
//!
//! Both sources are asserted non-empty before anything is scanned, because a
//! scan over an empty list passes silently and looks exactly like a guard.
//!
//! ## Shown to fail
//!
//! `scripts/break_create_page_verdict.py` mutates the page four ways — drop the
//! fetch, stop reading `can_publish`, collapse the two standards, re-hardcode a
//! check name — and each turns exactly one of these red and no others. A check
//! never seen to fail has not been shown to work.
//!
//! `falsification_registry`'s discovery half does not ask this file for a proof,
//! because it opens one named template rather than walking the repository, and
//! that exemption is deliberate there. The proof exists regardless; it is the
//! break script, and this is where to find it.

use fermi::workflows::agent_contract::{contract_checks, typed_tier_violations, ContractView};
use std::path::{Path, PathBuf};

fn page_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("templates/agent_create.html")
}

fn page() -> String {
    let p = page_path();
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Every check name the publish gate can report, from the gate's own tables.
///
/// `ContractView::default()` is deliberately the empty view: every requirement
/// fails, so `contract_checks` reports all of them, and `agent_id` is `""`
/// which is not in `TYPED_TIER_EXEMPT`, so the typed tier is live and
/// `output_contract: None` makes it speak.
///
/// The two `Warning`-severity checks that `run_publish_checks` appends
/// (`custom_temperature`, `has_executions`) are not included, because reaching
/// them needs a full `Agent` row and `Agent` has no `Default`. They are not
/// written in by hand either: a hand-copied list of names is the failure mode
/// this whole test is built to avoid, and a partial derived list is still
/// derived. The blocking set is the one an author would reach to re-implement.
fn gate_check_names() -> Vec<String> {
    let view = ContractView::default();

    let contract: Vec<String> = contract_checks(&view).into_iter().map(|c| c.name).collect();
    assert!(
        !contract.is_empty(),
        "`contract_checks` reported nothing for an empty ContractView, so the \
         scan below has no list and would pass over any page at all. Either the \
         requirement table is empty or `ContractView::default()` no longer \
         fails its own checks."
    );

    let typed: Vec<String> = typed_tier_violations(&view)
        .into_iter()
        .map(|f| f.check.to_string())
        .collect();
    assert!(
        !typed.is_empty(),
        "`typed_tier_violations` reported nothing for a view with no \
         `output_contract` and an `agent_id` that is not grandfathered. The \
         typed tier is the half of the gate an author is most likely to \
         re-implement, and this scan just lost it."
    );

    let mut names: Vec<String> = contract.into_iter().chain(typed).collect();
    names.sort();
    names.dedup();
    names
}

/// The page must ask the platform. Without this the other guards are vacuous.
#[test]
fn the_page_asks_the_platform() {
    let src = page();
    assert!(
        src.contains("/publish-checks"),
        "`templates/agent_create.html` no longer fetches \
         `/api/agents/:id/publish-checks`. That endpoint is \
         `lifecycle::publish_checks_handler`, which returns \
         `run_publish_checks` — the function `Gate::Admission` actually \
         refuses on. Without it this page is guessing again, and the last time \
         it guessed it promised authors a pass the gate then refused."
    );
}

/// `can_publish` is the platform's reduction of its own checks. Read it.
#[test]
fn the_verdict_is_read_and_not_derived() {
    let src = page();
    assert!(
        src.contains("can_publish"),
        "the page no longer reads `can_publish` from the publish-checks \
         payload. `publish_pipeline::can_publish` is what decides whether the \
         transition into `published` is allowed; a page that recomputes it \
         from the failing count is a second producer of the same verdict, and \
         when the two disagree the page will be the one that is wrong."
    );
}

/// Legality and ambition must stay two panels.
///
/// `CheckSeverity::Error` is what the platform refuses on and
/// `CheckSeverity::Warning` is advisory and never blocking — `can_publish`
/// filters on exactly that. Rolled into one list, an author with a default
/// temperature and an author with no output contract are shown the same red,
/// and "make it all green" becomes advice to prune the agent's ambition until
/// the page stops complaining.
///
/// See `docs/plans/HANDOFF_FIELD_STATE_ON_THE_TRACE.md` §4.5, where the
/// artifact trace had to invent this split that `CheckSeverity` already had.
#[test]
fn both_standards_are_rendered() {
    let src = page();
    for heading in ["Fit to publish", "As designed"] {
        assert!(
            src.contains(heading),
            "`{heading}` is gone from the create page, so the two standards \
             have been collapsed back into one. `Error` is legality and \
             `Warning` is ambition; showing them in one ramp is how a \
             deliverable agent came to read as a bunch of rejection."
        );
    }
}

/// And none of the gate's checks may be re-implemented here.
#[test]
fn no_publish_check_is_hardcoded_in_the_page() {
    let src = page();
    let names = gate_check_names();
    let offenders: Vec<&str> = names
        .iter()
        .filter(|n| src.contains(n.as_str()))
        .map(String::as_str)
        .collect();

    assert!(
        offenders.is_empty(),
        "`templates/agent_create.html` names publish checks itself: {offenders:?}. \
         These come from `contract_checks` / `typed_tier_violations`, which the \
         publish gate calls. A page that writes them down has forked the \
         requirement set: it will drift, and it will drift in the direction of \
         telling an author they are fine. Render what \
         `/api/agents/:id/publish-checks` returns, by the name the platform \
         gives it."
    );
}
