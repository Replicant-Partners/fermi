// src/agent_backend/tools/domains/biology.rs
//
// Phase 2 domain migration: Biology tools.
//
// Seven tools:
//   gbif_species_search     — response_shape declared
//   gbif_taxonomy_tree      — response_shape declared
//   inat_observations
//   mycobank_lookup
//   ncbi_genome_search      — response_shape declared
//   generate_specimen_art
//   segment_creature_wings  — is_llm_visible: false
//
// Each is a zero-size struct implementing PlatformTool. execute() calls the
// implementation directly without going through ToolRegistry dispatch.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;
use crate::tool_response_shapes::{response_for, ToolResponse};
use sqlx::Row;
use uuid::Uuid;

/// All Biology-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(GbifSpeciesSearch),
        Arc::new(GbifTaxonomyTree),
        Arc::new(InatObservations),
        Arc::new(MycobankLookup),
        Arc::new(NcbiGenomeSearch),
        Arc::new(GenerateSpecimenArt),
        Arc::new(SegmentCreatureWings),
    ]
}

// ─── gbif_species_search ─────────────────────────────────────────────────────

struct GbifSpeciesSearch;

#[async_trait]
impl PlatformTool for GbifSpeciesSearch {
    fn name(&self) -> &'static str {
        "gbif_species_search"
    }

    fn description(&self) -> &'static str {
        "Call this tool to query GBIF (Global Biodiversity Information Facility) for species data. \
         This tool is executed server-side — you do not need internet access to use it. \
         Returns real taxonomy, common names, and media from the live GBIF API."
    }

    fn input_schema(&self) -> Value {
        json!({
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
                },
                "scope": {
                    "type": "string",
                    "description": "Named taxonomic scope to search within. One of: insecta (default), plantae, fungi, animalia, aves, lepidoptera, hymenoptera, magnoliopsida. Omit to keep the historical insect-only behaviour. An unrecognised name is an error, not a fallback."
                },
                "higher_taxon_key": {
                    "type": "integer",
                    "description": "GBIF backbone key to scope the search to, for a taxon `scope` does not name. Takes precedence over `scope`. Defaults to 216 (Insecta)."
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_gbif_species_search(input).await
    }
}

// ─── gbif_taxonomy_tree ──────────────────────────────────────────────────────

struct GbifTaxonomyTree;

#[async_trait]
impl PlatformTool for GbifTaxonomyTree {
    fn name(&self) -> &'static str {
        "gbif_taxonomy_tree"
    }

    fn description(&self) -> &'static str {
        "Call this tool to fetch the full taxonomic hierarchy for a species from GBIF. \
         This tool is executed server-side — you do not need internet access to use it. \
         Returns real kingdom-through-species data with GBIF keys, plus sibling taxa at each rank."
    }

    fn input_schema(&self) -> Value {
        json!({
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
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_gbif_taxonomy_tree(input).await
    }
}

// ─── inat_observations ───────────────────────────────────────────────────────

struct InatObservations;

#[async_trait]
impl PlatformTool for InatObservations {
    fn name(&self) -> &'static str {
        "inat_observations"
    }

    fn description(&self) -> &'static str {
        "Call this tool to query iNaturalist for recent species observations near a location. \
         Server-side — no API key required. Returns community observations with species, date, \
         photo, quality grade, and coordinates. Use for foraging scouting: what has been observed \
         in this area recently?"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "lat": {
                    "type": "number",
                    "description": "Latitude of search centre"
                },
                "lng": {
                    "type": "number",
                    "description": "Longitude of search centre"
                },
                "radius_km": {
                    "type": "number",
                    "description": "Search radius in kilometres (default: 5, max: 50)",
                    "default": 5
                },
                "taxon": {
                    "type": "string",
                    "description": "Iconic taxon filter: Fungi | Plantae | Animalia etc. (default: Fungi)",
                    "default": "Fungi"
                },
                "days_back": {
                    "type": "integer",
                    "description": "How many days back to search (default: 30, max: 365)",
                    "default": 30
                },
                "quality_grade": {
                    "type": "string",
                    "description": "Minimum quality grade: research | needs_id | casual (default: needs_id)",
                    "enum": ["research", "needs_id", "casual"],
                    "default": "needs_id"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default: 20, max: 50)",
                    "default": 20
                }
            },
            "required": ["lat", "lng"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_inat_observations(input).await
    }
}

// ─── mycobank_lookup ─────────────────────────────────────────────────────────

struct MycobankLookup;

#[async_trait]
impl PlatformTool for MycobankLookup {
    fn name(&self) -> &'static str {
        "mycobank_lookup"
    }

    fn description(&self) -> &'static str {
        "Call this tool to look up authoritative fungal nomenclature from MycoBank. Server-side. \
         Returns accepted name, nomenclatural status, taxonomic classification, synonyms, and \
         MycoBank number. Use for species validation and authoritative naming."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Fungal species name to look up (scientific name)"
                },
                "include_synonyms": {
                    "type": "boolean",
                    "description": "Include synonyms and basionyms in the response (default: true)",
                    "default": true
                }
            },
            "required": ["name"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_mycobank_lookup(input).await
    }
}

// ─── ncbi_genome_search ──────────────────────────────────────────────────────

struct NcbiGenomeSearch;

#[async_trait]
impl PlatformTool for NcbiGenomeSearch {
    fn name(&self) -> &'static str {
        "ncbi_genome_search"
    }

    fn description(&self) -> &'static str {
        "Look up assembled genome statistics for a species from NCBI Assembly: genome size in Mb \
         and assembled chromosome count, with the assembly name and accession that supplied them. \
         Returns found=false for unsequenced species — most insects — which is a real answer, \
         not an error."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "scientific_name": {
                    "type": "string",
                    "description": "Species binomial, e.g. 'Danaus plexippus'"
                }
            },
            "required": ["scientific_name"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    fn response_shape(&self) -> Option<&'static ToolResponse> {
        response_for(self.name())
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::agent_backend::ncbi_tools::execute_ncbi_genome_search(input).await
    }
}

// ─── generate_specimen_art ───────────────────────────────────────────────────

struct GenerateSpecimenArt;

#[async_trait]
impl PlatformTool for GenerateSpecimenArt {
    fn name(&self) -> &'static str {
        "generate_specimen_art"
    }

    fn description(&self) -> &'static str {
        "Generate a unique naturalist illustration for a creature using Gemini image generation. \
         Fetches GBIF reference media for the species, then generates a stylized scientific \
         illustration. Saves the image to static/creatures/ and updates the creature record."
    }

    fn input_schema(&self) -> Value {
        json!({
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
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_generate_specimen_art(input, ctx).await
    }
}

// ─── segment_creature_wings ──────────────────────────────────────────────────

struct SegmentCreatureWings;

#[async_trait]
impl PlatformTool for SegmentCreatureWings {
    fn name(&self) -> &'static str {
        "segment_creature_wings"
    }

    fn description(&self) -> &'static str {
        "Segment a butterfly creature's minted image into animation layers (body, left wing, right \
         wing) using Gemini image editing. Stores layers in the database for client-side parametric \
         wing animation. Only works for butterfly species. Costs creature_animate credits."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "creature_id": {
                    "type": "string",
                    "description": "UUID of the butterfly creature to segment into animation layers"
                }
            },
            "required": ["creature_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Biology
    }

    /// Not surfaced in the LLM tool list — invoked only by platform code.
    fn is_llm_visible(&self) -> bool {
        false
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_segment_creature_wings(input, ctx).await
    }
}

// ─── Private execute implementations ───────────────────────────────────────

pub(crate) async fn execute_gbif_taxonomy_tree(
    input: &serde_json::Value,
) -> Result<String, String> {
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

pub(crate) async fn execute_inat_observations(input: &serde_json::Value) -> Result<String, String> {
    let lat = input
        .get("lat")
        .and_then(|v| v.as_f64())
        .ok_or("lat is required")?;
    let lng = input
        .get("lng")
        .and_then(|v| v.as_f64())
        .ok_or("lng is required")?;
    let radius_km = input
        .get("radius_km")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0)
        .min(50.0);
    let taxon = input
        .get("taxon")
        .and_then(|v| v.as_str())
        .unwrap_or("Fungi");
    let days_back = input
        .get("days_back")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(365);
    let quality_grade = input
        .get("quality_grade")
        .and_then(|v| v.as_str())
        .unwrap_or("needs_id");
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .min(50);

    // Calculate date range
    let d1 = (chrono::Utc::now() - chrono::Duration::days(days_back as i64))
        .format("%Y-%m-%d")
        .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let url = "https://api.inaturalist.org/v2/observations";
    let resp = client
        .get(url)
        .header("User-Agent", "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)")
        .query(&[
            ("lat", lat.to_string()),
            ("lng", lng.to_string()),
            ("radius", radius_km.to_string()),
            ("iconic_taxa[]", taxon.to_string()),
            ("quality_grade", quality_grade.to_string()),
            ("d1", d1),
            ("order_by", "observed_on".to_string()),
            ("order", "desc".to_string()),
            ("per_page", limit.to_string()),
            ("fields", "taxon.name,taxon.preferred_common_name,observed_on,quality_grade,location,photos.url".to_string()),
        ])
        .send()
        .await
        .map_err(|e| format!("iNaturalist API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("iNaturalist API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse iNaturalist response: {}", e))?;

    let results = data
        .get("results")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default();
    let total = data
        .get("total_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // Summarise into a compact form for the agent
    let observations: Vec<serde_json::Value> = results
        .iter()
        .map(|obs| {
            let taxon_name = obs
                .pointer("/taxon/name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let common_name = obs
                .pointer("/taxon/preferred_common_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let date = obs
                .get("observed_on")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let grade = obs
                .get("quality_grade")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let location = obs.get("location").and_then(|v| v.as_str()).unwrap_or("");
            let has_photo = obs.pointer("/photos/0/url").is_some();
            json!({
                "species": taxon_name,
                "common_name": common_name,
                "observed_on": date,
                "quality_grade": grade,
                "location": location,
                "has_photo": has_photo,
            })
        })
        .collect();

    // Count unique species
    let mut species_counts: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
    for obs in &results {
        let name = obs
            .pointer("/taxon/name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        *species_counts.entry(name).or_insert(0) += 1;
    }
    let mut species_summary: Vec<(&str, u32)> =
        species_counts.iter().map(|(k, v)| (*k, *v)).collect();
    species_summary.sort_by(|a, b| b.1.cmp(&a.1));

    serde_json::to_string_pretty(&json!({
        "search_params": {
            "lat": lat, "lng": lng,
            "radius_km": radius_km,
            "taxon": taxon,
            "days_back": days_back,
            "quality_grade": quality_grade,
        },
        "total_observations": total,
        "returned": observations.len(),
        "species_summary": species_summary.iter().take(10).map(|(s, c)| json!({"species": s, "count": c})).collect::<Vec<_>>(),
        "observations": observations,
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

// Gemini API response types used by generate_specimen_art.
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
    #[allow(dead_code)]
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

// ─── GBIF scope constants & helpers ────────────────────────────────────────
//
// Inlined from tools_legacy.rs so that execute_gbif_species_search can live
// here without a legacy dependency.

/// Verified GBIF backbone keys for common higher-taxon scopes.
/// Rows: (name, key, rank). All verified against the live API on 2026-08-17.
const GBIF_SCOPES: &[(&str, i64, &str)] = &[
    ("insecta", 216, "CLASS"),
    ("plantae", 6, "KINGDOM"),
    ("fungi", 5, "KINGDOM"),
    ("animalia", 1, "KINGDOM"),
    ("aves", 212, "CLASS"),
    ("lepidoptera", 797, "ORDER"),
    ("hymenoptera", 1457, "ORDER"),
    ("magnoliopsida", 220, "CLASS"),
];

/// Default higher-taxon scope: Insecta (216).
///
/// Historical default kept for backward-compat — changing it would silently
/// widen every existing caller's search.
const GBIF_DEFAULT_SCOPE_KEY: i64 = 216;

/// Resolve the `highertaxonKey` filter for a GBIF name search.
///
/// Precedence: explicit `higher_taxon_key` → named `scope` → Insecta default.
/// An unrecognised `scope` is an error, not a silent fallback.
fn gbif_higher_taxon_key(input: &serde_json::Value) -> Result<i64, String> {
    if let Some(k) = input.get("higher_taxon_key").and_then(|v| v.as_i64()) {
        return Ok(k);
    }
    match input.get("scope").and_then(|v| v.as_str()) {
        None => Ok(GBIF_DEFAULT_SCOPE_KEY),
        Some(name) => {
            let wanted = name.trim().to_ascii_lowercase();
            GBIF_SCOPES
                .iter()
                .find(|(n, _, _)| *n == wanted)
                .map(|(_, k, _)| *k)
                .ok_or_else(|| {
                    let known: Vec<&str> = GBIF_SCOPES.iter().map(|(n, _, _)| *n).collect();
                    format!(
                        "unknown scope `{name}`. Known scopes: {}. Or pass \
                         `higher_taxon_key` with a GBIF backbone key directly.",
                        known.join(", ")
                    )
                })
        }
    }
}

/// Pick the most-frequently-cited English vernacular name from a GBIF search
/// result's `vernacularNames` array. Ties keep the earliest-seen casing.
fn gbif_preferred_vernacular(species: &serde_json::Value, language: &str) -> Option<String> {
    let list = species.get("vernacularNames")?.as_array()?;
    let mut tally: Vec<(String, usize, String)> = Vec::new();
    for entry in list {
        if entry.get("language").and_then(|v| v.as_str()) != Some(language) {
            continue;
        }
        let Some(raw) = entry.get("vernacularName").and_then(|v| v.as_str()) else {
            continue;
        };
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        match tally.iter_mut().find(|(k, _, _)| *k == key) {
            Some(slot) => slot.1 += 1,
            None => tally.push((key, 1, name.to_string())),
        }
    }
    let mut best: Option<&(String, usize, String)> = None;
    for row in &tally {
        if best.is_none_or(|b| row.1 > b.1) {
            best = Some(row);
        }
    }
    best.map(|(_, _, original)| original.clone())
}

// ─── Context-free execute implementations ────────────────────────────────────

/// Look up a species (or search by name) on GBIF.
///
/// Keyless HTTP call — no `ToolContext` required.
/// `pub(crate)` so `tools/mod.rs::execute_context_free` and
/// `field_probe` handlers can call it directly.
pub async fn execute_gbif_species_search(
    input: &serde_json::Value,
) -> Result<String, String> {
    use serde_json::json;
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
    let higher_taxon = gbif_higher_taxon_key(input)?.to_string();
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.gbif.org/v1/species/search")
        .query(&[
            ("q", query),
            ("rank", rank),
            ("limit", limit_str.as_str()),
            ("highertaxonKey", higher_taxon.as_str()),
        ])
        .header("User-Agent", "AgentBestiaryWorld/1.0 (rabble.world)")
        .send()
        .await
        .map_err(|e| format!("GBIF request failed: {}", e))?;

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

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
                "vernacularName": gbif_preferred_vernacular(&s, "eng"),
                "vernacularNameLanguage": "eng",
                "vernacularNamesEnglish": s
                    .get("vernacularNames")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        let mut seen: Vec<String> = Vec::new();
                        for e in a {
                            if e.get("language").and_then(|v| v.as_str()) != Some("eng") {
                                continue;
                            }
                            if let Some(n) = e.get("vernacularName").and_then(|v| v.as_str()) {
                                let n = n.trim().to_string();
                                if !n.is_empty() && !seen.iter().any(|x| x.eq_ignore_ascii_case(&n))
                                {
                                    seen.push(n);
                                }
                            }
                        }
                        seen.truncate(8);
                        seen
                    }),
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

/// Look up authoritative fungal nomenclature from MycoBank (or GBIF fallback).
///
/// Keyless HTTP call — no `ToolContext` required.
/// `pub(crate)` so `tools/mod.rs::execute_context_free` and
/// `field_probe` handlers can call it directly.
pub async fn execute_mycobank_lookup(input: &serde_json::Value) -> Result<String, String> {
    use serde_json::json;
    let name = input
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("name is required")?;
    let include_synonyms = input
        .get("include_synonyms")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let api_key = std::env::var("MYCOBANK_API_KEY").unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    // If no API key, fall back to GBIF for fungal taxonomy
    if api_key.is_empty() {
        let gbif_url = "https://api.gbif.org/v1/species/match";
        let resp = client
            .get(gbif_url)
            .header(
                "User-Agent",
                "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)",
            )
            .query(&[("name", name), ("kingdom", "Fungi"), ("verbose", "true")])
            .send()
            .await
            .map_err(|e| format!("GBIF fallback request failed: {}", e))?;

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse GBIF response: {}", e))?;

        return serde_json::to_string_pretty(&json!({
            "source": "GBIF (MycoBank API key not configured)",
            "query": name,
            "accepted_name": data.get("species").or_else(|| data.get("canonicalName")),
            "status": data.get("status"),
            "rank": data.get("rank"),
            "kingdom": data.get("kingdom"),
            "phylum": data.get("phylum"),
            "class": data.get("class"),
            "order": data.get("order"),
            "family": data.get("family"),
            "genus": data.get("genus"),
            "gbif_key": data.get("speciesKey").or_else(|| data.get("usageKey")),
            "confidence": data.get("confidence"),
            "note": "Configure MYCOBANK_API_KEY for authoritative MycoBank nomenclature"
        }))
        .map_err(|e| format!("Serialization error: {}", e));
    }

    // MycoBank API
    let base_url = "https://webservices.bio-aware.com/cbsdatabase_new/mycobank/taxonnames";
    let resp = client
        .get(base_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header(
            "User-Agent",
            "AgentBestiaryWorld/1.0 (kask.bio/projects/wild)",
        )
        .query(&[("filter", format!("name startWith '{}'", name))])
        .send()
        .await
        .map_err(|e| format!("MycoBank API request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("MycoBank API error: {}", resp.status()));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse MycoBank response: {}", e))?;

    let items = data
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if items.is_empty() {
        return Ok(serde_json::to_string_pretty(&json!({
            "source": "MycoBank",
            "query": name,
            "found": false,
            "message": "No records found in MycoBank for this name"
        }))
        .unwrap_or_default());
    }

    // Find the best match — prefer exact name match with valid status
    let best = items
        .iter()
        .find(|item| {
            item.get("name")
                .and_then(|v| v.as_str())
                .map(|n| n.to_lowercase() == name.to_lowercase())
                .unwrap_or(false)
                && item
                    .get("nameStatus")
                    .and_then(|v| v.as_str())
                    .map(|s| s != "Illegitimate" && s != "Invalid")
                    .unwrap_or(true)
        })
        .or_else(|| items.first());

    let result = best.cloned().unwrap_or(json!({}));

    serde_json::to_string_pretty(&json!({
        "source": "MycoBank",
        "query": name,
        "found": true,
        "mycobank_number": result.get("mycobankNr"),
        "accepted_name": result.pointer("/synonymy/currentName").or_else(|| result.get("name")),
        "name_status": result.get("nameStatus"),
        "author": result.get("authors"),
        "year": result.get("year"),
        "rank": result.get("rank"),
        "phylum": result.pointer("/classification/phylum"),
        "class": result.pointer("/classification/class"),
        "order": result.pointer("/classification/order"),
        "family": result.pointer("/classification/family"),
        "genus": result.pointer("/classification/genus"),
        "synonyms_count": if include_synonyms { items.len() } else { 0 },
        "url": result.get("mycobankNr").and_then(|n| n.as_str())
            .map(|n| format!("https://www.mycobank.org/page/Name%20details%20page/field/Mycobank%20%23/{}", n)),
    }))
    .map_err(|e| format!("Serialization error: {}", e))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_biology() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Biology,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_seven() {
        assert_eq!(tools().len(), 7);
    }

    #[test]
    fn segment_creature_wings_is_not_llm_visible() {
        let tool = SegmentCreatureWings;
        assert!(
            !tool.is_llm_visible(),
            "segment_creature_wings must not be visible to the LLM"
        );
    }

    #[test]
    fn response_shape_tools_are_declared() {
        let with_shapes = [
            "gbif_species_search",
            "gbif_taxonomy_tree",
            "ncbi_genome_search",
        ];
        for tool in tools() {
            let has_shape = tool.response_shape().is_some();
            let expected = with_shapes.contains(&tool.name());
            assert_eq!(
                has_shape,
                expected,
                "tool `{}`: response_shape presence mismatch (expected: {expected})",
                tool.name()
            );
        }
    }
}
