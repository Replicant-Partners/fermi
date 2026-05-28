//! Workspace CRUD, gas helper, and shared utilities.
//! Workspace handlers — shared imports.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge, credit_charge_purchased_only, credit_deposit_typed, get_or_create_wallet,
    teams, AuthPrincipal,
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

use crate::handlers::agents::CreateAgentRequest;
use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

// ─── Workspace handlers ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListWorkspacesQuery {
    /// Filter to workspaces created by this vertical (e.g.
    /// `bestiary_workspace`, `rabble_swarm`, `fermi_forecast`). Omit
    /// to see all origins. See docs/VERTICAL_HARNESS_SPLIT.md §2.
    pub origin: Option<String>,
}

pub async fn list_workspaces_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    axum::extract::Query(q): axum::extract::Query<ListWorkspacesQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let user_teams = teams::get_user_teams(&state.db, &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Enrich with budget info + origin from DB
    let mut workspaces = Vec::new();
    for team in &user_teams {
        let row = sqlx::query(
            "SELECT workspace_budget, workspace_spent, origin FROM teams WHERE id = $1",
        )
        .bind(team.id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        let (budget, spent, origin) = match row {
            Some(r) => (
                r.try_get::<i32, _>("workspace_budget").unwrap_or(0),
                r.try_get::<i32, _>("workspace_spent").unwrap_or(0),
                r.try_get::<String, _>("origin")
                    .unwrap_or_else(|_| "bestiary_workspace".into()),
            ),
            None => (0, 0, "bestiary_workspace".into()),
        };

        // Apply origin filter if requested.
        if let Some(ref want) = q.origin {
            if &origin != want {
                continue;
            }
        }

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
            "owner_id": team.owner_id,
            "origin": origin,
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

    // Get budget + composition identity fields
    let budget_row = sqlx::query(
        "SELECT workspace_budget, workspace_spent, mission, coordination_strategist_id
         FROM teams WHERE id = $1",
    )
    .bind(ws_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let (budget, spent, mission, strategist_uuid) = match budget_row {
        Some(ref row) => (
            row.try_get::<i32, _>("workspace_budget").unwrap_or(0),
            row.try_get::<i32, _>("workspace_spent").unwrap_or(0),
            row.try_get::<Option<String>, _>("mission").unwrap_or(None),
            row.try_get::<Option<uuid::Uuid>, _>("coordination_strategist_id").unwrap_or(None),
        ),
        None => (0, 0, None, None),
    };

    // Resolve strategist name for display
    let strategist_name: Option<String> = if let Some(sid) = strategist_uuid {
        sqlx::query("SELECT agent_name, display_alias FROM agents WHERE agent_id = $1")
            .bind(sid)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|r| {
                r.try_get::<Option<String>, _>("display_alias")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| r.try_get::<String, _>("agent_name").unwrap_or_default())
            })
    } else {
        None
    };

    let is_composition = mission.is_some() || strategist_uuid.is_some();

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
        // Composition identity
        "mission": mission,
        "coordination_strategist_id": strategist_uuid,
        "coordination_strategist_name": strategist_name,
        "is_composition": is_composition,
        // Budget
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
                valence: None,
            output_contract: None,
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

    // Charge user's wallet. Funding a workspace = transferring credits
    // out of the user's wallet into the workspace's wallet, so by default
    // we require purchased balance (granted credits aren't transferable —
    // prevents granted-credit leakage into the broader economy).
    //
    // Admin exemption: sys admins can fund workspaces using any balance
    // (purchased OR granted) so they can spin up test workspaces without
    // first going through Stripe. This is gated on can_admin() so it
    // doesn't widen the surface for regular users.
    let user_wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Use full balance (granted + purchased) when:
    //   (a) caller is a platform admin, OR
    //   (b) caller is funding their own workspace (owner self-funds)
    //
    // Previously only admins could use granted credits for workspace funding.
    // This blocked the common flow where an operator receives admin-granted
    // credits and tries to fund their own workspace — those credits land in
    // granted_balance which credit_charge_purchased_only rejects, causing a
    // silent 402 that leaves workspace_budget and wallet out of sync.
    let is_self_fund = team.owner_id == principal.user_id();
    let charge_result = if principal.can_admin() || is_self_fund {
        credit_charge(
            &state.db,
            user_wallet.wallet_id,
            req.amount,
            "transfer_out",
            &format!("Fund workspace {}", team.name),
            Some(&workspace_id),
        )
        .await
    } else {
        credit_charge_purchased_only(
            &state.db,
            user_wallet.wallet_id,
            req.amount,
            "transfer_out",
            &format!("Fund workspace {}", team.name),
            Some(&workspace_id),
        )
        .await
    };
    charge_result.map_err(|e| (StatusCode::PAYMENT_REQUIRED, e.to_string()))?;

    // Credit workspace budget in teams table (display)
    sqlx::query("UPDATE teams SET workspace_budget = workspace_budget + $1 WHERE id = $2")
        .bind(req.amount)
        .bind(ws_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Also credit the workspace wallet (used for gas charges).
    // Use credit_deposit_typed so wallets.purchased_balance is updated
    // alongside balance — otherwise the wallet_balance_split_check
    // constraint (balance = granted_balance + purchased_balance) fires.
    // tx_type='transfer_in' pairs with the user-side 'transfer_out'.
    let ws_wallet = get_or_create_wallet(&state.db, "workspace", &workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    credit_deposit_typed(
        &state.db,
        ws_wallet.wallet_id,
        req.amount,
        "transfer_in",
        &format!("Funded from {}", principal.user_id()),
    )
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



// ─── Shared helpers (used by messages + coherence) ──────────────────

/// Parse @agent_name mentions from message content.
/// Returns (target_agent_name, query_text) if found.
///
/// Accepted mention characters: alphanumerics, `_`, `-`, `/`, and `.`.
/// `/` matters specifically — agents like `efra-ai/05-valuation` use it as
/// a namespacing separator and the previous regex stopped at the first
/// `/`, producing a stub like `efra-ai` that resolves to nothing. `.`
/// is included for symmetry with conventions like `foo.bar.v2`.
///
/// We deliberately don't include `:` or `@` to keep mention boundaries
/// unambiguous (so `@a@b` still parses as one mention `a`, not `a@b`).
pub fn parse_at_mention(content: &str) -> Option<(String, String)> {
    let re = regex::Regex::new(r"@([a-zA-Z0-9_./-]+)").ok()?;
    let m = re.find(content)?;
    let agent_name = re.captures(content)?.get(1)?.as_str().to_string();
    // Trim trailing punctuation that the regex greedily consumed but
    // almost certainly belongs to the surrounding prose, not the name —
    // e.g. "@efra-ai/05-valuation." should mention `efra-ai/05-valuation`,
    // not `efra-ai/05-valuation.`.
    let agent_name = agent_name
        .trim_end_matches(|c: char| c == '.' || c == '/' || c == '-' || c == '_')
        .to_string();
    if agent_name.is_empty() {
        return None;
    }
    // Query is everything except the @mention
    let query = format!("{}{}", &content[..m.start()], &content[m.end()..])
        .trim()
        .to_string();
    if query.is_empty() {
        return None;
    }
    Some((agent_name, query))
}

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

// ─── Set composition identity ─────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct SetCompositionIdentityRequest {
    pub mission: Option<String>,
    /// Strategist agent_name or UUID string.
    pub coordination_strategist_id: Option<String>,
}

/// POST /api/workspaces/:id/composition/identity
///
/// Set or update the mission and coordination strategist for a workspace,
/// upgrading a plain group workspace to a named composition.
/// Owner or admin only.
pub async fn set_composition_identity_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(body): Json<SetCompositionIdentityRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Auth: owner or admin
    let user_id = principal.user_id();
    let owner_row = sqlx::query("SELECT owner_id FROM teams WHERE id = $1")
        .bind(ws_uuid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".into()))?;

    let owner_id: String = owner_row.try_get("owner_id")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if owner_id != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Owner or admin required".into()));
    }

    // Resolve strategist name → UUID
    let strategist_uuid: Option<uuid::Uuid> = if let Some(ref sid) = body.coordination_strategist_id {
        if sid.is_empty() {
            None
        } else if let Ok(uuid) = sid.parse::<uuid::Uuid>() {
            Some(uuid)
        } else {
            sqlx::query("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
                .bind(sid)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten()
                .and_then(|r| r.try_get::<uuid::Uuid, _>("agent_id").ok())
        }
    } else {
        None
    };

    sqlx::query(
        "UPDATE teams SET
            mission = COALESCE($1, mission),
            coordination_strategist_id = COALESCE($2, coordination_strategist_id)
         WHERE id = $3",
    )
    .bind(&body.mission)
    .bind(strategist_uuid)
    .bind(ws_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "workspace_id": ws_uuid,
        "mission": body.mission,
        "coordination_strategist_id": body.coordination_strategist_id,
        "status": "updated",
    })))
}

// ─── Fork workspace to draft App manifest ────────────────────────────────────
//
// POST /api/workspaces/:id/fork-to-app
//
// Introspects the workspace and returns a draft PartialManifest the UI can
// review before publishing. The endpoint is read-only — it doesn't register
// the App. The user reviews the draft (and the structured suggestions about
// what looks intentional vs incidental), edits as needed, and then POSTs the
// finalized manifest to POST /api/apps the regular way.
//
// This is the server side of the "Save workspace as App" workspace-header
// button. See src/apps/workspace_fork.rs for the introspection logic.

pub async fn fork_workspace_to_app_draft_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use fermi::apps::workspace_fork::{introspect_workspace_to_draft, ForkError};

    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let user_id = principal.user_id();

    let draft = introspect_workspace_to_draft(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|e| match e {
            ForkError::NotFound => (StatusCode::NOT_FOUND, e.to_string()),
            ForkError::NotOwned => (StatusCode::FORBIDDEN, e.to_string()),
            ForkError::AlreadyAnApp(_) => (StatusCode::CONFLICT, e.to_string()),
            ForkError::Db(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
        })?;

    // Render the structured issues so the UI can group them by severity.
    let issues: Vec<serde_json::Value> = draft
        .issues
        .iter()
        .map(|i| {
            serde_json::json!({
                "severity": match i.severity {
                    fermi::apps::builder::Severity::Error => "error",
                    fermi::apps::builder::Severity::Warning => "warning",
                    fermi::apps::builder::Severity::Info => "info",
                    fermi::apps::builder::Severity::Suggestion => "suggestion",
                },
                "field": i.field,
                "message": i.message,
                "fix": i.fix.as_ref().map(|f| serde_json::json!({
                    "label": f.label,
                    "patch": f.patch.clone(),
                })),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "draft_manifest": draft.manifest.to_value(),
        "issues": issues,
        "source": {
            "workspace_id": draft.source.workspace_id,
            "workspace_name": draft.source.workspace_name,
            "workspace_slug": draft.source.workspace_slug,
            "mission": draft.source.mission,
            "composition_strategist_id": draft.source.composition_strategist_id,
            "origin": draft.source.origin,
            "message_count": draft.source.message_count,
            "agent_count": draft.source.agent_count,
        },
        "next_steps": [
            "Review the draft_manifest below.",
            "Accept/reject the suggestions in `issues` — each suggestion may carry a `fix.patch` (JSON Patch) the UI can apply with one click.",
            "Edit the draft as needed, especially: slug (must be unique), tagline, description, visibility.",
            "POST the finalized manifest to /api/apps to register the App."
        ]
    })))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: agent IDs containing `/` (e.g. `efra-ai/05-valuation`)
    /// must parse as one whole mention, not get truncated at the slash.
    /// Before the fix, the regex `[a-zA-Z0-9_-]+` stopped at `/` and
    /// `resolve_agent("efra-ai")` returned the "not in workspace" error
    /// even when the slash-suffixed agent had just been added.
    #[test]
    fn parse_at_mention_accepts_slashed_agent_name() {
        let parsed = parse_at_mention("@efra-ai/05-valuation please provide one for ASTS");
        assert_eq!(
            parsed,
            Some((
                "efra-ai/05-valuation".to_string(),
                "please provide one for ASTS".to_string()
            ))
        );
    }

    #[test]
    fn parse_at_mention_accepts_simple_agent_name() {
        let parsed = parse_at_mention("@xaman_ek what's next?");
        assert_eq!(
            parsed,
            Some(("xaman_ek".to_string(), "what's next?".to_string()))
        );
    }

    #[test]
    fn parse_at_mention_accepts_dotted_agent_name() {
        let parsed = parse_at_mention("@foo.bar.v2 hello");
        assert_eq!(parsed, Some(("foo.bar.v2".to_string(), "hello".to_string())));
    }

    #[test]
    fn parse_at_mention_strips_trailing_sentence_period() {
        let parsed = parse_at_mention("Hey @efra-ai/05-valuation. Thanks.");
        // Trailing `.` belongs to the prose, not the mention.
        assert_eq!(
            parsed.as_ref().map(|p| p.0.as_str()),
            Some("efra-ai/05-valuation")
        );
    }

    #[test]
    fn parse_at_mention_returns_none_without_mention() {
        assert_eq!(parse_at_mention("no mention here"), None);
    }

    #[test]
    fn parse_at_mention_returns_none_when_query_empty() {
        // Pure mention with no surrounding text is not actionable.
        assert_eq!(parse_at_mention("@xaman_ek"), None);
    }
}
