//! Does the loop's output carry the signal its claim requires?
//!
//! # The question no other rung asks
//!
//! | asks | module |
//! |---|---|
//! | does the declared object exist? | `schema_trust` |
//! | does the writer ever run? | `liveness_trust` |
//! | does the stored value equal its source? | `rollup_trust` |
//! | does the chain produce, stage by stage? | `loop_model` |
//! | **does what it produces carry the signal the claim needs?** | here |
//!
//! Not a rung of [`crate::ladder`]. The paper defines five and a test pins that
//! number; this sits *over* them, in the same position as loops and gates —
//! which is why `panel_absence::rung_of` returns `None` for those too.
//!
//! # Turning is not closed
//!
//! `loop_model` reports a loop as `turning` when every stage has produced. That
//! is a fact about rows. It is entirely compatible with the loop producing a
//! number that cannot distinguish the things it is named after, and two of this
//! platform's six loops are in exactly that state.
//!
//! **Loop 5.A, measured 2026-08-24.** Its `scored` stage is declared as
//! *"per-agent calibration is recorded"*. 239 signals, 7 agents, and on every
//! forecast with more than one contributing agent there is **exactly one
//! distinct score**: `record_forecast_calibration_signals` takes the
//! *forecast's* Brier and writes it once per name in `agents_used`. An agent
//! that carried the forecast and an agent that contributed nothing score
//! identically, by construction. Four of the agents have identical minima
//! (0.805) and means (0.987) because they are the same numbers.
//!
//! The loop is turning. What it produces contains no agent-level information at
//! all, so nothing downstream — the MoE router at Stage 0, composition
//! evolution, the counterfactual attributor — can be reading agent skill from
//! it, whatever it believes.
//!
//! # Why a contract and not a dashboard
//!
//! Because the failure is silent and *upward*: a metric at ceiling, or a metric
//! that is uniform across its subjects, produces a healthy-looking series. The
//! same reason `liveness_trust` exists one rung down — an empty table and an
//! unwritten one look alike, and a uniform metric and a well-calibrated fleet
//! look alike.
//!
//! # This module owns no arithmetic
//!
//! It declares the shape and the SQL; the verdicts are pure functions over
//! counts. Per `verification_for_agent_ecologies.md` §3.4 a trust calculation
//! must have exactly one implementation, and `loop_model` already owns "did it
//! run". Nothing here re-answers that.

/// A metric a loop's claim depends on, and what must be true of it.
#[derive(Debug, Clone, Copy)]
pub struct OutcomeContract {
    /// The loop and stage this is about. Checked to exist in
    /// [`crate::loop_model::LOOPS`] by `every_contract_names_a_real_stage`.
    pub loop_id: &'static str,
    pub stage: &'static str,
    /// The architecture's claim for the stage, quoted from `loop_model` so the
    /// two cannot drift.
    pub claim: &'static str,
    /// The narrower proposition this contract actually checks.
    ///
    /// **Never the claim itself.** A contract that says it verifies the claim is
    /// the over-reading this whole audit is about; the gap between the two is
    /// stated in `does_not_show` rather than left for a reader to notice.
    pub proposition: &'static str,
    /// What passing does *not* establish. Required.
    pub does_not_show: &'static str,
    /// One row per event, `(subjects, distinct_values)`.
    ///
    /// An "event" is whatever the metric is computed over — a forecast, a run —
    /// and a "subject" is what the metric claims to be *about*. If subjects
    /// sharing an event never differ, the metric is event-level wearing a
    /// subject-level name.
    pub spread_sql: &'static str,
    /// One row per producer of the metric: `(producer, n)`.
    pub producer_sql: &'static str,
    /// Fewest multi-subject events for the spread reading to mean anything.
    pub min_events: usize,
    /// Reach, when the claim says the output returns to its producer.
    ///
    /// `(producing_sql, receiving_sql, floor_pct)`. `None` when the claim makes
    /// no such promise — Loop 5.A's does not; a forecast's score is not owed
    /// back to the forecast.
    ///
    /// **The floor is the measured value, not a target.** Setting a target
    /// after taking a measurement is fitting the threshold to the data, and
    /// this codebase has the instrument for the alternative:
    /// `uninstrumented_swallows_may_only_decrease` records what is true and
    /// insists it improve. So the live tier fails if reach falls below the
    /// floor *and* if it rises above it without the floor being raised.
    pub reach: Option<(&'static str, &'static str, u32)>,
    /// What goes wrong when the signal is absent. Specific to this metric.
    pub why: &'static str,
}

/// One event's spread: how many subjects, and how many values between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSpread {
    pub subjects: usize,
    pub distinct_values: usize,
}

/// Can the metric tell its subjects apart?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Discrimination {
    /// Enough events with differing subject values to be a population.
    Discriminates { events: usize, varied: usize },
    /// Some variation, and less of it than `min_events`.
    ///
    /// **Evidence for uniformity, not absence of evidence** — which is what
    /// separates it from [`Underpowered`](Discrimination::Underpowered). There
    /// are plenty of events; almost none of them vary.
    ///
    /// This arm exists because the first version of this module said *"one
    /// varying event settles it"*, on the reasoning that the verdict is about
    /// whether the number CAN differ. Against production that returned
    /// `Discriminates { events: 50, varied: 1 }` — and the single varied event
    /// turned out to be two different producers' rows sharing a `rationale`
    /// string, so it was a grouping artifact and not variation at all. A rule
    /// that clears an instrument on one observation clears it on noise.
    ///
    /// The threshold is `min_events`, the same number the contract already
    /// declares for the other direction, so nothing here was fitted to the data
    /// after seeing it.
    Sparse { events: usize, varied: usize },
    /// Every multi-subject event has exactly one value.
    ///
    /// The metric is about the event, not the subject, whatever it is called.
    Uniform { events: usize },
    /// Too few multi-subject events to say either way.
    ///
    /// **Not a pass, and not a failure.** A metric observed on one two-subject
    /// event has demonstrated nothing, and calling that `Uniform` would be a
    /// positive claim on a single observation — the same error as `no_input`
    /// one rung down.
    Underpowered { events: usize, need: usize },
    /// No event has more than one subject, so the question does not arise.
    ///
    /// Distinct from `Underpowered`: there is nothing to wait for. A metric
    /// whose subjects never co-occur cannot be checked this way and needs a
    /// different instrument.
    NoSharedEvents,
}

impl Discrimination {
    /// Is this a finding, as opposed to a pass or an unavailable reading?
    ///
    /// Three-way, and the middle case is the one worth naming: `Underpowered`
    /// and `NoSharedEvents` are **not** findings and **not** passes. Folding
    /// either into `false` alongside `Discriminates` would let "we could not
    /// look" report as "we looked and it was fine", which is the error this
    /// module inherited its vocabulary for.
    pub fn is_finding(self) -> bool {
        matches!(
            self,
            Discrimination::Uniform { .. } | Discrimination::Sparse { .. }
        )
    }

    /// Did a reading happen at all?
    pub fn is_reading(self) -> bool {
        !matches!(
            self,
            Discrimination::Underpowered { .. } | Discrimination::NoSharedEvents
        )
    }
}

/// Classify the spread across events.
///
/// Only multi-subject events count. A single-subject event has one value by
/// definition, and counting those as evidence of uniformity is how this check
/// would report a fleet-wide defect on a platform with one agent per forecast.
pub fn classify_discrimination(spreads: &[EventSpread], min_events: usize) -> Discrimination {
    let shared: Vec<&EventSpread> = spreads.iter().filter(|s| s.subjects > 1).collect();
    if shared.is_empty() {
        return Discrimination::NoSharedEvents;
    }
    let varied = shared.iter().filter(|s| s.distinct_values > 1).count();

    // Not enough events to say anything in either direction.
    if shared.len() < min_events {
        return Discrimination::Underpowered {
            events: shared.len(),
            need: min_events,
        };
    }
    // Enough events, and enough of them vary to be a population rather than an
    // artifact. `min_events` is used for both directions on purpose: a second
    // constant here would be one fitted to whatever the data happened to show.
    if varied >= min_events {
        return Discrimination::Discriminates {
            events: shared.len(),
            varied,
        };
    }
    if varied > 0 {
        return Discrimination::Sparse {
            events: shared.len(),
            varied,
        };
    }
    Discrimination::Uniform {
        events: shared.len(),
    }
}

/// How many things write this metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Producers {
    /// Nobody writes it. `liveness_trust`'s question, not this one — reported
    /// so the spread verdict is not read as meaningful over an empty set.
    None,
    /// One producer, one denominator.
    Single,
    /// **More than one, and nothing compares them.**
    ///
    /// Loop 5.A's state: `brier_forecast_resolver v1` writes one signal per
    /// resolved forecast, and `brier v1` writes one signal per *aggregate over
    /// N forecasts*. Both land in `dimension = 'forecast_calibration'`, so any
    /// reader that averages the column is averaging per-forecast scores together
    /// with multi-forecast means and weighting them equally.
    ///
    /// The same shape as the seam registry's findings one layer down: two
    /// independently-correct producers of one vocabulary, and nothing that
    /// compares them.
    Conflated { producers: usize },
}

/// Does the loop's output come back to the subject that fed it?
///
/// The other half of "carries the signal its claim needs". A metric can
/// discriminate perfectly and still never reach the thing it is about: Loop 1
/// distils rules for 84 agents and 7 of them have ever had one retrieved, so
/// for the other 77 the loop consumes their experience and returns nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// At or above the declared floor.
    Closes { producing: usize, receiving: usize },
    /// Some subjects receive, and fewer than the floor.
    ///
    /// Reported against a **measured** floor rather than a target, so no number
    /// here was chosen after seeing the data — see [`OutcomeContract::reach_floor_pct`].
    Narrow { producing: usize, receiving: usize },
    /// Nothing comes back to anything.
    ///
    /// Unambiguous, and the only arm this module asserts on: a loop that
    /// returns to no producer at all is open, whatever its row counts say.
    Open { producing: usize },
    /// Nothing has produced, so the question does not arise. Not a pass.
    NoProducers,
}

/// Percent of producing subjects that receive.
///
/// Integer percent on purpose: a float here invites a threshold expressed to
/// two decimal places, which is a target fitted to a measurement.
pub fn reach_pct(producing: usize, receiving: usize) -> u32 {
    if producing == 0 {
        return 0;
    }
    ((receiving * 100) / producing) as u32
}

/// Classify reach against the contract's measured floor.
pub fn classify_reach(producing: usize, receiving: usize, floor_pct: u32) -> Reach {
    if producing == 0 {
        return Reach::NoProducers;
    }
    if receiving == 0 {
        return Reach::Open { producing };
    }
    if reach_pct(producing, receiving) >= floor_pct {
        Reach::Closes {
            producing,
            receiving,
        }
    } else {
        Reach::Narrow {
            producing,
            receiving,
        }
    }
}

/// Classify producer count against the declared expectation.
///
/// Two producers are not automatically wrong — they are automatically
/// *undeclared*, which is the thing this reports. A legitimate second producer
/// belongs in [`SHARED_METRICS`] with a reason.
pub fn classify_producers(producers: usize, declared_shared: bool) -> Producers {
    match producers {
        0 => Producers::None,
        1 => Producers::Single,
        n if declared_shared => {
            let _ = n;
            Producers::Single
        }
        n => Producers::Conflated { producers: n },
    }
}

/// A metric known not to carry the signal its claim needs.
///
/// The findings this module makes are real and some of them cannot be fixed
/// this week — Loop 5.A's uniformity needs the counterfactual attributor, which
/// is blocked behind Loop 4's claims, which are blocked behind the shape of the
/// requests. A suite that is permanently red for a known state is a suite
/// people stop reading, and §5.2 is explicit that the deletion which follows
/// looks like cleanup.
///
/// So the same instrument as `liveness_trust::KNOWN_SILENT`: a declared
/// baseline that **may only shrink**, where every entry states what would clear
/// it, and the live tier asserts that a declared gap is *still* there — so an
/// entry cannot quietly outlive its reason.
#[derive(Debug, Clone, Copy)]
pub struct KnownGap {
    /// `loop.stage`, matching an [`OutcomeContract`].
    pub metric: &'static str,
    /// Which finding: `uniform` or `conflated`.
    pub gap: &'static str,
    pub why: &'static str,
    /// What would remove this entry.
    ///
    /// Required, and the field that makes this a baseline rather than a
    /// permission. An exemption with no exit condition is permanent, and the
    /// one this list replaced — `semantic_rules.application_count` in
    /// `KNOWN_SILENT` — was removed by its own stated condition on the first
    /// run that met it.
    pub cleared_by: &'static str,
}

/// Every declared gap. **May only shrink.**
pub const KNOWN_GAPS: &[KnownGap] = &[
    KnownGap {
        metric: "loop5a.scored",
        gap: "uniform",
        why: "`record_forecast_calibration_signals` takes the forecast's Brier \
              and writes it once per name in `agents_used`. Measured 2026-08-24: \
              47 forecasts, several agents each, exactly one distinct score on \
              every one. An agent that carried the forecast and one that \
              contributed nothing are indistinguishable in this column.",
        cleared_by: "`attribution::counterfactual` already computes what each \
                     agent's claim was worth by synthesising the forecast for \
                     any subset — exact Shapley credit from one real forecast, \
                     no extra runs. It has never executed because \
                     `forecast_agent_claims` is empty, which is Loop 4's first \
                     empty link. Writing per-agent calibration from that, rather \
                     than copying the forecast's, removes this entry.",
    },
    KnownGap {
        metric: "loop5a.scored",
        gap: "conflated",
        why: "Two producers write `dimension = 'forecast_calibration'`: \
              `brier_forecast_resolver v1` (one signal per resolved forecast) \
              and `brier v1` (one per aggregate over N forecasts). Their \
              denominators differ and the column cannot say which is which, so \
              any reader that averages it weights a single-forecast score \
              equally with a mean over forty-eight.",
        cleared_by: "Either one producer, or two dimensions. `brier v1`'s \
                     rationale identifies neither the agent nor the forecast, \
                     so it also cannot be grouped into events — which is why \
                     the spread check has to scope itself to the other \
                     producer, and is reading two thirds of its subject.",
    },
];

/// Is this gap declared?
pub fn known_gap(metric: &str, gap: &str) -> Option<&'static KnownGap> {
    KNOWN_GAPS
        .iter()
        .find(|g| g.metric == metric && g.gap == gap)
}

/// Metrics deliberately written by more than one producer, with the reason.
///
/// Shaped like `liveness_trust::KNOWN_SILENT` and `grounding_trust`'s
/// cross-check exemptions, for the same reason: an entry must give a reason and
/// **the list may only shrink**.
///
/// Empty. `forecast_calibration` has two producers and that is a finding, not
/// an exemption — the two compute over different denominators and the column
/// cannot say which is which.
pub const SHARED_METRICS: &[(&str, &str)] = &[];

/// Is this metric's second producer declared?
pub fn shared_metric(key: &str) -> Option<&'static str> {
    SHARED_METRICS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, why)| *why)
}

/// Every metric a loop's claim rests on.
///
/// Rule for adding one: **if a loop's claim would be false when the metric is
/// uniform across its subjects, it belongs here.**
pub const OUTCOME_CONTRACTS: &[OutcomeContract] = &[
    OutcomeContract {
        loop_id: "loop5a",
        stage: "scored",
        claim: "A prediction is scored against an outcome that resolves \
            independently of it.",
        proposition: "The per-agent calibration signal takes different values for \
                  different agents on the same forecast.",
        does_not_show: "Nothing about whether any agent is well calibrated, and \
                    nothing about whether the outcome resolved independently. \
                    Discrimination is a property of the instrument: it says the \
                    number could differ between agents, not that the agents \
                    differ, and certainly not that the Brier is honest. \
                    `forecast_spacetime` and the commitment hash are what \
                    address independence; this addresses only whether the \
                    signal is about an agent at all.",
        // `rationale` identifies the event: `brier_forecast_resolver` writes one
        // distinct rationale per forecast, so grouping on it groups by forecast
        // without needing a column the table does not have. Stated because it is a
        // proxy, and a proxy that stops holding when a producer changes its wording.
        //
        // **Scoped to one producer, and that correction took two passes.**
        //
        // Grouping on `rationale` across the whole dimension merged rows from the
        // two producers and produced one "event" with 18 subjects and 2 distinct
        // values, which the check read as discrimination. Adding the producer to
        // the GROUP BY did not fix it: `brier v1` writes `Brier 0.000 over 1
        // forecasts` for *every* agent-aggregate that scores that way, so 18 rows
        // from three unrelated agents still collapsed into one bucket.
        //
        // `brier v1`'s rationale is not an event key at all — it identifies neither
        // the agent nor the forecast — so its rows cannot be grouped into events by
        // any column this table has, and no spread reading over them means
        // anything. That is not a gap in this check; it is part of the `Conflated`
        // finding the producer test reports, and the remedy is the same one.
        //
        // So this reads only `brier_forecast_resolver`, whose rationale carries the
        // forecast id and therefore is an event key. Scoping it is stated here
        // rather than done quietly, because a filter that silently drops a
        // producer is how a check comes to report on a third of its subject.
        spread_sql: "SELECT count(*)::bigint AS subjects, \
                        count(DISTINCT score)::bigint AS distinct_values \
                   FROM eval_signals \
                  WHERE dimension = 'forecast_calibration' \
                    AND evaluator_name = 'brier_forecast_resolver' \
                  GROUP BY rationale",
        producer_sql: "SELECT (evaluator_name || ' ' || evaluator_version) AS producer, \
                          count(*)::bigint AS n \
                     FROM eval_signals \
                    WHERE dimension = 'forecast_calibration' \
                    GROUP BY 1",
        min_events: 5,
        reach: None,
        why: "Loop 4's MoE router reads this at Stage 0 to weight agents, and \
          composition evolution proposes roster changes from it. A metric that \
          is uniform across agents on every forecast routes and re-rosters on \
          no information while looking like measured contribution — which is \
          worse than having no metric, because a missing number invites a \
          question and a uniform one does not.",
    },
    OutcomeContract {
        loop_id: "loop1",
        stage: "retrieved",
        claim: "An agent's own experience changes how it reasons: episodes \
                cluster into semantic rules, and those rules are retrieved into \
                the next prompt.",
        proposition: "An agent that distilled a rule has had one retrieved.",
        does_not_show: "Nothing about whether retrieval changed the agent's \
                        output. That is the measurement this platform does not \
                        have and cannot take from stored data: it needs a \
                        control arm, and forming one means suppressing rule \
                        injection for a turn, which nothing does. Reach is the \
                        weaker claim and the honest one — the rules came back \
                        to the agent that made them. Whether the agent was any \
                        different for it is unmeasured, and its own \
                        `extraction_utility` signal has fired twice.",
        // Discrimination is the other half here, and it is reported rather than
        // asserted: does one agent's rules differ in how often they are
        // retrieved, or is retrieval blanket? Either answer is legitimate, so
        // there is no finding to raise — which is why this stage carries a
        // `reach` and Loop 5.A does not.
        spread_sql: "SELECT count(*)::bigint AS subjects, \
                            count(DISTINCT application_count)::bigint AS distinct_values \
                       FROM semantic_rules \
                      GROUP BY agent_id",
        producer_sql: "SELECT 'consolidation' AS producer, count(*)::bigint AS n \
                         FROM semantic_rules",
        min_events: 5,
        reach: Some((
            "SELECT count(DISTINCT agent_id)::bigint FROM semantic_rules",
            "SELECT count(DISTINCT agent_id)::bigint FROM semantic_rules \
              WHERE application_count > 0",
            8,
        )),
        why: "A rule nobody retrieves is a dream cycle nobody woke from. The \
              agent paid for the consolidation, the rule sits in the table, and \
              the next prompt is built without it — so the loop's cost is real \
              and its effect is zero, for 77 of the 84 agents that have fed it.",
    },
];

/// The contract for a loop stage, if one is declared.
pub fn contract_for(loop_id: &str, stage: &str) -> Option<&'static OutcomeContract> {
    OUTCOME_CONTRACTS
        .iter()
        .find(|c| c.loop_id == loop_id && c.stage == stage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(subjects: usize, distinct_values: usize) -> EventSpread {
        EventSpread {
            subjects,
            distinct_values,
        }
    }

    /// The state Loop 5.A is actually in.
    #[test]
    fn a_metric_that_never_varies_between_subjects_is_uniform() {
        // Six forecasts, four agents each, one score each. Measured shape.
        let spreads: Vec<EventSpread> = (0..6).map(|_| ev(4, 1)).collect();
        assert_eq!(
            classify_discrimination(&spreads, 5),
            Discrimination::Uniform { events: 6 }
        );
    }

    /// One varying event in fifty does not clear the instrument.
    ///
    /// The state production was actually in, and the state the first version of
    /// this function called `Discriminates`. The single varied event was two
    /// producers' rows sharing a `rationale`, so it was a grouping artifact —
    /// but the rule would have cleared a real one-in-fifty too, and one
    /// observation cannot be told from noise.
    #[test]
    fn one_varying_event_in_fifty_is_sparse_not_discrimination() {
        let mut spreads: Vec<EventSpread> = (0..49).map(|_| ev(4, 1)).collect();
        spreads.push(ev(4, 2));
        assert_eq!(
            classify_discrimination(&spreads, 5),
            Discrimination::Sparse {
                events: 50,
                varied: 1
            }
        );
    }

    /// Variation at population scale clears it.
    #[test]
    fn variation_across_enough_events_discriminates() {
        let mut spreads: Vec<EventSpread> = (0..10).map(|_| ev(4, 3)).collect();
        spreads.extend((0..10).map(|_| ev(4, 1)));
        assert_eq!(
            classify_discrimination(&spreads, 5),
            Discrimination::Discriminates {
                events: 20,
                varied: 10
            }
        );
    }

    /// `Sparse` and `Underpowered` are different findings and must not merge.
    ///
    /// Few events with some variation is *no reading*. Many events with almost
    /// none is *evidence of uniformity*. Collapsing them would either excuse a
    /// uniform metric as unmeasured or condemn a small sample as broken.
    #[test]
    fn sparse_is_not_underpowered() {
        // 3 events, 1 varied, min 5 → nothing can be said.
        assert_eq!(
            classify_discrimination(&[ev(2, 2), ev(2, 1), ev(2, 1)], 5),
            Discrimination::Underpowered { events: 3, need: 5 }
        );
        // 50 events, 1 varied, min 5 → something can be said, and it is bad.
        let mut many: Vec<EventSpread> = (0..49).map(|_| ev(2, 1)).collect();
        many.push(ev(2, 2));
        assert!(matches!(
            classify_discrimination(&many, 5),
            Discrimination::Sparse { .. }
        ));
    }

    /// Uniformity needs a population before it is a finding.
    #[test]
    fn too_few_shared_events_is_underpowered_not_uniform() {
        assert_eq!(
            classify_discrimination(&[ev(2, 1)], 5),
            Discrimination::Underpowered { events: 1, need: 5 }
        );
    }

    /// Single-subject events carry no information about discrimination.
    ///
    /// This is the arm that would otherwise report a fleet-wide defect on a
    /// platform where one agent works each forecast: a hundred events, every
    /// one with a single value, and nothing wrong at all.
    #[test]
    fn events_with_one_subject_are_not_evidence_of_uniformity() {
        let spreads: Vec<EventSpread> = (0..100).map(|_| ev(1, 1)).collect();
        assert_eq!(
            classify_discrimination(&spreads, 5),
            Discrimination::NoSharedEvents
        );
    }

    /// An empty set is `NoSharedEvents`, never a pass.
    #[test]
    fn nothing_measured_is_not_a_pass() {
        assert_eq!(
            classify_discrimination(&[], 5),
            Discrimination::NoSharedEvents
        );
        assert!(!matches!(
            classify_discrimination(&[], 5),
            Discrimination::Discriminates { .. }
        ));
    }

    /// An unavailable reading is neither a finding nor a pass.
    ///
    /// The two predicates must disagree about `Underpowered` and
    /// `NoSharedEvents` — `is_finding` says no, `is_reading` says no. A single
    /// boolean here would have to pick one, and either choice turns "we could
    /// not look" into a verdict.
    #[test]
    fn an_unavailable_reading_is_neither_a_finding_nor_a_pass() {
        for v in [
            Discrimination::Underpowered { events: 1, need: 5 },
            Discrimination::NoSharedEvents,
        ] {
            assert!(!v.is_finding(), "{v:?} reported as a finding");
            assert!(!v.is_reading(), "{v:?} reported as a reading");
        }
        assert!(Discrimination::Uniform { events: 9 }.is_finding());
        assert!(Discrimination::Sparse {
            events: 9,
            varied: 1
        }
        .is_finding());
        assert!(!Discrimination::Discriminates {
            events: 9,
            varied: 9
        }
        .is_finding());
        assert!(Discrimination::Discriminates {
            events: 9,
            varied: 9
        }
        .is_reading());
    }

    /// Two producers of one metric is a finding until someone declares it.
    #[test]
    fn two_undeclared_producers_of_one_metric_is_a_finding() {
        assert_eq!(
            classify_producers(2, false),
            Producers::Conflated { producers: 2 }
        );
        assert_eq!(classify_producers(1, false), Producers::Single);
        assert_eq!(classify_producers(0, false), Producers::None);
        // Declared, with a reason on file: reported as one.
        assert_eq!(classify_producers(2, true), Producers::Single);
    }

    /// Reach is asserted only where it is unambiguous.
    #[test]
    fn reach_is_open_at_zero_and_narrow_below_the_floor() {
        // No producer receives: the loop returns to nothing.
        assert_eq!(classify_reach(84, 0, 8), Reach::Open { producing: 84 });
        // The measured state: 7 of 84 is 8%, which is the declared floor.
        assert_eq!(
            classify_reach(84, 7, 8),
            Reach::Closes {
                producing: 84,
                receiving: 7
            }
        );
        // One fewer and it has fallen below what was measured.
        assert_eq!(
            classify_reach(84, 6, 8),
            Reach::Narrow {
                producing: 84,
                receiving: 6
            }
        );
        // Nothing produced is not a pass.
        assert_eq!(classify_reach(0, 0, 8), Reach::NoProducers);
        assert!(!matches!(classify_reach(0, 0, 8), Reach::Closes { .. }));
    }

    /// The floor is a measurement, so it must match one.
    ///
    /// 7 receiving of 84 producing is 8%, and the contract declares 8. If a
    /// later session raises the floor without the ratchet having demanded it,
    /// this is where the invented target shows up.
    #[test]
    fn the_reach_floor_is_integer_percent_of_the_measurement_it_came_from() {
        assert_eq!(reach_pct(84, 7), 8);
        assert_eq!(
            reach_pct(0, 0),
            0,
            "no producers must not read as full reach"
        );
        assert_eq!(reach_pct(84, 84), 100);
        for c in OUTCOME_CONTRACTS {
            if let Some((_, _, floor)) = c.reach {
                assert!(
                    floor > 0 && floor <= 100,
                    "{}.{}: a reach floor of {floor}% is not a percentage",
                    c.loop_id,
                    c.stage
                );
            }
        }
    }

    /// A baseline entry must say what would remove it.
    #[test]
    fn every_known_gap_states_what_would_clear_it() {
        let mut seen = std::collections::HashSet::new();
        for g in KNOWN_GAPS {
            assert!(
                seen.insert((g.metric, g.gap)),
                "{}.{} is declared twice",
                g.metric,
                g.gap
            );
            assert!(
                matches!(g.gap, "uniform" | "conflated"),
                "{}: `{}` is not a gap this module can report",
                g.metric,
                g.gap
            );
            assert!(g.why.len() > 80, "{}.{}: say what it is", g.metric, g.gap);
            assert!(
                g.cleared_by.len() > 80,
                "{}.{}: a baseline with no exit condition is a standing \
                 permission nobody re-examines",
                g.metric,
                g.gap
            );
            assert!(
                contract_for(
                    g.metric.split('.').next().unwrap_or(""),
                    g.metric.split('.').nth(1).unwrap_or("")
                )
                .is_some(),
                "{} names no declared contract, so this entry excuses nothing",
                g.metric
            );
        }
    }

    /// The escape valve is empty, and that is a result.
    #[test]
    fn the_shared_metric_list_is_empty_and_every_entry_would_give_a_reason() {
        for (k, why) in SHARED_METRICS {
            assert!(
                why.len() > 80,
                "{k}: an exemption without a reason is a permanent one"
            );
        }
        assert!(
            shared_metric("forecast_calibration").is_none(),
            "`forecast_calibration`'s two producers compute over different \
             denominators — one per forecast, one per aggregate of N. That is \
             the finding, and excusing it would hide the only thing this check \
             has found."
        );
    }

    /// Every contract must say what it does not show.
    #[test]
    fn every_contract_states_its_own_limits() {
        for c in OUTCOME_CONTRACTS {
            assert!(
                c.does_not_show.len() > 120,
                "{}.{}: a contract that does not say what it fails to \
                 establish will be read as establishing the claim",
                c.loop_id,
                c.stage
            );
            assert!(
                c.proposition != c.claim,
                "{}.{}: the proposition IS the claim, which is the \
                 over-reading this field exists to prevent",
                c.loop_id,
                c.stage
            );
            assert!(
                c.why.len() > 80,
                "{}.{}: say what it costs",
                c.loop_id,
                c.stage
            );
            assert!(
                c.min_events > 1,
                "{}.{}: one event is not a population",
                c.loop_id,
                c.stage
            );
        }
    }

    /// Read-only, same guard as every other contract module.
    #[test]
    fn every_query_is_read_only() {
        for c in OUTCOME_CONTRACTS {
            for (label, sql) in [("spread", c.spread_sql), ("producer", c.producer_sql)] {
                let q = sql.to_ascii_lowercase();
                assert!(
                    q.trim_start().starts_with("select"),
                    "{}.{} {label}",
                    c.loop_id,
                    c.stage
                );
                for w in ["insert", "update ", "delete", "drop", "alter", "truncate"] {
                    assert!(
                        !q.contains(w),
                        "{}.{} {label} contains `{w}`",
                        c.loop_id,
                        c.stage
                    );
                }
            }
        }
    }

    /// The loop and stage must exist in the model that owns them.
    ///
    /// Same cross-boundary pin as `panel_absence::every_named_loop_stage_exists_in_the_loop_model`.
    /// A contract naming a stage that has been renamed would report on nothing
    /// and read as passing.
    #[test]
    fn every_contract_names_a_real_stage_and_quotes_its_claim() {
        for c in OUTCOME_CONTRACTS {
            let l = crate::loop_model::LOOPS
                .iter()
                .find(|l| l.id == c.loop_id)
                .unwrap_or_else(|| panic!("{} is not a declared loop", c.loop_id));
            assert!(
                l.stages.iter().any(|s| s.id == c.stage),
                "{} has no stage `{}`",
                c.loop_id,
                c.stage
            );
            assert_eq!(
                c.claim, l.claim,
                "{}.{}: the quoted claim has drifted from `loop_model`",
                c.loop_id, c.stage
            );
        }
    }
}
