use agent_bestiary_memory::MemoryStore;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use uuid::Uuid;
use vercel_runtime::{run, service_fn, Body, Error, Request, Response, StatusCode};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let service = service_fn(handler);
    run(service).await
}

async fn handler(req: Request) -> Result<Response<Body>, Error> {
    match req.method().as_str() {
        "GET" => list_agents(req).await,
        "POST" => create_agent(req).await,
        _ => Ok(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::from(
                json!({"error": {"code": "METHOD_NOT_ALLOWED", "message": "Method not allowed"}})
                    .to_string(),
            ))?),
    }
}

async fn list_agents(_req: Request) -> Result<Response<Body>, Error> {
    // Get database URL from environment
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| Error::from("DATABASE_URL environment variable not set"))?;

    // Connect to database
    let store = MemoryStore::new(&database_url)
        .await
        .map_err(|e| Error::from(format!("Database connection failed: {}", e)))?;

    // Fetch all agents
    let agents = store
        .list_agents()
        .await
        .map_err(|e| Error::from(format!("Failed to list agents: {}", e)))?;

    // Format response
    let agent_list: Vec<Value> = agents
        .iter()
        .map(|agent| {
            json!({
                "agent_id": agent.agent_id,
                "agent_name": agent.agent_name,
                "agent_type": agent.agent_type,
                "version": agent.version,
                "tier": agent.tier,
                "created_at": agent.created_at,
                "stats": {
                    "total_executions": agent.total_executions,
                    "successful_executions": agent.successful_executions,
                    "failed_executions": agent.failed_executions,
                    "total_cost_usd": agent.total_cost_usd,
                    "avg_execution_time_ms": agent.avg_execution_time_ms
                }
            })
        })
        .collect();

    let response = json!({
        "agents": agent_list,
        "total": agents.len(),
        "page": 1,
        "per_page": 20
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(response.to_string()))?)
}

async fn create_agent(req: Request) -> Result<Response<Body>, Error> {
    // Parse request body
    use http_body_util::BodyExt;
    let body_bytes = req.into_body().collect().await?.to_bytes();
    let create_req: CreateAgentRequest = serde_json::from_slice(&body_bytes)
        .map_err(|e| Error::from(format!("Invalid request body: {}", e)))?;

    // Get database URL
    let database_url = env::var("DATABASE_URL")
        .map_err(|_| Error::from("DATABASE_URL environment variable not set"))?;

    // Connect to database
    let store = MemoryStore::new(&database_url)
        .await
        .map_err(|e| Error::from(format!("Database connection failed: {}", e)))?;

    // Build agent struct
    let agent = agent_bestiary_memory::Agent {
        agent_id: Uuid::new_v4(),
        agent_name: create_req.agent_name.clone(),
        agent_type: create_req.agent_type,
        version: "1.0.0".to_string(),
        tier: "community".to_string(),
        executor_type: create_req.executor_type,
        model: create_req.model,
        temperature: create_req.temperature,
        mcp_servers: None,
        description: create_req.description,
        author: create_req.author.unwrap_or_else(|| "Unknown".to_string()),
        created_at: Utc::now(),
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: 0.0,
        avg_execution_time_ms: 0,
    };

    // Upsert agent
    let agent_id = store
        .upsert_agent(agent)
        .await
        .map_err(|e| Error::from(format!("Failed to create agent: {}", e)))?;

    let response = json!({
        "agent_id": agent_id,
        "agent_name": create_req.agent_name,
        "created_at": Utc::now(),
        "github_url": null
    });

    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .header("Content-Type", "application/json")
        .body(Body::from(response.to_string()))?)
}

#[derive(Debug, Deserialize)]
struct CreateAgentRequest {
    agent_name: String,
    agent_type: String,
    executor_type: String,
    model: String,
    #[serde(default = "default_temperature")]
    temperature: f32,
    description: Option<String>,
    author: Option<String>,
}

fn default_temperature() -> f32 {
    0.3
}
