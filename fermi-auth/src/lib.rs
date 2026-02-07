pub mod api_keys;
pub mod error;
pub mod jwt;
pub mod middleware;
pub mod oidc;
pub mod siwe;
pub mod types;

// Re-export commonly used types
pub use error::AuthError;
pub use jwt::{create_session_token, validate_session_token};
pub use middleware::{auth_middleware, optional_auth_middleware, AuthState};
pub use oidc::{
    build_github_auth_url, build_google_auth_url, generate_state, github_exchange_code,
    github_fetch_user_info, google_exchange_code, google_fetch_user_info, sync_user,
    CallbackParams, GitHubOAuthConfig, GoogleOAuthConfig, OAuthConfig, UserInfoResponse,
};
pub use siwe::{
    cleanup_expired_nonces, create_challenge, verify_signature, SiweChallenge,
    SiweChallengeResponse, SiweVerify, SiweVerifyResponse,
};
pub use types::{ApiKey, AuthPrincipal, AuthProvider, User, UserRole};
