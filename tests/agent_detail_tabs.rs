//! # The agent page's tabs are paired by position, not by name
//!
//! `static/js/widgets/tabs.js` matches buttons to panels **by index**:
//!
//! ```js
//! buttons.forEach((b, i) => b.classList.toggle("active", i === idx));
//! panels.forEach((p, i) => { p.style.display = i === idx ? "" : "none"; });
//! ```
//!
//! It never compares `data-tab` to `data-tab-panel`, so those attributes read
//! like a pairing and are in fact decoration. Adding a button in the middle
//! and its panel at the end shifts everything after it by one.
//!
//! That is not hypothetical. It is what happened when the Contract tab landed:
//! its button went in fourth, its panel went in last, and the result was
//! Contract rendering the economic ledger, Intelligence rendering the
//! contract, and the real Intelligence panel unreachable. Both halves looked
//! correct in isolation; only the ordering was wrong, and nothing in the page
//! could have told you.
//!
//! The honest fix would be to match on the attribute in `tabs.js`. That widget
//! is used by other pages this change does not own, so the ordering is pinned
//! here instead and the better fix is left as a deliberate follow-up.

use std::path::Path;

const PAGE: &str = "templates/agent_detail.html";

fn page() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(PAGE);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {PAGE}: {e}"))
}

/// Every `data-tab` in document order — the statically declared buttons, then
/// the ones `renderOwnerTabs` appends.
fn buttons(src: &str) -> Vec<String> {
    let mut out = capture(src, "<button data-tab=\"");
    out.extend(capture(src, "setAttribute(\"data-tab\", \""));
    out
}

fn capture(src: &str, prefix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find(prefix) {
        let after = &rest[i + prefix.len()..];
        if let Some(end) = after.find('"') {
            out.push(after[..end].to_string());
        }
        rest = after;
    }
    out
}

#[test]
fn every_tab_button_is_paired_with_the_panel_at_its_own_index() {
    let src = page();
    let btns = buttons(&src);
    let panels = capture(&src, "data-tab-panel=\"");

    assert!(
        btns.len() >= 8,
        "only found {} tab buttons — the parse is probably broken, which \
         would make this test vacuously pass",
        btns.len()
    );
    assert_eq!(
        btns.len(),
        panels.len(),
        "{} buttons and {} panels. `Tabs.init` zips them by index, so an \
         unmatched count means some tab shows nothing and one panel is \
         unreachable.\n  buttons: {btns:?}\n  panels:  {panels:?}",
        btns.len(),
        panels.len()
    );

    for (i, (b, p)) in btns.iter().zip(panels.iter()).enumerate() {
        assert_eq!(
            b, p,
            "tab {i}: the button says `{b}` and the panel at that index is \
             `{p}`. `tabs.js` pairs by POSITION, so this tab renders the wrong \
             content and so does every tab after it.\n\
             Move the `data-tab-panel=\"{b}\"` div to sit {i}th among the \
             panels.\n  buttons: {btns:?}\n  panels:  {panels:?}"
        );
    }
}

/// The Contract tab specifically, because it is the one this test was written
/// after and the one a future edit is most likely to move.
#[test]
fn the_contract_tab_exists_and_is_paired() {
    let src = page();
    let btns = buttons(&src);
    let panels = capture(&src, "data-tab-panel=\"");

    let bi = btns.iter().position(|b| b == "contract");
    let pi = panels.iter().position(|p| p == "contract");
    assert!(bi.is_some(), "the Contract tab button is gone");
    assert!(pi.is_some(), "the Contract tab panel is gone");
    assert_eq!(
        bi, pi,
        "the Contract button and panel are at different indices, which is the \
         exact bug this file exists for"
    );
}

/// It must not be owner-gated. "What does this return and where does it come
/// from" is a prospective consumer's question, and the tab is where the answer
/// lives.
#[test]
fn the_contract_tab_is_declared_statically_not_appended_for_owners() {
    let src = page();
    assert!(
        src.contains("<button data-tab=\"contract\""),
        "the Contract tab is no longer a static button. If it moved into \
         `renderOwnerTabs` it is now owner-only, and a visitor deciding \
         whether to compose with this agent can no longer see what it returns."
    );
}
