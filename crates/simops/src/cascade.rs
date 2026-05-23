/// Multi-stage energy cascade analyser.
///
/// Two modes:
///   `forward`  — given an input quantity at stage 0, propagate through the chain
///   `backward` — given a target output at the final stage, back-calculate inputs
///
/// Each stage result records the quantities, carbon delta, and stage-level NER.
/// The `CascadeResult` summary rolls up system NER and net carbon.
use serde::{Deserialize, Serialize};

use crate::process::ProcessConfig;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage_id: String,
    pub input_quantity: f64,
    pub input_unit: String,
    pub output_quantity: f64,
    pub output_unit: String,
    /// Carbon delta for this stage (kg CO₂-eq).  Negative = sequestration.
    pub carbon_delta_kg: f64,
    /// Input energy in kWh (for NER roll-up)
    pub input_energy_kwh: f64,
    /// Output energy in kWh (for NER roll-up)
    pub output_energy_kwh: f64,
    /// Stage-level NER: output_energy / input_energy.
    /// `None` when input_energy_kwh = 0 (resource has no embodied
    /// energy → NER is mathematically undefined, not zero). The
    /// kask KPI strip swaps the NER tile for the agent-recommended
    /// metric in that case (yield, specific energy, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_ner: Option<f64>,
    /// Operational cost for this stage (USD)
    pub opex_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CascadeResult {
    pub process_name: String,
    pub stages: Vec<StageResult>,
    /// Total primary input quantity (at stage 0 input)
    pub total_input_quantity: f64,
    pub total_input_unit: String,
    /// Final output quantity (at last stage output)
    pub final_output_quantity: f64,
    pub final_output_unit: String,
    /// Net carbon across all stages (kg CO₂-eq).  Negative = net sink.
    pub net_carbon_kg: f64,
    /// System-level NER: final output energy / total primary input
    /// energy. `None` when the primary input resource has no embodied
    /// energy (energy_density = 0 or unset) — NER is mathematically
    /// undefined for mass-conservation processes (beverages, foods,
    /// cosmetics) and the KPI strip should show an alternative metric
    /// (volumetric_yield, specific_energy_intensity) instead. Previous
    /// versions returned 0.0 as a fallback which looked like a real
    /// value and misled the operator into thinking the process was
    /// failing energetically when it was simply not an energy-conversion
    /// process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_ner: Option<f64>,
    /// Total OPEX across all stages (USD)
    pub total_opex_usd: f64,
    /// Annualised CAPEX (USD)
    pub annual_capex_usd: f64,
}

// ─── Engine ───────────────────────────────────────────────────────────────────

/// Forward cascade: propagate `input_quantity` through all stages in order.
pub fn cascade_forward(process: &ProcessConfig, input_quantity: f64) -> CascadeResult {
    let mut quantity = input_quantity;
    let mut stages = Vec::with_capacity(process.stages.len());
    let mut net_carbon = 0.0_f64;
    let mut total_opex = 0.0_f64;

    // Primary input energy (denominator for system NER)
    let first_stage = &process.stages[0];
    let primary_energy_kwh = first_stage.input.energy_kwh(input_quantity);

    for stage in &process.stages {
        let in_q = quantity;
        let out_q = stage.forward(in_q);
        let carbon = stage.carbon_delta(out_q);
        let in_e = stage.input.energy_kwh(in_q);
        let out_e = stage.output.energy_kwh(out_q);
        // NER is meaningful only when the input resource carries
        // embodied energy. For mass-conservation stages (water → tea,
        // milk → yogurt) the input has no quantifiable kWh content
        // and NER is undefined — emit None so the consumer knows to
        // show an alternative metric.
        let stage_ner = if in_e > 0.0 { Some(out_e / in_e) } else { None };
        let opex = stage.opex_usd(in_q);

        net_carbon += carbon;
        total_opex += opex;

        stages.push(StageResult {
            stage_id: stage.id.clone(),
            input_quantity: in_q,
            input_unit: stage.input.unit.clone(),
            output_quantity: out_q,
            output_unit: stage.output.unit.clone(),
            carbon_delta_kg: carbon,
            input_energy_kwh: in_e,
            output_energy_kwh: out_e,
            stage_ner,
            opex_usd: opex,
        });

        quantity = out_q;
    }

    let last = process.stages.last().unwrap();
    let final_energy_kwh = last.output.energy_kwh(quantity);
    let system_ner = if primary_energy_kwh > 0.0 {
        Some(final_energy_kwh / primary_energy_kwh)
    } else {
        None
    };

    CascadeResult {
        process_name: process.name.clone(),
        stages,
        total_input_quantity: input_quantity,
        total_input_unit: first_stage.input.unit.clone(),
        final_output_quantity: quantity,
        final_output_unit: last.output.unit.clone(),
        net_carbon_kg: net_carbon,
        system_ner,
        total_opex_usd: total_opex,
        annual_capex_usd: process.total_annual_capex_usd(),
    }
}

/// Backward cascade: back-calculate the primary input required to produce
/// `target_output` from the final stage.
pub fn cascade_backward(process: &ProcessConfig, target_output: f64) -> CascadeResult {
    let n = process.stages.len();
    // Work backwards to find required quantities at each stage boundary
    let mut quantities = vec![0.0_f64; n + 1];
    quantities[n] = target_output;

    for i in (0..n).rev() {
        quantities[i] = process.stages[i].backward(quantities[i + 1]);
    }

    // Now forward-fill stage results using the computed quantities
    let mut stages = Vec::with_capacity(n);
    let mut net_carbon = 0.0_f64;
    let mut total_opex = 0.0_f64;

    for (i, stage) in process.stages.iter().enumerate() {
        let in_q = quantities[i];
        let out_q = quantities[i + 1];
        let carbon = stage.carbon_delta(out_q);
        let in_e = stage.input.energy_kwh(in_q);
        let out_e = stage.output.energy_kwh(out_q);
        let stage_ner = if in_e > 0.0 { Some(out_e / in_e) } else { None };
        let opex = stage.opex_usd(in_q);

        net_carbon += carbon;
        total_opex += opex;

        stages.push(StageResult {
            stage_id: stage.id.clone(),
            input_quantity: in_q,
            input_unit: stage.input.unit.clone(),
            output_quantity: out_q,
            output_unit: stage.output.unit.clone(),
            carbon_delta_kg: carbon,
            input_energy_kwh: in_e,
            output_energy_kwh: out_e,
            stage_ner,
            opex_usd: opex,
        });
    }

    let first_stage = &process.stages[0];
    let last_stage = process.stages.last().unwrap();
    let primary_input = quantities[0];
    let primary_energy_kwh = first_stage.input.energy_kwh(primary_input);
    let final_energy_kwh = last_stage.output.energy_kwh(target_output);
    let system_ner = if primary_energy_kwh > 0.0 {
        Some(final_energy_kwh / primary_energy_kwh)
    } else {
        None
    };

    CascadeResult {
        process_name: process.name.clone(),
        stages,
        total_input_quantity: primary_input,
        total_input_unit: first_stage.input.unit.clone(),
        final_output_quantity: target_output,
        final_output_unit: last_stage.output.unit.clone(),
        net_carbon_kg: net_carbon,
        system_ner,
        total_opex_usd: total_opex,
        annual_capex_usd: process.total_annual_capex_usd(),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{CapexProfile, ProcessConfig, Resource, Stage};

    fn algae_h2_process() -> ProcessConfig {
        ProcessConfig {
            name: "Algae H2 Chain".into(),
            description: None,
            feature_of_interest: None,
            elec_price_per_kwh: Some(0.12),
            maintenance_cost_usd: Some(100.0),
            stages: vec![
                Stage {
                    id: "cultivation".into(),
                    efficiency: 0.03,
                    carbon_intensity: -1.8,
                    input: Resource { name: "photons".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    output: Resource { name: "biomass".into(), unit: "kg".into(), energy_density: Some(5.5), density_unit: Some("kcal/g".into()) },
                    capex: Some(CapexProfile { total_usd: 25.0, lifespan_years: 1.0 }),
                    opex_per_input_unit: Some(0.12),
                    sidestreams: None,
                    sensors: None,
                },
                Stage {
                    id: "fermentation".into(),
                    efficiency: 0.20,
                    carbon_intensity: 0.3,
                    input: Resource { name: "biomass".into(), unit: "kg".into(), energy_density: Some(5.5), density_unit: Some("kcal/g".into()) },
                    output: Resource { name: "hydrogen".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    capex: None,
                    opex_per_input_unit: None,
                    sidestreams: None,
                    sensors: None,
                },
                Stage {
                    id: "fuel_cell".into(),
                    efficiency: 0.60,
                    carbon_intensity: 0.0,
                    input: Resource { name: "hydrogen".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    output: Resource { name: "electricity".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    capex: None,
                    opex_per_input_unit: None,
                    sidestreams: None,
                    sensors: None,
                },
            ],
        }
    }

    #[test]
    fn forward_three_stages() {
        let process = algae_h2_process();
        let result = cascade_forward(&process, 1000.0);

        // Efficiency is applied in energy space:
        // cultivation: 1000 kWh light × 0.03 = 30 kWh biomass energy
        //   → 30 kWh / (5500 kcal/kg / 860.42 kWh per kcal) = 30 × 860.42/5500 ≈ 4.693 kg
        let biomass_energy_kwh = 1000.0 * 0.03;
        let kwh_per_kg = 5500.0 / 860.42;
        let expected_biomass_kg = biomass_energy_kwh / kwh_per_kg;
        assert!(
            (result.stages[0].output_quantity - expected_biomass_kg).abs() < 1e-6,
            "biomass_kg = {}", result.stages[0].output_quantity
        );
        // fermentation: biomass_energy_kwh × 0.20 = 6 kWh H2
        let h2_kwh = biomass_energy_kwh * 0.20;
        assert!((result.stages[1].output_quantity - h2_kwh).abs() < 1e-6);
        // fuel_cell: h2_kwh × 0.60
        assert!((result.stages[2].output_quantity - h2_kwh * 0.60).abs() < 1e-6);
    }

    #[test]
    fn backward_recovers_forward_input() {
        let process = algae_h2_process();
        let fwd = cascade_forward(&process, 1000.0);
        let target = fwd.final_output_quantity;

        let bwd = cascade_backward(&process, target);
        // back-calculated primary input should equal the forward input
        assert!((bwd.total_input_quantity - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn net_carbon_is_negative_for_algae_chain() {
        // Cultivation sequesters; fermentation emits a small amount.
        // At these efficiencies cultivation dominates → net sink.
        let process = algae_h2_process();
        let result = cascade_forward(&process, 10_000.0);
        assert!(result.net_carbon_kg < 0.0);
    }

    #[test]
    fn system_ner_very_low_for_algae_h2() {
        // 3% photosynthetic × 20% fermentation × 60% fuel cell = 0.0036
        let process = algae_h2_process();
        let result = cascade_forward(&process, 10_000.0);
        let ner = result.system_ner.expect("algae_h2 has photonic input → NER defined");
        assert!(ner < 0.01, "system NER should be tiny: {}", ner);
    }

    /// Regression: for a mass-conservation process (L→L beverage with
    /// no energy_density on any resource), system_ner must be `None`,
    /// NOT `Some(0.0)`. The latter previously misled the kask KPI strip
    /// into displaying '0.000' which looked like a real value. None is
    /// honest: NER is mathematically undefined here.
    #[test]
    fn system_ner_is_none_for_mass_conservation_process() {
        let process = ProcessConfig {
            name: "kombucha".into(),
            description: None,
            feature_of_interest: None,
            elec_price_per_kwh: None,
            maintenance_cost_usd: None,
            stages: vec![
                Stage {
                    id: "ferment".into(),
                    efficiency: 0.85,
                    carbon_intensity: 0.04,
                    input:  Resource { name: "water".into(), unit: "L".into(),
                                       energy_density: None, density_unit: None },
                    output: Resource { name: "kombucha".into(), unit: "L".into(),
                                       energy_density: None, density_unit: None },
                    capex: None, opex_per_input_unit: None,
                    sidestreams: None, sensors: None,
                },
            ],
        };
        let result = cascade_forward(&process, 200.0);
        assert!(result.system_ner.is_none(),
            "L→L with no energy_density must report NER as None (undefined), not Some(0.0)");
        // Per-stage NER also None for the same reason.
        assert!(result.stages[0].stage_ner.is_none());
        // But the cascade still flows — output should be 200 × 0.85 = 170.
        assert!((result.stages[0].output_quantity - 170.0).abs() < 1e-9);
    }

    // ─── Doc 11 — mass-tracking cascade ──────────────────────────────

    /// The kombucha bioink chain from Doc 11. Four mass-tracking stages
    /// (kg→kg, no energy_density on any resource) plus an L→L→kg shape
    /// that the engine can't auto-bridge — but every same-unit stage
    /// must propagate the mass forward correctly under the Option A
    /// mass-balance pass-through.
    ///
    /// We drop the fermentation L→kg stage from the full Doc 11 chain
    /// since that one genuinely requires the user to declare a bridge
    /// (density). The remaining four kg→kg stages must not collapse to 0.
    fn kombucha_purification_chain() -> ProcessConfig {
        let mass_stage = |id: &str, eff: f64, ci: f64| Stage {
            id: id.into(),
            efficiency: eff,
            carbon_intensity: ci,
            input: Resource {
                name: "pellicle".into(),
                unit: "kg".into(),
                energy_density: None,
                density_unit: None,
            },
            output: Resource {
                name: "pellicle".into(),
                unit: "kg".into(),
                energy_density: None,
                density_unit: None,
            },
            capex: None,
            opex_per_input_unit: None,
            sidestreams: None,
            sensors: None,
        };
        ProcessConfig {
            name: "Kombucha Bioink (purification only)".into(),
            description: None,
            feature_of_interest: None,
            elec_price_per_kwh: None,
            maintenance_cost_usd: None,
            stages: vec![
                mass_stage("alkali_purification", 0.85, 0.02),
                mass_stage("mechanical_homogenisation", 0.95, 0.01),
                mass_stage("bioink_formulation", 0.92, 0.05),
            ],
        }
    }

    /// Pre-fix this returned `final_output_quantity = 0.0` for every
    /// stage past the first because each stage's `energy_kwh(qty)` was
    /// 0 (no energy_density), collapsing the cascade silently.
    /// Post-fix the mass propagates through every stage.
    #[test]
    fn mass_tracking_cascade_does_not_collapse_to_zero() {
        let process = kombucha_purification_chain();
        let result = cascade_forward(&process, 10.0);

        // Each kg→kg stage applies its efficiency directly to mass.
        // 10 × 0.85 × 0.95 × 0.92 ≈ 7.429 kg
        let expected = 10.0 * 0.85 * 0.95 * 0.92;
        assert!(
            (result.final_output_quantity - expected).abs() < 1e-9,
            "expected {expected}, got {}",
            result.final_output_quantity
        );

        // Every intermediate stage must also have non-zero output.
        for s in &result.stages {
            assert!(
                s.output_quantity > 0.0,
                "stage {} collapsed to 0 (Doc 11 regression)",
                s.stage_id
            );
        }
    }

    #[test]
    fn mass_tracking_cascade_backward_recovers_forward_input() {
        let process = kombucha_purification_chain();
        let fwd = cascade_forward(&process, 10.0);
        let bwd = cascade_backward(&process, fwd.final_output_quantity);
        assert!(
            (bwd.total_input_quantity - 10.0).abs() < 1e-9,
            "round-trip drift: {}",
            bwd.total_input_quantity
        );
    }

    /// System NER for an all-mass-balance, no-energy-density cascade is
    /// undefined (denominator is 0); the existing guard at line 97 of
    /// `cascade_forward` already returns 0 for that case. We just want to
    /// confirm the rest of the cascade (mass, carbon, opex) still works.
    #[test]
    fn mass_tracking_cascade_emits_real_carbon_delta() {
        let process = kombucha_purification_chain();
        let result = cascade_forward(&process, 10.0);
        // Three stages with positive carbon_intensity → net positive carbon.
        assert!(
            result.net_carbon_kg > 0.0,
            "expected net positive carbon, got {}",
            result.net_carbon_kg
        );
    }
}
