/**
 * Notebook API Handlers
 *
 * RESTful endpoints for Fermi Notebook CRUD and execution.
 * Integrates with FPL executor and agent system.
 */
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

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
    State(_state): State<AppState>,
    principal: AuthPrincipal,
    Path(_id): Path<Uuid>,
    Json(req): Json<ExecuteNotebookRequest>,
) -> Result<Json<ExecuteNotebookResponse>, (StatusCode, String)> {
    // TODO: Integrate with FPL executor (src/executor.rs)
    // For now, return mock response

    let _user_id = principal.user_id();
    let _iterations = req.iterations.unwrap_or(10000);

    let response = ExecuteNotebookResponse {
        cells: serde_json::json!({}),
        final_probability: Some(0.67),
        execution_state: ExecutionState {
            status: "success".to_string(),
            completed_cells: vec![],
            error_message: None,
        },
        total_time_ms: 1250,
    };

    Ok(Json(response))
}
