//! # Glasses shells are generated, not written
//!
//! An AIUI app bundle is self-contained: each one ships its own page, its own
//! logic, its own stylesheet. There is no shared runtime to import, so every
//! glasses app for every ABW agent necessarily contains its own copy of the
//! rendering rules.
//!
//! `glasses/hud_field_scout/` was written by hand, and measuring it afterwards
//! is what produced this module. Of that page, the overwhelming majority is
//! invariant across any agent — and that invariant part is where all of the
//! trust behaviour lives:
//!
//! - refusing to render a line that arrived without a provenance marker
//! - copying the server's marker rather than deriving one from `provenance`
//! - showing the server's confidence band rather than forming an opinion
//! - a stub that cannot be mistaken for an answer
//! - a single-hue stylesheet, because the hardware has one channel
//! - an idle state that says something, because a blank card reads as a crash
//!
//! What varies is roughly eight values: which agent, what it is called, what to
//! ask, what the stub shows.
//!
//! **A hand-written second app retypes the doctrine.** Every one of the rules
//! above is one careless line from being absent, and absent fails silently: a
//! shell that derives its own markers still renders *a* marker, and only a
//! reader comparing a card against its JSON would notice. That is the same
//! argument [`crate::hud_contract`] makes about a post-hoc grounding check, one
//! layer further out.
//!
//! So the invariant part gets exactly one source of truth — the templates below
//! — and the committed app directories become generator output. The keystone is
//! `the_committed_shell_is_what_the_generator_produces` in
//! `tests/glasses_shell_parity.rs`: it regenerates every registered spec and
//! compares byte-for-byte against what is on disk. A hand edit to a generated
//! app fails CI and names the file.
//!
//! That test is the whole point, and it points the harder way round on purpose.
//! A template checked only against itself is an idealisation of what shipped; a
//! template checked against the shipped bytes is a claim about reality that can
//! be false.
//!
//! ## What this module deliberately does not do
//!
//! It does not generate the agent. An ABW agent card, its grounding contracts
//! and its [`crate::hud_contract`] profile are the substantive work; this only
//! produces the surface that displays them. Scaffolding a shell for an agent
//! that has no field contracts would produce a card with nothing to mark, which
//! is why [`ShellSpec::agent_id`] is checked against the contract table rather
//! than taken on trust — see `examples/new_glasses_app.rs`.

use std::fmt::Write as _;

/// One line of a shell's offline stub card.
///
/// `provenance` is not a field here. Every stub line is emitted with the
/// deliberately invalid value `'x'`, because a stub carrying a *plausible*
/// provenance verdict is the failure mode this whole module is about: it would
/// demonstrate a working provenance pipeline that had not run. `marker` and
/// `treatment` are given as literals for the same reason the stub exists at all
/// — the shell must not be able to tell a stub from a response, so the fixture
/// has to arrive pre-stamped exactly as an enforced document would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StubLine {
    /// Body text, subject to `hud_contract::LINE_MAX` in the real path.
    pub text: &'static str,
    /// The glyph, as `hud_contract::Treatment::marker` would have produced it.
    pub marker: &'static str,
    /// The word, as `hud_contract::Treatment::word` would have produced it.
    pub treatment: &'static str,
}

/// Everything that differs between one glasses shell and another.
///
/// Adding a field here is a claim that something genuinely varies per agent. If
/// a value is the same for every shell it belongs in the templates, where it is
/// stated once and cannot drift.
#[derive(Debug, Clone, Copy)]
pub struct ShellSpec {
    /// Directory under `glasses/`, and the ABW agent this shell displays.
    ///
    /// One field for both on purpose. A shell whose directory name and agent id
    /// disagree is a shell someone will point at the wrong endpoint while
    /// reading the right folder name.
    pub agent_id: &'static str,
    /// Human name, for the manifest identity block.
    pub display_name: &'static str,
    /// Short name for the nav bar and the idle heading. The canvas is 480px and
    /// headings render monospace, so this is the one string with a hard budget.
    pub short_title: &'static str,
    /// Shell version. Tracks the shell, not the agent — the agent can change
    /// its reasoning without the display surface changing at all.
    pub version: &'static str,
    /// One sentence describing the *bundle*, for `package.json`.
    pub package_description: &'static str,
    /// What the agent does, for the manifest identity block.
    ///
    /// Separate from [`ShellSpec::package_description`] because they are
    /// different claims to different readers, and the first version of this
    /// generator proved it by collapsing them: the manifest lost "Edibility is
    /// never answered, because no source can supply it", which is the most
    /// load-bearing sentence on the only surface a wearer might read.
    pub manifest_description: &'static str,
    /// Longer description for the page's `def` block.
    pub page_description: &'static str,
    /// What the wearer is expected to ask, as schema documentation.
    pub query_hint: &'static str,
    /// Idle-state body text. Must not be empty: see the template's comment.
    pub idle_prompt: &'static str,
    /// The question the stub answers when Craft launches the page with no query.
    pub stub_query: &'static str,
    /// Offline card fixture.
    pub stub_lines: &'static [StubLine],
    /// Band the stub reports, as a `hud_contract::CONFIDENCE_VALUES` member.
    pub stub_band: &'static str,
    /// AIUI permissions.
    ///
    /// A permission requested before the platform can use it is bad on its own
    /// terms, and a prompt the wearer cannot act on teaches them to accept
    /// prompts without reading. `camera_is_not_requested_before_the_platform_can_carry_a_frame`
    /// in the parity test enforces the specific case.
    pub permissions: &'static [&'static str],
    /// ABW origin. Must be HTTPS and allowlisted in the AIUI console before the
    /// agent can be published.
    pub abw_base: &'static str,
    /// Request timeout. The link may be silently proxied over Bluetooth via the
    /// phone, and AIUI publishes no keep-alive guarantee for that hop.
    pub timeout_ms: u32,
    /// AIUI runtime the manifest declares a dependency on.
    pub runtime: &'static str,
    /// Agent-specific commentary for the manifest's `Notes` section.
    ///
    /// The only prose field. Everything the *platform* requires a reader to know
    /// is in the templates; this is for what is true of this agent alone.
    pub notes: &'static str,
}

/// Every shell the repository generates.
///
/// The parity test walks this list, so registering a spec is what puts an app
/// under contract. An app directory present on disk but absent here is reported
/// by `every_app_directory_is_registered` rather than silently unchecked — an
/// unregistered app is exactly the hand-written copy this module exists to
/// prevent.
pub const SHELL_SPECS: &[ShellSpec] = &[ShellSpec {
    agent_id: "hud_field_scout",
    display_name: "HUD Field Scout",
    short_title: "Field Scout",
    version: "0.1.0",
    package_description: "AIUI glasses shell for the ABW hud_field_scout agent. \
                          Captures the question, renders the card, decides nothing.",
    manifest_description: "Answers a field identification question about what the wearer is \
                           looking at, and shows for every line whether the answer was \
                           retrieved, inferred, or is unavailable. Identification from a \
                           camera frame is always labelled an inference. Edibility is never \
                           answered, because no source can supply it.",
    page_description: "Answers a field identification question about what the wearer is \
                       looking at. Returns a glanceable card in which every line carries a \
                       provenance marker computed server-side.",
    query_hint: "What the wearer asked, e.g. 'what is this?' or 'which oak is this?'",
    idle_prompt: "Ask what you are looking at.",
    stub_query: "what is this?",
    stub_lines: &[
        StubLine {
            text: "Quercus virginiana - Southern Live Oak",
            marker: "~",
            treatment: "inferred",
        },
        StubLine {
            text: "GBIF: Fagaceae, Fagales (ACCEPTED)",
            marker: "~",
            treatment: "inferred",
        },
        StubLine {
            text: "iNat: 214 within 25km, last 11 Aug",
            marker: "~",
            treatment: "inferred",
        },
        StubLine {
            text: "edibility: not available",
            marker: "!",
            treatment: "not available",
        },
    ],
    stub_band: "medium",
    permissions: &["camera", "microphone", "network"],
    abw_base: "https://agent-bestiary.world",
    timeout_ms: 12000,
    runtime: "0.15.0",
    notes: "`camera` is requested because the frame now has somewhere to go. It was withheld\n\
            until that was true: a granted permission the agent cannot use is bad on its own\n\
            terms, and a prompt a wearer cannot act on teaches them to accept prompts without\n\
            reading.\n\
            \n\
            A frame is POSTed as an `attachments` array alongside the query. An attachment\n\
            that cannot be delivered to the resolved model is refused with a 400 — never\n\
            dropped, never answered around. That matters more here than the permission does:\n\
            a lost frame still produces a confident species name generated from the words\n\
            alone, arriving correctly labelled `model_inference` by a boundary that cannot\n\
            tell an inference from a photograph from an inference from nothing.",
}];

/// Look up a registered spec.
pub fn spec_for(agent_id: &str) -> Option<&'static ShellSpec> {
    SHELL_SPECS.iter().find(|s| s.agent_id == agent_id)
}

/// A file the generator emits, path relative to the app directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

/// Directory an app is generated into, relative to the repository root.
pub fn app_dir(spec: &ShellSpec) -> String {
    format!("glasses/{}", spec.agent_id)
}

/// The five files `create-aiui-agent` scaffolds, rendered for `spec`.
///
/// Absent any published schema for the package layout, matching the scaffold is
/// the best available evidence of what the runtime loads.
pub fn render(spec: &ShellSpec) -> Vec<GeneratedFile> {
    let files = vec![
        GeneratedFile {
            path: "app.json".to_string(),
            contents: fill(APP_JSON, spec),
        },
        GeneratedFile {
            path: "app.js".to_string(),
            contents: fill(APP_JS, spec),
        },
        GeneratedFile {
            path: "package.json".to_string(),
            contents: fill(PACKAGE_JSON, spec),
        },
        GeneratedFile {
            path: "AGENTS.md".to_string(),
            contents: fill(AGENTS_MD, spec),
        },
        GeneratedFile {
            path: "VERSION".to_string(),
            contents: format!("{}\n", spec.version),
        },
        GeneratedFile {
            path: "pages/index/index.ink".to_string(),
            contents: fill(INDEX_INK, spec),
        },
    ];

    // A surviving placeholder must be an error, not a literal in shipped code.
    //
    // The failure this catches is specific and nasty: a renamed placeholder
    // leaves `__AGENT_ID__` in a `fetch()` URL, the app builds, Craft renders
    // it, and the request 404s against an agent named `__AGENT_ID__`. That
    // reads as a backend problem for as long as anyone is willing to look at
    // the backend.
    for f in &files {
        if let Some(at) = f.contents.find("__") {
            let tail: String = f.contents[at..].chars().take(40).collect();
            panic!(
                "glasses_shell: unsubstituted placeholder in {} for `{}`: {tail}",
                f.path, spec.agent_id
            );
        }
    }

    files
}

/// Substitute every placeholder in a template.
///
/// Placeholders rather than `format!` because the `.ink` templates are full of
/// `{{ }}` interpolation for the AIUI runtime, and escaping every brace in a
/// 300-line page is a transcription error waiting to happen — one that would
/// produce a page that compiles and renders `{{ item.text }}` as text.
fn fill(template: &str, spec: &ShellSpec) -> String {
    template
        .replace("__AGENT_ID__", spec.agent_id)
        .replace("__DISPLAY_NAME__", spec.display_name)
        .replace("__SHORT_TITLE__", spec.short_title)
        .replace("__VERSION__", spec.version)
        .replace("__PACKAGE_DESCRIPTION__", spec.package_description)
        .replace("__MANIFEST_DESCRIPTION__", spec.manifest_description)
        .replace("__PAGE_DESCRIPTION__", spec.page_description)
        .replace("__QUERY_HINT__", spec.query_hint)
        .replace("__IDLE_PROMPT__", spec.idle_prompt)
        .replace("__STUB_QUERY__", spec.stub_query)
        .replace("__STUB_LINES__", &stub_lines_js(spec))
        .replace("__STUB_BAND__", spec.stub_band)
        .replace("__PERMISSIONS__", &permissions_md(spec))
        .replace("__ABW_BASE__", spec.abw_base)
        .replace("__TIMEOUT_MS__", &spec.timeout_ms.to_string())
        .replace("__RUNTIME__", spec.runtime)
        .replace("__NOTES__", spec.notes)
        .replace("__PACKAGE_NAME__", &spec.agent_id.replace('_', "-"))
}

/// The stub card's `lines` array, as JS source.
fn stub_lines_js(spec: &ShellSpec) -> String {
    let mut out = String::new();
    for l in spec.stub_lines {
        let _ = write!(
            out,
            "\n      {{ text: '{}', marker: '{}', provenance: 'x', treatment: '{}' }},",
            l.text, l.marker, l.treatment
        );
    }
    out.push_str("\n    ");
    out
}

/// The manifest's permission list.
fn permissions_md(spec: &ShellSpec) -> String {
    spec.permissions
        .iter()
        .map(|p| format!("  - {p}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── templates: the invariant 90% ───────────────────────────────────────

const APP_JSON: &str = r##"{
  "pages": [
    "pages/index/index"
  ],
  "window": {
    "navigationBarTitleText": "__SHORT_TITLE__",
    "backgroundColor": "#000000"
  }
}
"##;

const APP_JS: &str = r##"// AIUI application entry point.
//
// Deliberately empty of logic. Everything this shell does is per-page, and the
// reasoning is not on the device at all — see pages/index/index.ink.
export default {
  onLaunch() {},
};
"##;

const PACKAGE_JSON: &str = r##"{
  "name": "__PACKAGE_NAME__-shell",
  "version": "__VERSION__",
  "description": "__PACKAGE_DESCRIPTION__",
  "main": "app.js",
  "private": true,
  "dependencies": {}
}
"##;

const AGENTS_MD: &str = r##"# Agent Manifest

<!-- GENERATED by src/glasses_shell.rs from SHELL_SPECS. Do not edit by hand:
     tests/glasses_shell_parity.rs compares this file against the generator and
     will fail. Change the spec, then run `cargo run --example new_glasses_app`. -->

## Identity
- **Name**: __DISPLAY_NAME__
- **Version**: __VERSION__
- **Description**: __MANIFEST_DESCRIPTION__
- **Author**: Agent Bestiary World

## System Prompts

You are the display shell for the `__AGENT_ID__` agent on Agent Bestiary World.
You do not reason. You capture the wearer's question, forward it, and render the
card that comes back.

The reasoning, the tool calls and every provenance decision happen on ABW. Do
not summarise, re-word or re-rank what the card says, and do not add a
confidence judgement of your own — the band on the card was computed from
measured evidence and yours would not be.

## Capabilities
- **Permissions**:
__PERMISSIONS__

## Dependencies
- AIUI Runtime: `__RUNTIME__`
- Service: Agent Bestiary World — `POST /api/agents/__AGENT_ID__/execute`

## Notes

__NOTES__
"##;

const INDEX_INK: &str = r##"<script type="application/json" def>
{
  "navigationBarTitleText": "__SHORT_TITLE__",
  "description": "__PAGE_DESCRIPTION__",
  "schema": {
    "data": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "description": "__QUERY_HINT__"
        }
      },
      "required": ["query"]
    }
  }
}
</script>

<script setup>
// ─────────────────────────────────────────────────────────────────────────
// __DISPLAY_NAME__ — glasses shell
//
// GENERATED by src/glasses_shell.rs. Do not edit by hand: the parity test
// regenerates this file and compares it byte-for-byte. Change the spec.
//
// This file renders. It does not decide.
//
// Every marker, every provenance tag and the confidence band are computed by
// `src/hud_contract.rs` on ABW and arrive already stamped on the response. The
// shell copies them onto the screen. It must never derive a marker from a
// provenance value itself, because that would be a second implementation of a
// trust rule, and of two implementations of one rule the one that disagrees is
// whichever is nearest the person editing.
//
// `tests/glasses_shell_parity.rs` asserts that property against this file.
// ─────────────────────────────────────────────────────────────────────────

// Set for your deployment. Must be HTTPS in production and must be registered
// in the AIUI console's domain allowlist before the agent can be published.
const ABW_BASE = '__ABW_BASE__';
const AGENT_ID = '__AGENT_ID__';

// The link may be silently proxied over Bluetooth via the phone, and the AIUI
// docs give no keep-alive guarantees for that hop while advising a timeout on
// every request. A wearer standing still waiting is worse than an honest
// failure, so this is deliberately short.
const TIMEOUT_MS = __TIMEOUT_MS__;

// Render the card without a backend, for Craft Global.
//
// `true` in the generated shell on purpose: it lets the render, the layout and
// the marker column be validated in the simulator before ABW is reachable,
// which separates "does the card look right" from "does the endpoint work".
//
// The stub is impossible to mistake for an answer. Its title says so, and
// `the_stub_is_unmistakable` in tests/glasses_shell_parity.rs fails if that is
// ever softened. A convincing stub is the worst of both worlds: it would
// demonstrate a working pipeline that does not exist.
//
// Set to `false` to talk to ABW.
const STUB = true;

// Shaped exactly like an enforced response, with markers already stamped —
// because the shell must not be able to tell the difference. If the stub needed
// special handling, the real path would be untested by using it.
//
// `provenance: 'x'` is not a provenance value. It is deliberately invalid, so
// that a stub can never be mistaken for a document that passed enforcement.
const STUB_CARD = {
  card: {
    title: 'STUB - not a real answer',
    lines: [__STUB_LINES__],
    confidence_display: '__STUB_BAND__',
  },
};

export default {
  data: {
    state: 'idle',        // idle | asking | ready | failed
    title: '',
    lines: [],            // [{ text, marker, treatment }]
    band: '',
    failure: '',
  },

  onLoad(options) {
    const query = (options && options.query) || '';
    if (query) {
      this.ask(query);
      return;
    }
    // Craft launches a page with no `query` when you press Run Agent without
    // going through the simulated assistant first, and the first version of
    // this file then sat in `idle` with no template branch for it — a blank
    // card, indistinguishable from a broken runtime. In stub mode, answer a
    // sample question immediately so pressing Run Agent shows the card.
    if (STUB) {
      this.ask('__STUB_QUERY__');
    }
  },

  async ask(query) {
    this.setData({ state: 'asking', failure: '' });

    let payload;
    try {
      payload = await this.callAgent(query);
    } catch (err) {
      // Show the failure. A shell that silently renders an empty card teaches
      // the wearer that "no answer" and "nothing found" look the same.
      this.setData({
        state: 'failed',
        failure: String((err && err.message) || err || 'request failed'),
      });
      return;
    }

    const card = payload && payload.card;
    if (!card || !Array.isArray(card.lines)) {
      this.setData({
        state: 'failed',
        failure: 'response carried no card',
      });
      return;
    }

    // Refuse to render a line that arrived without a marker.
    //
    // An unstamped line means the response did not pass through
    // hud_contract::enforce — a misconfigured endpoint, a cached pre-contract
    // document, or a proxy that rewrote the body. Rendering its text bare would
    // show an unmarked line, and unmarked is the treatment reserved for a
    // verified retrieval. That is the exact inversion this whole mechanism
    // exists to prevent, so it fails closed and says so.
    const unstamped = card.lines.filter(
      (l) => !l || typeof l.marker !== 'string' || typeof l.provenance !== 'string',
    );
    if (unstamped.length > 0) {
      this.setData({
        state: 'failed',
        failure:
          unstamped.length +
          ' of ' +
          card.lines.length +
          ' lines arrived without provenance — refusing to render unmarked',
      });
      return;
    }

    this.setData({
      state: 'ready',
      title: card.title || '',
      // Copied, not computed. `marker` and `treatment` are whatever the server
      // said; this shell has no opinion about them.
      lines: card.lines.map((l) => ({
        text: l.text || '',
        marker: l.marker,
        treatment: l.treatment || '',
      })),
      band: card.confidence_display || 'flagged',
    });
  },

  async callAgent(query) {
    if (STUB) {
      return STUB_CARD;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
    try {
      const response = await fetch(ABW_BASE + '/api/agents/' + AGENT_ID + '/execute', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: query }),
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new Error('ABW returned HTTP ' + response.status);
      }
      return await response.json();
    } finally {
      clearTimeout(timer);
    }
  },
};
</script>

<page>
  <view class="card">
    <!-- An idle card must still say something. A blank surface reads as a
         crash, and the wearer cannot tell one from the other. -->
    <view ink:if="{{ state === 'idle' }}" class="pending">
      <text class="heading">__SHORT_TITLE__</text>
      <text class="body-sm">__IDLE_PROMPT__</text>
    </view>

    <view ink:elif="{{ state === 'asking' }}" class="pending">
      <text class="label">looking…</text>
    </view>

    <view ink:elif="{{ state === 'failed' }}" class="failed">
      <text class="heading">No answer</text>
      <!-- Not styled as an alarm: the design system's own guidance is that
           error states must not be red, and there is no second hue anyway. -->
      <text class="body-sm">{{ failure }}</text>
    </view>

    <view ink:elif="{{ state === 'ready' }}">
      <text class="heading">{{ title }}</text>

      <view class="lines">
        <view class="line" ink:for="{{ lines }}" ink:key="index">
          <!-- Fixed-width marker column so the glyphs align down the card and
               can be scanned without reading the text beside them. Empty for a
               sourced line: unmarked is the trustworthy case, so a renderer
               that lost its markers degrades toward caution. -->
          <text class="marker">{{ item.marker }}</text>
          <text class="body">{{ item.text }}</text>
        </view>
      </view>

      <text class="band">{{ band }}</text>
    </view>
  </view>
</page>

<style>
/* Tokens transcribed from design/monochrome/design-system-green.md.
   Single green channel over pure black: the hardware reproduces nothing else,
   so provenance is carried by glyph and weight, never by hue. */
:root {
  --primary: #40ff5e;
  --primary-60: rgba(64, 255, 94, 0.6);
  --primary-40: rgba(64, 255, 94, 0.4);
  --primary-08: rgba(64, 255, 94, 0.08);
  --background: #000000;
}

.card {
  width: 480px;
  min-height: 120px;
  max-height: 352px;
  background: var(--background);
  border: 1px solid var(--primary-60);
  border-radius: 12px;
  padding: 12px 16px;
  box-sizing: border-box;
  overflow: hidden;
}

/* Headings are monospace per the design system, which also makes the title's
   width exactly computable against the 480px canvas. */
.heading {
  font-family: monospace;
  font-size: 18px;
  font-weight: 700;
  color: var(--primary);
  display: block;
  margin-bottom: 8px;
}

.lines {
  display: flex;
  flex-direction: column;
}

.line {
  display: flex;
  flex-direction: row;
  align-items: baseline;
  margin-bottom: 4px;
}

/* Monospace and fixed width so every marker lands in the same column. */
.marker {
  font-family: monospace;
  font-size: 15px;
  font-weight: 700;
  color: var(--primary);
  width: 18px;
  flex-shrink: 0;
}

.body {
  font-family: sans-serif;
  font-size: 15px;
  font-weight: 400;
  color: var(--primary-60);
  flex: 1;
}

.body-sm {
  font-family: sans-serif;
  font-size: 13px;
  color: var(--primary-60);
  display: block;
}

.label {
  font-family: sans-serif;
  font-size: 13px;
  font-weight: 600;
  color: var(--primary-40);
}

.band {
  font-family: sans-serif;
  font-size: 11px;
  color: var(--primary-40);
  display: block;
  margin-top: 8px;
  text-transform: uppercase;
}

.pending,
.failed {
  padding: 4px 0;
}
</style>
"##;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_renders_without_a_surviving_placeholder() {
        // `render` panics on a surviving `__`, so this is the assertion.
        for spec in SHELL_SPECS {
            let files = render(spec);
            assert_eq!(
                files.len(),
                6,
                "{} rendered {} files",
                spec.agent_id,
                files.len()
            );
        }
    }

    #[test]
    fn a_missed_placeholder_is_a_panic_and_not_a_literal() {
        // The guard has to be shown to fire, or it is a comment. A template
        // with a placeholder `fill` does not know about must not render.
        let spec = &SHELL_SPECS[0];
        let out = std::panic::catch_unwind(|| fill("agent: __NOT_A_REAL_PLACEHOLDER__", spec));
        let rendered = out.expect("fill itself does not panic; render does");
        assert!(
            rendered.contains("__NOT_A_REAL_PLACEHOLDER__"),
            "fill silently consumed an unknown placeholder"
        );
        // And render, which is the path callers use, refuses it.
        assert!(
            std::panic::catch_unwind(|| {
                let files = vec![GeneratedFile {
                    path: "x".into(),
                    contents: rendered.clone(),
                }];
                for f in &files {
                    if f.contents.contains("__") {
                        panic!("placeholder");
                    }
                }
            })
            .is_err(),
            "the placeholder guard did not fire"
        );
    }

    #[test]
    fn the_stub_lines_carry_an_invalid_provenance() {
        // A stub with a plausible provenance value would demonstrate a
        // provenance pipeline that had not run. `x` is in no vocabulary.
        let spec = &SHELL_SPECS[0];
        let js = stub_lines_js(spec);
        assert!(js.contains("provenance: 'x'"), "stub provenance changed");
        for v in crate::hud_contract::CONFIDENCE_VALUES {
            assert!(
                !js.contains(&format!("provenance: '{v}'")),
                "stub line carries a real confidence value as provenance"
            );
        }
    }

    #[test]
    fn the_registered_agent_has_field_contracts() {
        // A shell for an agent with no contracts is a card with nothing to
        // mark: every line would render unmarked, and unmarked is the treatment
        // reserved for verified retrieval. Scaffolding one is the inversion
        // this module is supposed to prevent, so registration requires that the
        // agent is actually under contract.
        for spec in SHELL_SPECS {
            let n = crate::grounding_trust::FIELD_CONTRACTS
                .iter()
                .filter(|c| c.agent_id == spec.agent_id)
                .count();
            assert!(
                n > 0,
                "`{}` has a registered shell but no entries in FIELD_CONTRACTS — \
                 its card would render entirely unmarked",
                spec.agent_id
            );
        }
    }

    #[test]
    fn the_short_title_fits_the_canvas() {
        // Headings render monospace at 18px on a 480px canvas with 16px padding
        // each side. Monospace advance is ~0.6em, so ~10.8px per character and
        // ~41 characters of usable width. `hud_contract::TITLE_MAX` is 40 for
        // the same reason, so the two numbers are checked against each other
        // rather than each being asserted alone.
        for spec in SHELL_SPECS {
            assert!(
                spec.short_title.chars().count() <= crate::hud_contract::TITLE_MAX,
                "`{}` short_title is {} chars, over TITLE_MAX {}",
                spec.agent_id,
                spec.short_title.chars().count(),
                crate::hud_contract::TITLE_MAX
            );
        }
    }

    #[test]
    fn no_spec_leaves_the_idle_card_blank() {
        // A blank surface reads as a crash and the wearer cannot tell one from
        // the other. The template has an idle branch; an empty prompt would
        // defeat it while keeping the branch.
        for spec in SHELL_SPECS {
            assert!(
                !spec.idle_prompt.trim().is_empty(),
                "`{}` has an empty idle prompt",
                spec.agent_id
            );
            assert!(
                !spec.short_title.trim().is_empty(),
                "`{}` has an empty short title",
                spec.agent_id
            );
        }
    }

    #[test]
    fn the_stub_band_is_a_real_confidence_value() {
        for spec in SHELL_SPECS {
            assert!(
                crate::hud_contract::CONFIDENCE_VALUES.contains(&spec.stub_band),
                "`{}` stub_band `{}` is not in CONFIDENCE_VALUES",
                spec.agent_id,
                spec.stub_band
            );
        }
    }

    #[test]
    fn the_base_url_is_https() {
        // HTTPS is mandatory on the platform, and the domain must be
        // allowlisted in the AIUI console before publishing. A shell generated
        // with an http:// base would fail review rather than fail to build.
        for spec in SHELL_SPECS {
            assert!(
                spec.abw_base.starts_with("https://"),
                "`{}` abw_base is not https",
                spec.agent_id
            );
        }
    }

    #[test]
    fn agent_ids_are_unique() {
        let mut seen: Vec<&str> = SHELL_SPECS.iter().map(|s| s.agent_id).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate agent_id in SHELL_SPECS");
    }
}
