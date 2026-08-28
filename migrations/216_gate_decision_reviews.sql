-- Migration 216: human review of a gate decision.
--
-- `docs/AUDIT_loops_and_gates.md` §2.2 gave the platform a record of what it
-- refused (migration 214). This gives a person somewhere to say whether the
-- refusal was right.
--
-- ## Why this is not a dashboard feature
--
-- `gate_api::GATE_DOORS` was `&[]`, and the comment on it said the emptiness was
-- itself the finding: there was no endpoint anywhere that let a person act on a
-- gate. No way to review what a gate refused, no way to record that a refusal
-- was wrong. The whole surface was read-only.
--
-- That is defensible for an *override* — a gate a person can wave through is not
-- much of a gate, and nothing here overrides anything. It is not defensible for
-- a *judgement*, and the reason is arithmetic:
--
--     gate_trust's readings are computed from approve/refuse COUNTS.
--
-- `refuses_everything` catches the Γ bug's signature — asked, and approved
-- nothing. It cannot catch a gate that approves 90% and refuses the other 10%
-- *wrongly*: that reads `discriminating`, which the surface renders as the
-- healthy state. There is no counter that distinguishes a correct refusal from
-- an incorrect one, because correctness is not a property of the count. It is a
-- judgement about the subject, and only a reviewer holds it.
--
-- So this table is the only thing in the system that can answer "is this gate
-- refusing the right things", and until it existed the answer was structurally
-- unavailable rather than merely unmeasured.
--
-- ## Append-only, for migration 205's reason
--
-- NEVER updated and never deleted. Current state is the latest row per
-- decision_id, derived rather than stored, so a decision reviewed as upheld and
-- later overturned reads as exactly that rather than as "overturned" with the
-- first reviewer's name erased. A mutable verdict column would destroy the
-- disagreement, and a disagreement between two reviewers about the same refusal
-- is the most informative row this table can hold.
--
-- ## The load-bearing constraint
--
-- `overturned` requires a rationale. That verdict says the platform was wrong;
-- it is the row that should cause an engineering change, and an uncited overturn
-- is a complaint rather than a finding. Same argument as 205's citation CHECK on
-- `human_sourced`, and the same shape: the expensive verdict earns its weight by
-- being followable.
--
-- `upheld` deliberately requires nothing, and that asymmetry is deliberate for a
-- reason beyond 205's ("requiring a citation for every judgement would push
-- reviewers to paste a plausible URL"). Here it is worse: making the cheap
-- confirmation as expensive as the finding means nobody reviews the routine
-- decisions at all, and then the *denominator* is unknown. "3 overturned" and "3
-- overturned of 400 reviewed" are different findings and only the second is
-- actionable.
--
-- `unclear` is a first-class verdict, not a missing one. `gate_decisions.reason`
-- is free text truncated at the writer, and it is frequently not enough to judge
-- a decision from. Forcing a reviewer to pick upheld or overturned in that case
-- manufactures agreement, and the honest reading — *the ledger does not record
-- enough to review its own decisions* — is a finding about the ledger that would
-- otherwise be invisible. `gate_review::Standing::Inconclusive` reports it.
--
-- ## The closed vocabularies
--
-- `verdict` is owned by `seam_vocabulary::GateReviewVerdict` and `actor_kind` by
-- `seam_vocabulary::ActorKind`; both are registered in `VOCABULARIES` and both
-- CHECKs are compared against those types by
-- `tests/seam_vocabulary_contract.rs`. An unregistered CHECK on a closed set is
-- the `severity = 'L1'` setup exactly: Postgres holding one opinion, a Rust
-- string literal holding another, and nothing comparing them.

CREATE TABLE IF NOT EXISTS public.gate_decision_reviews (
    review_id    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The decision under review. A real foreign key, unlike migration 205's
    -- assertion_id — the target here is a row rather than an element inside a
    -- JSONB array, so Postgres can hold the reference and there is no reason to
    -- make a test do it.
    decision_id  BIGINT      NOT NULL
                 REFERENCES public.gate_decisions(id) ON DELETE CASCADE,

    -- Denormalised from gate_decisions so the per-gate standing is one index
    -- scan rather than a join, and so an orphaned review is detectable rather
    -- than merely unfindable. Kept honest by a trigger-free discipline: the
    -- writer reads the gate off the decision row it just fetched, and
    -- `gate_review::REVIEW_INSERT_SQL` is the only INSERT.
    gate         TEXT        NOT NULL
                 CHECK (gate IN ('coherence', 'grounding', 'input_binding',
                                 'admission', 'credit', 'rate_limit',
                                 'attachment')),

    -- Constrained to `seam_vocabulary::GATE_REVIEW_VERDICT`.
    verdict      TEXT        NOT NULL
                 CHECK (verdict IN ('upheld', 'overturned', 'unclear')),

    -- Why. REQUIRED for `overturned`, see the CHECK below.
    rationale    TEXT,

    -- Who decided, and what kind of thing they are. `actor_kind` distinguishes
    -- a replay harness from a person, because "reviewed" with no actor is how a
    -- queue becomes a rubber stamp — migration 205's words, and the same risk.
    --
    -- `tool` is genuinely reachable and not left in for symmetry: a harness that
    -- re-runs a gate against the recorded subject and reports whether it agrees
    -- is a legitimate reviewer, and a cheaper one than a person.
    actor        TEXT        NOT NULL,
    actor_kind   TEXT        NOT NULL,

    -- Anything the reviewer wants re-examinable later: the subject as it stood,
    -- a harness's output, a link. Retained so a verdict can be re-examined
    -- rather than merely trusted.
    evidence     JSONB,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Wrapped, and not as a style choice. Through PgBouncer a file run
-- statement-at-a-time commits the DROP and fails the ADD, so the net effect of
-- an unwrapped pair is to DELETE the constraint the migration was written to
-- add — and `run_migrations` logs the failure and continues to the next file.
-- `scripts/lint-migrations.sh` refused this file until it was wrapped, which is
-- the check working; `tests/constraint_trust.rs` ratchets the count of
-- grandfathered offenders and would have gone red on the next one.
DO $$ BEGIN
    ALTER TABLE public.gate_decision_reviews
        DROP CONSTRAINT IF EXISTS gate_decision_reviews_rationale_check;
    ALTER TABLE public.gate_decision_reviews
        ADD CONSTRAINT gate_decision_reviews_rationale_check
        CHECK (verdict <> 'overturned'
               OR (rationale IS NOT NULL AND length(trim(rationale)) > 0));

    ALTER TABLE public.gate_decision_reviews
        DROP CONSTRAINT IF EXISTS gate_decision_reviews_actor_kind_check;
    ALTER TABLE public.gate_decision_reviews
        ADD CONSTRAINT gate_decision_reviews_actor_kind_check
        CHECK (actor_kind IN ('tool', 'human', 'platform'));
END $$;

-- The three reads this table exists to serve.

-- "What is the latest verdict on this decision" — the derived-current-state
-- query, which needs the newest row per decision_id.
CREATE INDEX IF NOT EXISTS gate_decision_reviews_decision_idx
    ON public.gate_decision_reviews (decision_id, created_at DESC);

-- "Where does this gate stand" — the standing aggregate.
CREATE INDEX IF NOT EXISTS gate_decision_reviews_gate_idx
    ON public.gate_decision_reviews (gate, created_at DESC);

-- "What has been overturned" — the queue an engineer works, across all gates.
-- Partial, because that is the selective end and the one anybody pages through.
CREATE INDEX IF NOT EXISTS gate_decision_reviews_overturned_idx
    ON public.gate_decision_reviews (created_at DESC)
    WHERE verdict = 'overturned';

COMMENT ON TABLE public.gate_decision_reviews IS
    'Append-only log of human (or harness) judgements about rows in '
    'gate_decisions. NEVER updated and never deleted: current state is the '
    'latest row per decision_id, derived rather than stored, so two reviewers '
    'disagreeing about one refusal reads as a disagreement instead of as '
    'whichever wrote last. The only thing in the platform that can distinguish '
    'a correct refusal from an incorrect one - gate_trust''s readings are '
    'computed from counts, and correctness is not a property of a count.';

COMMENT ON COLUMN public.gate_decision_reviews.verdict IS
    'upheld | overturned | unclear. Owned by '
    'seam_vocabulary::GateReviewVerdict and registered there. `unclear` means '
    'the ledger did not record enough to judge the decision from, which is a '
    'finding about the ledger and not a missing answer.';

COMMENT ON COLUMN public.gate_decision_reviews.rationale IS
    'Why. Required for `overturned` by gate_decision_reviews_rationale_check: '
    'that verdict says the platform was wrong and should cause a change, and an '
    'uncited overturn is a complaint. Optional for `upheld` on purpose - making '
    'the cheap confirmation as costly as the finding leaves the denominator '
    'unknown, and "3 overturned" without "of 400 reviewed" is not actionable.';
