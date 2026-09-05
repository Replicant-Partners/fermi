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
//! So `SeamStatus` carries how the match was established, and the API says
//! `matched_by_label`. Presenting an asserted match as a verified one is the
//! error this whole audit line has been chasing: a check whose failure is
//! indistinguishable from success.
//!
//! # The entry contract
//!
//! Not every input comes from an upstream stage. A pipeline also has inputs
//! the *caller* supplies, and those stay available for the whole run:
//! `rabble_curator` takes a `location` and does not need it until stage 3.
//!
//! They are passed to [`plan`] as `entry_inputs` — in practice the card's
//! top-level `accepts`. An input satisfied that way is a materially weaker
//! claim than one produced upstream, because it holds only if the caller
//! actually supplies it. So it gets its own [`SeamStatus`] rather than an
//! easily-overlooked empty field on the matched one.
//!
//! An input that is neither produced upstream nor in the entry contract is
//! [`SeamStatus::Unmatched`], and blocks. Note what that makes checkable: a
//! caller reading the card cannot know to supply an input the card does not
//! advertise, so such a pipeline is **not invocable from its own
//! declaration**. That was the corpus's most common declaration defect — nine
//! sites across six cards the first time this ran.

use crate::agent_backend::agent_card::WorkflowTemplate;
use serde::{Deserialize, Serialize};

/// How a seam between two adjacent stages resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SeamStatus {
    /// Every input this stage accepts is produced by some upstream stage. By
    /// label only — see the module docs. `on` names the labels that lined up.
    MatchedByLabel { on: Vec<String> },
    /// Every input is accounted for, but some arrive from the caller rather
    /// than from any stage. Weaker than [`SeamStatus::MatchedByLabel`]: it
    /// holds only for callers that actually supply `from_entry`.
    MatchedWithEntryInputs {
        produced: Vec<String>,
        from_entry: Vec<String>,
    },
    /// Downstream needs inputs that no upstream stage produces and the entry
    /// contract does not offer. `missing` names them. A pipeline with an
    /// unmatched seam should not be started.
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
    /// Inputs some stage needs that no stage produces, in first-use order.
    /// This is the pipeline's *computed* entry contract — what a caller must
    /// actually hand it — whether or not the card declares them.
    pub required_entry_inputs: Vec<String>,
    /// The subset of `required_entry_inputs` absent from the declared
    /// `entry_inputs`. Each is an input a caller could not know to supply, so
    /// these block.
    pub undeclared_entry_inputs: Vec<String>,
    /// True when every seam is satisfied and every stage has an agent.
    pub runnable: bool,
    /// Why not, in one line, when `runnable` is false.
    pub blocked_reason: Option<String>,
}

/// Plan a declared pipeline. Pure: reads declarations, touches nothing.
///
/// `entry_inputs` is what a caller may hand the pipeline — in practice the
/// card's top-level `accepts`. Pass an empty slice to check the stages in
/// isolation.
pub fn plan(template: &WorkflowTemplate, entry_inputs: &[String]) -> PipelinePlan {
    let stages = &template.stages;

    if stages.is_empty() {
        return PipelinePlan {
            stage_count: 0,
            seams: vec![],
            open_slots: vec![],
            runnable_stages: vec![],
            required_entry_inputs: vec![],
            undeclared_entry_inputs: vec![],
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

    // `produced` accumulates what stages make as we walk forward; anything in
    // `entry_inputs` is available throughout. An input in neither is missing.
    //
    // Treating *only stage 0's* `accepts` as the entry contract was this
    // planner's first bug, and running it over the real corpus is what caught
    // it: 9 of 12 declared pipelines reported unmatched seams that were
    // nothing of the kind. A validator that fails on correct input is worse
    // than none — it teaches people to ignore it.
    let mut produced: Vec<String> = Vec::new();
    let mut seams: Vec<Seam> = Vec::new();
    let mut required_entry_inputs: Vec<String> = Vec::new();
    let mut undeclared_entry_inputs: Vec<String> = Vec::new();

    for (i, stage) in stages.iter().enumerate() {
        // Partition this stage's inputs by provenance. Done for *every*
        // stage, including the first, so an input the card never advertises
        // is caught even when there is no seam to hang it on.
        let mut from_upstream: Vec<String> = Vec::new();
        let mut from_entry: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();

        for label in &stage.accepts {
            if produced.contains(label) {
                from_upstream.push(label.clone());
                continue;
            }
            if !required_entry_inputs.contains(label) {
                required_entry_inputs.push(label.clone());
            }
            if entry_inputs.contains(label) {
                from_entry.push(label.clone());
            } else {
                missing.push(label.clone());
                if !undeclared_entry_inputs.contains(label) {
                    undeclared_entry_inputs.push(label.clone());
                }
            }
        }

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
            } else if !missing.is_empty() {
                SeamStatus::Unmatched { missing }
            } else if !from_entry.is_empty() {
                SeamStatus::MatchedWithEntryInputs {
                    produced: from_upstream,
                    from_entry,
                }
            } else {
                SeamStatus::MatchedByLabel { on: from_upstream }
            };
            seams.push(Seam {
                upstream: upstream.name.clone(),
                downstream: stage.name.clone(),
                status,
            });
        }

        for p in &stage.produces {
            if !produced.contains(p) {
                produced.push(p.clone());
            }
        }
    }

    let unmatched: Vec<String> = seams
        .iter()
        .filter_map(|s| match &s.status {
            SeamStatus::Unmatched { missing } => Some(format!(
                "{} → {} ({})",
                s.upstream,
                s.downstream,
                missing.join(", ")
            )),
            _ => None,
        })
        .collect();

    // Open slots are reported first: with a hole in the pipeline the seam
    // analysis around it is not yet meaningful, and "fill the slot" is the
    // actionable instruction.
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
            unmatched.join(", ")
        ))
    } else if !undeclared_entry_inputs.is_empty() {
        // Reachable when the gap is on stage 0, which has no seam.
        Some(format!(
            "{} undeclared entry input(s): {} — no stage produces them and \
             the card does not accept them",
            undeclared_entry_inputs.len(),
            undeclared_entry_inputs.join(", ")
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
        required_entry_inputs,
        undeclared_entry_inputs,
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
            mermaid: None,
            stages,
            description: None,
            synthesis: None,
            selection: None,
            nodes: vec![],
            edges: vec![],
        }
    }

    /// Plan with a declared entry contract.
    fn plan_with(t: &WorkflowTemplate, entry: &[&str]) -> PipelinePlan {
        let entry: Vec<String> = entry.iter().map(|s| s.to_string()).collect();
        plan(t, &entry)
    }

    #[test]
    fn a_well_formed_pipeline_is_runnable() {
        let p = plan_with(
            &tmpl(vec![
                stage("Intake", Some("a"), &[], &["brief"]),
                stage("Analyse", Some("b"), &["brief"], &["findings"]),
                stage("Report", Some("c"), &["findings"], &["report"]),
            ]),
            &[],
        );
        assert!(p.runnable, "{:?}", p.blocked_reason);
        assert_eq!(p.stage_count, 3);
        assert_eq!(p.seams.len(), 2, "n stages produce n-1 seams");
        assert_eq!(p.runnable_stages, vec!["Intake", "Analyse", "Report"]);
        assert!(
            p.required_entry_inputs.is_empty(),
            "a pipeline that makes everything it needs asks the caller for nothing"
        );
    }

    #[test]
    fn an_unmatched_seam_blocks_before_anything_is_spent() {
        let p = plan_with(
            &tmpl(vec![
                stage("Intake", Some("a"), &[], &["brief"]),
                stage("Analyse", Some("b"), &["market-data"], &["findings"]),
            ]),
            &[],
        );
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
        let p = plan_with(
            &tmpl(vec![
                stage("Intake", Some("a"), &[], &["logo", "brief"]),
                stage("Analyse", Some("b"), &["brief"], &["findings"]),
                stage("Render", Some("c"), &["logo", "findings"], &["image"]),
            ]),
            &[],
        );
        assert!(p.runnable, "{:?}", p.blocked_reason);
        assert!(matches!(
            p.seams[1].status,
            SeamStatus::MatchedByLabel { .. }
        ));
    }

    /// A caller-supplied input stays available for the whole run, not just
    /// the first stage.
    ///
    /// Regression: seeding availability from stage 0 alone reported 9 of 12
    /// real declared pipelines as having unmatched seams they did not have.
    /// `rabble_curator` is the canonical case — the caller passes a
    /// `location`, and nothing needs it until stage 3.
    #[test]
    fn an_entry_input_reaches_a_late_stage() {
        let p = plan_with(
            &tmpl(vec![
                stage("Resolve", Some("a"), &["species-name"], &["species-card"]),
                stage("Mint", Some("b"), &["species-card"], &["creature-record"]),
                stage(
                    "Fly",
                    Some("c"),
                    &["creature-record", "location"],
                    &["beacon"],
                ),
            ]),
            &["species-name", "location"],
        );
        assert!(
            p.runnable,
            "a caller-supplied input must not read as an unmatched seam: {:?}",
            p.blocked_reason
        );
        assert_eq!(
            p.required_entry_inputs,
            vec!["species-name", "location"],
            "the computed entry contract, in first-use order"
        );
        assert!(p.undeclared_entry_inputs.is_empty());
    }

    /// The weaker claim gets its own status. A seam satisfied only because
    /// the caller promised an input is not the same evidence as a seam fed by
    /// the stage before it, and collapsing the two is the overclaim this
    /// module exists to avoid.
    #[test]
    fn a_caller_supplied_match_is_distinguishable_from_a_produced_one() {
        let p = plan_with(
            &tmpl(vec![
                stage(
                    "Brief",
                    Some("a"),
                    &["creative-brief"],
                    &["structured-brief"],
                ),
                stage(
                    "Create",
                    Some("b"),
                    &["structured-brief", "style-reference"],
                    &["image"],
                ),
            ]),
            &["creative-brief", "style-reference"],
        );
        assert!(p.runnable, "{:?}", p.blocked_reason);
        assert_eq!(
            p.seams[0].status,
            SeamStatus::MatchedWithEntryInputs {
                produced: vec!["structured-brief".into()],
                from_entry: vec!["style-reference".into()],
            },
            "the report must say which half of the match rests on the caller"
        );
    }

    /// But an input that is neither produced upstream nor declared is still a
    /// real gap. The entry-contract fix must not become "assume anything
    /// missing was supplied externally", which would make the validator
    /// useless.
    #[test]
    fn a_genuinely_absent_input_is_still_unmatched() {
        let p = plan_with(
            &tmpl(vec![
                stage(
                    "Brief",
                    Some("a"),
                    &["creative-brief"],
                    &["structured-brief"],
                ),
                stage("Create", Some("b"), &["style-reference"], &["image"]),
            ]),
            &["creative-brief"],
        );
        assert!(!p.runnable);
        assert_eq!(
            p.seams[0].status,
            SeamStatus::Unmatched {
                missing: vec!["style-reference".into()]
            }
        );
        assert_eq!(p.undeclared_entry_inputs, vec!["style-reference"]);
    }

    /// Stage 0 has no seam, so a gap there has nothing to attach to. It must
    /// still block: an undeclared input on the first stage is exactly as
    /// uninvocable as one in the middle.
    #[test]
    fn an_undeclared_input_on_the_first_stage_still_blocks() {
        let p = plan_with(
            &tmpl(vec![
                stage("Intake", Some("a"), &["brand-guidelines"], &["brief"]),
                stage("Write", Some("b"), &["brief"], &["copy"]),
            ]),
            &[],
        );
        assert!(!p.runnable, "stage 0 is not exempt from the entry contract");
        assert_eq!(p.undeclared_entry_inputs, vec!["brand-guidelines"]);
        assert!(p
            .blocked_reason
            .as_ref()
            .unwrap()
            .contains("undeclared entry input"));
        assert!(
            p.seams
                .iter()
                .all(|s| !matches!(s.status, SeamStatus::Unmatched { .. })),
            "there is no seam to blame — the gap is at the pipeline's mouth"
        );
    }

    #[test]
    fn an_open_slot_is_a_typed_vacancy_not_an_error() {
        let p = plan_with(
            &tmpl(vec![
                stage("Crux", Some("debate_strategist"), &["claim"], &["crux"]),
                stage("Exchange", None, &["crux"], &["transcript"]),
                stage(
                    "Judge",
                    Some("debate_strategist"),
                    &["transcript"],
                    &["verdict"],
                ),
            ]),
            &["claim"],
        );
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
        let p = plan_with(
            &tmpl(vec![
                stage("A", Some("a"), &[], &["x"]),
                stage("B", Some("b"), &[], &["y"]),
            ]),
            &[],
        );
        assert_eq!(p.seams[0].status, SeamStatus::NothingRequired);
        assert!(p.runnable);
    }

    #[test]
    fn partial_matches_still_block() {
        // Two of three inputs available is not runnable, and the report must
        // name only the genuinely missing one.
        let p = plan_with(
            &tmpl(vec![
                stage("A", Some("a"), &[], &["x", "y"]),
                stage("B", Some("b"), &["x", "y", "z"], &["out"]),
            ]),
            &[],
        );
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
        let p = plan_with(&tmpl(vec![]), &[]);
        assert!(!p.runnable);
        assert_eq!(p.stage_count, 0);
        assert!(p.blocked_reason.unwrap().contains("no stages"));
    }

    #[test]
    fn a_single_stage_pipeline_has_no_seams_and_runs() {
        let p = plan_with(
            &tmpl(vec![stage("Only", Some("a"), &["in"], &["out"])]),
            &["in"],
        );
        assert!(p.runnable);
        assert!(p.seams.is_empty());
        assert_eq!(p.required_entry_inputs, vec!["in"]);
    }

    /// Open slots are reported ahead of unmatched seams. With a hole in the
    /// pipeline the seam analysis around it is not yet meaningful, and
    /// "fill the slot" is the actionable instruction.
    #[test]
    fn open_slots_are_reported_before_seam_problems() {
        let p = plan_with(
            &tmpl(vec![
                stage("A", Some("a"), &[], &["x"]),
                stage("B", None, &["x"], &["y"]),
                stage("C", Some("c"), &["nonexistent"], &["z"]),
            ]),
            &[],
        );
        assert!(!p.runnable);
        assert!(p.blocked_reason.as_ref().unwrap().contains("unfilled"));
    }

    /// An entry input declared but never used is not an error — cards
    /// advertise inputs their non-pipeline paths use too. It simply must not
    /// appear in the computed contract.
    #[test]
    fn an_unused_entry_input_is_not_reported_as_required() {
        let p = plan_with(
            &tmpl(vec![stage("Only", Some("a"), &["in"], &["out"])]),
            &["in", "never-used"],
        );
        assert!(p.runnable);
        assert_eq!(p.required_entry_inputs, vec!["in"]);
    }
}
