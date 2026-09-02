// src/agent_backend/tools/domains/mod.rs
//
// Phase 1: domain module directory — empty registration point.
//
// Each domain is added as a submodule in Phase 2, in the order documented in
// docs/plans/TOOL_REGISTRY_REFACTOR.md §3 (Phase 2 migration order):
//
//   1. simulation     — no ToolContext dependencies beyond input; easiest
//   2. financial      — no ToolContext dependencies beyond credentials
//   3. sports         — single tool, no ToolContext
//   4. weather        — wraps existing weather_tools::dispatch()
//   5. prediction_market
//   6. biology
//   7. spatial
//   8. media
//   9. marketplace
//  10. video
//  11. rabble
//  12. simops         — wraps existing simops_tools::execute_*
//  13. workspace
//  14. authoring      — validate_agent_card, build_output_contract
//  15. observability
//  16. coordination
//  17. platform       — execute_agent, delegate_to_agent last (most complex)
//
// cargo test passes after each domain is added.

use crate::agent_backend::tools::platform_tool::PlatformTool;
use std::sync::Arc;

pub mod authoring;
pub mod biology;
pub mod coordination;
pub mod financial;
pub mod marketplace;
pub mod media;
pub mod observability;
pub mod platform;
pub mod prediction_market;
pub mod rabble;
pub mod simops;
pub mod simulation;
pub mod spatial;
pub mod sports;
pub mod video;
pub mod weather;
pub mod workspace;

/// All migrated tools, in registration order.
///
/// In Phase 1 this returns an empty vec; the invariant test
/// `platform_tool_names_are_unique` passes trivially and serves as a
/// compile-time check that the infrastructure is wired correctly.
pub fn all_tools() -> Vec<Arc<dyn PlatformTool>> {
    let mut tools: Vec<Arc<dyn PlatformTool>> = vec![];
    tools.extend(simulation::tools());
    tools.extend(financial::tools());
    tools.extend(sports::tools());
    tools.extend(weather::tools());
    tools.extend(prediction_market::tools());
    tools.extend(biology::tools());
    tools.extend(spatial::tools());
    tools.extend(media::tools());
    tools.extend(marketplace::tools());
    tools.extend(video::tools());
    tools.extend(rabble::tools());
    tools.extend(simops::tools());
    tools.extend(workspace::tools());
    tools.extend(authoring::tools());
    tools.extend(observability::tools());
    tools.extend(coordination::tools());
    tools.extend(platform::tools());
    tools
}
