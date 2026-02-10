//! Agent creation wizard handlers.

use axum::{extract::State, http::StatusCode, Json};
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
pub async fn list_ontology_templates_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let seeds_dir = std::path::Path::new("agents/templates/ontology_seeds");
    if !seeds_dir.exists() {
        return Ok(Json(json!({ "templates": [] })));
    }

    let mut templates = Vec::new();
    let entries = std::fs::read_dir(seeds_dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    for entry in entries {
        let entry = entry.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                templates.push(val);
            }
        }
    }

    Ok(Json(json!({ "templates": templates })))
}

#[derive(Debug, Deserialize)]
pub struct GenerateOntologyRequest {
    domain_description: String,
}

pub async fn generate_ontology_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<GenerateOntologyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Rate limit LLM calls
    if let Err(retry) = state
        .rate_limits
        .llm
        .check(&format!("user:{}", principal.user_id()))
    {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("LLM rate limit exceeded. Retry after {} seconds.", retry),
        ));
    }
    // Charge 2 credits
    let wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        2,
        "ontology_generation",
        "Generate seed ontology",
        None,
    )
    .await?;

    // Call Claude to generate a Mermaid ontology
    let prompt = format!(
        r#"Generate a Mermaid ER diagram for the following domain. Use erDiagram syntax with entities and relationships. Keep it focused: 5-8 entities, clear relationship labels.

Domain: {}

Return ONLY the Mermaid diagram starting with "erDiagram", no markdown fences, no explanation."#,
        req.domain_description
    );

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-5-20250929",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mermaid = body["content"][0]["text"]
        .as_str()
        .unwrap_or("erDiagram\n    ENTITY_A ||--o{ ENTITY_B : relates_to\n")
        .to_string();

    Ok(Json(json!({
        "mermaid": mermaid,
        "domain": req.domain_description,
    })))
}

#[derive(Debug, Deserialize)]
pub struct GeneratePromptRequest {
    agent_type: String,
    description: String,
    ontology: Option<String>,
}

pub async fn generate_prompt_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(req): Json<GeneratePromptRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Rate limit LLM calls
    if let Err(retry) = state
        .rate_limits
        .llm
        .check(&format!("user:{}", principal.user_id()))
    {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("LLM rate limit exceeded. Retry after {} seconds.", retry),
        ));
    }
    // Charge 1 credit
    let wallet = get_or_create_wallet(&state.db, "user", &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        1,
        "prompt_generation",
        "Generate system prompt",
        None,
    )
    .await?;

    let ontology_ctx = req.ontology.as_deref().unwrap_or("(none provided)");
    let prompt = format!(
        r#"Generate a system prompt for a Fermi forecasting agent with these characteristics:

Type: {}
Description: {}
Ontology (Mermaid ER): {}

The system prompt should:
1. Define the agent's role and expertise clearly
2. Specify how it should approach research queries
3. Include confidence scoring guidelines (0.0-1.0)
4. List key evidence categories it should look for
5. Be 150-300 words

Return ONLY the system prompt text, no markdown, no explanation."#,
        req.agent_type, req.description, ontology_ctx
    );

    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LLM not configured".to_string(),
        ));
    }

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-sonnet-4-5-20250929",
            "max_tokens": 1024,
            "messages": [{"role": "user", "content": prompt}]
        }))
        .send()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let system_prompt = body["content"][0]["text"]
        .as_str()
        .unwrap_or("You are a specialist forecasting agent.")
        .to_string();

    Ok(Json(json!({
        "system_prompt": system_prompt,
    })))
}

pub async fn creation_guide_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Return structured tips from the prompt engineering guide
    Ok(Json(json!({
        "tips": [
            {
                "step": "identity",
                "title": "Naming",
                "content": "Use lowercase with underscores (e.g., market_research). The name becomes the agent's system identifier."
            },
            {
                "step": "identity",
                "title": "Type Selection",
                "content": "Research agents gather information. Risk agents assess threats. Sentiment agents track opinions. Forecasting agents predict outcomes."
            },
            {
                "step": "ontology",
                "title": "Seed Ontology",
                "content": "A seed ontology gives your agent initial structure. It defines entities and relationships the agent will track. The ontology evolves as the agent learns."
            },
            {
                "step": "capabilities",
                "title": "Temperature",
                "content": "Lower (0.1-0.3) for factual extraction and analysis. Higher (0.5-0.8) for creative or exploratory tasks. Default 0.3 works well for most agents."
            },
            {
                "step": "capabilities",
                "title": "System Prompt",
                "content": "Be specific about the agent's expertise, output format, and confidence scoring. Include domain terminology. The system prompt is the most important configuration."
            },
            {
                "step": "economics",
                "title": "Education Budget",
                "content": "Credits allocated for the ADM learning cycle. Each consolidation cycle costs 3 credits. More cycles = deeper learning. Start with 0 and add later."
            },
            {
                "step": "economics",
                "title": "How Optimization Works",
                "content": "Execute queries -> episodic memory stored -> consolidation extracts patterns -> semantic rules formed -> ontology evolves -> agent improves over time."
            }
        ]
    })))
}

pub async fn popular_tags_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT UNNEST(tags) as tag, COUNT(*) as cnt FROM agents WHERE tags IS NOT NULL GROUP BY tag ORDER BY cnt DESC LIMIT 20"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tags: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "tag": r.get::<String, _>("tag"),
                "count": r.get::<i64, _>("cnt"),
            })
        })
        .collect();

    Ok(Json(json!({ "tags": tags })))
}
