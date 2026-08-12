//! Pipeline planning — validate a declared `workflow_template` before running it.
//!
//! SPEC_31 P2. 36 stages are declared across 10 curated cards, each carrying
//! its own `accepts`/`produces`, and until now nothing executed them. This is
//! the planning half: work out whether a pipeline *can* run, and what it would
//! do, without running anything.
//!
//! # Why planning is separate from execution
//!
//! A pipeline that breaks at stage 4 has already spent the budget for stages
//! 1–3. Every seam is checkable up front from the declarations alone, so the
//! expensive discovery is avoidable. `pipeline_strategist`'s card states this
//! as its first rule; this module is that rule in code.
//!
//! Separating the two also makes the interesting logic testable without a
//! database, a network, or an LLM.
//!
//! # What a "match" means, and what it does not
//!
//! A seam matches when the upstream stage's `produces` contains a label the
//! downstream stage `accepts`. That is a **string comparison between two
//! declarations**, not a verified guarantee: `produces` is free text across
//! the corpus (267 distinct values, 234 of them appearing once), and only
//! three cards declare a typed `output_contract`.
//!
//! So `SeamStatus::Matched` carries how it was established, and the API says
//! `matched_by_label`. Presenting an asserted match as a verified one is the
//! error this whole audit line has been chasing: a check whose failure is
//! indistinguishable from success.

use crate::agent_backend::agent_card::WorkflowTemplate;
use serde::{Deserialize, Serialize};

/// How a seam between two adjacent stages resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SeamStatus {
    /// Upstream declares something downstream accepts. By label only — see
    /// the module docs. `on` names the labels that lined up.
    MatchedByLabel { on: Vec<String> },
    /// Downstream needs inputs nothing upstream produces. `missing` names
    /// them. A pipeline with an unmatched seam should not be started.
    Unmatched { missing: Vec<String> },
    /// The downstream stage declares no inputs, so nothing can fail to
    /// arrive. Distinguished from a match because there is no evidence of
    /// compatibility either way.
    NothingRequired,
    /// One side of the seam has no agent bound. Not an error: an open slot is
    /// a declared vacancy, and naming its contract is how it gets filled.
    OpenSlot { stage: String },
}

/// A seam between two adjacent stages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seam {
    pub upstream: String,
    pub downstream: String,
    #[serde(flatten)]
    pub status: SeamStatus,
}

/// A stage with an unfilled `agent`, reported as a typed vacancy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenSlot {
    pub index: usize,
    pub stage: String,
    /// What an agent filling this slot must accept…
    pub accepts: Vec<String>,
    /// …and what it must produce for the next stage to work.
    pub produces: Vec<String>,
    pub description: Option<String>,
}

/// The result of planning a pipeline: whether it can run, and why not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelinePlan {
    pub stage_count: usize,
    pub seams: Vec<Seam>,
    pub open_slots: Vec<OpenSlot>,
    /// Stage names in execution order, bound agents only.
    pub runnable_stages: Vec<String>,
    /// True when every seam is satisfied and every stage has an agent.
    pub runnable: bool,
    /// Why not, in one line, when `runnable` is false.
    pub blocked_reason: Option<String>,
}

/// Plan a declared pipeline. Pure: reads declarations, touches nothing.
pub fn plan(template: &WorkflowTemplate) -> PipelinePlan {
    let stages = &template.stages;

    if stages.is_empty() {
        return PipelinePlan {
            stage_count: 0,
            seams: vec![],
            open_slots: vec![],
            runnable_stages: vec![],
            runnable: false,
            blocked_reason: Some("the template declares no stages".into()),
        };
    }

    let open_slots: Vec<OpenSlot> = stages
        .iter()
        .enumerate()
        .filter(|(_, s)| s.agent.is_none())
        .map(|(i, s)| OpenSlot {
            index: i,
            stage: s.name.clone(),
            accepts: s.accepts.clone(),
            produces: s.produces.clone(),
            description: s.description.clone(),
        })
        .collect();

    // Availability starts with the pipeline's EXTERNAL inputs, then
    // accumulates what each stage produces.
    //
    // The first stage's `accepts` are supplied by the caller — they are the
    // pipeline's entry contract, not something an upstream stage makes. Later
    // stages routinely need them again: ar_card_producer's Intake takes a
    // `logo` and its Marker Generation stage takes that same `logo` alongside
    // the brief Intake produced.
    //
    // Omitting this was the planner's first bug, and running it over the real
    // corpus is what caught it: 9 of 12 declared pipelines reported unmatched
    // seams that were nothing of the kind. A validator that fails on correct
    // input is worse than none — it teaches people to ignore it.
    let mut available: Vec<String> = stages[0].accepts.clone();
    let mut seams: Vec<Seam> = Vec::new();

    for (i, stage) in stages.iter().enumerate() {
        if i > 0 {
            let upstream = &stages[i - 1];
            let status = if upstream.agent.is_none() {
                SeamStatus::OpenSlot {
                    stage: upstream.name.clone(),
                }
            } else if stage.agent.is_none() {
                SeamStatus::OpenSlot {
                    stage: stage.name.clone(),
                }
            } else if stage.accepts.is_empty() {
                SeamStatus::NothingRequired
            } else {
                let matched: Vec<String> = stage
                    .accepts
                    .iter()
                    .filter(|a| available.iter().any(|p| p == *a))
                    .cloned()
                    .collect();
                let missing: Vec<String> = stage
                    .accepts
                    .iter()
                    .filter(|a| !available.iter().any(|p| p == *a))
                    .cloned()
                    .collect();
                if missing.is_empty() {
                    SeamStatus::MatchedByLabel { on: matched }
                } else {
                    SeamStatus::Unmatched { missing }
                }
            };
            seams.push(Seam {
                upstream: upstream.name.clone(),
                downstream: stage.name.clone(),
                status,
            });
        }
        for p in &stage.produces {
            if !available.contains(p) {
                available.push(p.clone());
            }
        }
    }

    let unmatched: Vec<&Seam> = seams
        .iter()
        .filter(|s| matches!(s.status, SeamStatus::Unmatched { .. }))
        .collect();

    let blocked_reason = if !open_slots.is_empty() {
        Some(format!(
            "{} unfilled stage(s): {}",
            open_slots.len(),
            open_slots
                .iter()
                .map(|s| s.stage.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else if !unmatched.is_empty() {
        Some(format!(
            "{} unmatched seam(s): {}",
            unmatched.len(),
            unmatched
                .iter()
                .map(|s| format!("{} → {}", s.upstream, s.downstream))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else {
        None
    };

    PipelinePlan {
        stage_count: stages.len(),
        seams,
        open_slots,
        runnable_stages: stages
            .iter()
            .filter(|s| s.agent.is_some())
            .map(|s| s.name.clone())
            .collect(),
        runnable: blocked_reason.is_none(),
        blocked_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_backend::agent_card::WorkflowStage;

    fn stage(
        name: &str,
        agent: Option<&str>,
        accepts: &[&str],
        produces: &[&str],
    ) -> WorkflowStage {
        WorkflowStage {
            name: name.into(),
            agent: agent.map(String::from),
            accepts: accepts.iter().map(|s| s.to_string()).collect(),
            produces: produces.iter().map(|s| s.to_string()).collect(),
            description: None,
        }
    }

    fn tmpl(stages: Vec<WorkflowStage>) -> WorkflowTemplate {
        WorkflowTemplate {
            mermaid: String::new(),
            stages,
            description: None,
        }
    }

    #[test]
    fn a_well_formed_pipeline_is_runnable() {
        let p = plan(&tmpl(vec![
            stage("Intake", Some("a"), &[], &["brief"]),
            stage("Analyse", Some("b"), &["brief"], &["findings"]),
            stage("Report", Some("c"), &["findings"], &["report"]),
        ]));
        assert!(p.runnable, "{:?}", p.blocked_reason);
        assert_eq!(p.stage_count, 3);
        assert_eq!(p.seams.len(), 2, "n stages produce n-1 seams");
        assert_eq!(p.runnable_stages, vec!["Intake", "Analyse", "Report"]);
    }

    #[test]
    fn an_unmatched_seam_blocks_before_anything_is_spent() {
        let p = plan(&tmpl(vec![
            stage("Intake", Some("a"), &[], &["brief"]),
            stage("Analyse", Some("b"), &["market-data"], &["findings"]),
        ]));
        assert!(!p.runnable);
        assert!(p
            .blocked_reason
            .as_ref()
            .unwrap()
            .contains("Intake → Analyse"));
        assert_eq!(
            p.seams[0].status,
            SeamStatus::Unmatched {
                missing: vec!["market-data".into()]
            },
            "names exactly what is missing, so it is actionable"
        );
    }

    /// A stage may consume an artefact from ANY upstream stage, not just its
    /// immediate predecessor. Requiring adjacency would report a false
    /// unmatched seam on a pipeline that runs correctly.
    #[test]
    fn an_artefact_carries_past_intermediate_stages() {
        let p = plan(&tmpl(vec![
            stage("Intake", Some("a"), &[], &["logo", "brief"]),
            stage("Analyse", Some("b"), &["brief"], &["findings"]),
            stage("Render", Some("c"), &["logo", "findings"], &["image"]),
        ]));
        assert!(p.runnable, "{:?}", p.blocked_reason);
        assert!(matches!(
            p.seams[1].status,
            SeamStatus::MatchedByLabel { .. }
        ));
    }

    /// The first stage's `accepts` are the pipeline's entry contract, supplied
    /// by the caller. Later stages may need them again.
    ///
    /// Regression: omitting this reported 9 of 12 real declared pipelines as
    /// having unmatched seams they did not have. `ar_card_producer` is the
    /// canonical case — Intake takes a `logo`, and Marker Generation takes
    /// that same `logo` alongside the brief Intake produced.
    #[test]
    fn external_inputs_stay_available_to_later_stages() {
        let p = plan(&tmpl(vec![
            stage(
                "Intake",
                Some("a"),
                &["logo", "brand-guidelines"],
                &["structured-brief"],
            ),
            stage(
                "Marker",
                Some("a"),
                &["logo", "structured-brief"],
                &["ar-marker"],
            ),
        ]));
        assert!(
            p.runnable,
            "a caller-supplied input must not read as an unmatched seam: {:?}",
            p.blocked_reason
        );
    }

    /// But an input that is neither external nor produced upstream is still a
    /// real gap. The fix above must not become "assume anything missing was
    /// supplied externally", which would make the validator useless.
    #[test]
    fn a_genuinely_absent_input_is_still_unmatched() {
        let p = plan(&tmpl(vec![
            stage(
                "Brief",
                Some("a"),
                &["creative-brief"],
                &["structured-brief"],
            ),
            stage("Create", Some("b"), &["style-reference"], &["image"]),
        ]));
        assert!(!p.runnable);
        assert_eq!(
            p.seams[0].status,
            SeamStatus::Unmatched {
                missing: vec!["style-reference".into()]
            }
        );
    }

    #[test]
    fn an_open_slot_is_a_typed_vacancy_not_an_error() {
        let p = plan(&tmpl(vec![
            stage("Crux", Some("debate_strategist"), &["claim"], &["crux"]),
            stage("Exchange", None, &["crux"], &["transcript"]),
            stage(
                "Judge",
                Some("debate_strategist"),
                &["transcript"],
                &["verdict"],
            ),
        ]));
        assert!(!p.runnable, "cannot run with a hole in it");
        assert_eq!(p.open_slots.len(), 1);
        let slot = &p.open_slots[0];
        assert_eq!(slot.stage, "Exchange");
        // The contract is the point: this is what an agent must satisfy to
        // fill the vacancy, which is what makes the slot searchable.
        assert_eq!(slot.accepts, vec!["crux"]);
        assert_eq!(slot.produces, vec!["transcript"]);
        assert_eq!(slot.index, 1);
    }

    #[test]
    fn a_stage_requiring_nothing_is_not_reported_as_a_match() {
        // No evidence of compatibility either way — saying "matched" would
        // overclaim.
        let p = plan(&tmpl(vec![
            stage("A", Some("a"), &[], &["x"]),
            stage("B", Some("b"), &[], &["y"]),
        ]));
        assert_eq!(p.seams[0].status, SeamStatus::NothingRequired);
        assert!(p.runnable);
    }

    #[test]
    fn partial_matches_still_block() {
        // Two of three inputs available is not runnable, and the report must
        // name only the genuinely missing one.
        let p = plan(&tmpl(vec![
            stage("A", Some("a"), &[], &["x", "y"]),
            stage("B", Some("b"), &["x", "y", "z"], &["out"]),
        ]));
        assert!(!p.runnable);
        assert_eq!(
            p.seams[0].status,
            SeamStatus::Unmatched {
                missing: vec!["z".into()]
            }
        );
    }

    #[test]
    fn an_empty_template_is_blocked_not_runnable() {
        let p = plan(&tmpl(vec![]));
        assert!(!p.runnable);
        assert_eq!(p.stage_count, 0);
        assert!(p.blocked_reason.unwrap().contains("no stages"));
    }

    #[test]
    fn a_single_stage_pipeline_has_no_seams_and_runs() {
        let p = plan(&tmpl(vec![stage("Only", Some("a"), &["in"], &["out"])]));
        assert!(p.runnable);
        assert!(p.seams.is_empty());
    }

    /// Open slots are reported ahead of unmatched seams. With a hole in the
    /// pipeline the seam analysis around it is not yet meaningful, and
    /// "fill the slot" is the actionable instruction.
    #[test]
    fn open_slots_are_reported_before_seam_problems() {
        let p = plan(&tmpl(vec![
            stage("A", Some("a"), &[], &["x"]),
            stage("B", None, &["x"], &["y"]),
            stage("C", Some("c"), &["nonexistent"], &["z"]),
        ]));
        assert!(!p.runnable);
        assert!(p.blocked_reason.as_ref().unwrap().contains("unfilled"));
    }
}
