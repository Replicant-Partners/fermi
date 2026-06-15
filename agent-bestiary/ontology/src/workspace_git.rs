use crate::error::{OntologyError, Result};
use crate::types::GitConfig;
use chrono::{DateTime, Utc};
use git2::{Cred, IndexAddOption, PushOptions, RemoteCallbacks, Repository, Signature};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{error, info, warn};

/// A file entry in a workspace repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// A commit in a workspace repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCommit {
    pub sha: String,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub author: String,
}

/// Manages git repositories for workspaces.
/// Each workspace gets its own repo at `{base_path}/workspaces/{slug}/`.
#[derive(Clone)]
pub struct WorkspaceGitManager {
    config: GitConfig,
}

impl WorkspaceGitManager {
    pub fn new(config: GitConfig) -> Result<Self> {
        let ws_base = PathBuf::from(&config.base_path).join("workspaces");
        if !ws_base.exists() {
            fs::create_dir_all(&ws_base)?;
        }
        Ok(Self { config })
    }

    fn repo_path(&self, slug: &str) -> PathBuf {
        PathBuf::from(&self.config.base_path)
            .join("workspaces")
            .join(slug)
    }

    #[allow(dead_code)]
    fn github_url(&self, slug: &str) -> Option<String> {
        self.config
            .github_org
            .as_ref()
            .map(|org| format!("https://github.com/{}/fermi-workspace-{}", org, slug))
    }

    fn github_remote_url(&self, slug: &str) -> Option<String> {
        match (&self.config.github_org, &self.config.github_token) {
            (Some(org), Some(token)) => Some(format!(
                "https://{}@github.com/{}/fermi-workspace-{}.git",
                token, org, slug
            )),
            _ => None,
        }
    }

    /// Initialize or open a workspace's git repository.
    /// Creates the standard directory structure on first init.
    pub fn init_or_open(&self, slug: &str) -> Result<Repository> {
        let path = self.repo_path(slug);

        if !path.exists() {
            fs::create_dir_all(&path)?;
        }

        let repo = if path.join(".git").exists() {
            Repository::open(&path)?
        } else {
            info!("Initializing git repo for workspace: {}", slug);
            let repo = Repository::init(&path)?;

            // Set up remote if GitHub is configured
            if let Some(remote_url) = self.github_remote_url(slug) {
                match repo.remote(&self.config.remote_name, &remote_url) {
                    Ok(_) => info!("Added remote for workspace {}", slug),
                    Err(e) => warn!("Failed to add remote: {}", e),
                }
            }

            // Create standard directory structure
            for dir in &["context", "outputs", "agents", "ontology"] {
                let dir_path = path.join(dir);
                if !dir_path.exists() {
                    fs::create_dir_all(&dir_path)?;
                }
                // Add .gitkeep so empty dirs are tracked
                let keep = dir_path.join(".gitkeep");
                if !keep.exists() {
                    fs::write(&keep, "")?;
                }
            }

            // Create README
            let readme = format!(
                "# Workspace: {}\n\nThis repository tracks workspace activity, shared context, and agent ontologies.\n\n## Structure\n\n- `context/` — shared briefs and workspace knowledge\n- `outputs/` — pinned execution results\n- `agents/` — agent card snapshots for workspace members\n- `ontology/` — workspace-level ontology snapshots\n",
                slug
            );
            fs::write(path.join("README.md"), &readme)?;

            // Initial commit
            {
                let sig = Signature::now(&self.config.author_name, &self.config.author_email)?;
                let mut index = repo.index()?;
                index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
                index.write()?;
                let tree_oid = index.write_tree()?;
                let tree = repo.find_tree(tree_oid)?;
                repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &format!("workspace({}): initial structure", slug),
                    &tree,
                    &[],
                )?;
            }

            repo
        };

        Ok(repo)
    }

    /// Commit a file to the workspace repository.
    /// Creates parent directories as needed. Returns the commit info.
    pub fn commit_file(
        &self,
        slug: &str,
        file_path: &str,
        content: &str,
        message: &str,
    ) -> Result<WorkspaceCommit> {
        let repo = self.init_or_open(slug)?;
        let root = self.repo_path(slug);

        // Create parent directories if needed
        let full_path = root.join(file_path);
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Write file
        fs::write(&full_path, content)?;

        // Stage all
        let mut index = repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        // Get parent commit
        let parent = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        // Check for changes
        if let Some(ref p) = parent {
            if p.tree()?.id() == tree_oid {
                return Ok(WorkspaceCommit {
                    sha: p.id().to_string(),
                    message: p.message().unwrap_or("").to_string(),
                    timestamp: Utc::now(),
                    author: self.config.author_name.clone(),
                });
            }
        }

        let sig = Signature::now(&self.config.author_name, &self.config.author_email)?;
        let oid = if let Some(p) = parent {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&p])?
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])?
        };

        let sha = oid.to_string();
        info!(
            "Workspace {}: committed {} ({})",
            slug,
            file_path,
            &sha[..8]
        );

        // Auto-push
        if self.config.auto_push {
            if let Err(e) = self.push(&repo, slug) {
                error!("Failed to push workspace {}: {}", slug, e);
            }
        }

        Ok(WorkspaceCommit {
            sha,
            message: message.to_string(),
            timestamp: Utc::now(),
            author: self.config.author_name.clone(),
        })
    }

    /// Async wrapper for `commit_file` — moves blocking FS/git work to
    /// Tokio's blocking thread pool (Spec 21 Phase 4.2).
    /// Use this in async HTTP handlers; use `commit_file` only in sync contexts
    /// (spawn_blocking closures, tests).
    pub async fn commit_file_async(
        &self,
        slug: String,
        file_path: String,
        content: String,
        message: String,
    ) -> Result<WorkspaceCommit> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.commit_file(&slug, &file_path, &content, &message)
        })
        .await
        .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

    /// Commit binary data to the workspace repository.
    /// Like `commit_file` but takes raw bytes instead of a string.
    pub fn commit_file_bytes(
        &self,
        slug: &str,
        file_path: &str,
        content: &[u8],
        message: &str,
    ) -> Result<WorkspaceCommit> {
        let repo = self.init_or_open(slug)?;
        let root = self.repo_path(slug);

        // Create parent directories if needed
        let full_path = root.join(file_path);
        if let Some(parent) = full_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        // Write binary file
        fs::write(&full_path, content)?;

        // Stage all
        let mut index = repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        // Get parent commit
        let parent = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        // Check for changes
        if let Some(ref p) = parent {
            if p.tree()?.id() == tree_oid {
                return Ok(WorkspaceCommit {
                    sha: p.id().to_string(),
                    message: p.message().unwrap_or("").to_string(),
                    timestamp: Utc::now(),
                    author: self.config.author_name.clone(),
                });
            }
        }

        let sig = Signature::now(&self.config.author_name, &self.config.author_email)?;
        let oid = if let Some(p) = parent {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&p])?
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])?
        };

        let sha = oid.to_string();
        info!(
            "Workspace {}: committed binary {} ({}, {} bytes)",
            slug,
            file_path,
            &sha[..8],
            content.len()
        );

        // Auto-push
        if self.config.auto_push {
            if let Err(e) = self.push(&repo, slug) {
                error!("Failed to push workspace {}: {}", slug, e);
            }
        }

        Ok(WorkspaceCommit {
            sha,
            message: message.to_string(),
            timestamp: Utc::now(),
            author: self.config.author_name.clone(),
        })
    }

    /// Read raw bytes from a file in the workspace repository at HEAD.
    pub fn read_file_bytes(&self, slug: &str, file_path: &str) -> Result<Vec<u8>> {
        let repo = self.init_or_open(slug)?;
        let head = repo.head().map_err(|_| {
            OntologyError::RepoNotFound(format!("No commits in workspace {}", slug))
        })?;
        let commit = head.peel_to_commit()?;
        let tree = commit.tree()?;

        let entry = tree
            .get_path(std::path::Path::new(file_path))
            .map_err(|_| {
                OntologyError::RepoNotFound(format!(
                    "File not found: {} in workspace {}",
                    file_path, slug
                ))
            })?;

        let blob = entry
            .to_object(&repo)?
            .into_blob()
            .map_err(|_| OntologyError::RepoNotFound("Not a blob".to_string()))?;

        Ok(blob.content().to_vec())
    }

    /// List files in the workspace repository at HEAD.
    /// If `subdir` is Some, lists only files under that directory.
    pub fn list_files(&self, slug: &str, subdir: Option<&str>) -> Result<Vec<FileEntry>> {
        let repo = self.init_or_open(slug)?;
        let head = match repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(Vec::new()),
        };
        let commit = head.peel_to_commit()?;
        let tree = commit.tree()?;

        let mut entries = Vec::new();
        let prefix = subdir.unwrap_or("");

        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            let path = if dir.is_empty() {
                entry.name().unwrap_or("").to_string()
            } else {
                format!("{}{}", dir, entry.name().unwrap_or(""))
            };

            // Skip .gitkeep files
            if path.ends_with(".gitkeep") {
                return git2::TreeWalkResult::Ok;
            }

            // Filter by prefix
            if !prefix.is_empty() && !path.starts_with(prefix) {
                return git2::TreeWalkResult::Ok;
            }

            let is_dir = entry.kind() == Some(git2::ObjectType::Tree);
            let size = if !is_dir {
                entry
                    .to_object(&repo)
                    .ok()
                    .and_then(|obj| obj.as_blob().map(|b| b.size() as u64))
                    .unwrap_or(0)
            } else {
                0
            };

            let name = entry.name().unwrap_or("").to_string();
            entries.push(FileEntry {
                path,
                name,
                is_dir,
                size,
            });

            git2::TreeWalkResult::Ok
        })?;

        Ok(entries)
    }

    /// Read a file from the workspace repository at HEAD.
    pub fn read_file(&self, slug: &str, file_path: &str) -> Result<String> {
        let repo = self.init_or_open(slug)?;
        let head = repo.head().map_err(|_| {
            OntologyError::RepoNotFound(format!("No commits in workspace {}", slug))
        })?;
        let commit = head.peel_to_commit()?;
        let tree = commit.tree()?;

        let entry = tree
            .get_path(std::path::Path::new(file_path))
            .map_err(|_| {
                OntologyError::RepoNotFound(format!(
                    "File not found: {} in workspace {}",
                    file_path, slug
                ))
            })?;

        let blob = entry
            .to_object(&repo)?
            .into_blob()
            .map_err(|_| OntologyError::RepoNotFound("Not a blob".to_string()))?;

        String::from_utf8(blob.content().to_vec())
            .map_err(|e| OntologyError::RepoNotFound(format!("UTF-8 error: {}", e)))
    }

    /// Get the commit log for a workspace.
    pub fn get_log(&self, slug: &str, limit: usize) -> Result<Vec<WorkspaceCommit>> {
        let repo = self.init_or_open(slug)?;
        let head = match repo.head() {
            Ok(h) => h,
            Err(_) => return Ok(Vec::new()),
        };

        let oid = match head.target() {
            Some(oid) => oid,
            None => return Ok(Vec::new()),
        };

        let mut revwalk = repo.revwalk()?;
        revwalk.push(oid)?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut commits = Vec::new();
        for (i, rev) in revwalk.enumerate() {
            if i >= limit {
                break;
            }
            let oid = rev?;
            let commit = repo.find_commit(oid)?;
            let time = commit.time();
            let ts = DateTime::from_timestamp(time.seconds(), 0).unwrap_or_else(Utc::now);

            commits.push(WorkspaceCommit {
                sha: commit.id().to_string(),
                message: commit.message().unwrap_or("").to_string(),
                timestamp: ts,
                author: commit.author().name().unwrap_or("unknown").to_string(),
            });
        }

        Ok(commits)
    }

    /// Get a diff between two commits as a string.
    pub fn diff_commits(&self, slug: &str, from_sha: &str, to_sha: &str) -> Result<String> {
        let repo = self.init_or_open(slug)?;

        let from_oid = git2::Oid::from_str(from_sha)
            .map_err(|e| OntologyError::RepoNotFound(format!("Invalid SHA {}: {}", from_sha, e)))?;
        let to_oid = git2::Oid::from_str(to_sha)
            .map_err(|e| OntologyError::RepoNotFound(format!("Invalid SHA {}: {}", to_sha, e)))?;

        let from_commit = repo.find_commit(from_oid)?;
        let to_commit = repo.find_commit(to_oid)?;

        let from_tree = from_commit.tree()?;
        let to_tree = to_commit.tree()?;

        let diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), None)?;

        let mut output = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                _ => "",
            };
            output.push_str(prefix);
            if let Ok(content) = std::str::from_utf8(line.content()) {
                output.push_str(content);
            }
            true
        })?;

        Ok(output)
    }

    /// Push workspace repository to GitHub.
    fn push(&self, repo: &Repository, slug: &str) -> Result<bool> {
        let mut remote = match repo.find_remote(&self.config.remote_name) {
            Ok(r) => r,
            Err(_) => {
                if let Some(url) = self.github_remote_url(slug) {
                    repo.remote(&self.config.remote_name, &url)?
                } else {
                    return Ok(false);
                }
            }
        };

        let mut callbacks = RemoteCallbacks::new();
        if let Some(token) = &self.config.github_token {
            let t = token.clone();
            callbacks.credentials(move |_url, _user, _allowed| {
                Cred::userpass_plaintext("x-access-token", &t)
            });
        }

        let mut opts = PushOptions::new();
        opts.remote_callbacks(callbacks);

        let refspec = format!(
            "refs/heads/{}:refs/heads/{}",
            self.config.branch, self.config.branch
        );
        match remote.push(&[&refspec], Some(&mut opts)) {
            Ok(_) => {
                info!("Pushed workspace {} to GitHub", slug);
                Ok(true)
            }
            Err(e) => {
                error!("Failed to push workspace {}: {}", slug, e);
                Err(e.into())
            }
        }
    }

    /// Check if a workspace repo exists.
    pub fn repo_exists(&self, slug: &str) -> bool {
        self.repo_path(slug).join(".git").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(base: &str) -> GitConfig {
        GitConfig {
            base_path: base.to_string(),
            author_name: "Test".to_string(),
            author_email: "test@test.com".to_string(),
            branch: "main".to_string(),
            github_org: None,
            github_token: None,
            auto_push: false,
            remote_name: "origin".to_string(),
        }
    }

    #[test]
    fn test_init_workspace_repo() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceGitManager::new(test_config(tmp.path().to_str().unwrap())).unwrap();

        let repo = mgr.init_or_open("test-ws").unwrap();
        assert!(tmp.path().join("workspaces/test-ws/.git").exists());
        assert!(repo.head().is_ok()); // Should have initial commit
    }

    #[test]
    fn test_commit_and_read_file() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceGitManager::new(test_config(tmp.path().to_str().unwrap())).unwrap();

        mgr.commit_file(
            "test-ws",
            "context/brief.md",
            "# Project Brief\n\nWe analyze markets.",
            "Add project brief",
        )
        .unwrap();

        let content = mgr.read_file("test-ws", "context/brief.md").unwrap();
        assert!(content.contains("Project Brief"));
    }

    #[test]
    fn test_list_files() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceGitManager::new(test_config(tmp.path().to_str().unwrap())).unwrap();

        mgr.commit_file("test-ws", "context/brief.md", "brief", "Add brief")
            .unwrap();
        mgr.commit_file("test-ws", "outputs/result.json", "{}", "Add result")
            .unwrap();

        let files = mgr.list_files("test-ws", None).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"context/brief.md"));
        assert!(paths.contains(&"outputs/result.json"));
        assert!(paths.contains(&"README.md"));
    }

    #[test]
    fn test_git_log() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceGitManager::new(test_config(tmp.path().to_str().unwrap())).unwrap();

        mgr.commit_file("test-ws", "context/a.md", "a", "First file")
            .unwrap();
        mgr.commit_file("test-ws", "context/b.md", "b", "Second file")
            .unwrap();

        let log = mgr.get_log("test-ws", 10).unwrap();
        // Initial commit + 2 file commits
        assert!(log.len() >= 2);
        assert_eq!(log[0].message, "Second file");
    }

    #[test]
    fn test_diff() {
        let tmp = TempDir::new().unwrap();
        let mgr = WorkspaceGitManager::new(test_config(tmp.path().to_str().unwrap())).unwrap();

        mgr.commit_file("test-ws", "context/data.txt", "version 1", "v1")
            .unwrap();
        let log1 = mgr.get_log("test-ws", 1).unwrap();

        mgr.commit_file("test-ws", "context/data.txt", "version 2", "v2")
            .unwrap();
        let log2 = mgr.get_log("test-ws", 1).unwrap();

        let diff = mgr
            .diff_commits("test-ws", &log1[0].sha, &log2[0].sha)
            .unwrap();
        assert!(diff.contains("-version 1"));
        assert!(diff.contains("+version 2"));
    }
}
