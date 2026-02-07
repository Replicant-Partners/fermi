pub mod api_keys;
pub mod error;
pub mod jwt;
pub mod middleware;
pub mod oidc;
pub mod siwe;
pub mod teams;
pub mod types;
pub mod visibility;

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
pub use teams::{
    add_team_member, create_team, delete_team, get_member_role, get_team, get_team_members,
    get_user_teams, list_object_shares, remove_team_member, revoke_share, share_object,
    update_member_role,
};
pub use types::{
    ApiKey, AuthPrincipal, AuthProvider, MemberType, ObjectShare, ObjectType, Permission,
    ShareType, Team, TeamMember, TeamRole, User, UserRole, Visibility,
};
pub use visibility::{can_access, can_access_anonymous, can_edit, can_view, AccessLevel};
