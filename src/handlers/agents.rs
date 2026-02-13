//! Agent handlers — CRUD, listing, import, versions, avatar, catalogue.

use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{credit_charge, get_or_create_wallet, teams, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use agent_bestiary_memory::{Agent, AgentUpdate, EmbeddingGenerator, Episode};

use crate::{
    create_notification, resolve_agent, resolve_agent_card, AppState, GeminiContent,
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
    "claude-3-haiku-20240307".to_string()
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
                    {"id": "claude-3-haiku-20240307", "name": "Haiku", "speed": "fast", "cost_tier": "low", "description": "Fast, efficient"},
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

    let agent_type = card
        .get("agent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("research")
        .to_string();

    let caps = card.get("capabilities");
    let model = caps
        .and_then(|c| c.get("model"))
        .and_then(|v| v.as_str())
        .unwrap_or("claude-3-haiku-20240307")
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

    // Batch-load workspace memberships for all owned agents
    let agent_ids: Vec<uuid::Uuid> = agents.iter().map(|a| a.agent_id).collect();
    let ws_rows = sqlx::query(
        "SELECT wa.agent_id, t.name
         FROM workspace_agents wa
         JOIN teams t ON t.id = wa.workspace_id
         WHERE wa.agent_id = ANY($1)",
    )
    .bind(&agent_ids)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut ws_map: std::collections::HashMap<uuid::Uuid, Vec<String>> =
        std::collections::HashMap::new();
    for r in &ws_rows {
        let aid: uuid::Uuid = r.get("agent_id");
        let name: String = r.get("name");
        ws_map.entry(aid).or_default().push(name);
    }

    let agent_list: Vec<Value> = agents
        .iter()
        .map(|a| {
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
                "workspace_names": ws_map.get(&a.agent_id).cloned().unwrap_or_default(),
                "workspace_count": ws_map.get(&a.agent_id).map(|v| v.len()).unwrap_or(0),
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

    // Snapshot current state before applying updates
    if let Err(e) = state
        .memory_store
        .create_agent_version(db_agent.agent_id, &user_id)
        .await
    {
        eprintln!("Warning: failed to create version snapshot: {}", e);
    }

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

    Ok(Json(json!({ "message": "Agent updated successfully" })))
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

    // Snapshot current state before restoring
    if let Err(e) = state
        .memory_store
        .create_agent_version(db_agent.agent_id, &user_id)
        .await
    {
        eprintln!("Warning: failed to snapshot before restore: {}", e);
    }

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

    Ok(Json(json!({
        "message": format!("Restored to version {}", version_num),
        "version_restored": version_num,
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
