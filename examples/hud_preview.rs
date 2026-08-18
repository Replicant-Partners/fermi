//! # HUD preview — see what the glasses would show, with no glasses involved
//!
//! ```text
//! cargo run --example hud_preview
//! ```
//!
//! Runs hand-written agent responses through the real
//! [`fermi::hud_contract::enforce`] and prints the rendered card, so the
//! treatment a wearer would read is visible on a terminal. No network, no
//! database, no model, no device.
//!
//! ## What this does and does not demonstrate
//!
//! It exercises the **boundary**: nulling, subject conditioning, per-line
//! markers, the computed confidence band, the prose scan, and the sticky
//! correction marker. Those are the parts that decide what a wearer sees.
//!
//! It does **not** demonstrate the model. The responses below are fixtures,
//! written by hand, including the deliberately bad one. Whether a real model
//! actually leaves `edibility` null is a separate question that wants eval
//! cases — the boundary guarantees a fabrication is stripped, not that nothing
//! produces one.
//!
//! It also does not involve the glasses. Capture and the phone relay are not
//! built; see `docs/specs/HUD_AGENT_LAYERS.md`.

use serde_json::{json, Value};

use fermi::hud_contract::{self, legend};

const AGENT: &str = "hud_field_scout";

fn main() {
    println!("\n╭──────────────────────────────────────────────────────────────╮");
    println!("│  HUD PREVIEW — rendered through src/hud_contract.rs          │");
    println!("│  Monochrome green panel: provenance is a glyph, not a colour. │");
    println!("╰──────────────────────────────────────────────────────────────╯");

    println!("\nLEGEND (a wearer learns this once)");
    for (marker, word) in legend() {
        let shown = if marker.is_empty() { "(none)" } else { marker };
        println!("   {shown:<7} {word}");
    }

    for (title, note, response) in cases() {
        show(title, note, response);
    }

    println!("\n─── what is NOT shown here ──────────────────────────────────────");
    println!("  · Capture (layer 1) and the phone relay (layer 2) do not exist.");
    println!("    This is the agent boundary only.");
    println!("  · The responses above are fixtures. The model is not involved,");
    println!("    so this shows what the boundary GUARANTEES, not what a model");
    println!("    happens to produce.\n");
}

fn show(title: &str, note: &str, mut doc: Value) {
    println!("\n════════════════════════════════════════════════════════════════");
    println!("CASE: {title}");
    println!("      {note}");

    let claimed = doc
        .pointer("/card/confidence_display")
        .and_then(|v| v.as_str())
        .unwrap_or("(none)")
        .to_string();

    let report = hud_contract::enforce(AGENT, &mut doc);

    println!("\n  ┌─ ON GLASS ─────────────────────────────────────────────────┐");
    for line in hud_contract::render(&doc) {
        println!("  │ {line:<58} │");
    }
    println!("  └────────────────────────────────────────────────────────────┘");

    println!("\n  spoken (summary): {}", speech(&doc));

    println!("\n  block provenance:");
    for (block, verdict) in &report.grounding.provenance {
        println!(
            "    {:<14} {:<28} -> {}",
            block,
            verdict,
            hud_contract::spec_word(verdict)
        );
    }

    println!(
        "\n  band: model claimed `{claimed}`, platform computed `{}` (floor: {})",
        report.confidence_display,
        report.floor.unwrap_or("unknown")
    );

    if !report.grounding.violations.is_empty() {
        println!("\n  STRIPPED (had no possible source):");
        for v in &report.grounding.violations {
            println!("    {} was {}", v.path, v.removed);
        }
    }

    if !report.findings.is_empty() {
        println!("\n  FINDINGS:");
        for f in &report.findings {
            println!("    [{}]", f.check);
            for chunk in wrap(&f.message, 66) {
                println!("      {chunk}");
            }
        }
    }

    if report.corrected {
        println!("\n  marked corrected: this card stays `flagged` on every re-read.");
    }
}

fn speech(doc: &Value) -> String {
    match doc.get("summary") {
        Some(Value::String(s)) => format!("\"{s}\""),
        _ => "(nulled — carried a claim nothing could support)".to_string(),
    }
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn cases() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            "Voice only — everything went right",
            "Real GBIF and iNaturalist data. Note it still reads `medium`.",
            json!({
                "capture": { "modality": "voice", "image_present": false },
                "subject": {
                    "scientific_name": "Quercus virginiana",
                    "common_name": "Southern live oak",
                    "rank_reached": "species"
                },
                "taxonomy": {
                    "kingdom": "Plantae", "phylum": "Tracheophyta",
                    "class": "Magnoliopsida", "order": "Fagales",
                    "family": "Fagaceae", "genus": "Quercus",
                    "species": "Quercus virginiana",
                    "matched_name": "Quercus virginiana Mill.",
                    "gbif_usage_key": 2878092,
                    "vernacular_name": "Southern Live Oak",
                    "taxonomic_status": "ACCEPTED",
                    "fungal_nomenclature": null
                },
                "observations": {
                    "count_nearby": 214, "radius_km": 25.0,
                    "most_recent": "2026-08-11", "place_guess": "Chatham County, GA"
                },
                "edibility": {
                    "verdict": null, "lookalikes": null,
                    "hazard_check_performed": null
                },
                "card": {
                    "title": "Live oak?",
                    "lines": [
                        { "text": "Quercus virginiana - Southern Live Oak", "block": "subject" },
                        { "text": "GBIF: Fagaceae, Fagales (ACCEPTED)", "block": "taxonomy" },
                        { "text": "iNat: 214 within 25km, last 11 Aug", "block": "observations" },
                        { "text": "edibility: not available", "block": "edibility" }
                    ],
                    "confidence_display": "high"
                },
                "summary": "I think that is a southern live oak. GBIF places the name in Fagaceae."
            }),
        ),
        (
            "Camera + a fabricated safety verdict",
            "The dangerous case. Schema-valid in, safety claim invented.",
            json!({
                "capture": { "modality": "voice+image", "image_present": true },
                "subject": {
                    "scientific_name": "Cantharellus cibarius",
                    "common_name": "Golden chanterelle",
                    "rank_reached": "species"
                },
                "taxonomy": {
                    "kingdom": "Fungi", "phylum": "Basidiomycota",
                    "class": "Agaricomycetes", "order": "Cantharellales",
                    "family": "Hydnaceae", "genus": "Cantharellus",
                    "species": "Cantharellus cibarius",
                    "matched_name": "Cantharellus cibarius Fr.",
                    "gbif_usage_key": 5249504,
                    "vernacular_name": "Chanterelle",
                    "taxonomic_status": "ACCEPTED",
                    "fungal_nomenclature": "current"
                },
                "observations": {
                    "count_nearby": 38, "radius_km": 25.0,
                    "most_recent": "2026-08-06", "place_guess": "Sormland, Sweden"
                },
                "edibility": {
                    "verdict": "choice edible",
                    "lookalikes": null,
                    "hazard_check_performed": null
                },
                "card": {
                    "title": "Golden chanterelle",
                    "lines": [
                        { "text": "Cantharellus cibarius - Chanterelle", "block": "subject" },
                        { "text": "Choice edible, no toxic lookalikes", "block": "edibility" },
                        { "text": "iNat: 38 within 25km", "block": "observations" }
                    ],
                    "confidence_display": "high"
                },
                "summary": "That is a golden chanterelle, a choice edible with no dangerous lookalikes."
            }),
        ),
        (
            "Tools asked, tools empty",
            "`?` (asked, nothing) must look different from `!` (unanswerable).",
            json!({
                "capture": { "modality": "voice+image", "image_present": true },
                "subject": {
                    "scientific_name": null, "common_name": null, "rank_reached": null
                },
                "taxonomy": {
                    "kingdom": null, "phylum": null, "class": null, "order": null,
                    "family": null, "genus": null, "species": null,
                    "matched_name": null, "gbif_usage_key": null,
                    "vernacular_name": null, "taxonomic_status": null,
                    "fungal_nomenclature": null
                },
                "observations": {
                    "count_nearby": null, "radius_km": null,
                    "most_recent": null, "place_guess": null
                },
                "edibility": {
                    "verdict": null, "lookalikes": null,
                    "hazard_check_performed": null
                },
                "card": {
                    "title": "Not determined",
                    "lines": [
                        { "text": "No confident identification", "block": "subject" },
                        { "text": "GBIF: no match for the name tried", "block": "taxonomy" },
                        { "text": "edibility: not available", "block": "edibility" }
                    ],
                    "confidence_display": "low"
                },
                "summary": "I could not place this one. Try a clearer frame of the underside."
            }),
        ),
    ]
}
