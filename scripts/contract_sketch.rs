//! Compile an agent's output-contract sketch, or check the card still agrees
//! with it.
//!
//! ```text
//!   cargo run --bin contract-sketch -- equity_analyst           # print the contract
//!   cargo run --bin contract-sketch -- equity_analyst --check   # card vs sketch
//!   cargo run --bin contract-sketch -- --all --check            # every sketch in the corpus
//! ```
//!
//! ## Why there is no `--write`
//!
//! `serde_json::Map` is a `BTreeMap`, so serialising a card through it
//! alphabetises every key and turns a twelve-line contract change into a
//! whole-file diff. A tool whose output nobody can review is a tool whose
//! output nobody reviews.
//!
//! So this prints, and splicing is one line of `python3` that preserves your
//! card's key order:
//!
//! ```text
//!   cargo run --bin contract-sketch -- equity_analyst > /tmp/oc.json
//!   python3 - <<'PY'
//!   import json, collections
//!   p = "agents/curated/equity_analyst/agent_card.json"
//!   card = json.load(open(p), object_pairs_hook=collections.OrderedDict)
//!   oc   = json.load(open("/tmp/oc.json"), object_pairs_hook=collections.OrderedDict)
//!   card["capabilities"]["output_contract"] = oc["output_contract"]
//!   card["produces"] = oc["produces"]
//!   json.dump(card, open(p, "w"), indent=2, ensure_ascii=False)
//!   open(p, "a").write("\n")
//!   PY
//! ```
//!
//! What keeps the two in step afterwards is not this binary but
//! `tests/equity_analyst_contract.rs`, which fails if the card drifts from
//! the sketch. That is the right place for it: a generated artefact guarded
//! by a test stays generated, and one guarded by a habit does not.

use fermi::contract_sketch::{Ontology, Sketch};
use serde_json::Value;
use std::process::ExitCode;

const ROOTS: &[&str] = &["agents/curated", "agents/templates"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let all = args.iter().any(|a| a == "--all");
    let targets: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect();

    if !all && targets.is_empty() {
        eprintln!(
            "usage: contract-sketch <agent_id>... [--check]\n       contract-sketch --all --check"
        );
        return ExitCode::from(2);
    }

    let ids = if all { discover() } else { targets };
    if ids.is_empty() {
        eprintln!("no sketches found under {ROOTS:?}");
        return ExitCode::from(2);
    }

    let mut failed = 0usize;
    for id in &ids {
        match run(id, check) {
            Ok(out) => {
                if !check {
                    println!("{out}");
                } else {
                    println!("ok      {id}");
                }
            }
            Err(msg) => {
                failed += 1;
                eprintln!("FAILED  {id}\n{msg}");
            }
        }
    }

    if failed > 0 {
        eprintln!("\n{failed} of {} failed", ids.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn discover() -> Vec<String> {
    let mut out = Vec::new();
    for root in ROOTS {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path().join("output_contract.sketch.json");
            if p.exists() {
                if let Some(n) = e.file_name().to_str() {
                    out.push(n.to_string());
                }
            }
        }
    }
    out.sort();
    out
}

fn dir_for(id: &str) -> Result<std::path::PathBuf, String> {
    for root in ROOTS {
        let d = std::path::Path::new(root).join(id);
        if d.join("agent_card.json").exists() {
            return Ok(d);
        }
    }
    Err(format!("no agent_card.json for `{id}` under {ROOTS:?}"))
}

fn read(p: &std::path::Path) -> Result<Value, String> {
    let s = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {e}", p.display()))
}

fn run(id: &str, check: bool) -> Result<String, String> {
    let dir = dir_for(id)?;
    let card = read(&dir.join("agent_card.json"))?;
    let sketch_path = dir.join("output_contract.sketch.json");
    if !sketch_path.exists() {
        return Err(format!(
            "`{id}` has no output_contract.sketch.json. Write one — see \
             agents/curated/equity_analyst/output_contract.sketch.json for a worked \
             example, and docs/DESIGN_typed_output_contracts.md for the shape."
        ));
    }

    // The agent's real tools. Passed in rather than assumed, because the
    // check with teeth is the cross-reference: a `sourced` block may not
    // name a tool this agent cannot call.
    let tool_names: Vec<String> = card
        .pointer("/capabilities/mcp_tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut sketch = Sketch::from_json(&read(&sketch_path)?).map_err(render)?;

    // An ontology beside the card resolves `@entity` field types.
    let ont_path = dir.join("ontology.json");
    if ont_path.exists() {
        let ont = Ontology::from_json(&read(&ont_path)?)?;
        let errs = ont.expand(&mut sketch);
        if !errs.is_empty() {
            return Err(render(errs));
        }
    }

    let compiled = sketch.compile(&tool_names).map_err(render)?;

    if check {
        let on_card = card.pointer("/capabilities/output_contract");
        if on_card != Some(&compiled.output_contract) {
            return Err(
                "  the card's `capabilities.output_contract` is not what the \
                        sketch compiles to.\n  Either the card was hand-edited or the \
                        sketch changed. The sketch is the source of truth: recompile \
                        and splice (see the header of scripts/contract_sketch.rs).\n"
                    .to_string(),
            );
        }
        let produces: Vec<String> = card
            .get("produces")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if produces != compiled.produces {
            return Err(format!(
                "  the card's `produces` is {produces:?} but the declared type is \
                 {:?}. Every port must be the type name, or a downstream agent \
                 matching on it is matching a string that happens to look familiar.\n",
                compiled.produces
            ));
        }
        return Ok(String::new());
    }

    serde_json::to_string_pretty(&serde_json::json!({
        "output_contract": compiled.output_contract,
        "produces": compiled.produces,
        "generated_properties": compiled.generated_properties,
    }))
    .map_err(|e| e.to_string())
}

fn render(findings: Vec<fermi::card_contract::Finding>) -> String {
    findings
        .iter()
        .map(|f| format!("  [{}] {}", f.check, f.message))
        .collect::<Vec<_>>()
        .join("\n\n")
}
