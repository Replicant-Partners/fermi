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
    /// Stage-level NER: output_energy / input_energy
    pub stage_ner: f64,
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
    /// System-level NER: final output energy / total primary input energy
    pub system_ner: f64,
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
        let stage_ner = if in_e > 0.0 { out_e / in_e } else { 0.0 };
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
        final_energy_kwh / primary_energy_kwh
    } else {
        0.0
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
        let stage_ner = if in_e > 0.0 { out_e / in_e } else { 0.0 };
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
        final_energy_kwh / primary_energy_kwh
    } else {
        0.0
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
                },
                Stage {
                    id: "fermentation".into(),
                    efficiency: 0.20,
                    carbon_intensity: 0.3,
                    input: Resource { name: "biomass".into(), unit: "kg".into(), energy_density: Some(5.5), density_unit: Some("kcal/g".into()) },
                    output: Resource { name: "hydrogen".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    capex: None,
                    opex_per_input_unit: None,
                },
                Stage {
                    id: "fuel_cell".into(),
                    efficiency: 0.60,
                    carbon_intensity: 0.0,
                    input: Resource { name: "hydrogen".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    output: Resource { name: "electricity".into(), unit: "kWh".into(), energy_density: None, density_unit: None },
                    capex: None,
                    opex_per_input_unit: None,
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
        assert!(result.system_ner < 0.01, "system NER should be tiny: {}", result.system_ner);
    }
}
