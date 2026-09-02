// src/agent_backend/tools/platform_tool.rs
//
// Phase 0 / Phase 1 of the tool-registry migration.
//
// This file is the full PlatformTool trait design — including response_shape()
// — locked in before Phase 2 begins. The reason `response_shape()` belongs here
// rather than in a side table is given in the companion document:
//
//   docs/plans/TOOL_REGISTRY_REFACTOR.md §2.1.1
//
// Short version: Phase 2 touches every tool exactly once; adding response_shape()
// to the trait means filling it costs one method next to input_schema(), where
// the person editing the tool is already sitting. Without it, the side table gets
// filled in a second pass that does not happen.

use async_trait::async_trait;
use serde_json::Value;

use super::ToolContext;
use crate::tool_response_shapes::ToolResponse;

/// Domain category for grouping tools in the UI and agent discovery.
///
/// Every variant maps to a display label returned by `label()`. The serialised
/// form (via `serde::Serialize`) matches that label exactly so the API response
/// and the Rust enum use identical strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum ToolCategory {
    /// Core platform: search_knowledge, query_ontology, execute_agent,
    /// list_agents, web_search, delegate_to_agent, validate_agent_card,
    /// build_output_contract.
    Platform,
    /// Workspace-scoped: read/write_workspace_file, get_workspace_messages,
    /// list_workspace_agents, list_workspace_outputs, read_workspace_output.
    Workspace,
    /// Coordination loop: declare_intention, solicit_agent_plan,
    /// check_conflicts, get_intention_map, clear_intention,
    /// suggest_differentiation, emit_coherence_signal,
    /// propose_composition_change, record_coordination_observation.
    Coordination,
    /// Observability: query_eval_signals, query_eval_runs, query_anomalies,
    /// query_hitl_queue, query_timeline, query_dyad_state, classify_anomaly,
    /// route_to_hitl, run_evaluator_registry, get_agent_calibration.
    Observability,
    /// Financial: fmp_company_profile, fmp_income_statement, fmp_balance_sheet,
    /// fmp_cash_flow, fmp_ratios, fmp_key_metrics, fmp_dcf,
    /// fmp_analyst_estimates, fmp_historical_price.
    Financial,
    /// Prediction markets: polymarket_search, polymarket_event.
    PredictionMarket,
    /// Weather: weather_settlement_spec, weather_ensemble_forecast,
    /// weather_climatology, weather_dispersion_fit, weather_station_observation,
    /// weather_portfolio_risk, polymarket_weather_markets, polymarket_orderbook.
    Weather,
    /// SimOps: simops_cascade_forward/backward, simops_kpi_compute,
    /// simops_predictor_train/forecast, simops_optimize_scale/single_input,
    /// simops_load_process, simops_write_observation, simops_fetch_training_data,
    /// get_observations, describe_session, simops_check_constraints,
    /// simops_write_actuation_plan.
    SimOps,
    /// FPL / Monte Carlo: fermi_execute_fpl, fermi_sensitivity_analysis,
    /// run_monte_carlo, run_sensitivity_analysis.
    Simulation,
    /// Spatial: h3_resolve, geocode, create_beacon, query_beacons, save_grid_map.
    Spatial,
    /// Biology / Naturalist: gbif_species_search, gbif_taxonomy_tree,
    /// inat_observations, mycobank_lookup, ncbi_genome_search,
    /// generate_specimen_art, segment_creature_wings.
    Biology,
    /// Rabble / Creature / AR: mint_creature, activate_formation,
    /// scan_nearby_creatures.
    Rabble,
    /// Media generation: generate_image, edit_image, speak_text.
    Media,
    /// Shopping / Marketplace: get_shopping_profile, update_shopping_profile,
    /// list_marketplace, create_listing.
    Marketplace,
    /// Reduct video: reduct_list_projects, reduct_get_project,
    /// reduct_get_transcript, reduct_create_reel, reduct_add_block.
    Video,
    /// Card / contract authoring: validate_agent_card, build_output_contract.
    Authoring,
    /// Football API: call_football_api.
    Sports,
}

impl ToolCategory {
    /// The canonical display label, matching the JSON serialisation.
    pub fn label(self) -> &'static str {
        match self {
            ToolCategory::Platform => "Platform",
            ToolCategory::Workspace => "Workspace",
            ToolCategory::Coordination => "Coordination",
            ToolCategory::Observability => "Observability",
            ToolCategory::Financial => "Financial",
            ToolCategory::PredictionMarket => "PredictionMarket",
            ToolCategory::Weather => "Weather",
            ToolCategory::SimOps => "SimOps",
            ToolCategory::Simulation => "Simulation",
            ToolCategory::Spatial => "Spatial",
            ToolCategory::Biology => "Biology",
            ToolCategory::Rabble => "Rabble",
            ToolCategory::Media => "Media",
            ToolCategory::Marketplace => "Marketplace",
            ToolCategory::Video => "Video",
            ToolCategory::Authoring => "Authoring",
            ToolCategory::Sports => "Sports",
        }
    }
}

/// The contract every platform tool implements.
///
/// Replaces `BuiltinToolDef` (a data struct dispatched by a `match`) with a
/// trait that carries both metadata and execution in one place. The migration
/// proceeds domain by domain; the legacy `ToolRegistry::execute` match remains
/// the primary dispatch path until Phase 3.
///
/// **Invariants (see docs/plans/TOOL_REGISTRY_REFACTOR.md §4):**
/// - `name()` must return the exact string that exists in `BuiltinToolDef.name`
///   and in the `ToolRegistry::execute` match. Agent cards stored in the DB name
///   tools by these strings. Any rename breaks stored cards.
/// - `execute()` must not alter return values, error messages, or side effects
///   relative to the legacy function it replaces. The refactor is structural.
#[async_trait]
pub trait PlatformTool: Send + Sync {
    /// Stable name — must match the string in card `mcp_tools` declarations
    /// and in the `ToolRegistry::execute` arm. Rename only with a deprecation.
    fn name(&self) -> &'static str;

    /// Human-readable description injected into the LLM tool schema.
    fn description(&self) -> &'static str;

    /// JSON Schema for the tool's input object.
    /// Must be `{"type": "object", "properties": {...}, "required": [...]}`.
    fn input_schema(&self) -> Value;

    /// Domain category. Used for UI grouping and xaman_ek discovery.
    fn category(&self) -> ToolCategory;

    /// Whether to expose this tool in the LLM's tool list.
    /// Default `true`. Set `false` for infrastructure tools invoked only by code.
    fn is_llm_visible(&self) -> bool {
        true
    }

    /// Whether this tool requires a workspace context (`ctx.workspace_id.is_some()`).
    /// Default `false`. The registry enforces this guard before calling `execute`.
    fn requires_workspace(&self) -> bool {
        false
    }

    /// Whether this tool invokes another agent (execute_agent, delegate_to_agent).
    /// Used by registry constructors to filter delegation tools out of recursive
    /// execution contexts.
    fn is_delegation(&self) -> bool {
        false
    }

    /// The credential key this tool requires in `ctx.user_secrets`, if any.
    /// `None` means no credential is required beyond the standard context.
    /// Example: `Some("FMP_API_KEY")`.
    fn required_credential(&self) -> Option<&'static str> {
        None
    }

    /// **What this tool returns**, for contract authoring. `None` = nobody has
    /// read it — which is not the same as an empty response.
    ///
    /// A declared shape lets an author select fields from a list of things that
    /// actually exist rather than typing a plausible key from memory. An absent
    /// tool falls back to noun extraction from its description prose, marked
    /// `unconfirmed` — honest labelling of a weaker method.
    ///
    /// See: `src/tool_response_shapes.rs` for the `ToolResponse` type and
    /// `docs/plans/TOOL_REGISTRY_REFACTOR.md §2.1.1` for the rationale.
    fn response_shape(&self) -> Option<&'static ToolResponse> {
        None
    }

    /// Execute the tool. Called only after workspace and credential checks pass.
    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String>;
}

/// Flat catalogue entry serialised for the UI and card-authoring endpoints.
///
/// Constructed by `PlatformToolRegistry::all_tools_catalogue()`. The `category`
/// field serialises to a `"PascalCase"` string matching `ToolCategory::label()`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCatalogueEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ToolCategory,
    pub requires_workspace: bool,
    pub is_llm_visible: bool,
    pub required_credential: Option<&'static str>,
}
