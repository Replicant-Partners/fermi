-- ─────────────────────────────────────────────────────────────────────
-- 179 — team_members.capabilities (Spec 30 §2)
-- ─────────────────────────────────────────────────────────────────────
--
-- WHY
--
-- `team_members.role` is a single ladder: viewer < member < admin < owner.
-- It answers "who administers this team" and is then forced to double as
-- "who may do consequential work", which it is bad at. The concrete
-- failure:
--
--   * `resolve_forecast_handler` gates on `can_edit`.
--   * Spec 26 made a portfolio team-share grant `edit` on every forecast
--     inside it (and the console's own "share with team" button hardcodes
--     'edit').
--   * Therefore sharing a book so colleagues can HELP silently also
--     delegated the irreversible scoring decision to all of them.
--
-- Irreversible is not an exaggeration: mig-174's freeze trigger pins
-- `scored_probability` once resolved, and `resolve_forecast()` requires
-- status='active', so a mis-resolution cannot be redone.
--
-- There was no way to express "work on these with me" without also saying
-- "you may close them". A ladder cannot express that, because the two
-- concerns are orthogonal — the EVE distinction between a Director (admin)
-- and a role grant like Accountant (a specific power).
--
-- So: `role` keeps administering the team, and `capabilities` carries
-- discrete powers over the team's work.
--
-- VOCABULARY
--
--   'resolve'  may resolve/void forecasts on this team's shared surface
--   'spend'    may spend the team's shared credit pool on agent runs
--
-- Only 'resolve' is ENFORCED as of v0.11.10. 'spend' is declared here
-- because the treasury slice needs the same column and a second migration
-- on the same column for the same feature family would be churn — but
-- nothing reads it yet, and that is stated rather than implied.
--
-- Deliberately TEXT[] rather than booleans or a join table: the set is
-- small and closed, membership rows are read on every access check (so a
-- join is the wrong shape), and Postgres array containment with a GIN
-- index is exactly the query pattern. Validation of element values lives
-- in `fermi_auth::TeamCapability::from_str` — a CHECK constraint on array
-- elements would have to be dropped and recreated to add a capability,
-- which is the failure mode migration 157 exists to document.
--
-- BACKFILL
--
-- Owners and admins get 'resolve'; members and viewers do not. This is a
-- deliberate TIGHTENING, and it is the entire point: a member who could
-- previously resolve a team-shared forecast now cannot until granted.
--
-- Nothing breaks for solo users, because forecast OWNERS keep resolving
-- their own work through the object-admin path (owner ⇒ Permission::Admin
-- in `can_access`) and never consult this column. The tightening lands
-- exactly on the inherited/shared path, which is where the hazard was.

ALTER TABLE public.team_members
    ADD COLUMN IF NOT EXISTS capabilities TEXT[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN public.team_members.capabilities IS
    'Spec 30: discrete powers over this team''s work, orthogonal to `role` (which administers the team). Vocabulary: ''resolve'' (may resolve/void forecasts on the team''s shared surface), ''spend'' (may draw on the team credit pool — declared, not yet enforced). Validated by fermi_auth::TeamCapability::from_str rather than a CHECK, so adding a capability does not require a drop/recreate. Forecast owners never consult this column: they resolve via the object-admin path.';

-- Containment (`'resolve' = ANY(capabilities)`) is the only access pattern.
CREATE INDEX IF NOT EXISTS idx_team_members_capabilities
    ON public.team_members USING GIN (capabilities);

-- Backfill. Guarded on `capabilities = '{}'` so a re-run cannot clobber a
-- grant an operator has since changed — the runner executes every
-- migration on every boot.
UPDATE public.team_members
SET capabilities = ARRAY['resolve']
WHERE role IN ('owner', 'admin')
  AND capabilities = '{}';
