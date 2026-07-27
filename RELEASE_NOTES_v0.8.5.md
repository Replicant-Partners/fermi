## What's new

- **Ctrl+S now saves to the backend.** Previously, Save (Ctrl+S) only wrote a local `.fpl` file to disk and made a git commit — meaning your work was gone the moment you closed the composer or opened Fermi on a different machine. Save now also POSTs a private draft to the server on first save (so you get a `forecast_id` and can `Open in Cockpit` it later), then PUTs updates on every subsequent save. Never demotes a published forecast back to draft.

- **Autosave.** The composer now watches for edits — driver changes, new simulation results, Polymarket links, question keystrokes — and quietly persists to the backend once you've been idle for ~10 seconds. No modal, no interruption; you'll see `💾 Saved as draft to server (ID: …)` in the assistant messages panel the first time it fires on a new forecast. Cancels itself when nothing is dirty, so it doesn't hammer the API.

- **📥 Shared with me** and **📌 Unassigned** virtual portfolios. The Portfolio panel now pins two new entries at the top of its sidebar:
  - **Shared with me** — every forecast a teammate has shared with you (via team membership, direct share, or public visibility). Previously these were orphaned in the UX: the backend returned them but the console mixed them into your own lists with no way to filter.
  - **Unassigned** — forecasts you own that aren't in any named portfolio yet, including all the drafts autosave creates. Gives your loose work a discoverable home.
  Click a row → "→ Open in Cockpit" round-trips into the composer the same way named-portfolio rows do.

## Fixes

- **Forecast persistence is no longer local-first.** The single most common lost-work scenario — "I hit Ctrl+S, closed the composer, and now my forecast is gone" — is fixed. Draft state lives on the server from the moment you first save, and every subsequent edit checkpoints there too.

- **Shared forecasts are no longer orphaned.** If a teammate shared a forecast or portfolio with you but you had no named portfolio yet, that forecast was effectively invisible in the console. The new "Shared with me" bucket surfaces them regardless of portfolio membership.

## Known issues

- No "Autosaved 4s ago" indicator in the composer header yet. The timestamp is tracked internally (`last_autosave_at`), just not rendered. On the follow-up list.
- If a backend save fails (network hiccup), the composer surfaces a warning in the messages panel and the next autosave tick retries. There's no explicit "retry now" button — future work.
- On-close: navigating away from the composer while dirty doesn't force a synchronous save. Autosave will catch it within ~15s of your last edit, but if you close the whole app faster than that, the last few seconds of edits could still be lost. Belt-and-braces synchronous flush is queued.

## Breaking changes

None. The new `GET /api/forecasts?scope=…&unassigned=…` query params are additive — every existing client keeps working unchanged.

## Upgrade notes

Just **Update & Restart**. No data migration; the new endpoints and the composer's autosave loop start working immediately against your existing forecasts.
