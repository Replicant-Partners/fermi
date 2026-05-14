//! Deterministic skill implementations.
//!
//! A **skill** differs from a tool in one critical way: it is a pure deterministic
//! computation — no LLM call, no external HTTP dependency (unless the skill *is* the
//! HTTP call, like geocoding). Skills can be:
//!
//! 1. **LLM-visible** (`is_llm_visible = true`): offered to the LLM in the tool
//!    schema so the model can choose to invoke them.
//! 2. **Pipeline skills** (`is_llm_visible = false`): invoked by the executor
//!    directly as part of the agent's processing pipeline, without an LLM round-trip.
//! 3. **Directly callable**: invoked by name via `ToolRegistry::execute()` from
//!    any context, bypassing the LLM entirely.
//!
//! # Adding a new skill
//!
//! 1. Implement `Skill` in the appropriate domain module.
//! 2. Register it in `SkillRegistry::all()` below.
//! 3. Add the skill name to any agent card's `capabilities.skills` array.
//! 4. The conformance test (`validate_card_skills`) will catch undeclared names.

use async_trait::async_trait;
use super::ToolContext;

pub mod spatial;
pub mod simulation;
pub mod bio;
pub mod formation;
pub mod simops;

// ─── Skill trait ─────────────────────────────────────────────────────────────

/// The contract every deterministic skill implements.
#[async_trait]
pub trait Skill: Send + Sync {
    /// Stable identifier used in the tool dispatch table and card `skills` array.
    fn name(&self) -> &'static str;

    /// Human-readable description — shown to the LLM when `is_llm_visible`.
    fn description(&self) -> &'static str;

    /// JSON schema for the skill's input parameters.
    fn input_schema(&self) -> serde_json::Value;

    /// Whether this skill should appear in the LLM's tool list.
    /// Pure computation skills (Monte Carlo, H3 math) are LLM-visible so the
    /// model can choose to invoke them. Internal pipeline skills are not.
    fn is_llm_visible(&self) -> bool { true }

    /// Which domain category this skill belongs to (for catalogue / discovery).
    fn category(&self) -> SkillCategory;

    /// Execute the skill. Deterministic — same input produces same output.
    async fn execute(
        &self,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String>;
}

/// Domain category for a skill — used by xamanEK and the agent catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillCategory {
    /// H3 hexagonal grid, geocoding, beacons, spatial grids
    Spatial,
    /// Monte Carlo simulation, sensitivity analysis, FPL execution
    Simulation,
    /// GBIF taxonomy, wing segmentation, biological classification
    Biology,
    /// Onto4MAT formation algorithms, swarm coordination
    Formation,
    /// SimOps cascade, KPI computation, predictor, optimizer
    ProcessOptimization,
    /// Observability reads (eval signals, anomalies, timeline, dyads)
    Observability,
}

// ─── SkillRegistry ───────────────────────────────────────────────────────────

/// Central registry of all deterministic skills.
/// Returns a fresh `Vec<Box<dyn Skill>>` on each call — skills are stateless.
pub struct SkillRegistry;

impl SkillRegistry {
    /// All registered skills. Extend this list to add new skills platform-wide.
    pub fn all() -> Vec<Box<dyn Skill>> {
        vec![
            // ── Spatial ──────────────────────────────────────────────
            Box::new(spatial::H3Resolve),
            Box::new(spatial::Geocode),
            Box::new(spatial::CreateBeacon),
            Box::new(spatial::QueryBeacons),
            Box::new(spatial::SaveGridMap),
            Box::new(spatial::ScanNearbyCreatures),

            // ── Simulation ───────────────────────────────────────────
            Box::new(simulation::RunMonteCarlo),
            Box::new(simulation::RunSensitivityAnalysis),

            // ── Biology ──────────────────────────────────────────────
            Box::new(bio::GbifTaxonomyTree),
            Box::new(bio::SegmentCreatureWings),

            // ── Formation ────────────────────────────────────────────
            Box::new(formation::ActivateFormation),

            // ── SimOps process optimization ──────────────────────────
            Box::new(simops::SimopsCascadeForward),
            Box::new(simops::SimopsCascadeBackward),
            Box::new(simops::SimopsKpiCompute),
            Box::new(simops::SimopsPredictorTrain),
            Box::new(simops::SimopsPredictorForecast),
            Box::new(simops::SimopsOptimizeScale),
            Box::new(simops::SimopsOptimizeSingleInput),
        ]
    }

    /// Skills by category — used by xamanEK for capability discovery.
    pub fn by_category(category: SkillCategory) -> Vec<Box<dyn Skill>> {
        Self::all().into_iter().filter(|s| s.category() == category).collect()
    }

    /// Find a skill by name. O(n) — n is small (< 25 skills).
    pub fn find(name: &str) -> Option<Box<dyn Skill>> {
        Self::all().into_iter().find(|s| s.name() == name)
    }

    /// All skill names as a sorted list — for conformance tests and catalogue.
    pub fn names() -> Vec<&'static str> {
        let mut names: Vec<&'static str> = Self::all().iter().map(|s| s.name()).collect();
        names.sort_unstable();
        names
    }
}
