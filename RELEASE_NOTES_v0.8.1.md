## What's new

- **Marketplace cards now expand to a full agent profile.** Click the ▸ chevron on any Agent Fleet marketplace card to see the agent's IO contract (accepts → produces), model + temperature, version, author, sample queries, declared MCP tools, and skills. A "Open in ABW ↗" link takes you to the full agent page in the browser. Agents that need secrets get a ⚠ warning surfaced.

- **Three-tier Hire flow.** The Hire button on marketplace cards now opens a proper modal: pick the forecast → pick the driver (or hire ambient with no driver) → review terms + notes → Confirm. The stepper shows where you are; each step's selection auto-advances where sensible. Confirm currently drops a hint into the Composer and navigates you there for the final +Assign Agent step — full auto-assign wiring lands next.

- **"Cost low→high" / "Cost high→low"** sort chips replace the ambiguous "Cheapest" label. Both keep agents with no cost data at the bottom rather than jumping the queue at either end.

## Fixes

- **Score column no longer shows "0" for unrated agents.** Marketplace cards without any usage data now show an "unrated" pill instead of implying the agent scored zero. Same for cost and success cells — they're hidden entirely when there's no data behind them.

- **"No usage data yet — listed alphabetically" hint** now appears above the marketplace when nothing in the visible set has been run. Makes it obvious *why* sort chips aren't changing the order.

- **"No agents match the current filter" empty state** replaces silent blank space when the tier filter excludes everything.

## Known issues

- The Hire modal's Confirm step is scaffolding — it surfaces a hint and routes you to the Composer, but doesn't yet create the agent-to-driver binding on the server. Follow-up wiring touches the cockpit's `assign_agent_to_driver` and needs a server-side per-forecast agent binding endpoint.

- The driver picker in step 2 of the Hire modal only enumerates drivers when the picked forecast is the one currently open in the Composer. If you hire an agent for a different forecast, you can still proceed via ambient; the "switch cockpit to picked forecast, then show its drivers" flow is a follow-up.

- Terms section is a placeholder — will surface `fork_pricing` / `royalty` / `auto_collect_pct` per agent once ABW ships those fields at the API layer.

## Breaking changes

None.

## Upgrade notes

Just Update & Restart. No data migration needed.
