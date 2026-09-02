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
//! - Add a platform tool: implement `PlatformTool` in the appropriate domain
//!   module under `tools/domains/`, add to that module's `tools()` vec.
//! - Add an MCP integration: declare on the agent card; dispatch happens via
//!   `ctx.remote_mcp` in `PlatformToolRegistry::execute()`.
//! - Add a skill: `impl Skill` + register in `SkillRegistry::all()`.
//!   No touching the other categories.
//!
//! # File layout (Phase 4 complete)
//!
//! ```text
//! tools/
//! ├── mod.rs          This file — Skill trait, SkillRegistry, utility functions
//! ├── context.rs      ToolContext and EvalTrigger
//! ├── helpers.rs      Shared helpers (resolve_agent_id, parse_uuid_field)
//! ├── platform_tool.rs  PlatformTool trait, ToolCategory, ToolCatalogueEntry
//! ├── registry.rs     PlatformToolRegistry (the dispatch path since Phase 3)
//! └── domains/        One module per tool domain (101 tools across 17 modules)
//! ```
//!
//! `tools_legacy.rs` has been deleted (Phase 4). Adding a new tool means:
//! 1. Create a zero-size struct in the appropriate domain module
//! 2. `impl PlatformTool` for it
//! 3. Add it to that module's `tools()` vec
//! No other file needs to change.
//!
//! See: `docs/AGENT_MODEL.md`, `docs/plans/TOOL_REGISTRY_REFACTOR.md`

// Phase 4 complete: tools_legacy.rs has been deleted.
// All tool implementations now live in domain modules under tools/domains/.
// Tool execution goes through PlatformToolRegistry (tools/registry.rs).
// ToolContext and EvalTrigger are in tools/context.rs.

// The one place a `workspace_intentions` row is written, re-exported for
// `crate::plan_solicitation`.
//
// Crate-visible rather than `pub`: an intention row carries a `source` the
// platform vouches for, and a writer reachable from outside the crate is a
// writer whose provenance argument nobody checked. The floor needs it because
// it runs from an HTTP handler that builds no `ToolContext`, and giving that
// path its own INSERT would be a second answer to "what is an intention row" —
// the duplication §3.4 exists to forbid, on the field whose whole purpose is
// that it cannot be forged.
pub(crate) use domains::coordination::write_intention;

// ─── Platform tool registry (new) ──────────────────────────────────────────
//
// Phase 0 / Phase 1 of the tool-registry migration.
// See docs/plans/TOOL_REGISTRY_REFACTOR.md for the full plan.
//
// The new types live alongside the legacy ones throughout the migration.

pub mod context;
mod domains;
pub(crate) mod helpers;
pub mod platform_tool;
pub mod registry;

use std::sync::Arc;

pub use context::{EvalTrigger, ToolContext};
pub use platform_tool::{PlatformTool, ToolCatalogueEntry, ToolCategory};
pub use registry::PlatformToolRegistry;

/// All migrated platform tools, in registration order.
///
/// Phase 1: returns an empty vec. Grows in Phase 2 as domain modules are added.
/// The `platform_tool_names_are_unique` test enforces the uniqueness invariant
/// across the full set on every `cargo test` run.
pub fn all_tools() -> Vec<Arc<dyn PlatformTool>> {
    domains::all_tools()
}

/// Metadata struct for a builtin tool declaration.
///
/// Used by `weather_tools::tool_defs()` to self-describe weather tool schemas
/// for backward compatibility. New tools use `PlatformTool` impls instead.
/// See `docs/plans/TOOL_REGISTRY_REFACTOR.md`.
pub struct BuiltinToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub requires_workspace: bool,
    pub is_delegation: bool,
}

impl Default for BuiltinToolDef {
    fn default() -> Self {
        Self {
            name: "",
            description: "",
            input_schema: serde_json::Value::Object(Default::default()),
            requires_workspace: false,
            is_delegation: false,
        }
    }
}

// ─── Phase 4 replacements — PlatformRegistry-backed ────────────────────────
//
// These functions provide the same API as the legacy re-exports they replaced
// but are backed by PlatformToolRegistry / all_tools(). The legacy re-exports
// for these items have been removed above. Once tools_legacy.rs is deleted,
// these become the sole implementations.

/// All registered platform tool names, sorted for card validation.
///
/// Replaces `legacy::platform_tool_names()`.
pub fn platform_tool_names() -> Vec<&'static str> {
    let mut names = PlatformToolRegistry::all().tool_names();
    names.sort_unstable();
    names
}

/// All registered tool names the runtime can actually dispatch.
///
/// Use this to answer "will this call succeed". Distinct from
/// `platform_tool_names` which is used for card validation.
pub fn dispatchable_tool_names() -> Vec<&'static str> {
    platform_tool_names()
}

/// All tool names and descriptions — for the contract builder.
///
/// Returns `(name, description)` pairs.
/// Replaces `legacy::builtin_tool_catalogue()`.
pub fn builtin_tool_catalogue() -> Vec<(&'static str, &'static str)> {
    all_tools()
        .into_iter()
        .map(|t| (t.name(), t.description()))
        .collect()
}

// ─── Card validation ─────────────────────────────────────────────────────────

/// Why a declared tool name can't be published.
///
/// Moved from `tools_legacy.rs` in Phase 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolDeclarationError {
    /// No dispatch arm and no declared remote server owns the namespace.
    NotDispatchable,
    /// Namespaced like a remote MCP tool, but the agent declares no server
    /// by that name — so nothing would resolve it.
    UnknownRemoteServer { server: String },
}

impl std::fmt::Display for ToolDeclarationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotDispatchable => write!(
                f,
                "no platform tool by this name (would be advertised to the model and then fail \
                 with 'Unknown tool')"
            ),
            Self::UnknownRemoteServer { server } => write!(
                f,
                "looks like a remote MCP tool, but this agent declares no server named '{server}'"
            ),
        }
    }
}

/// Validate the tool names an agent wants to publish against the registry.
///
/// A name is publishable if it is either a registered platform tool, or a
/// remote MCP tool (`server__tool`) belonging to a server the agent declares.
/// Returns the names that would be phantom tools, each with a reason.
///
/// From `docs/plans/TOOL_REGISTRY_REFACTOR.md §2.6`.
pub fn validate_card_tools(
    declared: &[String],
    declared_servers: &[crate::agent_backend::mcp_client::RemoteMcpServer],
) -> Vec<(String, ToolDeclarationError)> {
    let registry = PlatformToolRegistry::all();
    declared
        .iter()
        .filter_map(|name| {
            if registry.tool(name).is_some() {
                return None; // known platform tool
            }
            match name.split_once(crate::agent_backend::mcp_client::NS_SEP) {
                Some((ns, _)) if !ns.is_empty() => {
                    let known = declared_servers.iter().any(|s| s.namespace() == ns);
                    if known {
                        None
                    } else {
                        Some((
                            name.clone(),
                            ToolDeclarationError::UnknownRemoteServer {
                                server: ns.to_string(),
                            },
                        ))
                    }
                }
                _ => Some((name.clone(), ToolDeclarationError::NotDispatchable)),
            }
        })
        .collect()
}

/// Validate tool names — backward-compat alias for `validate_card_tools`.
///
/// Call sites that used `invalid_tool_declarations` continue to work unchanged.
pub fn invalid_tool_declarations(
    declared: &[String],
    declared_servers: &[crate::agent_backend::mcp_client::RemoteMcpServer],
) -> Vec<(String, ToolDeclarationError)> {
    validate_card_tools(declared, declared_servers)
}

/// Whether a tool's input schema accepts an `endpoint` parameter.
///
/// Replaces `legacy::tool_takes_endpoint()`.
pub fn tool_takes_endpoint(tool: &str) -> bool {
    PlatformToolRegistry::all()
        .tool(tool)
        .map(|t| {
            t.input_schema()
                .get("properties")
                .and_then(|p| p.get("endpoint"))
                .is_some()
        })
        .unwrap_or(false)
}

/// Tools that can be executed without a `ToolContext`.
///
/// Replaces `legacy::CONTEXT_FREE_TOOLS`.
pub const CONTEXT_FREE_TOOLS: &[&str] = &[
    "call_football_api",
    "gbif_species_search",
    "gbif_taxonomy_tree",
    "inat_observations",
    "mycobank_lookup",
    "ncbi_genome_search",
    "web_search",
];

/// Whether `execute_context_free` can dispatch this tool.
///
/// Replaces `legacy::is_context_free()`.
pub fn is_context_free(tool: &str) -> bool {
    CONTEXT_FREE_TOOLS.contains(&tool) || crate::agent_backend::weather_tools::handles(tool)
}

/// Dispatch a context-free tool call.
///
/// Context-free tools make HTTP calls or pure computations — they don't
/// need a `ToolContext`. Replaces `legacy::execute_context_free()`.
pub async fn execute_context_free(tool: &str, input: &serde_json::Value) -> Result<String, String> {
    match tool {
        "call_football_api" => domains::sports::execute_call_football_api(input).await,
        "ncbi_genome_search" => {
            crate::agent_backend::ncbi_tools::execute_ncbi_genome_search(input).await
        }
        "web_search" => domains::platform::execute_web_search(input).await,
        "gbif_species_search" => domains::biology::execute_gbif_species_search(input).await,
        "gbif_taxonomy_tree" => domains::biology::execute_gbif_taxonomy_tree(input).await,
        "inat_observations" => domains::biology::execute_inat_observations(input).await,
        "mycobank_lookup" => domains::biology::execute_mycobank_lookup(input).await,
        name if crate::agent_backend::weather_tools::handles(name) => {
            crate::agent_backend::weather_tools::dispatch(name, input)
                .await
                .unwrap_or_else(|| Err(format!("Unknown weather tool: {name}")))
        }
        _ => Err(format!("Not a context-free tool: {tool}")),
    }
}

use crate::agent_backend::agent_card::AgentCard;
use ::dynamics;
use async_trait::async_trait;
/// Re-export biology's gbif_species_search for handlers that use it directly.
/// Re-exported for handlers that call these tools directly without a ToolContext.
pub use domains::biology::execute_gbif_species_search;
pub use domains::biology::execute_mycobank_lookup;
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
/// `text`, not `rust`: this is the SHAPE of an implementation, with `{...}` and
/// `...` where the body goes. Tagged `rust` it was compiled as a doctest and
/// had failed for as long as it has existed — six errors about `async_trait`,
/// `SkillCategory` and `json!` not being in scope, none of which is the
/// example's point. A doctest that cannot pass is a test nobody can act on;
/// see `src/distributions.rs` for the other resolution, where the examples
/// were real code and were given their imports instead.
///
/// ```text
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
        PlatformToolRegistry::standard()
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
                PlatformToolRegistry::standard().execute($name, input, ctx).await
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

// ─── Phase 1 invariant tests ──────────────────────────────────────────────────
//
// These tests enforce structural correctness across the full registered set.
// They run on every `cargo test` and grow to cover the real tool count once
// Phase 2 domain modules are added.

#[cfg(test)]
mod platform_registry_tests {
    use super::*;

    /// Every name returned by all_tools() is unique.
    ///
    /// A duplicate would cause one tool to shadow another in the registry
    /// HashMap with no error at runtime. This catches it at test time.
    #[test]
    fn platform_tool_names_are_unique() {
        let tools = all_tools();
        let mut seen = std::collections::HashSet::new();
        for tool in &tools {
            assert!(
                seen.insert(tool.name()),
                "Duplicate tool name: {}",
                tool.name()
            );
        }
    }

    /// The three registry constructors all build from the same tool set,
    /// filtered. In Phase 1 all three are empty; the test confirms the
    /// constructors compile and run without panicking.
    #[test]
    fn registry_constructors_are_coherent() {
        let all = PlatformToolRegistry::all();
        let std_ = PlatformToolRegistry::standard();
        let wnd = PlatformToolRegistry::workspace_no_delegation();

        // Standard ⊆ workspace_no_delegation ⊆ all  (by name set)
        for name in std_.tool_names() {
            assert!(
                all.tool(name).is_some(),
                "standard tool '{name}' missing from all-registry"
            );
        }
        for name in wnd.tool_names() {
            assert!(
                all.tool(name).is_some(),
                "workspace_no_delegation tool '{name}' missing from all-registry"
            );
        }
    }

    /// catalogue() returns one entry per registered tool.
    #[test]
    fn catalogue_entry_count_matches_tool_count() {
        assert_eq!(
            PlatformToolRegistry::all_tools_catalogue().len(),
            all_tools().len(),
            "catalogue entry count does not match all_tools() count"
        );
    }

    /// Phase 3 dispatch invariant: every tool name registered in
    /// `PlatformToolRegistry::all()` can be looked up by name.
    ///
    /// This is the static side of the integration test from
    /// docs/plans/TOOL_REGISTRY_REFACTOR.md §3 (Phase 3). The async
    /// dispatch path (that every registered tool returns Ok or a
    /// credential/workspace error rather than "Unknown tool") requires
    /// a live execution context and is tested by the dispatch delegate
    /// in each domain module's `execute()` body.
    #[test]
    fn registry_dispatches_every_registered_tool_name() {
        let registry = PlatformToolRegistry::all();
        for tool in all_tools() {
            assert!(
                registry.tool(tool.name()).is_some(),
                "PlatformToolRegistry::all() could not look up registered tool: {}",
                tool.name()
            );
        }
    }

    /// Delegation tools (execute_agent, delegate_to_agent, solicit_agent_plan)
    /// must be present in all() but absent from workspace_no_delegation().
    #[test]
    fn delegation_tools_excluded_from_no_delegation_registry() {
        let all = PlatformToolRegistry::all();
        let wnd = PlatformToolRegistry::workspace_no_delegation();
        let delegation_tools = ["execute_agent", "delegate_to_agent", "solicit_agent_plan"];
        for name in delegation_tools {
            assert!(
                all.tool(name).is_some(),
                "Expected delegation tool '{name}' in all() registry"
            );
            assert!(
                wnd.tool(name).is_none(),
                "Delegation tool '{name}' must not appear in workspace_no_delegation()"
            );
        }
    }

    /// Workspace-only tools must not appear in the standard() registry.
    /// Standard registry is for single-turn agents without workspace context.
    #[test]
    fn workspace_tools_excluded_from_standard_registry() {
        let std_ = PlatformToolRegistry::standard();
        let workspace_tools = [
            "write_workspace_file",
            "read_workspace_file",
            "get_workspace_messages",
            "list_workspace_agents",
            "evaluate_coherence",
            "coherence_snapshot",
            "delegate_to_agent",
        ];
        for name in workspace_tools {
            assert!(
                std_.tool(name).is_none(),
                "Workspace tool '{name}' must not appear in standard() registry"
            );
        }
    }
}
