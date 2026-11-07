//! SPEC 36 R0.3 / LLD G03 (#39): no user-supplied URL reaches a private address.
//!
//! Two kinds of check:
//! * the guard refuses internal targets before any request (the pin and
//!   redirect refusal are unit-tested in `src/net_guard.rs`);
//! * a source inventory: every outbound `reqwest` call site in production code
//!   is either listed as fixed-host here or goes through `net_guard`. A new
//!   call site that does neither fails this test.

use fermi::net_guard;

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

// The pin and the redirect refusal are exercised end to end against a local
// listener by `net_guard::tests::client_for_pins_and_refuses_redirects`
// (src/net_guard.rs), which builds the production `client_for` client.

/// Every outbound request in production code is either a known fixed-host
/// call or goes through `net_guard`.
///
/// Counted **per call**, not per file: every `.send()` in a production file
/// is counted, and each file is listed with the exact number it has. A new
/// send in any file changes its count and fails the test, so it cannot slip
/// into a file that is already listed. Comments and `#[cfg(test)] mod` blocks
/// are stripped. A file holding guarded sends must contain at least as many
/// `net_guard::guarded_client(` calls as it has guarded sends; which send
/// uses which client is checked by review, not by this scan.
#[test]
fn every_outbound_call_site_is_fixed_host_or_guarded() {
    let root = env!("CARGO_MANIFEST_DIR");
    let counts = outbound_send_counts(root);
    // (file or crate prefix, `.send()` calls, host). Hosts are constants or operator env.
    let fixed: &[(&str, usize, &str)] = &[
        (
            "src/agent_backend/tool_executor.rs",
            2,
            "anthropic; provider_base_url constants/env",
        ),
        (
            "src/agent_backend/multi_model_executor.rs",
            1,
            "provider_base_url constants/env",
        ),
        ("src/agent_backend/llm_executor.rs", 1, "api.anthropic.com"),
        (
            "src/agent_backend/weather_tools.rs",
            1,
            "open-meteo, weather.gov, polymarket",
        ),
        ("src/agent_backend/ncbi_tools.rs", 2, "NCBI"),
        ("src/agent_backend/tools/domains/sports.rs", 1, "api-sports"),
        ("src/agent_backend/tools/domains/spatial.rs", 1, "nominatim"),
        (
            "src/agent_backend/tools/domains/weather.rs",
            2,
            "openweathermap",
        ),
        (
            "src/agent_backend/tools/domains/biology.rs",
            14,
            "GBIF, iNaturalist, bio-aware, Gemini",
        ),
        (
            "src/agent_backend/tools/domains/video.rs",
            3,
            "REDUCT_BASE_URL operator env",
        ),
        (
            "src/agent_backend/tools/domains/platform.rs",
            1,
            "Brave search",
        ),
        ("src/agent_backend/tools/domains/financial.rs", 1, "FMP"),
        ("src/polymarket/mod.rs", 6, "gamma-api / data-api"),
        ("src/email.rs", 1, "api.resend.com"),
        ("src/handlers/eval_judge.rs", 1, "api.anthropic.com"),
        ("src/handlers/eval.rs", 1, "api.anthropic.com"),
        ("src/handlers/agents.rs", 1, "Gemini"),
        ("src/handlers/misc.rs", 1, "nominatim"),
        ("src/handlers/creatures/identity.rs", 3, "GBIF, Gemini"),
        ("src/handlers/creatures/mod.rs", 2, "GBIF, Gemini"),
        ("src/handlers/wild.rs", 1, "api.anthropic.com"),
        ("src/handlers/wizard.rs", 2, "api.anthropic.com"),
        ("src/voice/cartesia.rs", 1, "api.cartesia.ai"),
        (
            "agent-bestiary/evaluator-wildguard/src/lib.rs",
            1,
            "anthropic constant",
        ),
        (
            "agent-bestiary/evaluator-sotopia/src/lib.rs",
            1,
            "anthropic constant",
        ),
        (
            "agent-bestiary/evaluator-character/src/lib.rs",
            1,
            "anthropic constant",
        ),
        (
            "agent-bestiary/memory/src/embeddings.rs",
            5,
            "provider constants / NOMIC_BASE_URL env",
        ),
        (
            "agent-bestiary/memory/src/llm.rs",
            4,
            "provider constants (base_url None in production)",
        ),
        ("fermi-auth/src/oidc.rs", 5, "Google / GitHub"),
    ];
    // Client-side binaries: the base URL is the operator's own server, not a
    // server-side fetch of a user-supplied URL. Not counted.
    let client_prefixes = ["crates/abw-cli/", "crates/fermi-console/"];
    // Files with sends to user-supplied URLs: (file, guarded sends, fixed-host
    // sends in the same file).
    let guarded: &[(&str, usize, usize)] = &[
        ("src/agent_backend/mcp_client.rs", 1, 0),
        ("src/a2a_webhook.rs", 1, 0),
        ("src/handlers/push.rs", 1, 0),
        // edit_image's source fetch is guarded; both Gemini calls are fixed-host.
        ("src/agent_backend/tools/domains/media.rs", 1, 2),
    ];
    let mut problems = Vec::new();
    for (file, n) in &counts {
        if client_prefixes.iter().any(|p| file.starts_with(p)) {
            continue;
        }
        if let Some((_, g, f)) = guarded.iter().find(|(p, _, _)| p == file) {
            if *n != g + f {
                problems.push(format!(
                    "{file}: {n} sends, expected {g} guarded + {f} fixed-host"
                ));
            }
            let src = strip(&std::fs::read_to_string(format!("{root}/{file}")).unwrap());
            let clients = src.matches("net_guard::guarded_client(").count();
            if clients < *g {
                problems.push(format!(
                    "{file}: {g} guarded sends but {clients} net_guard::guarded_client calls"
                ));
            }
        } else if let Some((_, want, _)) = fixed.iter().find(|(f, _, _)| f == file) {
            if n != want {
                problems.push(format!(
                    "{file}: {n} sends, {want} listed as fixed-host. Classify the new call: \
                     fixed host (update the count with the host) or route it through net_guard"
                ));
            }
        } else {
            problems.push(format!("{file}: {n} unclassified outbound send(s)"));
        }
    }
    // A listed file that lost all its sends must be removed from the list.
    for (f, _, _) in fixed {
        if !counts.contains_key(*f) {
            problems.push(format!("{f}: listed but has no sends; remove it"));
        }
    }
    assert!(
        problems.is_empty(),
        "outbound call inventory:\n{}",
        problems.join("\n")
    );
}

/// `.send()` calls on reqwest request builders per production file.
fn outbound_send_counts(root: &str) -> std::collections::BTreeMap<String, usize> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack: Vec<std::path::PathBuf> = ["src", "agent-bestiary", "fermi-auth/src", "crates"]
        .iter()
        .map(|d| format!("{root}/{d}").into())
        .collect();
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if p.is_dir() {
                if !rel.ends_with("/tests")
                    && !rel.ends_with("/target")
                    && !rel.starts_with("src/bin")
                {
                    stack.push(p);
                }
                continue;
            }
            if !rel.ends_with(".rs") || rel.ends_with("/tests.rs") {
                continue;
            }
            // Every file is scanned: a file can reach reqwest through
            // `net_guard` or a `use` without naming the crate at the call.
            let src = strip(&std::fs::read_to_string(&p).unwrap());
            let n = src.matches(".send()").count();
            if n > 0 {
                out.insert(rel, n);
            }
        }
    }
    out
}

/// Remove `//` comments and `#[cfg(test)] mod … { … }` blocks.
fn strip(src: &str) -> String {
    let no_comments: String = src
        .lines()
        .map(|l| match l.find("//") {
            Some(i) if !l[..i].contains('"') => &l[..i],
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = String::new();
    let mut rest = no_comments.as_str();
    while let Some(i) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..i]);
        let after = &rest[i..];
        let is_mod = after["#[cfg(test)]".len()..]
            .trim_start()
            .starts_with("mod ");
        let Some(brace) = after.find('{') else {
            rest = "";
            break;
        };
        if !is_mod {
            out.push_str("#[cfg(test)]");
            rest = &after["#[cfg(test)]".len()..];
            continue;
        }
        let mut depth = 0usize;
        let mut end = after.len();
        for (j, ch) in after[brace..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = brace + j + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
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
        f.contains("env_credential_allowed(key, server)"),
        "env fallback must be gated"
    );
}
