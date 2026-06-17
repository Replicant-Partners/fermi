use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::RngCore;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    error::AuthError,
    types::{ApiKey, AuthPrincipal},
};

const KEY_PREFIX: &str = "ferm_";

/// Generate a new API key. Returns (plaintext_key, ApiKey metadata).
/// The plaintext key is only available at creation time — we store only the hash.
pub async fn create_api_key(
    pool: &PgPool,
    user_id: &str,
    name: &str,
    scopes: &[String],
) -> Result<(String, ApiKey), AuthError> {
    // Generate random key bytes
    let mut key_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut key_bytes);
    let raw_key = hex::encode(key_bytes);
    let plaintext_key = format!("{}{}", KEY_PREFIX, raw_key);
    let prefix = &plaintext_key[..12];

    // Hash the key with Argon2
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let key_hash = argon2
        .hash_password(plaintext_key.as_bytes(), &salt)
        .map_err(|e| AuthError::DatabaseError(format!("Failed to hash API key: {}", e)))?
        .to_string();

    let key_id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO api_keys (key_id, user_id, key_hash, key_prefix, name, scopes)
        VALUES ($1, (SELECT id FROM users WHERE user_id = $2 LIMIT 1), $3, $4, $5, $6)
        "#,
    )
    .bind(key_id)
    .bind(user_id)
    .bind(&key_hash)
    .bind(prefix)
    .bind(name)
    .bind(scopes)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok((
        plaintext_key,
        ApiKey {
            key_id,
            user_id: user_id.to_string(),
            name: name.to_string(),
            scopes: scopes.to_vec(),
        },
    ))
}

/// Validate an API key and return an AuthPrincipal.
/// Scans active keys by prefix, then verifies the full hash.
pub async fn validate_api_key(pool: &PgPool, key: &str) -> Result<AuthPrincipal, AuthError> {
    if key.len() < 12 {
        return Err(AuthError::InvalidToken);
    }

    let prefix = &key[..12];

    // Find candidate keys by prefix.
    //
    // We also pull the owning user's `role` here so that an API key issued
    // by a platform admin inherits admin authority on this request without
    // requiring the operator to manually attach an "admin" scope on every
    // key. The console's API-key UI doesn't expose scopes today, so without
    // this inheritance the only way for an admin to use the JSON API
    // surface for admin operations is to hand-edit the database. That
    // would defeat the "every operation goes through a handler" principle.
    //
    // The inheritance is conservative — we only ADD scopes derived from
    // the user's role; we don't remove or override explicitly-granted
    // scopes. So a key with explicit "write" scope still has "write"
    // regardless of the user's role, and a key with no scopes still
    // gets "admin"/"write" only because the user has those rights.
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, Vec<String>, Option<String>)>(
        r#"
        SELECT ak.key_id, ak.key_hash, u.user_id, ak.name, ak.scopes, u.role
        FROM api_keys ak
        JOIN users u ON ak.user_id = u.id
        WHERE ak.key_prefix = $1
          AND ak.is_active = TRUE
          AND (ak.expires_at IS NULL OR ak.expires_at > NOW())
        "#,
    )
    .bind(prefix)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    // Verify hash against each candidate
    let argon2 = Argon2::default();
    for (key_id, key_hash, user_id, name, scopes, user_role) in &rows {
        let parsed_hash = PasswordHash::new(key_hash).map_err(|_| AuthError::InvalidToken)?;
        if argon2.verify_password(key.as_bytes(), &parsed_hash).is_ok() {
            // Update last_used_at and request_count
            let _ = sqlx::query(
                r#"
                UPDATE api_keys
                SET last_used_at = NOW(), request_count = request_count + 1
                WHERE key_id = $1
                "#,
            )
            .bind(key_id)
            .execute(pool)
            .await;

            // Merge user-role-derived scopes into the key's scopes.
            // 'admin' role → grants "admin" + "write"
            // 'developer' role → grants "write"
            // 'viewer' role / NULL → no derived scopes
            let mut effective_scopes = scopes.clone();
            match user_role.as_deref() {
                Some("admin") => {
                    if !effective_scopes.iter().any(|s| s == "admin") {
                        effective_scopes.push("admin".to_string());
                    }
                    if !effective_scopes.iter().any(|s| s == "write") {
                        effective_scopes.push("write".to_string());
                    }
                }
                Some("developer") => {
                    if !effective_scopes.iter().any(|s| s == "write") {
                        effective_scopes.push("write".to_string());
                    }
                }
                _ => {}
            }

            return Ok(AuthPrincipal::ApiKey(ApiKey {
                key_id: *key_id,
                user_id: user_id.clone(),
                name: name.clone(),
                scopes: effective_scopes,
            }));
        }
    }

    Err(AuthError::InvalidToken)
}

/// List API keys for a user (metadata only, no hashes)
pub async fn list_api_keys(pool: &PgPool, user_id: &str) -> Result<Vec<ApiKeyInfo>, AuthError> {
    let rows = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT ak.key_id, ak.key_prefix, ak.name, ak.scopes,
               ak.last_used_at, ak.request_count, ak.expires_at,
               ak.is_active, ak.created_at
        FROM api_keys ak
        JOIN users u ON ak.user_id = u.id
        WHERE u.user_id = $1
        ORDER BY ak.created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(rows.into_iter().map(|r| r.into()).collect())
}

/// Revoke an API key
pub async fn revoke_api_key(pool: &PgPool, user_id: &str, key_id: Uuid) -> Result<(), AuthError> {
    let result = sqlx::query(
        r#"
        UPDATE api_keys
        SET is_active = FALSE, updated_at = NOW()
        WHERE key_id = $1
          AND user_id = (SELECT id FROM users WHERE user_id = $2 LIMIT 1)
        "#,
    )
    .bind(key_id)
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if result.rows_affected() == 0 {
        return Err(AuthError::UserNotFound); // key not found or not owned by user
    }

    Ok(())
}

/// Public API key info (no hash)
#[derive(Debug, serde::Serialize)]
pub struct ApiKeyInfo {
    pub key_id: Uuid,
    pub key_prefix: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub request_count: i64,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ApiKeyRow {
    key_id: Uuid,
    key_prefix: String,
    name: String,
    scopes: Vec<String>,
    last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    request_count: i64,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    is_active: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<ApiKeyRow> for ApiKeyInfo {
    fn from(r: ApiKeyRow) -> Self {
        Self {
            key_id: r.key_id,
            key_prefix: r.key_prefix,
            name: r.name,
            scopes: r.scopes,
            last_used_at: r.last_used_at,
            request_count: r.request_count,
            expires_at: r.expires_at,
            is_active: r.is_active,
            created_at: r.created_at,
        }
    }
}
