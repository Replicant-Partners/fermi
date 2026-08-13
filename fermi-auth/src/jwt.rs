use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::{
    error::AuthError,
    types::{AuthPrincipal, AuthProvider, Impersonation, ImpersonationMode, User, UserRole},
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

    /// Present only on admin "view as user" tokens.
    ///
    /// When set, every other claim above describes the **target** user,
    /// so a token that loses this field degrades into an ordinary
    /// session for the target rather than for the admin. That is the
    /// safe direction to fail, but it is still a downgrade, which is
    /// why the mint path is admin-gated and the TTL is short.
    ///
    /// `default` + `skip_serializing_if` keep ordinary session tokens
    /// byte-identical to before this field existed, so every JWT issued
    /// prior to this change still validates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imp: Option<ImpersonationClaims>,
}

/// The impersonation envelope carried inside a session JWT.
#[derive(Debug, Serialize, Deserialize)]
pub struct ImpersonationClaims {
    /// The admin's `user_id`.
    pub real_sub: String,
    pub real_email: String,
    /// The admin's role at mint time. Recorded for the audit trail;
    /// deliberately not consulted for access decisions.
    pub real_role: String,
    /// `impersonation_sessions.session_id`.
    pub sid: String,
    /// `read_only` | `assist`.
    pub mode: String,
}

const SESSION_DURATION_SECS: usize = 7 * 24 * 60 * 60; // 7 days

/// Impersonation tokens are minutes-scale, not days-scale. Support work
/// is bounded; a stale "view as" token lying around in a browser is a
/// liability with no upside. Re-minting is one click.
pub const IMPERSONATION_DURATION_SECS: usize = 30 * 60; // 30 minutes

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
        imp: None,
    };

    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::ConfigError)
}

/// Mint a short-lived admin "view as user" token.
///
/// The identity claims describe `target`; `admin` is recorded only in
/// the `imp` envelope. Callers must have already verified that the
/// caller is a platform admin and that `target` is an eligible subject
/// (see `handlers::impersonation`) — this function does no authorisation
/// of its own, it is purely the encoder.
pub fn create_impersonation_token(
    target: &User,
    admin: &User,
    session_id: uuid::Uuid,
    mode: ImpersonationMode,
    ttl_secs: usize,
    secret: &str,
) -> Result<(String, usize), AuthError> {
    let now = chrono::Utc::now().timestamp() as usize;
    let exp = now + ttl_secs;

    let claims = SessionClaims {
        sub: target.user_id.clone(),
        email: target.email.clone(),
        name: target.display_name.clone(),
        role: format!("{:?}", target.role).to_lowercase(),
        provider: format!("{:?}", target.auth_provider).to_lowercase(),
        github_username: target.github_username.clone(),
        google_id: target.google_id.clone(),
        exp,
        iat: now,
        imp: Some(ImpersonationClaims {
            real_sub: admin.user_id.clone(),
            real_email: admin.email.clone(),
            real_role: format!("{:?}", admin.role).to_lowercase(),
            sid: session_id.to_string(),
            mode: mode.as_str().to_string(),
        }),
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::ConfigError)?;

    Ok((token, exp))
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
    let effective = claims_to_user(&claims);

    let Some(imp) = claims.imp.as_ref() else {
        return Ok(AuthPrincipal::User(effective));
    };

    // An impersonation token whose session id is unparseable cannot be
    // tied to an audit row, so it cannot be honoured. Rejecting outright
    // (rather than degrading to a plain session for the target) keeps
    // the invariant "every impersonated request is auditable" total.
    let session_id = uuid::Uuid::parse_str(&imp.sid).map_err(|_| AuthError::InvalidToken)?;

    Ok(AuthPrincipal::Impersonated(Box::new(Impersonation {
        real: User {
            user_id: imp.real_sub.clone(),
            email: imp.real_email.clone(),
            display_name: None,
            role: parse_role(&imp.real_role),
            auth_provider: AuthProvider::Email,
            github_username: None,
            google_id: None,
            ethereum_address: None,
            ens_name: None,
        },
        effective,
        session_id,
        mode: ImpersonationMode::from_str(&imp.mode),
    })))
}

fn parse_role(role: &str) -> UserRole {
    match role {
        "admin" => UserRole::Admin,
        "viewer" => UserRole::Viewer,
        _ => UserRole::Developer,
    }
}

fn claims_to_user(claims: &SessionClaims) -> User {
    let role = parse_role(&claims.role);

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
            imp: None,
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

    // ─── Impersonation ─────────────────────────────────────────────

    fn admin_user() -> User {
        User {
            user_id: "admin-1".to_string(),
            email: "admin@example.com".to_string(),
            display_name: Some("Admin".to_string()),
            role: UserRole::Admin,
            auth_provider: AuthProvider::Email,
            github_username: None,
            google_id: None,
            ethereum_address: None,
            ens_name: None,
        }
    }

    fn mint_impersonation(secret: &str) -> AuthPrincipal {
        let (token, _exp) = create_impersonation_token(
            &test_user(),
            &admin_user(),
            uuid::Uuid::nil(),
            ImpersonationMode::ReadOnly,
            IMPERSONATION_DURATION_SECS,
            secret,
        )
        .unwrap();
        validate_session_token(&token, secret).unwrap()
    }

    #[test]
    fn impersonation_token_round_trips_to_impersonated_principal() {
        let principal = mint_impersonation("s");

        assert!(principal.is_impersonating());
        // Acts as the target...
        assert_eq!(principal.user_id(), "test-user-123");
        // ...while remaining attributable to the admin.
        assert_eq!(principal.real_user_id(), "admin-1");

        let imp = principal.impersonation().expect("impersonation context");
        assert_eq!(imp.mode, ImpersonationMode::ReadOnly);
        assert_eq!(imp.session_id, uuid::Uuid::nil());
        assert_eq!(imp.effective.email, "user@example.com");
        assert_eq!(imp.real.email, "admin@example.com");
    }

    /// The load-bearing security property: impersonating a non-admin
    /// must *drop* admin rights for the duration. If this ever flips,
    /// the feature stops being a diagnostic tool and becomes a way to
    /// carry admin authority into a user's context unnoticed.
    #[test]
    fn impersonating_a_non_admin_drops_admin_rights() {
        let principal = mint_impersonation("s");

        assert!(!principal.can_admin(), "admin rights must not survive");
        assert_eq!(principal.role_str(), "developer");
        // The admin's real role is still on record for the audit trail.
        assert_eq!(
            principal.impersonation().unwrap().real.role,
            UserRole::Admin
        );
    }

    /// An impersonated session is still a user session: email-bearing
    /// flows (invite acceptance) must keep working against the target.
    #[test]
    fn impersonated_principal_is_a_user_not_an_api_key() {
        let principal = mint_impersonation("s");

        assert!(principal.is_user());
        assert!(!principal.is_api_key());
        assert_eq!(
            principal.as_user().map(|u| u.email.as_str()),
            Some("user@example.com")
        );
    }

    /// Ordinary sessions must be untouched by the added claim, both in
    /// behaviour and on the wire (no `imp` key emitted).
    #[test]
    fn ordinary_session_carries_no_impersonation() {
        let token = create_session_token(&test_user(), "s").unwrap();
        let principal = validate_session_token(&token, "s").unwrap();

        assert!(!principal.is_impersonating());
        assert!(principal.impersonation().is_none());
        assert_eq!(principal.real_user_id(), principal.user_id());

        let payload = token.split('.').nth(1).expect("jwt payload segment");
        let decoded = base64_decode_url(payload);
        assert!(
            !decoded.contains("\"imp\""),
            "imp must be omitted from ordinary sessions, got: {decoded}"
        );
    }

    /// A token claiming impersonation but carrying a non-UUID session id
    /// can't be tied to an audit row, so it must be rejected rather than
    /// quietly downgraded into a session for the target.
    #[test]
    fn impersonation_token_with_unparseable_session_id_is_rejected() {
        let secret = "s";
        let now = chrono::Utc::now().timestamp() as usize;
        let claims = SessionClaims {
            sub: "test-user-123".into(),
            email: "user@example.com".into(),
            name: None,
            role: "developer".into(),
            provider: "email".into(),
            github_username: None,
            google_id: None,
            exp: now + 600,
            iat: now,
            imp: Some(ImpersonationClaims {
                real_sub: "admin-1".into(),
                real_email: "admin@example.com".into(),
                real_role: "admin".into(),
                sid: "not-a-uuid".into(),
                mode: "read_only".into(),
            }),
        };
        let token = encode(
            &jsonwebtoken::Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        assert!(validate_session_token(&token, secret).is_err());
    }

    /// Unknown/garbage modes must land on the most restrictive mode.
    #[test]
    fn unknown_mode_falls_back_to_read_only() {
        assert_eq!(
            ImpersonationMode::from_str("totally-new-mode"),
            ImpersonationMode::ReadOnly
        );
        assert_eq!(ImpersonationMode::from_str(""), ImpersonationMode::ReadOnly);
        assert_eq!(
            ImpersonationMode::from_str("assist"),
            ImpersonationMode::Assist
        );
    }

    /// Minimal base64url decoder so the wire-format assertion above
    /// doesn't pull in a dependency just for a test.
    fn base64_decode_url(s: &str) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut buf: Vec<u8> = Vec::new();
        let mut acc: u32 = 0;
        let mut bits = 0u32;
        for c in s.bytes().filter(|b| *b != b'=') {
            let Some(idx) = ALPHABET.iter().position(|a| *a == c) else {
                continue;
            };
            acc = (acc << 6) | idx as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                buf.push((acc >> bits) as u8);
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    }
}
