# Fermi Console v0.8.7 — Team-color dots everywhere

Small, focused release. Companion to v0.8.6's Dashboard rewrite:
addresses the "shared forecasts easy to associate with teams" ask from
the same v0.8.6 planning conversation.

## What's new

### Team-color dots on forecast rows

Every forecast row in the Portfolio panel (and every activity item in
the Dashboard's Recent Activity ticker) now shows a small colored dot
+ label indicating the forecast's primary team association. The color
comes from a **deterministic hash** of the team_id into a 7-color
theme palette, so the same team is always the same color everywhere
in the console.

**Primary team** is resolved in this order:

1. The forecast's owning `team_id` (Spec 24 §3.5.6), when set.
2. Otherwise, the first team-share found via `object_shares` — loaded
   lazily by a background fan-out (see below).
3. Otherwise, no dot renders.

The dot pattern is: `● Macro Desk` — 8px filled circle followed by a
tiny team name (truncated). No tooltip needed; the label is the
tooltip.

### Team cards match the dot colors

The Dashboard's Teams strip now colors each team's initial badge and
hover-border in the team's own color. Reading the Dashboard becomes:

> "The amber card at the top is Macro Desk. Those three forecasts
> with the amber dot? Also Macro Desk."

Consistent color mapping is the entire point — the operator learns
one thing (color → team) and then reads it everywhere without
thinking.

### Background team-shares cache

New `forecast_team_shares` cache on the console + a
`refresh_forecast_shares_cache` fan-out that runs after every
`fetch_forecasts`:

- Walks own forecasts (active + draft + resolved) where
  `share_count > 0`. Zero-share forecasts are skipped entirely — no
  wasted round-trips.
- Fires one `GET /api/forecasts/:id/shares` per eligible forecast in
  parallel. Real users have O(10) shared forecasts, not O(1000).
- De-dupes via `forecast_shares_in_flight` so a second
  `fetch_forecasts` during an in-progress refresh doesn't stampede.
- Failures are logged and dropped — one 403/500 doesn't stall the
  dot rendering for the rest of the book.

The cache is soft state — cleared implicitly on process restart, no
persistence. Refetched after any forecast list refresh.

## Migration

None. Zero data model changes. All fields are additive on the
FermiConsole struct.

## Known follow-ups

- **Multi-team dot stacks.** A forecast can be shared with multiple
  teams. Today we show only the primary. Consider a 2-3 dot mini-stack
  when a forecast has multiple team associations.
- **Dot in the composer.** The composer's portfolio-membership strip
  already shows which portfolios a forecast belongs to; the same
  visual language could apply to teams. Cheap addition.
- **Legend row in the Teams strip.** Right now the color → team
  mapping is discovered by reading the strip. A dedicated legend
  ("Colors used on your forecasts: ● Macro Desk  ● Sports") might
  help first-time operators. Consider after user feedback.
- **Real per-forecast cost rollup** (F1 from v0.8.6) — still open.

## Files touched

- `crates/fermi-console/src/main.rs` — new `forecast_team_shares` /
  `forecast_shares_in_flight` fields, `refresh_forecast_shares_cache`,
  `primary_team_id_for_forecast`, `team_name_by_id`, `find_own_forecast`,
  `team_color`, `render_team_dot`. Wired into `render_forecast_row`,
  `render_activity_item`, `render_dashboard_team_card`.
- `crates/fermi-console/Cargo.toml` — version bump.
