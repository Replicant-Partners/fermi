use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    error::AuthError,
    types::{AuthPrincipal, AuthProvider, User, UserRole},
};

// Cache for JWKS (JSON Web Key Set)
static JWKS_CACHE: Lazy<Arc<RwLock<Option<JwkSet>>>> = Lazy::new(|| Arc::new(RwLock::new(None)));

/// Zitadel OIDC JWT claims structure
#[derive(Debug, Serialize, Deserialize)]
pub struct ZitadelClaims {
    pub sub: String, // Zitadel user_id
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub preferred_username: Option<String>,
    pub name: Option<String>,
    pub picture: Option<String>, // Avatar URL

    #[serde(rename = "urn:zitadel:iam:org:id")]
    pub org_id: Option<String>, // Organization ID for multi-tenancy

    #[serde(rename = "urn:zitadel:iam:org:project:roles")]
    pub roles: Option<serde_json::Value>, // Role assignments

    // Social provider info (when using GitHub/Google)
    #[serde(rename = "urn:zitadel:iam:user:resourceowner:name")]
    pub idp_name: Option<String>, // "github", "google"

    // GitHub-specific claims
    pub github_username: Option<String>,
    pub github_id: Option<String>,

    // Google-specific claims
    pub google_id: Option<String>,

    pub exp: usize,
    pub iat: usize,
    pub iss: String,      // Issuer (Zitadel instance URL)
    pub aud: Vec<String>, // Audience (our client ID)
}

/// Validate JWT token and return AuthPrincipal
pub async fn validate_jwt(token: &str) -> Result<AuthPrincipal, AuthError> {
    let zitadel_issuer = std::env::var("ZITADEL_ISSUER").map_err(|_| AuthError::ConfigError)?;
    let zitadel_client_id =
        std::env::var("ZITADEL_CLIENT_ID").map_err(|_| AuthError::ConfigError)?;

    // Decode header to get key ID (kid)
    let header = decode_header(token)?;
    let kid = header.kid.ok_or(AuthError::InvalidToken)?;

    // Fetch JWKS if not cached
    let jwks = {
        let cache = JWKS_CACHE.read().await;
        if cache.is_none() {
            drop(cache);
            refresh_jwks(&zitadel_issuer).await?;
            JWKS_CACHE
                .read()
                .await
                .clone()
                .ok_or(AuthError::ConfigError)?
        } else {
            cache.clone().ok_or(AuthError::ConfigError)?
        }
    };

    // Find the key matching kid
    let jwk = jwks
        .keys
        .iter()
        .find(|k| k.common.key_id.as_ref() == Some(&kid))
        .ok_or(AuthError::InvalidToken)?;

    // Create decoding key from JWK
    let decoding_key = DecodingKey::from_jwk(jwk)?;

    // Validate JWT
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&zitadel_issuer]);
    validation.set_audience(&[&zitadel_client_id]);
    validation.validate_exp = true;

    let token_data = decode::<ZitadelClaims>(token, &decoding_key, &validation)?;
    let claims = token_data.claims;

    // Convert claims to User
    let user = claims_to_user(&claims);

    Ok(AuthPrincipal::User(user))
}

/// Refresh JWKS from Zitadel's well-known endpoint
async fn refresh_jwks(issuer: &str) -> Result<(), AuthError> {
    let jwks_url = format!("{}/.well-known/jwks.json", issuer);

    let response = reqwest::get(&jwks_url)
        .await
        .map_err(|_| AuthError::ConfigError)?;

    let jwks: JwkSet = response.json().await.map_err(|_| AuthError::ConfigError)?;

    let mut cache = JWKS_CACHE.write().await;
    *cache = Some(jwks);

    Ok(())
}

/// Convert Zitadel JWT claims to our User type
fn claims_to_user(claims: &ZitadelClaims) -> User {
    // Determine auth provider from claims
    let auth_provider = if claims.github_username.is_some() {
        AuthProvider::GitHub
    } else if claims.google_id.is_some() {
        AuthProvider::Google
    } else {
        AuthProvider::Email
    };

    User {
        user_id: claims.sub.clone(),
        email: claims.email.clone().unwrap_or_default(),
        display_name: claims.name.clone().or(claims.preferred_username.clone()),
        role: UserRole::Developer, // TODO: Extract from Zitadel roles
        org_id: claims.org_id.clone(),
        auth_provider,
        github_username: claims.github_username.clone(),
        google_id: claims.google_id.clone(),
        ethereum_address: None, // Only set for SIWE users
        ens_name: None,
    }
}

/// Force refresh of JWKS cache (useful for key rotation)
pub async fn force_refresh_jwks() -> Result<(), AuthError> {
    let zitadel_issuer = std::env::var("ZITADEL_ISSUER").map_err(|_| AuthError::ConfigError)?;
    refresh_jwks(&zitadel_issuer).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claims_to_user_github() {
        let claims = ZitadelClaims {
            sub: "test-user-id".to_string(),
            email: Some("user@example.com".to_string()),
            email_verified: Some(true),
            preferred_username: Some("testuser".to_string()),
            name: Some("Test User".to_string()),
            picture: None,
            org_id: None,
            roles: None,
            idp_name: Some("github".to_string()),
            github_username: Some("testuser".to_string()),
            github_id: Some("12345".to_string()),
            google_id: None,
            exp: 9999999999,
            iat: 1234567890,
            iss: "https://test.zitadel.cloud".to_string(),
            aud: vec!["test-client-id".to_string()],
        };

        let user = claims_to_user(&claims);
        assert_eq!(user.user_id, "test-user-id");
        assert_eq!(user.email, "user@example.com");
        assert_eq!(user.auth_provider, AuthProvider::GitHub);
        assert_eq!(user.github_username, Some("testuser".to_string()));
    }
}
