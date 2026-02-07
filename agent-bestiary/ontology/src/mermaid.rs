use crate::error::{OntologyError, Result};
use crate::types::{DiagramMetadata, MermaidConfig, MermaidDiagram};
use chrono::Utc;
use agent_bestiary_memory::MemoryStore;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Generates Mermaid ER diagrams from agent ontologies
pub struct MermaidGenerator {
    store: MemoryStore,
    config: MermaidConfig,
}

impl MermaidGenerator {
    /// Create a new Mermaid generator
    pub fn new(store: MemoryStore) -> Self {
        Self {
            store,
            config: MermaidConfig::default(),
        }
    }

    /// Create a new Mermaid generator with custom configuration
    pub fn with_config(store: MemoryStore, config: MermaidConfig) -> Self {
        Self { store, config }
    }

    /// Generate a Mermaid ER diagram for an agent
    pub async fn generate(&self, agent_id: Uuid) -> Result<MermaidDiagram> {
        // Fetch agent details
        let agent = self
            .store
            .get_agent(agent_id)
            .await?
            .ok_or_else(|| OntologyError::AgentNotFound(agent_id.to_string()))?;

        // Fetch entities
        let mut entities = self.store.get_agent_entities(agent_id).await?;
        if entities.is_empty() {
            return Err(OntologyError::NoEntities(agent.agent_name.clone()));
        }

        // Apply entity limit if configured
        if let Some(max) = self.config.max_entities {
            entities.truncate(max);
        }

        // Fetch facts (relationships) for this agent
        let facts = self.store.get_agent_facts(agent_id).await?;

        // Apply relationship limit if configured
        let facts = if let Some(max) = self.config.max_relationships {
            facts.into_iter().take(max).collect()
        } else {
            facts
        };

        // Build entity lookup map
        let entity_map: HashMap<Uuid, &agent_bestiary_memory::types::Entity> =
            entities.iter().map(|e| (e.entity_id, e)).collect();

        // Generate Mermaid content
        let mut content = String::from("erDiagram\n");

        // Add relationships
        for fact in &facts {
            if let (Some(source), Some(target)) = (
                entity_map.get(&fact.source_entity_id),
                entity_map.get(&fact.target_entity_id),
            ) {
                let cardinality_str = fact.relation_cardinality.to_mermaid();
                let label = if self.config.include_labels {
                    format!(" : \"{}\"", fact.relation_type)
                } else {
                    String::new()
                };

                content.push_str(&format!(
                    "    {} {}{}{}\n",
                    Self::sanitize_entity_name(&source.entity_type),
                    cardinality_str,
                    Self::sanitize_entity_name(&target.entity_type),
                    label
                ));
            }
        }

        content.push('\n');

        // Add entity definitions with attributes
        if self.config.include_attributes {
            // Group entities by type to avoid duplicates in diagram
            let mut entity_types_seen = HashSet::new();

            for entity in &entities {
                let entity_type = Self::sanitize_entity_name(&entity.entity_type);

                // Skip if we've already added this entity type definition
                if entity_types_seen.contains(&entity_type) {
                    continue;
                }
                entity_types_seen.insert(entity_type.clone());

                content.push_str(&format!("    {} {{\n", entity_type));

                // Add standard attributes from Entity struct
                content.push_str("        uuid entity_id PK\n");
                content.push_str("        uuid agent_id FK\n");
                content.push_str("        string entity_name\n");
                content.push_str("        string entity_type\n");
                content.push_str("        text summary\n");

                // Add temporal tracking
                content.push_str("        timestamp t_valid\n");
                content.push_str("        timestamp t_invalid\n");

                // Add metadata
                content.push_str("        float extraction_confidence\n");

                content.push_str("    }\n\n");
            }
        }

        // Create metadata
        let metadata = DiagramMetadata {
            agent_id,
            agent_name: agent.agent_name.clone(),
            entity_count: entities.len() as i32,
            relationship_count: facts.len() as i32,
            generated_at: Utc::now(),
            job_id: None,
        };

        Ok(MermaidDiagram { content, metadata })
    }

    /// Sanitize entity names for Mermaid (uppercase, no spaces)
    fn sanitize_entity_name(name: &str) -> String {
        name.to_uppercase().replace(' ', "_").replace('-', "_")
    }

    /// Get entity and relationship counts for an agent
    pub async fn get_stats(&self, agent_id: Uuid) -> Result<(i32, i32)> {
        let entities = self.store.get_agent_entities(agent_id).await?;
        let entity_count = entities.len() as i32;

        let facts = self.store.get_agent_facts(agent_id).await?;
        let fact_count = facts.len() as i32;

        Ok((entity_count, fact_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_entity_name() {
        assert_eq!(
            MermaidGenerator::sanitize_entity_name("Product Category"),
            "PRODUCT_CATEGORY"
        );
        assert_eq!(
            MermaidGenerator::sanitize_entity_name("market-segment"),
            "MARKET_SEGMENT"
        );
        assert_eq!(MermaidGenerator::sanitize_entity_name("Company"), "COMPANY");
    }
}
