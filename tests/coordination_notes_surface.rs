//! # The coordination-notes panel, and the seam that would break it silently
//!
//! `/api/agents/:agent_id/coordination-notes` shipped with no consumer. It is
//! the one place Loop 3 and Loop 1 meet, and the only surface on which the
//! platform's central claim about coordination is visible at all: a
//! strategist's brief is *a document*, and an episode written into an agent's
//! memory is what makes it something the agent learns from — because dreaming
//! reads episodes, not workspace git.
//!
//! ## Why the seam needs a guard rather than a review
//!
//! The endpoint is keyed by the agent's **uuid**. The specimen page is reached
//! by **name**. So the panel depends on `profile.agent_id` being in the
//! specimen payload, and it was not there until this panel needed it.
//!
//! That dependency fails in the worst available way. With no id the panel
//! renders *"the coordination notes could not be read"* — and the endpoint is
//! honestly empty today, so the panel is expected to render an empty state
//! anyway. A reader cannot tell the two apart, and neither can a screenshot: an
//! absent field and an absent note look identical, which is the exact
//! confusion the whole surface exists to end.
//!
//! Nothing else would catch it. The panel is on the third tab of a page whose
//! other two tabs are fine; `cargo check` has no opinion about a JSON key; and
//! the browser harness would need the whole specimen payload stubbed to see it.
//! So both ends of the seam are asserted here, cheaply, with no browser.
//!
//! ## Shown to fail
//!
//! Both directions were broken and watched go red — the key removed from the
//! handler, and the read removed from the page. See the session notes; the
//! mutations are two `str.replace`s and are reproducible by hand.

use std::path::Path;

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Both ends of the id seam, asserted against each other.
#[test]
fn the_specimen_payload_carries_the_id_the_notes_endpoint_needs() {
    let handler = std::fs::read_to_string(repo().join("src/handlers/specimen.rs"))
        .expect("read src/handlers/specimen.rs");
    let page = std::fs::read_to_string(repo().join("templates/specimen.html"))
        .expect("read templates/specimen.html");

    // The page reads it. If this changes, the assertion below is about the
    // wrong field and would keep passing while the panel broke.
    assert!(
        page.contains("(D.profile || {}).agent_id"),
        "the specimen page no longer reads `profile.agent_id`. The \
         coordination-notes endpoint is keyed by the uuid and the page is \
         reached by name, so something has to carry the id across. If the panel \
         now gets it another way, point this test at that instead — do not \
         delete it, because the failure it covers is invisible: with no id the \
         panel says \"could not be read\", and the endpoint is honestly empty \
         today, so nobody can tell that apart from having no notes."
    );

    // The handler serves it.
    assert!(
        handler.contains("\"agent_id\": agent_id,"),
        "`specimen_handler` no longer serves `agent_id` on `profile`, and the \
         coordination-notes panel reads it. The panel will render \"the \
         coordination notes could not be read\" for every agent — which is \
         indistinguishable, on the screen, from the empty state it is expected \
         to be in anyway."
    );

    // And it is inside `profile`, which is where the page looks. Serving it at
    // the top level would satisfy the string check above and still be null on
    // the page.
    let profile_start = handler
        .find("\"profile\": {")
        .expect("the specimen payload has no `profile` object");
    let profile = &handler[profile_start..];
    let record_start = profile.find("\"record\": {").unwrap_or(profile.len());
    assert!(
        profile[..record_start].contains("\"agent_id\": agent_id,"),
        "`agent_id` is served, but not inside `profile`, which is where the \
         page reads it from."
    );
}

/// A note nobody dreamt on must not be drawn as an absence.
///
/// `consolidated` is the field that decides whether a coordination note has
/// mattered. A note sitting in an agent's memory that consolidation has not run
/// over has changed nothing about what the agent does — it is
/// indistinguishable, *in the agent's behaviour*, from a note nobody sent.
///
/// Those are still two different things to a reader, and the whole reason the
/// endpoint carries the column is so a surface can tell them apart. A panel
/// that drew an unconsolidated note in the muted absent style, or omitted it,
/// would report the platform's own coordination floor as not existing.
#[test]
fn an_unconsolidated_note_renders_as_its_own_state() {
    let page = std::fs::read_to_string(repo().join("templates/specimen.html"))
        .expect("read templates/specimen.html");

    let start = page
        .find("function coordinationBlock()")
        .expect("the coordination-notes panel is gone from templates/specimen.html");
    let block = &page[start..start + page[start..].find("\n  function ").unwrap_or(4000)];

    assert!(
        block.contains("n.consolidated ?"),
        "the panel no longer branches on `consolidated`. That is the only field \
         that says whether a coordination note has changed anything, and a \
         panel that lists notes without it reports delivery as though it were \
         effect."
    );

    // Two states, two classes. One class for both is one state.
    for cls in ["dreamt", "not"] {
        assert!(
            block.contains(&format!("cn-state ${{n.consolidated ? \"dreamt\" : \"not\"}}"))
                || block.contains(&format!("\"{cls}\"")),
            "the panel does not distinguish `{cls}` visually"
        );
    }
    assert!(
        block.contains("not consolidated"),
        "the un-dreamt state has no words. `consolidated: false` means the note \
         is in memory and has changed nothing yet, and a reader shown only a \
         colour cannot know that."
    );

    // The empty case says which empty it is, in the platform's words.
    //
    // `detail` distinguishes "no agent anywhere has received one" (not this
    // agent's problem; look at Loop 3's `brief` stage) from "others have and
    // this one has not". They are different instructions, and a sentence
    // composed on the page cannot tell them apart because the page does not
    // hold the platform-wide count.
    assert!(
        block.contains("NOTES.detail"),
        "the empty case no longer renders the served `detail`. There are two \
         empties here and only the API knows which one this is — the panel \
         cannot compose the sentence itself, because it does not hold the \
         platform-wide count that separates them."
    );
}
