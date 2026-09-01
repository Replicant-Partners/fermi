# Tool Registry Refactor — Requirements & Design

**Status:** Planned  
**Owner:** Backend  
**Scope:** `src/agent_backend/tools_legacy.rs`, `src/agent_backend/tools/`, `src/agent_backend/weather_tools.rs`, `src/agent_backend/simops_tools.rs`

---

## 0. Problem Statement

The platform has ~98 server-side tools. Their dispatch and metadata live in two tightly coupled, manually maintained structures inside a single 8,000+ line file named `tools_legacy.rs`:

1. **A monolithic `match` arm** in `ToolRegistry::execute()` — every tool requires a new arm added by hand.
2. **A parallel `Vec<BuiltinToolDef>`** in `builtin_tools_core()` — every tool requires a separate hand-written JSON schema entry.

These two structures must stay in sync. When they diverge, you get **phantom tools**: names the model is told it can call, that return `Unknown tool: X` at runtime. The `ARMS_WITHOUT_DEFS` constant exists to name a known past instance of this gap (`equity_analyst`'s nine `fmp_*` tools). The file is called `legacy` because this was known to be the wrong design from the start.

A correct design already exists in the codebase — the `Skill` trait in `src/agent_backend/tools/mod.rs` — but it covers only ~18 tools and those tools' `execute()` implementations delegate back to `ToolRegistry::execute()` via the legacy path. The goal of this refactor is to complete the migration: make the `Skill` / `Tool` trait the single authoritative pattern, delete the `match`, and split the monolith into domain modules.

### What is not changing

- `ToolContext` — unchanged; all tools receive the same context struct.
- `EvalTrigger` trait — unchanged.
- Card format (`mcp_tools` allowlist in `agent_card.json`) — unchanged; cards still declare which tools they expose.
- `invalid_tool_declarations()` validation logic — adapted to query the new registry, not the old `platform_tool_names()` list.
- Remote MCP fallthrough in dispatch — preserved exactly.
- Behaviour of every individual tool — this is a structural refactor, not a behaviour change.
- `ToolAwareExecutor` — minimal changes only to call the new dispatch path.
- Gate system (`gate_trust.rs`, `CoherenceGate`, `grounding_trust`) — untouched.

---

## 1. Current State (read before touching anything)

### 1.1 File map

```
src/agent_backend/
  tools_legacy.rs          ← THE MONOLITH. ~8000 lines.
                             Contains: BuiltinToolDef, ToolContext, EvalTrigger,
                             ToolRegistry (match + vec), all execute_* functions.
  tool_executor.rs         ← ToolAwareExecutor. Calls ToolRegistry::execute().
  weather_tools.rs         ← Standalone module with handles()/tool_defs()/dispatch().
                             Already the right pattern. Used as reference.
  simops_tools.rs          ← SimOps execute_* functions. Dispatch arms in match.
  ncbi_tools.rs            ← NCBI execute_*. Dispatch arms in match.
  tools/
    mod.rs                 ← Re-exports legacy types + defines Skill trait (v1)
                             and SkillRegistry (~18 tools). These tools'
                             execute() delegates back to legacy match. CIRCULAR.
    skills/
      mod.rs               ← Second Skill trait definition (near-duplicate of
                             tools/mod.rs Skill). Resolve duplication.
```

### 1.2 `BuiltinToolDef` (current metadata struct)

```rust
pub struct BuiltinToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
    pub requires_workspace: bool,
    pub is_delegation: bool,   // execute_agent, delegate_to_agent
}
```

### 1.3 `Skill` trait (current, in `tools/mod.rs`)

```rust
pub trait Skill: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
    fn is_llm_visible(&self) -> bool { true }
    fn category(&self) -> SkillCategory;
    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String>;
}
```

### 1.4 `SkillCategory` (current)

```rust
pub enum SkillCategory {
    Spatial, Simulation, Biology, Formation, ProcessOptimization, Dynamics,
}
```

### 1.5 `ToolRegistry` constructors (current)

```rust
ToolRegistry::standard()                  // no workspace tools
ToolRegistry::with_workspace()            // all tools
ToolRegistry::with_workspace_no_delegation() // workspace tools minus execute/delegate_to_agent
```

### 1.6 Weather tools — the reference pattern

`weather_tools.rs` already does this correctly:
- `pub fn tool_defs() -> Vec<BuiltinToolDef>` — metadata, registered into the global `builtin_tools()`.
- `pub fn handles(name: &str) -> bool` — cheap name check.
- `pub async fn dispatch(name: &str, input: &Value) -> Option<Result<String, String>>` — actual dispatch.

The legacy match has one arm that calls this:
```rust
name if crate::agent_backend::weather_tools::handles(name) => {
    match crate::agent_backend::weather_tools::dispatch(name, input).await { ... }
}
```

This is the pattern to generalise.

---

## 2. Target Design

### 2.1 The `PlatformTool` trait

Replace `BuiltinToolDef` + the `Skill` trait with a single unified trait. Name it `PlatformTool` to avoid collision with the existing `Skill` during migration.

```rust
// src/agent_backend/tools/platform_tool.rs

use async_trait::async_trait;
use serde_json::Value;
use crate::agent_backend::tools_legacy::ToolContext;  // unchanged

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    /// Core platform: search_knowledge, query_ontology, execute_agent,
    /// list_agents, web_search, delegate_to_agent.
    Platform,
    /// Workspace-scoped: read/write_workspace_file, get_workspace_messages,
    /// list_workspace_agents, list_workspace_outputs, read_workspace_output.
    Workspace,
    /// Coordination loop: declare_intention, check_conflicts, get_intention_map,
    /// clear_intention, suggest_differentiation, emit_coherence_signal,
    /// solicit_agent_plan, propose_composition_change,
    /// record_coordination_observation.
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

#[async_trait]
pub trait PlatformTool: Send + Sync {
    /// Stable name — must match the string in card `mcp_tools` declarations.
    fn name(&self) -> &'static str;

    /// Human-readable description injected into LLM tool schema.
    fn description(&self) -> &'static str;

    /// JSON Schema for the tool's input object.
    /// Must be `{"type": "object", "properties": {...}, "required": [...]}`.
    fn input_schema(&self) -> Value;

    /// Domain category. Used for UI grouping and xaman_ek discovery.
    fn category(&self) -> ToolCategory;

    /// Whether to expose this tool in the LLM's tool list.
    /// Default true. Set false for infrastructure tools called only by code.
    fn is_llm_visible(&self) -> bool { true }

    /// Whether this tool requires a workspace context (`ctx.workspace_id.is_some()`).
    /// Default false. Registry enforces this before calling execute().
    fn requires_workspace(&self) -> bool { false }

    /// Whether this tool invokes another agent (execute_agent, delegate_to_agent).
    /// Used by ToolRegistry constructors to filter delegation tools out of
    /// recursive execution contexts.
    fn is_delegation(&self) -> bool { false }

    /// The credential key this tool requires in `ctx.user_secrets`, if any.
    /// Example: `Some("FMP_API_KEY")`, `Some("BRAVE_SEARCH_API_KEY")`.
    /// `None` means no credential required beyond the standard context.
    fn required_credential(&self) -> Option<&'static str> { None }

    /// **What this tool returns**, for contract authoring. `None` = nobody has
    /// read it, which is not the same as an empty response.
    ///
    /// Added to this design after the fact — see §2.1.1, which is the reason
    /// this refactor is worth doing now rather than later.
    fn response_shape(&self) -> Option<&'static ToolResponse> { None }

    /// Execute the tool. Called only after workspace and credential checks pass.
    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String>;
}
```

### 2.1.1 `response_shape()` — why this refactor should go now

**Amended 2026-09-01, answering "should the tool registry refactor happen now or
wait".** It should happen now, and this method is the reason.

The requirement that prompted the question: *when I define an agent and give it
tools, I should be able to just select the fields from the tools.* That is the
right instinct and it is the whole ballgame, because a field selected from a
tool's declared response is **correct by construction** — there is no prose to
parse and no heuristic to get wrong.

The alternative is what exists. `grounding_trust::FIELD_CONTRACTS` names the
source of each field in a hand-written `response_field` string:

```text
standings (rank, points, form, home/away splits)   container + key names
fixtures/headtohead                                endpoint only
fixtures/statistics.expected_goals                 endpoint + one leaf
estimated_size_mb (assembly total_length)           a key name, not an endpoint
```

Four shapes, one grammar, no schema. Reading it took a parser
(`field_probe::parse_hint`), and even then the head of `estimated_size_mb
(assembly total_length)` is indistinguishable from the head of `standings (rank,
points)` — one is a field name, the other an endpoint — so a second function
(`tool_takes_endpoint`) had to ask the tool's *input* schema which kind it was.
All of that is a **reader for legacy prose**. It should not be needed to author a
new contract, and with `response_shape()` it is not.

#### The table already exists, and its own header says why this method should replace it

`src/tool_response_shapes.rs` was written to answer exactly this — *"wouldn't the
tool determine which fields are available on a sourced thing?"* — and it is
right in every respect except where it lives. It carries `Evidence::Constructed`
vs `Evidence::Vendor` (a `json!` literal in this repo vs a vendor passthrough
that can change without us noticing), and absence means *unread* rather than
*empty*. Keep all of that.

Its header gives two reasons for being a side table rather than a field on
`BuiltinToolDef`:

> This is *contract-authoring* metadata, not tool-dispatch metadata … And
> practically, several `BuiltinToolDef` literals spell out every field rather
> than using `..Default::default()`, so adding one would touch a hundred
> definitions in a file two sessions are editing.

The first reason survives and is honoured by a **defaulted** method: nothing in
dispatch reads it, and `None` is the default, so no impl is obliged to have one.

**The second reason is a description of the problem this refactor removes.**
After it, a tool is a trait impl in a domain module, not a literal in a
contested 8,000-line file. Declaring a response costs one method next to
`input_schema`, where the person reading the tool's code is already sitting.

#### The number that makes the timing the point

**12 of ~100 tools have a declared response shape.** The other 88 fall back to
extracting nouns from description prose, marked `unconfirmed` — honest labelling
of a bad method, as that module says itself.

Filling 88 shapes into a side table is the kind of work that gets started and
abandoned. Filling them *as each tool is migrated in Phase 2* is the kind that
gets finished, because it is one method on a struct someone is already editing
for another reason.

So: **run the refactor, and add `response_shape()` before Phase 2 begins.**
Phase 2 touches every tool exactly once. Without this amendment it touches every
tool twice, and the second pass is the one that does not happen.

#### Phase 2's deliverable gains one line

A migrated domain module reports, per tool, whether its response is declared.
`Evidence` makes the gap legible rather than absent: a `Vendor` shape is weaker
than a `Constructed` one and says so, and no shape at all says *unread*.

### 2.2 `PlatformToolRegistry`

Replace `ToolRegistry` (with its constructor flags and `match`) with a registry backed by a `HashMap`.

```rust
// src/agent_backend/tools/registry.rs

pub struct PlatformToolRegistry {
    tools: HashMap<&'static str, Arc<dyn PlatformTool>>,
}

impl PlatformToolRegistry {
    /// Full registry — all tools including workspace and delegation.
    pub fn all() -> Self { Self::build(true, true) }

    /// Standard registry — no workspace tools, no delegation.
    /// Used for single-turn fallback execution.
    pub fn standard() -> Self { Self::build(false, false) }

    /// Workspace registry without delegation tools.
    /// Used for recursive execute_agent calls to prevent cycles.
    pub fn workspace_no_delegation() -> Self { Self::build(true, false) }

    fn build(include_workspace: bool, include_delegation: bool) -> Self {
        let all_tools = crate::agent_backend::tools::all_tools();
        let tools = all_tools
            .into_iter()
            .filter(|t| include_workspace || !t.requires_workspace())
            .filter(|t| include_delegation || !t.is_delegation())
            .map(|t| (t.name(), t))
            .collect();
        Self { tools }
    }

    /// Dispatch a tool call. Enforces workspace and credential checks
    /// before invoking execute().
    pub async fn execute(
        &self,
        tool_name: &str,
        input: &Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        match self.tool(tool_name) {
            Some(tool) => {
                // Workspace guard
                if tool.requires_workspace() && ctx.workspace_id.is_none() {
                    return Err(format!(
                        "Tool `{tool_name}` requires a workspace context."
                    ));
                }
                // Credential guard
                if let Some(key) = tool.required_credential() {
                    if ctx.user_secrets.as_ref()
                        .and_then(|s| s.get(key))
                        .is_none()
                    {
                        return Err(format!(
                            "Tool `{tool_name}` requires credential `{key}` \
                             which is not configured for this agent."
                        ));
                    }
                }
                tool.execute(input, ctx).await
            }
            None => {
                // Remote MCP fallthrough — unchanged from current behaviour.
                match ctx.remote_mcp.as_ref() {
                    Some(cat) if cat.get(tool_name).is_some() => {
                        cat.call(tool_name, input).await
                    }
                    Some(cat) if !cat.is_empty() => Err(format!(
                        "Unknown tool: {tool_name}. Remote MCP tools: {}",
                        cat.tools().iter()
                            .map(|t| t.qualified_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    _ => Err(format!("Unknown tool: {tool_name}")),
                }
            }
        }
    }

    /// Tool schema for Claude (LLM-visible tools only, filtered by card allowlist).
    pub fn to_claude_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&RemoteMcpCatalogue>,
    ) -> Vec<ClaudeTool> { /* same logic as today, query self.tools */ }

    /// Tool schema for OpenAI.
    pub fn to_openai_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&RemoteMcpCatalogue>,
    ) -> Vec<OpenAITool> { /* same logic as today */ }

    /// All tool names dispatchable by this registry instance.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.keys().copied().collect()
    }

    pub fn tool(&self, name: &str) -> Option<Arc<dyn PlatformTool>> {
        self.tools.get(name).cloned()
    }

    /// All tools across all categories — used for UI catalogue endpoint.
    pub fn all_tools_catalogue() -> Vec<ToolCatalogueEntry> {
        crate::agent_backend::tools::all_tools()
            .iter()
            .map(|t| ToolCatalogueEntry {
                name: t.name(),
                description: t.description(),
                category: t.category(),
                requires_workspace: t.requires_workspace(),
                is_llm_visible: t.is_llm_visible(),
                required_credential: t.required_credential(),
            })
            .collect()
    }
}

/// Flat catalogue entry — used by the bestiary UI and card authoring.
pub struct ToolCatalogueEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub category: ToolCategory,
    pub requires_workspace: bool,
    pub is_llm_visible: bool,
    pub required_credential: Option<&'static str>,
}
```

### 2.3 Domain module structure

```
src/agent_backend/tools/
  mod.rs                   ← pub fn all_tools() -> Vec<Arc<dyn PlatformTool>>
                             pub use platform_tool::{PlatformTool, ToolCategory, ToolCatalogueEntry}
                             pub use registry::PlatformToolRegistry
                             (keep SkillRegistry re-export during migration)
  platform_tool.rs         ← PlatformTool trait + ToolCategory enum (§2.1)
  registry.rs              ← PlatformToolRegistry (§2.2)
  domains/
    platform.rs            ← search_knowledge, query_ontology, execute_agent,
                             list_agents, web_search, delegate_to_agent,
                             validate_agent_card, build_output_contract
    workspace.rs           ← read/write_workspace_file, list_workspace_agents,
                             get_workspace_messages, list_workspace_outputs,
                             read_workspace_output
    coordination.rs        ← declare_intention, solicit_agent_plan, check_conflicts,
                             get_intention_map, clear_intention,
                             suggest_differentiation, emit_coherence_signal,
                             propose_composition_change,
                             record_coordination_observation
    observability.rs       ← query_eval_signals, query_eval_runs, query_anomalies,
                             query_hitl_queue, query_timeline, query_dyad_state,
                             classify_anomaly, route_to_hitl,
                             run_evaluator_registry, get_agent_calibration
    financial.rs           ← fmp_company_profile, fmp_income_statement,
                             fmp_balance_sheet, fmp_cash_flow, fmp_ratios,
                             fmp_key_metrics, fmp_dcf, fmp_analyst_estimates,
                             fmp_historical_price
    prediction_market.rs   ← polymarket_search, polymarket_event
    weather.rs             ← migrate from weather_tools.rs; wraps existing
                             handles()/dispatch() behind PlatformTool impls
    simops.rs              ← migrate from simops_tools.rs; wraps existing
                             execute_simops_* functions behind PlatformTool impls
    simulation.rs          ← fermi_execute_fpl, fermi_sensitivity_analysis,
                             run_monte_carlo, run_sensitivity_analysis
    spatial.rs             ← h3_resolve, geocode, create_beacon, query_beacons,
                             save_grid_map
    biology.rs             ← gbif_species_search, gbif_taxonomy_tree,
                             inat_observations, mycobank_lookup,
                             ncbi_genome_search, generate_specimen_art,
                             segment_creature_wings
    rabble.rs              ← mint_creature, activate_formation,
                             scan_nearby_creatures
    media.rs               ← generate_image, edit_image, speak_text
    marketplace.rs         ← get_shopping_profile, update_shopping_profile,
                             list_marketplace, create_listing
    video.rs               ← reduct_list_projects, reduct_get_project,
                             reduct_get_transcript, reduct_create_reel,
                             reduct_add_block
    sports.rs              ← call_football_api
```

### 2.4 `all_tools()` — the single registration point

```rust
// src/agent_backend/tools/mod.rs

pub fn all_tools() -> Vec<Arc<dyn PlatformTool>> {
    let mut tools: Vec<Arc<dyn PlatformTool>> = vec![];
    tools.extend(domains::platform::tools());
    tools.extend(domains::workspace::tools());
    tools.extend(domains::coordination::tools());
    tools.extend(domains::observability::tools());
    tools.extend(domains::financial::tools());
    tools.extend(domains::prediction_market::tools());
    tools.extend(domains::weather::tools());
    tools.extend(domains::simops::tools());
    tools.extend(domains::simulation::tools());
    tools.extend(domains::spatial::tools());
    tools.extend(domains::biology::tools());
    tools.extend(domains::rabble::tools());
    tools.extend(domains::media::tools());
    tools.extend(domains::marketplace::tools());
    tools.extend(domains::video::tools());
    tools.extend(domains::sports::tools());
    tools
}
```

Each domain module exports `pub fn tools() -> Vec<Arc<dyn PlatformTool>>`.

### 2.5 A domain module — concrete example (`financial.rs`)

Every domain follows this shape. Financial is shown because its tools are structurally repetitive (FMP API wrappers) and demonstrate how to reduce boilerplate within the new pattern.

```rust
// src/agent_backend/tools/domains/financial.rs

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools_legacy::ToolContext;

pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(FmpCompanyProfile),
        Arc::new(FmpIncomeStatement),
        Arc::new(FmpBalanceSheet),
        Arc::new(FmpCashFlow),
        Arc::new(FmpRatios),
        Arc::new(FmpKeyMetrics),
        Arc::new(FmpDcf),
        Arc::new(FmpAnalystEstimates),
        Arc::new(FmpHistoricalPrice),
    ]
}

// ── Shared executor ──────────────────────────────────────────────────────────
// Lifted verbatim from execute_fmp_api() in tools_legacy.rs.
async fn call_fmp(input: &Value, endpoint: &str, params: &[&str]) -> Result<String, String> {
    // ... existing execute_fmp_api() body ...
}

// ── Tool structs ─────────────────────────────────────────────────────────────

struct FmpCompanyProfile;

#[async_trait]
impl PlatformTool for FmpCompanyProfile {
    fn name(&self) -> &'static str { "fmp_company_profile" }
    fn description(&self) -> &'static str {
        "Get company profile including price, market cap, sector, industry, beta, \
         52-week range, CEO, description. Use this first to identify the company \
         and get current market data."
    }
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "Stock ticker symbol (e.g., AAPL, MSFT, GOOGL, TSLA)"
                }
            },
            "required": ["symbol"]
        })
    }
    fn category(&self) -> ToolCategory { ToolCategory::Financial }
    fn required_credential(&self) -> Option<&'static str> { Some("FMP_API_KEY") }
    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        call_fmp(input, "/stable/profile", &["symbol"]).await
    }
}

// ... remaining FMP tools follow same pattern ...
```

### 2.6 `validate_card_tools()` — updated phantom detection

```rust
// replaces invalid_tool_declarations() in tools_legacy.rs

pub fn validate_card_tools(
    declared: &[String],
    declared_servers: &[RemoteMcpServer],
) -> Vec<(String, ToolDeclarationError)> {
    let registry = PlatformToolRegistry::all();
    declared
        .iter()
        .filter_map(|name| {
            if registry.tool(name).is_some() {
                return None; // known platform tool
            }
            match name.split_once(NS_SEP) {
                Some((ns, _)) if !ns.is_empty() => {
                    let known = declared_servers.iter().any(|s| s.namespace() == ns);
                    if known { None }
                    else { Some((name.clone(), ToolDeclarationError::UnknownRemoteServer {
                        server: ns.to_string()
                    }))}
                }
                _ => Some((name.clone(), ToolDeclarationError::NotDispatchable)),
            }
        })
        .collect()
}
```

### 2.7 `ToolAwareExecutor` changes

Minimal. Replace `ToolRegistry` with `PlatformToolRegistry` in the struct field and constructor. The `execute_anthropic_loop` and `execute_openai_loop` call sites change only the type names; the logic is unchanged.

```rust
pub struct ToolAwareExecutor {
    inner: Arc<dyn AgentExecutor>,
    tool_registry: PlatformToolRegistry,  // was: ToolRegistry
    tool_context: Arc<ToolContext>,
    client: reqwest::Client,
}
```

---

## 3. Migration Plan

This is a **live codebase**. The migration must not break any existing behaviour. Execute phases in order; each phase compiles and passes tests independently.

### Phase 1 — Add the new trait and registry alongside the old (no deletion)

**Goal:** The new types exist and are tested. Nothing calls them yet.

1. Create `src/agent_backend/tools/platform_tool.rs` — `PlatformTool` trait and `ToolCategory` enum as specified in §2.1.
2. Create `src/agent_backend/tools/registry.rs` — `PlatformToolRegistry` as specified in §2.2, but with `fn build()` delegating to `ToolRegistry::execute()` for actual dispatch. This lets the registry exist and be tested before any tool is migrated.
3. Create `src/agent_backend/tools/domains/` directory with empty `mod.rs`.
4. Add `pub fn all_tools() -> Vec<Arc<dyn PlatformTool>> { vec![] }` in `tools/mod.rs`.
5. Write an invariant test:

```rust
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
```

**Deliverable:** Compiles. `cargo test` passes.

---

### Phase 2 — Migrate domain modules one at a time

For each domain module listed in §2.3:

1. Create the domain file with `pub fn tools() -> Vec<Arc<dyn PlatformTool>>`.
2. For each tool in the domain:
   - Define a zero-size struct (e.g., `struct FmpCompanyProfile;`).
   - Implement `PlatformTool`. The `execute()` body can initially delegate to the existing `execute_*` free function in `tools_legacy.rs`. This is acceptable for migration — the free functions are not deleted yet.
   - Copy the exact `name`, `description`, and `input_schema` from the `BuiltinToolDef` in `builtin_tools_core()`. Do not change any of these strings.
3. Register the module in `all_tools()`.
4. Add a test in the domain file:

```rust
#[test]
fn all_names_are_dispatchable() {
    for tool in tools() {
        assert!(
            !tool.name().is_empty(),
            "tool has empty name"
        );
    }
}
```

5. Run `cargo test` after each domain.

**Migration order** (do not vary — later domains depend on context types from earlier ones):

1. `simulation.rs` — no ToolContext dependencies beyond input; easiest
2. `financial.rs` — no ToolContext dependencies beyond credentials
3. `sports.rs` — single tool, no ToolContext
4. `weather.rs` — wraps existing `weather_tools::dispatch()`
5. `prediction_market.rs`
6. `biology.rs`
7. `spatial.rs`
8. `media.rs`
9. `marketplace.rs`
10. `video.rs`
11. `rabble.rs`
12. `simops.rs` — wraps existing `simops_tools::execute_*`
13. `workspace.rs`
14. `authoring.rs` — validate_agent_card, build_output_contract
15. `observability.rs`
16. `coordination.rs`
17. `platform.rs` — execute_agent, delegate_to_agent last (most complex context use)

**Deliverable:** `all_tools()` returns all 98 tools. The invariant test in Phase 1 passes with 98 entries. The legacy `match` still exists and is still the primary dispatch path.

---

### Phase 3 — Switch dispatch to the new registry

**Goal:** `ToolAwareExecutor` calls `PlatformToolRegistry::execute()`. The legacy `match` is no longer on the hot path.

1. Update `PlatformToolRegistry::execute()` to call `tool.execute()` directly (remove the delegation back to `ToolRegistry::execute()`).
2. Update `ToolAwareExecutor`: replace `ToolRegistry` field with `PlatformToolRegistry`.
3. Update the three construction sites:
   - `ToolAwareExecutor::new(inner, ToolRegistry::standard(), ctx)` → `PlatformToolRegistry::standard()`
   - `ToolRegistry::with_workspace_no_delegation()` → `PlatformToolRegistry::workspace_no_delegation()`
4. Update `invalid_tool_declarations()` call sites to use `validate_card_tools()`.
5. Update `builtin_tool_catalogue()` and `platform_tool_names()` callers to use `PlatformToolRegistry::all_tools_catalogue()`.

**Do not delete `tools_legacy.rs` yet.** The individual `execute_*` free functions are still called by domain module `execute()` implementations.

Write an integration test:

```rust
#[tokio::test]
async fn registry_dispatches_every_registered_tool_name() {
    // For pure tools (no ToolContext dependencies), verify the registry
    // returns Ok or a credential-missing error, never "Unknown tool".
    let registry = PlatformToolRegistry::all();
    let ctx = ToolContext::test_stub();  // see §4.3
    for tool in all_tools() {
        let result = registry.execute(tool.name(), &json!({}), &ctx).await;
        assert!(
            !matches!(result, Err(ref e) if e.starts_with("Unknown tool")),
            "Registry returned 'Unknown tool' for registered tool: {}",
            tool.name()
        );
    }
}
```

**Deliverable:** All tool calls go through `PlatformToolRegistry`. Legacy `match` is unreachable but still compiles.

---

### Phase 4 — Move execute_* bodies into domain modules, delete the legacy file

**Goal:** Remove `tools_legacy.rs`.

1. For each domain module, inline the body of its `execute_*` free function directly into `PlatformTool::execute()` (or into a private helper in the domain file). Shared helpers (e.g., `call_fmp()`, the FMP HTTP wrapper) become private functions in the domain module.
2. Once all execute_* functions have been inlined into domain modules, the remaining content of `tools_legacy.rs` is:
   - `ToolContext` — move to `tools/mod.rs` or a new `tools/context.rs`
   - `EvalTrigger` — move alongside `ToolContext`
   - `BuiltinToolDef`, `ToolDeclarationError`, `ToolRegistry` — delete
   - `ToolAwareExecutor` imports — update
3. Delete `tools_legacy.rs`.
4. Fix any remaining import paths. The file is named `legacy`; no other module should be importing from it by this point — if anything is, that import must be resolved before deletion.
5. Resolve the duplication between `tools/mod.rs`'s `Skill` trait and `tools/skills/mod.rs`'s `Skill` trait. Both should be deleted or replaced by `PlatformTool`. Any remaining `SkillRegistry` users (`xamanEK`, card authoring) should be updated to use `PlatformToolRegistry`.

**Deliverable:** `tools_legacy.rs` does not exist. `cargo build --workspace` and `cargo test --workspace` pass.

---

### Phase 5 — Wire the catalogue to the UI

**Goal:** The bestiary card editor UI gets tool data from the new registry, grouped by category.

1. Add or update the API endpoint that serves the tool list for the card editor UI. It should return `PlatformToolRegistry::all_tools_catalogue()` grouped by `ToolCategory`.
2. The response shape:

```json
{
  "categories": {
    "Financial": [
      {
        "name": "fmp_company_profile",
        "description": "...",
        "requires_workspace": false,
        "required_credential": "FMP_API_KEY"
      }
    ],
    "SimOps": [ ... ],
    ...
  },
  "total": 98
}
```

3. This replaces the current flat-list endpoint that drives the checkbox UI. The UI team can implement grouping on their side — this is a backend deliverable only.

**Deliverable:** The endpoint exists, returns all tools grouped by category, and includes `required_credential` so the UI can surface "this tool needs FMP_API_KEY."

---

## 3.6 What this is for: the agent compiles

**Added 2026-09-01, from the same conversation.** Recorded here because it is
what the refactor is *for*, and a refactor whose purpose is only tidiness gets
descoped halfway.

### Not a wizard

`/agents/new` is a six-step wizard and it feels like filling out forms, which is
the accurate reaction: a wizard asks a fixed sequence of questions and tells you
nothing until the end. What an author actually wants is the thing a compiler
gives them — **edit, compile, read the diagnostics, edit again** — and this repo
already has the verb. `contract-builder.js` compiles server-side on purpose:

> A browser-side compiler would be a second implementation of the publish gate
> and the two would drift, at which point this shows a green tick for a contract
> publish refuses.

Generalise that from the contract to the whole agent. An agent's card, tools and
contract are *source*. Compiling it resolves every declaration against what the
platform actually has, and reports what did not resolve. `response_shape()` is
what makes the interesting half of that check possible: *does the tool you named
return the field you claimed?*

### Three states, and the third is the one that matters

| state | meaning | who acts |
|---|---|---|
| **resolved** | a tool exists, is dispatchable, and declares a response containing this field | nobody |
| **error** | you named a tool that does not exist, or a field its declared response cannot supply | the author, now |
| **pending** | you named a field **no tool can supply yet** | nobody — it is a standing request for an integration |

`pending` is not a lesser `resolved`, it is a different kind of thing, and
getting it wrong is the difference between a platform that rewards ambition and
one that punishes it. From review:

> *I don't have a single agent that is all green because I haven't pruned
> genome_profiler — and I don't want to, because it should eventually be richer.*

Exactly right, and the type system already agrees: `Grounding::Unsourced` means
*no tool exists, so the field must be null, and a value here is the violation* —
`ratings.elo_current` carries the note "returns when a ClubElo or equivalent tool
is added". A declared field with no source is **honest ambition**, and the
platform has been able to say so since migration 200.

What is wrong is the surface. It collapses `pending` into `error`, so the only
way to get an agent "all green" is to delete the ambition — which is the same
defect this project keeps finding in a new place: *absent must look different
from bad.*

**So: green means zero errors, not zero pending.** An agent with four pending
fields and no errors compiles. It is finished, honest, and waiting on the world.

`genome_profiler` measured, since it is the case that prompted this:

```text
  6 Sourced      taxonomy, sister_taxa, genome size, chromosome count,
                 assembly name, assembly accession
  7 Unsourced    notable_genes, ploidy, divergence_mya, defining_traits,
                 iucn_status, population_trend, genetic_diversity_notes
  1 Derived      phylogeny.superorder
  1 Narrative    summary
```

It is the most honestly declared agent on the platform. Seven of its fifteen
fields say *no tool exists for this yet* — the "should eventually be richer" made
machine-readable. Pruning it to reach green would delete the best example of the
thing the contract system is for.

### And there is no green

Checked, because "I don't have a single agent that is all green" sounded
structural rather than incidental. It is:

```rust
pub enum Reading { Idle, Fault, Unknown }   // src/panel_absence.rs
```

**Three readings, and none of them means *working*.** An agent that does
everything right reads `idle`. `genome_profiler`'s health today is 8 `idle` and 2
`unknown`, with nothing at fault — so the screen has no way to say the thing that
is true, which is that this agent works.

The module name is the confession: `panel_absence`. It was built to explain
absences honestly, and it does that well. It was never built to assert presence,
so it cannot.

That is why "agent health" needs the compile rather than more panels. A positive
reading is not *no fault found*; it is **resolved declarations, sourced fields
with named evidence, and pulses that carried grades** — which is a statement with
a subject, and the only kind that can go green.

### Which makes "compile" the shape of the configuration shelf

Not steps. A declaration, its diagnostics, and the control that fixes it — the
row grammar the trace already settled on: **value · condition · act**, where the
condition is `resolved` / `error` / `pending` and the act is the editor that
closes it. The shelf already mounts `ContractBuilder` on the `field_contract`
rung; the rest is the same move on the other rungs.

## 4. Constraints & Invariants

### 4.1 Naming — must not change

Every `fn name()` implementation must return the exact string that currently appears as an arm in `ToolRegistry::execute()` and in `BuiltinToolDef.name`. These strings appear in:
- Agent card `mcp_tools` arrays (stored in DB)
- MCP client tool lists
- Gate trust logging
- Eval signal records

Any rename breaks stored cards and live agents. If a tool needs renaming, that is a separate migration with a deprecation period and is out of scope here.

### 4.2 Behaviour — must not change

No `execute()` body may alter the return value, error messages, or side effects of the tool it replaces. The refactor is purely structural. If a test fails after inlining a function body, the function was not inlined correctly.

### 4.3 `ToolContext::test_stub()`

The integration test in Phase 3 requires a minimal `ToolContext`. Add a constructor:

```rust
#[cfg(test)]
impl ToolContext {
    pub fn test_stub() -> Self {
        Self {
            memory_store: Arc::new(MemoryStore::test_stub()),
            embedder: Arc::new(NoopEmbedder),
            registry: Arc::new(AgentRegistry::empty()),
            current_agent_id: None,
            workspace_id: None,
            workspace_slug: None,
            workspace_git: None,
            db: None,
            gas_fees: None,
            user_id: None,
            user_secrets: None,
            credentials: Arc::new(ResolvedCredentials::empty()),
            parent_episode_id: None,
            eval_trigger: None,
            remote_mcp: None,
        }
    }
}
```

If stub constructors for `MemoryStore`, `AgentRegistry`, and `ResolvedCredentials` do not exist, add `::empty()` or `::test_stub()` constructors in `#[cfg(test)]` blocks in their respective files. Do not add these to production code paths.

### 4.4 `ARMS_WITHOUT_DEFS` — must stay empty

After Phase 2, every arm in the legacy `match` must correspond to a `BuiltinToolDef`. The `ARMS_WITHOUT_DEFS` slice must be empty. Add a test:

```rust
#[test]
fn no_arms_without_defs() {
    assert!(
        ARMS_WITHOUT_DEFS.is_empty(),
        "ARMS_WITHOUT_DEFS is non-empty: {:?}", ARMS_WITHOUT_DEFS
    );
}
```

This test already belongs in the codebase. Add it in Phase 1.

### 4.5 Remote MCP fallthrough — must be preserved exactly

`PlatformToolRegistry::execute()` must fall through to `ctx.remote_mcp` for any name not found in `self.tools`, using the exact same error message format as the current match fallthrough. Do not change the remote MCP behaviour.

### 4.6 `prompt_demands_structured_output` — do not touch

The heuristic in `tool_executor.rs` that detects JSON-contract prompts and bypasses the tool loop is fragile and load-bearing. It is not part of this refactor. Leave it exactly as-is.

### 4.7 `simops_skill!` macro — delete after Phase 4

The macro in `tools/mod.rs` delegates `execute()` back to `ToolRegistry::standard().execute()`. Once SimOps tools are proper `PlatformTool` implementations, this circular delegation is gone and the macro can be deleted.

---

## 5. What Success Looks Like

After Phase 4:

- `tools_legacy.rs` does not exist.
- Adding a new tool means: create a struct in the appropriate domain module, implement `PlatformTool`, add it to that module's `tools()` vec. No other file needs to change.
- `cargo build --workspace` and `cargo test --workspace` pass.
- The invariant test reports exactly N tools (where N = the count confirmed at end of Phase 2). No phantom tools. No arms without defs.
- `ARMS_WITHOUT_DEFS` test passes (empty slice).
- The `gate_trust` system, coherence gate, grounding gate, evaluator registry, and all handler code are unchanged.

After Phase 5:

- The card editor UI receives tools grouped by category with credential requirements surfaced.
- A card author can see "Financial (9, requires FMP_API_KEY)" without needing to know tool names from memory.

---

## 6. Out of Scope for This Refactor

- Schema derivation via `schemars` — `input_schema()` remains hand-written `json!({...})`. This is a follow-up once the structural migration is complete.
- The `SkillCategory` → `ToolCategory` merge in `tools/skills/mod.rs` beyond what is needed to remove duplication in Phase 4.
- Changes to `ToolContext` fields or any context wire-up.
- Changes to card storage, the `agent_card.rs` parser, or the `mcp_tools` allowlist format.
- Any UI implementation beyond the catalogue endpoint added in Phase 5.
- New tools. Add no new tools during this refactor. Tool additions are a separate PR after this lands.
