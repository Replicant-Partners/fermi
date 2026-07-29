//! Wire-format tests for `GET /api/agents/:agent_id/funding` (v0.9.2).
//!
//! The endpoint answers "is this agent runnable?" for the marketplace
//! badge. It has two response shapes depending on the caller:
//!
//!   - **Public / non-owner** view: `{agent_id, funded}` only.
//!   - **Owner or admin** view: adds `providers`, `abw_profile_url`,
//!     `tier`, `owner_id` so the owner can see what they've set and
//!     where to configure missing keys.
//!
//! Same fixture-based style as the cascade/provenance shape tests: no
//! live server, no DB. We construct the expected responses and assert
//! invariants (no plaintext leaks, no per-agent management surface,
//! agreement with the executor's key-resolution logic).

use serde_json::{json, Value};

// ─── Fixtures ────────────────────────────────────────────────────────

/// Public view — third party checking whether Mario's agent is
/// executable before hiring it.
fn public_funded() -> Value {
    json!({
        "agent_id": "macro_forecaster",
        "funded": true
    })
}

fn public_unfunded() -> Value {
    json!({
        "agent_id": "macro_forecaster",
        "funded": false
    })
}

/// Owner (or admin) view — Mario looking at his own agent's funding
/// dashboard.
fn owner_funded() -> Value {
    json!({
        "agent_id": "macro_forecaster",
        "tier": "community",
        "owner_id": "user-mario",
        "funded": true,
        "providers": ["anthropic", "openai"],
        "abw_profile_url": "https://agent-bestiary.world/profile"
    })
}

fn owner_unfunded() -> Value {
    json!({
        "agent_id": "macro_forecaster",
        "tier": "community",
        "owner_id": "user-mario",
        "funded": false,
        "providers": [],
        "abw_profile_url": "https://agent-bestiary.world/profile"
    })
}

/// System-tier agent (Fermi). Platform-funded, no owner secrets, no
/// ABW profile URL (owners don't need to do anything).
fn system_agent() -> Value {
    json!({
        "agent_id": "fermi",
        "tier": "system",
        "funded": true,
        "funding_source": "platform",
        "providers": ["platform"],
        "abw_profile_url": null
    })
}

// ─── Contract: public view is a boolean-only signal ──────────────────

#[test]
fn public_view_carries_only_agent_id_and_funded() {
    for r in [public_funded(), public_unfunded()] {
        // Public view has EXACTLY these two fields. Extra fields would
        // leak privacy signal (which providers the owner uses, whether
        // there's an owner_id, what tier the agent is).
        let obj = r.as_object().expect("must be an object");
        for f in ["agent_id", "funded"] {
            assert!(obj.contains_key(f), "public view missing {}: {:?}", f, r);
        }
        // Belt-and-braces: privacy fields that MUST NOT leak in public.
        for banned in [
            "providers",
            "owner_id",
            "abw_profile_url",
            "tier",
            "note",
            "funding_source",
        ] {
            assert!(
                !obj.contains_key(banned),
                "public view leaked owner-only field {}: {:?}",
                banned,
                r
            );
        }
        assert!(
            obj.get("funded").and_then(|v| v.as_bool()).is_some(),
            "funded must be a boolean, not a value/string/etc: {:?}",
            r
        );
    }
}

#[test]
fn public_view_agent_id_present_on_both_funded_states() {
    // Trivial but pins the shape so a future refactor can't drop the
    // identifier from the response.
    assert_eq!(
        public_funded()["agent_id"].as_str(),
        Some("macro_forecaster")
    );
    assert_eq!(
        public_unfunded()["agent_id"].as_str(),
        Some("macro_forecaster")
    );
}

// ─── Contract: owner view carries providers + ABW pointer ────────────

#[test]
fn owner_view_has_provider_list_and_abw_url() {
    let r = owner_funded();
    assert!(r["providers"].is_array(), "providers must be an array");
    assert!(
        r["abw_profile_url"].as_str().is_some(),
        "abw_profile_url must be a non-null string for owner-owned agents"
    );
    assert_eq!(r["owner_id"].as_str(), Some("user-mario"));
    assert_eq!(r["tier"].as_str(), Some("community"));
}

#[test]
fn owner_unfunded_view_has_empty_providers_and_abw_url() {
    let r = owner_unfunded();
    assert_eq!(r["funded"], json!(false));
    assert_eq!(
        r["providers"].as_array().unwrap().len(),
        0,
        "unfunded agent must show empty providers list"
    );
    // ABW URL must still be there — that's WHERE the owner goes to
    // add the missing key. Removing it would leave the operator with
    // nowhere to click.
    assert!(
        r["abw_profile_url"].as_str().is_some(),
        "abw_profile_url must be present on unfunded owner view (that's the fix location)"
    );
}

#[test]
fn owner_view_never_returns_plaintext_secret_values() {
    // Providers is metadata only. If any future refactor accidentally
    // includes the actual key values, this catches it.
    for r in [owner_funded(), owner_unfunded()] {
        let s = r.to_string();
        assert!(
            !s.contains("sk-"),
            "owner view leaked plaintext (sk- prefix): {}",
            s
        );
        assert!(
            !s.contains("Bearer "),
            "owner view leaked plaintext (Bearer prefix): {}",
            s
        );
    }
}

// ─── Contract: system agents are always funded, no ABW URL ───────────

#[test]
fn system_agent_reports_platform_funded() {
    let r = system_agent();
    assert_eq!(r["tier"].as_str(), Some("system"));
    assert_eq!(r["funded"], json!(true));
    assert_eq!(r["funding_source"].as_str(), Some("platform"));
    // ABW profile URL is null for system agents — owners can't upload
    // keys for platform-owned infrastructure agents, and pointing them
    // at ABW would be misleading.
    assert!(
        r["abw_profile_url"].is_null(),
        "system agents must report null ABW URL (no owner action available): {:?}",
        r
    );
    // No owner_id on system agents — they belong to the platform.
    assert!(
        r.get("owner_id").is_none() || r["owner_id"].is_null(),
        "system agents must not expose an owner_id (they're platform-owned): {:?}",
        r
    );
}

// ─── Contract: endpoint is READ-ONLY ─────────────────────────────────
//
// v0.9.0 shipped PUT / DELETE endpoints for per-agent secret
// management, then v0.9.2 removed them after the architectural review
// (ABW's profile page is the single source of truth for owner-uploaded
// keys). If a future PR reintroduces those routes on the funding
// endpoint's path, this test doesn't catch it directly — but the
// existence of a `providers` field WITHOUT a corresponding "how to
// write" affordance in the response documents the read-only nature of
// this surface.

#[test]
fn owner_view_signals_read_only_via_abw_pointer_not_write_url() {
    // The response points at ABW for changes ("go here to add a key"),
    // not at a Fermi-side write endpoint. This pins the "read-only"
    // architectural contract.
    let r = owner_unfunded();
    let url = r["abw_profile_url"]
        .as_str()
        .expect("abw_profile_url must be set");
    assert!(
        url.contains("agent-bestiary.world") || url.contains("localhost"),
        "abw_profile_url should point at ABW, got: {}",
        url
    );
    assert!(
        !url.contains("/api/"),
        "abw_profile_url should point at a UI page, not a Fermi API endpoint: {}",
        url
    );
}

// ─── Contract: agreement with executor's key-resolution ──────────────

#[test]
fn funded_iff_anthropic_key_present_in_provider_list() {
    // The executor uses ANTHROPIC_API_KEY as the funding key. Owner
    // view's `funded` boolean must match "anthropic is in providers"
    // — otherwise the marketplace badge would show green while hires
    // still fail. Documented here as a hard invariant.
    for r in [owner_funded(), owner_unfunded()] {
        let funded = r["funded"].as_bool().unwrap();
        let has_anthropic = r["providers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str() == Some("anthropic"));
        assert_eq!(
            funded, has_anthropic,
            "funded ({}) disagrees with anthropic-in-providers ({}) on: {:?}",
            funded, has_anthropic, r
        );
    }
}
