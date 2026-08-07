//! Slot-match scoring — orphan-principal binding suggestions.
//!
//! Spec 36a A.1.1 / A.1.4. Ported from kask `_scoreSlotCandidate` +
//! `_findSlotMatch` (`kask-sim-client.js:7896-7980`).
//!
//! When a stage has a principal input without `from_stage`, this module
//! scores every upstream output as a candidate binding and returns the
//! best match with score + reasons. kask renders this as an action chip;
//! operator accepts → dispatches `mutate_document` to write the bind.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::process_v2::{OutputRole, ProcessConfigV2};

// ─── Unit family compatibility ────────────────────────────────────────────────

/// Unit families for compatibility filtering (kask UNIT_FAMILIES).
fn unit_family(unit: &str) -> &'static str {
    let u = unit.to_lowercase();
    if ["l", "ml", "m3", "gal", "fl_oz"].contains(&u.as_str()) {
        return "volume";
    }
    if ["kg", "g", "mg", "tonne", "lb", "oz"].contains(&u.as_str()) {
        return "mass";
    }
    if ["kwh", "mj", "kj", "kcal"].contains(&u.as_str()) {
        return "energy";
    }
    if [
        "unit", "piece", "pieces", "can", "cans", "bottle", "bottles", "sachet", "sachets", "box",
        "boxes", "sku", "pack", "packs", "pouch", "pouches", "jar", "jars",
    ]
    .contains(&u.as_str())
    {
        return "discrete";
    }
    "other"
}

fn units_compatible(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, _) | (_, None) => true, // null is permissive
        (Some(a), Some(b)) => {
            let fa = unit_family(a);
            let fb = unit_family(b);
            if fa == "other" && fb == "other" {
                a.to_lowercase() == b.to_lowercase()
            } else {
                fa == fb
            }
        }
    }
}

// ─── Scoring ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotBindingSuggestion {
    /// Stage that has the orphan principal input.
    pub consumer_stage_id: String,
    /// Name of the orphan principal input on that stage.
    pub consumer_input_name: String,
    /// Best upstream stage to bind to.
    pub proposed_from_stage: String,
    /// Best upstream output name to bind to.
    pub proposed_from_output: String,
    /// Composite score (higher = better match).
    pub score: i32,
    /// Human-readable reasons for the score.
    pub reasons: Vec<String>,
    /// Confidence 0.0–1.0 (score normalised to typical max ~3000).
    pub confidence: f64,
    /// Whether slot names matched (operator vocabulary converging).
    pub name_matched: bool,
}

/// Score one upstream output as a candidate binding for a consumer input.
/// Returns None when units are incompatible (hard filter).
fn score_candidate(
    consumer_unit: Option<&str>,
    consumer_slot: Option<&str>,
    consumer_density: Option<f64>,
    consumer_stage_idx: usize,
    upstream_output_name: &str,
    upstream_output_role: &OutputRole,
    upstream_output_unit: Option<&str>,
    upstream_output_density: Option<f64>,
    upstream_output_slot: Option<&str>,
    upstream_stage_idx: usize,
    upstream_stage_id: &str,
    upstream_output_already_linked: bool,
) -> Option<(i32, Vec<String>, bool)> {
    // Hard filter: unit family compatibility
    if !units_compatible(consumer_unit, upstream_output_unit) {
        return None;
    }

    let mut score = 0i32;
    let mut reasons = Vec::new();

    // Role preference
    match upstream_output_role {
        OutputRole::DownstreamFeed | OutputRole::Product => {
            score += 1500;
            reasons.push("output role is feed/product (+1500)".into());
        }
        OutputRole::Sidestream | OutputRole::Waste => {
            score += 100;
            reasons.push("output role is sidestream/waste (last resort, +100)".into());
        }
    }

    // Slot name match
    let name_matched = match (consumer_slot, upstream_output_slot) {
        (Some(cs), Some(us)) if cs.to_lowercase() == us.to_lowercase() => {
            score += 1000;
            reasons.push(format!("slot names match '{}' (+1000)", cs));
            true
        }
        _ => false,
    };

    // Exact unit match
    if let (Some(cu), Some(uu)) = (consumer_unit, upstream_output_unit) {
        if cu.to_lowercase() == uu.to_lowercase() {
            score += 500;
            reasons.push(format!("qty_unit exact match '{}' (+500)", cu));
        }
    }

    // Density compatibility
    if let (Some(cd), Some(ud)) = (consumer_density, upstream_output_density) {
        if (cd - ud).abs() < 1e-9 {
            score += 250;
            reasons.push("density_kg_per_unit matches (+250)".into());
        }
    }

    // Adjacency bonus
    if upstream_stage_idx + 1 == consumer_stage_idx {
        score += 200;
        reasons.push("immediately upstream stage (+200)".into());
    } else {
        let distance = consumer_stage_idx.saturating_sub(upstream_stage_idx);
        let bonus = (50i32 - (distance as i32 - 1) * 10).max(0);
        if bonus > 0 {
            score += bonus;
            reasons.push(format!("upstream stage distance {} (+{})", distance, bonus));
        }
    }

    // Already feeds downstream
    if upstream_output_already_linked {
        score += 50;
        reasons.push("output already linked to other downstream stages (+50)".into());
    }

    Some((score, reasons, name_matched))
}

// ─── Main entry point ─────────────────────────────────────────────────────────

/// Analyse a ProcessConfigV2 and return binding suggestions for every
/// orphan principal input (principal without `from_stage` on a non-first stage).
///
/// This is the ABW-side implementation of kask's `_findSlotMatch` +
/// `_scoreSlotCandidate` — spec 36a A.1.1 + A.1.4.
pub fn suggest_principal_bindings(process: &ProcessConfigV2) -> Vec<SlotBindingSuggestion> {
    let stages = &process.stages;
    let mut suggestions = Vec::new();

    // Pre-compute which (stage_id, output_name) pairs are already linked
    // from any stage (signal that they're "known feeds-downstream" outputs).
    let mut already_linked: HashSet<(String, String)> = HashSet::new();
    for stage in stages {
        for inp in &stage.inputs {
            if let (Some(fs), fo) = (&inp.from_stage, &inp.from_output) {
                let out_name = fo.as_deref().unwrap_or(&inp.name);
                already_linked.insert((fs.clone(), out_name.to_string()));
            }
        }
    }

    // For each non-first stage, check for orphan principals
    for (consumer_idx, consumer_stage) in stages.iter().enumerate().skip(1) {
        let principals: Vec<_> = consumer_stage
            .inputs
            .iter()
            .filter(|i| i.role == crate::process_v2::InputRole::Principal && i.from_stage.is_none())
            .collect();

        // Only auto-suggest for single-orphan-principal stages.
        // Multi-principal ambiguity is left for the operator.
        if principals.len() != 1 {
            continue;
        }
        let consumer_input = &principals[0];

        let consumer_unit = consumer_input.qty_unit.as_deref();
        // `slot` is not yet in the v2 schema; treat input name as slot proxy
        let consumer_slot = Some(consumer_input.name.as_str());
        let consumer_density = consumer_input.density_kg_per_unit;

        let mut best: Option<SlotBindingSuggestion> = None;

        for (upstream_idx, upstream_stage) in stages.iter().enumerate().take(consumer_idx) {
            for upstream_output in &upstream_stage.outputs {
                let already = already_linked
                    .contains(&(upstream_stage.id.clone(), upstream_output.name.clone()));
                let output_slot = Some(upstream_output.name.as_str()); // name as slot proxy
                let output_unit = upstream_output
                    .density_kg_per_unit
                    .map(|_| upstream_output.qty_unit.as_str());

                if let Some((score, reasons, name_matched)) = score_candidate(
                    consumer_unit,
                    consumer_slot,
                    consumer_density,
                    consumer_idx,
                    &upstream_output.name,
                    &upstream_output.role,
                    Some(&upstream_output.qty_unit),
                    upstream_output.density_kg_per_unit,
                    output_slot,
                    upstream_idx,
                    &upstream_stage.id,
                    already,
                ) {
                    let candidate = SlotBindingSuggestion {
                        consumer_stage_id: consumer_stage.id.clone(),
                        consumer_input_name: consumer_input.name.clone(),
                        proposed_from_stage: upstream_stage.id.clone(),
                        proposed_from_output: upstream_output.name.clone(),
                        score,
                        reasons,
                        confidence: (score as f64 / 3250.0).clamp(0.0, 1.0),
                        name_matched,
                    };
                    if best.as_ref().map(|b| score > b.score).unwrap_or(true) {
                        best = Some(candidate);
                    }
                }
            }
        }

        if let Some(suggestion) = best {
            suggestions.push(suggestion);
        }
    }

    suggestions
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process_v2::*;
    use std::collections::BTreeMap;

    fn two_stage_process(orphan: bool) -> ProcessConfigV2 {
        ProcessConfigV2 {
            schema_version: 2,
            name: "test".into(),
            description: None,
            throughput: Throughput {
                basis_stage: Some("s1".into()),
                basis_input: Some("water".into()),
                qty_per_run: 100.0,
                qty_unit: "L".into(),
                runs_per_year: None,
            },
            stages: vec![
                StageV2 {
                    id: "s1".into(),
                    name: None,
                    description: None,
                    inputs: vec![Input {
                        name: "water".into(),
                        role: InputRole::Principal,
                        qty: Some(1.0),
                        qty_unit: Some("L".into()),
                        per: None,
                        per_unit: None,
                        per_basis: Some(PerBasis::Principal),
                        from_stage: None,
                        from_output: None,
                        unit_cost: Some(0.001),
                        cost_unit: Some("eur_per_L".into()),
                        cost_source: None,
                        risk_flags: None,
                        density_kg_per_unit: Some(1.0),
                        mass_balance: None,
                    }],
                    outputs: vec![Output {
                        name: "intermediate".into(),
                        role: OutputRole::DownstreamFeed,
                        qty_per_input_kg: None,
                        qty_unit: "L".into(),
                        density_kg_per_unit: Some(1.0),
                        capture_fraction: None,
                        value_per_unit_usd: None,
                        disposal_cost_per_unit_usd: None,
                    }],
                    efficiency: 0.9,
                    power_kwh_per_input_kg: None,
                    labor_hours_per_input_kg: None,
                    carbon_intensity: None,
                    duration_hours: None,
                },
                StageV2 {
                    id: "s2".into(),
                    name: None,
                    description: None,
                    inputs: vec![Input {
                        name: "intermediate".into(),
                        role: InputRole::Principal,
                        qty: None,
                        qty_unit: Some("L".into()),
                        per: None,
                        per_unit: None,
                        per_basis: None,
                        // orphan when from_stage is None
                        from_stage: if orphan { None } else { Some("s1".into()) },
                        from_output: None,
                        unit_cost: None,
                        cost_unit: None,
                        cost_source: None,
                        risk_flags: None,
                        density_kg_per_unit: Some(1.0),
                        mass_balance: None,
                    }],
                    outputs: vec![Output {
                        name: "product".into(),
                        role: OutputRole::Product,
                        qty_per_input_kg: None,
                        qty_unit: "L".into(),
                        density_kg_per_unit: Some(1.0),
                        capture_fraction: None,
                        value_per_unit_usd: Some(2.0),
                        disposal_cost_per_unit_usd: None,
                    }],
                    efficiency: 0.95,
                    power_kwh_per_input_kg: None,
                    labor_hours_per_input_kg: None,
                    carbon_intensity: None,
                    duration_hours: None,
                },
            ],
            elec_price_per_kwh: None,
            labor_cost_per_hour: None,
            carbon_price_per_tonne: None,
        }
    }

    #[test]
    fn orphan_principal_gets_suggestion() {
        let process = two_stage_process(true);
        let suggestions = suggest_principal_bindings(&process);
        assert_eq!(suggestions.len(), 1);
        let s = &suggestions[0];
        assert_eq!(s.consumer_stage_id, "s2");
        assert_eq!(s.proposed_from_stage, "s1");
        assert_eq!(s.proposed_from_output, "intermediate");
        assert!(s.score > 0);
        assert!(s.confidence > 0.0);
    }

    #[test]
    fn linked_principal_no_suggestion() {
        let process = two_stage_process(false);
        let suggestions = suggest_principal_bindings(&process);
        assert!(suggestions.is_empty(), "no orphans when from_stage is set");
    }

    #[test]
    fn feed_role_scores_higher_than_sidestream() {
        // Two upstream stages: one with downstream_feed, one with sidestream
        // Both volume-compatible — feed should win
        let score_feed = score_candidate(
            Some("L"),
            None,
            None,
            1,
            "out",
            &OutputRole::DownstreamFeed,
            Some("L"),
            None,
            None,
            0,
            "s0",
            false,
        );
        let score_ss = score_candidate(
            Some("L"),
            None,
            None,
            1,
            "out",
            &OutputRole::Sidestream,
            Some("L"),
            None,
            None,
            0,
            "s0",
            false,
        );
        assert!(score_feed.unwrap().0 > score_ss.unwrap().0);
    }

    #[test]
    fn incompatible_units_returns_none() {
        // Volume consumer, mass upstream → incompatible
        let result = score_candidate(
            Some("L"),
            None,
            None,
            1,
            "out",
            &OutputRole::DownstreamFeed,
            Some("kg"),
            None,
            None,
            0,
            "s0",
            false,
        );
        assert!(result.is_none());
    }

    #[test]
    fn null_units_are_permissive() {
        let result = score_candidate(
            None,
            None,
            None,
            1,
            "out",
            &OutputRole::DownstreamFeed,
            None,
            None,
            None,
            0,
            "s0",
            false,
        );
        assert!(result.is_some());
    }
}
