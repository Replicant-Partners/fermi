# Fermi Console v0.8.6 — Dashboard as command center

The Dashboard has been rewritten around the three pillars of Fermi:
a **self-improving agentic research team**, built for **probabilistic
forecasting**, powered by an **expanding marketplace of specialist
agents**. The old "book view" (Live / Drafts / Recently Resolved lists
stacked below stats) has moved to the Portfolio panel; the Dashboard
now tells the operator, at first glance:

- what research their agents are doing
- what it's costing
- what new agents are available to hire
- which teams they're part of
- what's been happening across their book

## What's new

### Command-center dashboard

Below the stats row the Dashboard now shows:

1. **Research** (left) — 7-day evidence and estimated spend, plus the
   most recent forecasts your agents worked on. Estimated cost is
   `sum(avg_cost_per_run × 1) over agents_used` for each forecast; the
   number is labeled "est." because ABW does not yet expose a
   per-forecast cost rollup.
2. **Marketplace** (right) — top Fresh and Rising agents, sorted by
   score. Click any card to jump to the Agent Fleet with the card
   pre-expanded for a Hire decision.
3. **Teams** — a horizontal strip of your teams. Click a card to jump
   to the Teams panel with that team selected. Roster is deferred to
   the Teams panel (single click away) rather than fanned-out from the
   Dashboard.
4. **Recent Activity** — same feed as before, now full-width with
   source-filter chips (`All / Mine / Team / Marketplace`). Team and
   Marketplace chips are disabled placeholders — the ingestion path
   for those event streams isn't in place yet.

### Portfolio panel picks up the book view

`VirtualPortfolio` gained three new buckets: `Live`, `Drafts`,
`Recently Resolved`. All three are client-side filters over data
`fetch_forecasts` already loads — no new round-trips. They render
before the existing `Shared with me` and `Unassigned` buckets in the
Portfolio sidebar so the operator's own book is the most prominent
thing on the panel.

### Marketplace shows server-only agents

`build_agent_marketplace` was iterating only over locally-installed
agent cards, using server cards purely to annotate them with
`execution_stats`. As a result, truly new community agents (the ones
you don't have on disk yet) were invisible from the Fresh tier. The
function now iterates the **union** of local + server agent IDs; local
data wins where present, otherwise it falls back to the server card's
fields (description, tags, version, author, model, accepts, produces,
requires_secrets). The Fresh tier is finally populated with real
discovery candidates.

### Teams fetched eagerly

`fetch_all_data` now calls `fetch_teams` alongside its other cold-load
fetches. Previously teams were pulled lazily on Panel::Teams open or
when a share modal opened. The Dashboard's Teams strip wants them on
first render.

## Migration

None. All changes are UI-layer. `VirtualPortfolio` gained variants but
persistence isn't touched. Bookmarked links still resolve; the
Dashboard's stat cards and hero row are unchanged.

## Known follow-ups

- **F1 — Real per-forecast cost rollup.** The Research card's cost
  number is an estimate. When ABW exposes a per-forecast agent-run
  aggregation (or `/api/me/executions?since=…`), swap the estimator
  for actual `credits_charged` sums. The card's visual shell won't
  change.
- **F2 — Team-colour dot on forecast rows.** A colored dot per row
  in Recent Activity + Portfolio indicating which team a forecast is
  shared with. Needs an eager share fan-out (`/api/forecasts/:id/shares`
  per own forecast) or a new `/api/me/shares` endpoint.
- **F3 — Team roster hover-reveal.** The Dashboard's Team cards show
  name + initial glyph. Rich roster (member initials, activity
  indicator) needs `get_team(id)` fan-out per team; deferred.
- **F4 — Team + Marketplace activity ingestion.** The disabled source
  chips on the activity feed are placeholders. Real content requires
  ingesting team-member events and marketplace publish events.
- **F5 — Composer PM affordance unification** (carried over from
  v0.8.4 known issues; still open).

## Files touched

- `crates/fermi-console/src/main.rs` — most changes; new
  `render_dashboard_research_card`, `render_dashboard_marketplace_card`,
  `render_dashboard_teams_strip`, `render_dashboard_team_card`,
  `render_dashboard_activity_feed` methods; `VirtualPortfolio` enum
  extended; `build_agent_marketplace` refactored to iterate union.
- `crates/fermi-console/Cargo.toml` — version bump.
