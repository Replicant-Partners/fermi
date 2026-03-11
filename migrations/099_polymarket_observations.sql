-- Migration 099: Polymarket Market Observations
--
-- Append-only table recording every Polymarket price snapshot taken
-- in the context of a Fermi forecast. Never mutated after insert.
--
-- Enables:
--   - Divergence tracking over time (Fermi probability vs crowd price)
--   - Calibration against crowd wisdom (was the crowd or your model right?)
--   - Historical price series for resolved forecasts
--   - Auto-resolution bridge (detect when PM market resolves)
--
-- Architecture:
--   - Append-only: rows are never updated or deleted
--   - Stateless: each observation is a complete snapshot
--   - Server-side: ABW fetches from Gamma API, console never calls PM directly
--   - Linked via forecast_id (optional) and pm_market_id (always present)

-- ═══════════════════════════════════════════════════════════════════
-- MARKET OBSERVATIONS (append-only price snapshots)
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS fermi_market_observations (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,

    -- Link to Fermi forecast (optional — can be exploratory search)
    forecast_id TEXT REFERENCES fermi_forecasts(id) ON DELETE SET NULL,

    -- Polymarket identifiers (immutable per market)
    pm_event_id TEXT NOT NULL,
    pm_market_id TEXT NOT NULL,
    pm_condition_id TEXT,
    pm_slug TEXT,

    -- The question being asked on Polymarket (snapshot — may differ from Fermi question)
    pm_question TEXT NOT NULL,
    pm_event_title TEXT,

    -- Market state at observation time
    market_price REAL NOT NULL
        CHECK (market_price >= 0 AND market_price <= 1),
    bid_price REAL
        CHECK (bid_price IS NULL OR (bid_price >= 0 AND bid_price <= 1)),
    ask_price REAL
        CHECK (ask_price IS NULL OR (ask_price >= 0 AND ask_price <= 1)),
    midpoint_price REAL
        CHECK (midpoint_price IS NULL OR (midpoint_price >= 0 AND midpoint_price <= 1)),
    spread REAL,

    -- Volume and liquidity (USD)
    volume_total REAL,
    volume_24h REAL,
    liquidity REAL,

    -- Price momentum at observation time
    price_change_1h REAL,
    price_change_1d REAL,
    price_change_1w REAL,
    price_change_1m REAL,

    -- Market lifecycle
    pm_end_date TIMESTAMPTZ,
    pm_active BOOLEAN NOT NULL DEFAULT true,
    pm_closed BOOLEAN NOT NULL DEFAULT false,
    pm_resolved BOOLEAN NOT NULL DEFAULT false,
    pm_outcome TEXT,              -- "Yes"/"No" once resolved

    -- Fermi state at observation time (for divergence tracking)
    fermi_probability REAL
        CHECK (fermi_probability IS NULL OR (fermi_probability >= 0 AND fermi_probability <= 1)),
    divergence_pp REAL,           -- (fermi_probability - market_price) * 100

    -- Confidence classification derived from volume + spread
    confidence_signal TEXT
        CHECK (confidence_signal IS NULL OR confidence_signal IN (
            'very_high', 'high', 'medium', 'low'
        )),

    -- Who triggered this observation
    observer_id TEXT NOT NULL REFERENCES users(user_id),

    -- How this observation was created
    observation_type TEXT NOT NULL DEFAULT 'search'
        CHECK (observation_type IN (
            'search',           -- user searched PM from portfolio panel
            'import',           -- user imported this PM question into Fermi
            'manual_link',      -- user linked an existing forecast to a PM market
            'refresh',          -- user or system refreshed a linked market price
            'scheduled',        -- automated periodic snapshot (future)
            'agent_research',   -- an orchestra agent pulled this during execution
            'resolution_check'  -- system checked if market has resolved
        )),

    -- Tags and metadata
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Indexes ───────────────────────────────────────────────────────

-- Find all observations for a forecast (divergence time series)
CREATE INDEX IF NOT EXISTS idx_market_obs_forecast
    ON fermi_market_observations(forecast_id, created_at)
    WHERE forecast_id IS NOT NULL;

-- Find all observations for a Polymarket market
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_market
    ON fermi_market_observations(pm_market_id, created_at);

-- Find all observations for a Polymarket event (may have multiple markets)
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_event
    ON fermi_market_observations(pm_event_id);

-- Find observations by user
CREATE INDEX IF NOT EXISTS idx_market_obs_observer
    ON fermi_market_observations(observer_id, created_at);

-- Find observations by type
CREATE INDEX IF NOT EXISTS idx_market_obs_type
    ON fermi_market_observations(observation_type);

-- Find unresolved linked markets (for resolution checker)
CREATE INDEX IF NOT EXISTS idx_market_obs_unresolved
    ON fermi_market_observations(pm_market_id, created_at)
    WHERE forecast_id IS NOT NULL
      AND pm_closed = false
      AND pm_resolved = false;

-- Find resolved observations (for Brier analysis)
CREATE INDEX IF NOT EXISTS idx_market_obs_resolved
    ON fermi_market_observations(forecast_id, created_at)
    WHERE pm_resolved = true;

-- ═══════════════════════════════════════════════════════════════════
-- CREDIT LEDGER: Add new tx_types for Polymarket operations
-- ═══════════════════════════════════════════════════════════════════
--
-- Uses DO block for PgBouncer transaction-mode safety.

DO $$
BEGIN
    ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
    ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
        CHECK (tx_type IN (
            'deposit', 'withdrawal',
            'execution_fee', 'gas_fee',
            'education_alloc', 'education_spend',
            'transfer_out', 'transfer_in',
            'grant', 'refund',
            'fork_royalty', 'fork_fee',
            'publish_fee', 'eval_fee',
            'consolidation_fee',
            'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
            'avatar_generate', 'embedding_import',
            'ontology_generation', 'prompt_generation', 'file_write',
            'creature_mint', 'creature_flight', 'creature_animate',
            'swarm_create', 'swarm_join', 'swarm_session_create', 'swarm_telemetry_ingest',
            'collection_create', 'rabble_chat',
            'gbif_contribution', 'rabble_platform_fee',
            'formation_activate', 'attraction_reward',
            'akp_alignment', 'akp_transfer', 'akp_bootstrap', 'akp_diff',
            'observation_session_create', 'observation_ingest',
            'flight_plan',
            'perch', 'fly', 'walk_in_fee', 'walk_in_revenue',
            'tether',
            'platform_read',
            'execution_royalty', 'agent_collaboration_payout',
            'agent_collect_out', 'agent_collect_in',
            'agent_allocate_dream', 'agent_allocate_education', 'agent_allocate_coherence',
            'creature_art',
            -- Polymarket integration (new in 099)
            'polymarket_search',
            'polymarket_import',
            'polymarket_snapshot'
        ));
END $$;
