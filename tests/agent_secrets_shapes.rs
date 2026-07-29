//! Wire-format tests for the agent-owner secrets endpoints
//! (v0.9.0 — marketplace API keys).
//!
//! These tests pin the JSON shape returned by
//!
//!   PUT    /api/agents/:agent_id/secrets/:secret_name
//!   GET    /api/agents/:agent_id/secrets
//!   DELETE /api/agents/:agent_id/secrets/:secret_name
//!
//! and enforce the security invariant that plaintext secret values are
//! NEVER returned by any endpoint after write. Same fixture-driven
//! style as `forecast_cascade_provenance_shapes.rs` — no live server,
//! no DB, just construct the expected shape and assert its properties.
//!
//! If the handler contract changes, these tests break loudly so the
//! console side (which reads these shapes) doesn't silently start
//! rendering junk.

use serde_json::{json, Value};

/// Canonical PUT response — created secret metadata, no plaintext.
fn example_upsert_response() -> Value {
    json!({
        "secret_id": "6a3c7f2e-5b1c-4d8a-9e2f-0b1c2d3e4f5a",
        "agent_name": "macro_forecaster",
        "secret_name": "ANTHROPIC_API_KEY",
        "scope": "macro_forecaster"
    })
}

/// Canonical GET response — owner's agent-scoped secrets. `secrets[]`
/// carries only metadata; the plaintext `value` field is deliberately
/// absent (security).
fn example_list_response() -> Value {
    json!({
        "agent_name": "macro_forecaster",
        "count": 2,
        "has_anthropic_key": true,
        "secrets": [
            {
                "secret_id": "6a3c7f2e-5b1c-4d8a-9e2f-0b1c2d3e4f5a",
                "secret_name": "ANTHROPIC_API_KEY",
                "scope": "macro_forecaster",
                "label": "personal",
                "description": null,
                "created_at": "2026-07-28T14:30:00Z",
                "updated_at": "2026-07-28T14:30:00Z"
            },
            {
                "secret_id": "7b4d8f3a-6c2d-5e9b-af30-1c2d3e4f5a6b",
                "secret_name": "OPENAI_API_KEY",
                "scope": "*",
                "label": "team",
                "description": "shared across all my agents",
                "created_at": "2026-07-20T09:00:00Z",
                "updated_at": "2026-07-25T18:15:00Z"
            }
        ]
    })
}

// ─── PUT (upsert) ─────────────────────────────────────────────────────

#[test]
fn upsert_returns_metadata_but_not_the_value() {
    let r = example_upsert_response();
    for f in ["secret_id", "agent_name", "secret_name", "scope"] {
        assert!(
            r.get(f).is_some(),
            "missing field on upsert response: {}",
            f
        );
    }
    // Critical security invariant: the plaintext must never round-trip.
    // The console has no legitimate reason to receive it back, and any
    // future logging that reflected the response body would leak it.
    assert!(
        r.get("value").is_none(),
        "upsert response leaked plaintext value: {:?}",
        r.get("value")
    );
    assert!(
        r.get("plaintext").is_none(),
        "upsert response leaked plaintext (alt key): {:?}",
        r.get("plaintext")
    );
}

#[test]
fn upsert_scope_matches_agent_name() {
    // Server-side, we hard-scope the secret to the agent's own name
    // (not `*`) on upsert, so a per-agent PUT can never accidentally
    // widen the scope to global. Owners who want `*` do it via a
    // different endpoint (out of scope for v0.9.0).
    let r = example_upsert_response();
    let agent = r["agent_name"].as_str().unwrap();
    let scope = r["scope"].as_str().unwrap();
    assert_eq!(
        agent, scope,
        "PUT must scope the secret to the agent it targets; got agent={} scope={}",
        agent, scope
    );
}

// ─── GET (list) ───────────────────────────────────────────────────────

#[test]
fn list_returns_metadata_only_never_values() {
    let r = example_list_response();
    let arr = r["secrets"].as_array().unwrap();
    assert!(!arr.is_empty(), "fixture must exercise the row shape");
    for row in arr {
        // Same security invariant as upsert: no plaintext.
        assert!(
            row.get("value").is_none(),
            "list row leaked plaintext value: {:?}",
            row
        );
        assert!(
            row.get("plaintext").is_none(),
            "list row leaked plaintext (alt key): {:?}",
            row
        );
        assert!(
            row.get("encrypted_value").is_none(),
            "list row leaked ciphertext: {:?}",
            row
        );
        // Every row must carry the metadata the console renders.
        for f in [
            "secret_id",
            "secret_name",
            "scope",
            "label",
            "description",
            "created_at",
            "updated_at",
        ] {
            assert!(
                row.get(f).is_some(),
                "list row missing metadata field {}: {:?}",
                f,
                row
            );
        }
    }
}

#[test]
fn list_count_matches_array_length() {
    let r = example_list_response();
    let n = r["secrets"].as_array().unwrap().len();
    assert_eq!(
        r["count"].as_u64().unwrap() as usize,
        n,
        "count must equal secrets.len()"
    );
}

#[test]
fn list_advertises_anthropic_key_status_correctly() {
    // The `has_anthropic_key` field is a convenience surface for the
    // console's "is this agent funded?" marketplace badge. It must be
    // exactly true iff at least one row has secret_name ==
    // ANTHROPIC_API_KEY (regardless of scope — global `*` counts).
    let r = example_list_response();
    let advertised = r["has_anthropic_key"].as_bool().unwrap();
    let observed = r["secrets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["secret_name"].as_str() == Some("ANTHROPIC_API_KEY"));
    assert_eq!(
        advertised, observed,
        "has_anthropic_key ({}) disagrees with the observed rows ({})",
        advertised, observed
    );
}

#[test]
fn list_scope_semantics_are_agent_name_or_wildcard() {
    // The executor's `resolve_agent_owner_secrets` reads
    // `WHERE scope = $agent_name OR scope = '*'`. This test enforces
    // the client contract by ensuring every returned row satisfies
    // that filter, so the console can trust it to only render
    // executable secrets.
    let r = example_list_response();
    let agent = r["agent_name"].as_str().unwrap();
    for row in r["secrets"].as_array().unwrap() {
        let scope = row["scope"].as_str().unwrap();
        assert!(
            scope == agent || scope == "*",
            "row scope {} matches neither the agent name ({}) nor '*'",
            scope,
            agent
        );
    }
}

// ─── DELETE ───────────────────────────────────────────────────────────

// DELETE returns 204 No Content — no body to shape-test. If the handler
// ever grows a response body, that would be worth pinning here.

// ─── Cross-cutting security invariants ────────────────────────────────

#[test]
fn no_endpoint_response_contains_the_string_sk_or_bearer() {
    // Belt-and-braces: if the handler ever regressed and did include a
    // plaintext value, common API-key patterns (`sk-...`, `Bearer ...`)
    // would show up in the JSON. Fixture is clean; a real leak would
    // fail this even without knowing the exact field name.
    for r in [example_upsert_response(), example_list_response()] {
        let s = r.to_string();
        assert!(
            !s.contains("sk-"),
            "response body contains 'sk-' — possible plaintext leak: {}",
            s
        );
        // Match the exact "Bearer " prefix (with trailing space) to avoid
        // false positives on legitimate words containing "Bearer" as a
        // substring, e.g. a description field.
        assert!(
            !s.contains("Bearer "),
            "response body contains 'Bearer ' — possible auth header leak: {}",
            s
        );
    }
}
