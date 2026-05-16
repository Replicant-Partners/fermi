-- Migration 122: Backfill ownership for curated/system agents seeded after
-- migration 111 ran.
--
-- Migration 111 did the initial ownership restore but only ran once.
-- Agents added to the bestiary after that point (e.g. simops_companion,
-- sidestream_miner, comparator, product_scout, regulatory_scanner,
-- valuechain_mapper, marketing_composer) were seeded with user_id = NULL
-- because seed_agents_to_database passes owner_id: None and user_id is
-- not in the upsert's DO UPDATE SET clause.
--
-- This migration is identical in intent to 111: assign the earliest admin
-- to any curated/system agent that still has a null owner. Idempotent —
-- won't touch agents that already have an owner.
--
-- PgBouncer-safe: single UPDATE, no BEGIN/COMMIT.

UPDATE public.agents
   SET user_id = (
       SELECT user_id
         FROM public.users
        WHERE role = 'admin'
        ORDER BY created_at ASC
        LIMIT 1
   )
 WHERE tier IN ('curated', 'system')
   AND user_id IS NULL
   AND EXISTS (SELECT 1 FROM public.users WHERE role = 'admin');
