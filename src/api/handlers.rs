/// API Request Handlers
use crate::agent_backend::{AgentCard, AgentRegistry, ExecutionContext};
use crate::api::types::{
    ErrorResponse, ExecuteAgentRequest, ExecuteAgentResponse, ListAgentsResponse,
};
use crate::ast::{AgentStmt, Program, Schedule, TimeUnit};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use std::sync::Arc;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<AgentRegistry>,
}

/// List all agents
pub async fn list_agents(
    State(state): State<AppState>,
) -> Result<Json<ListAgentsResponse>, AppError> {
    let agents = state
        .registry
        .list()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ListAgentsResponse { agents }))
}

/// Get a specific agent card
pub async fn get_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<AgentCard>, AppError> {
    let card = state
        .registry
        .get(&agent_id)
        .map_err(|_| AppError::NotFound(format!("Agent '{}' not found", agent_id)))?;

    Ok(Json(card))
}

/// Execute an agent
pub async fn execute_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(request): Json<ExecuteAgentRequest>,
) -> Result<Json<ExecuteAgentResponse>, AppError> {
    // Get agent card
    let card = state
        .registry
        .get(&agent_id)
        .map_err(|_| AppError::NotFound(format!("Agent '{}' not found", agent_id)))?;

    // Create agent statement from request
    let agent_stmt = AgentStmt {
        name: agent_id.clone(),
        agent_type: request.agent_type.or(Some(card.agent_type.clone())),
        query: request.query.clone(),
        executor: Some(card.capabilities.executor.clone()),
        schedule: Some(Schedule::Every {
            interval: 1,
            unit: TimeUnit::Day,
        }),
        driver_refs: request.driver_refs.unwrap_or_default(),
        depends_on: request.depends_on.unwrap_or_default(),
        confidence_threshold: request.confidence_threshold,
    };

    // Create execution context
    let context = ExecutionContext {
        program: Program { statements: vec![] },
        agent_card: card.clone(),
    };

    // Execute agent
    let output = state
        .registry
        .execute_agent(&agent_stmt, &context)
        .await
        .map_err(|e| AppError::Execution(e.to_string()))?;

    // Record execution stats
    state
        .registry
        .record_execution(&agent_id, &output)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ExecuteAgentResponse::from(&output)))
}

/// Register a new agent
pub async fn register_agent(
    State(state): State<AppState>,
    Json(card): Json<AgentCard>,
) -> Result<Json<AgentCard>, AppError> {
    state
        .registry
        .register(card.clone())
        .map_err(|e| AppError::Conflict(e.to_string()))?;

    Ok(Json(card))
}

/// Health check endpoint
pub async fn health_check() -> &'static str {
    "OK"
}

/// Save an agent card to filesystem
pub async fn save_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());

    state
        .registry
        .save_and_commit(&agent_id, &agents_dir)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({
        "message": format!("Agent '{}' saved and committed to git", agent_id)
    })))
}

/// Application errors
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Internal(String),
    Conflict(String),
    Execution(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::Execution(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: status.to_string(),
            message,
        });

        (status, body).into_response()
    }
}
