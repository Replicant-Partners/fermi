-- ═══════════════════════════════════════════════════════════════════════
-- Is the ontologist's extraction credential actually funded? READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- Loop 1's ability to LEARN (as opposed to merely run) depends entirely on one
-- credential. The chain, from src/handlers/consolidation.rs:54:
--
--   build_extraction_llm()
--     -> dream_member("semantic-rules") ............ the `ontologist` agent
--     -> resolve_credential(state, agent, provider)
--          -> funding_principal_for(agent) ......... system tier => platform principal
--          -> agent_credentials WHERE principal_id = <platform>
--                                 AND provider = 'openai'
--                                 AND (scope = 'ontologist' OR scope = '*')
--
-- Every step is `?` on an Option. Any miss returns None, and the caller then
-- runs consolidation WITHOUT an extractor: episodes are consumed, the job
-- completes, a credit is debited, and nothing is learned.
--
-- So the answer to "will it fail if I don't have a funded API call for the
-- responsible agents?" is: it will not fail — it will silently succeed at
-- nothing. That is what this checks for, before spending anything.
--
-- Run: PROBE_FILE=scripts/loop1_extractor_readiness.sql scripts/run_loop5_probe.sh
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '── 1. Does the credential store even exist, and what is in it? ─────────'
\echo '   (values are encrypted; only presence and scope are shown)'
SELECT principal_id,
       provider,
       scope,
       count(*) AS keys
  FROM agent_credentials
 GROUP BY principal_id, provider, scope
 ORDER BY principal_id, provider, scope;

\echo ''
\echo '── 2. The specific lookup build_extraction_llm performs ────────────────'
\echo '   ontologist is system-tier => funded by the platform principal.'
\echo '   A row here means extraction can work; no row means silent no-op.'
SELECT a.agent_name,
       a.tier,
       COALESCE(c.provider, '(none)') AS credential_provider,
       COALESCE(c.scope, '(none)')    AS credential_scope,
       CASE WHEN c.provider IS NULL
            THEN 'NO CREDENTIAL -> consolidation will run and learn nothing'
            ELSE 'ready' END AS verdict
  FROM agents a
  LEFT JOIN agent_credentials c
    ON c.provider = 'openai'
   AND (c.scope = a.agent_name OR c.scope = '*')
 WHERE a.agent_name = 'ontologist';

\echo ''
\echo '── 3. Every system-tier agent that needs a provider key ────────────────'
\echo '   Same failure mode applies to any platform-funded agent whose'
\echo '   credential is missing.'
SELECT a.agent_name, a.tier,
       CASE WHEN EXISTS (
              SELECT 1 FROM agent_credentials c
               WHERE (c.scope = a.agent_name OR c.scope = '*')
            ) THEN 'has a key' ELSE 'NO KEY' END AS credential
  FROM agents a
 WHERE a.tier = 'system'
   AND a.status <> 'archived'
 ORDER BY credential, a.agent_name
 LIMIT 25;
