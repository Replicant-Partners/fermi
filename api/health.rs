use serde_json::{json, Value};
use vercel_runtime::{run, service_fn, Error, Request};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let service = service_fn(handler);
    run(service).await
}

async fn handler(_req: Request) -> Result<Value, Error> {
    Ok(json!({
        "status": "ok",
        "service": "agent-bestiary",
        "description": "Active Dreaming Memory backend for AI agents",
        "version": "1.0.0",
        "api_version": "v1"
    }))
}
