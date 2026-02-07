use vercel_runtime::{run, Body, Error, Request, Response, StatusCode};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let path = req.uri().path();
    
    match (req.method().as_str(), path) {
        ("GET", "/api/health") => health_check().await,
        ("GET", "/api/agents") => {
            // TODO: Implement with database
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"message": "agents endpoint - TODO"}).to_string()))?)
        },
        _ => {
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))?)
        }
    }
}

async fn health_check() -> Result<Response<Body>, Error> {
    let response = json!({
        "status": "ok",
        "service": "agent-bestiary",
        "description": "Active Dreaming Memory backend for AI agents",
        "version": "1.0.0",
        "api_version": "v1"
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(response.to_string()))?)
}
