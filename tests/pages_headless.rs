//! # The pages, loaded in a real browser
//!
//! Every UI bug in this area so far was found by a person looking at a
//! screenshot, and each got a test afterwards that would not have caught the
//! other two:
//!
//! | bug | why the existing checks missed it |
//! |---|---|
//! | tabs paired to panels by index | the markup is correct; only clicking shows it |
//! | a page missing `common.css` | nothing throws, it just renders unstyled |
//! | a backtick ending a template literal | caught now, but only for `templates/` |
//!
//! `scripts/check_contract_builder.js` executes the widget against a DOM stub,
//! which is a real improvement and still blind to all three: a stub has no
//! layout, no stylesheets, and never loads a page. Worse, it is blind by
//! construction — the stub answers every `getElementById` with an element,
//! because otherwise the renders bail early and nothing is exercised. That is
//! precisely what hid `cbSketch` reading `#agent-name`, an element only the
//! wizard has, which meant nothing on `/contracts` could compile.
//!
//! So this drives Chrome. `scripts/check_pages_headless.js` serves the
//! templates and `static/` over loopback, stubs the API, and asserts on the
//! console, the network, `getComputedStyle`, and what a click actually shows.
//!
//! ## Hermetic, and why that was not a compromise
//!
//! The pages under test are served in production by `app_shell`, which reads
//! the template off disk and returns it with no interpolation. A static server
//! is therefore not an approximation — it is the same bytes. Stubbing the API
//! keeps this runnable on a clean checkout, which matters: the real database is
//! a remote Neon instance, and a UI check that needs production to answer is a
//! UI check that does not get run.
//!
//! ## No npm
//!
//! The driver (`scripts/cdp.js`) speaks the DevTools protocol over Node's
//! built-in `WebSocket`. Playwright would be the conventional answer and costs
//! a `package.json`, a `node_modules` and a ~150MB browser download for
//! features these checks do not use. If they ever need real interaction, throw
//! the driver away for Playwright rather than growing it.

use std::path::{Path, PathBuf};
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

/// The real `/api/contracts/tools` payload, written where the harness (and a
/// human debugging it) can find it.
///
/// Under `target/` rather than a random temp dir on purpose: the harness is
/// meant to be runnable by hand as
/// `node scripts/check_pages_headless.js target/tmp/tool_shapes.json`, and a
/// path that vanishes after the test makes that a guessing game. `target/` is
/// already ignored by git.
fn write_tool_shapes() -> PathBuf {
    let shapes = fermi::tool_response_shapes::declared_shapes_json();
    let payload = serde_json::json!({
        "tools": shapes.iter().filter_map(|s| s.get("tool").cloned()).collect::<Vec<_>>(),
        "response_shapes": shapes,
    });
    let dir = repo().join("target/tmp");
    std::fs::create_dir_all(&dir).expect("create target/tmp");
    let path = dir.join("tool_shapes.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&payload).unwrap())
        .expect("write tool_shapes.json");
    path
}

#[test]
fn the_pages_load_style_themselves_and_pair_their_tabs() {
    if !have("node") {
        eprintln!(
            "SKIPPED: `node` is not on PATH, so no page was loaded in a browser. \
             This is an absence of a check, not a passing one."
        );
        return;
    }
    if !chrome_present() {
        eprintln!(
            "SKIPPED: no Chrome or Chromium found, so no page was loaded in a \
             browser. This is an absence of a check, not a passing one."
        );
        return;
    }

    let shapes = write_tool_shapes();

    let out = Command::new("node")
        .arg("scripts/check_pages_headless.js")
        .arg(&shapes)
        .current_dir(repo())
        .output()
        .expect("run scripts/check_pages_headless.js");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "a page failed its checks in a real browser.\n\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("all checks pass"),
        "the harness exited cleanly without reporting that it ran. It prints on \
         success, so an exit-0 without that line means it never got there \u{2014} \
         which is how a check that quietly stopped checking would look.\n\n{stdout}\n{stderr}"
    );
}

/// The widget may only read elements it writes itself.
///
/// `cbSketch` read `document.getElementById("agent-name").value`. That input
/// belongs to `agent_create.html`; the standalone `/contracts` page has no such
/// field, so every compile threw — and because the compile is debounced, the
/// only visible symptom was a status chip that never left "Compiling…".
///
/// The browser check above catches that specific case. This catches the class,
/// and it does so without a browser, so it still fires on a machine with no
/// Chrome. A widget mounted by two different hosts can only safely depend on
/// the DOM it generates.
#[test]
fn the_contract_builder_only_reads_dom_it_owns() {
    let src = std::fs::read_to_string(repo().join("static/js/widgets/contract-builder.js"))
        .expect("read contract-builder.js");

    // Ids the widget reads that it does NOT put in its own MARKUP. Each one is
    // a dependency on a host page, so each needs a null guard and a reason.
    const HOST_OWNED: &[(&str, &str)] = &[(
        "agent-name",
        "the wizard's name input; read live so the title tracks typing. Guarded \
         by cbAgentTitle(), which falls back to the id loadAgent was given.",
    )];

    let markup_start = src.find("const MARKUP = `").expect(
        "the widget no longer has a MARKUP literal, so this test cannot tell \
                 what it owns",
    );
    let markup = &src[markup_start..];

    let mut unowned: Vec<String> = Vec::new();
    for cap in src.match_indices("getElementById(\"") {
        let rest = &src[cap.0 + "getElementById(\"".len()..];
        let Some(end) = rest.find('"') else { continue };
        let id = &rest[..end];
        // `"cb-view-" + i` and friends: a prefix, not an id.
        if id.ends_with('-') {
            continue;
        }
        if markup.contains(&format!("id=\"{id}\"")) {
            continue;
        }
        if HOST_OWNED.iter().any(|(k, _)| *k == id) {
            continue;
        }
        unowned.push(id.to_string());
    }
    unowned.sort();
    unowned.dedup();

    assert!(
        unowned.is_empty(),
        "the contract builder reads {:?}, which it does not put in its own \
         MARKUP. The widget is mounted by both `agent_create.html` and \
         `templates/contract_builder.html`; an element only one of them has is \
         `null` on the other, and `.value` on null throws. That is exactly what \
         `#agent-name` did, and it silently disabled compiling on the whole \
         standalone page.\n\nEither add the element to MARKUP, or add it to \
         HOST_OWNED with a null guard and a note saying which host owns it.",
        unowned
    );

    // A guard that stopped guarding would make the entry above a lie.
    for (id, _) in HOST_OWNED {
        assert!(
            !src.contains(&format!("getElementById(\"{id}\").value")),
            "`{id}` is read with `.value` directly. It is host-owned, so it is \
             null on the host that does not have it. Read it through a function \
             that checks first."
        );
    }
}
