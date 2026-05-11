/// Built-in tool registry for agent tool-use
///
/// Provides 30 platform tools that agents can invoke via the LLM tool-calling protocol:
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
///   - get_shopping_profile: retrieve user's shopping preference profile (workspace-only)
///   - update_shopping_profile: recompute composite shopping embedding (workspace-only)
///   - list_marketplace: browse active marketplace listings (workspace-only)
///   - create_listing: list a shopping profile on the marketplace (workspace-only)
///   - delegate_to_agent: delegate task to workspace agent with full tools (workspace-only)
///   - h3_resolve: H3 hexagonal grid operations (gps_to_h3, neighbors, distance, grid_disk)
///   - geocode: address to GPS coordinates via OpenStreetMap Nominatim
///   - create_beacon: create an AR beacon at an H3 cell (workspace-only)
///   - query_beacons: find AR beacons near a location
///   - save_grid_map: persist a named spatial grid (workspace-only)
///   - gbif_species_search: search GBIF for insect species data
///   - mint_creature: store a minted creature specimen (workspace-only)
///   - generate_specimen_art: generate unique naturalist illustration for a creature via Gemini
///   - scan_nearby_creatures: H3 proximity scan for enemy_sensor agent threat assessment
///   - web_search: search the web via Brave Search API (requires BRAVE_SEARCH_API_KEY)
///   - run_monte_carlo: execute FPL program via the real Monte Carlo engine, returns stats + histogram
///   - run_sensitivity_analysis: Sobol global sensitivity analysis (Saltelli) on an FPL program
use crate::agent_backend::agent_card::AgentCard;
use crate::agent_backend::executor::{AgentExecutor, ExecutionContext};
use crate::agent_backend::llm_executor::ClaudeTool;
use crate::agent_backend::multi_model_executor::{OpenAIFunction, OpenAITool};
use crate::agent_backend::registry::AgentRegistry;
use crate::agent_backend::tool_executor::ToolAwareExecutor;
use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::store::MemoryStore;
use agent_bestiary_memory::types::CoherenceEvaluation;
use agent_bestiary_memory::WorkspaceMessage;
use agent_bestiary_ontology::WorkspaceGitManager;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;
use serde_json::json;
use sqlx::Row;
use std::sync::Arc;
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
    pub db: Option<sqlx::PgPool>,
    pub gas_fees: Option<crate::gas::GasFees>,
    pub user_id: Option<String>,
    pub user_secrets: Option<std::collections::HashMap<String, String>>,
}

/// A built-in tool definition
struct BuiltinToolDef {
    name: &'static str,
    description: &'static str,
    input_schema: serde_json::Value,
    requires_workspace: bool,
    /// True for tools that invoke other agents (execute_agent, delegate_to_agent)
    is_delegation: bool,
}

impl Default for BuiltinToolDef {
    fn default() -> Self {
        Self {
            name: "",
            description: "",
            input_schema: json!({}),
            requires_workspace: false,
            is_delegation: false,
        }
    }
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: true,
        },
        BuiltinToolDef {
            name: "delegate_to_agent",
            description: "Delegate a task to another workspace agent who will execute with full tool access (image generation, file writing, etc). The delegation appears as a visible message in workspace chat. Use this instead of execute_agent when the target agent needs tools to do its work.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_name": {
                        "type": "string",
                        "description": "The name of the workspace agent to delegate to"
                    },
                    "task": {
                        "type": "string",
                        "description": "The task description for the target agent"
                    }
                },
                "required": ["agent_name", "task"]
            }),
            requires_workspace: true,
            is_delegation: true,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
        },
        // ─── Voice tools ───
        BuiltinToolDef {
            name: "speak_text",
            description: "Convert text to natural speech using Cartesia Sonic. Returns audio as base64-encoded PCM data.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to convert to speech (max 5000 characters)"
                    },
                    "voice": {
                        "type": "string",
                        "description": "Voice style: narrator (British), conversational (friendly), or storyteller (calm)",
                        "enum": ["narrator", "conversational", "storyteller"],
                        "default": "narrator"
                    }
                },
                "required": ["text"]
            }),
            requires_workspace: false,
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
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
            is_delegation: false,
        },
        // ─── Marketplace tools ───
        BuiltinToolDef {
            name: "get_shopping_profile",
            description: "Retrieve the current user's shopping preference profile for a given agent. Returns metadata, category tags, brand affinities, price sensitivity, and quality bias. Never exposes raw embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile (e.g. 'electronics', 'fitness'). Default: 'default'",
                        "default": "default"
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "update_shopping_profile",
            description: "Recompute the composite shopping embedding from recent episodes and update profile metadata (brand affinities, price sensitivity, quality bias, category tags). The embedding is computed server-side as a weighted centroid of episode embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile to update. Default: 'default'",
                        "default": "default"
                    },
                    "category_tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Category tags for the profile (e.g. ['electronics', 'espresso', 'kitchen'])"
                    },
                    "price_sensitivity": {
                        "type": "number",
                        "description": "Price sensitivity score 0.0 (price insensitive) to 1.0 (very price sensitive)"
                    },
                    "quality_bias": {
                        "type": "number",
                        "description": "Quality bias score 0.0 (value-focused) to 1.0 (premium-focused)"
                    },
                    "brand_affinities": {
                        "type": "object",
                        "description": "Brand affinity scores, e.g. {\"nike\": 0.85, \"breville\": 0.72}"
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "list_marketplace",
            description: "Browse active marketplace listings where consumers have listed their shopping profiles for advertiser queries. Filter by category. Returns listing metadata and pricing — never raw embeddings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Comma-separated category filter (e.g. 'electronics,kitchen')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum listings to return (default: 20)",
                        "default": 20
                    }
                },
                "required": []
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── AR Spatial Suite tools ───
        BuiltinToolDef {
            name: "h3_resolve",
            description: "Convert GPS coordinates to an H3 hexagonal grid cell ID, or convert an H3 cell ID back to GPS coordinates. Also computes k-ring neighbors and grid distance between cells. The foundation for all AR spatial operations.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["gps_to_h3", "h3_to_gps", "neighbors", "distance", "grid_disk"],
                        "description": "Operation: gps_to_h3 (lat/lng→cell), h3_to_gps (cell→lat/lng), neighbors (6 adjacent cells), distance (grid distance between 2 cells), grid_disk (all cells within k rings)"
                    },
                    "lat": {
                        "type": "number",
                        "description": "Latitude in decimal degrees (for gps_to_h3)"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude in decimal degrees (for gps_to_h3)"
                    },
                    "h3_cell": {
                        "type": "string",
                        "description": "H3 cell ID (for h3_to_gps, neighbors, distance)"
                    },
                    "h3_cell_b": {
                        "type": "string",
                        "description": "Second H3 cell ID (for distance operation)"
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution 0-15 (default: 12, ~9m² hexes). Higher = more precise.",
                        "default": 12
                    },
                    "k": {
                        "type": "integer",
                        "description": "Ring count for grid_disk (default: 1). Total cells = 3k²+3k+1",
                        "default": 1
                    }
                },
                "required": ["operation"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "geocode",
            description: "Convert a street address or place name to GPS coordinates (lat/lng) using OpenStreetMap Nominatim. Free, no API key required. Rate limited to 1 request per second.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "address": {
                        "type": "string",
                        "description": "Street address or place name to geocode (e.g. '221B Baker Street, London' or 'Eiffel Tower')"
                    }
                },
                "required": ["address"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "create_beacon",
            description: "Create an AR beacon — place an AR asset at a physical location. Stores the beacon in the database with H3 cell, orientation, TTL, and interaction triggers. Returns the beacon record with its public asset URL.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude of placement"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude of placement"
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution (default: 12)",
                        "default": 12
                    },
                    "asset_path": {
                        "type": "string",
                        "description": "Path to asset in workspace files (e.g. 'ar_assets/portal.png')"
                    },
                    "asset_type": {
                        "type": "string",
                        "description": "Asset type: image, model, video (default: image)",
                        "default": "image"
                    },
                    "azimuth_deg": {
                        "type": "number",
                        "description": "Compass bearing the asset faces, 0-360 (default: 0 = North)",
                        "default": 0
                    },
                    "elevation_deg": {
                        "type": "number",
                        "description": "Vertical tilt, -90 to 90 (default: 0 = eye level)",
                        "default": 0
                    },
                    "billboard": {
                        "type": "boolean",
                        "description": "If true, asset always faces the viewer (default: true)",
                        "default": true
                    },
                    "scale": {
                        "type": "number",
                        "description": "Scale factor (default: 1.0)",
                        "default": 1.0
                    },
                    "ttl_seconds": {
                        "type": "integer",
                        "description": "Time-to-live in seconds (default: 86400 = 24 hours)",
                        "default": 86400
                    },
                    "decay_style": {
                        "type": "string",
                        "description": "Decay style: fade, dissolve, instant, loop_decay (default: fade)",
                        "default": "fade"
                    },
                    "visibility": {
                        "type": "string",
                        "description": "Visibility: public, private, workspace (default: public)",
                        "default": "public"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags for the beacon"
                    },
                    "interaction": {
                        "type": "object",
                        "description": "Interaction triggers: on_gaze, on_tap, on_proximity, on_dwell"
                    }
                },
                "required": ["lat", "lng", "asset_path"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_beacons",
            description: "Query AR beacons near a location. Returns all active (non-expired) beacons within k rings of the specified H3 cell. Used by renderers to discover nearby AR content.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "lat": {
                        "type": "number",
                        "description": "Latitude to search around"
                    },
                    "lng": {
                        "type": "number",
                        "description": "Longitude to search around"
                    },
                    "h3_cell": {
                        "type": "string",
                        "description": "H3 cell to search around (alternative to lat/lng)"
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "Search radius in H3 rings (default: 3)",
                        "default": 3
                    },
                    "resolution": {
                        "type": "integer",
                        "description": "H3 resolution (default: 12)",
                        "default": 12
                    },
                    "include_expired": {
                        "type": "boolean",
                        "description": "Include expired beacons (default: false)",
                        "default": false
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "save_grid_map",
            description: "Save or update an AR grid map — a named spatial grid with quadrants and zones. Used by ar_cartographer to persist grid definitions to the database.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Human-readable name for the grid map"
                    },
                    "description": {
                        "type": "string",
                        "description": "Description of the space"
                    },
                    "center_lat": {
                        "type": "number",
                        "description": "Center latitude"
                    },
                    "center_lng": {
                        "type": "number",
                        "description": "Center longitude"
                    },
                    "grid_resolution": {
                        "type": "integer",
                        "description": "H3 resolution for placement grid (default: 12)",
                        "default": 12
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "Grid radius in rings (default: 5)",
                        "default": 5
                    },
                    "quadrants": {
                        "type": "array",
                        "description": "Named quadrant definitions [{h3_cell, name, description, tags, color}]"
                    },
                    "zones": {
                        "type": "array",
                        "description": "Zone groupings [{name, description, quadrants: [names], color}]"
                    }
                },
                "required": ["name", "center_lat", "center_lng"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Rabble.world creature tools ───
        BuiltinToolDef {
            name: "gbif_species_search",
            description: "Search the GBIF (Global Biodiversity Information Facility) API for insect species. Returns taxonomy, common names, and media references. Free, no API key required.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Species name (common or scientific) to search for"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "Specific GBIF species key for direct lookup"
                    },
                    "rank": {
                        "type": "string",
                        "description": "Taxonomic rank filter: SPECIES, GENUS, FAMILY (default: SPECIES)",
                        "default": "SPECIES"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Max results (default: 5)",
                        "default": 5
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "mint_creature",
            description: "Store a minted creature in the database. Creates the creature record with species data, asset path, variation notes, and generates a specimen name. Returns the creature ID and data card.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name of the species"
                    },
                    "common_name": {
                        "type": "string",
                        "description": "Common name (e.g. 'Red Admiral')"
                    },
                    "species_group": {
                        "type": "string",
                        "description": "Group: butterfly, dragonfly (default: butterfly)",
                        "default": "butterfly"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species key for reference"
                    },
                    "taxonomy": {
                        "type": "object",
                        "description": "Full taxonomy object (kingdom through species)"
                    },
                    "asset_path": {
                        "type": "string",
                        "description": "Path to the specimen image in workspace files"
                    },
                    "flight_silhouette_path": {
                        "type": "string",
                        "description": "Path to the flight-pose image (optional)"
                    },
                    "specimen_name": {
                        "type": "string",
                        "description": "Unique name for this specimen (e.g. 'Twilight Admiral')"
                    },
                    "variation_notes": {
                        "type": "string",
                        "description": "Description of what makes this specimen unique"
                    }
                },
                "required": ["scientific_name", "asset_path"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "generate_specimen_art",
            description: "Generate a unique naturalist illustration for a creature using Gemini image generation. Fetches GBIF reference media for the species, then generates a stylized scientific illustration. Saves the image to static/creatures/ and updates the creature record.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the creature to generate art for"
                    },
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name (used for GBIF lookup and prompt). Required if creature_id not provided."
                    },
                    "common_name": {
                        "type": "string",
                        "description": "Common name for prompt enrichment"
                    },
                    "species_group": {
                        "type": "string",
                        "description": "butterfly or dragonfly — affects illustration style"
                    },
                    "style": {
                        "type": "string",
                        "description": "Art style hint: 'naturalist' (default), 'watercolor', 'botanical', 'field-guide', 'ukiyo-e'",
                        "default": "naturalist"
                    },
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species key for reference media lookup"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "segment_creature_wings",
            description: "Segment a butterfly creature's minted image into animation layers (body, left wing, right wing) using Gemini image editing. Stores layers in the database for client-side parametric wing animation. Only works for butterfly species. Costs creature_animate credits.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the butterfly creature to segment into animation layers"
                    }
                },
                "required": ["creature_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "activate_formation",
            description: "Activate a premium swarm formation algorithm for a rabble. Charges credits based on the algorithm's cost. Returns the formation spec JSON for client-side execution in the SwarmEngine. Idempotent: re-activating the same algorithm in the same session returns the spec without double-charging.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "algorithm_name": {
                        "type": "string",
                        "description": "Algorithm name (e.g. 'v_formation', 'echelon', 'encircle', 'patrol', 'search')"
                    },
                    "swarm_id": {
                        "type": "string",
                        "description": "Rabble/swarm session UUID"
                    }
                },
                "required": ["algorithm_name", "swarm_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "create_listing",
            description: "List a shopping profile on the embedding marketplace so advertisers can run similarity queries against it. The consumer sets the price per query and can delist at any time. Costs a one-time listing fee.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "profile_name": {
                        "type": "string",
                        "description": "Name of the shopping profile to list. Default: 'default'",
                        "default": "default"
                    },
                    "price_credits": {
                        "type": "integer",
                        "description": "Credits to charge per advertiser query (min 1)"
                    },
                    "max_queries_per_buyer": {
                        "type": "integer",
                        "description": "Optional cap on queries per buyer (privacy control)"
                    },
                    "category_tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Category tags for marketplace discovery"
                    },
                    "description": {
                        "type": "string",
                        "description": "Public description of this listing"
                    }
                },
                "required": ["price_credits"]
            }),
            requires_workspace: true,
            is_delegation: false,
        },
        // ─── Enemy Sensor ───
        BuiltinToolDef {
            name: "scan_nearby_creatures",
            description: "Find creatures near a given creature using H3 hexagonal proximity. Returns the target creature's species info and all nearby creatures with taxonomy data. Used by the enemy_sensor agent to assess predation risk.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "creature_id": {
                        "type": "string",
                        "description": "UUID of the creature to scan around"
                    },
                    "radius_rings": {
                        "type": "integer",
                        "description": "H3 grid ring radius (default: 1, i.e. 7 cells at res 12)",
                        "default": 1
                    }
                },
                "required": ["creature_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Genome Profiler ───
        BuiltinToolDef {
            name: "gbif_taxonomy_tree",
            description: "Fetch the full taxonomic hierarchy for a species from GBIF. Returns kingdom through species with keys, plus sibling taxa at each rank for phylogenetic context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "gbif_key": {
                        "type": "integer",
                        "description": "GBIF species/taxon key"
                    },
                    "scientific_name": {
                        "type": "string",
                        "description": "Scientific name to look up (used if gbif_key not provided)"
                    }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Web Search ───
        BuiltinToolDef {
            name: "web_search",
            description: "Search the web for current information using Brave Search. Returns recent news, articles, and web pages with titles, URLs, descriptions, and publication dates. Use this to get up-to-date evidence that goes beyond your training data cutoff.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query. Be specific: include names, dates, ticker symbols, or event terms. E.g. 'RKLB Q4 2025 earnings revenue' or 'Fed interest rate decision March 2026'."
                    },
                    "count": {
                        "type": "integer",
                        "description": "Number of results to return (default: 5, max: 10)",
                        "default": 5
                    },
                    "freshness": {
                        "type": "string",
                        "description": "Filter by recency: 'pd' = past day, 'pw' = past week, 'pm' = past month, 'py' = past year. Omit for all-time results.",
                        "enum": ["pd", "pw", "pm", "py"]
                    }
                },
                "required": ["query"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Football (soccer) API ───
        BuiltinToolDef {
            name: "call_football_api",
            description: "Call the API-Football v3 REST API (api-football.com) to get live football/soccer data. Returns current standings, fixtures, results, team stats, player stats, injuries, lineups, head-to-head records, and match predictions. Requires FOOTBALL_API_KEY environment variable.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "endpoint": {
                        "type": "string",
                        "description": "API endpoint path (without leading slash). Examples: 'standings', 'fixtures', 'teams/statistics', 'players/topscorers', 'injuries', 'predictions', 'fixtures/headtohead', 'fixtures/statistics', 'fixtures/events', 'fixtures/lineups', 'players', 'leagues'"
                    },
                    "params": {
                        "type": "object",
                        "description": "Query parameters as key-value pairs. Common params: league (league ID), season (e.g. 2025), team (team ID), fixture (fixture ID), date (YYYY-MM-DD), from/to (date range), last (last N fixtures), next (next N fixtures), player (player ID). Example for PL standings: {\"league\": 39, \"season\": 2025}"
                    }
                },
                "required": ["endpoint"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Monte Carlo / FPL Simulation ───
        BuiltinToolDef {
            name: "run_monte_carlo",
            description: "Execute a Monte Carlo simulation from an FPL (Fermi Probabilistic Language) program. Parses the program, samples from each driver's distribution, and returns full statistics: mean, median, percentiles (p5/p25/p75/p95), std_dev, min/max, and a histogram. Use this to produce rigorous probabilistic results rather than reasoning about distributions informally.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {
                        "type": "string",
                        "description": "FPL source code defining drivers (with distributions), a model expression, and a simulate statement. Example:\n  driver x continuous { distribution: triangular(0.3, 0.6, 0.9) }\n  model: x\n  simulate 10000 iterations"
                    },
                    "iterations": {
                        "type": "integer",
                        "description": "Number of Monte Carlo iterations (default: 10000). Overrides the simulate statement in the FPL if provided.",
                        "default": 10000
                    }
                },
                "required": ["fpl_program"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "run_sensitivity_analysis",
            description: "Run Sobol global sensitivity analysis on an FPL program. Returns first-order and total-order Sobol indices for each driver, ranked by total-order impact, plus bootstrap standard errors for uncertainty quantification. Use this to identify which input variables drive the most outcome variance — a proper variance decomposition, not a heuristic tornado diagram.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fpl_program": {
                        "type": "string",
                        "description": "FPL source code with driver definitions and model expression."
                    },
                    "iterations": {
                        "type": "integer",
                        "description": "Baseline iterations for the analysis (default: 10000). More iterations improve index precision.",
                        "default": 10000
                    }
                },
                "required": ["fpl_program"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── SimOps — Universal Resource Efficiency Engine (SOSA-aligned) ───
        BuiltinToolDef {
            name: "simops_cascade_forward",
            description: "Run a forward cascade through a multi-stage transformation process. Propagates input_quantity through all stages computing output quantities, energy, carbon delta (kg CO₂-eq), stage NER, and OPEX at each step. Returns a CascadeResult with system-level NER, total carbon, and LCC.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name": {
                        "type": "string",
                        "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'. Omit to use ambu_bioreactor as default."
                    },
                    "process_json": {
                        "type": "object",
                        "description": "Inline process config JSON (overrides process_name). Full ProcessConfig schema."
                    },
                    "input_quantity": {
                        "type": "number",
                        "description": "Input quantity at stage 0 (in the units of the first stage's input resource)."
                    }
                },
                "required": ["input_quantity"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_cascade_backward",
            description: "Run a backward cascade to determine the primary input required to produce a specified output. Given target_output at the final stage, back-calculates all intermediate quantities and the required stage-0 input.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "process_name": {
                        "type": "string",
                        "description": "Named process config: 'ambu_bioreactor' or 'scoby_kombucha'."
                    },
                    "process_json": {
                        "type": "object",
                        "description": "Inline process config JSON (overrides process_name)."
                    },
                    "target_output": {
                        "type": "number",
                        "description": "Desired output quantity at the final stage (in the final stage's output units)."
                    }
                },
                "required": ["target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_kpi_compute",
            description: "Compute batch KPIs for a fermentation or cultivation run: NER (Net Energy Ratio), SEC (Specific Energy Consumption kWh/kg), LCC (Levelized Cost of Calories $/million kcal), and Harvest Intensity %. Takes measured energy inputs and batch output.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "primary_energy_kwh":     { "type": "number", "description": "Primary process energy input (e.g. LED lighting) in kWh." },
                    "climate_energy_kwh":     { "type": "number", "description": "Climate control energy (heating/cooling/Peltier) in kWh." },
                    "delivery_energy_kwh":    { "type": "number", "description": "Pumping and delivery energy in kWh." },
                    "harvest_energy_kwh":     { "type": "number", "description": "Harvest and post-processing energy in kWh." },
                    "output_mass_kg":         { "type": "number", "description": "Harvested output mass in kg (dry weight for biomass)." },
                    "caloric_density_kcal_g": { "type": "number", "description": "Caloric density of the output in kcal/g." },
                    "elec_price_per_kwh":     { "type": "number", "description": "Electricity price in USD/kWh (e.g. 0.22 for German industrial)." },
                    "consumables_cost_usd":   { "type": "number", "description": "Total consumables cost for the batch in USD (nutrients, substrate, CO₂, etc.)." },
                    "capex_contribution_usd": { "type": "number", "description": "Amortized CAPEX contribution for this batch in USD (optional, default 0)." }
                },
                "required": [
                    "primary_energy_kwh", "climate_energy_kwh", "delivery_energy_kwh",
                    "harvest_energy_kwh", "output_mass_kg", "caloric_density_kcal_g",
                    "elec_price_per_kwh", "consumables_cost_usd"
                ]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_predictor_train",
            description: "Fit an OLS linear regression model from historical observations. Takes an array of {features: {k: v, ...}, target: f64} records and returns model coefficients, intercept, R², and feature importance. Model JSON can be passed to simops_predictor_forecast or simops_optimize_* tools.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "observations": {
                        "type": "array",
                        "description": "Array of training observations. Each item must have 'features' (object of string→number) and 'target' (number).",
                        "items": {
                            "type": "object",
                            "properties": {
                                "features": { "type": "object", "additionalProperties": { "type": "number" } },
                                "target":   { "type": "number" }
                            },
                            "required": ["features", "target"]
                        },
                        "minItems": 4
                    }
                },
                "required": ["observations"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_predictor_forecast",
            description: "Predict yield or output for a planned operational batch using a trained OLS model. Takes a model_json (from simops_predictor_train) and a feature map. Returns predicted value, R², and caloric-positive/energy-sink status.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json": {
                        "type": "object",
                        "description": "Trained predictor model returned by simops_predictor_train."
                    },
                    "features": {
                        "type": "object",
                        "description": "Feature map for the planned batch (same keys as training features, e.g. {lighting_kwh: 120, nutrients_g: 6.5, temp_c: 27}).",
                        "additionalProperties": { "type": "number" }
                    }
                },
                "required": ["model_json", "features"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_optimize_scale",
            description: "Proportionally scale a reference operating point to hit a target output. All inputs in the reference are scaled by the same factor. Returns scaled input values, predicted output, convergence status, and residual. Use for holistic scale-up planning.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json":     { "type": "object", "description": "Trained predictor model from simops_predictor_train." },
                    "reference":      { "type": "object", "description": "Reference operating point: feature map of current/baseline input values.", "additionalProperties": { "type": "number" } },
                    "target_output":  { "type": "number", "description": "Target output value to achieve." },
                    "max_scale":      { "type": "number", "description": "Maximum scaling factor allowed (default: 5.0)." }
                },
                "required": ["model_json", "reference", "target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "simops_optimize_single_input",
            description: "Solve analytically for a single free input variable to hit a target output, holding all other inputs fixed. Use for questions like 'how much more LED power do I need to produce 5 kg biomass?'. Returns the required value and convergence report.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "model_json":    { "type": "object", "description": "Trained predictor model from simops_predictor_train." },
                    "fixed_inputs":  { "type": "object", "description": "Fixed input feature values (all features except the free one).", "additionalProperties": { "type": "number" } },
                    "free_feature":  { "type": "string", "description": "Name of the single input feature to solve for." },
                    "target_output": { "type": "number", "description": "Target output value to achieve." },
                    "min_value":     { "type": "number", "description": "Minimum allowed value for the free feature (default: 0)." },
                    "max_value":     { "type": "number", "description": "Maximum allowed value for the free feature (default: 1,000,000)." }
                },
                "required": ["model_json", "fixed_inputs", "free_feature", "target_output"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        // ─── Observability composition tools ───────────────────────
        // Consumed by observability_coordinator, eval_runner,
        // anomaly_triager, dyad_observer. See docs/AGENT_MODEL.md §3.
        BuiltinToolDef {
            name: "query_eval_signals",
            description: "Read per-evaluator, per-dimension scores from eval_signals. Required: run_id. Returns one row per (evaluator, dimension) with score, confidence, persona_version, model_used, rationale.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "run_id": { "type": "string", "description": "UUID of the eval_run to read signals for." }
                },
                "required": ["run_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_eval_runs",
            description: "List recent eval_runs for an agent. Returns run metadata including aggregated_signal, regression_detected, judge_enabled, pass/fail counts.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 20, "description": "Max runs to return (default 20, max 100)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_anomalies",
            description: "Read anomaly_events rows for an agent (drift / conflict / rupture / safety). Used by anomaly_triager and observability_coordinator.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 50, "description": "Max events to return (default 50, max 500)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_hitl_queue",
            description: "Read pending HITL events — anomaly_events where requires_review=true and resolved_at is null. Returns up to N events ordered by severity then recency.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 50, "description": "Max events to return (default 50, max 200)." }
                },
                "required": []
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_timeline",
            description: "Read agent_timeline_entries — per-episode rolled-up scoring view with persona_version_at_write and aggregated scores. Used by dyad_observer for longitudinal narrative.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." },
                    "limit": { "type": "integer", "default": 100, "description": "Max entries to return (default 100, max 500)." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
        BuiltinToolDef {
            name: "query_dyad_state",
            description: "Read dyad_state rows — per-(agent, human) running rapport / trust / reciprocity. Used by dyad_observer.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "UUID of the agent." }
                },
                "required": ["agent_id"]
            }),
            requires_workspace: false,
            is_delegation: false,
        },
    ]
}

/// Tool registry — collects available tools and dispatches execution
pub struct ToolRegistry {
    include_workspace: bool,
    exclude_delegation: bool,
}

impl ToolRegistry {
    /// Standard registry (4 tools, no workspace tools)
    pub fn standard() -> Self {
        Self {
            include_workspace: false,
            exclude_delegation: false,
        }
    }

    /// Registry with workspace tools
    pub fn with_workspace() -> Self {
        Self {
            include_workspace: true,
            exclude_delegation: false,
        }
    }

    /// Registry with workspace tools but NO delegation tools (for delegated agents)
    pub fn with_workspace_no_delegation() -> Self {
        Self {
            include_workspace: true,
            exclude_delegation: true,
        }
    }

    fn filter_tool(&self, t: &BuiltinToolDef) -> bool {
        if t.requires_workspace && !self.include_workspace {
            return false;
        }
        if t.is_delegation && self.exclude_delegation {
            return false;
        }
        true
    }

    /// Get available tools as Claude API format
    pub(crate) fn to_claude_tools(&self) -> Vec<ClaudeTool> {
        builtin_tools()
            .into_iter()
            .filter(|t| self.filter_tool(t))
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
            .filter(|t| self.filter_tool(t))
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
        // Collect builtin names first — Anthropic API rejects duplicate tool names with 400.
        let builtin_names: std::collections::HashSet<String> =
            tools.iter().map(|t| t.name.clone()).collect();
        for mcp in &card.capabilities.mcp_tools {
            // Only include MCP tools that have schemas and aren't already registered as builtins
            if let Some(ref schema) = mcp.input_schema {
                if !builtin_names.contains(&mcp.name) {
                    tools.push(ClaudeTool {
                        name: mcp.name.clone(),
                        description: mcp.description.clone(),
                        input_schema: schema.clone(),
                    });
                }
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
            "speak_text" => execute_speak_text(input).await,
            "reduct_list_projects" => execute_reduct_list_projects().await,
            "reduct_get_project" => execute_reduct_get_project(input).await,
            "reduct_get_transcript" => execute_reduct_get_transcript(input).await,
            "reduct_create_reel" => execute_reduct_create_reel(input).await,
            "reduct_add_block" => execute_reduct_add_block(input).await,
            "delegate_to_agent" => execute_delegate_to_agent(input, ctx).await,
            "evaluate_coherence" => execute_evaluate_coherence(input, ctx).await,
            "coherence_snapshot" => execute_coherence_snapshot(ctx).await,
            "get_workspace_messages" => execute_get_workspace_messages(input, ctx).await,
            "get_shopping_profile" => execute_get_shopping_profile(input, ctx).await,
            "update_shopping_profile" => execute_update_shopping_profile(input, ctx).await,
            "list_marketplace" => execute_list_marketplace(input, ctx).await,
            "create_listing" => execute_create_listing(input, ctx).await,
            // AR Spatial Suite
            "h3_resolve" => execute_h3_resolve(input).await,
            "geocode" => execute_geocode(input).await,
            "create_beacon" => execute_create_beacon(input, ctx).await,
            "query_beacons" => execute_query_beacons(input, ctx).await,
            "save_grid_map" => execute_save_grid_map(input, ctx).await,
            "gbif_species_search" => execute_gbif_species_search(input).await,
            "mint_creature" => execute_mint_creature(input, ctx).await,
            "generate_specimen_art" => execute_generate_specimen_art(input, ctx).await,
            "segment_creature_wings" => execute_segment_creature_wings(input, ctx).await,
            "activate_formation" => execute_activate_formation(input, ctx).await,
            "scan_nearby_creatures" => execute_scan_nearby_creatures(input, ctx).await,
            "gbif_taxonomy_tree" => execute_gbif_taxonomy_tree(input).await,
            // FMP (Financial Modeling Prep) tools for equity_analyst
            "fmp_company_profile" => execute_fmp_api(input, "/stable/profile", &["symbol"]).await,
            "fmp_income_statement" => {
                execute_fmp_api(
                    input,
                    "/stable/income-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_balance_sheet" => {
                execute_fmp_api(
                    input,
                    "/stable/balance-sheet-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_cash_flow" => {
                execute_fmp_api(
                    input,
                    "/stable/cash-flow-statement",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_ratios" => {
                execute_fmp_api(input, "/stable/ratios", &["symbol", "period", "limit"]).await
            }
            "fmp_key_metrics" => {
                execute_fmp_api(input, "/stable/key-metrics", &["symbol", "period", "limit"]).await
            }
            "fmp_dcf" => execute_fmp_api(input, "/stable/discounted-cash-flow", &["symbol"]).await,
            "fmp_analyst_estimates" => {
                execute_fmp_api(
                    input,
                    "/stable/analyst-estimates",
                    &["symbol", "period", "limit"],
                )
                .await
            }
            "fmp_historical_price" => {
                execute_fmp_api(
                    input,
                    "/stable/historical-price-eod/full",
                    &["symbol", "from", "to"],
                )
                .await
            }
            // Web Search
            "web_search" => execute_web_search(input).await,
            // Monte Carlo / FPL Simulation tools
            "run_monte_carlo" => execute_run_monte_carlo(input).await,
            "run_sensitivity_analysis" => execute_run_sensitivity_analysis(input).await,
            // Football (soccer) live data via API-Football v3
            "call_football_api" => execute_call_football_api(input).await,
            // Polymarket tools for prediction_market agent and general orchestra use
            "polymarket_search" => execute_polymarket_search(input).await,
            "polymarket_event" => execute_polymarket_event(input).await,
            // SimOps — Universal Resource Efficiency Engine (SOSA-aligned)
            "simops_cascade_forward"       => crate::agent_backend::simops_tools::execute_simops_cascade_forward(input).await,
            "simops_cascade_backward"      => crate::agent_backend::simops_tools::execute_simops_cascade_backward(input).await,
            "simops_kpi_compute"           => crate::agent_backend::simops_tools::execute_simops_kpi_compute(input).await,
            "simops_predictor_train"       => crate::agent_backend::simops_tools::execute_simops_predictor_train(input).await,
            "simops_predictor_forecast"    => crate::agent_backend::simops_tools::execute_simops_predictor_forecast(input).await,
            "simops_optimize_scale"        => crate::agent_backend::simops_tools::execute_simops_optimize_scale(input).await,
            "simops_optimize_single_input" => crate::agent_backend::simops_tools::execute_simops_optimize_single_input(input).await,
            // ─── Observability composition tools ───────────────
            "query_eval_signals" => execute_query_eval_signals(input, ctx).await,
            "query_eval_runs"    => execute_query_eval_runs(input, ctx).await,
            "query_anomalies"    => execute_query_anomalies(input, ctx).await,
            "query_hitl_queue"   => execute_query_hitl_queue(input, ctx).await,
            "query_timeline"     => execute_query_timeline(input, ctx).await,
            "query_dyad_state"   => execute_query_dyad_state(input, ctx).await,
            _ => Err(format!("Unknown tool: {}", tool_name)),
        }
    }
}

// ─── Tool implementations ──────────────────────────────────────────

/// Search Polymarket for events matching a query.
/// Used by orchestra agents (especially prediction_market) during research.
async fn execute_polymarket_search(input: &serde_json::Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let gamma = crate::polymarket::GammaClient::new();
    let events = gamma
        .search_events(query, limit)
        .await
        .map_err(|e| format!("Polymarket search failed: {}", e))?;

    if events.is_empty() {
        return Ok("No matching Polymarket markets found for this query.".to_string());
    }

    let mut output = String::new();
    for event in &events {
        output.push_str(&format!("## {}\n", event.title));
        output.push_str(&format!(
            "Event ID: {} | Volume 24h: ${:.0} | Liquidity: ${:.0}\n",
            event.id, event.volume_24hr, event.liquidity
        ));
        if let Some(ref end) = event.end_date {
            output.push_str(&format!("End date: {}\n", end));
        }
        for market in &event.markets {
            let processed = crate::polymarket::process_market_public(event, market);
            output.push_str(&format!(
                "  → {} | YES: {:.1}% | bid/ask: {:.3}/{:.3} | vol24h: ${:.0} | confidence: {}\n",
                processed.question,
                processed.market_price * 100.0,
                processed.bid_price,
                processed.ask_price,
                processed.volume_24h,
                processed.confidence_signal.label(),
            ));
            if let Some(ref change) = processed.price_change_1w {
                output.push_str(&format!("    1-week change: {:+.1}pp\n", change * 100.0));
            }
        }
        output.push('\n');
    }

    // Truncate if very large
    if output.len() > 24_000 {
        output.truncate(24_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

/// Get details for a specific Polymarket event by ID.
async fn execute_polymarket_event(input: &serde_json::Value) -> Result<String, String> {
    let event_id = input
        .get("event_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: event_id")?;

    let gamma = crate::polymarket::GammaClient::new();
    let event = gamma
        .get_event(event_id)
        .await
        .map_err(|e| format!("Polymarket event fetch failed: {}", e))?;

    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", event.title));
    output.push_str(&format!(
        "Description: {}\n\n",
        &event.description[..event.description.len().min(500)]
    ));
    output.push_str(&format!(
        "Total volume: ${:.0} | 24h volume: ${:.0} | Liquidity: ${:.0}\n",
        event.volume, event.volume_24hr, event.liquidity
    ));
    if let Some(ref end) = event.end_date {
        output.push_str(&format!("End date: {}\n", end));
    }
    output.push_str(&format!(
        "Active: {} | Closed: {}\n\n",
        event.active, event.closed
    ));

    output.push_str("## Markets\n\n");
    for market in &event.markets {
        let processed = crate::polymarket::process_market_public(&event, market);
        output.push_str(&format!("### {}\n", processed.question));
        output.push_str(&format!("  Market ID: {}\n", processed.pm_market_id));
        output.push_str(&format!(
            "  YES price: {:.1}% (midpoint: {:.1}%)\n",
            processed.market_price * 100.0,
            processed.midpoint_price * 100.0
        ));
        output.push_str(&format!(
            "  Bid/Ask: {:.3} / {:.3} (spread: {:.3})\n",
            processed.bid_price, processed.ask_price, processed.spread
        ));
        output.push_str(&format!(
            "  Volume 24h: ${:.0} | Total: ${:.0}\n",
            processed.volume_24h, processed.volume_total
        ));
        output.push_str(&format!("  Liquidity: ${:.0}\n", processed.liquidity));
        output.push_str(&format!(
            "  Confidence: {} ({:.0}% quality)\n",
            processed.confidence_signal.label(),
            processed.confidence_signal.quality_score() * 100.0
        ));
        if let Some(change) = processed.price_change_1w {
            output.push_str(&format!(
                "  1-week price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        if let Some(change) = processed.price_change_1m {
            output.push_str(&format!(
                "  1-month price change: {:+.1}pp\n",
                change * 100.0
            ));
        }
        output.push_str(&format!(
            "  Status: {}\n",
            if processed.resolved {
                "RESOLVED"
            } else if processed.closed {
                "CLOSED"
            } else if processed.active {
                "ACTIVE"
            } else {
                "INACTIVE"
            }
        ));
        if let Some(ref group) = processed.group_item_title {
            output.push_str(&format!("  Group: {}\n", group));
        }
        output.push('\n');
    }

    output.push_str(&format!(
        "Tags: {}\n",
        event
            .tags
            .iter()
            .map(|t| t.label.clone())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    output.push_str(&format!(
        "URL: https://polymarket.com/event/{}\n",
        event.slug
    ));

    Ok(output)
}

/// Generic FMP API executor — builds a GET request from the input parameters
/// and the endpoint path. Appends the FMP API key from env or hardcoded fallback.
async fn execute_fmp_api(
    input: &serde_json::Value,
    endpoint: &str,
    param_names: &[&str],
) -> Result<String, String> {
    let api_key = std::env::var("FMP_API_KEY")
        .unwrap_or_else(|_| "xadhcaZJ9suK6jthYq2axsDINSE31Nxj".to_string());

    let base_url = "https://financialmodelingprep.com";
    let mut url = format!("{}{}", base_url, endpoint);

    // Build query string from known parameter names
    let mut params: Vec<(String, String)> = Vec::new();
    for &name in param_names {
        if let Some(val) = input.get(name) {
            let s = match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                other => other.to_string().trim_matches('"').to_string(),
            };
            if !s.is_empty() {
                params.push((name.to_string(), s));
            }
        }
    }
    params.push(("apikey".to_string(), api_key));

    let query_string: String = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");

    url = format!("{}?{}", url, query_string);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "FermiConsole/1.0")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("FMP API request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read FMP response: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "FMP API error (HTTP {}): {}",
            status.as_u16(),
            body
        ));
    }

    // If response is empty array, return a clear message
    if body.trim() == "[]" {
        return Ok("No data found for the given parameters.".to_string());
    }

    // Compact the JSON if it's very large (>8k chars) — keep structure but trim
    if body.len() > 8000 {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            // For arrays, limit to first 3 entries to save token budget
            if let Some(arr) = parsed.as_array() {
                let limited: Vec<&serde_json::Value> = arr.iter().take(3).collect();
                let note = if arr.len() > 3 {
                    format!("\n[Showing 3 of {} results]", arr.len())
                } else {
                    String::new()
                };
                return Ok(format!(
                    "{}{}",
                    serde_json::to_string_pretty(&limited).unwrap_or(body),
                    note
                ));
            }
        }
    }

    Ok(body)
}

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

// ─── GBIF Taxonomy Tree ────────────────────────────────────────────

async fn execute_gbif_taxonomy_tree(input: &serde_json::Value) -> Result<String, String> {
    let client = reqwest::Client::new();
    let ua = "AgentBestiaryWorld/1.0 (rabble.world)";

    // Resolve GBIF key — either directly provided or via name search
    let gbif_key: i64 = if let Some(key) = input.get("gbif_key").and_then(|v| v.as_i64()) {
        key
    } else if let Some(name) = input.get("scientific_name").and_then(|v| v.as_str()) {
        let resp = client
            .get("https://api.gbif.org/v1/species/match")
            .query(&[("name", name), ("kingdom", "Animalia")])
            .header("User-Agent", ua)
            .send()
            .await
            .map_err(|e| format!("GBIF match failed: {}", e))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;
        body.get("usageKey")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| format!("No GBIF match for '{}'", name))?
    } else {
        return Err("Either 'gbif_key' or 'scientific_name' is required".to_string());
    };

    // Fetch the species record (includes full taxonomy)
    let species_url = format!("https://api.gbif.org/v1/species/{}", gbif_key);
    let species_resp = client
        .get(&species_url)
        .header("User-Agent", ua)
        .send()
        .await
        .map_err(|e| format!("GBIF species fetch failed: {}", e))?;
    let species: serde_json::Value = species_resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Fetch parent chain (full classification)
    let parents_url = format!("https://api.gbif.org/v1/species/{}/parents", gbif_key);
    let parents_resp = client
        .get(&parents_url)
        .header("User-Agent", ua)
        .send()
        .await
        .map_err(|e| format!("GBIF parents fetch failed: {}", e))?;
    let parents: serde_json::Value = parents_resp
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    // Fetch siblings at family level (for phylogenetic context)
    let family_key = species.get("familyKey").and_then(|v| v.as_i64());
    let siblings = if let Some(fk) = family_key {
        let sibs_url = format!("https://api.gbif.org/v1/species/{}/children?limit=10", fk);
        let sibs_resp = client
            .get(&sibs_url)
            .header("User-Agent", ua)
            .send()
            .await
            .ok();
        if let Some(r) = sibs_resp {
            r.json::<serde_json::Value>().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    // Fetch siblings at order level (other families in same order)
    let order_key = species.get("orderKey").and_then(|v| v.as_i64());
    let order_children = if let Some(ok) = order_key {
        let url = format!("https://api.gbif.org/v1/species/{}/children?limit=20", ok);
        let resp = client.get(&url).header("User-Agent", ua).send().await.ok();
        if let Some(r) = resp {
            r.json::<serde_json::Value>().await.ok()
        } else {
            None
        }
    } else {
        None
    };

    let result = json!({
        "species": species,
        "parents": parents,
        "family_siblings": siblings.unwrap_or(json!({"results": []})),
        "order_families": order_children.unwrap_or(json!({"results": []})),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── activate_formation tool ───────────────────────────────────────

async fn execute_activate_formation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let algorithm_name = input
        .get("algorithm_name")
        .and_then(|v| v.as_str())
        .ok_or("algorithm_name is required")?;
    let swarm_id_str = input
        .get("swarm_id")
        .and_then(|v| v.as_str())
        .ok_or("swarm_id is required")?;
    let swarm_id: uuid::Uuid = swarm_id_str
        .parse()
        .map_err(|_| "Invalid swarm_id UUID".to_string())?;

    let db = ctx.db.as_ref().ok_or("Database not available")?;
    let user_id = ctx
        .user_id
        .as_ref()
        .ok_or("User context required for formation activation")?;

    // Look up algorithm
    let algorithm = sqlx::query(
        "SELECT algorithm_id, name, display_name, formation_spec, tier, cost_credits \
         FROM swarm_algorithms WHERE name = $1",
    )
    .bind(algorithm_name)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Algorithm '{}' not found", algorithm_name))?;

    let algorithm_id: uuid::Uuid = algorithm.get("algorithm_id");
    let display_name: String = algorithm.get("display_name");
    let formation_spec: serde_json::Value = algorithm.get("formation_spec");
    let tier: String = algorithm.get("tier");
    let cost: i32 = algorithm.get("cost_credits");

    // Free algorithms return spec directly
    if tier == "free" {
        let result = json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Check idempotency
    let existing = sqlx::query(
        "SELECT activation_id FROM swarm_activations \
         WHERE user_id = $1 AND swarm_id = $2 AND algorithm_id = $3",
    )
    .bind(user_id)
    .bind(swarm_id)
    .bind(algorithm_id)
    .fetch_optional(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if existing.is_some() {
        let result = json!({
            "algorithm_id": algorithm_id,
            "name": algorithm_name,
            "display_name": display_name,
            "formation_spec": formation_spec,
            "activated": true,
            "charged": false,
            "message": "Already activated for this session",
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Charge credits
    let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
        .await
        .map_err(|e| format!("Wallet error: {}", e))?;

    fermi_auth::credit_charge(
        db,
        wallet.wallet_id,
        cost,
        "formation_activate",
        &format!("Activate {} formation", display_name),
        Some(&algorithm_id.to_string()),
    )
    .await
    .map_err(|e| format!("Payment failed: {}", e))?;

    // Insert activation
    let activation_id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO swarm_activations (activation_id, algorithm_id, user_id, swarm_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(activation_id)
    .bind(algorithm_id)
    .bind(user_id)
    .bind(swarm_id)
    .execute(db)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    let result = json!({
        "algorithm_id": algorithm_id,
        "activation_id": activation_id,
        "name": algorithm_name,
        "display_name": display_name,
        "formation_spec": formation_spec,
        "activated": true,
        "charged": true,
        "cost_credits": cost,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Enemy Sensor tool implementation ──────────────────────────────

async fn execute_scan_nearby_creatures(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::CellIndex;

    let creature_id_str = input
        .get("creature_id")
        .and_then(|v| v.as_str())
        .ok_or("creature_id is required")?;
    let creature_id: Uuid = creature_id_str
        .parse()
        .map_err(|_| "Invalid creature_id UUID".to_string())?;
    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    let pool = ctx.memory_store.pool();

    // 1. Look up target creature's current state + species info
    //    LEFT JOIN creature_state — creature may not have a state row yet (pre-flight)
    //    Fallback: use latest creature_flights for location
    let target = sqlx::query(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                COALESCE(cs.location_lat, cf.center_lat) AS location_lat,
                COALESCE(cs.location_lng, cf.center_lng) AS location_lng,
                cs.rabble_id, cs.state
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell, center_lat, center_lng FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON true
         WHERE c.creature_id = $1",
    )
    .bind(creature_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or("Creature not found")?;

    let h3_cell: Option<String> = target
        .try_get("h3_cell")
        .ok()
        .flatten()
        .filter(|s: &String| !s.is_empty());

    // Fallback: compute h3_cell from lat/lng if missing
    let h3_cell = match h3_cell {
        Some(c) => c,
        None => {
            let lat: Option<f64> = target.try_get("location_lat").ok().flatten();
            let lng: Option<f64> = target.try_get("location_lng").ok().flatten();
            match (lat, lng) {
                (Some(lat), Some(lng)) if lat != 0.0 || lng != 0.0 => {
                    use h3o::{LatLng, Resolution};
                    LatLng::new(lat, lng)
                        .map(|ll| ll.to_cell(Resolution::Twelve).to_string())
                        .map_err(|_| "Creature has no valid location".to_string())?
                }
                _ => return Err("Creature has no location — perch or fly first".to_string()),
            }
        }
    };

    let taxonomy: Option<serde_json::Value> = target.try_get("taxonomy").ok().flatten();
    let order = taxonomy
        .as_ref()
        .and_then(|t| t.get("order"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let family = taxonomy
        .as_ref()
        .and_then(|t| t.get("family"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");

    let target_info = json!({
        "creature_id": creature_id,
        "scientific_name": target.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
        "common_name": target.try_get::<Option<String>, _>("common_name").unwrap_or(None),
        "species_group": target.try_get::<Option<String>, _>("species_group").unwrap_or(None),
        "order": order,
        "family": family,
        "h3_cell": &h3_cell,
        "lat": target.try_get::<Option<f64>, _>("location_lat").unwrap_or(None),
        "lng": target.try_get::<Option<f64>, _>("location_lng").unwrap_or(None),
        "rabble_id": target.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten(),
    });

    // 2. Compute H3 grid disk
    let center_cell: CellIndex = h3_cell
        .parse()
        .map_err(|e| format!("Invalid H3 cell '{}': {}", h3_cell, e))?;
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    // 3. Query nearby creatures (excluding target, excluding private)
    //    Use LATERAL fallback to creature_flights for creatures without creature_state
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let sql = format!(
        "SELECT c.creature_id, c.scientific_name, c.common_name, c.species_group,
                c.taxonomy,
                COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) AS h3_cell,
                cs.rabble_id,
                COALESCE(cc.visibility, 'public') AS visibility
         FROM creatures c
         LEFT JOIN creature_state cs ON cs.creature_id = c.creature_id
         LEFT JOIN LATERAL (
             SELECT h3_cell FROM creature_flights
             WHERE creature_id = c.creature_id ORDER BY started_at DESC LIMIT 1
         ) cf ON cs.h3_cell IS NULL
         LEFT JOIN creature_conditions cc ON cc.creature_id = c.creature_id
         WHERE COALESCE(NULLIF(cs.h3_cell, ''), NULLIF(cf.h3_cell, '')) IN ({})
           AND c.creature_id != ${}
           AND COALESCE(cc.visibility, 'public') != 'private'
         LIMIT 50",
        in_clause,
        cell_strings.len() + 1
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    query = query.bind(creature_id);

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("DB error: {}", e))?;

    let target_rabble: Option<Uuid> = target
        .try_get::<Option<Uuid>, _>("rabble_id")
        .ok()
        .flatten();

    let nearby: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let tax: Option<serde_json::Value> = r.try_get("taxonomy").ok().flatten();
            let nearby_rabble: Option<Uuid> =
                r.try_get::<Option<Uuid>, _>("rabble_id").ok().flatten();
            let in_same_rabble = match (&target_rabble, &nearby_rabble) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
            json!({
                "creature_id": r.get::<Uuid, _>("creature_id"),
                "scientific_name": r.try_get::<Option<String>, _>("scientific_name").unwrap_or(None),
                "common_name": r.try_get::<Option<String>, _>("common_name").unwrap_or(None),
                "species_group": r.try_get::<Option<String>, _>("species_group").unwrap_or(None),
                "order": tax.as_ref().and_then(|t| t.get("order")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "family": tax.as_ref().and_then(|t| t.get("family")).and_then(|v| v.as_str()).unwrap_or("Unknown"),
                "h3_cell": r.try_get::<Option<String>, _>("h3_cell").unwrap_or(None),
                "in_same_rabble": in_same_rabble,
            })
        })
        .collect();

    let result = json!({
        "target": target_info,
        "nearby_count": nearby.len(),
        "nearby": nearby,
        "radius_rings": radius,
        "cells_searched": cell_strings.len(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── AR Spatial Suite tool implementations ─────────────────────────

async fn execute_h3_resolve(input: &serde_json::Value) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let operation = input
        .get("operation")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: operation")?;

    let parse_resolution = |input: &serde_json::Value| -> Result<Resolution, String> {
        let res = input
            .get("resolution")
            .and_then(|v| v.as_u64())
            .unwrap_or(12) as u8;
        Resolution::try_from(res).map_err(|_| format!("Invalid resolution: {}. Must be 0-15.", res))
    };

    let parse_cell = |s: &str| -> Result<CellIndex, String> {
        s.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell '{}': {}", s, e))
    };

    match operation {
        "gps_to_h3" => {
            let lat = input
                .get("lat")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lat'")?;
            let lng = input
                .get("lng")
                .and_then(|v| v.as_f64())
                .ok_or("gps_to_h3 requires 'lng'")?;
            let resolution = parse_resolution(input)?;

            let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
            let cell = ll.to_cell(resolution);
            let center = LatLng::from(cell);

            let result = json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(resolution),
                "center_lat": f64::from(center.lat()),
                "center_lng": f64::from(center.lng()),
                "input_lat": lat,
                "input_lng": lng,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "h3_to_gps" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("h3_to_gps requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;
            let center = LatLng::from(cell);

            let result = json!({
                "h3_cell": cell.to_string(),
                "resolution": u8::from(cell.resolution()),
                "lat": f64::from(center.lat()),
                "lng": f64::from(center.lng()),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "neighbors" => {
            let cell_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("neighbors requires 'h3_cell'")?;
            let cell = parse_cell(cell_str)?;

            // grid_disk(1) returns center + 6 neighbors
            let disk: Vec<CellIndex> = cell.grid_disk::<Vec<_>>(1);
            let neighbors: Vec<serde_json::Value> = disk
                .iter()
                .filter(|c| **c != cell)
                .map(|c| {
                    let ll = LatLng::from(*c);
                    json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let result = json!({
                "center": cell.to_string(),
                "neighbors": neighbors,
                "count": neighbors.len(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "distance" => {
            let cell_a_str = input
                .get("h3_cell")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell'")?;
            let cell_b_str = input
                .get("h3_cell_b")
                .and_then(|v| v.as_str())
                .ok_or("distance requires 'h3_cell_b'")?;
            let cell_a = parse_cell(cell_a_str)?;
            let cell_b = parse_cell(cell_b_str)?;

            let distance = cell_a
                .grid_distance(cell_b)
                .map_err(|_| "Cannot compute distance between cells at different resolutions or too far apart")?;

            let result = json!({
                "cell_a": cell_a.to_string(),
                "cell_b": cell_b.to_string(),
                "grid_distance": distance,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        "grid_disk" => {
            let lat = input.get("lat").and_then(|v| v.as_f64());
            let lng = input.get("lng").and_then(|v| v.as_f64());
            let cell_str = input.get("h3_cell").and_then(|v| v.as_str());
            let k = input.get("k").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let resolution = parse_resolution(input)?;

            let center_cell = if let Some(cs) = cell_str {
                parse_cell(cs)?
            } else if let (Some(lat), Some(lng)) = (lat, lng) {
                let ll = LatLng::new(lat, lng)
                    .map_err(|e| format!("Invalid coordinates: {}", e))?;
                ll.to_cell(resolution)
            } else {
                return Err("grid_disk requires either 'h3_cell' or 'lat'+'lng'".to_string());
            };

            let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(k);
            let cells: Vec<serde_json::Value> = disk
                .iter()
                .map(|c| {
                    let ll = LatLng::from(*c);
                    json!({
                        "h3_cell": c.to_string(),
                        "lat": f64::from(ll.lat()),
                        "lng": f64::from(ll.lng()),
                    })
                })
                .collect();

            let total = 3 * k * k + 3 * k + 1;
            let result = json!({
                "center": center_cell.to_string(),
                "k": k,
                "resolution": u8::from(resolution),
                "total_cells": total,
                "cells": cells,
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        other => Err(format!(
            "Unknown h3_resolve operation: '{}'. Use: gps_to_h3, h3_to_gps, neighbors, distance, grid_disk",
            other
        )),
    }
}

async fn execute_geocode(input: &serde_json::Value) -> Result<String, String> {
    let address = input
        .get("address")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: address")?;

    let client = reqwest::Client::new();
    let response = client
        .get("https://nominatim.openstreetmap.org/search")
        .query(&[
            ("q", address),
            ("format", "json"),
            ("limit", "3"),
            ("addressdetails", "1"),
        ])
        .header("User-Agent", "AgentBestiary/1.0 (AR Spatial Suite)")
        .send()
        .await
        .map_err(|e| format!("Geocoding request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Nominatim error: {}", response.status()));
    }

    let results: Vec<serde_json::Value> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse geocoding response: {}", e))?;

    if results.is_empty() {
        return Ok(json!({
            "status": "not_found",
            "message": format!("No results for '{}'. Try a more specific address or use GPS coordinates directly.", address)
        }).to_string());
    }

    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let lat: f64 = r
                .get("lat")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let lng: f64 = r
                .get("lon")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            json!({
                "lat": lat,
                "lng": lng,
                "display_name": r.get("display_name").and_then(|v| v.as_str()).unwrap_or(""),
                "type": r.get("type").and_then(|v| v.as_str()).unwrap_or(""),
                "importance": r.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.0),
            })
        })
        .collect();

    let result = json!({
        "query": address,
        "results": formatted,
        "best_match": formatted.first(),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_beacon(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("create_beacon requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("create_beacon requires a user context")?;

    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lat")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: lng")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let cell = ll.to_cell(resolution);
    let center = LatLng::from(cell);

    let asset_type = input
        .get("asset_type")
        .and_then(|v| v.as_str())
        .unwrap_or("image");
    let azimuth = input
        .get("azimuth_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let elevation = input
        .get("elevation_deg")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let billboard = input
        .get("billboard")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let scale = input.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let ttl_seconds = input
        .get("ttl_seconds")
        .and_then(|v| v.as_i64())
        .unwrap_or(86400) as i32;
    let decay_style = input
        .get("decay_style")
        .and_then(|v| v.as_str())
        .unwrap_or("fade");
    let visibility = input
        .get("visibility")
        .and_then(|v| v.as_str())
        .unwrap_or("public");
    let tags = input.get("tags").cloned().unwrap_or(json!([]));
    let interaction = input.get("interaction").cloned().unwrap_or(json!({}));

    let now = chrono::Utc::now();
    let expires_at = now + chrono::Duration::seconds(ttl_seconds as i64);
    let beacon_id = Uuid::new_v4();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_beacons (beacon_id, workspace_id, creator_id, agent_name,
         h3_cell, h3_resolution, center_lat, center_lng,
         asset_path, asset_type,
         azimuth_deg, elevation_deg, billboard, scale,
         ttl_seconds, decay_style, expires_at,
         visibility, tags, interaction, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $21)"
    )
    .bind(beacon_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind("ar_beacon")
    .bind(cell.to_string())
    .bind(res_num as i32)
    .bind(f64::from(center.lat()))
    .bind(f64::from(center.lng()))
    .bind(asset_path)
    .bind(asset_type)
    .bind(azimuth)
    .bind(elevation)
    .bind(billboard)
    .bind(scale)
    .bind(ttl_seconds)
    .bind(decay_style)
    .bind(expires_at)
    .bind(visibility)
    .bind(&tags)
    .bind(&interaction)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create beacon: {}", e))?;

    let result = json!({
        "beacon_id": beacon_id,
        "h3_cell": cell.to_string(),
        "h3_resolution": res_num,
        "center_lat": f64::from(center.lat()),
        "center_lng": f64::from(center.lng()),
        "asset_path": asset_path,
        "asset_url": format!("/api/beacons/{}/asset", beacon_id),
        "expires_at": expires_at.to_rfc3339(),
        "ttl_seconds": ttl_seconds,
        "decay_style": decay_style,
        "visibility": visibility,
        "orientation": {
            "azimuth_deg": azimuth,
            "elevation_deg": elevation,
            "billboard": billboard,
        },
        "scale": scale,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_beacons(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{CellIndex, LatLng, Resolution};

    let radius = input
        .get("radius_rings")
        .and_then(|v| v.as_u64())
        .unwrap_or(3) as u32;
    let res_num = input
        .get("resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let include_expired = input
        .get("include_expired")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let resolution =
        Resolution::try_from(res_num).map_err(|_| format!("Invalid resolution: {}", res_num))?;

    // Resolve center cell from h3_cell or lat/lng
    let center_cell = if let Some(cs) = input.get("h3_cell").and_then(|v| v.as_str()) {
        cs.parse::<CellIndex>()
            .map_err(|e| format!("Invalid H3 cell: {}", e))?
    } else {
        let lat = input
            .get("lat")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'h3_cell' or 'lat'+'lng'")?;
        let lng = input
            .get("lng")
            .and_then(|v| v.as_f64())
            .ok_or("query_beacons requires 'lng'")?;
        let ll = LatLng::new(lat, lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
        ll.to_cell(resolution)
    };

    // Compute all cells in the search radius
    let disk: Vec<CellIndex> = center_cell.grid_disk::<Vec<_>>(radius);
    let cell_strings: Vec<String> = disk.iter().map(|c| c.to_string()).collect();

    let pool = ctx.memory_store.pool();

    // Build query with IN clause for H3 cells
    let placeholders: Vec<String> = (1..=cell_strings.len())
        .map(|i| format!("${}", i))
        .collect();
    let in_clause = placeholders.join(", ");

    let time_filter = if include_expired {
        "".to_string()
    } else {
        format!(" AND expires_at > ${}", cell_strings.len() + 1)
    };

    let sql = format!(
        "SELECT beacon_id, workspace_id, h3_cell, h3_resolution, center_lat, center_lng,
                asset_path, asset_type, azimuth_deg, elevation_deg, billboard, scale,
                ttl_seconds, decay_style, expires_at, visibility, tags, interaction,
                created_at
         FROM ar_beacons WHERE h3_cell IN ({}){}
         ORDER BY created_at DESC LIMIT 100",
        in_clause, time_filter
    );

    let mut query = sqlx::query(&sql);
    for cs in &cell_strings {
        query = query.bind(cs);
    }
    if !include_expired {
        query = query.bind(chrono::Utc::now());
    }

    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Beacon query failed: {}", e))?;

    let beacons: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            json!({
                "beacon_id": row.get::<Uuid, _>("beacon_id"),
                "workspace_id": row.get::<Uuid, _>("workspace_id"),
                "h3_cell": row.get::<String, _>("h3_cell"),
                "center_lat": row.get::<f64, _>("center_lat"),
                "center_lng": row.get::<f64, _>("center_lng"),
                "asset_path": row.get::<String, _>("asset_path"),
                "asset_type": row.get::<String, _>("asset_type"),
                "asset_url": format!("/api/beacons/{}/asset", row.get::<Uuid, _>("beacon_id")),
                "orientation": {
                    "azimuth_deg": row.get::<f64, _>("azimuth_deg"),
                    "elevation_deg": row.get::<f64, _>("elevation_deg"),
                    "billboard": row.get::<bool, _>("billboard"),
                },
                "scale": row.get::<f64, _>("scale"),
                "expires_at": row.get::<chrono::DateTime<chrono::Utc>, _>("expires_at").to_rfc3339(),
                "visibility": row.get::<String, _>("visibility"),
                "tags": row.get::<serde_json::Value, _>("tags"),
                "interaction": row.get::<serde_json::Value, _>("interaction"),
            })
        })
        .collect();

    let result = json!({
        "center": center_cell.to_string(),
        "radius_rings": radius,
        "total_beacons": beacons.len(),
        "beacons": beacons,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_save_grid_map(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    use h3o::{LatLng, Resolution};

    let workspace_id = ctx
        .workspace_id
        .ok_or("save_grid_map requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("save_grid_map requires a user context")?;

    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: name")?;
    let center_lat = input
        .get("center_lat")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lat")?;
    let center_lng = input
        .get("center_lng")
        .and_then(|v| v.as_f64())
        .ok_or("Missing required parameter: center_lng")?;

    let description = input.get("description").and_then(|v| v.as_str());
    let grid_res = input
        .get("grid_resolution")
        .and_then(|v| v.as_u64())
        .unwrap_or(12) as u8;
    let radius_rings = input
        .get("radius_rings")
        .and_then(|v| v.as_i64())
        .unwrap_or(5) as i32;
    let quadrants = input.get("quadrants").cloned().unwrap_or(json!([]));
    let zones = input.get("zones").cloned().unwrap_or(json!([]));

    let resolution =
        Resolution::try_from(grid_res).map_err(|_| format!("Invalid resolution: {}", grid_res))?;
    // Center resolution is 3 levels above grid resolution (or 0 if grid_res < 3)
    let center_res_num = if grid_res >= 3 { grid_res - 3 } else { 0 };
    let center_resolution = Resolution::try_from(center_res_num)
        .map_err(|_| format!("Invalid center resolution: {}", center_res_num))?;

    let ll =
        LatLng::new(center_lat, center_lng).map_err(|e| format!("Invalid coordinates: {}", e))?;
    let center_cell = ll.to_cell(center_resolution);

    let k = radius_rings as u32;
    let total_cells = (3 * k * k + 3 * k + 1) as i32;
    let map_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO ar_grid_maps (map_id, workspace_id, creator_id, name, description,
         center_lat, center_lng, center_h3, center_resolution,
         grid_resolution, radius_rings, total_cells,
         quadrants, zones, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)",
    )
    .bind(map_id)
    .bind(workspace_id)
    .bind(user_id)
    .bind(name)
    .bind(description)
    .bind(center_lat)
    .bind(center_lng)
    .bind(center_cell.to_string())
    .bind(center_res_num as i32)
    .bind(grid_res as i32)
    .bind(radius_rings)
    .bind(total_cells)
    .bind(&quadrants)
    .bind(&zones)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to save grid map: {}", e))?;

    let result = json!({
        "map_id": map_id,
        "name": name,
        "center_h3": center_cell.to_string(),
        "center_resolution": center_res_num,
        "grid_resolution": grid_res,
        "radius_rings": radius_rings,
        "total_cells": total_cells,
        "quadrants_count": quadrants.as_array().map(|a| a.len()).unwrap_or(0),
        "zones_count": zones.as_array().map(|a| a.len()).unwrap_or(0),
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Rabble.world creature tools ───────────────────────────────────

async fn execute_gbif_species_search(input: &serde_json::Value) -> Result<String, String> {
    // Direct key lookup
    if let Some(key) = input.get("gbif_key").and_then(|v| v.as_i64()) {
        let url = format!("https://api.gbif.org/v1/species/{}", key);
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
            .map_err(|e| format!("GBIF request failed: {}", e))?;

        let species: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

        // Also fetch media
        let media_url = format!("https://api.gbif.org/v1/species/{}/media", key);
        let media_resp = client
            .get(&media_url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
            .ok();

        let media: Option<serde_json::Value> = if let Some(r) = media_resp {
            r.json().await.ok()
        } else {
            None
        };

        let result = json!({
            "species": species,
            "media": media.unwrap_or(json!({"results": []})),
        });
        return serde_json::to_string_pretty(&result)
            .map_err(|e| format!("Serialization error: {}", e));
    }

    // Search by name
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Either 'query' or 'gbif_key' is required")?;
    let rank = input
        .get("rank")
        .and_then(|v| v.as_str())
        .unwrap_or("SPECIES");
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

    let limit_str = limit.to_string();
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.gbif.org/v1/species/search")
        .query(&[
            ("q", query),
            ("rank", rank),
            ("limit", limit_str.as_str()),
            ("highertaxonKey", "216"), // Insecta
        ])
        .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
        .send()
        .await
        .map_err(|e| format!("GBIF request failed: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

    // Extract just the useful fields from results
    let results = body
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let species: Vec<serde_json::Value> = results
        .into_iter()
        .map(|s| {
            json!({
                "key": s.get("key"),
                "scientificName": s.get("scientificName"),
                "canonicalName": s.get("canonicalName"),
                "vernacularName": s.get("vernacularName"),
                "kingdom": s.get("kingdom"),
                "phylum": s.get("phylum"),
                "class": s.get("class"),
                "order": s.get("order"),
                "family": s.get("family"),
                "genus": s.get("genus"),
                "species": s.get("species"),
                "rank": s.get("rank"),
                "taxonomicStatus": s.get("taxonomicStatus"),
            })
        })
        .collect();

    let result = json!({
        "count": species.len(),
        "species": species,
        "note": "Use gbif_key with a species key for full details + media"
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_mint_creature(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx
        .workspace_id
        .ok_or("mint_creature requires a workspace context")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("mint_creature requires a user context")?;

    let scientific_name = input
        .get("scientific_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: scientific_name")?;
    let asset_path = input
        .get("asset_path")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: asset_path")?;

    let common_name = input.get("common_name").and_then(|v| v.as_str());
    let species_group = input
        .get("species_group")
        .and_then(|v| v.as_str())
        .unwrap_or("butterfly");
    let gbif_key = input.get("gbif_key").and_then(|v| v.as_i64());
    let taxonomy = input.get("taxonomy").cloned().unwrap_or(json!({}));
    let flight_silhouette_path = input.get("flight_silhouette_path").and_then(|v| v.as_str());
    let specimen_name = input.get("specimen_name").and_then(|v| v.as_str());
    let variation_notes = input.get("variation_notes").and_then(|v| v.as_str());

    let creature_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Generate a specimen name if not provided
    let final_specimen_name = specimen_name.map(|s| s.to_string()).unwrap_or_else(|| {
        let base = common_name.unwrap_or(scientific_name);
        format!("{} #{}", base, &creature_id.to_string()[..6])
    });

    let pool = ctx.memory_store.pool();
    sqlx::query(
        "INSERT INTO creatures (creature_id, owner_id, workspace_id,
         scientific_name, common_name, species_group, gbif_key,
         taxonomy, specimen_name, variation_notes,
         asset_path, flight_silhouette_path,
         created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)",
    )
    .bind(creature_id)
    .bind(user_id)
    .bind(workspace_id)
    .bind(scientific_name)
    .bind(common_name)
    .bind(species_group)
    .bind(gbif_key)
    .bind(&taxonomy)
    .bind(&final_specimen_name)
    .bind(variation_notes)
    .bind(asset_path)
    .bind(flight_silhouette_path)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to mint creature: {}", e))?;

    let result = json!({
        "creature_id": creature_id,
        "specimen_name": final_specimen_name,
        "scientific_name": scientific_name,
        "common_name": common_name,
        "species_group": species_group,
        "gbif_key": gbif_key,
        "asset_path": asset_path,
        "variation_notes": variation_notes,
        "data_card": {
            "minted_at": now.to_rfc3339(),
            "minted_by": user_id,
            "workspace_id": workspace_id,
            "taxonomy": taxonomy,
        }
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Generate a unique naturalist illustration for a creature.
///
/// Pipeline: resolve species → fetch GBIF media → build art prompt → Gemini generate → save PNG → update DB
async fn execute_generate_specimen_art(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image generation unavailable")?;

    let pool = ctx.memory_store.pool();

    // ── Step 1: Resolve creature data ──
    // Either from creature_id (DB lookup) or from input params directly
    let (creature_id, scientific_name, common_name, species_group, gbif_key) =
        if let Some(id_str) = input.get("creature_id").and_then(|v| v.as_str()) {
            let cid =
                Uuid::parse_str(id_str).map_err(|_| format!("Invalid creature_id: {}", id_str))?;
            let row = sqlx::query(
                "SELECT creature_id, scientific_name, common_name, species_group, gbif_key
                 FROM creatures WHERE creature_id = $1",
            )
            .bind(cid)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB lookup failed: {}", e))?
            .ok_or_else(|| format!("Creature {} not found", cid))?;

            (
                Some(cid),
                row.get::<String, _>("scientific_name"),
                row.get::<Option<String>, _>("common_name"),
                row.get::<String, _>("species_group"),
                row.get::<Option<i64>, _>("gbif_key"),
            )
        } else {
            let sci = input
                .get("scientific_name")
                .and_then(|v| v.as_str())
                .ok_or("Either creature_id or scientific_name is required")?;
            (
                None,
                sci.to_string(),
                input
                    .get("common_name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                input
                    .get("species_group")
                    .and_then(|v| v.as_str())
                    .unwrap_or("butterfly")
                    .to_string(),
                input.get("gbif_key").and_then(|v| v.as_i64()),
            )
        };

    let style = input
        .get("style")
        .and_then(|v| v.as_str())
        .unwrap_or("naturalist");

    // ── Step 2: Fetch GBIF reference media description ──
    let mut reference_desc = String::new();
    if let Some(key) = gbif_key {
        let client = reqwest::Client::new();
        let media_url = format!("https://api.gbif.org/v1/species/{}/media", key);
        if let Ok(resp) = client
            .get(&media_url)
            .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
            .send()
            .await
        {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(results) = body.get("results").and_then(|v| v.as_array()) {
                    // Collect descriptions from first few media items for reference
                    let descs: Vec<&str> = results
                        .iter()
                        .take(3)
                        .filter_map(|m| {
                            m.get("description")
                                .or(m.get("title"))
                                .and_then(|v| v.as_str())
                        })
                        .collect();
                    if !descs.is_empty() {
                        reference_desc = format!(" Reference descriptions: {}", descs.join("; "));
                    }
                }
            }
        }
    }

    // ── Step 3: Build art generation prompt ──
    let display_name = common_name
        .as_deref()
        .map(|c| format!("{} ({})", c, scientific_name))
        .unwrap_or_else(|| scientific_name.clone());

    let style_instruction = match style {
        "watercolor" => "Soft watercolor painting style with visible brush strokes and subtle color bleeding at edges. Muted earth tones with occasional vivid accents.",
        "botanical" => "Precise botanical illustration style on cream parchment background. Fine ink linework with delicate hand-tinted color washes. Labeled anatomical features.",
        "field-guide" => "Clean field guide illustration style. Crisp outlines, accurate proportions, neutral white background, specimen positioned at 3/4 view with wings spread.",
        "ukiyo-e" => "Japanese woodblock print (ukiyo-e) style in the tradition of Edo-period naturalist prints. Bold black outlines with flat color planes. Subtle gradation (bokashi) on wings. Warm washi paper background texture. Include a small red hanko seal stamp in one corner. Muted indigo, ochre, and grey tones with selective bold color accents. Multiple views of the same specimen at different scales, as in traditional insect study prints.",
        _ => "Detailed naturalist scientific illustration in the style of Maria Sibylla Merian. Rich, accurate colors on aged vellum background. Fine detail on wing patterns and body segments.",
    };

    let group_detail = match species_group.as_str() {
        "dragonfly" => "Show detailed wing venation patterns, elongated abdomen segments, and compound eye structure. Wings should be translucent with visible cells.",
        "beetle" => "Show detailed elytra (wing covers) with surface texture, compound eyes, segmented antennae, and jointed legs. Ventral view option showing wing deployment.",
        "bee" => "Show fuzzy body texture, compound eyes, pollen baskets on legs, translucent wing venation, and banded abdomen coloring.",
        "locust" => "Show powerful hind legs, segmented antennae, compound eyes, and folded wing structure. Textured exoskeleton detail.",
        "fly" => "Show compound eyes, halteres, translucent wing venation, and segmented body. Metallic sheen where appropriate.",
        "bug" => "Show piercing-sucking mouthparts, shield-shaped body, wing membrane detail, and segmented antennae.",
        _ => "Show detailed wing scale patterns, proboscis, antennae, and leg segments. Upper and lower wing surfaces visible.",
    };

    let prompt = format!(
        "Create a beautiful scientific illustration of a {} ({}).\n\n\
         Style: {}\n\n\
         Species details: {}\n\n\
         Requirements:\n\
         - Single specimen, centered composition\n\
         - Anatomically accurate proportions and markings\n\
         - {}\n\
         - No text, labels, or watermarks\n\
         - Square format, high detail\n\
         - Dark background (#1A2E20) to make the specimen pop{}",
        display_name,
        species_group,
        style_instruction,
        group_detail,
        if species_group == "dragonfly" {
            "Include subtle iridescence on wings and thorax"
        } else {
            "Include subtle iridescence on wing scales where appropriate"
        },
        reference_desc
    );

    // ── Step 4: Generate image via Gemini ──
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

    // Extract base64 image data
    let (mime_type, image_data) = gemini_resp
        .candidates
        .iter()
        .flat_map(|c| c.content.parts.iter())
        .find_map(|p| {
            p.inline_data
                .as_ref()
                .map(|d| (d.mime_type.clone(), d.data.clone()))
        })
        .ok_or("Gemini returned no image data")?;

    // ── Step 5: Save image to static/creatures/ ──
    let extension = if mime_type.contains("png") {
        "png"
    } else if mime_type.contains("webp") {
        "webp"
    } else {
        "jpg"
    };

    let file_id = creature_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let filename = format!("{}.{}", file_id, extension);
    let relative_path = format!("/static/creatures/{}", filename);
    let fs_path = format!("static/creatures/{}", filename);

    // Decode base64 and write
    use base64::Engine;
    let decoder = base64::engine::general_purpose::STANDARD;
    let bytes = decoder
        .decode(&image_data)
        .map_err(|e| format!("Failed to decode image data: {}", e))?;

    // Ensure directory exists
    std::fs::create_dir_all("static/creatures")
        .map_err(|e| format!("Failed to create creatures directory: {}", e))?;
    std::fs::write(&fs_path, &bytes).map_err(|e| format!("Failed to write image: {}", e))?;

    // ── Step 6: Update creature record if creature_id provided ──
    let generation_params = json!({
        "style": style,
        "prompt": prompt,
        "mime_type": mime_type,
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "gbif_key": gbif_key,
        "file_size_bytes": bytes.len(),
    });

    if let Some(cid) = creature_id {
        sqlx::query(
            "UPDATE creatures SET asset_path = $1, generation_params = $2, updated_at = NOW()
             WHERE creature_id = $3",
        )
        .bind(&relative_path)
        .bind(&generation_params)
        .bind(cid)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to update creature record: {}", e))?;
    }

    let result = json!({
        "status": "generated",
        "creature_id": creature_id,
        "asset_path": relative_path,
        "mime_type": mime_type,
        "file_size_bytes": bytes.len(),
        "style": style,
        "scientific_name": scientific_name,
        "common_name": common_name,
    });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Wing segmentation tool ────────────────────────────────────────

async fn execute_segment_creature_wings(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — wing segmentation unavailable")?;

    let pool = ctx.memory_store.pool();

    // Parse creature_id
    let creature_id_str = input
        .get("creature_id")
        .and_then(|v| v.as_str())
        .ok_or("creature_id is required")?;
    let creature_id = Uuid::parse_str(creature_id_str)
        .map_err(|_| format!("Invalid creature_id: {}", creature_id_str))?;

    // Look up creature
    let row =
        sqlx::query("SELECT species_group, animation_status FROM creatures WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB lookup failed: {}", e))?
            .ok_or_else(|| format!("Creature {} not found", creature_id))?;

    let species_group: String = row.get("species_group");
    if species_group != "butterfly" {
        return Err(
            "Wing segmentation only works for butterflies. Other species coming soon!".to_string(),
        );
    }

    let status: Option<String> = row.try_get("animation_status").unwrap_or(None);
    if status.as_deref() == Some("ready") {
        return Ok(json!({
            "status": "already_ready",
            "creature_id": creature_id,
            "layers": {
                "body": format!("/api/creatures/{}/animation/body", creature_id),
                "left_wing": format!("/api/creatures/{}/animation/left_wing", creature_id),
                "right_wing": format!("/api/creatures/{}/animation/right_wing", creature_id),
            }
        })
        .to_string());
    }

    // Charge credits if user_id and gas_fees available
    if let (Some(ref gas_fees), Some(ref user_id)) = (&ctx.gas_fees, &ctx.user_id) {
        let wallet = fermi_auth::get_or_create_wallet(pool, "user", user_id)
            .await
            .map_err(|e| format!("Wallet error: {}", e))?;
        crate::gas::charge_gas(
            pool,
            wallet.wallet_id,
            gas_fees.creature_animate,
            "creature_animate",
            &format!("Wing segmentation for creature {}", creature_id),
            Some(&creature_id.to_string()),
        )
        .await
        .map_err(|e| format!("Credit charge failed: {}", e.1))?;
    }

    // Set status to processing
    let _ = sqlx::query(
        "UPDATE creatures SET animation_status = 'processing', updated_at = NOW() WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await;

    // Fetch source image from creature_images
    let img_row =
        sqlx::query("SELECT image_bytes, mime_type FROM creature_images WHERE creature_id = $1")
            .bind(creature_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("DB error fetching image: {}", e))?
            .ok_or_else(|| "No image found for creature. Generate art first.".to_string())?;

    let image_bytes: Vec<u8> = img_row.get("image_bytes");
    let source_mime: String = img_row.get("mime_type");

    use base64::Engine;
    let encoder = base64::engine::general_purpose::STANDARD;
    let img_base64 = encoder.encode(&image_bytes);

    // Segmentation prompts
    let layers = [
        ("left_wing", "Isolate ONLY the left wing (viewer's left) of this butterfly specimen. Remove the body, right wing, antennae, and all other parts completely. Output ONLY the left wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image."),
        ("right_wing", "Isolate ONLY the right wing (viewer's right) of this butterfly specimen. Remove the body, left wing, antennae, and all other parts completely. Output ONLY the right wing on a fully transparent background (PNG with alpha). Preserve the exact wing shape, coloration, scale patterns, and venation. The wing should be positioned exactly where it appears in the original image."),
        ("body", "Isolate ONLY the body (thorax, abdomen, head, antennae, legs) of this butterfly specimen. Remove both wings completely, leaving only the central body structure. Output on a fully transparent background (PNG with alpha). Preserve exact body position, coloration, and detail from the original image."),
    ];

    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for (layer_name, prompt) in &layers {
        let body = json!({
            "contents": [{
                "parts": [
                    { "text": prompt },
                    {
                        "inlineData": {
                            "mimeType": source_mime,
                            "data": img_base64
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
            .map_err(|e| format!("Gemini request failed for {}: {}", layer_name, e))?;

        if !response.status().is_success() {
            let err = response.text().await.unwrap_or_default();
            let _ = sqlx::query(
                "UPDATE creatures SET animation_status = 'failed', updated_at = NOW() WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(pool)
            .await;
            return Err(format!("Gemini error for {}: {}", layer_name, err));
        }

        let gemini_resp: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Parse error for {}: {}", layer_name, e))?;

        let inline_data = gemini_resp
            .pointer("/candidates/0/content/parts")
            .and_then(|parts| parts.as_array())
            .and_then(|parts| parts.iter().find_map(|p| p.get("inlineData")))
            .ok_or_else(|| format!("No image in Gemini response for {}", layer_name))?;

        let mime_type = inline_data
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("image/png");
        let b64_data = inline_data
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("No image data for {}", layer_name))?;

        let decoded = encoder
            .decode(b64_data)
            .map_err(|e| format!("Decode error for {}: {}", layer_name, e))?;

        if decoded.len() < 100 {
            let _ = sqlx::query(
                "UPDATE creatures SET animation_status = 'failed', updated_at = NOW() WHERE creature_id = $1",
            )
            .bind(creature_id)
            .execute(pool)
            .await;
            return Err(format!(
                "Layer {} too small ({} bytes), segmentation likely failed",
                layer_name,
                decoded.len()
            ));
        }

        // Persist to DB (inline upsert — handlers module not accessible from lib crate)
        let _ = sqlx::query(
            "INSERT INTO creature_animation_layers (creature_id, layer_name, image_bytes, mime_type, file_size)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (creature_id, layer_name) DO UPDATE
             SET image_bytes = $3, mime_type = $4, file_size = $5, updated_at = NOW()",
        )
        .bind(creature_id)
        .bind(*layer_name)
        .bind(&decoded)
        .bind(mime_type)
        .bind(decoded.len() as i32)
        .execute(pool)
        .await;

        results.push(json!({
            "layer": layer_name,
            "mime_type": mime_type,
            "file_size_bytes": decoded.len(),
            "url": format!("/api/creatures/{}/animation/{}", creature_id, layer_name),
        }));

        // Rate limit between calls
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Mark as ready
    let _ = sqlx::query(
        "UPDATE creatures SET animation_status = 'ready', updated_at = NOW() WHERE creature_id = $1",
    )
    .bind(creature_id)
    .execute(pool)
    .await;

    Ok(json!({
        "status": "ready",
        "creature_id": creature_id,
        "message": "Wing segmentation complete. Your butterfly is now flight-ready.",
        "layers": results,
    })
    .to_string())
}

// ─── Marketplace tool implementations ──────────────────────────────

async fn execute_get_shopping_profile(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for get_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for get_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?;

    match profile {
        Some(p) => {
            let result = json!({
                "profile_id": p.profile_id,
                "profile_name": p.profile_name,
                "embedding_version": p.embedding_version,
                "episode_count": p.episode_count,
                "category_tags": p.category_tags,
                "price_sensitivity": p.price_sensitivity,
                "quality_bias": p.quality_bias,
                "brand_affinities": p.brand_affinities,
                "is_listed": p.is_listed,
                "updated_at": p.updated_at.to_rfc3339(),
            });
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("Serialization error: {}", e))
        }
        None => Ok(json!({
            "status": "not_found",
            "message": format!("No shopping profile '{}' found. Use update_shopping_profile to create one.", profile_name)
        })
        .to_string()),
    }
}

async fn execute_update_shopping_profile(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for update_shopping_profile")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for update_shopping_profile")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Extract metadata from input
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let price_sensitivity = input.get("price_sensitivity").and_then(|v| v.as_f64());
    let quality_bias = input.get("quality_bias").and_then(|v| v.as_f64());
    let brand_affinities = input.get("brand_affinities").cloned().unwrap_or(json!({}));

    // Compute composite embedding from episodes (weighted centroid)
    let episodes = ctx
        .memory_store
        .get_all_episodes_with_embeddings(agent_id)
        .await
        .map_err(|e| format!("Episode fetch failed: {}", e))?;

    let now = chrono::Utc::now();
    let mut weighted_sum: Option<Vec<f64>> = None;
    let mut total_weight = 0.0f64;
    let mut episode_count = 0i32;

    for episode in &episodes {
        if let Some(ref emb) = episode.embedding {
            let age_days = (now - episode.timestamp_ref).num_hours() as f64 / 24.0;
            let recency_weight = (-0.1 * age_days).exp();
            let success_weight = match episode.execution_status {
                agent_bestiary_memory::ExecutionStatus::Success => 1.0,
                _ => 0.3,
            };
            let w = recency_weight * success_weight;

            match &mut weighted_sum {
                Some(sum) => {
                    for (i, &val) in emb.iter().enumerate() {
                        if i < sum.len() {
                            sum[i] += w * val as f64;
                        }
                    }
                }
                None => {
                    weighted_sum = Some(emb.iter().map(|&v| w * v as f64).collect());
                }
            }
            total_weight += w;
            episode_count += 1;
        }
    }

    // L2 normalize the composite embedding
    let composite: Option<Vec<f32>> = weighted_sum.map(|sum| {
        let norm: f64 = sum.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 1e-10 {
            sum.iter().map(|&v| (v / norm) as f32).collect()
        } else {
            sum.iter().map(|&v| v as f32).collect()
        }
    });

    let profile_id = ctx
        .memory_store
        .upsert_shopping_profile(
            user_id,
            agent_id,
            profile_name,
            composite.as_deref(),
            episode_count,
            &category_tags,
            price_sensitivity,
            quality_bias,
            &brand_affinities,
        )
        .await
        .map_err(|e| format!("Profile upsert failed: {}", e))?;

    let result = json!({
        "profile_id": profile_id,
        "profile_name": profile_name,
        "episode_count": episode_count,
        "embedding_computed": composite.is_some(),
        "category_tags": category_tags,
        "price_sensitivity": price_sensitivity,
        "quality_bias": quality_bias,
        "brand_affinities": brand_affinities,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_list_marketplace(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let cat_str = input.get("category").and_then(|v| v.as_str());
    let cat_filter: Option<Vec<String>> =
        cat_str.map(|s| s.split(',').map(|t| t.trim().to_string()).collect());
    let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

    let listings = ctx
        .memory_store
        .get_active_listings(cat_filter.as_deref(), limit)
        .await
        .map_err(|e| format!("Marketplace query failed: {}", e))?;

    let items: Vec<serde_json::Value> = listings
        .iter()
        .map(|l| {
            json!({
                "listing_id": l.listing_id,
                "seller_id": l.seller_id,
                "price_credits": l.price_credits,
                "total_queries": l.total_queries,
                "category_tags": l.category_tags,
                "description": l.description,
            })
        })
        .collect();

    let result = json!({ "listings": items, "count": items.len() });
    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_create_listing(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for create_listing")?;
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or("No user context for create_listing")?;
    let profile_name = input
        .get("profile_name")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let price_credits = input
        .get("price_credits")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1) as i32;
    let max_queries = input
        .get("max_queries_per_buyer")
        .and_then(|v| v.as_i64())
        .map(|v| v as i32);
    let category_tags: Vec<String> = input
        .get("category_tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let description = input.get("description").and_then(|v| v.as_str());

    // Look up the profile
    let profile = ctx
        .memory_store
        .get_shopping_profile(user_id, agent_id, profile_name)
        .await
        .map_err(|e| format!("Profile lookup failed: {}", e))?
        .ok_or_else(|| {
            format!(
                "No shopping profile '{}' found. Create one with update_shopping_profile first.",
                profile_name
            )
        })?;

    // Charge listing fee if pool is available
    if let (Some(db), Some(gas)) = (&ctx.db, &ctx.gas_fees) {
        let wallet = fermi_auth::get_or_create_wallet(db, "user", user_id)
            .await
            .map_err(|e| format!("Wallet error: {}", e))?;
        fermi_auth::credit_charge(
            db,
            wallet.wallet_id,
            gas.marketplace_listing_fee,
            "marketplace_listing_fee",
            "Marketplace listing creation",
            Some(&profile.profile_id.to_string()),
        )
        .await
        .map_err(|e| format!("Insufficient credits for listing fee: {}", e))?;
    }

    let listing_id = ctx
        .memory_store
        .create_marketplace_listing(
            profile.profile_id,
            user_id,
            price_credits,
            max_queries,
            &category_tags,
            description,
        )
        .await
        .map_err(|e| format!("Listing creation failed: {}", e))?;

    let result = json!({
        "listing_id": listing_id,
        "profile_id": profile.profile_id,
        "status": "active",
        "price_credits": price_credits,
        "message": format!("Profile '{}' is now listed on the marketplace at {} credits per query.", profile_name, price_credits),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
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

    // Enrich card with KG context from past dream cycles
    let card = if let Some(ref db) = ctx.db {
        crate::agent_backend::kg_context::enrich_with_kg_context_by_name(
            &ctx.memory_store,
            &ctx.embedder,
            db,
            agent_name,
            query,
            card,
        )
        .await
    } else {
        card
    };

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
        creature_id: None,
        cognition_tier: None,
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

async fn execute_delegate_to_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_name = input
        .get("agent_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_name")?;
    let task = input
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: task")?;

    let ws_id = ctx
        .workspace_id
        .ok_or("delegate_to_agent requires a workspace context")?;
    let ws_slug = ctx.workspace_slug.as_deref().unwrap_or("");

    let pool = ctx.memory_store.pool();

    // Verify agent is in workspace
    let agent_row = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1 AND a.agent_name = $2",
    )
    .bind(ws_id)
    .bind(agent_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Agent '{}' is not in this workspace", agent_name))?;

    let target_agent_id: Uuid = agent_row.get("agent_id");
    let display: String = agent_row
        .try_get::<Option<String>, _>("display_alias")
        .unwrap_or(None)
        .unwrap_or_else(|| agent_name.to_string());

    // Post delegation message to workspace chat
    let delegation_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: ctx
            .current_agent_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        sender_name: Some(format!(
            "{} → {}",
            ctx.current_agent_id.map(|_| "compound").unwrap_or("system"),
            display
        )),
        content: format!("Delegating to {}: {}", display, task),
        message_type: "system_event".to_string(),
        metadata: json!({"delegation": true, "target": agent_name}),
        created_at: chrono::Utc::now(),
    };
    let _ = ctx
        .memory_store
        .store_workspace_message(&delegation_msg)
        .await;

    // Resolve agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent card not found: {}", e))?;

    // Enrich card with KG context from past dream cycles
    let card = crate::agent_backend::kg_context::enrich_with_kg_context(
        &ctx.memory_store,
        &ctx.embedder,
        target_agent_id,
        task,
        card,
    )
    .await;

    // Build execution context
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: task.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let context = ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
    };

    // Build a ToolAwareExecutor with workspace tools but NO delegation
    let tool_context = Arc::new(ToolContext {
        memory_store: ctx.memory_store.clone(),
        embedder: ctx.embedder.clone(),
        registry: ctx.registry.clone(),
        current_agent_id: Some(target_agent_id),
        workspace_id: Some(ws_id),
        workspace_slug: Some(ws_slug.to_string()),
        workspace_git: ctx.workspace_git.clone(),
        db: ctx.db.clone(),
        gas_fees: ctx.gas_fees.clone(),
        user_id: ctx.user_id.clone(),
        user_secrets: ctx.user_secrets.clone(),
    });

    let tool_executor = ToolAwareExecutor::new(
        ctx.registry.executor_arc(),
        ToolRegistry::with_workspace_no_delegation(),
        tool_context,
    );

    let output = tool_executor
        .execute(&stmt, &context)
        .await
        .map_err(|e| format!("Delegation failed: {}", e))?;

    // Post the result as a workspace message from the delegated agent
    let result_text = output
        .evidence
        .iter()
        .filter_map(|e| e.summary.clone())
        .collect::<Vec<_>>()
        .join("\n\n");

    let result_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: target_agent_id.to_string(),
        sender_name: Some(display.clone()),
        content: if result_text.is_empty() {
            "(no output)".to_string()
        } else {
            result_text.clone()
        },
        message_type: "execution_result".to_string(),
        metadata: json!({
            "delegated_by": ctx.current_agent_id,
            "tokens_used": output.tokens_used,
            "tool_invocations": output.tool_invocations.len(),
            "loop_iterations": output.loop_iterations,
        }),
        created_at: chrono::Utc::now(),
    };
    let _ = ctx.memory_store.store_workspace_message(&result_msg).await;

    // Return result to calling agent
    Ok(if result_text.is_empty() {
        format!("{} completed the delegation but produced no text output. Check workspace files for artifacts.", display)
    } else {
        result_text
    })
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

// ─── Voice synthesis tool ───────────────────────────────────────────

async fn execute_speak_text(input: &serde_json::Value) -> Result<String, String> {
    use crate::voice::{cartesia::VoiceStyle, CartesiaClient};

    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: text")?;

    if text.len() > 5000 {
        return Err("Text exceeds maximum length of 5000 characters".to_string());
    }

    let voice_str = input
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("narrator");

    let voice_style = match voice_str {
        "conversational" => VoiceStyle::Conversational,
        "storyteller" => VoiceStyle::Storyteller,
        _ => VoiceStyle::Narrator,
    };

    let api_key = std::env::var("CARTESIA_API_KEY")
        .map_err(|_| "CARTESIA_API_KEY not set — voice synthesis unavailable".to_string())?;

    let client = CartesiaClient::new(api_key);

    let audio_bytes = client
        .synthesize(text, voice_style)
        .await
        .map_err(|e| format!("Cartesia API error: {}", e))?;

    let duration_ms = client.estimate_duration_ms(text);

    // Encode as base64 for transport
    use base64::Engine;
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

    Ok(json!({
        "audio": audio_base64,
        "format": "pcm_f32le",
        "sample_rate": 44100,
        "duration_ms": duration_ms,
        "character_count": text.len(),
    })
    .to_string())
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

// ─── Football API ─────────────────────────────────────────────────

/// Call API-Football v3 (https://www.api-football.com/documentation-v3).
/// Requires FOOTBALL_API_KEY environment variable.
async fn execute_call_football_api(input: &serde_json::Value) -> Result<String, String> {
    let endpoint = input
        .get("endpoint")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: endpoint")?
        .trim_start_matches('/');

    let api_key = std::env::var("FOOTBALL_API_KEY")
        .map_err(|_| "FOOTBALL_API_KEY environment variable not set.".to_string())?;

    let client = reqwest::Client::new();
    let url = format!("https://v3.football.api-sports.io/{}", endpoint);

    let mut req = client
        .get(&url)
        .header("x-apisports-key", &api_key)
        .header("Accept", "application/json");

    // Apply query params from the `params` object
    if let Some(params) = input.get("params").and_then(|v| v.as_object()) {
        let query: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                (k.clone(), val)
            })
            .collect();
        req = req.query(&query);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("API-Football request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API-Football error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse API-Football response: {}", e))?;

    // Check API-level errors
    if let Some(errors) = data.get("errors") {
        if !errors.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            return Err(format!("API-Football errors: {}", errors));
        }
    }

    // Return the response, truncated if very large
    let result = serde_json::to_string_pretty(&data)
        .map_err(|e| format!("Serialization error: {}", e))?;

    if result.len() > 16000 {
        Ok(format!("{}... [truncated, {} total chars]", &result[..16000], result.len()))
    } else {
        Ok(result)
    }
}

// ─── Web Search ───────────────────────────────────────────────────

/// Search the web using the Brave Search API.
/// Requires BRAVE_SEARCH_API_KEY environment variable.
async fn execute_web_search(input: &serde_json::Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let count = input
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(10) as usize;
    let freshness = input
        .get("freshness")
        .and_then(|v| v.as_str());

    let api_key = std::env::var("BRAVE_SEARCH_API_KEY")
        .map_err(|_| "BRAVE_SEARCH_API_KEY environment variable not set. Get a free API key at https://brave.com/search/api/".to_string())?;

    let client = reqwest::Client::new();
    let mut req = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("Accept", "application/json")
        .header("X-Subscription-Token", &api_key)
        .query(&[("q", query), ("count", &count.to_string()), ("search_lang", "en")]);

    if let Some(f) = freshness {
        req = req.query(&[("freshness", f)]);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Brave Search request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Brave Search API error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Brave Search response: {}", e))?;

    let results = data
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array());

    let Some(results) = results else {
        return Ok("No web results found for this query.".to_string());
    };

    if results.is_empty() {
        return Ok("No web results found for this query.".to_string());
    }

    let mut output = format!("## Web Search Results for: {}\n\n", query);
    for (i, result) in results.iter().enumerate() {
        let title = result.get("title").and_then(|v| v.as_str()).unwrap_or("(no title)");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let description = result
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("(no description)");
        let age = result
            .get("age")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let published = result
            .get("page_age")
            .and_then(|v| v.as_str())
            .or(if age.is_empty() { None } else { Some(age) })
            .unwrap_or("(date unknown)");

        output.push_str(&format!(
            "**{}. {}**\n{}\n{}\n{}\n\n",
            i + 1, title, url, published, description
        ));
    }

    // Truncate to avoid context overflow
    if output.len() > 12_000 {
        output.truncate(12_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

// ─── Monte Carlo / FPL Simulation tools ───────────────────────────

/// Parse an FPL program string into an AST Program, returning a human-readable error on failure.
fn parse_fpl(source: &str) -> Result<crate::ast::Program, String> {
    let tokens = crate::lexer::Lexer::new(source)
        .tokenize()
        .map_err(|errs| {
            errs.iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    crate::parser::Parser::new(tokens)
        .parse()
        .map_err(|e| e.to_string())
}

/// Run a Monte Carlo simulation from an FPL program.
async fn execute_run_monte_carlo(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let mut executor = crate::executor::Executor::new(iterations);
    let results = executor
        .execute(&program)
        .map_err(|e| format!("Simulation error: {}", e))?;

    // Build a compact ASCII histogram (10 bins)
    let histogram = results.histogram(10);
    let max_count = histogram.iter().map(|(_, c)| *c).max().unwrap_or(1);
    let bar_width = 30usize;
    let mut hist_str = String::new();
    for (bin_start, count) in &histogram {
        let bar_len = (count * bar_width) / max_count;
        hist_str.push_str(&format!(
            "  {:>6.3} | {:<30} {}\n",
            bin_start,
            "#".repeat(bar_len),
            count
        ));
    }

    let result = json!({
        "iterations": results.iterations,
        "mean": results.mean,
        "median": results.median,
        "std_dev": results.std_dev,
        "min": results.min,
        "max": results.max,
        "percentiles": {
            "p5": results.p5,
            "p25": results.p25,
            "p75": results.p75,
            "p95": results.p95,
        },
        "base_rate": results.base_rate,
        "divergence_relative": results.divergence_relative,
        "divergence_absolute": results.divergence_absolute,
        "histogram_ascii": hist_str,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Run Sobol global sensitivity analysis on an FPL program.
async fn execute_run_sensitivity_analysis(input: &serde_json::Value) -> Result<String, String> {
    let source = input
        .get("fpl_program")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: fpl_program")?;

    let program = parse_fpl(source)?;

    let iterations = input
        .get("iterations")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let analysis = crate::sensitivity::full_sensitivity_analysis(&program, iterations)
        .map_err(|e| format!("Sensitivity analysis error: {}", e))?;

    // Build ranked driver list with indices
    let drivers: Vec<serde_json::Value> = analysis
        .ranked_drivers
        .iter()
        .filter_map(|name| analysis.driver_sensitivities.get(name))
        .map(|ds| {
            let ci_low = (ds.total_order_index - 1.96 * ds.standard_error).max(0.0);
            let ci_high = (ds.total_order_index + 1.96 * ds.standard_error).min(1.0);
            json!({
                "driver": ds.driver_name,
                "first_order_index": ds.first_order_index,
                "total_order_index": ds.total_order_index,
                "variance_contribution": ds.variance_contribution,
                "standard_error": ds.standard_error,
                "confidence_interval_95": [ci_low, ci_high],
            })
        })
        .collect();

    // ASCII tornado diagram
    let mut tornado = String::new();
    for ds in &drivers {
        let s_t = ds["total_order_index"].as_f64().unwrap_or(0.0);
        let bar_len = (s_t * 40.0) as usize;
        tornado.push_str(&format!(
            "  {:<30} | {:<40} {:.3}\n",
            ds["driver"].as_str().unwrap_or(""),
            "#".repeat(bar_len),
            s_t
        ));
    }

    let result = json!({
        "baseline": {
            "mean": analysis.baseline.mean,
            "std_dev": analysis.baseline.std_dev,
            "p5": analysis.baseline.p5,
            "p95": analysis.baseline.p95,
        },
        "drivers_ranked_by_total_order": drivers,
        "tornado_diagram_ascii": tornado,
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

// ─── Observability composition tools ───────────────────────────────
//
// Read-side wrappers around MemoryStore methods for the observability
// composition (observability_coordinator + eval_runner + anomaly_triager
// + dyad_observer). See docs/AGENT_MODEL.md §3 and §4.2.2.
//
// All six are pure reads — no gas charged, no writes. Action tools
// (run_evaluator_registry, route_to_hitl, classify_anomaly) will be
// added in a follow-up commit since they have larger blast radius.

fn parse_uuid_field(input: &serde_json::Value, field: &str) -> Result<Uuid, String> {
    let s = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {}", field))?;
    Uuid::parse_str(s).map_err(|e| format!("Invalid UUID for {}: {}", field, e))
}

async fn execute_query_eval_signals(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let run_id = parse_uuid_field(input, "run_id")?;
    let signals = ctx
        .memory_store
        .list_eval_signals_for_run(run_id)
        .await
        .map_err(|e| format!("Failed to list eval_signals: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "run_id": run_id,
        "count": signals.len(),
        "signals": signals,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_eval_runs(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = parse_uuid_field(input, "agent_id")?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 100);

    let runs = ctx
        .memory_store
        .list_eval_runs(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list eval_runs: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": runs.len(),
        "runs": runs,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_anomalies(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = parse_uuid_field(input, "agent_id")?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .clamp(1, 500);

    let events = ctx
        .memory_store
        .list_anomaly_events_for_agent(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list anomalies: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": events.len(),
        "anomalies": events,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_hitl_queue(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(50)
        .clamp(1, 200);

    let events = ctx
        .memory_store
        .list_pending_anomaly_events(limit)
        .await
        .map_err(|e| format!("Failed to list HITL queue: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "count": events.len(),
        "pending": events,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_timeline(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = parse_uuid_field(input, "agent_id")?;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(100)
        .clamp(1, 500);

    let entries = ctx
        .memory_store
        .list_timeline_entries(agent_id, limit)
        .await
        .map_err(|e| format!("Failed to list timeline: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": entries.len(),
        "timeline": entries,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_dyad_state(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_id = parse_uuid_field(input, "agent_id")?;

    let dyads = ctx
        .memory_store
        .list_dyads_for_agent(agent_id)
        .await
        .map_err(|e| format!("Failed to list dyads: {}", e))?;

    serde_json::to_string_pretty(&json!({
        "agent_id": agent_id,
        "count": dyads.len(),
        "dyads": dyads,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}
