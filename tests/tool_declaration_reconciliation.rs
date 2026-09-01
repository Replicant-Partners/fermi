//! # A card must declare the tools its own field contracts name
//!
//! ## The divergence
//!
//! Three places in this repo record which tool supplies a field, and they were
//! free to disagree:
//!
//! | source | for `football_analyst` | read by |
//! |---|---|---|
//! | `agent_card.json` → `capabilities.mcp_tools` | `execute_agent` | the contract builder, the publish gate, `invalid_tool_declarations` |
//! | [`grounding_trust::FIELD_CONTRACTS`] | `league_context` ← `call_football_api`, and five more paths | the trace view, `field_probe::declared_tool`, hop enforcement |
//! | the episode record | seven real calls: `standings`, `teams/statistics`×2, `players`×2, `injuries`, `players/topscorers` | the trace view |
//!
//! The second and third agree, and the trace already reconciles them per field
//! per episode — that is what produces `never asked · call_football_api would
//! close 4 of them`. Nothing reconciled either against the first, which is the
//! one the gate reads.
//!
//! This test is that missing reconciliation, in the direction that can be
//! checked statically.
//!
//! ## Why it is safe to enforce before the tool registry refactor
//!
//! `capabilities.mcp_tools` does not currently restrict anything.
//! `ToolRegistry::to_claude_tools_with_card_and_remote` offers every
//! LLM-visible builtin in the registry class and uses the card only to *add*
//! tools the registry lacks — so a card can omit a tool the agent calls
//! constantly, which is exactly what happened here across 218 runs at 99.1%.
//!
//! Whether that should change is a decision for the registry refactor, and this
//! test deliberately does not prejudge it. It asserts only a **fact**: this
//! agent's own contract says this tool supplies this field, so the card should
//! say the agent uses it. That is true whether `mcp_tools` ends up a grant, a
//! documentation field, or is deleted in favour of the registry class. Only the
//! *consequence* of the declaration is undecided, not its truth.
//!
//! ## Why only this direction
//!
//! Card-declares-but-contract-does-not-name is not an error: most declared
//! tools have no field contract at all, because only 9 agents have entries in
//! `FIELD_CONTRACTS`. And prompt-names-but-card-omits is a regex over prose —
//! reported by `scripts/tool_declaration_report.py`, deliberately not gated
//! here. See `docs/ISSUES_tool_declaration_gap.md`.

use fermi::grounding_trust::{Grounding, FIELD_CONTRACTS};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn card_path(agent: &str) -> PathBuf {
    repo().join(format!("agents/curated/{agent}/agent_card.json"))
}

/// `agent_id -> the tools its field contracts name`.
///
/// Read off the const rather than scanned out of the source. A regex over
/// `grounding_trust.rs` would be a second parser for a thing the compiler
/// already parsed, and it would quietly return nothing if the formatting
/// changed — passing the test by finding no work to do.
fn contracted_tools() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    let mut out: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
    for c in FIELD_CONTRACTS {
        if let Grounding::Sourced { tool, .. } = c.grounding {
            out.entry(c.agent_id).or_default().insert(tool);
        }
    }
    out
}

fn declared_tools(agent: &str) -> Option<BTreeSet<String>> {
    let raw = std::fs::read_to_string(card_path(agent)).ok()?;
    let card: serde_json::Value = serde_json::from_str(&raw).ok()?;
    Some(
        card.pointer("/capabilities/mcp_tools")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    )
}

#[test]
fn every_tool_a_field_contract_names_is_declared_on_the_card() {
    let contracted = contracted_tools();

    // A walk that found nothing would pass silently, which is how a
    // reconciliation test stops reconciling.
    assert!(
        contracted.len() >= 8,
        "only {} agent(s) have tool-sourced field contracts. FIELD_CONTRACTS is \
         either empty or this walk is broken, and either way the assertion below \
         checks nothing.",
        contracted.len()
    );

    let mut missing: Vec<String> = Vec::new();
    let mut no_card: Vec<&str> = Vec::new();

    for (agent, tools) in &contracted {
        let Some(declared) = declared_tools(agent) else {
            // Not every contracted agent is a curated card on disk. Recorded
            // rather than failed, so this test is about divergence and not
            // about where cards live.
            no_card.push(agent);
            continue;
        };
        let gap: Vec<&str> = tools
            .iter()
            .filter(|t| !declared.contains(**t))
            .copied()
            .collect();
        if !gap.is_empty() {
            missing.push(format!(
                "  {agent}\n      contract names: {}\n      card declares:  {}",
                gap.join(", "),
                if declared.is_empty() {
                    "(nothing)".to_string()
                } else {
                    declared.iter().cloned().collect::<Vec<_>>().join(", ")
                }
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "{} agent(s) have a field contract naming a tool their own card does not \
         declare:\n\n{}\n\nThe contract is the one that is right — it is what the \
         trace view reads, and the episode record backs it. The card is stale.\n\n\
         Fix by adding the tool to `capabilities.mcp_tools` with the description \
         from its registry definition. Do NOT copy the `input_schema`: the \
         registry owns it, 212 of 352 existing entries omit it, and a second copy \
         is a second thing to drift.\n\n\
         This is a statement of fact, not a permission grant. `mcp_tools` does \
         not currently restrict anything — see docs/ISSUES_tool_declaration_gap.md.\n\
         (agents with contracts but no curated card on disk, skipped: {:?})",
        missing.len(),
        missing.join("\n"),
        no_card
    );
}

/// The other half of the same fact: a named tool must actually exist.
///
/// `invalid_tool_declarations` already checks this for *card* declarations.
/// Nothing checked it for *contract* declarations, so a field contract could
/// name a tool that no longer dispatches — and the trace would print a
/// `run` button for it, which `field_probe` exists to make live.
#[test]
fn every_tool_a_field_contract_names_actually_dispatches() {
    let real = fermi::agent_backend::tools::platform_tool_names();
    let mut phantom: Vec<String> = Vec::new();

    for c in FIELD_CONTRACTS {
        if let Grounding::Sourced { tool, .. } = c.grounding {
            if !real.contains(&tool) {
                phantom.push(format!("{} · {} names `{tool}`", c.agent_id, c.path));
            }
        }
    }

    assert!(
        phantom.is_empty(),
        "{} field contract(s) name a tool with no dispatch arm:\n  {}\n\nThe trace \
         view offers a `run` button for a contracted tool, so a phantom name here \
         is an affordance that cannot work. If the tool was renamed, rename it \
         here too; if it was removed, the field's grounding is now `Unsourced` \
         and saying so is the point.",
        phantom.len(),
        phantom.join("\n  ")
    );
}
