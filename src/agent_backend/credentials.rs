//! Per-execution provider credentials.
//!
//! Implements SPEC_28 (`docs/specs/SPEC_28_UNIFIED_CREDENTIAL_PATH.md`),
//! which finishes applying `AGENT_CREDENTIAL_MODEL.md` §5 to the executor
//! layer: *"No env branch for agent keys."*
//!
//! # The problem this type exists to solve
//!
//! Credentials used to be bound at **executor construction** — the
//! process-wide `LLMExecutor` / `MultiModelExecutor` singletons captured
//! one key per provider from env at boot. A singleton built that way
//! cannot be per-agent funded, so every execution path that reached it
//! silently ran on the platform's key regardless of who owned the agent.
//!
//! The workaround (`ToolContext.user_secrets`) was honoured by exactly one
//! of four executor branches, and `ToolContext` is a *tool* abstraction —
//! execution paths that need no tools never built one, so they carried no
//! credentials at all.
//!
//! Consequence, measured: 17 of 96 curated agent cards bypass the tool
//! loop (structured-output contract or no tools registered), and 16 of
//! those declare `anthropic` — so they *succeeded* on the platform key.
//! A silent cross-tenant billing leak, not a visible error.
//!
//! # The model
//!
//! Credentials are bound to the **execution**, carried on
//! `ExecutionContext`, and resolved once per execution from the
//! `agent_credentials` store (never env). Executors become
//! credential-stateless, which is what makes sharing the startup
//! singleton correct again.
//!
//! An agent's funding must not depend on the shape of its output. That is
//! the invariant this type enforces: there is one way to obtain a key
//! (`key_for`), reachable identically from every executor branch.

use crate::agent_backend::executor::ExecutionError;
use std::collections::HashMap;
use std::sync::Arc;

/// How a credential was resolved. Recorded for telemetry and for honest
/// error messages; also the value the agent Manage page surfaces so an
/// owner can see *which* key powered a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    /// `(principal, provider, scope = agent_name)` — per-agent funding.
    /// The funding-isolation primitive: a bad key or blown quota is
    /// contained to one agent.
    AgentScoped,
    /// `(principal, provider, scope = '*')` — the principal's default.
    PrincipalDefault,
    /// Legacy `user_secrets` named `<PROVIDER>_API_KEY`. Transitional;
    /// removed with SPEC_28 P5.4 once every funded owner has store rows.
    LegacyUserSecrets,
    /// Nothing resolved. `key_for` fails loudly rather than falling
    /// through to a platform key.
    Unfunded,
}

impl CredentialSource {
    /// Stable string for telemetry / API responses.
    pub fn as_str(&self) -> &'static str {
        match self {
            CredentialSource::AgentScoped => "agent_scoped",
            CredentialSource::PrincipalDefault => "principal_default",
            CredentialSource::LegacyUserSecrets => "legacy_user_secrets",
            CredentialSource::Unfunded => "unfunded",
        }
    }
}

/// Provider credentials resolved for ONE execution.
///
/// Construct via `ResolvedCredentials::builder()` (server-side, from the
/// credential store) or `ResolvedCredentials::unfunded()` (tests, mock
/// executions, and any path that must not be able to spend money).
///
/// Cheap to clone the `Arc`; the map itself is never cloned per call.
#[derive(Clone, Debug, Default)]
pub struct ResolvedCredentials {
    /// provider name → plaintext key.
    keys: HashMap<String, String>,
    /// provider name → how that key was found.
    sources: HashMap<String, CredentialSource>,
    /// Principal whose budget bears the raw LLM cost (`abw-system` for
    /// platform-service agents, else the agent's owner). `None` when
    /// unfunded. Surfaced on episodes so funding is observable per run.
    funding_principal: Option<String>,
}

impl ResolvedCredentials {
    /// Explicitly unfunded. Any `key_for` call fails with a named error.
    ///
    /// This is the correct default: a new execution path that forgets to
    /// resolve credentials fails loudly instead of quietly spending the
    /// platform's money.
    pub fn unfunded() -> Self {
        Self::default()
    }

    /// Convenience: an `Arc`-wrapped unfunded set, the shape
    /// `ExecutionContext.credentials` wants.
    pub fn unfunded_arc() -> Arc<Self> {
        Arc::new(Self::unfunded())
    }

    pub fn builder() -> ResolvedCredentialsBuilder {
        ResolvedCredentialsBuilder::default()
    }

    /// The principal bearing cost for this execution, if funded.
    pub fn funding_principal(&self) -> Option<&str> {
        self.funding_principal.as_deref()
    }

    /// How `provider`'s key was resolved (`Unfunded` if absent).
    pub fn source_for(&self, provider: &str) -> CredentialSource {
        self.sources
            .get(provider)
            .copied()
            .unwrap_or(CredentialSource::Unfunded)
    }

    /// True if this execution can pay for `provider`.
    pub fn has(&self, provider: &str) -> bool {
        provider_needs_no_key(provider) || self.keys.contains_key(provider)
    }

    /// Providers this execution can pay for, for telemetry / debugging.
    pub fn funded_providers(&self) -> Vec<&str> {
        self.keys.keys().map(String::as_str).collect()
    }

    /// **The only way an executor obtains an API key.**
    ///
    /// `agent_id` is supplied by the caller (every executor has it on the
    /// card) so the error can name the agent without this type having to
    /// duplicate agent identity.
    ///
    /// Never consults env. Never falls back to a platform key: doing so is
    /// precisely the shared-pool leak SPEC_28 exists to close.
    pub fn key_for(&self, provider: &str, agent_id: &str) -> Result<&str, ExecutionError> {
        // Providers that legitimately need no credential (local Ollama).
        if provider_needs_no_key(provider) {
            return Ok("");
        }
        match self.keys.get(provider) {
            Some(k) if !k.trim().is_empty() => Ok(k.as_str()),
            _ => Err(ExecutionError::Unfunded {
                agent_id: agent_id.to_string(),
                provider: provider.to_string(),
            }),
        }
    }
}

/// Providers that authenticate by network locality rather than a key.
pub fn provider_needs_no_key(provider: &str) -> bool {
    provider == "ollama"
}

/// Is this agent tier funded by the platform (`abw-system`) rather than
/// by the account in its `owner_id`?
///
/// **`system` is not the only platform tier.** `curated` agents are
/// platform-authored, platform-operated, and have always been
/// platform-funded; they merely carry a human admin account in `owner_id`
/// because AGENT_CREDENTIAL_MODEL P5 ("migrate platform-service agents to
/// the abw-system owner") has not run yet.
///
/// Getting this wrong is not a subtle failure. When SPEC_28 removed the
/// executor's env fallback, the credential resolver mapped only
/// `tier == "system"` to `abw-system`; `curated` fell through to its
/// nominal owner — an account with zero stored credentials — and all 78
/// curated agents, the entire Fermi orchestra, began hard-failing
/// `Unfunded` the moment it deployed.
///
/// Lives in the lib rather than the binary so it is reachable from tests.
pub fn is_platform_funded(tier: &str) -> bool {
    tier.eq_ignore_ascii_case("system") || tier.eq_ignore_ascii_case("curated")
}

/// Public base URL for owner-facing remediation messages. `ABW_BASE_URL`
/// is operator config (which deployment this is), not a credential, so
/// reading it from env here does not violate SPEC_28.
pub fn abw_base_url() -> String {
    std::env::var("ABW_BASE_URL").unwrap_or_else(|_| "https://agent-bestiary.world".to_string())
}

#[derive(Default)]
pub struct ResolvedCredentialsBuilder {
    inner: ResolvedCredentials,
}

impl ResolvedCredentialsBuilder {
    /// Record a resolved key. Empty keys are ignored so a blank store row
    /// reads as unfunded rather than producing a 401 at the provider.
    pub fn key(
        mut self,
        provider: impl Into<String>,
        value: impl Into<String>,
        source: CredentialSource,
    ) -> Self {
        let value = value.into();
        if value.trim().is_empty() {
            return self;
        }
        let provider = provider.into();
        self.inner.sources.insert(provider.clone(), source);
        self.inner.keys.insert(provider, value);
        self
    }

    pub fn funding_principal(mut self, principal: impl Into<String>) -> Self {
        self.inner.funding_principal = Some(principal.into());
        self
    }

    pub fn build(self) -> ResolvedCredentials {
        self.inner
    }

    pub fn build_arc(self) -> Arc<ResolvedCredentials> {
        Arc::new(self.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfunded_fails_loudly_and_names_the_agent_and_provider() {
        let c = ResolvedCredentials::unfunded();
        let err = c.key_for("deepseek", "valuation_agent").unwrap_err();
        match err {
            ExecutionError::Unfunded { agent_id, provider } => {
                assert_eq!(agent_id, "valuation_agent");
                assert_eq!(provider, "deepseek");
            }
            other => panic!("expected Unfunded, got {:?}", other),
        }
        // The rendered message must point the OWNER at their profile,
        // not tell them to set a server env var they cannot reach.
        let msg = c
            .key_for("deepseek", "valuation_agent")
            .unwrap_err()
            .to_string();
        assert!(msg.contains("DEEPSEEK_API_KEY"), "names the key: {msg}");
        assert!(msg.contains("profile"), "points at the profile page: {msg}");
        assert!(
            !msg.contains("env var"),
            "must not instruct the owner to set a server env var: {msg}"
        );
    }

    #[test]
    fn ollama_needs_no_key() {
        let c = ResolvedCredentials::unfunded();
        assert_eq!(c.key_for("ollama", "local_agent").unwrap(), "");
        assert!(c.has("ollama"));
    }

    #[test]
    fn resolved_key_is_returned_with_its_source() {
        let c = ResolvedCredentials::builder()
            .funding_principal("mario")
            .key("deepseek", "sk-abc", CredentialSource::AgentScoped)
            .build();
        assert_eq!(c.key_for("deepseek", "valuation_agent").unwrap(), "sk-abc");
        assert_eq!(c.source_for("deepseek"), CredentialSource::AgentScoped);
        assert_eq!(c.funding_principal(), Some("mario"));
        // A provider we hold no key for is still unfunded — no cross-
        // provider bleed.
        assert!(c.key_for("anthropic", "valuation_agent").is_err());
        assert_eq!(c.source_for("anthropic"), CredentialSource::Unfunded);
    }

    #[test]
    fn blank_keys_are_treated_as_absent() {
        let c = ResolvedCredentials::builder()
            .key("anthropic", "   ", CredentialSource::PrincipalDefault)
            .build();
        assert!(c.key_for("anthropic", "a").is_err());
        assert!(!c.has("anthropic"));
    }
}
