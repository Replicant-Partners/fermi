## What's new

- **Dashboard action bar.** Three prominent hero buttons — **＋ New forecast**, **🔮 From Polymarket**, **📎 Paste PM URL** — sit above the stats cards so testers can start work without knowing the shortcuts. The Polymarket buttons open the search card **in-place on the Dashboard** instead of punting you over to Portfolio.

- **Keyboard shortcuts help modal.** Press **Ctrl+/** any time to open a categorized list of every hotkey (Forecast workflow, Navigation, Window, Help). Three redundant entry points so it's impossible to miss: the shortcut itself, a sidebar chip (**⌨ Shortcuts · Ctrl+/**) next to the version/wallet chips, and **Help → Keyboard Shortcuts**. Escape or backdrop-click dismisses.

- **Drafts and resolved forecasts are visible on the Dashboard.** Two new sections — **Drafts** (unpublished WIP, click to open in Composer) and **Recently Resolved** (last 10, Brier-sorted) — appear below Live. Previously, forecasts not in any named portfolio were orphaned in the UI: they weren't in the Portfolio panel (that's for named portfolios only) and the Dashboard only surfaced actives. Now the Dashboard is a full home for every forecast the operator owns.

- **Recent Activity is actually useful now.** Two changes: same-family resolutions collapse into a single summary row (a wall of "Resolved No: Will X win the 2026 FIFA World Cup" now reads "Resolved 8× 2026 fifa world cup — all No"), freeing seven slots for higher-signal content. And trivial-Brier resolutions (< 0.05) are demoted below drafts / probability revisions / notable calls, only showing when the feed would otherwise be empty. Perfect-Brier long-shot calls are no longer noise.

- **Pre-publish portfolio picker.** The composer's portfolio chip strip is now interactive **while the forecast is still a draft**. Clicking a chip queues a pending selection (dashed gold border) which gets applied automatically at publish. Header text changes to "ADD TO PORTFOLIOS ON PUBLISH:" and the hint reads "selections apply on publish (Ctrl+P)". No more publish-then-remember-to-add friction.

- **Sidebar version chip is clickable.** Three states: **⬆ Update to vX** (cyan, opens release notes), **vX — checking…** (dim, disabled), **vX — up to date** (green, re-runs the check with a verbose toast).

- **Drafts and probability revisions in Recent Activity.** New icons distinguish Draft (**✎**, gold), Published (**◐**, cyan), and Revised (**→**, cyan) from Resolved (**✓**, Brier-colored). Relative timestamps (`3m ago`) replace bare ISO dates.

## Fixes

- **Sidebar chip flexibility.** The version chip is now always clickable and communicates whether the update check has run yet, so the operator never has to guess whether they're on the latest build.

## Known issues

- The Composer's older Polymarket type-ahead strip still sits under the question field. It works, but the Dashboard's new in-place PM search card is the recommended flow — a follow-up will unify the two so both entry points render the same card.

## Breaking changes

None.

## Upgrade notes

Just Update & Restart. No data migration needed.
