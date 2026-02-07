use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::env;
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::{AuthProvider, User, UserRole};

#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct UserRecord {
    id: Uuid,
    user_id: Option<String>,
    email: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    role: String,
    zitadel_org_id: Option<String>,
    auth_provider: Option<String>,
    github_username: Option<String>,
    google_id: Option<String>,
    ethereum_address: Option<String>,
    ens_name: Option<String>,
}

/// OAuth provider configuration
#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OidcConfig {
    /// Load OIDC config from environment variables
    pub fn from_env() -> Result<Self, AuthError> {
        Ok(Self {
            issuer: env::var("ZITADEL_ISSUER").map_err(|_| AuthError::ConfigError)?,
            client_id: env::var("ZITADEL_CLIENT_ID").map_err(|_| AuthError::ConfigError)?,
            client_secret: env::var("ZITADEL_CLIENT_SECRET").map_err(|_| AuthError::ConfigError)?,
            redirect_uri: env::var("ZITADEL_REDIRECT_URI").map_err(|_| AuthError::ConfigError)?,
        })
    }
}

/// OAuth authorization request parameters
#[derive(Debug, Serialize)]
pub struct AuthorizationRequest {
    pub response_type: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub scope: String,
    pub state: String,
    pub nonce: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_hint: Option<String>,
}

/// OAuth callback query parameters
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

/// Token exchange request
#[derive(Debug, Serialize)]
pub struct TokenRequest {
    pub grant_type: String,
    pub code: String,
    pub redirect_uri: String,
    pub client_id: String,
    pub client_secret: String,
}

/// Token response from Zitadel
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub id_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

/// Generate authorization URL for OAuth flow
pub fn build_authorization_url(
    config: &OidcConfig,
    provider: AuthProvider,
    state: String,
    nonce: String,
) -> Result<String, AuthError> {
    let auth_endpoint = format!("{}/oauth/v2/authorize", config.issuer);

    let mut params = AuthorizationRequest {
        response_type: "code".to_string(),
        client_id: config.client_id.clone(),
        redirect_uri: config.redirect_uri.clone(),
        scope: "openid email profile".to_string(),
        state,
        nonce,
        prompt: None,
        login_hint: None,
    };

    // Add provider-specific hints
    match provider {
        AuthProvider::GitHub => {
            params.login_hint = Some("github".to_string());
        }
        AuthProvider::Google => {
            params.login_hint = Some("google".to_string());
        }
        AuthProvider::Email => {
            // Default Zitadel login
        }
        AuthProvider::Ethereum => {
            return Err(AuthError::ConfigError); // SIWE uses different flow
        }
    }

    // Build query string
    let query = serde_urlencoded::to_string(&params).map_err(|_| AuthError::ConfigError)?;

    Ok(format!("{}?{}", auth_endpoint, query))
}

/// Exchange authorization code for tokens
pub async fn exchange_code_for_token(
    config: &OidcConfig,
    code: String,
) -> Result<TokenResponse, AuthError> {
    let token_endpoint = format!("{}/oauth/v2/token", config.issuer);

    let token_request = TokenRequest {
        grant_type: "authorization_code".to_string(),
        code,
        redirect_uri: config.redirect_uri.clone(),
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
    };

    let client = reqwest::Client::new();
    let response = client
        .post(&token_endpoint)
        .form(&token_request)
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AuthError::OAuthError(format!(
            "Token exchange failed: {}",
            error_text
        )));
    }

    response
        .json::<TokenResponse>()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))
}

/// Fetch user info from Zitadel userinfo endpoint
#[derive(Debug, Deserialize)]
pub struct UserInfoResponse {
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub picture: Option<String>,
    #[serde(rename = "urn:zitadel:iam:org:id")]
    pub org_id: Option<String>,
}

pub async fn fetch_user_info(
    config: &OidcConfig,
    access_token: &str,
) -> Result<UserInfoResponse, AuthError> {
    let userinfo_endpoint = format!("{}/oidc/v1/userinfo", config.issuer);

    let client = reqwest::Client::new();
    let response = client
        .get(&userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AuthError::OAuthError(
            "Failed to fetch user info".to_string(),
        ));
    }

    response
        .json::<UserInfoResponse>()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))
}

/// Sync or create user in our database from Zitadel user info
pub async fn sync_user(
    pool: &PgPool,
    user_info: UserInfoResponse,
    provider: AuthProvider,
) -> Result<User, AuthError> {
    let email = user_info
        .email
        .ok_or(AuthError::OAuthError("No email provided".to_string()))?;

    // Try to find existing user by user_id or email
    let existing = sqlx::query_as::<_, UserRecord>(
        r#"
        SELECT id, user_id, email, display_name, avatar_url, role, zitadel_org_id, auth_provider,
               github_username, google_id, ethereum_address, ens_name
        FROM users
        WHERE user_id = $1 OR email = $2
        LIMIT 1
        "#,
    )
    .bind(&user_info.sub)
    .bind(&email)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    let record = if let Some(existing_user) = existing {
        // Update existing user
        sqlx::query_as::<_, UserRecord>(
            r#"
            UPDATE users
            SET user_id = $1,
                email = $2,
                display_name = $3,
                avatar_url = $4,
                auth_provider = $5,
                zitadel_org_id = $6,
                last_login_at = NOW(),
                updated_at = NOW()
            WHERE id = $7
            RETURNING id, user_id, email, display_name, avatar_url, role, zitadel_org_id, auth_provider,
                      github_username, google_id, ethereum_address, ens_name
            "#
        )
        .bind(&user_info.sub)
        .bind(&email)
        .bind(user_info.name.as_ref().or(user_info.preferred_username.as_ref()))
        .bind(&user_info.picture)
        .bind(format!("{:?}", provider).to_lowercase())
        .bind(&user_info.org_id)
        .bind(existing_user.id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    } else {
        // Create new user
        sqlx::query_as::<_, UserRecord>(
            r#"
            INSERT INTO users (user_id, email, display_name, avatar_url, auth_provider, zitadel_org_id, last_login_at, password_hash, password_salt)
            VALUES ($1, $2, $3, $4, $5, $6, NOW(), '', '')
            RETURNING id, user_id, email, display_name, avatar_url, role, zitadel_org_id, auth_provider,
                      github_username, google_id, ethereum_address, ens_name
            "#
        )
        .bind(&user_info.sub)
        .bind(&email)
        .bind(user_info.name.as_ref().or(user_info.preferred_username.as_ref()))
        .bind(&user_info.picture)
        .bind(format!("{:?}", provider).to_lowercase())
        .bind(&user_info.org_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    };

    // Parse role
    let role = match record.role.as_str() {
        "admin" => UserRole::Admin,
        "developer" => UserRole::Developer,
        "viewer" => UserRole::Viewer,
        _ => UserRole::Developer,
    };

    // Parse auth provider
    let auth_provider = match record.auth_provider.as_deref() {
        Some("github") => AuthProvider::GitHub,
        Some("google") => AuthProvider::Google,
        Some("ethereum") => AuthProvider::Ethereum,
        _ => AuthProvider::Email,
    };

    Ok(User {
        user_id: record.user_id.unwrap_or_else(|| record.id.to_string()),
        email: record.email,
        display_name: record.display_name,
        role,
        org_id: record.zitadel_org_id,
        auth_provider,
        github_username: record.github_username,
        google_id: record.google_id,
        ethereum_address: record.ethereum_address,
        ens_name: record.ens_name,
    })
}

/// Generate a secure random state parameter for CSRF protection
pub fn generate_state() -> String {
    use rand_core::{OsRng, RngCore};
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_state() {
        let state1 = generate_state();
        let state2 = generate_state();

        assert_eq!(state1.len(), 64);
        assert_eq!(state2.len(), 64);
        assert_ne!(state1, state2);
    }

    #[test]
    fn test_authorization_url_github() {
        let config = OidcConfig {
            issuer: "https://test.zitadel.cloud".to_string(),
            client_id: "test-client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "https://fermi.systems/auth/callback".to_string(),
        };

        let url = build_authorization_url(
            &config,
            AuthProvider::GitHub,
            "test-state".to_string(),
            "test-nonce".to_string(),
        )
        .unwrap();

        assert!(url.contains("oauth/v2/authorize"));
        assert!(url.contains("client_id=test-client"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("login_hint=github"));
    }
}
