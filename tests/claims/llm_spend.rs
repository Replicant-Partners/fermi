//! LLD G17: LLM spend is rate limited per handler, not by a path allowlist.
//!
//! The old middleware matched paths against `LLM_SPEND_ROUTES`
//! [V: `src/api_server.rs:244-263` @ 51235f00]. The workspace message,
//! coherence evaluate, claim evaluation, carbon, BOM pricing, forage and
//! flock handlers ran the tool loop and were not on it, and `/execute` was
//! charged twice (middleware and inline).

use crate::deployed::base_url;
use crate::support;

/// Calls that run a language model.
///
/// Platform executors, the rabble dispatcher, and direct provider HTTP calls.
/// Any function in `src/handlers` whose body contains one is a spender, and so
/// is any function that calls a spender (followed transitively, by name).
/// `executor.execute(` alone is not listed: in `notebooks.rs` it is the FPL
/// Monte Carlo executor, which calls no model.
const PRIMITIVES: &[&str] = &[
    "tool_executor.execute(",
    "ToolAwareExecutor::new(",
    "dispatch_rabble_action(",
    "dispatch_rabble_action_recorded(",
    "registry.executor().execute(",
    "api.anthropic.com",
    "api.openai.com",
    "openrouter.ai",
];

/// Paid image/vision model calls (Gemini), not admitted by this limiter:
/// bounded by the credits they charge first; scope is an open owner question
/// on #44.
const OUT_OF_SCOPE: &[&str] = &["generativelanguage.googleapis.com"];

fn matching(s: &str, open: usize, o: u8, c: u8) -> usize {
    let b = s.as_bytes();
    let mut depth = 0;
    let mut j = open;
    while j < b.len() {
        if b[j] == o {
            depth += 1;
        } else if b[j] == c {
            depth -= 1;
            if depth == 0 {
                return j;
            }
        }
        j += 1;
    }
    b.len()
}

/// Drop a `//` comment from a line, but not a `//` inside a string literal
/// (`"https://api.anthropic.com"` is code, and a primitive).
fn strip_line_comment(line: &str) -> &str {
    let b = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i + 1 < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str && b[i + 1] == b'/' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}

struct Handler {
    file: String,
    name: String,
    sig: String,
    body: String,
}

fn handlers() -> Vec<Handler> {
    let root = env!("CARGO_MANIFEST_DIR");
    let fn_re = regex::Regex::new(r"\n(pub(?:\(crate\))? )?async fn (\w+)\s*(<[^>]*>)?\(").unwrap();
    let mut out = Vec::new();
    for f in support::rust_files(&format!("{root}/src/handlers")) {
        let text = std::fs::read_to_string(&f).unwrap();
        let src: String = text
            .split("#[cfg(test)]")
            .next()
            .unwrap_or("")
            .lines()
            .map(strip_line_comment)
            .collect::<Vec<_>>()
            .join("\n");
        for m in fn_re.captures_iter(&src) {
            let whole = m.get(0).unwrap();
            let Some(open) = src[whole.end()..].find('{').map(|i| whole.end() + i) else {
                continue;
            };
            let close = matching(&src, open, b'{', b'}');
            out.push(Handler {
                file: f
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .trim_start_matches('/')
                    .to_string(),
                name: m[2].to_string(),
                sig: src[whole.start()..open].to_string(),
                body: src[open..close].to_string(),
            });
        }
    }
    out
}

fn spawn_spans(body: &str) -> Vec<(usize, usize)> {
    body.match_indices("tokio::spawn(")
        .map(|(i, m)| (i, matching(body, i + m.len() - 1, b'(', b')')))
        .collect()
}

fn admits(body: &str) -> bool {
    body.contains("rate_limits.llm.check(") || body.contains("llm_spend::admit(")
}

/// Every HTTP handler that reaches a language model call on the request
/// path is admitted by the LLM limiter (the `LlmSpend` extractor, or `admit`
/// / an inline check in its own body or in a helper it calls), and every
/// language-model call inside a spawned task either belongs to an admitted
/// request or asks `llm_spend::background` first. Fails on the old code:
/// no handler declared spend, and the list lived in a path matcher.
#[test]
fn every_handler_that_spends_declares_it() {
    let all = handlers();
    let calls = |body: &str, name: &str| -> bool {
        let pat = format!("{name}(");
        body.match_indices(&pat).any(|(i, _)| {
            body[..i]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        })
    };
    // Spenders: a primitive in the body, or a call to a spender.
    let mut spenders: Vec<String> = all
        .iter()
        .filter(|h| PRIMITIVES.iter().any(|p| h.body.contains(p)))
        .map(|h| h.name.clone())
        .collect();
    loop {
        let more: Vec<String> = all
            .iter()
            .filter(|h| !spenders.contains(&h.name))
            .filter(|h| spenders.iter().any(|s| calls(&h.body, s)))
            .map(|h| h.name.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        spenders.extend(more);
    }
    let admitting: Vec<&str> = all
        .iter()
        .filter(|h| admits(&h.body))
        .map(|h| h.name.as_str())
        .collect();
    let mut problems = Vec::new();
    let mut foreground = 0;
    for h in all.iter().filter(|h| h.sig.contains("State(")) {
        let spans = spawn_spans(&h.body);
        let mut fg = false;
        for s in spenders
            .iter()
            .map(String::as_str)
            .chain(PRIMITIVES.iter().copied())
        {
            let hits: Vec<usize> = if PRIMITIVES.contains(&s) {
                h.body.match_indices(s).map(|(i, _)| i).collect()
            } else {
                let pat = format!("{s}(");
                h.body
                    .match_indices(&pat)
                    .map(|(i, _)| i)
                    .filter(|i| {
                        h.body[..*i]
                            .chars()
                            .next_back()
                            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
                    })
                    .collect()
            };
            for i in hits {
                match spans.iter().find(|(a, b)| *a <= i && i <= *b) {
                    Some((a, _)) => {
                        let before = &h.body[*a..i];
                        let admitted_inside = before.contains("llm_spend::background(")
                            || (s.ends_with("_background") || s == "trigger_swarm_host_welcome");
                        // A handler that admitted the request in its own
                        // body before spawning covers the task it spawns.
                        let admitted_before = admits(&h.body[..*a]);
                        if !(h.sig.contains("LlmSpend") || admitted_inside || admitted_before) {
                            problems.push(format!(
                                "{}::{}: `{s}` in a spawned task without llm_spend::background",
                                h.file, h.name
                            ));
                        }
                    }
                    // Calling another handler: that handler is checked on
                    // its own, so it is not this one's foreground spend.
                    None if all.iter().any(|o| o.name == s && o.sig.contains("State(")) => {}
                    None => fg = true,
                }
            }
        }
        if fg {
            foreground += 1;
            let declared = h.sig.contains("LlmSpend")
                || admits(&h.body)
                || admitting.iter().any(|n| calls(&h.body, n));
            if !declared {
                problems.push(format!(
                    "{}::{}: reaches a model call without the LLM limiter",
                    h.file, h.name
                ));
            }
        }
    }
    // Functions that dispatch from a spawned context on their own must ask.
    for name in [
        "dispatch_rabble_action_background",
        "trigger_swarm_host_welcome",
    ] {
        let h = all
            .iter()
            .find(|h| h.name == name)
            .unwrap_or_else(|| panic!("{name} moved"));
        if !h.body.contains("llm_spend::background(") {
            problems.push(format!("{name} dispatches without llm_spend::background"));
        }
    }
    // An FPL-only handler is not a spender, so it is not throttled.
    for h in all.iter().filter(|h| h.file == "src/handlers/notebooks.rs") {
        if h.sig.contains("LlmSpend") && !spenders.contains(&h.name) {
            problems.push(format!(
                "{}::{} throttled but calls no model",
                h.file, h.name
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        foreground >= 12,
        "found only {foreground} spending handlers; did the scan break?"
    );
    let api = std::fs::read_to_string(format!("{}/src/api_server.rs", env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    assert!(
        !api.contains("const LLM_SPEND_ROUTES"),
        "a path allowlist is back"
    );
    let _ = OUT_OF_SCOPE;
}

/// Live: a spending handler is refused by the LLM limiter before any work,
/// even when the request itself would fail. On the old deployment BOM pricing
/// was not on the path list, so this never returned 429.
#[tokio::test]
#[ignore = "needs a deployment: ABW_TEST_BASE_URL + ABW_TEST_DATABASE_URL, run with --ignored"]
async fn deployed_a_spending_handler_is_rate_limited() {
    let base = base_url().expect("ABW_TEST_BASE_URL must be set");
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, move |pool, c| async move {
        let mut cc = c.lock().await;
        let user = support::make_user(&pool, &mut cc).await;
        let (key, info) =
            fermi_auth::api_keys::create_api_key(&pool, &user, "spec36 claims", &["write".into()])
                .await
                .expect("api key");
        cc.api_keys.push(info.key_id);
        drop(cc);

        // A workspace this user is not a member of: the handler would refuse
        // with 403/404 and run no model, so the probe spends nothing.
        let ws = uuid::Uuid::new_v4();
        let http = reqwest::Client::new();
        let mut statuses = Vec::new();
        let mut body_429 = String::new();
        for _ in 0..60 {
            let r = http
                .post(format!("{base}/api/workspaces/{ws}/actions/price_bom"))
                .bearer_auth(&key)
                .json(&serde_json::json!({}))
                .send()
                .await
                .expect("request");
            let s = r.status().as_u16();
            statuses.push(s);
            if s == 429 {
                body_429 = r.text().await.unwrap_or_default();
                break;
            }
        }
        let first_429 = statuses.iter().position(|s| *s == 429);
        assert!(
            first_429.is_some(),
            "60 calls to a spending handler, never rate limited: {statuses:?}"
        );
        assert!(
            statuses[..first_429.unwrap()].iter().all(|s| *s != 200),
            "the probe must not succeed (it would spend): {statuses:?}"
        );
        assert!(
            body_429.contains("LLM rate limit"),
            "refused by the LLM limiter, not another one: {body_429}"
        );
    })
    .await;
}
