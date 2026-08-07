//! Model URI → DynamicsModel dispatch.
//!
//! `resolve(uri)` is the single dispatch point. Add new models here.

use crate::models::{
    bc_optimization::BcOptimization,
    kombucha_f2_carbonation::KombuchaF2Carbonation,
    kombucha_fermentation::KombuchaFermentation,
    linear_decay::LinearDecay,
    pellicle_growth::PellicleGrowth,
    solid_liquid_extraction::{SolidLiquidExtraction, SolventKind},
};
use crate::{DynamicsModel, SkillInput};

/// Resolve a model URI to a boxed DynamicsModel instance.
/// Returns None if the URI is unknown.
pub fn resolve(model_uri: &str, input: Option<&SkillInput>) -> Option<Box<dyn DynamicsModel>> {
    let temp_c = input
        .and_then(|i| i.process_context.get("temperature_c"))
        .and_then(|v| v.as_f64())
        .unwrap_or(26.0);

    let ph_floor = input
        .and_then(|i| i.params_override.get("ph_floor"))
        .copied()
        .unwrap_or(2.5);

    match model_uri {
        "kask:dynamics/linear_decay@v1" => {
            // For linear decay, the property URI is in initial_state or defaults to brix
            let property_uri = input
                .and_then(|i| i.initial_state.keys().next())
                .cloned()
                .unwrap_or_else(|| "chem:ph_value".into());
            let k = input
                .and_then(|i| i.params_override.get("k"))
                .copied()
                .unwrap_or(0.1);
            let target = input
                .and_then(|i| i.params_override.get("target"))
                .copied()
                .unwrap_or(0.0);
            Some(Box::new(LinearDecay::new(property_uri, k, target)))
        }

        "kask:dynamics/kombucha_fermentation@v1" => Some(Box::new(
            KombuchaFermentation::from_temperature(temp_c, ph_floor),
        )),

        "kask:dynamics/pellicle_growth@v1" => {
            let p_max = input
                .and_then(|i| i.params_override.get("p_max"))
                .copied()
                .unwrap_or(8.0);
            Some(Box::new(PellicleGrowth::from_temperature(
                temp_c, ph_floor, p_max,
            )))
        }

        "kask:dynamics/bc_optimization@v1" => {
            let agitation_rpm = input
                .and_then(|i| i.process_context.get("agitation_rpm"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let do_pct = input
                .and_then(|i| i.process_context.get("do_saturation_pct"))
                .and_then(|v| v.as_f64())
                .unwrap_or(10.0);
            let carbon = input
                .and_then(|i| i.process_context.get("carbon_source"))
                .and_then(|v| v.as_str())
                .unwrap_or("glucose")
                .to_string();
            let bc_max = input
                .and_then(|i| i.params_override.get("bc_max"))
                .copied()
                .unwrap_or(6.0);
            Some(Box::new(BcOptimization::from_context(
                temp_c,
                agitation_rpm,
                do_pct,
                &carbon,
                ph_floor,
                bc_max,
            )))
        }

        "kask:dynamics/kombucha_f2_carbonation@v1" => {
            Some(Box::new(KombuchaF2Carbonation::from_context(temp_c, &{
                let mut p = input.map(|i| i.params_override.clone()).unwrap_or_default();
                p
            })))
        }

        "kask:dynamics/solid_liquid_extraction@v1" => {
            let solvent_str = input
                .and_then(|i| i.process_context.get("solvent"))
                .and_then(|v| v.as_str())
                .unwrap_or("water");
            let solvent = match solvent_str {
                "ethanol_water_50" => SolventKind::EthanolWater50,
                "acetone_water_65" => SolventKind::AcetoneWater65,
                "custom" => SolventKind::Custom,
                _ => SolventKind::Water,
            };
            let cs_initial = input
                .and_then(|i| i.params_override.get("cs_initial"))
                .copied();
            let ae = input.and_then(|i| i.params_override.get("Ae")).copied();
            let ea = input.and_then(|i| i.params_override.get("Ea")).copied();
            let ae_deg = input.and_then(|i| i.params_override.get("Ae_deg")).copied();
            let ea_deg = input.and_then(|i| i.params_override.get("Ea_deg")).copied();
            let degradation_onset = input
                .and_then(|i| i.params_override.get("degradation_onset"))
                .copied();
            Some(Box::new(SolidLiquidExtraction::from_context(
                temp_c,
                solvent,
                cs_initial,
                ae,
                ea,
                ae_deg,
                ea_deg,
                degradation_onset,
            )))
        }

        _ => None,
    }
}

/// All known model URIs — for error messages and the `list_dynamics_models` skill.
pub fn known_uris() -> Vec<&'static str> {
    vec![
        "kask:dynamics/linear_decay@v1",
        "kask:dynamics/kombucha_fermentation@v1",
        "kask:dynamics/kombucha_f2_carbonation@v1",
        "kask:dynamics/pellicle_growth@v1",
        "kask:dynamics/bc_optimization@v1",
        "kask:dynamics/solid_liquid_extraction@v1",
    ]
}

/// List model manifests — used by the dynamics_runner agent to auto-select a model.
pub fn list_manifests() -> Vec<crate::ModelManifest> {
    vec![
        LinearDecay::new("chem:ph_value", 0.1, 0.0).manifest(),
        KombuchaFermentation::default().manifest(),
        KombuchaF2Carbonation::default().manifest(),
        PellicleGrowth::default().manifest(),
        BcOptimization::default().manifest(),
        SolidLiquidExtraction::default().manifest(),
    ]
}
