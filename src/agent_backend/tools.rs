/// Built-in tool registry for agent tool-use
///
/// Provides 17 platform tools that agents can invoke via the LLM tool-calling protocol:
///   - search_knowledge: similarity search over agent's episodic memory
///   - query_ontology: get rules/entities/facts from knowledge graph
///   - execute_agent: invoke another agent (single-turn, no recursion)
///   - list_agents: discover available agents
///   - read_workspace_file: read a file from workspace git repo (workspace-only)
///   - list_workspace_agents: list agents in current workspace (workspace-only)
///   - generate_image: text-to-image via Gemini
///   - edit_image: image-to-image editing via Gemini
///   - write_workspace_file: write a file to workspace git repo (workspace-only)
///   - reduct_list_projects: list Reduct.video projects
///   - reduct_get_project: get project details with recordings and reels
///   - reduct_get_transcript: get recording transcript (JSON with timestamps)
///   - reduct_create_reel: create a new reel in a project
///   - reduct_add_block: add a clip or title block to a reel
///   - evaluate_coherence: run TEC evaluation on workspace messages (workspace-only)
///   - coherence_snapshot: get latest coherence evaluation (workspace-only)
///   - get_workspace_messages: read recent workspace conversation (workspace-only)
use crate::agent_backend::agent_card::AgentCard;
use crate::agent_backend::llm_executor::{ClaudeTool, ContentBlock};
use crate::agent_backend::multi_model_executor::{OpenAIFunction, OpenAITool};
use crate::agent_backend::registry::AgentRegistry;
use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::store::MemoryStore;
use agent_bestiary_memory::types::CoherenceEvaluation;
use agent_bestiary_ontology::WorkspaceGitManager;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

/// Context available to tools during execution
pub struct ToolContext {
    pub memory_store: Arc<MemoryStore>,
    pub embedder: Arc<dyn EmbeddingGenerator>,
    pub registry: Arc<AgentRegistry>,
    pub current_agent_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub workspace_slug: Option<String>,
    pub workspace_git: Option<Arc<WorkspaceGitManager>>,
}

/// A built-in tool definition
struct BuiltinToolDef {
    name: &'static str,
    description: &'static str,
    input_schema: serde_json::Value,
    requires_workspace: bool,
}

/// All 6 built-in tools
fn builtin_tools() -> Vec<BuiltinToolDef> {
    vec![
        BuiltinToolDef {
            name: "search_knowledge",
            description: "Search the agent's episodic memory for relevant past experiences using semantic similarity. Returns the most relevant episodes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant knowledge"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "query_ontology",
            description: "Query the agent's knowledge graph to retrieve semantic rules, entities, and facts. Specify which types to include.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "include_rules": {
                        "type": "boolean",
                        "description": "Include semantic rules (default: true)",
                        "default": true
                    },
                    "include_entities": {
                        "type": "boolean",
                        "description": "Include entities (default: true)",
                        "default": true
                    },
                    "include_facts": {
                        "type": "boolean",
                        "description": "Include facts/relationships (default: true)",
                        "default": true
                    }
                },
                "required": []
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "execute_agent",
            description: "Invoke another agent with a query and get its response. The sub-agent runs a single turn without tools to prevent recursion.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "description": "The name/ID of the agent to invoke"
                    },
                    "query": {
                        "type": "string",
                        "description": "The query to send to the agent"
                    }
                },
                "required": ["agent_name", "query"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "list_agents",
            description: "List all available agents in the registry with their names, types, and descriptions.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "read_workspace_file",
            description: "Read a file from the current workspace's git repository.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "The file path relative to workspace root"
                    }
                },
                "required": ["path"]
            }),
            requires_workspace: true,
        },
        BuiltinToolDef {
            name: "list_workspace_agents",
            description: "List all agents that are members of the current workspace.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: true,
        },
        BuiltinToolDef {
            name: "generate_image",
            description: "Generate an image from a text prompt using Gemini. Returns the image as base64-encoded data with its MIME type.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Text description of the image to generate"
                    }
                },
                "required": ["prompt"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "edit_image",
            description: "Edit/transform an image using a text prompt and a reference image URL via Gemini. Useful for style transfer, modifications, and artistic transformations. Returns the edited image as base64-encoded data.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "Text description of the desired edit/transformation"
                    },
                    "image_url": {
                        "type": "string",
                        "description": "URL of the source image to edit"
                    }
                },
                "required": ["prompt", "image_url"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "reduct_list_projects",
            description: "List all projects in the Reduct.video workspace. Returns project IDs, titles, and metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "reduct_get_project",
            description: "Get details of a Reduct.video project including its recordings and reels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    }
                },
                "required": ["project_id"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "reduct_get_transcript",
            description: "Get the transcript of a recording in a Reduct.video project. Returns segments with start/end timestamps and speaker labels.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "recording_id": {
                        "type": "string",
                        "description": "The recording ID within the project"
                    },
                    "format": {
                        "type": "string",
                        "description": "Transcript format: 'json' (with timestamps) or 'txt' (plain text). Default: json",
                        "default": "json"
                    }
                },
                "required": ["project_id", "recording_id"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "reduct_create_reel",
            description: "Create a new reel (highlight compilation) in a Reduct.video project. Returns the new reel ID.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "title": {
                        "type": "string",
                        "description": "Title for the new reel"
                    }
                },
                "required": ["project_id", "title"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "reduct_add_block",
            description: "Add a block to a Reduct.video reel. Use type 'doc-range' for video clips (requires recording_id, start, end times) or type 'title' for title cards (requires text).",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "project_id": {
                        "type": "string",
                        "description": "The Reduct project ID"
                    },
                    "reel_id": {
                        "type": "string",
                        "description": "The reel ID to add the block to"
                    },
                    "block_type": {
                        "type": "string",
                        "description": "Block type: 'doc-range' for video clip, 'title' for title card"
                    },
                    "recording_id": {
                        "type": "string",
                        "description": "Recording ID (required for doc-range blocks)"
                    },
                    "start": {
                        "type": "number",
                        "description": "Start time in seconds (required for doc-range blocks)"
                    },
                    "end": {
                        "type": "number",
                        "description": "End time in seconds (required for doc-range blocks)"
                    },
                    "text": {
                        "type": "string",
                        "description": "Title text (required for title blocks)"
                    }
                },
                "required": ["project_id", "reel_id", "block_type"]
            }),
            requires_workspace: false,
        },
        BuiltinToolDef {
            name: "write_workspace_file",
            description: "Write a file to the current workspace's git repository. For binary files (images), provide base64-encoded content and set is_base64 to true.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to workspace root (e.g. outputs/result.png)"
                    },
                    "content": {
                        "type": "string",
                        "description": "File content as text, or base64-encoded string for binary files"
                    },
                    "is_base64": {
                        "type": "boolean",
                        "description": "If true, content is base64-encoded binary data (default: false)",
                        "default": false
                    },
                    "commit_message": {
                        "type": "string",
                        "description": "Git commit message (default: auto-generated)",
                        "default": ""
                    }
                },
                "required": ["path", "content"]
            }),
            requires_workspace: true,
        },
        // ─── Coherence tools ───
        BuiltinToolDef {
            name: "evaluate_coherence",
            description: "Run a Thagard Explanatory Coherence (TEC) evaluation on recent workspace messages. Classifies utterances, detects coherence/incoherence relations, runs constraint-satisfaction settling, and returns global score, 7 principle scores, and health indicators. Results are stored for history.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message_limit": {
                        "type": "integer",
                        "description": "Number of recent messages to evaluate (default: 50, max: 100)",
                        "default": 50
                    }
                },
                "required": []
            }),
            requires_workspace: true,
        },
        BuiltinToolDef {
            name: "coherence_snapshot",
            description: "Get the latest stored coherence evaluation for the workspace without running a new evaluation. Returns global score, quality label, principle scores, and health indicators from the most recent evaluation.",
            input_schema: json!({
                "type": "object",
                "properties": {},
                "required": []
            }),
            requires_workspace: true,
        },
        BuiltinToolDef {
            name: "get_workspace_messages",
            description: "Read recent messages from the workspace conversation. Returns messages with sender name, content, type, and timestamp.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of messages to return (default: 20, max: 50)",
                        "default": 20
                    }
                },
                "required": []
            }),
            requires_workspace: true,
        },
    ]
}

/// Tool registry — collects available tools and dispatches execution
pub struct ToolRegistry {
    include_workspace: bool,
}

impl ToolRegistry {
    /// Standard registry (4 tools, no workspace tools)
    pub fn standard() -> Self {
        Self {
            include_workspace: false,
        }
    }

    /// Registry with workspace tools (6 tools)
    pub fn with_workspace() -> Self {
        Self {
            include_workspace: true,
        }
    }

    /// Get available tools as Claude API format
    pub(crate) fn to_claude_tools(&self) -> Vec<ClaudeTool> {
        builtin_tools()
            .into_iter()
            .filter(|t| !t.requires_workspace || self.include_workspace)
            .map(|t| ClaudeTool {
                name: t.name.to_string(),
                description: t.description.to_string(),
                input_schema: t.input_schema,
            })
            .collect()
    }

    /// Get available tools as OpenAI API format
    pub(crate) fn to_openai_tools(&self) -> Vec<OpenAITool> {
        builtin_tools()
            .into_iter()
            .filter(|t| !t.requires_workspace || self.include_workspace)
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.to_string(),
                    description: t.description.to_string(),
                    parameters: t.input_schema,
                },
            })
            .collect()
    }

    /// Also include any MCP tools declared on the agent card
    pub(crate) fn to_claude_tools_with_card(&self, card: &AgentCard) -> Vec<ClaudeTool> {
        let mut tools = self.to_claude_tools();
        for mcp in &card.capabilities.mcp_tools {
            // Only include MCP tools that have schemas (otherwise the LLM can't call them)
            if let Some(ref schema) = mcp.input_schema {
                tools.push(ClaudeTool {
                    name: mcp.name.clone(),
                    description: mcp.description.clone(),
                    input_schema: schema.clone(),
                });
            }
        }
        tools
    }

    /// Execute a tool by name
    pub async fn execute(
        &self,
        tool_name: &str,
        input: &serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        match tool_name {
            "search_knowledge" => execute_search_knowledge(input, ctx).await,
            "query_ontology" => execute_query_ontology(input, ctx).await,
            "execute_agent" => execute_execute_agent(input, ctx).await,
            "list_agents" => execute_list_agents(ctx).await,
            "read_workspace_file" => execute_read_workspace_file(input, ctx).await,
            "list_workspace_agents" => execute_list_workspace_agents(ctx).await,
            "generate_image" => execute_generate_image(input).await,
            "edit_image" => execute_edit_image(input).await,
            "write_workspace_file" => execute_write_workspace_file(input, ctx).await,
            "reduct_list_projects" => execute_reduct_list_projects().await,
            "reduct_get_project" => execute_reduct_get_project(input).await,
            "reduct_get_transcript" => execute_reduct_get_transcript(input).await,
            "reduct_create_reel" => execute_reduct_create_reel(input).await,
            "reduct_add_block" => execute_reduct_add_block(input).await,
            "evaluate_coherence" => execute_evaluate_coherence(input, ctx).await,
            "coherence_snapshot" => execute_coherence_snapshot(ctx).await,
            "get_workspace_messages" => execute_get_workspace_messages(input, ctx).await,
            _ => Err(format!("Unknown tool: {}", tool_name)),
        }
    }
}

// ─── Tool implementations ──────────────────────────────────────────

async fn execute_search_knowledge(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for search_knowledge")?;

    // Generate embedding for the query
    let embedding = ctx
        .embedder
        .generate(query)
        .await
        .map_err(|e| format!("Embedding generation failed: {}", e))?;

    // Search similar episodes
    let results = ctx
        .memory_store
        .search_similar_episodes(agent_id, &embedding, limit)
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    // Format results
    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|(episode, distance)| {
            json!({
                "query": episode.query,
                "context": episode.context,
                "timestamp": episode.timestamp_ref.to_rfc3339(),
                "similarity": 1.0 - distance,
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_ontology(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let include_rules = input
        .get("include_rules")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_entities = input
        .get("include_entities")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_facts = input
        .get("include_facts")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for query_ontology")?;

    let mut result = json!({});

    if include_rules {
        let rules = ctx
            .memory_store
            .get_agent_semantic_rules(agent_id)
            .await
            .map_err(|e| format!("Failed to get rules: {}", e))?;
        let rules_json: Vec<serde_json::Value> = rules
            .iter()
            .map(|r| {
                json!({
                    "content": r.rule_content,
                    "description": r.rule_description,
                    "confidence": r.confidence_score,
                    "status": r.verification_status,
                })
            })
            .collect();
        result["rules"] = json!(rules_json);
    }

    if include_entities {
        let entities = ctx
            .memory_store
            .get_agent_entities(agent_id)
            .await
            .map_err(|e| format!("Failed to get entities: {}", e))?;
        let entities_json: Vec<serde_json::Value> = entities
            .iter()
            .map(|e| {
                json!({
                    "name": e.entity_name,
                    "type": e.entity_type,
                    "summary": e.summary,
                })
            })
            .collect();
        result["entities"] = json!(entities_json);
    }

    if include_facts {
        let facts = ctx
            .memory_store
            .get_agent_facts(agent_id)
            .await
            .map_err(|e| format!("Failed to get facts: {}", e))?;
        let facts_json: Vec<serde_json::Value> = facts
            .iter()
            .map(|f| {
                json!({
                    "relation_type": f.relation_type,
                    "confidence": f.confidence,
                    "reasoning": f.reasoning,
                })
            })
            .collect();
        result["facts"] = json!(facts_json);
    }

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_execute_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_name = input
        .get("agent_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_name")?;
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;

    // Get the target agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent not found: {}", e))?;

    // Build a minimal AgentStmt for execution
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let context = crate::agent_backend::executor::ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
    };

    // Execute via the base executor (no tools — prevents recursion)
    let output = ctx
        .registry
        .execute_agent(&stmt, &context)
        .await
        .map_err(|e| format!("Agent execution failed: {}", e))?;

    // Format the output
    let result = json!({
        "agent": output.agent_name,
        "confidence": output.confidence,
        "evidence": output.evidence.iter().map(|e| {
            json!({
                "summary": e.summary,
                "key_findings": e.key_findings,
                "strength": e.strength,
            })
        }).collect::<Vec<_>>(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_list_agents(ctx: &ToolContext) -> Result<String, String> {
    let cards = ctx
        .registry
        .list_cards()
        .map_err(|e| format!("Failed to list agents: {}", e))?;

    let agents: Vec<serde_json::Value> = cards
        .iter()
        .map(|c| {
            json!({
                "id": c.agent_id,
                "type": c.agent_type,
                "description": c.metadata.description,
                "skills": c.capabilities.skills,
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_read_workspace_file(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    // read_file is sync (git2), so run on blocking thread
    let git = Arc::clone(git);
    let slug = slug.to_string();
    let path = path.to_string();
    tokio::task::spawn_blocking(move || git.read_file(&slug, &path))
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to read file: {}", e))
}

async fn execute_list_workspace_agents(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let pool = ctx.memory_store.pool();
    let rows = sqlx::query(
        "SELECT a.agent_name, a.agent_type, a.description
         FROM workspace_agents wa
         JOIN agents a ON wa.agent_id = a.id
         WHERE wa.workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Query failed: {}", e))?;

    use sqlx::Row;
    let agents: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "name": row.get::<String, _>("agent_name"),
                "type": row.get::<String, _>("agent_type"),
                "description": row.get::<Option<String>, _>("description"),
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Gemini image generation tools ─────────────────────────────────

/// Gemini API response types (shared with avatar generation)
#[derive(serde::Deserialize)]
struct GeminiToolResponse {
    candidates: Vec<GeminiToolCandidate>,
}

#[derive(serde::Deserialize)]
struct GeminiToolCandidate {
    content: GeminiToolContent,
}

#[derive(serde::Deserialize)]
struct GeminiToolContent {
    parts: Vec<GeminiToolPart>,
}

#[derive(serde::Deserialize)]
struct GeminiToolPart {
    text: Option<String>,
    #[serde(rename = "inlineData")]
    inline_data: Option<GeminiToolInlineData>,
}

#[derive(serde::Deserialize)]
struct GeminiToolInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

const GEMINI_IMAGE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent";

async fn execute_generate_image(input: &serde_json::Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image generation unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let body = json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"]
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image data from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

async fn execute_edit_image(input: &serde_json::Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image editing unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let image_url = input
        .get("image_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: image_url")?;

    // Fetch the source image and convert to base64
    let client = reqwest::Client::new();
    let img_response = client
        .get(image_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch source image: {}", e))?;

    if !img_response.status().is_success() {
        return Err(format!(
            "Failed to fetch image ({}): {}",
            img_response.status(),
            image_url
        ));
    }

    let content_type = img_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let img_bytes = img_response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    use base64::Engine;
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);

    let body = json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                {
                    "inline_data": {
                        "mime_type": content_type,
                        "data": img_b64
                    }
                }
            ]
        }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });

    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image + text from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

// ─── Workspace file write tool ─────────────────────────────────────

async fn execute_write_workspace_file(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: path")?;

    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: content")?;

    let is_base64 = input
        .get("is_base64")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let commit_message = input
        .get("commit_message")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let slug = ctx
        .workspace_slug
        .as_deref()
        .ok_or("Not in a workspace context")?;
    let git = ctx
        .workspace_git
        .as_ref()
        .ok_or("Workspace git not available")?;

    let message = if commit_message.is_empty() {
        format!("agent: write {}", path)
    } else {
        commit_message.to_string()
    };

    if is_base64 {
        // Decode base64 and write as binary
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| format!("Invalid base64 content: {}", e))?;
        let size = bytes.len();

        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let commit = tokio::task::spawn_blocking(move || {
            git.commit_file_bytes(&slug, &path, &bytes, &message)
        })
        .await
        .map_err(|e| format!("Join error: {}", e))?
        .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
            "size_bytes": size,
        })
        .to_string())
    } else {
        let git = Arc::clone(git);
        let slug = slug.to_string();
        let path = path.to_string();
        let content = content.to_string();
        let commit =
            tokio::task::spawn_blocking(move || git.commit_file(&slug, &path, &content, &message))
                .await
                .map_err(|e| format!("Join error: {}", e))?
                .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({
            "path": input.get("path").and_then(|v| v.as_str()).unwrap_or(""),
            "sha": commit.sha,
            "message": commit.message,
        })
        .to_string())
    }
}

// ─── Reduct.video API tools ────────────────────────────────────────

const REDUCT_BASE_URL: &str = "https://app.reduct.video/api/v3";

fn reduct_api_key() -> Result<String, String> {
    std::env::var("REDUCT_API_KEY")
        .map_err(|_| "REDUCT_API_KEY not set — Reduct.video tools unavailable".to_string())
}

async fn reduct_get(path: &str) -> Result<serde_json::Value, String> {
    let api_key = reduct_api_key()?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("X-Auth-Key", &api_key)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

async fn reduct_post(path: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let api_key = reduct_api_key()?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("X-Auth-Key", &api_key)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

async fn execute_reduct_list_projects() -> Result<String, String> {
    let data = reduct_get("/project").await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_project(input: &serde_json::Value) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let data = reduct_get(&format!("/project/{}", project_id)).await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_transcript(input: &serde_json::Value) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let recording_id = input
        .get("recording_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: recording_id")?;

    let format = input
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let ext = if format == "txt" { "txt" } else { "json" };
    let path = format!(
        "/project/{}/recording/{}/transcript.{}",
        project_id, recording_id, ext
    );

    if ext == "txt" {
        // Plain text transcript — fetch as text, not JSON
        let api_key = reduct_api_key()?;
        let url = format!("{}{}", REDUCT_BASE_URL, path);
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("X-Auth-Key", &api_key)
            .send()
            .await
            .map_err(|e| format!("Reduct API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Reduct API error {}: {}", status, error_text));
        }

        response
            .text()
            .await
            .map_err(|e| format!("Failed to read transcript: {}", e))
    } else {
        let data = reduct_get(&path).await?;
        serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
    }
}

async fn execute_reduct_create_reel(input: &serde_json::Value) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: title")?;

    let data = reduct_post(
        &format!("/project/{}/reel", project_id),
        &json!({ "title": title }),
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_add_block(input: &serde_json::Value) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let reel_id = input
        .get("reel_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: reel_id")?;

    let block_type = input
        .get("block_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: block_type")?;

    let body = match block_type {
        "doc-range" => {
            let recording_id = input
                .get("recording_id")
                .and_then(|v| v.as_str())
                .ok_or("doc-range block requires recording_id")?;
            let start = input
                .get("start")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires start time")?;
            let end = input
                .get("end")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires end time")?;

            json!({
                "type": "doc-range",
                "recording": recording_id,
                "start": start,
                "end": end
            })
        }
        "title" => {
            let text = input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("title block requires text")?;

            json!({
                "type": "title",
                "text": text
            })
        }
        other => {
            return Err(format!(
                "Unknown block type: {}. Use 'doc-range' or 'title'.",
                other
            ))
        }
    };

    let data = reduct_post(
        &format!("/project/{}/reel/{}/block", project_id, reel_id),
        &body,
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Coherence tools ───────────────────────────────────────────────

async fn execute_evaluate_coherence(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let message_limit = input
        .get("message_limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .min(100) as i64;

    // Fetch recent messages
    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, message_limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    if messages.is_empty() {
        return Ok(json!({
            "error": "No messages in workspace to evaluate"
        })
        .to_string());
    }

    // Convert to coherence-core Messages (reverse: DB returns DESC, observer expects chronological)
    let conv_id = ConversationId(workspace_id);
    let coherence_msgs: Vec<CoherenceMessage> = messages
        .iter()
        .rev()
        .map(|m| {
            let pid = ParticipantId(
                uuid::Uuid::parse_str(&m.sender_id).unwrap_or_else(|_| Uuid::new_v4()),
            );
            CoherenceMessage::new(pid, &m.content)
        })
        .collect();

    // Run observation pipeline: classify utterances + detect relations
    let observer = ConversationObserver::new(conv_id);
    let mut system = observer.observe(&coherence_msgs);

    // Run settling engine
    let engine = SettlingEngine::with_defaults();
    let _result = engine.settle(&mut system);

    // Extract snapshot
    let snapshot = system.snapshot();

    let principle_scores = serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));

    let health_indicators = json!({
        "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
        "converged": snapshot.global_coherence.converged,
        "accepted_count": snapshot.global_coherence.accepted_count,
        "rejected_count": snapshot.global_coherence.rejected_count,
        "settling_cycles": snapshot.global_coherence.settling_cycles,
        "utterance_stats": {
            "total": snapshot.utterance_stats.total,
            "evidence_density": snapshot.utterance_stats.evidence_density(),
            "explanation_density": snapshot.utterance_stats.explanation_density(),
        },
    });

    // Store evaluation
    let eval = CoherenceEvaluation {
        eval_id: Uuid::new_v4(),
        workspace_id,
        global_score: snapshot.global_coherence.score,
        quality_label: snapshot.global_coherence.quality_label().to_string(),
        principle_scores: principle_scores.clone(),
        health_indicators: health_indicators.clone(),
        utterance_count: snapshot.utterance_stats.total as i32,
        message_window: Some(json!({
            "message_count": messages.len(),
            "from": messages.last().map(|m| m.created_at),
            "to": messages.first().map(|m| m.created_at),
        })),
        created_at: chrono::Utc::now(),
    };

    let eval_id = ctx
        .memory_store
        .store_coherence_evaluation(&eval)
        .await
        .map_err(|e| format!("Failed to store evaluation: {}", e))?;

    let result = json!({
        "eval_id": eval_id,
        "global_score": eval.global_score,
        "quality_label": eval.quality_label,
        "principle_scores": principle_scores,
        "health_indicators": health_indicators,
        "utterance_count": eval.utterance_count,
        "messages_evaluated": messages.len(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_coherence_snapshot(ctx: &ToolContext) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let eval = ctx
        .memory_store
        .get_latest_coherence(workspace_id)
        .await
        .map_err(|e| format!("Failed to get coherence: {}", e))?;

    match eval {
        Some(e) => {
            let result = json!({
                "eval_id": e.eval_id,
                "global_score": e.global_score,
                "quality_label": e.quality_label,
                "principle_scores": e.principle_scores,
                "health_indicators": e.health_indicators,
                "utterance_count": e.utterance_count,
                "message_window": e.message_window,
                "evaluated_at": e.created_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "message": "No coherence evaluations yet for this workspace. Use evaluate_coherence to run the first evaluation."
        })
        .to_string()),
    }
}

async fn execute_get_workspace_messages(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or("Not in a workspace context")?;

    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .min(50) as i64;

    let messages = ctx
        .memory_store
        .get_workspace_messages(workspace_id, limit, None)
        .await
        .map_err(|e| format!("Failed to get messages: {}", e))?;

    let formatted: Vec<serde_json::Value> = messages
        .iter()
        .rev() // chronological order
        .map(|m| {
            json!({
                "sender": m.sender_name.as_deref().unwrap_or(&m.sender_id),
                "sender_type": m.sender_type,
                "content": m.content,
                "type": m.message_type,
                "timestamp": m.created_at.to_rfc3339(),
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}
