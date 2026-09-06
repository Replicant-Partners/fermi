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

// ─── substitutability ──────────────────────────────────────────

/// How much a shared `accepts` label narrows the fleet.
///
/// # Why this is three states and not a count
///
/// A label everybody answers to is not a seam, it is the platform's calling
/// convention. `query` is accepted by 24 of 102 cards; knowing an ask is a
/// `query` excludes nothing and recommending "any of these 24" is not
/// navigation. Meanwhile `workspace-state` is accepted by 8 — every one of them
/// a coordination or coherence agent — and that genuinely is a set of
/// substitutes.
///
/// Same shape as `gate_trust`'s readings, for the same reason: a measurement
/// that fires on almost everything has to say so itself, or a reader takes the
/// count for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Substitutes {
    /// One agent answers to this label. There is no substitute — which is a
    /// true and useful answer, not a missing one.
    Bespoke,
    /// A real set of interchangeable answerers.
    Cohort(usize),
    /// So many agents accept it that the label describes the calling
    /// convention rather than a specialisation.
    Universal(usize),
}

/// Above this share of the corpus, a shared label stops narrowing anything.
///
/// A tenth, and the argument is not the number: if more than one agent in ten
/// answers to a label, the label is describing how the platform is CALLED
/// rather than what any of them is FOR. `query` sits at 24%, `workspace-state`
/// at 8%, and the gap between those two is the whole distinction.
///
/// Held by `query_is_not_a_cohort_and_workspace_state_is`. If a domain label
/// ever crosses this line the guard fails, which is the intended outcome: it
/// means a specialisation has become a convention and somebody should decide
/// whether that was on purpose.
pub const UNIVERSAL_SHARE: f64 = 0.10;

/// Classify one label from the number of agents accepting it.
pub fn substitutes(accepting: usize, corpus: usize) -> Substitutes {
    if accepting <= 1 {
        return Substitutes::Bespoke;
    }
    if corpus > 0 && (accepting as f64) / (corpus as f64) > UNIVERSAL_SHARE {
        return Substitutes::Universal(accepting);
    }
    Substitutes::Cohort(accepting)
}

/// Who else answers the same ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answerers {
    /// The `accepts` label, as declared.
    pub question: String,
    /// Agent ids, sorted, so the set reads the same on every call.
    pub agents: Vec<String>,
    pub reading: Substitutes,
}

/// **Which agents are interchangeable for a given ask.**
///
/// # What this is, and what it deliberately is not
///
/// It is the half of "composability" that is computable from cards today, and
/// the only fleet claim about composition that `xaman_ek` can make without
/// asserting anything: *"five agents answer this question, here they are."*
///
/// It is **not** chainability — whether one agent's artifact can feed another's
/// input. That is not in the labels and cannot be derived from them. Measured
/// over every hand-off that has actually happened in production, the declared
/// ports predict **none** of them:
///
/// ```text
/// weather_oracle -> weather_ensemble_forecaster   produces fermi/weather_market_call
///                                                 accepts  forecast-question
///                                                 overlap  NONE            (4 hops)
/// ```
///
/// That is not drift. `produces` converged on *the artifact I make* and
/// `accepts` on *the question I answer* — in the twelve fully typed agents,
/// 100% of produces labels are namespaced and none of them appears on any
/// accepts. The cards describe **request/response**, so a caller poses a
/// question and a callee returns an artifact, and the two vocabularies are not
/// supposed to meet. Looking for a pipe finds nothing and the nothing means
/// the query was wrong.
///
/// Chainability lives in `episodes.parent_episode_id` — observed rather than
/// declared, which is the right shape for a fleet that reconfigures itself.
///
/// # The counter-intuitive part
///
/// Typing made accepts labels **less** shared, not more. A namespaced request
/// type is per-agent by construction (`abw/genome-query/1` is genome_profiler's
/// alone), so of twelve namespaced accepts labels in the corpus exactly one is
/// shared. The cohorts live in the organic, untyped vocabulary that agents
/// converged on without being told to — `workspace-state`, `location_context`,
/// `creature_name`.
///
/// `fermi/forecast-question/1` is the exception and the pattern worth copying:
/// a namespaced question type deliberately shared, answered by five agents. It
/// is the only place the typed vocabulary expresses a cohort, and it does it
/// well.
pub fn answerers(cards: &[(String, Vec<String>)]) -> Vec<Answerers> {
    use std::collections::BTreeMap;
    let corpus = cards.len();
    let mut by_label: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (agent, accepts) in cards {
        for label in accepts {
            by_label
                .entry(label.as_str())
                .or_default()
                .push(agent.as_str());
        }
    }
    by_label
        .into_iter()
        .map(|(question, mut agents)| {
            agents.sort_unstable();
            agents.dedup();
            Answerers {
                question: question.to_string(),
                reading: substitutes(agents.len(), corpus),
                agents: agents.into_iter().map(str::to_string).collect(),
            }
        })
        .collect()
}

// ─── chainability, observed ───────────────────────────────────

/// **Which agents have actually fed which**, from the run record.
///
/// The other half of composability, and the half that cannot come from cards.
/// [`answerers`] says who answers the same ask; this says what actually
/// composed. Declared ports predict none of it — measured over every hand-off
/// in production, the overlap between caller `produces` and callee `accepts` is
/// empty in every case — because the two vocabularies describe
/// request/response rather than a pipe.
///
/// # Why observed rather than computed
///
/// A computed compatibility check would refuse the entire real topology, and
/// as an authority it would also freeze a fleet whose whole point is that it
/// reconfigures itself. Observation has neither problem: it cannot refuse
/// anything, and it describes what the system became rather than what someone
/// predicted it would.
///
/// `parent_episode_id` arrived with migration 198 for a different reason — a
/// delegated run writes its own episode instead of folding its tokens into its
/// caller's — and the topology is a free consequence of that decision.
///
/// # Read-only, and joined through `agents` deliberately
///
/// Returns names rather than ids because a seam is read by a person or quoted
/// by a navigator, and a uuid pair is not an answer to *"what feeds what"*.
/// Agents that have since been deleted drop out of the join, which is correct:
/// a seam whose endpoint no longer exists is not a seam anyone can use.
pub const OBSERVED_SEAMS_SQL: &str = "SELECT pa.agent_name AS caller,                                              ca.agent_name AS callee,                                              count(*)::bigint AS hops,                                              max(c.created_at) AS last_seen                                         FROM episodes c                                         JOIN episodes p ON p.episode_id = c.parent_episode_id                                         JOIN agents ca ON ca.agent_id = c.agent_id                                         JOIN agents pa ON pa.agent_id = p.agent_id                                        WHERE c.parent_episode_id IS NOT NULL                                     GROUP BY 1, 2                                     ORDER BY hops DESC, caller, callee";

/// Does the declared vocabulary account for a seam that happened?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeamAgreement {
    /// The caller produces a label the callee accepts. The manifest describes
    /// the behaviour.
    Declared,
    /// They composed and no declared label connects them. **A finding about the
    /// cards, not about the composition** — the run succeeded.
    Undeclared,
}

/// Compare one observed hand-off against what the two cards declare.
///
/// Pure over the two label sets so it can be checked without a database, and
/// deliberately not a verdict on the composition: the only thing an
/// `Undeclared` seam tells you is that the ports do not describe a hand-off
/// that demonstrably works. Every seam in the corpus reads `Undeclared` today.
pub fn seam_agreement(caller_produces: &[String], callee_accepts: &[String]) -> SeamAgreement {
    if caller_produces
        .iter()
        .any(|p| callee_accepts.iter().any(|a| a == p))
    {
        SeamAgreement::Declared
    } else {
        SeamAgreement::Undeclared
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

    // ── substitutability ─────────────────────────────────────

    /// Every card on disk, as `answerers` wants them.
    fn corpus() -> Vec<(String, Vec<String>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/curated");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("agents/curated") {
            let Ok(entry) = entry else { continue };
            let card = entry.path().join("agent_card.json");
            let Ok(body) = std::fs::read_to_string(&card) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
                continue;
            };
            let id = entry.file_name().to_string_lossy().into_owned();
            let accepts = v
                .get("accepts")
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            out.push((id, accepts));
        }
        assert!(
            out.len() > 80,
            "found {} cards; a scan over an empty set passes for ever",
            out.len()
        );
        out
    }

    fn reading_of(rows: &[Answerers], label: &str) -> Option<Substitutes> {
        rows.iter().find(|r| r.question == label).map(|r| r.reading)
    }

    /// **The distinction the whole thing turns on.**
    ///
    /// `query` is accepted by roughly a quarter of the corpus. Reporting those
    /// as a set of substitutes would tell a navigator that two dozen agents
    /// are interchangeable for any free-text ask, which is true and useless —
    /// it is the platform's calling convention, not a specialisation.
    ///
    /// `workspace-state` is accepted by eight, every one a coordination or
    /// coherence agent, and that is a real cohort.
    ///
    /// Both readings are asserted against the cards on disk rather than a
    /// fixture, because a fixture would let the threshold drift away from the
    /// corpus it is supposed to describe.
    #[test]
    fn query_is_not_a_cohort_and_workspace_state_is() {
        let rows = answerers(&corpus());

        assert!(
            matches!(reading_of(&rows, "query"), Some(Substitutes::Universal(_))),
            "`query` is not reading as Universal: {:?}. It is accepted by a \
             quarter of the fleet; if it reads as a cohort then every ask \
             returns two dozen interchangeable agents and the signal is gone.",
            reading_of(&rows, "query")
        );

        assert!(
            matches!(
                reading_of(&rows, "workspace-state"),
                Some(Substitutes::Cohort(_))
            ),
            "`workspace-state` is not reading as a Cohort: {:?}. Either the \
             label stopped being shared, or it crossed UNIVERSAL_SHARE — and \
             the second means a specialisation became a convention, which is \
             worth someone deciding on rather than absorbing.",
            reading_of(&rows, "workspace-state")
        );
    }

    /// A label only one agent accepts has no substitute, and that is an answer.
    ///
    /// `Bespoke` rather than absent, for the reason this module's sibling
    /// `InputBinding::Undeclared` exists: nobody-answers-this and
    /// nobody-declared-anything are different findings, and collapsing them
    /// makes a navigator say "I don't know" where it could say "only this one".
    #[test]
    fn a_label_with_one_answerer_is_bespoke_not_missing() {
        let rows = answerers(&corpus());
        let bespoke = rows
            .iter()
            .filter(|r| r.reading == Substitutes::Bespoke)
            .count();
        assert!(
            bespoke > 100,
            "only {bespoke} bespoke labels across the corpus, which does not \
             match a fleet where 231 of 243 accepts labels are unshared. The \
             classification is collapsing distinct labels."
        );

        // And the typed request types are the clearest case: a namespaced
        // query type belongs to the one agent that answers it.
        assert_eq!(
            reading_of(&rows, "abw/genome-query/1"),
            Some(Substitutes::Bespoke),
            "a namespaced per-agent request type must read Bespoke; if it is a \
             cohort, two agents now claim the same typed question and one of \
             them is wrong"
        );
    }

    /// The one designed shared question type must keep working.
    ///
    /// `fermi/forecast-question/1` is the only namespaced accepts label in the
    /// corpus that more than one agent answers — five do. It is the pattern
    /// worth propagating: precision and sharing at once, which every other
    /// typed port gets only the first half of.
    ///
    /// Pinned as a floor rather than an equality: agents answering it may grow.
    #[test]
    fn the_shared_question_type_is_the_pattern_that_works() {
        let rows = answerers(&corpus());
        let row = rows
            .iter()
            .find(|r| r.question == "fermi/forecast-question/1")
            .expect(
                "`fermi/forecast-question/1` is gone. It was the only typed \
                 question type expressing a cohort; losing it means the typed \
                 vocabulary no longer expresses substitutability anywhere.",
            );
        assert!(
            row.agents.len() >= 5,
            "{} agents answer the shared forecast question, down from 5: {:?}",
            row.agents.len(),
            row.agents
        );
        assert!(
            matches!(row.reading, Substitutes::Cohort(_)),
            "the shared forecast question reads {:?} rather than a cohort",
            row.reading
        );
    }

    /// The three readings partition, and the threshold is what separates two
    /// of them.
    #[test]
    fn the_readings_partition_and_the_threshold_bites() {
        assert_eq!(substitutes(0, 100), Substitutes::Bespoke);
        assert_eq!(substitutes(1, 100), Substitutes::Bespoke);
        assert_eq!(substitutes(2, 100), Substitutes::Cohort(2));
        assert_eq!(substitutes(10, 100), Substitutes::Cohort(10));
        assert_eq!(
            substitutes(11, 100),
            Substitutes::Universal(11),
            "one agent past a tenth of the corpus must tip to Universal, or \
             UNIVERSAL_SHARE is not doing anything"
        );
        // A one-agent corpus cannot have a cohort, and must not divide by zero
        // into one either.
        assert_eq!(substitutes(1, 1), Substitutes::Bespoke);
        assert_eq!(substitutes(5, 0), Substitutes::Cohort(5));
    }

    // ── chainability, observed ───────────────────────────────

    /// Ports a card declares, for the seam comparisons below.
    fn card_ports(agent: &str) -> (Vec<String>, Vec<String>) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("agents/curated")
            .join(agent)
            .join("agent_card.json");
        let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{agent}: {e}"));
        let v: serde_json::Value = serde_json::from_str(&body).expect("card parses");
        let get = |k: &str| {
            v.get(k)
                .and_then(|a| a.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        (get("accepts"), get("produces"))
    }

    /// **Every hand-off that has actually happened is undeclared.**
    ///
    /// The three pairs below are the entire observed topology of the platform,
    /// read from `episodes.parent_episode_id`. Not one of them has a declared
    /// label in common, and all three ran successfully — `weather_oracle` has
    /// asked the ensemble forecaster four times.
    ///
    /// This is what makes computing chainability from labels a non-starter: as
    /// a check it would have a 100% false-negative rate on the real topology,
    /// and as an authority it would refuse the compositions the platform
    /// actually performs.
    ///
    /// # This test going red would be good news
    ///
    /// It asserts a defect, which is unusual, so the direction matters. If a
    /// pair here becomes `Declared` the port vocabulary has started describing
    /// real hand-offs — update the expectation and say which pair converged.
    /// The failure to be alarmed by is the opposite one: a pair disappearing,
    /// which means the seam stopped being exercised.
    #[test]
    fn every_observed_seam_is_undeclared() {
        // (caller, callee) — the observed topology, hops in the comment.
        const OBSERVED: &[(&str, &str)] = &[
            ("weather_oracle", "weather_ensemble_forecaster"), // 4 hops
            ("weather_oracle", "weather_calibrator"),          // 1
            ("simops_companion", "supply_chain_oracle"),       // 1
        ];

        for (caller, callee) in OBSERVED {
            let (_, produces) = card_ports(caller);
            let (accepts, _) = card_ports(callee);
            assert!(
                !produces.is_empty() && !accepts.is_empty(),
                "{caller} -> {callee}: one side declares no ports, so this pair \
                 cannot demonstrate anything about the vocabulary"
            );
            assert_eq!(
                seam_agreement(&produces, &accepts),
                SeamAgreement::Undeclared,
                "{caller} -> {callee} now has a declared label in common. That \
                 is the port vocabulary starting to describe real hand-offs, \
                 which is the outcome the ports rung is for — update this \
                 expectation and name the pair that converged.\n  \
                 produces: {produces:?}\n  accepts:  {accepts:?}"
            );
        }
    }

    /// The comparison must be able to see agreement when it exists.
    ///
    /// Without this the assertion above is satisfied by a function that always
    /// returns `Undeclared`, which would agree with the corpus and prove
    /// nothing — the vacuity that made the trace's 6,000-character window pass
    /// for months.
    #[test]
    fn the_seam_comparison_can_see_a_match() {
        let produces = vec!["fermi/weather_market_call".to_string()];
        assert_eq!(
            seam_agreement(&produces, &["fermi/weather_market_call".to_string()]),
            SeamAgreement::Declared
        );
        assert_eq!(
            seam_agreement(&produces, &["forecast-question".to_string()]),
            SeamAgreement::Undeclared
        );
        // An empty side cannot agree with anything.
        assert_eq!(seam_agreement(&produces, &[]), SeamAgreement::Undeclared);
        assert_eq!(seam_agreement(&[], &produces), SeamAgreement::Undeclared);
    }

    /// The topology query reads and joins, and never writes.
    #[test]
    fn the_seam_query_is_read_only_and_names_both_ends() {
        let q = OBSERVED_SEAMS_SQL.to_ascii_lowercase();
        assert!(q.trim_start().starts_with("select"));
        for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
            assert!(!q.contains(w), "the seam query contains `{w}`");
        }
        assert!(
            q.contains("parent_episode_id is not null"),
            "without the parent filter every episode joins to nothing and the \
             topology reads as empty rather than as unrecorded"
        );
        for end in ["caller", "callee"] {
            assert!(
                q.contains(end),
                "the query does not name `{end}`; a uuid pair is not an answer \
                 to what feeds what"
            );
        }
    }
}
