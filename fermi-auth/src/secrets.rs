use crate::AuthError;
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use chrono::{DateTime, Utc};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

/// AES-256-GCM encryptor for user secrets
pub struct SecretEncryptor {
    cipher: Aes256Gcm,
}

impl SecretEncryptor {
    /// Create from SECRETS_ENCRYPTION_KEY env var (hex-encoded 32-byte key)
    pub fn from_env() -> Result<Self, AuthError> {
        let key_hex =
            std::env::var("SECRETS_ENCRYPTION_KEY").map_err(|_| AuthError::SecretsNotConfigured)?;
        let key_bytes =
            hex::decode(&key_hex).map_err(|e| AuthError::EncryptionError(e.to_string()))?;
        if key_bytes.len() != 32 {
            return Err(AuthError::EncryptionError(
                "SECRETS_ENCRYPTION_KEY must be 32 bytes (64 hex chars)".to_string(),
            ));
        }
        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| AuthError::EncryptionError(e.to_string()))?;
        Ok(Self { cipher })
    }

    /// Encrypt plaintext, returns (ciphertext, nonce)
    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), AuthError> {
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| AuthError::EncryptionError(e.to_string()))?;
        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    /// Decrypt ciphertext with nonce
    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, AuthError> {
        let nonce = Nonce::from_slice(nonce);
        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| AuthError::EncryptionError(e.to_string()))?;
        String::from_utf8(plaintext).map_err(|e| AuthError::EncryptionError(e.to_string()))
    }
}

/// Secret metadata (never includes the decrypted value)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretInfo {
    pub secret_id: Uuid,
    pub secret_name: String,
    pub scope: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Secret access audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretAccessEntry {
    pub log_id: Uuid,
    pub secret_name: String,
    pub agent_name: String,
    pub workspace_id: Option<Uuid>,
    pub action: String,
    pub tool_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Store (or update) an encrypted secret
pub async fn store_secret(
    pool: &PgPool,
    encryptor: &SecretEncryptor,
    user_id: &str,
    secret_name: &str,
    plaintext_value: &str,
    scope: &str,
    label: Option<&str>,
    description: Option<&str>,
) -> Result<Uuid, AuthError> {
    let (ciphertext, nonce) = encryptor.encrypt(plaintext_value)?;

    let row = sqlx::query(
        r#"
        INSERT INTO user_secrets (user_id, secret_name, encrypted_value, nonce, scope, label, description)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (user_id, secret_name)
        DO UPDATE SET encrypted_value = $3, nonce = $4, scope = $5, label = $6, description = $7, updated_at = NOW()
        RETURNING secret_id
        "#,
    )
    .bind(user_id)
    .bind(secret_name)
    .bind(&ciphertext)
    .bind(&nonce)
    .bind(scope)
    .bind(label)
    .bind(description)
    .fetch_one(pool)
    .await?;

    // Log the create/update
    log_secret_access(pool, user_id, secret_name, "system", None, "created", None).await?;

    Ok(row
        .try_get::<Uuid, _>("secret_id")
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?)
}

/// Get a decrypted secret value (and log the access)
pub async fn get_secret(
    pool: &PgPool,
    encryptor: &SecretEncryptor,
    user_id: &str,
    secret_name: &str,
) -> Result<String, AuthError> {
    let row = sqlx::query(
        "SELECT encrypted_value, nonce FROM user_secrets WHERE user_id = $1 AND secret_name = $2",
    )
    .bind(user_id)
    .bind(secret_name)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => {
            let ciphertext: Vec<u8> = row
                .try_get("encrypted_value")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
            let nonce: Vec<u8> = row
                .try_get("nonce")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
            encryptor.decrypt(&ciphertext, &nonce)
        }
        None => Err(AuthError::SecretNotFound(secret_name.to_string())),
    }
}

/// List secret metadata for a user (never returns values)
pub async fn list_secrets(pool: &PgPool, user_id: &str) -> Result<Vec<SecretInfo>, AuthError> {
    let rows = sqlx::query(
        "SELECT secret_id, secret_name, scope, label, description, created_at, updated_at
         FROM user_secrets WHERE user_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut secrets = Vec::new();
    for row in rows {
        secrets.push(SecretInfo {
            secret_id: row
                .try_get("secret_id")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            secret_name: row
                .try_get("secret_name")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            scope: row
                .try_get("scope")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            label: row
                .try_get("label")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            description: row
                .try_get("description")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
        });
    }
    Ok(secrets)
}

/// Delete a secret
pub async fn delete_secret(
    pool: &PgPool,
    user_id: &str,
    secret_name: &str,
) -> Result<(), AuthError> {
    let result = sqlx::query("DELETE FROM user_secrets WHERE user_id = $1 AND secret_name = $2")
        .bind(user_id)
        .bind(secret_name)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::SecretNotFound(secret_name.to_string()));
    }

    log_secret_access(pool, user_id, secret_name, "system", None, "deleted", None).await?;
    Ok(())
}

/// Get all decrypted secrets available to a specific agent (scope match)
/// Returns a map of secret_name -> plaintext_value
/// Logs each access to the audit trail
pub async fn get_secrets_for_agent(
    pool: &PgPool,
    encryptor: &SecretEncryptor,
    user_id: &str,
    agent_name: &str,
) -> Result<HashMap<String, String>, AuthError> {
    let rows = sqlx::query(
        "SELECT secret_name, encrypted_value, nonce FROM user_secrets
         WHERE user_id = $1 AND (scope = $2 OR scope = '*')",
    )
    .bind(user_id)
    .bind(agent_name)
    .fetch_all(pool)
    .await?;

    let mut secrets = HashMap::new();
    for row in &rows {
        let name: String = row
            .try_get("secret_name")
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
        let ciphertext: Vec<u8> = row
            .try_get("encrypted_value")
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
        let nonce: Vec<u8> = row
            .try_get("nonce")
            .map_err(|e| AuthError::DatabaseError(e.to_string()))?;
        let plaintext = encryptor.decrypt(&ciphertext, &nonce)?;
        secrets.insert(name.clone(), plaintext);

        // Log the read
        let _ = log_secret_access(pool, user_id, &name, agent_name, None, "read", None).await;
    }
    Ok(secrets)
}

/// Append to secret access audit log
pub async fn log_secret_access(
    pool: &PgPool,
    user_id: &str,
    secret_name: &str,
    agent_name: &str,
    workspace_id: Option<Uuid>,
    action: &str,
    tool_name: Option<&str>,
) -> Result<(), AuthError> {
    sqlx::query(
        "INSERT INTO secret_access_log (user_id, secret_name, agent_name, workspace_id, action, tool_name)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(user_id)
    .bind(secret_name)
    .bind(agent_name)
    .bind(workspace_id)
    .bind(action)
    .bind(tool_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get audit log entries for a user
pub async fn get_secret_audit_log(
    pool: &PgPool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<SecretAccessEntry>, AuthError> {
    let rows = sqlx::query(
        "SELECT log_id, secret_name, agent_name, workspace_id, action, tool_name, created_at
         FROM secret_access_log WHERE user_id = $1
         ORDER BY created_at DESC LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(SecretAccessEntry {
            log_id: row
                .try_get("log_id")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            secret_name: row
                .try_get("secret_name")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            agent_name: row
                .try_get("agent_name")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            workspace_id: row
                .try_get("workspace_id")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            action: row
                .try_get("action")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            tool_name: row
                .try_get("tool_name")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| AuthError::DatabaseError(e.to_string()))?,
        });
    }
    Ok(entries)
}
