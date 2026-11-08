//! LLD G17: LLM spend is rate limited per handler, not by a path allowlist.
//!
//! The old middleware matched paths against `LLM_SPEND_ROUTES`
//! [V: `src/api_server.rs:244-263` @ 51235f00]. The workspace message,
//! coherence evaluate, claim evaluation, carbon, BOM pricing, forage and
//! flock handlers ran the tool loop and were not on it, and `/execute` was
//! charged twice (middleware and inline).

use crate::deployed::base_url;
use crate::support;

/// Calls that run a model (directly or through a local helper that does).
const PRIMITIVES: &[&str] = &[
    "tool_executor.execute(",
    "executor.execute(",
    "dispatch_rabble_action(",
    "dispatch_rabble_action_recorded(",
    "execute_carbon_run(",
    "evaluate_one(",
    "run_eval_cases(",
];

/// Handlers that reach a model through a path the primitive scan does not
/// follow (consolidation, composition dream, eval run). Pinned.
const PINNED: &[(&str, &str)] = &[
    ("src/handlers/consolidation.rs", "consolidate_agent_handler"),
    ("src/handlers/composition.rs", "composition_dream_handler"),
    ("src/handlers/eval.rs", "trigger_eval_run_handler"),
    ("src/handlers/eval.rs", "batch_eval_run_handler"),
];

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
            .map(|l| l.split("//").next().unwrap_or(""))
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

/// Every HTTP handler that awaits a model call on the request path is
/// admitted by the LLM limiter (extractor, or an inline check in it or a
/// local helper it calls); every model call in a spawned task either belongs
/// to an admitted request or asks `llm_spend::background` first. Fails on the old code: none of the
/// handlers above declared spend, and the list lived in a path matcher.
#[test]
fn every_handler_that_spends_declares_it() {
    let all = handlers();
    let local_admits: Vec<&str> = all
        .iter()
        .filter(|h| admits(&h.body))
        .map(|h| h.name.as_str())
        .collect();
    let mut problems = Vec::new();
    let mut foreground = 0;
    for h in all.iter().filter(|h| h.sig.contains("State(")) {
        let spans = spawn_spans(&h.body);
        let mut fg = false;
        for p in PRIMITIVES {
            for (i, _) in h.body.match_indices(p) {
                match spans.iter().find(|(a, b)| *a <= i && i <= *b) {
                    Some((a, _)) => {
                        // Covered when the request itself was admitted, or
                        // when the task asks before it dispatches.
                        let before = &h.body[*a..i];
                        if !(h.sig.contains("LlmSpend")
                            || before.contains("llm_spend::background("))
                        {
                            problems.push(format!(
                                "{}::{}: `{p}` in a spawned task without llm_spend::background",
                                h.file, h.name
                            ));
                        }
                    }
                    None => fg = true,
                }
            }
        }
        if fg {
            foreground += 1;
            let declared = h.sig.contains("LlmSpend")
                || admits(&h.body)
                || local_admits
                    .iter()
                    .any(|n| h.body.contains(&format!("{n}(")));
            if !declared {
                problems.push(format!(
                    "{}::{}: awaits a model call without the LLM limiter",
                    h.file, h.name
                ));
            }
        }
    }
    for (file, name) in PINNED {
        let h = all
            .iter()
            .find(|h| h.file == *file && h.name == *name)
            .unwrap_or_else(|| panic!("{file}::{name} moved; update PINNED"));
        if !h.sig.contains("LlmSpend") {
            problems.push(format!("{file}::{name}: pinned spender without LlmSpend"));
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
    // The background dispatcher asks the limiter before it dispatches.
    let rw = all
        .iter()
        .find(|h| h.name == "dispatch_rabble_action_background")
        .expect("background dispatcher");
    assert!(rw.body.contains("llm_spend::background("));
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
        for _ in 0..60 {
            let s = http
                .post(format!("{base}/api/workspaces/{ws}/actions/price_bom"))
                .bearer_auth(&key)
                .json(&serde_json::json!({}))
                .send()
                .await
                .expect("request")
                .status()
                .as_u16();
            statuses.push(s);
            if s == 429 {
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
    })
    .await;
}
