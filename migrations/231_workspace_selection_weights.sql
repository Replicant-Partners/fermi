-- Migration 231: workspace-level selection weights for Loop 4B.
--
-- Stores per-workspace scoring weights for the select_agent tool.
-- When set, these override the hardcoded defaults in execute_select_agent.
-- Loop 4B (selection performance consolidation) will eventually auto-update
-- these based on observed selection outcomes.
--
-- Shape: {"brier": 0.40, "cost": 0.20, "valence_fit": 0.20, "fidelity": 0.20}
-- NULL = use platform defaults.
ALTER TABLE teams ADD COLUMN IF NOT EXISTS selection_weights jsonb;
