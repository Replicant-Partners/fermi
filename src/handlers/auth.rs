//! Auth handlers — OAuth (Google/GitHub), API keys, SIWE.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{Redirect, Response},
    Json,
};
use fermi_auth::{
    api_keys, build_github_auth_url, build_google_auth_url, create_session_token, credit_grant,
    generate_state, get_or_create_wallet, github_exchange_code, github_fetch_user_info,
    google_exchange_code, google_fetch_user_info, sync_user, AuthPrincipal,
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
    let redirect_flag = match &q.redirect {
        Some(r) => format!(":redirect={}", r),
        None => String::new(),
    };
    let state_with_provider = format!("google:{}{}{}", csrf_state, mobile_flag, redirect_flag);
    let url = build_google_auth_url(config, &state_with_provider);
    Ok(Redirect::temporary(&url))
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
    let redirect_flag = match &q.redirect {
        Some(r) => format!(":redirect={}", r),
        None => String::new(),
    };
    let state_with_provider = format!("github:{}{}{}", csrf_state, mobile_flag, redirect_flag);
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
    let redirect_to = rest.split(":redirect=").nth(1).map(|s| s.to_string());

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

    // Sync user to database
    let user = sync_user(&state.db, &user_info).await.map_err(map_err)?;

    // Ensure wallet exists; grant onboarding credits if new
    if let Ok(wallet) = get_or_create_wallet(&state.db, "user", &user.user_id).await {
        if wallet.total_deposited == 0 && wallet.balance == 0 {
            let _ =
                credit_grant(&state.db, wallet.wallet_id, 100, "Welcome onboarding grant").await;
        }
    }

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
        // Allow redirect to rabble.world (full URL) or internal path
        let dest = redirect_to
            .filter(|r| {
                (r.starts_with('/') && !r.contains("//")) || r.starts_with("https://rabble.world")
            })
            .unwrap_or_else(|| "/dashboard".to_string());
        let cookie = format!(
            "abw_session={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=604800",
            token
        );
        let mut builder = Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, &dest)
            .header(header::SET_COOKIE, &cookie);
        // If redirecting to rabble.world, also set cookie scoped to that domain
        if dest.starts_with("https://rabble.world") {
            let rabble_cookie = format!(
                "abw_session={}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=604800; Domain=rabble.world",
                token
            );
            builder = builder.header(header::SET_COOKIE, rabble_cookie);
        }
        builder
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

/// Get current authenticated user
pub async fn auth_me(principal: AuthPrincipal) -> Json<Value> {
    match principal {
        AuthPrincipal::User(user) => Json(json!({
            "user_id": user.user_id,
            "email": user.email,
            "display_name": user.display_name,
            "role": user.role,
            "auth_provider": user.auth_provider,
            "github_username": user.github_username,
        })),
        AuthPrincipal::ApiKey(key) => Json(json!({
            "user_id": key.user_id,
            "auth_type": "api_key",
            "key_name": key.name,
            "scopes": key.scopes,
        })),
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

    // Grant onboarding credits to new users
    if is_new {
        if let Ok(wallet) = get_or_create_wallet(&state.db, "user", &user.user_id).await {
            if wallet.total_deposited == 0 && wallet.balance == 0 {
                let _ = credit_grant(
                    &state.db,
                    wallet.wallet_id,
                    100,
                    "Welcome onboarding grant (SIWE)",
                )
                .await;
            }
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
