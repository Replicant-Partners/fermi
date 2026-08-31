//! HTML page-serving handlers.

use axum::{
    extract::{Extension, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use fermi_auth::{rbac, AuthPrincipal, ObjectType};
use serde::Deserialize;

use crate::AppState;

// ─── App shells ───────────────────────────────────────────────
//
/// Serve a template from disk as an uncacheable app shell.
///
/// ## Why `no-store`
///
/// These pages carry no data. Every number on them arrives later by XHR, and the
/// shell is the rendering logic — markup, CSS and JS — which changes with every
/// deploy. Until now none of them sent a single cache directive: no
/// `Cache-Control`, no `ETag`, no `Last-Modified`. With nothing to go on a
/// browser falls back to heuristic freshness and an intermediary may cache the
/// document outright.
///
/// The result is the worst available combination, and it is not hypothetical:
/// the observatory was observed serving a stale shell — badges overlapping their
/// labels, a layout fix absent — while the API data rendered inside it was
/// minutes old. A page that is visibly live and silently months behind is far
/// more misleading than one that is obviously broken, because there is nothing
/// to notice. On a dashboard whose entire purpose is telling you what is
/// actually true, that is the one failure mode that must not exist.
///
/// `no-store` rather than `no-cache`: there is no value in revalidating a shell
/// that is read from local disk on every request, and `no-store` is the
/// directive intermediaries honour most consistently.
fn app_shell(path: &str) -> Response {
    let html = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error loading {}: {}", path, e);
            format!(
                "<h1>Page unavailable</h1><p>Could not load <code>{}</code>: {}</p>",
                path, e
            )
        }
    };
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, must-revalidate"),
        )],
        Html(html),
    )
        .into_response()
}

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

// ─── Agent page family (`/agent/:agent_id/*`) ──────────────────────

/// Read a page template, falling back to a minimal error body.
fn render_template(path: &str, title: &str) -> Html<String> {
    match std::fs::read_to_string(path) {
        Ok(content) => Html(content),
        Err(e) => {
            eprintln!("Error loading {}: {}", path, e);
            Html(format!(
                "<h1>{}</h1><p>Error loading template: {}</p>",
                title, e
            ))
        }
    }
}

/// Existence + visibility guard for the `/agent/:agent_id/*` page family.
///
/// These pages are shells: they render, then fetch their data from
/// `/api/agents/:agent_id`. That API deliberately answers 404 — not 403 —
/// for anything the caller cannot view, so it does not leak the existence
/// of private agents (see `get_agent_handler`).
///
/// The page handlers used to take no path parameter at all. They served a
/// full, crawlable, indexable shell for *any* slug — drafts, private
/// agents, and agents that never existed. The shell then failed its own
/// data fetch and sat there looking like a published-but-broken agent,
/// which is exactly the confusion this guard removes.
///
/// Note the owner case matters as much as the anonymous one: the detail
/// page carries the publish button, the Manage tab and the delete modal,
/// so a draft's owner MUST still be able to load it. That is why this
/// walks the same RBAC ladder as the API rather than simply requiring
/// public+published.
async fn require_visible_agent(
    state: &AppState,
    caller: Option<&AuthPrincipal>,
    agent_id: &str,
) -> Result<(), Response> {
    let agent = crate::resolve_agent(state, agent_id)
        .await
        .map_err(|_| agent_not_found())?;

    let vis = crate::handlers::agents::agent_effective_visibility(&agent);
    let owner_id = agent.owner_id.clone().unwrap_or_default();

    match caller {
        Some(principal) => {
            rbac::require_view(
                &state.db,
                principal,
                ObjectType::Agent,
                &agent.agent_id.to_string(),
                &owner_id,
                vis,
            )
            .await
            .map_err(|_| agent_not_found())?;
        }
        None => {
            if !rbac::visible_sync_anon(vis) {
                return Err(agent_not_found());
            }
        }
    }

    Ok(())
}

/// 404 body for a denied or missing agent page. Deliberately identical
/// for "does not exist" and "exists but you may not see it" — same
/// reasoning as the API.
fn agent_not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        render_template("templates/404.html", "404 — Not Found"),
    )
        .into_response()
}

pub async fn agent_detail(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    Path(agent_id): Path<String>,
) -> Response {
    let principal = caller.as_ref().map(|Extension(p)| p);
    if let Err(denied) = require_visible_agent(&state, principal, &agent_id).await {
        return denied;
    }
    render_template("templates/agent_detail.html", "Agent Bestiary").into_response()
}

pub async fn ontology_view(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    Path(agent_id): Path<String>,
) -> Response {
    let principal = caller.as_ref().map(|Extension(p)| p);
    if let Err(denied) = require_visible_agent(&state, principal, &agent_id).await {
        return denied;
    }
    render_template("templates/ontology.html", "Knowledge Graph").into_response()
}

// ─── API routes ────────────────────────────────────────────────────

/// Serves both `/projector` (no agent scope) and
/// `/agent/:agent_id/projector`. `Option<Path<..>>` distinguishes them:
/// the bare route has no path param, so extraction fails and yields
/// `None`, while the agent-scoped route gets the same guard as the rest
/// of the `/agent/:agent_id/*` family.
pub async fn projector_view(
    State(state): State<AppState>,
    caller: Option<Extension<AuthPrincipal>>,
    agent_id: Option<Path<String>>,
) -> Response {
    if let Some(Path(ref agent_id)) = agent_id {
        let principal = caller.as_ref().map(|Extension(p)| p);
        if let Err(denied) = require_visible_agent(&state, principal, agent_id).await {
            return denied;
        }
    }
    render_template("templates/projector.html", "Embedding Projector").into_response()
}

pub async fn dashboard_view() -> Response {
    app_shell("templates/dashboard.html")
}

/// The contract builder on its own page.
///
/// `/contracts?agent=<agent_id>` loads that agent's current contract. Lives
/// outside the create wizard because the wizard could only author a contract
/// at birth, and 90 of the 101 agents were already born — the interesting work
/// is modifying a contract, not minting one.
pub async fn contract_builder_view() -> Response {
    app_shell("templates/contract_builder.html")
}

pub async fn agent_create_view() -> Response {
    app_shell("templates/agent_create.html")
}

pub async fn workspace_view() -> Response {
    app_shell("templates/workspace.html")
}

// ─── Settings page ─────────────────────────────────────────────────

pub async fn settings_view() -> Response {
    app_shell("templates/settings.html")
}

pub async fn docs_view() -> Response {
    app_shell("templates/docs.html")
}

pub async fn marketplace_view() -> Response {
    app_shell("templates/marketplace.html")
}

pub async fn admin_view() -> Response {
    app_shell("templates/admin.html")
}

// ─── Phase 4 — Observatory pages (Plane D) ─────────────────────────

pub async fn observatory_view() -> Response {
    app_shell("templates/observatory.html")
}

/// Ecology — the "what lives here" lens.
///
/// Sibling to the Observatory. The Observatory asks a clinical question of
/// one agent at a time; this asks a structural question of the whole
/// population: what exists, how is it organised, and how did it get here.
pub async fn ecology_view() -> Response {
    app_shell("templates/ecology.html")
}

/// Rounds — what needs you, in what order.
///
/// Served alongside `/dashboard` rather than replacing it, so the two can be
/// compared before anything is deleted. The dashboard is a directory; a round is
/// ordered by who needs you most.
pub async fn rounds_view() -> Response {
    app_shell("templates/rounds.html")
}

/// The Bestiary — one register, three lenses.
///
/// Replaces three separately-implemented lists of the same agents: the
/// catalogue grid, the ecology register, and the Observatory's patient
/// register. A lens changes columns and sort, not the page.
pub async fn bestiary_view() -> Response {
    app_shell("templates/bestiary.html")
}

/// Loops and gates — one surface, three tabs.
///
/// A loop is a control cycle; a gate is the point in it where a correction is
/// permitted or refused. They are one picture from two sides — `loop_model`
/// stages carry `gated_by` — so they share a page and a reading vocabulary
/// rather than competing for two nav slots.
///
/// Served at both `/loops` and `/gates`, because both names are ones a reader
/// will reach for and neither should 404.
pub async fn loops_view() -> Response {
    app_shell("templates/loops.html")
}

/// One specimen, three tabs.
///
/// The collapse of the eight-tab agent page: Profile (what it is), Record (what
/// it has done), Health (is it working). Editing is a drawer rather than a tab,
/// because it is a different activity from reading.
pub async fn specimen_view() -> Response {
    app_shell("templates/specimen.html")
}

/// One artifact, and the checkpoints it passed.
///
/// The inversion the loop surfaces needed: the primary object is the episode and
/// the loops are the routes it can take. A census tells you 3,576 episodes
/// accumulated; this tells you what happened to one of them, which is the only
/// version a reader can follow without already holding the machine in their head.
pub async fn trace_view() -> Response {
    app_shell("templates/trace.html")
}

/// A workspace as its seams: every arrow an artifact crossing a checkpoint.
///
/// The composition-level zoom on the artifact trace. The workflow sequence
/// diagram and the verification ladder are the same drawing at two scales — an
/// arrow between two agents *is* an artifact taking a path, and a gate is a
/// checkpoint on it. They have been two unrelated screens because the same
/// interaction is written twice, as a message pair and as an episode, and
/// nothing joins them.
pub async fn flow_view() -> Response {
    app_shell("templates/flow.html")
}

/// Every exchange, across every agent — the aggregated stream.
///
/// A workspace already shows its own flow. What was missing is the view across
/// everything: one running log of who asked whom, what came back, and whether
/// anything checked it, with a trace one click away.
pub async fn stream_view() -> Response {
    app_shell("templates/stream.html")
}

/// Why the belts are quiet, and whose job each one is.
///
/// The destination of the artifact trace's default state. `unknown` on every
/// trust surface is overwhelmingly the *agent* declaring no structure to check
/// against — 89 of 96 real producing agents have no field contract — which is
/// authoring work, not a platform contract we owe.
pub async fn declarations_view() -> Response {
    app_shell("templates/declarations.html")
}

/// One gate: what it refused, and what anybody has said about it.
///
/// Three independent readings live here and must not collapse into one — does the
/// gate discriminate (from counts), does the durability claim hold (are there
/// rows), and has anybody judged the decisions right. A gate can be
/// `discriminating`, `idle` and `unreviewed` at once, and all three are true.
pub async fn gate_view() -> Response {
    app_shell("templates/gate.html")
}

pub async fn observatory_hitl_view() -> Response {
    app_shell("templates/observatory_hitl.html")
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
pub async fn invite_landing_view() -> Response {
    app_shell("templates/invite_landing.html")
}

#[cfg(test)]
mod app_shell_tests {
    use super::*;

    /// Every app shell must forbid caching.
    ///
    /// The failure this prevents has already happened once: a stale observatory
    /// shell rendered live API data, so the page looked current while its layout
    /// and JS were from an older deploy. There was nothing to notice — the
    /// numbers were right. On a dashboard built to tell you what is actually
    /// true, an invisibly stale rendering is the one failure that must not exist.
    #[tokio::test]
    async fn app_shells_are_never_cached() {
        let resp = app_shell("templates/observatory.html");
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<absent>");
        assert!(
            cc.contains("no-store"),
            "app shells must send no-store; got `{cc}`"
        );
    }

    /// A missing template must still answer, and must still be uncacheable —
    /// otherwise a transient deploy error can be cached as the page.
    #[tokio::test]
    async fn a_missing_template_is_reported_and_not_cached() {
        let resp = app_shell("templates/definitely-not-a-real-template.html");
        assert_eq!(resp.status(), StatusCode::OK);
        let cc = resp
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<absent>");
        assert!(cc.contains("no-store"), "got `{cc}`");
    }

    /// The shells are read from disk at request time, so a template that has
    /// been renamed or dropped from the image becomes a broken page rather than
    /// a compile error. Assert the ones this module names actually exist.
    #[test]
    fn every_named_template_exists_on_disk() {
        let src = include_str!("pages.rs");
        let mut checked = 0;
        for cap in src.split("app_shell(\"").skip(1) {
            let Some(path) = cap.split('"').next() else {
                continue;
            };
            if !path.starts_with("templates/") || path.contains("not-a-real-template") {
                continue;
            }
            assert!(
                std::path::Path::new(path).exists(),
                "{path} is served by a handler but is not on disk"
            );
            checked += 1;
        }
        assert!(
            checked >= 11,
            "expected to check the app shells, saw {checked}"
        );
    }
}
