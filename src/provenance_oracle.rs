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

use crate::grounding_trust::{response_floor, strength, EXTRACTION_CEILING, PROV_UNAVAILABLE};

/// Combines per-source verdicts into one floor, tracking unknowns separately.
///
/// Not a fold over `Option<&str>`, because that shape invites `filter_map`,
/// and silently dropping the unknowns is precisely the bug: a cluster of ten
/// with one gradeable prose episode would report `unavailable` (correct by
/// luck), while a cluster of ten with one gradeable tool episode would report
/// `tool_verified` (a fabrication about the other nine).
#[derive(Debug, Default)]
struct FloorAccumulator {
    /// Weakest strength seen among sources we could actually grade.
    known_worst: Option<u8>,
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
                let s = strength(v);
                self.known_worst = Some(self.known_worst.map_or(s, |w| w.min(s)));
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
        match self.known_worst {
            // Already at the bottom. No unknown can lower it further, so the
            // unknowns are irrelevant and the answer is knowable.
            Some(0) => Some(PROV_UNAVAILABLE),
            // Some sources graded above the bottom. If anything is unknown,
            // the true minimum could be lower and we must not guess.
            Some(_) if self.unknown_count > 0 => None,
            Some(s) => {
                let base = if s >= 2 {
                    crate::grounding_trust::PROV_TOOL
                } else {
                    crate::grounding_trust::PROV_INFERRED
                };
                // The extraction ceiling: reading well-sourced episodes and
                // writing a generalisation about them is judgement, and
                // judgement does not inherit retrieval.
                Some(if strength(base) > strength(EXTRACTION_CEILING) {
                    EXTRACTION_CEILING
                } else {
                    base
                })
            }
            // Nothing gradeable at all.
            None => None,
        }
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
                "ceiling": EXTRACTION_CEILING,
                "ceiling_applied": resolved == Some(EXTRACTION_CEILING)
                    && acc.known_worst.unwrap_or(0) >= 2,
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
}
