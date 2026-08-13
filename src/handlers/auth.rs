//! Auth handlers — OAuth (Google/GitHub), API keys, SIWE.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Redirect, Response},
    Json,
};
use fermi_auth::{
    api_keys, build_github_auth_url, build_google_auth_url, create_session_token, generate_state,
    get_or_create_wallet, github_exchange_code, github_fetch_user_info, google_exchange_code,
    google_fetch_user_info, sync_user_from_app, AuthPrincipal,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
// ─── Auth routes ───────────────────────────────────────────────────

/// Query param to track which provider started the flow
#[derive(Debug, Deserialize)]
pub struct AuthCallbackQuery {
    code: String,
    state: String,
}

/// Optional query params for OAuth flows
#[derive(Debug, Deserialize)]
pub struct OAuthQuery {
    pub mobile: Option<String>,
    /// Where to redirect after auth (e.g. "/rabble/" for Rabble web)
    pub redirect: Option<String>,
    /// App slug the sign-in was initiated from (e.g. "fermi_console").
    /// Stamped on the `users` row for NEW signups only. Silently
    /// ignored on existing-user logins.
    pub app: Option<String>,
}

/// Redirect to Google OAuth
/// Pass ?mobile=1 to get a deep link callback instead of cookie redirect
pub async fn auth_google(
    State(state): State<AppState>,
    Query(q): Query<OAuthQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let config = state.oauth.google().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Google OAuth not configured".to_string(),
        )
    })?;
    let csrf_state = generate_state();
    let mobile_flag = if q.mobile.is_some() { ":mobile" } else { "" };
    // URL-encode the redirect target before folding into the state.
    // The state uses `:` as a field delimiter, so a redirect URL with
    // a port (`http://127.0.0.1:PORT/callback`) would otherwise get
    // truncated at the first colon on the callback side — which is
    // the exact regression that broke the desktop console's automatic
    // browser sign-in flow. `url_encode` turns colons into `%3A` so
    // the delimiter parser stops only at real state boundaries.
    let redirect_flag = match &q.redirect {
        Some(r) => format!(":redirect={}", url_encode(r)),
        None => String::new(),
    };
    // App slug is passed through the OAuth `state` param — same
    // channel as mobile/redirect. Validated on the callback side.
    let app_flag = match q.app.as_deref().filter(|a| is_valid_app_slug(a)) {
        Some(a) => format!(":app={}", a),
        None => String::new(),
    };
    let state_with_provider = format!(
        "google:{}{}{}{}",
        csrf_state, mobile_flag, redirect_flag, app_flag
    );
    let url = build_google_auth_url(config, &state_with_provider);
    Ok(Redirect::temporary(&url))
}

/// Validate a slug from an untrusted query param before we round-trip
/// it through the OAuth state. Same shape check as `apps.slug`'s
/// database constraint, so anything we accept here would be a valid
/// App row if one exists.
fn is_valid_app_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    if bytes.len() < 3 || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// Redirect to GitHub OAuth
/// Pass ?mobile=1 to get a deep link callback instead of cookie redirect
pub async fn auth_github(
    State(state): State<AppState>,
    Query(q): Query<OAuthQuery>,
) -> Result<Redirect, (StatusCode, String)> {
    let config = state.oauth.github().map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "GitHub OAuth not configured".to_string(),
        )
    })?;
    let csrf_state = generate_state();
    let mobile_flag = if q.mobile.is_some() { ":mobile" } else { "" };
    // See `auth_google` above for why the redirect target is
    // URL-encoded before folding into the state param.
    let redirect_flag = match &q.redirect {
        Some(r) => format!(":redirect={}", url_encode(r)),
        None => String::new(),
    };
    let app_flag = match q.app.as_deref().filter(|a| is_valid_app_slug(a)) {
        Some(a) => format!(":app={}", a),
        None => String::new(),
    };
    let state_with_provider = format!(
        "github:{}{}{}{}",
        csrf_state, mobile_flag, redirect_flag, app_flag
    );
    let url = build_github_auth_url(config, &state_with_provider);
    Ok(Redirect::temporary(&url))
}

/// Handle OAuth callback from Google or GitHub
pub async fn auth_callback(
    State(state): State<AppState>,
    Query(params): Query<AuthCallbackQuery>,
) -> Response {
    match auth_callback_inner(state, params).await {
        Ok(resp) => resp,
        Err(msg) => {
            eprintln!("OAuth error: {}", msg);
            // Redirect to landing with error hint instead of showing raw error
            Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header(header::LOCATION, "/?auth_error=1")
                .body(axum::body::Body::empty())
                .unwrap_or_else(|_| {
                    Response::new(axum::body::Body::from(format!("OAuth error: {}", msg)))
                })
        }
    }
}

pub async fn auth_callback_inner(
    state: AppState,
    params: AuthCallbackQuery,
) -> Result<Response, String> {
    let map_err = |e: fermi_auth::AuthError| e.to_string();

    // Determine provider, mobile flag, and redirect from state prefix
    // Format: "provider:csrf[:mobile][:redirect=/path]"
    let (provider, rest) = params
        .state
        .split_once(':')
        .unwrap_or(("unknown", &params.state));
    let is_mobile = rest.contains(":mobile");
    // `:redirect=` and `:app=` can each carry until the next `:` or
    // end-of-string. Trailing values (e.g. `:app=fermi_console` at the
    // tail) are supported by taking the first segment after the
    // marker; anything appended after another `:` is stripped.
    //
    // The redirect value is URL-encoded on the outbound side
    // (`auth_google` / `auth_github`), so colons inside the redirect
    // URL survive the delimiter split as `%3A`. Decode after splitting.
    let redirect_to = rest
        .split(":redirect=")
        .nth(1)
        .map(|s| s.split(':').next().unwrap_or(s).to_string())
        .map(|encoded| url_decode(&encoded));
    let signup_app = rest
        .split(":app=")
        .nth(1)
        .map(|s| s.split(':').next().unwrap_or(s).to_string())
        .filter(|s| is_valid_app_slug(s));

    let user_info = match provider {
        "google" => {
            let config = state.oauth.google().map_err(|e| map_err(e))?;
            let tokens = google_exchange_code(config, &params.code)
                .await
                .map_err(map_err)?;
            google_fetch_user_info(&tokens.access_token)
                .await
                .map_err(map_err)?
        }
        "github" => {
            let config = state.oauth.github().map_err(|e| map_err(e))?;
            let tokens = github_exchange_code(config, &params.code)
                .await
                .map_err(map_err)?;
            github_fetch_user_info(&tokens.access_token)
                .await
                .map_err(map_err)?
        }
        _ => {
            return Err("Unknown OAuth provider".to_string());
        }
    };

    // Sync user to database. The `signup_app` slug (if any) is only
    // written on INSERT; existing users' signup_app_slug is preserved.
    let user = sync_user_from_app(&state.db, &user_info, signup_app.as_deref())
        .await
        .map_err(map_err)?;

    // Ensure wallet exists (onboarding grant is auto-applied inside get_or_create_wallet)
    let _ = get_or_create_wallet(&state.db, "user", &user.user_id).await;

    // Create session JWT
    let token = create_session_token(&user, &state.jwt_secret).map_err(map_err)?;

    if is_mobile {
        // Mobile flow: redirect to deep link with token
        let redirect_url = format!("rabble://auth?token={}&user_id={}", token, user.user_id);
        Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, redirect_url)
            .body(axum::body::Body::empty())
            .map_err(|e| e.to_string())
    } else {
        // Web flow: set cookie and redirect
        let dest = redirect_to
            .filter(|r| {
                (r.starts_with('/') && !r.contains("//"))
                    || r.starts_with("https://rabble.world")
                    || r.starts_with("https://silat.ooo")
                    || r.starts_with("https://kask.bio")
                    || r.starts_with("https://www.kask.bio")
                    || r.starts_with("http://127.0.0.1:")
                    || r.starts_with("http://localhost:")
            })
            .unwrap_or_else(|| "/dashboard".to_string());
        let cookie = format!(
            "abw_session={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=604800",
            token
        );
        // Cross-domain redirect: pass token in URL for the app to pick up
        let final_dest = if dest.starts_with("https://rabble.world") {
            format!(
                "https://rabble.world/#/auth?token={}&user_id={}",
                token, user.user_id
            )
        } else if dest.starts_with("https://silat.ooo") {
            format!(
                "https://silat.ooo/#/auth?token={}&user_id={}",
                token, user.user_id
            )
        } else if dest.starts_with("https://kask.bio") || dest.starts_with("https://www.kask.bio") {
            // Token-in-fragment pattern (same as rabble.world / silat.ooo).
            //
            // Why not the cookie? The session cookie set above uses
            // SameSite=Lax, which blocks the cookie on cross-origin fetch()
            // calls from kask.bio to agent-bestiary.world. kask's frontend
            // therefore never sees the session via /api/auth/me.
            //
            // Instead we hand the JWT to kask in the URL fragment.
            // kask.bio's hooks.js (consumeOAuthTokenFromHash) picks it up,
            // persists to localStorage['abw_api_token'], and abw-client.js
            // attaches it as Authorization: Bearer on subsequent calls.
            //
            // We cannot switch to SameSite=None because Chrome is rolling
            // out third-party-cookie blocking anyway. Token-in-fragment is
            // the future-proof pattern.
            let sep = if dest.contains('#') { '&' } else { '#' };
            format!("{}{}token={}&user_id={}", dest, sep, token, user.user_id)
        } else if dest.starts_with("http://127.0.0.1:") || dest.starts_with("http://localhost:") {
            // Desktop app flow: redirect to localhost callback with token
            let separator = if dest.contains('?') { "&" } else { "?" };
            format!(
                "{}{}token={}&user_id={}",
                dest, separator, token, user.user_id
            )
        } else {
            dest
        };
        Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, &final_dest)
            .header(header::SET_COOKIE, cookie)
            .body(axum::body::Body::empty())
            .map_err(|e| e.to_string())
    }
}

/// Logout — clear session cookie
pub async fn auth_logout() -> Result<Response, (StatusCode, String)> {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, "/")
        .header(
            header::SET_COOKIE,
            "abw_session=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0",
        )
        .body(axum::body::Body::empty())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Get current authenticated user.
///
/// For API-key principals we ALSO fetch the underlying user's display_name
/// / email from the `users` table so downstream UIs (the Fermi console
/// footer, share/invite dialogs) can render a friendly label instead of
/// the raw `user_id` UUID. Without this join, callers authenticated via
/// `FERMI_API_KEY` see the UUID as "who am I" — which is what the console
/// operator was hitting.
/// While impersonating, this reports the **target** user — that is the
/// point: every surface that renders "who am I" should render them. The
/// `impersonation` block is added alongside so the UI can show the
/// exit banner; it is the only place the admin's real identity leaks
/// into a response, and it is additive so existing clients ignore it.
pub async fn auth_me(State(state): State<AppState>, principal: AuthPrincipal) -> Json<Value> {
    if let Some(imp) = principal.impersonation() {
        return Json(json!({
            "user_id": imp.effective.user_id,
            "email": imp.effective.email,
            "display_name": imp.effective.display_name,
            "role": imp.effective.role,
            "auth_provider": imp.effective.auth_provider,
            "github_username": imp.effective.github_username,
            "impersonation": {
                "active": true,
                "mode": imp.mode.as_str(),
                "session_id": imp.session_id,
                "real_user_id": imp.real.user_id,
                "real_email": imp.real.email,
                "viewing_as": imp.effective.display_name
                    .clone()
                    .unwrap_or_else(|| imp.effective.email.clone()),
            },
        }));
    }

    match principal {
        AuthPrincipal::User(user) => Json(json!({
            "user_id": user.user_id,
            "email": user.email,
            "display_name": user.display_name,
            "role": user.role,
            "auth_provider": user.auth_provider,
            "github_username": user.github_username,
        })),
        // Unreachable: the early return above owns this case. Kept as an
        // explicit arm (rather than a `_` wildcard) so that adding a
        // future principal variant is still a compile error here.
        AuthPrincipal::Impersonated(imp) => Json(json!({
            "user_id": imp.effective.user_id,
            "email": imp.effective.email,
            "display_name": imp.effective.display_name,
            "role": imp.effective.role,
            "auth_provider": imp.effective.auth_provider,
            "github_username": imp.effective.github_username,
        })),
        AuthPrincipal::ApiKey(key) => {
            // Best-effort lookup — if the row is missing (dev fixture,
            // orphaned key) we still return the key info.
            let profile =
                sqlx::query("SELECT display_name, email FROM users WHERE user_id = $1 LIMIT 1")
                    .bind(&key.user_id)
                    .fetch_optional(&state.db)
                    .await
                    .ok()
                    .flatten();
            let display_name: Option<String> = profile
                .as_ref()
                .and_then(|r| r.try_get("display_name").ok());
            let email: Option<String> = profile.as_ref().and_then(|r| r.try_get("email").ok());
            Json(json!({
                "user_id": key.user_id,
                "auth_type": "api_key",
                "key_name": key.name,
                "scopes": key.scopes,
                "display_name": display_name,
                "email": email,
            }))
        }
    }
}

// ─── API key management ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    name: String,
    scopes: Option<Vec<String>>,
}

pub async fn create_api_key(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let scopes = body.scopes.unwrap_or_else(|| vec!["read".to_string()]);
    let (plaintext_key, key_info) =
        api_keys::create_api_key(&state.db, &principal.user_id(), &body.name, &scopes)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "key": plaintext_key,
        "key_id": key_info.key_id,
        "name": key_info.name,
        "scopes": key_info.scopes,
        "note": "Save this key — it cannot be retrieved again."
    })))
}

pub async fn list_api_keys(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let keys = api_keys::list_api_keys(&state.db, &principal.user_id())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "api_keys": keys })))
}

pub async fn revoke_api_key(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(key_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    api_keys::revoke_api_key(&state.db, &principal.user_id(), key_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "revoked" })))
}

// ─── CLI login flow (localhost-callback OAuth) ────────────────────
//
// The `abw` CLI authenticates by opening a browser to `/auth/cli` with a
// `callback` URL pointing at a localhost listener and a `state` nonce for
// CSRF defence. The server either uses an existing session or runs the user
// through standard OAuth, then redirects to `/auth/cli/finish` which mints
// a long-lived API key (scope = "cli") and redirects to the callback with
// `?api_key=...&user=...&state=...`.
//
// This is the same flow `gh auth login`, `gcloud auth login`, and
// `fly auth login` use.

#[derive(Debug, Deserialize)]
pub struct AuthCliQuery {
    /// Localhost callback URL provided by the CLI. Must be http://127.0.0.1:* or http://localhost:*.
    pub callback: String,
    /// CSRF nonce echoed back unmodified.
    pub state: String,
}

/// GET /auth/cli — entry point for CLI login.
///
/// If the caller already has a valid session, immediately hands off to
/// /auth/cli/finish. Otherwise renders a minimal page with provider buttons
/// that route through /auth/google or /auth/github with a redirect back to
/// /auth/cli/finish.
pub async fn auth_cli_start(
    Query(q): Query<AuthCliQuery>,
    principal: Option<AuthPrincipal>,
) -> Result<Response, (StatusCode, String)> {
    // Basic safety check on the callback target.
    if !is_safe_cli_callback(&q.callback) {
        return Err((
            StatusCode::BAD_REQUEST,
            "callback must be http://127.0.0.1:* or http://localhost:*".into(),
        ));
    }

    if principal.is_some() {
        // Already authenticated → straight to the finish handler.
        let finish = format!(
            "/auth/cli/finish?cb={}&state={}",
            url_encode(&q.callback),
            url_encode(&q.state)
        );
        return Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, finish)
            .body(axum::body::Body::empty())
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()));
    }

    // Render a small "pick a provider" page.
    let finish_path = format!(
        "/auth/cli/finish?cb={}&state={}",
        url_encode(&q.callback),
        url_encode(&q.state)
    );
    let google_url = format!("/auth/google?redirect={}", url_encode(&finish_path));
    let github_url = format!("/auth/github?redirect={}", url_encode(&finish_path));

    let body = render_cli_login_page(&google_url, &github_url);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(axum::body::Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Debug, Deserialize)]
pub struct AuthCliFinishQuery {
    pub cb: String,
    pub state: String,
}

/// GET /auth/cli/finish — mints an API key and redirects to the CLI's callback.
///
/// Requires an authenticated session (the OAuth round-trip from /auth/cli set
/// the cookie). Mints an api_key with scope = "cli" named "abw-cli (<host>)"
/// where <host> is the callback's host:port for easy revocation later.
pub async fn auth_cli_finish(
    State(state): State<AppState>,
    Query(q): Query<AuthCliFinishQuery>,
    principal: AuthPrincipal,
) -> Result<Response, (StatusCode, String)> {
    if !is_safe_cli_callback(&q.cb) {
        return Err((
            StatusCode::BAD_REQUEST,
            "callback must be http://127.0.0.1:* or http://localhost:*".into(),
        ));
    }

    let user_id = principal.user_id();

    // Build a readable key name so the user can revoke specific machines later.
    let cb_host = url::Url::parse(&q.cb)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "cli".to_string());
    let key_name = format!("abw-cli ({})", cb_host);

    let (plaintext_key, _info) =
        api_keys::create_api_key(&state.db, &user_id, &key_name, &["cli".to_string()])
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Surface display name in the redirect (CLI shows it under `abw whoami`).
    let display = principal_display(&principal);

    // Append our params to the callback URL.
    let sep = if q.cb.contains('?') { '&' } else { '?' };
    let redirect = format!(
        "{}{}api_key={}&user={}&state={}",
        q.cb,
        sep,
        url_encode(&plaintext_key),
        url_encode(&display),
        url_encode(&q.state)
    );

    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, redirect)
        .body(axum::body::Body::empty())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn is_safe_cli_callback(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:")
}

fn principal_display(p: &AuthPrincipal) -> String {
    // Renders the effective identity: inside a view-as session every
    // "who am I" surface should show the user being viewed.
    match p.as_user() {
        Some(u) => u.display_name.clone().unwrap_or_else(|| {
            if !u.email.is_empty() {
                u.email.clone()
            } else {
                u.user_id.clone()
            }
        }),
        None => match p {
            AuthPrincipal::ApiKey(k) => format!("api_key:{}", k.name),
            _ => p.user_id(),
        },
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Inverse of `url_encode`. Decodes percent-escapes (`%XX`) back to
/// their byte values; non-percent characters pass through verbatim.
/// Used on the OAuth callback path to un-escape a redirect target that
/// was folded into the `state` param (needed because URL hosts contain
/// colons which would otherwise collide with the state's `:` delimiters).
///
/// Failing to decode a `%XX` sequence keeps the literal `%XX` in the
/// output rather than dropping it — an operator-facing garbled URL is
/// easier to debug than a silent truncation.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex_str = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex_str, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn render_cli_login_page(google_url: &str, github_url: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<title>ABW — CLI login</title>
<style>
  body {{ font-family: -apple-system, system-ui, "Segoe UI", sans-serif;
          max-width: 480px; margin: 6em auto; padding: 0 2em;
          color: #1a1a1a; background: #1d2021; color: #ebdbb2; }}
  h1 {{ color: #fabd2f; font-weight: 600; font-size: 1.5em; margin-bottom: 0.5em; }}
  p {{ line-height: 1.6; color: #a89984; }}
  .btn {{ display: inline-block; padding: 12px 24px; margin: 8px 8px 0 0;
          border: 1px solid #fabd2f; color: #fabd2f; text-decoration: none;
          border-radius: 4px; font-weight: 500; transition: background 0.15s; }}
  .btn:hover {{ background: #fabd2f; color: #1d2021; }}
  .dim {{ color: #665c54; font-size: 0.85em; margin-top: 4em; }}
  code {{ background: #3c3836; padding: 2px 6px; border-radius: 3px; color: #fabd2f; }}
</style>
</head>
<body>
  <h1>ABW CLI login</h1>
  <p>Authorise the <code>abw</code> command-line on this machine. After
     sign-in you'll be redirected to the CLI's local listener, which will
     save a per-machine API key.</p>
  <p>
    <a class="btn" href="{}">Sign in with Google</a>
    <a class="btn" href="{}">Sign in with GitHub</a>
  </p>
  <p class="dim">The CLI never sees your password. You can revoke this
     machine's access from <code>/settings/api-keys</code>.</p>
</body></html>"##,
        google_url, github_url
    )
}

// ─── SIWE (Sign In With Ethereum) ──────────────────────────────────

pub async fn siwe_challenge_handler(
    State(state): State<AppState>,
    Json(body): Json<fermi_auth::SiweChallenge>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let domain = std::env::var("SIWE_DOMAIN")
        .or_else(|_| {
            std::env::var("OAUTH_REDIRECT_URI").map(|u| {
                // Extract host from URL like https://agent-bestiary.world/auth/callback
                u.replace("https://", "")
                    .replace("http://", "")
                    .split('/')
                    .next()
                    .unwrap_or("agent-bestiary.world")
                    .to_string()
            })
        })
        .unwrap_or_else(|_| "agent-bestiary.world".to_string());

    let challenge = fermi_auth::create_challenge(body.address.clone(), domain, &state.db)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(json!({
        "message": challenge.message,
        "nonce": challenge.nonce,
    })))
}

pub async fn siwe_verify_handler(
    State(state): State<AppState>,
    Json(body): Json<fermi_auth::SiweVerify>,
) -> Result<Response, (StatusCode, String)> {
    let result = fermi_auth::verify_signature(body.message, body.signature, &state.db)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    let eth_address = result.ethereum_address.clone();

    // Find or create user by ethereum address
    let user_row = sqlx::query(
        "SELECT user_id, email, display_name, avatar_url, role FROM users WHERE ethereum_address = $1",
    )
    .bind(&eth_address)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let is_new;
    let user = if let Some(row) = user_row {
        is_new = false;
        fermi_auth::User {
            user_id: row.get("user_id"),
            email: row.get::<Option<String>, _>("email").unwrap_or_default(),
            display_name: row.get("display_name"),
            role: fermi_auth::UserRole::Developer,
            auth_provider: fermi_auth::AuthProvider::Ethereum,
            github_username: None,
            google_id: None,
            ethereum_address: Some(eth_address.clone()),
            ens_name: result.ens_name.clone(),
        }
    } else {
        is_new = true;
        let user_id = format!("eth_{}", &eth_address[2..10].to_lowercase());
        let display_name = result.ens_name.clone().unwrap_or_else(|| {
            format!(
                "{}...{}",
                &eth_address[..6],
                &eth_address[eth_address.len() - 4..]
            )
        });

        sqlx::query(
            "INSERT INTO users (user_id, display_name, role, auth_provider, ethereum_address, ens_name)
             VALUES ($1, $2, 'user', 'ethereum', $3, $4)
             ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(&user_id)
        .bind(&display_name)
        .bind(&eth_address)
        .bind(&result.ens_name)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        fermi_auth::User {
            user_id,
            email: String::new(),
            display_name: Some(display_name),
            role: fermi_auth::UserRole::Developer,
            auth_provider: fermi_auth::AuthProvider::Ethereum,
            github_username: None,
            google_id: None,
            ethereum_address: Some(eth_address.clone()),
            ens_name: result.ens_name.clone(),
        }
    };

    // Ensure wallet exists (onboarding grant is auto-applied inside get_or_create_wallet)
    let _ = get_or_create_wallet(&state.db, "user", &user.user_id).await;

    // Spec 24 §3.8.1: SIWE callers usually have no email at sign-in
    // (the row is created without one), so this branch is a no-op for
    // pure-Ethereum accounts. It only fires for users who previously
    // linked an email (via OIDC) and now happen to be signing in via
    // SIWE — in that case the existing users row carries their email
    // and we honour any pending invites addressed to it. Best-effort:
    // failure does not block sign-in.
    if !user.email.is_empty() {
        match fermi_auth::invites::claim_pending_for_email(&state.db, &user.user_id, &user.email)
            .await
        {
            Ok(0) => {}
            Ok(n) => eprintln!(
                "[invites] siwe_verify: back-filled {} pending invite(s) for user_id={}",
                n, user.user_id
            ),
            Err(e) => eprintln!(
                "[invites] siwe_verify: claim_pending_for_email failed for user_id={}: {}",
                user.user_id, e
            ),
        }
    }

    // Issue JWT and set cookie
    let token = create_session_token(&user, &state.jwt_secret)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let cookie = format!(
        "abw_session={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=604800",
        token
    );

    let body = serde_json::to_string(&json!({
        "user_id": user.user_id,
        "display_name": user.display_name,
        "ethereum_address": eth_address,
    }))
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::SET_COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(body))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
