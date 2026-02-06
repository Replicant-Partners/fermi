use crate::{MemoryError, Result};
use serde::{Deserialize, Serialize};

/// Embedding generator trait
#[async_trait::async_trait]
pub trait EmbeddingGenerator: Send + Sync {
    async fn generate(&self, text: &str) -> Result<Vec<f32>>;
    async fn generate_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

/// Anthropic embedding generator
pub struct AnthropicEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    client: reqwest::Client,
}

impl AnthropicEmbeddings {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "voyage-2".to_string(), // Anthropic's embedding model
            dimension: 1024,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
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
}

/// OpenAI embedding generator (alternative)
pub struct OpenAIEmbeddings {
    api_key: String,
    model: String,
    dimension: usize,
    client: reqwest::Client,
}

impl OpenAIEmbeddings {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            model: "text-embedding-3-large".to_string(),
            dimension: 1024,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_model(mut self, model: String, dimension: usize) -> Self {
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
}

/// Mock embedding generator for testing
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

        let mut embedding = vec![0.0; self.dimension];
        for i in 0..self.dimension {
            embedding[i] = ((hash.wrapping_add(i as u64)) % 1000) as f32 / 1000.0;
        }

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
}
