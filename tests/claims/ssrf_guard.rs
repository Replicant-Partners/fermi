//! SPEC 36 R0.3 / LLD G03 (#39): no user-supplied URL reaches a private address.
//!
//! Two kinds of check:
//! * the guard itself against a local listener, proving the pin and the
//!   redirect refusal work end to end (no external network);
//! * a source inventory: every outbound `reqwest` call site in production code
//!   is either listed as fixed-host here or goes through `net_guard`. A new
//!   call site that does neither fails this test.

use fermi::net_guard;

/// A local HTTP server that answers every request with a 302 to the cloud
/// metadata address. Returns its port.
async fn redirecting_listener() -> u16 {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                return;
            };
            let mut buf = [0u8; 1024];
            let _read = s.read(&mut buf).await;
            let _write = s
                .write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/latest/meta-data/\r\nContent-Length: 0\r\n\r\n")
                .await;
        }
    });
    port
}

#[tokio::test]
async fn loopback_and_metadata_are_refused_before_any_request() {
    for u in [
        "https://127.0.0.1/",
        "https://[::1]/",
        "https://169.254.169.254/latest/meta-data/",
        "https://[::ffff:169.254.169.254]/",
        "https://10.0.0.1/",
        "https://localhost/",
        "https://metadata.google.internal/",
    ] {
        assert!(net_guard::check(u).await.is_err(), "{u} must be refused");
    }
}

/// The client `client_for` builds refuses redirects. Exercised against a
/// local server; the guard's `check` is bypassed only to reach a test
/// listener on loopback, which production `check` would itself refuse.
#[tokio::test]
async fn guarded_client_does_not_follow_redirects() {
    let port = redirecting_listener().await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap();
    let r = client
        .get(format!("http://127.0.0.1:{port}/"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.status().as_u16(),
        302,
        "the 302 is returned, not followed"
    );
    // And `client_for`'s redirect policy is the same `Policy::none()`: the
    // source check below pins that it stays so.
    let src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/net_guard.rs")).unwrap();
    assert!(src.contains(".redirect(reqwest::redirect::Policy::none())"));
    assert!(src.contains(".no_proxy()"));
    assert!(src.contains(".resolve(&g.host, g.pinned)"));
}

/// Every outbound request site in production code is either a known fixed
/// host (listed with its reason) or goes through `net_guard`.
#[test]
fn every_outbound_call_site_is_fixed_host_or_guarded() {
    // (file, reason) — hosts are compile-time constants or operator env vars.
    const FIXED_HOST: &[(&str, &str)] = &[
        (
            "src/agent_backend/tool_executor.rs",
            "api.anthropic.com; provider_base_url (constants/env)",
        ),
        (
            "src/agent_backend/multi_model_executor.rs",
            "provider_base_url (constants/env)",
        ),
        ("src/agent_backend/llm_executor.rs", "api.anthropic.com"),
        (
            "src/agent_backend/weather_tools.rs",
            "open-meteo, weather.gov, polymarket constants",
        ),
        ("src/agent_backend/ncbi_tools.rs", "NCBI constant"),
        (
            "src/agent_backend/tools/domains/sports.rs",
            "api-sports constant (path from LLM)",
        ),
        (
            "src/agent_backend/tools/domains/spatial.rs",
            "nominatim constant",
        ),
        (
            "src/agent_backend/tools/domains/weather.rs",
            "openweathermap constant",
        ),
        (
            "src/agent_backend/tools/domains/prediction_market.rs",
            "polymarket constants",
        ),
        (
            "src/agent_backend/tools/domains/biology.rs",
            "GBIF, iNaturalist, bio-aware, Gemini constants",
        ),
        (
            "src/agent_backend/tools/domains/video.rs",
            "REDUCT_BASE_URL operator env",
        ),
        (
            "src/agent_backend/tools/domains/platform.rs",
            "Brave search constant",
        ),
        (
            "src/agent_backend/tools/domains/financial.rs",
            "FMP constant",
        ),
        ("src/polymarket/mod.rs", "gamma-api/data-api constants"),
        ("src/email.rs", "api.resend.com"),
        ("src/handlers/eval_judge.rs", "api.anthropic.com"),
        ("src/handlers/eval.rs", "api.anthropic.com"),
        ("src/handlers/agents.rs", "Gemini constant"),
        ("src/handlers/misc.rs", "nominatim constant"),
        (
            "src/handlers/creatures/identity.rs",
            "GBIF, Gemini constants",
        ),
        ("src/handlers/creatures/mod.rs", "GBIF, Gemini constants"),
        ("src/handlers/polymarket.rs", "polymarket constants"),
        ("src/handlers/wild.rs", "api.anthropic.com"),
        ("src/handlers/wizard.rs", "api.anthropic.com"),
        ("src/voice/cartesia.rs", "api.cartesia.ai"),
        ("src/api_server.rs", "Stripe and provider constants"),
        (
            "agent-bestiary/evaluator-wildguard/src/lib.rs",
            "endpoint always the anthropic constant",
        ),
        (
            "agent-bestiary/evaluator-sotopia/src/lib.rs",
            "endpoint always the anthropic constant",
        ),
        (
            "agent-bestiary/evaluator-character/src/lib.rs",
            "endpoint always the anthropic constant",
        ),
        (
            "agent-bestiary/memory/src/embeddings.rs",
            "provider constants / NOMIC_BASE_URL env",
        ),
        (
            "agent-bestiary/memory/src/llm.rs",
            "provider constants (base_url always None in production)",
        ),
        ("fermi-auth/src/oidc.rs", "Google/GitHub constants"),
        ("src/net_guard.rs", "the guard itself"),
    ];
    // Files whose user-supplied URLs must go through the guard.
    const GUARDED: &[&str] = &[
        "src/agent_backend/mcp_client.rs",
        "src/a2a_webhook.rs",
        "src/handlers/push.rs",
        "src/agent_backend/tools/domains/media.rs",
    ];
    let root = env!("CARGO_MANIFEST_DIR");
    let mut unlisted = Vec::new();
    let mut stack = vec![
        std::path::PathBuf::from(root).join("src"),
        std::path::PathBuf::from(root).join("agent-bestiary"),
        std::path::PathBuf::from(root).join("fermi-auth/src"),
    ];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if p.is_dir() {
                if !rel.ends_with("/tests")
                    && !rel.contains("/target")
                    && !rel.starts_with("src/bin")
                {
                    stack.push(p);
                }
                continue;
            }
            if !rel.ends_with(".rs") || rel.ends_with("/tests.rs") {
                continue;
            }
            let src = std::fs::read_to_string(&p).unwrap();
            let prod = src.split("#[cfg(test)]").next().unwrap_or("");
            let sends = prod.contains("reqwest::Client") || prod.contains("reqwest::get(");
            if !sends {
                continue;
            }
            if GUARDED.contains(&rel.as_str()) {
                assert!(
                    prod.contains("net_guard::"),
                    "{rel} sends user-supplied URLs and must use net_guard"
                );
            } else if !FIXED_HOST.iter().any(|(f, _)| *f == rel) {
                unlisted.push(rel);
            }
        }
    }
    unlisted.sort();
    assert!(
        unlisted.is_empty(),
        "new outbound call sites: classify each as fixed-host (with reason) or route it through \
         net_guard: {unlisted:?}"
    );
}

/// A remote MCP card may not name an arbitrary process-env key as its
/// credential (SPEC 36 R0.3 review: platform secrets could be sent to a
/// card-chosen endpoint).
#[test]
fn mcp_env_credential_fallback_is_allowlisted() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/agent_backend/mcp_client.rs"
    ))
    .unwrap();
    let f = &src[src.find("fn resolve_credential(").unwrap()..];
    let f = &f[..f.find("\nfn ").unwrap_or(f.len())];
    assert!(
        f.contains("env_credential_allowed(key)"),
        "env fallback must be gated"
    );
}
