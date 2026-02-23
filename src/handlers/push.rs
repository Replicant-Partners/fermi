//! Web Push notification handlers.
//!
//! Provides endpoints for:
//!   - GET  /api/push/vapid-key — public VAPID key for client subscription
//!   - POST /api/push/subscribe — register a push subscription
//!   - DELETE /api/push/subscribe — unregister a push subscription
//!   - POST /api/push/proximity — check proximity and push nearby rabble alerts
//!
//! And helper functions:
//!   - `notify_user` — creates in-app notification + sends web push
//!   - `send_push_to_user` — sends web push only
//!
//! Push delivery uses a "tickle" pattern with VAPID JWT authentication.
//! The server sends a minimal push to wake the service worker, which then
//! fetches notifications from the API and displays them natively.
//!
//! VAPID keys stored in push_config table. Generate with:
//!   npx web-push generate-vapid-keys

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

// ═══════════════════════════════════════════════════════════════════════════
// VAPID KEY — served to client for push subscription
// ═══════════════════════════════════════════════════════════════════════════

/// GET /api/push/vapid-key — return the public VAPID key for client subscription.
///
/// The client uses this to call `pushManager.subscribe({ applicationServerKey })`.
/// No auth required — the public key is public.
pub async fn get_vapid_key_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let pool = state.memory_store.pool();

    // Try database first
    let row = sqlx::query("SELECT vapid_public_key FROM push_config WHERE id = 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    if let Some(r) = row {
        let key: String = r.get("vapid_public_key");
        return Ok(Json(json!({ "public_key": key })));
    }

    // Fall back to environment variable
    match std::env::var("VAPID_PUBLIC_KEY") {
        Ok(key) if !key.is_empty() => Ok(Json(json!({ "public_key": key }))),
        _ => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Push notifications not configured. Set VAPID_PUBLIC_KEY env var.".into(),
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBSCRIPTION MANAGEMENT
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
    pub user_agent: Option<String>,
}

/// POST /api/push/subscribe — register a push subscription.
///
/// Called after the client successfully subscribes to push via the browser API.
/// Stores the subscription endpoint + keys for later push delivery.
pub async fn subscribe_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<SubscribeRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh_key, auth_key, user_agent)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (user_id, endpoint) DO UPDATE SET
             p256dh_key = $4, auth_key = $5, user_agent = $6,
             active = true, failed_count = 0, last_used_at = NOW()",
    )
    .bind(id)
    .bind(&user_id)
    .bind(&req.endpoint)
    .bind(&req.p256dh)
    .bind(&req.auth)
    .bind(&req.user_agent)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    eprintln!("[push] Subscription registered for user {}", user_id);

    Ok(Json(json!({
        "status": "subscribed",
        "subscription_id": id,
    })))
}

#[derive(Debug, Deserialize)]
pub struct UnsubscribeRequest {
    pub endpoint: String,
}

/// DELETE /api/push/subscribe — unregister a push subscription.
pub async fn unsubscribe_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<UnsubscribeRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();

    sqlx::query(
        "UPDATE push_subscriptions SET active = false
         WHERE user_id = $1 AND endpoint = $2",
    )
    .bind(&user_id)
    .bind(&req.endpoint)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "unsubscribed" })))
}

// ═══════════════════════════════════════════════════════════════════════════
// VAPID JWT SIGNING — ES256 (ECDSA P-256) per RFC 8292
// ═══════════════════════════════════════════════════════════════════════════

/// Build a VAPID Authorization header for a push endpoint.
///
/// Format: `vapid t=<JWT>, k=<public_key_base64url>`
///
/// The JWT is signed with ES256 using the private key from push_config.
fn build_vapid_auth(
    vapid_private_b64: &str,
    vapid_public_b64: &str,
    vapid_subject: &str,
    endpoint: &str,
) -> Result<String, String> {
    // Parse endpoint to get audience (origin)
    let audience = url::Url::parse(endpoint)
        .map(|u| format!("{}://{}", u.scheme(), u.host_str().unwrap_or("")))
        .map_err(|e| format!("Invalid endpoint URL: {}", e))?;

    // Build JWT header + payload
    let header = json!({"alg": "ES256", "typ": "JWT"});
    let now = chrono::Utc::now().timestamp() as u64;
    let payload = json!({
        "aud": audience,
        "exp": now + 12 * 3600,
        "sub": vapid_subject,
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string().as_bytes());
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    let signing_input = format!("{}.{}", header_b64, payload_b64);

    // Decode private key and sign
    let priv_bytes = URL_SAFE_NO_PAD
        .decode(vapid_private_b64)
        .map_err(|e| format!("Invalid private key: {}", e))?;

    let signing_key = SigningKey::from_bytes(priv_bytes.as_slice().into())
        .map_err(|e| format!("Invalid P-256 key: {}", e))?;

    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let sig_bytes = signature.to_bytes();
    let sig_b64 = URL_SAFE_NO_PAD.encode(&sig_bytes);

    let jwt = format!("{}.{}", signing_input, sig_b64);
    Ok(format!("vapid t={}, k={}", jwt, vapid_public_b64))
}

// ═══════════════════════════════════════════════════════════════════════════
// PUSH DELIVERY — tickle push with VAPID authentication
// ═══════════════════════════════════════════════════════════════════════════

/// Send a push notification to all active subscriptions of a user.
///
/// Uses VAPID JWT authentication (ES256). Sends a "tickle" push (empty body)
/// to wake the service worker, which fetches from /api/notifications.
///
/// Fire-and-forget: errors are logged but never propagated to the caller.
pub async fn send_push_to_user(
    pool: &PgPool,
    user_id: &str,
    title: &str,
    body: &str,
    notification_type: &str,
    _url: Option<&str>,
    _icon: Option<&str>,
) {
    // Get VAPID keys
    let vapid = match sqlx::query(
        "SELECT vapid_public_key, vapid_private_key, vapid_subject FROM push_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => row,
        _ => {
            // No VAPID keys — can't send push, but in-app notifications still work
            return;
        }
    };

    let vapid_public: String = vapid.get("vapid_public_key");
    let vapid_private: String = vapid.get("vapid_private_key");
    let vapid_subject: String = vapid
        .try_get("vapid_subject")
        .unwrap_or_else(|_| "mailto:hello@rabble.world".into());

    // Get all active subscriptions for this user
    let subs = match sqlx::query(
        "SELECT id, endpoint, p256dh_key, auth_key
         FROM push_subscriptions
         WHERE user_id = $1 AND active = true AND failed_count < 10",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            eprintln!(
                "[push] Failed to fetch subscriptions for {}: {}",
                user_id, e
            );
            return;
        }
    };

    if subs.is_empty() {
        return;
    }

    let client = reqwest::Client::new();

    for sub in &subs {
        let sub_id: Uuid = sub.get("id");
        let endpoint: String = sub.get("endpoint");

        // Build VAPID Authorization header for this endpoint
        let auth_header =
            match build_vapid_auth(&vapid_private, &vapid_public, &vapid_subject, &endpoint) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("[push] VAPID auth failed for sub {}: {}", sub_id, e);
                    continue;
                }
            };

        let result = client
            .post(&endpoint)
            .header("Authorization", &auth_header)
            .header("Content-Length", "0")
            .header("TTL", "86400")
            .header("Urgency", "normal")
            .header("Topic", notification_type)
            .send()
            .await;

        match result {
            Ok(response) => {
                let status = response.status();
                if status.is_success() || status.as_u16() == 201 {
                    let _ = sqlx::query(
                        "UPDATE push_subscriptions SET last_used_at = NOW(), failed_count = 0
                         WHERE id = $1",
                    )
                    .bind(sub_id)
                    .execute(pool)
                    .await;
                } else if status.as_u16() == 410 || status.as_u16() == 404 {
                    eprintln!(
                        "[push] Subscription {} gone ({}), deactivating",
                        sub_id, status
                    );
                    let _ =
                        sqlx::query("UPDATE push_subscriptions SET active = false WHERE id = $1")
                            .bind(sub_id)
                            .execute(pool)
                            .await;
                } else {
                    let resp_body = response.text().await.unwrap_or_default();
                    eprintln!(
                        "[push] Push to sub {} returned {} for user {} — {}",
                        sub_id, status, user_id, resp_body
                    );
                    let _ = sqlx::query(
                        "UPDATE push_subscriptions SET failed_count = failed_count + 1,
                         active = CASE WHEN failed_count >= 9 THEN false ELSE active END
                         WHERE id = $1",
                    )
                    .bind(sub_id)
                    .execute(pool)
                    .await;
                }
            }
            Err(e) => {
                eprintln!(
                    "[push] HTTP error for sub {} (user {}): {}",
                    sub_id, user_id, e
                );
                let _ = sqlx::query(
                    "UPDATE push_subscriptions SET failed_count = failed_count + 1 WHERE id = $1",
                )
                .bind(sub_id)
                .execute(pool)
                .await;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CONVENIENCE HELPER — the primary entry point for notification code
// ═══════════════════════════════════════════════════════════════════════════

/// Create an in-app notification AND send a web push in one call.
///
/// This is what most notification-creating code should call:
///   - Friendship requests/acceptances
///   - Rabble joins/leaves/ends
///   - Creature invites
///   - Follower events
///
/// The in-app notification is always created (stored in `notifications` table).
/// The web push is sent best-effort in a background task (fire-and-forget).
pub async fn notify_user(
    pool: &PgPool,
    user_id: &str,
    notification_type: &str,
    title: &str,
    message: Option<&str>,
    metadata: Option<&Value>,
    _push_url: Option<&str>,
) {
    let default_meta = json!({});
    let meta = metadata.unwrap_or(&default_meta);

    // 1. Insert in-app notification (always)
    if let Err(e) = sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, message, metadata, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(notification_type)
    .bind(title)
    .bind(message)
    .bind(meta)
    .execute(pool)
    .await
    {
        eprintln!(
            "[notify] Failed to create notification for {}: {}",
            user_id, e
        );
    }

    // 2. Send web push in background (fire-and-forget)
    let pool = pool.clone();
    let user_id = user_id.to_string();
    let title = title.to_string();
    let body = message.unwrap_or("").to_string();
    let ntype = notification_type.to_string();

    tokio::spawn(async move {
        send_push_to_user(&pool, &user_id, &title, &body, &ntype, None, None).await;
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// PROXIMITY PUSH — "Rabble nearby!" alerts
//
// Called when a user's location is updated (from tether telemetry or
// periodic client check-in). Finds active rabbles within a configurable
// radius and sends push notifications for ones the user hasn't been
// alerted about recently.
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct ProximityCheckRequest {
    pub lat: f64,
    pub lng: f64,
    pub radius_km: Option<f64>,
}

/// POST /api/push/proximity — check for nearby rabbles and send push alerts.
///
/// Called by the client periodically (or on significant location change).
/// Finds active public rabbles within radius_km (default 2km) that the user
/// hasn't been alerted about in the last 4 hours.
pub async fn proximity_check_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ProximityCheckRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = state.memory_store.pool();
    let radius_km = req.radius_km.unwrap_or(2.0).max(0.1).min(50.0);

    // Find active public rabbles within radius using PostGIS-style haversine.
    // Excludes rabbles the user created or already has creatures in.
    let nearby = sqlx::query(
        "SELECT * FROM (
            SELECT s.swarm_id, s.name, s.location_name, s.creature_count,
                    s.center_lat, s.center_lng, s.walk_in_price,
                    s.anchor_creature_id,
                    c.specimen_name AS anchor_creature_name,
                    (6371 * acos(LEAST(1.0, GREATEST(-1.0,
                        cos(radians($1)) * cos(radians(s.center_lat)) *
                        cos(radians(s.center_lng) - radians($2)) +
                        sin(radians($1)) * sin(radians(s.center_lat))
                    )))) AS distance_km
             FROM swarm_events s
             LEFT JOIN creatures c ON c.creature_id = s.anchor_creature_id
             WHERE s.status = 'active'
               AND s.visibility = 'public'
               AND s.creator_id != $3
               AND s.center_lat BETWEEN $1 - ($4 / 111.0) AND $1 + ($4 / 111.0)
               AND s.center_lng BETWEEN $2 - ($4 / (111.0 * GREATEST(cos(radians($1)), 0.01)))
                                    AND $2 + ($4 / (111.0 * GREATEST(cos(radians($1)), 0.01)))
               AND s.swarm_id NOT IN (
                   SELECT cs.rabble_id FROM creature_state cs
                   JOIN creatures cr ON cr.creature_id = cs.creature_id
                   WHERE cr.owner_id = $3 AND cs.rabble_id IS NOT NULL
               )
         ) nearby
         WHERE distance_km <= $4
         ORDER BY distance_km ASC
         LIMIT 10",
    )
    .bind(req.lat)
    .bind(req.lng)
    .bind(&user_id)
    .bind(radius_km)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut alerted = 0;

    for row in &nearby {
        let swarm_id: Uuid = row.get("swarm_id");
        let name: String = row.get("name");
        let distance: f64 = row.try_get("distance_km").unwrap_or(0.0);
        let creature_count: i32 = row.try_get("creature_count").unwrap_or(0);
        let location_name: Option<String> = row
            .try_get::<Option<String>, _>("location_name")
            .unwrap_or(None);

        // Check if we already alerted about this rabble recently (4h cooldown)
        let recently_alerted = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM notifications
                WHERE user_id = $1 AND type = 'rabble_nearby'
                AND metadata->>'swarm_id' = $2
                AND created_at > NOW() - INTERVAL '4 hours'
            )",
        )
        .bind(&user_id)
        .bind(swarm_id.to_string())
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if recently_alerted {
            continue;
        }

        // Send proximity notification
        let dist_str = if distance < 1.0 {
            format!("{}m away", (distance * 1000.0).round() as i32)
        } else {
            format!("{:.1}km away", distance)
        };

        let body_text = format!(
            "{} creature{} gathering{} — {}",
            creature_count,
            if creature_count == 1 { "" } else { "s" },
            if let Some(ref loc) = location_name {
                format!(" at {}", loc)
            } else {
                String::new()
            },
            dist_str,
        );

        notify_user(
            pool,
            &user_id,
            "rabble_nearby",
            &format!("🦋 {} is nearby!", name),
            Some(&body_text),
            Some(&json!({
                "swarm_id": swarm_id,
                "distance_km": distance,
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
            })),
            None,
        )
        .await;

        alerted += 1;
    }

    Ok(Json(json!({
        "nearby_count": nearby.len(),
        "alerted": alerted,
        "radius_km": radius_km,
    })))
}
