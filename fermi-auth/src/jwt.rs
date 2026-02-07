use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::{
    error::AuthError,
    types::{AuthPrincipal, AuthProvider, User, UserRole},
};

/// Session JWT claims — self-issued HS256 tokens
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String, // user_id
    pub email: String,
    pub name: Option<String>,
    pub role: String,     // "admin", "developer", "viewer"
    pub provider: String, // "google", "github", "ethereum", "email"
    pub github_username: Option<String>,
    pub google_id: Option<String>,
    pub exp: usize, // expiry (unix timestamp)
    pub iat: usize, // issued at (unix timestamp)
}

const SESSION_DURATION_SECS: usize = 7 * 24 * 60 * 60; // 7 days

/// Create a session JWT for an authenticated user
pub fn create_session_token(user: &User, secret: &str) -> Result<String, AuthError> {
    let now = chrono::Utc::now().timestamp() as usize;

    let claims = SessionClaims {
        sub: user.user_id.clone(),
        email: user.email.clone(),
        name: user.display_name.clone(),
        role: format!("{:?}", user.role).to_lowercase(),
        provider: format!("{:?}", user.auth_provider).to_lowercase(),
        github_username: user.github_username.clone(),
        google_id: user.google_id.clone(),
        exp: now + SESSION_DURATION_SECS,
        iat: now,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::ConfigError)
}

/// Validate a session JWT and return AuthPrincipal
pub fn validate_session_token(token: &str, secret: &str) -> Result<AuthPrincipal, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    let claims = token_data.claims;
    Ok(AuthPrincipal::User(claims_to_user(&claims)))
}

fn claims_to_user(claims: &SessionClaims) -> User {
    let role = match claims.role.as_str() {
        "admin" => UserRole::Admin,
        "developer" => UserRole::Developer,
        "viewer" => UserRole::Viewer,
        _ => UserRole::Developer,
    };

    let auth_provider = match claims.provider.as_str() {
        "github" => AuthProvider::GitHub,
        "google" => AuthProvider::Google,
        "ethereum" => AuthProvider::Ethereum,
        _ => AuthProvider::Email,
    };

    User {
        user_id: claims.sub.clone(),
        email: claims.email.clone(),
        display_name: claims.name.clone(),
        role,
        auth_provider,
        github_username: claims.github_username.clone(),
        google_id: claims.google_id.clone(),
        ethereum_address: None,
        ens_name: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user() -> User {
        User {
            user_id: "test-user-123".to_string(),
            email: "user@example.com".to_string(),
            display_name: Some("Test User".to_string()),
            role: UserRole::Developer,
            auth_provider: AuthProvider::GitHub,
            github_username: Some("testuser".to_string()),
            google_id: None,
            ethereum_address: None,
            ens_name: None,
        }
    }

    #[test]
    fn test_create_and_validate_token() {
        let user = test_user();
        let secret = "test-secret-key-that-is-long-enough";

        let token = create_session_token(&user, secret).unwrap();
        let principal = validate_session_token(&token, secret).unwrap();

        assert_eq!(principal.user_id(), "test-user-123");
        if let AuthPrincipal::User(u) = principal {
            assert_eq!(u.email, "user@example.com");
            assert_eq!(u.auth_provider, AuthProvider::GitHub);
            assert_eq!(u.github_username, Some("testuser".to_string()));
        } else {
            panic!("Expected User principal");
        }
    }

    #[test]
    fn test_invalid_secret_fails() {
        let user = test_user();
        let token = create_session_token(&user, "secret-1").unwrap();
        let result = validate_session_token(&token, "secret-2");
        assert!(result.is_err());
    }

    #[test]
    fn test_expired_token_fails() {
        let user = test_user();
        let secret = "test-secret";

        // Manually create an expired token
        let claims = SessionClaims {
            sub: user.user_id,
            email: user.email,
            name: user.display_name,
            role: "developer".to_string(),
            provider: "github".to_string(),
            github_username: user.github_username,
            google_id: None,
            exp: 1000000000, // way in the past
            iat: 999999000,
        };

        let token = encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let result = validate_session_token(&token, secret);
        assert!(result.is_err());
    }
}
