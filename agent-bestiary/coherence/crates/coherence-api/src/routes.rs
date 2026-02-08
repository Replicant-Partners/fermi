//! API route handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use coherence_core::{
    types::{ConversationId, Message, ParticipantId},
    CoherenceSnapshot, CoherenceSystem,
};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

use crate::state::{AppState, Session};

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub title: Option<String>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: ConversationId,
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct SubmitMessageRequest {
    pub participant_id: Option<ParticipantId>,
    pub content: String,
}

#[derive(Serialize)]
pub struct SubmitMessageResponse {
    pub utterance_count: usize,
    pub relation_count: usize,
}

#[derive(Serialize)]
pub struct SessionSummary {
    pub session_id: ConversationId,
    pub title: Option<String>,
    pub utterance_count: usize,
    pub evaluated: bool,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ─── Router ────────────────────────────────────────────────────────────────

/// Build the Axum router with all endpoints.
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", get(get_session))
        .route("/sessions/{id}/messages", post(submit_message))
        .route("/sessions/{id}/evaluate", post(evaluate))
        .with_state(state)
}

// ─── Handlers ──────────────────────────────────────────────────────────────

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "coherence-evaluator"
    }))
}

async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> (StatusCode, Json<CreateSessionResponse>) {
    let conv_id = ConversationId::new();
    let observer = ConversationObserver::new(conv_id);
    let system = CoherenceSystem::new(conv_id);

    let session = Session {
        system,
        observer,
        title: req.title.clone(),
    };

    state.sessions.write().await.insert(conv_id, session);

    (
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            session_id: conv_id,
            title: req.title,
        }),
    )
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<SessionSummary>> {
    let sessions = state.sessions.read().await;
    let summaries: Vec<SessionSummary> = sessions
        .iter()
        .map(|(id, session)| SessionSummary {
            session_id: *id,
            title: session.title.clone(),
            utterance_count: session.system.utterance_count(),
            evaluated: session.system.is_evaluated(),
        })
        .collect();
    Json(summaries)
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<CoherenceSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let conv_id = ConversationId(id);
    let sessions = state.sessions.read().await;
    let session = sessions.get(&conv_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("session {id} not found"),
            }),
        )
    })?;

    Ok(Json(session.system.snapshot()))
}

async fn submit_message(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(req): Json<SubmitMessageRequest>,
) -> Result<Json<SubmitMessageResponse>, (StatusCode, Json<ErrorResponse>)> {
    let conv_id = ConversationId(id);
    let mut sessions = state.sessions.write().await;
    let session = sessions.get_mut(&conv_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("session {id} not found"),
            }),
        )
    })?;

    let participant_id = req.participant_id.unwrap_or_else(ParticipantId::new);
    let message = Message::new(participant_id, req.content);
    session
        .observer
        .observe_message(&mut session.system, &message);

    Ok(Json(SubmitMessageResponse {
        utterance_count: session.system.utterance_count(),
        relation_count: session.system.relation_count(),
    }))
}

async fn evaluate(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<CoherenceSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let conv_id = ConversationId(id);
    let mut sessions = state.sessions.write().await;
    let session = sessions.get_mut(&conv_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("session {id} not found"),
            }),
        )
    })?;

    let engine = SettlingEngine::new(state.settling_config.clone());
    engine.settle(&mut session.system);

    Ok(Json(session.system.snapshot()))
}
