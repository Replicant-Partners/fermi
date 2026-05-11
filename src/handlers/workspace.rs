//! Workspace handlers — CRUD, chat, hire/add, coherence, ontology, git/files.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge_purchased_only, get_or_create_wallet, teams, AuthPrincipal,
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
use fermi::agent_backend::kg_context::enrich_with_kg_context;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use super::agents::CreateAgentRequest;
use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

// ─── Workspace handlers ────────────────────────────────────────────

pub async fn list_workspaces_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let user_teams = teams::get_user_teams(&state.db, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Enrich with budget info from DB
    let mut workspaces = Vec::new();
    for team in &user_teams {
        let budget_row =
            sqlx::query("SELECT workspace_budget, workspace_spent FROM teams WHERE id = $1")
                .bind(team.id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let (budget, spent) = match budget_row {
            Some(row) => (
                row.try_get::<i32, _>("workspace_budget").unwrap_or(0),
                row.try_get::<i32, _>("workspace_spent").unwrap_or(0),
            ),
            None => (0, 0),
        };

        // Agent previews for this workspace
        let agent_rows = sqlx::query(
            "SELECT a.agent_name, a.display_alias
             FROM workspace_agents wa
             JOIN agents a ON a.agent_id = wa.agent_id
             WHERE wa.workspace_id = $1
             ORDER BY wa.added_at DESC
             LIMIT 5",
        )
        .bind(team.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let agent_count: i64 =
            sqlx::query("SELECT COUNT(*) as cnt FROM workspace_agents WHERE workspace_id = $1")
                .bind(team.id)
                .fetch_one(&state.db)
                .await
                .ok()
                .map(|r| r.get::<i64, _>("cnt"))
                .unwrap_or(0);

        let agent_previews: Vec<Value> = agent_rows
            .iter()
            .map(|r| {
                let name: String = r.try_get("agent_name").unwrap_or_default();
                let alias: Option<String> = r.try_get("display_alias").unwrap_or(None);
                let initial = alias
                    .as_deref()
                    .unwrap_or(&name)
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                json!({ "agent_name": name, "display_alias": alias, "initial": initial })
            })
            .collect();

        workspaces.push(json!({
            "id": team.id,
            "name": team.name,
            "slug": team.slug,
            "description": team.description,
            "workspace_budget": budget,
            "workspace_spent": spent,
            "workspace_remaining": budget - spent,
            "agent_count": agent_count,
            "agent_previews": agent_previews,
        }));
    }

    Ok(Json(json!({ "workspaces": workspaces })))
}

pub async fn get_workspace_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let team = teams::get_team(&state.db, ws_uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Get budget
    let budget_row =
        sqlx::query("SELECT workspace_budget, workspace_spent FROM teams WHERE id = $1")
            .bind(ws_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (budget, spent) = match budget_row {
        Some(row) => (
            row.try_get::<i32, _>("workspace_budget").unwrap_or(0),
            row.try_get::<i32, _>("workspace_spent").unwrap_or(0),
        ),
        None => (0, 0),
    };

    // Get workspace agents from junction table
    let agent_rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.description, a.total_executions,
                a.display_alias, a.agent_type, a.tags, wa.relationship,
                a.sample_queries, a.accepts, a.produces, a.prompt_template
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1
         ORDER BY wa.added_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent_list: Vec<Value> = agent_rows
        .iter()
        .map(|r| {
            json!({
                "agent_id": r.try_get::<uuid::Uuid, _>("agent_id").ok(),
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
                "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
                "agent_type": r.try_get::<String, _>("agent_type").unwrap_or_default(),
                "tags": r.try_get::<Vec<String>, _>("tags").unwrap_or_default(),
                "total_executions": r.try_get::<i32, _>("total_executions").unwrap_or(0),
                "relationship": r.try_get::<String, _>("relationship").unwrap_or_default(),
                "sample_queries": r.try_get::<Option<Vec<String>>, _>("sample_queries").unwrap_or(None),
                "accepts": r.try_get::<Option<Vec<String>>, _>("accepts").unwrap_or(None),
                "produces": r.try_get::<Option<Vec<String>>, _>("produces").unwrap_or(None),
                "prompt_template": r.try_get::<Option<String>, _>("prompt_template").unwrap_or(None),
            })
        })
        .collect();

    // Get members with display names from users table
    let member_rows = sqlx::query(
        "SELECT tm.member_id, tm.role, u.display_name, u.email, u.avatar_url
         FROM team_members tm
         LEFT JOIN users u ON u.user_id = tm.member_id
         WHERE tm.team_id = $1
         ORDER BY tm.joined_at",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let member_list: Vec<Value> = member_rows
        .iter()
        .map(|r| {
            let member_id: String = r.try_get("member_id").unwrap_or_default();
            let display_name: Option<String> = r.try_get("display_name").unwrap_or(None);
            let email: Option<String> = r.try_get("email").unwrap_or(None);
            let avatar_url: Option<String> = r.try_get("avatar_url").unwrap_or(None);
            let role: String = r.try_get("role").unwrap_or_default();
            json!({
                "member_id": member_id,
                "display_name": display_name.or(email.clone()).unwrap_or_else(|| member_id.chars().take(8).collect()),
                "avatar_url": avatar_url,
                "role": role,
            })
        })
        .collect();

    Ok(Json(json!({
        "id": team.id,
        "name": team.name,
        "slug": team.slug,
        "description": team.description,
        "workspace_budget": budget,
        "workspace_spent": spent,
        "workspace_remaining": budget - spent,
        "agents": agent_list,
        "members": member_list,
    })))
}

pub async fn list_workspace_agents_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Query workspace_agents junction table joined with agents
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.agent_type, a.description, a.total_executions,
                a.display_alias, a.model,
                wa.relationship, wa.added_by, wa.added_at
         FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1
         ORDER BY wa.added_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent_list: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "agent_id": r.try_get::<uuid::Uuid, _>("agent_id").ok(),
                "agent_name": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>, _>("display_alias").unwrap_or(None),
                "agent_type": r.try_get::<String, _>("agent_type").unwrap_or_default(),
                "model": r.try_get::<String, _>("model").unwrap_or_default(),
                "description": r.try_get::<Option<String>, _>("description").unwrap_or(None),
                "total_executions": r.try_get::<i32, _>("total_executions").unwrap_or(0),
                "relationship": r.try_get::<String, _>("relationship").unwrap_or_default(),
                "added_by": r.try_get::<String, _>("added_by").unwrap_or_default(),
                "added_at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("added_at").ok(),
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agent_list })))
}

pub async fn create_workspace_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify the user is a member of this workspace
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &principal.user_id())
        .await
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                "Not a member of this workspace".to_string(),
            )
        })?;

    // Create agent owned by workspace
    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: req.agent_name.clone(),
        agent_type: req.agent_type,
        version: "1.0.0".to_string(),
        tier: "community".to_string(),
        executor_type: req.executor_type,
        model: req.model,
        temperature: req.temperature,
        mcp_servers: None,
        description: req.description,
        author: principal.user_id(),
        system_prompt: req.system_prompt,
        visibility: "shared".to_string(), // workspace agents are shared by default
        owner_id: Some(workspace_id),
        tags: req.tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: req.education_budget_credits,
        education_credits_used: 0,
        auto_collect_pct: 0,
        display_alias: None,
        llm_provider: "anthropic".to_string(),
        embedding_provider: "anthropic".to_string(),
        embedding_model: "voyage-2".to_string(),
        embedding_dimension: 1024,
        sample_queries: vec![],
        status: "draft".to_string(),
        fork_pricing: None,
        forked_from: None,
        fork_count: 0,
        accepts: vec![],
        produces: vec![],
        workflow_template: None,
        prompt_template: None,
        requires_secrets: None,
        model_ladder: serde_json::Value::Array(vec![]),
        min_tier: "free".to_string(),
        capability_gates: serde_json::Value::Object(serde_json::Map::new()),
        persona_version: 1,
        fermi_contract: None,
        model_params: serde_json::Value::Object(serde_json::Map::new()),
    };

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": req.agent_name,
        "workspace_id": ws_uuid,
        "message": "Workspace agent created successfully"
    })))
}

#[derive(Deserialize)]
pub struct FundWorkspaceRequest {
    amount: i32,
}

pub async fn fund_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<FundWorkspaceRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    if req.amount <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Amount must be positive".to_string(),
        ));
    }

    // Verify owner
    let team = teams::get_team(&state.db, ws_uuid)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    if team.owner_id != principal.user_id() {
        return Err((
            StatusCode::FORBIDDEN,
            "Only workspace owner can fund it".to_string(),
        ));
    }

    // Charge user's wallet — purchased balance only (funding a workspace = moving credits out)
    let user_wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    credit_charge_purchased_only(
        &state.db,
        user_wallet.wallet_id,
        req.amount,
        "transfer_out",
        &format!("Fund workspace {}", team.name),
        Some(&workspace_id),
    )
    .await
    .map_err(|e| (StatusCode::PAYMENT_REQUIRED, e.to_string()))?;

    // Credit workspace budget in teams table (display)
    sqlx::query("UPDATE teams SET workspace_budget = workspace_budget + $1 WHERE id = $2")
        .bind(req.amount)
        .bind(ws_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Also credit the workspace wallet (used for gas charges)
    let ws_wallet = get_or_create_wallet(&state.db, "workspace", &workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    sqlx::query("UPDATE wallets SET balance = balance + $1, total_deposited = total_deposited + $1 WHERE wallet_id = $2")
        .bind(req.amount)
        .bind(ws_wallet.wallet_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Auto-commit budget log to workspace git repo
    let wg = state.workspace_git.clone();
    let db_clone = state.db.clone();
    let uid = principal.user_id();
    let amt = req.amount;
    let team_name = team.name.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let entry = format!(
                "## {} — Funded {} credits\n\nBy: {}\nWorkspace: {}\n\n---\n",
                chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
                amt,
                uid,
                team_name,
            );
            // Append to budget_log or create it
            let existing = wg
                .read_file(&slug, "context/budget_log.md")
                .unwrap_or_default();
            let updated = format!("{}{}", existing, entry);
            let _ = wg.commit_file(
                &slug,
                "context/budget_log.md",
                &updated,
                &format!("Fund workspace: +{} credits", amt),
            );
        }
    });

    Ok(Json(json!({
        "message": "Workspace funded successfully",
        "amount": req.amount,
        "workspace_id": ws_uuid,
    })))
}

// ─── Workspace Gas Helper ──────────────────────────────────────────

/// Charge gas from workspace wallet and sync workspace_spent on teams table.
pub async fn charge_workspace_gas(
    pool: &PgPool,
    ws_uuid: uuid::Uuid,
    workspace_id: &str,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> Result<i32, (StatusCode, String)> {
    let ws_wallet = get_or_create_wallet(pool, "workspace", workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let charged = charge_gas(
        pool,
        ws_wallet.wallet_id,
        amount,
        tx_type,
        description,
        related_id,
    )
    .await?;
    // Keep teams.workspace_spent in sync for display
    let _ = sqlx::query("UPDATE teams SET workspace_spent = workspace_spent + $1 WHERE id = $2")
        .bind(charged)
        .bind(ws_uuid)
        .execute(pool)
        .await;
    Ok(charged)
}

// ─── Workspace Chat ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    content: String,
    #[serde(default)]
    message_type: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
}

/// Parse @agent_name mentions from message content.
/// Returns (target_agent_name, query_text) if found.
pub fn parse_at_mention(content: &str) -> Option<(String, String)> {
    // Match @word_chars at start or after whitespace
    let re = regex::Regex::new(r"@([a-zA-Z0-9_-]+)").ok()?;
    let m = re.find(content)?;
    let agent_name = re.captures(content)?.get(1)?.as_str().to_string();
    // Query is everything except the @mention
    let query = format!("{}{}", &content[..m.start()], &content[m.end()..])
        .trim()
        .to_string();
    if query.is_empty() {
        return None;
    }
    Some((agent_name, query))
}

/// Broadcast a workspace message to SSE subscribers.
fn broadcast_message(state: &AppState, workspace_id: uuid::Uuid, msg_json: &Value) {
    let _ = state.ws_broadcast.send(crate::WorkspaceEvent {
        workspace_id,
        message: msg_json.clone(),
    });
}

/// Build the standard message JSON shape from a WorkspaceMessage.
fn message_to_json(m: &WorkspaceMessage) -> Value {
    json!({
        "message_id": m.message_id,
        "sender_type": m.sender_type,
        "sender_id": m.sender_id,
        "sender_name": m.sender_name,
        "content": m.content,
        "message_type": m.message_type,
        "metadata": m.metadata,
        "created_at": m.created_at.to_rfc3339(),
    })
}

/// Load workspace context files from the git repo's context/ directory.
pub async fn load_workspace_context(workspace_git: &WorkspaceGitManager, slug: &str) -> String {
    let files = match workspace_git.list_files(slug, Some("context")) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut context_parts = Vec::new();
    for file in &files {
        if file.is_dir {
            continue;
        }
        if let Ok(content) = workspace_git.read_file(slug, &file.path) {
            context_parts.push(format!("--- {} ---\n{}", file.path, content));
        }
    }
    if context_parts.is_empty() {
        String::new()
    } else {
        format!("[Workspace Context]\n{}", context_parts.join("\n\n"))
    }
}

pub async fn post_workspace_message_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<PostMessageRequest>,
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

    // Charge message gas
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.message_send,
        "gas_fee",
        "Chat message",
        None,
    )
    .await?;

    // Detect @agent_name invocation
    let at_mention = parse_at_mention(&req.content);
    let is_invocation =
        req.message_type.as_deref() == Some("agent_invocation") || at_mention.is_some();

    // Look up user display name
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "user".to_string(),
        sender_id: user_id.clone(),
        sender_name: display_name.or_else(|| Some(user_id.clone())),
        content: req.content.clone(),
        message_type: if is_invocation {
            "agent_invocation".to_string()
        } else {
            "chat".to_string()
        },
        metadata: req.metadata.clone().unwrap_or(json!({})),
        created_at: chrono::Utc::now(),
    };

    let msg_id = state
        .memory_store
        .store_workspace_message(&msg)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Broadcast user message to SSE subscribers
    broadcast_message(&state, ws_uuid, &message_to_json(&msg));

    // If @agent invocation, spawn background execution
    if is_invocation {
        // Extract target agent and query
        let (target_agent, query) = if let Some((name, q)) = at_mention {
            (name, q)
        } else if let Some(meta) = &req.metadata {
            let name = meta
                .get("target_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let q = meta
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or(&req.content)
                .to_string();
            (name, q)
        } else {
            ("".to_string(), req.content.clone())
        };

        if !target_agent.is_empty() {
            // Verify agent is in workspace
            let agent_in_ws = sqlx::query(
                "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
                 JOIN agents a ON a.agent_id = wa.agent_id
                 WHERE wa.workspace_id = $1 AND a.agent_name = $2",
            )
            .bind(ws_uuid)
            .bind(&target_agent)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if let Some(agent_row) = agent_in_ws {
                let agent_name: String = agent_row.try_get("agent_name").unwrap_or_default();
                let agent_display: Option<String> =
                    agent_row.try_get("display_alias").unwrap_or(None);
                let display = agent_display.unwrap_or_else(|| agent_name.clone());

                // Clone what we need for the background task
                let state2 = state.clone();
                let ws_id = workspace_id.clone();
                let ws_uuid2 = ws_uuid;
                let query2 = query.clone();
                let agent_name2 = agent_name.clone();
                let display2 = display.clone();
                let user_id2 = user_id.clone();

                tokio::spawn(async move {
                    // Load workspace context
                    let slug = get_workspace_slug(&state2.db, ws_uuid2)
                        .await
                        .unwrap_or_default();
                    let ws_context = load_workspace_context(&state2.workspace_git, &slug).await;

                    // Build augmented query with workspace context
                    let augmented_query = if ws_context.is_empty() {
                        query2.clone()
                    } else {
                        format!("{}\n\n{}", ws_context, query2)
                    };

                    // Resolve and execute
                    let result = async {
                        let db_agent = resolve_agent(&state2, &agent_name2).await?;
                        let card = resolve_agent_card(&state2, &db_agent);

                        // Enrich card with KG context from past dream cycles
                        let card = enrich_with_kg_context(
                            &state2.memory_store,
                            &state2.embedder,
                            db_agent.agent_id,
                            &augmented_query,
                            card,
                        )
                        .await;

                        let agent_stmt = ast::AgentStmt {
                            name: agent_name2.clone(),
                            agent_type: Some(card.agent_type.clone()),
                            query: augmented_query.clone(),
                            executor: Some(ast::ExecutorType::LLM),
                            schedule: None,
                            driver_refs: vec![],
                            depends_on: vec![],
                            confidence_threshold: None,
                        };
                        let program = ast::Program {
                            statements: vec![ast::Statement::Agent(agent_stmt.clone())],
                        };
                        let context = ExecutionContext {
                            program,
                            agent_card: card.clone(),
                            creature_id: None,
                            cognition_tier: None,
                        };

                        // Resolve user secrets for this agent
                        let user_secrets = if let Some(ref encryptor) = state2.secret_encryptor {
                            fermi_auth::get_secrets_for_agent(
                                &state2.db, encryptor, &user_id2, &agent_name2,
                            )
                            .await
                            .ok()
                            .filter(|s| !s.is_empty())
                        } else {
                            None
                        };

                        // Check for missing required secrets
                        if let Some(ref req_secrets) = db_agent.requires_secrets {
                            if let Ok(requirements) = serde_json::from_value::<Vec<serde_json::Value>>(req_secrets.clone()) {
                                let resolved = user_secrets.as_ref();
                                let missing: Vec<String> = requirements.iter()
                                    .filter(|r| r.get("is_required").and_then(|v| v.as_bool()).unwrap_or(true))
                                    .filter(|r| {
                                        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                        !resolved.map_or(false, |s| s.contains_key(name))
                                    })
                                    .filter_map(|r| r.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()))
                                    .collect();

                                if !missing.is_empty() {
                                    let hint = format!(
                                        "@{} needs credentials to function: {}. Go to Profile > Connections to add them.",
                                        agent_name2,
                                        missing.join(", ")
                                    );
                                    // Store as system message in workspace
                                    let _ = sqlx::query(
                                        "INSERT INTO workspace_messages (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
                                         VALUES ($1, 'system', 'system', 'System', $2, 'system', '{}'::jsonb)"
                                    )
                                    .bind(ws_uuid2)
                                    .bind(&hint)
                                    .execute(&state2.db)
                                    .await;
                                }
                            }
                        }

                        // Use ToolAwareExecutor with workspace tools
                        let tool_context = Arc::new(ToolContext {
                            memory_store: state2.memory_store.clone(),
                            embedder: state2.embedder.clone(),
                            registry: state2.registry.clone(),
                            current_agent_id: Some(db_agent.agent_id),
                            workspace_id: Some(ws_uuid2),
                            workspace_slug: Some(slug.clone()),
                            workspace_git: Some(state2.workspace_git.clone()),
                            db: Some(state2.db.clone()),
                            gas_fees: Some(state2.gas_fees.clone()),
                            user_id: Some(user_id2.clone()),
                            user_secrets,
                            eval_trigger: Some(Arc::new(
                                crate::handlers::eval::EvalTriggerImpl {
                                    state: state2.clone(),
                                },
                            )),
                        });
                        let tool_executor = ToolAwareExecutor::new(
                            state2.registry.executor_arc(),
                            ToolRegistry::with_workspace(),
                            tool_context,
                        );
                        let output = tool_executor
                            .execute(&agent_stmt, &context)
                            .await
                            .map_err(|e| {
                                (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    format!("Execution failed: {}", e),
                                )
                            })?;

                        // Record stats
                        let _ = state2.registry.record_execution(&agent_name2, &output);

                        // Store episode
                        let mut episode =
                            agent_output_to_episode(db_agent.agent_id, &query2, &output);
                        let embed_text = format!(
                            "{} {}",
                            query2,
                            output.metadata.reasoning.as_deref().unwrap_or("")
                        );
                        if let Ok(embedding) = state2.embedder.generate(&embed_text).await {
                            episode.embedding = Some(embedding);
                        }
                        let _ = state2.memory_store.store_episode(episode).await;

                        // Charge execution gas from workspace wallet
                        let tokens = output.tokens_used.unwrap_or(0) as i32;
                        let (exec_fee, gas_fee) = state2.gas_fees.execution_fee(tokens);
                        let total = exec_fee + gas_fee;
                        let _ = charge_workspace_gas(
                            &state2.db,
                            ws_uuid2,
                            &ws_id,
                            total,
                            "execution_fee",
                            &format!("@{} execution ({}tk)", agent_name2, tokens),
                            None,
                        )
                        .await;

                        // Auto-commit ontology snapshot to workspace repo
                        if !slug.is_empty() {
                            if let Ok(snapshot) = sqlx::query(
                                "SELECT version, mermaid_content, dream_synopsis FROM ontology_snapshots
                                 WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1"
                            )
                            .bind(db_agent.agent_id)
                            .fetch_optional(&state2.db)
                            .await
                            {
                                if let Some(snap) = snapshot {
                                    let version: i32 = snap.try_get("version").unwrap_or(0);
                                    let mermaid: Option<String> = snap.try_get("mermaid_content").unwrap_or(None);
                                    let synopsis: Option<String> = snap.try_get("dream_synopsis").unwrap_or(None);
                                    let content = format!(
                                        "# Ontology Snapshot v{}\n\n{}\n\n{}",
                                        version,
                                        synopsis.as_deref().unwrap_or(""),
                                        mermaid.as_deref().unwrap_or("(no diagram)")
                                    );
                                    let path = format!("ontology/{}/snapshot_v{}.md", agent_name2, version);
                                    let _ = state2.workspace_git.commit_file(
                                        &slug, &path, &content,
                                        &format!("Ontology snapshot v{} for {}", version, agent_name2),
                                    );
                                }
                            }
                        }

                        Ok::<_, (StatusCode, String)>(output)
                    }
                    .await;

                    // Post result message
                    let (content, metadata, msg_type) = match result {
                        Ok(output) => {
                            let evidence_summary = output
                                .evidence
                                .iter()
                                .map(|e| {
                                    format!("- {}", e.summary.as_deref().unwrap_or("(no summary)"))
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            let reasoning = output
                                .metadata
                                .reasoning
                                .as_deref()
                                .unwrap_or("No reasoning provided");
                            let content = format!(
                                "{}\n\n{}",
                                reasoning,
                                if evidence_summary.is_empty() {
                                    String::new()
                                } else {
                                    format!("**Evidence:**\n{}", evidence_summary)
                                }
                            );
                            let meta = json!({
                                "agent_name": agent_name2,
                                "confidence": output.confidence,
                                "execution_time_ms": output.execution_time_ms,
                                "tokens_used": output.tokens_used,
                                "status": format!("{:?}", output.status),
                                "evidence_count": output.evidence.len(),
                            });
                            (content, meta, "execution_result".to_string())
                        }
                        Err((_status, err_msg)) => (
                            format!("Execution failed: {}", err_msg),
                            json!({"agent_name": agent_name2, "error": true}),
                            "execution_result".to_string(),
                        ),
                    };

                    let result_msg = WorkspaceMessage {
                        message_id: uuid::Uuid::new_v4(),
                        workspace_id: ws_uuid2,
                        sender_type: "agent".to_string(),
                        sender_id: agent_name2.clone(),
                        sender_name: Some(display2),
                        content,
                        message_type: msg_type,
                        metadata,
                        created_at: chrono::Utc::now(),
                    };
                    let _ = state2
                        .memory_store
                        .store_workspace_message(&result_msg)
                        .await;
                    broadcast_message(&state2, ws_uuid2, &message_to_json(&result_msg));
                });
            } else {
                // Agent not in workspace — post system error
                let err_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "system".to_string(),
                    sender_name: Some("System".to_string()),
                    content: format!("Agent '{}' is not in this workspace. Use Hire or Add to bring them in first.", target_agent),
                    message_type: "system_event".to_string(),
                    metadata: json!({}),
                    created_at: chrono::Utc::now(),
                };
                let _ = state.memory_store.store_workspace_message(&err_msg).await;
                broadcast_message(&state, ws_uuid, &message_to_json(&err_msg));
            }
        }
    }

    // Auto-evaluate coherence every N messages (background, best-effort)
    let auto_eval_interval: i64 = std::env::var("COHERENCE_AUTO_EVAL_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let store = state.memory_store.clone();
    let broadcast_tx = state.ws_broadcast.clone();
    tokio::spawn(async move {
        // Check last eval time
        let since = match store.get_latest_coherence(ws_uuid).await {
            Ok(Some(e)) => e.created_at,
            _ => chrono::DateTime::<chrono::Utc>::MIN_UTC,
        };
        let count = store
            .count_workspace_messages_since(ws_uuid, since)
            .await
            .unwrap_or(0);
        if count >= auto_eval_interval {
            // Run coherence evaluation (no gas charge for auto-eval)
            let messages = match store.get_workspace_messages(ws_uuid, 50, None).await {
                Ok(m) => m,
                Err(_) => return,
            };
            if messages.is_empty() {
                return;
            }
            let conv_id = ConversationId(ws_uuid);
            let coherence_msgs: Vec<CoherenceMessage> = messages
                .iter()
                .rev()
                .map(|m| {
                    let pid = ParticipantId(
                        uuid::Uuid::parse_str(&m.sender_id)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    );
                    CoherenceMessage::new(pid, &m.content)
                })
                .collect();

            let observer = ConversationObserver::new(conv_id);
            let mut system = observer.observe(&coherence_msgs);
            let engine = SettlingEngine::with_defaults();
            engine.settle(&mut system);
            let snapshot = system.snapshot();

            let principle_scores =
                serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));
            let health_indicators = json!({
                "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
                "converged": snapshot.global_coherence.converged,
                "accepted_count": snapshot.global_coherence.accepted_count,
                "rejected_count": snapshot.global_coherence.rejected_count,
            });

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
                    "auto": true,
                })),
                created_at: chrono::Utc::now(),
            };

            if let Ok(eval_id) = store.store_coherence_evaluation(&eval).await {
                let update_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "coherence_evaluator".to_string(),
                    sender_name: Some("Coherence Evaluator".to_string()),
                    content: format!(
                        "Coherence: {:.0}% ({}) | {} utterances",
                        eval.global_score * 100.0,
                        eval.quality_label,
                        eval.utterance_count,
                    ),
                    message_type: "coherence_update".to_string(),
                    metadata: json!({
                        "eval_id": eval_id,
                        "global_score": eval.global_score,
                        "quality_label": eval.quality_label,
                        "auto": true,
                    }),
                    created_at: chrono::Utc::now(),
                };
                let _ = store.store_workspace_message(&update_msg).await;
                let _ = broadcast_tx.send(crate::WorkspaceEvent {
                    workspace_id: ws_uuid,
                    message: message_to_json(&update_msg),
                });
            }
        }
    });

    Ok(Json(json!({
        "message_id": msg_id,
        "sender_type": msg.sender_type,
        "sender_id": msg.sender_id,
        "content": msg.content,
        "message_type": msg.message_type,
        "created_at": msg.created_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    limit: Option<i64>,
    before: Option<String>,
}

pub async fn get_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<MessageQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let limit = params.limit.unwrap_or(50).min(200);
    let before = params.before.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, limit, before)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

#[derive(Debug, Deserialize)]
pub struct PollQuery {
    since: String,
}

pub async fn poll_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<PollQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let since = chrono::DateTime::parse_from_rfc3339(&params.since)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid timestamp format".to_string(),
            )
        })?
        .with_timezone(&chrono::Utc);

    let messages = state
        .memory_store
        .get_workspace_messages_since(ws_uuid, since)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

// ─── SSE Stream ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    since: Option<String>,
}

pub async fn workspace_messages_stream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<StreamQuery>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Fetch missed messages if `since` is provided (reconnection catch-up)
    let backfill: Vec<Value> = if let Some(ref since_str) = params.since {
        if let Ok(since) = chrono::DateTime::parse_from_rfc3339(since_str) {
            let since_utc = since.with_timezone(&chrono::Utc);
            state
                .memory_store
                .get_workspace_messages_since(ws_uuid, since_utc)
                .await
                .unwrap_or_default()
                .iter()
                .map(message_to_json)
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut rx = state.ws_broadcast.subscribe();

    let stream = async_stream::stream! {
        // Send backfill messages first (catch-up on reconnect)
        for msg_json in backfill {
            let data = serde_json::to_string(&msg_json).unwrap_or_default();
            yield Ok(Event::default().data(data));
        }

        // Keepalive interval (30s) to prevent proxy timeouts
        let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
        keepalive.tick().await; // skip first immediate tick

        loop {
            tokio::select! {
                result = rx.recv() => {
                    match result {
                        Ok(event) => {
                            if event.workspace_id == ws_uuid {
                                let data = serde_json::to_string(&event.message).unwrap_or_default();
                                yield Ok(Event::default().data(data));
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // Buffer overflow — tell client to refetch
                            yield Ok(Event::default().event("lagged").data("refetch"));
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            break;
                        }
                    }
                }
                _ = keepalive.tick() => {
                    yield Ok(Event::default().comment("keepalive"));
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keepalive"),
    ))
}

// ─── Workspace Hire / Add ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct HireAddRequest {
    agent_id: uuid::Uuid,
    #[serde(default)]
    include_optional: bool,
}

/// Post a system message to workspace chat (helper) + broadcast to SSE.
pub async fn post_system_message(state: &AppState, workspace_id: uuid::Uuid, content: &str) {
    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id,
        sender_type: "system".to_string(),
        sender_id: "system".to_string(),
        sender_name: None,
        content: content.to_string(),
        message_type: "system_event".to_string(),
        metadata: json!({}),
        created_at: chrono::Utc::now(),
    };
    let _ = state.memory_store.store_workspace_message(&msg).await;
    broadcast_message(state, workspace_id, &message_to_json(&msg));
}

pub async fn hire_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be admin+ to hire
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;
    if !role.can_invite() {
        return Err((
            StatusCode::FORBIDDEN,
            "Admin role required to hire agents".to_string(),
        ));
    }

    // Resolve agent
    let agent = state
        .memory_store
        .get_agent(req.agent_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Agent not found: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // Must not own the agent (use /add for your own)
    if agent.owner_id.as_deref() == Some(&user_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use /add for your own agents".to_string(),
        ));
    }

    // Agent must be public (or shared with caller — future)
    if agent.visibility != "public" {
        return Err((StatusCode::FORBIDDEN, "Agent is not public".to_string()));
    }

    // Check if agent is already in workspace
    let already =
        sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
            .bind(ws_uuid)
            .bind(req.agent_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if already.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Agent is already in this workspace".to_string(),
        ));
    }

    // Charge hire gas from workspace wallet
    let agent_id_str = req.agent_id.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_hire,
        "gas_fee",
        &format!("Hire agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'hired') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(req.agent_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} hired {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "hired",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Hired agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    // ─── Auto-hire dependencies ───
    let card = resolve_agent_card(&state, &agent);
    let deps = &card.dependencies;
    let mut deps_hired: Vec<String> = Vec::new();
    let mut deps_gas: i32 = 0;

    // Collect dep names to hire: required always, optional if requested
    let mut dep_names: Vec<String> = deps.required.clone();
    if req.include_optional {
        dep_names.extend(deps.optional.clone());
    }

    for dep_name in &dep_names {
        // Resolve dep agent by name
        let dep_agent = match state.memory_store.get_agent_by_name(dep_name).await {
            Ok(a) => a,
            Err(_) => continue, // Skip if not found in DB
        };

        // Check if already in workspace
        let already =
            sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
                .bind(ws_uuid)
                .bind(dep_agent.agent_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

        if already.is_some() {
            continue; // Already hired
        }

        // Charge gas for dep hire
        let _ = charge_workspace_gas(
            &state.db,
            ws_uuid,
            &workspace_id,
            state.gas_fees.agent_hire,
            "gas_fee",
            &format!("Auto-hire dep {}", dep_name),
            None,
        )
        .await;

        // Insert workspace_agents row
        let _ = sqlx::query(
            "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'hired') ON CONFLICT DO NOTHING",
        )
        .bind(ws_uuid)
        .bind(dep_agent.agent_id)
        .bind(&user_id)
        .execute(&state.db)
        .await;

        deps_hired.push(dep_name.clone());
        deps_gas += state.gas_fees.agent_hire;
    }

    if !deps_hired.is_empty() {
        post_system_message(
            &state,
            ws_uuid,
            &format!("Auto-hired dependencies: {}", deps_hired.join(", ")),
        )
        .await;
    }

    // ─── Inject workflow scaffold from compound agent ───
    let mut scaffold_injected = false;
    if let Some(ref tmpl) = card.workflow_template {
        let existing: Option<String> =
            sqlx::query_scalar("SELECT workflow_mermaid FROM teams WHERE id = $1")
                .bind(ws_uuid)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

        if existing.as_ref().map_or(true, |s| s.is_empty()) {
            let scaffold_meta = serde_json::json!({
                "source": "workflow_template",
                "compound_agent": agent.agent_name,
                "stages": tmpl.stages,
                "is_scaffold": true,
                "generated_at": chrono::Utc::now().to_rfc3339(),
            });
            let _ = sqlx::query(
                "UPDATE teams SET workflow_mermaid = $1, workflow_meta = $2 WHERE id = $3",
            )
            .bind(&tmpl.mermaid)
            .bind(&scaffold_meta)
            .bind(ws_uuid)
            .execute(&state.db)
            .await;
            scaffold_injected = true;
        }
    }

    Ok(Json(json!({
        "message": "Agent hired successfully",
        "agent_name": agent.agent_name,
        "relationship": "hired",
        "gas_charged": state.gas_fees.agent_hire + deps_gas,
        "dependencies_hired": deps_hired,
        "scaffold_injected": scaffold_injected,
    })))
}

pub async fn add_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be workspace member
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Resolve agent — must own it
    let agent = state
        .memory_store
        .get_agent(req.agent_id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Agent not found: {}", e)))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    if agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "You don't own this agent. Use /hire instead.".to_string(),
        ));
    }

    // Check if agent is already in workspace
    let already =
        sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
            .bind(ws_uuid)
            .bind(req.agent_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if already.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Agent is already in this workspace".to_string(),
        ));
    }

    // Charge add gas from workspace wallet
    let agent_id_str = req.agent_id.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_add,
        "gas_fee",
        &format!("Add agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'owned') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(req.agent_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} added {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "owned",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Added agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    Ok(Json(json!({
        "message": "Agent added successfully",
        "agent_name": agent.agent_name,
        "relationship": "owned",
        "gas_charged": state.gas_fees.agent_add,
    })))
}

pub async fn remove_workspace_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;
    let agent_uuid: uuid::Uuid = agent_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;

    // Must be admin+ or the person who added
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let row = sqlx::query(
        "SELECT added_by FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(ws_uuid)
    .bind(agent_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = row.ok_or((StatusCode::NOT_FOUND, "Agent not in workspace".to_string()))?;
    let added_by: String = row.try_get("added_by").unwrap_or_default();

    if added_by != user_id && !role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Must be admin or the person who added".to_string(),
        ));
    }

    sqlx::query("DELETE FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
        .bind(ws_uuid)
        .bind(agent_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} removed an agent from the workspace", user_id),
    )
    .await;

    Ok(Json(json!({ "message": "Agent removed from workspace" })))
}

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

    // For premium tiers, run coherence_consultant agent
    let consultant_output = if depth == "recommendations" || depth == "dream_notes" {
        let consultant_id = "coherence_consultant";
        match state.registry.get(consultant_id) {
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
                        "Coherence score: {:.0}% ({}). Principles: {:?}. Health: {:?}.\n\n\
                         Recent messages:\n{}\n\n\
                         Provide specific, actionable recommendations for improving workspace coherence.",
                        eval.global_score * 100.0, eval.quality_label,
                        principle_scores, health_indicators, msg_summary,
                    )
                } else {
                    format!(
                        "Coherence score: {:.0}% ({}). Principles: {:?}. Health: {:?}.\n\n\
                         Full workspace conversation:\n{}\n\n\
                         Write dream notes: a narrative synthesis of what this workspace has learned, \
                         connections made, knowledge gaps identified, and emerging themes.",
                        eval.global_score * 100.0, eval.quality_label,
                        principle_scores, health_indicators, msg_summary,
                    )
                };

                let agent_stmt = ast::AgentStmt {
                    name: consultant_id.to_string(),
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
                let context = ExecutionContext {
                    program,
                    agent_card: card,
                    creature_id: None,
                    cognition_tier: None,
                };
                match state.registry.execute_agent(&agent_stmt, &context).await {
                    Ok(output) => output.metadata.reasoning,
                    Err(e) => {
                        eprintln!("Coherence consultant failed: {:?}", e);
                        Some(format!("Consultant unavailable: {:?}", e))
                    }
                }
            }
            Err(_) => Some("Coherence consultant agent not available".to_string()),
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

    let update_msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "system".to_string(),
        sender_id: "coherence_evaluator".to_string(),
        sender_name: Some("Coherence Evaluator".to_string()),
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

/// Helper: get workspace slug from UUID
pub async fn get_workspace_slug(
    pool: &PgPool,
    ws_uuid: uuid::Uuid,
) -> Result<String, (StatusCode, String)> {
    let row = sqlx::query("SELECT slug FROM teams WHERE id = $1")
        .bind(ws_uuid)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "Workspace not found".to_string()))?;
    Ok(row.get::<String, _>("slug"))
}

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
