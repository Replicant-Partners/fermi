//! HTML page-serving handlers.

use axum::{
    extract::Query,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

// ─── Fallback (404) ────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn fallback_404() -> (StatusCode, Html<String>) {
    let html = std::fs::read_to_string("templates/404.html")
        .unwrap_or_else(|_| "<h1>404 — Not Found</h1>".to_string());
    (StatusCode::NOT_FOUND, Html(html))
}

// ─── Page routes ───────────────────────────────────────────────────

/// Landing page — serves Flutter web app for rabble.world, ABW landing for everything else
pub async fn landing(headers: axum::http::HeaderMap) -> Html<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    if host.starts_with("rabble.world") {
        // Serve Flutter web app if built, otherwise show coming soon
        if let Ok(content) = std::fs::read_to_string("static/rabble/index.html") {
            return Html(content);
        }
        return Html("<html><body style='background:#0D1B14;color:#F5F0E8;font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0'><div style='text-align:center'><h1>Rabble</h1><p style='color:#7A9A84'>Coming soon</p></div></body></html>".to_string());
    }

    let html = match std::fs::read_to_string("templates/landing.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/landing.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading landing template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn aspiration() -> Html<String> {
    let html = match std::fs::read_to_string("templates/aspiration.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/aspiration.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading aspiration template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn catalogue() -> Html<String> {
    println!("Catalogue route called");
    let html = match std::fs::read_to_string("templates/index.html") {
        Ok(content) => {
            println!(
                "Successfully loaded templates/index.html ({} bytes)",
                content.len()
            );
            content
        }
        Err(e) => {
            eprintln!("Error loading templates/index.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

// ─── Fermi Console installer landing (Bar A) ───────────────────
//
// `/fermi-console/install`    → friendly HTML page with a big Copy button.
// `/fermi-console/install.sh` → the actual bash script served as text/plain
//                                so `curl -fsSL .../install.sh | bash` works,
//                                AND cautious testers can preview it in a
//                                browser tab before running.
//
// The script is `include_str!`'d rather than read from disk at request
// time. Reason: the Railway Dockerfile deliberately doesn't COPY the
// `scripts/` tree into the runtime image, so a disk-based serve would
// 404 in production. Baking the script into the binary means the
// released api-server always serves the script that shipped with it.
// Single source of truth, no drift, no filesystem dependency.
const INSTALL_SCRIPT: &str = include_str!("../../scripts/install-fermi-console.sh");

/// Serve the friendly install landing page. All the client-side logic
/// (copy button, OS sniffing, host substitution) lives inside the
/// template; this handler just streams the file.
pub async fn install_page() -> Html<String> {
    let html = match std::fs::read_to_string("templates/install.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/install.html: {}", e);
            // Graceful fallback — if the template is missing we still
            // want the tester to be able to get moving, so surface the
            // raw one-liner.
            format!(
                "<h1>Install Fermi Console</h1>\n\
                 <p>Run this in a Linux terminal:</p>\n\
                 <pre>curl -fsSL https://raw.githubusercontent.com/Replicant-Partners/\
                 fermi/main/scripts/install-fermi-console.sh | bash</pre>\n\
                 <p>(template load failed: {})</p>",
                e
            )
        }
    };
    Html(html)
}

/// Serve the install script as `text/plain` so browsers show it as
/// source (letting cautious testers eyeball it) and `curl | bash`
/// treats it as a script.
///
/// We deliberately do NOT set `Content-Disposition: attachment` — the
/// happy path is piping into bash, not saving a file.
pub async fn install_script() -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    // Short cache: script changes are rare but we want fixes to
    // propagate quickly when they do land.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    (StatusCode::OK, headers, INSTALL_SCRIPT).into_response()
}

// ─── Download indirection ───────────────────────────────────────────────────────
//
// `GET /fermi-console/download` → 302 to the actual binary URL.
//
// The install script and the in-app updater both talk to *this* URL,
// never directly to GitHub. That lets us:
//
//   * ship testers a stable download URL even while the source repo
//     is private (`github.com/Replicant-Partners/fermi/releases/...`
//     returns 404 anonymously),
//   * swap the backend later (R2, Cloudflare, S3, self-hosted) without
//     touching a single tester's install,
//   * pin a version cohort by env var without redeploying binaries.
//
// Configuration: `FERMI_CONSOLE_DOWNLOAD_URL` env var, defaults to the
// GitHub Releases "latest" URL. Query params:
//
//   ?v=vX.Y.Z          version pin (in-app updater uses this)
//   ?platform=<slug>   which platform's binary to serve

/// Platform slugs the release workflow actually publishes a binary for.
///
/// This is a closed allowlist, not a passthrough, for two reasons. The
/// slug is interpolated into an outbound URL we then 302 to, so an
/// unvalidated value is an open-redirect primitive. And a typo'd slug
/// would otherwise redirect to a 404 asset, which surfaces to the tester
/// as a corrupt download rather than "you asked for something that
/// doesn't exist".
///
/// Kept in sync with `.github/workflows/release-console.yml` (matrix
/// `platform`), `crates/fermi-console/src/updater.rs`
/// (`BINARY_ASSET_NAME`) and `scripts/install-fermi-console.sh`.
const PLATFORMS: [&str; 3] = ["linux-x86_64", "macos-aarch64", "macos-x86_64"];

/// What an omitted `?platform=` means.
///
/// Every fermi-console built before macOS support existed calls this
/// endpoint with no `platform` at all, and there is no way to reach back
/// and upgrade them. Defaulting to Linux is what keeps those installs
/// updating instead of silently breaking.
const DEFAULT_PLATFORM: &str = "linux-x86_64";

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Optional version pin (e.g. "v0.8.0"). Falls back to "latest".
    #[serde(default)]
    pub v: Option<String>,
    /// Optional platform slug (e.g. "macos-aarch64"). Falls back to
    /// [`DEFAULT_PLATFORM`]; see the note there on why.
    #[serde(default)]
    pub platform: Option<String>,
}

/// Validate a requested platform slug against [`PLATFORMS`].
///
/// Returns the *allowlisted* `&'static str`, never the caller's string,
/// so nothing attacker-controlled can reach the outbound URL.
fn resolve_platform(requested: Option<&str>) -> Result<&'static str, String> {
    let Some(p) = requested else {
        return Ok(DEFAULT_PLATFORM);
    };
    PLATFORMS
        .iter()
        .find(|known| **known == p)
        .copied()
        // A 400 rather than a silent fallback: a client asking for
        // "macos-arm64" (the plausible near-miss for "macos-aarch64")
        // wants to be told it guessed wrong, not handed a Linux ELF that
        // dies on exec with no diagnostic a tester could act on.
        .ok_or_else(|| {
            format!(
                "unknown platform '{}'; expected one of: {}",
                p,
                PLATFORMS.join(", ")
            )
        })
}

/// Resolve the URL we redirect to, given an already-validated platform.
///
/// Priority order:
///   1. Explicit env override (staging, R2 bucket, whatever).
///   2. GitHub Releases with the requested version.
///   3. GitHub Releases latest.
///
/// The env var is a URL *template* — we substitute `{version}` and
/// `{platform}` when present, so a single env var can serve every
/// combination. A template with neither placeholder is treated as a
/// fixed URL, which is how a single-artifact staging host is pointed at.
fn resolve_download_url(version: &str, platform: &'static str, template: Option<String>) -> String {
    let asset = format!("fermi-console-{}", platform);

    match template {
        Some(t) if t.contains("{version}") || t.contains("{platform}") => t
            .replace("{version}", version)
            .replace("{platform}", platform),
        Some(t) => t,
        None => {
            // Default: GitHub Releases. Note this returns 404 if the
            // repo is private; the readable error is preferable to
            // exposing that fact via our redirect.
            if version == "latest" {
                format!(
                    "https://github.com/Replicant-Partners/fermi/releases/latest/download/{}",
                    asset
                )
            } else {
                format!(
                    "https://github.com/Replicant-Partners/fermi/releases/download/{}/{}",
                    version, asset
                )
            }
        }
    }
}

pub async fn fermi_console_download(Query(q): Query<DownloadQuery>) -> Response {
    let version = q.v.as_deref().unwrap_or("latest");

    let platform = match resolve_platform(q.platform.as_deref()) {
        Ok(p) => p,
        Err(msg) => return (StatusCode::BAD_REQUEST, msg).into_response(),
    };

    let url = resolve_download_url(
        version,
        platform,
        std::env::var("FERMI_CONSOLE_DOWNLOAD_URL").ok(),
    );

    // 302 (Found) rather than 301 (Moved Permanently) so we can rotate
    // the target later without cache poisoning.
    Redirect::temporary(&url).into_response()
}

#[cfg(test)]
mod download_tests {
    use super::*;

    #[test]
    fn omitted_platform_still_serves_linux() {
        // Every console built before macOS support existed sends no
        // `platform` at all. If this ever stops resolving to the Linux
        // asset, that entire installed base stops self-updating.
        assert_eq!(resolve_platform(None).unwrap(), "linux-x86_64");
    }

    #[test]
    fn every_published_platform_is_accepted() {
        for p in PLATFORMS {
            assert_eq!(resolve_platform(Some(p)).unwrap(), p);
        }
    }

    #[test]
    fn unknown_platform_is_rejected_not_defaulted() {
        // "macos-arm64" is the near-miss a human will actually type.
        let err = resolve_platform(Some("macos-arm64")).unwrap_err();
        assert!(err.contains("macos-arm64"), "error should echo the input");
        assert!(
            err.contains("macos-aarch64"),
            "error should list valid slugs"
        );
    }

    #[test]
    fn platform_slug_cannot_be_injected_into_the_redirect() {
        // The redirect target is built by interpolating the platform into
        // a github.com URL. Anything that escapes the path segment is an
        // open redirect, so the allowlist must reject these outright
        // rather than sanitising them.
        for hostile in [
            "../../../../evil.example/payload",
            "linux-x86_64/../../..",
            "@evil.example",
            "",
        ] {
            assert!(
                resolve_platform(Some(hostile)).is_err(),
                "{hostile:?} must not be accepted"
            );
        }
    }

    #[test]
    fn default_urls_point_at_the_per_platform_asset() {
        assert_eq!(
            resolve_download_url("latest", "macos-aarch64", None),
            "https://github.com/Replicant-Partners/fermi/releases/latest/download/fermi-console-macos-aarch64"
        );
        assert_eq!(
            resolve_download_url("v0.11.13", "linux-x86_64", None),
            "https://github.com/Replicant-Partners/fermi/releases/download/v0.11.13/fermi-console-linux-x86_64"
        );
    }

    #[test]
    fn env_template_substitutes_both_placeholders() {
        let t = "https://cdn.example/{version}/{platform}.bin".to_string();
        assert_eq!(
            resolve_download_url("v1.2.3", "macos-x86_64", Some(t)),
            "https://cdn.example/v1.2.3/macos-x86_64.bin"
        );
    }

    #[test]
    fn placeholderless_env_template_is_served_verbatim() {
        // Pre-existing behaviour: a fixed staging URL ignores both params
        // rather than having them appended.
        let t = "https://staging.example/console".to_string();
        assert_eq!(
            resolve_download_url("v1.2.3", "macos-aarch64", Some(t.clone())),
            t
        );
    }

    #[test]
    fn version_only_template_still_honours_platform_free_hosts() {
        // A template that pins the platform itself but varies by version.
        let t = "https://cdn.example/{version}/fermi-console".to_string();
        assert_eq!(
            resolve_download_url("v2.0.0", "linux-x86_64", Some(t)),
            "https://cdn.example/v2.0.0/fermi-console"
        );
    }
}

pub async fn agent_detail() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_detail.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_detail.html: {}", e);
            format!(
                "<h1>Agent Bestiary</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn ontology_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/ontology.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/ontology.html: {}", e);
            format!(
                "<h1>Knowledge Graph</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

// ─── API routes ────────────────────────────────────────────────────

pub async fn projector_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/projector.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/projector.html: {}", e);
            format!(
                "<h1>Embedding Projector</h1><p>Error loading template: {}</p>",
                e
            )
        }
    };
    Html(html)
}

pub async fn dashboard_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/dashboard.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/dashboard.html: {}", e);
            format!("<h1>Dashboard</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn agent_create_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/agent_create.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/agent_create.html: {}", e);
            format!("<h1>Create Agent</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn workspace_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/workspace.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/workspace.html: {}", e);
            format!("<h1>Workspace</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

// ─── Settings page ─────────────────────────────────────────────────

pub async fn settings_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/settings.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/settings.html: {}", e);
            format!("<h1>Settings</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn docs_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/docs.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/docs.html: {}", e);
            format!("<h1>Documentation</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn marketplace_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/marketplace.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/marketplace.html: {}", e);
            format!("<h1>Marketplace</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn admin_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/admin.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/admin.html: {}", e);
            format!("<h1>Admin</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

// ─── Phase 4 — Observatory pages (Plane D) ─────────────────────────

pub async fn observatory_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/observatory.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/observatory.html: {}", e);
            format!("<h1>Observatory</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn observatory_hitl_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/observatory_hitl.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/observatory_hitl.html: {}", e);
            format!("<h1>Review Queue</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}

pub async fn apps_catalogue_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/apps.html") {
        Ok(content) => content,
        Err(e) => format!("<h1>Apps</h1><p>Error loading template: {}</p>", e),
    };
    Html(html)
}

pub async fn app_detail_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/app_detail.html") {
        Ok(content) => content,
        Err(e) => format!("<h1>App</h1><p>Error loading template: {}</p>", e),
    };
    Html(html)
}

/// Invite landing page. Served at `/invites/:token` when an operator
/// shares an invite link. The template loads inline JS that:
///   1. Fetches invite details from /api/invites/by-token/:token
///   2. Renders inviter + target + permission + expiry
///   3. Shows Accept / Decline buttons
///   4. Falls back to a sign-in prompt when the accept POST returns 401
///
/// The route is a static template; token extraction happens client-side
/// so we don't need to thread the Path extractor through here.
pub async fn invite_landing_view() -> Html<String> {
    let html = match std::fs::read_to_string("templates/invite_landing.html") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading templates/invite_landing.html: {}", e);
            format!("<h1>Invitation</h1><p>Error loading template: {}</p>", e)
        }
    };
    Html(html)
}
