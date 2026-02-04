use http::{StatusCode, header};
use vercel_runtime::{run, Body, Error, Request, Response};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(handler).await
}

async fn handler(_req: Request) -> Result<Response<Body>, Error> {
    let response = serde_json::json!({
        "status": "ok",
        "service": "fermi-backend",
        "version": "0.4.0"
    });

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::Text(response.to_string()))?)
}
