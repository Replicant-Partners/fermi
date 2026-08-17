-- Migration 202: fix migration 200's legacy tag, and force the genuinely
-- legacy genome profiles to regenerate.
--
-- ## Part 1 — migration 200's guard could not tell the two populations apart
--
-- Migration 200 tagged pre-contract profiles with `_grounding_review` using:
--
--     WHERE genome_profile ? 'genome'
--       AND NOT genome_profile ? '_grounding_review'
--
-- `run_migrations` replays every migration on every boot, so that predicate
-- runs again each time — and it catches **any** profile with a `genome` key,
-- including ones written *after* the contract existed. `popovi`, the first
-- correctly-grounded profile, was tagged as legacy on a subsequent boot.
--
-- On its own that was a mislabelled row. It became harmful when
-- `grounding_trust::PRE_CONTRACT_MARKER` started *trusting* the tag: a
-- document carrying it has every non-narrative field treated as unsourced
-- regardless of contract. So once `ncbi_genome_search` returns a real
-- `tool_verified` genome size, a new correct profile would have had it
-- stripped — because a migration had mislabelled it as legacy.
--
-- Two guards, each individually reasonable, combining into a data-destroying
-- one. The lesson is narrow and worth writing down: **a marker that a later
-- check will trust must be applied by a discriminator that cannot drift.**
-- "Has a genome key" describes shape. "Has provenance keys" describes
-- history, which is what was actually meant.
--
-- Post-contract documents always carry `taxonomy_provenance`, because
-- `enforce` stamps it unconditionally. That is the correct discriminator, and
-- Part 1 uses it to un-tag every row migration 200 caught by mistake.
--
-- ## Part 2 — force regeneration of the real legacy rows
--
-- The remaining tagged rows genuinely predate any tool. Their values are
-- fabricated: `"800–1200"` for a genome size, `"2n = 16–24 (typical for
-- Acrididae)"` for a karyotype. The read path strips them, so nothing wrong
-- reaches a player — but the profile renders empty when a real answer is now
-- obtainable for roughly a third of species via `ncbi_genome_search`, and
-- correct taxonomy is obtainable for all of them via `reconcile`.
--
-- A genome profile is a one-time purchase that becomes a static read, so
-- these will not regenerate on their own: `cache_is_valid` returns true for
-- any profile with a non-empty `taxonomy`, which these have.
--
-- So this migration clears the cache slot down to the review marker, with the
-- superseded document nested inside it. `taxonomy` is then absent, the cache
-- check misses, and the next read regenerates under the full contract.
-- Nothing is deleted: the fabricated document is retained verbatim, both
-- because "tag, do not delete" was the requirement and because comparing a
-- model's guess against a later measurement is a free calibration signal.
--
-- Cost: 2 credits per regeneration, charged to whoever next opens the card.
-- 13 rows.
--
-- Idempotent via the `superseded_profile` guard.
--
-- One DO block rather than three top-level statements. Not because the two
-- UPDATEs need to be atomic — both are guarded and `run_migrations` replays on
-- every boot, so a half-apply self-heals — but because PgBouncer is in
-- transaction-pooling mode and `scripts/lint-migrations.sh` warns on the shape.
-- A warning nobody acts on is a warning everybody learns to scroll past, and
-- the next migration with the same shape will be the one where atomicity did
-- matter.

DO $$
BEGIN
    -- ─── Part 1: un-tag the post-contract profiles ────────────────────────

    UPDATE public.creature_conditions
       SET genome_profile = genome_profile - '_grounding_review'
     WHERE genome_profile IS NOT NULL
       AND genome_profile ? '_grounding_review'
       AND genome_profile ? 'taxonomy_provenance';

    COMMENT ON COLUMN public.creature_conditions.genome_profile IS
      'Cached phylogenetic profile. A `_grounding_review` key means the document predates the grounding contract (src/grounding_trust.rs) and every non-narrative field is treated as unsourced. Presence of `taxonomy_provenance` means the reverse: it was written under the contract. Never tag on shape alone — see migration 202.';

    -- ─── Part 2: archive and clear the genuine legacy rows ────────────────

    UPDATE public.creature_conditions
       SET genome_profile = jsonb_build_object(
               '_grounding_review',
               coalesce(genome_profile -> '_grounding_review', '{}'::jsonb)
               || jsonb_build_object(
                      'superseded_profile', genome_profile - '_grounding_review',
                      'invalidated_at', to_jsonb(now()),
                      'invalidated_by', 'migrations/202',
                      'reason', 'Cleared to force regeneration under the grounding '
                                'contract: ncbi_genome_search can now source genome '
                                'size and chromosome count, and reconcile() corrects '
                                'taxonomy against the creature record. The superseded '
                                'document is retained verbatim for comparison.'
                  )
           )
     WHERE genome_profile IS NOT NULL
       AND genome_profile ? '_grounding_review'
       AND NOT genome_profile ? 'taxonomy_provenance'
       AND NOT (genome_profile -> '_grounding_review') ? 'superseded_profile';
END $$;
