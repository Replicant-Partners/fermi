use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// User identity across the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub role: UserRole,
    pub auth_provider: AuthProvider,
    pub github_username: Option<String>,
    pub google_id: Option<String>,
    pub ethereum_address: Option<String>,
    pub ens_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthProvider {
    Email,
    GitHub,
    Google,
    Ethereum,
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
    pub user_id: String,
    pub name: String,
    pub scopes: Vec<String>,
}

/// Authentication principal — either a user session or an API key
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

    pub fn role_str(&self) -> &str {
        match self {
            AuthPrincipal::User(user) => match user.role {
                UserRole::Admin => "admin",
                UserRole::Developer => "developer",
                UserRole::Viewer => "viewer",
            },
            AuthPrincipal::ApiKey(_) => "developer",
        }
    }
}

// ─── Visibility Model ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Private,
    Shared,
    Public,
}

impl Visibility {
    pub fn from_legacy(s: &str) -> Self {
        match s {
            "public" => Visibility::Public,
            "unlisted" => Visibility::Shared,
            _ => Visibility::Private,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Visibility::Private => "private",
            Visibility::Shared => "shared",
            Visibility::Public => "public",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ObjectType {
    Agent,
    Capability,
    Forecast,
    Index,
    Repo,
    File,
    /// Added migration 117 — workspace-level sharing for App workspaces.
    Workspace,
}

impl ObjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ObjectType::Agent => "agent",
            ObjectType::Capability => "capability",
            ObjectType::Forecast => "forecast",
            ObjectType::Index => "index",
            ObjectType::Repo => "repo",
            ObjectType::File => "file",
            ObjectType::Workspace => "workspace",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent" => Some(ObjectType::Agent),
            "capability" => Some(ObjectType::Capability),
            "forecast" => Some(ObjectType::Forecast),
            "index" => Some(ObjectType::Index),
            "repo" => Some(ObjectType::Repo),
            "file" => Some(ObjectType::File),
            "workspace" => Some(ObjectType::Workspace),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    View,
    Edit,
    Admin,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::View => "view",
            Permission::Edit => "edit",
            Permission::Admin => "admin",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "admin" => Permission::Admin,
            "edit" => Permission::Edit,
            _ => Permission::View,
        }
    }
}

// ─── Team Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Viewer,
    Member,
    Admin,
    Owner,
}

impl TeamRole {
    pub fn can_invite(&self) -> bool {
        matches!(self, TeamRole::Admin | TeamRole::Owner)
    }

    pub fn can_share(&self) -> bool {
        matches!(self, TeamRole::Member | TeamRole::Admin | TeamRole::Owner)
    }

    pub fn can_admin(&self) -> bool {
        matches!(self, TeamRole::Admin | TeamRole::Owner)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            TeamRole::Owner => "owner",
            TeamRole::Admin => "admin",
            TeamRole::Member => "member",
            TeamRole::Viewer => "viewer",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "owner" => TeamRole::Owner,
            "admin" => TeamRole::Admin,
            "member" => TeamRole::Member,
            _ => TeamRole::Viewer,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemberType {
    User,
    Agent,
}

impl MemberType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberType::User => "user",
            MemberType::Agent => "agent",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "agent" => MemberType::Agent,
            _ => MemberType::User,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ShareType {
    Team,
    User,
}

impl ShareType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShareType::Team => "team",
            ShareType::User => "user",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub owner_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub team_id: Uuid,
    pub member_type: MemberType,
    pub member_id: String,
    pub role: TeamRole,
    pub joined_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectShare {
    pub id: Uuid,
    pub object_type: ObjectType,
    pub object_id: String,
    pub share_type: ShareType,
    pub share_target: String,
    pub permission: Permission,
    pub granted_by: String,
}
