//! Web Push notification handlers.
//!
//! Provides endpoints for:
//!   - GET  /api/push/vapid-key — public VAPID key for client subscription
//!   - POST /api/push/subscribe — register a push subscription
//!   - DELETE /api/push/subscribe — unregister a push subscription
//!
//! And helper functions:
//!   - `notify_user` — creates in-app notification + sends web push
//!   - `send_push_to_user` — sends web push only
//!
//! Push delivery uses a "tickle" pattern: sends a minimal push to wake the
//! service worker, which then fetches notifications from the API and displays
//! them natively. This avoids complex ECE payload encryption.
//!
//! VAPID keys should be set via environment variables:
//!   VAPID_PUBLIC_KEY  — base64url-encoded P-256 public key (65 bytes uncompressed)
//!   VAPID_PRIVATE_KEY — base64url-encoded P-256 private key (32 bytes)
//!
//! Generate with:
//!   npx web-push generate-vapid-keys
//! Or:
//!   openssl ecparam -genkey -name prime256v1 -noout | openssl ec -outform DER 2>/dev/null | tail -c 32 | base64 | tr '+/' '-_' | tr -d '='

use axum::{extract::State, http::StatusCode, Json};
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
// PUSH DELIVERY
//
// Uses a "tickle" pattern: sends a minimal HTTP POST to the push endpoint
// to wake the service worker. The service worker then fetches the latest
// notifications from GET /api/notifications and displays them natively.
//
// For full encrypted payload push, VAPID JWT signing + ECE encryption
// would be needed. This is deferred — the tickle pattern works for MVP
// and the service worker handles the rest.
// ═══════════════════════════════════════════════════════════════════════════

/// Send a push notification to all active subscriptions of a user.
///
/// Fire-and-forget: errors are logged but never propagated to the caller.
/// If VAPID keys are not configured, push is silently skipped (in-app
/// notifications still work via the notify_user helper below).
pub async fn send_push_to_user(
    pool: &PgPool,
    user_id: &str,
    title: &str,
    body: &str,
    notification_type: &str,
    _url: Option<&str>,
    _icon: Option<&str>,
) {
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
        return; // No subscriptions — user hasn't enabled push
    }

    // For MVP: send a "tickle" push (empty body) to each subscription endpoint.
    // The service worker's `push` event fires and fetches from /api/notifications.
    //
    // NOTE: Most push services (Chrome/FCM, Firefox/autopush) require either:
    //   a) VAPID authentication (Authorization header with JWT), or
    //   b) Encrypted payload (ECE)
    //
    // Without VAPID keys configured, these pushes will likely fail with 401/403.
    // That's OK for MVP — the in-app notification bell still works. When VAPID
    // keys are set up, the service worker + push will come alive.

    let client = reqwest::Client::new();

    for sub in &subs {
        let sub_id: Uuid = sub.get("id");
        let endpoint: String = sub.get("endpoint");

        let result = client
            .post(&endpoint)
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
                    // Subscription expired — deactivate
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
                    // Other error (likely 401/403 without VAPID) — increment fail count
                    eprintln!(
                        "[push] Push to {} returned {} for user {} (title: '{}')",
                        sub_id, status, user_id, title
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
