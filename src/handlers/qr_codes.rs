//! QR code generation for Rabble events.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
};
use image::Luma;
use qrcode::QrCode;
use sqlx::Row;

use crate::AppState;

/// GET /api/rabble/:id/qr — generate QR code PNG for a rabble's join URL.
pub async fn rabble_qr_handler(
    State(state): State<AppState>,
    Path(swarm_id): Path<uuid::Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Look up swarm and get/generate qr_token
    let row = sqlx::query("SELECT qr_token FROM swarm_events WHERE swarm_id = $1")
        .bind(swarm_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Rabble not found".into()))?;

    let qr_token: Option<String> = row.try_get("qr_token").ok().flatten();

    let token = if let Some(t) = qr_token {
        t
    } else {
        // Generate and save a token
        let token = generate_qr_token();
        sqlx::query("UPDATE swarm_events SET qr_token = $1 WHERE swarm_id = $2")
            .bind(&token)
            .bind(swarm_id)
            .execute(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        token
    };

    let join_url = format!("https://rabble.world/join/{}", token);

    // Generate QR code
    let code = QrCode::new(join_url.as_bytes()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("QR generation failed: {}", e),
        )
    })?;

    let img = code.render::<Luma<u8>>().min_dimensions(256, 256).build();

    // Encode as PNG
    let mut png_bytes: Vec<u8> = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::L8,
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("PNG encoding failed: {}", e),
        )
    })?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        png_bytes,
    ))
}

/// GET /api/rabble/join/:qr_token — resolve QR token to rabble details.
/// For private rabbles, returns limited info (name + requires_invite flag).
/// For shared/public, returns full details.
pub async fn resolve_qr_token_handler(
    State(state): State<AppState>,
    caller: Option<axum::extract::Extension<fermi_auth::AuthPrincipal>>,
    Path(qr_token): Path<String>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT swarm_id, name, description, location_name, creator_id, visibility,
                starts_at, ends_at, status, funding_mode, suggested_contribution,
                invite_pool_remaining, total_contributions
         FROM swarm_events WHERE qr_token = $1",
    )
    .bind(&qr_token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Invalid QR code".into()))?;

    let swarm_id: uuid::Uuid = row.try_get("swarm_id").unwrap_or_default();
    let visibility: String = row
        .try_get("visibility")
        .unwrap_or_else(|_| "public".into());
    let creator_id: String = row.try_get("creator_id").unwrap_or_default();
    let caller_id = caller.map(|c| c.0.user_id());

    // Private rabbles: only show limited info unless caller is creator or invited
    if visibility == "private" {
        let is_authorized = caller_id.as_ref().map_or(false, |uid| uid == &creator_id);

        // Check object_shares if not creator
        let is_invited = if !is_authorized {
            if let Some(ref uid) = caller_id {
                sqlx::query(
                    "SELECT 1 FROM object_shares
                     WHERE object_type = 'rabble' AND object_id = $1::text
                     AND (share_target = $2 OR share_target IN
                          (SELECT team_id::text FROM team_members WHERE member_id = $2))
                     LIMIT 1",
                )
                .bind(swarm_id)
                .bind(uid)
                .fetch_optional(&state.db)
                .await
                .map(|r| r.is_some())
                .unwrap_or(false)
            } else {
                false
            }
        } else {
            true
        };

        if !is_authorized && !is_invited {
            return Ok(axum::Json(serde_json::json!({
                "swarm_id": swarm_id,
                "name": row.try_get::<Option<String>, _>("name").ok().flatten(),
                "visibility": "private",
                "requires_invite": true,
                "status": row.try_get::<String, _>("status").ok(),
            })));
        }
    }

    // Count current creatures in this rabble — source of truth: creature_state
    let creature_count: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM creature_state
         WHERE rabble_id = $1 AND state IN ('hosting', 'in_rabble')",
    )
    .bind(swarm_id)
    .fetch_one(&state.db)
    .await
    .map(|r| r.try_get("cnt").unwrap_or(0))
    .unwrap_or(0);

    // Fetch creatures currently at this rabble (for portal projection).
    // Source of truth: creature_state.rabble_id (not creature_flights).
    let creature_rows = sqlx::query(
        "SELECT DISTINCT ON (c.creature_id)
                c.creature_id, c.specimen_name, c.common_name, c.scientific_name,
                c.species_group, c.asset_path, c.animation_status,
                c.owner_id,
                COALESCE(cs.location_lat, 0) AS center_lat,
                COALESCE(cs.location_lng, 0) AS center_lng,
                u.display_name AS owner_name
         FROM creature_state cs
         JOIN creatures c ON c.creature_id = cs.creature_id
         LEFT JOIN users u ON u.user_id = c.owner_id
         WHERE cs.rabble_id = $1
           AND cs.state IN ('hosting', 'in_rabble')
         ORDER BY c.creature_id
         LIMIT 50",
    )
    .bind(swarm_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let creatures: Vec<serde_json::Value> = creature_rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "creature_id": r.try_get::<uuid::Uuid, _>("creature_id").ok(),
                "specimen_name": r.try_get::<Option<String>, _>("specimen_name").ok().flatten(),
                "common_name": r.try_get::<Option<String>, _>("common_name").ok().flatten(),
                "scientific_name": r.try_get::<String, _>("scientific_name").ok(),
                "species_group": r.try_get::<String, _>("species_group").ok(),
                "asset_path": r.try_get::<String, _>("asset_path").ok(),
                "animation_status": r.try_get::<Option<String>, _>("animation_status").ok().flatten(),
                "owner_id": r.try_get::<String, _>("owner_id").ok(),
                "owner_name": r.try_get::<Option<String>, _>("owner_name").ok().flatten(),
                "lat": r.try_get::<f64, _>("center_lat").ok(),
                "lng": r.try_get::<f64, _>("center_lng").ok(),
            })
        })
        .collect();

    Ok(axum::Json(serde_json::json!({
        "swarm_id": swarm_id,
        "name": row.try_get::<Option<String>, _>("name").ok().flatten(),
        "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
        "location_name": row.try_get::<Option<String>, _>("location_name").ok().flatten(),
        "visibility": visibility,
        "starts_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("starts_at").ok().flatten().map(|t| t.to_rfc3339()),
        "ends_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ends_at").ok().flatten().map(|t| t.to_rfc3339()),
        "status": row.try_get::<String, _>("status").ok(),
        "funding_mode": row.try_get::<String, _>("funding_mode").ok(),
        "suggested_contribution": row.try_get::<i32, _>("suggested_contribution").ok(),
        "invite_pool_remaining": row.try_get::<i32, _>("invite_pool_remaining").ok(),
        "total_contributions": row.try_get::<i32, _>("total_contributions").ok(),
        "creature_count": creature_count,
        "creatures": creatures,
    })))
}

/// Generate a short alphanumeric QR token (8 chars).
fn generate_qr_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..8)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
