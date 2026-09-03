//! Compile an agent's `input_contract.sketch.json` into the typed input
//! contract for splicing into the agent card, or check the card still agrees.
//!
//! ```text
//!   cargo run --bin input-contract-sketch -- supply_chain_oracle
//!   cargo run --bin input-contract-sketch -- supply_chain_oracle --check
//!   cargo run --bin input-contract-sketch -- --all --check
//! ```
//!
//! ## Sketch format
//!
//! Simpler than `output_contract.sketch.json` — no grounding map, no
//! calibration block, no provenance. Just the schema the callee declares
//! for its callers:
//!
//! ```json
//! {
//!   "accepts_schema": "scro/bom-query/1",
//!   "title": "BOM pricing request",
//!   "fields": [
//!     { "name": "task",            "type": "string",  "note": "Discriminator — always 'resolve_bom'" },
//!     { "name": "bom_items",       "type": "array",   "note": "Items to price. Each: name, qty, unit, role." },
//!     { "name": "process_context", "type": "object?", "note": "Optional process context." },
//!     { "name": "currency",        "type": "string?", "note": "ISO 4217 currency code (default EUR)." }
//!   ]
//! }
//! ```
//!
//! Types: `string`, `number`, `integer`, `boolean`, `array`, `object`.
//! Append `?` to mark optional. Omitting `?` means the field is required.
//!
//! ## Compiled output
//!
//! ```json
//! {
//!   "input_contract": {
//!     "accepts_schema": "scro/bom-query/1",
//!     "title": "BOM pricing request",
//!     "required": ["task", "bom_items"],
//!     "schema": {
//!       "$id": "scro/bom-query/1",
//!       "title": "BOM pricing request",
//!       "type": "object",
//!       "required": ["task", "bom_items"],
//!       "properties": {
//!         "task":    { "type": "string", "description": "Discriminator — always 'resolve_bom'" },
//!         "bom_items": { "type": "array",  "items": {}, "description": "Items to price..." }
//!       },
//!       "additionalProperties": true
//!     }
//!   }
//! }
//! ```
//!
//! ## Splicing
//!
//! ```text
//!   cargo run --bin input-contract-sketch -- supply_chain_oracle > /tmp/ic.json
//!   python3 - <<'PY'
//!   import json, collections
//!   p = "agents/curated/supply_chain_oracle/agent_card.json"
//!   card = json.load(open(p), object_pairs_hook=collections.OrderedDict)
//!   ic   = json.load(open("/tmp/ic.json"))
//!   card["capabilities"]["input_contract"] = ic["input_contract"]
//!   json.dump(card, open(p, "w"), indent=2, ensure_ascii=False)
//!   open(p, "a").write("\n")
//!   PY
//! ```
//!
//! What keeps the card in step with the sketch afterwards is not this binary
//! but a test that fails if the card drifts:
//!   `tests/supply_chain_oracle_input_contract.rs` (see Phase C).
//! A generated artefact guarded by a test stays generated; one guarded by a
//! habit does not.

use serde_json::{json, Value};
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
            "usage: input-contract-sketch <agent_id>... [--check]\n\
             usage: input-contract-sketch --all --check"
        );
        return ExitCode::from(2);
    }

    let ids = if all { discover() } else { targets };
    if ids.is_empty() {
        eprintln!("no input_contract.sketch.json files found under {ROOTS:?}");
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
            let p = e.path().join("input_contract.sketch.json");
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

/// Parse a type annotation from the sketch. Returns `(base_type, is_required)`.
/// Trailing `?` marks the field as optional.
fn parse_type(raw: &str) -> (&str, bool) {
    if let Some(base) = raw.strip_suffix('?') {
        (base, false)
    } else {
        (raw, true)
    }
}

/// Map a sketch type name to the matching JSON Schema type string.
fn json_schema_type(t: &str) -> Result<&'static str, String> {
    match t {
        "string" => Ok("string"),
        "number" => Ok("number"),
        "integer" => Ok("integer"),
        "boolean" => Ok("boolean"),
        "array" => Ok("array"),
        "object" => Ok("object"),
        other => Err(format!(
            "unknown type `{other}` — use string, number, integer, boolean, array, or object \
             (append `?` for optional)"
        )),
    }
}

/// Compile a sketch Value into the `{ "input_contract": { ... } }` envelope
/// that gets spliced into the card.
fn compile(sketch: &Value) -> Result<Value, String> {
    let accepts_schema = sketch
        .get("accepts_schema")
        .and_then(|v| v.as_str())
        .ok_or("sketch must have `accepts_schema` (e.g. \"scro/bom-query/1\")")?;

    let title = sketch.get("title").and_then(|v| v.as_str()).unwrap_or("");

    let fields = sketch
        .get("fields")
        .and_then(|v| v.as_array())
        .ok_or("sketch must have a `fields` array")?;

    if fields.is_empty() {
        return Err(
            "sketch `fields` array is empty — an input contract with no fields is not \
                    useful. Add at least one field entry."
                .to_string(),
        );
    }

    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();

    for (i, field) in fields.iter().enumerate() {
        let name = field
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("fields[{i}] is missing `name`"))?;
        let type_raw = field
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("field `{name}` is missing `type`"))?;
        let note = field.get("note").and_then(|v| v.as_str()).unwrap_or("");

        let (base_type, is_required) = parse_type(type_raw);
        let js_type = json_schema_type(base_type)?;

        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), json!(js_type));
        if !note.is_empty() {
            prop.insert("description".to_string(), json!(note));
        }
        // For array fields, include a permissive `items: {}` so downstream
        // JSON Schema validators don't reject on a missing items keyword.
        // The callee's own contract governs the items shape; the input
        // contract just declares "this field must be an array".
        if base_type == "array" {
            prop.insert("items".to_string(), json!({}));
        }
        properties.insert(name.to_string(), Value::Object(prop));

        if is_required {
            required.push(json!(name));
        }
    }

    let schema = json!({
        "$id": accepts_schema,
        "title": title,
        "type": "object",
        "required": required,
        "properties": Value::Object(properties),
        // Callers may pass extra context fields without being rejected.
        // Strict mode would break unknown-but-harmless extensions.
        "additionalProperties": true,
    });

    Ok(json!({
        "input_contract": {
            "accepts_schema": accepts_schema,
            "title": title,
            "required": required,
            "schema": schema,
        }
    }))
}

fn run(id: &str, check: bool) -> Result<String, String> {
    let dir = dir_for(id)?;
    let card = read(&dir.join("agent_card.json"))?;
    let sketch_path = dir.join("input_contract.sketch.json");

    if !sketch_path.exists() {
        return Err(format!(
            "`{id}` has no input_contract.sketch.json.\n\
             Write one — see agents/curated/supply_chain_oracle/input_contract.sketch.json\n\
             for a worked example, and the header of scripts/input_contract_sketch.rs for\n\
             the field format."
        ));
    }

    let sketch = read(&sketch_path)?;
    let compiled = compile(&sketch)?;

    if check {
        let on_card = card.pointer("/capabilities/input_contract");
        let expected = compiled.get("input_contract").unwrap();
        if on_card != Some(expected) {
            return Err(format!(
                "  the card's `capabilities.input_contract` is not what the sketch compiles to.\n\
                 Either the card was hand-edited or the sketch changed. The sketch is the source\n\
                 of truth: recompile and splice (see the header of scripts/input_contract_sketch.rs).\n\
                 \n\
                 on card:    {}\n\
                 from sketch:{}\n",
                serde_json::to_string_pretty(&on_card.unwrap_or(&Value::Null))
                    .unwrap_or_default(),
                serde_json::to_string_pretty(expected).unwrap_or_default(),
            ));
        }
        return Ok(String::new());
    }

    serde_json::to_string_pretty(&compiled).map_err(|e| e.to_string())
}
