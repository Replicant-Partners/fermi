/// Key Performance Indicators for a single operational batch.
///
/// Accepts raw operational measurements and returns the four core KPIs
/// from the SimOps framework:
///   - NER  (Net Energy Ratio)
///   - SEC  (Specific Energy Consumption, kWh/kg output)
///   - LCC  (Levelized Cost of Output — generalised; default: $/million kcal)
///   - Harvest Intensity (% of total energy spent on the final separation step)
///
/// This module is intentionally dependency-free: pure numeric functions.
use serde::{Deserialize, Serialize};

pub const KCAL_PER_KWH: f64 = 860.42;

// ─── Input ────────────────────────────────────────────────────────────────────

/// Raw operational measurements for a single batch / observation window.
/// Field names use generic labels so this struct is domain-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchObservation {
    // ── Energy sinks ──────────────────────────────────────────────────────────
    /// Primary energy input (LED lighting, solar, process heat …) in kWh
    pub primary_energy_kwh: f64,
    /// Climate control (HVAC, cooling, dehumidification) in kWh
    pub climate_energy_kwh: f64,
    /// Circulation / mixing / pumping in kWh
    pub delivery_energy_kwh: f64,
    /// Separation / harvesting / drying in kWh
    pub harvest_energy_kwh: f64,

    // ── Output ────────────────────────────────────────────────────────────────
    /// Mass of dry output (biomass, product) in kg
    pub output_mass_kg: f64,
    /// Caloric density of the output in kcal/g (e.g. 5.5 for Chlorella)
    pub caloric_density_kcal_g: f64,

    // ── Economic inputs ───────────────────────────────────────────────────────
    /// Electricity price ($/kWh)
    pub elec_price_per_kwh: f64,
    /// Consumables cost for this batch (nutrients, reagents, CO₂) in USD
    pub consumables_cost_usd: f64,
    /// Annualised CAPEX contribution for this batch period (USD)
    pub capex_contribution_usd: f64,
}

impl BatchObservation {
    pub fn total_energy_kwh(&self) -> f64 {
        self.primary_energy_kwh
            + self.climate_energy_kwh
            + self.delivery_energy_kwh
            + self.harvest_energy_kwh
    }

    /// Total caloric output in kcal
    pub fn total_kcal_out(&self) -> f64 {
        self.output_mass_kg * 1_000.0 * self.caloric_density_kcal_g
    }

    /// Total energy input in kcal-equivalent
    pub fn total_energy_kcal_in(&self) -> f64 {
        self.total_energy_kwh() * KCAL_PER_KWH
    }
}

// ─── Output ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EnergyStatus {
    /// NER > 1.0 — system produces more energy than it consumes
    CaloricPositive,
    /// NER ≤ 1.0 — system is a net energy consumer (normal for indoor farms)
    EnergySink,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiReport {
    /// Net Energy Ratio: kcal_out / kcal_in.  Target > 1.0.
    pub ner: f64,
    /// Specific Energy Consumption: kWh / kg output.
    /// Benchmark: SEC > 15 kWh/kg suggests light leakage or HVAC inefficiency.
    pub sec_kwh_per_kg: f64,
    /// Levelized Cost of Output: $/million kcal.
    /// Hanging bag target: $80–$120.  Glass tubular: $250–$400.
    pub cost_per_million_kcal: f64,
    /// Fraction of total energy attributed to the harvest/separation step (%).
    /// If > 25%, the system is over-reliant on high-energy separation.
    pub harvest_intensity_pct: f64,
    /// Total OPEX for this batch (USD)
    pub total_opex_usd: f64,
    pub status: EnergyStatus,
}

// ─── Calculator ───────────────────────────────────────────────────────────────

pub fn compute_kpis(obs: &BatchObservation) -> KpiReport {
    let total_kwh = obs.total_energy_kwh();
    let total_kcal_out = obs.total_kcal_out();
    let total_kcal_in = obs.total_energy_kcal_in();

    let ner = if total_kcal_in > 0.0 {
        total_kcal_out / total_kcal_in
    } else {
        0.0
    };

    let sec_kwh_per_kg = if obs.output_mass_kg > 0.0 {
        total_kwh / obs.output_mass_kg
    } else {
        0.0
    };

    let total_opex_usd =
        total_kwh * obs.elec_price_per_kwh + obs.consumables_cost_usd + obs.capex_contribution_usd;

    let cost_per_million_kcal = if total_kcal_out > 0.0 {
        (total_opex_usd / total_kcal_out) * 1_000_000.0
    } else {
        0.0
    };

    let harvest_intensity_pct = if total_kwh > 0.0 {
        (obs.harvest_energy_kwh / total_kwh) * 100.0
    } else {
        0.0
    };

    KpiReport {
        ner,
        sec_kwh_per_kg,
        cost_per_million_kcal,
        harvest_intensity_pct,
        total_opex_usd,
        status: if ner > 1.0 {
            EnergyStatus::CaloricPositive
        } else {
            EnergyStatus::EnergySink
        },
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_batch() -> BatchObservation {
        BatchObservation {
            primary_energy_kwh: 120.0,
            climate_energy_kwh: 35.0,
            delivery_energy_kwh: 10.0,
            harvest_energy_kwh: 15.0,
            output_mass_kg: 4.5,
            caloric_density_kcal_g: 5.5,
            elec_price_per_kwh: 0.12,
            consumables_cost_usd: 8.50,
            capex_contribution_usd: 0.0,
        }
    }

    #[test]
    fn total_energy_sums_correctly() {
        let b = reference_batch();
        assert!((b.total_energy_kwh() - 180.0).abs() < 1e-9);
    }

    #[test]
    fn ner_is_less_than_one_for_indoor_farm() {
        let report = compute_kpis(&reference_batch());
        assert!(report.ner < 1.0);
        assert_eq!(report.status, EnergyStatus::EnergySink);
    }

    #[test]
    fn sec_value() {
        let report = compute_kpis(&reference_batch());
        // 180 kWh / 4.5 kg = 40.0 kWh/kg
        assert!((report.sec_kwh_per_kg - 40.0).abs() < 1e-9);
    }

    #[test]
    fn harvest_intensity_pct() {
        let report = compute_kpis(&reference_batch());
        // 15 / 180 × 100 = 8.33%
        assert!((report.harvest_intensity_pct - 15.0 / 180.0 * 100.0).abs() < 1e-6);
    }

    #[test]
    fn cost_per_million_kcal_reasonable() {
        let report = compute_kpis(&reference_batch());
        // Indoor farm with $0.12/kWh and 180 kWh for 4.5 kg @ 5.5 kcal/g:
        // opex ≈ $30.10, output ≈ 24,750 kcal → ~$1216/million kcal
        // (hanging bag targets assume larger scale; unit test uses a small batch)
        assert!(report.cost_per_million_kcal > 0.0);
        assert!(report.cost_per_million_kcal < 5000.0);
    }

    #[test]
    fn caloric_positive_when_output_very_high() {
        let mut b = reference_batch();
        b.output_mass_kg = 1000.0; // unrealistically high — just tests the flag
        let report = compute_kpis(&b);
        assert_eq!(report.status, EnergyStatus::CaloricPositive);
    }
}
