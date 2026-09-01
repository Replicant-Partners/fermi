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
