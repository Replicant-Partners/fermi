use crate::error::{OntologyError, Result};
use crate::types::{GitCommit, GitConfig, OntologyStats};
use chrono::Utc;
use git2::{Cred, IndexAddOption, PushOptions, RemoteCallbacks, Repository, Signature};
use std::fs;
use std::path::PathBuf;
use tracing::{error, info, warn};

/// Manages git operations for ontology versioning
/// Each agent gets its own git repository
pub struct GitManager {
    config: GitConfig,
}

impl GitManager {
    /// Create a new GitManager
    pub fn new(config: GitConfig) -> Result<Self> {
        // Ensure base path exists
        let base_path = PathBuf::from(&config.base_path);
        if !base_path.exists() {
            fs::create_dir_all(&base_path)?;
        }

        Ok(Self { config })
    }

    /// Get the repository path for a specific agent
    fn get_agent_repo_path(&self, agent_name: &str) -> PathBuf {
        PathBuf::from(&self.config.base_path).join(agent_name)
    }

    /// Get the GitHub repository URL for an agent
    fn get_github_url(&self, agent_name: &str) -> Option<String> {
        self.config
            .github_org
            .as_ref()
            .map(|org| format!("https://github.com/{}/fermi-agent-{}", org, agent_name))
    }

    /// Get the GitHub remote URL with authentication
    fn get_github_remote_url(&self, agent_name: &str) -> Option<String> {
        match (&self.config.github_org, &self.config.github_token) {
            (Some(org), Some(token)) => Some(format!(
                "https://{}@github.com/{}/fermi-agent-{}.git",
                token, org, agent_name
            )),
            _ => None,
        }
    }

    /// Initialize or open an agent's git repository
    fn init_or_open_repo(&self, agent_name: &str) -> Result<Repository> {
        let repo_path = self.get_agent_repo_path(agent_name);

        // Create directory if it doesn't exist
        if !repo_path.exists() {
            fs::create_dir_all(&repo_path)?;
        }

        // Initialize or open repository
        let repo = if repo_path.join(".git").exists() {
            Repository::open(&repo_path)?
        } else {
            info!("Initializing new git repository for agent: {}", agent_name);
            let repo = Repository::init(&repo_path)?;

            // Set up remote if GitHub is configured
            if let Some(remote_url) = self.get_github_remote_url(agent_name) {
                match repo.remote(&self.config.remote_name, &remote_url) {
                    Ok(_) => {
                        info!(
                            "Added remote '{}' for agent {}",
                            self.config.remote_name, agent_name
                        );
                    }
                    Err(e) => {
                        warn!("Failed to add remote: {}", e);
                    }
                }
            }

            repo
        };

        Ok(repo)
    }

    /// Commit an ontology to git
    pub fn commit_ontology(
        &self,
        agent_name: &str,
        mermaid_content: &str,
        stats: &OntologyStats,
    ) -> Result<GitCommit> {
        let repo = self.init_or_open_repo(agent_name)?;
        let repo_path = self.get_agent_repo_path(agent_name);

        // Write ontology file (at repo root)
        let file_name = "ontology.mermaid";
        let file_path = repo_path.join(file_name);
        fs::write(&file_path, mermaid_content)?;

        // Write README if it doesn't exist
        let readme_path = repo_path.join("README.md");
        if !readme_path.exists() {
            let readme_content = self.generate_readme(agent_name, stats);
            fs::write(&readme_path, readme_content)?;
        }

        // Stage all files
        let mut index = repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        // Check if there are changes to commit
        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        // Get parent commit (if exists)
        let parent_commit = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        // Check if tree has changed
        if let Some(ref parent) = parent_commit {
            if parent.tree()?.id() == tree_oid {
                info!("No changes to commit for agent: {}", agent_name);
                // Return existing commit
                return Ok(GitCommit {
                    sha: parent.id().to_string(),
                    message: parent.message().unwrap_or("").to_string(),
                    timestamp: Utc::now(),
                    agent_name: agent_name.to_string(),
                    file_path: file_name.to_string(),
                    github_url: self.get_github_url(agent_name),
                    pushed_to_remote: false,
                });
            }
        }

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

        let sha = commit_oid.to_string();
        info!("Created commit {} for agent {}", &sha[..8], agent_name);

        // Push to GitHub if configured
        let pushed_to_remote = if self.config.auto_push {
            self.push_to_github(&repo, agent_name).unwrap_or_else(|e| {
                error!("Failed to push to GitHub: {}", e);
                false
            })
        } else {
            false
        };

        Ok(GitCommit {
            sha,
            message,
            timestamp: Utc::now(),
            agent_name: agent_name.to_string(),
            file_path: file_name.to_string(),
            github_url: self.get_github_url(agent_name),
            pushed_to_remote,
        })
    }

    /// Push repository to GitHub
    fn push_to_github(&self, repo: &Repository, agent_name: &str) -> Result<bool> {
        // Check if remote exists
        let mut remote = match repo.find_remote(&self.config.remote_name) {
            Ok(remote) => remote,
            Err(_) => {
                // Try to add remote if it doesn't exist
                if let Some(remote_url) = self.get_github_remote_url(agent_name) {
                    repo.remote(&self.config.remote_name, &remote_url)?
                } else {
                    warn!("No GitHub configuration found, skipping push");
                    return Ok(false);
                }
            }
        };

        info!("Pushing to GitHub: {}", agent_name);

        // Set up callbacks for authentication
        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = &self.config.github_token {
            let token_clone = token.clone();
            callbacks.credentials(move |_url, _username_from_url, _allowed_types| {
                Cred::userpass_plaintext("x-access-token", &token_clone)
            });
        }

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // Push to remote
        let refspec = format!(
            "refs/heads/{}:refs/heads/{}",
            self.config.branch, self.config.branch
        );
        match remote.push(&[&refspec], Some(&mut push_options)) {
            Ok(_) => {
                info!("Successfully pushed agent {} to GitHub", agent_name);
                Ok(true)
            }
            Err(e) => {
                error!("Failed to push to GitHub: {}", e);
                Err(e.into())
            }
        }
    }

    /// Generate a detailed commit message
    fn generate_commit_message(&self, agent_name: &str, stats: &OntologyStats) -> String {
        let mut message = format!("agent({}): consolidation snapshot\n\n", agent_name);

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

    /// Generate README for agent repository
    fn generate_readme(&self, agent_name: &str, stats: &OntologyStats) -> String {
        format!(
            r#"# Fermi Agent: {}

This repository contains the ontology for the **{}** forecasting agent.

## Ontology

The `ontology.mermaid` file contains the agent's knowledge graph as a Mermaid ER diagram.

## Current Statistics

- **Entities**: {}
- **Relationships**: {}
- **Semantic Rules**: {}
- **Episodes Consolidated**: {}

## About Fermi ADM

This ontology is automatically generated by Fermi's Active Dreaming Memory (ADM) system.
The agent consolidates episodic memories into semantic knowledge, building and evolving
this ontology over time.

## Viewing the Ontology

You can view the Mermaid diagram on GitHub, or use any Mermaid-compatible viewer:
- [Mermaid Live Editor](https://mermaid.live/)
- GitHub markdown preview (automatic)
- VS Code with Mermaid extension

---

*Last updated: {}*
"#,
            agent_name,
            agent_name,
            stats.entity_count,
            stats.fact_count,
            stats.rule_count,
            stats.episode_count,
            stats.collected_at.to_rfc3339()
        )
    }

    /// Get the latest commit for an agent
    pub fn get_latest_commit(&self, agent_name: &str) -> Result<Option<GitCommit>> {
        let repo_path = self.get_agent_repo_path(agent_name);

        if !repo_path.join(".git").exists() {
            return Ok(None);
        }

        let repo = Repository::open(&repo_path)?;

        // Get HEAD commit
        let head = match repo.head() {
            Ok(head) => head,
            Err(_) => return Ok(None), // No commits yet
        };

        let commit = head.peel_to_commit()?;

        Ok(Some(GitCommit {
            sha: commit.id().to_string(),
            message: commit.message().unwrap_or("").to_string(),
            timestamp: Utc::now(),
            agent_name: agent_name.to_string(),
            file_path: "ontology.mermaid".to_string(),
            github_url: self.get_github_url(agent_name),
            pushed_to_remote: false, // We don't track this retroactively
        }))
    }

    /// Read an ontology file from the repository
    pub fn read_ontology(&self, agent_name: &str) -> Result<String> {
        let file_path = self
            .get_agent_repo_path(agent_name)
            .join("ontology.mermaid");

        if !file_path.exists() {
            return Err(OntologyError::RepoNotFound(format!(
                "Ontology file not found for agent: {}",
                agent_name
            )));
        }

        Ok(fs::read_to_string(file_path)?)
    }

    /// List all agent ontologies
    pub fn list_ontologies(&self) -> Result<Vec<String>> {
        let base_path = PathBuf::from(&self.config.base_path);

        if !base_path.exists() {
            return Ok(Vec::new());
        }

        let mut agents = Vec::new();

        for entry in fs::read_dir(base_path)? {
            let entry = entry?;
            let path = entry.path();

            // Check if this is a directory with a .git subdirectory
            if path.is_dir() && path.join(".git").exists() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    agents.push(name.to_string());
                }
            }
        }

        Ok(agents)
    }

    /// Get repository path for an agent
    pub fn get_repo_path(&self, agent_name: &str) -> PathBuf {
        self.get_agent_repo_path(agent_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_config(base_path: &str) -> GitConfig {
        GitConfig {
            base_path: base_path.to_string(),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            branch: "main".to_string(),
            github_org: None,
            github_token: None,
            auto_push: false,
            remote_name: "origin".to_string(),
        }
    }

    #[test]
    fn test_create_git_manager() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap());

        let manager = GitManager::new(config).unwrap();
        assert!(PathBuf::from(manager.config.base_path).exists());
    }

    #[test]
    fn test_per_agent_repos() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap());
        let manager = GitManager::new(config).unwrap();

        let stats = OntologyStats::new(10, 25, 5, 100, None);
        let mermaid_content = "erDiagram\n    COMPANY ||--o{ PRODUCT : produces\n";

        // Commit for agent1
        let commit1 = manager
            .commit_ontology("agent1", mermaid_content, &stats)
            .unwrap();

        // Commit for agent2
        let commit2 = manager
            .commit_ontology("agent2", mermaid_content, &stats)
            .unwrap();

        // Verify separate repos
        assert_ne!(commit1.sha, commit2.sha);
        assert!(manager.get_repo_path("agent1").join(".git").exists());
        assert!(manager.get_repo_path("agent2").join(".git").exists());
    }

    #[test]
    fn test_commit_ontology() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap());
        let manager = GitManager::new(config).unwrap();

        let stats = OntologyStats::new(10, 25, 5, 100, None);
        let mermaid_content = "erDiagram\n    COMPANY ||--o{ PRODUCT : produces\n";

        let commit = manager
            .commit_ontology("test_agent", mermaid_content, &stats)
            .unwrap();

        assert!(!commit.sha.is_empty());
        assert!(commit.message.contains("test_agent"));
        assert!(commit.message.contains("Entities: 10"));
        assert_eq!(commit.file_path, "ontology.mermaid");
    }

    #[test]
    fn test_read_ontology() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(temp_dir.path().to_str().unwrap());
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
        let config = create_test_config(temp_dir.path().to_str().unwrap());
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

    #[test]
    fn test_github_url_generation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = create_test_config(temp_dir.path().to_str().unwrap());
        config.github_org = Some("Replicant-Partners".to_string());

        let manager = GitManager::new(config).unwrap();
        let url = manager.get_github_url("market_research").unwrap();
        assert_eq!(
            url,
            "https://github.com/Replicant-Partners/fermi-agent-market_research"
        );
    }
}
