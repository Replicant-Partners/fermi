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

/// GET /api/rabble/join/:qr_token — resolve QR token to rabble details (public).
pub async fn resolve_qr_token_handler(
    State(state): State<AppState>,
    Path(qr_token): Path<String>,
) -> Result<axum::Json<serde_json::Value>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT swarm_id, name, description, location_name, latitude, longitude,
                starts_at, ends_at, status, funding_mode, suggested_contribution,
                invite_pool_remaining, total_contributions
         FROM swarm_events WHERE qr_token = $1",
    )
    .bind(&qr_token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Invalid QR code".into()))?;

    // Count current creatures in this rabble
    let swarm_id: uuid::Uuid = row.try_get("swarm_id").unwrap_or_default();
    let creature_count: i64 = sqlx::query(
        "SELECT COUNT(*) as cnt FROM creature_flights WHERE swarm_id = $1 AND ended_at IS NULL",
    )
    .bind(swarm_id)
    .fetch_one(&state.db)
    .await
    .map(|r| r.try_get("cnt").unwrap_or(0))
    .unwrap_or(0);

    Ok(axum::Json(serde_json::json!({
        "swarm_id": swarm_id,
        "name": row.try_get::<Option<String>, _>("name").ok().flatten(),
        "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
        "location_name": row.try_get::<Option<String>, _>("location_name").ok().flatten(),
        "latitude": row.try_get::<Option<f64>, _>("latitude").ok().flatten(),
        "longitude": row.try_get::<Option<f64>, _>("longitude").ok().flatten(),
        "starts_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("starts_at").ok().flatten().map(|t| t.to_rfc3339()),
        "ends_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("ends_at").ok().flatten().map(|t| t.to_rfc3339()),
        "status": row.try_get::<String, _>("status").ok(),
        "funding_mode": row.try_get::<String, _>("funding_mode").ok(),
        "suggested_contribution": row.try_get::<i32, _>("suggested_contribution").ok(),
        "invite_pool_remaining": row.try_get::<i32, _>("invite_pool_remaining").ok(),
        "total_contributions": row.try_get::<i32, _>("total_contributions").ok(),
        "creature_count": creature_count,
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
