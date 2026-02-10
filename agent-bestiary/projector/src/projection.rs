use crate::error::{ProjectorError, Result};
use crate::types::*;
use agent_bestiary_memory::MemoryStore;
use chrono::Utc;
use linfa::prelude::*;
use linfa_reduction::Pca;
use ndarray::Array2;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

pub struct ProjectionEngine {
    store: Arc<MemoryStore>,
}

impl ProjectionEngine {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    /// Project all embeddings for a single agent
    pub async fn project_agent(
        &self,
        agent_id: Uuid,
        agent_name: &str,
        method: &ProjectionMethod,
        dimensions: u8,
    ) -> Result<ProjectionResult> {
        if !(2..=3).contains(&dimensions) {
            return Err(ProjectorError::InvalidDimensions(dimensions));
        }

        let records = self.collect_agent_embeddings(agent_id, agent_name).await?;
        if records.is_empty() {
            return Err(ProjectorError::NoEmbeddings);
        }

        let min_needed = dimensions as usize + 1;
        if records.len() < min_needed {
            return Err(ProjectorError::InsufficientEmbeddings {
                needed: min_needed,
                got: records.len(),
            });
        }

        info!(
            "Projecting {} embeddings for agent {} via {}",
            records.len(),
            agent_name,
            method.name()
        );

        let (points, explained_variance) = self.run_projection(&records, method, dimensions)?;

        Ok(ProjectionResult {
            agent_id: Some(agent_id),
            agent_name: Some(agent_name.to_string()),
            method: method.name().to_string(),
            dimensions,
            point_count: points.len(),
            points,
            computed_at: Utc::now(),
            explained_variance,
        })
    }

    /// Project embeddings across all agents (bestiary-wide)
    pub async fn project_bestiary(
        &self,
        method: &ProjectionMethod,
        dimensions: u8,
        limit: usize,
    ) -> Result<ProjectionResult> {
        if !(2..=3).contains(&dimensions) {
            return Err(ProjectorError::InvalidDimensions(dimensions));
        }

        let records = self.collect_global_embeddings(limit).await?;
        if records.is_empty() {
            return Err(ProjectorError::NoEmbeddings);
        }

        let min_needed = dimensions as usize + 1;
        if records.len() < min_needed {
            return Err(ProjectorError::InsufficientEmbeddings {
                needed: min_needed,
                got: records.len(),
            });
        }

        info!(
            "Projecting {} embeddings (bestiary-wide) via {}",
            records.len(),
            method.name()
        );

        let (points, explained_variance) = self.run_projection(&records, method, dimensions)?;

        Ok(ProjectionResult {
            agent_id: None,
            agent_name: None,
            method: method.name().to_string(),
            dimensions,
            point_count: points.len(),
            points,
            computed_at: Utc::now(),
            explained_variance,
        })
    }

    /// Project with temporal keyframes for animation
    pub async fn project_agent_temporal(
        &self,
        agent_id: Uuid,
        agent_name: &str,
        method: &ProjectionMethod,
        dimensions: u8,
        num_keyframes: usize,
    ) -> Result<TemporalProjectionResult> {
        if !(2..=3).contains(&dimensions) {
            return Err(ProjectorError::InvalidDimensions(dimensions));
        }

        let mut records = self.collect_agent_embeddings(agent_id, agent_name).await?;
        if records.is_empty() {
            return Err(ProjectorError::NoEmbeddings);
        }

        // Sort by timestamp for temporal ordering
        records.sort_by_key(|r| r.timestamp);

        let min_needed = dimensions as usize + 1;
        if records.len() < min_needed {
            return Err(ProjectorError::InsufficientEmbeddings {
                needed: min_needed,
                got: records.len(),
            });
        }

        info!(
            "Computing temporal projection for agent {} ({} embeddings, {} keyframes)",
            agent_name,
            records.len(),
            num_keyframes
        );

        // Build the full matrix and fit PCA on the entire dataset for consistent axes
        let matrix = self.build_matrix(&records);
        let dims = dimensions as usize;
        let dataset = DatasetBase::from(matrix.clone());
        let pca_model = Pca::params(dims)
            .fit(&dataset)
            .map_err(|e| ProjectorError::ProjectionFailed(e.to_string()))?;

        // Determine keyframe boundaries (evenly spaced by index)
        let total = records.len();
        let step = if num_keyframes > 1 {
            (total as f64 / num_keyframes as f64).ceil() as usize
        } else {
            total
        };

        let mut keyframes = Vec::new();
        let mut idx = step.min(total);

        while idx <= total {
            let subset = &records[..idx];
            let subset_matrix = self.build_matrix(subset);
            let subset_dataset = DatasetBase::from(subset_matrix);
            let projected = pca_model.transform(subset_dataset);
            let coords = projected.records();

            let points: Vec<TemporalPoint> = subset
                .iter()
                .enumerate()
                .map(|(i, rec)| {
                    let row = coords.row(i);
                    TemporalPoint {
                        id: rec.id,
                        source: rec.source.clone(),
                        label: rec.label.clone(),
                        x: row[0] as f32,
                        y: row[1] as f32,
                        z: if dims == 3 { Some(row[2] as f32) } else { None },
                    }
                })
                .collect();

            let label = format!(
                "{} points ({})",
                points.len(),
                subset.last().unwrap().timestamp.format("%Y-%m-%d")
            );

            keyframes.push(TemporalKeyframe {
                timestamp: subset.last().unwrap().timestamp,
                label,
                point_count: points.len(),
                points,
            });

            if idx == total {
                break;
            }
            idx = (idx + step).min(total);
        }

        Ok(TemporalProjectionResult {
            agent_id,
            method: method.name().to_string(),
            dimensions,
            total_points: total,
            keyframes,
            computed_at: Utc::now(),
        })
    }

    // ─── Internal helpers ──────────────────────────────────────────

    fn run_projection(
        &self,
        records: &[EmbeddingRecord],
        method: &ProjectionMethod,
        dimensions: u8,
    ) -> Result<(Vec<ProjectedPoint>, Option<Vec<f64>>)> {
        let matrix = self.build_matrix(records);
        let dims = dimensions as usize;

        match method {
            ProjectionMethod::Pca => {
                let dataset = DatasetBase::from(matrix);
                let pca = Pca::params(dims)
                    .fit(&dataset)
                    .map_err(|e| ProjectorError::ProjectionFailed(e.to_string()))?;
                let projected = pca.transform(dataset);
                let coords = projected.records();
                let explained: Vec<f64> = pca.explained_variance_ratio().iter().copied().collect();

                let points = self.map_to_projected_points(records, coords, dims);
                Ok((points, Some(explained)))
            }
            ProjectionMethod::Tsne { .. } => {
                // t-SNE not yet implemented — fall back to PCA
                // linfa-tsne requires additional dependency; defer to v2
                let dataset = DatasetBase::from(matrix);
                let pca = Pca::params(dims)
                    .fit(&dataset)
                    .map_err(|e| ProjectorError::ProjectionFailed(e.to_string()))?;
                let projected = pca.transform(dataset);
                let coords = projected.records();
                let explained: Vec<f64> = pca.explained_variance_ratio().iter().copied().collect();

                let points = self.map_to_projected_points(records, coords, dims);
                Ok((points, Some(explained)))
            }
        }
    }

    fn build_matrix(&self, records: &[EmbeddingRecord]) -> Array2<f64> {
        let n = records.len();
        let d = records[0].embedding.len();
        let mut data = Vec::with_capacity(n * d);
        for rec in records {
            data.extend(rec.embedding.iter().map(|&v| v as f64));
        }
        Array2::from_shape_vec((n, d), data).expect("Shape mismatch building embedding matrix")
    }

    fn map_to_projected_points(
        &self,
        records: &[EmbeddingRecord],
        coords: &Array2<f64>,
        dims: usize,
    ) -> Vec<ProjectedPoint> {
        records
            .iter()
            .enumerate()
            .map(|(i, rec)| {
                let row = coords.row(i);
                ProjectedPoint {
                    id: rec.id,
                    source: rec.source.clone(),
                    label: rec.label.clone(),
                    x: row[0] as f32,
                    y: row[1] as f32,
                    z: if dims == 3 { Some(row[2] as f32) } else { None },
                    metadata: rec.metadata.clone(),
                    timestamp: rec.timestamp,
                }
            })
            .collect()
    }

    /// Collect all embedding-bearing records for a single agent
    async fn collect_agent_embeddings(
        &self,
        agent_id: Uuid,
        agent_name: &str,
    ) -> Result<Vec<EmbeddingRecord>> {
        let mut records = Vec::new();

        // Episodes
        if let Ok(episodes) = self.store.get_all_episodes_with_embeddings(agent_id).await {
            for ep in episodes {
                if let Some(embedding) = ep.embedding {
                    records.push(EmbeddingRecord {
                        id: ep.episode_id,
                        source: EmbeddingSource::Episode,
                        label: truncate(&ep.query, 80),
                        embedding,
                        metadata: PointMetadata {
                            agent_id,
                            agent_name: Some(agent_name.to_string()),
                            execution_status: Some(ep.execution_status.to_string()),
                            consolidated: Some(ep.consolidated),
                            cluster_id: None,
                            confidence_score: None,
                            verification_status: None,
                            entity_type: None,
                            extraction_confidence: None,
                            member_count: None,
                        },
                        timestamp: ep.timestamp_ref,
                    });
                }
            }
        }

        // Semantic rules
        if let Ok(rules) = self.store.get_agent_semantic_rules(agent_id).await {
            for rule in rules {
                if let Some(embedding) = rule.embedding {
                    records.push(EmbeddingRecord {
                        id: rule.rule_id,
                        source: EmbeddingSource::SemanticRule,
                        label: truncate(&rule.rule_content, 80),
                        embedding,
                        metadata: PointMetadata {
                            agent_id,
                            agent_name: Some(agent_name.to_string()),
                            execution_status: None,
                            consolidated: None,
                            cluster_id: None,
                            confidence_score: Some(rule.confidence_score),
                            verification_status: Some(rule.verification_status.to_string()),
                            entity_type: None,
                            extraction_confidence: None,
                            member_count: None,
                        },
                        timestamp: rule.created_at,
                    });
                }
            }
        }

        // Entities
        if let Ok(entities) = self.store.get_agent_entities(agent_id).await {
            for entity in entities {
                if let Some(embedding) = entity.embedding {
                    records.push(EmbeddingRecord {
                        id: entity.entity_id,
                        source: EmbeddingSource::Entity,
                        label: entity.entity_name.clone(),
                        embedding,
                        metadata: PointMetadata {
                            agent_id,
                            agent_name: Some(agent_name.to_string()),
                            execution_status: None,
                            consolidated: None,
                            cluster_id: None,
                            confidence_score: None,
                            verification_status: None,
                            entity_type: Some(entity.entity_type.clone()),
                            extraction_confidence: Some(entity.extraction_confidence),
                            member_count: None,
                        },
                        timestamp: entity.t_valid,
                    });
                }
            }
        }

        // Communities
        if let Ok(communities) = self.store.get_agent_communities(agent_id).await {
            for comm in communities {
                if let Some(embedding) = comm.embedding {
                    records.push(EmbeddingRecord {
                        id: comm.community_id,
                        source: EmbeddingSource::Community,
                        label: comm
                            .community_name
                            .unwrap_or_else(|| "unnamed cluster".to_string()),
                        embedding,
                        metadata: PointMetadata {
                            agent_id,
                            agent_name: Some(agent_name.to_string()),
                            execution_status: None,
                            consolidated: None,
                            cluster_id: None,
                            confidence_score: None,
                            verification_status: None,
                            entity_type: None,
                            extraction_confidence: None,
                            member_count: Some(comm.member_count),
                        },
                        timestamp: comm.created_at,
                    });
                }
            }
        }

        Ok(records)
    }

    /// Collect embeddings across all agents for bestiary-wide view
    async fn collect_global_embeddings(&self, limit: usize) -> Result<Vec<EmbeddingRecord>> {
        let mut records = Vec::new();
        let agents = self
            .store
            .list_agents()
            .await
            .map_err(|e| ProjectorError::Database(e.into()))?;

        for agent in &agents {
            if agent.agent_name.starts_with("test_agent_") {
                continue;
            }
            let agent_records = self
                .collect_agent_embeddings(agent.agent_id, &agent.agent_name)
                .await?;
            records.extend(agent_records);
            if records.len() >= limit {
                records.truncate(limit);
                break;
            }
        }

        Ok(records)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
