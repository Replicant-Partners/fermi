-- ═══════════════════════════════════════════════════════════════════════
-- Migration 187 — forecast_agent_claims: retain what each agent actually claimed
--
-- PROBLEM
-- -------
-- Loop 5 needs per-agent credit, but a forecast's outcome scores the TEAM.
-- Splitting a team score across members is not identifiable when every member
-- is cited on every forecast: the membership matrix is rank-deficient, so all
-- members receive the same number forever, at any sample size. (The Observatory
-- surfaces this as L5-I03.)
--
-- The way out is not more real-world compositions — it is counterfactual
-- re-runs. The forecast model is re-runnable, so if we know what each agent
-- individually claimed we can synthesise the forecast for ANY subset of agents
-- by applying that subset's claims and neutralising the rest. That yields
-- v(S) over all 2^n subsets from a single real forecast, which is exactly the
-- input exact Shapley attribution needs (src/attribution/).
--
-- The blocker was that the claims are not kept. The football factor agents each
-- emit a real, individually-scorable quantity —
--
--     [MULTIPLIER] Suggested p50: 1.15 (p5: 1.05, p95: 1.28)
--
-- mapped to specific drivers by `driver_prefix_for_agent`
-- (src/handlers/workspace/agent_params_hook.rs). But the hook UPSERTs those
-- values into `workspace_outputs` key='params', which is CURRENT STATE: the
-- next agent's write, or the next run, overwrites them. Nothing anywhere
-- records that agent X claimed 1.15 for driver `socio` at time T.
--
-- Consequence: every forecast that resolves is permanently unattributable at
-- the agent level, because the per-agent inputs that produced it no longer
-- exist. This is why the table is worth adding before more data accrues rather
-- than after — the backlog cannot be reconstructed.
--
-- Related dead substrate, deliberately NOT relied on here:
-- `forecast_spacetime.drivers_snapshot` and `.sobol_snapshot` (mig-140) were
-- specced to hold `{name, specialist, p50, sobol_weight}` but no writer exists
-- — `fn_forecast_spacetime_on_update` inserts ten columns and neither is among
-- them. This table is the claim ledger; populating those snapshots is a
-- separate concern.
--
-- DESIGN: append-only ledger + as-of reconstruction
-- -------------------------------------------------
-- A claim is recorded when the agent makes it, which is generally BEFORE any
-- forecast row exists (the hook writes params, then triggers a refit, which is
-- what may produce or revise a forecast). So the ledger does not require a
-- forecast_id at write time.
--
-- Binding claims to a forecast is therefore a temporal join: the forecast
-- evaluated at time T used, for each driver, the most recent claim for that
-- (workspace, driver) with claimed_at <= T. `forecast_id` is nullable and may
-- be stamped later by whatever stage learns the association; when NULL, use
-- the as-of join. This mirrors how forecast_spacetime treats revisions and
-- avoids making the hook aware of forecast lifecycle.
--
-- Append-only: there is no UPDATE path and no unique constraint collapsing
-- history. Re-running an agent appends a new row; the old claim stays, because
-- "what did this agent believe at the moment that forecast was made" is the
-- question the whole attribution rests on.
--
-- `neutral_value` is the value a driver takes when its agent is ABSENT from a
-- counterfactual subset — the identity for that driver's combination rule
-- (1.0 for a multiplier). Stored per row so the counterfactual engine never
-- has to hardcode a convention that might differ per driver family.
-- ═══════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.forecast_agent_claims (
    claim_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- Where the claim was made. Always known.
    workspace_id   UUID NOT NULL,

    -- Who made it. agent_id may be NULL if the name did not resolve at write
    -- time; agent_name is always recorded so attribution can still be
    -- reconstructed after a rename (cf. the agents_used name/agent_id split
    -- that broke Loop 5's reader in the first place).
    agent_id       UUID REFERENCES public.agents(agent_id) ON DELETE SET NULL,
    agent_name     TEXT NOT NULL,

    -- What was claimed. `driver` is the param prefix (socio, institutional,
    -- dynamic, squad, tactical, fixture, ...). One row per (agent, driver):
    -- an agent covering three drivers writes three rows, so credit can later
    -- be resolved at either agent or driver granularity.
    driver         TEXT NOT NULL,
    p5             REAL,
    p50            REAL NOT NULL,
    p95            REAL,

    -- Identity value for this driver when its claimant is absent from a
    -- counterfactual subset.
    neutral_value  REAL NOT NULL DEFAULT 1.0,

    -- Optional forward link, stamped if/when the producing forecast is known.
    forecast_id    TEXT REFERENCES public.fermi_forecasts(id) ON DELETE SET NULL,

    -- Provenance of the claim text, for audit.
    source         TEXT NOT NULL DEFAULT 'multiplier_hook',
    raw_evidence   TEXT,

    claimed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The as-of reconstruction: latest claim per (workspace, driver) at or before a
-- given instant. This is the hot path for building counterfactual subsets.
CREATE INDEX IF NOT EXISTS idx_agent_claims_asof
    ON public.forecast_agent_claims (workspace_id, driver, claimed_at DESC);

-- Per-agent history, for the Loop 5 per-agent rollup.
CREATE INDEX IF NOT EXISTS idx_agent_claims_agent
    ON public.forecast_agent_claims (agent_id, claimed_at DESC)
    WHERE agent_id IS NOT NULL;

-- Direct lookup once a claim has been bound to a forecast.
CREATE INDEX IF NOT EXISTS idx_agent_claims_forecast
    ON public.forecast_agent_claims (forecast_id)
    WHERE forecast_id IS NOT NULL;

COMMENT ON TABLE public.forecast_agent_claims IS
  'Append-only ledger of the individual quantitative claims agents make (driver multipliers with p5/p50/p95). Enables counterfactual subset re-runs and therefore exact Shapley per-agent credit; see src/attribution/ and migration header.';
