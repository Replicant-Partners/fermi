-- Royalty credits paid out, by recipient wallet owner, over a window.
--
-- Single source of truth: `include_str!`d by src/handlers/economics.rs
-- AND executed verbatim by scripts/smoke_economics.sh.
--
-- $1 = window in days (text, e.g. '30')
--
-- `agent_royalty_in` is the tx_type used by gas::charge_execution_with_royalty
-- when depositing to the owner's wallet. Deposits are stored positive.
SELECT w.owner_id,
       COALESCE(SUM(l.amount), 0) AS royalty_credits
  FROM credit_ledger l
  JOIN wallets w ON w.wallet_id = l.wallet_id
 WHERE l.tx_type = 'agent_royalty_in'
   AND l.created_at >= NOW() - ($1 || ' days')::interval
 GROUP BY w.owner_id
 ORDER BY royalty_credits DESC
 LIMIT 50
