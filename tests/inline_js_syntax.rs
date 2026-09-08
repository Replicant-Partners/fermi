//! # Every inline `<script>` in every template parses
//!
//! A template shipped a blank page. The cause was an HTML comment placed
//! inside a JavaScript template literal, containing backticks:
//!
//! ```text
//!   card.innerHTML = `
//!     <!-- POSITION IS LOAD-BEARING: `Tabs.init` pairs by index -->
//!   `;
//! ```
//!
//! The first backtick in the comment **ends the template literal**. Everything
//! after it parses as code, so `Tabs.init` became a bare identifier and the
//! browser reported `Uncaught SyntaxError: Unexpected identifier 'Tabs'`.
//! Inside a template literal an HTML comment is not a comment; it is string
//! content, and nothing about looking at it says so.
//!
//! ## Why nothing caught it
//!
//! The check that let it through extracted scripts with a non-greedy regex and
//! then syntax-checked the longest match. `agent_detail.html` has five inline
//! scripts; the regex stops at the first `</script>` it sees, including one
//! inside a string, so the thing being checked was a truncated fragment that
//! happened to parse. A regex over a parsed format, in other words — and it
//! reported OK for a page that did not load.
//!
//! `scripts/lint-inline-js.py` uses `html.parser`, which treats script content
//! as CDATA and gets the boundaries right. This test runs it over every
//! template so the class of bug cannot come back through a different file.
//!
//! ## Skipped rather than assumed when `node` is absent
//!
//! A syntax check needs a JS parser. Where `node` is unavailable this test
//! reports that it could not run, and does not pass quietly — the same
//! distinction the rest of this codebase makes between `unverified` and
//! `valid`.

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn templates() -> Vec<String> {
    let dir = repo().join("templates");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read templates/: {e}"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("html"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

fn have_node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn every_inline_script_in_every_template_parses() {
    if !have_node() {
        // Not a pass. Announced, so a green run without node is legible as
        // "not checked here" rather than as "checked and fine".
        eprintln!(
            "SKIPPED: `node` is not on PATH, so inline JavaScript was not \
             syntax-checked. This is an absence of a check, not a passing one."
        );
        return;
    }

    let files = templates();
    assert!(
        files.len() >= 20,
        "only found {} templates — the walk is probably broken, which would \
         make this test vacuously pass",
        files.len()
    );

    let out = Command::new("python3")
        .arg("scripts/lint-inline-js.py")
        .args(&files)
        .current_dir(repo())
        .output()
        .expect("run scripts/lint-inline-js.py");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "inline JavaScript failed to parse. A template literal containing a \
         stray backtick is the usual cause, and an HTML comment inside one is \
         the usual place — inside a template literal a comment is string \
         content, and a backtick in it ends the string.\n\n{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The linter prints one line per script. If it silently found none, the
    // extraction is broken and this test proves nothing.
    let checked = stdout.lines().filter(|l| l.starts_with("OK")).count();
    assert!(
        checked >= 40,
        "only {checked} inline script(s) were checked across {} templates. The \
         extraction is probably broken — which is exactly how the regex \
         version of this check reported OK for a page that did not load.",
        files.len()
    );
}

/// **The falsifier.** Put the original bug in front of the linter.
///
/// The check this replaced was a non-greedy regex over `<script>`…`</script>`.
/// It reported OK for a page that did not load, because it stopped at the first
/// `</script>` — which was inside a string. So "the linter runs and is quiet"
/// proves nothing on its own; what needs proving is that it goes red on the
/// exact shape that shipped a blank page:
///
/// ```text
///   el.innerHTML = `
///     <!-- POSITION IS LOAD-BEARING: `Tabs.init` pairs by index -->
///   `;
/// ```
///
/// Registered in `tests/falsification_registry.rs::SCANS`, which is what
/// noticed this file had no such proof.
#[test]
fn the_linter_sees_a_backtick_inside_a_comment_inside_a_template_literal() {
    if !have_node() {
        eprintln!(
            "SKIPPED: `node` is not on PATH, so the linter's falsifier did not run. \
             This is an absence of a check, not a passing one."
        );
        return;
    }

    let dir = std::env::temp_dir().join(format!("fermi-lintfals-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let bad = dir.join("bad.html");

    // A raw string with `@@` standing in for a backtick.
    //
    // Two reasons, and the second cost an hour. This file is itself scanned by
    // the suite above, so a literal stray backtick here would be the bug
    // rather than a fixture for it. And the first version used `\` line
    // continuations, which in a Rust string literal strip the newline AND the
    // next line's leading whitespace — the fixture came out four characters
    // shorter and parsed cleanly, so the falsifier reported the linter blind
    // when the linter was fine. A fixture that has to reproduce a syntax error
    // exactly must not be assembled by anything that rewrites whitespace.
    let src = r#"<html><body><script>
const el = document.createElement('div');
el.innerHTML = @@
  <!-- POSITION IS LOAD-BEARING: @@Tabs.init@@ pairs by index -->
  <div>hi</div>
@@;
</script></body></html>
"#
    .replace("@@", "`");
    std::fs::write(&bad, &src).expect("write the bad template");

    let out = Command::new("python3")
        .arg("scripts/lint-inline-js.py")
        .arg(&bad)
        .current_dir(repo())
        .output()
        .expect("run the linter");

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !out.status.success(),
        "the linter accepted a template whose inline script cannot parse. That is \
         the exact failure mode of the regex version it replaced — quiet about a \
         page that does not load.\n\n{combined}"
    );
    assert!(
        combined.contains("bad.html"),
        "the linter failed but did not name the file, so a real failure would not \
         say where to look.\n\n{combined}"
    );
}

/// The specific comment that broke the page, pinned.
///
/// Narrower than the syntax check and it survives without `node`: the comment
/// documenting the tab-ordering rule sits inside a template literal, so it must
/// carry no backtick and no `${`.
#[test]
fn the_tab_ordering_comment_stays_template_literal_safe() {
    let src = std::fs::read_to_string(repo().join("templates/agent_detail.html"))
        .expect("read agent_detail.html");

    let start = src
        .find("<!-- Contract Tab.")
        .expect("the tab-ordering comment is gone");
    let end = src[start..]
        .find("-->")
        .map(|i| start + i)
        .expect("unterminated comment");
    let comment = &src[start..end];

    assert!(
        !comment.contains('`'),
        "the tab-ordering comment contains a backtick. It sits inside a JS \
         template literal, so that ends the string and the page stops loading \
         with `Unexpected identifier`. This exact comment did that once.\n\n{comment}"
    );
    assert!(
        !comment.contains("${"),
        "the tab-ordering comment contains `${{`, which interpolates inside a \
         template literal rather than being read as text.\n\n{comment}"
    );
}

/// **The standalone widget scripts parse too.**
///
/// # The gap this closes
///
/// The suite above exists for one defect: an HTML comment inside a JavaScript
/// template literal, containing backticks, which end the literal. Its own
/// header describes it verbatim.
///
/// It scans `templates/` — inline `<script>` blocks — and the widgets moved
/// out of the templates. `static/js/widgets/*.js` is where the large template
/// literals live now: `contract-builder.js` builds its whole markup in one,
/// and `nav.js` builds the navigation in another.
///
/// So the check that names this bug did not cover the files where it recurs,
/// and it recurred: adding two links to `nav.js` with an explanatory HTML
/// comment above them — backticks around the link names — ended the template
/// literal and broke the navigation on every page. `node --check` caught it in
/// seconds; nothing in the suite would have.
///
/// That is the third time in this codebase's recent history that a scan was
/// only as good as the list it scanned: `TRUST_MODULES` did not list
/// `port_trust`, and the trace's fold check searched a 6,000-character window
/// that no longer reached its target.
///
/// # Why `node --check` rather than the inline linter
///
/// These are whole files rather than extracted fragments, so there is nothing
/// to extract — the parser can read them directly, which removes the
/// extraction step that the regex version of the sibling check got wrong.
#[test]
fn every_widget_script_parses() {
    if !have_node() {
        eprintln!(
            "SKIPPED: `node` is not on PATH, so the widget scripts were not \
             syntax-checked. This is an absence of a check, not a passing one."
        );
        return;
    }

    let dir = repo().join("static/js");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("js") {
                files.push(p);
            }
        }
    }

    assert!(
        files.len() >= 10,
        "only found {} scripts under static/js — the walk is broken, which \
         would make this vacuously pass",
        files.len()
    );

    let mut broken: Vec<String> = Vec::new();
    for f in &files {
        let out = Command::new("node")
            .arg("--check")
            .arg(f)
            .current_dir(repo())
            .output()
            .expect("run node --check");
        if !out.status.success() {
            broken.push(format!(
                "{}\n{}",
                f.strip_prefix(repo()).unwrap_or(f).display(),
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }

    assert!(
        broken.is_empty(),
        "{} widget script(s) do not parse. A template literal containing a \
         stray backtick is the usual cause, and an HTML comment inside one is \
         the usual place — inside a template literal a comment is string \
         content, and a backtick in it ends the string.\n\n{}",
        broken.len(),
        broken.join("\n\n")
    );
}

/// **The falsifier for the widget scan.**
///
/// The original bug, in the shape it actually took, put in front of the same
/// parser. Without this the walk above could find zero files, or `node --check`
/// could be reporting success for something it never read, and the suite would
/// stay green either way.
#[test]
fn the_widget_scan_sees_a_backtick_inside_a_comment_inside_a_template_literal() {
    if !have_node() {
        eprintln!("SKIPPED: no node on PATH.");
        return;
    }

    let dir = std::env::temp_dir().join(format!("fermi-widget-lint-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let bad = dir.join("bad.js");
    // Exactly the shape: a comment inside a template literal, with backticks.
    std::fs::write(
        &bad,
        "const MARKUP = `\n  <!-- `New` and `Contracts` were not navigable -->\n  <a href=\"/x\">x</a>\n`;\n",
    )
    .expect("write fixture");

    let out = Command::new("node")
        .arg("--check")
        .arg(&bad)
        .output()
        .expect("run node --check");

    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !out.status.success(),
        "`node --check` accepted a template literal ended by a backtick inside \
         an HTML comment. That is the defect this whole file exists for, and if \
         the parser does not object then the scan above proves nothing."
    );
}
