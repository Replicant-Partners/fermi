-- 244 — platform sponsor wallet (SPEC 36 R0.1 / G01, directive §4 "Do now").
--
-- App templates declare an `initial_budget` (fermi_forecast 500, kask_simops
-- 1500, …). Until now spawn deposited it from nowhere: credits were minted, no
-- wallet was debited, and the ledger total rose by the budget [V: the deployed
-- probe `tests/claims/deployed.rs::deployed_app_spawn_does_not_mint_credits`
-- observed 201 + ledger +505 for a zero-balance caller at a3cea516].
--
-- Decision (owner, 2026-11-06, option c): the platform sponsors the
-- *template* budget; the caller pays any `extra_budget`. Sponsorship is a real
-- transfer out of a real wallet, so it is visible, bounded and auditable:
--   * the platform wallet has a finite balance, topped up by an explicit,
--     ledgered `grant` (tx_type 'grant', description names the operator);
--   * when it is empty, spawn returns 402 instead of minting.
--
-- Atomic: a DO block, because the runner executes each file through a pooler
-- (see migration 205's note on why multi-statement files use DO $$).

DO $$
BEGIN
    ALTER TABLE wallets DROP CONSTRAINT IF EXISTS wallets_owner_type_check;
    ALTER TABLE wallets ADD CONSTRAINT wallets_owner_type_check
        CHECK (owner_type = ANY (ARRAY['user', 'workspace', 'agent', 'platform']));

    INSERT INTO wallets (owner_type, owner_id)
    VALUES ('platform', 'platform:app_sponsorship')
    ON CONFLICT (owner_id) DO NOTHING;
END $$;
