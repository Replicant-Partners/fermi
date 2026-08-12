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
//! A blocked pipeline is not necessarily a bug in the agent — it usually means
//! the card's stage declarations were written as documentation and never
//! validated, so they have drifted from what the agent does. `dream_coordinator`
//! is the clearest case: its Narrate stage accepts `consolidation-summary`,
//! which no upstream stage produces, yet dreaming works in production. The code
//! is right and the declaration is stale.
//!
//! That distinction matters for P2: executing declared pipelines requires the
//! declarations to be true first.

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
        rows.push((id, pipeline::plan(&template)));
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

    let total = rows.len();
    let runnable = rows.iter().filter(|(_, p)| p.runnable).count();
    let slots: usize = rows.iter().map(|(_, p)| p.open_slots.len()).sum();
    println!(
        "\n{total} declared pipeline(s) · {runnable} runnable · {} blocked · {slots} open slot(s)",
        total - runnable
    );
}
