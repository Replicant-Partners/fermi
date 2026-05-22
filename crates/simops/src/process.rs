/// Process configuration — fully generic, loaded from JSON/TOML.
///
/// A `ProcessConfig` describes any multi-stage transformation chain:
/// algae cultivation → fermentation → fuel cell, SCOBY fermentation,
/// CCU polymer synthesis, or anything else.  The SimOps engine operates
/// solely on these types; nothing is hard-coded to a specific domain.
use serde::{Deserialize, Serialize};

// ─── Resource ─────────────────────────────────────────────────────────────────

/// A resource flowing into or out of a `Stage`.
///
/// `energy_density` is optional: set it when the resource carries embodied
/// energy that must be converted to kWh-equivalent for NER calculations.
/// Examples:
///   - biomass: 5.5 kcal/g  (caloric density)
///   - hydrogen: 33.3 kWh/kg
///   - photons/electricity: no density needed (already in kWh)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resource {
    /// Human-readable name, e.g. "biomass", "hydrogen", "electricity"
    pub name: String,
    /// SI unit of quantity, e.g. "kg", "kWh", "L"
    pub unit: String,
    /// Embodied energy per unit mass (optional).
    /// Use `density_unit` to record the dimension (e.g. "kcal/g", "kWh/kg").
    pub energy_density: Option<f64>,
    pub density_unit: Option<String>,
}

impl Resource {
    /// Convert a quantity of this resource to kWh for NER calculations.
    /// Returns `None` if no `energy_density` is set (resource is not an
    /// energy carrier and quantity is already in kWh or another non-energy unit).
    pub fn to_kwh(&self, quantity: f64) -> Option<f64> {
        match (self.energy_density, self.density_unit.as_deref()) {
            (Some(density), Some("kcal/g")) => {
                // quantity in kg → grams → kcal → kWh  (1 kWh = 860.42 kcal)
                Some(quantity * 1_000.0 * density / 860.42)
            }
            (Some(density), Some("kWh/kg")) => Some(quantity * density),
            (Some(density), Some("MJ/kg")) => Some(quantity * density / 3.6),
            // Already in kWh — no conversion needed
            _ => None,
        }
    }

    /// Energy in kWh: either direct (unit == "kWh") or via density conversion.
    pub fn energy_kwh(&self, quantity: f64) -> f64 {
        if self.unit == "kWh" {
            quantity
        } else {
            self.to_kwh(quantity).unwrap_or(0.0)
        }
    }

    /// Convert kWh back to the native quantity unit of this resource.
    /// Inverse of `energy_kwh`.
    pub fn quantity_from_kwh(&self, energy_kwh: f64) -> f64 {
        if self.unit == "kWh" {
            energy_kwh
        } else {
            let per_unit = self.energy_kwh(1.0);
            if per_unit > 0.0 { energy_kwh / per_unit } else { 0.0 }
        }
    }
}

// ─── CapexProfile ─────────────────────────────────────────────────────────────

/// Capital expenditure for the equipment associated with a stage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapexProfile {
    /// Total installed cost in USD
    pub total_usd: f64,
    /// Amortisation period in years
    pub lifespan_years: f64,
}

impl CapexProfile {
    /// Annualised CAPEX (straight-line)
    pub fn annual_usd(&self) -> f64 {
        if self.lifespan_years > 0.0 {
            self.total_usd / self.lifespan_years
        } else {
            0.0
        }
    }
}

// ─── Sidestream ───────────────────────────────────────────────────────────────

/// A secondary output from a stage — waste, by-product, or recoverable
/// resource. Sidestreams don't affect the primary mass/energy balance
/// (the cascade engine ignores them for NER/SEC/LCC calculations) but are
/// surfaced to the `sidestream_miner` agent and the kask Compose UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sidestream {
    /// Unique identifier within the stage, e.g. "co2", "pellicle"
    pub id: String,
    /// Human-readable name, e.g. "CO₂", "SCOBY pellicle"
    pub name: String,
    /// The resource that constitutes this sidestream
    pub resource: Resource,
    /// Fraction of total sidestream produced that we're capturing (0.0–1.0).
    /// 0.0 = all vented/discarded; 1.0 = fully captured.
    pub capture_fraction: f64,
    /// Market value per unit of resource (USD). `None` = unknown / not valued.
    pub value_per_unit_usd: Option<f64>,
    /// Current handling: "vented" | "captured" | "sold" | "discarded" | "recycled"
    pub current_disposition: Option<String>,
}

// ─── Sensor ───────────────────────────────────────────────────────────────────

/// An instrument attached to a stage that produces SOSA observations.
/// Sensors are metadata for the `sensor_advisor` agent and for wiring
/// stage outputs into the SOSA observation store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sensor {
    /// Unique identifier within the stage, e.g. "tea_temp", "ph_probe"
    pub id: String,
    /// Human-readable name, e.g. "Brew temperature", "pH probe"
    pub name: String,
    /// What the sensor measures, e.g. "temperature", "pH", "dissolved_oxygen"
    pub measures: String,
    /// SI unit of the observed property, e.g. "degC", "dimensionless", "mg/L"
    pub unit: String,
    /// Optional SOSA ObservableProperty URI for semantic provenance
    pub sosa_property_uri: Option<String>,
}

// ─── Stage ────────────────────────────────────────────────────────────────────

/// A single transformation stage in a process chain.
///
/// `efficiency` is the fraction of input energy/mass that becomes output:
///   output_quantity = input_quantity × efficiency
///
/// `carbon_intensity` is kg CO₂-eq per kg of *output*:
///   negative = the stage sequesters carbon (e.g. photosynthesis)
///   positive = the stage releases carbon (e.g. combustion, fermentation off-gas)
///   zero     = no net carbon change (e.g. ideal fuel cell)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stage {
    /// Unique identifier within the process, e.g. "cultivation", "fermentation"
    pub id: String,
    /// Fraction of input that becomes output (0 < efficiency ≤ 1)
    pub efficiency: f64,
    /// kg CO₂-eq / kg output.  Negative values indicate carbon sequestration.
    pub carbon_intensity: f64,
    /// Input resource specification
    pub input: Resource,
    /// Output resource specification
    pub output: Resource,
    /// Optional CAPEX for LCC calculation
    pub capex: Option<CapexProfile>,
    /// Optional per-unit opex (e.g. $/kWh electricity, $/kg nutrients)
    pub opex_per_input_unit: Option<f64>,
    /// Secondary outputs (waste, by-products, recoverable resources).
    /// Optional — existing YAML/JSON without this field deserialises fine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sidestreams: Option<Vec<Sidestream>>,
    /// Measurement instruments attached to this stage.
    /// Optional — existing YAML/JSON without this field deserialises fine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensors: Option<Vec<Sensor>>,
}

impl Stage {
    /// Should this stage be propagated as a same-unit mass-balance instead of
    /// going through the energy-equivalent bridge?
    ///
    /// The energy bridge (`input.energy_kwh` → × efficiency →
    /// `output.quantity_from_kwh`) is the right model for processes where
    /// each resource carries embodied energy that's actually relevant —
    /// fuel cells, electrolyte chains, anything whose efficiency the user
    /// already thinks of in NER terms.
    ///
    /// It is the wrong model for mass-tracking processes (food, biological,
    /// material) where the user has declared input.unit == output.unit and
    /// has not annotated either resource with an `energy_density`. Going
    /// through the bridge in that case multiplies quantity by 0 and
    /// silently collapses the cascade — see Doc 11.
    ///
    /// Trigger conditions (any-mismatch falls through to the energy bridge):
    ///   - input.unit == output.unit
    ///   - neither resource carries a meaningful energy_density
    ///     (None **or** Some(0.0); see below)
    ///
    /// This is a strict subset of the cases where the energy bridge currently
    /// returns 0, so it cannot change the output of any process whose energy
    /// bridge worked previously. kWh→kWh stages also satisfy the trigger and
    /// produce identical results, so they're unaffected in practice.
    ///
    /// **`energy_density: 0` is treated identically to `None`** — both mean
    /// "this resource has no embodied energy worth tracking", which is
    /// exactly when the energy bridge should NOT be used. Without this,
    /// any process whose Resources block was initialised with explicit
    /// zeros (a very common pattern — water tanks, beverage pipelines,
    /// food processing) silently collapses to 0 output because
    /// `input_quantity × 0 (kWh/L) = 0 kWh`, then `× efficiency = 0`,
    /// then `quantity_from_kwh(0) = 0`. The cascade chain breaks at the
    /// first such stage and every downstream stage sees 0 input.
    fn use_mass_balance(&self) -> bool {
        fn has_energy(d: Option<f64>) -> bool {
            matches!(d, Some(v) if v > 0.0)
        }
        self.input.unit == self.output.unit
            && !has_energy(self.input.energy_density)
            && !has_energy(self.output.energy_density)
    }

    /// Propagate a given input quantity forward through this stage.
    ///
    /// Two propagation modes:
    ///
    /// 1. **Mass-balance** (when [`Self::use_mass_balance`] returns true):
    ///    `output_quantity = input_quantity × efficiency`. Direct, unit-
    ///    preserving. The user's intuition for declarations like
    ///    "fermentation efficiency 0.72 on tea media (L→L)".
    ///
    /// 2. **Energy bridge** (default): efficiency is applied in
    ///    energy-equivalent space so cross-unit stages (e.g. kg biomass →
    ///    kWh hydrogen) are handled correctly:
    ///    `output_energy_kWh = input_energy_kWh × efficiency`, then
    ///    converted back to the output resource's native unit.
    pub fn forward(&self, input_quantity: f64) -> f64 {
        if self.use_mass_balance() {
            return input_quantity * self.efficiency;
        }
        let input_energy_kwh = self.input.energy_kwh(input_quantity);
        let output_energy_kwh = input_energy_kwh * self.efficiency;
        self.output.quantity_from_kwh(output_energy_kwh)
    }

    /// Back-calculate the required input to achieve a target output quantity.
    ///
    /// Mirrors [`Self::forward`]'s two propagation modes — mass-balance when
    /// the stage qualifies, energy bridge otherwise.
    pub fn backward(&self, target_output: f64) -> f64 {
        if self.efficiency <= 0.0 {
            return f64::INFINITY;
        }
        if self.use_mass_balance() {
            return target_output / self.efficiency;
        }
        let output_energy_kwh = self.output.energy_kwh(target_output);
        let input_energy_kwh = output_energy_kwh / self.efficiency;
        self.input.quantity_from_kwh(input_energy_kwh)
    }

    /// Carbon delta (kg CO₂-eq) for a given output quantity.
    /// Negative = sequestration, positive = emission.
    pub fn carbon_delta(&self, output_quantity: f64) -> f64 {
        self.carbon_intensity * output_quantity
    }

    /// Operational cost for a given input quantity.
    pub fn opex_usd(&self, input_quantity: f64) -> f64 {
        self.opex_per_input_unit.unwrap_or(0.0) * input_quantity
    }
}

// ─── ProcessConfig ────────────────────────────────────────────────────────────

/// Top-level process definition.  Stages are ordered: output of stage[n]
/// feeds input of stage[n+1].  The engine validates unit compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub name: String,
    pub description: Option<String>,
    /// SOSA FeatureOfInterest URI for semantic provenance
    pub feature_of_interest: Option<String>,
    pub stages: Vec<Stage>,
    /// Electricity price ($/kWh) — used in LCC for energy-input stages
    pub elec_price_per_kwh: Option<f64>,
    /// Fixed annual maintenance cost (USD)
    pub maintenance_cost_usd: Option<f64>,
}

impl ProcessConfig {
    /// Validate that adjacent stages have compatible resources
    /// (output unit of stage[n] == input unit of stage[n+1]).
    pub fn validate(&self) -> anyhow::Result<()> {
        for w in self.stages.windows(2) {
            let out = &w[0].output;
            let inp = &w[1].input;
            if out.name != inp.name || out.unit != inp.unit {
                anyhow::bail!(
                    "Stage '{}' output ({} {}) does not match stage '{}' input ({} {})",
                    w[0].id, out.name, out.unit,
                    w[1].id, inp.name, inp.unit,
                );
            }
        }
        Ok(())
    }

    pub fn total_annual_capex_usd(&self) -> f64 {
        self.stages
            .iter()
            .filter_map(|s| s.capex.as_ref().map(|c| c.annual_usd()))
            .sum()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn algae_stage() -> Stage {
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
        }
    }

    #[test]
    fn forward_propagation() {
        let stage = algae_stage();
        // 1000 kWh light × 3% efficiency = 30 kWh biomass energy
        // 30 kWh / (5500 kcal/kg ÷ 860.42 kWh/kcal) ≈ 4.693 kg
        let expected_kg = 30.0 * 860.42 / 5500.0;
        let out = stage.forward(1000.0);
        assert!((out - expected_kg).abs() < 1e-6, "got {out}");
    }

    #[test]
    fn backward_propagation() {
        let stage = algae_stage();
        // Round-trip: forward then backward should recover the input
        let input = 1000.0_f64;
        let output = stage.forward(input);
        let recovered = stage.backward(output);
        assert!((recovered - input).abs() < 1e-6, "got {recovered}");
    }

    #[test]
    fn carbon_delta_is_negative_for_sink() {
        let stage = algae_stage();
        let delta = stage.carbon_delta(stage.forward(1000.0));
        assert!(delta < 0.0, "cultivation should be a carbon sink");
    }

    #[test]
    fn resource_kwh_conversion() {
        let r = Resource {
            name: "biomass".into(), unit: "kg".into(),
            energy_density: Some(5.5), density_unit: Some("kcal/g".into()),
        };
        // 1 kg biomass = 1000g × 5.5 kcal/g = 5500 kcal = 5500/860.42 kWh ≈ 6.392 kWh
        let kwh = r.to_kwh(1.0).unwrap();
        assert!((kwh - 5500.0 / 860.42).abs() < 1e-6);
    }

    #[test]
    fn process_validation_catches_mismatch() {
        let mut stage2 = algae_stage();
        stage2.id = "fermentation".into();
        stage2.input = Resource { name: "hydrogen".into(), unit: "kWh".into(), energy_density: None, density_unit: None };
        let config = ProcessConfig {
            name: "bad".into(),
            description: None,
            feature_of_interest: None,
            stages: vec![algae_stage(), stage2],
            elec_price_per_kwh: None,
            maintenance_cost_usd: None,
        };
        assert!(config.validate().is_err());
    }

    // ─── Doc 11 — mass-balance pass-through ──────────────────────────

    /// Helper: a kg→kg stage with no energy_density on either side —
    /// the canonical mass-tracking process shape (food, biological,
    /// material) that used to silently collapse to 0.
    fn alkali_purification_stage() -> Stage {
        Stage {
            id: "alkali_purification".into(),
            efficiency: 0.85,
            carbon_intensity: 0.02,
            input: Resource {
                name: "pellicle".into(),
                unit: "kg".into(),
                energy_density: None,
                density_unit: None,
            },
            output: Resource {
                name: "purified_pellicle".into(),
                unit: "kg".into(),
                energy_density: None,
                density_unit: None,
            },
            capex: None,
            opex_per_input_unit: None,
            sidestreams: None,
            sensors: None,
        }
    }

    #[test]
    fn use_mass_balance_triggers_when_units_match_and_no_energy_density() {
        let stage = alkali_purification_stage();
        assert!(
            stage.use_mass_balance(),
            "kg→kg with no energy_density must use mass-balance propagation"
        );
    }

    #[test]
    fn use_mass_balance_does_not_trigger_when_units_differ() {
        let mut stage = alkali_purification_stage();
        stage.output.unit = "L".into();
        assert!(
            !stage.use_mass_balance(),
            "kg→L must fall through to the energy bridge regardless of densities"
        );
    }

    /// Regression: `energy_density: Some(0.0)` must be treated as
    /// "no embodied energy" (same as None), not as "use the energy
    /// bridge with a zero multiplier". The latter silently collapses
    /// the cascade — see kask kombucha workspace, every Resources
    /// block was initialised with `energy_density: 0` for L→L
    /// beverage stages and downstream stages saw 0 input.
    #[test]
    fn use_mass_balance_triggers_when_energy_density_is_zero() {
        let stage = Stage {
            id: "cold_steep".into(),
            efficiency: 0.93,
            carbon_intensity: 0.04,
            input: Resource {
                name: "water".into(),
                unit: "L".into(),
                energy_density: Some(0.0),
                density_unit: None,
            },
            output: Resource {
                name: "cold_steep_tea".into(),
                unit: "L".into(),
                energy_density: Some(0.0),
                density_unit: None,
            },
            capex: None,
            opex_per_input_unit: None,
            sidestreams: None,
            sensors: None,
        };
        assert!(
            stage.use_mass_balance(),
            "L→L with energy_density=0 on both sides must use mass-balance — \
             0 means 'no embodied energy', not 'energy bridge with zero kWh'"
        );
        // And the forward pass must actually flow non-zero quantity.
        let out = stage.forward(200.0);
        assert!(
            (out - 186.0).abs() < 1e-9,
            "200 L × 0.93 = 186 L expected; got {}",
            out
        );
    }

    /// Asymmetric zero: input declares energy_density=0 but output
    /// declares None. Still mass-balance (neither side carries
    /// meaningful energy).
    #[test]
    fn use_mass_balance_triggers_when_one_side_zero_other_none() {
        let mut stage = alkali_purification_stage();
        stage.input.energy_density = Some(0.0);
        assert!(
            stage.use_mass_balance(),
            "energy_density=0 ↔ energy_density=None equivalence: neither \
             carries energy → mass-balance"
        );
    }

    #[test]
    fn use_mass_balance_does_not_trigger_when_either_side_has_energy_density() {
        // The cross-unit fermentation stage from the algae chain has a
        // density on the biomass side — must NOT use mass-balance.
        let stage = Stage {
            id: "fermentation".into(),
            efficiency: 0.20,
            carbon_intensity: 0.3,
            input: Resource {
                name: "biomass".into(),
                unit: "kg".into(),
                energy_density: Some(5.5),
                density_unit: Some("kcal/g".into()),
            },
            output: Resource {
                name: "biomass".into(),
                unit: "kg".into(),
                energy_density: None,
                density_unit: None,
            },
            capex: None,
            opex_per_input_unit: None,
            sidestreams: None,
            sensors: None,
        };
        assert!(
            !stage.use_mass_balance(),
            "having an energy_density on one side means the user intends \
             the energy bridge — don't silently switch propagation modes"
        );
    }

    /// Doc 11 regression: forward propagation of a mass-tracking stage
    /// must NOT return 0. Pre-fix, this returned 0.0 because the energy
    /// bridge multiplied by `energy_density: None → 0 kWh`.
    #[test]
    fn forward_propagation_uses_mass_balance_when_no_energy_density() {
        let stage = alkali_purification_stage();
        // 10 kg in × 0.85 efficiency → 8.5 kg out (mass-balance).
        let out = stage.forward(10.0);
        assert!(
            (out - 8.5).abs() < 1e-9,
            "expected mass-balance 8.5 kg, got {}",
            out
        );
    }

    #[test]
    fn backward_propagation_uses_mass_balance_when_no_energy_density() {
        let stage = alkali_purification_stage();
        // Want 8.5 kg out at 0.85 efficiency → need 10 kg in.
        let needed = stage.backward(8.5);
        assert!(
            (needed - 10.0).abs() < 1e-9,
            "expected mass-balance 10 kg, got {}",
            needed
        );
    }

    /// Round-trip on a mass-balance stage must recover the input.
    #[test]
    fn mass_balance_round_trip_recovers_input() {
        let stage = alkali_purification_stage();
        let input = 12.3_f64;
        let recovered = stage.backward(stage.forward(input));
        assert!((recovered - input).abs() < 1e-9);
    }

    /// Same-unit kWh→kWh stages already worked under the energy bridge
    /// and must continue to produce identical results under the
    /// mass-balance path that now also fires for them.
    #[test]
    fn kwh_to_kwh_stage_unchanged_under_mass_balance() {
        let stage = Stage {
            id: "fuel_cell".into(),
            efficiency: 0.60,
            carbon_intensity: 0.0,
            input: Resource {
                name: "hydrogen".into(),
                unit: "kWh".into(),
                energy_density: None,
                density_unit: None,
            },
            output: Resource {
                name: "electricity".into(),
                unit: "kWh".into(),
                energy_density: None,
                density_unit: None,
            },
            capex: None,
            opex_per_input_unit: None,
            sidestreams: None,
            sensors: None,
        };
        assert!(stage.use_mass_balance());
        // 100 kWh × 0.60 → 60 kWh, both paths agree.
        let out = stage.forward(100.0);
        assert!((out - 60.0).abs() < 1e-9, "got {}", out);
    }
}
