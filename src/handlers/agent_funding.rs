//! Agent funding status (v0.9.2 — marketplace signal).
//!
//! Single endpoint:
//!
//!   GET /api/agents/:agent_id/funding
//!
//! Returns whether an agent is executable — i.e. whether its owner has
//! uploaded the API keys required to actually run it. The console
//! renders this as a green/grey "funded" badge on marketplace cards so
//! operators don't hire dead-end agents.
//!
//! # Model
//!
//! Per the marketplace architecture (see
//! `docs/fermi/FERMI_CHAT_AND_AGENT_CREATION_DESIGN.md` and v0.9.0's
//! `resolve_agent_owner_secrets`):
//!
//!   - **System-tier agents** (Fermi, xaman_ek) are platform-funded via
//!     the server's `ANTHROPIC_API_KEY` env var. Always report funded.
//!   - **Owner-owned agents** are funded iff their owner has stored
//!     `ANTHROPIC_API_KEY` under a globally-scoped (`scope='*'`) or
//!     per-agent (`scope=agent_name`) secret. Owners set these via
//!     ABW's profile page: `https://agent-bestiary.world/profile`.
//!
//! # Auth + privacy
//!
//! Two response shapes depending on caller:
//!
//!   - **Public / non-owner** view: just `{funded: bool}`. Anyone who
//!     can see the agent can see whether it's runnable, so marketplace
//!     cards can render the badge without leaking provider details.
//!   - **Owner / admin** view: adds `providers`, `abw_profile_url`,
//!     `tier`, `owner_id` — lists which providers have keys so the
//!     owner can see what they've set. Provider list is metadata only;
//!     plaintext key values are never returned by any endpoint.
//!
//! # Not a management endpoint
//!
//! This endpoint deliberately does not accept PUT / DELETE / POST.
//! v0.9.0 shipped those (per-agent scope) and v0.9.2 removed them
//! after the architectural review clarified that ABW's profile page is
//! the single source of truth for owner secrets. Owners upload keys on
//! ABW; the console only reads status from here.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde_json::{json, Value};

use crate::{abw_profile_url, resolve_agent, AppState};

/// Which provider secrets we surface in the owner-view response.
/// Anthropic is the only one currently routed through the executor's
/// key resolution; the design extends naturally to OpenAI, Mistral,
/// etc. when we wire their executor paths in v0.9.3+.
const KNOWN_PROVIDER_SECRETS: &[(&str, &str)] = &[
    ("ANTHROPIC_API_KEY", "anthropic"),
    ("OPENAI_API_KEY", "openai"),
    ("MISTRAL_API_KEY", "mistral"),
    ("QWEN_API_KEY", "qwen"),
    ("DEEPSEEK_API_KEY", "deepseek"),
    ("OPENROUTER_API_KEY", "openrouter"),
    ("KIMI_API_KEY", "kimi"),
    ("GLM_API_KEY", "zhipu"),
    ("GEMINI_API_KEY", "gemini"),
];

/// GET /api/agents/:agent_id/funding
///
/// Auth: any authenticated user can hit this to see the boolean; owner
/// (or admin) additionally sees the provider list + the ABW profile URL
/// where they'd add missing keys.
pub async fn get_agent_funding_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = resolve_agent(&state, &agent_id).await?;
    let caller_id = principal.user_id();
    let is_admin = principal.can_admin();
    let is_owner = agent
        .owner_id
        .as_deref()
        .map(|o| o == caller_id)
        .unwrap_or(false);
    let is_privileged = is_owner || is_admin;

    // System agents (Fermi, xaman_ek, other infra): platform-funded.
    // Report funded=true unconditionally. Owners of system agents (the
    // platform account itself) get the same response as non-owners —
    // no ABW profile URL because the platform env var is the source.
    if agent.tier.eq_ignore_ascii_case("system") {
        return Ok(Json(if is_privileged {
            json!({
                "agent_id": agent.agent_name,
                "tier": "system",
                "funded": true,
                "funding_source": "platform",
                "providers": ["platform"],
                "abw_profile_url": null,
            })
        } else {
            json!({
                "agent_id": agent.agent_name,
                "funded": true,
            })
        }));
    }

    // Owner-owned agent: look up the owner's secrets. Reads the same
    // path the executor uses (get_secrets_for_agent), so this endpoint
    // and the executor agree by construction on what "funded" means.
    let encryptor = match state.secret_encryptor.as_ref() {
        Some(e) => e,
        None => {
            // Secrets subsystem unconfigured. Everything looks unfunded
            // by definition; return that honestly.
            return Ok(Json(if is_privileged {
                json!({
                    "agent_id": agent.agent_name,
                    "tier": agent.tier,
                    "funded": false,
                    "providers": [],
                    "abw_profile_url": abw_profile_url(),
                    "note": "Server secrets subsystem is not configured (SECRETS_ENCRYPTION_KEY missing).",
                })
            } else {
                json!({
                    "agent_id": agent.agent_name,
                    "funded": false,
                })
            }));
        }
    };
    let owner_id = match agent.owner_id.as_deref() {
        Some(o) => o,
        None => {
            // Owner-less non-system agent. Data-integrity issue but
            // report as unfunded so the marketplace hides it.
            return Ok(Json(if is_privileged {
                json!({
                    "agent_id": agent.agent_name,
                    "tier": agent.tier,
                    "funded": false,
                    "providers": [],
                    "abw_profile_url": abw_profile_url(),
                    "note": "Agent has no owner_id; marketplace treats it as unfunded.",
                })
            } else {
                json!({
                    "agent_id": agent.agent_name,
                    "funded": false,
                })
            }));
        }
    };

    // The executor calls this same helper at hire time. Using it here
    // means the endpoint's answer is definitionally the same signal the
    // executor sees — no drift possible between "marketplace says
    // funded" and "executor runs successfully".
    let secrets =
        fermi_auth::get_secrets_for_agent(&state.db, encryptor, owner_id, &agent.agent_name)
            .await
            .unwrap_or_default();

    // Which of the KNOWN_PROVIDER_SECRETS the owner has set. Currently
    // funding = has Anthropic (the only executor-wired path). When
    // v0.9.3+ wires other providers, this stays honest — secrets
    // present but not yet routed still show up in the provider list
    // for the owner's info, but `funded` reflects only what the
    // executor can actually use.
    let mut providers: Vec<&'static str> = Vec::new();
    for (secret_name, provider_label) in KNOWN_PROVIDER_SECRETS {
        if secrets.contains_key(*secret_name) {
            providers.push(provider_label);
        }
    }
    let funded = secrets.contains_key("ANTHROPIC_API_KEY");

    let response = if is_privileged {
        json!({
            "agent_id": agent.agent_name,
            "tier": agent.tier,
            "owner_id": owner_id,
            "funded": funded,
            "providers": providers,
            "abw_profile_url": abw_profile_url(),
        })
    } else {
        // Non-owner view: boolean only. Preserves the marketplace
        // signal without revealing which specific providers the owner
        // uses (privacy — a competitor listing agents shouldn't be
        // able to profile which LLM vendors the owner has accounts
        // with).
        json!({
            "agent_id": agent.agent_name,
            "funded": funded,
        })
    };

    Ok(Json(response))
}
