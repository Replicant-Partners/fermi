//! Creature collection handlers.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

// ─── Collections (authenticated) ───────────────────────────────────

/// GET /api/collections — user's creature collections
pub async fn list_collections_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> impl IntoResponse {
    let pool = state.memory_store.pool();
    match sqlx::query(
        "SELECT collection_id, owner_id, name, description, creature_ids, created_at, updated_at
         FROM creature_collections WHERE owner_id = $1
         ORDER BY updated_at DESC",
    )
    .bind(principal.user_id())
    .fetch_all(pool)
    .await
    {
        Ok(rows) => {
            let collections: Vec<serde_json::Value> = rows
                .iter()
                .map(|row| {
                    json!({
                        "collection_id": row.get::<Uuid, _>("collection_id"),
                        "name": row.get::<String, _>("name"),
                        "description": row.get::<Option<String>, _>("description"),
                        "creature_ids": row.get::<serde_json::Value, _>("creature_ids"),
                        "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
                        "updated_at": row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").to_rfc3339(),
                    })
                })
                .collect();
            (StatusCode::OK, Json(json!({ "collections": collections }))).into_response()
        }
        Err(e) => {
            eprintln!("Failed to list collections: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "Failed to list collections"})),
            )
                .into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
    pub creature_ids: Option<Vec<Uuid>>,
}

/// POST /api/collections — create a collection
pub async fn create_collection_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let collection_id = Uuid::new_v4();
    let creature_ids = req.creature_ids.unwrap_or_default();
    let now = chrono::Utc::now();

    let pool = state.memory_store.pool();
    sqlx::query(
        "INSERT INTO creature_collections (collection_id, owner_id, name, description, creature_ids, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $6)",
    )
    .bind(collection_id)
    .bind(&user_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(json!(creature_ids))
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "collection_id": collection_id,
        "name": req.name,
        "creature_count": creature_ids.len(),
    })))
}

#[derive(Deserialize)]
pub struct UpdateCollectionRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub creature_ids: Option<Vec<Uuid>>,
}

/// PUT /api/collections/:collection_id — update a collection
pub async fn update_collection_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(collection_id): Path<Uuid>,
    Json(req): Json<UpdateCollectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Verify ownership
    let existing =
        sqlx::query("SELECT owner_id FROM creature_collections WHERE collection_id = $1")
            .bind(collection_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    let owner: String = existing.get("owner_id");
    if owner != user_id {
        return Err((StatusCode::FORBIDDEN, "Not your collection".to_string()));
    }

    // Build dynamic update
    let mut sets = vec!["updated_at = NOW()".to_string()];
    let mut bind_idx = 1u32;
    let mut binds: Vec<String> = Vec::new();

    if let Some(ref name) = req.name {
        bind_idx += 1;
        sets.push(format!("name = ${}", bind_idx));
        binds.push(name.clone());
    }
    if let Some(ref desc) = req.description {
        bind_idx += 1;
        sets.push(format!("description = ${}", bind_idx));
        binds.push(desc.clone());
    }

    let creature_ids_json = req.creature_ids.as_ref().map(|ids| json!(ids));
    if creature_ids_json.is_some() {
        bind_idx += 1;
        sets.push(format!("creature_ids = ${}", bind_idx));
    }

    let sql = format!(
        "UPDATE creature_collections SET {} WHERE collection_id = $1 AND owner_id = ${}",
        sets.join(", "),
        bind_idx + 1,
    );

    let mut query = sqlx::query(&sql).bind(collection_id);
    for s in &binds {
        query = query.bind(s);
    }
    if let Some(ref ids_json) = creature_ids_json {
        query = query.bind(ids_json);
    }
    query = query.bind(&user_id);

    query
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "collection_id": collection_id,
        "updated": true,
    })))
}
