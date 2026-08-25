//! Answers "how well-grounded was the evidence?" for the memory layer.
//!
//! The only implementation of [`ProvenanceOracle`]. It lives here rather than
//! in `agent-bestiary-memory` because the field contracts live here: `fermi`
//! depends on the memory crate, so the memory crate cannot call back into
//! `grounding_trust` without a cycle. See `agent_bestiary_memory::provenance`
//! for why that boundary is a trait and not a copied function.
//!
//! # What it reads
//!
//! Per source episode, two things: the subject agent's slug (to find its field
//! contract) and the retained `response_text` (migration 199). Then
//! [`grounding_trust::response_floor`] per episode, and the minimum across
//! them, capped at the extraction ceiling.
//!
//! The floor is recomputed on every call rather than cached on `episodes`, and
//! that is deliberate. A cached verdict freezes the contract that produced it,
//! so when `football_analyst` finally gets a football data tool, every rule
//! extracted from its past episodes would still carry the verdict issued when
//! it had none. Recomputation means a new contract retroactively improves
//! every floor derived from the episodes it covers, which is the direction
//! this whole layer is trying to move in.
//!
//! # Unknown is not a value in the lattice
//!
//! The subtle part. Provenance verdicts are ordered (`tool_verified` >
//! `model_inference` > `unavailable_no_tool_source`), but *unknown* is not a
//! rung on that ladder — it is the absence of information about a rung, and it
//! has to be handled separately from the `min`.
//!
//! Consider nine `tool_verified` episodes and one whose response was never
//! retained. The floor is not `tool_verified`: the tenth episode might be
//! anything, and claiming the strongest value would be asserting exactly what
//! is not known. Nor is it `unavailable_no_tool_source`: the tenth episode is
//! not known to be ungrounded either. The honest answer is unknown.
//!
//! But now consider nine `tool_verified` episodes and one that is *known* to
//! be ungrounded, plus one unretained. Here the answer is
//! `unavailable_no_tool_source` and the unknown episode changes nothing —
//! there is no verdict it could turn out to hold that would lower a floor
//! already resting on the bottom.
//!
//! So: an unknown source poisons the result only when it could still move it.
//! That is [`FloorAccumulator`], and it is tested both ways, because getting
//! it wrong in the lenient direction would let one ungradeable episode in a
//! cluster of ten manufacture a clean floor for the other nine.

use agent_bestiary_memory::error::Result as MemoryResult;
use agent_bestiary_memory::provenance::{ExtractionFloor, ProvenanceOracle};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::grounding_trust::{
    extracted_floor, floor, response_floor, strength, PROVENANCE_VALUES, PROV_UNAVAILABLE,
};

/// Combines per-source verdicts into one floor, tracking unknowns separately.
///
/// Not a fold over `Option<&str>`, because that shape invites `filter_map`,
/// and silently dropping the unknowns is precisely the bug: a cluster of ten
/// with one gradeable prose episode would report `unavailable` (correct by
/// luck), while a cluster of ten with one gradeable tool episode would report
/// `tool_verified` (a fabrication about the other nine).
#[derive(Debug, Default)]
struct FloorAccumulator {
    /// Verdicts we could actually grade, canonicalised.
    ///
    /// **Verdicts, not strengths.** This held `Option<u8>` — the weakest
    /// *strength* — and `resolve` then reconstructed a verdict from it with
    /// `if s >= 2 { tool_verified } else { model_inference }`. That is the
    /// tier collapse `grounding_trust::floor` documents as a fixed bug:
    /// a value settled by a human came back claiming a tool had run, and
    /// `tool_no_match` ("the tool answered, and had nothing") came back as
    /// `unavailable_no_tool_source` ("no tool exists"). Both misattribute
    /// mechanism, and both were invisible because the strength was right.
    ///
    /// Keeping the verdicts means the arithmetic can be delegated to the layer
    /// that owns the vocabulary, which is what §3.4 requires and what the trait
    /// docs in `agent_bestiary_memory::provenance` already said this did.
    known: Vec<&'static str>,
    /// Sources whose provenance could not be established at all.
    unknown_count: usize,
    /// Total sources considered, so an empty cluster is distinguishable from
    /// a cluster nothing could be said about.
    total: usize,
}

impl FloorAccumulator {
    fn observe(&mut self, verdict: Option<&str>) {
        self.total += 1;
        match verdict {
            Some(v) => {
                // Canonicalise to the `&'static str` the runtime can emit, the
                // same way `floor` does, so an unrecognised token cannot be
                // echoed back as though it were vocabulary.
                let canonical = PROVENANCE_VALUES
                    .iter()
                    .copied()
                    .find(|c| *c == v)
                    .unwrap_or(PROV_UNAVAILABLE);
                self.known.push(canonical);
            }
            None => self.unknown_count += 1,
        }
    }

    /// The floor, or `None` for unknown.
    fn resolve(&self) -> Option<&'static str> {
        // An empty cluster is a finding, not a gap: we know there was no
        // evidence, and a rule with no sources is ungrounded. Handled by the
        // caller before any `observe`, but stated here so the accumulator is
        // correct in isolation.
        if self.total == 0 {
            return Some(PROV_UNAVAILABLE);
        }
        // Nothing gradeable at all.
        if self.known.is_empty() {
            return None;
        }

        // The raw floor decides whether an unknown could still move the answer,
        // and it has to be the raw one: the ceiling can clamp a strength-2 floor
        // down to 1, and asking the clamped value whether it is at the bottom
        // would say no when the unknowns are in fact irrelevant.
        let raw = floor(self.known.iter().copied());
        if self.unknown_count > 0 && strength(raw) > 0 {
            // Some sources graded above the bottom and something is ungradeable:
            // the true minimum could be lower and we must not guess.
            return None;
        }

        // Floor and extraction ceiling, both from `grounding_trust`. This module
        // computes neither: reading well-sourced episodes and generalising over
        // them is judgement, and judgement does not inherit retrieval — but that
        // rule belongs to the layer that owns the vocabulary.
        Some(extracted_floor(self.known.iter().copied()))
    }

    /// Did the extraction ceiling actually clamp the answer?
    ///
    /// Reported in the basis so a reader can tell "the sources were weak" from
    /// "the sources were strong and extraction is judgement". Both produce
    /// `model_inference`, and they are completely different facts.
    fn ceiling_bit(&self) -> bool {
        if self.known.is_empty() {
            return false;
        }
        let raw = floor(self.known.iter().copied());
        strength(raw) > strength(crate::grounding_trust::EXTRACTION_CEILING)
    }
}

/// Resolves provenance floors from `episodes` and the field contracts.
pub struct DbProvenanceOracle {
    pool: PgPool,
}

impl DbProvenanceOracle {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl ProvenanceOracle for DbProvenanceOracle {
    async fn extraction_floor(&self, episode_ids: &[Uuid]) -> MemoryResult<ExtractionFloor> {
        if episode_ids.is_empty() {
            // Known-empty, therefore ungrounded — not unknown. A rule with no
            // sources had no evidence, and that is a fact about it.
            return Ok(ExtractionFloor {
                floor: Some(PROV_UNAVAILABLE.to_string()),
                basis: json!({
                    "sources": [],
                    "reason": "empty_source_cluster",
                    "note": "no evidence at all, which is a finding rather than a gap"
                }),
            });
        }

        // Read-only. `agent_name` is the slug the field contracts are keyed
        // by; `response_text` exists only for episodes recorded after
        // migration 199, and NULL there means the evidence is gone rather
        // than unrecorded.
        let rows = sqlx::query(
            "SELECT e.episode_id, a.agent_name, e.response_text
               FROM episodes e
               JOIN agents a ON a.agent_id = e.agent_id
              WHERE e.episode_id = ANY($1)",
        )
        .bind(episode_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut acc = FloorAccumulator::default();
        let mut per_source = Vec::with_capacity(rows.len());

        for row in &rows {
            let episode_id: Uuid = row.try_get("episode_id")?;
            let agent_name: String = row.try_get("agent_name")?;
            let response_text: Option<String> = row.try_get("response_text")?;

            let (verdict, why) = match response_text.as_deref() {
                None => (None, "no_retained_response_pre_migration_199"),
                Some(text) => match response_floor(&agent_name, text) {
                    Some(v) => (Some(v), "graded_against_field_contract"),
                    None => (None, "agent_has_no_field_contract"),
                },
            };
            acc.observe(verdict);
            per_source.push(json!({
                "episode_id": episode_id,
                "agent": agent_name,
                "floor": verdict,
                "why": why,
            }));
        }

        // Episodes named in the cluster that no longer exist. Counted as
        // unknown rather than skipped: a deleted episode is evidence we
        // cannot inspect, and dropping it from the denominator would let
        // cleanup work raise a rule's apparent grounding.
        let missing = episode_ids.len().saturating_sub(rows.len());
        for _ in 0..missing {
            acc.observe(None);
        }

        let resolved = acc.resolve();
        Ok(ExtractionFloor {
            floor: resolved.map(|s| s.to_string()),
            basis: json!({
                "sources": per_source,
                "declared_sources": episode_ids.len(),
                "resolved_sources": rows.len(),
                "missing_sources": missing,
                "ungradeable_sources": acc.unknown_count,
                "ceiling": crate::grounding_trust::EXTRACTION_CEILING,
                // Was the ceiling the thing that decided the answer? True only
                // when the unclamped floor was stronger than the ceiling — asked
                // of the raw floor rather than of a reconstructed strength, so
                // the basis says what actually happened.
                "ceiling_applied": acc.ceiling_bit(),
                "reason": if resolved.is_some() {
                    "min_over_sources_capped_at_extraction_ceiling"
                } else {
                    "at_least_one_source_ungradeable_and_it_could_still_lower_the_floor"
                },
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding_trust::{PROV_INFERRED, PROV_TOOL};

    fn acc(verdicts: &[Option<&str>]) -> FloorAccumulator {
        let mut a = FloorAccumulator::default();
        for v in verdicts {
            a.observe(*v);
        }
        a
    }

    #[test]
    fn an_ungradeable_source_poisons_a_floor_it_could_still_lower() {
        // Nine measured episodes and one nobody can grade is not nine
        // measured episodes. The tenth could be anything, and answering
        // `tool_verified` would assert exactly what is not known.
        let mut v = vec![Some(PROV_TOOL); 9];
        v.push(None);
        assert_eq!(acc(&v).resolve(), None);
    }

    #[test]
    fn an_ungradeable_source_is_irrelevant_once_the_floor_is_on_the_bottom() {
        // The other half of the rule, and the half that keeps it from being
        // useless. If a known source is already ungrounded, no verdict the
        // unknown one turns out to hold could lower the minimum, so the
        // answer is knowable. Without this, a single unretained episode
        // anywhere in a cluster would render every floor unknown and the
        // column would carry no signal at all.
        assert_eq!(
            acc(&[Some(PROV_TOOL), Some(PROV_UNAVAILABLE), None]).resolve(),
            Some(PROV_UNAVAILABLE)
        );
    }

    #[test]
    fn a_cluster_nothing_can_be_graded_in_is_unknown_not_ungrounded() {
        // The common case today: 58 of 3352 episodes retain a response. A
        // corpus-wide report must show this as missing coverage, not as
        // ungrounded rules, because the remedy is different — retention and
        // contracts, not retracting the rules.
        assert_eq!(acc(&[None, None, None]).resolve(), None);
    }

    #[test]
    fn no_number_of_sourced_episodes_lifts_an_extraction_past_the_ceiling() {
        let v = vec![Some(PROV_TOOL); 20];
        assert_eq!(acc(&v).resolve(), Some(PROV_INFERRED));
    }

    #[test]
    fn an_empty_accumulator_is_ungrounded_not_verified() {
        // `min` over nothing has no answer, and the identity element for
        // `min` is the MAXIMUM value. The natural implementation therefore
        // claims a rule with no sources was measured. This is the single most
        // likely way the calculation breaks and it breaks in the direction
        // that manufactures trust.
        assert_eq!(acc(&[]).resolve(), Some(PROV_UNAVAILABLE));
    }

    #[test]
    fn the_floor_is_the_weakest_source_not_the_most_common() {
        assert_eq!(
            acc(&[
                Some(PROV_TOOL),
                Some(PROV_TOOL),
                Some(PROV_TOOL),
                Some(PROV_INFERRED)
            ])
            .resolve(),
            Some(PROV_INFERRED)
        );
    }

    /// The mechanism must survive, not just the strength.
    ///
    /// The regression this module was carrying. `FloorAccumulator` kept the
    /// weakest *strength* and `resolve` rebuilt a verdict from it with
    /// `if s >= 2 { tool_verified } else { model_inference }`. Both cases below
    /// came back naming a mechanism that never ran, and both looked right
    /// because the strength was right — the exact error
    /// `grounding_trust::floor` documents as fixed, reintroduced one layer out.
    #[test]
    fn the_floor_names_a_mechanism_that_actually_occurred() {
        use crate::grounding_trust::{PROV_HUMAN_ENDORSED, PROV_NO_MATCH, PROV_UNAVAILABLE};

        // Strength 1. The old code produced `model_inference` — a model was
        // never involved; a person was.
        assert_eq!(
            acc(&[Some(PROV_TOOL), Some(PROV_HUMAN_ENDORSED)]).resolve(),
            Some(PROV_HUMAN_ENDORSED),
            "a floor set by a person's endorsement must not report as a model's \
             inference"
        );

        // Strength 0. The old code produced `unavailable_no_tool_source` — "no
        // tool exists" — for a source whose tool ran and found nothing.
        assert_eq!(
            acc(&[Some(PROV_TOOL), Some(PROV_NO_MATCH)]).resolve(),
            Some(PROV_NO_MATCH),
            "`the tool answered and had nothing` must not report as `no tool \
             exists`"
        );

        // And the genuinely absent case still reports absence.
        assert_eq!(acc(&[]).resolve(), Some(PROV_UNAVAILABLE));
    }

    /// This module owns no arithmetic.
    ///
    /// §3.4: a trust calculation must have exactly one implementation, and the
    /// layer that owns the vocabulary must own it. With no ungradeable sources
    /// there is nothing for this module to decide, so its answer must be
    /// `extracted_floor`'s answer — for every combination, not for the handful
    /// someone thought to write a case for.
    #[test]
    fn with_nothing_ungradeable_the_answer_is_exactly_extracted_floor() {
        use crate::grounding_trust::{extracted_floor, PROVENANCE_VALUES};
        for a in PROVENANCE_VALUES {
            for b in PROVENANCE_VALUES {
                let mine = acc(&[Some(a), Some(b)]).resolve();
                let theirs = extracted_floor([*a, *b]);
                assert_eq!(
                    mine,
                    Some(theirs),
                    "({a}, {b}): this module disagreed with the layer that owns \
                     the calculation"
                );
            }
        }
    }

    /// The basis must distinguish "weak sources" from "strong sources, and
    /// extraction is judgement". Both yield `model_inference`.
    #[test]
    fn the_basis_says_whether_the_ceiling_was_what_decided_it() {
        assert!(
            acc(&[Some(PROV_TOOL), Some(PROV_TOOL)]).ceiling_bit(),
            "two tool-verified sources clamp to the ceiling, and the basis \
             should say the ceiling is why"
        );
        assert!(
            !acc(&[Some(PROV_TOOL), Some(PROV_INFERRED)]).ceiling_bit(),
            "the floor was already at the ceiling's strength; the ceiling \
             changed nothing"
        );
    }
}
