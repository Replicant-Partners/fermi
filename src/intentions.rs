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
}
