# Fermi Console v0.10.0 — Fermi Chat drawer (Slice 1)

**Feature release.** Ships the first slice of Fermi Chat per
`docs/fermi/FERMI_CHAT_AND_AGENT_CREATION_DESIGN.md` — a right-edge
slide-in drawer that lets the operator talk to Fermi from anywhere in
the console. Foundation for design-mode agent creation (v0.11.0).

## What this is

Fermi is already an ABW agent — `agents/curated/fermi/agent_card.json`,
`agent_type: "meta"`, `tier: "system"`. This release does **not** add
new agent infrastructure. It adds a UI pattern on top of the standard
`execute_agent` endpoint against `agent_id="fermi"`, threading a
compact **context envelope** describing "what the operator is looking
at right now" so Fermi can answer with situational awareness.

Same shape ABW's Xaman-Ek uses in the wider platform, applied here to
the Fermi console's forecasting-orchestra domain.

## What operators see

- **New sidebar chip: `💬 Fermi · Ctrl+;`.** Sits next to the
  Shortcuts chip in the sidebar footer. Purple accent + darker
  background when the drawer is open so it's obvious whether Fermi is
  already visible.
- **Hotkey `Ctrl+;`** (Cmd+; on macOS) — toggles the drawer from
  anywhere in the console. Also bound as `Ctrl+Shift+;` so the shape
  operators reach for on some keyboard layouts also works. Added to
  the `Ctrl+/` shortcuts modal so it's discoverable.
- **Right-edge drawer** — 380px wide, absolute-positioned so opening
  it doesn't reflow the panel behind. Border-left in Fermi purple.
- **Empty state** — big 🔮 + "Ask Fermi" + a one-liner telling the
  operator what Fermi is for (decomposition, agent picking, base-rate
  suggestions, FPL questions).
- **Message list** — user messages in cyan-labeled elevated bubbles,
  Fermi replies in purple-bordered deep-purple bubbles, timestamps
  (HH:MM local) in the header of each. Errors surface in-transcript
  as a distinct red-bordered `Error` role so the operator can see
  what they asked *and* what went wrong in one place.
- **Thinking indicator** — while `execute_agent` is in flight, a
  purple "🔮 Fermi is thinking…" pill appears at the bottom of the
  list; the Send button dims to `…`; input is soft-locked (returns
  early instead of queuing a second turn).
- **Input row** — same `TextInput` widget used elsewhere. Click Send
  to fire. (Enter-to-submit comes in a later slice.)

## What Fermi sees on each turn

Every message the operator sends is wrapped with a JSON envelope
capturing situation:

```json
{
  "surface": "composer" | "portfolio" | "dashboard" | "agent_fleet" | "leaderboard" | "teams",
  "forecast_id": "...",
  "forecast_question": "...",
  "predicted_probability": 0.29,
  "drivers": [
    {"name": "strength_factor", "kind": "continuous", "assigned_agent": null},
    {"name": "disruption",      "kind": "binary",     "assigned_agent": "macro_forecaster"}
  ],
  "pm_link": {"event_id": "...", "market_id": "...", "market_price": 0.31, "question": "..."},
  "portfolios": ["EPL", "Tech"],
  "user": "Ivan"
}
```

Missing fields are omitted (not sent as `null`) so the payload stays
compact. Fermi's system prompt is already Tetlock-savvy; the envelope
gives it what it needs to answer "how should I decompose this?" or
"which agent should I assign to `disruption`?" without asking clarifiers.

Wire format the client sends:

```
[fermi_console_context] {"surface":"composer","forecast_question":"…", …}

[operator] How should I decompose this?
```

## What we deliberately did not ship

Per the design doc's slicing:

- **No persistence.** Chat history is RAM only — refresh loses it.
  Slice 3 wires to ABW's chat table (or a new Fermi-scoped table if
  the existing one doesn't key on `agent_id`). The `session_id`
  field on `FermiChatState` is already scaffolded for it.
- **No tool dispatch.** Fermi replies are text only. Slice 2 adds
  `open_forecast`, `open_portfolio`, `run_simulation`, `assign_agent`,
  `set_base_rate`, `link_polymarket` to Fermi's card and wires
  console-side handlers. The `tool_call` / `tool_result` fields on
  `ChatMessage` and the `Tool` variant of `ChatRole` are already
  scaffolded.
- **No design mode.** The create-agent walk-through is Slice 4. The
  `design_step` field on `ChatMessage` is already scaffolded.
- **No streaming.** Non-streaming per turn is fine for the initial
  launch; streaming can slot in at any point since the render code
  already updates on `cx.notify()`.
- **No Enter-to-submit.** Click Send only. Enter binding is a polish
  follow-up.
- **No focus-trap.** The drawer never traps focus; the operator can
  still interact with the panel behind (compose, click portfolios,
  etc.) while asking Fermi about it.

## Files touched

- **`crates/fermi-console/src/chat.rs`** *(new, ~275 LOC)* —
  `FermiChatState`, `ChatMessage`, `ChatRole`, `build_context_envelope`,
  `wrap_query_with_envelope`, `extract_reply_text`, per-message
  render helper.
- **`crates/fermi-console/src/main.rs`** — module declaration, two
  new actions (`ToggleFermiChat`, `SendFermiChat`), `fermi_chat`
  field on `FermiConsole`, `on_toggle_fermi_chat` +
  `on_send_fermi_chat` + `send_fermi_chat_from_input` handlers,
  `build_fermi_envelope` context builder, `render_fermi_chat_drawer`,
  sidebar chip, drawer overlay in the render tree, `Ctrl+;` and
  `Ctrl+Shift+;` keybindings, shortcuts-modal entry.
- **`crates/fermi-console/Cargo.toml`** — 0.9.5 → 0.10.0.
- **`RELEASE_NOTES_v0.10.0.md`** — this file.

Validation: `cargo check -p fermi-console` clean; no new warnings
introduced beyond the 54 pre-existing ones.

## Response extraction contract

Fermi's textual reply is read out of `AgentExecutionResult`'s JSON
form with a fallback chain, so a shape change on any single field
degrades gracefully rather than silently rendering blank messages
(the composer PM typeahead's failure mode from v0.9.5 was fresh in
mind here):

1. `metadata.reasoning` — primary. This is what
   `execute_agent_handler` emits for llm-executor agents in Fermi's
   category.
2. Concatenated `evidence[].summary` — useful when Fermi hands off
   to an orchestra sub-agent.
3. Top-level `text` / `response` / `answer` / `output` /
   `final_answer` — defensive coverage for streaming-buffer /
   direct-Anthropic shapes.
4. Last-resort visible message including the response `status` so
   the operator always sees *something* useful in the transcript.

Diagnostic trace on every completed turn:

```
[fermi-chat] agent responded — tokens=Some(1247) credits=Some(0.09) status="Ok"
```

Failure traces include the query context:

```
[fermi-chat] execute_agent error: HttpError { ... }
```

## Design-doc mapping

Per `docs/fermi/FERMI_CHAT_AND_AGENT_CREATION_DESIGN.md`
§Implementation slices:

- **Slice 1 (this release) — Static chat drawer (RAM only) ~1 day.**
  Done: `FermiChatState`, drawer, `Ctrl+;`, sidebar chip, send-flow
  via `execute_agent("fermi", …)`, envelope built from
  `CockpitState + FermiConsole`.
- **Slice 2 — Tool dispatch ~2 days.** Next candidate.
- **Slice 3 — Chat persistence ~1 day.**
- **Slice 4 — Design mode (create-agent walk-through) ~2–3 days.**
- **Slice 5 (future) — Streaming responses, richer previews.**

## Roadmap

Recommended next: **Slice 2 (tool dispatch)** so Fermi's replies can
open forecasts, run simulations, and assign agents — the moment
Fermi becomes a *hand* not just a *mouth*. That's the v0.10.1 or
v0.11.0 slice.

Alternate next: **v0.9.6 (credit flow)** — caller wallet → owner
wallet at hire time; completes the marketplace loop started in
v0.9.0–v0.9.2.

v0.10.0
