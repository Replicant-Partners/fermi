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

    /// Return the user ID as a UUID, for binding to UUID columns in the DB.
    /// The user_id string may be a UUID (from the `users.id` column) or a
    /// text identifier (Zitadel ID / Ethereum address). This method parses
    /// the string and returns `None` if it is not a valid UUID.
    pub fn user_uuid(&self) -> Option<Uuid> {
        Uuid::parse_str(&self.user_id()).ok()
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
    /// Spec 24 §3.1.2 / migration 152 — portfolio-level sharing.
    Portfolio,
    Index,
    Repo,
    File,
    /// Added migration 117 — workspace-level sharing for App workspaces.
    /// Note: on the current Neon schema this value is NOT in the
    /// object_shares CHECK constraint (migration 117 didn't take); the
    /// enum keeps it for code-level use until that's fixed separately.
    Workspace,
    // ─── v0.10.4 substrate additions ────────────────────────────────
    //
    // These variants are consumed by `fermi_auth::rbac::require` so
    // every tenant app's owner check can go through the same helper.
    // They are NOT (yet) in the `object_shares.object_type` CHECK
    // constraint — an `object_shares` row with these values would be
    // rejected by the DB. Extend the CHECK constraint (mig 157
    // pattern) when a specific resource needs share/team ACL beyond
    // owner+admin.
    /// Rabble creature (owner-only or admin; no shares yet).
    Creature,
    /// Team / workspace primary resource. Not to be confused with
    /// `Workspace` (which was reserved for a separate mig-117 flow).
    Team,
    /// Rabble swarm event / creature gathering.
    SwarmEvent,
    /// simOps / Rabble swarm telemetry session.
    SwarmSession,
    /// SOSA sensor platform.
    SosaPlatform,
    /// SOSA observation session.
    ObservationSession,
    /// AR beacon (spatial tenant).
    ArBeacon,
    /// Apps directory entry (owner_user_id).
    App,
}

impl ObjectType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ObjectType::Agent => "agent",
            ObjectType::Capability => "capability",
            ObjectType::Forecast => "forecast",
            ObjectType::Portfolio => "portfolio",
            ObjectType::Index => "index",
            ObjectType::Repo => "repo",
            ObjectType::File => "file",
            ObjectType::Workspace => "workspace",
            ObjectType::Creature => "creature",
            ObjectType::Team => "team",
            ObjectType::SwarmEvent => "swarm_event",
            ObjectType::SwarmSession => "swarm_session",
            ObjectType::SosaPlatform => "sosa_platform",
            ObjectType::ObservationSession => "observation_session",
            ObjectType::ArBeacon => "ar_beacon",
            ObjectType::App => "app",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "agent" => Some(ObjectType::Agent),
            "capability" => Some(ObjectType::Capability),
            "forecast" => Some(ObjectType::Forecast),
            "portfolio" => Some(ObjectType::Portfolio),
            "index" => Some(ObjectType::Index),
            "repo" => Some(ObjectType::Repo),
            "file" => Some(ObjectType::File),
            "workspace" => Some(ObjectType::Workspace),
            "creature" => Some(ObjectType::Creature),
            "team" => Some(ObjectType::Team),
            "swarm_event" => Some(ObjectType::SwarmEvent),
            "swarm_session" => Some(ObjectType::SwarmSession),
            "sosa_platform" => Some(ObjectType::SosaPlatform),
            "observation_session" => Some(ObjectType::ObservationSession),
            "ar_beacon" => Some(ObjectType::ArBeacon),
            "app" => Some(ObjectType::App),
            _ => None,
        }
    }

    /// The tenant resource table this object type is stored in. Used
    /// by the RBAC substrate to map `ObjectType` → SQL for admin
    /// diagnostics + reassign flows (`POST /api/admin/rbac/reassign`).
    ///
    /// Returns `(table_name, primary_key_column, owner_column)`.
    ///
    /// Extend this when a new tenant resource lands. If a variant is
    /// used only for `object_shares` ACL (never as a first-class owned
    /// resource that can be reassigned), return `None`.
    pub fn owner_table(&self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            ObjectType::Agent => Some(("agents", "agent_id", "user_id")),
            ObjectType::Team => Some(("teams", "id", "owner_id")),
            ObjectType::Forecast => Some(("fermi_forecasts", "id", "owner_id")),
            ObjectType::Portfolio => Some(("fermi_portfolios", "id", "owner_id")),
            ObjectType::Creature => Some(("creatures", "creature_id", "owner_id")),
            ObjectType::SwarmEvent => Some(("swarm_events", "swarm_id", "creator_id")),
            ObjectType::SwarmSession => Some(("swarm_sessions", "session_id", "owner_id")),
            ObjectType::SosaPlatform => Some(("sosa_platforms", "platform_id", "owner_id")),
            ObjectType::ObservationSession => {
                Some(("observation_sessions", "session_id", "owner_id"))
            }
            ObjectType::ArBeacon => Some(("ar_beacons", "beacon_id", "creator_id")),
            ObjectType::App => Some(("apps", "id", "owner_user_id")),
            // Types without a canonical owned-resource table:
            // Capability, Index, Repo, File, Workspace — either
            // per-owner via a separate table not modelled here, or
            // reserved for future use.
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
    /// Which vertical created the team (`fermi_forecast`, `rabble_swarm`,
    /// `kask_simops`, …). ABW is shared substrate; consumers scope by this.
    #[serde(default)]
    pub origin: Option<String>,
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
