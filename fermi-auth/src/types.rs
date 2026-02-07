use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User identity across the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub user_id: String, // Zitadel uses custom string IDs, not UUIDs
    pub email: String,
    pub display_name: Option<String>,
    pub role: UserRole,
    pub org_id: Option<String>, // Zitadel organization ID for multi-tenancy
    pub auth_provider: AuthProvider, // How user signed in
    pub github_username: Option<String>, // For GitHub users
    pub google_id: Option<String>, // For Google users
    pub ethereum_address: Option<String>, // For Web3 wallet users (checksummed)
    pub ens_name: Option<String>, // ENS domain if resolved
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthProvider {
    Email,
    GitHub,
    Google,
    Ethereum, // Web3 wallet (SIWE)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Developer,
    Viewer,
}

impl UserRole {
    pub fn can_write(&self) -> bool {
        matches!(self, UserRole::Admin | UserRole::Developer)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, UserRole::Admin)
    }
}

/// API Key for programmatic access
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub key_id: Uuid,
    pub user_id: String, // Links back to User
    pub name: String,
    pub scopes: Vec<String>,
}

/// Authentication principal - either a user or an API key
#[derive(Debug, Clone)]
pub enum AuthPrincipal {
    User(User),
    ApiKey(ApiKey),
}

impl AuthPrincipal {
    pub fn user_id(&self) -> String {
        match self {
            AuthPrincipal::User(user) => user.user_id.clone(),
            AuthPrincipal::ApiKey(key) => key.user_id.clone(),
        }
    }

    pub fn can_write(&self) -> bool {
        match self {
            AuthPrincipal::User(user) => user.role.can_write(),
            AuthPrincipal::ApiKey(key) => key.scopes.contains(&"write".to_string()),
        }
    }

    pub fn can_admin(&self) -> bool {
        match self {
            AuthPrincipal::User(user) => user.role.can_admin(),
            AuthPrincipal::ApiKey(key) => key.scopes.contains(&"admin".to_string()),
        }
    }

    pub fn is_user(&self) -> bool {
        matches!(self, AuthPrincipal::User(_))
    }

    pub fn is_api_key(&self) -> bool {
        matches!(self, AuthPrincipal::ApiKey(_))
    }
}
