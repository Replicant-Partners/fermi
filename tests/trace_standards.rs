//! # The artifact trace answers two questions, and only one of them can be red
//!
//! A user opened one `football_analyst` trace and reported it as *"a bunch of
//! rejection"* about an artifact that was **deliverable**. They were right about
//! the page and wrong about the artifact, which means the page was wrong:
//!
//! > *"Some of the things the agent was trying to do were incomplete, and 11
//! > things couldn't be checked but were **omissions not hallucinations**. The
//! > error is **agent completeness vs aspiration**, not failure in the sense of
//! > producing untrusted or unsourced data."*
//!
//! Four different kinds of finding were rendered in one red:
//!
//! | what the page said | what it was |
//! |---|---|
//! | header badge `VIOLATIONS` | a **fault** — the model asserted what it could not know |
//! | `1 of 8 owed`, tone hardcoded `bad` | a **shortfall** — commissioned work not delivered |
//! | `11 ◌` in the fault colour | mostly **capability gaps** and **compliance** |
//! | `records only` | a **ceiling**, not a finding at all |
//!
//! So the one event that should alarm a reader was the *smallest number on the
//! page* and was drowned by two larger numbers that both mean "the agent did not
//! do everything it hoped to".
//!
//! ## Why this needs a browser
//!
//! `field_state` already proves the Rust side, and thoroughly:
//! `a_shortfall_and_a_fault_are_not_the_same_finding` pins the pair that must
//! never collapse, and `exactly_one_finding_is_red_and_it_is_the_fault` pins the
//! standing rule. **Both are worth nothing if the page then paints them the
//! same.** The tone travels to the client as a served string; whether a row ends
//! up wearing it is a fact about the DOM and about nothing else.
//!
//! The distinction is not academic. The first version of this work had every
//! Rust check green while the page rendered no strip at all, because a splice
//! had removed three helper functions — the browser said
//! `emptiness is not defined` and the test suite said nothing.
//!
//! ## Shown to fail
//!
//! `scripts/break_trace_standards.py` reinstates ten collapses — a shortfall in
//! the fault colour, every field chip one tone, the two standards merged, row A
//! unnamed, a verdict on the description, `records only`, `why_not_control`
//! discarded, the legend folded, the legend ordered faults-last, and the caption
//! without its pruning sentence — and asserts the harness goes red for each. It
//! ran, and it does.
//!
//! Two of the ten were informative in their own right:
//!
//! * the `why_not_control` mutation came back **green**, and the check was
//!   wrong rather than the break. The sentence reaches the reader through two
//!   render sites, so a page-wide search for it passed while the answer that
//!   needs it had lost it. The assertion now names the element.
//! * the legend-ordering mutation went red by *throwing*, which is a pass for
//!   the wrong reason and demonstrates nothing about the ordering. It was
//!   rewritten to reverse the comparator instead.

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The file with `/* … */` and `// …` spans removed.
///
/// Crude — it does not know about strings, so a `//` inside a URL literal takes
/// the rest of that line with it. That is the safe direction for this scan: it
/// can only ever remove code from consideration, never add a comment to it, so
/// a false pass is possible and a false failure is not. Every token here appears
/// many times in real code if it appears at all.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            match src[i + 2..].find("*/") {
                Some(end) => i += 2 + end + 2,
                None => break,
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            match src[i..].find('\n') {
                Some(end) => i += end,
                None => break,
            }
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
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
fn only_a_fault_renders_red_on_the_artifact_trace() {
    if !have("node") {
        eprintln!(
            "SKIPPED: `node` is not on PATH, so the artifact trace was not \
             rendered. This is an absence of a check, not a passing one."
        );
        return;
    }
    if !chrome_present() {
        eprintln!(
            "SKIPPED: no Chrome or Chromium found, so the artifact trace was \
             not rendered. This is an absence of a check, not a passing one."
        );
        return;
    }

    let out = Command::new("node")
        .arg("scripts/check_trace_standards.js")
        .current_dir(repo())
        .output()
        .expect("run scripts/check_trace_standards.js");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the artifact trace failed its checks in a real browser.\n\n{stdout}\n{stderr}"
    );
    // The harness prints on success, so an exit-0 without this line means it
    // never reached the assertions.
    assert!(
        stdout.contains("exactly one thing on the page is red"),
        "the harness exited cleanly without reporting that it ran.\n\n{stdout}\n{stderr}"
    );
}

/// The page prints the served run state and does not spell its own.
///
/// The browser check above proves the tokens on one fixture. This covers the
/// *source*: `field_state::Observed` is the one producer, and the failure mode
/// is not a wrong colour but a **fourth inline copy of the five-way match** —
/// which is what the page had, in JavaScript, derived from `produced`, `kind`,
/// `settleable` and a scan of the tool calls.
///
/// That copy could not see the run record, so it could not tell a tool that was
/// ASKED and had nothing from one that was never called. Both are empty fields
/// and only the second is the agent's, and the page contradicted its own rows
/// because of it.
///
/// Runs without a browser, so it still fires on a machine with no Chrome.
#[test]
fn the_trace_page_prints_the_served_run_state_rather_than_deriving_one() {
    let page = std::fs::read_to_string(repo().join("templates/trace.html"))
        .expect("read templates/trace.html");

    // Rendered code only, and both comment forms have to go.
    //
    // Every token asserted below is quoted in the comments that explain why it
    // is not spelled in the code, so a scan over the whole file reports the
    // explanation as the offence. A line filter is not enough: the first
    // version of this test dropped `//` lines and still tripped on a
    // continuation line inside a CSS `/* */` block, which begins with a word.
    let code = strip_comments(&page);

    // **That the row renders the token and wears the served tone is the
    // browser harness's property, not this one.**
    //
    // Two assertions here used to claim it — `code.contains("f.observed")` and
    // a `contains("finding_tone")` with an `||` in it. Both were mutated and
    // **both stayed green**: the strings occur at more than one site, so
    // deleting the one that matters left the other. A check that cannot fail
    // for the case it names is worse than no check, because it reports
    // coverage. `only_a_fault_renders_red_on_the_artifact_trace` asserts the
    // rendering against a real DOM and did catch both.
    //
    // What is left here is what only a source scan can see: where the legend
    // comes from, and whether the page has grown a second copy of a decision.
    assert!(
        code.contains("trace.field_states"),
        "the page does not read `field_states` off the payload, so its run-state \
         legend is written here. A hand-written legend beside served tokens is \
         exactly how `unsourced` came to mean a declared kind on one panel and a \
         violation on the next — and how a paragraph about `returned nothing` \
         came to sit over rows reading `never asked`."
    );

    // The tone must not be recomputed from a count. These are the two
    // expressions that did it, and both are named in the handoff.
    for smuggled in ["empty > 0 ? \"bad\"", "weakSourced > 0 ? \"bad\""] {
        assert!(
            !code.contains(smuggled),
            "`{smuggled}` is back. That is a tone derived from a count, which \
             is a second and weaker copy of `Finding::tone` — and it is the \
             expression that painted a shortfall with the fault colour while \
             the prose one line above separated `owed` from `no_data` and \
             `excused` correctly."
        );
    }

    // `records only` collapsed four declared enforcement modes into two.
    assert!(
        !code.contains("records only"),
        "`records only` reaches a reader again. `command_registry` declares \
         four modes and `artifact_trace` serves all four: `amend` REMOVES the \
         bad part, and `report` returns the verdict to the caller. Printing one \
         word for three of them made the platform sound impotent about the only \
         mode that changes what a caller receives."
    );
}
