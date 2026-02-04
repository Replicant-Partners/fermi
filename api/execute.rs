use http::{StatusCode, header};
use vercel_runtime::{run, Body, Error, Request, Response};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ExecuteRequest {
    fpl_code: String,
    iterations: Option<usize>,
}

#[derive(Serialize)]
struct ExecuteResponse {
    success: bool,
    result: Option<ExecuteResult>,
    error: Option<String>,
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
    run(handler).await
}

async fn handler(req: Request) -> Result<Response<Body>, Error> {
    let body_bytes = req.body();

    let execute_req: ExecuteRequest = match serde_json::from_slice(body_bytes) {
        Ok(req) => req,
        Err(e) => {
            let error_response = ExecuteResponse {
                success: false,
                result: None,
                error: Some(format!("Invalid request: {}", e)),
            };
            return Ok(Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::Text(serde_json::to_string(&error_response)?))?)
        }
    };

    // TODO: Integrate actual FPL execution engine
    // For now, return placeholder response
    let response = ExecuteResponse {
        success: true,
        result: Some(ExecuteResult {
            p50: 1200.0,
            p10: 800.0,
            p90: 1800.0,
            mean: 1205.0,
            iterations: execute_req.iterations.unwrap_or(10000),
            duration_ms: 234,
        }),
        error: None,
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::Text(serde_json::to_string(&response)?))?)
}
