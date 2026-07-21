//! Self-updater — phones home to GitHub Releases and offers to install
//! newer builds in-place.
//!
//! # Why this shape
//!
//! Distributing a rapidly-iterating Rust GUI to remote testers has one
//! non-negotiable requirement: **testers must never have to think about
//! version drift.** The pattern that solves this without standing up
//! any distribution infrastructure of our own is:
//!
//! 1. Publish releases on GitHub with a predictable asset name.
//! 2. On app launch, query the public `releases/latest` API.
//! 3. If the tag is newer than `env!("CARGO_PKG_VERSION")`, surface a
//!    non-blocking "Update available" affordance with release notes
//!    pulled straight from the release body.
//! 4. On confirm: download the fresh binary next to `current_exe()`,
//!    `rename(2)` over it (atomic on POSIX because the kernel keeps
//!    the running inode alive), then spawn a new process and exit.
//!
//! This is exactly what `rustup`, `zoxide`, `bat`, `sccache`, and Zed
//! itself do. It costs zero infra beyond GitHub Releases, which we
//! already have.
//!
//! # Non-goals for this iteration
//!
//! - **Signed updates** — we rely on TLS + GitHub's org auth for now.
//!   The `checksums.txt` published alongside the release is available
//!   for out-of-band verification but we don't gate installs on it.
//!   When we ship to non-employee users the natural upgrade is a
//!   minisign or cosign signature over the tarball.
//! - **Delta updates** — the binary is ~40 MB stripped; full download
//!   is fine on the release cadence we're targeting (weekly-ish).
//! - **Windows** — the workflow only builds Linux x86_64 today. macOS
//!   would work with a build matrix change; Windows would additionally
//!   need `self_replace` since you can't `rename` over a running .exe.
//!
//! # Config
//!
//! - `FERMI_UPDATE_REPO` env var overrides `Replicant-Partners/fermi`
//!   for local staging tests.
//! - `FERMI_DISABLE_UPDATE_CHECK=1` skips the network call entirely
//!   (useful for offline demos).

use anyhow::{anyhow, bail, Context as _, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

/// Repository owner/name. Override via `FERMI_UPDATE_REPO` for testing
/// against a fork or a staging release channel.
const DEFAULT_REPO: &str = "Replicant-Partners/fermi";

/// Asset name uploaded by `.github/workflows/release-console.yml`. Kept
/// in sync with the workflow's "Package binary" step — if you rename
/// one, rename the other.
#[cfg(target_os = "linux")]
const BINARY_ASSET_NAME: &str = "fermi-console-linux-x86_64";

// Non-Linux platforms have no pre-built release today; the updater is a
// no-op there (`check_latest` returns Ok(None)). Contributors on macOS
// build from source.
#[cfg(not(target_os = "linux"))]
const BINARY_ASSET_NAME: &str = "unsupported";

/// A GitHub release we've decided the user should upgrade to. Cheaply
/// cloneable because the UI stashes it in a struct field and consumes
/// it later on Confirm.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    /// Bare version like "0.8.1" — the tag with any leading `v` stripped.
    pub version: String,
    /// The GitHub tag as-is (e.g. "v0.8.1"), for display.
    pub tag: String,
    /// Release body markdown. Rendered as-is (no HTML parsing) into the
    /// modal — GitHub's release body is authored as markdown so this
    /// looks fine even without a renderer.
    pub notes: String,
    /// ISO-8601 timestamp for the "published Xh ago" subtitle.
    pub published_at: String,
    /// Direct download URL for the bare binary asset.
    pub download_url: String,
    /// Size in bytes, for the download progress bar.
    pub size_bytes: u64,
}

/// State machine for the in-progress download. Owned by the UI, mutated
/// from the download task via `cx.update`.
#[derive(Debug, Clone)]
pub enum DownloadState {
    /// Modal is open but user hasn't clicked "Update & Restart" yet.
    Idle,
    /// Bytes downloaded / total bytes. `total == 0` means Content-Length
    /// was missing — the UI shows an indeterminate spinner.
    Downloading { received: u64, total: u64 },
    /// Binary is on disk, we're about to swap it in.
    Installing,
    /// Swap succeeded, we're about to exec the new binary.
    Restarting,
    /// Something went wrong. String is user-visible.
    Failed(String),
}

impl Default for DownloadState {
    fn default() -> Self {
        DownloadState::Idle
    }
}

// ─── GitHub API DTOs ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

// ─── Public API ───────────────────────────────────────────────────────

/// Resolve the target repo, honouring `FERMI_UPDATE_REPO`.
fn repo() -> String {
    std::env::var("FERMI_UPDATE_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string())
}

/// True when the update mechanism is disabled by env var or platform.
pub fn is_disabled() -> bool {
    if std::env::var("FERMI_DISABLE_UPDATE_CHECK").is_ok() {
        return true;
    }
    // Only Linux has pre-built binaries in the workflow today.
    !cfg!(target_os = "linux")
}

/// Query GitHub for the latest release and return it iff the tag is
/// strictly newer than `current_version`. Returns `Ok(None)` in the
/// common case (no update available) so the UI can `if let Some(_)`.
///
/// Deliberately silent on transient network failure — a tester on a
/// bad Wi-Fi shouldn't see a scary "update check failed" toast every
/// launch. Real errors bubble up so the "Check for Updates…" menu item
/// can surface them when the user explicitly asked.
pub async fn check_latest(current_version: &str) -> Result<Option<ReleaseInfo>> {
    if is_disabled() {
        log::debug!("[updater] disabled — skipping check");
        return Ok(None);
    }

    let url = format!("https://api.github.com/repos/{}/releases/latest", repo());
    log::debug!("[updater] GET {}", url);

    let client = reqwest::Client::builder()
        .user_agent(format!("fermi-console/{}", current_version))
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to query GitHub releases")?;

    if !resp.status().is_success() {
        // 404 is normal when no release exists yet — treat as "no update".
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        bail!("GitHub releases API returned {}", resp.status());
    }

    let release: GhRelease = resp.json().await.context("failed to parse release JSON")?;

    if release.draft {
        log::debug!("[updater] latest release is a draft — ignoring");
        return Ok(None);
    }

    let version = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer(&version, current_version) {
        log::debug!(
            "[updater] up to date (latest={}, current={})",
            version,
            current_version
        );
        return Ok(None);
    }

    // Find the bare-binary asset (the one the install script uses).
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == BINARY_ASSET_NAME)
        .ok_or_else(|| {
            anyhow!(
                "release {} has no {} asset — check the release workflow",
                release.tag_name,
                BINARY_ASSET_NAME
            )
        })?;

    Ok(Some(ReleaseInfo {
        version,
        tag: release.tag_name.clone(),
        notes: release
            .body
            .unwrap_or_else(|| "(no release notes)".to_string()),
        published_at: release.published_at.unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        size_bytes: asset.size,
    }))
}

/// Naive `x.y.z[-suffix]` comparison. We only care about strict > for
/// the "is a newer version out?" question, so this is fine.
///
/// Pre-release suffixes like `-dev` sort *lower* than the base version
/// (matching semver semantics), so a `0.7.0-dev` client will correctly
/// prompt to upgrade to a released `0.7.0`.
fn is_newer(candidate: &str, current: &str) -> bool {
    let (cand_core, cand_pre) = split_semver(candidate);
    let (cur_core, cur_pre) = split_semver(current);

    match cand_core.cmp(&cur_core) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => {
            // Same numeric version; pre-release loses to release.
            // "0.7.0" > "0.7.0-dev"; "0.7.0-dev" is not > "0.7.0".
            match (cand_pre.is_empty(), cur_pre.is_empty()) {
                (true, false) => true,  // release beats pre-release
                (false, true) => false, // pre-release doesn't beat release
                _ => false,             // don't nag inside same pre-release channel
            }
        }
    }
}

fn split_semver(v: &str) -> ([u32; 3], String) {
    let (core, pre) = match v.find('-') {
        Some(i) => (&v[..i], v[i + 1..].to_string()),
        None => (v, String::new()),
    };
    let mut parts = [0u32; 3];
    for (i, piece) in core.splitn(3, '.').enumerate().take(3) {
        parts[i] = piece.parse().unwrap_or(0);
    }
    (parts, pre)
}

/// Callback fired periodically during download so the UI can update
/// its progress bar. `received` and `total` are in bytes; `total == 0`
/// means the server didn't send Content-Length.
pub type ProgressFn = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Download the new binary to a sibling temp path next to
/// `std::env::current_exe()`, verify it's non-empty and executable,
/// and atomically rename over the current binary.
///
/// Returns the path of the freshly-installed binary (same as
/// `current_exe`) on success. The caller is responsible for
/// re-executing it — see [`restart`].
///
/// Safe to call while the old binary is running: on Linux (and macOS),
/// `rename(2)` swaps the directory entry while the kernel keeps the
/// running inode alive, so the current process keeps functioning
/// until it decides to exec the new one.
pub async fn download_and_install(
    release: &ReleaseInfo,
    on_progress: ProgressFn,
) -> Result<PathBuf> {
    let current_exe =
        std::env::current_exe().context("cannot resolve current_exe() for self-update")?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| anyhow!("current_exe has no parent directory"))?;

    // Sibling temp path — same filesystem guarantees atomic rename.
    let temp_path = exe_dir.join(format!(".{}.new", file_name(&current_exe)));
    if temp_path.exists() {
        let _ = std::fs::remove_file(&temp_path);
    }

    // Also stash a backup of the current binary. If the new one is bad
    // we can restore. Overwritten on each update — testers only ever
    // roll back exactly one version, which matches how they think.
    let backup_path = exe_dir.join(format!(".{}.old", file_name(&current_exe)));

    log::info!(
        "[updater] downloading {} → {}",
        release.download_url,
        temp_path.display()
    );

    let client = reqwest::Client::builder()
        .user_agent(format!("fermi-console/{}", env!("CARGO_PKG_VERSION")))
        // No timeout — downloads can be slow, we rely on progress
        // updates for the UI to stay responsive.
        .build()?;

    let mut resp = client
        .get(&release.download_url)
        .send()
        .await
        .context("failed to start download")?
        .error_for_status()
        .context("download returned error status")?;

    let total = resp.content_length().unwrap_or(release.size_bytes);

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .with_context(|| format!("failed to create temp file at {}", temp_path.display()))?;

    let mut received = 0u64;
    on_progress(0, total);

    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        on_progress(received, total);
    }
    file.flush().await?;
    drop(file);

    // Sanity check: refuse to install a zero-byte or absurdly-small file.
    let meta = std::fs::metadata(&temp_path)?;
    if meta.len() < 1024 * 1024 {
        let _ = std::fs::remove_file(&temp_path);
        bail!(
            "downloaded binary is suspiciously small ({} bytes) — refusing to install",
            meta.len()
        );
    }

    // chmod +x on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&temp_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&temp_path, perms)?;
    }

    // Snapshot the current binary as backup. Best-effort; failure here
    // is not fatal (we already have the new binary staged).
    if backup_path.exists() {
        let _ = std::fs::remove_file(&backup_path);
    }
    if let Err(e) = std::fs::rename(&current_exe, &backup_path) {
        log::warn!(
            "[updater] could not stash backup ({}); attempting overwrite anyway",
            e
        );
    }

    // Atomic swap: rename the temp into place.
    std::fs::rename(&temp_path, &current_exe)
        .with_context(|| format!("failed to install new binary to {}", current_exe.display()))?;

    log::info!(
        "[updater] installed {} at {}",
        release.tag,
        current_exe.display()
    );

    Ok(current_exe)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("fermi-console")
        .to_string()
}

/// Re-execute the just-installed binary and exit the current process.
///
/// On Unix we spawn a detached child with the same CLI args and env,
/// then `std::process::exit(0)` from the parent. This is preferred
/// over `execv` because GPUI has an active window + graphics context
/// that would leak into the new process image; a fresh process gets
/// a clean GPU context.
///
/// This function does not return on success.
pub fn restart(new_exe: &Path) -> Result<()> {
    log::info!("[updater] restarting via {}", new_exe.display());
    let mut cmd = std::process::Command::new(new_exe);
    cmd.args(std::env::args().skip(1));

    // We deliberately don't detach or fork here. The parent exits
    // immediately after spawn(), so the child is reparented to init
    // (PID 1) and inherits nothing from our GPUI graphics context.
    // That's exactly what we want — a clean process image.
    cmd.spawn().context("failed to spawn new binary")?;
    std::process::exit(0);
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_detected() {
        assert!(is_newer("0.8.0", "0.7.0"));
        assert!(is_newer("0.7.1", "0.7.0"));
        assert!(is_newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn same_or_older_rejected() {
        assert!(!is_newer("0.7.0", "0.7.0"));
        assert!(!is_newer("0.6.9", "0.7.0"));
        assert!(!is_newer("0.7.0", "0.7.1"));
    }

    #[test]
    fn release_beats_prerelease_at_same_core() {
        // Client on 0.7.0-dev should upgrade to 0.7.0.
        assert!(is_newer("0.7.0", "0.7.0-dev"));
        // Client on 0.7.0 should NOT downgrade to a 0.7.0-dev of the same core.
        assert!(!is_newer("0.7.0-dev", "0.7.0"));
    }

    #[test]
    fn v_prefix_and_leading_zeros() {
        let (parts, pre) = split_semver("v0.10.5-alpha.2");
        // Note: our splitter doesn't strip the 'v'; check_latest() does that.
        // Here we just verify the core numeric parts are handled robustly.
        // "v0" parses to 0 via unwrap_or(0).
        assert_eq!(parts, [0, 10, 5]);
        assert_eq!(pre, "alpha.2");
    }
}
