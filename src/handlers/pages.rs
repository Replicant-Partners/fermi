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
// GitHub Releases "latest" URL. Query param `?v=vX.Y.Z` is honoured
// when set (in-app updater uses it to fetch a specific version).

#[derive(Debug, Deserialize)]
pub struct DownloadQuery {
    /// Optional version pin (e.g. "v0.8.0"). Falls back to "latest".
    #[serde(default)]
    pub v: Option<String>,
}

pub async fn fermi_console_download(Query(q): Query<DownloadQuery>) -> Response {
    // Priority order for resolving the download URL:
    //   1. Explicit env override (staging, R2 bucket, whatever).
    //   2. GitHub Releases with the requested version.
    //   3. GitHub Releases latest.
    //
    // The env var is a URL *template* — we substitute `{version}` if
    // it's present so a single env var can serve both `latest` and
    // version-pinned requests. If the template omits the placeholder
    // we ignore the ?v= param and always serve the same URL.
    let version = q.v.as_deref().unwrap_or("latest");

    let url = match std::env::var("FERMI_CONSOLE_DOWNLOAD_URL") {
        Ok(template) if template.contains("{version}") => template.replace("{version}", version),
        Ok(template) => template,
        Err(_) => {
            // Default: GitHub Releases. Note this returns 404 if the
            // repo is private; the readable error is preferable to
            // exposing that fact via our redirect.
            if version == "latest" {
                "https://github.com/Replicant-Partners/fermi/releases/latest/download/fermi-console-linux-x86_64".to_string()
            } else {
                format!(
                    "https://github.com/Replicant-Partners/fermi/releases/download/{}/fermi-console-linux-x86_64",
                    version
                )
            }
        }
    };

    // 302 (Found) rather than 301 (Moved Permanently) so we can rotate
    // the target later without cache poisoning.
    Redirect::temporary(&url).into_response()
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
