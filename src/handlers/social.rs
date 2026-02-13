//! Social contact handlers — asymmetric follow model.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::super::AppState;
use fermi_auth::AuthPrincipal;

/// GET /api/contacts — list my contacts with profile info
pub async fn list_contacts_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let rows = sqlx::query(
        "SELECT c.id, c.contact_id, c.nickname, c.created_at,
         u.display_name, u.avatar_url, u.bio
         FROM contacts c
         LEFT JOIN users u ON u.user_id = c.contact_id
         WHERE c.user_id = $1
         ORDER BY c.created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let contacts: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id": row.get::<Uuid, _>("id"),
                "contact_id": row.get::<String, _>("contact_id"),
                "nickname": row.get::<Option<String>, _>("nickname"),
                "display_name": row.get::<Option<String>, _>("display_name"),
                "avatar_url": row.get::<Option<String>, _>("avatar_url"),
                "bio": row.get::<Option<String>, _>("bio"),
                "created_at": row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "contacts": contacts })))
}

#[derive(Deserialize)]
pub struct AddContactRequest {
    pub contact_id: String,
    pub nickname: Option<String>,
}

/// POST /api/contacts — add a contact
pub async fn add_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<AddContactRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    if user_id == req.contact_id {
        return Err((StatusCode::BAD_REQUEST, "Cannot add yourself".to_string()));
    }

    // Check contact exists as a user
    let exists = sqlx::query("SELECT 1 FROM users WHERE user_id = $1")
        .bind(&req.contact_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if exists.is_none() {
        return Err((StatusCode::NOT_FOUND, "User not found".to_string()));
    }

    let id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO contacts (id, user_id, contact_id, nickname)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (user_id, contact_id) DO NOTHING",
    )
    .bind(id)
    .bind(&user_id)
    .bind(&req.contact_id)
    .bind(&req.nickname)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::CONFLICT, "Contact already added".to_string()));
    }

    Ok(Json(json!({
        "id": id,
        "contact_id": req.contact_id,
        "added": true,
    })))
}

/// DELETE /api/contacts/:id — remove a contact by row id or contact_id
pub async fn remove_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id_or_contact): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    // Support both row UUID and contact user_id for backwards compatibility
    let result = if let Ok(row_uuid) = id_or_contact.parse::<Uuid>() {
        sqlx::query("DELETE FROM contacts WHERE id = $1 AND user_id = $2")
            .bind(row_uuid)
            .bind(&user_id)
            .execute(pool)
            .await
    } else {
        sqlx::query("DELETE FROM contacts WHERE user_id = $1 AND contact_id = $2")
            .bind(&user_id)
            .bind(&id_or_contact)
            .execute(pool)
            .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Contact not found".to_string()));
    }

    Ok(Json(json!({ "removed": true })))
}

#[derive(Deserialize)]
pub struct UpdateContactRequest {
    pub nickname: Option<String>,
}

/// PUT /api/contacts/:id — update nickname by row id or contact_id
pub async fn update_contact_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(id_or_contact): Path<String>,
    Json(req): Json<UpdateContactRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let result = if let Ok(row_uuid) = id_or_contact.parse::<Uuid>() {
        sqlx::query("UPDATE contacts SET nickname = $1 WHERE id = $2 AND user_id = $3")
            .bind(&req.nickname)
            .bind(row_uuid)
            .bind(&user_id)
            .execute(pool)
            .await
    } else {
        sqlx::query("UPDATE contacts SET nickname = $1 WHERE user_id = $2 AND contact_id = $3")
            .bind(&req.nickname)
            .bind(&user_id)
            .bind(&id_or_contact)
            .execute(pool)
            .await
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Contact not found".to_string()));
    }

    Ok(Json(json!({ "updated": true })))
}
