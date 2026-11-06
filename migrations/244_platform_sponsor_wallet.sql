-- 244 — platform sponsor wallet and sponsorship ledger (SPEC 36 R0.1 / G01,
-- directive §4 "Do now", as amended 2026-11-06).
--
-- App templates declare an `initial_budget` (fermi_forecast 500, kask_simops
-- 1500, …). Until now spawn deposited it from nowhere: credits were minted, no
-- wallet was debited, and the ledger total rose by the budget [V: the deployed
-- probe `tests/claims/deployed.rs::deployed_app_spawn_does_not_mint_credits`
-- observed 201 + ledger +505 for a zero-balance caller at a3cea516].
--
-- Decision (owner, 2026-11-06): the platform sponsors the *template* budget,
-- up to a per-caller quota; beyond it, or when the sponsor is short, the
-- caller pays. Sponsorship is a real transfer out of a real wallet (topped up
-- only by ledgered grants), so it is visible, bounded and auditable.
--
-- `app_sponsorships` is the quota's source of truth: one row per sponsored
-- spawn, keyed to the beneficiary, written in the spawn's transaction. It is
-- deliberately NOT tied to `teams` by a cascading FK — deleting a workspace
-- must not refund the caller's quota slot (review of #132, finding 1).
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

    -- owner_id is unique across owner types; refuse to adopt a same-named
    -- wallet of another type as the sponsor.
    IF EXISTS (SELECT 1 FROM wallets
               WHERE owner_id = 'platform:app_sponsorship' AND owner_type <> 'platform') THEN
        RAISE EXCEPTION 'wallet platform:app_sponsorship exists with owner_type <> platform';
    END IF;

    CREATE TABLE IF NOT EXISTS app_sponsorships (
        sponsorship_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        sponsor_wallet  UUID NOT NULL REFERENCES wallets(wallet_id),
        beneficiary_id  TEXT NOT NULL,          -- the caller (users.user_id)
        app_slug        TEXT NOT NULL,
        workspace_id    UUID NOT NULL,          -- no FK: survives workspace deletion
        amount          INTEGER NOT NULL CHECK (amount > 0),
        ledger_tx_id    UUID NOT NULL REFERENCES credit_ledger(tx_id),
        created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
    );
    CREATE INDEX IF NOT EXISTS idx_app_sponsorships_beneficiary
        ON app_sponsorships (sponsor_wallet, beneficiary_id, created_at);

    -- Append-only, enforced (directive rule 1 / SPEC 36 I3): the quota must not
    -- be resettable by editing history.
    CREATE OR REPLACE FUNCTION app_sponsorships_immutable() RETURNS trigger AS $f$
    BEGIN
        RAISE EXCEPTION 'app_sponsorships is append-only';
    END;
    $f$ LANGUAGE plpgsql;
    DROP TRIGGER IF EXISTS trg_app_sponsorships_immutable ON app_sponsorships;
    CREATE TRIGGER trg_app_sponsorships_immutable
        BEFORE UPDATE OR DELETE ON app_sponsorships
        FOR EACH ROW EXECUTE FUNCTION app_sponsorships_immutable();
    DROP TRIGGER IF EXISTS trg_app_sponsorships_no_truncate ON app_sponsorships;
    CREATE TRIGGER trg_app_sponsorships_no_truncate
        BEFORE TRUNCATE ON app_sponsorships
        FOR EACH STATEMENT EXECUTE FUNCTION app_sponsorships_immutable();
    -- Interim: the app role owns this table and could drop these triggers
    -- until database roles are separated (G15 / SPEC 36 R2.8, directive
    -- Phase 1). In Phase 1/3 this table becomes a projection of ledger events.
END $$;
