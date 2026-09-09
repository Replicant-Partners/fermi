//! # The nav reaches the page, and a careful guard is why it did not
//!
//! `static/js/widgets/nav.js` declares `const Nav = { … }`. In a classic script
//! a top-level `const` is a binding in the global **lexical** environment, and
//! that is not a property of `window` — only `var` and function declarations
//! become those. So the two ways a template can reach the widget are not
//! equivalent:
//!
//! ```text
//!   Nav.init({ … })                  // resolves, works
//!   if (window.Nav) Nav.init({ … })  // window.Nav is undefined, skipped
//! ```
//!
//! Eighteen templates used the first form. Nine used the second and rendered
//! with no navigation at all — `bestiary`, `declarations`, `flow`, `gate`,
//! `loops`, `rounds`, `specimen`, `stream`, `trace`. The platform's primary
//! surfaces, on every load, found by screenshot.
//!
//! ## The guard was the defect
//!
//! `if (window.Nav)` reads as defensive. What it did was convert a one-line
//! omission into an invisible loss of the header, because **a guard whose false
//! branch is silent cannot tell you it took the false branch**. Nothing threw,
//! nothing logged, and the page looked deliberate.
//!
//! The fix makes the guard true rather than removing it: `nav.js` now assigns
//! `window.Nav`, so a template that checks before calling gets the fallback it
//! was reaching for, and a genuine failure to load still degrades to a page
//! with content and no header rather than a page with neither.
//!
//! ## What is asserted
//!
//! | | |
//! |---|---|
//! | [`the_nav_is_reachable_the_way_templates_reach_for_it`] | if any template tests `window.Nav`, `nav.js` must set it |
//! | [`loading_the_nav_and_initialising_it_go_together`] | neither half without the other, both directions |
//!
//! Falsified by `scripts/break_nav_reach.py`.
//!
//! ## Not a browser
//!
//! This is a source scan. It cannot see a nav that renders behind something, or
//! one styled to zero height. It closes the specific gap that put nine pages on
//! screen without a header; a headless browser over the routes is still the
//! missing check, as `contract_builder_headless` also says of itself.

use std::path::{Path, PathBuf};

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn nav_js() -> String {
    let p = repo().join("static/js/widgets/nav.js");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// Does the widget actually assign `window.Nav` in code that runs?
///
/// Line-oriented and comment-stripped, because the first version of this asked
/// `nav_js().contains("window.Nav = Nav")` and **that string survives being
/// commented out**. `scripts/break_nav_reach.py` mutates the assignment to
/// `// window.Nav = Nav;` — the exact edit a person makes while debugging — and
/// the guard stayed green over the bug it was written for. It was written,
/// mutated, and watched not fire; this is the second version.
///
/// Only line comments are stripped. A `/* … */` block around the assignment
/// would still fool this, and saying so is cheaper than implying a parser: the
/// mutation that has actually happened in this repository is `//`.
fn assigns_window_nav(src: &str) -> bool {
    src.lines().any(|line| {
        let code = line.split_once("//").map_or(line, |(before, _)| before);
        code.contains("window.Nav") && code.contains('=')
    })
}

/// Every `templates/*.html`, as `(file name, contents)`.
///
/// Read from the directory rather than from a list, because a template arrives
/// by being added to this folder and a hand-maintained roster would not have
/// mentioned the next one.
fn templates() -> Vec<(String, String)> {
    let dir = repo().join("templates");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .flatten()
        .map(|e| e.path())
        .filter(|p: &PathBuf| p.extension().and_then(|s| s.to_str()) == Some("html"))
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            Some((name, std::fs::read_to_string(&p).ok()?))
        })
        .collect();
    out.sort();
    assert!(
        !out.is_empty(),
        "no templates were read, so both scans below have nothing to look at \
         and would pass over any repository at all"
    );
    out
}

/// If a template asks `window.Nav`, the answer must be yes.
#[test]
fn the_nav_is_reachable_the_way_templates_reach_for_it() {
    let all = templates();
    let askers: Vec<&str> = all
        .iter()
        .filter(|(_, body)| body.contains("window.Nav"))
        .map(|(name, _)| name.as_str())
        .collect();

    if askers.is_empty() {
        // Not a pass to celebrate: it means the property has no subject, and a
        // scan with no subject is indistinguishable from a scan that works.
        // Said out loud so a green run reads as "nothing asks" rather than
        // "everything is fine".
        eprintln!(
            "NOTE: no template tests `window.Nav`, so this test asserted \
             nothing. If the templates were changed to call `Nav.init` bare, \
             delete this test rather than leaving it green over an empty set."
        );
        return;
    }

    assert!(
        assigns_window_nav(&nav_js()),
        "{} template(s) gate their nav on `window.Nav` and \
         `static/js/widgets/nav.js` never assigns it: {askers:?}.\n\n\
         `const Nav = {{ … }}` is a global lexical binding, not a property of \
         `window`, so every one of these pages renders with no header and \
         nothing thrown — the guard takes its silent false branch on every \
         load. Assign `window.Nav = Nav` at the end of the widget, or change \
         these templates to call `Nav.init` bare. Do not delete the guard and \
         leave it at that: a page whose script fails to load should still show \
         its content.",
        askers.len()
    );
}

/// The two halves travel together, in both directions.
///
/// A template that loads the widget and never initialises it ships the cost of
/// the request and none of the header. A template that initialises a widget it
/// never loaded throws on load, or silently skips if it happens to have used
/// the `window.Nav` form — which is how the first failure hid.
#[test]
fn loading_the_nav_and_initialising_it_go_together() {
    let mut loads_without_init = Vec::new();
    let mut inits_without_loading = Vec::new();

    for (name, body) in templates() {
        let loads = body.contains("widgets/nav.js");
        let inits = body.contains("Nav.init");
        if loads && !inits {
            loads_without_init.push(name.clone());
        }
        if inits && !loads {
            inits_without_loading.push(name);
        }
    }

    assert!(
        loads_without_init.is_empty(),
        "template(s) load `nav.js` and never call `Nav.init`, so they pay for \
         the script and render no header: {loads_without_init:?}"
    );
    assert!(
        inits_without_loading.is_empty(),
        "template(s) call `Nav.init` without loading `nav.js`: \
         {inits_without_loading:?}. With the bare form this throws and takes \
         the rest of the page's script with it; with the `window.Nav` form it \
         is silent, which is the failure this whole file exists for."
    );
}
