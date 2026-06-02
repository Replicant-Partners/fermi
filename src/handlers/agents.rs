//! Agent handlers — CRUD, listing, import, versions, avatar, catalogue.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{credit_charge, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::{Agent, AgentUpdate, Episode};

use crate::{
    resolve_agent, resolve_agent_card, AppState, GeminiContent,
    GeminiGenerationConfig, GeminiPart, GeminiRequest, GeminiResponse,
};

#[derive(Debug, Deserialize)]
pub struct ListAgentsParams {
    search: Option<String>,
    tag: Option<String>,
    tags: Option<String>, // comma-separated, OR semantics
    sort: Option<String>, // "newest", "executions", "name"
    page: Option<usize>,
    limit: Option<usize>,
}

pub async fn list_agents(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    Query(params): Query<ListAgentsParams>,
) -> Json<Value> {
    let caller_id = caller.map(|Extension(p)| p.user_id());

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
            .filter(|a| !a.agent_name.starts_with("test_agent_"))
            .filter(|a| {
                // Owner always sees their own agents (any status)
                if let Some(ref uid) = caller_id {
                    if a.owner_id.as_deref() == Some(uid.as_str()) {
                        return true;
                    }
                }
                // Everyone else: only published + public
                a.status == "published" && a.visibility == "public"
            })
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

        if !page_agents.is_empty() || total > 0 {
            let agents: Vec<Value> = page_agents
                .iter()
                .map(|a| {
                    // Merge filesystem card data if available
                    let card = state.registry.get(&a.agent_name).ok();
                    let card_json = card.as_ref().and_then(|_c| {
                        let path = format!("agents/curated/{}/agent_card.json", a.agent_name);
                        std::fs::read_to_string(&path).ok()
                            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                    });

                    let owner_display = a.owner_id.as_deref()
                        .and_then(|oid| owner_names.get(oid))
                        .cloned();

                    let mut agent_val = json!({
                        "agent_id": a.agent_name,
                        "uuid": a.agent_id,
                        "display_alias": a.display_alias.as_deref().unwrap_or(""),
                        "agent_type": a.agent_type,
                        "version": a.version,
                        "tier": a.tier,
                        "description": a.description.as_deref().unwrap_or(""),
                        "author": a.author,
                        "model": a.model,
                        "llm_provider": a.llm_provider,
                        "model_ladder": a.model_ladder,
                        "min_tier": a.min_tier,
                        "capability_gates": a.capability_gates,
                        "tags": a.tags,
                        "sample_queries": a.sample_queries,
                        "visibility": a.visibility,
                        "owner_id": a.owner_id.as_deref().unwrap_or(""),
                        "owner_display_name": owner_display,
                        "system_prompt": a.system_prompt.as_deref().unwrap_or(""),
                        "status": a.status,
                        "fork_pricing": a.fork_pricing,
                        "forked_from": a.forked_from,
                        "fork_count": a.fork_count,
                        "accepts": a.accepts,
                        "produces": a.produces,
                        "workflow_template": a.workflow_template,
                        "prompt_template": a.prompt_template,
                        "requires_secrets": a.requires_secrets,
                        "model_params": a.model_params,
                        "capabilities": {
                            "executor": a.executor_type,
                            "model": a.model,
                            "temperature": a.temperature,
                            "mcp_tools": card.as_ref().map(|c| c.capabilities.mcp_tools.iter().map(|t| json!({"name": t.name, "description": t.description})).collect::<Vec<_>>()).unwrap_or_default(),
                            "skills": card.as_ref().map(|c| c.capabilities.skills.clone()).unwrap_or_default(),
                        },
                        "ontology_stats": {
                            "last_updated": a.last_consolidated_at,
                            "current_commit": a.current_ontology_commit,
                        },
                        "execution_stats": {
                            "total_executions": a.total_executions,
                            "successful_executions": a.successful_executions,
                            "failed_executions": a.failed_executions,
                            "total_cost_usd": a.total_cost_usd,
                            "avg_execution_time_ms": a.avg_execution_time_ms,
                        },
                        "dreaming": {
                            "budget_credits": a.dreaming_budget_credits,
                            "credits_used": a.dreaming_credits_used,
                            "credits_remaining": a.dreaming_budget_credits - a.dreaming_credits_used,
                        },
                        "workspace_count": workspace_counts.get(&a.agent_id).copied().unwrap_or(0),
                        "source": "database",
                    });

                    // Overlay rich fields from filesystem card
                    if let Some(cj) = &card_json {
                        if let Some(obj) = agent_val.as_object_mut() {
                            // Metadata (tags, created date)
                            if let Some(meta) = cj.get("metadata") {
                                obj.insert("metadata".to_string(), meta.clone());
                            }
                            // Performance stats
                            if let Some(perf) = cj.get("performance") {
                                obj.insert("performance".to_string(), perf.clone());
                            }
                            // Usage stats
                            if let Some(usage) = cj.get("usage") {
                                obj.insert("usage".to_string(), usage.clone());
                            }
                            // Wallet
                            if let Some(wallet) = cj.get("wallet") {
                                obj.insert("wallet".to_string(), wallet.clone());
                            }
                            // Ontology stats from card (entities/relationships counts)
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
    "anthropic".to_string()
}
pub fn default_embedding_model() -> String {
    "voyage-2".to_string()
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
            {"id": "anthropic", "name": "Voyage-2 (Anthropic)", "model": "voyage-2", "dimension": 1024, "env_var": "ANTHROPIC_API_KEY", "available": check_env("ANTHROPIC_API_KEY")},
            {"id": "openai", "name": "text-embedding-3-large (OpenAI)", "model": "text-embedding-3-large", "dimension": 1024, "env_var": "OPENAI_API_KEY", "available": check_env("OPENAI_API_KEY")},
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
        mcp_servers: caps.and_then(|c| c.get("mcp_tools")).cloned(),
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
        embedding_provider: "anthropic".to_string(),
        embedding_model: "voyage-2".to_string(),
        embedding_dimension: 1024,
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
        fermi_contract: card
            .get("capabilities")
            .and_then(|c| c.get("fermi_contract"))
            .cloned(),
        model_params: card
            .get("capabilities")
            .and_then(|c| c.get("model_params"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        valence: card
            .get("metadata")
            .and_then(|m| m.get("valence"))
            .cloned(),
        output_contract: card
            .get("capabilities")
            .and_then(|c| c.get("output_contract"))
            .cloned(),
    };

    let agent_id = state.memory_store.create_agent(&agent).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to import agent: {}", e),
        )
    })?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_name": agent_name,
        "message": "Agent imported successfully"
    })))
}

// ─── Custom embeddings import endpoint ─────────────────────────────

#[derive(Deserialize)]
pub struct ImportEmbeddingsRequest {
    episodes: Vec<ImportedEpisode>,
}

#[derive(Deserialize)]
pub struct ImportedEpisode {
    query: String,
    summary: Option<String>,
    embedding: Vec<f32>,
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

    if agent.owner_id.as_deref() != Some(&user_id) {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".to_string()));
    }

    if req.episodes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No episodes provided".to_string()));
    }

    // Validate embedding dimensions
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

    // Create episodes with provided embeddings
    let mut imported = 0;
    for ep in &req.episodes {
        let episode = Episode {
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

        state
            .memory_store
            .store_episode(episode)
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

    let agents = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list agents: {}", e),
            )
        })?;

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
            let total_count = abw_names.len() as i32
                + other_counts.values().sum::<i32>();
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
            })
        })
        .collect();

    Ok(Json(json!({ "agents": agent_list })))
}

pub async fn update_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(updates): Json<AgentUpdate>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let user_id = principal.user_id();

    // Owner check
    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
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
    let workspaces = match sqlx::query(
        "SELECT DISTINCT workspace_id FROM workspace_agents WHERE agent_id = $1",
    )
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

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((StatusCode::FORBIDDEN, "Not the owner".to_string()));
    }

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

    // Owner check
    if db_agent.owner_id.as_deref() != Some(&principal.user_id()) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

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

/// GET /api/agents/:id/calibration
///
/// Returns the agent's measured calibration profile — how accurately its
/// outputs have been validated by ground-truth signals over time.
///
/// Sources:
/// - `eval_signals` where `dimension = "forecast_calibration"` (Brier scores
///   from the BrierEvaluator, inverted so 1.0 = perfect calibration)
/// - `fermi_forecasts` where `agents_used @> [{agent_id}]` and `brier_score IS NOT NULL`
///
/// Domain decomposition: derived from the agent's `fermi_contract.kg_fact_categories`
/// and `tags` to give per-domain calibration scores where available.
///
/// Used by `moe_router_strategist` Stage 0 via the `get_agent_calibration` MCP tool.
pub async fn get_agent_calibration_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Query(q): Query<CalibrationQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let aid = db_agent.agent_id;

    // ── eval_signals forecast_calibration scores ──────────────────────────────
    let signal_rows = sqlx::query(
        "SELECT score, confidence, created_at
         FROM eval_signals
         WHERE agent_id = $1 AND dimension = 'forecast_calibration'
         ORDER BY created_at DESC
         LIMIT 200",
    )
    .bind(aid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let n_eval = signal_rows.len();
    let eval_mean: Option<f64> = if n_eval > 0 {
        let sum: f64 = signal_rows
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum();
        Some(sum / n_eval as f64)
    } else {
        None
    };

    // Trend: compare last 10 vs prior 10 (if enough data)
    let trend = if n_eval >= 20 {
        let recent: f64 = signal_rows[..10]
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum::<f64>() / 10.0;
        let older: f64 = signal_rows[10..20]
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum::<f64>() / 10.0;
        if recent > older + 0.05 { "improving" }
        else if recent < older - 0.05 { "degrading" }
        else { "stable" }
    } else {
        "insufficient_data"
    };

    // ── fermi_forecasts direct Brier scores ───────────────────────────────────
    let forecast_rows = sqlx::query(
        "SELECT brier_score, tags, question_text, created_at
         FROM fermi_forecasts
         WHERE agents_used @> $1::jsonb
           AND brier_score IS NOT NULL
           AND status = 'resolved'
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(json!([{"agent_id": aid.to_string()}]))
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let n_resolved = forecast_rows.len();
    let brier_mean: Option<f64> = if n_resolved > 0 {
        let sum: f64 = forecast_rows
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("brier_score").ok())
            .sum();
        Some(sum / n_resolved as f64)
    } else {
        None
    };

    // ── Domain decomposition via agent tags ───────────────────────────────────
    // Group forecasts by matching against agent's tag categories.
    // Tags on forecasts are stored in the `tags` JSONB column.
    let mut domain_scores: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();

    for row in &forecast_rows {
        let score: f64 = match row.try_get::<f64, _>("brier_score") {
            Ok(s) => s,
            Err(_) => continue,
        };
        // forecast_calibration = 1 - brier (higher is better)
        let calibration = 1.0 - score.clamp(0.0, 1.0);

        let tags: Vec<String> = row
            .try_get::<serde_json::Value, _>("tags")
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Map forecast tags to domain using agent's own tags as the classifier
        let agent_tags = &db_agent.tags;
        let matched_domain = tags.iter()
            .find(|t| agent_tags.iter().any(|at| at.contains(t.as_str()) || t.contains(at.as_str())))
            .map(|t| t.clone())
            .unwrap_or_else(|| "general".to_string());

        let entry = domain_scores.entry(matched_domain).or_insert((0.0, 0));
        entry.0 += calibration;
        entry.1 += 1;
    }

    let domain_calibration: serde_json::Value = domain_scores
        .iter()
        .map(|(domain, (sum, count))| {
            (domain.clone(), json!({
                "calibration_mean": sum / *count as f64,
                "n": count,
            }))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    // ── eval_signals projection_accuracy scores (SimOps hard-verified) ───────
    // Hard-verified signal: deferred comparison against real SOSA observations.
    // Only populated for simops_dynamics_runner / simops_cascade agents.
    // These are epistemically stronger than LLM-judged signals — the batch
    // resolves independently of the prediction.
    let projection_rows = sqlx::query(
        "SELECT score, confidence, flags, created_at
         FROM eval_signals
         WHERE agent_id = $1 AND dimension = 'projection_accuracy'
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(aid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let n_projection = projection_rows.len();
    let projection_mean: Option<f64> = if n_projection > 0 {
        let sum: f64 = projection_rows
            .iter()
            .filter_map(|r| r.try_get::<f64, _>("score").ok())
            .sum();
        Some(sum / n_projection as f64)
    } else {
        None
    };

    // Per-model breakdown from projection flags
    let mut model_accuracy: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    for row in &projection_rows {
        let score: f64 = match row.try_get::<f64, _>("score") {
            Ok(s) => s,
            Err(_) => continue,
        };
        let flags: serde_json::Value = row
            .try_get::<serde_json::Value, _>("flags")
            .unwrap_or(serde_json::json!({}));
        let model_uri = flags
            .get("model_uri")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let entry = model_accuracy.entry(model_uri).or_insert((0.0, 0));
        entry.0 += score;
        entry.1 += 1;
    }
    let model_accuracy_json: serde_json::Value = model_accuracy
        .iter()
        .map(|(model, (sum, count))| {
            (model.clone(), json!({
                "accuracy_mean": sum / *count as f64,
                "n": count,
            }))
        })
        .collect::<serde_json::Map<_, _>>()
        .into();

    // ── Composite calibration score ───────────────────────────────────────────
    // Priority order (most authoritative first):
    //   1. Direct Brier from resolved fermi_forecasts
    //   2. projection_accuracy from hard-verified SOSA deltas (SimOps)
    //   3. eval_signals forecast_calibration from LLM-judged evaluators
    let calibration_score = match (brier_mean, projection_mean, eval_mean) {
        (Some(b), _, _) => Some(1.0 - b), // Brier inverted: lower = higher calibration
        (None, Some(p), _) => Some(p),    // projection_accuracy: already 0-1 higher=better
        (None, None, Some(e)) => Some(e),
        _ => None,
    };

    // Confidence: saturates at n=20. Count across all signal sources.
    let n_total = n_resolved.max(n_projection).max(n_eval);
    let confidence = (n_total as f64 / 20.0).min(1.0);

    // ── Doc 12 § Capability 4 — optional version partitioning ────────────────
    //
    // When the caller asks for `?partition_by=version`, attach per-version
    // observation counts from `sosa_observations.produced_by_version_*`
    // (stamped by Doc 12 § Capability 1). Honest about the limit: per-version
    // Brier scores stay NULL because `fermi_forecasts.agents_used` doesn't
    // carry `agent_version_id` yet — wiring that is the prerequisite for
    // version-partitioned Brier and is documented in the response.
    let partition_by = q.partition_by.as_deref().unwrap_or("none");
    let window_days = q.window_days.unwrap_or(90).max(1);

    let partitions_block: Option<Value> = if partition_by == "version" {
        let cutoff_ms = (chrono::Utc::now()
            - chrono::Duration::days(window_days))
        .timestamp_millis();

        let part_rows = sqlx::query(
            "SELECT o.produced_by_version_number AS version_number,
                    v.created_at                 AS version_deployed_at,
                    COUNT(*)::BIGINT             AS n_observations
             FROM sosa_observations o
             LEFT JOIN agent_versions v
               ON v.agent_id = $1
              AND v.version_number = o.produced_by_version_number
             WHERE (o.produced_by_agent_id = $2 OR o.produced_by_agent_id = $3)
               AND o.phenomenon_time >= $4
               AND ($5::uuid IS NULL OR o.session_id IN (
                     SELECT session_id FROM observation_sessions WHERE platform_id IN (
                       SELECT platform_id FROM sosa_platforms WHERE owner_id IN (
                         SELECT user_id FROM workspaces WHERE workspace_id = $5))))
             GROUP BY o.produced_by_version_number, v.created_at
             ORDER BY o.produced_by_version_number ASC NULLS LAST",
        )
        .bind(aid)
        .bind(&db_agent.agent_name)
        .bind(aid.to_string())
        .bind(cutoff_ms)
        .bind(q.workspace_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Query failed: {}", e)))?;

        let partitions: Vec<Value> = part_rows
            .iter()
            .map(|r| {
                let vn: Option<i32> = r.try_get("version_number").ok();
                let vt: Option<chrono::DateTime<chrono::Utc>> =
                    r.try_get("version_deployed_at").ok();
                let n: i64 = r.try_get("n_observations").unwrap_or(0);
                calibration_partition_json(vn, vt, n)
            })
            .collect();

        Some(json!({
            "partition_by": "version",
            "window_days": window_days,
            "partitions": partitions,
            // Honest v1 disclosure (Doc 12 § Capability 4). Consumers can
            // read `partitions` for version-stamped observation counts now;
            // per-version Brier requires `agent_version_id` to also be
            // recorded in `fermi_forecasts.agents_used` entries — a
            // downstream change tracked separately from this endpoint.
            "brier_status": "unstamped",
            "brier_note": "Per-version Brier requires version-stamped forecasts in fermi_forecasts.agents_used; current rows surface observation counts only.",
        }))
    } else {
        None
    };

    Ok(Json(json!({
        "agent_id": aid,
        "agent_name": db_agent.agent_name,

        // Primary calibration score (0.0–1.0, higher = better calibrated)
        "calibration_score": calibration_score,
        "confidence": confidence,
        "trend": trend,

        // Source breakdown
        "n_resolved_forecasts": n_resolved,
        "n_eval_signals": n_eval,
        "n_projection_observations": n_projection,
        "brier_mean": brier_mean,                     // direct Brier (lower = better)
        "eval_calibration_mean": eval_mean,           // LLM-judged signals (higher = better)
        "projection_accuracy_mean": projection_mean,  // hard-verified SOSA delta (higher = better)

        // Per-domain decomposition (requires forecast tags to match agent tags)
        "domain_calibration": domain_calibration,

        // Per-model accuracy (SimOps agents: accuracy per dynamics model URI)
        "model_accuracy": model_accuracy_json,

        // Per-version decomposition (Doc 12 § Capability 4). Present only when
        // the caller passed `?partition_by=version`.
        "version_partition": partitions_block,

        // Interpretation
        "interpretation": match calibration_score {
            Some(s) if s >= 0.80 => "well_calibrated",
            Some(s) if s >= 0.65 => "reasonably_calibrated",
            Some(s) if s >= 0.50 => "weakly_calibrated",
            Some(_) => "poorly_calibrated",
            None => "no_data",
        },
        "note": if n_resolved < 5 && n_projection < 5 {
            Some("Fewer than 5 hard-verified observations — calibration estimate is preliminary.")
        } else {
            None
        },
    })))
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
    let loop1_rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias,
                a.dreaming_budget_credits, a.dreaming_credits_used,
                a.last_consolidated_at,
                COUNT(e.episode_id) FILTER (WHERE e.consolidated = false) AS unconsolidated
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
        json!({
            "agent_id": r.try_get::<uuid::Uuid,_>("agent_id").ok(),
            "agent_name": r.try_get::<String,_>("agent_name").unwrap_or_default(),
            "display_alias": r.try_get::<Option<String>,_>("display_alias").unwrap_or(None),
            "unconsolidated_episodes": unconsolidated,
            "budget_exhausted": budget > 0 && used >= budget,
            "days_since_dreaming": days_since,
            "needs_attention": unconsolidated > 20 || days_since > 14 || (budget > 0 && used >= budget),
        })
    }).collect();

    let loop1_attention = loop1.iter().filter(|r| r["needs_attention"].as_bool().unwrap_or(false)).count();

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

    let loop2: Vec<Value> = hitl_rows.iter().map(|r| {
        let created: chrono::DateTime<chrono::Utc> =
            r.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());
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
    }).collect();

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

    let loop3: Vec<Value> = coherence_rows.iter().map(|r| {
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
    }).collect();

    let loop3_attention = loop3.iter().filter(|r| r["needs_attention"].as_bool().unwrap_or(false)).count();

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

    let loop4: Vec<Value> = proposals_rows.iter().map(|r| {
        let created: chrono::DateTime<chrono::Utc> =
            r.try_get("created_at").unwrap_or_else(|_| chrono::Utc::now());
        json!({
            "version_id": r.try_get::<uuid::Uuid,_>("composition_version_id").ok(),
            "workspace_id": r.try_get::<uuid::Uuid,_>("workspace_id").ok(),
            "workspace_name": r.try_get::<String,_>("workspace_name").unwrap_or_default(),
            "version_number": r.try_get::<i32,_>("version_number").unwrap_or(0),
            "diff_summary": r.try_get::<Option<String>,_>("diff_summary").unwrap_or(None),
            "proposed_by": r.try_get::<Option<String>,_>("proposed_by").unwrap_or(None),
            "days_pending": (chrono::Utc::now() - created).num_days(),
        })
    }).collect();

    // ── Loop 5: calibration ──────────────────────────────────────────────────
    let cal_rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias,
                COUNT(f.id) FILTER (WHERE f.brier_score IS NOT NULL) AS n_resolved,
                AVG(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL) AS avg_brier
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
        json!({
            "agent_id": r.try_get::<uuid::Uuid,_>("agent_id").ok(),
            "agent_name": r.try_get::<String,_>("agent_name").unwrap_or_default(),
            "display_alias": r.try_get::<Option<String>,_>("display_alias").unwrap_or(None),
            "n_resolved": n,
            "calibration_score": calibration,
            "confidence": confidence,
            "status": if n == 0 { "cold" } else if confidence < 0.5 { "warming" } else { "warm" },
        })
    }).collect();

    let loop5_cold = loop5.iter().filter(|r| r["status"].as_str() == Some("cold")).count();
    let loop5_warm = loop5.iter().filter(|r| r["status"].as_str() == Some("warm")).count();

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

#[derive(Deserialize)]
pub struct CalibrationQuery {
    /// `version` enables Doc 12 § Capability 4 partitioning. Any other value
    /// (or the default) keeps the legacy single-aggregate response shape.
    #[serde(default)]
    pub partition_by: Option<String>,
    /// Time window in days for observations; defaults to 90.
    #[serde(default)]
    pub window_days: Option<i64>,
    /// Optional workspace filter. When supplied, only observations whose
    /// session belongs to the workspace are counted.
    #[serde(default)]
    pub workspace_id: Option<Uuid>,
}

/// Per-version row in the `version_partition.partitions` array.
fn calibration_partition_json(
    version_number: Option<i32>,
    version_deployed_at: Option<chrono::DateTime<chrono::Utc>>,
    n_observations: i64,
) -> Value {
    json!({
        "version_number": version_number,
        "version_deployed_at": version_deployed_at.map(|t| t.to_rfc3339()),
        "n_observations": n_observations,
        // Per-version Brier is intentionally NULL: the existing
        // `fermi_forecasts.agents_used` doesn't yet carry version stamps,
        // so a per-partition Brier mean would be spurious. The field stays
        // present so consumers can light it up later without restructuring.
        "n_resolved": 0,
        "brier_mean": Value::Null,
    })
}


// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
            llm_provider: Some("anthropic".into()),
            model_ladder: Some(json!([])),
            min_tier: Some("free".into()),
            capability_gates: Some(json!({})),
            model_params: Some(json!({})),
            valence: Some(json!({})),
            output_contract: Some(json!({})),
            version: Some("1.0.0".into()),
        };
        let fields = collect_changed_fields(&updates);
        // 23 fields on AgentUpdate today — if the count drifts here,
        // either a field was added (good — wire it up above) or a
        // maintainer wired one twice (bad — dedupe).
        assert_eq!(
            fields.len(),
            23,
            "AgentUpdate has fields that collect_changed_fields doesn't cover: got {:?}",
            fields
        );
    }
}
