use crate::{Episode, Result};
use std::collections::HashMap;
use uuid::Uuid;

/// DBSCAN cluster result
#[derive(Debug, Clone)]
pub struct EpisodeCluster {
    pub cluster_id: Uuid,
    pub episodes: Vec<Episode>,
    pub centroid: Option<Vec<f32>>,
}

/// DBSCAN clustering for episodes
pub struct DBSCANClustering {
    epsilon: f64,       // Maximum distance between two points to be neighbors
    min_samples: usize, // Minimum number of points to form a dense region
}

impl DBSCANClustering {
    pub fn new(epsilon: f64, min_samples: usize) -> Self {
        Self {
            epsilon,
            min_samples,
        }
    }

    /// Compute cosine distance between two embeddings
    fn cosine_distance(a: &[f32], b: &[f32]) -> f64 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            return 1.0; // Maximum distance
        }

        let cosine_similarity = dot_product / (mag_a * mag_b);
        (1.0 - cosine_similarity as f64).max(0.0) // Convert to distance
    }

    /// Find neighbors of a point within epsilon distance
    fn find_neighbors(&self, episodes: &[Episode], point_idx: usize) -> Vec<usize> {
        let point_embedding = match &episodes[point_idx].embedding {
            Some(emb) => emb,
            None => return vec![],
        };

        let mut neighbors = Vec::new();

        for (idx, episode) in episodes.iter().enumerate() {
            if idx == point_idx {
                continue;
            }

            if let Some(embedding) = &episode.embedding {
                let distance = Self::cosine_distance(point_embedding, embedding);
                if distance <= self.epsilon {
                    neighbors.push(idx);
                }
            }
        }

        neighbors
    }

    /// Perform DBSCAN clustering
    pub fn cluster(&self, episodes: Vec<Episode>) -> Result<Vec<EpisodeCluster>> {
        let n = episodes.len();

        // Track cluster assignments (-1 = noise, 0+ = cluster id)
        let mut cluster_ids = vec![-1i32; n];
        let mut visited = vec![false; n];
        let mut current_cluster_id = 0i32;

        for point_idx in 0..n {
            if visited[point_idx] {
                continue;
            }

            visited[point_idx] = true;
            let neighbors = self.find_neighbors(&episodes, point_idx);

            // If not enough neighbors, mark as noise
            if neighbors.len() < self.min_samples {
                cluster_ids[point_idx] = -1; // Noise
                continue;
            }

            // Start a new cluster
            cluster_ids[point_idx] = current_cluster_id;

            // Expand cluster
            let mut seed_set = neighbors.clone();
            let mut i = 0;

            while i < seed_set.len() {
                let neighbor_idx = seed_set[i];

                if !visited[neighbor_idx] {
                    visited[neighbor_idx] = true;
                    let neighbor_neighbors = self.find_neighbors(&episodes, neighbor_idx);

                    if neighbor_neighbors.len() >= self.min_samples {
                        // Add new neighbors to seed set
                        for &nn in &neighbor_neighbors {
                            if !seed_set.contains(&nn) {
                                seed_set.push(nn);
                            }
                        }
                    }
                }

                // Add to cluster if not already assigned
                if cluster_ids[neighbor_idx] == -1 {
                    cluster_ids[neighbor_idx] = current_cluster_id;
                }

                i += 1;
            }

            current_cluster_id += 1;
        }

        // Group episodes by cluster
        let mut cluster_map: HashMap<i32, Vec<Episode>> = HashMap::new();

        for (idx, &cluster_id) in cluster_ids.iter().enumerate() {
            if cluster_id >= 0 {
                cluster_map
                    .entry(cluster_id)
                    .or_default()
                    .push(episodes[idx].clone());
            }
        }

        // Convert to cluster objects
        let mut clusters = Vec::new();

        for (_, cluster_episodes) in cluster_map {
            let centroid = Self::compute_centroid(&cluster_episodes);

            clusters.push(EpisodeCluster {
                cluster_id: Uuid::new_v4(),
                episodes: cluster_episodes,
                centroid,
            });
        }

        Ok(clusters)
    }

    /// Compute centroid (mean) of cluster embeddings
    fn compute_centroid(episodes: &[Episode]) -> Option<Vec<f32>> {
        let embeddings: Vec<&Vec<f32>> = episodes
            .iter()
            .filter_map(|e| e.embedding.as_ref())
            .collect();

        if embeddings.is_empty() {
            return None;
        }

        let dim = embeddings[0].len();
        let mut centroid = vec![0.0f32; dim];

        for embedding in &embeddings {
            for (i, &val) in embedding.iter().enumerate() {
                centroid[i] += val;
            }
        }

        let count = embeddings.len() as f32;
        for val in &mut centroid {
            *val /= count;
        }

        Some(centroid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmbeddingGenerator, ExecutionStatus, MockEmbeddings};
    use chrono::Utc;
    use rust_decimal::Decimal;
    use serde_json::json;

    #[tokio::test]
    async fn test_dbscan_clustering() {
        let embedder = MockEmbeddings::new(128);
        let agent_id = Uuid::new_v4();

        // Create episodes with similar queries that should cluster
        let mut episodes = Vec::new();

        // Cluster 1: AMD-related queries
        let amd_queries = vec![
            "AMD market share analysis",
            "AMD datacenter revenue",
            "AMD GPU performance",
        ];

        for query in amd_queries {
            let embedding = embedder.generate(query).await.unwrap();
            episodes.push(Episode {
                episode_id: Uuid::new_v4(),
                agent_id,
                timestamp_ref: Utc::now(),
                query: query.to_string(),
                context: json!({}),
                execution_status: ExecutionStatus::Failure,
                error_details: Some("Test error".to_string()),
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(Decimal::new(1, 3)),
                embedding: Some(embedding),
                consolidated: false,
                tags: vec![],
                provenance: crate::Provenance::AutoPass,
                authority_weight: 0.5,
                dyad_id: None,
                persona_version_at_write: None,
            });
        }

        // Cluster 2: Intel-related queries
        let intel_queries = vec![
            "Intel processor market share",
            "Intel CPU benchmarks",
            "Intel datacenter chips",
        ];

        for query in intel_queries {
            let embedding = embedder.generate(query).await.unwrap();
            episodes.push(Episode {
                episode_id: Uuid::new_v4(),
                agent_id,
                timestamp_ref: Utc::now(),
                query: query.to_string(),
                context: json!({}),
                execution_status: ExecutionStatus::Failure,
                error_details: Some("Test error".to_string()),
                provenance: crate::Provenance::AutoPass,
                authority_weight: 0.5,
                dyad_id: None,
                persona_version_at_write: None,
                execution_time_ms: 1000,
                tokens_used: Some(100),
                cost_usd: Some(Decimal::new(1, 3)),
                embedding: Some(embedding),
                consolidated: false,
                tags: vec![],
            });
        }

        // Run DBSCAN
        let clusterer = DBSCANClustering::new(0.5, 2);
        let clusters = clusterer.cluster(episodes).unwrap();

        println!("✅ DBSCAN found {} clusters", clusters.len());

        // Should find at least 1 cluster (possibly 2 if AMD and Intel separate)
        assert!(clusters.len() >= 1);

        for (i, cluster) in clusters.iter().enumerate() {
            println!("   Cluster {}: {} episodes", i + 1, cluster.episodes.len());
            assert!(cluster.episodes.len() >= 2); // Min samples

            // Check centroid exists
            assert!(cluster.centroid.is_some());
        }
    }

    #[test]
    fn test_cosine_distance() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let c = vec![1.0, 0.0, 0.0];

        // Perpendicular vectors should have distance ~1.0
        let dist_ab = DBSCANClustering::cosine_distance(&a, &b);
        assert!((dist_ab - 1.0).abs() < 0.01);

        // Identical vectors should have distance 0.0
        let dist_ac = DBSCANClustering::cosine_distance(&a, &c);
        assert!(dist_ac.abs() < 0.01);
    }
}
