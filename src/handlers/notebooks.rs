//! Fermi FPL API Handlers
//!
//! Two categories of endpoints:
//!
//! **1. Stateless FPL execution** — `POST /api/fpl/execute`
//!    Submit raw FPL source, get back simulation results immediately.
//!    Used by the Fermi thick client's ⌘R simulation and any external
//!    integrations. No persistence; credits charged per run.
//!
//! **2. Notebook CRUD** — `/api/notebooks/*`
//!    Persistent FPL programs ("notebooks") owned by a user.
//!    The thick client writes `.fpl` files locally; notebooks are the
//!    server-side counterpart used by the web dashboard and sharing flows.
//!    Originally conceived as a browser-based FPL editor — that role is
//!    now fully covered by the GPUI thick client, but the table / API
//!    remain useful as a publish/share target.
//!
//! Both paths share the same FPL lexer → parser → executor pipeline.
//! The `execute_notebook_handler` also converts cell-based JSON into FPL
//! before executing, for legacy compatibility.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Row;
use std::time::Instant;
use uuid::Uuid;

use crate::AppState;
use fermi::gas::charge_gas;
use fermi::{executor::Executor, lexer::Lexer, parser::Parser};
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

// ─── Request/Response Types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateNotebookRequest {
    pub title: String,
    pub description: Option<String>,
    pub visibility: Option<String>, // "private" | "shared" | "public"
    pub team_id: Option<String>,
    pub org_id: Option<String>,
    pub template_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NotebookResponse {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub permissions: NotebookPermissions,
    pub cells: Vec<JsonValue>,
    pub dependency_graph: JsonValue,
    pub metadata: NotebookMetadata,
}

#[derive(Debug, Serialize)]
pub struct NotebookPermissions {
    pub visibility: String,
    pub owner_id: String,
    pub team_id: Option<String>,
    pub org_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NotebookMetadata {
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub author_id: String,
    pub tags: Vec<String>,
    pub portfolio_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNotebookRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub cells: Option<Vec<JsonValue>>,
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListNotebooksQuery {
    pub visibility: Option<String>,
    pub portfolio_id: Option<String>,
    pub team_id: Option<String>,
    pub org_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteNotebookRequest {
    pub iterations: Option<i32>,
    pub seed: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteNotebookResponse {
    pub cells: JsonValue, // Map of cell_id -> output
    pub final_probability: Option<f64>,
    pub execution_state: ExecutionState,
    pub total_time_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct ExecutionState {
    pub status: String, // "idle" | "running" | "success" | "error"
    pub completed_cells: Vec<String>,
    pub error_message: Option<String>,
}

// ─── Handlers ───────────────────────────────────────────────────────

/// POST /api/notebooks
pub async fn create_notebook_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateNotebookRequest>,
) -> Result<Json<NotebookResponse>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let notebook_id = Uuid::new_v4();
    let user_id = principal.user_id();
    let visibility = req.visibility.unwrap_or_else(|| "private".to_string());

    // Validate visibility
    if !["private", "shared", "public"].contains(&visibility.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            "visibility must be 'private', 'shared', or 'public'".into(),
        ));
    }

    // Create empty notebook
    let now = chrono::Utc::now();
    let cells = serde_json::json!([]);
    let dependency_graph = serde_json::json!({"nodes": [], "edges": []});

    sqlx::query(
        "INSERT INTO fermi_notebooks
         (id, title, description, owner_id, visibility, team_id, org_id,
          cells, execution_state, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(notebook_id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(
        Uuid::parse_str(&user_id)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
    )
    .bind(&visibility)
    .bind(&req.team_id.as_ref().and_then(|s| Uuid::parse_str(s).ok()))
    .bind(&req.org_id)
    .bind(&cells)
    .bind("idle") // execution_state
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = NotebookResponse {
        id: notebook_id.to_string(),
        title: req.title,
        description: req.description,
        permissions: NotebookPermissions {
            visibility,
            owner_id: user_id.to_string(),
            team_id: req.team_id,
            org_id: req.org_id,
        },
        cells: vec![],
        dependency_graph,
        metadata: NotebookMetadata {
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            version: 1,
            author_id: user_id.to_string(),
            tags: vec![],
            portfolio_id: None,
        },
    };

    Ok(Json(response))
}

/// GET /api/notebooks/:id
pub async fn get_notebook_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id): Path<Uuid>,
) -> Result<Json<NotebookResponse>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();

    let row = sqlx::query(
        "SELECT id, title, description, owner_id, visibility, team_id, org_id,
                cells, execution_state, last_executed_at, created_at, updated_at
         FROM fermi_notebooks
         WHERE id = $1",
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Notebook not found".to_string()))?;

    let owner_id: Uuid = row.get("owner_id");
    let visibility: String = row.get("visibility");

    // Access control
    if visibility == "private" && owner_id.to_string() != user_id {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let response = NotebookResponse {
        id: row.get::<String, _>("id"),
        title: row.get("title"),
        description: row.get("description"),
        permissions: NotebookPermissions {
            visibility,
            owner_id: owner_id.to_string(),
            team_id: row.get::<Option<Uuid>, _>("team_id").map(|u| u.to_string()),
            org_id: row.get("org_id"),
        },
        cells: row.get("cells"),
        dependency_graph: serde_json::json!({"nodes": [], "edges": []}), // Not stored in DB currently
        metadata: NotebookMetadata {
            created_at: row
                .get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .to_rfc3339(),
            updated_at: row
                .get::<chrono::DateTime<chrono::Utc>, _>("updated_at")
                .to_rfc3339(),
            version: 1, // Not stored in DB currently
            author_id: owner_id.to_string(),
            tags: vec![],       // Not stored in DB currently
            portfolio_id: None, // Not stored in DB currently
        },
    };

    Ok(Json(response))
}

/// PUT /api/notebooks/:id
pub async fn update_notebook_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateNotebookRequest>,
) -> Result<Json<NotebookResponse>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();

    // Check ownership
    let owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM fermi_notebooks WHERE id = $1")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Notebook not found".to_string()))?;

    if owner.to_string() != user_id {
        return Err((StatusCode::FORBIDDEN, "Not the owner".to_string()));
    }

    // Update with COALESCE for optional fields
    let now = chrono::Utc::now();
    sqlx::query(
        "UPDATE fermi_notebooks
         SET title = COALESCE($2, title),
             description = COALESCE($3, description),
             cells = COALESCE($4, cells),
             visibility = COALESCE($5, visibility),
             updated_at = $6
         WHERE id = $1",
    )
    .bind(id.to_string())
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.cells)
    .bind(&req.visibility)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Return updated notebook
    get_notebook_handler(State(state), principal, Path(id)).await
}

/// DELETE /api/notebooks/:id
pub async fn delete_notebook_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();

    let owner: Uuid = sqlx::query_scalar("SELECT owner_id FROM fermi_notebooks WHERE id = $1")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Notebook not found".to_string()))?;

    if owner.to_string() != user_id {
        return Err((StatusCode::FORBIDDEN, "Not the owner".to_string()));
    }

    sqlx::query("DELETE FROM fermi_notebooks WHERE id = $1")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/notebooks
pub async fn list_notebooks_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Query(query): Query<ListNotebooksQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = state.memory_store.pool();
    let user_id = principal.user_id();
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0);

    // Build query based on visibility
    let rows = if let Some(vis) = &query.visibility {
        sqlx::query(
            "SELECT id, title, description, owner_id, visibility, created_at, updated_at
             FROM fermi_notebooks
             WHERE visibility = $1
             ORDER BY updated_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(vis)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await
    } else {
        // Default: show user's private notebooks + shared/public ones
        sqlx::query(
            "SELECT id, title, description, owner_id, visibility, created_at, updated_at
             FROM fermi_notebooks
             WHERE owner_id::text = $1 OR visibility IN ('shared', 'public')
             ORDER BY updated_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&user_id)
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(pool)
        .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let notebooks: Vec<JsonValue> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.get::<String, _>("id"),
                "title": row.get::<String, _>("title"),
                "description": row.get::<Option<String>, _>("description"),
                "visibility": row.get::<String, _>("visibility"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "notebooks": notebooks,
        "total": notebooks.len()
    })))
}

/// POST /api/notebooks/:id/execute
pub async fn execute_notebook_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteNotebookRequest>,
) -> Result<Json<ExecuteNotebookResponse>, (StatusCode, String)> {
    let start_time = Instant::now();
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Get notebook
    let notebook = sqlx::query("SELECT cells, owner_id FROM fermi_notebooks WHERE id = $1")
        .bind(id.to_string())
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Notebook not found".to_string()))?;

    let cells: Vec<JsonValue> = notebook.get("cells");
    let owner_id: Uuid = notebook.get("owner_id");

    // Access control: owner or shared/public
    // (simplified - you may want to check visibility)
    if owner_id.to_string() != user_id {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    // Charge credits for execution
    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Base cost: 1 credit per execution + 1 credit per 1000 iterations
    let iterations = req.iterations.unwrap_or(10000);
    let iteration_cost = (iterations / 1000).max(1);
    let total_cost = 1 + iteration_cost;

    charge_gas(
        pool,
        wallet.wallet_id,
        total_cost,
        "notebook_execute",
        &format!("Execute notebook {} ({} iterations)", id, iterations),
        Some(&id.to_string()),
    )
    .await
    .map_err(|e| e)?;

    // Convert cells to FPL program text
    let fpl_source = cells_to_fpl(&cells)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Invalid cells: {}", e)))?;

    // Parse FPL
    let lexer = Lexer::new(&fpl_source);
    let tokens = lexer.tokenize().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Tokenization failed: {:?}", e),
        )
    })?;

    let parser = Parser::new(tokens);
    let program = parser
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse failed: {}", e)))?;

    // Execute FPL
    let mut executor = if let Some(seed_val) = req.seed {
        Executor::with_seed(iterations as usize, seed_val as u64)
    } else {
        Executor::new(iterations as usize)
    };

    let results = executor.execute(&program).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Execution failed: {}", e),
        )
    })?;

    // Build cell outputs
    let mut cell_outputs = serde_json::Map::new();

    // For now, put the main results in a "model" cell output
    // TODO: Map individual driver outputs to their respective cells
    cell_outputs.insert(
        "results".to_string(),
        serde_json::json!({
            "mean": results.mean,
            "median": results.median,
            "std_dev": results.std_dev,
            "p5": results.p5,
            "p25": results.p25,
            "p75": results.p75,
            "p95": results.p95,
            "min": results.min,
            "max": results.max,
            "iterations": results.iterations,
            "base_rate": results.base_rate,
            "divergence_relative": results.divergence_relative,
            "divergence_absolute": results.divergence_absolute,
        }),
    );

    let elapsed = start_time.elapsed().as_millis() as i64;

    // Update notebook execution state
    sqlx::query(
        "UPDATE fermi_notebooks
         SET execution_state = 'complete',
             last_executed_at = NOW(),
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id.to_string())
    .execute(pool)
    .await
    .ok(); // Don't fail if update fails

    let response = ExecuteNotebookResponse {
        cells: serde_json::Value::Object(cell_outputs),
        final_probability: Some(results.mean),
        execution_state: ExecutionState {
            status: "success".to_string(),
            completed_cells: vec!["results".to_string()],
            error_message: None,
        },
        total_time_ms: elapsed,
    };

    Ok(Json(response))
}

/// Convert notebook cells (JSONB) to FPL program text
fn cells_to_fpl(cells: &[JsonValue]) -> Result<String, String> {
    let mut fpl_lines = Vec::new();

    for cell in cells {
        let cell_type = cell
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or("Cell missing type")?;

        match cell_type {
            "question" => {
                if let Some(text) = cell.get("text").and_then(|t| t.as_str()) {
                    fpl_lines.push(format!("question \"{}\"", text));
                }
                if let Some(base_rate) = cell.get("base_rate") {
                    if let Some(value) = base_rate.get("value").and_then(|v| v.as_f64()) {
                        fpl_lines.push(format!("base_rate {}", value));
                    }
                }
            }
            "driver" => {
                let name = cell
                    .get("name")
                    .and_then(|n| n.as_str())
                    .ok_or("Driver cell missing name")?;
                let dist = cell
                    .get("distribution")
                    .ok_or("Driver cell missing distribution")?;
                let dist_type = dist
                    .get("type")
                    .and_then(|t| t.as_str())
                    .ok_or("Distribution missing type")?;

                // Convert distribution to FPL syntax
                let dist_str = match dist_type {
                    "normal" => {
                        let mean = dist
                            .get("mean")
                            .and_then(|m| m.as_f64())
                            .ok_or("Normal missing mean")?;
                        let std = dist
                            .get("std")
                            .and_then(|s| s.as_f64())
                            .ok_or("Normal missing std")?;
                        format!("normal({}, {})", mean, std)
                    }
                    "uniform" => {
                        let min = dist
                            .get("min")
                            .and_then(|m| m.as_f64())
                            .ok_or("Uniform missing min")?;
                        let max = dist
                            .get("max")
                            .and_then(|m| m.as_f64())
                            .ok_or("Uniform missing max")?;
                        format!("uniform({}, {})", min, max)
                    }
                    "beta" => {
                        let alpha = dist
                            .get("alpha")
                            .and_then(|a| a.as_f64())
                            .ok_or("Beta missing alpha")?;
                        let beta = dist
                            .get("beta")
                            .and_then(|b| b.as_f64())
                            .ok_or("Beta missing beta")?;
                        format!("beta({}, {})", alpha, beta)
                    }
                    "triangular" => {
                        let min = dist
                            .get("min")
                            .and_then(|m| m.as_f64())
                            .ok_or("Triangular missing min")?;
                        let mode = dist
                            .get("mode")
                            .and_then(|m| m.as_f64())
                            .ok_or("Triangular missing mode")?;
                        let max = dist
                            .get("max")
                            .and_then(|m| m.as_f64())
                            .ok_or("Triangular missing max")?;
                        format!("triangular({}, {}, {})", min, mode, max)
                    }
                    "lognormal" => {
                        let mean = dist
                            .get("mean")
                            .and_then(|m| m.as_f64())
                            .ok_or("LogNormal missing mean")?;
                        let std = dist
                            .get("std")
                            .and_then(|s| s.as_f64())
                            .ok_or("LogNormal missing std")?;
                        format!("lognormal({}, {})", mean, std)
                    }
                    _ => return Err(format!("Unknown distribution type: {}", dist_type)),
                };

                fpl_lines.push(format!("driver {} ~ {}", name, dist_str));
            }
            "model" => {
                if let Some(expr) = cell.get("expression").and_then(|e| e.as_str()) {
                    fpl_lines.push(format!("model {}", expr));
                }
            }
            _ => {
                // Skip other cell types (markdown, visualization, etc.)
            }
        }
    }

    if fpl_lines.is_empty() {
        return Err("No executable cells found".to_string());
    }

    Ok(fpl_lines.join("\n"))
}

// ─── Stateless FPL Execution ────────────────────────────────────────────────
//
// POST /api/fpl/execute
//
// Execute a raw FPL program without storing it. Used by the Fermi thick client
// for ⌘R simulation and by any external integrations that want server-side
// Monte Carlo without managing notebook state.
//
// Input:  { "fpl_source": "...", "iterations": 10000, "seed": null }
// Output: same ExecuteNotebookResponse shape

#[derive(Debug, Deserialize)]
pub struct FplExecuteRequest {
    /// The raw FPL source text to execute.
    pub fpl_source: String,
    /// Number of Monte Carlo iterations (default 10 000, max 100 000).
    pub iterations: Option<usize>,
    /// Optional deterministic seed for reproducible results.
    pub seed: Option<u32>,
}

/// Response for the stateless FPL execution endpoint.
#[derive(Debug, Serialize)]
pub struct FplExecuteResponse {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub p5: f64,
    pub p25: f64,
    pub p75: f64,
    pub p95: f64,
    pub min: f64,
    pub max: f64,
    pub base_rate: Option<f64>,
    pub divergence_relative: Option<f64>,
    pub divergence_absolute: Option<f64>,
    pub iterations: usize,
    pub execution_time_ms: i64,
    pub credits_charged: i32,
}

pub async fn fpl_execute_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<FplExecuteRequest>,
) -> Result<Json<FplExecuteResponse>, (StatusCode, String)> {
    let start_time = Instant::now();
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Charge credits: 1 base + 1 per 1 000 iterations.
    let iterations = req.iterations.unwrap_or(10_000).clamp(100, 100_000);
    let cost = 1 + (iterations / 1_000).max(1) as i32;

    let wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    charge_gas(
        pool,
        wallet.wallet_id,
        cost,
        "fpl_execute",
        &format!("Stateless FPL execution ({} iterations)", iterations),
        None,
    )
    .await
    .map_err(|e| e)?;

    // Parse FPL.
    let lexer = Lexer::new(&req.fpl_source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Tokenization failed: {:?}", e)))?;

    let parser = Parser::new(tokens);
    let program = parser
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Parse failed: {}", e)))?;

    // Execute FPL.
    let mut executor = if let Some(seed_val) = req.seed {
        Executor::with_seed(iterations, seed_val as u64)
    } else {
        Executor::new(iterations)
    };

    let results = executor
        .execute(&program)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Execution failed: {}", e)))?;

    let elapsed_ms = start_time.elapsed().as_millis() as i64;

    Ok(Json(FplExecuteResponse {
        mean: results.mean,
        median: results.median,
        std_dev: results.std_dev,
        p5: results.p5,
        p25: results.p25,
        p75: results.p75,
        p95: results.p95,
        min: results.min,
        max: results.max,
        base_rate: results.base_rate,
        divergence_relative: results.divergence_relative,
        divergence_absolute: results.divergence_absolute,
        iterations: results.iterations,
        execution_time_ms: elapsed_ms,
        credits_charged: cost,
    }))
}

/// GET /api/fpl/health — lightweight liveness probe for the FPL execution path.
/// Parses and executes a trivial FPL program to verify the engine is working.
pub async fn fpl_health_handler() -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    use fermi::{executor::Executor, lexer::Lexer, parser::Parser};

    let source = "question \"health check\" ~ 0.5\nmodel 0.5";
    let lexer = Lexer::new(source);
    let tokens = lexer
        .tokenize()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{:?}", e)))?;
    let parser = Parser::new(tokens);
    let program = parser
        .parse()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut executor = Executor::new(100);
    executor
        .execute(&program)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(axum::Json(serde_json::json!({
        "status": "ok",
        "engine": "fermi-fpl",
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
