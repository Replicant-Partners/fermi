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
// The canonical reserved-slug list and slug validators live in
// `crate::apps::builder` so the CLI, the xamanEK app_design session, the
// fork-from-workspace flow, and this HTTP handler all share the same rules.
// Re-exported here for backwards compatibility with callers that may have
// been depending on the local symbol name.
use fermi::apps::builder::{is_reserved, validate_slug, validate_visibility};

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

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnWorkspaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub extra_budget: Option<i32>,
    pub auto_hire_override: Option<Vec<String>>,
    /// Arbitrary parameters bound to this workspace instance.
    /// Written to `.app/params.json` in the workspace git repo.
    pub params: Option<Value>,
    /// Workspace IDs this workspace depends on (upstream).
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct BatchSpawnRequest {
    pub instances: Vec<SpawnWorkspaceRequest>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Fetch an App row by slug. Returns (row as Value) or 404.
async fn get_app_row(db: &sqlx::PgPool, slug: &str) -> Result<Value, (StatusCode, String)> {
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
    let is_admin = principal.as_ref().map(|p| p.can_admin()).unwrap_or(false);
    let caller_id = principal.as_ref().map(|p| p.user_id());
    let include_archived = q.include_archived.unwrap_or(false);

    // Base owner/visibility clause.
    //   • Unauthenticated       → public only.
    //   • Authenticated non-admin → public + rows they own.
    //   • Admin                  → no owner/visibility gate (so third-party
    //                              apps stuck in `private`/`unlisted` are
    //                              still visible for moderation/support).
    let owner_clause: String = if is_admin {
        "1=1".to_string()
    } else if caller_id.is_some() {
        "(visibility = 'public' OR owner_user_id = $1)".to_string()
    } else {
        "visibility = 'public'".to_string()
    };

    let visibility_filter = match q.visibility.as_deref() {
        Some(v) => format!(" AND visibility = '{}'", v.replace('\'', "''")),
        None => String::new(),
    };
    let owner_filter = match q.owner.as_deref() {
        Some(o) => format!(" AND owner_user_id = '{}'", o.replace('\'', "''")),
        None => String::new(),
    };
    let prefix_filter = match q.slug_prefix.as_deref() {
        Some(p) => format!(
            " AND slug LIKE '{}%'",
            p.replace('\'', "''").replace('%', "\\%")
        ),
        None => String::new(),
    };
    let archived_filter = if include_archived {
        ""
    } else {
        " AND archived_at IS NULL"
    };

    let sql = format!(
        r#"SELECT id, slug, name, tagline, owner_user_id, owner_team_id,
                  homepage_url, icon_url, composition_slug, schema_slug,
                  schema_json, workspace_template, revenue_share,
                  pricing_policy, visibility, published_at, archived_at,
                  description, metadata, created_at, updated_at
           FROM apps
           WHERE {}{}{}{}{}
           ORDER BY created_at DESC LIMIT 200"#,
        owner_clause, visibility_filter, owner_filter, prefix_filter, archived_filter
    );

    // Only bind $1 when the query actually references it.
    let mut query = sqlx::query(&sql);
    if !is_admin {
        if let Some(ref uid) = caller_id {
            query = query.bind(uid);
        }
    }
    let rows = query
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Enrich rows with owner display names so the /apps catalogue and
    // admin surface can render "by <name>" without a second round-trip.
    let apps: Vec<Value> = rows.iter().map(row_to_app_json).collect();
    let owner_ids: Vec<String> = apps
        .iter()
        .filter_map(|a| a["owner_user_id"].as_str().map(String::from))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let owner_names: std::collections::HashMap<String, String> = if owner_ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        sqlx::query(
            "SELECT user_id, COALESCE(display_name, email, user_id) as name \
             FROM users WHERE user_id = ANY($1)",
        )
        .bind(&owner_ids)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| {
            (
                r.try_get::<String, _>("user_id").unwrap_or_default(),
                r.try_get::<String, _>("name").unwrap_or_default(),
            )
        })
        .collect()
    };

    let apps: Vec<Value> = apps
        .into_iter()
        .map(|mut a| {
            if let Some(oid) = a["owner_user_id"].as_str() {
                if let Some(name) = owner_names.get(oid) {
                    a.as_object_mut()
                        .unwrap()
                        .insert("owner_display_name".into(), Value::String(name.clone()));
                }
            }
            a
        })
        .collect();

    Ok(Json(json!({ "apps": apps, "total": apps.len() })))
}

// ─── POST /api/apps ──────────────────────────────────────────────────────────

pub async fn create_app_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateAppRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let owner_id = principal.user_id();

    // Slug + visibility validation lives in apps::builder so all entry points
    // (HTTP, CLI, xamanEK app_design session, fork-from-workspace) agree.
    if let Err(msg) = validate_slug(&body.slug) {
        // Reserved slugs are a conflict; everything else is a bad request.
        let status = if is_reserved(&body.slug) {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        return Err((status, msg));
    }

    let visibility = body.visibility.as_deref().unwrap_or("private");
    if let Err(msg) = validate_visibility(visibility) {
        return Err((StatusCode::BAD_REQUEST, msg));
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
            (
                StatusCode::CONFLICT,
                format!("App slug '{}' is already in use", body.slug),
            )
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, msg)
        }
    })?;

    let id: Uuid = row
        .try_get("id")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
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
        let is_admin = principal.as_ref().map(|p| p.can_admin()).unwrap_or(false);
        let caller_id = principal.as_ref().map(|p| p.user_id());
        let owner_id = app["owner_user_id"].as_str().unwrap_or("");
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
    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        "use update_app_handler_inner".into(),
    ))
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
        return Err((
            StatusCode::FORBIDDEN,
            "Only the App owner can update it".into(),
        ));
    }

    // Build a SET clause from whichever fields are present.
    // We use direct SQL so we can patch any subset cleanly.
    if let Some(ref v) = body.visibility {
        if !["private", "unlisted", "public"].contains(&v.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                "visibility must be 'private', 'unlisted', or 'public'".into(),
            ));
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
            return Err((
                StatusCode::FORBIDDEN,
                "This App is private; only the owner can spawn workspaces from it".into(),
            ));
        }
    }
    // Archived apps cannot spawn new workspaces.
    if app["archived_at"].is_string() {
        return Err((
            StatusCode::GONE,
            "This App has been archived and no longer accepts new workspaces".into(),
        ));
    }

    // 3. Parse workspace template.
    let template = &app["workspace_template"];
    let initial_budget: i32 = template
        .get("initial_budget")
        .and_then(|v| v.as_i64())
        .unwrap_or(100) as i32;
    let extra_budget: i32 = req.extra_budget.unwrap_or(0).max(0);
    let total_budget = initial_budget + extra_budget;

    let auto_hire: Vec<String> = req.auto_hire_override.unwrap_or_else(|| {
        template
            .get("auto_hire")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    });

    let initial_files: Vec<(String, String)> = template
        .get("initial_files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    let path = f.get("path")?.as_str()?.to_string();
                    let content = f.get("content")?.as_str()?.to_string();
                    Some((path, content))
                })
                .collect()
        })
        .unwrap_or_default();

    // 4. Generate workspace name and slug.
    let ws_name = req
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| {
            let pattern = template
                .get("default_name_pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("{app} workspace");
            pattern
                .replace("{app}", app["name"].as_str().unwrap_or(&slug))
                .replace("{user}", &caller_id)
                .replace("{date}", &chrono::Utc::now().format("%Y-%m-%d").to_string())
        });

    let ws_slug = {
        let base = slug
            .chars()
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
        &slug, // <-- origin = app slug
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
            let _ = credit_deposit(
                &state.db,
                ws_wallet.wallet_id,
                total_budget,
                "App workspace initial budget",
            )
            .await;
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
        match state
            .workspace_git
            .commit_file(&slug_for_git, path, content, &commit_msg)
        {
            Ok(_) => files_written += 1,
            Err(e) => {
                tracing::warn!(ws_id = %ws_id, path = %path, error = %e, "Failed to write initial file during App spawn")
            }
        }
    }
    if files_written > 0 {
        let _ = sqlx::query("UPDATE teams SET git_commit_count = $1 WHERE id = $2")
            .bind(files_written as i32)
            .bind(ws_id)
            .execute(&state.db)
            .await;
    }

    // 7b. Write params.json if params were provided.
    if let Some(ref params) = req.params {
        let params_content = serde_json::to_string_pretty(params).unwrap_or_default();
        let commit_msg = "App provisioning: params.json";
        match state.workspace_git.commit_file(
            &slug_for_git,
            ".app/params.json",
            &params_content,
            commit_msg,
        ) {
            Ok(_) => files_written += 1,
            Err(e) => {
                tracing::warn!(ws_id = %ws_id, error = %e, "Failed to write params.json during App spawn")
            }
        }
        // Also store params as a workspace output for cross-workspace reads
        let _ = sqlx::query(
            "INSERT INTO workspace_outputs (workspace_id, key, value, version, updated_at, updated_by)
             VALUES ($1, 'params', $2, 1, NOW(), $3)
             ON CONFLICT (workspace_id, key) DO UPDATE SET value = $2, updated_at = NOW()"
        )
        .bind(ws_id)
        .bind(params)
        .bind(&caller_id)
        .execute(&state.db)
        .await;
    }

    // 7c. Wire dependency edges if depends_on was specified.
    let mut deps_wired = 0usize;
    if let Some(ref depends_on) = req.depends_on {
        for upstream_id_str in depends_on {
            if let Ok(upstream_uuid) = upstream_id_str.parse::<Uuid>() {
                let res = sqlx::query(
                    "INSERT INTO workspace_dependencies (upstream_id, downstream_id, dependency_type)
                     VALUES ($1, $2, 'output')
                     ON CONFLICT (upstream_id, downstream_id) DO NOTHING"
                )
                .bind(upstream_uuid)
                .bind(ws_id)
                .execute(&state.db)
                .await;
                if res.is_ok() {
                    deps_wired += 1;
                }
            }
        }
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
                     ON CONFLICT (workspace_id, agent_id) DO NOTHING",
                )
                .bind(ws_id)
                .bind(agent_id)
                .bind(&caller_id)
                .execute(&state.db)
                .await;
                if res.is_ok() {
                    agents_hired += 1;
                }
            }
        } else {
            tracing::warn!(ws_id = %ws_id, agent = %agent_name, "Agent not found during App spawn auto-hire");
        }
    }

    // 9. Return the provisioned workspace + provenance block.
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "workspace_id": ws_id,
            "workspace_slug": ws_slug,
            "name": ws_name,
            "origin": slug,
            "budget": total_budget,
            "provisioned": {
                "files_written": files_written,
                "agents_hired": agents_hired,
                "dependencies_wired": deps_wired,
                "has_params": req.params.is_some(),
            }
        })),
    ))
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
    //
    // LEFT JOIN fermi_forecasts so the response carries `forecast_id` for
    // workspace-backed forecasts. NULL for non-fermi-forecast Apps, harmless.
    // This is the link the console uses to load the FPL when opening a
    // workspace from the dashboard.
    let rows = sqlx::query(
        r#"SELECT t.id, t.name, t.slug, t.description, t.owner_id,
                  t.workspace_budget, t.workspace_spent, t.origin,
                  t.created_at,
                  f.id::text AS forecast_id
           FROM teams t
           JOIN team_members m ON m.team_id = t.id
           LEFT JOIN fermi_forecasts f ON f.workspace_id = t.id
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
        "forecast_id": r.try_get::<Option<String>, _>("forecast_id").ok().flatten(),
    })).collect();

    Ok(Json(
        json!({ "workspaces": workspaces, "total": workspaces.len() }),
    ))
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
        return Err((
            StatusCode::FORBIDDEN,
            "Only the App owner can publish it".into(),
        ));
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
        return Err((
            StatusCode::FORBIDDEN,
            "Only the App owner can archive it".into(),
        ));
    }

    sqlx::query("UPDATE apps SET archived_at = COALESCE(archived_at, NOW()) WHERE slug = $1")
        .bind(&slug)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    get_app_row(&state.db, &slug).await.map(Json)
}

// ─── POST /api/apps/:slug/sync-auto-hire ─────────────────────────────────────
//
// Reconciles already-spawned workspaces of an App with the current
// `workspace_template.auto_hire` list. For every existing workspace of
// this App, ensures each agent in auto_hire has a corresponding
// `workspace_agents` row (system relationship). Idempotent —
// re-runs are no-ops once everything is in sync.
//
// Use case: the auto_hire list is changed after workspaces have already
// been spawned. Without this endpoint, those workspaces would never
// pick up the newly-added agents short of a manual hire-per-workspace
// loop. This is the batch path. Future: also add an opt-in
// `dry_run: true` for previewing diffs.
//
// Auth: caller must be the App owner OR a platform admin OR own at
// least one workspace within this App. The last branch is for curated
// platform apps (`owner_user_id = "sys"`) where users who have spawned
// workspaces should be able to reconcile their fleet with the App's
// current auto_hire — they have skin in the game and the operation is
// idempotent + only adds curated agents from the manifest.
pub async fn sync_auto_hire_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();
    let app = get_app_row(&state.db, &slug).await?;
    let owner_id = app["owner_user_id"].as_str().unwrap_or("");

    let is_owner = caller_id == owner_id;
    let is_admin = principal.can_admin();
    let has_workspace_in_app: bool = if !is_owner && !is_admin {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(
                SELECT 1 FROM teams t
                JOIN team_members m ON m.team_id = t.id
                WHERE t.origin = $1 AND m.member_id = $2
             )",
        )
        .bind(&slug)
        .bind(&caller_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(false)
    } else {
        false
    };

    if !is_owner && !is_admin && !has_workspace_in_app {
        return Err((
            StatusCode::FORBIDDEN,
            "Sync auto-hire requires App owner, platform admin, \
             or membership in at least one workspace of this App"
                .into(),
        ));
    }

    // Pull the current auto_hire list from the App's workspace_template.
    let auto_hire: Vec<String> = app["workspace_template"]
        .get("auto_hire")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if auto_hire.is_empty() {
        return Ok(Json(json!({
            "ok": true,
            "note": "auto_hire is empty — nothing to sync",
            "workspaces_visited": 0,
            "hires_added": 0,
        })));
    }

    // Resolve all agent_name → agent_id in ONE query (instead of N).
    // Names not in the registry are reported but don't fail the batch.
    let agent_rows = sqlx::query(
        "SELECT agent_name, agent_id FROM agents
         WHERE agent_name = ANY($1) AND status IN ('active','published','draft')",
    )
    .bind(&auto_hire)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut agent_ids: Vec<Uuid> = Vec::with_capacity(agent_rows.len());
    let mut found_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in &agent_rows {
        if let (Ok(name), Ok(id)) = (
            r.try_get::<String, _>("agent_name"),
            r.try_get::<Uuid, _>("agent_id"),
        ) {
            agent_ids.push(id);
            found_names.insert(name);
        }
    }
    let skipped_agents: Vec<String> = auto_hire
        .iter()
        .filter(|n| !found_names.contains(*n))
        .cloned()
        .collect();

    // List every workspace of this App in one query.
    let workspace_ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM teams WHERE origin = $1")
        .bind(&slug)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let workspaces_visited = workspace_ids.len();

    // Bulk-upsert all (workspace, agent) pairs in ONE INSERT.
    //
    // Postgres `unnest` + cross-product is the standard pattern for bulk
    // inserts without prepared-statement multi-row VALUES (which sqlx
    // doesn't support natively for variable-length arrays). We build two
    // parallel arrays — one per pair — and unnest them together.
    //
    // For a fleet of 60 workspaces × 12 agents = 720 pairs, this is ONE
    // network roundtrip instead of 720. Latency drops from minutes to
    // sub-second.
    //
    // RETURNING workspace_id lets us count per-workspace hires_added so
    // the response detail isn't lost in the bulk shape.
    let (hires_added, per_workspace): (i64, Vec<Value>) =
        if !workspace_ids.is_empty() && !agent_ids.is_empty() {
            // Build the cross-product of (workspace_id, agent_id) pairs.
            let mut ws_col: Vec<Uuid> = Vec::with_capacity(workspace_ids.len() * agent_ids.len());
            let mut ag_col: Vec<Uuid> = Vec::with_capacity(workspace_ids.len() * agent_ids.len());
            for ws in &workspace_ids {
                for ag in &agent_ids {
                    ws_col.push(*ws);
                    ag_col.push(*ag);
                }
            }

            let inserted_rows = sqlx::query(
                "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship)
             SELECT ws, ag, $3, 'system'
             FROM unnest($1::uuid[], $2::uuid[]) AS pairs(ws, ag)
             ON CONFLICT (workspace_id, agent_id) DO NOTHING
             RETURNING workspace_id",
            )
            .bind(&ws_col)
            .bind(&ag_col)
            .bind(&caller_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("bulk hire insert failed: {}", e),
                )
            })?;

            // Aggregate per-workspace.
            let mut counts: std::collections::HashMap<Uuid, i64> = std::collections::HashMap::new();
            for r in &inserted_rows {
                if let Ok(ws) = r.try_get::<Uuid, _>("workspace_id") {
                    *counts.entry(ws).or_insert(0) += 1;
                }
            }
            let total = inserted_rows.len() as i64;
            let details: Vec<Value> = counts
                .iter()
                .map(|(ws, n)| json!({ "workspace_id": ws, "hires_added": n }))
                .collect();

            tracing::info!(
                app_slug = %slug,
                workspaces = workspaces_visited,
                agents = agent_ids.len(),
                hires_added = total,
                "sync_auto_hire: bulk insert complete"
            );

            (total, details)
        } else {
            (0, Vec::new())
        };

    Ok(Json(json!({
        "ok": true,
        "app_slug": slug,
        "workspaces_visited": workspaces_visited,
        "hires_added": hires_added,
        "auto_hire_agents": auto_hire,
        "skipped_agents_not_in_registry": skipped_agents,
        "details": per_workspace,
    })))
}

// ─── GET /api/apps/:slug/schema ──────────────────────────────────────────────
//
// Returns the App's schema_json — the machine-readable action grammar
// that tells UI builders, CLI generators, and MCP clients exactly what
// action types the strategist agent can emit, their field shapes, and
// how to parse the __ACTION__ blocks from companion responses.
//
// Public for public Apps (no auth required). Private Apps require auth.

pub async fn get_app_schema_handler(
    State(state): State<AppState>,
    principal: Option<AuthPrincipal>,
    Path(slug): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let app = get_app_row(&state.db, &slug).await?;

    // Visibility check: private/unlisted requires auth + ownership or admin
    let visibility = app["visibility"].as_str().unwrap_or("private");
    if visibility != "public" {
        let caller_id = principal.as_ref().map(|p| p.user_id());
        let owner_id = app["owner_user_id"].as_str().unwrap_or("");
        let is_admin = principal.as_ref().map(|p| p.can_admin()).unwrap_or(false);
        if caller_id.as_deref() != Some(owner_id) && !is_admin {
            return Err((
                StatusCode::FORBIDDEN,
                "Not authorised to view this App's schema".into(),
            ));
        }
    }

    let schema = app.get("schema_json")
        .cloned()
        .filter(|v| !v.is_null())
        .unwrap_or_else(|| json!({
            "schema_slug": app["schema_slug"],
            "note": "No schema_json declared for this App. Add schema_json to the App manifest to describe the action grammar.",
        }));

    Ok(Json(json!({
        "app_slug":   slug,
        "app_name":   app["name"],
        "schema_slug": app["schema_slug"],
        "schema":     schema,
    })))
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
    // Admins see every app (public/unlisted/private) so their dashboard
    // Apps block reflects the full platform, including third-party
    // developers' in-progress work.
    let visibility_clause = if principal.can_admin() {
        "TRUE"
    } else {
        "(a.visibility = 'public' OR a.owner_user_id = $1)"
    };

    let sql = format!(
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
            a.owner_user_id,
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
          AND {visibility_clause}
        ORDER BY my_workspace_count DESC, a.created_at DESC
        LIMIT 100
        "#,
        visibility_clause = visibility_clause,
    );

    let rows = sqlx::query(&sql)
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
                "owner_user_id":      r.try_get::<String, _>("owner_user_id").unwrap_or_default(),
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

// ─── POST /api/apps/:slug/workspaces/batch ───────────────────────────────────
/// Batch-spawn multiple workspaces from the same App template.
/// Each instance gets its own name, params, and dependency edges.
/// Returns all spawned workspace IDs.
///
/// This is the mechanism for instantiating prediction portfolios:
/// 32 team priors, 50 state forecasts, 100 earnings forecasts, etc.
pub async fn batch_spawn_workspaces_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(slug): Path<String>,
    Json(req): Json<BatchSpawnRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    if req.instances.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "instances array is empty".into()));
    }
    if req.instances.len() > 200 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Maximum 200 instances per batch".into(),
        ));
    }

    let caller_id = principal.user_id();

    // Verify the App exists and is spawnable
    let app = get_app_row(&state.db, &slug).await?;
    let visibility = app["visibility"].as_str().unwrap_or("private");
    if visibility == "private" {
        let owner_id = app["owner_user_id"].as_str().unwrap_or("");
        if caller_id != owner_id && !principal.can_admin() {
            return Err((StatusCode::FORBIDDEN, "This App is private".into()));
        }
    }
    if app["archived_at"].is_string() {
        return Err((StatusCode::GONE, "This App has been archived".into()));
    }

    // Parse template once
    let template = &app["workspace_template"];
    let base_budget: i32 = template
        .get("initial_budget")
        .and_then(|v| v.as_i64())
        .unwrap_or(100) as i32;
    let default_auto_hire: Vec<String> = template
        .get("auto_hire")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let initial_files: Vec<(String, String)> = template
        .get("initial_files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    let path = f.get("path")?.as_str()?.to_string();
                    let content = f.get("content")?.as_str()?.to_string();
                    Some((path, content))
                })
                .collect()
        })
        .unwrap_or_default();
    let name_pattern = template
        .get("default_name_pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("{app} workspace");

    let mut results: Vec<Value> = Vec::with_capacity(req.instances.len());
    let mut errors: Vec<Value> = Vec::new();

    for (idx, instance) in req.instances.iter().enumerate() {
        // Generate workspace name
        let ws_name = instance
            .name
            .as_deref()
            .filter(|n| !n.trim().is_empty())
            .map(|n| n.to_string())
            .unwrap_or_else(|| {
                name_pattern
                    .replace("{app}", app["name"].as_str().unwrap_or(&slug))
                    .replace("{user}", &caller_id)
                    .replace("{date}", &chrono::Utc::now().format("%Y-%m-%d").to_string())
            });

        let ws_slug = {
            let base = slug
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect::<String>();
            let suffix = &Uuid::new_v4().to_string()[..8];
            format!("{}-{}", base, suffix)
        };

        // Create the workspace (team)
        let team = match fermi_auth::teams::create_team(
            &state.db,
            &ws_name,
            &ws_slug,
            instance.description.as_deref(),
            &caller_id,
            &slug,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                errors.push(json!({
                    "index": idx,
                    "name": ws_name,
                    "error": format!("{}", e),
                }));
                continue;
            }
        };

        let ws_id = team.id;
        let total_budget = base_budget + instance.extra_budget.unwrap_or(0).max(0);

        // Budget
        if total_budget > 0 {
            if let Ok(ws_wallet) =
                get_or_create_wallet(&state.db, "workspace", &ws_id.to_string()).await
            {
                let _ = credit_deposit(
                    &state.db,
                    ws_wallet.wallet_id,
                    total_budget,
                    "Batch spawn budget",
                )
                .await;
                let _ = sqlx::query("UPDATE teams SET workspace_budget = $1 WHERE id = $2")
                    .bind(total_budget)
                    .bind(ws_id)
                    .execute(&state.db)
                    .await;
            }
        }

        // Initial files
        let mut files_written = 0usize;
        for (path, content) in &initial_files {
            if state
                .workspace_git
                .commit_file(
                    &ws_slug,
                    path,
                    content,
                    &format!("Batch provisioning: {}", path),
                )
                .is_ok()
            {
                files_written += 1;
            }
        }

        // Params
        if let Some(ref params) = instance.params {
            let params_content = serde_json::to_string_pretty(params).unwrap_or_default();
            if state
                .workspace_git
                .commit_file(
                    &ws_slug,
                    ".app/params.json",
                    &params_content,
                    "Batch provisioning: params.json",
                )
                .is_ok()
            {
                files_written += 1;
            }
            let _ = sqlx::query(
                "INSERT INTO workspace_outputs (workspace_id, key, value, version, updated_at, updated_by)
                 VALUES ($1, 'params', $2, 1, NOW(), $3)
                 ON CONFLICT (workspace_id, key) DO UPDATE SET value = $2, updated_at = NOW()"
            ).bind(ws_id).bind(params).bind(&caller_id).execute(&state.db).await;
        }

        // Dependencies
        let mut deps_wired = 0usize;
        if let Some(ref depends_on) = instance.depends_on {
            for upstream_str in depends_on {
                if let Ok(upstream_uuid) = upstream_str.parse::<Uuid>() {
                    if sqlx::query(
                        "INSERT INTO workspace_dependencies (upstream_id, downstream_id, dependency_type)
                         VALUES ($1, $2, 'output') ON CONFLICT DO NOTHING"
                    ).bind(upstream_uuid).bind(ws_id).execute(&state.db).await.is_ok() {
                        deps_wired += 1;
                    }
                }
            }
        }

        // Auto-hire agents
        let auto_hire = instance
            .auto_hire_override
            .as_ref()
            .unwrap_or(&default_auto_hire);
        let mut agents_hired = 0usize;
        for agent_name in auto_hire {
            if let Ok(Some(row)) = sqlx::query(
                "SELECT agent_id FROM agents WHERE agent_name = $1 AND status IN ('active', 'published', 'draft')"
            ).bind(agent_name).fetch_optional(&state.db).await {
                if let Ok(agent_id) = row.try_get::<Uuid, _>("agent_id") {
                    if sqlx::query(
                        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship)
                         VALUES ($1, $2, $3, 'system') ON CONFLICT DO NOTHING"
                    ).bind(ws_id).bind(agent_id).bind(&caller_id).execute(&state.db).await.is_ok() {
                        agents_hired += 1;
                    }
                }
            }
        }

        results.push(json!({
            "index": idx,
            "workspace_id": ws_id,
            "workspace_slug": ws_slug,
            "name": ws_name,
            "provisioned": {
                "files_written": files_written,
                "agents_hired": agents_hired,
                "dependencies_wired": deps_wired,
                "has_params": instance.params.is_some(),
            }
        }));
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "app": slug,
            "spawned": results.len(),
            "errors": errors.len(),
            "workspaces": results,
            "failed": errors,
        })),
    ))
}
