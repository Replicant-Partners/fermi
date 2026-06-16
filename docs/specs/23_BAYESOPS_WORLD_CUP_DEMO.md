# BayesOps + Self-Improving Forecasts — Design Paper

**Date:** 2026-06-16
**Author:** Ivan Labra
**Status:** Design — pre-implementation. Phase 1 + Phase 2 BayesOps libraries (`crates/posterior`, `crates/posterior-reg`) shipped 2026-06-16; the wiring described here is the next move.
**Audience:** the parallel World Cup string and anyone implementing the next phase of Fermi Console.

---

## 0. The point of this document

A claim worth making: *Fermi forecasts learn from their own history.* As evidence accumulates — matches resolve, agents produce research, market data shifts — the distribution parameters that drive Monte Carlo simulations get refit against that history. The forecast rate moves because the underlying priors moved, and the human watching can see *why* it moved.

This paper specifies how to wire the shipped BayesOps libraries into the existing five-loop architecture (`FEEDBACK_LOOPS.md`) to demonstrate that claim end-to-end on the 2026 World Cup forecast portfolio.

It is deliberately a small specification. Most of what makes the demo work already exists. The new work is **a refit hook with an impact gate**, **a sparkline-anchored UX affordance in the forecast editor**, and **a spacetime view of the forecast's trajectory**. That's it.

---

## 1. Framing

### 1.1 One user, one console

The user is the human running the forecast — a forecaster operating in Fermi Console. They are responsible for everything: writing the FPL, watching the rate move, accepting or rejecting fitted parameters, reading the trajectory.

This paper does not introduce a separate "operator" role. Affordances that *might* in another system live behind a back-office surface (accept/reject fitted parameters, inspect impact assessments) live in this one inside the forecast editor itself. The forecaster decides; the forecaster sees the decision; the forecaster sees the consequences. No second view.

### 1.2 Fermi Console is the App

ABW Apps are App-shaped artifacts (manifest, fleet, workspace template, UI surface). Fermi Console *is* such an artifact. The 2026 World Cup forecasts are a **portfolio of forecasts inside that one App** — not a separate App.

This means the BayesOps wiring declaration — which drivers are learnable, which extractors map upstream resolutions into observations, what auto-accept thresholds apply — lives **at the forecast level, alongside the FPL**, not in a separate App manifest. The FPL already has the `learnable: true` annotation per the existing `BAYESOPS_CONTRACT.md`. We extend that annotation with a `feeds_from` block per driver.

One declaration per forecast (or per forecast template, if forecasts inherit from one). No App-level manifest dance.

### 1.3 BayesOps is two things, but one is on the critical path

**BayesOps as a posterior fitter** is the demo. Historical observations → `fit_marginal()` → `FittedDistribution` → injected as FPL `Driver` parameters → tighter Monte Carlo → updated rate. This is Loop A from `FEEDBACK_LOOPS.md §4`. We ship the wiring for this on the critical path.

**BayesOps as a synthetic-data generator** is the same library called in a different mode (sample from a fitted `ConditionalPosterior`, fold the samples back as weighted observations). The capability is shipped — `crates/posterior-reg` exposes it via MCP and HTTP today. We document the pattern and put the audit infrastructure in place (one column on the snapshot ledger; one event type) so the demo can show it if an agent uses it, but **it is not on the critical path**. The demo's headline is the fitter wired into the loop, not the synthesizer.

---

## 2. What already exists and what we build

### 2.1 What already exists

- **The five-loop infrastructure** (`FEEDBACK_LOOPS.md`). Loops 1, 2, 3-inner, 5 are closed and running. Loop 5's calibration signals already flow through to the routing classifier and the consolidation worker's semantic rules.
- **The workspace resolution lifecycle** (`migrations/147`, `WORKSPACE_RESOLUTION.md`). `POST /api/workspaces/:id/resolve` writes the outcome, publishes to `workspace_outputs.resolution`, fans out via `workspace_messages`, and has a labeled TODO insertion point at `src/handlers/workspace/resolution.rs:286` waiting for the refit hook.
- **The learnable-driver contract** (commits `889ca58`, `981eeda`, `BAYESOPS_CONTRACT.md`). FPL drivers can be marked `learnable: true`. The executor reads `params.<driver_name>_fitted` and substitutes for the static prior. The console has the toggle UX.
- **The BayesOps libraries** (commits `89747bc`, `8f3ef51`). `fit_marginal`, `fit_conditional`, `ConditionalPosterior`, HTTP surface, MCP tools. 106 tests passing.
- **The forecast editor pane in Fermi Console** with sparkline distributions per driver. This is where the BayesOps UX lands.
- **Agent-generated research, the forecast wiki, market histograms, inside/outside rate bands**. All existing. None of these change.

### 2.2 What we build, in priority order

1. **The refit hook** with the impact gate. The TODO at `resolution.rs:286` becomes a real function. (§3)
2. **The forecast-driver UX affordance**: the existing sparkline gains a "fitted from N obs" state and an inline accept gesture for high-impact fits. (§4)
3. **The spacetime view**: a chronological replay of rate, posterior trajectory, and evidence events synchronized on one timeline. (§5)

Nothing else. The paper ends after three sections of mechanism.

---

## 3. The refit hook

### 3.1 Where it lives

`src/handlers/workspace/refit.rs`, a new file exposing:

```rust
pub(crate) async fn refit_workspace(
    pool: &PgPool,
    workspace_id: Uuid,
    triggered_by: TriggerReason,
) -> Result<RefitOutcome, RefitError>;
```

Called from the post-commit `tokio::spawn` block in `resolution.rs:286` (the resolution doc's labeled insertion point). Also exposed as `POST /api/workspaces/:id/refit` for manual triggers and for the accept gesture in §4.

### 3.2 What it does

1. **Read** `workspace_outputs[ws_id].learnable_manifest` to discover learnable drivers. Use the published manifest, not the FPL source — the manifest is the executor's source of truth and is always fresh.
2. **Collect observations per driver**: prefer an explicit `workspace_outputs[ws_id].observations.<driver_name>` array if present; otherwise walk `workspace_dependencies` to find upstream workspaces and apply the driver's `feeds_from.extractor` (§3.4) to each upstream's resolution outcome.
3. **Fit**: call `posterior::fit_marginal()` for marginal drivers or `posterior_reg::fit_conditional()` if features are declared.
4. **Run the impact gate** (§3.3) on each fitted distribution.
5. **Write per gate decision** (§3.5).
6. **Persist a snapshot** in `bayesops_posterior_snapshots` regardless of decision. This is the spacetime view's data source (§5).
7. **Emit evidence events** as `workspace_messages` rows with `message_type = 'system_event'`.
8. **Recurse upstream** with cycle detection per the resolution doc's spec.

### 3.3 The impact gate

The single design question: was the fit big enough that the forecaster needs to see it before it takes effect?

```rust
struct ImpactAssessment {
    rate_before: f64,            // Monte Carlo with the current prior
    rate_after:  f64,            // Monte Carlo with the proposed fitted posterior
    delta_pp:    f64,            // |rate_after - rate_before| in percentage points
    ci_width_change: f64,        // (current CI - new CI) / current CI; positive = tightened
}

enum GateDecision {
    AutoAccept,    // |delta_pp| < 2.0 AND ci_width_change > -0.5
    StageInline,   // anything else
    HardBlock,     // |delta_pp| > 20.0 — likely a fitting bug
}
```

Per gate decision:

- **AutoAccept**: write the fitted params directly. Post a `bayesops_fit_accepted` evidence event. The sparkline updates next time the editor renders.
- **StageInline**: hold the fit in `bayesops_pending_fits`. Post a `bayesops_fit_pending` evidence event. The driver's sparkline gains a "pending fit" badge with the impact and an inline accept gesture (§4).
- **HardBlock**: drop the fit. Log loudly. Post a `bayesops_fit_failed` event with diagnostics. Don't surface to the user — this is a system fault, not a decision.

**Thresholds.** 2pp default; per-driver overrides via the `feeds_from.auto_accept_threshold` block in the FPL annotation. The default is intentionally conservative — we expect to tune it once the demo runs and we see what actually fires.

**Why we run MC twice per fit.** Each impact assessment runs ~10K MC iterations twice per learnable driver per refit. Cheap (≪1s) and the only honest way to know whether a fit deserves the forecaster's attention.

### 3.4 Driver extractors

The `feeds_from` block on a learnable driver tells the refit hook how to derive a scalar observation from an upstream resolution outcome. Extractors are named primitives. We ship four; the trait is open so the registry grows by code addition.

```rust
trait Extractor: Send + Sync {
    fn name(&self) -> &str;
    fn extract(
        &self,
        upstream_resolution: &JsonValue,
        workspace_context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError>;
}
```

Built-in registrations:

| Extractor | Behaviour |
|---|---|
| `binary_winner_id_match` | 1.0 if `outcome[winner_field]` matches this workspace's entity, else 0.0 |
| `binary_field_value` | 1.0 if `outcome.path == value`, else 0.0 |
| `scalar_field_value` | `outcome.path` as f64 |
| `scalar_difference` | `outcome.field_a - outcome.field_b` from this workspace's entity perspective |

Discoverability is solved by the existing MCP tool surface — `fermi_list_extractors` (new, trivial) returns the registry contents with config schemas.

FPL annotation shape:

```fpl
driver continuous won_in_group_stage {
    distribution: triangular(0.3, 0.5, 0.7)
    learnable: true
    feeds_from: {
        source: "upstream_resolutions",
        extractor: "binary_winner_id_match",
        config: { winner_field: "winner_team_id", match_value: "${workspace.entity_id}" }
    }
}
```

Parser changes: extend the existing `learnable` annotation parser to accept a `feeds_from` block as a JSON-literal field. Small change; same shape as the existing driver-annotation pattern.

### 3.5 Writes

Both gate paths write the fit metadata to `bayesops_posterior_snapshots`. The auto-accept path additionally writes `params.<driver_name>_fitted` directly via `INSERT ... ON CONFLICT DO UPDATE SET value = $merged, version = version + 1` against `workspace_outputs`, mirroring `outputs.rs:67-115` exactly, and manually fans out `upstream_output_updated` system events. The stage path inserts a row into `bayesops_pending_fits` and posts the pending event.

The `bayesops_posterior_snapshots` table has a `synthetic_n` column tracking how many observations were synthetic — present from day one so synthetic-data usage (when it happens) is auditable without schema changes.

### 3.6 Schema

```sql
-- Migration 148: BayesOps refit ledger and pending queue

CREATE TABLE bayesops_posterior_snapshots (
    snapshot_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID NOT NULL,
    driver_name      TEXT NOT NULL,
    fitted           JSONB NOT NULL,         -- FittedDistribution
    metadata         JSONB NOT NULL,         -- FitMetadata
    n_observations   INT NOT NULL,
    synthetic_n      INT NOT NULL DEFAULT 0, -- of n_observations, how many were synthetic
    ci_width         DOUBLE PRECISION NOT NULL,
    n_eff            DOUBLE PRECISION NOT NULL,
    quality          TEXT NOT NULL,
    rate_before      DOUBLE PRECISION,
    rate_after       DOUBLE PRECISION,
    decision         TEXT NOT NULL CHECK (decision IN ('auto_accepted', 'staged', 'hard_blocked')),
    triggered_by     TEXT NOT NULL,
    fitted_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX ON bayesops_posterior_snapshots (workspace_id, driver_name, fitted_at);

CREATE TABLE bayesops_pending_fits (
    pending_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID NOT NULL,
    driver_name      TEXT NOT NULL,
    snapshot_id      UUID NOT NULL REFERENCES bayesops_posterior_snapshots(snapshot_id),
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending', 'accepted', 'rejected', 'expired')),
    staged_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    decided_at       TIMESTAMPTZ,
    decided_by       TEXT,
    decision_notes   TEXT
);
CREATE INDEX ON bayesops_pending_fits (workspace_id, status, staged_at);
```

---

## 4. The forecast-driver UX affordance

### 4.1 Anchor: the existing sparkline

The forecast editor pane in Fermi Console already shows a small sparkline distribution per driver. That sparkline visualises the current Monte Carlo input — whatever `params.<driver>_fitted` evaluates to, or the static prior if no fit exists.

This is where BayesOps lives in the UX. We don't add a new panel.

### 4.2 What changes on the sparkline

Three new visual states, layered on what already renders:

**State A — fitted (auto-accepted).** When `params.<driver>_fitted` is present, the sparkline renders the fitted posterior (it already does, given the executor reads the JSON). We add a small badge: *"fit from N obs · last updated 2 min ago"*. Hovering the badge shows a popover with the FitMetadata: quality, n_eff, CI width, source description. Clicking the badge opens the spacetime view (§5) scrolled to this fit's event.

**State B — pending fit.** When a fit was staged by the impact gate, the sparkline renders **two overlapped traces**: the current prior in grey, the proposed fitted posterior in colour. A badge reads *"proposed fit · rate impact +6pp"*. Two buttons next to the badge: **Accept** writes the fit (calls `POST /api/bayesops/pending/:id/accept`), **Dismiss** drops it (calls `/reject`). Optional notes field expands inline.

**State C — fit failed.** When the impact gate hard-blocked or fitting errored, the sparkline gains a small warning glyph. Hovering shows the error. No buttons — this is system feedback, not a user decision.

The natural progression from the user's point of view: they're editing a forecast. They see sparklines per driver. A match resolves. The sparkline for an affected driver updates — either silently (auto-accept) with a small "fit refreshed" notification, or with a visible "proposed fit, click to accept" affordance. They never leave the editor.

### 4.3 The endpoints behind the UX

```
GET    /api/workspaces/:id/bayesops/state
       → { drivers: [{ name, sparkline_data, badge, pending_fit? }] }

POST   /api/bayesops/pending/:pending_id/accept   (body: { notes? })
POST   /api/bayesops/pending/:pending_id/reject   (body: { notes? })
```

The `state` endpoint is what the editor calls on render and on every `bayesops_*` workspace event. One round trip, all the data the sparklines need. The accept and reject handlers are thin wrappers around the same write path the auto-accept path uses.

### 4.4 What's intentionally not built here

No separate review panel, no list-of-all-pending-fits page, no operator dashboard. The pending fits surface where the driver lives. If you have 12 pending fits across 12 drivers, you see 12 inline affordances on 12 sparklines. That's correct — these are 12 unrelated decisions about 12 different parts of the model. A flat list would be the wrong abstraction.

---

## 5. The spacetime view

### 5.1 What it is

The forecast wiki shows current state. The spacetime view shows trajectory. Per your "time-travel the wiki page" framing, we build it incrementally.

**Pass A (ship for demo).** Split-pane view, accessible from a tab in the cockpit and from any `bayesops_fit_*` evidence event:

- **Left pane:** the current wiki page (no time travel of content yet).
- **Right pane:** a chart with three traces on a shared time axis:
  - Forecast rate over time (from `fermi_forecast_updates`)
  - CI width over time (from `bayesops_posterior_snapshots`, aggregated to forecast level when multiple drivers contribute)
  - Market rate over time where present (existing Polymarket integration)
- **Below the chart:** event dots on the same time axis. Categories:
  - BayesOps fits (real-data dots and synthetic-data dots distinguished by colour)
  - Upstream resolutions
  - Agent research updates (existing system events)
  - Market data ingestions
- Hovering an event dot highlights the corresponding rate movement. Clicking an event scrolls the wiki to a "what was added then" callout.

**Pass B (later, deferred).** The wiki *re-renders* at a selected timestamp — full History Flow analogy. Requires versioned wiki content beyond what `fermi_forecast_updates` carries. Worth doing once Pass A proves the metaphor works.

### 5.2 Data sources

All read-only. No new writes beyond the snapshot ledger §3.6 already adds.

| Element | Source |
|---|---|
| Rate over time | `fermi_forecast_updates` |
| CI width over time | `bayesops_posterior_snapshots` |
| Market rate | existing Polymarket integration |
| Inside/outside bands | existing calibration endpoint |
| Event dots (BayesOps) | `workspace_messages` filtered `metadata->>'event' LIKE 'bayesops_%'` |
| Event dots (research, resolution, market) | `workspace_messages` of `message_type = 'system_event'`, by `metadata->>'event'` |

### 5.3 Endpoint

```
GET /api/workspaces/:id/spacetime?from=...&to=...
    → { rate_series, ci_series, market_series, events, wiki_current }
```

Pure aggregation. No business logic.

### 5.4 Surface

`crates/fermi-console`. New view, accessible from the cockpit's existing tab strip and from click-through on any BayesOps evidence event. The forecast wiki is its source of structural truth; we wrap it, not replace it.

---

## 6. The end-to-end walkthrough

Concrete. ARG plays MEX. The match resolves. What the forecaster sees:

1. **They're in their WC 2026 forecast portfolio in Fermi Console.** They have the `team_prior_ARG` forecast open in the editor. The `won_in_group_stage` driver shows a sparkline — current prior, no fit yet.

2. **Outside the editor, the H2H match workspace resolves** via `POST /api/workspaces/h2h_arg_vs_mex/resolve` with `outcome: {winner_team_id: "ARG"}`. The existing resolution handler commits the resolution. The refit hook fires post-commit in a background task.

3. **The hook reads `team_prior_ARG`'s learnable manifest**, finds `won_in_group_stage`, walks `workspace_dependencies` to the seven prior resolved ARG H2H matches, applies `binary_winner_id_match` to each to derive `[1, 0, 1, 1, 1, 0, 1, 1]` (the eighth being today's match). It calls `posterior::fit_marginal(observations, None, DistFamily::Beta)`. The fit returns `Beta(6.1, 3.4)`.

4. **The impact gate runs MC twice.** Before: 22%. After: 26%. Δ = 4pp. Above 2pp threshold. **Stage.** Snapshot written. `bayesops_pending_fits` row inserted. `bayesops_fit_pending` event posted to `team_prior_ARG`'s workspace messages.

5. **The forecaster's editor receives the workspace event** (pg_notify → SSE → editor refetches `bayesops/state`). The sparkline for `won_in_group_stage` re-renders showing both traces (grey prior, coloured proposed fit), with the badge *"proposed fit · rate impact +4pp"* and two buttons.

6. **The forecaster clicks Accept.** `POST /api/bayesops/pending/:id/accept`. The handler writes `params.won_in_group_stage_fitted` directly, updates the pending row, posts a `bayesops_fit_accepted` event. The sparkline re-renders to State A: a single trace (the fitted posterior) with badge *"fit from 8 obs · just now"*.

7. **The next time the editor runs the forecast** (manually or on schedule), the executor uses the new posterior. The rate becomes 26%. `fermi_forecast_updates` records the revision.

8. **The forecaster clicks the spacetime tab.** They see the trajectory: rate moved from 22% to 26% at the moment they accepted; CI tightened; a BayesOps fit event sits on the timeline at that moment; the upstream resolution event sits just before it. They scroll back to see the whole season — every match resolution, every fit, every rate revision, every research update.

That's the demo.

---

## 7. What we are not building

- Separate operator surface. Forecaster does it all in the editor.
- Persistent posterior store beyond the snapshot ledger. Spec 14 Phase 5 deferred.
- `data_driven()` FPL syntax. Spec 14 Phase 5 deferred.
- Full-history wiki time travel. Spacetime view Pass B deferred.
- Automatic synthetic-data injection by the refit hook. Capability exists in the libraries; agent-mediated usage is documented but unscripted. Demo doesn't depend on it.
- App manifest declaration. Per §1.2 there's no separate App. The FPL `feeds_from` annotation is the entire declaration burden.
- Mass-matrix HMC improvements, additional model variants (Spec 14 Phase 2b). LinearNormal is enough for WC.
- Agent context-generation changes. Agents continue producing research as they do; we attribute on the read side via spacetime, not on the write side.

---

## 8. Phases

Three discrete phases, each independently demoable. Estimated effort: 4 days total.

### Phase R-1 — Refit hook + impact gate (~2 days)

- Migration 148
- `crates/posterior` extractor trait + registry, four built-in extractors
- `src/handlers/workspace/refit.rs` with `refit_workspace()`
- Wiring into `resolution.rs:286` TODO
- `POST /api/workspaces/:id/refit` manual endpoint
- Impact gate runs MC twice, classifies, writes snapshot, auto-accepts or stages
- Evidence events posted
- FPL parser accepts `feeds_from` annotation
- Integration tests: resolution triggers refit, auto-accept writes params, stage creates pending row, hard block surfaces error

**End of R-1:** `curl POST /resolve` on a WC h2h workspace triggers a refit on the upstream team-prior; snapshot stored; either params written or pending row created; event posted. No UI yet.

### Phase R-2 — Sparkline UX (~1 day)

- `GET /api/workspaces/:id/bayesops/state`
- `POST /api/bayesops/pending/:pending_id/accept` and `/reject`
- Console sparkline States A/B/C
- Click-through from sparkline badge to spacetime view (stubbed until R-3)
- Editor subscribes to workspace events, refetches state on `bayesops_*`
- Integration tests: pending → accept writes params; pending → reject doesn't

**End of R-2:** the forecaster sees pending fits inline on driver sparklines, accepts them with one click, watches the sparkline update. Rate movement appears in the existing rate display on next forecast run.

### Phase R-3 — Spacetime view Pass A (~1 day)

- `GET /api/workspaces/:id/spacetime` aggregation endpoint
- Console split-pane view: wiki left, timeline right
- Three traces, event dots, hover highlighting, click scrolls wiki
- Synthetic vs real data dots distinguished
- Integration tests: spacetime endpoint returns expected shape for workspace with multiple refits

**End of R-3:** the forecaster opens any forecast, clicks Trajectory, sees the entire history of evidence and rate movement. Demo is complete.

---

## 9. Open questions

1. **Threshold tuning during the demo.** §3.3 picks 2pp default; per-driver overrides via `feeds_from.auto_accept_threshold`. Reasonable for binary drivers; possibly wrong for scalar drivers. We learn-by-doing during R-1 and adjust before R-2 ships.

2. **The fpl_source of truth for `feeds_from`.** Each forecast's FPL file. If forecasts inherit from templates and templates change, we need a versioning story. For the demo, treat each forecast independently and ignore template inheritance.

3. **Sparkline rendering with two overlapped traces.** Cockpit currently renders one trace per driver. Two traces is a small extension. Confirm the cockpit's existing sparkline component (`crates/fermi-console/src/cockpit.rs`) supports overlay, or budget a few extra hours for that change.

---

## 10. References

- `docs/architecture/FEEDBACK_LOOPS.md` — the loop framework. This paper is its Loop A wiring.
- `docs/specs/14_BAYESOPS_SPEC.md` — the BayesOps library spec.
- `docs/fermi/BAYESOPS_CONTRACT.md` — the learnable-driver wire format.
- `docs/fermi/WORKSPACE_RESOLUTION.md` — the resolution endpoint and TODO insertion point.
- `migrations/147_workspace_resolution.sql` — the lifecycle schema.
- `crates/posterior/` and `crates/posterior-reg/` — the shipped libraries.
- `src/handlers/workspace/resolution.rs:286` — where R-1's hook lands.
- `crates/fermi-console/src/cockpit.rs` — where R-2 and R-3 land.
