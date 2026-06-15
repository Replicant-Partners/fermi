// ─────────────────────────────────────────────────────────────────────
// Embedding generators — Spec 22 (Embedding Portability)
//
// Production embedding identity (as of 2026-06-11, when Spec 22 landed):
//   provider: anthropic
//   model:    voyage-2
//   dim:      1024
//   version:  manual epoch "2024-01-01" (see VOYAGE_MODEL_VERSION below)
//
// All embeddings in the database written BEFORE Spec 22 are stamped by the
// backfill migration with:
//   model_id            = "anthropic/voyage-2"
//   model_version       = "unknown_pre_provenance"
//   provenance_trusted  = false   (we can't prove what produced them)
//
// All embeddings written AFTER Spec 22 land go through
// `EmbeddingGenerator::generate_provenanced()` which returns the vector
// bundled with the model identity that produced it. Storing fns accept the
// `ProvenancedEmbedding` struct and persist provenance in the same
// transaction as the vector — see docs/specs/22_EMBEDDING_PORTABILITY_SPEC.md.
//
// MODEL VERSION POLICY: Vendors (Voyage, OpenAI, Mistral, Qwen) do not
// expose stable embedding-model version strings via their APIs. We capture
// `model_version` as a manual "YYYY-MM-DD" epoch managed by the constants
// below. Bump these constants when:
//   (a) we observe quality drift on benchmarks
//   (b) a vendor announces a model update
//   (c) we switch to a measurably different model snapshot
// Bumping a version string is the trigger for the re-embed worker (Phase 3
// of Spec 22) to refresh existing vectors stamped with the old version.
// ─────────────────────────────────────────────────────────────────────

use crate::{MemoryError, Result};
use serde::{Deserialize, Serialize};

// Manual epoch versions for embedding models. See "MODEL VERSION POLICY"
// above for the bump rules.
pub const VOYAGE_MODEL_VERSION: &str = "2024-01-01";
pub const OPENAI_EMBED_VERSION: &str = "2024-01-01";
pub const MISTRAL_EMBED_VERSION: &str = "2024-01-01";
pub const QWEN_EMBED_VERSION: &str = "2024-01-01";
pub const MOCK_EMBED_VERSION: &str = "mock-v1";
// Spec 22 Phase 2.0: reference open model for the closed-model anchor set.
// `nomic-embed-text-v1.5` is the canonical reference embedder — fully open
// weights, self-hostable, no vendor dependency. Version bumped when the
// underlying model weights change (rare; the v1.5 release has been stable
// since 2024-02).
pub const NOMIC_EMBED_VERSION: &str = "v1.5-2024-02-15";

/// A vector bundled with the full provenance required by Spec 22.
///
/// This is the only type that storing fns accept for persistence. The
/// compiler enforces the discipline that the spec calls "no code path
/// that skips provenance": you cannot write an embedding to the DB
/// without going through `generate_provenanced()` (or supplying every
/// provenance field manually, in which case you're explicitly opting in).
#[derive(Debug, Clone)]
pub struct ProvenancedEmbedding {
    /// The vector itself.
    pub vector: Vec<f32>,
    /// The EXACT text that was passed to the embedder. Not a reconstruction.
    /// This is the asset; the vector is derived from it.
    pub source_text: String,
    /// Globally unique model identifier in the form "<provider>/<model>",
    /// e.g. "anthropic/voyage-2". Stable across restarts; matches the
    /// `model_id` column persisted in `embedding_provenance`.
    pub model_id: String,
    /// Manual epoch version string, format "YYYY-MM-DD" or "mock-v1".
    /// Bumped in code when we suspect or observe vendor drift.
    pub model_version: String,
    /// Output dimensionality. Guards against silent model swaps; MUST
    /// equal `vector.len()`.
    pub dim: i32,
}

/// Embedding generator trait.
///
/// Implementors MUST expose a stable `model_id()` / `model_version()` /
/// `dim()` triple. The trait's default `generate_provenanced()` impl
/// bundles those with the vector returned by `generate()`.
#[async_trait::async_trait]
pub trait EmbeddingGenerator: Send + Sync {
    /// Generate a single embedding. Read-only call sites (query
    /// embeddings, not persisted) may use this directly. Persisting call
    /// sites MUST use `generate_provenanced()` instead.
    async fn generate(&self, text: &str) -> Result<Vec<f32>>;

    /// Batch generation. Same caveat: results that will be persisted
    /// should go through a provenance-aware wrapper.
    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;

    /// Output dimensionality. MUST equal the length of every vector
    /// returned by `generate()` / `generate_batch()`.
    fn dimension(&self) -> usize;

    /// Globally unique model identifier in the form "<provider>/<model>".
    /// Persisted as `embedding_model_id` on every embedded row.
    fn model_id(&self) -> &str;

    /// Manual epoch version string. Persisted as `embedding_model_version`
    /// on every embedded row. Bumped in code on observed drift.
    fn model_version(&self) -> &str;

    /// Convenience: bundles the vector with full provenance. Prefer this
    /// over `generate()` at call sites that persist the result.
    async fn generate_provenanced(&self, text: &str) -> Result<ProvenancedEmbedding> {
        let vector = self.generate(text).await?;
        Ok(ProvenancedEmbedding {
            vector,
            source_text: text.to_string(),
            model_id: self.model_id().to_string(),
            model_version: self.model_version().to_string(),
            dim: self.dimension() as i32,
        })
    }

    /// Batch convenience. Returns one `ProvenancedEmbedding` per input
    /// text in the same order.
    async fn generate_provenanced_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<ProvenancedEmbedding>> {
        let vectors = self.generate_batch(texts).await?;
        if vectors.len() != texts.len() {
            return Err(MemoryError::InvalidData(format!(
                "embedder returned {} vectors for {} inputs",
                vectors.len(),
                texts.len()
            )));
        }
        let model_id = self.model_id().to_string();
        let model_version = self.model_version().to_string();
        let dim = self.dimension() as i32;
        Ok(texts
            .iter()
            .zip(vectors.into_iter())
            .map(|(text, vector)| ProvenancedEmbedding {
                vector,
                source_text: text.clone(),
                model_id: model_id.clone(),
                model_version: model_version.clone(),
                dim,
            })
            .collect())
    }
}

/// Anthropic embedding generator (Voyage models via Anthropic's API surface).
pub struct AnthropicEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    model_id_cached: String,
    client: reqwest::Client,
}

impl AnthropicEmbeddings {
    pub fn new(api_key: String) -> Self {
        let model = "voyage-2".to_string();
        let model_id_cached = format!("anthropic/{}", model);
        Self {
            api_key,
            model,
            dimension: 1024,
            model_id_cached,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
        self.model_id_cached = format!("anthropic/{}", model);
        self.model = model;
        self.dimension = dimension;
        self
    }
}

#[derive(Serialize)]
struct AnthropicEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct AnthropicEmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

#[async_trait::async_trait]
impl EmbeddingGenerator for AnthropicEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.generate_batch(&[text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = AnthropicEmbeddingRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/embeddings")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::InvalidData(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let data: AnthropicEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("Failed to parse response: {}", e)))?;

        Ok(data.embeddings)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id_cached
    }

    fn model_version(&self) -> &str {
        VOYAGE_MODEL_VERSION
    }
}

/// OpenAI embedding generator.
pub struct OpenAIEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    model_id_cached: String,
    client: reqwest::Client,
}

impl OpenAIEmbeddings {
    pub fn new(api_key: String) -> Self {
        let model = "text-embedding-3-large".to_string();
        let model_id_cached = format!("openai/{}", model);
        Self {
            api_key,
            model,
            dimension: 1024,
            model_id_cached,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
        self.model_id_cached = format!("openai/{}", model);
        self.model = model;
        self.dimension = dimension;
        self
    }
}

#[derive(Serialize)]
struct OpenAIEmbeddingRequest {
    input: Vec<String>,
    model: String,
    dimensions: Option<usize>,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<OpenAIEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAIEmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingGenerator for OpenAIEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.generate_batch(&[text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = OpenAIEmbeddingRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
            dimensions: Some(self.dimension),
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::InvalidData(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let data: OpenAIEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("Failed to parse response: {}", e)))?;

        Ok(data.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id_cached
    }

    fn model_version(&self) -> &str {
        OPENAI_EMBED_VERSION
    }
}

/// Mistral embedding generator.
pub struct MistralEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    model_id_cached: String,
    client: reqwest::Client,
}

impl MistralEmbeddings {
    pub fn new(api_key: String) -> Self {
        let model = "mistral-embed".to_string();
        let model_id_cached = format!("mistral/{}", model);
        Self {
            api_key,
            model,
            dimension: 1024,
            model_id_cached,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
        self.model_id_cached = format!("mistral/{}", model);
        self.model = model;
        self.dimension = dimension;
        self
    }
}

#[derive(Serialize)]
struct MistralEmbeddingRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct MistralEmbeddingResponse {
    data: Vec<MistralEmbeddingData>,
}

#[derive(Deserialize)]
struct MistralEmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingGenerator for MistralEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.generate_batch(&[text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = MistralEmbeddingRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
        };

        let response = self
            .client
            .post("https://api.mistral.ai/v1/embeddings")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::InvalidData(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let data: MistralEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("Failed to parse response: {}", e)))?;

        Ok(data.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id_cached
    }

    fn model_version(&self) -> &str {
        MISTRAL_EMBED_VERSION
    }
}

/// Qwen embedding generator.
///
/// Drive-by fix from Spec 22: the previous `provider_name()` returned
/// `"qwen/text-embedding-v2"` while the default model was `text-embedding-v3`,
/// a latent mismatch. The new `model_id()` derives directly from `self.model`
/// so the mismatch is impossible.
pub struct QwenEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    model_id_cached: String,
    client: reqwest::Client,
}

impl QwenEmbeddings {
    pub fn new(api_key: String) -> Self {
        let model = "text-embedding-v3".to_string();
        let model_id_cached = format!("qwen/{}", model);
        Self {
            api_key,
            model,
            dimension: 1024,
            model_id_cached,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
        self.model_id_cached = format!("qwen/{}", model);
        self.model = model;
        self.dimension = dimension;
        self
    }
}

#[derive(Serialize)]
struct QwenEmbeddingRequest {
    input: QwenInput,
    model: String,
}

#[derive(Serialize)]
struct QwenInput {
    texts: Vec<String>,
}

#[derive(Deserialize)]
struct QwenEmbeddingResponse {
    output: QwenOutput,
}

#[derive(Deserialize)]
struct QwenOutput {
    embeddings: Vec<QwenEmbeddingData>,
}

#[derive(Deserialize)]
struct QwenEmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingGenerator for QwenEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.generate_batch(&[text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = QwenEmbeddingRequest {
            input: QwenInput {
                texts: texts.to_vec(),
            },
            model: self.model.clone(),
        };

        let response = self
            .client
            .post("https://dashscope.aliyuncs.com/api/v1/services/embeddings/text-embedding/text-embedding")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("API request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::InvalidData(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let data: QwenEmbeddingResponse = response
            .json()
            .await
            .map_err(|e| MemoryError::InvalidData(format!("Failed to parse response: {}", e)))?;

        Ok(data
            .output
            .embeddings
            .into_iter()
            .map(|d| d.embedding)
            .collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id_cached
    }

    fn model_version(&self) -> &str {
        QWEN_EMBED_VERSION
    }
}

/// Nomic embedding generator — the reference OPEN model for Spec 22 anchors.
///
/// Speaks the OpenAI-compatible embeddings API protocol. Works against any
/// endpoint that implements it:
///   - Ollama (`ollama run nomic-embed-text`) — default endpoint
///     `http://localhost:11434/v1/embeddings`
///   - Local Python sidecar (FastAPI + sentence-transformers)
///   - Self-hosted vLLM / llama.cpp server
///
/// **NOT** intended for the production hot path. This client is used only by
/// the anchor refresh worker (Phase 2.3) and the Tier 2 translator (Phase 6).
/// The reference model exists so we can co-embed anchor texts against both
/// vendor models and our open reference; if a vendor goes dark, the
/// reference-side embeddings remain available and let us fit a translator.
///
/// Per Spec 22 §2.0: `model_id = "nomic/embed-text-v1.5"`, dim = 768.
pub struct NomicEmbeddings {
    base_url: String,
    api_key: Option<String>,
    model: String,
    dimension: usize,
    model_id_cached: String,
    client: reqwest::Client,
}

impl NomicEmbeddings {
    /// Construct with explicit base URL.
    ///
    /// `base_url` should be the FULL endpoint, e.g.
    /// `"http://localhost:11434/v1/embeddings"`. `api_key` is optional and only
    /// used if the endpoint requires bearer auth (most self-hosted deployments
    /// don't).
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        let model = "nomic-embed-text".to_string();
        let model_id_cached = "nomic/embed-text-v1.5".to_string();
        Self {
            base_url: base_url.into(),
            api_key,
            model,
            dimension: 768,
            model_id_cached,
            client: reqwest::Client::new(),
        }
    }

    /// Construct from environment variables:
    ///   - `NOMIC_BASE_URL` (default: `http://localhost:11434/v1/embeddings`)
    ///   - `NOMIC_API_KEY`  (optional)
    pub fn from_env() -> Self {
        let base_url = std::env::var("NOMIC_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:11434/v1/embeddings".to_string());
        let api_key = std::env::var("NOMIC_API_KEY").ok();
        Self::new(base_url, api_key)
    }

    /// Override the model name (e.g. `"nomic-embed-text-v1"` for the older
    /// snapshot). `model_id` is automatically prefixed with `"nomic/"`.
    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
        self.model_id_cached = format!("nomic/{}", model);
        self.model = model;
        self.dimension = dimension;
        self
    }
}

// OpenAI-compatible request/response shapes. Ollama, vLLM, llama.cpp server
// and most Python sidecars implement this protocol.
#[derive(Serialize)]
struct NomicEmbedRequest {
    input: Vec<String>,
    model: String,
}

#[derive(Deserialize)]
struct NomicEmbedResponse {
    data: Vec<NomicEmbedDatum>,
}

#[derive(Deserialize)]
struct NomicEmbedDatum {
    embedding: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingGenerator for NomicEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.generate_batch(&[text.to_string()]).await?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let request = NomicEmbedRequest {
            input: texts.to_vec(),
            model: self.model.clone(),
        };

        let mut req_builder = self.client.post(&self.base_url).json(&request);
        if let Some(key) = &self.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", key));
        }

        let response = req_builder.send().await.map_err(|e| {
            MemoryError::InvalidData(format!("Nomic API request failed: {}", e))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(MemoryError::InvalidData(format!(
                "Nomic API error {} from {}: {}",
                status, self.base_url, body
            )));
        }

        let data: NomicEmbedResponse = response.json().await.map_err(|e| {
            MemoryError::InvalidData(format!("Failed to parse Nomic response: {}", e))
        })?;

        if data.data.is_empty() {
            return Err(MemoryError::InvalidData(
                "Nomic API returned no embeddings".to_string(),
            ));
        }

        Ok(data.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        &self.model_id_cached
    }

    fn model_version(&self) -> &str {
        NOMIC_EMBED_VERSION
    }
}

/// Mock embedding generator for testing.
///
/// Produces deterministic hash-based vectors. The `model_id()` is
/// distinguishable from any real provider so backfill / provenance code
/// can correctly avoid stamping Mock-generated rows as if they came from
/// a real vendor.
pub struct MockEmbeddings {
    dimension: usize,
}

impl MockEmbeddings {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }
}

#[async_trait::async_trait]
impl EmbeddingGenerator for MockEmbeddings {
    async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        // Generate deterministic mock embedding based on text hash
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        let embedding: Vec<f32> = (0..self.dimension)
            .map(|i| ((hash.wrapping_add(i as u64)) % 1000) as f32 / 1000.0)
            .collect();

        Ok(embedding)
    }

    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut embeddings = Vec::new();
        for text in texts {
            embeddings.push(self.generate(text).await?);
        }
        Ok(embeddings)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn model_id(&self) -> &str {
        "mock/deterministic-hash"
    }

    fn model_version(&self) -> &str {
        MOCK_EMBED_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_embeddings() {
        let generator = MockEmbeddings::new(1024);

        let embedding = generator.generate("test query").await.unwrap();
        assert_eq!(embedding.len(), 1024);

        // Should be deterministic
        let embedding2 = generator.generate("test query").await.unwrap();
        assert_eq!(embedding, embedding2);

        // Different text should give different embedding
        let embedding3 = generator.generate("different query").await.unwrap();
        assert_ne!(embedding, embedding3);
    }

    #[tokio::test]
    async fn test_mock_batch_embeddings() {
        let generator = MockEmbeddings::new(512);

        let texts = vec!["query 1".to_string(), "query 2".to_string()];
        let embeddings = generator.generate_batch(&texts).await.unwrap();

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), 512);
        assert_eq!(embeddings[1].len(), 512);
        assert_ne!(embeddings[0], embeddings[1]);
    }

    #[tokio::test]
    async fn test_mock_provenance_bundling() {
        let generator = MockEmbeddings::new(1024);
        let p = generator.generate_provenanced("hello").await.unwrap();
        assert_eq!(p.vector.len(), 1024);
        assert_eq!(p.dim, 1024);
        assert_eq!(p.source_text, "hello");
        assert_eq!(p.model_id, "mock/deterministic-hash");
        assert_eq!(p.model_version, MOCK_EMBED_VERSION);
    }

    #[tokio::test]
    async fn test_mock_provenance_batch_alignment() {
        let generator = MockEmbeddings::new(64);
        let texts = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let provenanced = generator.generate_provenanced_batch(&texts).await.unwrap();
        assert_eq!(provenanced.len(), 3);
        for (p, t) in provenanced.iter().zip(texts.iter()) {
            assert_eq!(p.source_text, *t);
            assert_eq!(p.vector.len(), 64);
            assert_eq!(p.dim, 64);
        }
    }

    #[test]
    fn test_nomic_defaults() {
        let nomic = NomicEmbeddings::new("http://localhost:11434/v1/embeddings", None);
        assert_eq!(nomic.model_id(), "nomic/embed-text-v1.5");
        assert_eq!(nomic.dimension(), 768);
        assert_eq!(nomic.model_version(), NOMIC_EMBED_VERSION);
    }

    #[test]
    fn test_nomic_with_model_overrides_id() {
        let nomic = NomicEmbeddings::new("http://x", None)
            .with_model("nomic-embed-text-v1".to_string(), 768);
        assert_eq!(nomic.model_id(), "nomic/nomic-embed-text-v1");
    }

    #[test]
    fn test_model_id_provider_prefix() {
        // Mocks the real impls without API keys: verify cached model_id wiring.
        let voyage = AnthropicEmbeddings::new("sk-fake".to_string());
        assert_eq!(voyage.model_id(), "anthropic/voyage-2");

        let openai = OpenAIEmbeddings::new("sk-fake".to_string());
        assert_eq!(openai.model_id(), "openai/text-embedding-3-large");

        let mistral = MistralEmbeddings::new("sk-fake".to_string());
        assert_eq!(mistral.model_id(), "mistral/mistral-embed");

        let qwen = QwenEmbeddings::new("sk-fake".to_string());
        // Drive-by fix: previously returned "qwen/text-embedding-v2" while the
        // default model was v3. Now derived from self.model and always consistent.
        assert_eq!(qwen.model_id(), "qwen/text-embedding-v3");
    }

    #[test]
    fn test_with_model_updates_model_id() {
        let voyage = AnthropicEmbeddings::new("k".to_string())
            .with_model("voyage-3-large".to_string(), 2048);
        assert_eq!(voyage.model_id(), "anthropic/voyage-3-large");
        assert_eq!(voyage.dimension(), 2048);
    }
}
