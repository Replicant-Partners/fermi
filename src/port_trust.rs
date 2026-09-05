//! # Port trust contract — does the invocation match the declared interface?
//!
//! **Rung 5 of the verification ladder** (`crate::ladder`) — the top of it.
//! Written fourth:
//!
//! | Contract | Question |
//! | --- | --- |
//! | [`crate::schema_trust`] | Is the column present? |
//! | [`crate::rollup_trust`] | Is the column telling the truth? |
//! | [`crate::grounding_trust`] | Could this value have come from anywhere? |
//! | `port_trust` | **Is the caller sending what the agent said it takes?** |
//!
//! ## Why this moved server-side
//!
//! `negotiate::bind_input` in `crates/fermi-console` already answered this
//! correctly and shipped in v0.16.0. Its only callers were two sites in
//! `cockpit.rs` — the desktop console. **The API server never called it**, so
//! every HTTP execute path was unchecked, including the creature modules that
//! charge credits.
//!
//! Worse, the server did not merely fail to check: `stamp_invocation`
//! (`api_server.rs`) reads `input_binding` out of a **caller-supplied** JSON
//! object and writes it onto the episode as a queryable tag. So the record of
//! whether the interface matched was the caller's *claim* about the match. A
//! client with no knowledge of the card could assert `declared:query` and the
//! platform would file it as fact.
//!
//! That is the same shape as everything else this workstream has found: a
//! value that looks verified because it is present and well-formed, where
//! nothing checked it. Here the fix is cheap, because the verifier already
//! existed — it was pointed at the wrong process.
//!
//! ## Why the rule is wider than the console's
//!
//! `scripts/port_census.py` ran the console's rule across all 100 curated
//! cards and found it would flag **56**. Eight of those are false positives:
//!
//! ```text
//! sensor_advisor      free_text_stage_description
//! simops_advisor      free_text_process_description
//! comparator          compare_experiment_task
//! marketing_composer  compose_marketing_task
//! product_scout       scout_products_task
//! regulatory_scanner  scan_regulations_task
//! sidestream_miner    find_sidestreams_task
//! valuechain_mapper   map_valuechain_task
//! ```
//!
//! All eight plainly declare a free-text port; the console's rule misses them
//! because it matches `query|question|prompt` and four exact words. So this
//! implementation additionally recognises `free_text*` and the `*_task`
//! convention, taking the flagged set from 56 to 47.
//!
//! Deliberately **not** added: bare `description`. `ar_beacon` accepts
//! `description`/`location`, and posting a research prompt into a field meant
//! for a short caption is a real mismatch, not a false positive. The line is
//! judgement, and it is drawn here rather than in a regex nobody can read.
//!
//! ## Why this rule is temporary
//!
//! It should not exist. It is a heuristic guessing intent from 510 free-text
//! labels, and there is no setting at which it is correct: widen it and it
//! swallows non-text ports; narrow it and it misses real declarations. Every
//! adjustment to [`is_text_input`] is evidence for the registry, not progress
//! toward a good rule.
//!
//! When `accepts` entries are type references rather than strings — one
//! registered `fermi/free_text_query` that agents point at — this module
//! keeps [`bind_input`] and deletes [`is_text_input`]. Deleting it is the
//! success condition.

use serde::{Deserialize, Serialize};

/// Does this label name a port that takes free-running text?
///
/// See the module docs for why this is a heuristic and why its deletion is
/// the goal. Matched on a lowercased copy.
pub fn is_text_input(label: &str) -> bool {
    let l = label.to_ascii_lowercase().replace('-', "_");
    l.contains("query")
        || l.contains("question")
        || l.contains("prompt")
        // Two conventions the corpus uses for "describe it in your own words",
        // both missed by the console's narrower rule.
        || l.contains("free_text")
        || l.ends_with("_task")
        || matches!(l.as_str(), "content" | "topic" | "narrative" | "text")
}

/// How a free-text prompt maps onto what the agent says it accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputBinding {
    /// The agent declares a text-shaped input. Carries the agent's own label
    /// so a report speaks the designer's vocabulary rather than ours.
    Declared { label: String },
    /// The agent declares inputs and none of them takes free text. Sending a
    /// prose query binds it to an interface it never advertised.
    NoTextInput { declared: Vec<String> },
    /// The agent declares no inputs at all. **Not a mismatch** — an absence.
    /// Silence must not read as contradiction, or the check reports a defect
    /// for every card that simply says nothing.
    Undeclared,
}

impl InputBinding {
    /// Stable label for tags and telemetry. Matches the vocabulary
    /// `stamp_invocation` already writes, so existing episode tags stay
    /// comparable across the change.
    pub fn as_tag(&self) -> String {
        match self {
            InputBinding::Declared { label } => format!("declared:{label}"),
            InputBinding::NoTextInput { .. } => "no_text_input".to_string(),
            InputBinding::Undeclared => "undeclared".to_string(),
        }
    }

    /// True only for a genuine mismatch.
    pub fn is_mismatch(&self) -> bool {
        matches!(self, InputBinding::NoTextInput { .. })
    }
}

/// Resolve which declared input a free-text prompt is being sent as.
///
/// Prefers the canonical `query` when present, so the common case reports a
/// stable label rather than whichever synonym sorted first.
pub fn bind_input(accepts: &[String]) -> InputBinding {
    if accepts.is_empty() {
        return InputBinding::Undeclared;
    }
    if let Some(exact) = accepts.iter().find(|a| a.eq_ignore_ascii_case("query")) {
        return InputBinding::Declared {
            label: exact.clone(),
        };
    }
    if let Some(shaped) = accepts.iter().find(|a| is_text_input(a)) {
        return InputBinding::Declared {
            label: shaped.clone(),
        };
    }
    InputBinding::NoTextInput {
        declared: accepts.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_canonical_label_wins_over_a_synonym() {
        let b = bind_input(&v(&["forecast-question", "query"]));
        assert_eq!(
            b,
            InputBinding::Declared {
                label: "query".into()
            }
        );
    }

    #[test]
    fn each_designers_own_word_for_a_question_is_recognised() {
        for label in [
            "query",
            "forecast-question",
            "factor-x1-query",
            "confederation-query",
            "research_prompt",
            "content",
            "topic",
            "narrative",
            "text",
        ] {
            assert!(
                !bind_input(&v(&[label])).is_mismatch(),
                "`{label}` is question-shaped and must not be reported as a \
                 mismatch — a check that flags correct cards gets ignored"
            );
        }
    }

    #[test]
    fn the_eight_false_positives_the_census_found_are_fixed() {
        // Every one of these plainly declares a free-text port and was
        // flagged by the console's narrower rule. Named individually so a
        // future narrowing has to delete a real agent's name to pass.
        for label in [
            "free_text_stage_description",
            "free_text_process_description",
            "compare_experiment_task",
            "compose_marketing_task",
            "scout_products_task",
            "scan_regulations_task",
            "find_sidestreams_task",
            "map_valuechain_task",
        ] {
            assert!(
                is_text_input(label),
                "`{label}` is a free-text port; flagging it is the \
                 false-positive class scripts/port_census.py identified"
            );
        }
    }

    #[test]
    fn a_structured_port_is_still_a_mismatch() {
        // The widening must not swallow genuinely structured inputs.
        for label in [
            "gbif_key",
            "species_data",
            "reference-image",
            "h3-cell",
            "coherence-scores",
            "process_config_json",
            "keyframes",
        ] {
            assert!(
                !is_text_input(label),
                "`{label}` is structured; if the rule accepts it, the check \
                 has stopped detecting anything"
            );
        }
    }

    #[test]
    fn description_is_deliberately_not_text_shaped() {
        // `ar_beacon` accepts description/location. Posting a research
        // prompt into a caption field IS a mismatch, so this stays out. The
        // assertion documents the judgement rather than leaving it implicit
        // in the absence of a branch.
        assert!(!is_text_input("description"));
        assert!(is_text_input("free_text_stage_description"));
    }

    #[test]
    fn declaring_nothing_is_an_absence_not_a_contradiction() {
        let b = bind_input(&[]);
        assert_eq!(b, InputBinding::Undeclared);
        assert!(
            !b.is_mismatch(),
            "an agent that declared nothing has not contradicted anything"
        );
    }

    #[test]
    fn a_structured_only_agent_reports_what_it_did_declare() {
        let b = bind_input(&v(&["species_data", "taxonomy", "gbif_key"]));
        assert!(b.is_mismatch());
        assert_eq!(b.as_tag(), "no_text_input");
        match b {
            InputBinding::NoTextInput { declared } => {
                assert_eq!(declared.len(), 3, "the report must name the ports");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn tag_vocabulary_matches_what_stamp_invocation_already_writes() {
        // v0.16.0 episodes carry `ibind:declared-query` style tags produced
        // from these strings. Changing the vocabulary would silently split
        // the time series at the deploy boundary.
        assert_eq!(bind_input(&v(&["query"])).as_tag(), "declared:query");
        assert_eq!(bind_input(&v(&["gbif_key"])).as_tag(), "no_text_input");
        assert_eq!(bind_input(&[]).as_tag(), "undeclared");
    }

    #[test]
    fn the_pilot_agent_now_binds_cleanly() {
        // genome_profiler v1.2.0 accepts `query` after the port fix. Before
        // it, this returned NoTextInput while the handler sent prose.
        assert_eq!(
            bind_input(&v(&["query"])),
            InputBinding::Declared {
                label: "query".into()
            }
        );
    }

    /// **`is_mismatch` does not measure caller error, and promoting this gate
    /// to a Control would refuse half the corpus.**
    ///
    /// `command_registry` declares `Gate::InputBinding` a `Metric` on
    /// `agent.execute` and says the mismatch RATE is *"the number that would
    /// justify making it fatal"*. It was never computed. Here it is, over the
    /// cards on disk — and it justifies the opposite.
    ///
    /// `bind_input` is a pure function of the agent's own `accepts`: it asks
    /// whether any declared label *looks like free text*, and never looks at
    /// the query. So `NoTextInput` does not mean "a caller sent the wrong
    /// thing". It means **this agent's `accepts` lists the semantic slots its
    /// prompt needs rather than a transport shape**:
    ///
    /// ```text
    /// enemy_sensor     [creature_id, species_data, location_context]      62 pulses
    /// prey_locator     [...]                                              94 pulses
    /// naturalist       [creature_name, scientific_name, species_group]     47 pulses
    /// species_resolver [species-name, common-name, taxonomic-group]        15 pulses
    /// ```
    ///
    /// None of those is refusing prose. They are describing what their prompt
    /// wants *told*, and they are invoked with a query like everything else.
    /// Refusing them would break working agents with hundreds of pulses between
    /// them.
    ///
    /// **So the gate is blocked on a vocabulary question, not on a threshold.**
    /// `accepts` is doing two jobs — what an agent can be *handed*, and what its
    /// prompt needs to *know* — and until it means one thing, a mismatch count
    /// is not evidence about callers. That is the ports rung's problem
    /// (`docs/plans/PORTS_RUNG_EDITOR.md`), and this test is here so the
    /// promotion cannot be argued for without meeting the number first.
    ///
    /// The count may FALL freely: every agent that adopts a text label is
    /// progress. It may not rise without somebody saying why.
    #[test]
    fn promoting_input_binding_to_a_control_would_refuse_half_the_corpus() {
        let mut mismatch = Vec::new();
        let mut total = 0usize;
        for tier in std::fs::read_dir("agents").expect("agents/").flatten() {
            if !tier.path().is_dir() {
                continue;
            }
            for agent in std::fs::read_dir(tier.path()).expect("tier").flatten() {
                let card = agent.path().join("agent_card.json");
                if !card.exists() {
                    continue;
                }
                let raw = std::fs::read_to_string(&card).expect("read");
                let v: serde_json::Value = match serde_json::from_str(&raw) {
                    Ok(v) => v,
                    // A card this crate cannot parse is another test's finding.
                    Err(_) => continue,
                };
                let accepts: Vec<String> = v
                    .get("accepts")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                total += 1;
                if bind_input(&accepts).is_mismatch() {
                    mismatch.push(
                        agent
                            .path()
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    );
                }
            }
        }

        assert!(
            total > 80,
            "only {total} card(s) scanned — this guard is going vacuous"
        );
        // 47 of 102 when first measured; 48 of 102 after
        // `regulatory_lens_translator` was authored. A ratchet, not a target.
        //
        // **The rise is recorded rather than absorbed.** This number may fall
        // freely and may not rise, and it rose once — so here is the reason, in
        // the place a reader meets the number.
        //
        // `regulatory_lens_translator` declares `accepts` and nothing textual,
        // which is the 48th instance of the pattern that blocks this gate: an
        // agent listing the semantic slots its prompt needs told rather than a
        // transport shape it can be handed. It is not a badly authored card and
        // nothing here asks its author to change it. That is the point — the
        // pattern is what agents on this platform naturally do, it is still
        // spreading, and each new one makes `Gate::InputBinding` less
        // promotable rather than more.
        //
        // A rise weakens the case for promotion, so absorbing one silently
        // would be the failure this ratchet exists to prevent. If it reaches 50
        // the honest reading is that `accepts` has settled into meaning "what
        // my prompt needs", and `why_not_control` should stop describing the
        // mismatch rate as a threshold at all.
        assert!(
            mismatch.len() <= 48,
            "{} of {total} cards declare inputs and none textual, up from 48. \
             Every one of these would be REFUSED if `Gate::InputBinding` were \
             promoted, and they are not caller errors — they are agents using \
             `accepts` as a list of what their prompt needs told. Adding one \
             makes the gate less promotable, not more. Raise this only with the \
             new card NAMED in the comment above, as 48 was:\n  {}",
            mismatch.len(),
            mismatch.join("\n  ")
        );
        assert!(
            mismatch.len() * 3 > total,
            "the mismatch share has fallen below a third of the corpus ({} of \
             {total}). That is the direction that unblocks promotion — recount, \
             lower the ceiling above, and revisit `why_not_control` on \
             `agent.execute`, which currently says the rate is the number that \
             would justify making it fatal.",
            mismatch.len()
        );
    }
}
