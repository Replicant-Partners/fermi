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

/// The human a commit is attributed to.
///
/// Exists because `commit_file` hardcodes the configured system signature,
/// which makes "which teammate changed this" unanswerable. Git already has
/// a first-class slot for this — we just weren't filling it.
#[derive(Debug, Clone)]
pub struct CommitAuthor {
    pub name: String,
    pub email: String,
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

    /// Delete the on-disk workspace repository. Used by the admin
    /// wipe-fermi-forecasts handler (Spec 23 demo cleanup).
    ///
    /// Returns `Ok(true)` if the directory existed and was deleted,
    /// `Ok(false)` if it didn't exist (idempotent — safe to call on a
    /// slug whose repo was already gone), `Err(_)` on I/O failure.
    ///
    /// **This is destructive and irrecoverable**: the entire git history
    /// for the workspace is removed. The caller is responsible for
    /// confirming intent (typically via an admin-only HTTP endpoint).
    pub fn delete_workspace_repo(&self, slug: &str) -> Result<bool> {
        let path = self.repo_path(slug);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_dir_all(&path).map_err(|e| {
            error!(slug, error = %e, "failed to delete workspace repo");
            OntologyError::IoError(e)
        })?;
        info!(slug, "workspace repo deleted");
        Ok(true)
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
        tokio::task::spawn_blocking(move || this.commit_file(&slug, &file_path, &content, &message))
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

    /// Read a file as it stood at a specific commit.
    ///
    /// The counterpart of [`Self::read_file`] (which reads HEAD) and the
    /// missing half of revert: `diff_commits` could already show what
    /// changed, but nothing could recover the earlier content, so "undo"
    /// was unimplementable.
    ///
    /// Returns `Ok(None)` when the path did not exist at that commit — a
    /// legitimate answer (the file was added later), distinct from a
    /// missing repo or a bad SHA, which are errors.
    pub fn read_file_at(&self, slug: &str, file_path: &str, sha: &str) -> Result<Option<String>> {
        let repo = self.init_or_open(slug)?;
        let oid = git2::Oid::from_str(sha)
            .map_err(|e| OntologyError::RepoNotFound(format!("Invalid SHA {}: {}", sha, e)))?;
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;

        let entry = match tree.get_path(std::path::Path::new(file_path)) {
            Ok(e) => e,
            // Absent at this revision, not an error.
            Err(_) => return Ok(None),
        };

        let blob = entry
            .to_object(&repo)?
            .into_blob()
            .map_err(|_| OntologyError::RepoNotFound("Not a blob".to_string()))?;

        String::from_utf8(blob.content().to_vec())
            .map(Some)
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
        // TOPOLOGICAL as well as TIME. Git timestamps have one-second
        // resolution, and these commits are written milliseconds apart — a
        // save followed by a cascade, or a repo's seeded `initial
        // structure` commit followed immediately by the first real one. On
        // a tie, TIME alone leaves the order undefined, so the History pane
        // that promises "newest first" could and did render a parent above
        // its own child. TOPOLOGICAL makes ancestry the tie-breaker, which
        // is what `git log` does by default.
        revwalk.set_sorting(git2::Sort::TIME | git2::Sort::TOPOLOGICAL)?;

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

    /// Get a diff between two revisions as a string.
    ///
    /// Both sides accept anything `git rev-parse` accepts — a full sha, an
    /// abbreviated one, `HEAD~2`, a branch name, or `<sha>^` for a parent.
    ///
    /// This used to be `Oid::from_str`, which parses *only* a full hex sha
    /// and rejects everything else. The forecast history UI asks for
    /// `<sha>^` (the natural spelling of "what did this commit change"),
    /// so every diff request it ever made failed with `unable to parse OID
    /// - too long`, and the pane rendered "No diff available" for every
    /// revision. Nothing was wrong with the history itself.
    pub fn diff_commits(&self, slug: &str, from_rev: &str, to_rev: &str) -> Result<String> {
        let repo = self.init_or_open(slug)?;
        let from_commit = Self::resolve_commit(&repo, from_rev)?;
        let to_commit = Self::resolve_commit(&repo, to_rev)?;
        Self::render_diff(&repo, Some(&from_commit), &to_commit)
    }

    /// Diff one commit against its parent — "what did this change?".
    ///
    /// Distinct from `diff_commits(slug, "<sha>^", "<sha>")` because the
    /// **root commit has no parent**, and asking for `^` on it is an error
    /// rather than an empty answer. Every forecast repo is seeded with an
    /// `initial structure` commit, so the root is a revision users can and
    /// do click on. Against no parent, the whole tree reads as added, which
    /// is exactly what that commit did.
    pub fn diff_commit_with_parent(&self, slug: &str, rev: &str) -> Result<String> {
        let repo = self.init_or_open(slug)?;
        let commit = Self::resolve_commit(&repo, rev)?;
        let parent = commit.parent(0).ok();
        Self::render_diff(&repo, parent.as_ref(), &commit)
    }

    /// Resolve any revision string to a commit.
    fn resolve_commit<'r>(repo: &'r Repository, rev: &str) -> Result<git2::Commit<'r>> {
        repo.revparse_single(rev)
            .and_then(|obj| obj.peel_to_commit())
            .map_err(|e| OntologyError::RepoNotFound(format!("Invalid revision {}: {}", rev, e)))
    }

    /// Render `from → to` as a unified diff. `from = None` diffs against
    /// the empty tree, which is how a root commit is shown.
    fn render_diff(
        repo: &Repository,
        from: Option<&git2::Commit<'_>>,
        to: &git2::Commit<'_>,
    ) -> Result<String> {
        let from_tree = match from {
            Some(c) => Some(c.tree()?),
            None => None,
        };
        let to_tree = to.tree()?;

        let diff = repo.diff_tree_to_tree(from_tree.as_ref(), Some(&to_tree), None)?;

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
    /// Commit a SET of files as ONE commit, attributed to a specific human.
    ///
    /// Two gaps in [`Self::commit_file`] that this closes, both of which
    /// make it unusable as a collaboration record:
    ///
    /// **1. Authorship.** `commit_file` hardcodes the configured system
    /// signature, so every commit is by the platform. "Which teammate made
    /// which change" is then unanswerable no matter how much we commit.
    ///
    /// **2. Atomicity.** One logical action — revising a driver — changes
    /// the generated program, the driver state, and the probability
    /// snapshot. Looping `commit_file` yields three commits for one act,
    /// which makes the log unreadable and the diffs meaningless. Here the
    /// whole set lands as one commit, so a commit == an action.
    ///
    /// Returns `Ok(None)` when the tree is unchanged. `commit_file`
    /// synthesises a fake `WorkspaceCommit` in that case, reporting the
    /// parent's SHA with a fresh `Utc::now()` timestamp — a commit that
    /// never happened, at a time it didn't happen. `None` lets callers skip
    /// the DB bookkeeping instead of recording a phantom revision.
    ///
    /// `author` falls back to the configured system identity when `None`,
    /// which is correct for genuinely systemic writes (cron, refits with no
    /// operator behind them).
    pub fn commit_files_as(
        &self,
        slug: &str,
        files: &[(String, String)],
        message: &str,
        author: Option<&CommitAuthor>,
    ) -> Result<Option<WorkspaceCommit>> {
        let repo = self.init_or_open(slug)?;
        let root = self.repo_path(slug);

        for (rel_path, content) in files {
            let full_path = root.join(rel_path);
            if let Some(parent) = full_path.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent)?;
                }
            }
            fs::write(&full_path, content)?;
        }

        let mut index = repo.index()?;
        index.add_all(["."].iter(), IndexAddOption::DEFAULT, None)?;
        index.write()?;

        let tree_oid = index.write_tree()?;
        let tree = repo.find_tree(tree_oid)?;

        let parent = repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .and_then(|oid| repo.find_commit(oid).ok());

        // Idempotence: re-committing identical state is a no-op, not an
        // empty commit. Matters because the commit hook fires on every
        // mutating request, including ones that change nothing.
        if let Some(ref p) = parent {
            if p.tree()?.id() == tree_oid {
                return Ok(None);
            }
        }

        let (name, email) = match author {
            Some(a) => (a.name.as_str(), a.email.as_str()),
            None => (
                self.config.author_name.as_str(),
                self.config.author_email.as_str(),
            ),
        };
        let sig = Signature::now(name, email)?;

        let oid = if let Some(p) = parent {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&p])?
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])?
        };

        let sha = oid.to_string();
        info!(
            "Workspace {}: committed {} file(s) as {} ({})",
            slug,
            files.len(),
            name,
            &sha[..8.min(sha.len())]
        );

        if self.config.auto_push {
            if let Err(e) = self.push(&repo, slug) {
                error!("Failed to push workspace {}: {}", slug, e);
            }
        }

        Ok(Some(WorkspaceCommit {
            sha,
            message: message.to_string(),
            timestamp: Utc::now(),
            author: name.to_string(),
        }))
    }

    /// Async wrapper for [`Self::commit_files_as`]. Use this from async HTTP
    /// handlers — git2 and the filesystem are blocking, and a forecast save
    /// is on the request path.
    pub async fn commit_files_as_async(
        &self,
        slug: String,
        files: Vec<(String, String)>,
        message: String,
        author: Option<CommitAuthor>,
    ) -> Result<Option<WorkspaceCommit>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || {
            this.commit_files_as(&slug, &files, &message, author.as_ref())
        })
        .await
        .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

    /// Async wrapper for [`Self::read_file_at`].
    pub async fn read_file_at_async(
        &self,
        slug: String,
        file_path: String,
        sha: String,
    ) -> Result<Option<String>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.read_file_at(&slug, &file_path, &sha))
            .await
            .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

    /// Async wrapper for [`Self::get_log`].
    pub async fn get_log_async(&self, slug: String, limit: usize) -> Result<Vec<WorkspaceCommit>> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.get_log(&slug, limit))
            .await
            .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

    /// Async wrapper for [`Self::diff_commits`].
    pub async fn diff_commits_async(
        &self,
        slug: String,
        from_sha: String,
        to_sha: String,
    ) -> Result<String> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.diff_commits(&slug, &from_sha, &to_sha))
            .await
            .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

    /// Async [`Self::diff_commit_with_parent`].
    pub async fn diff_commit_with_parent_async(&self, slug: String, rev: String) -> Result<String> {
        let this = self.clone();
        tokio::task::spawn_blocking(move || this.diff_commit_with_parent(&slug, &rev))
            .await
            .map_err(|e| OntologyError::ConfigError(format!("spawn_blocking failed: {}", e)))?
    }

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
