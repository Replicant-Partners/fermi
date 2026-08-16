//! Agent handlers — CRUD, listing, import, versions, avatar, catalogue.

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge, get_or_create_wallet, rbac, visibility::AccessLevel, AuthPrincipal, ObjectType,
    Visibility,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::{Agent, AgentUpdate, Episode};

use crate::agent_economics::{measured_exec_stats, MeasuredExecStats};
use crate::{
    resolve_agent, resolve_agent_card, AppState, GeminiContent, GeminiGenerationConfig, GeminiPart,
    GeminiRequest, GeminiResponse,
};

// ─── Shared visibility helpers ────────────────────────────────

/// Map an `Agent`'s persisted `visibility` + `status` to the substrate
/// [`Visibility`] enum used by [`fermi_auth::rbac`].
///
/// An agent is only "reachable" as public when *both*
/// `visibility = 'public'` and `status = 'published'` — a draft with
/// `visibility='public'` is still author-only. This function bakes
/// that rule in one place so every handler (list, detail, execute,
/// wallet, funding) gets the same answer.
///
/// `pub(crate)` so the HTML page handlers in `handlers::pages` can gate
/// the `/agent/:agent_id/*` shells on exactly the same rule the JSON API
/// uses. A fourth hand-rolled copy of this two-line rule is how the page
/// and the API end up disagreeing about what is public.
pub(crate) fn agent_effective_visibility(agent: &Agent) -> Visibility {
    if agent.status == "published" && agent.visibility == "public" {
        Visibility::Public
    } else if agent.visibility == "unlisted" {
        Visibility::Shared
    } else {
        Visibility::Private
    }
}

/// Sync ACL check used by list filters. `visible_sync` is O(1) —
/// admin / owner / public only, no share/team ACL. Detail endpoints
/// use the async `rbac::require*` path which does the full ladder.
///
/// See `fermi_auth::rbac::visible_sync` for the semantics.
fn agent_visible_to_caller(agent: &Agent, caller: Option<&AuthPrincipal>) -> bool {
    let vis = agent_effective_visibility(agent);
    match caller {
        Some(p) => rbac::visible_sync(p, agent.owner_id.as_deref(), vis),
        None => rbac::visible_sync_anon(vis),
    }
}

/// Build the rich agent JSON object served by both list_agents and
/// get_agent_handler. Kept in one place so the client contract is
/// identical whether the caller lists the catalogue or fetches a
/// single agent by name.
/// Merge a DB agent row with its on-disk `agent_card.json` (when one
/// exists) into the catalogue's canonical JSON shape.
///
/// `pub(crate)` so the Ecology lens builds specimens from exactly the same
/// merge the catalogue uses — taxonomy, valence, domain knowledge and the
/// accepts/produces interfaces all live in the card, not the table, so a
/// second hand-rolled merge would quietly diverge.
///
/// `measured` supplies execution stats derived from `episodes` (see
/// [`crate::agent_economics`] for why the `agents` row can't be trusted for
/// them). Pass `None` only where run stats are irrelevant to the caller;
/// the output is then tagged `source: "agents_row"` so a consumer can tell
/// an unmeasured zero from a measured one.
pub(crate) fn build_agent_json(
    state: &AppState,
    agent: &Agent,
    owner_display: Option<String>,
    workspace_count: i64,
    measured: Option<&MeasuredExecStats>,
) -> Value {
    let card = state.registry.get(&agent.agent_name).ok();
    let card_json = card.as_ref().and_then(|_c| {
        let path = format!("agents/curated/{}/agent_card.json", agent.agent_name);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    });

    let mut agent_val = json!({
        "agent_id": agent.agent_name,
        "uuid": agent.agent_id,
        "display_alias": agent.display_alias.as_deref().unwrap_or(""),
        "agent_type": agent.agent_type,
        "version": agent.version,
        "tier": agent.tier,
        "description": agent.description.as_deref().unwrap_or(""),
        "author": agent.author,
        "model": agent.model,
        "llm_provider": agent.llm_provider,
        "model_ladder": agent.model_ladder,
        "min_tier": agent.min_tier,
        "capability_gates": agent.capability_gates,
        "tags": agent.tags,
        "sample_queries": agent.sample_queries,
        "visibility": agent.visibility,
        "owner_id": agent.owner_id.as_deref().unwrap_or(""),
        "owner_display_name": owner_display,
        "system_prompt": agent.system_prompt.as_deref().unwrap_or(""),
        "status": agent.status,
        "fork_pricing": agent.fork_pricing,
        "forked_from": agent.forked_from,
        "fork_count": agent.fork_count,
        "accepts": agent.accepts,
        "produces": agent.produces,
        "workflow_template": agent.workflow_template,
        "prompt_template": agent.prompt_template,
        // The agent's own declaration of how it expects to be invoked and
        // what it will hand back. Withholding it forces a caller to
        // hardcode assumptions keyed on specific agent ids, which locks out
        // every agent that caller has never heard of.
        "fermi_contract": agent.fermi_contract,
        "output_contract": agent.output_contract,
        "requires_secrets": agent.requires_secrets,
        "model_params": agent.model_params,
        "capabilities": {
            "executor": agent.executor_type,
            "model": agent.model,
            "temperature": agent.temperature,
            "mcp_tools": card.as_ref().map(|c| c.capabilities.mcp_tools.iter().map(|t| json!({"name": t.name, "description": t.description})).collect::<Vec<_>>()).unwrap_or_default(),
            "skills": card.as_ref().map(|c| c.capabilities.skills.clone()).unwrap_or_default(),
        },
        "ontology_stats": {
            "last_updated": agent.last_consolidated_at,
            "current_commit": agent.current_ontology_commit,
        },
        "execution_stats": match measured {
            Some(m) => json!({
                "total_executions": m.executions,
                "successful_executions": m.successful,
                "failed_executions": m.failed,
                "total_cost_usd": m.cost_usd,
                "tokens_used": m.tokens_used,
                "avg_execution_time_ms": m.avg_execution_time_ms,
                // Non-zero means `total_cost_usd` is a partial sum. A
                // consumer presenting spend should say so rather than
                // pass an incomplete figure off as a total.
                "episodes_missing_cost": m.episodes_missing_cost,
                "source": "episodes",
            }),
            // No rollup was loaded. Report the row's counters but label
            // them, so a consumer seeing zeros can tell "never ran" apart
            // from "nobody measured".
            None => json!({
                "total_executions": agent.total_executions,
                "successful_executions": agent.successful_executions,
                "failed_executions": agent.failed_executions,
                "total_cost_usd": agent.total_cost_usd,
                "avg_execution_time_ms": agent.avg_execution_time_ms,
                "source": "agents_row",
            }),
        },
        "dreaming": {
            "budget_credits": agent.dreaming_budget_credits,
            "credits_used": agent.dreaming_credits_used,
            "credits_remaining": agent.dreaming_budget_credits - agent.dreaming_credits_used,
        },
        "workspace_count": workspace_count,
        "embedding": {
            "provider": agent.embedding_provider,
            "model": agent.embedding_model,
            "dimension": agent.embedding_dimension,
        },
        "source": "database",
    });

    // Overlay rich fields from filesystem card (if any)
    if let Some(cj) = &card_json {
        if let Some(obj) = agent_val.as_object_mut() {
            if let Some(meta) = cj.get("metadata") {
                obj.insert("metadata".to_string(), meta.clone());
            }
            if let Some(perf) = cj.get("performance") {
                obj.insert("performance".to_string(), perf.clone());
            }
            if let Some(usage) = cj.get("usage") {
                obj.insert("usage".to_string(), usage.clone());
            }
            if let Some(wallet) = cj.get("wallet") {
                obj.insert("wallet".to_string(), wallet.clone());
            }
            if let Some(onto) = cj.get("ontology_stats") {
                let mut merged = obj.get("ontology_stats").cloned().unwrap_or(json!({}));
                if let (Some(m), Some(o)) = (merged.as_object_mut(), onto.as_object()) {
                    for (k, v) in o {
                        if m.get(k).map(|existing| existing.is_null()).unwrap_or(true) {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                }
                obj.insert("ontology_stats".to_string(), merged);
            }
        }
    }

    agent_val
}

#[derive(Debug, Deserialize)]
pub struct ListAgentsParams {
    search: Option<String>,
    tag: Option<String>,
    tags: Option<String>, // comma-separated, OR semantics
    sort: Option<String>, // "newest", "executions", "name"
    page: Option<usize>,
    limit: Option<usize>,
    /// Restrict to members of an orchestra (`fermi`, `xaman_ek`).
    /// Resolved against the roster views from mig-172 — the same
    /// predicate `/api/orchestras/:name/members` and the agent Manage
    /// page use. Prefer this over `?tag=fermi-orchestra`, which only
    /// matches a hand-maintained metadata tag that nothing in the
    /// approval flow writes.
    orchestra: Option<String>,
}

pub async fn list_agents(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    Query(params): Query<ListAgentsParams>,
) -> Json<Value> {
    // Admins see everything (including drafts/private) so third-party
    // agents made by external users are discoverable in the catalogue for
    // moderation and support. Owners still see their own; everyone else
    // sees only published + public rows. Uses `rbac::visible_sync` —
    // no per-agent DB roundtrip, since list filters can't afford the
    // O(N) share/team lookup. Detail endpoints use the async
    // `rbac::require_view` for the full ACL.
    let caller_ref = caller.as_ref().map(|Extension(p)| p);

    // Batch-load workspace membership counts for all agents
    let workspace_counts: std::collections::HashMap<uuid::Uuid, i64> =
        sqlx::query("SELECT agent_id, COUNT(*) as cnt FROM workspace_agents GROUP BY agent_id")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| (r.get::<uuid::Uuid, _>("agent_id"), r.get::<i64, _>("cnt")))
            .collect();

    // Primary: database (filter out test agents + apply visibility)
    if let Ok(db_agents) = state.memory_store.list_agents().await {
        let real_agents: Vec<_> = db_agents
            .into_iter()
            .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
            .filter(|a| agent_visible_to_caller(a, caller_ref))
            .collect();

        // Apply search filter
        let mut filtered: Vec<_> = if let Some(ref search) = params.search {
            let q = search.to_lowercase();
            real_agents
                .into_iter()
                .filter(|a| {
                    a.agent_name.to_lowercase().contains(&q)
                        || a.display_alias
                            .as_deref()
                            .map(|d| d.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || a.description
                            .as_deref()
                            .map(|d| d.to_lowercase().contains(&q))
                            .unwrap_or(false)
                        || a.tags.iter().any(|t| t.to_lowercase().contains(&q))
                })
                .collect()
        } else {
            real_agents
        };

        // Apply orchestra-membership filter. Authoritative: reads the
        // roster view, so an agent approved into Fermi shows up here the
        // moment it's a member — no tag bookkeeping required.
        if let Some(ref orchestra) = params.orchestra {
            match crate::handlers::orchestras::orchestra_view_name(orchestra) {
                Some(view) => {
                    // `view` is a compile-time constant from ORCHESTRAS,
                    // never caller input — safe to interpolate.
                    let member_ids: std::collections::HashSet<uuid::Uuid> =
                        sqlx::query_scalar(&format!("SELECT agent_id FROM public.{}", view))
                            .fetch_all(&state.db)
                            .await
                            .unwrap_or_default()
                            .into_iter()
                            .collect();
                    filtered.retain(|a| member_ids.contains(&a.agent_id));
                }
                // Unknown orchestra name: return empty rather than
                // silently ignoring the filter and dumping the catalogue.
                None => filtered.clear(),
            }
        }

        // Apply tag filter (single tag)
        if let Some(ref tag) = params.tag {
            let t = tag.to_lowercase();
            filtered.retain(|a| a.tags.iter().any(|at| at.to_lowercase() == t));
        }

        // Apply multi-tag filter (comma-separated, OR semantics)
        if let Some(ref tags_str) = params.tags {
            let tag_list: Vec<String> = tags_str
                .split(',')
                .map(|t| t.trim().to_lowercase())
                .collect();
            if !tag_list.is_empty() {
                filtered.retain(|a| {
                    a.tags
                        .iter()
                        .any(|at| tag_list.contains(&at.to_lowercase()))
                });
            }
        }

        // Sort
        match params.sort.as_deref() {
            Some("executions") => {
                filtered.sort_by(|a, b| b.total_executions.cmp(&a.total_executions))
            }
            Some("name") => filtered.sort_by(|a, b| {
                let na = a.display_alias.as_deref().unwrap_or(&a.agent_name);
                let nb = b.display_alias.as_deref().unwrap_or(&b.agent_name);
                na.to_lowercase().cmp(&nb.to_lowercase())
            }),
            _ => filtered.sort_by(|a, b| b.agent_id.cmp(&a.agent_id)), // newest first (UUID v4 ~ creation order for DB-inserted)
        }

        let total = filtered.len();
        let limit = params.limit.unwrap_or(50).min(200);
        let page = params.page.unwrap_or(1).max(1);
        let offset = (page - 1) * limit;
        let pages = (total + limit - 1) / limit.max(1);

        let page_agents: Vec<_> = filtered.into_iter().skip(offset).take(limit).collect();

        // Batch-load owner display names
        let owner_ids: Vec<String> = page_agents
            .iter()
            .filter_map(|a| a.owner_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let owner_names: std::collections::HashMap<String, String> = if !owner_ids.is_empty() {
            sqlx::query(
                "SELECT user_id, COALESCE(display_name, email, user_id) as name FROM users WHERE user_id = ANY($1)",
            )
            .bind(&owner_ids)
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
            .iter()
            .map(|r| (r.get::<String, _>("user_id"), r.get::<String, _>("name")))
            .collect()
        } else {
            std::collections::HashMap::new()
        };

        // An explicit filter that matches nothing is a legitimate empty
        // answer, not a signal that the catalogue is unavailable. Without
        // this, a zero-match filter fell through to the filesystem
        // fallback below, which ignores every filter and returns the
        // whole `agents/curated` directory — so `?orchestra=fermi` on an
        // empty roster (or a typo'd `?tag=`) would answer with a pile of
        // unrelated agents rather than `[]`.
        let filter_requested = params.search.is_some()
            || params.tag.is_some()
            || params.tags.is_some()
            || params.orchestra.is_some();

        // Real run stats for this page, measured from `episodes`. Scoped to
        // the page so the query stays O(1) as the catalogue grows.
        let page_uuids: Vec<Uuid> = page_agents.iter().map(|a| a.agent_id).collect();
        let exec_stats = measured_exec_stats(&state.db, &page_uuids).await;

        if !page_agents.is_empty() || total > 0 || filter_requested {
            let agents: Vec<Value> = page_agents
                .iter()
                .map(|a| {
                    let owner_display = a
                        .owner_id
                        .as_deref()
                        .and_then(|oid| owner_names.get(oid))
                        .cloned();
                    let ws_count = workspace_counts.get(&a.agent_id).copied().unwrap_or(0);
                    build_agent_json(
                        &state,
                        a,
                        owner_display,
                        ws_count,
                        exec_stats.get(&a.agent_id),
                    )
                })
                .collect();
            return Json(json!({
                "agents": agents,
                "total": total,
                "page": page,
                "limit": limit,
                "pages": pages,
            }));
        }
    }

    // Fallback: filesystem
    let agents_dir = "agents/curated";
    let mut agents = Vec::new();
    if let Ok(entries) = std::fs::read_dir(agents_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let card_path = path.join("agent_card.json");
                if card_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&card_path) {
                        if let Ok(card) = serde_json::from_str::<Value>(&content) {
                            agents.push(card);
                        }
                    }
                }
            }
        }
    }
    let fs_total = agents.len();
    Json(json!({ "agents": agents, "total": fs_total, "page": 1, "limit": fs_total, "pages": 1 }))
}

/// GET /api/agents/:agent_id — single agent detail lookup.
///
/// Previously the agent detail page pulled the full list and filtered
/// client-side, which meant agents beyond the first 200 rows — or agents
/// whose visibility was `private`/`draft` (third-party author's own work,
/// admin moderating) — rendered as "Specimen not found". This endpoint
/// resolves by name and applies the same visibility rules as the list
/// handler, so owners and admins can always deep-link to their agents.
pub async fn get_agent_handler(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = resolve_agent(&state, &agent_id).await?;

    // v0.10.5: authenticated callers go through the full RBAC ladder
    // (admin → owner → public → direct share → team share). Anonymous
    // callers only see the public+published slice. Both branches
    // return 404 (not 403) on denial so we don't leak existence of
    // private agents through the response code.
    let owner_id = agent.owner_id.clone().unwrap_or_default();
    let vis = agent_effective_visibility(&agent);
    match caller.as_ref() {
        Some(Extension(p)) => {
            rbac::require_view(
                &state.db,
                p,
                ObjectType::Agent,
                &agent.agent_id.to_string(),
                &owner_id,
                vis,
            )
            .await?;
        }
        None => {
            if !rbac::visible_sync_anon(vis) {
                return Err((
                    StatusCode::NOT_FOUND,
                    format!("Agent '{}' not found", agent_id),
                ));
            }
        }
    }

    // Workspace count for this agent — keeps parity with list_agents.
    let workspace_count: i64 =
        sqlx::query("SELECT COUNT(*) as cnt FROM workspace_agents WHERE agent_id = $1")
            .bind(agent.agent_id)
            .fetch_one(&state.db)
            .await
            .map(|r| r.try_get::<i64, _>("cnt").unwrap_or(0))
            .unwrap_or(0);

    // Owner display name (best-effort).
    let owner_display = if let Some(ref oid) = agent.owner_id {
        sqlx::query(
            "SELECT COALESCE(display_name, email, user_id) as name FROM users WHERE user_id = $1",
        )
        .bind(oid)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String, _>("name").ok())
    } else {
        None
    };

    // Measured run stats — keeps parity with list_agents, which is the
    // point: detail and list disagreeing about how many times an agent has
    // run is exactly the kind of drift that hides a dead column.
    let exec_stats = measured_exec_stats(&state.db, &[agent.agent_id]).await;

    Ok(Json(build_agent_json(
        &state,
        &agent,
        owner_display,
        workspace_count,
        exec_stats.get(&agent.agent_id),
    )))
}

/// Public endpoint: serves cached avatar only (no generation)
pub async fn get_cached_avatar(
    State(state): State<crate::AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Try DB first
    if let Ok(Some(row)) = sqlx::query("SELECT avatar_json FROM agent_avatars WHERE agent_id = $1")
        .bind(&agent_id)
        .fetch_optional(&state.db)
        .await
    {
        let avatar: Value = row.get("avatar_json");
        return Ok(Json(avatar));
    }

    // Fallback: try filesystem (migrate to DB on hit)
    let cache_path = format!("avatars_cache/{}.json", agent_id);
    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached_data) = serde_json::from_str::<Value>(&cached) {
            // Persist to DB for next deploy
            let _ = sqlx::query(
                "INSERT INTO agent_avatars (agent_id, avatar_json)
                 VALUES ($1, $2) ON CONFLICT (agent_id) DO NOTHING",
            )
            .bind(&agent_id)
            .bind(&cached_data)
            .execute(&state.db)
            .await;
            return Ok(Json(cached_data));
        }
    }
    Err((
        StatusCode::NOT_FOUND,
        "No cached avatar. Use POST /api/agents/:id/avatar/generate to create one.".to_string(),
    ))
}

/// Protected endpoint: generates avatar via Gemini, charges credits
pub async fn generate_avatar(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Check cache first (free) — DB then filesystem
    let cache_dir = "avatars_cache";
    std::fs::create_dir_all(cache_dir).ok();
    let cache_path = format!("{}/{}.json", cache_dir, agent_id);

    if let Ok(Some(row)) = sqlx::query("SELECT avatar_json FROM agent_avatars WHERE agent_id = $1")
        .bind(&agent_id)
        .fetch_optional(&state.db)
        .await
    {
        let avatar: Value = row.get("avatar_json");
        return Ok(Json(avatar));
    }

    if let Ok(cached) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached_data) = serde_json::from_str::<Value>(&cached) {
            return Ok(Json(cached_data));
        }
    }

    // Charge credits for generation
    let user_id = principal.user_id();
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.avatar_generate,
        "avatar_generate",
        &format!("Avatar generation for {}", agent_id),
        Some(&agent_id),
    )
    .await?;

    if state.gemini_api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Avatar generation disabled (GEMINI_API_KEY not set)".to_string(),
        ));
    }

    let beasts = [
        "fox", "crane", "tiger", "dragon", "owl", "wolf", "bear", "phoenix",
    ];
    let scenes = [
        "misty mountain",
        "moonlit lake",
        "bamboo forest",
        "snowy peak",
        "tranquil garden",
        "coastal cliff",
        "autumn valley",
        "starlit temple",
    ];

    let beast_idx = agent_id.bytes().sum::<u8>() as usize % beasts.len();
    let scene_idx = (agent_id.bytes().map(|b| b as usize).sum::<usize>() / 7) % scenes.len();

    let beast = beasts[beast_idx];
    let scene = scenes[scene_idx];

    let prompt = format!(
        "A {} in {} in the style of Hasui Kawase. Japanese woodblock print aesthetic, \
        serene composition, soft color palette, atmospheric depth, elegant simplicity.",
        beast, scene
    );

    let request = GeminiRequest {
        contents: vec![GeminiContent {
            parts: vec![GeminiPart { text: prompt }],
        }],
        generation_config: GeminiGenerationConfig {
            response_modalities: vec!["IMAGE".to_string()],
        },
    };

    let client = reqwest::Client::new();
    let response = client
        .post("https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent")
        .header("x-goog-api-key", &state.gemini_api_key)
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to call Gemini API: {}", e),
            )
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Gemini API error {}: {}", status, error_text),
        ));
    }

    let gemini_response: GeminiResponse = response.json().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse Gemini response: {}", e),
        )
    })?;

    if let Some(candidate) = gemini_response.candidates.first() {
        for part in &candidate.content.parts {
            if let Some(inline_data) = &part.inline_data {
                let result = json!({
                    "agent_id": agent_id,
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data
                    }
                });

                // Persist to DB (durable) and filesystem (fast local cache)
                let _ = sqlx::query(
                    "INSERT INTO agent_avatars (agent_id, avatar_json)
                     VALUES ($1, $2)
                     ON CONFLICT (agent_id) DO UPDATE SET avatar_json = $2, created_at = NOW()",
                )
                .bind(&agent_id)
                .bind(&result)
                .execute(&state.db)
                .await;
                std::fs::write(&cache_path, serde_json::to_string(&result).unwrap()).ok();
                println!("Cached new avatar for {} (DB + filesystem)", agent_id);

                return Ok(Json(result));
            }
        }
    }

    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        "No image generated".to_string(),
    ))
}

// ─── Agent CRUD handlers ───────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct CreateAgentRequest {
    pub(crate) agent_name: String,
    #[serde(default = "default_agent_type")]
    pub(crate) agent_type: String,
    pub(crate) description: Option<String>,
    pub(crate) system_prompt: Option<String>,
    #[serde(default = "default_model")]
    pub(crate) model: String,
    #[serde(default = "default_temperature")]
    pub(crate) temperature: f64,
    #[serde(default = "default_executor")]
    pub(crate) executor_type: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default = "default_visibility")]
    pub(crate) visibility: String,
    #[serde(default)]
    pub(crate) education_budget_credits: i32,
    pub(crate) display_alias: Option<String>,
    #[serde(default = "default_llm_provider")]
    pub(crate) llm_provider: String,
    #[serde(default = "default_embedding_provider")]
    pub(crate) embedding_provider: String,
    #[serde(default = "default_embedding_model")]
    pub(crate) embedding_model: String,
    #[serde(default = "default_embedding_dimension")]
    pub(crate) embedding_dimension: i32,
    #[serde(default)]
    pub(crate) accepts: Vec<String>,
    #[serde(default)]
    pub(crate) produces: Vec<String>,
    pub(crate) prompt_template: Option<String>,
}

pub fn default_agent_type() -> String {
    "research".to_string()
}
pub fn default_model() -> String {
    "claude-haiku-4-5-20251001".to_string()
}
pub fn default_temperature() -> f64 {
    0.3
}
pub fn default_executor() -> String {
    "llm".to_string()
}
pub fn default_visibility() -> String {
    "private".to_string()
}
pub fn default_llm_provider() -> String {
    "anthropic".to_string()
}
pub fn default_embedding_provider() -> String {
    // Must track the platform's ACTIVE embedder (src/api_server.rs builds
    // OpenAIEmbeddings). Leaving this as "anthropic"/"voyage-2" stamped new
    // agents with an identity no vector ever had (Anthropic serves no
    // embeddings API) and mislabelled the correct OpenAI vectors in the
    // portability view. Single source of truth for new-agent embedding intent.
    "openai".to_string()
}
pub fn default_embedding_model() -> String {
    "text-embedding-3-large".to_string()
}
pub fn default_embedding_dimension() -> i32 {
    1024
}

pub async fn create_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Slug validation — `agent_name` is URL-routed via
    // /api/agents/:agent_id/... so it must satisfy the platform-wide
    // snake_case rule. See `fermi::slug` for the full rule and why.
    // Without this, an agent named `efra-ai/05-valuation` becomes
    // unreachable at its own URL and breaks the @-mention parser
    // downstream.
    fermi::slug::validate_http("agent_name", &req.agent_name)?;

    // SPEC_30 — classify at birth. Before mig-186 taxonomy lived only in
    // on-disk cards, so an agent authored through this endpoint could never
    // be classified and sat under `Incertae sedis` forever. Derived ranks
    // only; kingdom/family/genus are editorial and stay unset.
    let derived_taxonomy = fermi::taxonomy::derive(&fermi::taxonomy::DeriveInput {
        agent_name: req.agent_name.clone(),
        agent_type: req.agent_type.clone(),
        produces: req.produces.clone(),
        has_required_deps: false,
        has_instruments: false,
    });

    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: req.agent_name.clone(),
        agent_type: req.agent_type,
        version: "1.0.0".to_string(),
        tier: "community".to_string(),
        executor_type: req.executor_type,
        model: req.model,
        temperature: req.temperature,
        mcp_servers: None,
        mcp_tools: None,
        description: req.description,
        author: user_id.clone(),
        system_prompt: req.system_prompt,
        visibility: req.visibility,
        owner_id: Some(user_id.clone()),
        tags: req.tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: req.education_budget_credits,
        education_credits_used: 0,
        auto_collect_pct: 0,
        display_alias: req.display_alias,
        llm_provider: req.llm_provider,
        embedding_provider: req.embedding_provider,
        embedding_model: req.embedding_model,
        embedding_dimension: req.embedding_dimension,
        sample_queries: vec![],
        status: "draft".to_string(),
        fork_pricing: None,
        forked_from: None,
        fork_count: 0,
        accepts: req.accepts,
        produces: req.produces,
        workflow_template: None,
        prompt_template: req.prompt_template,
        requires_secrets: None,
        model_ladder: serde_json::Value::Array(vec![]),
        min_tier: "free".to_string(),
        capability_gates: serde_json::Value::Object(serde_json::Map::new()),
        persona_version: 1,
        fermi_contract: None,
        model_params: serde_json::Value::Object(serde_json::Map::new()),
        valence: None,
        output_contract: None,
        taxonomy: Some(derived_taxonomy),
    };

    // If education budget requested, debit from user's wallet
    if req.education_budget_credits > 0 {
        let wallet = get_or_create_wallet(&state.db, "user", &user_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Wallet error: {}", e),
                )
            })?;
        credit_charge(
            &state.db,
            wallet.wallet_id,
            req.education_budget_credits,
            "education_alloc",
            &format!("Education budget for agent {}", req.agent_name),
            None,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("Insufficient credits: {}", e),
            )
        })?;
    }

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to create agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": req.agent_name,
        "message": "Agent created successfully"
    })))
}

// ─── Model catalogue endpoint ──────────────────────────────────────

pub async fn model_catalogue_handler(State(_state): State<AppState>) -> Json<Value> {
    let check_env = |key: &str| -> bool { std::env::var(key).is_ok() };

    Json(json!({
        "providers": [
            {
                "id": "anthropic",
                "name": "Anthropic",
                "models": [
                    {"id": "claude-haiku-4-5-20251001", "name": "Haiku 4.5", "speed": "fast", "cost_tier": "low", "description": "Fast, efficient"},
                    {"id": "claude-sonnet-4-5-20250929", "name": "Sonnet 4.5", "speed": "balanced", "cost_tier": "medium", "description": "Balanced"},
                    {"id": "claude-opus-4-6", "name": "Opus 4.6", "speed": "slow", "cost_tier": "high", "description": "Most capable"}
                ],
                "env_var": "ANTHROPIC_API_KEY",
                "available": check_env("ANTHROPIC_API_KEY")
            },
            {
                "id": "mistral",
                "name": "Mistral",
                "models": [
                    {"id": "mistral-large-latest", "name": "Mistral Large", "speed": "balanced", "cost_tier": "medium", "description": "Most capable Mistral model"},
                    {"id": "mistral-medium-latest", "name": "Mistral Medium", "speed": "fast", "cost_tier": "low", "description": "Balanced Mistral model"},
                    {"id": "open-mistral-nemo", "name": "Mistral Nemo", "speed": "fast", "cost_tier": "low", "description": "Lightweight open model"}
                ],
                "env_var": "MISTRAL_API_KEY",
                "available": check_env("MISTRAL_API_KEY")
            },
            {
                "id": "openrouter",
                "name": "OpenRouter",
                "models": [
                    {"id": "anthropic/claude-3-opus", "name": "Claude 3 Opus (via OR)", "speed": "slow", "cost_tier": "high", "description": "Anthropic via OpenRouter"},
                    {"id": "meta-llama/llama-3.1-70b-instruct", "name": "Llama 3.1 70B", "speed": "fast", "cost_tier": "low", "description": "Meta open model"},
                    {"id": "google/gemini-pro-1.5", "name": "Gemini Pro 1.5", "speed": "balanced", "cost_tier": "medium", "description": "Google via OpenRouter"},
                    {"id": "mistralai/mixtral-8x22b-instruct", "name": "Mixtral 8x22B", "speed": "fast", "cost_tier": "low", "description": "Mistral MoE via OpenRouter"}
                ],
                "env_var": "OPENROUTER_API_KEY",
                "available": check_env("OPENROUTER_API_KEY")
            },
            {
                "id": "qwen",
                "name": "Qwen",
                "models": [
                    {"id": "qwen-max", "name": "Qwen Max", "speed": "slow", "cost_tier": "medium", "description": "Most capable Qwen model"},
                    {"id": "qwen-plus", "name": "Qwen Plus", "speed": "balanced", "cost_tier": "low", "description": "Balanced Qwen model"},
                    {"id": "qwen-turbo", "name": "Qwen Turbo", "speed": "fast", "cost_tier": "low", "description": "Fast Qwen model"}
                ],
                "env_var": "QWEN_API_KEY",
                "available": check_env("QWEN_API_KEY")
            },
            {
                "id": "deepseek",
                "name": "DeepSeek",
                "models": [
                    {"id": "deepseek-chat", "name": "DeepSeek V3", "speed": "fast", "cost_tier": "low", "description": "DeepSeek's flagship chat model — strong reasoning, low cost"},
                    {"id": "deepseek-reasoner", "name": "DeepSeek R1", "speed": "slow", "cost_tier": "low", "description": "Chain-of-thought reasoning model — comparable to o1 at fraction of cost"}
                ],
                "env_var": "DEEPSEEK_API_KEY",
                "base_url_env": "DEEPSEEK_BASE_URL",
                "default_base_url": "https://api.deepseek.com/v1",
                "available": check_env("DEEPSEEK_API_KEY")
            },
            {
                "id": "kimi",
                "name": "Kimi (Moonshot AI)",
                "models": [
                    {"id": "moonshot-v1-128k", "name": "Kimi 128k", "speed": "balanced", "cost_tier": "low", "description": "128k context window — strong at long-document analysis"},
                    {"id": "moonshot-v1-32k", "name": "Kimi 32k", "speed": "fast", "cost_tier": "low", "description": "32k context, faster and cheaper"},
                    {"id": "moonshot-v1-8k", "name": "Kimi 8k", "speed": "fast", "cost_tier": "low", "description": "8k context, lowest latency"}
                ],
                "env_var": "KIMI_API_KEY",
                "base_url_env": "KIMI_BASE_URL",
                "default_base_url": "https://api.moonshot.cn/v1",
                "available": check_env("KIMI_API_KEY")
            }
        ],
        "embedding_providers": [
            {"id": "openai", "name": "text-embedding-3-large (OpenAI)", "model": "text-embedding-3-large", "dimension": 1024, "env_var": "OPENAI_API_KEY", "available": check_env("OPENAI_API_KEY"), "default": true},
            {"id": "mistral", "name": "mistral-embed (Mistral)", "model": "mistral-embed", "dimension": 1024, "env_var": "MISTRAL_API_KEY", "available": check_env("MISTRAL_API_KEY")},
            {"id": "qwen", "name": "text-embedding-v3 (Qwen)", "model": "text-embedding-v3", "dimension": 1024, "env_var": "QWEN_API_KEY", "available": check_env("QWEN_API_KEY")}
        ]
    }))
}

// ─── Import agent endpoint ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct ImportAgentRequest {
    agent_card_json: Value,
}

pub async fn import_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<ImportAgentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let card = &req.agent_card_json;

    // Extract fields from agent_card.json format
    let agent_name = card
        .get("agent_id")
        .or_else(|| card.get("agent_name"))
        .and_then(|v| v.as_str())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Missing agent_id or agent_name in card".to_string(),
        ))?
        .to_string();

    // Same slug rule as `create_agent_handler` — an imported card whose
    // identifier breaks URL routing (or `@`-mentions) is rejected at the
    // door rather than landing in the DB and producing surprises later.
    fermi::slug::validate_http("agent_name", &agent_name)?;

    let agent_type = card
        .get("agent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("research")
        .to_string();

    let caps = card.get("capabilities");
    let model = caps
        .and_then(|c| c.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("claude-haiku-4-5-20251001")
        .to_string();

    let temperature = caps
        .and_then(|c| c.get("temperature"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);

    let executor_type = caps
        .and_then(|c| c.get("executor"))
        .and_then(|v| v.as_str())
        .unwrap_or("llm")
        .to_string();

    let meta = card.get("metadata");
    let description = meta
        .and_then(|m| m.get("description"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tags: Vec<String> = meta
        .and_then(|m| m.get("tags"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let system_prompt = card
        .get("system_prompt")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // SPEC_30 — an imported card may carry a taxonomy. Trust its EDITORIAL
    // ranks (kingdom/family/genus are a human's claim about kinship, and the
    // author of the card is a human) but always recompute the derived ranks
    // from the card's actual structure, so an imported taxonomy cannot
    // assert a class that contradicts its own agent_type.
    let imported_taxonomy = card
        .get("metadata")
        .and_then(|m| m.get("taxonomy"))
        .filter(|v| v.is_object())
        .cloned();
    let taxonomy = Some(fermi::taxonomy::merge(
        imported_taxonomy.as_ref(),
        &fermi::taxonomy::derive(&fermi::taxonomy::input_from_card(card)),
    ));

    let agent = Agent {
        agent_id: uuid::Uuid::new_v4(),
        agent_name: agent_name.clone(),
        agent_type,
        version: card
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0.0")
            .to_string(),
        tier: "community".to_string(),
        executor_type,
        model,
        temperature,
        // Was `c.get("mcp_tools")` — a long-standing bug: it stored the
        // agent's *platform* tool declarations in the column meant for
        // *remote MCP server* configs. Nothing read the column, so it
        // went unnoticed. Now that the DB is the source of truth for
        // agent config (see resolve_agent_card), it has to be the right
        // field or a new agent would come up with a server list that is
        // actually a tool list.
        mcp_servers: caps.and_then(|c| c.get("mcp_servers")).cloned(),
        // Persisted, not left to inherit: an agent created through this
        // path has no filesystem card to inherit from (the card arrives in
        // the request body), so dropping this would mean the agent
        // publishes nothing over /mcp/agents/:id. Validated against the
        // dispatch table below.
        mcp_tools: caps.and_then(|c| c.get("mcp_tools")).cloned(),
        description,
        author: user_id.clone(),
        system_prompt,
        visibility: "private".to_string(),
        owner_id: Some(user_id),
        tags,
        current_ontology_commit: None,
        current_ontology_snapshot_id: None,
        last_consolidated_at: None,
        total_executions: 0,
        successful_executions: 0,
        failed_executions: 0,
        total_cost_usd: None,
        avg_execution_time_ms: 0,
        dreaming_budget_credits: 5,
        dreaming_credits_used: 0,
        dreaming_budget_reset_at: None,
        education_budget_credits: 0,
        education_credits_used: 0,
        auto_collect_pct: 0,
        display_alias: None,
        llm_provider: "anthropic".to_string(),
        embedding_provider: default_embedding_provider(),
        embedding_model: default_embedding_model(),
        embedding_dimension: default_embedding_dimension(),
        sample_queries: vec![],
        status: "draft".to_string(),
        fork_pricing: None,
        forked_from: None,
        fork_count: 0,
        accepts: card
            .get("accepts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        produces: card
            .get("produces")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        workflow_template: card.get("workflow_template").cloned(),
        prompt_template: card
            .get("prompt_template")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        requires_secrets: card.get("requires_secrets").cloned(),
        model_ladder: card
            .get("capabilities")
            .and_then(|c| c.get("model_ladder"))
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![])),
        min_tier: card
            .get("capabilities")
            .and_then(|c| c.get("min_tier"))
            .and_then(|v| v.as_str())
            .unwrap_or("free")
            .to_string(),
        capability_gates: card
            .get("capabilities")
            .and_then(|c| c.get("capability_gates"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        persona_version: 1,
        // Orchestra membership is NOT importable.
        //
        // `agents.fermi_contract IS NOT NULL` is the predicate behind the
        // `orchestra_fermi_members` view (mig-172), i.e. it *is* Fermi
        // orchestra membership. Copying it out of a user-supplied card
        // let any authenticated user mint themselves an "admin-approved"
        // Fermi member by pasting a contract and self-publishing — a
        // complete bypass of the review flow in
        // `handlers::orchestras::approve_orchestra_request_handler`.
        //
        // Only platform admins may carry a contract through import (used
        // for restoring/migrating curated cards). Everyone else imports
        // the agent without membership and goes through
        // `POST /api/orchestras/fermi/requests` like any other candidate.
        fermi_contract: if principal.can_admin() {
            card.get("capabilities")
                .and_then(|c| c.get("fermi_contract"))
                .cloned()
        } else {
            None
        },
        model_params: card
            .get("capabilities")
            .and_then(|c| c.get("model_params"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        valence: card.get("metadata").and_then(|m| m.get("valence")).cloned(),
        output_contract: card
            .get("capabilities")
            .and_then(|c| c.get("output_contract"))
            .cloned(),
        taxonomy,
    };

    // Tell the importer we stripped the contract rather than letting them
    // discover it later by wondering why the orchestra panel says nothing.
    let contract_stripped = agent.fermi_contract.is_none()
        && card
            .get("capabilities")
            .and_then(|c| c.get("fermi_contract"))
            .map(|v| !v.is_null())
            .unwrap_or(false);

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to import agent: {}", e),
        )
    })?;

    let mut body = json!({
        "agent_id": agent_id,
        "agent_name": agent_name,
        "message": "Agent imported successfully"
    });
    if contract_stripped {
        body["fermi_contract_stripped"] = json!(true);
        body["note"] = json!(
            "The card's fermi_contract was not imported — Fermi orchestra \
             membership is admin-reviewed. Request it via the agent's \
             Manage → Orchestras panel once the agent is published."
        );
    }
    Ok(Json(body))
}

// ─── Custom embeddings import endpoint ─────────────────────────────

#[derive(Deserialize)]
pub struct ImportEmbeddingsRequest {
    episodes: Vec<ImportedEpisode>,
}

/// Imported episode payload.
///
/// Spec 22 (Phase 1.6) breaking change: clients MUST supply `model_id`,
/// `model_version`, `dim`, and `source_text` alongside the vector so the
/// server can record what produced it. Imports are persisted with
/// `provenance_trusted = false` because the model identity is asserted by
/// the client and unverifiable.
#[derive(Deserialize)]
pub struct ImportedEpisode {
    query: String,
    summary: Option<String>,
    embedding: Vec<f32>,
    /// The exact text the client embedded to produce `embedding`. Required.
    source_text: String,
    /// Model identifier the client used, in "<provider>/<model>" form
    /// (e.g. "anthropic/voyage-2"). Required.
    model_id: String,
    /// Manual epoch version string the client used (e.g. "2024-01-01"). Required.
    model_version: String,
    /// Output dimensionality. Must equal `embedding.len()`.
    dim: i32,
}

pub async fn import_embeddings_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
    Json(req): Json<ImportEmbeddingsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Load agent to verify ownership and get embedding dimension
    let agent = state
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
        })?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // v0.10.5: substrate RBAC. Embedding import writes to the agent's
    // memory — Admin permission required (owner or platform admin).
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &agent.agent_id.to_string(),
        agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&agent),
    )
    .await?;
    let _ = user_id; // now unused directly; kept as a local for clarity below

    if req.episodes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No episodes provided".to_string()));
    }

    // Spec 22 §1.6 — validate embedding dimensions AND the client-supplied
    // provenance fields. The provenance is asserted, not verified.
    for (i, ep) in req.episodes.iter().enumerate() {
        if ep.embedding.len() as i32 != agent.embedding_dimension {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Episode {}: expected {} dimensions, got {}. Embeddings must match agent's embedding model ({}).",
                    i, agent.embedding_dimension, ep.embedding.len(), agent.embedding_model
                ),
            ));
        }
        if ep.dim != ep.embedding.len() as i32 {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Episode {}: declared dim={} does not match embedding length {}",
                    i,
                    ep.dim,
                    ep.embedding.len()
                ),
            ));
        }
        if ep.model_id.trim().is_empty() || ep.model_version.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Episode {}: model_id and model_version are required (Spec 22 §1.6)",
                    i
                ),
            ));
        }
        if ep.source_text.is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "Episode {}: source_text is required and must be non-empty (Spec 22 §1.6)",
                    i
                ),
            ));
        }
    }

    // Charge gas
    let wallet = fermi_auth::get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;

    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.embedding_import,
        "embedding_import",
        &format!(
            "Import {} episodes with embeddings for agent {}",
            req.episodes.len(),
            agent.agent_name
        ),
        Some(&agent_id.to_string()),
    )
    .await?;

    // Create episodes with provided embeddings and client-supplied provenance.
    // Per Spec 22 §1.6: imports are persisted with `provenance_trusted = false`
    // because the model identity is asserted, not verifiable.
    let mut imported = 0;
    for ep in &req.episodes {
        let episode = Episode {
            response_text: None,
            episode_id: uuid::Uuid::new_v4(),
            agent_id,
            timestamp_ref: chrono::Utc::now(),
            query: ep.query.clone(),
            context: serde_json::json!({
                "source": "import",
                "summary": ep.summary
            }),
            execution_status: agent_bestiary_memory::ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            // Imported episodes were not executed here, so this deployment
            // bore no provider cost for them. Left `None` so an import
            // cannot inflate a per-agent or per-forecast spend total.
            input_tokens: None,
            output_tokens: None,
            cost_basis: None,
            cost_rate_key: None,
            parent_episode_id: None,
            embedding: Some(ep.embedding.clone()),
            consolidated: false,
            tags: vec![],
            provenance: agent_bestiary_memory::Provenance::AutoPass,
            authority_weight: 0.5,
            dyad_id: None,
            persona_version_at_write: None,
            provider_used: None,
            model_used: None,
        };

        // Client-asserted provenance — mark untrusted via source_ref.
        let provenance = agent_bestiary_memory::ProvenancedEmbedding {
            vector: ep.embedding.clone(),
            source_text: ep.source_text.clone(),
            model_id: ep.model_id.clone(),
            model_version: ep.model_version.clone(),
            dim: ep.dim,
        };
        let source_ref = serde_json::json!({
            "kind": "client_import",
            "caller": user_id,
            "trusted": false,
        });

        // Spec 22 §1.6: client-imported rows are stamped `provenance_trusted=false`.
        state
            .memory_store
            .store_episode_with_untrusted_provenance(
                episode,
                &provenance,
                Some(source_ref),
                "client_import",
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to store episode: {}", e),
                )
            })?;
        imported += 1;
    }

    Ok(Json(json!({
        "imported": imported,
        "agent_id": agent_id,
        "message": format!("Imported {} episodes with embeddings", imported)
    })))
}

// ─────────────────────────────────────────────────────────────────────
// Spec 22 §UX — Embedding Portability affordance for the agent card.
//
// Three endpoints surface portability state and let owners exercise it:
//
//   GET  /api/agents/:id/embeddings/stats           — public, aggregate counts
//   POST /api/agents/:id/embeddings/export/consent  — owner-only, issues a
//                                                     scoped one-shot token
//                                                     acknowledging the
//                                                     invertibility warning
//   GET  /api/agents/:id/embeddings/export          — owner-only, JSONL dump;
//                                                     requires the consent
//                                                     token in the
//                                                     `X-Export-Consent` header
//                                                     for raw-vector exports,
//                                                     not required for the
//                                                     source-only format
//
// Security posture (Spec 22 §Security):
//   - Aggregate stats are non-leaky (counts only).
//   - source_only export rung-1+2: the source corpus + structure are the
//     SAFE default. The participant owns it; exporting it does not leak.
//   - full export rung-3: includes the raw vectors. INVERTIBLE — anyone
//     holding them can recover substantial source content. Requires explicit
//     consent token and is logged.
// ─────────────────────────────────────────────────────────────────────

/// Aggregate embedding-portability stats for an agent's episode store.
///
/// Returns counts by model_id × trusted, count of NULL-embedded episodes,
/// and a roll-up trust ratio. Aggregate-only — no source text, no vectors,
/// no per-row data. Safe for public viewers of any agent.
pub async fn embeddings_stats_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Verify the agent exists (404 otherwise) but don't require auth here.
    let agent = state
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    let pool = state.memory_store.pool();

    // Episodes — by model_id / model_version / trusted, plus NULL bucket.
    let by_model = sqlx::query(
        r#"
        SELECT
            COALESCE(embedding_model_id, '__null__')      AS model_id,
            COALESCE(embedding_model_version, '__null__') AS model_version,
            COALESCE(embedding_dim, 0)                    AS dim,
            provenance_trusted                            AS trusted,
            COUNT(*)                                      AS n,
            COUNT(embedding) FILTER (WHERE embedding IS NOT NULL) AS n_with_vec
          FROM episodes
         WHERE agent_id = $1
         GROUP BY embedding_model_id, embedding_model_version,
                  embedding_dim, provenance_trusted
         ORDER BY n DESC
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("stats: {e}")))?;

    let mut by_model_json = Vec::new();
    let mut total_episodes: i64 = 0;
    let mut total_with_vector: i64 = 0;
    let mut total_trusted_with_vector: i64 = 0;
    for row in &by_model {
        let model_id: String = row.try_get("model_id").unwrap_or_default();
        let model_version: String = row.try_get("model_version").unwrap_or_default();
        let dim: i32 = row.try_get("dim").unwrap_or(0);
        let trusted: bool = row.try_get("trusted").unwrap_or(false);
        let n: i64 = row.try_get("n").unwrap_or(0);
        let n_with_vec: i64 = row.try_get("n_with_vec").unwrap_or(0);

        total_episodes += n;
        total_with_vector += n_with_vec;
        if trusted {
            total_trusted_with_vector += n_with_vec;
        }

        by_model_json.push(json!({
            "model_id": if model_id == "__null__" { Value::Null } else { json!(model_id) },
            "model_version": if model_version == "__null__" {
                Value::Null
            } else {
                json!(model_version)
            },
            "dim": if dim == 0 { Value::Null } else { json!(dim) },
            "trusted": trusted,
            "episodes": n,
            "episodes_with_vector": n_with_vec,
        }));
    }

    let trust_ratio = if total_with_vector > 0 {
        (total_trusted_with_vector as f64) / (total_with_vector as f64)
    } else {
        0.0
    };

    // Provenance event-log stats — append-only history.
    let provenance_total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM embedding_provenance WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": agent.agent_name,
        "embedding_intent": {
            "provider": agent.embedding_provider,
            "model":    agent.embedding_model,
            "dimension": agent.embedding_dimension,
        },
        "episodes": {
            "total": total_episodes,
            "with_vector": total_with_vector,
            "trusted_with_vector": total_trusted_with_vector,
            "trust_ratio": trust_ratio,
            "by_model": by_model_json,
        },
        "provenance_events_total": provenance_total,
        "portability": {
            "source_only_exportable": total_episodes,
            "full_exportable": total_with_vector,
            "spec": "Spec 22 — Embedding Portability"
        }
    })))
}

/// Request body for the export consent gate.
#[derive(Deserialize)]
pub struct ExportConsentRequest {
    /// Must equal `"i_understand_embeddings_are_invertible"`. Forces the
    /// caller to acknowledge the security warning explicitly rather than
    /// click-through dismiss it.
    acknowledged_invertibility: String,
}

/// Owner-only: issue a single-use, time-bounded consent token authorising a
/// full (raw-vector) export. Returns a 32-char hex token to be presented in
/// the `X-Export-Consent` header on the subsequent GET .../export call.
///
/// Tokens are stored in-process (state.consent_tokens). Single-machine
/// deployment is fine for now; multi-instance deployments would need a
/// shared store but we are nowhere near that scale.
pub async fn embeddings_export_consent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
    Json(req): Json<ExportConsentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let agent = state
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // v0.10.5: substrate RBAC. Export consent is a sensitive action
    // (issues a scoped one-shot token) — Admin required.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &agent.agent_id.to_string(),
        agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&agent),
    )
    .await?;

    const REQUIRED_PHRASE: &str = "i_understand_embeddings_are_invertible";
    if req.acknowledged_invertibility != REQUIRED_PHRASE {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "acknowledged_invertibility must equal {:?} (Spec 22 §Security)",
                REQUIRED_PHRASE
            ),
        ));
    }

    // Issue a single-use token.
    let token = {
        use rand::RngCore;
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(5);

    state.export_consent.insert(
        token.clone(),
        crate::ExportConsentEntry {
            agent_id,
            user_id: user_id.clone(),
            expires_at,
            consumed: false,
        },
    );

    Ok(Json(json!({
        "token": token,
        "expires_at": expires_at.to_rfc3339(),
        "warning": "Raw embeddings are invertible. Anyone holding the exported \
                    file can recover substantial source content via \
                    embedding-inversion attacks. Treat the file as if it were \
                    the source text itself.",
        "audit_id": format!("export_{}_{}", agent_id, chrono::Utc::now().timestamp()),
    })))
}

/// Query params for the export endpoint.
#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    /// `"source_only"` (default) returns source_text + provenance metadata,
    /// no vectors. `"full"` includes the raw vector and requires a consent
    /// token in the `X-Export-Consent` header.
    #[serde(default)]
    format: Option<String>,
}

/// Owner-only: streamed JSONL export of the agent's episode store.
///
/// Two formats:
///   - source_only (default): one line per episode with
///       { episode_id, agent_id, timestamp_ref, query, summary, source_text,
///         source_ref, embedding_model_id, embedding_model_version,
///         embedding_dim, provenance_trusted }
///     SAFE — the source corpus is the participant's actual asset (Spec 22
///     "what's owned" rungs 1–2). No vectors leak; export is logged.
///
///   - full: adds the `embedding` vector. INVERTIBLE — requires the
///     `X-Export-Consent` token issued by the /consent endpoint.
pub async fn embeddings_export_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
    Query(q): Query<ExportQuery>,
    headers: axum::http::HeaderMap,
) -> Result<Response<String>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let agent = state
        .memory_store
        .get_agent(agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?
        .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;

    // v0.10.5: substrate RBAC. Bulk export of embeddings + source_text
    // is data egress — Admin required.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &agent.agent_id.to_string(),
        agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&agent),
    )
    .await?;
    let _ = user_id;

    let format = q.format.as_deref().unwrap_or("source_only");
    let include_vectors = match format {
        "source_only" => false,
        "full" => true,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("format must be 'source_only' or 'full', got {:?}", other),
            ))
        }
    };

    // Consent gate for full export.
    if include_vectors {
        let token = headers
            .get("X-Export-Consent")
            .and_then(|h| h.to_str().ok())
            .ok_or((
                StatusCode::FORBIDDEN,
                "Full export requires X-Export-Consent header. POST to \
                 /api/agents/:id/embeddings/export/consent first to obtain a \
                 token. (Spec 22 §Security)"
                    .to_string(),
            ))?
            .to_string();

        let mut entry = state.export_consent.get_mut(&token).ok_or((
            StatusCode::FORBIDDEN,
            "Invalid or expired consent token".to_string(),
        ))?;
        if entry.consumed {
            return Err((
                StatusCode::FORBIDDEN,
                "Consent token already used (single-use)".to_string(),
            ));
        }
        if entry.expires_at < chrono::Utc::now() {
            return Err((StatusCode::FORBIDDEN, "Consent token expired".to_string()));
        }
        if entry.agent_id != agent_id {
            return Err((
                StatusCode::FORBIDDEN,
                "Consent token is for a different agent".to_string(),
            ));
        }
        if entry.user_id != user_id {
            return Err((
                StatusCode::FORBIDDEN,
                "Consent token belongs to a different user".to_string(),
            ));
        }
        entry.consumed = true;
        drop(entry);
        // Token consumed — leave it in the map so retries with the same
        // token explicitly fail rather than silently re-authorize.
    }

    // Stream episodes. For solo-dev scale this is fine in-memory; if the
    // agent grows past ~50k episodes we should switch to a body-stream.
    let pool = state.memory_store.pool();
    let rows = sqlx::query(
        r#"
        SELECT episode_id, agent_id, timestamp_ref, query, context,
               source_text, source_ref,
               embedding_model_id, embedding_model_version, embedding_dim,
               provenance_trusted, embedding
          FROM episodes
         WHERE agent_id = $1
         ORDER BY timestamp_ref ASC
        "#,
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("export: {e}")))?;

    let mut body = String::with_capacity(rows.len() * 256);
    // First line: a metadata header describing the export. Lets the importer
    // verify scope before reading any episode lines.
    let header_obj = json!({
        "kind": "abw_embedding_export_header",
        "spec": "Spec 22",
        "agent_id": agent_id,
        "agent_name": agent.agent_name,
        "format": format,
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "exported_by": user_id,
        "episode_count": rows.len(),
        "warning": if include_vectors {
            "Includes raw vectors — INVERTIBLE. Treat as the source corpus itself."
        } else {
            "Source corpus + provenance only. Vectors NOT included; safe to share."
        },
    });
    body.push_str(&serde_json::to_string(&header_obj).unwrap());
    body.push('\n');

    for row in &rows {
        let episode_id: Uuid = row.try_get("episode_id").unwrap_or_else(|_| Uuid::nil());
        let timestamp_ref: chrono::DateTime<chrono::Utc> = row
            .try_get("timestamp_ref")
            .unwrap_or_else(|_| chrono::Utc::now());
        let query: String = row.try_get("query").unwrap_or_default();
        let context: Value = row.try_get("context").unwrap_or(Value::Null);
        let source_text: Option<String> = row.try_get("source_text").ok();
        let source_ref: Option<Value> = row.try_get("source_ref").ok();
        let embedding_model_id: Option<String> = row.try_get("embedding_model_id").ok();
        let embedding_model_version: Option<String> = row.try_get("embedding_model_version").ok();
        let embedding_dim: Option<i32> = row.try_get("embedding_dim").ok();
        let provenance_trusted: bool = row.try_get("provenance_trusted").unwrap_or(false);

        // Summary preserved from context for the import round-trip.
        let summary = context.get("summary").cloned().unwrap_or(Value::Null);

        let mut obj = serde_json::Map::new();
        obj.insert("episode_id".into(), json!(episode_id));
        obj.insert("timestamp_ref".into(), json!(timestamp_ref));
        obj.insert("query".into(), json!(query));
        obj.insert("summary".into(), summary);
        obj.insert("source_text".into(), json!(source_text));
        obj.insert("source_ref".into(), source_ref.unwrap_or(Value::Null));
        obj.insert("model_id".into(), json!(embedding_model_id));
        obj.insert("model_version".into(), json!(embedding_model_version));
        obj.insert("dim".into(), json!(embedding_dim));
        obj.insert("provenance_trusted".into(), json!(provenance_trusted));

        if include_vectors {
            let vec_opt: Option<pgvector::Vector> = row.try_get("embedding").ok();
            if let Some(v) = vec_opt {
                obj.insert("embedding".into(), json!(v.to_vec()));
            } else {
                obj.insert("embedding".into(), Value::Null);
            }
        }

        body.push_str(&serde_json::to_string(&Value::Object(obj)).unwrap());
        body.push('\n');
    }

    // Audit log: every export is recorded as an embedding_provenance event
    // with kind="export" — matches Spec 22's "consented, scoped, logged"
    // requirement and keeps the export trail in the same append-only log
    // as the original writes.
    let audit_ref = json!({
        "kind": "export",
        "format": format,
        "exported_by": user_id,
        "include_vectors": include_vectors,
        "episode_count": rows.len(),
    });
    let _ = sqlx::query(
        r#"
        INSERT INTO embedding_provenance (
            target_table, target_id, agent_id, user_id,
            source_text, source_ref,
            model_id, model_version, dim,
            trusted, notes
        ) VALUES (
            'episodes', $1, $2, $3, NULL, $4, $5, $6, $7, $8, $9
        )
        "#,
    )
    .bind(agent_id)
    .bind(agent_id)
    .bind(&user_id)
    .bind(&audit_ref)
    .bind(agent.embedding_model.as_str())
    .bind("export_event")
    .bind(agent.embedding_dimension)
    .bind(true)
    .bind(format!("export:{}:{}", format, rows.len()))
    .execute(pool)
    .await;

    let filename = format!(
        "abw_export_{}_{}.jsonl",
        agent.agent_name,
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(body)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("response: {e}")))
}

pub async fn list_curated_agents_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agents = state
        .registry
        .list_cards()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{}", e)))?;

    let curated: Vec<Value> = agents
        .iter()
        .map(|card| {
            json!({
                "agent_id": card.agent_id,
                "agent_type": card.agent_type,
                "version": card.version,
                "description": card.metadata.description,
                "tags": card.metadata.tags,
                "model": card.capabilities.model,
                "sample_queries": card.metadata.sample_queries,
                "system_prompt": card.system_prompt,
                "accepts": card.accepts,
                "produces": card.produces,
                "workflow_template": card.workflow_template,
                "prompt_template": card.prompt_template,
                "requires_secrets": card.requires_secrets,
                "capabilities": {
                    "executor": card.capabilities.executor,
                    "model": card.capabilities.model,
                    "mcp_tools": card.capabilities.mcp_tools.iter().map(|t| json!({"name": t.name, "description": t.description})).collect::<Vec<_>>(),
                    "skills": card.capabilities.skills,
                },
            })
        })
        .collect();

    Ok(Json(json!({ "agents": curated })))
}

pub async fn list_my_agents_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let agents: Vec<_> = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list agents: {}", e),
            )
        })?
        .into_iter()
        .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
        .collect();

    // Batch-load workspace memberships, segmented by origin so the
    // harness Collection UI shows ABW workspaces as pills and rolls
    // up rabble / fermi / other-vertical memberships into counts.
    // Previously this returned every workspace name regardless of
    // origin, which produced 50+ rabble pills on system agents like
    // enemy_sensor that get auto-hired into every swarm.
    let agent_ids: Vec<uuid::Uuid> = agents.iter().map(|a| a.agent_id).collect();
    let ws_rows = sqlx::query(
        "SELECT wa.agent_id, t.name, t.origin
         FROM workspace_agents wa
         JOIN teams t ON t.id = wa.workspace_id
         WHERE wa.agent_id = ANY($1)",
    )
    .bind(&agent_ids)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // ABW workspaces → pills (full names listed).
    // Other origins → roll up to {origin: count}.
    let mut ws_names_abw: std::collections::HashMap<uuid::Uuid, Vec<String>> =
        std::collections::HashMap::new();
    let mut ws_counts_by_origin: std::collections::HashMap<
        uuid::Uuid,
        std::collections::BTreeMap<String, i32>,
    > = std::collections::HashMap::new();
    for r in &ws_rows {
        let aid: uuid::Uuid = r.get("agent_id");
        let name: String = r.get("name");
        let origin: String = r
            .try_get::<String, _>("origin")
            .unwrap_or_else(|_| "bestiary_workspace".into());
        if origin == "bestiary_workspace" {
            ws_names_abw.entry(aid).or_default().push(name);
        } else {
            *ws_counts_by_origin
                .entry(aid)
                .or_default()
                .entry(origin)
                .or_insert(0) += 1;
        }
    }

    let agent_list: Vec<Value> = agents
        .iter()
        .map(|a| {
            let abw_names = ws_names_abw.get(&a.agent_id).cloned().unwrap_or_default();
            let other_counts = ws_counts_by_origin
                .get(&a.agent_id)
                .cloned()
                .unwrap_or_default();
            let total_count = abw_names.len() as i32 + other_counts.values().sum::<i32>();
            json!({
                "agent_id": a.agent_id,
                "agent_name": a.agent_name,
                "display_alias": a.display_alias,
                "agent_type": a.agent_type,
                "description": a.description,
                "visibility": a.visibility,
                "tags": a.tags,
                "model": a.model,
                "total_executions": a.total_executions,
                "education_budget_credits": a.education_budget_credits,
                "education_credits_used": a.education_credits_used,
                "status": a.status,
                "fork_pricing": a.fork_pricing,
                "forked_from": a.forked_from,
                "fork_count": a.fork_count,
                "workspace_names": abw_names,
                "workspace_counts_by_origin": other_counts,
                "workspace_count": total_count,
                // Spec 22 — embedding intent on dashboard tiles, owner sees
                // this on every agent they own.
                "embedding": {
                    "provider": a.embedding_provider,
                    "model": a.embedding_model,
                    "dimension": a.embedding_dimension,
                },
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agent_list })))
}

/// GET /api/me/providers
///
/// Returns the distinct set of LLM providers active across the caller's
/// agents and their execution history. Used to drive the provider filter
/// in the observatory — the list is data-driven, not hardcoded.
///
/// Sources (unioned, deduplicated, sorted):
///   1. `agents.llm_provider` for owned agents
///   2. `agents.model_ladder` JSONB — providers declared in tier rungs
///   3. `agent_timeline_entries.provider_used` — providers actually observed
///      in recent execution history (last 90 days)
///
/// Response: `{ "providers": ["anthropic", "mistral", "qwen", ...] }`
pub async fn list_my_providers_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let mut providers: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    // 1. Primary provider from owned agents + model_ladder rungs
    let agent_rows =
        sqlx::query("SELECT llm_provider, model_ladder FROM agents WHERE user_id = $1")
            .bind(&user_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for row in &agent_rows {
        if let Ok(p) = row.try_get::<String, _>("llm_provider") {
            if !p.is_empty() {
                providers.insert(p);
            }
        }
        // Extract providers from model_ladder JSONB array
        if let Ok(ladder) = row.try_get::<serde_json::Value, _>("model_ladder") {
            if let Some(arr) = ladder.as_array() {
                for rung in arr {
                    if let Some(p) = rung.get("provider").and_then(|v| v.as_str()) {
                        if !p.is_empty() {
                            providers.insert(p.to_string());
                        }
                    }
                }
            }
        }
    }

    // 2. Providers observed in recent execution history (timeline entries)
    // Only for agents owned by this user — join via agent ownership.
    let timeline_rows = sqlx::query(
        r#"SELECT DISTINCT ate.provider_used
           FROM agent_timeline_entries ate
           JOIN agents a ON a.agent_id = ate.agent_id
           WHERE a.user_id = $1
             AND ate.provider_used IS NOT NULL
             AND ate.created_at >= NOW() - INTERVAL '90 days'"#,
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default(); // non-fatal — timeline may be sparse

    for row in &timeline_rows {
        if let Ok(p) = row.try_get::<String, _>("provider_used") {
            if !p.is_empty() {
                providers.insert(p);
            }
        }
    }

    // 3. Also include curated agents' providers — they are part of the
    //    user's observable fleet even if not owned.
    let curated_rows = sqlx::query(
        "SELECT DISTINCT llm_provider FROM agents WHERE user_id IS NULL AND tier = 'curated'",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for row in &curated_rows {
        if let Ok(p) = row.try_get::<String, _>("llm_provider") {
            if !p.is_empty() {
                providers.insert(p);
            }
        }
    }

    let list: Vec<&str> = providers.iter().map(String::as_str).collect();
    Ok(Json(serde_json::json!({ "providers": list })))
}

pub async fn update_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(mut updates): Json<AgentUpdate>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let user_id = principal.user_id();

    // v0.10.5: RBAC via substrate. Edit = owner, platform admin, or
    // holder of an edit/admin share (once agent shares land in
    // object_shares). Same semantics as the old hand-rolled check for
    // now (no shares yet); the substrate makes future sharing
    // features drop-in.
    rbac::require_edit(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    // Authorize first, then validate: a caller with no edit rights should
    // learn that, not which fields happen to be editable.
    reject_lifecycle_fields(&updates, &agent_id).map_err(|m| (StatusCode::BAD_REQUEST, m))?;
    // Defensive: keep the values out of the SET clause even if the guard
    // above is ever relaxed to a warning.
    updates.status = None;
    updates.visibility = None;

    // Reject phantom tool declarations before they reach the DB.
    //
    // A name in `mcp_tools` must resolve to a dispatch arm in
    // `ToolRegistry::execute`, or be a `server__tool` name from a server the
    // agent declares. Anything else is advertised to the model and over
    // `/mcp/agents/:id`, gets called, and answers `Unknown tool: X`.
    // Nothing validated this before, which is how cards ended up asserting
    // capabilities that were never wired.
    //
    // Servers are taken from this same update when it changes them, so a
    // single PUT can add a server and publish its tools atomically.
    if let Some(raw_tools) = updates.mcp_tools.as_ref() {
        if !raw_tools.is_null() {
            let declared: Vec<String> = serde_json::from_value::<
                Vec<fermi::agent_backend::agent_card::McpTool>,
            >(raw_tools.clone())
            .map(|v| v.into_iter().map(|t| t.name).collect())
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    format!("mcp_tools must be [{{name, description, input_schema}}]: {e}"),
                )
            })?;

            let servers = match updates.mcp_servers.as_ref() {
                Some(raw) if !raw.is_null() => {
                    fermi::agent_backend::mcp_client::interpret_db_column(raw).unwrap_or_default()
                }
                // Not being changed in this PUT — validate against whatever
                // the agent effectively has today.
                _ => {
                    resolve_agent_card(&state, &db_agent)
                        .capabilities
                        .mcp_servers
                }
            };

            let invalid =
                fermi::agent_backend::tools::invalid_tool_declarations(&declared, &servers);
            if !invalid.is_empty() {
                let detail = invalid
                    .iter()
                    .map(|(name, why)| format!("'{name}': {why}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("cannot publish undispatchable tools — {detail}"),
                ));
            }
        }
    }

    // Capture pre-update version number for the activity-feed event (Doc 12 §
    // Capability 3). Snapshotting *after* the update means the previous max
    // is the from-version; cheap query, runs once per PUT.
    let from_version_number = state
        .memory_store
        .list_agent_versions(db_agent.agent_id)
        .await
        .ok()
        .and_then(|vs| vs.first().map(|v| v.version_number))
        .unwrap_or(0);

    // Apply the update first, then snapshot. This is the inversion documented
    // in Doc 12 § Capability 1: when `create_agent_version` runs *after*
    // `update_agent`, the freshly-inserted row reflects the *current* state
    // of the `agents` table. `MAX(version_number)` is then the canonical
    // pointer to "the version this agent is currently at" — the property
    // every other Capability in this spec depends on.
    state
        .memory_store
        .update_agent(db_agent.agent_id, &updates)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to update agent: {}", e),
            )
        })?;

    let new_version = state
        .memory_store
        .create_agent_version(db_agent.agent_id, &user_id)
        .await
        .ok();

    // Doc 12 § Capability 3 — emit agent_card.updated to every workspace
    // where this agent is hired. Best-effort, async, doesn't block the PUT.
    if let Some(ref v) = new_version {
        let to_version_id = v.version_id;
        let to_version_number = v.version_number;
        let agent_uuid = db_agent.agent_id;
        let agent_name = db_agent.agent_name.clone();
        let changelog_summary = updates.description.clone();
        let changed_fields = collect_changed_fields(&updates);
        let event_state = state.clone();
        tokio::spawn(async move {
            broadcast_agent_card_updated(
                &event_state,
                agent_uuid,
                &agent_name,
                from_version_number,
                None,
                to_version_number,
                Some(to_version_id),
                &changed_fields,
                changelog_summary.as_deref(),
                "owner",
            )
            .await;
        });
    }

    Ok(Json(json!({
        "message": "Agent updated successfully",
        "version_number": new_version.as_ref().map(|v| v.version_number),
        "version_id": new_version.as_ref().map(|v| v.version_id),
    })))
}

/// Lifecycle fields are not editable through `PUT /api/agents/:agent_id`.
///
/// `AgentUpdate` carries `status` and `visibility` because internal callers
/// legitimately write them — `restore_agent_version_handler` restores a
/// prior visibility, and `workflows::publish_pipeline` sets both. But
/// accepting them over the generic update route made the entire publish
/// gate optional: a single
///
/// ```text
/// PUT /api/agents/:id  {"status":"published","visibility":"public"}
/// ```
///
/// put an agent into the public catalogue with no publish checks, no
/// lifecycle transition validation and no publish fee — bypassing
/// `publish_pipeline::publish_agent`, its admin-only `force` gate and its
/// `admin_bypass_events` audit trail entirely.
///
/// Rejects rather than silently dropping: a client that believed it was
/// publishing needs to learn that it wasn't.
fn reject_lifecycle_fields(updates: &AgentUpdate, agent_id: &str) -> Result<(), String> {
    let attempted = match (updates.status.is_some(), updates.visibility.is_some()) {
        (false, false) => return Ok(()),
        (true, true) => "status and visibility",
        (true, false) => "status",
        (false, true) => "visibility",
    };
    Err(format!(
        "cannot change {attempted} via PUT /api/agents/:agent_id — lifecycle \
         transitions run through the publish pipeline so publish checks, fees \
         and audit logging are applied. Use POST /api/agents/{agent_id}/publish, \
         /archive or /restore."
    ))
}

/// Doc 12 § Capability 3 — collect the names of fields the caller is changing
/// in this PUT. Used in the `agent_card.updated` event body so app-side UIs
/// can render "system_prompt and model_ladder changed" without diffing.
fn collect_changed_fields(updates: &AgentUpdate) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if updates.description.is_some() {
        fields.push("description");
    }
    if updates.system_prompt.is_some() {
        fields.push("system_prompt");
    }
    if updates.visibility.is_some() {
        fields.push("visibility");
    }
    if updates.tags.is_some() {
        fields.push("tags");
    }
    if updates.model.is_some() {
        fields.push("model");
    }
    if updates.temperature.is_some() {
        fields.push("temperature");
    }
    if updates.display_alias.is_some() {
        fields.push("display_alias");
    }
    if updates.status.is_some() {
        fields.push("status");
    }
    if updates.fork_pricing.is_some() {
        fields.push("fork_pricing");
    }
    if updates.accepts.is_some() {
        fields.push("accepts");
    }
    if updates.produces.is_some() {
        fields.push("produces");
    }
    if updates.workflow_template.is_some() {
        fields.push("workflow_template");
    }
    if updates.prompt_template.is_some() {
        fields.push("prompt_template");
    }
    if updates.requires_secrets.is_some() {
        fields.push("requires_secrets");
    }
    if updates.mcp_servers.is_some() {
        fields.push("mcp_servers");
    }
    if updates.mcp_tools.is_some() {
        fields.push("mcp_tools");
    }
    if updates.llm_provider.is_some() {
        fields.push("llm_provider");
    }
    if updates.model_ladder.is_some() {
        fields.push("model_ladder");
    }
    if updates.min_tier.is_some() {
        fields.push("min_tier");
    }
    if updates.capability_gates.is_some() {
        fields.push("capability_gates");
    }
    if updates.model_params.is_some() {
        fields.push("model_params");
    }
    if updates.valence.is_some() {
        fields.push("valence");
    }
    if updates.output_contract.is_some() {
        fields.push("output_contract");
    }
    if updates.version.is_some() {
        fields.push("version");
    }
    if updates.education_budget_credits.is_some() {
        fields.push("education_budget_credits");
    }
    if updates.taxonomy.is_some() {
        fields.push("taxonomy");
    }
    fields
}

/// Doc 12 § Capability 3 — fan an agent_card.updated system_event into every
/// workspace where the given agent is hired. Best-effort; errors are logged
/// but do not propagate, because the underlying PUT has already committed.
async fn broadcast_agent_card_updated(
    state: &AppState,
    agent_uuid: Uuid,
    agent_name: &str,
    from_version_number: i32,
    from_version_id: Option<Uuid>,
    to_version_number: i32,
    to_version_id: Option<Uuid>,
    changed_fields: &[&'static str],
    changelog_summary: Option<&str>,
    changed_by: &str,
) {
    let workspaces =
        match sqlx::query("SELECT DISTINCT workspace_id FROM workspace_agents WHERE agent_id = $1")
            .bind(agent_uuid)
            .fetch_all(&state.db)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!(
                    "agent_card.updated: failed to look up hired workspaces for {}: {}",
                    agent_name, e
                );
                return;
            }
        };

    if workspaces.is_empty() {
        return;
    }

    let body = json!({
        "kind": "agent_card.updated",
        "agent_id": agent_uuid,
        "agent_name": agent_name,
        "from_version_number": from_version_number,
        "from_version_id": from_version_id,
        "to_version_number": to_version_number,
        "to_version_id": to_version_id,
        "changed_fields": changed_fields,
        "changelog_summary": changelog_summary,
        "changed_by": changed_by,
        "changed_at": chrono::Utc::now().to_rfc3339(),
    });

    let content = format!(
        "@{} updated to v{} ({} changed)",
        agent_name,
        to_version_number,
        if changed_fields.is_empty() {
            "no field set".to_string()
        } else {
            changed_fields.join(", ")
        },
    );

    for row in workspaces {
        let workspace_id: Uuid = match row.try_get("workspace_id") {
            Ok(id) => id,
            Err(_) => continue,
        };

        let msg = agent_bestiary_memory::WorkspaceMessage {
            message_id: Uuid::new_v4(),
            workspace_id,
            sender_type: "system".to_string(),
            sender_id: "system".to_string(),
            sender_name: Some("System".to_string()),
            content: content.clone(),
            message_type: "system_event".to_string(),
            metadata: body.clone(),
            created_at: chrono::Utc::now(),
        };

        let _ = state.memory_store.store_workspace_message(&msg).await;

        // In-process + cross-replica broadcast — matches the pattern used by
        // every other system_event emitter (see workspace::messages::broadcast_message).
        let msg_json = json!({
            "message_id": msg.message_id,
            "sender_type": msg.sender_type,
            "sender_id": msg.sender_id,
            "sender_name": msg.sender_name,
            "content": msg.content,
            "message_type": msg.message_type,
            "metadata": msg.metadata,
            "created_at": msg.created_at.to_rfc3339(),
        });
        let _ = state.ws_broadcast.send(crate::WorkspaceEvent {
            workspace_id,
            message: msg_json.clone(),
        });
        let pool = state.db.clone();
        let channel = format!("ws_{}", workspace_id.as_simple());
        let payload = serde_json::to_string(&msg_json).unwrap_or_default();
        tokio::spawn(async move {
            let _ = sqlx::query("SELECT pg_notify($1, $2)")
                .bind(&channel)
                .bind(&payload)
                .execute(&pool)
                .await;
        });
    }
}

// ─── Agent Version History ─────────────────────────────────────────

pub async fn list_agent_versions_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let versions = state
        .memory_store
        .list_agent_versions(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "versions": versions.iter().map(|v| json!({
            "version_number": v.version_number,
            "description": v.description,
            "tags": v.tags,
            "model": v.model,
            "visibility": v.visibility,
            "display_alias": v.display_alias,
            "changed_by": v.changed_by,
            "created_at": v.created_at.to_rfc3339(),
        })).collect::<Vec<_>>(),
    })))
}

pub async fn get_agent_version_handler(
    State(state): State<AppState>,
    Path((agent_id, version_num)): Path<(String, i32)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let version = state
        .memory_store
        .get_agent_version(db_agent.agent_id, version_num)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Version not found: {}", e)))?;

    Ok(Json(json!({
        "version_number": version.version_number,
        "description": version.description,
        "system_prompt": version.system_prompt,
        "tags": version.tags,
        "model": version.model,
        "temperature": version.temperature,
        "visibility": version.visibility,
        "display_alias": version.display_alias,
        "changed_by": version.changed_by,
        "created_at": version.created_at.to_rfc3339(),
    })))
}

pub async fn restore_agent_version_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, version_num)): Path<(String, i32)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // v0.10.5: substrate RBAC. Restore is a write (rewinds state), so
    // Edit permission is the minimum. Owner + admin both pass.
    rbac::require_edit(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    // Capture pre-restore version number for the activity-feed event (Doc 12 §
    // Capability 3), same shape as update_agent_handler.
    let from_version_number = state
        .memory_store
        .list_agent_versions(db_agent.agent_id)
        .await
        .ok()
        .and_then(|vs| vs.first().map(|v| v.version_number))
        .unwrap_or(0);

    // Load the target version
    let version = state
        .memory_store
        .get_agent_version(db_agent.agent_id, version_num)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, format!("Version not found: {}", e)))?;

    // Apply as update
    let updates = AgentUpdate {
        description: version.description,
        system_prompt: version.system_prompt,
        visibility: version.visibility,
        tags: Some(version.tags),
        model: version.model,
        temperature: version.temperature,
        display_alias: version.display_alias,
        ..Default::default()
    };

    state
        .memory_store
        .update_agent(db_agent.agent_id, &updates)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Snapshot *after* the restore so MAX(version_number) points at the
    // current effective state (Doc 12 § Capability 1 ordering invariant).
    let new_version = state
        .memory_store
        .create_agent_version(db_agent.agent_id, &user_id)
        .await
        .ok();

    if let Some(ref v) = new_version {
        let to_version_id = v.version_id;
        let to_version_number = v.version_number;
        let agent_uuid = db_agent.agent_id;
        let agent_name = db_agent.agent_name.clone();
        let event_state = state.clone();
        let changelog = format!("restored from v{}", version_num);
        tokio::spawn(async move {
            broadcast_agent_card_updated(
                &event_state,
                agent_uuid,
                &agent_name,
                from_version_number,
                None,
                to_version_number,
                Some(to_version_id),
                &["system_prompt", "model", "tags", "visibility"],
                Some(&changelog),
                "owner",
            )
            .await;
        });
    }

    Ok(Json(json!({
        "message": format!("Restored to version {}", version_num),
        "version_restored": version_num,
        "version_number": new_version.as_ref().map(|v| v.version_number),
        "version_id": new_version.as_ref().map(|v| v.version_id),
    })))
}

pub async fn delete_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // v0.10.5: substrate RBAC. Delete is destructive — Admin (owner
    // or platform admin) required. No share/team can delete an agent,
    // by design.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    state
        .memory_store
        .delete_agent(db_agent.agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to delete agent: {}", e),
            )
        })?;

    Ok(Json(json!({ "message": "Agent deleted successfully" })))
}

// ─── Agent Dependencies ────────────────────────────────────────────

pub async fn get_agent_dependencies_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let card = resolve_agent_card(&state, &db_agent);

    let deps = &card.dependencies;
    if deps.required.is_empty() && deps.optional.is_empty() {
        return Ok(Json(json!({
            "agent_name": db_agent.agent_name,
            "has_dependencies": false,
            "required": [],
            "optional": [],
            "total_hire_cost": 0,
        })));
    }

    let hire_cost = state.gas_fees.agent_hire;

    // Resolve each dependency name to an agent record
    let mut required = Vec::new();
    for name in &deps.required {
        let available = state.memory_store.get_agent_by_name(name).await.is_ok();
        required.push(json!({
            "agent_name": name,
            "available": available,
            "hire_cost": hire_cost,
        }));
    }

    let mut optional = Vec::new();
    for name in &deps.optional {
        let available = state.memory_store.get_agent_by_name(name).await.is_ok();
        optional.push(json!({
            "agent_name": name,
            "available": available,
            "hire_cost": hire_cost,
        }));
    }

    let required_cost = required.len() as i32 * hire_cost;
    let optional_cost = optional.len() as i32 * hire_cost;

    Ok(Json(json!({
        "agent_name": db_agent.agent_name,
        "has_dependencies": true,
        "required": required,
        "optional": optional,
        "required_cost": required_cost,
        "optional_cost": optional_cost,
        "total_hire_cost": hire_cost + required_cost + optional_cost,
    })))
}

// ─── Calibration endpoint (Loop 5) ──────────────────────────────────────────

/// Brier skill score against a base-rate ("climatological") reference.
///
/// Returns `(outcome_base_rate, baseline_brier, skill_score)`.
///
/// A raw Brier mean is not interpretable on its own, and surfacing it as
/// "calibration" overstates what has been measured. On a question set where
/// nearly every outcome resolves NO, a forecaster that simply predicts the base
/// rate on every question scores near-perfectly while demonstrating no skill at
/// all. The 48 World Cup tournament-winner forecasts are exactly that shape —
/// 47 NO, one YES — where a zero-knowledge flat `p = 1/48` earns a mean Brier of
/// 0.0204, i.e. "98% calibrated". Loop 5a was reporting that as a closed loop.
///
/// The reference forecaster predicts the observed base rate `b` on every
/// question. Its mean Brier reduces exactly to `b(1-b)`:
///
/// ```text
///   mean (b - y)^2  =  b(b-1)^2 + (1-b)b^2  =  b(1-b)      for y ∈ {0,1}
/// ```
///
/// `skill = 1 - brier/baseline`. Positive beats the base rate; `<= 0` does not,
/// however flattering the raw score looks. Consumers should gate on skill rather
/// than on `calibration_score` alone.
///
/// Skill is `None` when there are no resolved forecasts, or when every outcome
/// resolved the same way — a degenerate set has `baseline == 0`, leaving no
/// reference to score against (undefined, not infinite).
/// How much the Loop 5 signal currently means, as a machine-readable class.
///
/// Loop 5a closed recently, so every score it emits is provisional. Consumers
/// (notably `moe_router_strategist`, which turns these into routing weights)
/// need that stated rather than inferred from `confidence`, which is a bare
/// ratio and reads as authoritative. Mirrors the `verdict` column in
/// `scripts/loop5_brier_mechanical_check.sql` so the API and the probe agree.
///
/// Deliberately independent of whether the *mechanism* works: a mechanically
/// perfect loop with n=3 is still `provisional`. Use the probe for mechanism.
// Argument order deliberately mirrors the derivation order (count → baseline →
// skill) and the match tuple below: two adjacent `Option<f64>` parameters are
// trivial to transpose at a call site, and doing so silently changes the class.

/// GET /api/agents/:id/calibration
///
/// Returns the agent's measured calibration profile — how accurately its
/// outputs have been validated by ground-truth signals over time.
///
/// Sources:
/// - `eval_signals` where `dimension = "forecast_calibration"` (Brier scores
///   from the BrierEvaluator, inverted so 1.0 = perfect calibration)
/// - `fermi_forecasts` resolved rows citing this agent in `agents_used` (all
///   three element shapes — `agent_id`, `agent_name`, `name`) with a non-null
///   `brier_score`
///
/// Alongside the raw score it returns `brier_skill_score` — performance against
/// a base-rate reference forecaster. Gate "is this loop closed?" on skill, not
/// on `calibration_score`, which is inflated by outcome-skewed question sets.
///
/// Domain decomposition: derived from the agent's `fermi_contract.kg_fact_categories`
/// and `tags` to give per-domain calibration scores where available.
///
/// Used by `moe_router_strategist` Stage 0 via the `get_agent_calibration` MCP tool.
pub use fermi::calibration::{brier_skill, CalibrationQuery};

pub async fn get_agent_calibration_handler(
    State(state): State<AppState>,
    // Optional: this route sits under optional_auth_middleware, and the
    // handler doesn't use the principal anyway. Requiring AuthPrincipal here
    // made the route 401 with "missing authentication context" — which broke
    // the moe_router_strategist's get_agent_calibration read path.
    _principal: Option<AuthPrincipal>,
    Path(agent_id): Path<String>,
    Query(q): Query<CalibrationQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    fermi::calibration::compute_agent_calibration(&state.db, &db_agent, &q)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

// ─── Loop health summary (GET /api/me/loop-health) ────────────────────────────

/// Aggregates the health of all five feedback loops for the authenticated user.
/// Used by the dashboard to surface what needs attention across loops.
pub async fn loop_health_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db = &state.db;
    let memory = &state.memory_store;

    // ── Loop 1: individual learning ─────────────────────────────────────────
    // The previous LIMIT 20 made the flagged list look static after
    // consolidations — process one agent, next-in-queue bubbles up
    // into the slot, list stays at 20. Bumping to LIMIT 100 lets the
    // frontend's needs_attention filter shrink the visible queue
    // correctly: after a successful consolidation the agent's
    // `unconsolidated` drops to 0 and `last_consolidated_at = NOW`,
    // so needs_attention flips to false and the row drops out of
    // the flagged subset that the JS shows by default.
    //
    // Maturity columns (entities/facts/rules/cycles) are correlated subqueries
    // rather than extra LEFT JOINs on purpose: joining episodes AND entities AND
    // facts multiplies rows, so an agent with 20 entities and 30 facts would
    // report 600 of each. The existing episode join is already a GROUP BY, and
    // adding siblings to it is the classic way these counts silently inflate.
    let loop1_rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias,
                a.dreaming_budget_credits, a.dreaming_credits_used,
                a.last_consolidated_at,
                COUNT(e.episode_id) FILTER (WHERE e.consolidated = false) AS unconsolidated,
                COUNT(e.episode_id) AS total_episodes,
                (SELECT COUNT(*) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id AND j.status = 'completed') AS completed_cycles,
                (SELECT COUNT(*) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id AND j.status = 'failed') AS failed_cycles,
                (SELECT COALESCE(SUM(j.rules_rejected), 0) FROM consolidation_jobs j
                  WHERE j.agent_id = a.agent_id) AS rules_rejected,
                (SELECT COUNT(*) FROM entities x       WHERE x.agent_id = a.agent_id) AS entities,
                (SELECT COUNT(*) FROM facts x          WHERE x.agent_id = a.agent_id) AS facts,
                (SELECT COUNT(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS rules
         FROM agents a
         LEFT JOIN episodes e ON e.agent_id = a.agent_id
         WHERE a.user_id = $1 AND a.status != 'archived'
         GROUP BY a.agent_id
         ORDER BY unconsolidated DESC, a.last_consolidated_at ASC NULLS FIRST
         LIMIT 100",
    )
    .bind(&user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let loop1: Vec<Value> = loop1_rows.iter().map(|r| {
        let budget: i32 = r.try_get("dreaming_budget_credits").unwrap_or(0);
        let used: i32 = r.try_get("dreaming_credits_used").unwrap_or(0);
        let unconsolidated: i64 = r.try_get("unconsolidated").unwrap_or(0);
        let last_consolidated: Option<chrono::DateTime<chrono::Utc>> =
            r.try_get("last_consolidated_at").unwrap_or(None);
        let days_since = last_consolidated
            .map(|t| (chrono::Utc::now() - t).num_days())
            .unwrap_or(999);

        // Did the dreaming actually teach the agent anything? A cycle that ran
        // and extracted nothing advances `last_consolidated_at` exactly like a
        // productive one, so backlog and recency alone cannot tell a healthy
        // agent from a loop that is burning credits and learning nothing.
        let gi = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
        let (band, diagnosis) = crate::handlers::dreaming_maturity::classify_maturity(
            crate::handlers::dreaming_maturity::MaturityInputs {
                completed_cycles: gi("completed_cycles"),
                failed_cycles: gi("failed_cycles"),
                entities: gi("entities"),
                facts: gi("facts"),
                rules: gi("rules"),
                rules_rejected: gi("rules_rejected"),
                unconsolidated_episodes: unconsolidated,
                total_episodes: gi("total_episodes"),
            },
        );
        let unproductive =
            band == crate::handlers::dreaming_maturity::DreamMaturity::Unproductive;

        json!({
            "agent_id": r.try_get::<uuid::Uuid,_>("agent_id").ok(),
            "agent_name": r.try_get::<String,_>("agent_name").unwrap_or_default(),
            "display_alias": r.try_get::<Option<String>,_>("display_alias").unwrap_or(None),
            "unconsolidated_episodes": unconsolidated,
            "budget_exhausted": budget > 0 && used >= budget,
            "days_since_dreaming": days_since,
            "maturity": band.as_str(),
            "diagnosis": diagnosis,
            "completed_cycles": gi("completed_cycles"),
            "failed_cycles": gi("failed_cycles"),
            "ontology_size": gi("entities") + gi("facts") + gi("rules"),
            // An unproductive agent always needs attention, however fresh its
            // last cycle looks — that freshness is precisely the illusion.
            "needs_attention": unproductive || unconsolidated > 20 || days_since > 14 || (budget > 0 && used >= budget),
        })
    }).collect();

    let loop1_attention = loop1
        .iter()
        .filter(|r| r["needs_attention"].as_bool().unwrap_or(false))
        .count();

    // ── Loop 2: HITL correction ──────────────────────────────────────────────
    let hitl_rows = sqlx::query(
        "SELECT ae.event_id, ae.agent_id, ae.kind, ae.severity, ae.created_at,
                a.agent_name, a.display_alias
         FROM anomaly_events ae
         JOIN agents a ON a.agent_id = ae.agent_id
         WHERE a.user_id = $1
           AND ae.requires_review = TRUE
           AND ae.resolved_at IS NULL
         ORDER BY ae.severity DESC, ae.created_at ASC
         LIMIT 10",
    )
    .bind(&user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let loop2: Vec<Value> = hitl_rows
        .iter()
        .map(|r| {
            let created: chrono::DateTime<chrono::Utc> = r
                .try_get("created_at")
                .unwrap_or_else(|_| chrono::Utc::now());
            let days_old = (chrono::Utc::now() - created).num_days();
            json!({
                "event_id": r.try_get::<uuid::Uuid,_>("event_id").ok(),
                "agent_id": r.try_get::<uuid::Uuid,_>("agent_id").ok(),
                "agent_name": r.try_get::<String,_>("agent_name").unwrap_or_default(),
                "display_alias": r.try_get::<Option<String>,_>("display_alias").unwrap_or(None),
                "kind": r.try_get::<String,_>("kind").unwrap_or_default(),
                "severity": r.try_get::<String,_>("severity").unwrap_or_default(),
                "days_old": days_old,
            })
        })
        .collect();

    // ── Loop 3: workspace coherence ──────────────────────────────────────────
    let coherence_rows = sqlx::query(
        "SELECT t.id, t.name, t.origin, t.mission,
                MAX(ce.evaluated_at) AS last_coherence_at,
                (SELECT ce2.global_score FROM coherence_evaluations ce2
                 WHERE ce2.workspace_id = t.id
                 ORDER BY ce2.evaluated_at DESC LIMIT 1) AS latest_score
         FROM teams t
         JOIN team_members tm ON tm.team_id = t.id
         LEFT JOIN coherence_evaluations ce ON ce.workspace_id = t.id
         WHERE tm.member_id = $1
           AND tm.role IN ('owner', 'admin')
           AND t.origin NOT IN ('rabble_swarm', 'personal_workspace')
           AND (t.archived_at IS NULL)
         GROUP BY t.id
         ORDER BY last_coherence_at ASC NULLS FIRST
         LIMIT 10",
    )
    .bind(&user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let loop3: Vec<Value> = coherence_rows
        .iter()
        .map(|r| {
            let last_eval: Option<chrono::DateTime<chrono::Utc>> =
                r.try_get("last_coherence_at").unwrap_or(None);
            let hours_since = last_eval
                .map(|t| (chrono::Utc::now() - t).num_hours())
                .unwrap_or(9999);
            let score: Option<f64> = r.try_get("latest_score").unwrap_or(None);
            json!({
                "workspace_id": r.try_get::<uuid::Uuid,_>("id").ok(),
                "name": r.try_get::<String,_>("name").unwrap_or_default(),
                "origin": r.try_get::<String,_>("origin").unwrap_or_default(),
                "mission": r.try_get::<Option<String>,_>("mission").unwrap_or(None),
                "latest_coherence_score": score,
                "hours_since_coherence": hours_since,
                "needs_attention": hours_since > 48 || score.map(|s| s < 0.4).unwrap_or(false),
            })
        })
        .collect();

    let loop3_attention = loop3
        .iter()
        .filter(|r| r["needs_attention"].as_bool().unwrap_or(false))
        .count();

    // ── Loop 4: composition evolution proposals ──────────────────────────────
    let proposals_rows = sqlx::query(
        "SELECT cv.composition_version_id, cv.workspace_id, cv.version_number,
                cv.diff_summary, cv.proposed_by, cv.created_at,
                t.name AS workspace_name
         FROM composition_versions cv
         JOIN teams t ON t.id = cv.workspace_id
         JOIN team_members tm ON tm.team_id = t.id
         WHERE tm.member_id = $1
           AND tm.role IN ('owner', 'admin')
           AND cv.accepted_by IS NULL
           AND cv.rejected_by IS NULL
         ORDER BY cv.created_at DESC
         LIMIT 10",
    )
    .bind(&user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let loop4: Vec<Value> = proposals_rows
        .iter()
        .map(|r| {
            let created: chrono::DateTime<chrono::Utc> = r
                .try_get("created_at")
                .unwrap_or_else(|_| chrono::Utc::now());
            json!({
                "version_id": r.try_get::<uuid::Uuid,_>("composition_version_id").ok(),
                "workspace_id": r.try_get::<uuid::Uuid,_>("workspace_id").ok(),
                "workspace_name": r.try_get::<String,_>("workspace_name").unwrap_or_default(),
                "version_number": r.try_get::<i32,_>("version_number").unwrap_or(0),
                "diff_summary": r.try_get::<Option<String>,_>("diff_summary").unwrap_or(None),
                "proposed_by": r.try_get::<Option<String>,_>("proposed_by").unwrap_or(None),
                "days_pending": (chrono::Utc::now() - created).num_days(),
            })
        })
        .collect();

    // ── Loop 5: calibration ──────────────────────────────────────────────────
    // `calibration_score` here is a TEAM number: the mean Brier of the
    // forecasts an agent participated in. On a composition that cites every
    // member on every forecast it is identical across members by construction,
    // which is why the dashboard showed four football agents at an identical
    // "99% · n=48" — one team score rendered four times, not four measurements.
    //
    // Worse, 99% was itself an artefact: on a 48-team tournament where 47
    // resolve NO, a forecaster that knows nothing scores ~98%. So the card was
    // presenting base-rate skew as near-perfect per-agent calibration.
    //
    // Two corrections here. `brier_skill_score` measures performance against a
    // forecaster that predicts the base rate on every question (<= 0 means no
    // skill, however flattering the raw number). `mean_contribution` is the
    // real per-agent signal — exact Shapley credit from counterfactual subset
    // re-runs — gated on both validity checks, and null until attributed
    // forecasts exist.
    let cal_rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias,
                COUNT(f.id) FILTER (WHERE f.brier_score IS NOT NULL) AS n_resolved,
                AVG(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS avg_brier,
                AVG(CASE WHEN f.actual_outcome THEN 1.0 ELSE 0.0 END)
                    FILTER (WHERE f.brier_score IS NOT NULL) AS base_rate,
                (SELECT AVG(c.shapley_value)
                   FROM forecast_agent_credit c
                   JOIN forecast_attributions at
                     ON at.forecast_id = c.forecast_id
                    AND at.neutralisation = c.neutralisation
                  WHERE c.agent_id = a.agent_id
                    AND c.neutralisation = 'identity'
                    AND at.efficiency_residual < 1e-6
                    AND (at.reconstruction_error IS NULL OR at.reconstruction_error < 0.01)
                ) AS mean_contribution,
                (SELECT COUNT(*)
                   FROM forecast_agent_credit c2
                  WHERE c2.agent_id = a.agent_id AND c2.neutralisation = 'identity'
                ) AS n_attributed
         FROM agents a
         LEFT JOIN fermi_forecasts f ON f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
           AND f.status = 'resolved'
         WHERE a.user_id = $1
           AND a.status != 'archived'
           AND (a.fermi_contract IS NOT NULL OR a.output_contract IS NOT NULL)
         GROUP BY a.agent_id
         ORDER BY n_resolved DESC",
    )
    .bind(&user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let loop5: Vec<Value> = cal_rows.iter().map(|r| {
        let n: i64 = r.try_get("n_resolved").unwrap_or(0);
        let avg_brier: Option<f64> = r.try_get("avg_brier").unwrap_or(None);
        let calibration = avg_brier.map(|b| 1.0 - b.clamp(0.0, 1.0));
        let confidence = (n as f64 / 20.0).min(1.0);
        let base_rate: Option<f64> = r.try_get("base_rate").unwrap_or(None);
        let n_yes = base_rate.map(|b| (b * n as f64).round() as usize).unwrap_or(0);
        let (_, baseline, skill) = brier_skill(avg_brier, n_yes, n as usize);
        let contribution: Option<f64> = r.try_get("mean_contribution").unwrap_or(None);
        let n_attributed: i64 = r.try_get("n_attributed").unwrap_or(0);
        json!({
            "agent_id": r.try_get::<uuid::Uuid,_>("agent_id").ok(),
            "agent_name": r.try_get::<String,_>("agent_name").unwrap_or_default(),
            "display_alias": r.try_get::<Option<String>,_>("display_alias").unwrap_or(None),
            "n_resolved": n,
            // Team-scoped. Kept for continuity; do not read as agent skill.
            "calibration_score": calibration,
            "calibration_scope": "team",
            "confidence": confidence,
            // Is that team number informative, or base-rate skew?
            "brier_baseline": baseline,
            "brier_skill_score": skill,
            "beats_base_rate": skill.map(|s| s > 0.0),
            // Agent-scoped: the real Loop 5 signal.
            "mean_contribution": contribution,
            "n_attributed": n_attributed,
            "status": if n == 0 { "cold" } else if confidence < 0.5 { "warming" } else { "warm" },
            // A high calibration_score with no skill is the trap this card fell
            // into; surface it so the UI can label rather than celebrate it.
            "warning": match (skill, n) {
                (Some(s), _) if s <= 0.0 =>
                    Some("Raw calibration is base-rate skew, not skill — this agent does not beat always predicting the base rate."),
                (None, x) if x > 0 =>
                    Some("All outcomes resolved the same way; skill is undefined and the raw score is uninformative."),
                _ => None,
            },
        })
    }).collect();

    let loop5_cold = loop5
        .iter()
        .filter(|r| r["status"].as_str() == Some("cold"))
        .count();
    let loop5_warm = loop5
        .iter()
        .filter(|r| r["status"].as_str() == Some("warm"))
        .count();

    Ok(Json(json!({
        "loop1": {
            "label": "Learning",
            "agents": loop1,
            "needs_attention": loop1_attention,
            "status": if loop1_attention > 0 { "amber" } else { "green" },
        },
        "loop2": {
            "label": "Correction",
            "queue": loop2,
            "unreviewed": loop2.len(),
            "status": if !loop2.is_empty() { if loop2.iter().any(|r| r["severity"].as_str() == Some("critical")) { "red" } else { "amber" } } else { "green" },
        },
        "loop3": {
            "label": "Coherence",
            "workspaces": loop3,
            "needs_attention": loop3_attention,
            "status": if loop3_attention > 0 { "amber" } else { "green" },
        },
        "loop4": {
            "label": "Evolution",
            "proposals": loop4,
            "pending": loop4.len(),
            "status": if !loop4.is_empty() { "amber" } else { "green" },
        },
        "loop5": {
            "label": "Calibration",
            "agents": loop5,
            "warm": loop5_warm,
            "cold": loop5_cold,
            "status": if loop5_warm == 0 && !loop5.is_empty() { "amber" } else { "green" },
        },
    })))
}

// ─── Doc 12 § Capability 4 — calibration query types ────────────────────────
//
// Used by `get_agent_calibration_handler` above. When the caller passes
// `?partition_by=version`, the handler attaches a `version_partition` block
// to its response carrying per-version observation counts from
// `sosa_observations.produced_by_version_*` (stamped by Doc 12 § Capability 1).

// ══════════════════════════════════════════════════════════════
// Remote MCP servers
// ══════════════════════════════════════════════════════════════
//
// Writes go through the normal `PUT /api/agents/:agent_id` with an
// `mcp_servers` field, so they inherit the existing RBAC ladder, agent
// versioning, and the `agent_card.updated` broadcast for free. These two
// endpoints cover what a plain PUT can't express.

/// `GET /api/agents/:agent_id/mcp-servers`
///
/// The effective server list plus where it came from, so the UI can
/// distinguish "this agent has no servers" from "this agent inherits its
/// servers from a filesystem card" — which determines whether saving
/// needs to seed the DB first.
///
/// Credential *values* are never returned. Only the key name and whether
/// it currently resolves, so an operator can see at a glance which
/// servers are one secret away from working.
pub async fn get_agent_mcp_servers_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Reading config is an edit-level concern: endpoints and credential
    // key names are operational detail, not catalogue metadata.
    rbac::require_edit(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    let db_declared = db_agent
        .mcp_servers
        .as_ref()
        .map(|v| !v.is_null())
        .unwrap_or(false);

    // The resolved card applies the DB-overrides-file precedence, so this
    // is exactly what the executor will use.
    let card = crate::resolve_agent_card(&state, &db_agent);
    let servers = &card.capabilities.mcp_servers;

    // Which secrets this owner actually has, so we can report resolvability
    // without ever echoing a value.
    let owner_secrets = crate::resolve_agent_owner_secrets(&state, &db_agent).await;

    let items: Vec<Value> = servers
        .iter()
        .map(|s| {
            let cred_keys = s.credential_key_names();
            let resolved = cred_keys.iter().any(|k| {
                owner_secrets
                    .as_ref()
                    .and_then(|m| m.get(k))
                    .map(|v| !v.trim().is_empty())
                    .unwrap_or(false)
                    || std::env::var(k)
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false)
            });
            json!({
                "name": s.name,
                "endpoint": s.endpoint,
                "transport": s.transport,
                "tool_allowlist": s.tool_allowlist,
                "timeout_secs": s.timeout_secs,
                "protocol_version": s.protocol_version,
                // The auth block minus its value. Projected explicitly so a
                // read → edit → save round trip is lossless: without
                // scheme/header, a client has to guess "bearer" and would
                // silently downgrade a server using a raw custom header.
                // `secret_key`/`env` are key NAMES, never values.
                "auth": s.auth.as_ref().map(|a| json!({
                    "scheme": a.scheme,
                    "header": a.header,
                    "secret_key": a.secret_key,
                    "env": a.env,
                })),
                // Diagnostics, not secrets.
                "credential_keys": cred_keys,
                "credential_required": !cred_keys.is_empty(),
                "credential_resolved": resolved,
                "transport_supported": s.http_endpoint().is_ok(),
                "transport_error": s.http_endpoint().err(),
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_id": db_agent.agent_id,
        "agent_name": db_agent.agent_name,
        "servers": items,
        // false => the list is inherited from the filesystem card and the
        // first save must persist it to the DB (which then becomes
        // authoritative). true => the DB already owns this config.
        "db_is_authoritative": db_declared,
        "source": if db_declared { "database" } else { "agent_card_file" },
    })))
}

#[derive(Deserialize)]
pub struct TestMcpServerRequest {
    /// A candidate server config. Not required to be saved first — the
    /// point is to validate *before* persisting.
    #[serde(flatten)]
    pub server: fermi::agent_backend::mcp_client::RemoteMcpServer,
}

/// `POST /api/agents/:agent_id/mcp-servers/test`
///
/// Attempt discovery against a candidate server and report what happened.
///
/// This exists because every failure mode here is external and silent
/// otherwise: a wrong endpoint, a missing credential, a server whose
/// `tools/list` is open but whose `tools/call` is gated. Without a test
/// action an operator saves a config and finds out only when an agent run
/// mysteriously has no tools.
///
/// Uses the agent owner's secret scope, so a successful test means *this
/// agent* can really reach the server — not merely that the platform can.
pub async fn test_agent_mcp_server_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(req): Json<TestMcpServerRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    rbac::require_edit(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    let mut server = req.server;
    if server.name.trim().is_empty() {
        server.name = "candidate".to_string();
    }

    let owner_secrets = crate::resolve_agent_owner_secrets(&state, &db_agent).await;
    let cat = fermi::agent_backend::mcp_client::RemoteMcpCatalogue::discover(
        std::slice::from_ref(&server),
        owner_secrets.as_ref(),
    )
    .await;

    let failure = cat.failures.first().map(|(_, e)| e.clone());

    Ok(Json(json!({
        "ok": failure.is_none() && !cat.is_empty(),
        "error": failure,
        "tool_count": cat.len(),
        // Namespaced exactly as the model will see them, so the operator
        // is previewing the real tool surface.
        "tools": cat.tools().iter().map(|t| json!({
            "name": t.qualified_name,
            "remote_name": t.remote_name,
            "description": t.description,
        })).collect::<Vec<_>>(),
    })))
}

/// `GET /api/agents/:agent_id/published-tools`
///
/// What this agent publishes over `/mcp/agents/:id`, plus the full menu of
/// what it *could* publish, so the UI can render checkboxes rather than
/// asking an operator to type tool names from memory.
///
/// Writes go through `PUT /api/agents/:agent_id` with an `mcp_tools` field,
/// which validates every name against the dispatch table.
pub async fn get_agent_published_tools_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    rbac::require_edit(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_effective_visibility(&db_agent),
    )
    .await?;

    let db_declared = db_agent
        .mcp_tools
        .as_ref()
        .map(|v| !v.is_null())
        .unwrap_or(false);

    let card = resolve_agent_card(&state, &db_agent);
    let published: Vec<String> = card
        .capabilities
        .mcp_tools
        .iter()
        .map(|t| t.name.clone())
        .collect();

    // Everything the compile-time dispatcher can run.
    let platform: Vec<Value> = fermi::agent_backend::tools::platform_tools()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "input_schema": t.input_schema,
                "requires_workspace": t.requires_workspace,
                "is_delegation": t.is_delegation,
                "published": published.iter().any(|p| p == t.name),
                "kind": "platform",
            })
        })
        .collect();

    // Remote tools this agent could re-publish, namespaced as dispatch will
    // generate them. Discovery is best-effort: a third-party endpoint being
    // down must not stop an operator editing the rest of the list.
    let mut remote: Vec<Value> = Vec::new();
    let mut remote_errors: Vec<Value> = Vec::new();
    if !card.capabilities.mcp_servers.is_empty() {
        let owner_secrets = crate::resolve_agent_owner_secrets(&state, &db_agent).await;
        let cat = fermi::agent_backend::mcp_client::RemoteMcpCatalogue::discover(
            &card.capabilities.mcp_servers,
            owner_secrets.as_ref(),
        )
        .await;
        for t in cat.tools() {
            remote.push(json!({
                "name": t.qualified_name,
                "description": t.description,
                "input_schema": t.input_schema,
                "requires_workspace": false,
                "is_delegation": false,
                "published": published.iter().any(|p| p == &t.qualified_name),
                "kind": "remote",
            }));
        }
        for (server, err) in &cat.failures {
            remote_errors.push(json!({ "server": server, "error": err }));
        }
    }

    // Anything published that we can't account for is a phantom tool: it
    // will be advertised and then fail with `Unknown tool`. Surfaced rather
    // than hidden, because pre-validation cards could contain these.
    let known: Vec<&str> = platform
        .iter()
        .chain(remote.iter())
        .filter_map(|t| t["name"].as_str())
        .collect();
    let phantom: Vec<&String> = published
        .iter()
        .filter(|p| !known.contains(&p.as_str()))
        .collect();

    Ok(Json(json!({
        "agent_id": db_agent.agent_id,
        "agent_name": db_agent.agent_name,
        "published": published,
        "available": platform.into_iter().chain(remote).collect::<Vec<_>>(),
        "remote_discovery_errors": remote_errors,
        "phantom": phantom,
        // false => inherited from the filesystem card; the first save
        // copies it into the DB, which then becomes authoritative.
        "db_is_authoritative": db_declared,
        "source": if db_declared { "database" } else { "agent_card_file" },
        // The MCP endpoint external clients point at.
        "mcp_endpoint": format!("/mcp/agents/{}", db_agent.agent_name),
    })))
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Lifecycle-field guard on PUT ────────────────────────────
    //
    // These are the regression tests for the publish-gate bypass: before
    // this guard, `PUT /api/agents/:id` wrote `status` and `visibility`
    // straight into the SET clause, so the publish pipeline (checks, fee,
    // transition validation, admin-only force, bypass audit) could be
    // skipped entirely by anyone with edit rights on the agent.

    #[test]
    fn put_rejects_status_change() {
        let updates = AgentUpdate {
            status: Some("published".into()),
            ..Default::default()
        };
        let err = reject_lifecycle_fields(&updates, "my_agent").unwrap_err();
        assert!(err.contains("cannot change status"), "got: {err}");
        assert!(err.contains("/api/agents/my_agent/publish"), "got: {err}");
    }

    #[test]
    fn put_rejects_visibility_change() {
        let updates = AgentUpdate {
            visibility: Some("public".into()),
            ..Default::default()
        };
        let err = reject_lifecycle_fields(&updates, "my_agent").unwrap_err();
        assert!(err.contains("cannot change visibility"), "got: {err}");
    }

    /// The exact bypass that made the publish gate optional.
    #[test]
    fn put_rejects_combined_publish_bypass() {
        let updates = AgentUpdate {
            status: Some("published".into()),
            visibility: Some("public".into()),
            ..Default::default()
        };
        let err = reject_lifecycle_fields(&updates, "zk_authored_probe").unwrap_err();
        assert!(
            err.contains("cannot change status and visibility"),
            "got: {err}"
        );
    }

    /// Ordinary card edits must still pass. The guard is about lifecycle,
    /// not about freezing the card.
    #[test]
    fn put_allows_non_lifecycle_edits() {
        let updates = AgentUpdate {
            description: Some("a better description".into()),
            system_prompt: Some("a better prompt".into()),
            tags: Some(vec!["research".into()]),
            temperature: Some(0.4),
            ..Default::default()
        };
        assert!(reject_lifecycle_fields(&updates, "my_agent").is_ok());
    }

    /// A no-op PUT is not an error.
    #[test]
    fn put_allows_empty_update() {
        assert!(reject_lifecycle_fields(&AgentUpdate::default(), "my_agent").is_ok());
    }

    // ─── Effective visibility ──────────────────────────────────

    fn agent_with(status: &str, visibility: &str) -> Agent {
        Agent {
            agent_id: uuid::Uuid::nil(),
            agent_name: "probe".into(),
            agent_type: default_agent_type(),
            version: "1.0.0".into(),
            tier: "community".into(),
            executor_type: default_executor(),
            model: default_model(),
            temperature: default_temperature(),
            mcp_servers: None,
            mcp_tools: None,
            description: None,
            author: "tester".into(),
            system_prompt: None,
            visibility: visibility.into(),
            owner_id: Some("tester".into()),
            tags: vec![],
            current_ontology_commit: None,
            current_ontology_snapshot_id: None,
            last_consolidated_at: None,
            total_executions: 0,
            successful_executions: 0,
            failed_executions: 0,
            total_cost_usd: None,
            avg_execution_time_ms: 0,
            dreaming_budget_credits: 0,
            dreaming_credits_used: 0,
            dreaming_budget_reset_at: None,
            education_budget_credits: 0,
            education_credits_used: 0,
            auto_collect_pct: 0,
            display_alias: None,
            llm_provider: default_llm_provider(),
            embedding_provider: default_embedding_provider(),
            embedding_model: default_embedding_model(),
            embedding_dimension: default_embedding_dimension(),
            sample_queries: vec![],
            status: status.into(),
            fork_pricing: None,
            forked_from: None,
            fork_count: 0,
            accepts: vec![],
            produces: vec![],
            workflow_template: None,
            prompt_template: None,
            requires_secrets: None,
            model_ladder: json!([]),
            min_tier: "free".into(),
            capability_gates: json!({}),
            persona_version: 1,
            fermi_contract: None,
            model_params: json!({}),
            valence: None,
            output_contract: None,
            taxonomy: None,
        }
    }

    /// The rule the `/agent/:agent_id/*` page guard now shares with the
    /// JSON API: public requires BOTH published and public. A draft with
    /// `visibility='public'` is still author-only — which is precisely
    /// the state that used to render a crawlable public page.
    #[test]
    fn public_requires_both_published_and_public() {
        assert_eq!(
            agent_effective_visibility(&agent_with("published", "public")),
            Visibility::Public
        );
        assert_eq!(
            agent_effective_visibility(&agent_with("draft", "public")),
            Visibility::Private
        );
        assert_eq!(
            agent_effective_visibility(&agent_with("published", "private")),
            Visibility::Private
        );
        assert_eq!(
            agent_effective_visibility(&agent_with("archived", "public")),
            Visibility::Private
        );
        assert_eq!(
            agent_effective_visibility(&agent_with("draft", "unlisted")),
            Visibility::Shared
        );
    }

    /// Doc 12 § Capability 3 — `collect_changed_fields` must surface every
    /// field the PUT body sets, so the activity-feed event can render
    /// "system_prompt and model_ladder changed" without diffing.
    #[test]
    fn collect_changed_fields_lists_every_set_field() {
        let updates = AgentUpdate {
            system_prompt: Some("new prompt".to_string()),
            model_ladder: Some(json!([{"tier": "premium"}])),
            ..Default::default()
        };
        let fields = collect_changed_fields(&updates);
        assert!(fields.contains(&"system_prompt"));
        assert!(fields.contains(&"model_ladder"));
        assert_eq!(fields.len(), 2);
    }

    /// Empty update — used by clients that PUT with no body to bump a
    /// version manually. Field list is empty; the activity event still
    /// renders with `(no field set)` per the broadcast formatting.
    #[test]
    fn collect_changed_fields_is_empty_when_no_fields_set() {
        let updates = AgentUpdate::default();
        let fields = collect_changed_fields(&updates);
        assert!(fields.is_empty());
    }

    /// Verify every field on AgentUpdate has a matching arm in
    /// `collect_changed_fields`. If a new field is added to the struct
    /// and a maintainer forgets to wire it here, the agent_card.updated
    /// event silently loses signal. This test fires on every full-set
    /// AgentUpdate to keep the two in sync.
    #[test]
    fn collect_changed_fields_covers_every_agent_update_field() {
        let updates = AgentUpdate {
            description: Some("d".into()),
            system_prompt: Some("s".into()),
            visibility: Some("v".into()),
            tags: Some(vec!["t".into()]),
            model: Some("m".into()),
            temperature: Some(0.1),
            education_budget_credits: Some(1),
            display_alias: Some("a".into()),
            status: Some("s".into()),
            fork_pricing: Some(json!({})),
            accepts: Some(vec!["x".into()]),
            produces: Some(vec!["y".into()]),
            workflow_template: Some(json!({})),
            prompt_template: Some("p".into()),
            requires_secrets: Some(json!([])),
            mcp_servers: Some(json!([])),
            mcp_tools: Some(json!([])),
            llm_provider: Some("anthropic".into()),
            model_ladder: Some(json!([])),
            min_tier: Some("free".into()),
            capability_gates: Some(json!({})),
            model_params: Some(json!({})),
            valence: Some(json!({})),
            output_contract: Some(json!({})),
            version: Some("1.0.0".into()),
            taxonomy: Some(json!({})),
        };
        let fields = collect_changed_fields(&updates);
        // 26 fields on AgentUpdate today — if the count drifts here,
        // either a field was added (good — wire it up above) or a
        // maintainer wired one twice (bad — dedupe).
        assert_eq!(
            fields.len(),
            26,
            "AgentUpdate has fields that collect_changed_fields doesn't cover: got {:?}",
            fields
        );
    }
}
