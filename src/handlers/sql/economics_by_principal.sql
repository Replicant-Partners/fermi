-- Cost by funding principal, over a window.
--
-- Single source of truth: `include_str!`d by src/handlers/economics.rs
-- AND executed verbatim by scripts/smoke_economics.sh. Editing this file
-- changes both, so the smoke test can never drift from the handler.
--
-- $1 = window in days (text, e.g. '30')
-- $2 = optional funding principal filter (text or NULL)
--
-- COALESCE to 'unattributed' rather than dropping the row: episodes
-- written before SPEC_28, or by paths that never set the field, still
-- represent real spend and hiding them would understate cost.
SELECT COALESCE(NULLIF(e.context->>'funding_principal', ''), 'unattributed')
            AS funding_principal,
       COUNT(*)                                   AS executions,
       COALESCE(SUM(e.tokens_used), 0)            AS tokens,
       COALESCE(SUM(e.cost_usd), 0)               AS cost_usd,
       COUNT(*) FILTER (WHERE e.cost_usd IS NULL) AS missing_cost
  FROM episodes e
 WHERE e.created_at >= NOW() - ($1 || ' days')::interval
   AND ($2::text IS NULL
        OR COALESCE(NULLIF(e.context->>'funding_principal', ''), 'unattributed') = $2)
 GROUP BY 1
 ORDER BY cost_usd DESC
