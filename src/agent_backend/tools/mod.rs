//! Agent tool and skill infrastructure.
//!
//! # Primitives
//!
//! The ABW platform has four first-class primitives:
//!
//! - **Agent** — execution unit with identity, persona (system_prompt + valence),
//!   capabilities (model_ladder, capability_gates), and memory (episodes → dreaming)
//! - **Composition** — goal-bearing assemblage of agents with a coordination
//!   strategist (cohere_and_coordinate). Has cascade RSI and tune-team RSI loops.
//! - **Tool / Skill** — callable capabilities. Three kinds (see below).
//! - **RSI loops** — cascade (member agents learn composition context) and
//!   tune-team (strategist proposes structural changes, HITL gates execution).
//!
//! # Three kinds of callable capability
//!
//! ```text
//! 1. PLATFORM TOOLS     Infrastructure always available to every agent.
//!                       Memory search, workspace files, agent delegation,
//!                       coherence evaluation, observability reads, image gen.
//!                       Defined in tools_legacy::builtin_tools().
//!                       Registered by ToolRegistry.
//!
//! 2. MCP TOOLS          External API integrations declared on the agent card.
//!                       Polymarket, FMP financials, Reduct.video, Football API,
//!                       GBIF, web search. Require API keys in agent secrets.
//!                       Declared in agent_card.capabilities.mcp_tools.
//!                       Merged into LLM schema by to_claude_tools_with_card().
//!
//! 3. SKILLS             Deterministic computations — declared on the card via
//!                       agent_card.capabilities.skills. Validated at startup
//!                       by validate_card_skills(). Can be LLM-visible (offered
//!                       in the tool schema) or pipeline-only (invoked directly).
//!                       Implemented via the Skill trait + SkillRegistry.
//!
//!                       Current skills:
//!                         Spatial:   h3_resolve, geocode, create_beacon,
//!                                    query_beacons, save_grid_map,
//!                                    scan_nearby_creatures
//!                         Simulation: run_monte_carlo, run_sensitivity_analysis
//!                         Biology:   gbif_taxonomy_tree, segment_creature_wings
//!                         Formation: activate_formation
//!                         SimOps:    simops_cascade_*, simops_kpi_*,
//!                                    simops_predictor_*, simops_optimize_*
//! ```
//!
//! # Extensibility
//!
//! - Add a platform tool: add to `builtin_tools()` in `tools_legacy.rs` +
//!   match arm in `execute()`.
//! - Add an MCP integration: add to `execute()` match + declare on agent card.
//! - Add a skill: `impl Skill` + register in `SkillRegistry::all()`.
//!   No touching the other categories.
//!
//! # File layout
//!
//! ```text
//! tools/
//! ├── mod.rs          This file — interfaces, Skill trait, SkillRegistry, ToolRegistry
//! └── (implementations live in ../tools_legacy.rs during migration;
//!      will be split into platform/, mcp/, skills/ in the fermi-fpl PR)
//! ```
//!
//! See: `docs/AGENT_MODEL.md`, `docs/architecture/LEARNING_MECHANICS_SIMPLIFICATION.md`,
//!      `docs/STATE_OF_PROJECT.md §3`

// Pull all legacy implementations into scope.
// TODO(fermi-tools-split): move implementations into platform/, mcp/, skills/
// subdirectories in a dedicated PR once the fermi-fpl extraction is complete.
// The interfaces below (Skill, SkillRegistry, ToolRegistry, validate_card_skills)
// are already correct and stable.
#[path = "../tools_legacy.rs"]
mod legacy;

// Re-export everything the rest of the codebase uses from the legacy module.
pub use legacy::{
    // Tool-declaration validation. Names in `capabilities.mcp_tools` must
    // resolve to a dispatch arm in `ToolRegistry::execute`, or they become
    // phantom tools: advertised to the model and over `/mcp/agents/:id`,
    // then answered with `Unknown tool: X`.
    // Name + description for every builtin, so the contract builder can turn
    // a declared tool into a candidate evidence block. The description is
    // real evidence about document shape in a way a port label is not.
    builtin_tool_catalogue,
    dispatchable_tool_names,
    // Two keyless HTTP tools, re-exported so a handler that already holds a name
    // can ground it without standing up a full `ToolContext`. Neither takes
    // `ctx`, so requiring a memory store, an embedder and an agent registry to
    // reach them would push callers toward re-implementing the lookup — and a
    // second copy of a lookup is a second answer to the same question.
    execute_gbif_species_search,
    execute_mycobank_lookup,
    invalid_tool_declarations,
    platform_tool_names,
    platform_tools,
    BuiltinToolDef,
    EvalTrigger,
    ToolContext,
    ToolDeclarationError,
    ToolRegistry,
};

use crate::agent_backend::agent_card::AgentCard;
use ::dynamics;
use async_trait::async_trait;
use serde_json::json;

// ─── Skill trait ─────────────────────────────────────────────────────────────

/// The contract every deterministic skill implements.
///
/// Skills are pure-function capabilities declared on the agent card via
/// `capabilities.skills`. They differ from tools in that they are:
/// - Deterministic (same input → same output, no LLM involved)
/// - Validated at startup (`validate_card_skills` checks all declared names exist)
/// - Independently extensible (`impl Skill` + register — no touching ToolRegistry)
///
/// # Example
///
/// ```rust
/// pub struct RunMonteCarlo;
///
/// #[async_trait]
/// impl Skill for RunMonteCarlo {
///     fn name(&self) -> &'static str { "run_monte_carlo" }
///     fn description(&self) -> &'static str { "Execute FPL via Monte Carlo engine." }
///     fn category(&self) -> SkillCategory { SkillCategory::Simulation }
///     fn input_schema(&self) -> serde_json::Value { json!({...}) }
///     async fn execute(&self, input: &serde_json::Value, ctx: &ToolContext)
///         -> Result<String, String> { ... }
/// }
/// ```
#[async_trait]
pub trait Skill: Send + Sync {
    /// Stable identifier — used in dispatch table and `capabilities.skills` declarations.
    fn name(&self) -> &'static str;

    /// Human-readable description — shown to LLM when `is_llm_visible`.
    fn description(&self) -> &'static str;

    /// JSON schema for the skill's input parameters.
    fn input_schema(&self) -> serde_json::Value;

    /// Whether this skill appears in the LLM's tool list.
    ///
    /// `true` (default): the LLM can choose to invoke it.
    /// `false`: the executor invokes it directly as part of the pipeline,
    ///          without an LLM round-trip (e.g. internal classification pipelines).
    fn is_llm_visible(&self) -> bool {
        true
    }

    /// Domain category — used by xamanEK for capability discovery and
    /// composition recommendations.
    fn category(&self) -> SkillCategory;

    /// Execute the skill. Must be deterministic for a given input.
    async fn execute(&self, input: &serde_json::Value, ctx: &ToolContext)
        -> Result<String, String>;
}

/// Domain category for a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCategory {
    /// H3 hexagonal grid, geocoding, AR beacons, spatial grids.
    Spatial,
    /// Monte Carlo simulation, FPL sensitivity analysis.
    Simulation,
    /// GBIF taxonomy, wing segmentation, biological classification.
    Biology,
    /// Onto4MAT formation algorithms, swarm coordination.
    Formation,
    /// SimOps cascade, KPI computation, predictor, optimizer.
    ProcessOptimization,
    /// ODE-based dynamics models (fermentation, pellicle, BC optimization).
    Dynamics,
}

// ─── SkillRegistry ───────────────────────────────────────────────────────────

/// Central registry of all deterministic skills.
///
/// `all()` returns boxed instances — skills are stateless so construction
/// is cheap. The registry is the single source of truth for:
/// - Dispatch: `ToolRegistry::execute()` routes to the matching skill
/// - Schema: `ToolRegistry::to_claude_tools()` includes LLM-visible skills
/// - Validation: `validate_card_skills()` checks card declarations against this list
pub struct SkillRegistry;

// Thin wrappers that delegate to legacy implementations.
// When tools_legacy.rs is split, these wrappers move to skills/ domain files.
struct H3Resolve;
struct Geocode;
struct RunMonteCarlo;
struct RunSensitivityAnalysis;
struct GbifTaxonomyTree;
struct SegmentCreatureWings;
struct ActivateFormation;
struct ScanNearbyCreatures;
struct SimopsCascadeForward;
struct SimopsCascadeBackward;
struct SimopsKpiCompute;
struct SimopsPredictorTrain;
struct SimopsPredictorForecast;
struct SimopsOptimizeScale;
struct SimopsOptimizeSingleInput;
struct ApplyDynamicsModel;
struct ListDynamicsModels;
struct ApplyRheologyModel;

#[async_trait]
impl Skill for H3Resolve {
    fn name(&self) -> &'static str {
        "h3_resolve"
    }
    fn description(&self) -> &'static str {
        "H3 hexagonal grid: gps_to_h3, h3_to_gps, neighbors, distance, grid_disk."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Spatial
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"operation":{"type":"string"},"lat":{"type":"number"},"lng":{"type":"number"},"h3_cell":{"type":"string"},"resolution":{"type":"integer","default":12},"k":{"type":"integer","default":1}},"required":["operation"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("h3_resolve", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for Geocode {
    fn name(&self) -> &'static str {
        "geocode"
    }
    fn description(&self) -> &'static str {
        "Convert address to GPS coordinates via OpenStreetMap Nominatim."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Spatial
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"address":{"type":"string"}},"required":["address"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("geocode", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for RunMonteCarlo {
    fn name(&self) -> &'static str {
        "run_monte_carlo"
    }
    fn description(&self) -> &'static str {
        "Execute an FPL program via Monte Carlo engine. Returns mean, p5/p50/p95, std_dev, histogram."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Simulation
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"program":{"type":"string"},"iterations":{"type":"integer","default":10000}},"required":["program"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("run_monte_carlo", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for RunSensitivityAnalysis {
    fn name(&self) -> &'static str {
        "run_sensitivity_analysis"
    }
    fn description(&self) -> &'static str {
        "Sobol global sensitivity analysis (Saltelli) on an FPL program."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Simulation
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"program":{"type":"string"},"samples":{"type":"integer","default":1000}},"required":["program"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("run_sensitivity_analysis", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for GbifTaxonomyTree {
    fn name(&self) -> &'static str {
        "gbif_taxonomy_tree"
    }
    fn description(&self) -> &'static str {
        "Resolve full GBIF taxonomy tree for a taxon."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Biology
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"taxon_key":{"type":"integer"},"scientific_name":{"type":"string"}}})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("gbif_taxonomy_tree", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for SegmentCreatureWings {
    fn name(&self) -> &'static str {
        "segment_creature_wings"
    }
    fn description(&self) -> &'static str {
        "Segment wing regions from creature image for phenotype analysis."
    }
    fn is_llm_visible(&self) -> bool {
        false
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Biology
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"image_path":{"type":"string"},"creature_id":{"type":"string"}},"required":["image_path"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("segment_creature_wings", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for ActivateFormation {
    fn name(&self) -> &'static str {
        "activate_formation"
    }
    fn description(&self) -> &'static str {
        "Activate an Onto4MAT formation algorithm for a swarm."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Formation
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"swarm_id":{"type":"string"},"algorithm_id":{"type":"string"}},"required":["swarm_id","algorithm_id"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("activate_formation", input, ctx)
            .await
    }
}
#[async_trait]
impl Skill for ScanNearbyCreatures {
    fn name(&self) -> &'static str {
        "scan_nearby_creatures"
    }
    fn description(&self) -> &'static str {
        "H3 proximity scan for creature threat assessment."
    }
    fn is_llm_visible(&self) -> bool {
        false
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::Spatial
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({"type":"object","properties":{"h3_cell":{"type":"string"},"radius_rings":{"type":"integer","default":3}},"required":["h3_cell"]})
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        legacy::ToolRegistry::standard()
            .execute("scan_nearby_creatures", input, ctx)
            .await
    }
}

macro_rules! simops_skill {
    ($struct:ident, $name:literal, $desc:literal) => {
        #[async_trait] impl Skill for $struct {
            fn name(&self) -> &'static str { $name }
            fn description(&self) -> &'static str { $desc }
            fn category(&self) -> SkillCategory { SkillCategory::ProcessOptimization }
            fn input_schema(&self) -> serde_json::Value { json!({"type":"object","properties":{"process_name":{"type":"string"},"process_json":{"type":"object"}}}) }
            async fn execute(&self, input: &serde_json::Value, ctx: &ToolContext) -> Result<String, String> {
                legacy::ToolRegistry::standard().execute($name, input, ctx).await
            }
        }
    }
}

simops_skill!(
    SimopsCascadeForward,
    "simops_cascade_forward",
    "SimOps forward cascade: compute downstream outputs."
);
simops_skill!(
    SimopsCascadeBackward,
    "simops_cascade_backward",
    "SimOps backward cascade: infer inputs to achieve target outputs."
);
simops_skill!(
    SimopsKpiCompute,
    "simops_kpi_compute",
    "Compute KPIs from a batch of process observations."
);
simops_skill!(
    SimopsPredictorTrain,
    "simops_predictor_train",
    "Train a SimOps surrogate predictor from historical observations."
);
simops_skill!(
    SimopsPredictorForecast,
    "simops_predictor_forecast",
    "Forecast outputs using a trained SimOps surrogate predictor."
);
simops_skill!(
    SimopsOptimizeScale,
    "simops_optimize_scale",
    "Scale a process configuration to a new target throughput."
);
simops_skill!(
    SimopsOptimizeSingleInput,
    "simops_optimize_single_input",
    "Optimize a single input variable to hit a target output."
);

#[async_trait]
impl Skill for ApplyDynamicsModel {
    fn name(&self) -> &'static str {
        "apply_dynamics_model"
    }
    fn description(&self) -> &'static str {
        "Run an ODE-based dynamics model (kombucha fermentation, pellicle growth, BC optimization, linear decay) \
         and return trajectories for each state dimension over the requested horizon. \
         Input: model_uri, initial_state (property URIs → values), process_context, \
         params_override, horizon {kind, days}, sample_cadence {hours}."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::ProcessOptimization
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "model_uri": { "type": "string", "description": "e.g. kask:dynamics/kombucha_fermentation@v1" },
                "initial_state": { "type": "object", "description": "Property URIs → initial values" },
                "process_context": { "type": "object", "description": "temperature_c, agitation_rpm, etc." },
                "params_override": { "type": "object", "description": "Override default model parameters" },
                "horizon": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string", "enum": ["fixed", "until_property_reaches"] },
                        "days": { "type": "number" }
                    },
                    "required": ["kind"]
                },
                "sample_cadence": {
                    "type": "object",
                    "properties": { "hours": { "type": "number" } }
                },
                "generated_by": { "type": "string" }
            },
            "required": ["model_uri", "initial_state", "horizon"]
        })
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<String, String> {
        let skill_input: dynamics::SkillInput = serde_json::from_value(input.clone())
            .map_err(|e| format!("Invalid apply_dynamics_model input: {e}"))?;
        let output = dynamics::apply_dynamics_model(skill_input)?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl Skill for ApplyRheologyModel {
    fn name(&self) -> &'static str {
        "apply_rheology_model"
    }
    fn description(&self) -> &'static str {
        "Compute instantaneous fluid rheology (viscosity, flow index, regime) for an algae \
         suspension at given operating conditions. Power-law model with Arrhenius temperature \
         dependence. Input: model_uri, temperature_c, shear_rate_per_s, volume_fraction, \
         optional params_override. No time integration — single operating point."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::ProcessOptimization
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "model_uri": { "type": "string", "default": "kask:rheology/algae_viscosity@v1" },
                "temperature_c": { "type": "number", "description": "Fluid temperature in °C" },
                "shear_rate_per_s": { "type": "number", "description": "Shear rate in s⁻¹" },
                "volume_fraction": { "type": "number", "description": "Algae volume fraction 0–1 (e.g. 0.15 = 15%)" },
                "params_override": { "type": "object", "description": "Override k0, ea, c_n, n_min, density_kg_m3" }
            },
            "required": ["temperature_c", "shear_rate_per_s", "volume_fraction"]
        })
    }
    async fn execute(
        &self,
        input: &serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<String, String> {
        let uri = input
            .get("model_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("kask:rheology/algae_viscosity@v1");
        let model = dynamics::resolve_rheology(uri)
            .ok_or_else(|| format!("Unknown rheology model URI: {uri}"))?;
        let rheology_input: dynamics::RheologyInput = serde_json::from_value(input.clone())
            .map_err(|e| format!("Invalid apply_rheology_model input: {e}"))?;
        let output = model.compute(&rheology_input)?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl Skill for ListDynamicsModels {
    fn name(&self) -> &'static str {
        "list_dynamics_models"
    }
    fn description(&self) -> &'static str {
        "List all available ODE dynamics models with their manifests: \
         URI, applies_to_set (property URIs), params_schema, context_schema. \
         Use to discover which model covers a given set of sensor property URIs."
    }
    fn category(&self) -> SkillCategory {
        SkillCategory::ProcessOptimization
    }
    fn input_schema(&self) -> serde_json::Value {
        json!({ "type": "object", "properties": {} })
    }
    async fn execute(
        &self,
        _input: &serde_json::Value,
        _ctx: &ToolContext,
    ) -> Result<String, String> {
        let manifests = dynamics::registry::list_manifests();
        serde_json::to_string(&manifests).map_err(|e| e.to_string())
    }
}

impl SkillRegistry {
    /// All registered skills. Extend this list to add new skills platform-wide.
    pub fn all() -> Vec<Box<dyn Skill>> {
        vec![
            Box::new(H3Resolve),
            Box::new(Geocode),
            Box::new(ScanNearbyCreatures),
            Box::new(RunMonteCarlo),
            Box::new(RunSensitivityAnalysis),
            Box::new(GbifTaxonomyTree),
            Box::new(SegmentCreatureWings),
            Box::new(ActivateFormation),
            Box::new(SimopsCascadeForward),
            Box::new(SimopsCascadeBackward),
            Box::new(SimopsKpiCompute),
            Box::new(SimopsPredictorTrain),
            Box::new(SimopsPredictorForecast),
            Box::new(SimopsOptimizeScale),
            Box::new(SimopsOptimizeSingleInput),
            // ── Dynamics (ODE-based time evolution) ──────────────────
            Box::new(ApplyDynamicsModel),
            Box::new(ListDynamicsModels),
            // ── Rheology (instantaneous fluid properties) ─────────────
            Box::new(ApplyRheologyModel),
        ]
    }

    /// Find a skill by name.
    pub fn find(name: &str) -> Option<Box<dyn Skill>> {
        Self::all().into_iter().find(|s| s.name() == name)
    }

    /// All registered skill names — used by validate_card_skills and the catalogue.
    pub fn names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Self::all().iter().map(|s| s.name()).collect();
        names.sort_unstable();
        names
    }

    /// Skills by category — used by xamanEK for capability discovery.
    pub fn by_category(category: SkillCategory) -> Vec<Box<dyn Skill>> {
        Self::all()
            .into_iter()
            .filter(|s| s.category() == category)
            .collect()
    }
}

// ─── Skill validation ────────────────────────────────────────────────────────

/// Return any skill labels on a card that match a registered **executable** skill name.
///
/// # Skill label taxonomy
///
/// `capabilities.skills` on an agent card serves **two purposes**:
///
/// 1. **Taxonomy labels** — free-text domain descriptions like `"market-analysis"`,
///    `"coherence-analysis"`, `"sentiment-detection"`. These are read by xamanEK for
///    discovery and composition recommendations. They do NOT need to be in the
///    SkillRegistry — they are human-readable capability descriptions.
///
/// 2. **Executable skill names** — names like `"h3_resolve"`, `"run_monte_carlo"`,
///    `"simops_cascade_forward"`. These MUST be in the SkillRegistry. When present
///    on a card, the executor can invoke them directly by name.
///
/// This function returns the executable (registry-matched) skills for a card.
/// Use it to build the runtime skill set at execution time.
pub fn validate_card_skills(card: &AgentCard) -> Vec<String> {
    let registered: std::collections::HashSet<&'static str> =
        SkillRegistry::names().into_iter().collect();
    card.capabilities
        .skills
        .iter()
        .filter(|label| registered.contains(label.as_str()))
        .cloned()
        .collect()
}
