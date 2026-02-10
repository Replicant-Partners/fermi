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
    auth_provider: Option<String>,
    github_username: Option<String>,
    google_id: Option<String>,
    ethereum_address: Option<String>,
    ens_name: Option<String>,
}

/// Google OAuth2 configuration
#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl GoogleOAuthConfig {
    pub fn from_env() -> Result<Self, AuthError> {
        Ok(Self {
            client_id: env::var("GOOGLE_CLIENT_ID").map_err(|_| AuthError::ConfigError)?,
            client_secret: env::var("GOOGLE_CLIENT_SECRET").map_err(|_| AuthError::ConfigError)?,
            redirect_uri: env::var("OAUTH_REDIRECT_URI").map_err(|_| AuthError::ConfigError)?,
        })
    }
}

/// GitHub OAuth2 configuration
#[derive(Debug, Clone)]
pub struct GitHubOAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl GitHubOAuthConfig {
    pub fn from_env() -> Result<Self, AuthError> {
        Ok(Self {
            client_id: env::var("GITHUB_CLIENT_ID").map_err(|_| AuthError::ConfigError)?,
            client_secret: env::var("GITHUB_CLIENT_SECRET").map_err(|_| AuthError::ConfigError)?,
            redirect_uri: env::var("OAUTH_REDIRECT_URI").map_err(|_| AuthError::ConfigError)?,
        })
    }
}

/// Multi-provider OAuth config
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub google: Option<GoogleOAuthConfig>,
    pub github: Option<GitHubOAuthConfig>,
}

impl OAuthConfig {
    /// Load all available OAuth configs from environment.
    /// Does not fail if a provider is not configured — just skips it.
    pub fn from_env() -> Self {
        Self {
            google: GoogleOAuthConfig::from_env().ok(),
            github: GitHubOAuthConfig::from_env().ok(),
        }
    }

    pub fn google(&self) -> Result<&GoogleOAuthConfig, AuthError> {
        self.google.as_ref().ok_or(AuthError::ConfigError)
    }

    pub fn github(&self) -> Result<&GitHubOAuthConfig, AuthError> {
        self.github.as_ref().ok_or(AuthError::ConfigError)
    }
}

/// OAuth callback query parameters
#[derive(Debug, Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

/// Google token response
#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub id_token: Option<String>,
    pub refresh_token: Option<String>,
}

/// GitHub token response
#[derive(Debug, Deserialize)]
pub struct GitHubTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub scope: Option<String>,
}

/// Unified user info from either provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfoResponse {
    pub provider_id: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub provider: AuthProvider,
    // Provider-specific fields
    pub github_username: Option<String>,
    pub github_id: Option<String>,
    pub google_id: Option<String>,
}

/// Google userinfo API response
#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    id: String,
    email: Option<String>,
    #[allow(dead_code)]
    verified_email: Option<bool>,
    name: Option<String>,
    picture: Option<String>,
}

/// GitHub user API response
#[derive(Debug, Deserialize)]
struct GitHubUserInfo {
    id: i64,
    login: String,
    name: Option<String>,
    email: Option<String>,
    avatar_url: Option<String>,
}

/// GitHub email API response (for getting primary email)
#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

// --- Google OAuth2 flow ---

/// Build Google OAuth2 authorization URL
pub fn build_google_auth_url(config: &GoogleOAuthConfig, state: &str) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", &config.client_id),
        ("redirect_uri", &config.redirect_uri),
        ("scope", "openid email profile"),
        ("state", state),
        ("access_type", "offline"),
        ("prompt", "consent"),
    ];
    let query = serde_urlencoded::to_string(params).unwrap_or_default();
    format!("https://accounts.google.com/o/oauth2/v2/auth?{}", query)
}

/// Exchange Google authorization code for tokens
pub async fn google_exchange_code(
    config: &GoogleOAuthConfig,
    code: &str,
) -> Result<GoogleTokenResponse, AuthError> {
    let client = reqwest::Client::new();
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &config.redirect_uri),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
        ])
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AuthError::OAuthError(format!(
            "Google token exchange failed: {}",
            error_text
        )));
    }

    response
        .json::<GoogleTokenResponse>()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))
}

/// Fetch user info from Google
pub async fn google_fetch_user_info(access_token: &str) -> Result<UserInfoResponse, AuthError> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(AuthError::OAuthError(
            "Failed to fetch Google user info".to_string(),
        ));
    }

    let info: GoogleUserInfo = response
        .json()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    Ok(UserInfoResponse {
        provider_id: info.id.clone(),
        email: info.email,
        name: info.name,
        avatar_url: info.picture,
        provider: AuthProvider::Google,
        github_username: None,
        github_id: None,
        google_id: Some(info.id),
    })
}

// --- GitHub OAuth2 flow ---

/// Build GitHub OAuth2 authorization URL
pub fn build_github_auth_url(config: &GitHubOAuthConfig, state: &str) -> String {
    let params = [
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("scope", "user:email read:user"),
        ("state", state),
    ];
    let query = serde_urlencoded::to_string(params).unwrap_or_default();
    format!("https://github.com/login/oauth/authorize?{}", query)
}

/// Exchange GitHub authorization code for tokens
pub async fn github_exchange_code(
    config: &GitHubOAuthConfig,
    code: &str,
) -> Result<GitHubTokenResponse, AuthError> {
    let client = reqwest::Client::new();
    let response = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("code", code),
            ("redirect_uri", config.redirect_uri.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(AuthError::OAuthError(format!(
            "GitHub token exchange failed: {}",
            error_text
        )));
    }

    response
        .json::<GitHubTokenResponse>()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))
}

/// Fetch user info from GitHub
pub async fn github_fetch_user_info(access_token: &str) -> Result<UserInfoResponse, AuthError> {
    let client = reqwest::Client::new();

    // Fetch user profile
    let user_response = client
        .get("https://api.github.com/user")
        .header("User-Agent", "fermi-auth")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    if !user_response.status().is_success() {
        return Err(AuthError::OAuthError(
            "Failed to fetch GitHub user info".to_string(),
        ));
    }

    let user: GitHubUserInfo = user_response
        .json()
        .await
        .map_err(|e| AuthError::OAuthError(e.to_string()))?;

    // If email is not public, fetch from /user/emails
    let email = if user.email.is_some() {
        user.email.clone()
    } else {
        let emails_response = client
            .get("https://api.github.com/user/emails")
            .header("User-Agent", "fermi-auth")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuthError(e.to_string()))?;

        if emails_response.status().is_success() {
            let emails: Vec<GitHubEmail> = emails_response.json().await.unwrap_or_default();
            emails
                .into_iter()
                .find(|e| e.primary && e.verified)
                .map(|e| e.email)
        } else {
            None
        }
    };

    let github_id = user.id.to_string();
    Ok(UserInfoResponse {
        provider_id: github_id.clone(),
        email,
        name: user.name.or(Some(user.login.clone())),
        avatar_url: user.avatar_url,
        provider: AuthProvider::GitHub,
        github_username: Some(user.login),
        github_id: Some(github_id),
        google_id: None,
    })
}

// --- User sync (shared across providers) ---

/// Sync or create user in our database from OAuth user info
pub async fn sync_user(pool: &PgPool, user_info: &UserInfoResponse) -> Result<User, AuthError> {
    let email = user_info
        .email
        .as_ref()
        .ok_or(AuthError::OAuthError("No email provided".to_string()))?;

    let provider_str = match user_info.provider {
        AuthProvider::Google => "google",
        AuthProvider::GitHub => "github",
        AuthProvider::Ethereum => "ethereum",
        AuthProvider::Email => "email",
    };

    // Try to find existing user by provider-specific ID or email
    let existing = match user_info.provider {
        AuthProvider::Google => {
            sqlx::query_as::<_, UserRecord>(
                r#"
                SELECT id, user_id, email, display_name, avatar_url, role, auth_provider,
                       github_username, google_id, ethereum_address, ens_name
                FROM users
                WHERE google_id = $1 OR email = $2
                LIMIT 1
                "#,
            )
            .bind(&user_info.google_id)
            .bind(email)
            .fetch_optional(pool)
            .await
        }
        AuthProvider::GitHub => {
            sqlx::query_as::<_, UserRecord>(
                r#"
                SELECT id, user_id, email, display_name, avatar_url, role, auth_provider,
                       github_username, google_id, ethereum_address, ens_name
                FROM users
                WHERE github_id = $1 OR email = $2
                LIMIT 1
                "#,
            )
            .bind(&user_info.github_id)
            .bind(email)
            .fetch_optional(pool)
            .await
        }
        _ => {
            sqlx::query_as::<_, UserRecord>(
                r#"
                SELECT id, user_id, email, display_name, avatar_url, role, auth_provider,
                       github_username, google_id, ethereum_address, ens_name
                FROM users
                WHERE email = $1
                LIMIT 1
                "#,
            )
            .bind(email)
            .fetch_optional(pool)
            .await
        }
    }
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    let record = if let Some(existing_user) = existing {
        // Update existing user with latest info from provider
        sqlx::query_as::<_, UserRecord>(
            r#"
            UPDATE users
            SET email = $1,
                display_name = COALESCE($2, display_name),
                avatar_url = COALESCE($3, avatar_url),
                auth_provider = $4,
                github_username = COALESCE($5, github_username),
                github_id = COALESCE($6, github_id),
                google_id = COALESCE($7, google_id),
                last_login_at = NOW(),
                updated_at = NOW()
            WHERE id = $8
            RETURNING id, user_id, email, display_name, avatar_url, role, auth_provider,
                      github_username, google_id, ethereum_address, ens_name
            "#,
        )
        .bind(email)
        .bind(&user_info.name)
        .bind(&user_info.avatar_url)
        .bind(provider_str)
        .bind(&user_info.github_username)
        .bind(&user_info.github_id)
        .bind(&user_info.google_id)
        .bind(existing_user.id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    } else {
        // Create new user
        let new_user_id = Uuid::new_v4().to_string();
        sqlx::query_as::<_, UserRecord>(
            r#"
            INSERT INTO users (user_id, email, display_name, avatar_url, auth_provider,
                               github_username, github_id, google_id,
                               last_login_at, password_hash, password_salt)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), '', '')
            RETURNING id, user_id, email, display_name, avatar_url, role, auth_provider,
                      github_username, google_id, ethereum_address, ens_name
            "#,
        )
        .bind(&new_user_id)
        .bind(email)
        .bind(&user_info.name)
        .bind(&user_info.avatar_url)
        .bind(provider_str)
        .bind(&user_info.github_username)
        .bind(&user_info.github_id)
        .bind(&user_info.google_id)
        .fetch_one(pool)
        .await
        .map_err(|e| AuthError::DatabaseError(e.to_string()))?
    };

    let role = match record.role.as_str() {
        "admin" => UserRole::Admin,
        "developer" => UserRole::Developer,
        "viewer" => UserRole::Viewer,
        _ => UserRole::Developer,
    };

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
    fn test_google_auth_url() {
        let config = GoogleOAuthConfig {
            client_id: "test-google-client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "https://agent-bestiary.world/auth/callback".to_string(),
        };

        let url = build_google_auth_url(&config, "test-state");

        assert!(url.contains("accounts.google.com"));
        assert!(url.contains("client_id=test-google-client"));
        assert!(url.contains("state=test-state"));
        assert!(url.contains("scope=openid+email+profile"));
    }

    #[test]
    fn test_github_auth_url() {
        let config = GitHubOAuthConfig {
            client_id: "test-github-client".to_string(),
            client_secret: "secret".to_string(),
            redirect_uri: "https://agent-bestiary.world/auth/callback".to_string(),
        };

        let url = build_github_auth_url(&config, "test-state");

        assert!(url.contains("github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=test-github-client"));
        assert!(url.contains("state=test-state"));
    }
}
