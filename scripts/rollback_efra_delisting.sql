-- ═══════════════════════════════════════════════════════════════════
-- ROLLBACK — restore the efra agents' Fermi membership
--
-- Undoes scripts/delist_efra_from_fermi.sql. Restores the exact rows as
-- they existed before delisting on 2026-08-07, including their original
-- `granted_at` timestamps so the restored state is indistinguishable from
-- the pre-delisting state.
--
-- Provenance is restored as `curated_seed` — the value it actually had.
-- Do NOT "restore" these as `approved`: they were never approved, and the
-- whole point of the delisting was to stop unreviewed membership from
-- being indistinguishable from reviewed membership.
--
-- Only needed if the delisting was a mistake. The intended path forward
-- is for the owner to submit membership requests
-- (POST /api/orchestras/fermi/requests) and for a Fermi maintainer to
-- approve them, which produces `source='approved'` with a real receipt.
-- ═══════════════════════════════════════════════════════════════════

BEGIN;

INSERT INTO public.orchestra_members
    (orchestra_name, agent_id, source, request_id, granted_by, granted_at)
VALUES
  ('fermi','436c16bd-b9a0-43e1-94b0-eebdc7264997','curated_seed',NULL,NULL,'2026-08-04 17:55:20.630452+00'),
  ('fermi','fe34b407-2b1d-4b52-8852-1a0cea4657ee','curated_seed',NULL,NULL,'2026-08-04 18:02:42.498957+00'),
  ('fermi','7e819940-fde0-4d23-a601-d1276f61ffda','curated_seed',NULL,NULL,'2026-08-04 18:15:00.243398+00'),
  ('fermi','da2f8b21-a8ab-4849-86ab-8244fd45333a','curated_seed',NULL,NULL,'2026-08-04 18:17:54.337573+00'),
  ('fermi','bc0a9c85-1b55-4cf0-88df-ebaa3c02acab','curated_seed',NULL,NULL,'2026-08-04 18:19:48.128547+00'),
  ('fermi','b17ea5eb-0465-443d-b1e2-719912ba102a','curated_seed',NULL,NULL,'2026-08-04 18:21:27.894499+00'),
  ('fermi','75873bda-be3f-4670-9f6d-69915546380c','curated_seed',NULL,NULL,'2026-08-04 18:26:05.572135+00'),
  ('fermi','e9e9ff74-5d6a-49a9-a5e8-a47ee2d9f9bd','curated_seed',NULL,NULL,'2026-08-04 18:27:24.680252+00'),
  ('fermi','4565ed12-3b94-4ea5-8daa-19a9d9f00927','curated_seed',NULL,NULL,'2026-08-04 18:29:20.949152+00')
ON CONFLICT (orchestra_name, agent_id) DO NOTHING;

DO $$
DECLARE n INTEGER;
BEGIN
    SELECT COUNT(*) INTO n FROM public.orchestra_fermi_members;
    RAISE NOTICE '[rollback] fermi roster is now % member(s) (expected 21)', n;
END $$;

COMMIT;
