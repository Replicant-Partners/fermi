# v0.11.6 — Make the Brier loop real; remote MCP servers as an agent capability

Two independent features. The first fixes a calibration pipeline that was
silently corrupting its own inputs and had never run end-to-end. The
second lets an ABW agent consume tools from third-party MCP servers.

## 1. Brier scoring integrity (mig-174)

`brier_score` is computed against `predicted_probability` at resolution —
but that column stayed mutable afterwards, and **nine** server-side
writers update it without filtering on status:

```
apply_wc_cascades.rs      forecasts.rs (×2)
resim_wc.rs               bayesops.rs
relationships/{recompose,propagation,undo}.rs
workspace/refit.rs
```

**Observed damage.** All 47 Polymarket-resolved forecasts had
`predicted_probability` overwritten *after* `resolved_at` by the World Cup
cascade binary, which pinned eliminated forecasts to 0.001 and clamped
survivors to 0.999 — 91 post-resolution revisions. The stored pair
`(predicted_probability, brier_score)` became mutually inconsistent:
recomputing Brier from the table gave ~1e-6 for every row, while stored
scores ranged up to 0.195. The 0.001 write doubled as the cascade job's
idempotency marker, which is why it looked deliberate.

**Fix, in three layers:**

| Layer | What |
|---|---|
| `scored_probability` | The probability Brier was actually computed against, snapshotted at resolution. The audit anchor — Brier stays reproducible no matter what later moves the live value. |
| `resolution_source` | Structured provenance, with a `CHECK`. |
| BEFORE UPDATE trigger | Freezes the scoring tuple once resolved. The next writer that forgets a status filter gets rejected by the database instead of trusted. |

The third layer is the one that matters: the first two are corrections,
the trigger is what stops the tenth writer from reintroducing the bug.

## 2. Loop 5 — the trajectory calibration columns

`forecast_spacetime` (mig-140) declared four columns that nothing ever
wrote: `brier_at_this_point`, `loop5_calibration`, `loop1_signal`,
`loop3_coherence`. `GET /api/forecasts/:id/spacetime` returned `null` for
all of them, always — so the "RSI proof data" the table exists for did not
exist.

`backfill_spacetime_calibration` now fills the two that ground truth makes
computable, at resolution time:

* **`brier_at_this_point`** — what the Brier *would* have been had the
  forecast resolved at that revision. This is the whole point of a
  trajectory: it shows whether successive revisions moved toward or away
  from the truth. Scoped to revisions at or before resolution, because
  scoring a post-resolution revision reads as a perfect call.
* **`loop5_calibration`** — a snapshot of contributing agents' calibration
  *at scoring time*, so a later reader isn't comparing against today's
  numbers.

`loop1_signal` and `loop3_coherence` are deliberately left `NULL`: they
aren't derivable from resolution, and inventing values would be worse than
absent. Migration 175 backfills history.

**Also: nothing was scheduling resolution.** A Polymarket-linked forecast
only resolved when an operator clicked "check resolutions" in the console.
Markets settled, forecasts stayed `active`, no Brier was computed, Loop 5
stayed cold. `spawn_resolution_sweeper` now drives it — paced and bounded,
disable with `PM_RESOLUTION_SWEEP_SECS=0`.

## 3. Remote MCP servers as an agent capability

`src/agent_backend/mcp_client.rs` (new) lets an agent consume tools from
third-party MCP servers, with `agents.mcp_servers` as the source of truth
for that config.

Migration 177 clears legacy `mcp_tools` data out of that column: the old
create path wrote the wrong field — harmless while nothing read it,
actively wrong now that `resolve_agent_card` treats it as authoritative.

Design: `docs/architecture/REMOTE_MCP_CLIENT.md`.

## Migrations

| # | Purpose |
|---|---|
| 174 | `scored_probability` + `resolution_source` + freeze trigger |
| 175 | Backfill `forecast_spacetime` Loop 5 columns for already-resolved forecasts |
| 177 | Clear legacy `mcp_tools` payloads out of `agents.mcp_servers` |
