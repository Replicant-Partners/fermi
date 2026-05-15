//! App registry handlers — CRUD for the App primitive.
//!
//! An App is a registered platform artifact that ties together a composition,
//! a canonical document schema, a workspace template, and a UI pointer.
//! See `docs/specs/01_APP_PRIMITIVE.md` for the full design.
//!
//! Routes:
//! ```text
//!   GET  /api/apps                       list Apps (filtered by visibility)
//!   POST /api/apps                       register a new App
//!   GET  /api/apps/:slug                 get one App
//!   PUT  /api/apps/:slug                 update an App (owner only)
//!   POST /api/apps/:slug/workspaces      spawn a workspace from an App
//!   GET  /api/apps/:slug/workspaces      list workspaces this App spawned (caller only)
//!   POST /api/apps/:slug/publish         promote visibility to "public"
//!   POST /api/apps/:slug/archive         archive an App
//! ```

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use fermi_auth::{credit_deposit, get_or_create_wallet, teams, AuthPrincipal};

use crate::{resolve_agent, AppState};

// ─── Reserved origin tags ────────────────────────────────────────────────────
//
// These strings cannot be used as App slugs because existing workspaces
// already use them as origin values. Enforced in code, not in the DB
// (simpler to extend without a migration).

const RESERVED_SLUGS: &[&str] = &[
    "bestiary_workspace",
    "rabble_swarm",
    "personal_workspace",
    "fermi_forecast",
    "silat_workspace",
];

fn is_reserved(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

// ─── Request types ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateAppRequest {
    pub slug: String,
    pub name: String,
    pub tagline: Option<String>,
    pub homepage_url: Option<String>,
    pub icon_url: Option<String>,
    pub composition_slug: Option<String>,
    pub schema_slug: Option<String>,
    pub schema_json: Option<Value>,
    pub workspace_template: Option<Value>,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAppRequest {
    pub name: Option<String>,
    pub tagline: Option<String>,
    pub homepage_url: Option<String>,
    pub icon_url: Option<String>,
    pub composition_slug: Option<String>,
    pub schema_slug: Option<String>,
    pub schema_json: Option<Value>,
    pub workspace_template: Option<Value>,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    pub visibility: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListAppsQuery {
    pub visibility: Option<String>,
    pub owner: Option<String>,
    pub slug_prefix: Option<String>,
    pub include_archived: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SpawnWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub extra_budget: Option<i32>,
    pub auto_hire_override: Option<Vec<String>>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Fetch an App row by slug. Returns (row as Value) or 404.
async fn get_app_row(
    db: &sqlx::PgPool,
    slug: &str,
) -> Result<Value, (StatusCode, String)> {
    let row = sqlx::query(
        r#"SELECT id, slug, name, tagline, owner_user_id, owner_team_id,
                  homepage_url, icon_url, composition_slug, schema_slug,
                  schema_json, workspace_template, revenue_share,
                  pricing_policy, visibility, published_at, archived_at,
                  description, metadata, created_at, updated_at
           FROM apps WHERE slug = $1"#,
    )
    .bind(slug)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, format!("App '{}' not found", slug)))?;

    Ok(row_to_app_json(&row))
}

fn row_to_app_json(row: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id":                 row.try_get::<Uuid, _>("id").ok(),
        "slug":               row.try_get::<String, _>("slug").unwrap_or_default(),
        "name":               row.try_get::<String, _>("name").unwrap_or_default(),
        "tagline":            row.try_get::<Option<String>, _>("tagline").ok().flatten(),
        "owner_user_id":      row.try_get::<String, _>("owner_user_id").unwrap_or_default(),
        "owner_team_id":      row.try_get::<Option<Uuid>, _>("owner_team_id").ok().flatten(),
        "homepage_url":       row.try_get::<Option<String>, _>("homepage_url").ok().flatten(),
        "icon_url":           row.try_get::<Option<String>, _>("icon_url").ok().flatten(),
        "composition_slug":   row.try_get::<Option<String>, _>("composition_slug").ok().flatten(),
        "schema_slug":        row.try_get::<Option<String>, _>("schema_slug").ok().flatten(),
        "schema_json":        row.try_get::<Option<Value>, _>("schema_json").ok().flatten(),
        "workspace_template": row.try_get::<Value, _>("workspace_template").unwrap_or(json!({})),
        "revenue_share":      row.try_get::<Option<Value>, _>("revenue_share").ok().flatten(),
        "pricing_policy":     row.try_get::<String, _>("pricing_policy").unwrap_or_else(|_| "platform_default".into()),
        "visibility":         row.try_get::<String, _>("visibility").unwrap_or_else(|_| "private".into()),
        "published_at":       row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("published_at").ok().flatten().map(|t| t.to_rfc3339()),
        "archived_at":        row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("archived_at").ok().flatten().map(|t| t.to_rfc3339()),
        "description":        row.try_get::<Option<String>, _>("description").ok().flatten(),
        "metadata":           row.try_get::<Value, _>("metadata").unwrap_or(json!({})),
        "created_at":         row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
        "updated_at":         row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").ok().map(|t| t.to_rfc3339()),
    })
}

// ─── GET /api/apps ───────────────────────────────────────────────────────────

pub async fn list_apps_handler(
    State(state): State<AppState>,
    principal: Option<AuthPrincipal>,
    Query(q): Query<ListAppsQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.as_ref().map(|p| p.user_id());
    let include_archived = q.include_archived.unwrap_or(false);

    // Build the WHERE clauses dynamically.
    // - Unauthenticated: public only.
    // - Authenticated: public + own private/unlisted.
    let rows = if let Some(ref uid) = caller_id {
        let visibility_filter = match q.visibility.as_deref() {
            Some(v) => format!(" AND visibility = '{}'", v.replace('\'', "''")),
            None => String::new(),
        };
        let owner_filter = match q.owner.as_deref() {
            Some(o) => format!(" AND owner_user_id = '{}'", o.replace('\'', "''")),
            None => String::new(),
        };
        let prefix_filter = match q.slug_prefix.as_deref() {
            Some(p) => format!(" AND slug LIKE '{}%'", p.replace('\'', "''").replace('%', "\\%")),
            None => String::new(),
        };
        let archived_filter = if include_archived {
            String::new()
        } else {
            " AND archived_at IS NULL".to_string()
        };
        let sql = format!(
            r#"SELECT id, slug, name, tagline, owner_user_id, owner_team_id,
                      homepage_url, icon_url, composition_slug, schema_slug,
                      schema_json, workspace_template, revenue_share,
                      pricing_policy, visibility, published_at, archived_at,
                      description, metadata, created_at, updated_at
               FROM apps
               WHERE (visibility = 'public' OR owner_user_id = $1)
               {}{}{}{} ORDER BY created_at DESC LIMIT 200"#,
            visibility_filter, owner_filter, prefix_filter, archived_filter
        );
        sqlx::query(&sql)
            .bind(uid)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    } else {
        // Unauthenticated: public only.
        let archived_filter = if include_archived { "" } else { " AND archived_at IS NULL" };
        let sql = format!(
            r#"SELECT id, slug, name, tagline, owner_user_id, owner_team_id,
                      homepage_url, icon_url, composition_slug, schema_slug,
                      schema_json, workspace_template, revenue_share,
                      pricing_policy, visibility, published_at, archived_at,
                      description, metadata, created_at, updated_at
               FROM apps WHERE visibility = 'public'{} ORDER BY created_at DESC LIMIT 200"#,
            archived_filter
        );
        sqlx::query(&sql)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let apps: Vec<Value> = rows.iter().map(row_to_app_json).collect();
    Ok(Json(json!({ "apps": apps, "total": apps.len() })))
}

// ─── POST /api/apps ──────────────────────────────────────────────────────────

pub async fn create_app_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let owner_id = principal.user_id();

    // Validate slug format (same regex as DB CHECK)
    if !body.slug.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        return Err((StatusCode::BAD_REQUEST, "slug must start with a lowercase letter".into()));
    }
    let valid_slug = body.slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid_slug || body.slug.len() < 3 || body.slug.len() > 64 {
        return Err((StatusCode::BAD_REQUEST, "slug must be 3-64 chars, lowercase letters, digits, underscores only".into()));
    }
    if is_reserved(&body.slug) {
        return Err((StatusCode::CONFLICT, format!("'{}' is a reserved origin tag and cannot be used as an App slug", body.slug)));
    }

    let visibility = body.visibility.as_deref().unwrap_or("private");
    if !["private", "unlisted", "public"].contains(&visibility) {
        return Err((StatusCode::BAD_REQUEST, "visibility must be 'private', 'unlisted', or 'public'".into()));
    }

    let workspace_template = body.workspace_template.unwrap_or(json!({}));
    let metadata = body.metadata.unwrap_or(json!({}));

    let row = sqlx::query(
        r#"INSERT INTO apps (
            slug, name, tagline, owner_user_id,
            homepage_url, icon_url,
            composition_slug, schema_slug, schema_json,
            workspace_template, visibility, description, metadata
           ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
           RETURNING id"#,
    )
    .bind(&body.slug)
    .bind(&body.name)
    .bind(&body.tagline)
    .bind(&owner_id)
    .bind(&body.homepage_url)
    .bind(&body.icon_url)
    .bind(&body.composition_slug)
    .bind(&body.schema_slug)
    .bind(&body.schema_json)
    .bind(&workspace_template)
    .bind(visibility)
    .bind(&body.description)
    .bind(&metadata)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            (StatusCode::CONFLICT, format!("App slug '{}' is already in use", body.slug))
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    let id: Uuid = row.try_get("id").map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let app = get_app_row(&state.db, &body.slug).await?;
    tracing::info!(app_id = %id, slug = %body.slug, owner = %owner_id, "App registered");
    Ok((StatusCode::CREATED, Json(app)))
}

// ─── GET /api/apps/:slug ─────────────────────────────────────────────────────

pub async fn get_app_handler(
    State(state): State<AppState>,
    principal: Option<AuthPrincipal>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app = get_app_row(&state.db, &slug).await?;

    let visibility = app["visibility"].as_str().unwrap_or("private");
    if visibility == "private" {
        let caller_id = principal.map(|p| p.user_id());
        let owner_id = app["owner_user_id"].as_str().unwrap_or("");
        let is_admin = false; // TODO: wire through can_admin() when principal is available
        if caller_id.as_deref() != Some(owner_id) && !is_admin {
            return Err((StatusCode::NOT_FOUND, format!("App '{}' not found", slug)));
        }
    }

    Ok(Json(app))
}

// ─── PUT /api/apps/:slug ─────────────────────────────────────────────────────

pub async fn update_app_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Read body manually since we can't have two Path extractors
    // This is called from the route with a Json extractor wired separately.
    // The handler is called as update_app_handler_inner after body parsing.
    // See route registration — we use a wrapper approach.
    let _ = principal; // consumed below in the inner handler
    Err((StatusCode::INTERNAL_SERVER_ERROR, "use update_app_handler_inner".into()))
}

pub async fn update_app_handler_full(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
    Json(body): Json<UpdateAppRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();
    let app = get_app_row(&state.db, &slug).await?;
    let owner_id = app["owner_user_id"].as_str().unwrap_or("");
    if caller_id != owner_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Only the App owner can update it".into()));
    }

    // Build a SET clause from whichever fields are present.
    // We use direct SQL so we can patch any subset cleanly.
    if let Some(ref v) = body.visibility {
        if !["private", "unlisted", "public"].contains(&v.as_str()) {
            return Err((StatusCode::BAD_REQUEST, "visibility must be 'private', 'unlisted', or 'public'".into()));
        }
    }

    sqlx::query(
        r#"UPDATE apps SET
            name               = COALESCE($2, name),
            tagline            = COALESCE($3, tagline),
            homepage_url       = COALESCE($4, homepage_url),
            icon_url           = COALESCE($5, icon_url),
            composition_slug   = COALESCE($6, composition_slug),
            schema_slug        = COALESCE($7, schema_slug),
            schema_json        = COALESCE($8, schema_json),
            workspace_template = COALESCE($9, workspace_template),
            description        = COALESCE($10, description),
            metadata           = COALESCE($11, metadata),
            visibility         = COALESCE($12, visibility)
           WHERE slug = $1"#,
    )
    .bind(&slug)
    .bind(&body.name)
    .bind(&body.tagline)
    .bind(&body.homepage_url)
    .bind(&body.icon_url)
    .bind(&body.composition_slug)
    .bind(&body.schema_slug)
    .bind(&body.schema_json)
    .bind(&body.workspace_template)
    .bind(&body.description)
    .bind(&body.metadata)
    .bind(&body.visibility)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    get_app_row(&state.db, &slug).await.map(Json)
}

// ─── POST /api/apps/:slug/workspaces ─────────────────────────────────────────

pub async fn spawn_workspace_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
    Json(req): Json<SpawnWorkspaceRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let caller_id = principal.user_id();

    // 1. Fetch the App.
    let app = get_app_row(&state.db, &slug).await?;

    // 2. Visibility check.
    let visibility = app["visibility"].as_str().unwrap_or("private");
    if visibility == "private" {
        let owner_id = app["owner_user_id"].as_str().unwrap_or("");
        if caller_id != owner_id && !principal.can_admin() {
            return Err((StatusCode::FORBIDDEN, "This App is private; only the owner can spawn workspaces from it".into()));
        }
    }
    // Archived apps cannot spawn new workspaces.
    if app["archived_at"].is_string() {
        return Err((StatusCode::GONE, "This App has been archived and no longer accepts new workspaces".into()));
    }

    // 3. Parse workspace template.
    let template = &app["workspace_template"];
    let initial_budget: i32 = template.get("initial_budget")
        .and_then(|v| v.as_i64()).unwrap_or(100) as i32;
    let extra_budget: i32 = req.extra_budget.unwrap_or(0).max(0);
    let total_budget = initial_budget + extra_budget;

    let auto_hire: Vec<String> = req.auto_hire_override.unwrap_or_else(|| {
        template.get("auto_hire")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    });

    let initial_files: Vec<(String, String)> = template.get("initial_files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter().filter_map(|f| {
                let path = f.get("path")?.as_str()?.to_string();
                let content = f.get("content")?.as_str()?.to_string();
                Some((path, content))
            }).collect()
        })
        .unwrap_or_default();

    // 4. Generate workspace name and slug.
    let ws_name = req.name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            let pattern = template.get("default_name_pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("{app} workspace");
            pattern.replace("{app}", app["name"].as_str().unwrap_or(&slug))
                   .replace("{user}", &caller_id)
                   .replace("{date}", &chrono::Utc::now().format("%Y-%m-%d").to_string())
        });

    let ws_slug = {
        let base = slug.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let suffix = &Uuid::new_v4().to_string()[..8];
        format!("{}-{}", base, suffix)
    };

    // 5. Create the workspace (= team) with origin = app.slug.
    let team = fermi_auth::teams::create_team(
        &state.db,
        &ws_name,
        &ws_slug,
        req.description.as_deref(),
        &caller_id,
        &slug,   // <-- origin = app slug
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            (StatusCode::CONFLICT, "Slug collision — please retry".into())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    let ws_id = team.id;
    let ws_id_str = ws_id.to_string();

    // 6. Seed workspace budget.
    if total_budget > 0 {
        if let Ok(ws_wallet) = get_or_create_wallet(&state.db, "workspace", &ws_id_str).await {
            let _ = credit_deposit(&state.db, ws_wallet.wallet_id, total_budget, "App workspace initial budget").await;
            let _ = sqlx::query("UPDATE teams SET workspace_budget = $1 WHERE id = $2")
                .bind(total_budget)
                .bind(ws_id)
                .execute(&state.db)
                .await;
        }
    }

    // 7. Write initial files via the workspace git manager.
    let slug_for_git = ws_slug.clone();
    let mut files_written = 0usize;
    for (path, content) in &initial_files {
        let commit_msg = format!("App provisioning: {}", path);
        match state.workspace_git.commit_file(&slug_for_git, path, content, &commit_msg) {
            Ok(_) => files_written += 1,
            Err(e) => tracing::warn!(ws_id = %ws_id, path = %path, error = %e, "Failed to write initial file during App spawn"),
        }
    }
    if files_written > 0 {
        let _ = sqlx::query(
            "UPDATE teams SET git_commit_count = $1 WHERE id = $2"
        )
        .bind(files_written as i32)
        .bind(ws_id)
        .execute(&state.db)
        .await;
    }

    // 8. Auto-hire agents.
    // We bypass hire_agent_handler (which has caller ownership/visibility checks)
    // and do a direct INSERT — the App spawn itself is the authorization event.
    let mut agents_hired = 0usize;
    for agent_name in &auto_hire {
        let agent_row = sqlx::query(
            "SELECT agent_id FROM agents WHERE agent_name = $1 AND status IN ('active', 'published', 'draft')"
        )
        .bind(agent_name)
        .fetch_optional(&state.db)
        .await;

        if let Ok(Some(row)) = agent_row {
            if let Ok(agent_id) = row.try_get::<Uuid, _>("agent_id") {
                let res = sqlx::query(
                    "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship)
                     VALUES ($1, $2, $3, 'system')
                     ON CONFLICT (workspace_id, agent_id) DO NOTHING"
                )
                .bind(ws_id)
                .bind(agent_id)
                .bind(&caller_id)
                .execute(&state.db)
                .await;
                if res.is_ok() { agents_hired += 1; }
            }
        } else {
            tracing::warn!(ws_id = %ws_id, agent = %agent_name, "Agent not found during App spawn auto-hire");
        }
    }

    // 9. Return the provisioned workspace + provenance block.
    Ok((StatusCode::CREATED, Json(json!({
        "workspace_id": ws_id,
        "workspace_slug": ws_slug,
        "name": ws_name,
        "origin": slug,
        "budget": total_budget,
        "provisioned": {
            "files_written": files_written,
            "agents_hired": agents_hired,
        }
    }))))
}

// ─── GET /api/apps/:slug/workspaces ──────────────────────────────────────────

pub async fn list_app_workspaces_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();

    // Verify the App exists.
    let _app = get_app_row(&state.db, &slug).await?;

    // Return workspaces this user is a member of, filtered to origin = slug.
    let rows = sqlx::query(
        r#"SELECT t.id, t.name, t.slug, t.description, t.owner_id,
                  t.workspace_budget, t.workspace_spent, t.origin,
                  t.created_at
           FROM teams t
           JOIN team_members m ON m.team_id = t.id
           WHERE t.origin = $1 AND m.member_id = $2
           ORDER BY t.created_at DESC"#,
    )
    .bind(&slug)
    .bind(&caller_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let workspaces: Vec<Value> = rows.iter().map(|r| json!({
        "id":          r.try_get::<Uuid, _>("id").ok(),
        "name":        r.try_get::<String, _>("name").unwrap_or_default(),
        "slug":        r.try_get::<String, _>("slug").unwrap_or_default(),
        "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
        "owner_id":    r.try_get::<String, _>("owner_id").unwrap_or_default(),
        "budget":      r.try_get::<i32, _>("workspace_budget").unwrap_or(0),
        "spent":       r.try_get::<i32, _>("workspace_spent").unwrap_or(0),
        "origin":      r.try_get::<String, _>("origin").unwrap_or_default(),
        "created_at":  r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
    })).collect();

    Ok(Json(json!({ "workspaces": workspaces, "total": workspaces.len() })))
}

// ─── POST /api/apps/:slug/publish ────────────────────────────────────────────

pub async fn publish_app_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();
    let app = get_app_row(&state.db, &slug).await?;
    let owner_id = app["owner_user_id"].as_str().unwrap_or("");
    if caller_id != owner_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Only the App owner can publish it".into()));
    }

    sqlx::query(
        "UPDATE apps SET visibility = 'public', published_at = COALESCE(published_at, NOW()) WHERE slug = $1"
    )
    .bind(&slug)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    get_app_row(&state.db, &slug).await.map(Json)
}

// ─── POST /api/apps/:slug/archive ────────────────────────────────────────────

pub async fn archive_app_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();
    let app = get_app_row(&state.db, &slug).await?;
    let owner_id = app["owner_user_id"].as_str().unwrap_or("");
    if caller_id != owner_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Only the App owner can archive it".into()));
    }

    sqlx::query(
        "UPDATE apps SET archived_at = COALESCE(archived_at, NOW()) WHERE slug = $1"
    )
    .bind(&slug)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    get_app_row(&state.db, &slug).await.map(Json)
}

// ─── GET /api/me/apps-health ─────────────────────────────────────────────────
//
// Single-query rollup of Apps the caller can see, with per-app counts
// of workspaces the caller has spawned. Replaces the dashboard's
// N+1 fetch pattern (one /api/apps + one /api/apps/:slug/workspaces
// per app) with a single round-trip.
//
// Returns one row per visible app (public + caller's own private/
// unlisted), excluding archived ones. The per-user counts come from
// a single grouped subquery over (teams, team_members), keyed by
// teams.origin.
//
// Schema dependence: only on baseline columns
//   apps.{slug, name, tagline, description, homepage_url, icon_url,
//         composition_slug, visibility, owner_user_id, archived_at,
//         created_at}
//   teams.{id, origin, created_at}
//   team_members.{team_id, member_id}
// All of these are stable foundation fields (per the apps schema
// review during this commit).

pub async fn apps_health_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();

    let rows = sqlx::query(
        r#"
        SELECT
            a.slug,
            a.name,
            a.tagline,
            a.description,
            a.homepage_url,
            a.icon_url,
            a.composition_slug,
            a.visibility,
            a.created_at,
            COALESCE(c.my_count, 0)::int        AS my_workspace_count,
            c.last_my_spawn_at
        FROM apps a
        LEFT JOIN (
            SELECT
                t.origin,
                COUNT(*)            AS my_count,
                MAX(t.created_at)   AS last_my_spawn_at
            FROM teams t
            JOIN team_members m ON m.team_id = t.id
            WHERE m.member_id = $1
            GROUP BY t.origin
        ) c ON c.origin = a.slug
        WHERE a.archived_at IS NULL
          AND (a.visibility = 'public' OR a.owner_user_id = $1)
        ORDER BY my_workspace_count DESC, a.created_at DESC
        LIMIT 100
        "#,
    )
    .bind(&caller_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let apps: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "slug":               r.try_get::<String, _>("slug").unwrap_or_default(),
                "name":               r.try_get::<String, _>("name").unwrap_or_default(),
                "tagline":            r.try_get::<Option<String>, _>("tagline").ok().flatten(),
                "description":        r.try_get::<Option<String>, _>("description").ok().flatten(),
                "homepage_url":       r.try_get::<Option<String>, _>("homepage_url").ok().flatten(),
                "icon_url":           r.try_get::<Option<String>, _>("icon_url").ok().flatten(),
                "composition_slug":   r.try_get::<Option<String>, _>("composition_slug").ok().flatten(),
                "visibility":         r.try_get::<String, _>("visibility").unwrap_or_else(|_| "public".into()),
                "my_workspace_count": r.try_get::<i32, _>("my_workspace_count").unwrap_or(0),
                "last_my_spawn_at":   r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_my_spawn_at")
                                       .ok().flatten().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "apps":  apps,
        "count": apps.len(),
    })))
}
