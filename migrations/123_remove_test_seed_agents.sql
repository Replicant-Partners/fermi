-- Migration 123: Remove test regression-fixture agents from production.
--
-- seed_market_research, seed_geopolitical_risk, and seed_crypto_sentiment
-- are regression test fixtures defined in agent-bestiary/memory/src/seed.rs
-- (SeedData::build()). They were accidentally written to the production
-- database and appear in the public catalogue under "Alena Taranka"
-- (the deterministic test-user name in seed.rs).
--
-- These agents have no agent_card.json on disk, are not in the curated
-- bestiary, and will not be re-seeded by seed_agents_to_database().
-- Safe to hard-delete: cascade will clean up any related eval_runs,
-- episodes, ontology_snapshots, workspace_agents rows, etc.
--
-- Idempotent: DELETE WHERE agent_name IN (...) is a no-op if already gone.

DELETE FROM public.agents
 WHERE agent_name IN (
     'seed_market_research',
     'seed_geopolitical_risk',
     'seed_crypto_sentiment'
 );
