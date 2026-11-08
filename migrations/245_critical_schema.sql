-- 245: the statements `ensure_critical_schema` ran on every boot
-- (src/api_server.rs @ 59d76b00), moved into the ledger-tracked sequence
-- (SPEC 36 G16 / #120). Each was an idempotent ensure; together they apply
-- cleanly in one transaction on the dev DB (verified with BEGIN … ROLLBACK,
-- 2026-11-08). Runs last, as the function did, after every other migration.

-- teams.mission
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS mission TEXT;

-- teams.coordination_strategist_id
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS coordination_strategist_id UUID;

-- teams.strategist_assigned_at
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS strategist_assigned_at TIMESTAMPTZ;

-- composition_versions.table
CREATE TABLE IF NOT EXISTS public.composition_versions ( composition_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), workspace_id UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE, version_number INT NOT NULL, mission TEXT, coordination_strategist_id UUID, member_agent_ids UUID[], member_weights JSONB, diff_summary TEXT, proposed_by TEXT, accepted_by TEXT, rejected_by TEXT, rejection_note TEXT, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW() );

-- composition_versions.rejected_by
ALTER TABLE public.composition_versions ADD COLUMN IF NOT EXISTS rejected_by TEXT;

-- composition_versions.rejection_note
ALTER TABLE public.composition_versions ADD COLUMN IF NOT EXISTS rejection_note TEXT;

-- harness_snapshots.table
CREATE TABLE IF NOT EXISTS public.harness_snapshots ( snapshot_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), content_hash TEXT NOT NULL UNIQUE, conductor_card_hash TEXT NOT NULL, routing_weights_hash TEXT, specialist_roster_hash TEXT NOT NULL, bayesops_params_hash TEXT, conductor_version TEXT NOT NULL, specialist_roster JSONB NOT NULL, routing_weights JSONB, bayesops_params JSONB, parent_hash TEXT, surface_changed TEXT, change_rationale TEXT, captured_at TIMESTAMPTZ NOT NULL DEFAULT NOW() );

-- fermi_forecast_updates.table
CREATE TABLE IF NOT EXISTS public.fermi_forecast_updates ( id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, previous_probability REAL NOT NULL, new_probability REAL NOT NULL, reason TEXT, agent_id TEXT, evidence_added JSONB, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), revision_trigger TEXT );

-- fermi_forecast_updates.recheck_revision_trigger
DO $$ BEGIN ALTER TABLE public.fermi_forecast_updates DROP CONSTRAINT IF EXISTS fermi_forecast_updates_revision_trigger_check; ALTER TABLE public.fermi_forecast_updates ADD CONSTRAINT fermi_forecast_updates_revision_trigger_check CHECK ( revision_trigger IS NULL OR revision_trigger IN ( 'initial', 'evidence_update', 'agent_correction', 'schedule_rerun', 'manual', 'bayesops_refit', 'cascade', 'cascade_undo' ) ); END $$;

-- fermi_forecast_updates.idx_forecast
CREATE INDEX IF NOT EXISTS idx_forecast_updates_forecast ON public.fermi_forecast_updates(forecast_id);

-- fermi_forecast_updates.idx_time
CREATE INDEX IF NOT EXISTS idx_forecast_updates_time ON public.fermi_forecast_updates(created_at);

-- forecast_relationships.table
CREATE TABLE IF NOT EXISTS public.forecast_relationships ( id UUID PRIMARY KEY DEFAULT gen_random_uuid(), kind TEXT NOT NULL, forecast_ids TEXT[] NOT NULL, parameters JSONB NOT NULL DEFAULT '{}'::jsonb, description TEXT, owner_id TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), archived_at TIMESTAMPTZ );

-- forecast_relationships.idx_kind
CREATE INDEX IF NOT EXISTS idx_relationships_kind ON public.forecast_relationships(kind);

-- forecast_relationships.idx_owner
CREATE INDEX IF NOT EXISTS idx_relationships_owner ON public.forecast_relationships(owner_id);

-- forecast_relationships.idx_forecast_ids
CREATE INDEX IF NOT EXISTS idx_relationships_forecast_ids ON public.forecast_relationships USING gin (forecast_ids);

-- pending_cascades.table
CREATE TABLE IF NOT EXISTS public.pending_cascades ( id UUID PRIMARY KEY DEFAULT gen_random_uuid(), relationship_id UUID NOT NULL REFERENCES public.forecast_relationships(id) ON DELETE CASCADE, trigger_forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, trigger_kind TEXT NOT NULL, outcome BOOLEAN, source TEXT NOT NULL DEFAULT 'manual', status TEXT NOT NULL DEFAULT 'pending', owner_id TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), decided_at TIMESTAMPTZ, decided_by TEXT, notes TEXT, proposed_snapshot JSONB );

-- pending_cascades.idx_status
CREATE INDEX IF NOT EXISTS idx_pending_cascades_status ON public.pending_cascades(status);

-- pending_cascades.idx_owner
CREATE INDEX IF NOT EXISTS idx_pending_cascades_owner ON public.pending_cascades(owner_id, status, created_at DESC);

-- pending_cascades.idx_trigger
CREATE INDEX IF NOT EXISTS idx_pending_cascades_trigger ON public.pending_cascades(trigger_forecast_id);

-- pending_cascades.idx_relationship
CREATE INDEX IF NOT EXISTS idx_pending_cascades_relationship ON public.pending_cascades(relationship_id);

-- pending_cascades.recheck_status
DO $$ BEGIN ALTER TABLE public.pending_cascades DROP CONSTRAINT IF EXISTS pending_cascades_status_check; ALTER TABLE public.pending_cascades ADD CONSTRAINT pending_cascades_status_check CHECK (status IN ('pending', 'applied', 'dismissed', 'superseded')); END $$;

-- forecast_commitments.table
CREATE TABLE IF NOT EXISTS public.forecast_commitments ( commitment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), forecast_id TEXT REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, revision_id TEXT REFERENCES public.fermi_forecast_updates(id) ON DELETE SET NULL, predicted_probability REAL NOT NULL, fpl_source_hash TEXT, harness_snapshot_id UUID REFERENCES public.harness_snapshots(snapshot_id), commitment_hash TEXT NOT NULL UNIQUE, anchor_method TEXT NOT NULL DEFAULT 'db_timestamp', anchor_ref TEXT, anchor_note TEXT, emitted_at TIMESTAMPTZ NOT NULL, committed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), sosa_projection_id TEXT, CONSTRAINT commitment_has_subject CHECK ( forecast_id IS NOT NULL OR sosa_projection_id IS NOT NULL ) );

-- forecast_commitments.idx_forecast
CREATE INDEX IF NOT EXISTS idx_forecast_commitments_forecast ON public.forecast_commitments(forecast_id, committed_at DESC);

-- forecast_commitments.idx_hash
CREATE INDEX IF NOT EXISTS idx_forecast_commitments_hash ON public.forecast_commitments(commitment_hash);

-- forecast_splits.table
CREATE TABLE IF NOT EXISTS public.forecast_splits ( forecast_id TEXT PRIMARY KEY REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, split TEXT NOT NULL CHECK (split IN ('held_in','held_out','validation')), split_hash_input TEXT NOT NULL, split_salt TEXT NOT NULL, assigned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), contamination_status TEXT NOT NULL DEFAULT 'pending' CHECK (contamination_status IN ('pending','clean','contaminated','exempt')), probe_transcript TEXT, evidence_freeze_cutoff TIMESTAMPTZ );

-- forecast_spacetime.table
CREATE TABLE IF NOT EXISTS public.forecast_spacetime ( spacetime_id UUID PRIMARY KEY DEFAULT gen_random_uuid(), forecast_id TEXT NOT NULL REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE, revision_seq INTEGER NOT NULL DEFAULT 0, predicted_probability REAL NOT NULL, previous_probability REAL, revision_trigger TEXT, revision_reason TEXT, triggering_agent TEXT, evidence_delta JSONB, drivers_snapshot JSONB, base_rate_snapshot JSONB, fpl_snapshot TEXT, sobol_snapshot JSONB, harness_snapshot_id UUID REFERENCES public.harness_snapshots(snapshot_id), brier_at_this_point REAL, loop1_signal JSONB, loop3_coherence REAL, loop5_calibration JSONB, committed_at TIMESTAMPTZ, revision_ts TIMESTAMPTZ NOT NULL DEFAULT NOW(), UNIQUE (forecast_id, revision_seq) );

-- forecast_spacetime.idx_forecast
CREATE INDEX IF NOT EXISTS idx_spacetime_forecast ON public.forecast_spacetime(forecast_id, revision_seq ASC);

-- forecast_spacetime.trigger_fn
CREATE OR REPLACE FUNCTION public.fn_forecast_spacetime_on_update() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN INSERT INTO public.forecast_spacetime ( forecast_id, revision_seq, predicted_probability, previous_probability, revision_trigger, revision_reason, triggering_agent, evidence_delta, fpl_snapshot, revision_ts ) SELECT NEW.forecast_id, COALESCE(( SELECT MAX(revision_seq) + 1 FROM public.forecast_spacetime WHERE forecast_id = NEW.forecast_id ), 1), NEW.new_probability, NEW.previous_probability, COALESCE(NEW.revision_trigger, 'evidence_update'), NEW.reason, NEW.agent_id, NEW.evidence_added, (SELECT fpl_source FROM public.fermi_forecasts WHERE id = NEW.forecast_id), NEW.created_at; RETURN NEW; END; $$;

-- forecast_spacetime.trigger_drop
DROP TRIGGER IF EXISTS trg_forecast_spacetime ON public.fermi_forecast_updates;

-- forecast_spacetime.trigger_create
CREATE TRIGGER trg_forecast_spacetime AFTER INSERT ON public.fermi_forecast_updates FOR EACH ROW EXECUTE FUNCTION public.fn_forecast_spacetime_on_update();

-- fn.compute_brier_score
CREATE OR REPLACE FUNCTION public.compute_brier_score( predicted REAL, actual BOOLEAN ) RETURNS REAL AS $$ BEGIN RETURN (predicted - (CASE WHEN actual THEN 1.0 ELSE 0.0 END)) ^ 2; END; $$ LANGUAGE plpgsql IMMUTABLE;

-- fn.resolve_forecast
CREATE OR REPLACE FUNCTION public.resolve_forecast( p_forecast_id TEXT, p_actual_outcome BOOLEAN, p_resolved_by TEXT, p_resolution_notes TEXT DEFAULT NULL ) RETURNS REAL AS $$ DECLARE v_predicted REAL; v_brier REAL; v_status TEXT; BEGIN SELECT predicted_probability, status INTO v_predicted, v_status FROM public.fermi_forecasts WHERE id = p_forecast_id FOR UPDATE; IF NOT FOUND THEN RAISE EXCEPTION 'Forecast % not found', p_forecast_id; END IF; IF v_status != 'active' THEN RAISE EXCEPTION 'Forecast % is not active (status: %)', p_forecast_id, v_status; END IF; v_brier := public.compute_brier_score(v_predicted, p_actual_outcome); UPDATE public.fermi_forecasts SET actual_outcome = p_actual_outcome, brier_score = v_brier, scored_probability = v_predicted, resolution_source = COALESCE(resolution_source, 'operator'), status = 'resolved', resolved_at = NOW(), resolved_by = p_resolved_by, resolution_notes = p_resolution_notes, updated_at = NOW() WHERE id = p_forecast_id; RETURN v_brier; END; $$ LANGUAGE plpgsql;

-- fermi_market_observations.table
CREATE TABLE IF NOT EXISTS public.fermi_market_observations ( id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text, forecast_id TEXT REFERENCES public.fermi_forecasts(id) ON DELETE SET NULL, pm_event_id TEXT NOT NULL, pm_market_id TEXT NOT NULL, pm_condition_id TEXT, pm_slug TEXT, pm_question TEXT NOT NULL, pm_event_title TEXT, market_price REAL NOT NULL CHECK (market_price >= 0 AND market_price <= 1), bid_price REAL CHECK (bid_price IS NULL OR (bid_price >= 0 AND bid_price <= 1)), ask_price REAL CHECK (ask_price IS NULL OR (ask_price >= 0 AND ask_price <= 1)), midpoint_price REAL CHECK (midpoint_price IS NULL OR (midpoint_price >= 0 AND midpoint_price <= 1)), spread REAL, volume_total REAL, volume_24h REAL, liquidity REAL, price_change_1h REAL, price_change_1d REAL, price_change_1w REAL, price_change_1m REAL, pm_end_date TIMESTAMPTZ, pm_active BOOLEAN NOT NULL DEFAULT true, pm_closed BOOLEAN NOT NULL DEFAULT false, pm_resolved BOOLEAN NOT NULL DEFAULT false, pm_outcome TEXT, fermi_probability REAL CHECK (fermi_probability IS NULL OR (fermi_probability >= 0 AND fermi_probability <= 1)), divergence_pp REAL, confidence_signal TEXT CHECK (confidence_signal IS NULL OR confidence_signal IN ('very_high', 'high', 'medium', 'low')), observer_id TEXT NOT NULL REFERENCES public.users(user_id), observation_type TEXT NOT NULL DEFAULT 'search' CHECK (observation_type IN ('search', 'import', 'manual_link', 'refresh', 'scheduled', 'agent_research', 'resolution_check')), tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[], metadata JSONB NOT NULL DEFAULT '{}'::jsonb, created_at TIMESTAMPTZ NOT NULL DEFAULT NOW() );

-- fermi_market_observations.idx_forecast
CREATE INDEX IF NOT EXISTS idx_market_obs_forecast ON public.fermi_market_observations(forecast_id, created_at) WHERE forecast_id IS NOT NULL;

-- fermi_market_observations.idx_pm_market
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_market ON public.fermi_market_observations(pm_market_id, created_at);

-- fermi_market_observations.idx_pm_event
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_event ON public.fermi_market_observations(pm_event_id);

-- fermi_market_observations.idx_observer
CREATE INDEX IF NOT EXISTS idx_market_obs_observer ON public.fermi_market_observations(observer_id, created_at);

-- fermi_market_observations.idx_type
CREATE INDEX IF NOT EXISTS idx_market_obs_type ON public.fermi_market_observations(observation_type);

-- fermi_market_observations.idx_unresolved
CREATE INDEX IF NOT EXISTS idx_market_obs_unresolved ON public.fermi_market_observations(pm_market_id, created_at) WHERE forecast_id IS NOT NULL AND pm_closed = false AND pm_resolved = false;

-- fermi_market_observations.idx_resolved
CREATE INDEX IF NOT EXISTS idx_market_obs_resolved ON public.fermi_market_observations(forecast_id, created_at) WHERE pm_resolved = true;

-- forecast_invites.table
CREATE TABLE IF NOT EXISTS public.forecast_invites ( id UUID PRIMARY KEY DEFAULT gen_random_uuid(), target_type TEXT NOT NULL CHECK (target_type IN ('forecast', 'portfolio', 'team')), target_id TEXT NOT NULL, permission TEXT NOT NULL CHECK (permission IN ('view', 'edit', 'admin', 'owner', 'member', 'viewer')), invitee_user_id TEXT, invitee_email TEXT, token TEXT UNIQUE, inviter_id TEXT NOT NULL, message TEXT, status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'declined', 'revoked', 'expired')), expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '14 days', created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), accepted_at TIMESTAMPTZ, CONSTRAINT forecast_invites_recipient_exactly_one CHECK ((invitee_user_id IS NOT NULL AND invitee_email IS NULL) OR (invitee_user_id IS NULL AND invitee_email IS NOT NULL)) );

-- forecast_invites.idx_recipient_user
CREATE INDEX IF NOT EXISTS idx_invites_recipient_user ON public.forecast_invites(invitee_user_id) WHERE invitee_user_id IS NOT NULL AND status = 'pending';

-- forecast_invites.idx_recipient_email
CREATE INDEX IF NOT EXISTS idx_invites_recipient_email ON public.forecast_invites(LOWER(invitee_email)) WHERE invitee_email IS NOT NULL AND status = 'pending';

-- forecast_invites.idx_target
CREATE INDEX IF NOT EXISTS idx_invites_target ON public.forecast_invites(target_type, target_id) WHERE status = 'pending';

-- agents.updated_at
ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- agents.updated_at.backfill
UPDATE public.agents SET updated_at = created_at WHERE updated_at IS NULL;

-- fermi_forecasts.counterfactual_probability
ALTER TABLE public.fermi_forecasts ADD COLUMN IF NOT EXISTS counterfactual_probability REAL CHECK (counterfactual_probability IS NULL OR (counterfactual_probability >= 0 AND counterfactual_probability <= 1));

-- fermi_forecasts.scored_probability
ALTER TABLE public.fermi_forecasts ADD COLUMN IF NOT EXISTS scored_probability REAL;

-- fermi_forecasts.resolution_source
ALTER TABLE public.fermi_forecasts ADD COLUMN IF NOT EXISTS resolution_source TEXT CHECK (resolution_source IS NULL OR resolution_source IN ( 'operator', 'polymarket_oracle', 'polymarket_price_heuristic', 'workspace_upstream', 'backtest_seed', 'unknown'));

-- fn.fermi_forecasts_freeze_resolved
CREATE OR REPLACE FUNCTION public.fn_fermi_forecasts_freeze_resolved() RETURNS TRIGGER LANGUAGE plpgsql AS $$ BEGIN IF OLD.status = 'resolved' AND NEW.status = 'resolved' THEN IF NEW.scored_probability IS DISTINCT FROM OLD.scored_probability THEN RAISE WARNING 'fermi_forecasts %: scored_probability is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.scored_probability, NEW.scored_probability; NEW.scored_probability := OLD.scored_probability; END IF; IF NEW.brier_score IS DISTINCT FROM OLD.brier_score THEN RAISE WARNING 'fermi_forecasts %: brier_score is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.brier_score, NEW.brier_score; NEW.brier_score := OLD.brier_score; END IF; IF NEW.actual_outcome IS DISTINCT FROM OLD.actual_outcome THEN RAISE WARNING 'fermi_forecasts %: actual_outcome is immutable once resolved (attempted % -> %); keeping original', OLD.id, OLD.actual_outcome, NEW.actual_outcome; NEW.actual_outcome := OLD.actual_outcome; END IF; IF NEW.predicted_probability IS DISTINCT FROM OLD.predicted_probability THEN RAISE WARNING 'fermi_forecasts %: predicted_probability is frozen once resolved (attempted % -> %); keeping original. Filter on status = ''active'' in the calling UPDATE.', OLD.id, OLD.predicted_probability, NEW.predicted_probability; NEW.predicted_probability := OLD.predicted_probability; END IF; END IF; RETURN NEW; END; $$;

-- fermi_forecasts.freeze_trigger_drop
DROP TRIGGER IF EXISTS trg_fermi_forecasts_freeze_resolved ON public.fermi_forecasts;

-- fermi_forecasts.freeze_trigger_create
CREATE TRIGGER trg_fermi_forecasts_freeze_resolved BEFORE UPDATE ON public.fermi_forecasts FOR EACH ROW EXECUTE FUNCTION public.fn_fermi_forecasts_freeze_resolved();

