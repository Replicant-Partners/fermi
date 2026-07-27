# Fermi chat + create-agent-through-conversation

Status: **design** · Owner: console · Predecessor: `SCENARIO_TREE_DESIGN.md`

## The framing (read this first)

**Fermi is an ABW agent.** Its card lives at
`agents/curated/fermi/agent_card.json` and satisfies the same schema
every other ABW agent satisfies — the one documented in
`agents/templates/README.md`, `docs/AGENT_MODEL.md`, and enforced by
`src/agent_backend/agent_card.rs`. Fermi's system prompt is longer
than most, its `agent_type` is `"meta"` and its `tier` is `"system"`,
but there's nothing in Fermi's execution model that isn't already in
the platform:

- Fermi is invoked through the same LLM executor path as every other
  `executor: "llm"` agent.
- Fermi's MCP tools (`execute_agent`) are declared in the standard
  `capabilities.mcp_tools` block.
- Fermi's model ladder, capability gates, temperature, and valence
  all live in the same fields any other agent uses.

**Therefore neither of the two features we want here (chat interface,
create-agent-through-conversation) needs new agent infrastructure.**
Both are UI patterns applied to an agent that already exists. The
platform's model — see `docs/AGENT_MODEL.md` §3.3–§3.4 — already
names this:

> **§3.3 xamanEK is the omnipresent meta-agent** — it can answer
> cross-surface questions and navigate the user across the system.
>
> **§3.4 The "create an agent" and "create a composition" flows are
> themselves agent homes — the agent in residence is something like
> an `agent_designer` (or xamanEK in design mode). The form fields
> surface ABW's opinions … as a conversation about how the user's
> agent should collaborate.**

**Fermi is the Fermi-app equivalent of xamanEK in that document.**
It plays the same role, inside the same substrate, over a narrower
domain (forecast decomposition + agent selection instead of
cross-surface navigation). What we're designing here is:

1. **A chat surface** for talking to Fermi from inside the console
   — the same conversational pattern xamanEK exposes on ABW, applied
   to the Fermi app.
2. **An agent-design pathway** through that chat surface — Fermi in
   "design mode" walks the user through the standard ABW agent
   creation contract (`agent_card.json` + `ontology.mermaid` +
   `README.md`) and emits a well-formed card. Same output shape any
   hand-authored curated agent produces.

Neither invents anything. Both are ABW patterns instantiated for the
Fermi domain.

## Vocabulary

| User-facing (UI) | Internal (Rust / DB / spec) |
| --- | --- |
| **Fermi** | `agents/curated/fermi/`, `agent_type: "meta"`, `tier: "system"` |
| **Ask Fermi / talk to Fermi** | Chat runtime invoking the `fermi` agent via the standard LLM executor path |
| **Design mode** | Fermi with an additional system-prompt segment activating `agent_designer` behaviour |
| **Design a new agent** | Fermi emits an `agent_card.json` (+ optional companion files) into the standard `agents/community/` or `agents/curated/` slot |

Same seam as the "scenario ↔ cascade group" mapping in
`SCENARIO_TREE_DESIGN.md`. Internal names stay as they are; UI reads
"Fermi" and "design a new agent".

## Part 1 — Chat interface

### What it does

An operator can ask Fermi anything from anywhere in the Fermi
console. Same content model as ABW's Xaman-Ek chat:

- Multi-turn conversation, message history persisted per session.
- Fermi has context: which forecast is open, which portfolio is
  selected, what the current model probability is, whether a
  simulation is running, what agents are assigned. All of this is
  passed as a **context envelope** on each turn — Fermi never has
  to ask "what forecast are we on?" if the operator has one open.
- Fermi can dispatch actions: fire an orchestra member, open a
  forecast, apply a base rate — via the same `execute_agent` MCP
  tool it already declares in its card, plus a small set of
  console-scoped tools we add (see below).

### Where it lives

The console has five panels today (Dashboard, Portfolio, Agent Fleet,
Composer, Leaderboard/Teams). Fermi chat is **not** a sixth panel —
it's a **persistent slide-in drawer** so Fermi is always one keystroke
away regardless of where the operator is looking. Same UX
metaphor as Cursor's chat or Claude Code's inline panel.

Placement decisions:

- **Anchor:** right edge of the window, above the ρ correlation
  chip on the Portfolio panel and above the Trajectory tab on the
  composer. Drawer width ~360px; content underneath re-flows.
- **Toggle:** hotkey `Ctrl+;` (mirror of `Ctrl+/` for the shortcut
  modal — semicolon reads as "and…" for conversation). A `💬 Fermi`
  chip lives in the top-right of the sidebar footer, next to the
  version chip.
- **Persistence:** drawer state (open/closed) is per-session, not
  saved. Chat history *is* saved server-side (Part 3).
- **Not a modal:** the operator can compose or view a forecast
  while asking Fermi. The drawer never traps focus.

### Context envelope

Every turn the console sends Fermi a JSON blob describing "where the
operator is right now." Same pattern as `docs/AGENT_MODEL.md` §3.3's
"surface-resident agents"; Fermi reads this at every turn.

```json
{
  "surface": "composer",
  "forecast_id": "a3b7…",
  "forecast_question": "Will Manchester City win the 2026-27 EPL?",
  "predicted_probability": 0.295,
  "sim_state": "idle | running | done",
  "drivers": [
    { "name": "strength_factor", "kind": "continuous", "assigned_agent": null },
    { "name": "conditions",      "kind": "continuous", "assigned_agent": null },
    { "name": "disruption",      "kind": "binary",     "assigned_agent": null }
  ],
  "pm_link": null,
  "portfolios": ["EPL", "company perfromance"],
  "cascade_groups": [],
  "last_evidence": []
}
```

Envelope is compact — Fermi doesn't need the whole FPL, just the
"what am I looking at" summary. Full state remains queryable through
tool calls if needed.

### Console-scoped MCP tools

Fermi's card already declares `execute_agent` — enough for it to
delegate research. To make chat useful we add a small set of
console-navigation tools to the same `capabilities.mcp_tools` block
on Fermi's card. **No new agent infrastructure** — these are
standard MCP tools per the ABW spec.

| Tool | Purpose |
| --- | --- |
| `open_forecast` | `{forecast_id}` — navigate composer to that forecast |
| `open_portfolio` | `{portfolio_id}` — Portfolio panel, portfolio selected |
| `open_virtual_bucket` | `{bucket: "shared_with_me" \| "unassigned" \| "drafts"}` — the virtual portfolios I shipped in v0.8.5+ |
| `run_simulation` | fires Ctrl+R on the open forecast |
| `assign_agent` | `{driver_name, agent_id, query}` — same as clicking + Assign Agent on a driver, wraps existing SDK |
| `set_base_rate` | `{ref_class, freq, n, source, reasoning}` — writes metadata.base_rate |
| `link_polymarket` | `{event_id, market_id}` — same code path as v0.8.11's pm_link |

Each tool is a thin wrapper over the ApiClient method that already
exists (or the direct CockpitState mutator). Fermi remains a
standard LLM agent; the tools are its hands.

## Part 2 — Create-agent-through-conversation

### The contract

Fermi in design mode walks the operator through the **exact same
fields** documented in `agents/templates/DESIGN_CHECKLIST.md` and
implemented by `src/agent_backend/agent_card.rs`. No parallel schema.
No new file layout. The output is a valid `agent_card.json` that
`cargo check` accepts on startup, drops into `agents/community/<id>/`,
and appears in the marketplace immediately.

From `docs/AGENT_MODEL.md` §7:

> Templates in `agents/templates/` get regenerated from the current
> AgentCard shape, with design intent … inlined into the comments.
> **xamanEK ingests this doc as part of its agent-design prompts.**

Fermi does the same, over the same source of truth. The `templates/`
files are Fermi's authoritative reference material in design mode.

### The flow

Operator: *"Fermi, I want a new agent that specialises in insurance
underwriting for cyber risk."*

Fermi walks them through DESIGN_CHECKLIST's nine steps
conversationally, one section at a time. For each step Fermi:

1. **Explains the field** with the doc's own language (avoids
   drift — Fermi and the checklist agree because Fermi reads the
   checklist).
2. **Proposes defaults** biased toward the operator's stated
   domain (forecasting-adjacent — most cyber-underwriting agents
   will be `executor: "llm"`, model ladder from haiku/sonnet/opus,
   `analytical` valence, etc.).
3. **Waits for confirmation or override.** No auto-progression.
4. **Emits the running JSON preview** in the chat pane at each
   step — the operator sees the card grow.

Nine-step walk (matches DESIGN_CHECKLIST.md's own structure):

| Step | Fields resolved |
| --- | --- |
| 1 | `agent_id`, `agent_type`, `description`, `sample_queries` |
| 2 | `valence` (`primary_affect`, `arousal`, `valence`, `personality_traits`) |
| 3 | `capabilities.executor`, `model_ladder`, `temperature`, `capability_gates` |
| 4 | `accepts`, `produces`, `dependencies.required`, `dependencies.optional` |
| 5 | `requires_secrets`, `capabilities.mcp_tools` |
| 6 | `ontology.mermaid` (optional; skip for pure LLM agents) |
| 7 | compound? (usually no — skip) |
| 8 | observability defaults (mostly inherited, one prompt about drift threshold) |
| 9 | pre-build sanity: identity contract matches system_prompt, sample_queries specific, model_ladder has a `free` rung |

At the end Fermi shows the full card, asks for a confirm, and on
approval writes to disk (or POSTs to the agent CRUD endpoint —
depends on where the operator's persona lives; see below).

### Where the card lands

The ABW spec has three tiers:

| Tier | Location | Trust |
| --- | --- | --- |
| `curated` | `agents/curated/<id>/` | Reviewed by Fermi team |
| `community` | `agents/community/<id>/` | Any authenticated user |
| `system` | Internal only | Infrastructure |

Fermi's create-agent flow always writes to **`community`** unless the
operator is a Fermi team member (a flag we can check via the auth
principal). No manual gate on submission — the eval framework and
observatory catch drift automatically.

Two possible physical write paths:

- **A. Direct disk write** (single-tenant desktop-console world):
  Fermi writes to `agents/community/<id>/agent_card.json` on the
  operator's local machine, `cargo check` picks it up on next launch,
  agent immediately available.
- **B. Server-side registration** (multi-tenant / shared marketplace):
  Fermi POSTs the card to ABW's agent CRUD endpoint. Card lives in
  the `agents` table; the Fermi console fetches it via the existing
  `list_agents` route on next refresh.

We ship **B** — it's the pattern that scales, matches how everything
else the console does (forecasts, portfolios, teams) is persisted,
and doesn't force a restart. Route B needs one new SDK method
(`ApiClient::create_agent`) wrapping the existing ABW endpoint.

If ABW's agent-create endpoint doesn't yet accept the full card
shape (unlikely but worth checking), the sub-feature is gated on
that landing.

### Domain-specific defaults Fermi contributes

Fermi's system prompt already knows Tetlock-calibrated forecast
methodology and the eight orchestra members' `accepts`/`produces`
signatures. In design mode it uses that knowledge to propose sensible
defaults:

- **`accepts` vocabulary** biased toward forecasting: `forecast-question`,
  `evidence-set`, `driver-decomposition`, `base-rate-request`,
  `multiplier-suggestion`. Falls back to freeform when the agent's
  domain isn't forecasting-adjacent (Fermi can still design a general
  ABW agent, just less opinionated).
- **`produces` vocabulary** similarly biased: `p5-p50-p95-multiplier`,
  `base-rate`, `evidence-with-source`, `probability-estimate`.
- **`dependencies.optional`** pre-populated with the orchestra
  members that would compose well with the proposed agent.
- **`sample_queries`** phrased in the Tetlock idiom (specific
  entity + specific metric + specific timeframe + output format).
- **`valence`** biased toward `analytical` + evidence-driven traits
  unless the operator explicitly steers otherwise.

None of this is Fermi-specific *plumbing* — it's Fermi's system-prompt
knowledge showing up in defaults, exactly what ABW's `agent_designer`
concept describes.

## Part 3 — Chat persistence + session model

### Server-side

Chat history persists on the server so `Ctrl+;` on a different
device or after a restart continues the conversation. ABW likely
already has a chat table for xamanEK conversations; the Fermi
console reuses it (or requests a new one keyed on `(user_id,
agent_id="fermi")`).

Contract:

```
POST /api/agents/fermi/messages   { text, context_envelope }
    → { assistant_message, tool_calls[], created_agent? }
GET  /api/agents/fermi/messages?limit=50  → last N turns
```

If ABW's existing chat endpoint doesn't key on `agent_id`, we
propose a minor extension. Otherwise we reuse verbatim.

### Client-side

`CockpitState` / `FermiConsole` gains:

```rust
pub struct FermiChatState {
    pub messages: Vec<ChatMessage>,   // ordered oldest→newest
    pub input: Entity<TextInput>,
    pub loading: bool,                // waiting on Fermi's response
    pub drawer_open: bool,            // Ctrl+; toggle
    pub design_mode: Option<AgentDraft>,  // running card during Part 2 flow
    pub session_id: Option<String>,   // server-side session key
}
```

`FermiChatState` is a peer of `CockpitState` on `FermiConsole` (not
inside the cockpit — chat spans surfaces).

### Message shape

```rust
pub enum ChatRole { User, Assistant, Tool }

pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    pub tool_call: Option<ToolCall>,       // when role=Assistant fires a tool
    pub tool_result: Option<JsonValue>,    // when role=Tool
    pub created_at: DateTime<Utc>,
    pub design_step: Option<u8>,           // 1-9 when in design mode
}
```

The `design_step` field lets the UI render a progress indicator
during Part 2 without introducing a parallel "conversation kind"
concept.

## Data plumbing — options

Three routes to make chat work; parallel to the SCENARIO_TREE options
so the pattern is familiar.

| | **A. Reuse ABW chat endpoint** | **B. New Fermi chat endpoint** | **C. Console-only, in-memory** |
| --- | --- | --- | --- |
| Server change | none if endpoint already keys on `agent_id`; small extension otherwise | new `/api/agents/fermi/messages` handler | none |
| Where history lives | ABW `chat_messages` table | new Fermi-scoped table | RAM, lost on close |
| Cross-device continuity | yes | yes | no |
| Tool dispatch (open forecast, run sim) | yes — MCP path already there | yes | yes |
| Effort | ~2 hours (client + wire) | ~1 day (server + client) | ~half a day (client only) |

**Recommendation: A**, with a fallback to C for the initial slice if
A requires a backend change that hasn't landed yet. Chat is UX-cheap
in RAM for demoing; persistence can slot in without changing the
front-end shape.

## Non-goals

- **Not** a general-purpose LLM chat. Fermi refuses to answer
  questions outside forecasting / agent design / console navigation
  — same guardrail Xaman-Ek has for ABW-adjacent topics.
- **Not** a replacement for the composer's driver → agent assignment
  UI. Chat is *another* way to compose; the click-through affordances
  stay.
- **Not** a place to design compositions. Compositions are ABW's
  concept; if we want a Fermi-native "orchestra composition" surface,
  it's a separate design (that spec would live alongside this one).
- **No new agent tiers.** Fermi-designed agents land as `community`.
- **No streaming responses in Slice 1.** Non-streaming chat is
  enough for the initial launch; streaming can come in a later slice.
- **No voice.** Text only. Voice is an ABW-level concern.

## Implementation slices

Each slice is independently valuable and each ships something the
operator can actually use.

### Slice 1 — Static chat drawer (RAM only) [~1 day]

- `FermiChatState` on `FermiConsole`.
- Right-edge slide-in drawer, `Ctrl+;` toggle, sidebar footer chip.
- Send-message flow: POST to `execute_agent` for `agent_id="fermi"`
  (this endpoint already works — the console already calls it for
  orchestra members). Response renders in the drawer.
- Context envelope built from `CockpitState` + `FermiConsole` on
  every send.
- **No persistence** — RAM only. Refresh loses history.
- **No tool dispatch** — Fermi's replies are text only in this
  slice. Console-scoped MCP tools come in Slice 2.

Demoable as: "operator hits Ctrl+;, asks Fermi a question about the
open forecast, gets a text answer that references the actual state."

### Slice 2 — Tool dispatch [~2 days]

- Add the console-scoped MCP tools to Fermi's `agent_card.json`
  (open_forecast, open_portfolio, run_simulation, assign_agent,
  set_base_rate, link_polymarket).
- Console dispatches Fermi's `tool_call` responses to the
  corresponding CockpitState / FermiConsole methods.
- Each tool call renders as a compact chip in the chat pane:
  `⚡ opening forecast a3b7…` with a click-to-cancel affordance.

Demoable as: "Fermi opens the Arsenal forecast and runs a simulation
because I asked it to."

### Slice 3 — Chat persistence [~1 day]

- Wire to ABW's existing chat endpoint (route A) — if it keys on
  `agent_id`, no server work. Otherwise minor extension.
- Session id stored in FermiChatState + rehydrated on console
  startup.

Demoable as: "close the console, reopen, Fermi's still where we
left off."

### Slice 4 — Design mode (create-agent walkthrough) [~2–3 days]

- Fermi's system prompt gains a design-mode segment activated by
  the phrase "design a new agent" / an explicit `AgentDraft` field
  on the envelope.
- Nine-step conversational walk over DESIGN_CHECKLIST fields.
- Chat pane renders the running `agent_card.json` preview beside
  the message stream (split view).
- On confirm, POST to `ApiClient::create_agent(card)` (new SDK
  method). Backend either accepts the card into `agents` table or
  writes it to disk per the deployment model.

Demoable as: "in five minutes of chat, produce a valid ABW community
agent that the marketplace picks up on next refresh."

### Slice 5 (future) — Streaming responses, richer previews

Turns each Fermi reply into a token-streamed message; adds
inline forecast previews (e.g. showing a mini trajectory chart
inside a Fermi answer). Nice-to-have; not required for parity with
Xaman-Ek's current level.

## Open questions

1. **Does ABW's chat endpoint already scope by `agent_id`?** If yes,
   Slice 3 is trivial. If no, we need a small backend extension.
   Answer determines Slice 3 effort.
2. **Where's the agent-create backend route?** The ABW `agents`
   table exists; the console currently only lists agents
   (`list_agents`). If a create endpoint exists, Slice 4 uses it
   verbatim. If not, we add one — additive, no schema changes.
3. **What's the drift-threshold default for a Fermi-designed
   agent?** DESIGN_CHECKLIST proposes `0.35` for "rapidly evolving
   community agents." Fermi should propose `0.35` by default for
   the agents it designs, since they'll evolve based on operator
   feedback in early days.
4. **Do we ship a design-mode "confirmation before persist"
   step?** DESIGN_CHECKLIST is nine steps but committing to disk /
   the server is a tenth action. We add an explicit "review and
   confirm" screen showing the full card as a JSON code block —
   the operator can copy-edit before final commit. Recommended.
5. **Where do compound agents fit?** DESIGN_CHECKLIST §7 covers
   them but they're a separate concept from atomic agents. Slice
   4 handles atomic only; compound agents get a follow-up slice.

## Files this design implies (when we build)

**Slice 1:**
- `crates/fermi-console/src/chat.rs` (new) — `FermiChatState`, drawer
  render, send flow.
- `crates/fermi-console/src/main.rs` — chip in sidebar footer,
  `Ctrl+;` binding, drawer container.

**Slice 2:**
- `agents/curated/fermi/agent_card.json` — extend
  `capabilities.mcp_tools` with the console-scoped tools.
- `crates/fermi-console/src/chat.rs` — tool-dispatch handling.

**Slice 3:**
- `crates/fermi-console/src/api/client.rs` — chat message SDK
  methods (send, list).
- Possibly `src/handlers/chat.rs` — if we need an agent-scoped
  chat handler.

**Slice 4:**
- `crates/fermi-console/src/chat.rs` — `AgentDraft` state, design-mode
  UI (split view).
- `crates/fermi-console/src/api/client.rs` — `create_agent(card)`
  SDK method.
- Possibly `src/handlers/agents.rs` — if a create endpoint doesn't
  exist yet.

## Reference

- `docs/AGENT_MODEL.md` §3.3, §3.4, §7 — the platform's own framing of
  xamanEK-as-meta-agent and create-flow-as-agent-home.
- `agents/templates/DESIGN_CHECKLIST.md` — the nine-step walk Fermi
  follows verbatim in design mode.
- `agents/templates/agent_card.json` — the schema Fermi emits.
- `agents/curated/xaman_ek/agent_card.json` — the ABW peer.
- `agents/curated/fermi/agent_card.json` — Fermi's own card, the
  base Fermi extends with design-mode segment + console-scoped MCP
  tools.
- `SCENARIO_TREE_DESIGN.md` — sibling design doc that established the
  UI-vocabulary vs internal-name pattern this doc reuses.
