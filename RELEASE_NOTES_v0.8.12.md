# Fermi Console v0.8.12 — Orphaned-forecast repair + build fix

Patch release. Fixes four bugs that together made pre-v0.8.10
Polymarket imports **impossible to repair from the UI** — the
operator hit these on two EPL forecasts (Chelsea and Man City) that
had been orphaned before v0.8.10's pm_link fix landed.

Also unbreaks the workspace build from a `log::` import that got
introduced without pulling in the `log` crate.

## Bugs fixed

### 1. `link_polymarket_market` didn't persist for existing forecasts

The v0.8.10 fix wired `pm_link` from `persist_backend_save` and
`publish_forecast` — but only on the `created == true` branch.
When an operator opened an orphaned forecast (created before
v0.8.10, or one whose `metadata.polymarket` never got persisted)
and clicked a Polymarket type-ahead match, the linkage set in
cockpit RAM, autosave fired with `created == false`, and `pm_link`
**never ran**. The chip strip's suggestion click became a silent
no-op — the operator could click all day and the linkage would
never reach the server.

Fix: `link_polymarket_market` now fires `pm_link` directly
whenever `self.forecast_id.is_some()`. Idempotent server-side
(`POST /api/polymarket/link` is an UPDATE). The create-path
`pm_link` in `persist_backend_save` / `publish_forecast` stays
because on first-create the `forecast_id` doesn't exist yet at
this call site.

### 2. `pm_typeahead_search` leaked `pm_suggestions_loading = true`

Line 1266–1268 (before the fix):

```rust
let still_current = this
    .update(cx, |state, _| state.pm_suggest_seq == captured_seq)
    .unwrap_or(false);
if !still_current {
    return;  // ← never resets pm_suggestions_loading
}
```

A stale callback that lost its seq race returned here without
flipping the loading flag off. Result: the type-ahead strip's
`🔮 SEARCHING POLYMARKET…` label stuck on with no recovery path —
the observer's next search early-returns on line 1244 if the
query hasn't changed, so the operator was stuck.

This was the root cause of Man City's stuck spinner in the
operator's screenshot.

Fix: the still-current check now runs inside `state.update` and
flips `pm_suggestions_loading = false` on the way out when the
seq check fails, followed by `cx.notify()` so the strip clears.

### 3. No UI to re-trigger PM search on a saved forecast

Once the observer-driven search has run once against a given
question, `pm_suggest_last_query` caches it and future calls
no-op unless the input text actually changes (line 1244). An
operator loading a saved forecast with no PM link had no way to
retry the search — the strip either showed stale empty results
or collapsed silently.

Fix: new action-bar chip `🔮 Search PM` visible only when
`forecast_id.is_some() && pm_event_id.is_none()`. Clicking it
calls `retry_pm_typeahead(cx)`, which resets
`pm_suggest_dismissed` / `pm_suggest_last_query` /
`pm_suggestions_loading` before firing a fresh search. Works
regardless of whether bug #2 previously left the loading flag
stuck.

### 4. No UI to delete a forecast

`ApiClient::delete_forecast` and
`DELETE /api/forecasts/:forecast_id` have existed for months;
the console just never wired a button. An orphaned forecast
was permanent from the UI's perspective.

Fix: new action-bar chip `🗑 Delete` with a two-click confirm
pattern (no modal dialog needed — matches the composer's
minimalist chip vocabulary):

- First click primes the chip: label becomes "Click again to
  confirm" and the border/text flip to red.
- Second click within 3 seconds fires
  `DELETE /api/forecasts/:id`, resets the composer to a clean
  state (drops `forecast_id`, `workspace_id`, cascade groups,
  timeline data, PM state, etc. — same pattern as
  `on_new_forecast`), and posts an assistant message with the
  deleted-forecast id.
- If the operator doesn't confirm within 3 seconds, the chip
  auto-disarms.

Only visible when `forecast_id.is_some()` — draft cockpits
don't need it (Ctrl+N handles the local reset).

## Build fix

### `log::` calls in `src/handlers/forecasts.rs` (unbreaks workspace)

Commit `70651ed` ("Backend + console: self-heal orphan users rows
on forecast write") added two `log::warn!` / `log::info!` calls to
`ensure_user_row` — but the top-level `fermi` crate doesn't depend
on `log` (only `fermi-console` does), so `cargo check --workspace`
started failing with `E0433: use of unresolved module or unlinked
crate log`.

Fix: switched both call sites to `tracing::warn!` / `tracing::info!`
with structured fields, matching the convention used everywhere
else in the file (`tracing::info!(user_id = %..., "...")`). Zero
runtime behavior change; the logs just route through the
established tracing subscriber.

## Migration

None. All fixes are additive on the client side + one bug fix in
the handler module.

## Files touched

- `src/handlers/forecasts.rs` — `log::` → `tracing::` in two
  call sites in `ensure_user_row`.
- `crates/fermi-console/src/cockpit.rs`:
  - `link_polymarket_market`: fires `pm_link` immediately when
    `forecast_id.is_some()`.
  - `pm_typeahead_search`: flips `pm_suggestions_loading = false`
    on the stale-callback early-return path.
  - New `retry_pm_typeahead(cx)` method.
  - New `arm_delete_forecast(cx)` + `confirm_delete_forecast(cx)`
    methods with the two-click confirm state.
  - New `delete_forecast_armed` field on `CockpitState` +
    initializer.
  - `render_action_bar`: two new conditional chips (`🔮 Search
    PM`, `🗑 Delete`) with the visibility rules above.
- `crates/fermi-console/Cargo.toml` — version bump.

## What this unblocks

The operator now has three complementary recovery paths for an
orphaned forecast:

1. **Re-associate**: click `🔮 Search PM`, wait for typeahead
   matches, click the right one — `link_polymarket_market` now
   persists the association server-side.
2. **Delete and re-import**: click `🗑 Delete` twice, then use
   Ctrl+O (Import) to bring the market back in fresh with a
   clean association.
3. **Manually keep the orphan**: nothing to do — the forecast
   still works without a PM link, just no crowd price / trajectory
   ticks.

## Known follow-ups

- **Route-registration smoke test** was proposed in the v0.8.11
  release notes to catch future GET-missing bugs at compile-time;
  still worth doing but not in this release.
- **Confirmation dialogs.** The two-click delete pattern is
  minimal but not undoable. A proper undo toast would be more
  forgiving; needs a general toast system in the console first.
