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
//! 2. On app launch, list the public `releases` API and pick the
//!    highest *version* (see [`check_latest`] for why not
//!    `releases/latest`).
//! 3. If that tag is newer than `env!("CARGO_PKG_VERSION")`, surface a
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

/// How many releases to consider. GitHub returns newest-published
/// first; 30 is the API default page size and is many months of our
/// release cadence, so the highest version is certainly within it.
const RELEASE_PAGE_SIZE: u32 = 30;

/// Query GitHub and return the best available release iff it is
/// strictly newer than `current_version`. Returns `Ok(None)` in the
/// common case (no update available) so the UI can `if let Some(_)`.
///
/// # Why we list releases instead of using `releases/latest`
///
/// `GET /releases/latest` does **not** mean "highest version" — it
/// returns whichever non-prerelease release was *published most
/// recently*. Those differ whenever releases don't publish in version
/// order, which happens routinely:
///
/// * Pushing several tags together (`git push origin v1 v2 v3`) starts
///   concurrent CI runs. Whichever build finishes last owns the
///   pointer. This bit us for real on v0.10.15–17: v0.10.17's build was
///   fastest and published at 13:44:29, v0.10.16's finished 84 seconds
///   later at 13:45:53, so every client was told v0.10.16 was current
///   and the newest release was invisible.
/// * Back-porting a fix to an older line after a newer line shipped.
/// * Re-running a failed older release workflow.
///
/// Trusting the pointer therefore risks both missing an upgrade and
/// advertising a *downgrade*. Listing and choosing by version makes the
/// answer independent of publish timing.
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

    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page={}",
        repo(),
        RELEASE_PAGE_SIZE
    );
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

    let releases: Vec<GhRelease> = resp
        .json()
        .await
        .context("failed to parse release list JSON")?;
    log::debug!("[updater] {} release(s) returned", releases.len());

    let Some(best) = pick_best_release(&releases, current_version) else {
        log::debug!("[updater] up to date (current={})", current_version);
        return Ok(None);
    };

    // `pick_best_release` only returns candidates that already have the
    // asset, so this lookup cannot fail — but resolve it by name rather
    // than smuggling an index out of the selector.
    let asset = best
        .assets
        .iter()
        .find(|a| a.name == BINARY_ASSET_NAME)
        .ok_or_else(|| {
            anyhow!(
                "release {} has no {} asset — check the release workflow",
                best.tag_name,
                BINARY_ASSET_NAME
            )
        })?;

    log::info!(
        "[updater] update available: {} (current {})",
        best.tag_name,
        current_version
    );

    Ok(Some(ReleaseInfo {
        version: strip_v(&best.tag_name),
        tag: best.tag_name.clone(),
        notes: best
            .body
            .clone()
            .unwrap_or_else(|| "(no release notes)".to_string()),
        published_at: best.published_at.clone().unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
        size_bytes: asset.size,
    }))
}

/// Strip a leading `v` from a tag to get a bare version.
fn strip_v(tag: &str) -> String {
    tag.trim_start_matches('v').to_string()
}

/// Choose the highest-version installable release that beats
/// `current_version`, or `None` if we're already current.
///
/// Skips, in order:
///   * drafts and pre-releases — not for testers;
///   * releases missing [`BINARY_ASSET_NAME`] — a partially-failed
///     workflow publishes the release before uploading assets, and
///     offering an update we can't download is worse than offering
///     none. Falling through to the next-highest keeps the updater
///     working while a broken release sits at the top.
///
/// Pure and total over its input so it can be tested without network.
fn pick_best_release<'a>(
    releases: &'a [GhRelease],
    current_version: &str,
) -> Option<&'a GhRelease> {
    releases
        .iter()
        .filter(|r| {
            if r.draft || r.prerelease {
                return false;
            }
            if !r.assets.iter().any(|a| a.name == BINARY_ASSET_NAME) {
                log::debug!(
                    "[updater] skipping {} — no {} asset",
                    r.tag_name,
                    BINARY_ASSET_NAME
                );
                return false;
            }
            is_newer(&strip_v(&r.tag_name), current_version)
        })
        // Max by version, not by position: the list arrives ordered by
        // publish time, which is precisely the ordering we can't trust.
        .max_by(|a, b| {
            let (a_core, a_pre) = split_semver(&strip_v(&a.tag_name));
            let (b_core, b_pre) = split_semver(&strip_v(&b.tag_name));
            // Release sorts above pre-release at equal cores, matching
            // `is_newer`'s semantics: empty suffix wins.
            a_core
                .cmp(&b_core)
                .then_with(|| a_pre.is_empty().cmp(&b_pre.is_empty()))
        })
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

// ─── Release-note formatting ──────────────────────────────────────────

/// Longest release body the update modal renders inline, in **characters**.
pub const RELEASE_NOTES_MAX_CHARS: usize = 2400;

/// Clip a GitHub release body down to something the update modal can display
/// without growing past the window, appending a pointer to the full notes.
///
/// Truncation is char-aware by necessity. The previous implementation sliced
/// by byte index (`&notes[..2400]`) and panicked with "end byte index 2400 is
/// not a char boundary" whenever the cutoff landed inside a multibyte
/// codepoint. Release notes are dense with em-dashes, smart quotes and
/// box-drawing characters, so that panic fired on essentially every upgrade
/// attempt.
pub fn truncate_release_notes(notes: &str) -> String {
    if notes.chars().count() <= RELEASE_NOTES_MAX_CHARS {
        return notes.to_string();
    }
    let head: String = notes.chars().take(RELEASE_NOTES_MAX_CHARS).collect();
    format!("{head}…\n\n(truncated — see GitHub for full notes)")
}

// ─── Tests ────────────────────────────────────────────────────────

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

    #[test]
    fn numeric_not_lexicographic_ordering() {
        // The classic string-compare bug: "0.10.0" < "0.9.0" as text.
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.9.0", "0.10.0"));
        assert!(is_newer("0.10.17", "0.10.9"));
    }

    // ── truncate_release_notes ───────────────────────────────────

    #[test]
    fn short_notes_pass_through_untouched() {
        let notes = "### Fixes\n• Fixed the thing — properly this time.";
        assert_eq!(truncate_release_notes(notes), notes);
    }

    #[test]
    fn notes_exactly_at_the_limit_are_not_truncated() {
        let notes = "—".repeat(RELEASE_NOTES_MAX_CHARS);
        assert_eq!(truncate_release_notes(&notes), notes);
    }

    /// Regression: the cutoff must never land inside a multibyte codepoint.
    /// This previously panicked with "end byte index 2400 is not a char
    /// boundary; it is inside '—'" on every upgrade attempt.
    #[test]
    fn truncation_never_splits_a_codepoint() {
        // Sweep every alignment of a 3-byte codepoint across the boundary;
        // a byte-indexed implementation panics for most of these.
        for pad in 0..8 {
            let mut notes = "a".repeat(pad);
            notes.push_str(&"— dash ".repeat(1200));
            let out = truncate_release_notes(&notes);
            assert!(out.ends_with("(truncated — see GitHub for full notes)"));
            // The retained head is the limit in *chars*, not bytes.
            let head = out.split('…').next().unwrap();
            assert_eq!(head.chars().count(), RELEASE_NOTES_MAX_CHARS);
        }
    }

    #[test]
    fn truncation_handles_4_byte_codepoints() {
        // Emoji are 4 bytes each, so byte 2400 lands mid-codepoint here too.
        let notes = "🎉".repeat(RELEASE_NOTES_MAX_CHARS + 50);
        let out = truncate_release_notes(&notes);
        let head = out.split('…').next().unwrap();
        assert_eq!(head.chars().count(), RELEASE_NOTES_MAX_CHARS);
    }

    // ── pick_best_release ────────────────────────────────────────

    /// Build a release carrying the platform asset the updater needs.
    fn rel(tag: &str) -> GhRelease {
        GhRelease {
            tag_name: tag.to_string(),
            body: Some(format!("notes for {}", tag)),
            published_at: Some("2026-08-01T00:00:00Z".to_string()),
            prerelease: false,
            draft: false,
            assets: vec![GhAsset {
                name: BINARY_ASSET_NAME.to_string(),
                browser_download_url: format!("https://example.test/{}/bin", tag),
                size: 42_000_000,
            }],
        }
    }

    /// A release whose workflow published the release but never
    /// uploaded the binary.
    fn rel_without_asset(tag: &str) -> GhRelease {
        GhRelease {
            assets: vec![],
            ..rel(tag)
        }
    }

    #[test]
    fn picks_highest_version_regardless_of_publish_order() {
        // This is the exact production incident that motivated the fix:
        // v0.10.15/16/17 were tagged together, v0.10.17's build finished
        // FIRST, so GitHub listed v0.10.16 as most-recently-published and
        // `/releases/latest` returned it. The list arrives newest-published
        // first, which is this order.
        let releases = vec![rel("v0.10.16"), rel("v0.10.15"), rel("v0.10.17")];
        let best = pick_best_release(&releases, "0.10.14").expect("an update exists");
        assert_eq!(
            best.tag_name, "v0.10.17",
            "must choose by version, not by publish position"
        );
    }

    #[test]
    fn does_not_offer_a_downgrade() {
        // Client already on the highest version: a stale "latest"
        // pointer at v0.10.16 must not drag it backwards.
        let releases = vec![rel("v0.10.16"), rel("v0.10.17")];
        assert!(pick_best_release(&releases, "0.10.17").is_none());
    }

    #[test]
    fn back_ported_patch_does_not_mask_newer_line() {
        // A 0.9.x fix published today, long after 0.10.x shipped.
        // `/releases/latest` would return v0.9.6; we must not.
        let releases = vec![rel("v0.9.6"), rel("v0.10.2"), rel("v0.9.5")];
        let best = pick_best_release(&releases, "0.9.5").expect("an update exists");
        assert_eq!(best.tag_name, "v0.10.2");
    }

    #[test]
    fn skips_releases_without_the_platform_asset() {
        // Top release exists but its workflow died before uploading the
        // binary. Offering it would hand the user an undownloadable
        // update; fall through to the newest one we can actually install.
        let releases = vec![rel_without_asset("v0.10.18"), rel("v0.10.17")];
        let best = pick_best_release(&releases, "0.10.14").expect("an installable update exists");
        assert_eq!(best.tag_name, "v0.10.17");
    }

    #[test]
    fn skips_drafts_and_prereleases() {
        let mut draft = rel("v0.11.0");
        draft.draft = true;
        let mut pre = rel("v0.10.20");
        pre.prerelease = true;
        let releases = vec![draft, pre, rel("v0.10.17")];
        let best = pick_best_release(&releases, "0.10.14").expect("a stable update exists");
        assert_eq!(best.tag_name, "v0.10.17");
    }

    #[test]
    fn empty_release_list_is_not_an_error() {
        assert!(pick_best_release(&[], "0.10.14").is_none());
    }

    #[test]
    fn all_releases_unusable_yields_none() {
        // Nothing installable and nothing newer — must not panic or
        // fabricate a candidate.
        let releases = vec![rel_without_asset("v0.10.18"), rel("v0.10.1")];
        assert!(pick_best_release(&releases, "0.10.17").is_none());
    }

    #[test]
    fn selected_release_carries_its_own_notes_and_url() {
        // Guards against an index/borrow mix-up handing back one
        // release's metadata attached to another's version.
        let releases = vec![rel("v0.10.16"), rel("v0.10.17"), rel("v0.10.15")];
        let best = pick_best_release(&releases, "0.10.14").unwrap();
        assert_eq!(best.tag_name, "v0.10.17");
        assert_eq!(best.body.as_deref(), Some("notes for v0.10.17"));
        assert_eq!(
            best.assets[0].browser_download_url,
            "https://example.test/v0.10.17/bin"
        );
        assert_eq!(strip_v(&best.tag_name), "0.10.17");
    }
}
