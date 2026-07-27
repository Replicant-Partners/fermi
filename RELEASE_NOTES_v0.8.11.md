# Fermi Console v0.8.11 — Cascade thread bug fixes + scenario design doc

Closes the cascade thread that started with v0.8.8 (Provenance) and
completed the compose-and-plan surface in v0.8.10. This is a patch
release: no new features, two backend integration bugs discovered
during live use of Slices A/B, plus the design doc for the next
scenario-tree slice and a full handoff document for continuity.

## Bugfixes

### `GET /api/forecasts/:id/groups` was never registered

The route in `src/api_server.rs` for `/api/forecasts/:forecast_id/groups`
was bound to **PUT only**. The GET handler
(`membership::get_forecast_groups_handler`) was implemented and
correct, but no route wired it up — so v0.8.9's
`load_forecast_cascade_groups` fired on every `open_forecast` and got
back `HTTP 405: Method Not Allowed`. The composer surfaced this
inline as **`HTTP 405:`** in the `CASCADES:` chip strip, right next
to the "not in any cascade group" label. Symptom was silent:
memberships never rendered on any live forecast, and there was no
error toast — only the small inline banner.

**Fix.** One-line change in `src/api_server.rs`: add
`.get(handlers::relationships::membership::get_forecast_groups_handler)`
next to the existing `.put(...)`. Handler unchanged.

**Regression class.** Slice A shipped without integration-testing the
GET path against a running server. Cascade shape-tests protect the
wire format but don't catch route-registration gaps. Follow-up: the
release checklist should run `cargo test --test 'route_smoke'` (once
we write one) that hits every handler through the router.

### Polymarket imports dropped the market link on save

Every Polymarket import silently lost its market link:

1. Operator imports a PM market — `import_polymarket_forecast` stores
   `pm_event_id` / `pm_market_id` / `pm_market_price` in
   `CockpitState` (RAM only).
2. Operator saves (Ctrl+S) or publishes (Ctrl+P) — the payload sent
   to `POST /api/forecasts` (or `PUT /api/forecasts/:id`) includes
   `question_text`, `predicted_probability`, `fpl_source`, resolution
   criteria, target date, CI, and sim results. **Nothing PM-related.**
3. Operator reloads the forecast later — server returns
   `metadata.polymarket = null` (never persisted). Cockpit's PM
   hydration block skips. `pm_event_id` stays `None`.
4. The question-input observer sees `pm_event_id.is_none()` on load
   and fires the typeahead search. Strip appears as
   **`🔍 SEARCHING POLYMARKET…`** and either sticks there (if the
   search hangs) or eventually completes into an empty list — either
   way, no PM panel, 0 PM ticks in the Trajectory, no crowd delta.

Root cause: the client SDK had `pm_search` and `pm_snapshot` but no
`pm_link` — so there was literally no way for the console to call
`POST /api/polymarket/link` (which has existed and worked for
months). The import path stored state in memory and the save path
never included it.

**Fix.** Three parts:

- `ApiClient::pm_link(forecast_id, pm_event_id, pm_market_id)` —
  new method in `crates/fermi-console/src/api/client.rs`, thin
  wrapper over the existing endpoint.
- `persist_backend_save` (Ctrl+S path) now fires `pm_link` on the
  create branch when the cockpit has PM state in memory.
  Fire-and-forget via `tokio::spawn`; success/failure logs a line so
  the operator can grep for `[save] pm_link` if their PM data
  vanishes.
- `publish_forecast` (Ctrl+P path) does the same on its create
  branch, so import → publish (skipping Ctrl+S) also persists the
  link.

Only fires on `created == true` — for existing forecasts we trust
the server-side link; re-linking on every autosave would waste
round-trips and could stomp a manual re-link.

**Existing broken forecasts** (imported before v0.8.11 and saved
without a link) still open without PM data. Re-import fixes them
one at a time. No batch backfill planned; if the number is large,
`UPDATE fermi_forecasts SET metadata = jsonb_set(...)` from a stable
per-forecast `pm_event_id` list is trivial.

## Documentation

### `docs/fermi/SCENARIO_TREE_DESIGN.md` — new

Design doc for the scenario-aware Portfolio Risk view (Slices 3-5).
Ships now so the vocabulary (**user-facing "scenario" ↔ internal
"cascade group"**) is settled before Slice 2 (cockpit UI vocabulary
sweep) or Slice 3 (marginal-constraint tree math) ship. Sections:

- Vocabulary mapping table (Scenario ↔ cascade_group ↔ relationship_group)
- Three correctness levels (independence / naive filter / **marginal
  constraints** — the correct math with worked EPL numbers)
- Data plumbing options (client fan-out / backend enrichment /
  **cached listing** chosen)
- Backend `member_ids` addition to `list_groups_handler`
- Client `scenarios_cache` design
- UX changes (`scenario-aware · N/16 valid` label swap; ρ slider
  stays as-is)
- 5 implementation slices with clear per-slice value
- 3 open questions for the next slice review

### `docs/fermi/CASCADE_THREAD_HANDOFF.md` — new

End-to-end handoff document for the cascade thread (v0.8.8 →
v0.8.11). Covers what shipped, what didn't, why, and where the next
session should pick up. Authoritative baseline for anyone (human or
ai) continuing the scenario-tree work.

## Migration

None. Backend addition is additive (existing PUT still works).
Client addition is a new method; nothing calls it besides the two
save-path hooks. No schema changes.

## Files touched

- `src/api_server.rs` — GET binding on `/api/forecasts/:forecast_id/groups`.
- `crates/fermi-console/src/api/client.rs` — new `pm_link` method.
- `crates/fermi-console/src/cockpit.rs` — fire `pm_link` from
  `persist_backend_save` and `publish_forecast` create branches
  (fire-and-forget `tokio::spawn`, warning log on failure).
- `crates/fermi-console/Cargo.toml` — version bump.
- `docs/fermi/SCENARIO_TREE_DESIGN.md` — new design doc.
- `docs/fermi/CASCADE_THREAD_HANDOFF.md` — new handoff doc.
