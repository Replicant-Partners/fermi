// src/agent_backend/tools/domains/workspace.rs
//
// Phase 4 domain migration: Workspace tools.
//
// Eight tools:
//   read_workspace_file       — requires_workspace: true
//   read_workspace_output     — requires_workspace: false
//   list_workspace_outputs    — requires_workspace: false
//   list_workspace_agents     — requires_workspace: true
//   write_workspace_file      — requires_workspace: true
//   evaluate_coherence        — requires_workspace: true
//   coherence_snapshot        — requires_workspace: true
//   get_workspace_messages    — requires_workspace: true
//
// Each is a zero-size struct implementing PlatformTool. execute() calls
// a private function defined in this module.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::types::CoherenceEvaluation;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Workspace-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(ReadWorkspaceFile),
        Arc::new(ReadWorkspaceOutput),
        Arc::new(ListWorkspaceOutputs),
        Arc::new(ListWorkspaceAgents),
        Arc::new(SelectAgent),
        Arc::new(ExecuteCoordinationGraph),
        Arc::new(WriteWorkspaceFile),
        Arc::new(EvaluateCoherence),
        Arc::new(CoherenceSnapshot),
        Arc::new(GetWorkspaceMessages),
    ]
}

// ─── read_workspace_file ──────────────────────────────────────────────────────

struct ReadWorkspaceFile;

#[async_trait]
impl PlatformTool for ReadWorkspaceFile {
    fn name(&self) -> &'static str {
        "read_workspace_file"
    }

    fn description(&self) -> &'static str {
        "Read a file from the current workspace's git repository."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The file path relative to workspace root"
                }
            },
            "required": ["path"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_read_workspace_file(input, ctx).await
    }
}

// ─── read_workspace_output ────────────────────────────────────────────────────

struct ReadWorkspaceOutput;

#[async_trait]
impl PlatformTool for ReadWorkspaceOutput {
    fn name(&self) -> &'static str {
        "read_workspace_output"
    }

    fn description(&self) -> &'static str {
        "Read a typed output from any workspace. Use this to consume results published by upstream workspaces (e.g., team prior → tournament path). Returns the output value, version, and last update time."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "UUID of the workspace to read from"
                },
                "key": {
                    "type": "string",
                    "description": "Output key, e.g. 'predicted_probability', 'driver_scores', 'sobol_indices'"
                }
            },
            "required": ["workspace_id", "key"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_read_workspace_output(input, ctx).await
    }
}

// ─── list_workspace_outputs ───────────────────────────────────────────────────

struct ListWorkspaceOutputs;

#[async_trait]
impl PlatformTool for ListWorkspaceOutputs {
    fn name(&self) -> &'static str {
        "list_workspace_outputs"
    }

    fn description(&self) -> &'static str {
        "List all published outputs for a workspace. Returns keys, values, versions, and update times."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "workspace_id": {
                    "type": "string",
                    "description": "UUID of the workspace to list outputs from"
                }
            },
            "required": ["workspace_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_list_workspace_outputs(input, ctx).await
    }
}

// ─── select_agent ────────────────────────────────────────────────────────────
//
// Rank agents that declare a given input_schema_id, scoring by calibration
// (Brier), cost, valence fit, and fidelity.
//
// Scope controls the candidate pool:
//   workspace  (default) — agents in this workspace only
//   fleet:<id>           — agents tagged fleet:<id> (Phase 3, stubbed)
//   marketplace          — all public ABW agents (Phase 4, stubbed)
//
// Each level is a curated subset of the one above. The goal is a fully open
// marketplace — scoping is a trust and relevance mechanism, not a ceiling.

struct SelectAgent;

#[async_trait]
impl PlatformTool for SelectAgent {
    fn name(&self) -> &'static str {
        "select_agent"
    }

    fn description(&self) -> &'static str {
        "Rank agents that declare a given input schema ID, scoring by Brier calibration, \
         cost, valence fit, and fidelity. Use to fill open slots in a coordination graph.\n\n\
         `scope.level` controls the candidate pool:\n\
         · workspace (default) — agents in this workspace\n\
         · fleet:<id>          — all agents tagged with that fleet (e.g. 'fermi', 'simops')\n\
         · marketplace         — all public ABW agents (Phase 4, returns workspace scope for now)\n\n\
         Returns a ranked candidate list with per-criterion breakdown. Route to the top \
         candidate or reason about the breakdown for unusual queries."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input_schema_id": {
                    "type": "string",
                    "description": "Schema ID to match against candidates (e.g. 'scro/bom-query/1')."
                },
                "scope": {
                    "type": "object",
                    "description": "Optional. Candidate pool scope. Default: workspace-scoped.",
                    "properties": {
                        "level": {
                            "type": "string",
                            "description": "workspace | fleet | marketplace"
                        },
                        "fleet_id": {
                            "type": "string",
                            "description": "Required when level=fleet. Fleet name tag (e.g. 'fermi', 'simops')."
                        }
                    }
                },
                "query": {
                    "type": "string",
                    "description": "Optional. The query to be sent. Context only — does not affect scoring yet."
                },
                "criteria": {
                    "type": "object",
                    "description": "Optional scoring weights summing to 1.0. Defaults: brier=0.40, cost=0.20, valence_fit=0.20, fidelity=0.20.",
                    "properties": {
                        "brier":       { "type": "number", "description": "Calibration score weight" },
                        "cost":        { "type": "number", "description": "Price per call weight (lower cost scores higher)" },
                        "valence_fit": { "type": "number", "description": "Personality complement weight" },
                        "fidelity":    { "type": "number", "description": "Gate::OutputSchema approval rate weight" }
                    }
                }
            },
            "required": ["input_schema_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_select_agent(input, ctx).await
    }
}

pub(crate) async fn execute_select_agent(
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;
    let pool = ctx.memory_store.pool();
    let db = ctx
        .db
        .as_ref()
        .ok_or("select_agent requires a database context")?;

    let input_schema_id = input
        .get("input_schema_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: input_schema_id")?;

    // Scope — determines the candidate pool.
    //   workspace  (default) — agents in this workspace.
    //   fleet:<id>           — agents tagged fleet:<id>. Phase 3 (stubbed: falls back to workspace).
    //   marketplace          — all public ABW agents.   Phase 4 (stubbed: falls back to workspace).
    let scope_level = input
        .get("scope")
        .and_then(|s| s.get("level"))
        .and_then(|v| v.as_str())
        .unwrap_or("workspace");
    let fleet_id = input
        .get("scope")
        .and_then(|s| s.get("fleet_id"))
        .and_then(|v| v.as_str());
    // Stubs for Phase 3/4 — currently fall back to workspace scope with a note.
    let scope_note: Option<&str> = match scope_level {
        "fleet" if fleet_id.is_some() => {
            Some("fleet scope not yet implemented — returning workspace candidates")
        }
        "marketplace" => {
            Some("marketplace scope not yet implemented — returning workspace candidates")
        }
        _ => None,
    };

    // Criteria weights.
    //
    // Priority (highest to lowest):
    //   1. Explicit criteria in the tool call (the caller knows what they want)
    //   2. Workspace-level weights (Loop 4B updates these from observed outcomes)
    //   3. Platform defaults
    //
    // Workspace weights are stored in `teams.selection_weights` (migration 231)
    // and updated by the selection performance consolidation step in Loop 4B.
    // An explicit call-level override always wins — a strategist that has
    // reason to weight calibration heavily for this specific query type should
    // not be overridden by a workspace default.
    let workspace_weights: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT selection_weights FROM teams WHERE id = $1")
            .bind(workspace_id)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .flatten();
    let crit = input.get("criteria");
    let w_brier = crit
        .and_then(|c| c.get("brier"))
        .and_then(|v| v.as_f64())
        .or_else(|| {
            workspace_weights
                .as_ref()
                .and_then(|w| w.get("brier"))
                .and_then(|v| v.as_f64())
        })
        .unwrap_or(0.40);
    let w_cost = crit
        .and_then(|c| c.get("cost"))
        .and_then(|v| v.as_f64())
        .or_else(|| {
            workspace_weights
                .as_ref()
                .and_then(|w| w.get("cost"))
                .and_then(|v| v.as_f64())
        })
        .unwrap_or(0.20);
    let w_valence_fit = crit
        .and_then(|c| c.get("valence_fit"))
        .and_then(|v| v.as_f64())
        .or_else(|| {
            workspace_weights
                .as_ref()
                .and_then(|w| w.get("valence_fit"))
                .and_then(|v| v.as_f64())
        })
        .unwrap_or(0.20);
    let w_fidelity = crit
        .and_then(|c| c.get("fidelity"))
        .and_then(|v| v.as_f64())
        .or_else(|| {
            workspace_weights
                .as_ref()
                .and_then(|w| w.get("fidelity"))
                .and_then(|v| v.as_f64())
        })
        .unwrap_or(0.20);

    // 1. Candidates: agents whose input_contract.accepts_schema matches, scoped.
    //
    // workspace (default) — agents that are members of this workspace.
    // fleet:<id>          — agents tagged `fleet:<id>` anywhere in the platform.
    //                       Fleet membership is declared via the `tags` column;
    //                       a creator joins a fleet by adding `fleet:fermi` etc.
    //                       Visibility must be 'public' for fleet/marketplace scope.
    // marketplace         — all public agents (Phase 4 — falls back to workspace).
    let rows = if scope_level == "fleet" {
        if let Some(fid) = fleet_id {
            let fleet_tag = format!("fleet:{fid}");
            sqlx::query(
                "SELECT a.agent_id, a.agent_name, a.agent_type, a.description,
                        a.input_contract->>'accepts_schema'  AS input_schema_id,
                        a.output_contract->>'produces_schema' AS output_schema_id,
                        a.competition,
                        a.valence
                 FROM agents a
                 WHERE $1 = ANY(a.tags)
                   AND a.input_contract->>'accepts_schema' = $2
                   AND a.visibility = 'public'",
            )
            .bind(&fleet_tag)
            .bind(input_schema_id)
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Fleet query failed: {e}"))?
        } else {
            return Err("scope.level 'fleet' requires scope.fleet_id".to_string());
        }
    } else {
        // workspace scope (default) — and the fallback for marketplace (Phase 4).
        sqlx::query(
            "SELECT a.agent_id, a.agent_name, a.agent_type, a.description,
                    a.input_contract->>'accepts_schema'  AS input_schema_id,
                    a.output_contract->>'produces_schema' AS output_schema_id,
                    a.competition,
                    a.valence
             FROM workspace_agents wa
             JOIN agents a ON a.agent_id = wa.agent_id
             WHERE wa.workspace_id = $1
               AND a.input_contract->>'accepts_schema' = $2",
        )
        .bind(workspace_id)
        .bind(input_schema_id)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Query failed: {e}"))?
    };

    if rows.is_empty() {
        return serde_json::to_string_pretty(&json!({
            "input_schema_id": input_schema_id,
            "scope": { "level": scope_level, "fleet_id": fleet_id },
            "candidates": [],
            "note": "No agents declare this input schema in the queried scope. \
                     For workspace scope: add agents that declare \
                     input_contract.accepts_schema matching this value as workspace members."
        }))
        .map_err(|e| e.to_string());
    }

    // 2a. Workspace valence centroid — used for valence_fit scoring.
    //
    // valence_fit measures how much a candidate COMPLEMENTS the workspace's
    // existing personality distribution. Higher complement distance = more
    // diverse = better fit (homophily check, same logic as the dreaming audit).
    //
    // centroid = (avg arousal, avg valence) over workspace members that have
    // declared a valence. Candidates with no valence get 0.5 (neutral fit).
    let centroid: Option<(f64, f64)> = sqlx::query(
        "SELECT AVG((a.valence->>'arousal')::float)   AS avg_arousal,
                AVG((a.valence->>'valence')::float)    AS avg_valence
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1
           AND a.valence IS NOT NULL
           AND (a.valence->>'arousal') IS NOT NULL
           AND (a.valence->>'valence') IS NOT NULL",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .and_then(|r| {
        let a = r.try_get::<Option<f64>, _>("avg_arousal").ok().flatten();
        let v = r.try_get::<Option<f64>, _>("avg_valence").ok().flatten();
        match (a, v) {
            (Some(a), Some(v)) => Some((a, v)),
            _ => None,
        }
    });

    // 2. Score each candidate
    let mut scored: Vec<serde_json::Value> = Vec::new();

    for row in &rows {
        let agent_name: String = row.get("agent_name");
        let agent_id: uuid::Uuid = row.get("agent_id");
        let agent_type: String = row.get("agent_type");
        let description: Option<String> = row.get("description");
        let input_sid: Option<String> = row.get("input_schema_id");
        let output_sid: Option<String> = row.get("output_schema_id");
        let competition: Option<serde_json::Value> = row.try_get("competition").unwrap_or(None);

        // Brier: mean forecast_calibration score from eval_signals (Brier inverted
        // to 0-1 where higher = better calibrated, consistent with calibration_score).
        let brier_score: Option<f64> = sqlx::query_scalar(
            "SELECT AVG(score) FROM eval_signals
             WHERE agent_id = $1 AND signal_type = 'forecast_calibration'
               AND score IS NOT NULL",
        )
        .bind(agent_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten()
        .map(|b: f64| 1.0 - b); // Brier inverted: lower raw = higher calibration

        // Cost: lower price = higher score. Free (0) → 1.0; 100 credits → 0.0.
        let price = competition
            .as_ref()
            .and_then(|c| c.get("price_credits_per_call"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cost_score = 1.0 - (price as f64).min(100.0) / 100.0;

        let support_tier = competition
            .as_ref()
            .and_then(|c| c.get("support_tier"))
            .and_then(|v| v.as_str())
            .unwrap_or("community")
            .to_string();

        let domains: Vec<String> = competition
            .as_ref()
            .and_then(|c| c.get("domains"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        // Valence fit: complement distance from the workspace centroid.
        //
        // A candidate that sits where the workspace already has coverage adds
        // less than one who fills a gap. Euclidean distance on the (arousal,
        // valence) affect plane, normalised to [0, 1] by dividing by the
        // maximum possible distance (sqrt(2) on a unit square).
        // Candidates with no declared valence get 0.5 (neutral fit).
        let valence: Option<serde_json::Value> = row.try_get("valence").unwrap_or(None);
        let valence_fit_score: f64 = match (centroid, &valence) {
            (Some((ca, cv)), Some(val)) => {
                let va = val.get("arousal").and_then(|v| v.as_f64());
                let vv = val.get("valence").and_then(|v| v.as_f64());
                match (va, vv) {
                    (Some(va), Some(vv)) => {
                        // Euclidean complement distance, normalised
                        let dist = ((va - ca).powi(2) + (vv - cv).powi(2)).sqrt();
                        // Max distance on [-1,1]x[0,1] plane ≈ sqrt(4+1)=2.24;
                        // practical range for arousal[0,1] x valence[-1,1] is sqrt(2)
                        (dist / std::f64::consts::SQRT_2).min(1.0)
                    }
                    _ => 0.5,
                }
            }
            _ => 0.5,
        };

        // Fidelity: Gate::OutputSchema ledger (Retention::Recorded since mig 230).
        // approved / (approved + refused). Undetermined rows excluded — they are
        // the absence of a check, not a pass. Absent = 0.5 prior (no data yet).
        let fidelity_score: f64 = {
            let row = sqlx::query(
                "SELECT
                    COUNT(*) FILTER (WHERE decision = 'approved') AS approved,
                    COUNT(*) FILTER (WHERE decision = 'refused')  AS refused
                 FROM gate_decisions
                 WHERE gate = 'output_schema' AND subject = $1",
            )
            .bind(&agent_name)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
            match row {
                Some(r) => {
                    let approved: i64 = r.try_get("approved").unwrap_or(0);
                    let refused: i64 = r.try_get("refused").unwrap_or(0);
                    let checkable = approved + refused;
                    if checkable > 0 {
                        approved as f64 / checkable as f64
                    } else {
                        0.5
                    }
                }
                None => 0.5,
            }
        };

        // Composite score: weighted sum over criteria
        let brier_component = brier_score.unwrap_or(0.5); // 0.5 prior when no data
        let composite = w_brier * brier_component
            + w_cost * cost_score
            + w_valence_fit * valence_fit_score
            + w_fidelity * fidelity_score;

        scored.push(json!({
            "agent":       agent_name,
            "agent_type":  agent_type,
            "description": description,
            "score": (composite * 1000.0).round() / 1000.0,
            "breakdown": {
                "brier":       brier_score,
                "brier_note":  if brier_score.is_none() {
                    Some("no calibration data — using 0.5 prior")
                } else { None },
                "cost":        cost_score,
                "valence_fit": valence_fit_score,
                "valence_fit_note": if centroid.is_some() && valence.is_some() {
                    None
                } else if centroid.is_none() {
                    Some("no workspace valence centroid — using 0.5 prior (members have no declared valence)")
                } else {
                    Some("candidate has no declared valence — using 0.5 prior")
                },
                "fidelity":    fidelity_score,
                "fidelity_note": if fidelity_score == 0.5 {
                    Some("no gate_decisions rows yet — using 0.5 prior (accrues as delegation hops run)")
                } else { None }
            },
            "input_schema_id":  input_sid,
            "output_schema_id": output_sid,
            "competition": {
                "domains":               domains,
                "price_credits_per_call": price,
                "support_tier":           support_tier
            }
        }));
    }

    // Sort by composite score descending
    scored.sort_by(|a, b| {
        let sa = a.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let sb = b.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Best-effort: write a selection record for competition-stats and Loop 4.
    // The tool succeeds even if this insert fails — a missed trace row is
    // not worth failing a coordination graph traversal over.
    let top_candidate = scored
        .first()
        .and_then(|c| c.get("agent"))
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(ref db) = ctx.db {
        let _ = sqlx::query(
            "INSERT INTO select_agent_decisions
             (input_schema_id, scope_level, scope_fleet_id, workspace_id,
              criteria_weights, candidates, selected, parent_episode_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(input_schema_id)
        .bind(scope_level)
        .bind(fleet_id)
        .bind(workspace_id)
        .bind(serde_json::json!({
            "brier": w_brier, "cost": w_cost,
            "valence_fit": w_valence_fit, "fidelity": w_fidelity
        }))
        .bind(serde_json::Value::Array(
            scored
                .iter()
                .map(|c| {
                    json!({
                        "agent": c.get("agent"),
                        "score": c.get("score")
                    })
                })
                .collect(),
        ))
        .bind(&top_candidate)
        .bind(ctx.parent_episode_id)
        .execute(db)
        .await;
    }

    serde_json::to_string_pretty(&json!({
        "input_schema_id": input_schema_id,
        "scope": {
            "level": scope_level,
            "fleet_id": fleet_id,
            "note": scope_note
        },
        "criteria_weights": {
            "brier":       w_brier,
            "cost":        w_cost,
            "valence_fit": w_valence_fit,
            "fidelity":    w_fidelity
        },
        "candidates": scored
    }))
    .map_err(|e| e.to_string())
}

// ─── execute_coordination_graph ───────────────────────────────────────────────
//
// Traverses a typed coordination graph (workflow_template.nodes + edges)
// without LLM narration. Calls select_agent for open slots, execute_agent
// for each bound node, and returns a CoordinationTrace.
//
// Strategist agents call this instead of manually looping over execute_agent
// calls. The LLM handles failures and recovery; the executor handles traversal.

struct ExecuteCoordinationGraph;

#[async_trait]
impl PlatformTool for ExecuteCoordinationGraph {
    fn name(&self) -> &'static str {
        "execute_coordination_graph"
    }

    fn description(&self) -> &'static str {
        "Traverse a typed coordination graph (workflow_template.nodes + edges) and \
         return a CoordinationTrace. For each node: open slots are filled by \
         select_agent, bound nodes execute directly via execute_agent. Stops at \
         the first failure and reports what completed.\n\n\
         You handle failures and recovery. The executor handles traversal order, \
         schema validation at each hop, and agent selection for open slots."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "entry_input": {
                    "type": "string",
                    "description": "The query or JSON payload to send to the first node in the graph."
                },
                "workflow_template": {
                    "type": "object",
                    "description": "Optional. Override the composition's declared workflow_template. If absent, reads from the current composition."
                }
            },
            "required": ["entry_input"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_execute_coordination_graph(input, ctx).await
    }
}

async fn execute_execute_coordination_graph(
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use crate::agent_backend::agent_card::WorkflowTemplate;
    use crate::agent_backend::coordination_graph::execute_coordination_graph;

    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let entry_input = input
        .get("entry_input")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: entry_input")?;

    // Resolve the workflow_template to traverse.
    //
    // Priority:
    //   1. Explicit `workflow_template` parameter (caller passes it, e.g. after
    //      calling get_workflow_template) — always wins.
    //   2. Auto-read from the workspace composition: look up
    //      `teams.coordination_strategist_id`, fetch that agent's
    //      `agents.workflow_template`, deserialise. This is the path a
    //      strategist agent takes when it calls the tool without a template
    //      argument — it just says "execute my composition's graph".
    //   3. Empty template — the executor returns an informative error.
    let template: WorkflowTemplate = if let Some(tpl) = input.get("workflow_template") {
        serde_json::from_value(tpl.clone()).map_err(|e| {
            format!("Invalid workflow_template: {e}. Ensure it has `nodes` and `edges` arrays.")
        })?
    } else if let Some(ref db) = ctx.db {
        // Auto-read: workspace → composition strategist → agent card's workflow_template
        let strategist_template: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT a.workflow_template
             FROM teams t
             JOIN agents a ON a.agent_id = t.coordination_strategist_id
             WHERE t.id = $1
               AND t.coordination_strategist_id IS NOT NULL",
        )
        .bind(workspace_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .flatten();
        match strategist_template {
            Some(v) => serde_json::from_value(v).unwrap_or_else(|_| WorkflowTemplate {
                mermaid: None,
                stages: vec![],
                description: None,
                synthesis: None,
                selection: None,
                nodes: vec![],
                edges: vec![],
            }),
            None => WorkflowTemplate {
                mermaid: None,
                stages: vec![],
                description: None,
                synthesis: None,
                selection: None,
                nodes: vec![],
                edges: vec![],
            },
        }
    } else {
        WorkflowTemplate {
            mermaid: None,
            stages: vec![],
            description: None,
            synthesis: None,
            selection: None,
            nodes: vec![],
            edges: vec![],
        }
    };

    let trace = execute_coordination_graph(&template, entry_input, ctx).await;

    serde_json::to_string_pretty(&trace).map_err(|e| e.to_string())
}

// ─── list_workspace_agents ────────────────────────────────────────────────────

struct ListWorkspaceAgents;

#[async_trait]
impl PlatformTool for ListWorkspaceAgents {
    fn name(&self) -> &'static str {
        "list_workspace_agents"
    }

    fn description(&self) -> &'static str {
        "List all agents that are members of the current workspace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, _input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_list_workspace_agents(ctx).await
    }
}

// ─── write_workspace_file ─────────────────────────────────────────────────────

struct WriteWorkspaceFile;

#[async_trait]
impl PlatformTool for WriteWorkspaceFile {
    fn name(&self) -> &'static str {
        "write_workspace_file"
    }

    fn description(&self) -> &'static str {
        "Write a file to the current workspace's git repository. For binary files (images), provide base64-encoded content and set is_base64 to true."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path relative to workspace root (e.g. outputs/result.png)"
                },
                "content": {
                    "type": "string",
                    "description": "File content as text, or base64-encoded string for binary files"
                },
                "is_base64": {
                    "type": "boolean",
                    "description": "If true, content is base64-encoded binary data (default: false)",
                    "default": false
                },
                "commit_message": {
                    "type": "string",
                    "description": "Git commit message (default: auto-generated)",
                    "default": ""
                }
            },
            "required": ["path", "content"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_write_workspace_file(input, ctx).await
    }
}

// ─── evaluate_coherence ───────────────────────────────────────────────────────

struct EvaluateCoherence;

#[async_trait]
impl PlatformTool for EvaluateCoherence {
    fn name(&self) -> &'static str {
        "evaluate_coherence"
    }

    fn description(&self) -> &'static str {
        "Run a Thagard Explanatory Coherence (TEC) evaluation on recent workspace messages. Classifies utterances, detects coherence/incoherence relations, runs constraint-satisfaction settling, and returns global score, 7 principle scores, and health indicators. Results are stored for history."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message_limit": {
                    "type": "integer",
                    "description": "Number of recent messages to evaluate (default: 50, max: 100)",
                    "default": 50
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_evaluate_coherence(input, ctx).await
    }
}

// ─── coherence_snapshot ───────────────────────────────────────────────────────

struct CoherenceSnapshot;

#[async_trait]
impl PlatformTool for CoherenceSnapshot {
    fn name(&self) -> &'static str {
        "coherence_snapshot"
    }

    fn description(&self) -> &'static str {
        "Get the latest stored coherence evaluation for the workspace without running a new evaluation. Returns global score, quality label, principle scores, and health indicators from the most recent evaluation."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, _input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_coherence_snapshot(ctx).await
    }
}

// ─── get_workspace_messages ───────────────────────────────────────────────────

struct GetWorkspaceMessages;

#[async_trait]
impl PlatformTool for GetWorkspaceMessages {
    fn name(&self) -> &'static str {
        "get_workspace_messages"
    }

    fn description(&self) -> &'static str {
        "Read recent messages from the workspace conversation. Returns messages with sender name, content, type, and timestamp."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 20, max: 50)",
                    "default": 20
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Workspace
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_get_workspace_messages(input, ctx).await
    }
}

// ─── Private execute functions ────────────────────────────────────────────────

async fn execute_read_workspace_file(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    // read_file is sync (git2), so run on blocking thread
    let git = Arc::clone(git);
    let slug = slug.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || git.read_file(&slug, &path))
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to read file: {}", e))
}

/// Read a single typed output from any workspace (cross-workspace read).
async fn execute_read_workspace_output(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;
    let key = input
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: key")?;

    let ws_uuid: Uuid = workspace_id
        .parse()
        .map_err(|_| "Invalid workspace_id — must be a UUID".to_string())?;

    let pool = ctx.memory_store.pool();
    let row = sqlx::query(
        "SELECT value, version, updated_at, updated_by
         FROM workspace_outputs
         WHERE workspace_id = $1 AND key = $2",
    )
    .bind(ws_uuid)
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?
    .ok_or_else(|| format!("Output '{}' not found in workspace {}", key, workspace_id))?;

    let value: serde_json::Value = row.get("value");
    let version: i32 = row.get("version");
    let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

    Ok(serde_json::json!({
        "workspace_id": workspace_id,
        "key": key,
        "value": value,
        "version": version,
        "updated_at": updated_at.to_rfc3339(),
    })
    .to_string())
}

/// List all published outputs for a workspace (cross-workspace read).
async fn execute_list_workspace_outputs(
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: workspace_id")?;

    let ws_uuid: Uuid = workspace_id
        .parse()
        .map_err(|_| "Invalid workspace_id — must be a UUID".to_string())?;

    let pool = ctx.memory_store.pool();
    let rows = sqlx::query(
        "SELECT key, value, version, updated_at
         FROM workspace_outputs
         WHERE workspace_id = $1
         ORDER BY key",
    )
    .bind(ws_uuid)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Database error: {}", e))?;

    let outputs: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "key": r.get::<String, _>("key"),
                "value": r.get::<serde_json::Value, _>("value"),
                "version": r.get::<i32, _>("version"),
                "updated_at": r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "workspace_id": workspace_id,
        "outputs": outputs,
        "count": outputs.len(),
    })
    .to_string())
}

async fn execute_list_workspace_agents(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let pool = ctx.memory_store.pool();
    // Returns typed capability info so strategist agents (moe_router, pipeline,
    // cohere_and_coordinate) can route on schema IDs rather than description
    // heuristics. `accepts` and `produces` are the schema ID arrays; the
    // `*_schema_id` fields are the canonical type names from the compiled
    // output_contract and input_contract, ready for schema-ID matching.
    let rows = sqlx::query(
        "SELECT a.agent_name,
                a.agent_type,
                a.description,
                a.accepts,
                a.produces,
                a.output_contract->>'produces_schema' AS output_schema_id,
                a.input_contract->>'accepts_schema'    AS input_schema_id
         FROM workspace_agents wa
         JOIN agents a ON wa.agent_id = a.id
         WHERE wa.workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Query failed: {}", e))?;

    let agents: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name":             row.get::<String, _>("agent_name"),
                "type":             row.get::<String, _>("agent_type"),
                "description":      row.get::<Option<String>, _>("description"),
                // Typed interface — use these for routing decisions, not description text.
                "accepts":          row.get::<Vec<String>, _>("accepts"),
                "produces":         row.get::<Vec<String>, _>("produces"),
                "input_schema_id":  row.get::<Option<String>, _>("input_schema_id"),
                "output_schema_id": row.get::<Option<String>, _>("output_schema_id"),
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_write_workspace_file(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: content")?;

    let is_base64 = input
        .get("is_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let commit_message = input
        .get("commit_message")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    let message = if commit_message.is_empty() {
        format!("agent: write {}", path)
    } else {
        commit_message.to_string()
    };

    if is_base64 {
        // Decode base64 and write as binary
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| format!("Invalid base64 content: {}", e))?;
        let size = bytes.len();

        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let commit = tokio::task::spawn_blocking(move || {
            git.commit_file_bytes(&slug, &path, &bytes, &message)
        })
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
            "size_bytes": size,
        })
        .to_string())
    } else {
        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let content = content.to_string();
        let commit =
            tokio::task::spawn_blocking(move || git.commit_file(&slug, &path, &content, &message))
                .await
                .map_err(|e| format!("Join error: {}", e))?
                .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
        })
        .to_string())
    }
}

async fn execute_evaluate_coherence(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let message_limit = input
        .get("message_limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .min(100) as i64;

    // Fetch recent messages
    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, message_limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    if messages.is_empty() {
        return Ok(json!({
            "error": "No messages in workspace to evaluate"
        })
        .to_string());
    }

    // Convert to coherence-core Messages (reverse: DB returns DESC, observer expects chronological)
    let conv_id = ConversationId(workspace_id);
    let coherence_msgs: Vec<CoherenceMessage> = messages
        .iter()
        .rev()
        .map(|m| {
            let pid = ParticipantId(
                uuid::Uuid::parse_str(&m.sender_id).unwrap_or_else(|_| Uuid::new_v4()),
            );
            CoherenceMessage::new(pid, &m.content)
        })
        .collect();

    // Run observation pipeline: classify utterances + detect relations
    let observer = ConversationObserver::new(conv_id);
    let mut system = observer.observe(&coherence_msgs);

    // Run settling engine
    let engine = SettlingEngine::with_defaults();
    let _result = engine.settle(&mut system);

    // Extract snapshot
    let snapshot = system.snapshot();

    let principle_scores = serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));

    let health_indicators = json!({
        "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
        "converged": snapshot.global_coherence.converged,
        "accepted_count": snapshot.global_coherence.accepted_count,
        "rejected_count": snapshot.global_coherence.rejected_count,
        "settling_cycles": snapshot.global_coherence.settling_cycles,
        "utterance_stats": {
            "total": snapshot.utterance_stats.total,
            "evidence_density": snapshot.utterance_stats.evidence_density(),
            "explanation_density": snapshot.utterance_stats.explanation_density(),
        },
    });

    // Store evaluation
    let eval = CoherenceEvaluation {
        eval_id: Uuid::new_v4(),
        workspace_id,
        global_score: snapshot.global_coherence.score,
        quality_label: snapshot.global_coherence.quality_label().to_string(),
        principle_scores: principle_scores.clone(),
        health_indicators: health_indicators.clone(),
        utterance_count: snapshot.utterance_stats.total as i32,
        message_window: Some(json!({
            "message_count": messages.len(),
            "from": messages.last().map(|m| m.created_at),
            "to": messages.first().map(|m| m.created_at),
        })),
        created_at: chrono::Utc::now(),
    };

    let eval_id = ctx
        .memory_store
        .store_coherence_evaluation(&eval)
        .await
        .map_err(|e| format!("Failed to store evaluation: {}", e))?;

    let result = json!({
        "eval_id": eval_id,
        "global_score": eval.global_score,
        "quality_label": eval.quality_label,
        "principle_scores": principle_scores,
        "health_indicators": health_indicators,
        "utterance_count": eval.utterance_count,
        "messages_evaluated": messages.len(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_coherence_snapshot(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let eval = ctx
        .memory_store
        .get_latest_coherence(workspace_id)
        .await
        .map_err(|e| format!("Failed to get coherence: {}", e))?;

    match eval {
        Some(e) => {
            let result = json!({
                "eval_id": e.eval_id,
                "global_score": e.global_score,
                "quality_label": e.quality_label,
                "principle_scores": e.principle_scores,
                "health_indicators": e.health_indicators,
                "utterance_count": e.utterance_count,
                "message_window": e.message_window,
                "evaluated_at": e.created_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "message": "No coherence evaluations yet for this workspace. Use evaluate_coherence to run the first evaluation."
        })
        .to_string()),
    }
}

async fn execute_get_workspace_messages(
    input: &Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .min(50) as i64;

    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    let formatted: Vec<serde_json::Value> = messages
        .iter()
        .rev() // chronological order
        .map(|m| {
            json!({
                "sender": m.sender_name.as_deref().unwrap_or(&m.sender_id),
                "sender_type": m.sender_type,
                "content": m.content,
                "type": m.message_type,
                "timestamp": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_workspace() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Workspace,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_ten() {
        assert_eq!(tools().len(), 10);
    }

    #[test]
    fn workspace_flags_are_correct() {
        let tools = tools();
        let requires: Vec<(&str, bool)> = tools
            .iter()
            .map(|t| (t.name(), t.requires_workspace()))
            .collect();

        // false: read_workspace_output, list_workspace_outputs
        for (name, flag) in &requires {
            match *name {
                "read_workspace_output" | "list_workspace_outputs" => {
                    assert!(!flag, "tool `{}` should NOT require workspace", name);
                }
                _ => {
                    assert!(flag, "tool `{}` should require workspace", name);
                }
            }
        }
    }
}
