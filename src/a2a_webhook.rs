//! # A2A webhook delivery
//!
//! HTTP POST delivery of A2A `StreamResponse` payloads to caller-registered
//! webhook URLs after task completion.
//!
//! Delivery is best-effort (fire-and-forget via `tokio::spawn`). Failures are
//! logged and recorded in `a2a_push_configs.last_error`. The platform does not
//! retry in Phase 4; retries are Phase 5.
//!
//! Design: docs/DESIGN_a2a_provider.md §9 Phase 4.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// A registered webhook configuration row (subset of `a2a_push_configs`).
#[derive(Debug, Clone)]
pub struct PushConfig {
    pub config_id: Uuid,
    pub webhook_url: String,
    pub auth_scheme: Option<String>,
    pub auth_credentials: Option<String>,
}

/// Deliver a `StreamResponse` payload to one webhook URL.
///
/// Returns `Ok(())` on 2xx. Any other status or network error is an `Err`.
pub async fn deliver(config: &PushConfig, payload: &Value) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP client build failed: {}", e))?;

    let mut req = client
        .post(&config.webhook_url)
        .header("Content-Type", "application/a2a+json")
        .json(payload);

    // Attach auth header if declared.
    if let (Some(scheme), Some(creds)) = (&config.auth_scheme, &config.auth_credentials) {
        req = req.header("Authorization", format!("{} {}", scheme, creds));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("HTTP send failed: {}", e))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("Webhook returned HTTP {}", resp.status()))
    }
}

/// Load all push configs registered for a given task_id.
pub async fn configs_for_task(pool: &PgPool, task_id: Uuid) -> Vec<PushConfig> {
    sqlx::query(
        "SELECT config_id, webhook_url, auth_scheme, auth_credentials
         FROM a2a_push_configs
         WHERE task_id = $1 AND delivered_at IS NULL",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|row| {
        use sqlx::Row;
        Some(PushConfig {
            config_id: row.try_get("config_id").ok()?,
            webhook_url: row.try_get("webhook_url").ok()?,
            auth_scheme: row.try_get("auth_scheme").ok()?,
            auth_credentials: row.try_get("auth_credentials").ok()?,
        })
    })
    .collect()
}

/// Mark a config as delivered and record any error.
pub async fn record_delivery(pool: &PgPool, config_id: Uuid, success: bool, error: Option<&str>) {
    let _ = sqlx::query(
        "UPDATE a2a_push_configs
         SET delivery_attempts = delivery_attempts + 1,
             delivered_at      = CASE WHEN $2 THEN NOW() ELSE delivered_at END,
             last_error        = $3
         WHERE config_id = $1",
    )
    .bind(config_id)
    .bind(success)
    .bind(error)
    .execute(pool)
    .await;
}

/// Fire all registered webhooks for a completed task.
///
/// Called from `send_message_handler` after `episode_boundary::close`.
/// Spawns one background task per config; never blocks the handler.
pub fn fire_for_task(pool: PgPool, task_id: Uuid, payload: Value) {
    tokio::spawn(async move {
        let configs = configs_for_task(&pool, task_id).await;
        for cfg in configs {
            let cfg_id = cfg.config_id;
            match deliver(&cfg, &payload).await {
                Ok(()) => {
                    tracing::info!(
                        task_id = %task_id,
                        config_id = %cfg_id,
                        url = %cfg.webhook_url,
                        "a2a webhook delivered"
                    );
                    record_delivery(&pool, cfg_id, true, None).await;
                }
                Err(e) => {
                    tracing::warn!(
                        task_id = %task_id,
                        config_id = %cfg_id,
                        url = %cfg.webhook_url,
                        error = %e,
                        "a2a webhook delivery failed"
                    );
                    record_delivery(&pool, cfg_id, false, Some(&e)).await;
                }
            }
        }
    });
}

/// Insert a new push config row and return its config_id.
pub async fn register(
    pool: &PgPool,
    task_id: Uuid,
    agent_slug: &str,
    caller_user_id: &str,
    webhook_url: &str,
    auth_scheme: Option<&str>,
    auth_credentials: Option<&str>,
    token: Option<&str>,
) -> Result<Uuid, String> {
    let config_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO a2a_push_configs
             (config_id, task_id, agent_slug, caller_user_id, webhook_url,
              auth_scheme, auth_credentials, token)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(config_id)
    .bind(task_id)
    .bind(agent_slug)
    .bind(caller_user_id)
    .bind(webhook_url)
    .bind(auth_scheme)
    .bind(auth_credentials)
    .bind(token)
    .execute(pool)
    .await
    .map_err(|e| format!("DB insert failed: {}", e))?;
    Ok(config_id)
}
