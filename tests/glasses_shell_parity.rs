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

use std::fmt::Write as _;
use std::path::Path;

const SHELL_DIR: &str = "glasses/hud_field_scout";
const PAGE: &str = "glasses/hud_field_scout/pages/index/index.ink";
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

/// Every state the page can be in must render something.
///
/// The first version had no `idle` branch, so launching the page without a query
/// — which is what Craft does when you press Run Agent directly — produced a
/// blank card. A blank surface reads as a crash, and neither a wearer nor a
/// developer can tell one from the other.
#[test]
fn every_state_has_a_template_branch() {
    let p = page();
    for state in ["'idle'", "'asking'", "'failed'", "'ready'"] {
        assert!(
            p.contains(&format!("state === {state}")),
            "no template branch for state {state} — that state renders a blank card"
        );
    }
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

// ─── the generated apps are what the generator generates ────────────────

/// Directories under `glasses/` that are deliberately not generated shells.
///
/// `minimal_probe` is a two-line "does the runtime load anything at all" page.
/// It renders no card, has no provenance to carry, and generating it from the
/// card template would give it the whole trust apparatus with nothing to apply
/// it to — a shell asserting it copies markers it never receives.
///
/// This list is short on purpose. An exemption is how a hand-written app gets
/// back in, so each entry needs a reason that is about the app not being a card
/// surface, never about the generator being inconvenient.
const NOT_GENERATED: &[&str] = &["minimal_probe"];

/// **The keystone.** Every committed file in every registered shell is exactly
/// what `src/glasses_shell.rs` produces for it.
///
/// This points the harder way round deliberately. A template validated only
/// against its own output is an idealisation of what shipped; a template
/// validated against the shipped bytes is a claim about reality that can be
/// false — and was. The first run of this comparison found three real
/// divergences: `app.js` referenced `pages/card/index.ink`, a path that does not
/// exist; the manifest's permission list had drifted to four-space nesting away
/// from the sample convention; and collapsing the package and manifest
/// descriptions onto one spec field had silently dropped "Edibility is never
/// answered, because no source can supply it" from the manifest.
///
/// It is also what makes the rest of this file's assertions general. Each of the
/// tests above examines a single app, and that is sufficient rather than lazy:
/// once byte-parity holds, every registered shell is the same template with
/// different substitutions, so a doctrine assertion proved against one instance
/// is proved against all of them. Without this test they would each be a claim
/// about one hand-written file.
#[test]
fn the_committed_shell_is_what_the_generator_produces() {
    for spec in fermi::glasses_shell::SHELL_SPECS {
        let dir = fermi::glasses_shell::app_dir(spec);
        for file in fermi::glasses_shell::render(spec) {
            let path = format!("{dir}/{}", file.path);
            let on_disk = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{path} is registered but unreadable: {e}"));
            if on_disk != file.contents {
                panic!("{}", divergence(&path, &on_disk, &file.contents));
            }
        }
    }
}

/// Every app directory is either generated or explicitly exempt.
///
/// An app on disk that no spec covers is unchecked, and an unchecked app is the
/// hand-written copy the generator exists to prevent — it would carry its own
/// transcription of the fail-closed unstamped check, and a transcription that
/// lost it would still render markers. The failure has to be loud at the point
/// the directory appears, not at the point someone wonders why coverage looks
/// thin.
#[test]
fn every_app_directory_is_registered_or_exempt() {
    let mut unregistered = Vec::new();
    for entry in std::fs::read_dir("glasses").expect("read glasses/") {
        let entry = entry.expect("dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if NOT_GENERATED.contains(&name.as_str()) {
            continue;
        }
        if fermi::glasses_shell::spec_for(&name).is_none() {
            unregistered.push(name);
        }
    }
    assert!(
        unregistered.is_empty(),
        "glasses/ contains app(s) with no ShellSpec: {unregistered:?}.\n\n\
         Add a ShellSpec in src/glasses_shell.rs and regenerate, or add the \
         directory to NOT_GENERATED with a reason it is not a card surface."
    );
}

/// The generator refuses to register a shell for an agent it cannot mark up.
///
/// Asserted here as well as in the module's own tests because this is the
/// boundary condition a person hits when they scaffold a shell for the first
/// time: pick an agent with no field contracts and every line on the card comes
/// back unmarked. Unmarked is the treatment reserved for verified retrieval, so
/// the friendliest possible failure is the correct one.
#[test]
fn a_registered_shell_has_an_agent_under_grounding_contract() {
    for spec in fermi::glasses_shell::SHELL_SPECS {
        let n = fermi::grounding_trust::FIELD_CONTRACTS
            .iter()
            .filter(|c| c.agent_id == spec.agent_id)
            .count();
        assert!(
            n > 0,
            "`{}` has a shell but no field contracts",
            spec.agent_id
        );
    }
}

/// Report *where* a generated file diverged, not that it did.
///
/// The first version of this used `assert_eq!` on the two documents, which
/// printed sixteen kilobytes of escaped `.ink` source and buried the one changed
/// line. A check whose output cannot be read is a check someone reruns with the
/// expected value pasted in, which is the same as not having it — so it names
/// the line, both sides of it, and what to do next.
fn divergence(path: &str, on_disk: &str, generated: &str) -> String {
    let d: Vec<&str> = on_disk.lines().collect();
    let g: Vec<&str> = generated.lines().collect();

    let mut report = format!("{path} is not what src/glasses_shell.rs produces.\n\n");

    let first = (0..d.len().max(g.len())).find(|&i| d.get(i) != g.get(i));
    match first {
        Some(i) => {
            let _ = write!(
                report,
                "first divergence at line {}:\n  on disk:   {}\n  generated: {}\n",
                i + 1,
                d.get(i).map_or("<end of file>", |l| l),
                g.get(i).map_or("<end of file>", |l| l),
            );
            if d.len() != g.len() {
                let _ = write!(
                    report,
                    "\nlengths differ: {} lines on disk, {} generated\n",
                    d.len(),
                    g.len()
                );
            }
        }
        None => {
            report.push_str(
                "the lines are identical, so the difference is trailing \
                 whitespace or a final newline\n",
            );
        }
    }

    report.push_str(
        "\nThis is not automatically a fault in the file on disk. Decide which \
         side is wrong:\n\
         \n\
         - the template lost something a hand edit had added -> put it in the \
           template or the ShellSpec\n\
         - the file was edited directly -> `cargo run --example new_glasses_app` \
           to regenerate\n\
         \n\
         `-- --check` lists every drifted file without writing anything.\n",
    );
    report
}
