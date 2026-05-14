#!/usr/bin/env rust
//! Web UI for Fermi Agent Bestiary
//!
//! Beautiful dashboard using Ayu Mirage theme matching the report system

use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use fermi::agent_backend::{
    agent_card::AgentCard, executor::ExecutionContext, llm_executor::LLMExecutor,
    registry::AgentRegistry,
};
use fermi::ast::{AgentStmt, ExecutorType};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::services::ServeDir;

/// Application state
#[derive(Clone)]
struct AppState {
    registry: Arc<AgentRegistry>,
}

/// Template for agent listing page
#[derive(Template)]
#[template(path = "agents_list.html")]
struct AgentsTemplate {
    page: String,
    agents: Vec<AgentCard>,
    total_executions: u32,
    total_cost: f64,
    avg_success_rate: f64,
}

/// Template for agent detail page
#[derive(Template)]
#[template(path = "agent_view.html")]
struct AgentDetailTemplate {
    page: String,
    agent: AgentCard,
}

/// Template for execution page
#[derive(Template)]
#[template(path = "agent_execute.html")]
struct ExecuteTemplate {
    page: String,
    agent: AgentCard,
}

/// Template for dashboard
#[derive(Template)]
#[template(path = "dashboard_view.html")]
struct DashboardTemplate {
    page: String,
    agents: Vec<AgentCard>,
    total_agents: usize,
    total_executions: u32,
    total_tokens: u64,
    total_cost: f64,
    avg_success_rate: f64,
    avg_confidence: f64,
    total_entities: u32,
    total_relationships: u32,
    evolution_commits: u32,
    total_forecasts: u32,
}

/// Execute request
#[derive(Deserialize)]
struct ExecuteRequest {
    query: String,
}

/// Execute response
#[derive(Serialize)]
struct ExecuteResponse {
    agent_name: String,
    status: String,
    confidence: f64,
    execution_time_ms: u64,
    tokens_used: u64,
    evidence: Vec<EvidenceResponse>,
}

#[derive(Serialize)]
struct EvidenceResponse {
    id: String,
    source: String,
    summary: String,
    key_findings: Vec<String>,
    relevance: f64,
}

/// Error response
#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

/// Askama error handler
struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template: {}", err),
            )
                .into_response(),
        }
    }
}

/// Home page - list all agents
async fn index(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let agents = state
        .registry
        .list_cards()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total_executions: u32 = agents.iter().map(|a| a.usage.total_executions).sum();
    let total_cost: f64 = agents.iter().map(|a| a.usage.total_cost_usd).sum();
    let avg_success_rate = if !agents.is_empty() {
        agents
            .iter()
            .map(|a| a.performance.accuracy_rate)
            .sum::<f64>()
            / agents.len() as f64
    } else {
        0.0
    };

    let template = AgentsTemplate {
        page: "agents".to_string(),
        agents,
        total_executions,
        total_cost,
        avg_success_rate,
    };

    Ok(HtmlTemplate(template))
}

/// Agent detail page
async fn agent_detail(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let agent = state
        .registry
        .get(&agent_id)
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let template = AgentDetailTemplate {
        page: "agents".to_string(),
        agent,
    };

    Ok(HtmlTemplate(template))
}

/// Execute page
async fn execute_page(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let agent = state
        .registry
        .get(&agent_id)
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let template = ExecuteTemplate {
        page: "agents".to_string(),
        agent,
    };

    Ok(HtmlTemplate(template))
}

/// Execute agent API endpoint
async fn execute_agent_api(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(request): Json<ExecuteRequest>,
) -> Result<Json<ExecuteResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get agent card
    let card = state.registry.get(&agent_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Agent not found: {}", e),
            }),
        )
    })?;

    // Create agent statement
    let agent = AgentStmt {
        name: agent_id.clone(),
        agent_type: Some("research".to_string()),
        query: request.query.clone(),
        executor: Some(ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    // Create execution context
    let program = fermi::ast::Program {
        statements: vec![fermi::ast::Statement::Agent(agent.clone())],
    };

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
    };

    // Execute agent
    let result = state
        .registry
        .execute_agent(&agent, &context)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Execution failed: {}", e),
                }),
            )
        })?;

    // Record execution
    state
        .registry
        .record_execution(&agent_id, &result)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to record execution: {}", e),
                }),
            )
        })?;

    // Convert response
    let response = ExecuteResponse {
        agent_name: result.agent_name,
        status: format!("{:?}", result.status),
        confidence: result.confidence,
        execution_time_ms: result.execution_time_ms,
        tokens_used: result.tokens_used.unwrap_or(0) as u64,
        evidence: result
            .evidence
            .iter()
            .map(|e| EvidenceResponse {
                id: e.id.clone(),
                source: e.source.clone(),
                summary: e.summary.clone().unwrap_or_default(),
                key_findings: e.key_findings.clone(),
                relevance: e.relevance.unwrap_or(0.0),
            })
            .collect(),
    };

    Ok(Json(response))
}

/// Dashboard page
async fn dashboard(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let agents = state
        .registry
        .list_cards()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total_agents = agents.len();
    let total_executions: u32 = agents.iter().map(|a| a.usage.total_executions).sum();
    let total_tokens: u64 = agents.iter().map(|a| a.usage.total_tokens_used).sum();
    let total_cost: f64 = agents.iter().map(|a| a.usage.total_cost_usd).sum();

    let avg_success_rate = if !agents.is_empty() {
        agents
            .iter()
            .map(|a| a.performance.accuracy_rate)
            .sum::<f64>()
            / agents.len() as f64
    } else {
        0.0
    };

    let avg_confidence = if !agents.is_empty() {
        agents
            .iter()
            .map(|a| a.performance.avg_confidence)
            .sum::<f64>()
            / agents.len() as f64
    } else {
        0.0
    };

    let total_entities: u32 = agents.iter().map(|a| a.ontology_stats.entities).sum();
    let total_relationships: u32 = agents.iter().map(|a| a.ontology_stats.relationships).sum();
    let evolution_commits: u32 = agents
        .iter()
        .map(|a| a.ontology_stats.evolution_commits)
        .sum();
    let total_forecasts: u32 = agents
        .iter()
        .map(|a| a.performance.forecasts_contributed)
        .sum();

    let template = DashboardTemplate {
        page: "dashboard".to_string(),
        agents,
        total_agents,
        total_executions,
        total_tokens,
        total_cost,
        avg_success_rate,
        avg_confidence,
        total_entities,
        total_relationships,
        evolution_commits,
        total_forecasts,
    };

    Ok(HtmlTemplate(template))
}

/// Save agent to git
async fn save_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());

    state
        .registry
        .save_and_commit(&agent_id, &agents_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "message": format!("Agent '{}' saved and committed to git", agent_id),
        "agent_id": agent_id
    })))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,agent_web_ui=debug".to_string()),
        )
        .init();

    // Load agents directory
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());

    // Create agent registry with LLM executor if API key is available
    let registry = if let Ok(llm_executor) = LLMExecutor::from_env() {
        tracing::info!("✓ Using LLM Executor (Claude API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(llm_executor)))
    } else {
        tracing::warn!("⚠ No ANTHROPIC_API_KEY found, using Mock Executor");
        Arc::new(AgentRegistry::new())
    };

    // Load agents from filesystem
    match registry.load_from_directory(&agents_dir) {
        Ok(count) if count > 0 => {
            tracing::info!("✓ Loaded {} agent(s) from {}", count, agents_dir);
        }
        Ok(_) => {
            tracing::warn!("⚠ No agents found in {}", agents_dir);
        }
        Err(e) => {
            tracing::error!("⚠ Failed to load agents: {}", e);
        }
    }

    let state = AppState { registry };

    // Build router
    let app = Router::new()
        .route("/", get(index))
        .route("/agents/:id", get(agent_detail))
        .route("/agents/:id/execute", get(execute_page))
        .route("/agents/:id/save", post(save_agent))
        .route("/api/agents/:id/execute", post(execute_agent_api))
        .route("/dashboard", get(dashboard))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    // Start server
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3002".to_string())
        .parse::<u16>()?;

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("🚀 Fermi Agent Bestiary Web UI starting on http://{}", addr);
    tracing::info!("   Open http://localhost:{} in your browser", port);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
