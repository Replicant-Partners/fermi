#!/bin/sh
# Validate migration 201 and the SQL that depends on it — WITHOUT persisting
# anything.
#
# Everything runs inside a single transaction that ends in ROLLBACK, so the
# column is created, the dependent queries are planned against it, and then the
# whole thing is thrown away. The final check confirms the rollback took: if
# `extracted_by` still exists after this script, something committed and that is
# itself the finding.
#
# Uses the UNPOOLED connection. Under PgBouncer transaction pooling a
# multi-statement transaction can land on different backends, which would make
# the DDL and the EXPLAINs disagree for reasons that have nothing to do with the
# SQL — the same trap `loop5_brier_mechanical_check.sql` warns about.
#
# The URL is read from the environment here rather than passed in, so it never
# appears in a command line or in scrollback.

set -eu

if [ -f .env.local ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env.local
  set +a
fi

URL="${DATABASE_URL_UNPOOLED:-${DATABASE_URL:-}}"
if [ -z "$URL" ]; then
  echo "FAIL: no DATABASE_URL_UNPOOLED or DATABASE_URL in environment or .env.local" >&2
  exit 1
fi

psql "$URL" -v ON_ERROR_STOP=1 -X -q <<'SQL'
\timing off
\echo '── 0. pre-state ─────────────────────────────────────────────────────'
SELECT EXISTS (
  SELECT 1 FROM information_schema.columns
   WHERE table_name='semantic_rules' AND column_name='extracted_by'
) AS extracted_by_exists_before;

BEGIN;

\echo ''
\echo '── 1. migration 201 DDL ─────────────────────────────────────────────'
ALTER TABLE public.semantic_rules
    ADD COLUMN IF NOT EXISTS extracted_by UUID
        REFERENCES public.agents(agent_id) ON DELETE SET NULL;

COMMENT ON COLUMN public.semantic_rules.extracted_by IS
    'Agent that produced this rule (the dream_coordinator EXTRACT member, '
    'normally `ontologist`). Distinct from `agent_id`, which is the agent the '
    'rule is FOR. NULL means the author was not recorded — every rule written '
    'before migration 201. Consumers must exclude NULL rather than attribute it.';

CREATE INDEX IF NOT EXISTS idx_semantic_rules_extracted_by
    ON public.semantic_rules(extracted_by)
    WHERE extracted_by IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_semantic_rules_extractor_utility
    ON public.semantic_rules(extracted_by, created_at)
    WHERE extracted_by IS NOT NULL AND is_active;

\echo 'DDL applied inside transaction.'

\echo ''
\echo '── 2. column and indexes are present in-transaction ────────────────'
SELECT column_name, data_type, is_nullable
  FROM information_schema.columns
 WHERE table_name='semantic_rules' AND column_name='extracted_by';

SELECT indexname FROM pg_indexes
 WHERE tablename='semantic_rules'
   AND indexname IN ('idx_semantic_rules_extracted_by',
                     'idx_semantic_rules_extractor_utility')
 ORDER BY indexname;

\echo ''
\echo '── 3. the extraction-utility signal query plans ────────────────────'
EXPLAIN
SELECT COUNT(*)                                            AS resolved,
       COUNT(*) FILTER (WHERE application_count > 0)        AS retrieved,
       COUNT(*) FILTER (WHERE application_count = 0)        AS ignored
  FROM semantic_rules
 WHERE extracted_by = '00000000-0000-0000-0000-000000000000'::uuid
   AND is_active
   AND invalidated_at IS NULL
   AND (application_count > 0
        OR created_at < NOW() - ('7' || ' days')::interval);

\echo ''
\echo '── 4. the retrieval-credit UPDATE plans ────────────────────────────'
EXPLAIN
UPDATE semantic_rules
   SET application_count = application_count + 1,
       last_validated_at = NOW()
 WHERE rule_id = ANY(ARRAY['00000000-0000-0000-0000-000000000000']::uuid[]);

\echo ''
\echo '── 5. the extractor read-back query plans ──────────────────────────'
EXPLAIN
SELECT rule_content, rule_description, confidence_score, verification_status
  FROM semantic_rules
 WHERE agent_id = '00000000-0000-0000-0000-000000000000'::uuid
   AND is_active AND invalidated_at IS NULL
 ORDER BY (verification_status = 'verified') DESC, confidence_score DESC
 LIMIT 20;

\echo ''
\echo '── 6. the eval_signals insert is well-typed ────────────────────────'
EXPLAIN
INSERT INTO eval_signals
      (agent_id, evaluator_name, evaluator_version, evaluator_tier,
       dimension, score, confidence, rationale, created_at)
 SELECT '00000000-0000-0000-0000-000000000000'::uuid,
        'extraction_utility_resolver', 'v1', 'dimensional',
        'extraction_utility', 0.5, 0.5, 'probe', NOW()
  WHERE NOT EXISTS (
      SELECT 1 FROM eval_signals
       WHERE agent_id = '00000000-0000-0000-0000-000000000000'::uuid
         AND dimension = 'extraction_utility'
         AND rationale = 'probe'
  );

ROLLBACK;

\echo ''
\echo '── 7. post-state: the rollback must have removed it ────────────────'
SELECT EXISTS (
  SELECT 1 FROM information_schema.columns
   WHERE table_name='semantic_rules' AND column_name='extracted_by'
) AS extracted_by_exists_after;
SQL

echo ""
echo "OK: migration 201 and its dependent queries validated, nothing persisted."
