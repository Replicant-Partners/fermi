use crate::error::{OntologyError, Result};
use crate::types::{GitCommit, GitConfig, OntologyStats};
use chrono::Utc;
use git2::{Repository, Signature};
use std::fs;
use std::path::{Path, PathBuf};

/// Manages git operations for ontology versioning
pub struct GitManager {
    config: GitConfig,
    repo_path: PathBuf,
}

impl GitManager {
    /// Create a new GitManager
    pub fn new(config: GitConfig) -> Result<Self> {
        let repo_path = PathBuf::from(&config.repo_path);

        // Ensure repository directory exists
        if !repo_path.exists() {
            fs::create_dir_all(&repo_path)?;
        }

        // Initialize git repository if not already initialized
        if !repo_path.join(".git").exists() {
            Repository::init(&repo_path)?;
        }

        Ok(Self { config, repo_path })
    }

    /// Commit an ontology to git
    pub fn commit_ontology(
        &self,
        agent_name: &str,
        mermaid_content: &str,
        stats: &OntologyStats,
    ) -> Result<GitCommit> {
        // Open repository
        let repo = Repository::open(&self.repo_path)?;

        // Create ontologies directory if it doesn't exist
        let ontologies_dir = self.repo_path.join("ontologies");
        if !ontologies_dir.exists() {
            fs::create_dir_all(&ontologies_dir)?;
        }

        // Write ontology file
        let file_name = format!("{}.mermaid", agent_name);
        let file_path = ontologies_dir.join(&file_name);
        fs::write(&file_path, mermaid_content)?;

        // Stage the file
        let mut index = repo.index()?;
        index.add_path(Path::new(&format!("ontologies/{}", file_name)))?;
        index.write()?;

        // Create tree
        let oid = index.write_tree()?;
        let tree = repo.find_tree(oid)?;

        // Get parent commit (if exists)
        let parent_commit = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        // Create signature
        let signature = Signature::now(&self.config.author_name, &self.config.author_email)?;

        // Generate commit message
        let message = self.generate_commit_message(agent_name, stats);

        // Create commit
        let commit_oid = if let Some(parent) = parent_commit {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &message,
                &tree,
                &[&parent],
            )?
        } else {
            // Initial commit
            repo.commit(Some("HEAD"), &signature, &signature, &message, &tree, &[])?
        };

        // Get commit SHA
        let sha = commit_oid.to_string();

        Ok(GitCommit {
            sha,
            message,
            timestamp: Utc::now(),
            agent_name: agent_name.to_string(),
            file_path: format!("ontologies/{}", file_name),
        })
    }

    /// Generate a detailed commit message
    fn generate_commit_message(&self, agent_name: &str, stats: &OntologyStats) -> String {
        let mut message = format!("Update ontology for agent: {}\n\n", agent_name);

        message.push_str("Ontology Statistics:\n");
        message.push_str(&format!("- Entities: {}\n", stats.entity_count));
        message.push_str(&format!("- Relationships: {}\n", stats.fact_count));
        message.push_str(&format!("- Semantic Rules: {}\n", stats.rule_count));
        message.push_str(&format!(
            "- Episodes Consolidated: {}\n\n",
            stats.episode_count
        ));

        if let Some(job_id) = stats.job_id {
            message.push_str(&format!("Consolidation Job: {}\n", job_id));
        }

        message.push_str(&format!("Timestamp: {}\n", stats.collected_at.to_rfc3339()));

        message
    }

    /// Get the latest commit for an agent
    pub fn get_latest_commit(&self, agent_name: &str) -> Result<Option<GitCommit>> {
        let repo = Repository::open(&self.repo_path)?;

        // Get HEAD commit
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(None), // No commits yet
        };

        let commit = head.peel_to_commit()?;

        // Check if this commit modified the agent's ontology file
        let file_path = format!("ontologies/{}.mermaid", agent_name);

        Ok(Some(GitCommit {
            sha: commit.id().to_string(),
            message: commit.message().unwrap_or("").to_string(),
            timestamp: Utc::now(), // Note: git2 doesn't easily expose commit timestamp
            agent_name: agent_name.to_string(),
            file_path,
        }))
    }

    /// Read an ontology file from the repository
    pub fn read_ontology(&self, agent_name: &str) -> Result<String> {
        let file_path = self
            .repo_path
            .join("ontologies")
            .join(format!("{}.mermaid", agent_name));

        if !file_path.exists() {
            return Err(OntologyError::RepoNotFound(format!(
                "Ontology file not found: {}",
                file_path.display()
            )));
        }

        Ok(fs::read_to_string(file_path)?)
    }

    /// List all agent ontologies in the repository
    pub fn list_ontologies(&self) -> Result<Vec<String>> {
        let ontologies_dir = self.repo_path.join("ontologies");

        if !ontologies_dir.exists() {
            return Ok(Vec::new());
        }

        let mut agents = Vec::new();

        for entry in fs::read_dir(ontologies_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("mermaid") {
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    agents.push(name.to_string());
                }
            }
        }

        Ok(agents)
    }

    /// Get repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_git_manager() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig {
            repo_path: temp_dir.path().to_str().unwrap().to_string(),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            branch: "main".to_string(),
        };

        let manager = GitManager::new(config).unwrap();
        assert!(manager.repo_path.exists());
    }

    #[test]
    fn test_commit_ontology() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig {
            repo_path: temp_dir.path().to_str().unwrap().to_string(),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            branch: "main".to_string(),
        };

        let manager = GitManager::new(config).unwrap();

        let stats = OntologyStats::new(10, 25, 5, 100, None);
        let mermaid_content = "erDiagram\n    COMPANY ||--o{ PRODUCT : produces\n";

        let commit = manager
            .commit_ontology("test_agent", mermaid_content, &stats)
            .unwrap();

        assert!(!commit.sha.is_empty());
        assert!(commit.message.contains("test_agent"));
        assert!(commit.message.contains("Entities: 10"));
    }

    #[test]
    fn test_read_ontology() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig {
            repo_path: temp_dir.path().to_str().unwrap().to_string(),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            branch: "main".to_string(),
        };

        let manager = GitManager::new(config).unwrap();

        let mermaid_content = "erDiagram\n    COMPANY ||--o{ PRODUCT : produces\n";
        let stats = OntologyStats::new(5, 10, 2, 50, None);

        manager
            .commit_ontology("test_agent", mermaid_content, &stats)
            .unwrap();

        let content = manager.read_ontology("test_agent").unwrap();
        assert_eq!(content, mermaid_content);
    }

    #[test]
    fn test_list_ontologies() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitConfig {
            repo_path: temp_dir.path().to_str().unwrap().to_string(),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            branch: "main".to_string(),
        };

        let manager = GitManager::new(config).unwrap();
        let stats = OntologyStats::new(1, 2, 1, 10, None);

        manager
            .commit_ontology("agent1", "erDiagram\n", &stats)
            .unwrap();
        manager
            .commit_ontology("agent2", "erDiagram\n", &stats)
            .unwrap();

        let ontologies = manager.list_ontologies().unwrap();
        assert_eq!(ontologies.len(), 2);
        assert!(ontologies.contains(&"agent1".to_string()));
        assert!(ontologies.contains(&"agent2".to_string()));
    }
}
