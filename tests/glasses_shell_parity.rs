//! # Glasses shell parity — the shell renders, it does not decide
//!
//! `glasses/hud_field_scout/` is a second surface that displays provenance, and
//! a second surface is a second place for a trust rule to be implemented
//! slightly differently. `port_binding_parity` exists because the port census
//! and the port gate could drift; this exists for the same reason one layer out.
//!
//! The property under test is narrow and load-bearing:
//!
//! > **The shell copies the markers the server computed. It never derives one.**
//!
//! If the shell mapped `provenance` to a glyph itself, there would be two
//! implementations of [`fermi::hud_contract::treatment`], and of two
//! implementations of one rule the one that disagrees is whichever is nearest
//! the person editing. Worse, the disagreement would be invisible: both would
//! render *a* marker, and only a careful reader comparing a card against its
//! JSON would notice they had diverged.
//!
//! These are text assertions over a `.ink` file rather than a running render.
//! That is a real limitation — see the module's final test, which says so out
//! loud rather than letting the file's presence imply more coverage than it has.

use std::path::Path;

const SHELL_DIR: &str = "glasses/hud_field_scout";
const PAGE: &str = "glasses/hud_field_scout/pages/card/index.ink";
const MANIFEST: &str = "glasses/hud_field_scout/AGENTS.md";

fn page() -> String {
    std::fs::read_to_string(PAGE).unwrap_or_else(|e| panic!("read {PAGE}: {e}"))
}

fn manifest() -> String {
    std::fs::read_to_string(MANIFEST).unwrap_or_else(|e| panic!("read {MANIFEST}: {e}"))
}

/// The page with the `STUB_CARD` literal removed.
///
/// The stub is *data shaped like a server response*, so it necessarily contains
/// server-computed values — markers and a confidence band. Scanning it for those
/// values reports the shell as deciding when it is only holding a fixture, and
/// the first version of `the_shell_does_not_derive_markers_itself` did exactly
/// that. A check that fires on correct output gets switched off, so it is scoped
/// instead.
///
/// This is a narrowing, not a weakening: the excluded region is bounded, its
/// presence is asserted, and the remaining text is checked to still be most of
/// the file — a malformed exclusion that swallowed the page would otherwise turn
/// the parity test inert while staying green.
fn logic_without_stub() -> String {
    let p = page();
    let Some(start) = p.find("const STUB_CARD") else {
        return p;
    };
    let end = p[start..]
        .find("\n};")
        .map(|i| start + i + 3)
        .expect("STUB_CARD literal is not terminated by a `};` line");
    let stripped = format!("{}{}", &p[..start], &p[end..]);
    assert!(
        stripped.len() * 100 / p.len() > 60,
        "stripping the stub removed {}% of the page — the exclusion is \
         malformed and this test would be inert",
        100 - (stripped.len() * 100 / p.len())
    );
    stripped
}

// ─── the package is shaped the way the runtime expects ──────────────────

/// The five files `create-aiui-agent` scaffolds. Absent any published schema for
/// the package layout, matching the scaffold is the best available evidence of
/// what the runtime loads.
#[test]
fn the_package_has_the_files_the_scaffold_produces() {
    for f in ["AGENTS.md", "app.js", "app.json", "package.json"] {
        let p = Path::new(SHELL_DIR).join(f);
        assert!(p.exists(), "missing {}", p.display());
    }
    assert!(Path::new(PAGE).exists(), "missing {PAGE}");
}

/// `.ink` is a four-block single-file component. A missing block is a page the
/// runtime will not load, and the failure would first appear in Craft rather
/// than here.
#[test]
fn the_page_has_all_four_ink_blocks() {
    let p = page();
    for block in [
        "<script type=\"application/json\" def>",
        "<script setup>",
        "<page>",
        "<style>",
    ] {
        assert!(p.contains(block), "page is missing its `{block}` block");
    }
}

/// Directives are `ink:`-prefixed, not `wx:`. Easy to get wrong coming from
/// mini-program syntax, and a `wx:if` would silently never render.
#[test]
fn the_template_uses_ink_directives_not_wx() {
    let p = page();
    assert!(
        p.contains("ink:for"),
        "no `ink:for` — the lines cannot render"
    );
    assert!(p.contains("ink:if"), "no `ink:if`");
    assert!(
        !p.contains("wx:if") && !p.contains("wx:for"),
        "page uses `wx:` directives, which this runtime does not bind"
    );
}

// ─── the property this file exists for ─────────────────────────────────

/// **The parity rule.** The rendered marker comes from the response.
#[test]
fn the_shell_renders_the_servers_marker() {
    let p = page();
    assert!(
        p.contains("item.marker"),
        "the template does not render `item.marker`, so whatever it is showing \
         is not the marker hud_contract computed"
    );
}

/// **The parity rule, negatively.** The shell must not own a provenance-to-glyph
/// table.
///
/// Checked by looking for the marker glyphs as literals in the page's script and
/// template. They are legitimate in the stylesheet and in prose; a literal `"~"`
/// next to a provenance value in logic is a second mapping.
#[test]
fn the_shell_does_not_derive_markers_itself() {
    let p = logic_without_stub();
    // The provenance vocabulary must not appear as branching logic in the shell.
    for verdict in [
        "tool_verified",
        "model_inference",
        "tool_no_match",
        "unavailable_no_tool_source",
        "platform_derived",
        "human_sourced",
        "human_endorsed",
        "pending_tool_check",
        "pending_human_check",
    ] {
        // `provenance` may be *checked for presence* (it is, to fail closed),
        // but a specific verdict named in the shell means it is deciding.
        assert!(
            !p.contains(verdict),
            "the shell names the provenance value `{verdict}`. Only \
             hud_contract may map a verdict to a treatment; a copy here is a \
             second answer to one question, and the divergence would be \
             invisible because both would render *a* marker."
        );
    }
    // And it must not carry a band table either.
    for band in ["\"high\"", "'high'", "\"medium\"", "'medium'"] {
        assert!(
            !p.contains(band),
            "the shell names the confidence band {band}. The band is computed \
             from the measured floor on ABW and is only ever displayed here."
        );
    }
}

/// A line that arrived without provenance must not render as bare text.
///
/// Unmarked is the treatment reserved for a verified retrieval, so rendering an
/// unstamped line unmarked would present the *least* verified content with the
/// *most* trusted treatment. The shell has to fail closed, and this asserts it
/// does rather than trusting the comment that says so.
#[test]
fn an_unstamped_line_is_refused_rather_than_rendered() {
    let p = page();
    assert!(
        p.contains("unstamped"),
        "no refusal path for lines lacking provenance"
    );
    assert!(
        p.contains("refusing to render unmarked"),
        "the refusal does not say why, so the next author will remove it"
    );
    // The refusal must come before the render, not after.
    let refusal = p.find("unstamped").expect("refusal");
    let render = p.find("state: 'ready'").expect("ready state");
    assert!(
        refusal < render,
        "the unstamped check runs after the card is already displayed"
    );
}

/// The stub must be impossible to mistake for an answer.
///
/// A convincing stub is worse than none: it demonstrates a working pipeline that
/// does not exist, and the demonstration is the whole reason someone would run
/// it. So the marking is asserted rather than left to a comment.
#[test]
fn the_stub_is_unmistakable() {
    let p = page();
    if !p.contains("const STUB = true") {
        return; // Stub disabled; nothing to mark.
    }
    let title_line = p
        .lines()
        .find(|l| l.contains("title:") && l.contains("STUB"))
        .unwrap_or_else(|| {
            panic!(
                "STUB is enabled but the stub card's title does not say so. A \
                 stub that looks like an answer demonstrates a pipeline that \
                 does not exist."
            )
        });
    assert!(
        title_line.contains("not a real answer") || title_line.contains("NOT REAL"),
        "the stub title `{}` is not explicit enough",
        title_line.trim()
    );
}

/// The stub must be shaped like an enforced response, markers included.
///
/// If the stub needed special handling in the render path, then exercising the
/// stub would not exercise the real path, and the simulator run would prove
/// nothing about the thing being simulated.
#[test]
fn the_stub_goes_through_the_same_render_path() {
    let p = page();
    if !p.contains("const STUB = true") {
        return;
    }
    // Returned from callAgent, so it passes through every check the real
    // response does — including the unstamped-line refusal.
    let stub_return = p.find("return STUB_CARD").expect("stub returned");
    let call_agent = p.find("async callAgent").expect("callAgent");
    assert!(
        stub_return > call_agent,
        "the stub short-circuits somewhere other than callAgent, so it skips \
         the checks the real path runs"
    );
    assert!(
        p.contains("marker: '~'") && p.contains("marker: '!'"),
        "the stub card carries no markers, so it would trip the unstamped-line \
         refusal and never demonstrate the marker column at all"
    );
}

// ─── the monochrome constraint ─────────────────────────────────────────

/// Single green channel. A second hue is not merely off-brand: the panel cannot
/// reproduce it, so a distinction encoded in colour does not exist.
#[test]
fn the_stylesheet_introduces_no_second_hue() {
    let p = page();
    // The published tokens.
    assert!(p.contains("#40ff5e"), "primary token `#40ff5e` is absent");
    assert!(
        p.contains("#000000"),
        "background must be pure black per the design system"
    );
    // Any other hex colour is a second hue. Collect them and check.
    let mut offenders: Vec<String> = Vec::new();
    let bytes: Vec<char> = p.chars().collect();
    for (i, c) in bytes.iter().enumerate() {
        if *c != '#' {
            continue;
        }
        let hex: String = bytes[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        if hex.len() != 6 {
            continue;
        }
        let lower = hex.to_ascii_lowercase();
        if lower != "40ff5e" && lower != "000000" {
            offenders.push(format!("#{hex}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "stylesheet uses {offenders:?}, which the hardware cannot reproduce. \
         The design system's own guidance includes \"do not introduce a second \
         hue\" and \"do not make error states red\"."
    );
}

/// The canvas is 480px wide with a 352px ceiling. Content past that is outside
/// the comfortable field of view, which the design system explicitly forbids.
#[test]
fn the_card_respects_the_published_canvas() {
    let p = page();
    assert!(p.contains("480px"), "canvas width 480px not declared");
    assert!(
        p.contains("max-height: 352px"),
        "352px ceiling not declared"
    );
    assert!(p.contains("min-height: 120px"), "120px floor not declared");
}

// ─── the manifest ──────────────────────────────────────────────────────

/// Follows the convention the shipped samples use. The spec doc, the `aiui-dev`
/// skill and the scaffold template disagree with each other, and only the
/// samples' shape is known to have been packaged and uploaded.
#[test]
fn the_manifest_follows_the_sample_convention() {
    let m = manifest();
    for section in ["# Agent Manifest", "## Identity", "## Capabilities"] {
        assert!(m.contains(section), "manifest is missing `{section}`");
    }
    assert!(
        m.contains("- **Permissions**:"),
        "permissions must use the samples' `- **Permissions**:` shape; the spec \
         doc's dotted `fs.read` style appears in no shipped sample"
    );
}

/// **Camera stays unrequested until a frame has somewhere to go.**
///
/// ABW cannot yet accept an attachment, so a granted camera permission would be
/// one the agent cannot use — and a prompt a wearer cannot act on teaches them
/// to accept prompts without reading. This fails when someone adds `camera`
/// without landing the plumbing, and the fix is to land the plumbing.
#[test]
fn camera_is_not_requested_before_the_platform_can_carry_a_frame() {
    let m = manifest();
    let card = std::fs::read_to_string("agents/curated/hud_field_scout/agent_card.json")
        .expect("read agent card");
    let card: serde_json::Value = serde_json::from_str(&card).expect("parse card");
    let accepts: Vec<String> = card
        .get("accepts")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let platform_takes_images = accepts.iter().any(|a| a == "image");
    let shell_asks_for_camera = m.lines().any(|l| l.trim() == "- camera");

    assert_eq!(
        shell_asks_for_camera, platform_takes_images,
        "the shell asks for camera={shell_asks_for_camera} while the agent card \
         accepts image={platform_takes_images}. These must move together: a \
         camera permission with nowhere to send the frame is unusable, and an \
         image-accepting agent with no camera permission cannot be fed."
    );
}

// ─── what this file does not cover ─────────────────────────────────────

/// Says the limitation out loud, so the file's existence does not imply more
/// than it checks.
///
/// Everything above is a text assertion over a source file. **Nothing here has
/// rendered.** Not asserted, and not assertable from Rust: that the runtime
/// parses this page, that `fetch` reaches ABW through the phone's Bluetooth
/// proxy, that the flex layout lands where intended on a waveguide, that 60
/// characters actually fit 480px at 15px, or that the glyphs are legible on
/// green-on-black at arm's length.
///
/// Those need Craft Global and then a device. This test exists so that the gap
/// is recorded in the same place someone would look for coverage, rather than
/// discovered by trusting a green suite.
#[test]
fn the_uncovered_surface_is_named() {
    let uncovered = [
        "the runtime parses this .ink page",
        "fetch reaches ABW over the Bluetooth-proxied link",
        "the layout lands correctly on a waveguide",
        "LINE_MAX=60 actually fits 480px at 15px sans-serif",
        "the marker glyphs are legible green-on-black at distance",
    ];
    assert_eq!(
        uncovered.len(),
        5,
        "if this list changed, say so in the commit: these are the things a \
         green run here does NOT establish"
    );
}
