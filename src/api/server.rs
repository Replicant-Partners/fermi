/// Axum Server Configuration
use crate::agent_backend::AgentRegistry;
use crate::api::handlers::{
    execute_agent, get_agent, health_check, list_agents, register_agent, save_agent, AppState,
};
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Create the Axum application with all routes
pub fn create_app(registry: Arc<AgentRegistry>) -> Router {
    let state = AppState { registry };

    Router::new()
        // Health check
        .route("/health", get(health_check))
        // Agent endpoints
        .route("/agents", get(list_agents))
        .route("/agents", post(register_agent))
        .route("/agents/:id", get(get_agent))
        .route("/agents/:id/execute", post(execute_agent))
        .route("/agents/:id/save", post(save_agent))
        // Add state
        .with_state(state)
        // Add middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_health_check() {
        let registry = Arc::new(AgentRegistry::new());
        let app = create_app(registry);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
