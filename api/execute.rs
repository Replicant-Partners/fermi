use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vercel_runtime::{Error, Request, run, service_fn, Body};

#[derive(Deserialize)]
struct ExecuteRequest {
    fpl_code: String,
    iterations: Option<usize>,
}

#[derive(Serialize)]
struct ExecuteResult {
    p50: f64,
    p10: f64,
    p90: f64,
    mean: f64,
    iterations: usize,
    duration_ms: u64,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let service = service_fn(handler);
    run(service).await
}

async fn handler(req: Request) -> Result<Value, Error> {
    // Read the request body
    let body_bytes = match req.body() {
        Body::Text(s) => s.as_bytes().to_vec(),
        Body::Binary(b) => b.to_vec(),
        Body::Empty => vec![],
    };

    let execute_req: ExecuteRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            return Ok(json!({
                "success": false,
                "error": format!("Invalid request: {}", e)
            }));
        }
    };

    // TODO: Integrate actual FPL execution engine
    // For now, return placeholder response
    let result = ExecuteResult {
        p50: 1200.0,
        p10: 800.0,
        p90: 1800.0,
        mean: 1205.0,
        iterations: execute_req.iterations.unwrap_or(10000),
        duration_ms: 234,
    };

    Ok(json!({
        "success": true,
        "result": result
    }))
}
