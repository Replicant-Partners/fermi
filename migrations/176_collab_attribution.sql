-- ─────────────────────────────────────────────────────────────────────
-- 176 — collaboration attribution (Spec 26 §4.1)
-- ─────────────────────────────────────────────────────────────────────
--
-- Teams shipped without the one thing a human team needs: knowing WHO
-- did WHAT. Two columns close it.
--
--   fermi_forecast_updates.actor_user_id
--     The principal who caused this revision. Distinct from `agent_id`
--     (already present) which records WHICH agent produced the number.
--     Both matter: "Alice · via elo-scout" is the real event; before
--     this column the human half was simply thrown away, so every
--     revision in a shared forecast was anonymous and the Teams
--     Activity tab had to guess from `updated_at`.
--
--     Deliberately NULLable with no backfill: pre-existing rows have no
--     recoverable actor and we refuse to guess (attributing them to the
--     forecast owner would be a lie the UI can't distinguish from
--     truth). The activity feeds render NULL as actor_kind='system'.
--
--   fermi_portfolio_forecasts.added_by
--     The principal who added this forecast to this portfolio. Curation
--     is a first-class team act ("Bo pulled this into the WC book") and
--     was previously invisible.
--
--     Backfilled to the portfolio owner. Unlike revisions this IS a
--     defensible approximation — before shares existed, only the owner
--     could add to their own portfolio — and it keeps historical rows
--     out of the 'system' bucket where they'd be noise.
--
-- Everything else in Spec 26 is DERIVED from tables that already exist
-- (see §4.2). No event-log table on purpose: derivation is retroactively
-- correct for all history and no writer can forget to log.
--
-- Each statement is standalone and idempotent. No DO block needed —
-- ADD COLUMN IF NOT EXISTS / CREATE INDEX IF NOT EXISTS are single
-- statements, so PgBouncer transaction-mode can't split anything
-- (cf. the callout in migrations/119_teams_mission_defensive.sql).

-- ─── Revision attribution ────────────────────────────────────────────

ALTER TABLE public.fermi_forecast_updates
    ADD COLUMN IF NOT EXISTS actor_user_id TEXT;

COMMENT ON COLUMN public.fermi_forecast_updates.actor_user_id IS
    'Spec 26 §4.1: users.user_id of the principal who caused this revision. Orthogonal to agent_id (which agent produced the number) — a scheduled agent run has both. NULL means unattributable (pre-176 row, or a system/cron writer); the activity feeds surface those as actor_kind=''system'' rather than blaming the owner.';

-- Powers GET /api/teams/:id/contributions (group by actor) and the
-- ?actor= filter on the activity feeds. Partial: the vast majority of
-- historical rows are NULL and indexing them buys nothing.
CREATE INDEX IF NOT EXISTS idx_forecast_updates_actor
    ON public.fermi_forecast_updates(actor_user_id, created_at DESC)
    WHERE actor_user_id IS NOT NULL;

-- The activity feeds all scan "recent updates for these forecast ids,
-- newest first". The existing idx_forecast_updates_forecast is on
-- forecast_id alone, so PG still sorts. Composite kills the sort.
CREATE INDEX IF NOT EXISTS idx_forecast_updates_forecast_time
    ON public.fermi_forecast_updates(forecast_id, created_at DESC);

-- ─── Curation attribution ────────────────────────────────────────────

ALTER TABLE public.fermi_portfolio_forecasts
    ADD COLUMN IF NOT EXISTS added_by TEXT;

COMMENT ON COLUMN public.fermi_portfolio_forecasts.added_by IS
    'Spec 26 §4.1: users.user_id of whoever added this forecast to this portfolio. Backfilled to the portfolio owner for pre-176 rows (defensible: before object_shares, only the owner could add). Surfaces as the ''portfolio_add'' event in the portfolio and team activity feeds.';

UPDATE public.fermi_portfolio_forecasts pf
SET added_by = p.owner_id::text
FROM public.fermi_portfolios p
WHERE p.id = pf.portfolio_id
  AND pf.added_by IS NULL;

-- ─── Indexes for the provenance + inheritance queries ────────────────
--
-- Spec 26 §2 inheritance resolves "which portfolios contain this
-- forecast, and is one of them shared with me". idx_pf_forecast (mig
-- 094) covers the forecast→portfolio direction. The share lookup joins
-- object_shares on (object_type, object_id) which mig 009's
-- idx_object_shares_object already covers.
--
-- What's missing is the reverse direction used by
-- GET /api/teams/:id/shared: "every object shared with THIS team".
-- mig 009's idx_object_shares_target is (share_type, share_target) —
-- good, but the query also filters object_type, so make it covering.

CREATE INDEX IF NOT EXISTS idx_object_shares_target_type
    ON public.object_shares(share_type, share_target, object_type);

-- GET /api/teams/:id/contributions and the team activity feed both walk
-- "forecasts owned by members of this team". team_members(member_id) is
-- indexed (mig 009) but the team→members direction wants the composite
-- so the membership expansion is index-only.
CREATE INDEX IF NOT EXISTS idx_team_members_team_type
    ON public.team_members(team_id, member_type, member_id);
