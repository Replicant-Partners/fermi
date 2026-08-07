//! Workspace coherence evaluation, ontology, files, git log, and workflow.
//! Workspace handlers — shared imports.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge, credit_charge_purchased_only, credit_deposit_typed, get_or_create_wallet, teams,
    AuthPrincipal,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::convert::Infallible;
use std::sync::Arc;

use agent_bestiary_memory::{Agent, CoherenceEvaluation, WorkspaceMessage};
use agent_bestiary_ontology::WorkspaceGitManager;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use super::core::{charge_workspace_gas, get_workspace_slug, parse_at_mention};
use super::messages::{broadcast_message, message_to_json};
use crate::handlers::agents::CreateAgentRequest;
use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

// ─── Coherence Evaluation ────────────────────────────────────────────

/// Run TEC coherence evaluation on recent workspace messages.
/// Supports tiered depth: "index" (free), "recommendations" (2cr), "dream_notes" (5cr).
pub async fn evaluate_coherence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Verify membership
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Parse depth tier (default: "index" which is free)
    let depth = body
        .as_ref()
        .and_then(|b| b.get("depth"))
        .and_then(|d| d.as_str())
        .unwrap_or("index")
        .to_string();

    let credit_cost = match depth.as_str() {
        "index" => 0,
        "recommendations" => 2,
        "dream_notes" => 5,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Invalid depth '{}'. Use: index, recommendations, dream_notes",
                    depth
                ),
            ))
        }
    };

    // Charge credits if tier requires it
    if credit_cost > 0 {
        charge_workspace_gas(
            &state.db,
            ws_uuid,
            &workspace_id,
            credit_cost,
            "gas_fee",
            &format!("Coherence evaluation ({})", depth),
            None,
        )
        .await?;
    }

    // Fetch recent messages (last 50)
    let messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, 50, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if messages.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No messages in workspace to evaluate".to_string(),
        ));
    }

    // Convert workspace messages to coherence-core Messages
    let conv_id = ConversationId(ws_uuid);
    let coherence_messages: Vec<CoherenceMessage> = messages
        .iter()
        .rev() // messages come DESC, observer expects chronological
        .map(|m| {
            let pid = ParticipantId(
                uuid::Uuid::parse_str(&m.sender_id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            );
            CoherenceMessage::new(pid, &m.content)
        })
        .collect();

    // Run observation pipeline: classify utterances + detect relations
    let observer = ConversationObserver::new(conv_id);
    let mut system = observer.observe(&coherence_messages);

    // Run settling engine
    let engine = SettlingEngine::with_defaults();
    let _result = engine.settle(&mut system);

    // Extract snapshot
    let snapshot = system.snapshot();

    // Build principle scores JSON
    let principle_scores: serde_json::Value =
        serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));

    // Build health indicators
    let health_indicators = json!({
        "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
        "converged": snapshot.global_coherence.converged,
        "accepted_count": snapshot.global_coherence.accepted_count,
        "rejected_count": snapshot.global_coherence.rejected_count,
        "evidence_density": snapshot.utterance_stats.evidence_density(),
        "explanation_density": snapshot.utterance_stats.explanation_density(),
    });

    // Store evaluation
    let eval = CoherenceEvaluation {
        eval_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
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

    let eval_id = state
        .memory_store
        .store_coherence_evaluation(&eval)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // For premium tiers, invoke cohere_and_coordinate directly.
    // This replaces the former coherence_consultant sub-call per
    // docs/architecture/LEARNING_MECHANICS_SIMPLIFICATION.md.
    let consultant_output = if depth == "recommendations" || depth == "dream_notes" {
        match state.registry.get("cohere_and_coordinate") {
            Ok(card) => {
                let msg_summary: String = messages
                    .iter()
                    .rev()
                    .take(20)
                    .map(|m| {
                        format!(
                            "[{}]: {}",
                            m.sender_name.as_deref().unwrap_or("?"),
                            m.content
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                let query_text = if depth == "recommendations" {
                    format!(
                        "Coherence score: {:.0}% ({}). Principle scores: {:?}. Health indicators: {:?}.\n\n\
                         Recent conversation:\n{}\n\n\
                         Run Stage 2 (Diagnose) and Stage 3 (Coordinate): identify which TEC principles \
                         are weak, classify any incoherence as destructive vs productive, and provide \
                         specific actionable recommendations for improving workspace coherence. \
                         Distinguish productive tension (protect it) from destructive incoherence (fix it).",
                        eval.global_score * 100.0, eval.quality_label,
                        principle_scores, health_indicators, msg_summary,
                    )
                } else {
                    format!(
                        "Coherence score: {:.0}% ({}). Principle scores: {:?}. Health indicators: {:?}.\n\n\
                         Full workspace conversation:\n{}\n\n\
                         Write dream notes for this workspace session: a narrative synthesis of what \
                         this team has learned together, connections made between ideas, knowledge gaps \
                         identified, recurring coherence patterns, and emerging themes. \
                         Write in first person as the workspace strategist reflecting on the session.",
                        eval.global_score * 100.0, eval.quality_label,
                        principle_scores, health_indicators, msg_summary,
                    )
                };

                let agent_stmt = ast::AgentStmt {
                    name: "cohere_and_coordinate".to_string(),
                    agent_type: Some(card.agent_type.clone()),
                    query: query_text,
                    executor: Some(ast::ExecutorType::LLM),
                    schedule: None,
                    driver_refs: vec![],
                    depends_on: vec![],
                    confidence_threshold: None,
                };
                let program = ast::Program {
                    statements: vec![ast::Statement::Agent(agent_stmt.clone())],
                };
                // SPEC_28 — this path calls `registry.execute_agent`
                // directly (no ToolContext), so before this change it had
                // no way to carry credentials at all and always drew on
                // the platform's env key. `cohere_and_coordinate` is a
                // platform-service agent, so resolving its DB row funds it
                // from the `abw-system` principal's store.
                let credentials = match crate::resolve_agent(&state, "cohere_and_coordinate").await
                {
                    Ok(db_agent) => {
                        crate::build_execution_credentials(&state, &db_agent, &card).await
                    }
                    // Not registered in the DB: unfunded, which fails
                    // loudly below rather than silently spending.
                    Err(_) => {
                        fermi::agent_backend::credentials::ResolvedCredentials::unfunded_arc()
                    }
                };

                let context = ExecutionContext {
                    program,
                    agent_card: card,
                    creature_id: None,
                    cognition_tier: None,
                    credentials,
                };
                match state.registry.execute_agent(&agent_stmt, &context).await {
                    Ok(output) => output.metadata.reasoning,
                    Err(e) => {
                        eprintln!("cohere_and_coordinate failed: {:?}", e);
                        Some(format!("Strategist unavailable: {:?}", e))
                    }
                }
            }
            Err(_) => Some("cohere_and_coordinate agent not available".to_string()),
        }
    } else {
        None
    };

    // Post coherence update to workspace chat
    let chat_content = if let Some(ref consultant) = consultant_output {
        format!(
            "Coherence: {:.0}% ({}) | {} utterances | {}\n\n{}",
            eval.global_score * 100.0,
            eval.quality_label,
            eval.utterance_count,
            snapshot.feedback_action,
            consultant,
        )
    } else {
        format!(
            "Coherence: {:.0}% ({}) | {} utterances | {}",
            eval.global_score * 100.0,
            eval.quality_label,
            eval.utterance_count,
            snapshot.feedback_action,
        )
    };

    let (sender_id, sender_name) = if consultant_output.is_some() {
        (
            "cohere_and_coordinate".to_string(),
            "Cohere & Coordinate".to_string(),
        )
    } else {
        (
            "coherence_evaluator".to_string(),
            "Coherence Evaluator".to_string(),
        )
    };

    let update_msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "system".to_string(),
        sender_id,
        sender_name: Some(sender_name),
        content: chat_content,
        message_type: "coherence_update".to_string(),
        metadata: json!({
            "eval_id": eval_id,
            "depth": depth,
            "global_score": eval.global_score,
            "quality_label": eval.quality_label,
            "principle_scores": principle_scores,
            "health_indicators": health_indicators,
        }),
        created_at: chrono::Utc::now(),
    };

    let _ = state
        .memory_store
        .store_workspace_message(&update_msg)
        .await;
    broadcast_message(&state, ws_uuid, &message_to_json(&update_msg));

    let mut response = json!({
        "eval_id": eval_id,
        "depth": depth,
        "credits_charged": credit_cost,
        "global_score": eval.global_score,
        "quality_label": eval.quality_label,
        "principle_scores": principle_scores,
        "health_indicators": health_indicators,
        "utterance_count": eval.utterance_count,
        "message_window": eval.message_window,
    });

    if let Some(ref consultant) = consultant_output {
        if let Some(obj) = response.as_object_mut() {
            obj.insert(
                if depth == "recommendations" {
                    "recommendations"
                } else {
                    "dream_notes"
                }
                .to_string(),
                json!(consultant),
            );
        }
    }

    Ok(Json(response))
}

/// Get latest coherence evaluation for a workspace.
pub async fn get_coherence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let eval = state
        .memory_store
        .get_latest_coherence(ws_uuid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match eval {
        Some(e) => Ok(Json(json!({
            "eval_id": e.eval_id,
            "global_score": e.global_score,
            "quality_label": e.quality_label,
            "principle_scores": e.principle_scores,
            "health_indicators": e.health_indicators,
            "utterance_count": e.utterance_count,
            "message_window": e.message_window,
            "created_at": e.created_at,
        }))),
        None => Ok(Json(
            json!({ "eval_id": null, "message": "No evaluations yet" }),
        )),
    }
}

/// Get coherence evaluation history for a workspace.
pub async fn get_coherence_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let limit = params.limit.unwrap_or(20).min(100);

    let evals = state
        .memory_store
        .get_coherence_history(ws_uuid, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = evals
        .iter()
        .map(|e| {
            json!({
                "eval_id": e.eval_id,
                "global_score": e.global_score,
                "quality_label": e.quality_label,
                "principle_scores": e.principle_scores,
                "health_indicators": e.health_indicators,
                "utterance_count": e.utterance_count,
                "created_at": e.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "evaluations": items })))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    limit: Option<i64>,
}

/// Merge ontology snapshots from all agents in a workspace into a combined view
pub async fn get_workspace_ontology_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Get all agents in workspace with their latest ontology snapshots
    let rows = sqlx::query(
        "SELECT a.agent_name, a.display_alias, os.version, os.mermaid_content, os.dream_synopsis, os.entity_count, os.fact_count, os.created_at
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         LEFT JOIN LATERAL (
            SELECT * FROM ontology_snapshots
            WHERE agent_id = a.agent_id
            ORDER BY created_at DESC LIMIT 1
         ) os ON true
         WHERE wa.workspace_id = $1"
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent_ontologies = Vec::new();
    let mut merged_mermaid_parts = Vec::new();
    let mut total_entities = 0i32;
    let mut total_facts = 0i32;

    for row in &rows {
        let agent_name: String = row.try_get("agent_name").unwrap_or_default();
        let display_alias: Option<String> = row.try_get("display_alias").unwrap_or(None);
        let version: Option<i32> = row.try_get("version").unwrap_or(None);
        let mermaid: Option<String> = row.try_get("mermaid_content").unwrap_or(None);
        let synopsis: Option<String> = row.try_get("dream_synopsis").unwrap_or(None);
        let entities: Option<i32> = row.try_get("entity_count").unwrap_or(None);
        let facts: Option<i32> = row.try_get("fact_count").unwrap_or(None);

        total_entities += entities.unwrap_or(0);
        total_facts += facts.unwrap_or(0);

        if let Some(ref m) = mermaid {
            // Extract relationship lines from mermaid (skip the erDiagram header)
            let lines: Vec<&str> = m
                .lines()
                .filter(|l| {
                    !l.trim().is_empty()
                        && !l.trim().starts_with("erDiagram")
                        && !l.trim().starts_with("%%")
                })
                .collect();
            if !lines.is_empty() {
                merged_mermaid_parts.push(format!("    %% {} %%", agent_name));
                merged_mermaid_parts.extend(lines.iter().map(|l| l.to_string()));
            }
        }

        agent_ontologies.push(json!({
            "agent_name": agent_name,
            "display_alias": display_alias,
            "version": version,
            "entity_count": entities,
            "fact_count": facts,
            "dream_synopsis": synopsis,
            "has_ontology": mermaid.is_some(),
        }));
    }

    let merged_mermaid = if merged_mermaid_parts.is_empty() {
        None
    } else {
        Some(format!("erDiagram\n{}", merged_mermaid_parts.join("\n")))
    };

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "agent_count": rows.len(),
        "total_entities": total_entities,
        "total_facts": total_facts,
        "merged_mermaid": merged_mermaid,
        "agent_ontologies": agent_ontologies,
    })))
}

// ---------------------------------------------------------------------------
// Workspace Git / Files handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FilesQuery {
    path: Option<String>,
}

pub async fn list_workspace_files_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<FilesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let files = state
        .workspace_git
        .list_files(&slug, query.path.as_deref())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = files
        .iter()
        .map(|f| {
            json!({
                "path": f.path,
                "name": f.name,
                "is_dir": f.is_dir,
                "size": f.size,
            })
        })
        .collect();

    Ok(Json(json!({ "files": items })))
}

pub async fn read_workspace_file_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path((workspace_id, file_path)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let content = state
        .workspace_git
        .read_file(&slug, &file_path)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    Ok(Json(json!({
        "path": file_path,
        "content": content,
    })))
}

/// Serve workspace files as raw bytes with correct Content-Type.
/// Used for images and other binary files.
pub async fn read_workspace_file_raw_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path((workspace_id, file_path)): Path<(String, String)>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let bytes = state
        .workspace_git
        .read_file_bytes(&slug, &file_path)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Determine content type from extension
    let content_type = match file_path.rsplit('.').next().unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" | "md" => "text/plain; charset=utf-8",
        "html" => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    };

    Ok(axum::response::Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Length", bytes.len().to_string())
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

#[derive(Debug, Deserialize)]
pub struct WriteFileBody {
    content: String,
    #[serde(default)]
    is_base64: bool,
    message: Option<String>,
}

pub async fn write_workspace_file_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, file_path)): Path<(String, String)>,
    Json(body): Json<WriteFileBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    // Charge gas for file write
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.file_write,
        "file_write",
        &format!("Write file: {}", file_path),
        None,
    )
    .await?;

    let commit_msg = body
        .message
        .unwrap_or_else(|| format!("{} updated {}", principal.user_id(), file_path));

    let commit = if body.is_base64 {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&body.content)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid base64: {}", e)))?;
        state
            .workspace_git
            .commit_file_bytes(&slug, &file_path, &bytes, &commit_msg)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        state
            .workspace_git
            .commit_file(&slug, &file_path, &body.content, &commit_msg)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    // Update git tracking columns
    let _ = sqlx::query(
        "UPDATE teams SET git_latest_commit = $1, git_commit_count = git_commit_count + 1 WHERE id = $2",
    )
    .bind(&commit.sha)
    .bind(ws_uuid)
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "path": file_path,
        "commit": {
            "sha": commit.sha,
            "message": commit.message,
            "timestamp": commit.timestamp,
        },
    })))
}

#[derive(Debug, Deserialize)]
pub struct GitLogQuery {
    limit: Option<usize>,
}

pub async fn workspace_git_log_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitLogQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;
    let limit = query.limit.unwrap_or(20);

    let log = state
        .workspace_git
        .get_log(&slug, limit)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items: Vec<Value> = log
        .iter()
        .map(|c| {
            json!({
                "sha": c.sha,
                "message": c.message,
                "timestamp": c.timestamp,
                "author": c.author,
            })
        })
        .collect();

    Ok(Json(json!({ "commits": items })))
}

#[derive(Debug, Deserialize)]
pub struct GitDiffQuery {
    from: String,
    to: String,
}

pub async fn workspace_git_diff_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let diff = state
        .workspace_git
        .diff_commits(&slug, &query.from, &query.to)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "from": query.from,
        "to": query.to,
        "diff": diff,
    })))
}

// ─── File Upload (multipart) ───────────────────────────────────────

/// Blocked file extensions (security)
const BLOCKED_EXTENSIONS: &[&str] = &[".exe", ".sh", ".bat", ".dll", ".so", ".cmd", ".ps1"];

/// Max file size: 5 MB
const MAX_UPLOAD_SIZE: usize = 5 * 1024 * 1024;

pub async fn upload_workspace_file_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let slug = get_workspace_slug(&state.db, ws_uuid).await?;

    let mut uploaded = Vec::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("file").to_string();
        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue, // skip non-file fields
        };

        // Read bytes
        let bytes = field.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Failed to read file data: {}", e),
            )
        })?;

        // Validate: not empty
        if bytes.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("File '{}' is empty", file_name),
            ));
        }

        // Validate: size limit
        if bytes.len() > MAX_UPLOAD_SIZE {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "File '{}' is {} MB, max is 5 MB",
                    file_name,
                    bytes.len() / (1024 * 1024)
                ),
            ));
        }

        // Validate: blocked extensions
        let lower_name = file_name.to_lowercase();
        for ext in BLOCKED_EXTENSIONS {
            if lower_name.ends_with(ext) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("File type '{}' is not allowed", ext),
                ));
            }
        }

        // Sanitize filename — strip path traversal
        let safe_name = file_name
            .replace("..", "")
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        let safe_name = safe_name
            .rsplit('/')
            .next()
            .unwrap_or(&safe_name)
            .to_string();

        if safe_name.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "Invalid file name".to_string()));
        }

        // Determine target path based on field name
        let target_path = if field_name == "context" {
            format!("context/{}", safe_name)
        } else {
            format!("uploads/{}", safe_name)
        };

        // Calculate fee
        let fee = state.gas_fees.upload_fee(bytes.len());

        // Charge gas BEFORE writing (fail-fast on insufficient credits)
        charge_workspace_gas(
            &state.db,
            ws_uuid,
            &workspace_id,
            fee,
            "file_write",
            &format!("Upload file: {} ({}B)", safe_name, bytes.len()),
            None,
        )
        .await?;

        // Write to git repo
        let commit_msg = format!("{} uploaded {}", principal.user_id(), target_path);
        let commit = state
            .workspace_git
            .commit_file_bytes(&slug, &target_path, &bytes, &commit_msg)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        // Update git tracking
        let _ = sqlx::query(
            "UPDATE teams SET git_latest_commit = $1, git_commit_count = git_commit_count + 1 WHERE id = $2",
        )
        .bind(&commit.sha)
        .bind(ws_uuid)
        .execute(&state.db)
        .await;

        uploaded.push(json!({
            "path": target_path,
            "size": bytes.len(),
            "fee": fee,
            "commit_sha": commit.sha,
        }));
    }

    if uploaded.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "No files were uploaded".to_string(),
        ));
    }

    let total_fee: i32 = uploaded
        .iter()
        .filter_map(|u| u["fee"].as_i64())
        .map(|f| f as i32)
        .sum();

    Ok(Json(json!({
        "uploaded": uploaded,
        "total_fee": total_fee,
    })))
}

// ─── Workflow Visualization ──────────────────────────────────────────

/// Auto-generate a mermaid sequence diagram + companion metadata from workspace messages.
fn generate_workflow_from_messages(messages: &[WorkspaceMessage]) -> (String, Value) {
    use std::collections::BTreeMap;

    // Participant registry: name → type
    let mut participants: BTreeMap<String, Value> = BTreeMap::new();
    let mut lines: Vec<String> = Vec::new();

    // Sanitize names for mermaid (no spaces, no special chars)
    fn safe_name(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    // Truncate label for arrow text
    fn label(s: &str, max: usize) -> String {
        let clean: String = s.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        let trimmed = clean.trim();
        if trimmed.len() <= max {
            trimmed.to_string()
        } else {
            format!("{}...", &trimmed[..max])
        }
    }

    // Detect UUID-like strings and replace with "User"
    fn display_name(name: Option<&str>, fallback: &str) -> String {
        let raw = name.unwrap_or(fallback);
        // If it looks like a UUID (32+ hex chars with hyphens/underscores), use "User"
        if raw.len() >= 32
            && raw
                .chars()
                .all(|c| c.is_ascii_hexdigit() || c == '-' || c == '_')
        {
            "User".to_string()
        } else {
            raw.to_string()
        }
    }

    for msg in messages {
        match msg.message_type.as_str() {
            "agent_invocation" => {
                // User → Agent invocation
                let user_name =
                    safe_name(&display_name(msg.sender_name.as_deref(), &msg.sender_id));
                participants
                    .entry(user_name.clone())
                    .or_insert_with(|| json!({"type": "human"}));

                // Extract target agent from @mention
                if let Some((agent, _query)) = parse_at_mention(&msg.content) {
                    let agent_safe = safe_name(&agent);
                    participants
                        .entry(agent_safe.clone())
                        .or_insert_with(|| json!({"type": "agent"}));
                    lines.push(format!(
                        "    {}->>{}:{}",
                        user_name,
                        agent_safe,
                        label(&msg.content, 60)
                    ));
                }
            }
            "execution_result" => {
                // Agent → User response
                let agent_safe = safe_name(&msg.sender_id);
                participants
                    .entry(agent_safe.clone())
                    .or_insert_with(|| json!({"type": "agent"}));

                // Check for tool invocations in metadata
                if let Some(tools) = msg
                    .metadata
                    .get("tool_invocations")
                    .and_then(|v| v.as_array())
                {
                    for tool in tools {
                        if let Some(tool_name) = tool.get("tool_name").and_then(|v| v.as_str()) {
                            let tool_safe = safe_name(tool_name);
                            participants
                                .entry(tool_safe.clone())
                                .or_insert_with(|| json!({"type": "tool"}));
                            lines.push(format!("    {}->>{}: invoke", agent_safe, tool_safe));
                            lines.push(format!("    {}-->>{}:result", tool_safe, agent_safe));
                        }
                    }
                }

                // Find the most recent human participant for the return arrow
                let user_name = participants
                    .iter()
                    .find(|(_, v)| v.get("type").and_then(|t| t.as_str()) == Some("human"))
                    .map(|(k, _)| k.clone())
                    .unwrap_or_else(|| "User".to_string());

                let conf_label = msg
                    .metadata
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .map(|c| format!(" ({}%)", (c * 100.0) as i32))
                    .unwrap_or_default();

                lines.push(format!(
                    "    {}-->>{}:{}{}",
                    agent_safe,
                    user_name,
                    label(&msg.content, 40),
                    conf_label
                ));
            }
            _ => {} // Skip chat, system_event, coherence_update
        }
    }

    // Build mermaid text
    let mut mermaid = String::from("sequenceDiagram\n");
    for (name, _) in &participants {
        mermaid.push_str(&format!("    participant {}\n", name));
    }
    for line in &lines {
        mermaid.push_str(line);
        mermaid.push('\n');
    }

    // Build meta
    let meta = json!({
        "participants": participants,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "message_count": messages.len(),
    });

    (mermaid, meta)
}

pub async fn get_workspace_workflow_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Auth: must be a member
    let role = fermi_auth::teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if role.is_none() {
        return Err((StatusCode::FORBIDDEN, "Not a workspace member".to_string()));
    }

    // Check for existing scaffold (injected when compound agent was hired)
    let existing: Option<(Option<String>, Option<serde_json::Value>)> =
        sqlx::query_as("SELECT workflow_mermaid, workflow_meta FROM teams WHERE id = $1")
            .bind(ws_uuid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    if let Some((Some(ref mermaid), Some(ref meta))) = existing {
        if !mermaid.is_empty()
            && meta
                .get("is_scaffold")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            // Scaffold exists — return it as-is (don't regenerate from messages)
            return Ok(Json(json!({
                "mermaid": mermaid,
                "meta": meta,
            })));
        }
    }

    // No scaffold — generate from messages (Layer 1 behavior)
    let mut messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, 500, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    messages.reverse(); // store returns DESC, we need ASC

    let (mermaid_text, meta) = generate_workflow_from_messages(&messages);

    // Cache on the teams record
    let _ = sqlx::query("UPDATE teams SET workflow_mermaid = $1, workflow_meta = $2 WHERE id = $3")
        .bind(&mermaid_text)
        .bind(&meta)
        .bind(ws_uuid)
        .execute(&state.db)
        .await;

    Ok(Json(json!({
        "mermaid": mermaid_text,
        "meta": meta,
    })))
}

// ---------------------------------------------------------------------------
// Agent creation wizard helpers
// ---------------------------------------------------------------------------
