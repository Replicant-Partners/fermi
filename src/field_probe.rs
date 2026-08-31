//! Running the tool a field contract names.
//!
//! # Why
//!
//! A contract says which tool could settle a field:
//!
//! ```text
//! FieldContract {
//!     agent_id: "football_analyst",
//!     path: "head_to_head",
//!     grounding: Grounding::Sourced { tool: "call_football_api", .. },
//! }
//! ```
//!
//! and the trace printed `call_football_api` beside the row and offered no way to
//! run it. Sixteen tools are named across the contracts, on rows a reader can do
//! nothing about. **A name the platform can print and cannot offer is a
//! description, not an affordance**, and the screen was made of them.
//!
//! # What this does NOT do, and why not
//!
//! It does not decide anything. It runs the tool and hands back what came out.
//!
//! The temptation is to compare the tool's answer to the agent's claim and write
//! a verdict, and it cannot be done honestly, because the contract does not say
//! **where in the response** the value lives. `response_field` is prose:
//!
//! ```text
//! response_field: "standings (rank, points, form, home/away splits)"
//! response_field: "fixtures/headtohead"
//! ```
//!
//! One of those is an endpoint path and the other is a sentence. Matching a
//! claimed number against a response by looking for it is string-matching
//! dressed as verification — the same move that produced the genome error, one
//! layer along. So the platform performs the retrieval, and a person performs the
//! comparison, and the settle form is right there to record what they concluded.
//!
//! It also cannot fill in the query. `call_football_api` wants
//! `{endpoint, params}` with a league id, a season and a team id; those come from
//! what the episode was **about**, not from the contract. The caller supplies
//! them, and the UI says so rather than pretending to know.
//!
//! # The narrow door
//!
//! Only tools that need no [`ToolContext`] — no workspace, no memory store, no
//! credentials of ours, no delegation. A read-only surface must not be the door
//! to any of those. See [`crate::agent_backend::tools::CONTEXT_FREE_TOOLS`].

use serde_json::Value;

/// The tool a contract names for this field, if it names one.
///
/// Read from the contract rather than accepted from the caller. A probe endpoint
/// that ran whatever tool the request asked for would be a general-purpose
/// outbound HTTP proxy with an audit trail that said "field verification".
pub fn declared_tool(agent_id: &str, path: &str) -> Option<&'static str> {
    crate::grounding_trust::contracts_for(agent_id)
        .find(|c| c.path == path)
        .and_then(|c| match c.grounding {
            crate::grounding_trust::Grounding::Sourced { tool, .. } => Some(tool),
            _ => None,
        })
}

/// What the contract says the answer lives in, when it says anything.
///
/// Prose as often as not, which is why it is surfaced to the caller as a hint
/// rather than used to build the call. Where it happens to be an endpoint path —
/// `fixtures/headtohead` — the UI can prefill it, and where it is a sentence the
/// caller reads it and decides.
pub fn response_hint(agent_id: &str, path: &str) -> Option<&'static str> {
    crate::grounding_trust::contracts_for(agent_id)
        .find(|c| c.path == path)
        .and_then(|c| match c.grounding {
            crate::grounding_trust::Grounding::Sourced { response_field, .. } => {
                Some(response_field)
            }
            _ => None,
        })
}

/// A `response_field` hint, parsed into the parts a search can use.
///
/// The hints have a grammar, and it was being treated as an opaque string:
///
/// ```text
/// standings (rank, points, form, home/away splits)   container + names
/// fixtures/headtohead                                endpoint only
/// fixtures/statistics (shots, possession, cards)     endpoint + names
/// fixtures/statistics.expected_goals                 endpoint + one leaf
/// assembly_name                                      a bare key name
/// best_bid / best_ask / book_quality.tradeable       several names
/// ```
#[derive(Debug, Default, serde::Serialize)]
pub struct HintTarget {
    /// What the head of the hint names. An endpoint for the API pass-throughs;
    /// for other tools it is the enclosing block, and nothing is claimed of it.
    pub endpoint: Option<String>,
    /// The single leaf a dotted hint names.
    pub leaf: Option<String>,
    /// Names worth looking for in a response, leaf first.
    pub keys: Vec<String>,
    /// Names from the hint that are **prose, not keys** — `home/away splits`.
    ///
    /// Separated rather than searched, because searching them guarantees a miss
    /// and a miss reported beside real misses makes the real ones cheaper. What
    /// a reader needs to know is that these were never looked for.
    pub prose: Vec<String>,
}

/// Parse a contract's `response_field`.
pub fn parse_hint(hint: &str) -> HintTarget {
    let head = hint.split(" (").next().unwrap_or_default().trim();
    let listed: Vec<String> = hint
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(inner, _)| {
            inner
                .split(',')
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let atoms: Vec<&str> = head
        .split(" + ")
        .flat_map(|a| a.split(" / "))
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .collect();

    let mut t = HintTarget::default();
    match atoms.as_slice() {
        // A head listing several names names no single container. `endpoint`
        // stays `None`, which is what stops a surface prefilling
        // `{"endpoint": "best_bid / best_ask / midpoint / book_quality"}` into a
        // tool that has no endpoints — and then reporting a mismatch against it.
        [] => {}
        [one] => {
            if let Some((container, name)) = one.rsplit_once('.') {
                t.endpoint = Some(container.to_string());
                t.leaf = Some(name.to_string());
                t.keys.push(name.to_string());
            } else if one.contains('/') || !listed.is_empty() {
                // `fixtures/headtohead` is a path; `standings (rank, points)` is
                // a container followed by its keys. Either way the head is where
                // the answer lives, not the name to look for inside it.
                t.endpoint = Some(one.to_string());
            } else {
                // `assembly_name` is the key AND the whole hint. Naming it an
                // endpoint would invent one for a tool that takes none.
                t.keys.push(one.to_string());
            }
        }
        many => t.keys.extend(many.iter().map(|a| {
            a.rsplit_once('.')
                .map_or_else(|| a.to_string(), |(_, n)| n.to_string())
        })),
    }
    t.keys.extend(listed);
    // A name with a space in it is a description of a group of keys, not a key.
    let (keys, prose): (Vec<String>, Vec<String>) = t
        .keys
        .drain(..)
        .partition(|k| !k.contains(char::is_whitespace));
    t.keys = keys;
    t.prose = prose;
    t
}

/// The endpoint a probe request actually asked for, when the tool has endpoints.
///
/// `call_football_api` takes `{endpoint, params}`, and a reader who presses a
/// replay chip is running whatever endpoint that call used — which may not be
/// this field's. Read from the request so the outcome can say when the two
/// disagree. `None` for every tool with no endpoint in its input, and no
/// mismatch is claimed in that case.
pub fn endpoint_of(input: &Value) -> Option<String> {
    input
        .get("endpoint")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// The endpoint this field's contract names — when the tool has endpoints.
///
/// One implementation, because two callers need the same answer and a
/// disagreement between them is an accusation: the trace composes the probe
/// query from this, and the probe endpoint checks the query it receives against
/// it. If the two parsed the hint differently, the platform would report the
/// reader's own prefill as the wrong endpoint.
///
/// `None` where the tool takes no endpoint at all. `estimated_size_mb (assembly
/// total_length)` parses to the same shape as `standings (rank, points)` and
/// `ncbi_genome_search` has no endpoints, so calling that head one produced a
/// query the tool could only refuse — under a sentence calling it "the endpoint
/// this field's contract names".
pub fn probe_endpoint(agent_id: &str, path: &str) -> Option<String> {
    let tool = declared_tool(agent_id, path)?;
    if !crate::agent_backend::tools::tool_takes_endpoint(tool) {
        return None;
    }
    parse_hint(response_hint(agent_id, path)?).endpoint
}

/// Can this field's tool actually be run from a surface?
pub fn is_runnable(tool: &str) -> bool {
    crate::agent_backend::tools::is_context_free(tool)
}

/// How much of a tool response travels back.
///
/// API-Football returns whole seasons. The caller is reading it to decide one
/// field, and a megabyte through the JSON encoder to answer that is a bad trade —
/// but a silent truncation is worse, so the outcome says when it cut.
pub const RESPONSE_CHARS: usize = 12_000;

/// One place a hinted name turns up in a response.
#[derive(Debug, serde::Serialize)]
pub struct KeyHit {
    /// The name from the contract that matched.
    pub key: String,
    /// Where, as a path from the response root: `$.response[0].statistics[12]`.
    pub at: String,
    /// `key` when the name is an object key; `value` when it is a string value.
    ///
    /// Both are needed and only the first was obvious. API-Football returns
    /// fixture statistics as a list of `{type, value}` pairs, so expected goals
    /// arrives as `{"type":"expected_goals","value":"1.23"}` — the name is a
    /// **value**, and a key-only search reports NOT FOUND with the number
    /// sitting in the payload. On the one field this screen was built to settle,
    /// that is the difference between catching the agent and exonerating it.
    pub site: &'static str,
    /// The value found, for a key hit; the enclosing object, for a value hit —
    /// because `{"type":"expected_goals","value":"1.23"}` is the answer and
    /// `"expected_goals"` on its own is not.
    pub sample: String,
}

/// How many hits travel back, and how much of each.
const MAX_HITS: usize = 12;
const SAMPLE_CHARS: usize = 200;

/// Lowercase, alphanumerics only — so `expected_goals` matches `Expected Goals`.
///
/// Deliberately loose in one direction only: it can match a name written in a
/// different style, and it cannot match a different name.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn sample_of(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if s.chars().count() > SAMPLE_CHARS {
        s.chars().take(SAMPLE_CHARS).collect::<String>() + "…"
    } else {
        s
    }
}

/// What a search over one response body found.
#[derive(Debug, Default, serde::Serialize)]
pub struct Search {
    /// Was the body JSON at all? `false` makes `missing` mean *unknown*.
    pub parsed: bool,
    /// Up to [`MAX_HITS`] places, in document order.
    pub found: Vec<KeyHit>,
    /// How many places there are altogether. Counted past the cap, because a
    /// silently truncated list of evidence is the same fault as a silently
    /// truncated response.
    pub total: usize,
    pub missing: Vec<String>,
}

/// Walk a response and record every place a hinted name appears.
fn locate(v: &Value, at: &str, wanted: &[(String, String)], out: &mut Search) {
    match v {
        Value::Object(map) => {
            for (k, child) in map {
                let here = format!("{at}.{k}");
                if let Some((_, orig)) = wanted.iter().find(|(n, _)| *n == norm(k)) {
                    out.total += 1;
                    if out.found.len() < MAX_HITS {
                        out.found.push(KeyHit {
                            key: orig.clone(),
                            at: here.clone(),
                            site: "key",
                            sample: sample_of(child),
                        });
                    }
                }
                if let Value::String(s) = child {
                    if let Some((_, orig)) = wanted.iter().find(|(n, _)| *n == norm(s)) {
                        out.total += 1;
                        if out.found.len() < MAX_HITS {
                            out.found.push(KeyHit {
                                key: orig.clone(),
                                at: here.clone(),
                                site: "value",
                                sample: sample_of(v),
                            });
                        }
                    }
                }
                locate(child, &here, wanted, out);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                locate(child, &format!("{at}[{i}]"), wanted, out);
            }
        }
        _ => {}
    }
}

/// Search a response body for the names a contract's hint gives.
///
/// Over the **whole** body, before [`RESPONSE_CHARS`] truncation. The client did
/// this on the truncated copy, which meant a large payload could report NOT
/// FOUND for a name that was in the part that did not travel — a false negative
/// on a trust surface, produced by a display limit.
pub fn search(body: &str, keys: &[String]) -> Search {
    if keys.is_empty() {
        // Nothing named, so nothing to look for — `fixtures/headtohead` names an
        // endpoint and no key inside it. `parsed` still reports whether the body
        // was JSON, because that is a different question.
        return Search {
            parsed: serde_json::from_str::<Value>(body).is_ok(),
            ..Default::default()
        };
    }
    let Ok(doc) = serde_json::from_str::<Value>(body) else {
        return Search {
            parsed: false,
            missing: keys.to_vec(),
            ..Default::default()
        };
    };
    let wanted: Vec<(String, String)> = keys.iter().map(|k| (norm(k), k.clone())).collect();
    let mut out = Search {
        parsed: true,
        ..Default::default()
    };
    locate(&doc, "$", &wanted, &mut out);
    out.missing = keys
        .iter()
        .filter(|k| !out.found.iter().any(|h| h.key == **k))
        .cloned()
        .collect();
    out
}

/// The outcome of running a named tool.
#[derive(Debug, serde::Serialize)]
pub struct Probe {
    pub tool: &'static str,
    /// `true` when the tool returned. **Not** a verdict about the field: a tool
    /// can answer perfectly and still have nothing for this fixture, which is
    /// what `tool_no_match` exists to say.
    pub ok: bool,
    pub response: String,
    pub truncated: bool,
    pub chars: usize,
    /// The names looked for, from the contract's hint. Reported so a reader can
    /// see what a miss actually means.
    pub searched: Vec<String>,
    /// Names in the hint that are prose and were **not** searched.
    pub not_searched: Vec<String>,
    /// Was the body JSON at all? A `false` here is why `found` is empty.
    pub parsed: bool,
    pub found: Vec<KeyHit>,
    /// How many places the names appear altogether, when more than `found` holds.
    pub found_total: usize,
    pub missing: Vec<String>,
    /// Digest of the whole body, so a surface can tell two probes apart — or
    /// recognise that two fields just received the identical payload, which is
    /// what happens when one endpoint carries both and is the thing a reader
    /// otherwise has no way to notice.
    pub digest: String,
}

/// Run the tool the contract names for this field, and locate the hinted names.
///
/// The search happens here rather than in the caller because this is the last
/// place the untruncated body exists.
pub async fn run(tool: &'static str, input: &Value, target: &HintTarget) -> Probe {
    let (ok, body) = match crate::agent_backend::tools::execute_context_free(tool, input).await {
        Ok(s) => (true, s),
        // The error is the answer here, and it is often the useful one: a
        // missing API key, a refused endpoint, a rate limit. Returned rather
        // than logged, because the person who clicked is the person who needs it.
        Err(e) => (false, e),
    };
    let chars = body.chars().count();
    // Not searched when the tool refused: the body is an error message, and
    // "none of these names appear" said of a rate-limit notice is noise dressed
    // as evidence.
    let found = if ok {
        search(&body, &target.keys)
    } else {
        Search::default()
    };
    Probe {
        tool,
        ok,
        response: body.chars().take(RESPONSE_CHARS).collect(),
        truncated: chars > RESPONSE_CHARS,
        chars,
        searched: target.keys.clone(),
        not_searched: target.prose.clone(),
        parsed: found.parsed,
        found_total: found.total,
        missing: found.missing,
        found: found.found,
        digest: crate::artifact_hash::of_text(&body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The runnable list and the dispatcher must agree.
    ///
    /// A surface offers a button for every tool in `CONTEXT_FREE_TOOLS`. If the
    /// dispatcher then refuses one, the refusal arrives after the click — which
    /// is worse than never offering it, because the reader has already decided
    /// the platform could do the thing.
    #[tokio::test]
    async fn every_offered_tool_is_actually_dispatchable() {
        for tool in crate::agent_backend::tools::CONTEXT_FREE_TOOLS {
            // An empty input: every one of these must fail on a MISSING
            // PARAMETER or a missing key, never on being unknown. The unknown
            // branch is the one that means the two lists have drifted.
            let out =
                crate::agent_backend::tools::execute_context_free(tool, &serde_json::json!({}))
                    .await;
            if let Err(e) = out {
                assert!(
                    !e.contains("cannot be run from here"),
                    "`{tool}` is offered as runnable and the dispatcher does not \
                     know it. The button would be refused after the click."
                );
            }
        }
    }

    /// Which contract-named tools a reader can actually run, and which cannot.
    ///
    /// The number that matters: every tool a contract names is printed on the
    /// trace beside a row, so the ones that are not runnable are precisely the
    /// rows where the name is still a description.
    ///
    /// The unrunnable set is pinned rather than counted, because each entry has
    /// a different reason and the reasons are the useful part. If the list
    /// shrinks, a row somewhere gained a button and this fails so the change is
    /// noticed; if it grows, a contract has started naming something a surface
    /// can never offer.
    #[test]
    fn every_contract_named_tool_is_runnable_or_says_why_not() {
        /// Named by a contract, and not reachable from a read-only surface.
        ///
        /// Alphabetical, because the set it is compared against comes from a
        /// `BTreeSet` and a hand-ordered list would fail on ordering rather than
        /// on the thing this test is about.
        const NEEDS_CONTEXT: &[(&str, &str)] = &[
            (
                "reduct_add_block",
                "writes to a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_create_reel",
                "writes to a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_get_project",
                "reads a Reduct project on the agent owner's credentials",
            ),
            (
                "reduct_get_transcript",
                "reads a Reduct project on the agent owner's credentials",
            ),
            (
                "scan_nearby_creatures",
                "reads the caller's creature and its neighbourhood out of the \
                 memory store, so it has no meaning without a ToolContext",
            ),
        ];

        let named: std::collections::BTreeSet<&str> = crate::grounding_trust::FIELD_CONTRACTS
            .iter()
            .filter_map(|c| match c.grounding {
                crate::grounding_trust::Grounding::Sourced { tool, .. } => Some(tool),
                _ => None,
            })
            .collect();

        let blocked: Vec<&str> = named.iter().copied().filter(|t| !is_runnable(t)).collect();
        let declared: Vec<&str> = NEEDS_CONTEXT.iter().map(|(t, _)| *t).collect();

        assert_eq!(
            blocked, declared,
            "the set of contract-named tools that cannot be run from a surface \
             has changed. Every one of these is printed on the trace beside a \
             row, so an entry here is a row where the tool's name is a \
             description and nothing more."
        );

        for (tool, why) in NEEDS_CONTEXT {
            assert!(
                why.len() > 30,
                "{tool} is excluded with no real reason, and \"it needs context\" \
                 is what would be assumed rather than checked"
            );
        }

        // And the useful figure, asserted so it cannot quietly regress.
        let runnable = named.len() - blocked.len();
        assert!(
            runnable >= 11,
            "only {runnable} of {} contract-named tools can be run from a \
             surface; it was 11",
            named.len()
        );
    }

    /// The hint grammar, on the four shapes the contracts actually use.
    #[test]
    fn a_hint_yields_an_endpoint_and_the_names_to_look_for() {
        let t = parse_hint("fixtures/statistics.expected_goals");
        assert_eq!(t.endpoint.as_deref(), Some("fixtures/statistics"));
        assert_eq!(t.leaf.as_deref(), Some("expected_goals"));
        assert_eq!(t.keys, ["expected_goals"]);

        let t = parse_hint("fixtures/statistics (shots, possession, cards)");
        assert_eq!(t.endpoint.as_deref(), Some("fixtures/statistics"));
        assert_eq!(t.keys, ["shots", "possession", "cards"]);

        // A bare key name is the whole hint: a key, and NOT an endpoint. Calling
        // it one would prefill an `endpoint` into a tool that takes none, and
        // then the mismatch check would have a phantom to compare against.
        let t = parse_hint("assembly_name");
        assert_eq!(t.keys, ["assembly_name"]);
        assert_eq!(t.endpoint, None);

        // Several names, so no single container is claimed.
        let t = parse_hint("best_bid / best_ask / midpoint / book_quality.tradeable");
        assert_eq!(t.endpoint, None);
        assert_eq!(t.keys, ["best_bid", "best_ask", "midpoint", "tradeable"]);

        // An endpoint with nothing to look for inside it. Better than inventing
        // `headtohead` as a key: the probe reports "no names to search".
        let t = parse_hint("fixtures/headtohead");
        assert_eq!(t.endpoint.as_deref(), Some("fixtures/headtohead"));
        assert!(t.keys.is_empty());

        // Prose is set aside, not searched. A guaranteed miss reported beside
        // real misses is what makes the real ones cheap to ignore.
        let t = parse_hint("standings (rank, points, form, home/away splits)");
        assert_eq!(t.keys, ["rank", "points", "form"]);
        assert_eq!(t.prose, ["home/away splits"]);
    }

    /// The case the whole screen exists for: xG is a **value**, not a key.
    ///
    /// API-Football returns fixture statistics as `{type, value}` pairs. A
    /// key-only search over this reports NOT FOUND while the number is right
    /// there, which would exonerate an agent this trace can prove wrong.
    #[test]
    fn a_name_that_is_a_value_is_found_and_says_so() {
        let body = serde_json::json!({
            "response": [{
                "team": {"id": 50},
                "statistics": [
                    {"type": "Shots on Goal", "value": 7},
                    {"type": "expected_goals", "value": "1.23"},
                ],
            }],
        })
        .to_string();

        let s = search(&body, &["expected_goals".to_string()]);
        assert!(s.parsed);
        assert!(s.missing.is_empty());
        assert_eq!(s.found.len(), 1);
        assert_eq!(s.total, 1);
        assert_eq!(s.found[0].site, "value");
        assert_eq!(s.found[0].at, "$.response[0].statistics[1].type");
        // The enclosing object, because that is where the number is.
        assert!(s.found[0].sample.contains("1.23"));
    }

    /// Style differences match; different names do not.
    #[test]
    fn the_search_is_loose_about_style_and_strict_about_identity() {
        let body = r#"{"Expected Goals": 1.5, "shots_off_goal": 3}"#;
        let s = search(body, &["expected_goals".to_string()]);
        assert_eq!(s.found.len(), 1);
        assert_eq!(s.found[0].site, "key");

        let s = search(body, &["shots_on_goal".to_string()]);
        assert!(
            s.found.is_empty(),
            "`shots_off_goal` is not `shots_on_goal`"
        );
        assert_eq!(s.missing, ["shots_on_goal"]);
    }

    /// A capped list of places still reports how many there are.
    ///
    /// `standings` carries a `rank` per team. Twelve paths is plenty to read and
    /// "twelve" said of twenty is the same fault as a silently truncated
    /// response, which this module already refuses to commit.
    #[test]
    fn a_capped_evidence_list_says_how_much_it_capped() {
        let rows: Vec<Value> = (0..30).map(|i| serde_json::json!({"rank": i})).collect();
        let body = serde_json::json!({"response": rows}).to_string();
        let s = search(&body, &["rank".to_string()]);
        assert_eq!(s.found.len(), MAX_HITS);
        assert_eq!(s.total, 30);
    }

    /// A hint that names an endpoint and no key searches for nothing.
    #[test]
    fn an_endpoint_with_no_named_key_searches_for_nothing() {
        let s = search(r#"{"response":[]}"#, &[]);
        assert!(
            s.parsed,
            "the body was still JSON, and that is worth knowing"
        );
        assert_eq!(s.total, 0);
        assert!(
            s.missing.is_empty(),
            "nothing was asked for, so nothing is absent"
        );
    }

    /// A body that is not JSON reports that, rather than reporting a miss.
    ///
    /// "None of these names appear" said of an HTML error page is a false
    /// negative wearing the clothes of a finding.
    #[test]
    fn an_unparseable_body_is_not_a_miss() {
        let s = search("<html>rate limited</html>", &["rank".to_string()]);
        assert!(!s.parsed);
        assert!(s.found.is_empty());
        assert_eq!(s.missing, ["rank"], "still unknown, not still absent");
    }

    /// Two fields whose contracts name the same endpoint get the same payload.
    ///
    /// `match_statistics` and `advanced_metrics.xg` both come from
    /// `fixtures/statistics`. That is correct and it confused a reader, because
    /// the page showed two identical 16KB answers and said nothing. The digest
    /// is what lets a surface say "same call, different key".
    #[test]
    fn two_fields_can_share_one_endpoint_and_differ_only_in_the_key() {
        let stats = parse_hint(response_hint("football_analyst", "match_statistics").unwrap());
        let xg = parse_hint(response_hint("football_analyst", "advanced_metrics.xg").unwrap());
        assert_eq!(stats.endpoint, xg.endpoint);
        assert_ne!(stats.keys, xg.keys);
    }

    /// An endpoint is claimed only where the tool has endpoints.
    ///
    /// Both hints below parse to a head and a parenthesised list. One head is an
    /// API path and the other is a field name, and the only thing that knows the
    /// difference is the tool's own input schema.
    #[test]
    fn a_tool_with_no_endpoints_is_given_none() {
        assert_eq!(
            probe_endpoint("football_analyst", "advanced_metrics.xg").as_deref(),
            Some("fixtures/statistics"),
        );
        assert_eq!(
            probe_endpoint("football_analyst", "league_context").as_deref(),
            Some("standings"),
        );
        // `ncbi_genome_search` takes `{scientific_name}`. Its hint
        // `estimated_size_mb (assembly total_length)` has the shape of an
        // endpoint and is a field name, and the prefill built from it was a
        // query the tool could only refuse.
        assert_eq!(
            probe_endpoint("genome_profiler", "genome.estimated_size_mb"),
            None,
            "a tool whose schema declares no `endpoint` was given one anyway"
        );
        assert!(
            crate::agent_backend::tools::tool_takes_endpoint("call_football_api"),
            "the schema read is not finding `call_football_api`'s endpoint, so \
             every field would now say the tool takes none"
        );
    }

    /// The tool is read from the contract, never from the request.
    #[test]
    fn the_tool_comes_from_the_contract() {
        assert_eq!(
            declared_tool("football_analyst", "head_to_head"),
            Some("call_football_api")
        );
        assert_eq!(declared_tool("football_analyst", "no_such_field"), None);
        assert_eq!(declared_tool("no_such_agent", "head_to_head"), None);
    }
}
