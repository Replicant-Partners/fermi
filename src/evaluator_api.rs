//! What the platform concluded about its own machinery, and what those
//! conclusions do not mean.
//!
//! The third instance of [`crate::surface`]'s pattern:
//!
//! | part | owner |
//! |---|---|
//! | declared model | [`crate::native_evaluators::registry`] — id, and the question it asks |
//! | measurement | `Observation` — one snapshot, gathered once |
//! | interpretation | `Verdict` — `Healthy` / `Finding{severity}` / `Inconclusive` |
//! | door | here |
//! | caveat | here |
//!
//! Nothing recomputes a verdict. `native_evaluators` owns them, and
//! `every_evaluator_can_produce_a_finding` — the exemplar the whole
//! falsification registry generalises — already proves each can fire.
//!
//! # Why this needed a surface at all
//!
//! These six evaluators are the only thing on the platform that turns counters
//! into sentences with remedies, and they were reachable through exactly one
//! route: `/api/admin/schema-health`, admin-scoped, alongside the whole
//! platform's diagnostics. The verdicts already carry a `remedy` field. Nobody
//! could see it.
//!
//! # `Inconclusive` is the reading that matters
//!
//! It is **not a pass**, and three of the six are usually in it. The counters
//! most of them read are process-local and reset on restart, so on a freshly
//! booted server "no instrumented write has been attempted since boot" is the
//! honest answer and `Healthy` would be a lie.
//!
//! Mapped onto the same three words the loop and gate surfaces use, so a client
//! branches once:
//!
//! | verdict | reading |
//! |---|---|
//! | `Finding` with `Warning` or `Critical` | `fault` |
//! | `Finding` with `Notice` | `unknown` — reported, never asserted |
//! | `Inconclusive` | `unknown` — nothing was watched |
//! | `Healthy` | `idle` |
//!
//! `Notice` and `Inconclusive` sharing `unknown` is deliberate and is why
//! [`EvaluatorView::token`] exists: they mean different things and neither is a
//! verdict a reader should act on as though it were one.

use crate::native_evaluators::{self, Observation, Severity, Verdict};
use crate::panel_absence::Reading;
use crate::surface::{Caveat, Door};

/// Every human door into an evaluator.
///
/// **Empty, and for a better reason than the gates'.** An evaluator is a pure
/// function over a snapshot: there is nothing to approve, dismiss or override,
/// and a verdict a person could wave away would not be worth computing. What a
/// reader does with a finding is act on the *subject* it names — a sink, a gate,
/// a loop — and those have their own doors.
///
/// Declared empty rather than omitted so that the day someone wants an
/// "acknowledge this finding" button, the argument for it has to be written
/// down. Suppressing a finding is exactly the move §5.2 warns about.
pub const EVALUATOR_DOORS: &[Door] = &[];

/// What each evaluator's verdict does not establish.
///
/// One entry per evaluator whose passing verdict is narrower than it reads,
/// which is most of them.
pub const EVALUATOR_CAVEATS: &[Caveat] = &[
    Caveat {
        subject: "loop_stalled_in_code",
        checked: "No loop's first empty link is a code fault \
                  (`no_trigger`, `writes_refused`, `gate_refuses_everything`).",
        does_not_show: "That the remaining loops are idle rather than broken, \
                        which is what its own `Healthy` detail says. Four of six \
                        are currently stopped with a reason of `unobserved` or \
                        `awaiting_agent` — states `panel_absence` classifies \
                        `unknown` precisely because no contract can say. This is \
                        a known over-claim in the evaluator's own wording, \
                        recorded here rather than silently fixed: narrowing it \
                        flips a live verdict platform-wide and \
                        `each_evaluator_fires_for_its_own_reason` pins the \
                        current shape.",
    },
    Caveat {
        subject: "refused_writes",
        checked: "No instrumented sink has been attempted and refused every \
                  time.",
        does_not_show: "That any write has succeeded, or that any has been \
                        attempted at all. `write_accounting`'s counters are \
                        process-local `AtomicU64`s starting at zero, so on a \
                        fresh process this evaluator returns `Inconclusive` and \
                        must not be read as a pass. `liveness_trust` answers \
                        whether rows exist; this answers only whether attempts \
                        are being refused.",
    },
    Caveat {
        subject: "gate_admitting_everything",
        checked: "Which gates have been asked and have refused nothing.",
        does_not_show: "That those gates are broken. A gate legitimately refuses \
                        nothing when nothing has warranted refusal, and \
                        asserting otherwise would assert that violations must \
                        exist. It is surfaced as a `Notice` — reported, never \
                        asserted — because a control that never fires and one \
                        that is not wired produce identical observations \
                        everywhere else.",
    },
    Caveat {
        subject: "positive_control",
        checked: "At least one liveness contract is passing and at least one \
                  loop is turning.",
        does_not_show: "Anything about the other contracts or loops. It is the \
                        control that distinguishes 'every path is broken' from \
                        'the runner is broken', and a green here means only \
                        that the machinery can produce a pass at all — not that \
                        the platform is well.",
    },
    Caveat {
        subject: "undocumented_silence",
        checked: "No liveness contract is silent without a written reason.",
        does_not_show: "That the sinks are filling. A `Conditional` contract is \
                        excused from being silent by design — its writer fires \
                        only when it detects something — so this passing is \
                        compatible with every conditional sink being empty for \
                        ever.",
    },
];

/// One evaluator's conclusion, as a surface needs it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EvaluatorView {
    pub id: String,
    /// The question it answers, in one line. From the evaluator itself.
    pub asks: String,
    /// `idle` | `fault` | `unknown` — the same three words as the loop and gate
    /// surfaces.
    pub reading: Reading,
    /// `healthy` | `critical` | `warning` | `notice` | `inconclusive`.
    ///
    /// Five tokens over three readings: `notice` and `inconclusive` both read
    /// `unknown` and mean different things.
    pub token: &'static str,
    /// The evaluator's own sentence.
    pub detail: String,
    /// What to do about it, when the verdict carries one.
    pub remedy: Option<&'static str>,
    /// The sinks, gates or loops the finding names, so a reader is sent to a
    /// thing rather than to an investigation.
    pub subjects: Vec<String>,
    /// What this verdict does not establish, when it is narrower than it reads.
    pub caveat: Option<Caveat>,
}

/// The reading and token for one verdict.
pub fn read(v: &Verdict) -> (Reading, &'static str) {
    match v {
        Verdict::Healthy { .. } => (Reading::Idle, "healthy"),
        Verdict::Finding {
            severity: Severity::Critical,
            ..
        } => (Reading::Fault, "critical"),
        Verdict::Finding {
            severity: Severity::Warning,
            ..
        } => (Reading::Fault, "warning"),
        // Reported, never asserted. Not a fault, and emphatically not a pass.
        Verdict::Finding {
            severity: Severity::Notice,
            ..
        } => (Reading::Unknown, "notice"),
        // Nothing could be concluded. The state three of six are usually in.
        Verdict::Inconclusive { .. } => (Reading::Unknown, "inconclusive"),
    }
}

fn caveat_for(id: &str) -> Option<Caveat> {
    EVALUATOR_CAVEATS.iter().find(|c| c.subject == id).copied()
}

/// Assemble the evaluator surface from one snapshot.
///
/// Takes the `Observation` rather than gathering it, so the whole surface
/// describes one instant — the same reason `native_evaluators::run` takes one.
pub fn views(o: &Observation) -> Vec<EvaluatorView> {
    native_evaluators::registry()
        .iter()
        .map(|e| {
            let v = e.evaluate(o);
            let (reading, token) = read(&v);
            let (detail, remedy, subjects) = match &v {
                Verdict::Healthy { detail } => (detail.clone(), None, vec![]),
                Verdict::Finding {
                    detail,
                    remedy,
                    subjects,
                    ..
                } => (detail.clone(), Some(*remedy), subjects.clone()),
                Verdict::Inconclusive { why } => (why.clone(), None, vec![]),
            };
            EvaluatorView {
                id: e.id().to_string(),
                asks: e.asks().to_string(),
                reading,
                token,
                detail,
                remedy,
                subjects,
                caveat: caveat_for(e.id()),
            }
        })
        .collect()
}

/// The header, in four buckets.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct EvaluatorTally {
    pub total: usize,
    pub healthy: usize,
    /// `Warning` or `Critical`. The only bucket that is a finding.
    pub findings: usize,
    /// `Notice`. Reported, never asserted.
    pub notices: usize,
    /// Nothing could be concluded. **Not a pass.**
    pub inconclusive: usize,
}

pub fn tally(views: &[EvaluatorView]) -> EvaluatorTally {
    let mut t = EvaluatorTally {
        total: views.len(),
        healthy: 0,
        findings: 0,
        notices: 0,
        inconclusive: 0,
    };
    for v in views {
        match v.token {
            "healthy" => t.healthy += 1,
            "critical" | "warning" => t.findings += 1,
            "notice" => t.notices += 1,
            "inconclusive" => t.inconclusive += 1,
            _ => {}
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(severity: Severity) -> Verdict {
        Verdict::Finding {
            severity,
            detail: "d".into(),
            subjects: vec!["s".into()],
            remedy: "r",
        }
    }

    /// The five tokens over three readings, and the two that share one.
    #[test]
    fn a_notice_and_an_inconclusive_both_read_unknown_and_are_not_the_same() {
        assert_eq!(
            read(&Verdict::Healthy { detail: "d".into() }),
            (Reading::Idle, "healthy")
        );
        assert_eq!(
            read(&finding(Severity::Critical)),
            (Reading::Fault, "critical")
        );
        assert_eq!(
            read(&finding(Severity::Warning)),
            (Reading::Fault, "warning")
        );
        assert_eq!(
            read(&finding(Severity::Notice)),
            (Reading::Unknown, "notice")
        );
        assert_eq!(
            read(&Verdict::Inconclusive { why: "w".into() }),
            (Reading::Unknown, "inconclusive")
        );

        // Neither is a pass. This is the property a client depends on: three of
        // six evaluators are usually `Inconclusive`, and a surface that renders
        // `unknown` as green would report a healthy platform on every fresh
        // boot.
        for v in [
            finding(Severity::Notice),
            Verdict::Inconclusive { why: "w".into() },
        ] {
            assert_ne!(read(&v).0, Reading::Idle, "{v:?} read as a pass");
            assert_ne!(read(&v).0, Reading::Fault, "{v:?} read as a finding");
        }
    }

    /// A `Notice` is not a finding in the tally either.
    #[test]
    fn the_buckets_partition_the_evaluators_and_a_notice_is_not_a_finding() {
        let vs: Vec<EvaluatorView> = [
            Verdict::Healthy { detail: "d".into() },
            finding(Severity::Critical),
            finding(Severity::Notice),
            Verdict::Inconclusive { why: "w".into() },
        ]
        .iter()
        .map(|v| {
            let (reading, token) = read(v);
            EvaluatorView {
                id: "e".into(),
                asks: "q".into(),
                reading,
                token,
                detail: "d".into(),
                remedy: None,
                subjects: vec![],
                caveat: None,
            }
        })
        .collect();
        let t = tally(&vs);
        assert_eq!(t.total, 4);
        assert_eq!(t.healthy, 1);
        assert_eq!(t.findings, 1, "a Notice must not count as a finding");
        assert_eq!(t.notices, 1);
        assert_eq!(t.inconclusive, 1);
        assert_eq!(
            t.healthy + t.findings + t.notices + t.inconclusive,
            t.total,
            "an evaluator fell through the buckets, so the header omits it"
        );
    }

    /// Every caveat names a real evaluator.
    ///
    /// A caveat on an evaluator that has been renamed qualifies nothing and
    /// looks like coverage — and the one it would silently stop qualifying is
    /// `loop_stalled_in_code`, whose over-claim is the reason this list exists.
    #[test]
    fn every_caveat_names_a_declared_evaluator() {
        let ids: Vec<&str> = native_evaluators::registry()
            .iter()
            .map(|e| e.id())
            .collect();
        for c in EVALUATOR_CAVEATS {
            assert!(
                ids.contains(&c.subject),
                "`{}` is not a declared evaluator, so this caveat qualifies \
                 nothing. Declared: {ids:?}",
                c.subject
            );
        }
        // The one that must never lose its caveat.
        assert!(
            EVALUATOR_CAVEATS
                .iter()
                .any(|c| c.subject == "loop_stalled_in_code"),
            "`loop_stalled_in_code` says `the rest are idle rather than broken` \
             about loops that are classified `unknown`. That over-claim is \
             recorded rather than fixed, and this caveat is the whole of the \
             record."
        );
    }

    /// Every caveat is a caveat, by the shared rules.
    #[test]
    fn every_caveat_satisfies_the_shared_rules() {
        let problems = crate::surface::caveat_problems(EVALUATOR_CAVEATS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
    }

    /// Doors are governed even while there are none.
    #[test]
    fn every_evaluator_door_satisfies_the_shared_rules() {
        let problems = crate::surface::door_problems(EVALUATOR_DOORS);
        assert!(problems.is_empty(), "\n  {}\n", problems.join("\n  "));
    }

    /// An empty snapshot yields no passes.
    ///
    /// The positive-control question turned on itself: over an `Observation`
    /// with nothing in it, every evaluator must decline to conclude rather than
    /// report health. A surface built on this is at its most dangerous
    /// immediately after a deploy, when the counters are cold and the snapshot
    /// is nearly empty.
    #[test]
    fn nothing_observed_produces_no_healthy_verdict() {
        let vs = views(&Observation::default());
        assert!(!vs.is_empty(), "no evaluator ran");
        let healthy: Vec<&str> = vs
            .iter()
            .filter(|v| v.token == "healthy")
            .map(|v| v.id.as_str())
            .collect();
        assert!(
            healthy.is_empty(),
            "over an empty observation these evaluators reported health: \
             {healthy:?}. A cold snapshot must produce `inconclusive`, or the \
             surface reads green on every fresh boot."
        );
    }
}
