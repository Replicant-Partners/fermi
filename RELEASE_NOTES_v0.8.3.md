## What's new

- **Inline portfolio membership in the Composer.** A new "IN PORTFOLIOS:" chip strip appears right under the question field, listing every portfolio you own. Click a chip to add or remove the current forecast — no more Portfolio-panel round-trip. Shows a "publish (Ctrl+P) to enable" hint while the forecast is still a draft.

- **Recent Activity is now clickable.** The Dashboard's recent-activity feed rows carry the forecast id and route to `open_forecast` on click. Hover state indicates the row is actionable.

- **Publish → Dashboard updates immediately.** Publishing a forecast now signals the parent to refetch the active/resolved/draft lists on the next tick, instead of waiting up to 30 seconds for the background refresh loop. The forecast appears in the Dashboard's Live section (and Recent Activity) within a second of publish.

- **Schedules tab shows on-demand assignments.** Previously the tab was empty unless the FPL declared a recurring `Schedule::Every` block, which meant agents hired via the marketplace or the driver editor's +Assign flow never surfaced. Now the empty state enumerates every AST agent assigned to a driver and offers **▶ Run now / 📅 Daily / 📅 Weekly** buttons per row — one click promotes an on-demand assignment to a persisted recurring schedule.

## Fixes

- **Driver editor labels no longer wrap character-by-character.** The `Name`, `Rationale`, `p5 (low)`, `p50 (mid)`, `p95 (high)`, and `Unit` labels in the driver editor were breaking into vertical single-character stacks when the right panel was narrow. The label element now sets `flex_none + overflow_hidden + whitespace_nowrap` — labels truncate gracefully at the column edge instead of shredding. The field inputs themselves are unaffected.

## Known issues

- The +Assign Agent flow still creates `Schedule::Once` bindings by default. The new Schedules tab UX (upgrade to Daily/Weekly per row) is the primary way to make an assignment recurring; a follow-up will surface Daily/Weekly directly in the +Assign chooser so the operator can pick a cadence at hire time.
- The composer's portfolio chip strip lists every portfolio you own, without paging. If you have >20 portfolios the strip will wrap onto multiple lines; a search/filter row is queued for a follow-up.

## Breaking changes

None.

## Upgrade notes

Just Update & Restart. No data migration needed.
