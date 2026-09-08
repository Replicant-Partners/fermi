//! # The evaluator board, and the one thing it must never do
//!
//! `/api/evaluators` shipped with no consumer. Six checks the platform runs on
//! its own machinery — and the only verdicts anywhere on the platform that
//! carry a written `remedy` — were reachable through `/api/admin/schema-health`
//! and rendered by nothing at all.
//!
//! The board exists now. It has one property that no unit test in
//! `evaluator_api` can see, because the property is about what a reader is
//! *shown*:
//!
//! > **`unknown` must not render as a pass.**
//!
//! `nothing_observed_produces_no_healthy_verdict` already proves the server
//! side: over an empty `Observation` every evaluator declines to conclude. That
//! guarantee is worth nothing if the page then paints `inconclusive` green,
//! which is the state three of the six are usually in — most of the counters
//! they read are process-local `AtomicU64`s that reset on restart. A surface
//! that colours it as health reports a healthy platform on every fresh boot,
//! the one moment it is least entitled to.
//!
//! `notice` shares the same `unknown` reading and means something different
//! again: reported, never asserted. Two tokens, one reading, and folding either
//! into a neighbour is the whole failure mode.
//!
//! ## Why a browser
//!
//! The markup being correct is not the question. `scripts/lint-inline-js.py`
//! already proves the script parses, and a string match on the template would
//! prove only that some source contains the word `unknown`. What decides
//! whether a reader is misled is the class the row *ends up carrying* after the
//! render, which is a fact about the DOM and nothing else.
//!
//! ## Shown to fail
//!
//! `scripts/break_evaluator_board.py` mutates the page eight ways — an
//! unrecognised token falling through to healthy, `inconclusive` coloured as a
//! pass, `notice` folded into the fault colour, the caveat dropped, a passing
//! verdict with a caveat left unflagged, the tally collapsed to one number, the
//! empty door list hidden, and a subject that is a sentence turned into a link —
//! and asserts the harness goes red for each. It ran, and it does.
//!
//! One of those eight came back green on the first attempt and needed the
//! *break* fixed rather than the check: the mutation had appended a comment and
//! changed nothing. A `str.replace` that matched nothing is indistinguishable
//! from a guard that did not fire, which is why every mutation asserts its
//! pattern is present first.

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn chrome_present() -> bool {
    [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ]
    .iter()
    .any(|b| have(b))
        || Path::new("/opt/google/chrome/chrome").exists()
}

#[test]
fn no_evaluator_reading_of_unknown_renders_as_a_pass() {
    if !have("node") {
        eprintln!(
            "SKIPPED: `node` is not on PATH, so the evaluator board was not \
             rendered. This is an absence of a check, not a passing one."
        );
        return;
    }
    if !chrome_present() {
        eprintln!(
            "SKIPPED: no Chrome or Chromium found, so the evaluator board was \
             not rendered. This is an absence of a check, not a passing one."
        );
        return;
    }

    let out = Command::new("node")
        .arg("scripts/check_evaluator_board.js")
        .current_dir(repo())
        .output()
        .expect("run scripts/check_evaluator_board.js");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the evaluator board failed its checks in a real browser.\n\n{stdout}\n{stderr}"
    );
    // The harness prints on success, so an exit-0 without this line means it
    // never reached the assertions — which is how a check that quietly stopped
    // checking would look from here.
    assert!(
        stdout.contains("reads as a pass"),
        "the harness exited cleanly without reporting that it ran.\n\n{stdout}\n{stderr}"
    );
}

/// Every token the server can serve has a reading on the page.
///
/// The browser check above covers the five declared tokens and one invented
/// one. This covers the *set*: `evaluator_api::read` is the sole producer of
/// these tokens, and a token it can return that the page has no entry for falls
/// through to `EV_UNRECOGNISED`, which is safe but silent — the reader is told
/// "a token this page does not know" about a token the platform ships.
///
/// So the two lists are compared. This runs without a browser, so it still
/// fires on a machine with no Chrome, and it is the direction that matters:
/// tokens are added on the server and the page is the thing that forgets.
#[test]
fn the_page_has_a_reading_for_every_token_the_server_can_serve() {
    let page = std::fs::read_to_string(repo().join("templates/loops.html"))
        .expect("read templates/loops.html");

    // The declared set, from the one place that produces it.
    let declared = [
        fermi::evaluator_api::read(&fermi::native_evaluators::Verdict::Healthy {
            detail: String::new(),
        })
        .1,
        fermi::evaluator_api::read(&fermi::native_evaluators::Verdict::Inconclusive {
            why: String::new(),
        })
        .1,
        fermi::evaluator_api::read(&fermi::native_evaluators::Verdict::Finding {
            severity: fermi::native_evaluators::Severity::Critical,
            detail: String::new(),
            subjects: vec![],
            remedy: "",
        })
        .1,
        fermi::evaluator_api::read(&fermi::native_evaluators::Verdict::Finding {
            severity: fermi::native_evaluators::Severity::Warning,
            detail: String::new(),
            subjects: vec![],
            remedy: "",
        })
        .1,
        fermi::evaluator_api::read(&fermi::native_evaluators::Verdict::Finding {
            severity: fermi::native_evaluators::Severity::Notice,
            detail: String::new(),
            subjects: vec![],
            remedy: "",
        })
        .1,
    ];

    // The `EV_TOKEN` map on the page, which is the only place a token becomes a
    // colour.
    let start = page
        .find("const EV_TOKEN = {")
        .expect("`EV_TOKEN` is gone from templates/loops.html, so nothing on the page maps a token to a reading");
    let map = &page[start..start + page[start..].find("\n  };").expect("unterminated EV_TOKEN")];

    for token in declared {
        assert!(
            map.contains(&format!("{token}:")),
            "`evaluator_api::read` can return `{token}` and the evaluator board \
             has no entry for it. It will render as `unknown` with the words \"a \
             token this page does not know\" \u{2014} which is the safe fallback and \
             is still wrong about a token the platform ships. Add it to \
             `EV_TOKEN` with the sentence that says what it means.\n\nThe page's \
             map:\n{map}"
        );
    }

    // And the reverse, which is the one that misleads rather than merely
    // omitting: a page entry claiming `idle` for something the server never
    // reads as `idle`.
    for token in ["notice", "inconclusive"] {
        let entry = map
            .split(&format!("{token}:"))
            .nth(1)
            .unwrap_or("")
            .split(']')
            .next()
            .unwrap_or("");
        assert!(
            entry.contains("\"unknown\""),
            "the page maps `{token}` to something other than `unknown`. Both \
             `notice` and `inconclusive` read `unknown` on the server and mean \
             different things; neither is a pass and neither is a fault. Entry \
             found: {entry}"
        );
    }
}
