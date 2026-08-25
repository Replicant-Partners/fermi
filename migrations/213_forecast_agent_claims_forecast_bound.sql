-- ═══════════════════════════════════════════════════════════════════════
-- Migration 213 — a claim may be bound to a FORECAST instead of a workspace.
--
-- PROBLEM
-- -------
-- `forecast_agent_claims` (migration 187) is the sole input to the Shapley
-- attribution engine in `src/attribution/`. It has held **zero rows since it
-- was created**. `docs/HANDOFF_loops_and_gates.md` §4.2 records the count and
-- the cause: 61 quantified agent judgements were produced and all 61 were
-- discarded, every one of them because it was produced outside any workspace.
--
-- The cause is this table's own shape, not the producers'. A claim is only
-- worth writing if attribution can later FIND it, and what makes it findable
-- is its binding. This table declared two bindings and got their obligations
-- backwards:
--
--   * `workspace_id UUID NOT NULL`   — the weaker binding, mandatory.
--   * `forecast_id  TEXT`            — the stronger binding, optional. Its own
--     comment in 187 calls it an "optional forward link, stamped if/when the
--     producing forecast is known".
--
-- `forecast_id` is strictly the better of the two. `load_agent_claims`
-- (`src/handlers/attribution.rs`) says so in SQL — it orders
-- `(forecast_id = $1) DESC` and its doc comment reads "an explicit binding
-- beats a temporal inference", falling back to the (workspace, driver,
-- claimed_at <= T) as-of join only when no forecast-bound row exists. So the
-- reader already prefers the column that could never be the sole binding.
--
-- The consequence is the one measured above. The Fermi Console is the primary
-- producer of driver-bound agent judgements, and it calls
-- `POST /api/agents/:id/execute` and `/execute/stream` with no workspace —
-- it does not have one, and does not need one, because it knows the exact
-- `(forecast_id, driver)` the run is bound to, which is more than a workspace
-- would have told it. `src/handlers/execution.rs` therefore gated the whole
-- claim-writing hook on `if let Some(ws_id) = ws_id_opt`, since a NULL
-- `workspace_id` could not be inserted. The one producer that knew the
-- forecast was the one producer that could not write at all.
--
-- CHANGE
-- ------
-- Drop `NOT NULL` from `workspace_id` and require, instead, that at least one
-- binding is present. "Some binding" is the real invariant: an unbound claim is
-- unreachable by either arm of the reader's WHERE clause and is therefore
-- indistinguishable from a claim that was never written — which is precisely
-- the failure this table has been in since 187.
--
-- `driver NOT NULL` is untouched and stays that way. A claim is an adjustment
-- applied to a driver and neutralisable at `neutral_value`; neutralisability is
-- what lets the engine synthesise the forecast for any SUBSET of agents. None
-- of that means anything without a driver. See migration 205, which made the
-- same distinction from the other side: an *assertion* exists whenever the
-- agent ran, a *claim* is that assertion bound to a driver.
--
-- Not a data migration: there are no rows to backfill, and that is the entire
-- point of the change.
-- ═══════════════════════════════════════════════════════════════════════

-- One DO block, for the reason migration 205 records: PgBouncer runs in
-- transaction-pooling mode, where top-level statements get separate implicit
-- transactions with no rollback between them.
DO $$
BEGIN
    ALTER TABLE public.forecast_agent_claims
        ALTER COLUMN workspace_id DROP NOT NULL;

    COMMENT ON COLUMN public.forecast_agent_claims.workspace_id IS
        'Where the claim was made, when there is a workspace. NULLABLE since '
        'migration 213: a run bound to an explicit (forecast_id, driver) — '
        'which is how the Fermi Console executes agents — has no workspace and '
        'needs none, because forecast_id is the stronger binding and the one '
        'load_agent_claims prefers. At least one of workspace_id / forecast_id '
        'must be present; see forecast_agent_claims_has_binding.';

    -- DROP+ADD rather than a bare ADD, so re-running the file is a no-op
    -- instead of a duplicate-object error.
    ALTER TABLE public.forecast_agent_claims
        DROP CONSTRAINT IF EXISTS forecast_agent_claims_has_binding;
    ALTER TABLE public.forecast_agent_claims
        ADD CONSTRAINT forecast_agent_claims_has_binding
        CHECK (workspace_id IS NOT NULL OR forecast_id IS NOT NULL);
END $$;
