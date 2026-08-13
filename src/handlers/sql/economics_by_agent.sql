-- Per-agent cost joined to credit revenue, over a window.
--
-- Single source of truth: `include_str!`d by src/handlers/economics.rs
-- AND executed verbatim by scripts/smoke_economics.sh.
--
-- $1 = window in days (text, e.g. '30')
-- $2 = optional funding principal filter (text or NULL)
--
-- Revenue is matched via credit_ledger.related_id = episode_id (set by
-- gas::charge_gas), so a fee is only counted when it is genuinely tied
-- to a run in-window. `execution_fee` rows are stored negative (a debit
-- on the caller's wallet — see fermi-auth/src/credits.rs binding
-- `-amount`), hence SUM(-amount) to get a positive revenue figure.
WITH ep AS (
    SELECT e.episode_id, e.agent_id, e.tokens_used, e.cost_usd,
           COALESCE(NULLIF(e.context->>'funding_principal', ''), 'unattributed')
               AS funding_principal,
           e.context->>'provider'   AS provider,
           e.context->>'model_used' AS model
      FROM episodes e
     WHERE e.created_at >= NOW() - ($1 || ' days')::interval
       AND ($2::text IS NULL
            OR COALESCE(NULLIF(e.context->>'funding_principal', ''), 'unattributed') = $2)
),
fees AS (
    SELECT l.related_id, SUM(-l.amount) AS fee_credits
      FROM credit_ledger l
     WHERE l.tx_type = 'execution_fee'
       AND l.created_at >= NOW() - ($1 || ' days')::interval
     GROUP BY l.related_id
)
SELECT a.agent_name,
       a.tier,
       a.user_id                        AS owner_id,
       ep.funding_principal,
       MAX(ep.provider)                 AS provider,
       MAX(ep.model)                    AS model,
       COUNT(*)                         AS executions,
       COALESCE(SUM(ep.tokens_used), 0) AS tokens,
       COALESCE(SUM(ep.cost_usd), 0)    AS cost_usd,
       COALESCE(SUM(f.fee_credits), 0)  AS fee_credits
  FROM ep
  JOIN agents a ON a.agent_id = ep.agent_id
  LEFT JOIN fees f ON f.related_id = ep.episode_id::text
 GROUP BY a.agent_name, a.tier, a.user_id, ep.funding_principal
 ORDER BY cost_usd DESC
 LIMIT 200
