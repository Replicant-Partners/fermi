-- ─────────────────────────────────────────────────────────────────────
-- 154 — object_shares backfill for existing team_id rows
-- ─────────────────────────────────────────────────────────────────────
--
-- Spec 24 §3.2 Wave 2 step 5: every forecast/portfolio that already has
-- a team_id set gets a corresponding object_shares row so the canonical
-- can_access / can_view helpers see the team share. Without this
-- backfill, the ACL switch in 2.4b would break access for existing
-- team-shared content — the team_id column alone is not enough once
-- handlers switch to object_shares-based access checks.
--
-- The team_id column stays as a "primary team share" pointer so
-- idx_forecasts_team / idx_portfolios_team remains useful.
--
-- ON CONFLICT DO NOTHING makes the migration idempotent: re-running
-- produces zero new rows because the rows already exist.

INSERT INTO object_shares
    (object_type, object_id, share_type, share_target, permission, granted_by)
SELECT 'forecast', id::text, 'team', team_id::text, 'edit', owner_id::text
FROM fermi_forecasts WHERE team_id IS NOT NULL
ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING;

INSERT INTO object_shares
    (object_type, object_id, share_type, share_target, permission, granted_by)
SELECT 'portfolio', id::text, 'team', team_id::text, 'edit', owner_id::text
FROM fermi_portfolios WHERE team_id IS NOT NULL
ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING;
