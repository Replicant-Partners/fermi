//! Loop 3 Stage 0 — prospective conflict detection over declared intentions.
//!
//! The detection logic lives here, separate from the tool dispatch that loads
//! rows and writes signals, so the part with judgment in it is unit-testable
//! without a database.
//!
//! ## What is detected, and what is honestly not
//!
//! The `intention_coordinator` card names four conflict classes. Three are
//! decidable from the declarations alone; one is not, and this module says so
//! rather than pretending:
//!
//! | Class | Decidable? | How |
//! |---|---|---|
//! | Resource conflict | **Yes, certainly** | Two active intentions naming the same target. No semantics needed |
//! | Dependency | **Yes, certainly** | `depends_on` naming something no completed intention produced |
//! | Duplication | **Yes, probabilistically** | Cosine similarity between descriptions above a threshold |
//! | Contradiction | **No** | "A plans to assert X while B plans to assert not-X" needs to understand X. Reported as a duplication candidate for the caller to judge; never asserted here |
//!
//! Claiming to detect contradiction with a similarity score would be the same
//! error as the platform's older habit of reporting a number whose wiring it had
//! not checked. High similarity means *these two are about the same thing*,
//! which is the input to the judgment, not the judgment.
//!
//! ## Whose plan is being compared
//!
//! Every class above presumes the two rows are two *agents'* plans. Until
//! mig-218 that presumption was unchecked: the only caller of
//! `declare_intention` was the coordination strategist declaring on members'
//! behalf from a transcript, so a whole map could be one agent's guesswork and
//! read identically to a map every member had filled in itself.
//!
//! [`IntentionSource`] restores the distinction ReMALIS (arXiv:2407.12532 §3.1)
//! draws between an agent's private intention `I_j` and another party's belief
//! `b_i(I_j | m_ij)` about it, and [`detect_conflicts`] now acts on it: see
//! [`IntentionSource::is_first_hand`].

use serde::{Deserialize, Serialize};

/// Cosine similarity at or above which two intentions are treated as covering
/// the same work.
///
/// 0.82 rather than a rounder number: `MIN_SIMILARITY` for KG retrieval is
/// 0.30, but that gate answers "is this worth putting in a prompt", where a
/// false positive costs some tokens. Here a false positive tells two agents to
/// stop and differentiate, which costs a turn and can suppress legitimately
/// parallel work. The threshold is deliberately high enough that it fires on
/// paraphrase rather than on topic adjacency.
pub const DUPLICATION_THRESHOLD: f32 = 0.82;

/// Where a row in the intention map came from.
///
/// The difference between the first two and [`Inferred`](Self::Inferred) is the
/// difference between an intention and a belief about one. Both are worth
/// having; conflating them makes the coordinator's own guesses look like
/// corroborating reports from the team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentionSource {
    /// The agent declared its own next action.
    SelfDeclared,
    /// The platform asked the agent for its plan and recorded the reply. Still
    /// the agent's own words — `solicit_agent_plan` performed the round trip
    /// and the platform can vouch for it.
    Solicited,
    /// A third party wrote this from observation. In practice, the coordination
    /// strategist reading the transcript.
    Inferred,
    /// Written before mig-218, so its author is unrecorded. Not guessed at.
    Unattributed,
}

impl IntentionSource {
    /// Did this come from the agent it is about?
    ///
    /// The question conflict detection needs. `Unattributed` answers *no*, not
    /// because those rows are known to be second-hand but because the honest
    /// reading of an unrecorded author is that we cannot claim first-hand.
    pub fn is_first_hand(&self) -> bool {
        matches!(self, Self::SelfDeclared | Self::Solicited)
    }

    /// The database spelling.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SelfDeclared => "self",
            Self::Solicited => "solicited",
            Self::Inferred => "inferred",
            Self::Unattributed => "unattributed",
        }
    }

    /// Parse the database spelling. Anything unrecognised reads as
    /// `Unattributed` rather than panicking or defaulting to a stronger claim.
    pub fn from_db(s: &str) -> Self {
        match s {
            "self" => Self::SelfDeclared,
            "solicited" => Self::Solicited,
            "inferred" => Self::Inferred,
            _ => Self::Unattributed,
        }
    }
}

/// One agent's declared next action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intention {
    pub intention_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub action_type: String,
    pub tool: Option<String>,
    pub description: String,
    pub targets: Vec<String>,
    pub depends_on: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    /// Whether this is the agent's own plan or somebody's reading of it.
    pub source: IntentionSource,
    /// The agent that wrote the row, when recorded.
    pub declared_by: Option<String>,
}

/// What the coordinator found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Conflict {
    /// Two agents will write or consume the same resource.
    Resource {
        agent_a: String,
        agent_b: String,
        target: String,
    },
    /// An agent's plan needs an output nothing has produced.
    Dependency { agent: String, needs: String },
    /// Two agents appear to be doing the same work. `similarity` is evidence,
    /// not a verdict — see the module docs on contradiction.
    Duplication {
        agent_a: String,
        agent_b: String,
        similarity: f32,
    },
}

impl Conflict {
    /// The signal name the card's Stage 0 protocol expects.
    pub fn signal(&self) -> &'static str {
        match self {
            Conflict::Resource { .. } => "RESOURCE_CONFLICT",
            Conflict::Dependency { .. } => "DEPENDENCY_WAIT",
            Conflict::Duplication { .. } => "OVERLAP_WARNING",
        }
    }
}

/// Detect conflicts across the active intention map.
///
/// `produced` is the set of output names already completed in this workspace,
/// used to decide whether a `depends_on` entry is satisfied.
///
/// When `only_agent` is `Some`, results are filtered to conflicts involving
/// that agent — the shape `check_conflicts(agent_id)` wants.
pub fn detect_conflicts(
    intentions: &[Intention],
    produced: &[String],
    only_agent: Option<&str>,
) -> Vec<Conflict> {
    let mut out = Vec::new();

    // ── Resource: same target named by two active intentions ──
    for (i, a) in intentions.iter().enumerate() {
        for b in intentions.iter().skip(i + 1) {
            for t in &a.targets {
                if b.targets.iter().any(|bt| bt == t) {
                    out.push(Conflict::Resource {
                        agent_a: a.agent_name.clone(),
                        agent_b: b.agent_name.clone(),
                        target: t.clone(),
                    });
                }
            }
        }
    }

    // ── Dependency: needs something nobody has produced ──
    //
    // An intention whose dependency is named by another *active* intention is
    // not a conflict but a sequencing fact, and it is still worth surfacing:
    // the dependent agent should wait. An intention whose dependency nothing
    // has produced or plans to produce is the more serious case. Both are
    // reported as DEPENDENCY_WAIT because the action for the agent is the same.
    for a in intentions {
        for need in &a.depends_on {
            if !produced.iter().any(|p| p == need) {
                out.push(Conflict::Dependency {
                    agent: a.agent_name.clone(),
                    needs: need.clone(),
                });
            }
        }
    }

    // ── Duplication: semantically equivalent descriptions ──
    for (i, a) in intentions.iter().enumerate() {
        for b in intentions.iter().skip(i + 1) {
            // Neither side first-hand: this pair is the coordinator agreeing
            // with itself, and reporting it as an overlap is the defect
            // mig-218 exists to end.
            //
            // Both rows were written by one caller, from one transcript, in
            // one turn. Two paraphrases of the same observed activity are
            // exactly what a 0.82 cosine threshold is tuned to catch, so this
            // pass fires *most reliably* in the case where it means least —
            // and `suggest_differentiation` then asks two agents to split work
            // neither of them ever said they were doing.
            //
            // Resource and dependency conflicts are NOT suppressed the same
            // way: those are decidable from named targets and outputs, so an
            // inferred row still carries a checkable claim about a file. Only
            // duplication rests entirely on the two descriptions being two
            // independent reports, which here they are not.
            if !a.source.is_first_hand() && !b.source.is_first_hand() {
                continue;
            }
            let sim = match (&a.embedding, &b.embedding) {
                (Some(ea), Some(eb)) => cosine(ea, eb),
                // No embedding on one side: fall back to the certain signal
                // only. Same tool AND same non-empty target is already covered
                // by the resource pass, so the honest degraded behaviour is to
                // report nothing rather than to guess from string overlap.
                _ => continue,
            };
            if sim >= DUPLICATION_THRESHOLD {
                out.push(Conflict::Duplication {
                    agent_a: a.agent_name.clone(),
                    agent_b: b.agent_name.clone(),
                    similarity: sim,
                });
            }
        }
    }

    if let Some(who) = only_agent {
        out.retain(|c| match c {
            Conflict::Resource {
                agent_a, agent_b, ..
            }
            | Conflict::Duplication {
                agent_a, agent_b, ..
            } => agent_a == who || agent_b == who,
            Conflict::Dependency { agent, .. } => agent == who,
        });
    }
    out
}

/// The single signal the card's Stage 0 emits for a whole check.
///
/// `CLEAR` when nothing was found. Otherwise the most serious class present,
/// ordered resource > dependency > duplication: a resource conflict will
/// corrupt work, an unmet dependency will waste it, and an overlap merely
/// duplicates it.
pub fn overall_signal(conflicts: &[Conflict]) -> &'static str {
    if conflicts
        .iter()
        .any(|c| matches!(c, Conflict::Resource { .. }))
    {
        "CONFLICT_ALERT"
    } else if conflicts
        .iter()
        .any(|c| matches!(c, Conflict::Dependency { .. }))
    {
        "DEPENDENCY_WAIT"
    } else if conflicts
        .iter()
        .any(|c| matches!(c, Conflict::Duplication { .. }))
    {
        "OVERLAP_WARNING"
    } else {
        "CLEAR"
    }
}

/// How much of an intention map is the team's own account of itself.
///
/// Reported alongside every conflict check so a reader can tell a coordinated
/// workspace from one where the coordinator is talking to itself. A map that is
/// entirely `inferred` is not a coordination failure — it is a map nobody was
/// asked to confirm, and the remedy is `solicit_agent_plan`, not a differently
/// worded brief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grounding {
    pub self_declared: usize,
    pub solicited: usize,
    pub inferred: usize,
    pub unattributed: usize,
}

impl Grounding {
    pub fn of(intentions: &[Intention]) -> Self {
        let mut g = Grounding {
            self_declared: 0,
            solicited: 0,
            inferred: 0,
            unattributed: 0,
        };
        for i in intentions {
            match i.source {
                IntentionSource::SelfDeclared => g.self_declared += 1,
                IntentionSource::Solicited => g.solicited += 1,
                IntentionSource::Inferred => g.inferred += 1,
                IntentionSource::Unattributed => g.unattributed += 1,
            }
        }
        g
    }

    pub fn total(&self) -> usize {
        self.self_declared + self.solicited + self.inferred + self.unattributed
    }

    pub fn first_hand(&self) -> usize {
        self.self_declared + self.solicited
    }

    /// A one-line reading, so the strategist is told what its map is worth
    /// rather than left to infer it from four counts.
    pub fn reading(&self) -> &'static str {
        match (self.total(), self.first_hand()) {
            (0, _) => "EMPTY — no agent has an active intention.",
            (t, f) if f == t => "GROUNDED — every intention is the agent's own.",
            (_, 0) => {
                "UNGROUNDED — no member has stated a plan; this map is entirely \
                 second-hand. Overlap detection between second-hand rows is \
                 suppressed because it would only measure your own paraphrasing. \
                 Call solicit_agent_plan before treating this map as coordination."
            }
            _ => {
                "PARTIAL — some intentions are first-hand and some are inferred. \
                 Conflicts involving an inferred row are claims about what you \
                 believe an agent will do, not what it said."
            }
        }
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// First-hand by default: the tests below are about conflict semantics,
    /// and the provenance-specific cases opt into `Inferred` explicitly.
    fn intent(name: &str, desc: &str) -> Intention {
        Intention {
            intention_id: format!("i-{name}"),
            agent_id: format!("id-{name}"),
            agent_name: name.to_string(),
            action_type: "research".into(),
            tool: None,
            description: desc.into(),
            targets: vec![],
            depends_on: vec![],
            embedding: None,
            source: IntentionSource::SelfDeclared,
            declared_by: Some(format!("id-{name}")),
        }
    }

    #[test]
    fn same_target_is_a_resource_conflict() {
        let mut a = intent("alice", "write the brief");
        let mut b = intent("bob", "also write the brief");
        a.targets = vec!["report.md".into()];
        b.targets = vec!["report.md".into()];

        let c = detect_conflicts(&[a, b], &[], None);
        assert_eq!(
            c,
            vec![Conflict::Resource {
                agent_a: "alice".into(),
                agent_b: "bob".into(),
                target: "report.md".into()
            }]
        );
        assert_eq!(overall_signal(&c), "CONFLICT_ALERT");
    }

    #[test]
    fn unmet_dependency_is_reported() {
        let mut a = intent("alice", "synthesise the findings");
        a.depends_on = vec!["cpi_analysis".into()];

        let c = detect_conflicts(&[a.clone()], &[], None);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].signal(), "DEPENDENCY_WAIT");

        // Satisfied once produced.
        let c2 = detect_conflicts(&[a], &["cpi_analysis".to_string()], None);
        assert!(c2.is_empty());
        assert_eq!(overall_signal(&c2), "CLEAR");
    }

    /// The case that motivates the embedding column: same work, different
    /// words. String comparison sees nothing here.
    #[test]
    fn paraphrased_duplication_is_caught_by_similarity() {
        let mut a = intent("alice", "research UK CPI trend");
        let mut b = intent("bob", "investigate British inflation data");
        // Near-identical vectors stand in for the embedder.
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        b.embedding = Some(vec![0.99, 0.01, 0.0]);

        let c = detect_conflicts(&[a, b], &[], None);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].signal(), "OVERLAP_WARNING");
        assert_eq!(overall_signal(&c), "OVERLAP_WARNING");
    }

    /// Adjacent-but-different work must not be flagged. Telling two agents to
    /// differentiate when they are already differentiated costs a turn and
    /// suppresses legitimate parallel work.
    #[test]
    fn merely_related_work_is_not_duplication() {
        let mut a = intent("alice", "research UK CPI");
        let mut b = intent("bob", "research US unemployment");
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        b.embedding = Some(vec![0.0, 1.0, 0.0]);

        assert!(detect_conflicts(&[a, b], &[], None).is_empty());
    }

    /// A missing embedding must degrade to certain signals only, never to a
    /// guess. Prospective coordination staying online through an embedder
    /// outage matters more than catching every overlap.
    #[test]
    fn missing_embedding_degrades_without_guessing() {
        let a = intent("alice", "research UK CPI trend");
        let b = intent("bob", "research UK CPI trend"); // identical text, no vectors
        let c = detect_conflicts(&[a, b], &[], None);
        assert!(
            c.is_empty(),
            "without embeddings the checker must not infer duplication, got {c:?}"
        );
    }

    #[test]
    fn filtering_by_agent_keeps_only_their_conflicts() {
        let mut a = intent("alice", "x");
        let mut b = intent("bob", "y");
        let mut c = intent("carol", "z");
        a.targets = vec!["shared.md".into()];
        b.targets = vec!["shared.md".into()];
        c.depends_on = vec!["nothing".into()];

        let all = detect_conflicts(&[a.clone(), b.clone(), c.clone()], &[], None);
        assert_eq!(all.len(), 2);

        let only_carol = detect_conflicts(&[a, b, c], &[], Some("carol"));
        assert_eq!(only_carol.len(), 1);
        assert!(matches!(only_carol[0], Conflict::Dependency { .. }));
    }

    /// Severity ordering: a resource conflict corrupts work, an unmet
    /// dependency wastes it, an overlap merely duplicates it.
    #[test]
    fn overall_signal_reports_the_most_serious_class() {
        let mut a = intent("alice", "x");
        let mut b = intent("bob", "y");
        a.targets = vec!["f".into()];
        b.targets = vec!["f".into()];
        a.depends_on = vec!["missing".into()];

        let c = detect_conflicts(&[a, b], &[], None);
        assert!(c.len() >= 2);
        assert_eq!(overall_signal(&c), "CONFLICT_ALERT");
    }

    #[test]
    fn an_empty_map_is_clear() {
        assert_eq!(overall_signal(&detect_conflicts(&[], &[], None)), "CLEAR");
    }

    /// Mismatched embedding dimensions must not panic or spuriously match.
    #[test]
    fn dimension_mismatch_is_not_a_match() {
        let mut a = intent("alice", "x");
        let mut b = intent("bob", "y");
        a.embedding = Some(vec![1.0, 0.0]);
        b.embedding = Some(vec![1.0, 0.0, 0.0]);
        assert!(detect_conflicts(&[a, b], &[], None).is_empty());
    }

    // ── Provenance (mig-218) ───────────────────────────────────────────

    /// The defect this exists to end.
    ///
    /// Both rows were written by the strategist from one transcript. A high
    /// cosine between them measures the strategist's paraphrasing, not two
    /// agents converging on the same work, and acting on it sends two agents
    /// off to differentiate work neither of them claimed.
    #[test]
    fn two_inferred_intentions_do_not_overlap_with_each_other() {
        let mut a = intent("alice", "research UK CPI trend");
        let mut b = intent("bob", "investigate British inflation data");
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        b.embedding = Some(vec![0.99, 0.01, 0.0]);
        a.source = IntentionSource::Inferred;
        b.source = IntentionSource::Inferred;
        a.declared_by = Some("id-strategist".into());
        b.declared_by = Some("id-strategist".into());

        let c = detect_conflicts(&[a, b], &[], None);
        assert!(
            c.is_empty(),
            "an overlap between two second-hand rows is the coordinator agreeing \
             with itself, got {c:?}"
        );
    }

    /// One first-hand side is enough. An agent that said what it is doing, set
    /// against the coordinator's reading of a second agent, is a real claim
    /// worth checking — half the pair is grounded and the warning names
    /// something the team can confirm or deny.
    #[test]
    fn one_first_hand_side_still_produces_an_overlap_warning() {
        let mut a = intent("alice", "research UK CPI trend");
        let mut b = intent("bob", "investigate British inflation data");
        a.embedding = Some(vec![1.0, 0.0, 0.0]);
        b.embedding = Some(vec![0.99, 0.01, 0.0]);
        a.source = IntentionSource::Solicited;
        b.source = IntentionSource::Inferred;

        let c = detect_conflicts(&[a, b], &[], None);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].signal(), "OVERLAP_WARNING");
    }

    /// Resource conflicts survive the suppression, and must.
    ///
    /// A named target is a checkable claim about a file. Even when the
    /// coordinator inferred both rows, "these two will both write report.md"
    /// is either true or false about the workspace — unlike a cosine between
    /// two of its own sentences, which is only ever true about its prose.
    #[test]
    fn suppression_is_scoped_to_duplication_only() {
        let mut a = intent("alice", "write the brief");
        let mut b = intent("bob", "also write the brief");
        a.targets = vec!["report.md".into()];
        b.targets = vec!["report.md".into()];
        a.depends_on = vec!["cpi_analysis".into()];
        a.source = IntentionSource::Inferred;
        b.source = IntentionSource::Inferred;

        let c = detect_conflicts(&[a, b], &[], None);
        assert!(c.iter().any(|x| matches!(x, Conflict::Resource { .. })));
        assert!(c.iter().any(|x| matches!(x, Conflict::Dependency { .. })));
        assert_eq!(overall_signal(&c), "CONFLICT_ALERT");
    }

    /// An unrecorded author must not be read as a first-hand statement.
    ///
    /// Rows predating mig-218 are almost certainly the strategist's, but
    /// `unattributed` is what is known. Treating it as `self` would silently
    /// re-enable the behaviour on exactly the historical rows this change is
    /// about.
    #[test]
    fn unattributed_is_not_first_hand() {
        assert!(!IntentionSource::Unattributed.is_first_hand());
        assert!(!IntentionSource::Inferred.is_first_hand());
        assert!(IntentionSource::SelfDeclared.is_first_hand());
        assert!(IntentionSource::Solicited.is_first_hand());
        assert_eq!(
            IntentionSource::from_db("nonsense"),
            IntentionSource::Unattributed
        );
        for s in [
            IntentionSource::SelfDeclared,
            IntentionSource::Solicited,
            IntentionSource::Inferred,
            IntentionSource::Unattributed,
        ] {
            assert_eq!(IntentionSource::from_db(s.as_str()), s, "round trip {s:?}");
        }
    }

    /// A map nobody confirmed reports itself as such.
    ///
    /// This is the reading the strategist most needs and the one it could
    /// never get: "CLEAR" over an all-inferred map used to be indistinguishable
    /// from "CLEAR" over a map every member had filled in.
    #[test]
    fn an_all_inferred_map_reports_itself_ungrounded() {
        let mut a = intent("alice", "x");
        let mut b = intent("bob", "y");
        a.source = IntentionSource::Inferred;
        b.source = IntentionSource::Inferred;

        let g = Grounding::of(&[a.clone(), b.clone()]);
        assert_eq!(g.total(), 2);
        assert_eq!(g.first_hand(), 0);
        assert!(g.reading().starts_with("UNGROUNDED"));

        let mut c = intent("carol", "z");
        c.source = IntentionSource::Solicited;
        assert!(Grounding::of(&[a, b, c]).reading().starts_with("PARTIAL"));

        let d = intent("dave", "w"); // SelfDeclared
        assert!(Grounding::of(&[d]).reading().starts_with("GROUNDED"));
        assert!(Grounding::of(&[]).reading().starts_with("EMPTY"));
    }
}
