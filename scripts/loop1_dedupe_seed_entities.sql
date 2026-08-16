-- ═══════════════════════════════════════════════════════════════════════
-- Remove duplicate seed entities left by non-idempotent boot seeding
--
--   DRY RUN by default.  Pass -v apply=1 to actually write.
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT HAPPENED
-- -------------
-- `seed_cep_entities` (src/api_server.rs) seeds an agent's `fermi_contract.
-- seed_facts` into the `entities` table at boot. Its idempotency guard was:
--
--     let has_cep = existing.iter().any(|e| e.entity_type.starts_with("cep_"));
--     if has_cep { return; }
--
-- but the loop it guards writes whatever `entity_type` the card declares. Any
-- card whose seed facts are not `cep_`-prefixed therefore never tripped the
-- guard, and **every server boot re-seeded the entire set**.
--
-- Measured before the fix: 15 distinct seed facts stored as 2,475 rows —
-- exactly 165 identical copies of each.
--
--     football_institution_agent      7 facts → 1155 rows
--     macro_data_agent                4 facts →  660 rows
--     fixture_context_agent           4 facts →  660 rows
--     weather_oracle                  2 facts →    2 rows   (unaffected: it
--                                                            declares six real
--                                                            cep_ facts, which
--                                                            tripped the guard)
--
-- The code fix keys idempotency on (entity_name, entity_type) per fact, so this
-- stops growing. This script removes what already accumulated.
--
-- WHAT IT DOES
-- ------------
-- For each (agent_id, entity_name, entity_type) group among *seed* entities —
-- no `source_episodes`, no embedding — keeps the OLDEST row by `t_valid`
-- (tie-broken by `entity_id` for determinism) and deletes the rest.
--
-- SAFETY
-- ------
-- Three predicates confine this to seed data, and each is load-bearing:
--
--   * `cardinality(source_episodes) = 0` — learned entities came from an
--     episode and always carry its id. This cannot touch anything dreaming
--     produced.
--   * `embedding IS NULL` — anything embedded is reachable and in use.
--   * `count(*) > 1` on the exact (name, type) triple — only genuine
--     duplicates, never a unique row.
--
-- `facts` rows reference `entities.entity_id` with ON DELETE CASCADE
-- (migrations/010, lines 134-135). Seed entities have no facts pointing at
-- them, but the dry run reports any that do so you can see before applying.
--
-- TWO MODES
-- ---------
--   default        Keep the oldest copy of each fact, delete the rest. The
--                  survivors stay unembedded, so they remain unreachable.
--
--   -v reseed=1    Delete ALL copies, including the survivor. Because seeding
--                  is now idempotent per (name, type) AND embeds at write
--                  time, the next server boot recreates exactly one copy of
--                  each fact WITH an embedding — making them retrievable for
--                  the first time. This is the option to use if you want the
--                  seed knowledge actually reaching agents.
--
--                  Safe because these rows are, by the predicates below,
--                  unreachable today: deleting them removes nothing any agent
--                  can currently recall. A fact no longer declared in its
--                  card's `seed_facts` will not come back — which is correct,
--                  since the card is the source of truth for seed data.
--
-- USAGE
--   Dry run:  psql "$URL" -f scripts/loop1_dedupe_seed_entities.sql
--   Dedupe:   psql "$URL" -f scripts/loop1_dedupe_seed_entities.sql -v apply=1
--   Reseed:   psql "$URL" -f scripts/loop1_dedupe_seed_entities.sql -v apply=1 -v reseed=1
--             └─ then restart the server to regenerate them embedded
-- ════════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\pset pager off
\if :{?apply}
\else
  \set apply 0
\endif
\if :{?reseed}
\else
  \set reseed 0
\endif

-- Scope: seed rows that NO retrieval path can currently reach.
--
-- `cep_%` rows are deliberately excluded even though they are also unembedded
-- seed data. `get_top_k_entities_with_cep` injects them unconditionally, so
-- they are working as designed today — `biotech_analyst`'s 22 and
-- `weather_oracle`'s 6 are live knowledge. The safety argument for reseed mode
-- ("deleting removes nothing any agent can currently recall") is true only for
-- the unreachable class, so the scope must match the argument exactly.
CREATE TEMP VIEW seed_rows AS
SELECT e.entity_id, e.agent_id, e.entity_name, e.entity_type, e.t_valid
  FROM entities e
 WHERE e.embedding IS NULL
   AND e.entity_type NOT LIKE 'cep\_%'
   AND (e.source_episodes IS NULL OR cardinality(e.source_episodes) = 0);

-- In reseed mode `rn > 0` selects every row; otherwise the oldest survives.
CREATE TEMP VIEW doomed AS
SELECT entity_id, agent_id, entity_name, entity_type
  FROM (
    SELECT s.*,
           row_number() OVER (
             PARTITION BY s.agent_id, s.entity_name, s.entity_type
             ORDER BY s.t_valid ASC, s.entity_id ASC
           ) AS rn
      FROM seed_rows s
  ) ranked
 WHERE rn > (CASE WHEN :reseed = 1 THEN 0 ELSE 1 END);

\echo ''
\echo '── Mode ─────────────────────────────────────────────────────────────'
SELECT CASE WHEN :reseed = 1
            THEN 'RESEED  — delete every copy; next boot recreates them embedded'
            ELSE 'DEDUPE  — keep oldest copy; survivors stay unembedded'
       END AS mode;

\echo ''
\echo '── Scope: seed entities in range ───────────────────────────────────'
SELECT count(DISTINCT agent_id)                              AS agents,
       count(DISTINCT (agent_id, entity_name, entity_type))  AS distinct_facts,
       count(*)                                              AS rows_to_delete
  FROM doomed;

\echo ''
\echo '── Per agent: before and after ─────────────────────────────────────────'
SELECT a.agent_name,
       count(*)                                                  AS seed_rows_now,
       count(*) FILTER (WHERE d.entity_id IS NOT NULL)           AS to_delete,
       count(*) - count(*) FILTER (WHERE d.entity_id IS NOT NULL) AS will_remain
  FROM seed_rows s
  JOIN agents a  ON a.agent_id = s.agent_id
  LEFT JOIN doomed d ON d.entity_id = s.entity_id
 GROUP BY a.agent_name
 ORDER BY to_delete DESC;

\echo ''
\echo '── Safety check: nothing currently retrievable may be in scope (must be 0)'
\echo '   Covers embedded rows, episode-derived rows, and cep_ rows (which are'
\echo '   injected unconditionally and so are reachable without an embedding).'
SELECT count(*) AS retrievable_rows_in_scope_must_be_zero
  FROM entities e
 WHERE e.entity_id IN (SELECT entity_id FROM doomed)
   AND (e.embedding IS NOT NULL
        OR e.entity_type LIKE 'cep\_%'
        OR (e.source_episodes IS NOT NULL AND cardinality(e.source_episodes) > 0));

\echo ''
\echo '── Safety check: facts that would cascade-delete with these entities ───'
SELECT count(*) AS cascading_facts
  FROM facts f
 WHERE f.source_entity_id IN (SELECT entity_id FROM doomed)
    OR f.target_entity_id IN (SELECT entity_id FROM doomed);

\if :apply
\echo ''
\echo '── APPLYING: deleting duplicate seed entities ──────────────────────────'
DELETE FROM entities WHERE entity_id IN (SELECT entity_id FROM doomed);

\echo ''
\echo '── Post-state: seed entities per agent ─────────────────────────────────'
SELECT a.agent_name, count(*) AS seed_rows
  FROM entities e JOIN agents a ON a.agent_id = e.agent_id
 WHERE e.embedding IS NULL
   AND (e.source_episodes IS NULL OR cardinality(e.source_episodes) = 0)
 GROUP BY 1 ORDER BY 2 DESC;

\if :reseed
\echo ''
\echo 'NEXT STEP: restart the server. Seeding is idempotent per (name, type) and'
\echo 'now embeds at write time, so boot will recreate one copy of each fact'
\echo 'WITH an embedding. Then re-run:'
\echo '  PROBE_FILE=scripts/loop1_retrievability_census.sql scripts/run_loop5_probe.sh'
\echo 'and confirm the "seed, no cep_ prefix (UNREACHABLE)" class is gone.'
\else
\echo ''
\echo 'Note: deduping alone does not make these retrievable. Survivors are still'
\echo 'unembedded. Re-run with -v reseed=1 to have boot regenerate them with'
\echo 'embeddings. See scripts/loop1_retrievability_census.sql.'
\endif
\else
\echo ''
\echo '════════════════════════════════════════════════════════════════════════'
\echo ' DRY RUN — nothing written. Re-run with  -v apply=1  to apply.'
\echo '════════════════════════════════════════════════════════════════════════'
\endif
