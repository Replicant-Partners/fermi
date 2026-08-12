//! Audit every declared `workflow_template` in the curated corpus.
//!
//! SPEC_31 P2. Ten-plus cards declare stage pipelines with per-stage
//! `accepts`/`produces`, and until the planner existed nothing checked whether
//! those declarations actually chain. Run this after editing any card's
//! workflow:
//!
//! ```sh
//! cargo run --example plan_probe
//! ```
//!
//! A blocked pipeline is usually not a bug in the agent. It means the card's
//! stage declarations were written as documentation and never validated, so
//! they drifted from what the agent does. That distinction matters: executing
//! declared pipelines requires the declarations to be true first.

use fermi::agent_backend::agent_card::WorkflowTemplate;
use fermi::pipeline;

fn main() {
    let mut rows = Vec::new();
    let dir = match std::fs::read_dir("agents/curated") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("run from the repo root: {e}");
            std::process::exit(1);
        }
    };

    for entry in dir.flatten() {
        let path = entry.path().join("agent_card.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(card) = serde_json::from_str::<serde_json::Value>(&text) else {
            eprintln!("  unparseable: {}", path.display());
            continue;
        };
        let Some(wf) = card.get("workflow_template").filter(|v| !v.is_null()) else {
            continue;
        };
        let Ok(template) = serde_json::from_value::<WorkflowTemplate>(wf.clone()) else {
            eprintln!(
                "  workflow_template does not deserialise: {}",
                path.display()
            );
            continue;
        };
        let id = card["agent_id"].as_str().unwrap_or("?").to_string();

        // The card's top-level `accepts` IS the pipeline's entry contract:
        // what a caller may hand it. Stages legitimately consume those inputs
        // long after stage 0.
        let entry: Vec<String> = card["accepts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        rows.push((id, pipeline::plan(&template, &entry)));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    println!(
        "{:<28} {:>6} {:>9} {:>6}  {}",
        "agent", "stages", "runnable", "slots", "blocked because"
    );
    for (id, p) in &rows {
        println!(
            "{:<28} {:>6} {:>9} {:>6}  {}",
            id,
            p.stage_count,
            p.runnable,
            p.open_slots.len(),
            p.blocked_reason.clone().unwrap_or_default()
        );
    }

    // The computed entry contract is the useful artefact for a caller: these
    // are the inputs the pipeline cannot make for itself.
    println!("\nentry contracts (what a caller must supply):");
    for (id, p) in &rows {
        if p.required_entry_inputs.is_empty() {
            continue;
        }
        let undeclared = if p.undeclared_entry_inputs.is_empty() {
            String::new()
        } else {
            format!("   UNDECLARED: {}", p.undeclared_entry_inputs.join(", "))
        };
        println!(
            "  {:<26} {}{}",
            id,
            p.required_entry_inputs.join(", "),
            undeclared
        );
    }

    let total = rows.len();
    let runnable = rows.iter().filter(|(_, p)| p.runnable).count();
    let slots: usize = rows.iter().map(|(_, p)| p.open_slots.len()).sum();
    let undeclared: usize = rows
        .iter()
        .map(|(_, p)| p.undeclared_entry_inputs.len())
        .sum();
    println!(
        "\n{total} declared pipeline(s) · {runnable} runnable · {} blocked · \
         {slots} open slot(s) · {undeclared} undeclared entry input(s)",
        total - runnable
    );
}
