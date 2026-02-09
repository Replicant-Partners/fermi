-- Migration 016: Coherence evaluations table
-- Stores TEC coherence evaluation results for workspace conversations

CREATE TABLE IF NOT EXISTS coherence_evaluations (
    eval_id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id      UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    global_score      DOUBLE PRECISION NOT NULL,
    quality_label     TEXT NOT NULL,
    principle_scores  JSONB NOT NULL DEFAULT '{}',
    health_indicators JSONB NOT NULL DEFAULT '{}',
    utterance_count   INTEGER NOT NULL DEFAULT 0,
    message_window    JSONB,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_coherence_workspace
    ON coherence_evaluations(workspace_id, created_at DESC);
