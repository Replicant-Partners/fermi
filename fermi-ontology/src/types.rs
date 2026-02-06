use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Represents a Mermaid ER diagram
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MermaidDiagram {
    /// The complete Mermaid diagram content
    pub content: String,

    /// Diagram metadata
    pub metadata: DiagramMetadata,
}

/// Metadata about a Mermaid diagram
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramMetadata {
    /// Agent ID this diagram represents
    pub agent_id: Uuid,

    /// Agent name
    pub agent_name: String,

    /// Number of entities in the diagram
    pub entity_count: i32,

    /// Number of relationships in the diagram
    pub relationship_count: i32,

    /// When the diagram was generated
    pub generated_at: DateTime<Utc>,

    /// Consolidation job that created this diagram
    pub job_id: Option<Uuid>,
}

/// Represents a git commit for an ontology
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    /// The git commit SHA
    pub sha: String,

    /// Commit message
    pub message: String,

    /// When the commit was created
    pub timestamp: DateTime<Utc>,

    /// Agent name (used for file naming)
    pub agent_name: String,

    /// Path to the ontology file in the repo
    pub file_path: String,
}

/// Statistics about an ontology snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyStats {
    /// Total number of entities
    pub entity_count: i32,

    /// Total number of facts/relationships
    pub fact_count: i32,

    /// Total number of semantic rules
    pub rule_count: i32,

    /// Number of episodic memories consolidated
    pub episode_count: i32,

    /// Consolidation job details
    pub job_id: Option<Uuid>,

    /// When these stats were collected
    pub collected_at: DateTime<Utc>,
}

impl OntologyStats {
    /// Create stats from entity and fact counts
    pub fn new(
        entity_count: i32,
        fact_count: i32,
        rule_count: i32,
        episode_count: i32,
        job_id: Option<Uuid>,
    ) -> Self {
        Self {
            entity_count,
            fact_count,
            rule_count,
            episode_count,
            job_id,
            collected_at: Utc::now(),
        }
    }
}

/// Configuration for git repository management
#[derive(Debug, Clone)]
pub struct GitConfig {
    /// Path to the git repository
    pub repo_path: String,

    /// Git author name
    pub author_name: String,

    /// Git author email
    pub author_email: String,

    /// Branch to commit to (default: "main")
    pub branch: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            repo_path: "./ontologies".to_string(),
            author_name: "Fermi ADM".to_string(),
            author_email: "adm@fermi.ai".to_string(),
            branch: "main".to_string(),
        }
    }
}

/// Configuration for Mermaid generation
#[derive(Debug, Clone)]
pub struct MermaidConfig {
    /// Include entity attributes in diagram
    pub include_attributes: bool,

    /// Include relationship labels
    pub include_labels: bool,

    /// Maximum entities to include (for large ontologies)
    pub max_entities: Option<usize>,

    /// Maximum relationships to include
    pub max_relationships: Option<usize>,
}

impl Default for MermaidConfig {
    fn default() -> Self {
        Self {
            include_attributes: true,
            include_labels: true,
            max_entities: None,
            max_relationships: None,
        }
    }
}

/// Cardinality types for relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cardinality {
    /// One-to-one: ||--||
    OneToOne,

    /// One-to-many: ||--o{
    OneToMany,

    /// Many-to-one: }o--||
    ManyToOne,

    /// Many-to-many: }o--o{
    ManyToMany,
}

impl Cardinality {
    /// Convert to Mermaid syntax
    pub fn to_mermaid(&self) -> &'static str {
        match self {
            Cardinality::OneToOne => "||--||",
            Cardinality::OneToMany => "||--o{",
            Cardinality::ManyToOne => "}o--||",
            Cardinality::ManyToMany => "}o--o{",
        }
    }
}

impl std::str::FromStr for Cardinality {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "one_to_one" | "onetoone" | "1:1" => Ok(Cardinality::OneToOne),
            "one_to_many" | "onetomany" | "1:n" => Ok(Cardinality::OneToMany),
            "many_to_one" | "manytoone" | "n:1" => Ok(Cardinality::ManyToOne),
            "many_to_many" | "manytomany" | "n:n" | "m:n" => Ok(Cardinality::ManyToMany),
            _ => Err(format!("Invalid cardinality: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cardinality_to_mermaid() {
        assert_eq!(Cardinality::OneToOne.to_mermaid(), "||--||");
        assert_eq!(Cardinality::OneToMany.to_mermaid(), "||--o{");
        assert_eq!(Cardinality::ManyToOne.to_mermaid(), "}o--||");
        assert_eq!(Cardinality::ManyToMany.to_mermaid(), "}o--o{");
    }

    #[test]
    fn test_cardinality_from_str() {
        assert_eq!(
            "one_to_one".parse::<Cardinality>().unwrap(),
            Cardinality::OneToOne
        );
        assert_eq!(
            "1:n".parse::<Cardinality>().unwrap(),
            Cardinality::OneToMany
        );
        assert_eq!(
            "n:1".parse::<Cardinality>().unwrap(),
            Cardinality::ManyToOne
        );
        assert_eq!(
            "m:n".parse::<Cardinality>().unwrap(),
            Cardinality::ManyToMany
        );
    }

    #[test]
    fn test_ontology_stats_creation() {
        let stats = OntologyStats::new(10, 25, 8, 150, None);
        assert_eq!(stats.entity_count, 10);
        assert_eq!(stats.fact_count, 25);
        assert_eq!(stats.rule_count, 8);
        assert_eq!(stats.episode_count, 150);
    }
}
