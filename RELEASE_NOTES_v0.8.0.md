## What's new

- **Invite anyone with a link.** Team, portfolio, and forecast invite dialogs now pop a modal with the shareable URL the moment you send the invite. Copy it to Slack, WhatsApp, or paste it into email yourself — no waiting on server-side email delivery. New testers landing on the invite page see Google/GitHub sign-in buttons up front and return to the invite one click later.

- **Dashboard PM sync chip.** The `⚡ Sync N PM markets` chip on the Dashboard header shows how many active forecasts are linked to Polymarket and triggers a resolution check without leaving the panel.

- **Portfolio Risk view.** Every portfolio detail now includes six inline risk metrics: concentration (HHI), independent P(any yes), correlated P(any yes) with a ρ slider (−0.5 / 0 / 0.3 / 0.6 / 0.9), expected book Brier (1000-sample Monte Carlo), drawdown-if-your-biggest-edge-is-wrong, and a top-4 joint outcome tree. All computed client-side from the forecasts you already have loaded — no extra round-trips.

- **Cascades UX.** The `⛓ RELATIONSHIPS` sub-panel lists every declared cascade rule that touches this portfolio (mutex, implies, at-most-n, conjunction, conditional, exhaustive cover). "+ Declare" opens an inline sheet: pick a kind, tick the forecasts, set `n` if applicable, add a description, submit. Rules render with kind badge, a preview of the forecasts they touch, and Remove.

- **Agent Fleet as ranked marketplace.** The Agent Fleet tab now leads with a marketplace: agents ranked by a transparent score combining success rate, cost per run, popularity, and this-session contribution. Filter by tier (Popular / Established / Rising / Fresh) or sort by cheapest / most reliable / most used / best contribution. Each card shows the score, cost per run color-graded from green ($≤0.05) to orange, success rate, tag pills, and an Assign button.

- **Portfolio Constellation drill-down.** Portfolio rows are compact by default; click the ▸ chevron to expand a per-row drill-down with the full question, tags, Brier if resolved, and explicit Open / Remove actions. Six quick-filter chips (active / hot / linked / edge / shared / resolved) with AND semantics compose with the free-text filter.

- **Trajectory chart polish.** Right-margin legend column (you / crowd / base) with de-collision, faint horizontal gridlines, and colorblind-safe shape channel on event markers (circle = rate revision, diamond = BayesOps refit, ring = market observation, tick = agent run). The event log below is now grouped into HistoryFlow phases, each headed by its rate revision with an auto-generated summary of the activity that led there.

## Fixes

- **Base rate no longer silently reverts.** Fixed the "click Update base rate, see the new %, leave the panel, come back, see the old %" bug. Every code path that mutates the base rate (question decomposition, ⟳ Update, Polymarket anchor) now PATCHes `metadata.base_rate` to the server, and hydration on open unconditionally prefers metadata over the FPL template's initial anchor. Forecasts whose base rate was set before this version get an opportunistic backfill on next open — no manual action required.

- **Invite copy-link chip in forecast/team invite lists no longer hardcodes the production URL** — reads the live API base URL instead, so localhost and staging deployments produce links that actually resolve.

- **Trajectory event log inline labels don't collide with the worm** anymore when crowd/base/model values cluster together — the right-margin legend column takes over.

- **Removed the orphan `cockpit_old.rs` module** and cleaned up stale eval-brier / execution.rs compile errors that surfaced under stricter linting.

## Known issues

- The "Assign to a driver…" button on marketplace cards navigates to the Composer with a hint toast but doesn't fully automate the driver-assignment picker yet — you still need to click a driver's "+ Assign Agent" affordance. The full one-click hire flow lands in a follow-up.
- The forecast-outcome-contribution signal in the marketplace currently derives from session confidence only. A server-side per-agent Brier-delta contribution feed is planned but not shipped in this release.
- Portfolio row mini-worms show model-vs-crowd but not a time-series sparkline yet — the sparkline pass is queued behind a `/api/portfolios/:id/timeline` endpoint.
- P(any yes) at ρ is a linear-blend approximation between the independent value (ρ=0), max single event (ρ=+1), and Σp bound (ρ=−1). Correct at the limits, honest but rough in the middle — treat it as directional, not a precise joint probability.

## Breaking changes

None. Existing forecasts, invites, and portfolios keep working. Client is backward-compatible with previous server versions except for the two new client-side features that require a matching server (`metadata` on the update endpoint for base-rate persistence; `email_sent` + `invite_url` on invite creation for the just-created modal). Both features degrade gracefully — the client falls back to constructing invite URLs locally and treats `metadata` as optional.

## Upgrade notes

Just Update & Restart. On first open of a pre-existing forecast whose base rate was set locally in a previous session, expect a log line like `[base-rate-backfill] AST carries base_rate but server metadata is empty — persisting` — that's the migration self-healing, one-time per forecast.
