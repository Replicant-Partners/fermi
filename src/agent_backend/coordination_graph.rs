//! Typed coordination graph executor.
//!
//! Traverses a `WorkflowTemplate` typed directed graph (`nodes` + `edges`).
//! Supports three topologies in one algorithm:
//!
//! ## Sequential pipeline
//! ```text
//! entry → A → B → C → output
//! ```
//! Each level has one node; each node receives its predecessor's output.
//!
//! ## MoE fan-out / fan-in
//! ```text
//!           ┌─ member_A ─┐
//! entry ────┼─ member_B ─┼──► synthesis → output
//!           └─ member_C ─┘
//! ```
//! All member nodes receive the same entry input and execute in the same
//! level. Their outputs are combined by the declared synthesis protocol.
//! `edges` can be empty for pure MoE (all nodes at level 0).
//!
//! ## Hybrid
//! ```text
//!                ┌─ analyst_1 ─┐
//! entry → fetch ─┤             ├─ synthesise → output
//!                └─ analyst_2 ─┘
//! ```
//! Mixed topologies compose naturally: the level algorithm handles any DAG.
//!
//! ## Level algorithm
//!
//! BFS from entry nodes (no incoming edges). A node's level = max level of
//! all its predecessors + 1. Nodes at the same level have no dependencies
//! between them and receive the same synthesised input from the level above.
//! This is the natural expression of a DAG execution plan without inventing
//! a new primitive.
//!
//! ## Synthesis at fan-in points
//!
//! When multiple predecessor outputs feed into one node (or into the final
//! result), they are combined according to `workflow_template.synthesis`:
//!
//! | protocol     | meaning                                          |
//! |---|---|
//! | `selection`  | pick the single output with the best gate status |
//! | `aggregation`| wrap all outputs in a JSON array                 |
//! | `cep_weighted`| same shape as aggregation; weighting is upstream |
//! | `max_risk`   | pick the output with the highest risk finding    |
//! | `pipeline`   | last node's output only (no fan-in needed)       |

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::agent_backend::agent_card::{CoordinationEdge, CoordinationNode, WorkflowTemplate};
use crate::agent_backend::tools::ToolContext;

// ─── Output types ─────────────────────────────────────────────────────────────

/// Record of a single node's execution within a coordination graph run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub node_id: String,
    /// Agent that ran. `None` if no candidate was found for an open slot.
    pub agent: Option<String>,
    /// How the agent was determined: `"pinned"` | `"select_agent"` | `"no_candidate"`.
    pub selected_by: String,
    pub input_schema: Option<String>,
    pub output_schema: Option<String>,
    /// Gate verdict on the input before dispatch.
    pub gate_input: String,
    /// Gate verdict on the output after dispatch.
    pub gate_output: String,
    pub duration_ms: u64,
    /// Raw JSON output from the agent's envelope, if successful.
    pub output: Option<Value>,
    /// Failure message if this step could not complete.
    pub failure: Option<String>,
}

/// Topology of the graph — detected automatically from the node/edge structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    /// Single path: A → B → C.
    Pipeline,
    /// All nodes at the same level, same input, synthesised outputs.
    FanOut,
    /// Mixed levels with convergence points.
    Hybrid,
}

/// Full trace of a coordination graph execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinationTrace {
    /// Detected topology of the graph.
    pub topology: Topology,
    /// Synthesis protocol declared on the graph.
    pub synthesis: Option<String>,
    /// Per-step execution records, in traversal order.
    pub steps: Vec<TraceStep>,
    /// Node IDs where `select_agent` found no candidates.
    pub open_slots: Vec<String>,
    /// Node ID where the first failure occurred, if any.
    pub failure_at: Option<String>,
    pub failure: Option<String>,
    pub final_output_schema: Option<String>,
    pub final_output: Option<Value>,
}

// ─── Entry point ──────────────────────────────────────────────────────────────

pub async fn execute_coordination_graph(
    template: &WorkflowTemplate,
    entry_input: &str,
    ctx: &ToolContext,
) -> CoordinationTrace {
    if template.nodes.is_empty() {
        return CoordinationTrace {
            topology: Topology::Pipeline,
            synthesis: template.synthesis.clone(),
            steps: vec![],
            open_slots: vec![],
            failure_at: None,
            failure: Some(
                "This workflow_template has no typed nodes. Declare `nodes` and `edges` to \
                 use the graph executor. The legacy `stages` list is not traversed by the \
                 executor — it is still read by pipeline_strategist's seam check."
                    .to_string(),
            ),
            final_output_schema: None,
            final_output: None,
        };
    }

    // 1. Assign nodes to levels.
    let levels = assign_levels(&template.nodes, &template.edges);
    let topology = detect_topology(&levels);
    let synthesis = template.synthesis.as_deref().unwrap_or("selection");

    // 2. Execute level by level, collecting outputs.
    let mut node_outputs: HashMap<String, Value> = HashMap::new();
    let mut all_steps: Vec<TraceStep> = Vec::new();
    let mut open_slots: Vec<String> = Vec::new();

    for level in &levels {
        // Determine what input to give nodes at this level.
        // Entry nodes (no predecessors) always receive the original entry_input.
        // Downstream nodes receive a synthesis of their predecessors' outputs.
        let has_incoming: HashSet<String> = template.edges.iter().map(|e| e.to.clone()).collect();

        for node in level {
            let node_input = if !has_incoming.contains(&node.id) {
                // Entry node: original input.
                entry_input.to_string()
            } else {
                // Fan-in: collect outputs of all this node's predecessors.
                let predecessor_outputs: Vec<Value> = template
                    .edges
                    .iter()
                    .filter(|e| e.to == node.id)
                    .filter_map(|e| node_outputs.get(&e.from))
                    .cloned()
                    .collect();
                synthesise_inputs(&predecessor_outputs, synthesis, entry_input)
            };

            let step = execute_node(node, &node_input, ctx).await;

            if let Some(ref out) = step.output {
                node_outputs.insert(node.id.clone(), out.clone());
            }
            if step.selected_by == "no_candidate" {
                open_slots.push(node.id.clone());
            }
            let failed = step.failure.is_some();
            let failure_msg = step.failure.clone();
            let node_id = node.id.clone();
            all_steps.push(step);

            if failed {
                return CoordinationTrace {
                    topology,
                    synthesis: template.synthesis.clone(),
                    steps: all_steps,
                    open_slots,
                    failure_at: Some(node_id),
                    failure: failure_msg,
                    final_output_schema: None,
                    final_output: None,
                };
            }
        }
    }

    // 3. Final output: synthesise all leaf-node outputs (nodes with no outgoing edges).
    let has_outgoing: HashSet<String> = template.edges.iter().map(|e| e.from.clone()).collect();
    let leaf_outputs: Vec<Value> = template
        .nodes
        .iter()
        .filter(|n| !has_outgoing.contains(&n.id))
        .filter_map(|n| node_outputs.get(&n.id))
        .cloned()
        .collect();

    let final_output = synthesise_outputs(&leaf_outputs, synthesis);
    let final_output_schema = all_steps.last().and_then(|s| s.output_schema.clone());

    CoordinationTrace {
        topology,
        synthesis: template.synthesis.clone(),
        steps: all_steps,
        open_slots,
        failure_at: None,
        failure: None,
        final_output_schema,
        final_output,
    }
}

// ─── Level assignment ─────────────────────────────────────────────────────────

/// Assign nodes to BFS levels so all nodes at the same level can execute
/// with the same upstream inputs.
///
/// Level = max(predecessor levels) + 1. Entry nodes (no predecessors) are
/// level 0. Disconnected nodes get the maximum level + 1 so they run last.
fn assign_levels<'a>(
    nodes: &'a [CoordinationNode],
    edges: &[CoordinationEdge],
) -> Vec<Vec<&'a CoordinationNode>> {
    if edges.is_empty() {
        // No edges: all nodes are peers (pure fan-out).
        return vec![nodes.iter().collect()];
    }

    let has_incoming: HashSet<&str> = edges.iter().map(|e| e.to.as_str()).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in edges {
        adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
    }

    // BFS from entry nodes; assign max level.
    let mut level_of: HashMap<&str, usize> = HashMap::new();
    let mut queue: VecDeque<(&str, usize)> = nodes
        .iter()
        .filter(|n| !has_incoming.contains(n.id.as_str()))
        .map(|n| (n.id.as_str(), 0))
        .collect();

    while let Some((id, lvl)) = queue.pop_front() {
        let entry = level_of.entry(id).or_insert(lvl);
        if lvl > *entry {
            *entry = lvl;
        }
        for &child in adj.get(id).map(|v| v.as_slice()).unwrap_or(&[]) {
            queue.push_back((child, lvl + 1));
        }
    }

    // Nodes unreachable from entry nodes get max_level + 1.
    let max_lvl = level_of.values().copied().max().unwrap_or(0);
    for n in nodes {
        level_of.entry(n.id.as_str()).or_insert(max_lvl + 1);
    }

    // Group into level buckets; sort each bucket in declaration order.
    let max_lvl = level_of.values().copied().max().unwrap_or(0);
    let mut buckets: Vec<Vec<&CoordinationNode>> = vec![vec![]; max_lvl + 2];
    for n in nodes {
        let lvl = *level_of.get(n.id.as_str()).unwrap_or(&0);
        buckets[lvl].push(n);
    }
    buckets.into_iter().filter(|b| !b.is_empty()).collect()
}

fn detect_topology(levels: &[Vec<&CoordinationNode>]) -> Topology {
    let all_single = levels.iter().all(|l| l.len() == 1);
    if all_single {
        Topology::Pipeline
    } else if levels.len() == 1 {
        Topology::FanOut
    } else {
        Topology::Hybrid
    }
}

// ─── Synthesis ────────────────────────────────────────────────────────────────

/// Combine predecessor outputs into one input string for the next node.
///
/// For a fan-in node: synthesise what came before into something the next
/// node can consume. The exact format depends on the synthesis protocol.
fn synthesise_inputs(predecessor_outputs: &[Value], synthesis: &str, entry_input: &str) -> String {
    match predecessor_outputs.len() {
        0 => entry_input.to_string(),
        1 => serde_json::to_string_pretty(&predecessor_outputs[0])
            .unwrap_or_else(|_| entry_input.to_string()),
        _ => {
            let combined = synthesise_outputs(predecessor_outputs, synthesis);
            serde_json::to_string_pretty(&combined).unwrap_or_else(|_| entry_input.to_string())
        }
    }
}

/// Combine leaf outputs into the final output according to synthesis protocol.
///
/// | protocol      | result                                                     |
/// |---|---|
/// | `selection`   | the single output with the best gate status (valid > rest) |
/// | `aggregation` | array of all outputs                                       |
/// | `cep_weighted`| array of all outputs (upstream handles weighting)          |
/// | `max_risk`    | output whose top-level `risk` or `severity` is highest     |
/// | `pipeline`    | the last output (only one in a pipeline)                   |
fn synthesise_outputs(outputs: &[Value], synthesis: &str) -> Option<Value> {
    if outputs.is_empty() {
        return None;
    }
    if outputs.len() == 1 {
        return Some(outputs[0].clone());
    }
    match synthesis {
        "aggregation" | "cep_weighted" => Some(json!({ "members": outputs })),
        "max_risk" => {
            // Pick the output with the highest risk/severity field.
            let scored = outputs.iter().max_by_key(|v| {
                v.get("risk")
                    .or_else(|| v.get("severity"))
                    .and_then(|r| r.as_str())
                    .map(|s| match s {
                        "critical" | "high" => 3usize,
                        "medium" => 2,
                        "low" => 1,
                        _ => 0,
                    })
                    .unwrap_or(0)
            });
            scored.cloned()
        }
        // "selection" and "pipeline" and default: pick the first/best.
        // For selection the LLM strategist chooses the best from the trace;
        // here we return all members so it has the full picture.
        _ => Some(json!({ "candidates": outputs })),
    }
}

// ─── Node execution ───────────────────────────────────────────────────────────

async fn execute_node(node: &CoordinationNode, input: &str, ctx: &ToolContext) -> TraceStep {
    let start = Instant::now();

    let (agent_name, selected_by) = match resolve_agent(node, ctx).await {
        Ok((name, method)) => (name, method),
        Err(reason) => {
            return TraceStep {
                node_id: node.id.clone(),
                agent: None,
                selected_by: "no_candidate".to_string(),
                input_schema: node.input_schema.clone(),
                output_schema: node.output_schema.clone(),
                gate_input: "unverified_no_candidate".to_string(),
                gate_output: "unverified_no_candidate".to_string(),
                duration_ms: start.elapsed().as_millis() as u64,
                output: None,
                failure: Some(reason),
            };
        }
    };

    let exec_input = json!({ "agent_id": agent_name, "query": input });
    let result =
        crate::agent_backend::tools::domains::platform::execute_execute_agent(&exec_input, ctx)
            .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(raw_json) => {
            let parsed: Value =
                serde_json::from_str(&raw_json).unwrap_or_else(|_| json!({ "raw": raw_json }));
            let gate_input = parsed
                .pointer("/envelope/validation/input_status")
                .and_then(|v| v.as_str())
                .unwrap_or("unverified_no_schema")
                .to_string();
            let gate_output = parsed
                .pointer("/envelope/validation/status")
                .and_then(|v| v.as_str())
                .unwrap_or("unverified_no_schema")
                .to_string();
            let output_payload = parsed
                .pointer("/envelope/payload")
                .cloned()
                .or_else(|| Some(parsed.clone()));
            TraceStep {
                node_id: node.id.clone(),
                agent: Some(agent_name),
                selected_by,
                input_schema: node.input_schema.clone(),
                output_schema: node.output_schema.clone(),
                gate_input,
                gate_output,
                duration_ms,
                output: output_payload,
                failure: None,
            }
        }
        Err(e) => TraceStep {
            node_id: node.id.clone(),
            agent: Some(agent_name),
            selected_by,
            input_schema: node.input_schema.clone(),
            output_schema: node.output_schema.clone(),
            gate_input: "unverified_no_schema".to_string(),
            gate_output: "unverified_no_schema".to_string(),
            duration_ms,
            output: None,
            failure: Some(e),
        },
    }
}

// ─── Agent resolution ─────────────────────────────────────────────────────────

async fn resolve_agent(
    node: &CoordinationNode,
    ctx: &ToolContext,
) -> Result<(String, String), String> {
    if let Some(ref name) = node.agent {
        if node.pinned || node.input_schema.is_none() {
            return Ok((name.clone(), "pinned".to_string()));
        }
    }

    if let Some(ref schema_id) = node.input_schema {
        let select_input = json!({ "input_schema_id": schema_id });
        match crate::agent_backend::tools::domains::workspace::execute_select_agent(
            &select_input,
            ctx,
        )
        .await
        {
            Ok(raw) => {
                if let Ok(parsed) = serde_json::from_str::<Value>(&raw) {
                    if let Some(top) = parsed
                        .get("candidates")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                    {
                        if let Some(name) = top.get("agent").and_then(|v| v.as_str()) {
                            return Ok((name.to_string(), "select_agent".to_string()));
                        }
                    }
                }
                if let Some(ref name) = node.agent {
                    return Ok((name.clone(), "pinned".to_string()));
                }
                Err(format!(
                    "No candidates for input_schema '{}' on node '{}'.",
                    schema_id, node.id
                ))
            }
            Err(e) => {
                if let Some(ref name) = node.agent {
                    return Ok((name.clone(), "pinned".to_string()));
                }
                Err(format!(
                    "select_agent failed for node '{}': {}.",
                    node.id, e
                ))
            }
        }
    } else {
        Err(format!(
            "Node '{}' has neither a bound agent nor an input_schema.",
            node.id
        ))
    }
}
