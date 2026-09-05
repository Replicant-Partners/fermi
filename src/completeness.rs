//! # Did the agent fill the fields it was asked for?
//!
//! The question nothing asked. `grounding_trust::enforce` checks whether a tool
//! *could* have supplied a value and never whether the agent *did* produce one,
//! so an empty contracted field inherits its block's provenance and reads as
//! sourced. The artifact trace computed it on the page, from the values, and
//! said so in italics: *"that answer is computed here, and it is the only one on
//! this page that no checkpoint stands behind."*
//!
//! Rendered, that absence became `no gate` in a dotted box under a red header —
//! which reads as the agent bypassing a checkpoint when it is the platform
//! missing one. This module is the checkpoint.
//!
//! ## The distinction the whole thing turns on
//!
//! An empty field is three different situations with three different owners, and
//! collapsing them is what made a compliant agent look like a failing one:
//!
//! | the field | whose | verdict |
//! |---|---|---|
//! | `unsourced`, or `derived` | nobody's — the contract requires null | excused |
//! | `sourced`, tool was asked and had nothing | the **world's** | excused, counted |
//! | `sourced`, tool was never called | the **agent's** | owed |
//! | `inferred` or `narrative`, empty | the **agent's** — commissioned work | owed |
//!
//! *Lucanus cervus* is the case that forces it. `ncbi_genome_search` was called,
//! returned 229 bytes saying no assembly exists for the European stag beetle,
//! and the agent correctly nulled four genome fields. The trace's own row legend
//! calls that *"a capability gap and nobody's fault"* — and question three
//! counted all four against the agent, in red, on the same page.
//!
//! ## What it deliberately does not judge
//!
//! *The tool was asked, answered with substance, and the field is still empty.*
//! That is a real possibility and a real fault, and it is **not** assessed here.
//! Telling it apart from an honest miss needs a judgement about whether the
//! answer was inside that response, which the trace makes explicit with a byte
//! count and says only re-running settles. A gate must not accuse on a
//! judgement, so this one only reports what is unambiguous: the tool was never
//! called at all.
//!
//! That makes the count a floor, never a ceiling. Said plainly rather than left
//! for a reader to discover, because a floor presented as a total is how a
//! number stops being trusted.

use crate::grounding_trust::{GradedField, GroundingKind};

/// Why an empty field is the agent's to answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Owed {
    /// The contract names a tool and the record shows it was never called. The
    /// grade's "the tool had nothing" is not what happened.
    ToolNeverCalled,
    /// A judgement or a paragraph the agent was commissioned to produce, and
    /// did not. Nothing retrieves an answer to this; the fix is the agent.
    CommissionedWork,
}

impl Owed {
    pub fn as_str(self) -> &'static str {
        match self {
            Owed::ToolNeverCalled => "tool_never_called",
            Owed::CommissionedWork => "commissioned_work",
        }
    }
}

/// One contracted field the agent owed and did not deliver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Gap {
    pub path: &'static str,
    pub why: Owed,
    /// The tool that would have settled it, when the contract names one.
    pub tool: Option<&'static str>,
}

/// What the agent owed, what the world owed, and what nobody owed.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Assessment {
    /// Fields the agent was asked to fill: everything except a required
    /// absence.
    pub asked_for: usize,
    /// Of those, how many came back with a value.
    pub filled: usize,
    /// Empty, and the agent's to answer.
    pub owed: Vec<Gap>,
    /// Empty because the named tool was asked and had nothing. Not a fault, and
    /// carried rather than dropped: it is a standing request for a source that
    /// does not exist yet, which is the same thing `pending` means on the shelf.
    pub no_data: Vec<&'static str>,
    /// Empty because the contract requires it \u2014 `unsourced`, or `derived` and
    /// ours to compute.
    pub excused: usize,
}

impl Assessment {
    /// Nothing to say: the agent has no field contract, so there is no list of
    /// fields it was asked for and no verdict is available.
    ///
    /// Distinct from a clean pass, and the distinction is the one
    /// `gate_trust::Decision::Undetermined` exists for. Most agents on this
    /// platform land here.
    ///
    /// Keyed on `asked_for` alone. An earlier version also required
    /// `excused == 0`, which made a contract of nothing but required absences
    /// come back **determinate and clean** — a green verdict about an agent
    /// that was never asked for anything, which is the precise error this whole
    /// module is about, reproduced inside it.
    pub fn is_undetermined(&self) -> bool {
        self.asked_for == 0
    }
}

/// Assess one graded document against the tools the run actually called.
///
/// `tools_called` is the tool names from `AgentOutput::tool_invocations`. An
/// empty slice means the run made no calls, which is a real state and not
/// missing data: an agent whose contract names a tool and which called nothing
/// owes every sourced field.
pub fn assess(fields: &[GradedField], tools_called: &[&str]) -> Assessment {
    let mut a = Assessment::default();
    for f in fields {
        // A required absence is nobody's gap. `unsourced` must be null and
        // `derived` is the platform's to fill, so neither counts toward what the
        // agent was asked for \u2014 including when it IS filled, because a
        // platform-computed value is not the agent's work.
        if matches!(f.kind, GroundingKind::Unsourced | GroundingKind::Derived) {
            a.excused += 1;
            continue;
        }

        a.asked_for += 1;
        if has_value(&f.value) {
            a.filled += 1;
            continue;
        }

        match f.kind {
            GroundingKind::Sourced => match f.settleable_by {
                // The contract names a tool. Whether it ran is the whole
                // question, and the run record is the only thing that can say.
                Some(tool) if tools_called.contains(&tool) => a.no_data.push(f.path),
                Some(tool) => a.owed.push(Gap {
                    path: f.path,
                    why: Owed::ToolNeverCalled,
                    tool: Some(tool),
                }),
                // `Sourced` with no tool named is a malformed contract rather
                // than a fault of this run. Counted as no-data so a contract
                // bug cannot be charged to the agent.
                None => a.no_data.push(f.path),
            },
            GroundingKind::Inferred | GroundingKind::Narrative => a.owed.push(Gap {
                path: f.path,
                why: Owed::CommissionedWork,
                tool: None,
            }),
            // Excused above; unreachable, and returning rather than panicking
            // because a gate must not be able to take down the request it
            // observes.
            GroundingKind::Unsourced | GroundingKind::Derived => {}
        }
    }
    a
}

/// Is there a claim here at all?
///
/// `null`, absent and `[]` are all empty; `0` and `false` are not. That last
/// pair is the `??`-versus-absent trap this repo keeps finding: a chromosome
/// count of zero is a measurement, and reading it as missing would report an
/// agent that answered correctly as one that answered nothing.
fn has_value(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::String(s) => !s.trim().is_empty(),
        serde_json::Value::Object(o) => o.values().any(has_value),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding_trust::GroundingKind as K;
    use serde_json::json;

    fn f(
        path: &'static str,
        kind: K,
        tool: Option<&'static str>,
        value: serde_json::Value,
    ) -> GradedField {
        GradedField {
            path,
            block: "b",
            value,
            provenance: "tool_verified",
            settleable_by: tool,
            kind,
        }
    }

    /// **The case the gate exists for.** *Lucanus cervus*: NCBI was called and
    /// has no assembly for the European stag beetle, so four sourced fields are
    /// correctly null. That is the world's gap, and the agent owes nothing.
    #[test]
    fn a_tool_that_was_asked_and_had_nothing_is_nobodys_fault() {
        let fields = vec![
            f(
                "taxonomy",
                K::Sourced,
                Some("gbif_taxonomy_tree"),
                json!({"order": "Coleoptera"}),
            ),
            f(
                "genome.estimated_size_mb",
                K::Sourced,
                Some("ncbi_genome_search"),
                json!(null),
            ),
            f(
                "genome.chromosome_count",
                K::Sourced,
                Some("ncbi_genome_search"),
                json!(null),
            ),
        ];
        let a = assess(&fields, &["gbif_taxonomy_tree", "ncbi_genome_search"]);
        assert!(
            a.owed.is_empty(),
            "the agent called the tool and the tool had nothing; charging that to \
             the agent is what made a compliant run read as a failing one: {:?}",
            a.owed
        );
        assert_eq!(a.no_data.len(), 2);
        assert_eq!((a.asked_for, a.filled), (3, 1));
    }

    /// The same two empty fields, and now the tool was never called. Same
    /// document, opposite verdict \u2014 which is the entire reason the run record
    /// is an input.
    #[test]
    fn a_tool_that_was_never_called_is_the_agents_gap() {
        let fields = vec![
            f(
                "genome.estimated_size_mb",
                K::Sourced,
                Some("ncbi_genome_search"),
                json!(null),
            ),
            f(
                "genome.chromosome_count",
                K::Sourced,
                Some("ncbi_genome_search"),
                json!(null),
            ),
        ];
        let a = assess(&fields, &["gbif_taxonomy_tree"]);
        assert_eq!(a.owed.len(), 2);
        assert!(a.owed.iter().all(|g| g.why == Owed::ToolNeverCalled));
        assert_eq!(a.owed[0].tool, Some("ncbi_genome_search"));
        assert!(a.no_data.is_empty());
    }

    /// A required absence is not the agent's, and is not counted as work it was
    /// asked to do even when the platform has filled it.
    #[test]
    fn a_required_absence_is_excused_and_so_is_a_derived_value() {
        let fields = vec![
            f("conservation.iucn_status", K::Unsourced, None, json!(null)),
            f(
                "phylogeny.superorder",
                K::Derived,
                None,
                json!("Holometabola"),
            ),
        ];
        let a = assess(&fields, &[]);
        assert_eq!((a.asked_for, a.excused), (0, 2));
        assert!(a.owed.is_empty());
        assert!(
            a.is_undetermined(),
            "a document of nothing but excused fields says nothing about the \
             agent, and must not read as a clean pass"
        );
    }

    /// Commissioned work the agent did not do. Nothing retrieves this.
    #[test]
    fn an_empty_judgement_is_owed_by_the_agent() {
        let fields = vec![
            f("assessment", K::Inferred, None, json!(null)),
            f("summary", K::Narrative, None, json!("")),
        ];
        let a = assess(&fields, &[]);
        assert_eq!(a.owed.len(), 2);
        assert!(a.owed.iter().all(|g| g.why == Owed::CommissionedWork));
    }

    /// Zero is a measurement.
    ///
    /// NCBI reports `chromosome_count: 0` for a contig-level assembly, which is
    /// true and is the answer. Reading it as missing would report an agent that
    /// answered correctly as one that answered nothing \u2014 the `??`-versus-absent
    /// trap, in the one module whose whole job is telling absence from content.
    #[test]
    fn zero_and_false_are_answers_and_empty_containers_are_not() {
        let filled = vec![
            f("a", K::Sourced, Some("t"), json!(0)),
            f("b", K::Sourced, Some("t"), json!(false)),
        ];
        assert_eq!(assess(&filled, &["t"]).filled, 2);

        let empty = vec![
            f("c", K::Sourced, Some("t"), json!([])),
            f("d", K::Sourced, Some("t"), json!("   ")),
            f("e", K::Sourced, Some("t"), json!({ "x": null })),
        ];
        let a = assess(&empty, &["t"]);
        assert_eq!(a.filled, 0);
        assert_eq!(a.no_data.len(), 3);
    }

    /// No contract, no verdict. Distinct from a clean pass, and it is the
    /// majority state on this platform.
    #[test]
    fn no_contract_is_undetermined_rather_than_approved() {
        let a = assess(&[], &[]);
        assert!(a.is_undetermined());
        assert_eq!(a.owed.len(), 0);
    }

    /// A `Sourced` field naming no tool is a broken contract, not a broken run.
    #[test]
    fn a_sourced_field_with_no_tool_named_is_not_charged_to_the_agent() {
        let fields = vec![f("x", K::Sourced, None, json!(null))];
        let a = assess(&fields, &[]);
        assert!(
            a.owed.is_empty(),
            "a contract that names no tool cannot be answered by any run, so the \
             gap is the card's"
        );
        assert_eq!(a.no_data, vec!["x"]);
    }
}
