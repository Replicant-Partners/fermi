pub mod error;
pub mod jwt;
pub mod middleware;
pub mod oidc;
pub mod siwe;
pub mod types;

// Re-export commonly used types
pub use error::AuthError;
pub use oidc::{
    build_authorization_url, exchange_code_for_token, fetch_user_info, generate_state, sync_user,
    CallbackParams, OidcConfig, TokenResponse, UserInfoResponse,
};
pub use siwe::{
    cleanup_expired_nonces, create_challenge, verify_signature, SiweChallenge,
    SiweChallengeResponse, SiweVerify, SiweVerifyResponse,
};
pub use types::{ApiKey, AuthPrincipal, AuthProvider, User, UserRole};
