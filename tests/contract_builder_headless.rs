//! # The contract builder's tool-driven affordances, executed
//!
//! Three bugs in this widget in a row reached the user's screen and were found
//! by screenshot: tabs paired to panels by index, a standalone page missing its
//! stylesheets, and a backtick inside an HTML comment inside a template literal
//! that ended the string and blanked the page. Two are now pinned —
//! `tests/agent_detail_tabs.rs` and `tests/inline_js_syntax.rs`.
//!
//! Neither would have caught the fourth. `cbLoadToolNames` fetched the declared
//! response shapes, stored them, and did not redraw. The fetch resolves after
//! `mount()` has already rendered, so the shapes were correct, present, and
//! invisible: the field picker never appeared and an author was back to typing
//! response keys from memory — the exact affordance the table was built to
//! replace. Nothing about the source looks wrong, and it parses.
//!
//! So this runs the widget. `scripts/check_contract_builder.js` stubs a DOM,
//! mounts it, holds the tools fetch open so the before/after is observable, and
//! asserts on the rendered markup.
//!
//! ## Why the shapes are passed in rather than written in the JavaScript
//!
//! The harness asserts that `estimated_size_mb` comes from `ncbi_genome_search`
//! and that two different tools both return `species`. Those are claims about
//! [`fermi::tool_response_shapes::TOOL_RESPONSES`]. Written as a JavaScript
//! fixture they would be a second copy, and a second copy would keep passing
//! about a tool whose response had changed — which is precisely the class of
//! failure the table exists to remove. So this test serialises the real table
//! through [`fermi::tool_response_shapes::declared_shapes_json`], the same
//! function `/api/contracts/tools` calls, and hands the harness those bytes.
//!
//! ## Not a browser
//!
//! A DOM stub executes JavaScript. It has no layout, no stylesheets, and never
//! loads a page, so it cannot see a missing `<link>` or a tab wired to the
//! wrong panel. This narrows the gap; it does not close it. A real headless
//! browser over `/agent/:id` and `/contracts` is still the missing check.

use std::path::Path;
use std::process::Command;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn have_node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn the_contract_builder_offers_tool_fields_and_finds_the_tool_for_a_field() {
    if !have_node() {
        // Announced rather than passed quietly, so a green run without node
        // reads as "not checked here" and not as "checked and fine" — the same
        // distinction between `unverified` and `valid` the contracts make.
        eprintln!(
            "SKIPPED: `node` is not on PATH, so the contract builder was not \
             executed. This is an absence of a check, not a passing one."
        );
        return;
    }

    let shapes = fermi::tool_response_shapes::declared_shapes_json();
    assert!(
        shapes.len() >= 10,
        "only {} tool response shape(s) declared. The harness asserts against \
         this table, so a table that had emptied out would make it pass \
         vacuously. The floor is a smoke check, not a target — raise it if you \
         like, but do not lower it to make a shrinking table green.",
        shapes.len()
    );

    let payload = serde_json::json!({
        "tools": shapes
            .iter()
            .filter_map(|s| s.get("tool").cloned())
            .collect::<Vec<_>>(),
        "response_shapes": shapes,
    });

    let dir = std::env::temp_dir().join(format!(
        "fermi-cb-headless-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir for the shapes payload");
    let path = dir.join("shapes.json");
    std::fs::write(&path, serde_json::to_vec_pretty(&payload).unwrap())
        .expect("write the shapes payload");

    let out = Command::new("node")
        .arg("scripts/check_contract_builder.js")
        .arg(&path)
        .current_dir(repo())
        .output()
        .expect("run scripts/check_contract_builder.js");

    let _ = std::fs::remove_dir_all(&dir);

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "the contract builder failed its headless checks.\n\n{stdout}\n{stderr}"
    );
    // A harness that silently did nothing would exit 0. It prints on success.
    assert!(
        stdout.contains("all checks pass"),
        "the harness exited cleanly without reporting that it ran. It prints on \
         success, so an exit-0 without that line means it never got there.\n\n{stdout}\n{stderr}"
    );
}

/// The two facts the harness reads out of the table, asserted here too.
///
/// The harness skips a check when the table stops supporting it, which keeps it
/// honest but also silent. This says out loud what those checks depend on, so
/// removing either from `TOOL_RESPONSES` fails here rather than quietly
/// shrinking what the harness covers.
#[test]
fn the_table_still_supports_what_the_headless_check_asserts() {
    let genome = fermi::tool_response_shapes::response_for("ncbi_genome_search")
        .expect("ncbi_genome_search is no longer declared");
    assert!(
        genome.fields.iter().any(|f| f.field == "estimated_size_mb"),
        "`estimated_size_mb` is gone from ncbi_genome_search. It is the field \
         whose fabrication for 56 episodes started this work, and the reverse \
         lookup is demonstrated on it."
    );

    // Two tools returning one name is not a defect to resolve; it is the case
    // the reverse lookup must present rather than decide. If it stops
    // occurring, the harness's clash assertions stop running.
    let both: Vec<&str> = fermi::tool_response_shapes::TOOL_RESPONSES
        .iter()
        .filter(|t| t.fields.iter().any(|f| f.field == "species"))
        .map(|t| t.tool)
        .collect();
    assert!(
        both.len() >= 2,
        "only {:?} returns a field named `species`. The headless check uses the \
         clash to prove the lookup shows tool AND path on every hit instead of \
         picking one; with a single producer it proves nothing.",
        both
    );
}

/// **The widget must be themed wherever it mounts.**
///
/// # The bug this pins
///
/// `contract-builder.css` gave the widget's form controls their background and
/// colour under `.cb-standalone`, and `.cb-standalone` existed on exactly one
/// element in the codebase: the `<body>` of `templates/contract_builder.html`.
///
/// The widget is injected by JavaScript into `specimen.html` and
/// `agent_create.html` as well. Both load the stylesheet; neither has the
/// class. So every input the builder rendered on those two pages had **no
/// background at all** and fell back to the user agent default — white fields
/// in a dark theme, and only on the pages where the widget is actually used.
///
/// Most of the controls carry no class of their own (`<input value="...">`
/// inside a `cb-` container), so no class-based selector reaches them. The
/// theming has to hang off an ancestor, and the ancestor was wrong.
///
/// This is the fourth bug of one family in this widget. `check_contract_builder.js`
/// opens by naming three: tabs paired to panels by index, **a page missing its
/// stylesheets**, and a backtick that ended a template literal. All four are
/// "the code runs, the markup is right, and it looks wrong" — which no syntax
/// check sees.
///
/// # Why this is static rather than a computed style
///
/// The right assertion is `getComputedStyle(input).backgroundColor`, and the
/// harness this file drives uses a DOM stub with no layout. A real-browser
/// check exists (`scripts/check_pages_headless.js`) and would be the better
/// home once it mounts the builder.
///
/// Until then this pins the invariant that actually broke: **the class the
/// widget puts on its root must be the class the stylesheet themes its
/// controls under.** That is the pairing that came apart, and it is checkable
/// from the two files.
#[test]
fn the_widget_root_class_is_the_class_its_controls_are_themed_under() {
    let js = std::fs::read_to_string(repo().join("static/js/widgets/contract-builder.js"))
        .expect("contract-builder.js");
    let css = std::fs::read_to_string(repo().join("static/css/contract-builder.css"))
        .expect("contract-builder.css");

    // The widget's outermost element. `form-section` is generic and shared with
    // host pages, which is why the theming could not hang off it.
    assert!(
        js.contains("cb-root"),
        "the widget no longer puts `cb-root` on its outermost element. Its \
         controls are then reachable only through an ancestor it does not \
         control, which is how they came to be white on every page except the \
         one dedicated to them."
    );

    // The rule that themes form CONTROLS must reach that root.
    //
    // Located by selector rather than by the first `background: var(--bg0)` in
    // the file — which is `.cb-shape-row`, a container, and finding it first is
    // how the first version of this test reported a failure that was not there.
    let lines: Vec<&str> = css.lines().collect();
    let control_selector = lines.iter().position(|l| {
        l.contains("cb-root")
            && (l.contains(" input") || l.contains(" select") || l.contains(" textarea"))
    });
    let at = control_selector.unwrap_or_else(|| {
        panic!(
            "no selector mentions both `cb-root` and a form control, so the \
             widget's inputs are themed only under some other ancestor. It \
             mounts into `specimen.html` and `agent_create.html` as well as its \
             own page, and a selector that reaches only one of them leaves the \
             other two with user-agent-default white inputs."
        )
    });

    // …and that rule has to actually set a background, or the selector reaches
    // the controls and says nothing about how they look.
    let body: String = lines[at..]
        .iter()
        .take_while(|l| !l.trim_start().starts_with('}'))
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.contains("background:"),
        "the control selector reaches `cb-root` and sets no background, which \
         is the defect verbatim: a themed-looking rule that leaves the field \
         white.\n\n{body}"
    );

    // And the pages that mount it must load the stylesheet at all — the
    // sibling bug named in the harness header.
    for page in [
        "templates/contract_builder.html",
        "templates/agent_create.html",
        "templates/specimen.html",
    ] {
        let body = std::fs::read_to_string(repo().join(page)).unwrap_or_default();
        assert!(
            body.contains("contract-builder.css"),
            "{page} mounts the contract builder and does not link \
             `contract-builder.css`. That exact defect is one of the three in \
             this widget's history."
        );
    }
}
