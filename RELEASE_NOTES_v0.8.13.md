# Fermi Console v0.8.13 — Publish gate, error banner, agent-registry fallback, orchestrate logging

Patch release. Four fixes together make the console usable when the
shared Anthropic account is depleted (or when a user's binary can't
find `agents/curated/`) — the exact state Mario (mo@axolotl.partners)
hit on his EPL forecasts.

None of this fixes the *root* problem of the shared Anthropic account
being out of credits — top that up and everything works again. But
the console now:

- Doesn't lie about *why* Publish is disabled.
- Actually lets the operator publish when they've done partial work.
- Shows the operator-facing error message instead of nested JSON.
- Falls back to server-side agent list when the local filesystem
  registry is empty.
- Logs enough at the `Ctrl+Enter` path to trace what actually fires.

## What changed

### Fix 1 — loosened publish gate + visible reason

**Before:** `pub_disabled = locked || !has_question || sim_results.is_none()`.
Meant that when Fermi's LLM decomposition errored (shared Anthropic
account empty), `sim_results` never populated and the Publish chip
sat grayed out with **no explanation**. Mario could save v3, v4, v5
and never publish.

**After:** allow Publish when *any* of the following:

- `sim_results.is_some()` (unchanged — the ideal case)
- The forecast has a base rate set on the question node
- At least one driver has a distribution with a non-zero anchor OR
  a binary probability greater than zero

Also renders an **inline "— disabled: <reason>"** hint right next to
the Publish chip when the gate refuses. Three possible reasons:

- `"forecast is resolved"` — locked forecast
- `"add a question first"` — empty question
- `"simulate (⌘R), set a base rate, or set at least one driver"` —
  no publishable content yet

No more silent-refusal-on-click.

### Fix 2 — un-truncate the Anthropic error banner

**Before:** the error surfaced as
`"API error: {"type":"error","error":{"type":"error","error":{"ty…"`
— nested JSON truncated at ~140 chars, obscuring the actionable
message.

**After:** new `extract_anthropic_error_message` helper scans the raw
error text for `"message":"..."`, handles escaped quotes, and pulls
out just the human sentence. Same code path also `log::error!` the
full untruncated string so anyone piping stderr to a log file
(`fermi-console 2>&1 | tee run.log`) sees the exact payload.

For the current outage the banner now reads:
`⚠ Agent 'fermi' failed: Your credit balance is too low to access
the Anthropic API. Please go to Plans & Billing to upgrade or
purchase credits.`

Instead of the JSON wall.

**Tests:** 4 unit tests in `cockpit::extractor_tests` cover:
- nested wrapping (the real Anthropic case)
- absence of `message` field
- escaped `\"` inside the message
- truncated input where the outer wrapper cut off mid-JSON

### Fix 3 — server-backed agent registry fallback

**Before:** the composer's agent picker reads `state.registry.list_cards()`
— a **local filesystem** registry populated from `agents/curated/` at
startup. When the operator's binary can't find that folder (packaged
distribution, unexpected CWD, missing `$AGENTS_DIR`), the picker
shows `"No research agents found in registry."` even though the
server has the full list at `GET /api/agents?tag=fermi-orchestra`.

**After:** three new fields on `CockpitState`:

- `server_agent_cards: Vec<JsonValue>`
- `server_agent_cards_loading: bool`
- `server_agent_cards_fetched: bool`

Plus `load_server_agent_cards()` method that fetches
`/api/agents?tag=fermi-orchestra&limit=200` once per session.

The picker (`render_agent_picker`) builds `available_agents` from the
local registry first, and **falls through to `state.server_agent_cards`
when the local list is empty**. `open_agent_picker` triggers the
lazy fetch when it detects the local registry has zero
fermi-orchestra tags.

Execution paths are unchanged — the SSE stream endpoint
(`/api/agents/:id/execute/stream`) resolves cards server-side, so the
picker only needs the local `AgentCard` struct for the chip label,
which the server-cached rows can supply.

**Consequence:** Mario now sees the full agent list regardless of where
his binary is running from. The `"No research agents found in
registry."` empty state now only shows when *both* the local and
server lists are empty.

### Fix 4 — logging on the `orchestrate_question` path

Added two `log::info!` calls in `orchestrate_question`:

- On entry: question length, current driver count, forecast_id.
- Between domain detection and program replacement: detected domain,
  generated driver count, driver names.

Also `log::warn!` on the empty-question short-circuit.

Grep marker `[orchestrate]` so `fermi-console 2>&1 | tee run.log` +
`grep '\[orchestrate\]' run.log` traces the Ctrl+Enter path
end-to-end. This is diagnostic infrastructure for the still-open
mystery of why Mario's Man-United forecast has one degenerate
`Driver 1` at zeros instead of the three-driver template
`orchestrate_question` should generate.

## What this does NOT fix

- **The shared Anthropic account being out of credits.** That's a
  billing action, not a code change. Top up the ABW server's
  Anthropic account and every user's agent execution starts working
  again.
- **Per-user API keys.** The `ToolContext.user_secrets` field
  (`src/handlers/execution.rs:118`) is still hardcoded `None`.
  Building the multi-tenant secrets flow is a separate feature
  (documented in the handoff doc's "known follow-ups").
- **Why `Driver 1` ended up on Mario's forecast.** The added
  `[orchestrate]` logging is instrumentation, not a fix. Next time
  Mario reproduces the bug, `grep '\[orchestrate\]' run.log` should
  say whether the method was reached, what domain was detected, and
  how many drivers were generated — enough signal to close the loop.

## Migration

None. All fixes are additive on client fields + a new client-side
GET request path. Server contract unchanged. No schema drift.

## Files touched

- `crates/fermi-console/src/cockpit.rs`:
  - `mark_agent_failed`: uses `extract_anthropic_error_message`
    and logs full raw error.
  - `extract_anthropic_error_message` fn + 4 unit tests in
    `extractor_tests` module.
  - `CockpitState`: three new fields (`server_agent_cards`,
    `server_agent_cards_loading`, `server_agent_cards_fetched`)
    + initializer.
  - `load_server_agent_cards` method.
  - `open_agent_picker`: kicks the lazy fetch when local
    registry is empty.
  - `render_agent_picker`: falls through to server cache when
    local `available_agents` is empty.
  - `render_action_bar`: new gate `has_publishable_content`,
    `pub_disabled_reason` string, inline "— disabled" hint
    following the Publish chip.
  - `orchestrate_question`: two `log::info!` entry-points.
- `crates/fermi-console/Cargo.toml`: version bump.

## What Mario should try next

1. Update his console to v0.8.13.
2. On the Man-United forecast, press `Ctrl+Enter` again.
3. If Publish is still grayed, the chip should now show
   *"— disabled: simulate (⌘R), set a base rate, or set at least one
   driver"*. Hit `Ctrl+R` to simulate — the current default drivers
   will produce a sim result and Publish should light up.
4. If the agent picker still shows `"No research agents found…"`,
   check the log for `[registry]` to see if the server hydrate
   succeeded.
5. If the Fermi banner appears with the credit-balance message —
   that's the shared account. No client fix, needs the billing
   action.
