//! LLM spend control, classified by handler (LLD G17).
//!
//! The limiter used to be applied by a middleware matching request paths
//! against `LLM_SPEND_ROUTES` [V: `src/api_server.rs:244-263` @ 51235f00].
//! A path list is a declaration about handlers that lives somewhere else, and
//! it drifted: the workspace message, coherence evaluate, claim evaluation,
//! carbon, BOM pricing, forage, flock and MCP paths ran the tool loop and were
//! not on it. It also charged `/execute` and `/execute/stream` twice, once in
//! the middleware and once inline.
//!
//! Now the handler declares its spend:
//!
//! * a handler that awaits a model call on the request path takes the
//!   [`LlmSpend`] extractor (or checks inline, as `execute`, `stream`, `a2a`
//!   and `mcp` do), so an over-limit request is refused with 429 before any
//!   work;
//! * a handler whose model call runs in a spawned task asks [`background`]
//!   inside that task, so an over-limit dispatch is skipped and logged while
//!   the user's action itself (a presence change, a flight record) still
//!   succeeds.
//!
//! `tests/claims/llm_spend.rs` checks that every handler reaching a model
//! call does one of the two.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use fermi::gate_trust::{decided, Decision, Gate};
use fermi_auth::AuthPrincipal;

use crate::AppState;

/// Proof that this request was admitted by the LLM limiter.
pub struct LlmSpend;

/// One admission against the per-user LLM limiter, recorded to
/// `Gate::RateLimit` either way. `Err` carries the retry-after seconds.
pub(crate) fn admit(state: &AppState, key: &str, site: &'static str) -> Result<u32, u64> {
    match state.rate_limits.llm.check(key) {
        Ok(remaining) => {
            decided(Gate::RateLimit, Decision::Approved, None);
            Ok(remaining)
        }
        Err(retry) => {
            decided(Gate::RateLimit, Decision::Refused, Some(site));
            Err(retry)
        }
    }
}

#[async_trait]
impl FromRequestParts<AppState> for LlmSpend {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Keyed on the principal `auth_middleware` resolved; an unauthenticated
        // request on a protected route never reaches here.
        let key = match parts.extensions.get::<AuthPrincipal>() {
            Some(p) => format!("user:{}", p.user_id()),
            None => {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    "LLM spend requires an authenticated principal".into(),
                ))
            }
        };
        admit(state, &key, "llm limiter: handler")
            .map(|_| LlmSpend)
            .map_err(|retry| {
                (
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "LLM rate limit exceeded ({}/min). Retry after {retry} seconds.",
                        state.rate_limits.llm.max_requests
                    ),
                )
            })
    }
}

/// Admission for a model call made from a spawned task. `false` means skip
/// the call: the limiter refused it and the refusal is recorded.
pub(crate) fn background(state: &AppState, user_id: &str, site: &'static str) -> bool {
    match admit(state, &format!("user:{user_id}"), site) {
        Ok(_) => true,
        Err(retry) => {
            tracing::warn!(
                user = %user_id,
                site,
                retry_after_secs = retry,
                "background LLM dispatch skipped: rate limit"
            );
            false
        }
    }
}
