-- Migration 126: Extend agent_versions to capture full capability config.
--
-- The current snapshot (migration 024) captures: system_prompt, tags,
-- model, temperature, changed_by. It misses the fields that also determine
-- agent behaviour: model_ladder, capability_gates, min_tier, output_contract,
-- and the human-readable version string.
--
-- An observation (eval score, anomaly, coherence signal) is only meaningful
-- relative to the full configuration that produced it — not just the prompt.
-- Changing model_ladder or min_tier changes behaviour as significantly as
-- changing the system_prompt, but was previously invisible in version history.
--
-- See docs/architecture/AGENT_VERSION_HISTORY.md for the full design.
--
-- PgBouncer-safe: each ALTER is a single statement. All columns nullable
-- so existing rows are unaffected.

ALTER TABLE public.agent_versions
    ADD COLUMN IF NOT EXISTS model_ladder     JSONB;

ALTER TABLE public.agent_versions
    ADD COLUMN IF NOT EXISTS capability_gates JSONB;

ALTER TABLE public.agent_versions
    ADD COLUMN IF NOT EXISTS min_tier         TEXT;

ALTER TABLE public.agent_versions
    ADD COLUMN IF NOT EXISTS output_contract  JSONB;

-- The human-readable semver string (e.g. "2.0.0") — distinct from the
-- auto-incrementing version_number integer. Lets developers see "v2.0.0"
-- in the history rather than "version 7".
ALTER TABLE public.agent_versions
    ADD COLUMN IF NOT EXISTS version_string   TEXT;
