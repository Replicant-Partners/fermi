-- Migration 206: a post-contract profile is not automatically a correct one.
--
-- ## The gap migration 202 left
--
-- Migration 202 split the cached genome profiles in two using the presence of
-- `taxonomy_provenance` as the discriminator, on the reasoning that
-- post-contract documents always carry it because `enforce` stamps it
-- unconditionally. That reasoning is sound and the discriminator was the right
-- one — it replaced a shape test ("has a genome key") that had been
-- mislabelling correct rows on every reboot.
--
-- What it also did, without saying so, was treat **post-contract as a proxy for
-- correct.** Part 2 supersedes only rows lacking `taxonomy_provenance`, so a
-- profile written under the contract was left untouched whatever it contained.
--
-- `Antaxius beieri` is the counter-example, and it is the same row that
-- motivated the cross-check in the first place. A bush-cricket — Orthoptera /
-- Tettigoniidae on its own creature record — profiled as a cerambycid beetle,
-- Coleoptera / Cerambycidae. The profile carries `taxonomy_provenance`, so 202
-- classified it as post-contract and left it, and the live cross-check went red
-- again the next time it ran.
--
-- Both facts are true at once: the document *was* written under the contract,
-- and its taxonomy *is* wrong. `enforce` alone could never have caught it —
-- `Sourced` asserts that a tool COULD supply the field, never that this value
-- CAME from it — which is precisely why `reconcile` was added, and why the
-- empirical tier exists. The profile predates the deploy that wired
-- `reconcile` into the `genome_profiler` boundary, so it is the last cohort of
-- rows that can hold this defect.
--
-- ## Why reconcile rather than clear
--
-- Migration 202 cleared its rows to force regeneration, because their values
-- were fabricated throughout — genome sizes, karyotypes, IUCN statuses, all of
-- it. That is not the case here. This document is wrong in exactly one block,
-- and the correct value is already on the creature row, GBIF-verified, one JOIN
-- away. Regenerating would spend a credit and an LLM call to re-derive
-- something already known, and would risk re-deriving it wrongly.
--
-- So: canonical wins, in place. Identical to what
-- `grounding_trust::reconcile` now does at write time, expressed in SQL for
-- the rows that predate it.
--
-- ## Retain what you superseded
--
-- The replaced taxonomy is kept under `_reconciled`, not discarded. Same rule
-- as `Violation.removed` and 202's `superseded_profile`: a value the platform
-- overwrote is calibration data about the model that produced it, and deleting
-- it destroys the only record of how confidently wrong it was. It also makes
-- this migration auditable — a reader can see exactly what changed rather than
-- taking the commit message's word for it.
--
-- Idempotent via the `_reconciled` guard: a row already corrected is skipped,
-- so the replay on every boot is a no-op rather than a second overwrite that
-- would archive the corrected value over the original.

-- One DO block. PgBouncer runs in transaction-pooling mode, where top-level
-- statements get separate implicit transactions.
DO $$
BEGIN
    UPDATE public.creature_conditions cc
       SET genome_profile =
               -- Replace the taxonomy block wholesale with the creature's
               -- canonical copy, and record what was there before.
               jsonb_set(
                   jsonb_set(
                       cc.genome_profile,
                       '{taxonomy}',
                       c.taxonomy
                   ),
                   '{_reconciled}',
                   jsonb_build_object(
                       'superseded_taxonomy', cc.genome_profile -> 'taxonomy',
                       'reconciled_at', to_jsonb(now()),
                       'reconciled_by', 'migrations/206',
                       'reason', 'Profile taxonomy contradicted the creature''s '
                                 'GBIF-verified record. Written under the grounding '
                                 'contract but before reconcile() was wired at the '
                                 'genome_profiler boundary, so enforce() passed it: '
                                 'the field was present, typed, and declared Sourced. '
                                 'Canonical wins; the superseded value is retained as '
                                 'calibration data.'
                   )
               )
      FROM public.creatures c
     WHERE c.creature_id = cc.creature_id
       AND cc.genome_profile IS NOT NULL
       AND c.taxonomy IS NOT NULL
       -- Post-contract only. Pre-contract rows are 202's business and are
       -- already superseded; touching them here would archive a cleared
       -- document over the fabricated one it was meant to preserve.
       AND cc.genome_profile ? 'taxonomy_provenance'
       -- Idempotence.
       AND NOT cc.genome_profile ? '_reconciled'
       -- Only rows that actually disagree. A profile matching its creature
       -- needs no rewrite, and rewriting it would add a `_reconciled` marker
       -- claiming a correction that never happened — the same class of error as
       -- migration 200 tagging correct rows as legacy.
       AND lower(cc.genome_profile -> 'taxonomy' ->> 'order')
             IS DISTINCT FROM lower(c.taxonomy ->> 'order');

    COMMENT ON COLUMN public.creature_conditions.genome_profile IS
        'Cached phylogenetic profile. A `_grounding_review` key means the document '
        'predates the grounding contract (src/grounding_trust.rs) and every '
        'non-narrative field is treated as unsourced. Presence of '
        '`taxonomy_provenance` means it was written under the contract - which is '
        'NOT the same as correct: see migration 206, where a post-contract profile '
        'contradicted its creature''s GBIF taxonomy because it predated '
        'reconcile(). A `_reconciled` key means the taxonomy block was overwritten '
        'from the canonical creature record, with the superseded value retained. '
        'Never tag on shape alone - see migration 202.';
END $$;
