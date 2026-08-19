<script type="application/json" def>
{
  "navigationBarTitleText": "Field Scout",
  "description": "Answers a field identification question about what the wearer is looking at. Returns a glanceable card in which every line carries a provenance marker computed server-side.",
  "schema": {
    "data": {
      "type": "object",
      "properties": {
        "query": {
          "type": "string",
          "description": "What the wearer asked, e.g. 'what is this?' or 'which oak is this?'"
        }
      },
      "required": ["query"]
    }
  }
}
</script>

<script setup>
// ─────────────────────────────────────────────────────────────────────────
// HUD Field Scout — glasses shell
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
const ABW_BASE = 'https://agent-bestiary.world';
const AGENT_ID = 'hud_field_scout';

// The link may be silently proxied over Bluetooth via the phone, and the AIUI
// docs give no keep-alive guarantees for that hop while advising a timeout on
// every request. A wearer standing still waiting is worse than an honest
// failure, so this is deliberately short.
const TIMEOUT_MS = 12000;

// Render the card without a backend, for Craft Global.
//
// `true` in this reference shell on purpose: it lets the render, the layout and
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
const STUB_CARD = {
  card: {
    title: 'STUB - not a real answer',
    lines: [
      { text: 'Quercus virginiana - Southern Live Oak', marker: '~', provenance: 'x', treatment: 'inferred' },
      { text: 'GBIF: Fagaceae, Fagales (ACCEPTED)', marker: '~', provenance: 'x', treatment: 'inferred' },
      { text: 'iNat: 214 within 25km, last 11 Aug', marker: '~', provenance: 'x', treatment: 'inferred' },
      { text: 'edibility: not available', marker: '!', provenance: 'x', treatment: 'not available' },
    ],
    confidence_display: 'medium',
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
      this.ask('what is this?');
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
      <text class="heading">Field Scout</text>
      <text class="body-sm">Ask what you are looking at.</text>
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
