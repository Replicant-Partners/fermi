//! ProcessConfig v2 — multi-input, multi-output stage model.
//!
//! Spec 30 reframes the SimOps stage from singular input/output to symmetric
//! `inputs[]`/`outputs[]` with explicit roles, upstream linkage, and
//! naive mass-conservation. This module contains all v2 types.
//!
//! v1 types (`process.rs`) are kept for backward compatibility in existing
//! code paths. The cascade engine v2 (`cascade_v2.rs`) exclusively uses
//! the types defined here.
//!
//! ## Schema version
//!
//! The top-level `schema_version: 2` field on `ProcessConfigV2` is
//! hard-required. Deserialising a document without it, or with any value
//! other than 2, produces a deserialisation error at the handler boundary.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

// ─── Flexible f64 deserialiser ────────────────────────────────────────────────
// Accepts bare f64 (0.97) OR tagged-union ({ "value": 0.97 }) OR string ("0.97").
// Spec 36a A.4.3: kask sends efficiency as tagged-union from some code paths;
// ABW should accept both and always emit bare f64 in responses.

fn deserialize_flexible_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    use serde::de::{self, MapAccess, Visitor};
    use std::fmt;

    struct Flex;
    impl<'de> Visitor<'de> for Flex {
        type Value = f64;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "a number or {{ value: number }} object")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>()
                .map_err(|_| de::Error::invalid_value(de::Unexpected::Str(v), &self))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<f64, A::Error> {
            let mut value: Option<f64> = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "value" {
                    value = Some(map.next_value()?);
                } else {
                    let _: serde_json::Value = map.next_value()?;
                }
            }
            value.ok_or_else(|| de::Error::missing_field("value"))
        }
    }
    d.deserialize_any(Flex)
}

fn deserialize_optional_flexible_f64<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<f64>, D::Error> {
    use serde::de::{self, MapAccess, Visitor};
    use std::fmt;

    struct OptFlex;
    impl<'de> Visitor<'de> for OptFlex {
        type Value = Option<f64>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "null, a number, or {{ value: number }}")
        }
        fn visit_none<E: de::Error>(self) -> Result<Option<f64>, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Option<f64>, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Option<f64>, E> {
            Ok(Some(v))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Option<f64>, E> {
            Ok(Some(v as f64))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Option<f64>, E> {
            Ok(Some(v as f64))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Option<f64>, E> {
            Ok(Some(v.parse::<f64>().map_err(|_| {
                de::Error::invalid_value(de::Unexpected::Str(v), &self)
            })?))
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Option<f64>, A::Error> {
            let mut value: Option<f64> = None;
            while let Some(key) = map.next_key::<String>()? {
                if key == "value" {
                    value = Some(map.next_value()?);
                } else {
                    let _: serde_json::Value = map.next_value()?;
                }
            }
            Ok(value)
        }
        fn visit_some<D2: Deserializer<'de>>(self, d: D2) -> Result<Option<f64>, D2::Error> {
            deserialize_flexible_f64(d).map(Some)
        }
    }
    d.deserialize_any(OptFlex)
}

// ─── Role enums ───────────────────────────────────────────────────────────────

/// How an input participates in mass-balance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputRole {
    /// Primary flow basis — efficiency operates on principal mass.
    Principal,
    /// Acts on principal (e.g. enzyme, SCOBY, yeast) — tracked for cost
    /// but excluded from mass-balance pool by default.
    Catalyst,
    /// Additive that is transformed/dissipated — included in mass-balance.
    Consumable,
}

/// Explicit per-input mass-balance mode override.
/// When set, overrides the role-based default.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MassBalanceMode {
    Include,
    Exclude,
}

/// Scaling basis for ratio-declared external inputs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PerBasis {
    /// Scale proportionally with the summed principal input qty (native unit).
    Principal,
    /// Absolute per cascade run — independent of principal qty.
    Batch,
}

/// What role an output plays in the process.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputRole {
    /// Output linked to by downstream stages via `from_stage`.
    DownstreamFeed,
    /// Terminal value-bearing output.
    Product,
    /// Captured byproduct — optional credit.
    Sidestream,
    /// Uncaptured — may carry disposal cost.
    Waste,
}

// ─── CarbonIntensity ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum CarbonIntensity {
    /// Operator-declared value in kg CO₂-eq per kg output.
    Synthetic { value: f64 },
    /// Defer to a lifecycle assessment database lookup (future).
    Lca { source: String },
}

impl CarbonIntensity {
    pub fn value_kg_per_kg(&self) -> f64 {
        match self {
            Self::Synthetic { value } => *value,
            Self::Lca { .. } => 0.0, // LCA not yet resolved
        }
    }
}

// ─── Input ────────────────────────────────────────────────────────────────────

/// One input declaration on a stage.
///
/// Either an external input (declares `qty` + optionally `per`/`per_unit`) or
/// an upstream-linked input (declares `from_stage`). Not both.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Input {
    /// Display name. For upstream-linked inputs, must match an output name
    /// on the upstream stage (or use `from_output` to disambiguate).
    pub name: String,

    /// How this input participates in mass-balance.
    pub role: InputRole,

    // ── External input fields ──────────────────────────────────────────────
    /// Quantity per `per` unit (ratio) or per run (if `per_basis: batch`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty: Option<f64>,

    /// Unit of qty (e.g. "L", "g", "kg").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty_unit: Option<String>,

    /// Ratio denominator quantity. With `per_unit` and `per_basis: principal`,
    /// means "qty qty_unit per per per_unit of principal input".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per: Option<f64>,

    /// Unit of the ratio denominator. Must match a principal input's qty_unit
    /// when `per_basis: principal`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_unit: Option<String>,

    /// How this input's qty scales. Default: `Principal`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_basis: Option<PerBasis>,

    // ── Upstream linkage ───────────────────────────────────────────────────
    /// Stage ID this input is sourced from. Mutually exclusive with `qty`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_stage: Option<String>,

    /// Optional output name disambiguator when the upstream stage has
    /// multiple outputs with the same name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_output: Option<String>,

    // ── Cost (external inputs only) ────────────────────────────────────────
    /// Cost per input unit (e.g. EUR/L, EUR/g, EUR/kg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost: Option<f64>,

    /// Cost unit string (e.g. "eur_per_L", "eur_per_g", "eur_per_kg").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_unit: Option<String>,

    /// Optional provenance of the cost figure (e.g. "supply_chain_oracle:2026-05-22").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_source: Option<String>,

    /// Supply chain risk flags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_flags: Option<Vec<String>>,

    // ── Mass-balance ───────────────────────────────────────────────────────
    /// Density for kg conversion. Required for mass-balance participation
    /// when qty_unit ≠ "kg". If absent and unit ≠ "kg", the input
    /// contributes 0 kg to mass-balance with a cascade_note explaining why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density_kg_per_unit: Option<f64>,

    /// Explicit mass-balance mode override. When set, overrides role default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mass_balance: Option<MassBalanceMode>,
}

impl Input {
    pub fn is_external(&self) -> bool {
        self.from_stage.is_none()
    }

    pub fn is_upstream_linked(&self) -> bool {
        self.from_stage.is_some()
    }

    /// Default mass-balance participation based on role.
    /// Can be overridden by `mass_balance` field.
    pub fn contributes_to_mass_balance(&self) -> bool {
        match &self.mass_balance {
            Some(MassBalanceMode::Include) => true,
            Some(MassBalanceMode::Exclude) => false,
            None => match self.role {
                InputRole::Principal | InputRole::Consumable => true,
                InputRole::Catalyst => false,
            },
        }
    }

    /// Convert qty to kg using density_kg_per_unit.
    /// Returns None if conversion is not possible (no density, unit ≠ "kg").
    pub fn qty_to_kg(&self, resolved_qty: f64) -> Option<f64> {
        let unit = self.qty_unit.as_deref().unwrap_or("kg");
        if unit == "kg" {
            Some(resolved_qty)
        } else if let Some(density) = self.density_kg_per_unit {
            Some(resolved_qty * density)
        } else {
            None
        }
    }
}

// ─── Output ───────────────────────────────────────────────────────────────────

/// One output declaration on a stage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Output {
    /// Name. Downstream stages reference this via `inputs[].name` + `from_stage`.
    pub name: String,

    /// Role of this output in the process.
    pub role: OutputRole,

    /// Declared yield ratio: kg of this output per kg of total mass-balance input.
    /// `None` on a `downstream_feed` means "take the residual."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qty_per_input_kg: Option<f64>,

    /// Unit of output quantity (e.g. "L", "kg").
    pub qty_unit: String,

    /// Density for back-converting output kg to native unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub density_kg_per_unit: Option<f64>,

    /// Fraction of sidestream captured (0.0–1.0). Sidestream only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_fraction: Option<f64>,

    /// Value per unit of output (EUR or USD — cascade normalises).
    /// Product: revenue. Sidestream: credit when captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_per_unit_usd: Option<f64>,

    /// Disposal cost per unit. Waste only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposal_cost_per_unit_usd: Option<f64>,
}

// ─── Stage v2 ─────────────────────────────────────────────────────────────────

/// A single transformation stage in a v2 process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageV2 {
    /// Unique identifier within the process.
    pub id: String,

    /// Optional human-readable name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// All inputs to this stage (replaces singular `input` + `bom`).
    pub inputs: Vec<Input>,

    /// All outputs from this stage (replaces singular `output` + `sidestreams`).
    pub outputs: Vec<Output>,

    /// Fraction of total mass-balance input that becomes total output.
    /// Accepts bare f64 (0.97) OR tagged-union ({ "value": 0.97 }) for
    /// backward compat with kask process YAMLs. Always emitted as bare f64.
    #[serde(deserialize_with = "deserialize_flexible_f64")]
    pub efficiency: f64,

    /// Energy consumption per kg of total mass-balance input (kWh/kg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_kwh_per_input_kg: Option<f64>,

    /// Labour hours per kg of total mass-balance input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labor_hours_per_input_kg: Option<f64>,

    /// Carbon intensity (kg CO₂-eq per kg output).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carbon_intensity: Option<CarbonIntensity>,

    /// Duration of this stage in hours (optional, can be inferred from dynamics).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_hours: Option<f64>,
}

// ─── Throughput ───────────────────────────────────────────────────────────────

/// Absolute scaling block. Declares the "1 unit of production scale."
///
/// All fields are optional at deserialisation time. When `basis_stage` or
/// `basis_input` are null/absent, the cascade engine emits a `bind_suggestion`
/// cascade_note and auto-selects the first stage with a principal input.
/// Spec 36a A.4.2: kask should stop auto-filling; ABW accepts null + annotates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Throughput {
    /// Which stage contains the basis input. Null = cascade auto-selects + emits bind_suggestion note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis_stage: Option<String>,
    /// Name of the basis input on that stage. Null = cascade auto-selects first principal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basis_input: Option<String>,
    /// Quantity per run in `qty_unit`. Defaults to 1.0 when absent.
    #[serde(default = "default_qty_per_run")]
    pub qty_per_run: f64,
    /// Unit of the basis quantity (e.g. "L", "kg"). Defaults to "unit".
    #[serde(default = "default_qty_unit")]
    pub qty_unit: String,
    /// Runs per year (for annualised economics).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs_per_year: Option<f64>,
}

fn default_qty_per_run() -> f64 {
    1.0
}
fn default_qty_unit() -> String {
    "unit".to_string()
}

// ─── ScaleRequest ─────────────────────────────────────────────────────────────

/// How to scale the cascade — replaces the flat `quantity` field.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScaleRequest {
    /// Use `process.throughput.qty_per_run` as the absolute basis quantity.
    FromThroughput,
    /// Override: explicit quantity at a named input on a named stage.
    Explicit {
        stage_id: String,
        input_name: String,
        qty: f64,
        qty_unit: String,
    },
}

impl Default for ScaleRequest {
    fn default() -> Self {
        Self::FromThroughput
    }
}

// ─── ProcessConfigV2 ──────────────────────────────────────────────────────────

/// Top-level v2 process definition.
///
/// `schema_version: 2` is hard-required. Any other value is rejected at
/// deserialisation time by the handler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfigV2 {
    /// Must be exactly 2.
    pub schema_version: u32,

    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Absolute scaling block.
    pub throughput: Throughput,

    pub stages: Vec<StageV2>,

    /// Global electricity price (EUR/kWh) for energy economics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elec_price_per_kwh: Option<f64>,

    /// Global labour cost (EUR/hour) for labour economics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labor_cost_per_hour: Option<f64>,

    /// Carbon price (EUR/tonne CO₂-eq) for carbon economics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub carbon_price_per_tonne: Option<f64>,
}

// ─── HTTP request body ────────────────────────────────────────────────────────

/// Twin parallelism scaling regime (spec 24 + spec 31 M5).
/// Determines how a per-instance metric scales to a total across N instances.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ScalingRegime {
    /// Named shorthand: "linear" | "constant"
    Named(String),
    /// Structured regime: { kind: "power", exponent: 0.6 }
    ///                 or { kind: "shared", base: 50, per_instance: 15 }
    Structured {
        kind: String,
        #[serde(default)]
        exponent: Option<f64>,
        #[serde(default)]
        base: Option<f64>,
        #[serde(default)]
        per_instance: Option<f64>,
    },
}

impl ScalingRegime {
    /// Apply this regime to (base_value_per_instance, instance_count).
    /// Returns the total across all active instances.
    pub fn apply(&self, base_value: f64, n: usize) -> f64 {
        if !base_value.is_finite() {
            return base_value;
        }
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            return base_value;
        }
        let n_f = n as f64;
        match self {
            Self::Named(s) if s == "constant" => base_value,
            Self::Named(_) => base_value * n_f, // "linear" or unknown → linear
            Self::Structured {
                kind,
                exponent,
                base,
                per_instance,
            } => match kind.as_str() {
                "constant" => base_value,
                "power" => {
                    let exp = exponent.unwrap_or(1.0).max(0.0);
                    base_value * n_f.powf(exp)
                }
                "shared" => {
                    let b = base.unwrap_or(0.0);
                    let pi = per_instance.unwrap_or(base_value);
                    b + pi * n_f
                }
                _ => base_value * n_f, // unknown → linear (safe default)
            },
        }
    }
}

impl Default for ScalingRegime {
    fn default() -> Self {
        Self::Named("linear".into())
    }
}

/// Scaling map per lens for a parallel stage.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StageScaling {
    #[serde(default)]
    pub materials: Option<ScalingRegime>,
    #[serde(default)]
    pub energy: Option<ScalingRegime>,
    #[serde(default)]
    pub labor: Option<ScalingRegime>,
    #[serde(default)]
    pub carbon: Option<ScalingRegime>,
    #[serde(default)]
    pub output_qty: Option<ScalingRegime>,
}

/// One physical vessel/instance within a parallel stage.
#[derive(Debug, Clone, Deserialize)]
pub struct ParallelInstance {
    pub id: String,
    /// "running" | "quarantined" | "harvested" | "failed"
    #[serde(default = "default_running")]
    pub status: String,
}
fn default_running() -> String {
    "running".into()
}

impl ParallelInstance {
    /// Active instances contribute to the stage total.
    /// Failed instances are excluded.
    pub fn is_active(&self) -> bool {
        self.status != "failed"
    }
}

/// Parallelism block for one stage in the twin manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct StageParallelism {
    pub kind: String, // "parallel_instances"
    #[serde(default)]
    pub instances: Vec<ParallelInstance>,
    #[serde(default)]
    pub scaling: StageScaling,
}

impl StageParallelism {
    pub fn active_count(&self) -> usize {
        self.instances
            .iter()
            .filter(|i| i.is_active())
            .count()
            .max(1)
    }
}

/// Twin manifest — read from `simops/twins/{twin_id}/twin.yaml`.
/// Passed in the cascade request body when the operator has a twin selected.
/// Absent = all stages are singleton (backward compat, no scaling applied).
#[derive(Debug, Clone, Deserialize)]
pub struct TwinManifest {
    #[serde(default)]
    pub twin_id: Option<String>,
    /// Per-stage parallelism map. Keys are stage IDs.
    /// Stages absent from this map are singleton (N=1, no scaling).
    #[serde(default)]
    pub parallelism: std::collections::HashMap<String, StageParallelism>,
}

/// Request body for `POST /api/simops/cascade` with a v2 process.
#[derive(Debug, Clone, Deserialize)]
pub struct CascadeRequestV2 {
    pub process: ProcessConfigV2,
    /// "forward" only for v2. "backward" returns 400 (deferred to spec 30.6).
    pub direction: String,
    /// How to determine the absolute input quantity.
    #[serde(default)]
    pub scale: ScaleRequest,
    /// Optional twin manifest. When present, parallelism scaling (4 regimes)
    /// is applied to each stage's economics. Absent = all singleton.
    /// Spec 36a A.2.3, spec 24 amendment 2026-05-29.
    #[serde(default)]
    pub twin: Option<TwinManifest>,
}

/// Thin envelope for schema-version detection at the handler boundary.
/// Deserialises just enough to read `process.schema_version`.
#[derive(Debug, Deserialize)]
pub struct CascadeRequestEnvelope {
    pub process: SchemaVersionProbe,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SchemaVersionProbe {
    pub schema_version: Option<u32>,
}
