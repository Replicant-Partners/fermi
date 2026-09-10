//! Claim evaluation — the agent-driven half of the DPP Studio.
//!
//!   POST /api/workspaces/:id/actions/evaluate_claims
//!
//! ## Why this exists
//!
//! [`super::lens_actions`] renders claims by looking them up in
//! `regulatory-lens/rulesets/*.yaml`. That is a cache read, and it has two
//! properties that make it unable to answer the question the app is for:
//!
//!   1. The shipped rulesets are `synthetic_representative` — hand-authored.
//!      A lookup can only ever restate what somebody typed into the YAML.
//!   2. The lookup key is `claim_id`, and the UI mints ids as
//!      `claim_<timestamp>`. So a claim a user actually wrote can never match
//!      an entry, however ordinary the claim is. Every user claim resolves to
//!      `not_in_ruleset` forever — not a cold cache, an unhittable one.
//!
//! This handler is the evaluator. It runs `regulatory_lens_translator` against
//! the live regulatory corpus via `web_search`, enforces the grounding
//! contract over what comes back, and persists the *enforced* document to
//! `regulatory-lens/ontology/evaluated/{market}/{claim_id}.yaml`.
//!
//! `compare_lenses` then reads those evaluations as its primary source, so the
//! loop closes: evaluate once, read many times. That is also why the write is
//! done here rather than by the agent calling `write_workspace_file` itself —
//! persisting the raw reply would persist fields the gate is about to strip,
//! and the cache would then serve ungrounded values as though they had passed.
//!
//! ## What is deliberately NOT done here
//!
//! No regulatory rules live in this file. There is no pattern list, no
//! prohibited-phrase table, no status defaulting. A hardcoded rule here would
//! be the synthetic ruleset again, one layer down and harder to see. The
//! handler orchestrates, gates and persists; the corpus decides and the agent
//! judges.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use fermi_auth::AuthPrincipal;

use super::actions::resolve_workspace;
use crate::{grounding_trust, AppState};
use fermi::grounding_anomaly;

/// The agent that does the evaluating.
const EVALUATOR: &str = "regulatory_lens_translator";

/// Wall-clock budget for a single claim. One claim is up to two `web_search`
/// calls per market plus the reasoning over them, inside a five-iteration tool
/// loop whose HTTP client already allows 90s per hop.
const PER_CLAIM_TIMEOUT_SECS: u64 = 210;

/// How many claims one request will evaluate.
///
/// Bounded because each claim is a real LLM run with real searches behind it.
/// A seven-claim label would otherwise be one opaque multi-minute request that
/// reports nothing until it finishes, and times out behind most proxies. The
/// response carries `remaining`, so a caller drains the queue by calling again
/// and can show progress per batch.
const MAX_CLAIMS_PER_REQUEST: usize = 3;

// ─── request / response ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EvaluateClaimsRequest {
    /// Specific claims to evaluate. When omitted, the handler picks the
    /// unevaluated ones itself, oldest-first in document order.
    pub claim_ids: Option<Vec<String>>,
    /// Markets in scope. Defaults to all three.
    pub markets: Option<Vec<String>>,
    /// Re-evaluate claims that already have a stored evaluation.
    #[serde(default)]
    pub force: bool,
    pub source_message_id: Option<String>,
}

/// The three markets, as the lowercase tokens used in the document blocks and
/// in the on-disk path. Kept next to the display token so the two cannot drift.
const MARKETS: [(&str, &str); 3] = [("eu", "EU"), ("us", "US"), ("cn", "CN")];

fn market_key(token: &str) -> Option<&'static str> {
    MARKETS
        .iter()
        .find(|(k, disp)| k.eq_ignore_ascii_case(token) || disp.eq_ignore_ascii_case(token))
        .map(|(k, _)| *k)
}

// ─── workspace document loading ──────────────────────────────────────────────

/// Read a workspace file, falling back to the platform copy shipped in
/// `apps/adaptogen-lab/`. Same fallback chain as `lens_actions`, so a
/// workspace that has never been seeded still works.
async fn read_doc(
    state: &AppState,
    slug: &str,
    ws_path: &'static str,
    platform_path: &'static str,
) -> Option<Vec<u8>> {
    let git = state.workspace_git.clone();
    let slug_s = slug.to_string();
    tokio::task::spawn_blocking(move || {
        git.read_file_bytes(&slug_s, ws_path)
            .ok()
            .or_else(|| std::fs::read(platform_path).ok())
    })
    .await
    .ok()
    .flatten()
}

/// Does a stored evaluation already exist for this claim in this market?
async fn has_evaluation(state: &AppState, slug: &str, market: &str, claim_id: &str) -> bool {
    let path = format!("regulatory-lens/ontology/evaluated/{market}/{claim_id}.yaml");
    let git = state.workspace_git.clone();
    let slug_s = slug.to_string();
    tokio::task::spawn_blocking(move || git.read_file_bytes(&slug_s, &path).is_ok())
        .await
        .unwrap_or(false)
}

// ─── prompt construction ─────────────────────────────────────────────────────

/// Summarise the bill of materials for the agent.
///
/// Included because a claim cannot be evaluated in isolation: "naturally rich
/// in antioxidants" turns on what is actually in the bottle, and the hibiscus
/// 药食同源 question only arises because hibiscus is on the BOM. The agent gets
/// the composition the user actually wrote, not a fixture.
fn describe_product(composition: &Value) -> String {
    let name = composition
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed product)");
    let pid = composition
        .get("product_id")
        .and_then(|v| v.as_str())
        .unwrap_or("(no id)");
    let format = composition
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("unspecified");

    let mut out = format!("Product: {name} ({pid})\nFormat: {format}\n");

    // The BOM lives under `consists_of` in the DPP composition schema. Accept
    // `ingredients` too — older workspace copies use it.
    let items = composition
        .get("consists_of")
        .or_else(|| composition.get("ingredients"))
        .and_then(|v| v.as_array());

    match items {
        Some(items) if !items.is_empty() => {
            out.push_str("Bill of materials (from the workspace document):\n");
            for it in items {
                let nm = it
                    .get("display_name")
                    .or_else(|| it.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unnamed)");
                let role = it.get("role").and_then(|v| v.as_str()).unwrap_or("");
                let qty = it
                    .get("quantity")
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                let origin = it.get("origin").and_then(|v| v.as_str()).unwrap_or("");
                out.push_str(&format!("  - {nm}"));
                if !role.is_empty() {
                    out.push_str(&format!(" [role: {role}]"));
                }
                if !qty.is_empty() {
                    out.push_str(&format!(" qty: {qty}"));
                }
                if !origin.is_empty() {
                    out.push_str(&format!(" origin: {origin}"));
                }
                out.push('\n');
            }
        }
        _ => {
            out.push_str(
                "Bill of materials: not present in the workspace. Evaluate the \
                 claim on its wording alone and say in `explanation` that the \
                 composition was unavailable, since an ingredient-dependent \
                 claim cannot be settled without it.\n",
            );
        }
    }
    out
}

/// The output shape, authored here rather than in the agent card.
///
/// This lives beside the parser that reads it on purpose. The repo's recurring
/// failure is two copies of one decision drifting apart; a shape declared in a
/// system prompt and parsed in a handler is exactly that shape of bug. It also
/// has to be here for a second, harder reason: putting JSON-contract wording
/// into the system prompt trips `prompt_demands_structured_output` in
/// `src/agent_backend/tool_executor.rs`, which bypasses the tool loop entirely
/// and would silently remove `web_search` — leaving an agent that answers
/// regulatory questions from memory while looking like it searched.
fn output_shape(markets: &[&str]) -> String {
    let mut blocks = String::new();
    for m in markets {
        blocks.push_str(&format!(
            r#"  "{m}": {{
    "status": "allowed | conditionally_allowed | not_allowed | not_evaluated",
    "basis": "the provision you concluded governs this claim, or null",
    "rendered_text": "compliant wording for this market, or null if none exists",
    "needs_expert": true
  }},
  "{m}_evidence": {{
    "queries_run": ["the web_search queries you actually issued for {m}"],
    "citations": [
      {{
        "url": "a URL that came back in one of those searches",
        "title": "the result title",
        "snippet": "the retrieved text that bears on this claim",
        "provision": "the regulation or section it identifies, or null"
      }}
    ]
  }},
"#
        ));
    }
    format!(
        r#"{{
  "claim_id": "<echo the claim id>",
  "candidate_text": "<echo the candidate text>",
{blocks}  "explanation": "Why this claim lands differently across the markets you evaluated. This is the prose the operator reads.",
  "summary": "One or two sentences: the headline for this claim."
}}"#
    )
}

fn build_query(claim: &Value, composition: &Value, markets: &[&str]) -> String {
    let claim_id = claim
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let text = claim
        .get("candidate_text")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let pressure = claim
        .get("claim_pressure")
        .and_then(|v| v.as_str())
        .unwrap_or("medium");
    let notes = claim.get("notes").and_then(|v| v.as_str()).unwrap_or("");

    let market_list = markets
        .iter()
        .map(|m| m.to_uppercase())
        .collect::<Vec<_>>()
        .join(", ");

    let mut q = String::new();
    q.push_str("Evaluate one label claim against the live regulatory corpus.\n\n");
    q.push_str(&describe_product(composition));
    q.push_str(&format!(
        "\nClaim under evaluation:\n  id:             {claim_id}\n  candidate text: \"{text}\"\n  claim pressure: {pressure}\n"
    ));
    if !notes.is_empty() {
        q.push_str(&format!("  author notes:   {notes}\n"));
    }
    q.push_str(&format!("\nMarkets in scope: {market_list}\n"));

    q.push_str(
        "\nHow to proceed\n\
         Search before you judge. Issue at least two web_search calls per \
         market against that market's own corpus — the EFSA claims register and \
         Reg 1924/2006 for the EU, 21 CFR part 101 and FTC substantiation \
         guidance for the US, the SAMR announcements and the GB standards for \
         China. Search the claim's wording and its claim type, not just the \
         ingredient. Then read what came back and decide.\n\n\
         Consult regulatory-lens/ontology/patterns.yaml first with \
         read_workspace_file. If a precedent there settles part of this, cite it \
         in `explanation` and say which part it settled.\n\n",
    );

    q.push_str("Reply with a single fenced json block in this shape:\n\n```json\n");
    q.push_str(&output_shape(markets));
    q.push_str("\n```\n\n");

    q.push_str(
        "Rules that decide whether this answer is usable\n\
         - Every citation must be a result you actually received in this run. A \
           URL you did not retrieve is the one failure this whole pipeline exists \
           to prevent, because a plausible provision number is indistinguishable \
           from a real one to anyone who does not open the register.\n\
         - If a market's searches return nothing that bears on the claim, set \
           `citations: []` and `status: \"not_evaluated\"`. An empty search is a \
           gap, never a clearance. Do not fall back to what you remember.\n\
         - `status` and `basis` are your judgement over the retrieved text. They \
           are labelled model_inference in the stored document, and the citations \
           are labelled separately. Do not describe a verdict as verified.\n\
         - `rendered_text` is null when no compliant wording exists for a market. \
           A null there is a real answer and more useful than a stretch.\n\
         - Set `needs_expert: true` whenever the evidence is thin, absent, or in \
           tension. Under-flagging is worse than over-flagging here.\n\
         - Do not write workspace files for this task. The platform runs the \
           grounding gate over your reply and persists the enforced document \
           itself, so anything you wrote directly would bypass that gate.\n",
    );

    q
}

// ─── search credential ───────────────────────────────────────────────

/// Where this run's `brave_search` key would come from, or `None`.
///
/// Store first, because that is what AGENT_CREDENTIAL_MODEL.md §2 makes
/// authoritative — "Never env vars" — and `api_server`'s startup bootstrap
/// seeds `(abw-system, brave_search, '*')` from the env var once so an
/// operator can move a key in without a migration.
///
/// env second, and this is a compromise worth naming rather than hiding.
/// `web_search` cannot read the store: `ToolContext` carries no encryptor
/// (agent_backend/tools/context.rs), so the tool still reads env at runtime.
/// Checking the store alone would refuse runs that would in fact have worked;
/// checking env alone would be the legacy path the spec replaced. Either
/// satisfies the preflight, and the refusal message names the store first.
///
/// The honest consequence: a key present in the store but absent from env
/// passes here and then fails inside the tool. That asymmetry belongs to
/// `web_search` and to the fact that platform-tier agents have no
/// tool-secret path at all (`resolve_agent_owner_secrets` returns `None` for
/// curated and system tiers by design). Fixing it means giving tools a way to
/// reach the store, which is a change to shared credential plumbing and not
/// something a claims endpoint should make on its own.
async fn resolve_search_credential(
    state: &AppState,
    agent: &agent_bestiary_memory::types::Agent,
) -> Option<&'static str> {
    if let Some(encryptor) = state.secret_encryptor.as_ref() {
        let principal = crate::funding_principal_for(agent)
            .unwrap_or_else(|| "abw-system".to_string());
        if let Ok(Some(key)) = fermi_auth::resolve_agent_credential(
            &state.db,
            encryptor,
            &principal,
            "brave_search",
            &agent.agent_name,
        )
        .await
        {
            if !key.trim().is_empty() {
                return Some("credential_store");
            }
        }
    }
    if std::env::var("BRAVE_SEARCH_API_KEY")
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
    {
        return Some("env_bootstrap");
    }
    None
}

// ─── evaluation of one claim ─────────────────────────────────────────────────

struct ClaimOutcome {
    claim_id: String,
    doc: Value,
    report: grounding_trust::Report,
}

/// Run the evaluator over one claim and gate the result.
async fn evaluate_one(
    state: AppState,
    ws_uuid: Uuid,
    user_id: String,
    claim: Value,
    composition: Arc<Value>,
    markets: Vec<&'static str>,
) -> Result<ClaimOutcome, (String, String)> {
    let claim_id = claim
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let query = build_query(&claim, &composition, &markets);

    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(PER_CLAIM_TIMEOUT_SECS),
        crate::handlers::rabble_workspace::dispatch_rabble_action(
            &state,
            ws_uuid,
            EVALUATOR,
            "evaluate_claim",
            &query,
            &user_id,
            None,
        ),
    )
    .await
    .map_err(|_| {
        (
            claim_id.clone(),
            format!("evaluation timed out after {PER_CLAIM_TIMEOUT_SECS}s"),
        )
    })?
    .map_err(|e| (claim_id.clone(), e))?;

    // Parse the reply with the same scanner the delegation hop uses, rather
    // than a second copy of it — two brace-scanners that agree on almost
    // every input is the drift this repo keeps rediscovering.
    //
    // An unparseable reply becomes a document that says nothing, never one
    // that says everything is fine. Note what this fallback omits: no market
    // blocks at all. That matters, because a fallback carrying empty blocks
    // would be stamped `tool_no_match` and read as "searched, found nothing"
    // when in truth the reply was unreadable.
    let mut doc = fermi::agent_backend::envelope::extract_json(&reply).unwrap_or_else(|| {
        json!({
            "explanation": Value::Null,
            "summary": format!(
                "The evaluator replied but the reply could not be read as a \
                 document, so nothing was evaluated for this claim. First 400 \
                 characters of the reply: {}",
                reply.chars().take(400).collect::<String>()
            ),
        })
    });

    // A reply that parsed to something other than an object is the same
    // failure wearing a different shape.
    if !doc.is_object() {
        doc = json!({
            "explanation": Value::Null,
            "summary": "The evaluator's reply parsed to a non-object, so \
                        nothing was evaluated for this claim.",
        });
    }

    // Echo the identifiers from the request rather than trusting the reply's,
    // so a hallucinated claim_id cannot cause an evaluation to be filed
    // against the wrong claim.
    if let Some(obj) = doc.as_object_mut() {
        obj.insert("claim_id".to_string(), json!(claim_id));
        obj.insert(
            "candidate_text".to_string(),
            claim.get("candidate_text").cloned().unwrap_or(Value::Null),
        );
    }

    // The gate. Strips ungrounded fields, stamps every block, scans the prose
    // for authorities whose evidence block came back empty.
    let report = grounding_trust::enforce(EVALUATOR, &mut doc);
    if !report.is_clean() {
        grounding_anomaly::spawn_raise(
            Arc::clone(&state.memory_store),
            EVALUATOR,
            None,
            report.clone(),
        );
    }

    Ok(ClaimOutcome {
        claim_id,
        doc,
        report,
    })
}

// ─── persistence ─────────────────────────────────────────────────────────────

/// Build the per-market YAML that becomes the workspace's regulatory memory.
///
/// One file per (claim, market) rather than one per claim, because that is the
/// grain at which the answer is consumed — `compare_lenses` renders a column
/// per market — and the grain at which a human endorses. A reviewer signs off
/// the EU verdict without thereby signing off the CN one.
pub(super) fn evaluation_yaml(
    doc: &Value,
    claim_id: &str,
    market_key: &str,
    action_id: Uuid,
    verdict_prov: &str,
    evidence_prov: &str,
) -> String {
    // A market the evaluator did not answer for still gets a record.
    //
    // This used to return `None`, and that was a non-terminating bug rather
    // than a tidy one. `has_evaluation` looks for the file; no file meant the
    // claim stayed a candidate; the caller drains by re-requesting while
    // `remaining > 0`; so a claim whose reply omitted one market block was an
    // permanent candidate and the queue never emptied. The client could only
    // discover this by giving up.
    //
    // Recording the miss is also the more honest option on its own terms.
    // "We asked and got no answer for CN" is a fact worth keeping, and it is
    // a different fact from "nobody has asked yet" — which is exactly the
    // distinction the rest of this pipeline is built to preserve.
    let missing = doc.get(market_key).is_none();
    let block = doc.get(market_key).cloned().unwrap_or(Value::Null);
    let evidence = doc.get(format!("{market_key}_evidence").as_str());

    let record = json!({
        "claim_id": claim_id,
        "candidate_text": doc.get("candidate_text").cloned().unwrap_or(Value::Null),
        "market": market_key.to_uppercase(),
        "status": block
            .get("status")
            .cloned()
            .unwrap_or_else(|| json!("not_evaluated")),
        "basis": block.get("basis").cloned().unwrap_or(Value::Null),
        "rendered_text": block.get("rendered_text").cloned().unwrap_or(Value::Null),
        "needs_expert": block.get("needs_expert").cloned().unwrap_or(json!(true)),
        // Two stamps, never one. The verdict and the evidence under it have
        // different strengths and a consumer must be able to see both.
        "provenance": {
            "verdict": verdict_prov,
            "evidence": evidence_prov,
        },
        "citations": evidence
            .and_then(|e| e.get("citations"))
            .cloned()
            .unwrap_or(json!([])),
        "queries_run": evidence
            .and_then(|e| e.get("queries_run"))
            .cloned()
            .unwrap_or(json!([])),
        "explanation": doc.get("explanation").cloned().unwrap_or(Value::Null),
        // Distinguishes "the evaluator answered for this market" from "the
        // evaluator returned no block for it". Without this the two are
        // indistinguishable once both are `not_evaluated` on disk.
        "market_block_returned": !missing,
        "evaluated_by": EVALUATOR,
        "evaluated_at": chrono::Utc::now().to_rfc3339(),
        "action_id": action_id.to_string(),
        // Set by a human reviewer through the endorsement path. `null` means
        // nobody has signed this off, which is different from a rejection.
        "endorsement": Value::Null,
    });

    let body = serde_yaml::to_string(&record).unwrap_or_default();
    format!(
        "# Regulatory claim evaluation — written by the platform, not by hand.\n\
         #\n\
         # Produced by `{EVALUATOR}` via POST /actions/evaluate_claims and\n\
         # persisted AFTER `grounding_trust::enforce` ran over the reply. The\n\
         # values below are the enforced ones: any field the agent could not\n\
         # ground has already been nulled.\n\
         #\n\
         # provenance.verdict   how the status/basis were arrived at\n\
         # provenance.evidence  whether a tool really returned the citations\n\
         #\n\
         # `tool_no_match` on evidence means the corpus was searched and had\n\
         # nothing. That is a GAP, not a clearance. Do not read an absent\n\
         # citation as permission.\n\
         #\n\
         # Edit `endorsement` to record a qualified reviewer's sign-off.\n\
         \n{body}"
    )
}

// ─── handler ─────────────────────────────────────────────────────────────────

pub async fn evaluate_claims_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<EvaluateClaimsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    // ── Preflight: refuse rather than degrade ────────────────────────────
    //
    // Two things must be true before spending anything: the evaluator has to
    // be installed, and the corpus has to be reachable.
    //
    // The second one is why this check exists at all. `web_search` reads its
    // key at call time and, when absent, returns the words "BRAVE_SEARCH_API_KEY
    // environment variable not set" as its tool RESULT rather than raising
    // (tools/domains/platform.rs:1077). That string is handed to the model,
    // which then does the only thing it can: answers the regulatory question
    // from training data. The run SUCCEEDS. Every citation is absent, so each
    // evidence block is stamped `tool_no_match` — "the corpus was searched and
    // had nothing" — when the corpus was never reached. The result is then
    // cached and served by `compare_lenses`, indistinguishable from a real
    // search that came up empty.
    //
    // A missing key is an operator problem with a one-line fix. A workspace of
    // evaluations that quietly came from model memory is not fixable at all,
    // because nothing separates them from the real ones. So: refuse.
    let db_agent = crate::resolve_agent(&state, EVALUATOR)
        .await
        .map_err(|(_code, msg)| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "Claim evaluation is unavailable: the evaluator `{EVALUATOR}` \
                     is not installed on this platform ({msg}). Its agent card \
                     must load and be seeded before it can be run."
                ),
            )
        })?;

    match resolve_search_credential(&state, &db_agent).await {
        Some(_source) => {}
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "Claim evaluation is unavailable: no `brave_search` credential \
                     is reachable, so web_search cannot query the regulatory \
                     corpus. Running anyway would produce verdicts from the \
                     model's training data stamped `tool_no_match`, which is \
                     indistinguishable from a real search that found nothing — \
                     and those would be cached and served as evaluations. \
                     \n\nPut the key in the credential store as \
                     (principal `{}`, provider `brave_search`), which is where \
                     docs/specs/AGENT_CREDENTIAL_MODEL.md §2 requires it to live. \
                     Setting BRAVE_SEARCH_API_KEY in the environment also works \
                     and seeds the store once at startup, but it is a bootstrap \
                     seed rather than the source of truth. Until one of the two \
                     is present, `compare_lenses` will correctly report these \
                     claims as not_evaluated.",
                    crate::funding_principal_for(&db_agent)
                        .unwrap_or_else(|| "abw-system".to_string()),
                ),
            ));
        }
    }

    // Markets in scope.
    let markets: Vec<&'static str> = match req.markets.as_ref() {
        Some(ms) => {
            let resolved: Vec<&'static str> = ms.iter().filter_map(|m| market_key(m)).collect();
            if resolved.is_empty() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    "No recognised market in `markets` — expected EU, US or CN".to_string(),
                ));
            }
            resolved
        }
        None => MARKETS.iter().map(|(k, _)| *k).collect(),
    };

    // The claims document the user authored.
    let claims_bytes = read_doc(
        &state,
        &slug,
        "dpp/claims.yaml",
        "apps/adaptogen-lab/dpp/claims.yaml",
    )
    .await
    .ok_or((
        StatusCode::NOT_FOUND,
        "No claims document found. Write claims to dpp/claims.yaml first.".to_string(),
    ))?;

    let claims_doc: Value = serde_yaml::from_slice(&claims_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("dpp/claims.yaml parse error: {e}"),
        )
    })?;

    let all_claims: Vec<Value> = claims_doc
        .get("source_claims")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if all_claims.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "dpp/claims.yaml has no `source_claims` entries to evaluate.".to_string(),
        ));
    }

    // The BOM, for context. Absent is survivable — the agent is told to say so.
    let composition: Arc<Value> = Arc::new(
        read_doc(
            &state,
            &slug,
            "dpp/composition.yaml",
            "apps/adaptogen-lab/dpp/composition.yaml",
        )
        .await
        .and_then(|b| serde_yaml::from_slice(&b).ok())
        .unwrap_or(Value::Null),
    );

    // Which claims to run.
    let mut candidates: Vec<Value> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();

    for claim in &all_claims {
        let cid = claim.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if cid.is_empty() {
            continue;
        }
        if let Some(ref wanted) = req.claim_ids {
            if !wanted.iter().any(|w| w == cid) {
                continue;
            }
        }
        if !req.force {
            // Fully covered means every market in scope already has a stored
            // evaluation. A partially covered claim is re-run: the missing
            // market is the reason to run it.
            let mut covered = true;
            for m in &markets {
                if !has_evaluation(&state, &slug, m, cid).await {
                    covered = false;
                    break;
                }
            }
            if covered {
                skipped.push(json!({
                    "claim_id": cid,
                    "reason": "already_evaluated",
                    "detail": "A stored evaluation exists for every market in \
                               scope. Pass force: true to re-evaluate.",
                }));
                continue;
            }
        }
        candidates.push(claim.clone());
    }

    let remaining = candidates.len().saturating_sub(MAX_CLAIMS_PER_REQUEST);
    candidates.truncate(MAX_CLAIMS_PER_REQUEST);

    if candidates.is_empty() {
        return Ok(Json(json!({
            "action_type": "evaluate_claims",
            "evaluated": [],
            "skipped": skipped,
            "failed": [],
            "attempted": [],
            "remaining": 0,
            "markets": markets,
            "written_paths": [],
            "grounding_summary": { "is_clean": true, "violation_count": 0 },
            "note": "Nothing to evaluate. Every claim in scope already has a \
                     stored evaluation for every market requested.",
        })));
    }

    // Log the action first so the persisted evaluations can reference it.
    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());
    let action_payload = json!({
        "claim_ids": candidates.iter()
            .map(|c| c.get("id").cloned().unwrap_or(Value::Null))
            .collect::<Vec<_>>(),
        "markets": markets,
        "force": req.force,
        "evaluator": EVALUATOR,
    });
    let action_id = super::actions::log_action(
        &state,
        ws_uuid,
        "evaluate_claims",
        "user",
        &user_id,
        Some("adaptogen_lab_regulatory"),
        &action_payload,
        "auto",
        source_msg_id,
    )
    .await
    // Soft-fail, matching lens_actions: the evaluation is the product, the log
    // row is auditing infrastructure. A missing migration must not lose work
    // that has already cost real search calls and tokens.
    .unwrap_or_else(|_| Uuid::new_v4());

    // Run the batch concurrently. Bounded by MAX_CLAIMS_PER_REQUEST, so this
    // is at most three in flight.
    let mut set = tokio::task::JoinSet::new();
    for claim in candidates {
        set.spawn(evaluate_one(
            state.clone(),
            ws_uuid,
            user_id.clone(),
            claim,
            Arc::clone(&composition),
            markets.clone(),
        ));
    }

    let mut evaluated: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();
    // Every claim this request took a run at, successful or not.
    //
    // The caller needs this to page, and `remaining` alone cannot serve that
    // purpose: with `force: true` a claim never leaves the candidate set, so a
    // client draining on `remaining > 0` re-requests the same first three
    // forever. `attempted` lets it exclude what it has already spent money on.
    let mut attempted: Vec<String> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new();
    let mut any_violations = 0usize;
    let mut all_clean = true;

    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(outcome)) => {
                let ClaimOutcome {
                    claim_id,
                    doc,
                    report,
                } = outcome;

                all_clean &= report.is_clean();
                any_violations += report.violations.len();

                // Provenance stamps the gate wrote, read back rather than
                // recomputed — the stamp in the stored file must be the one
                // the gate actually issued.
                let stamp = |block: &str| -> String {
                    doc.get(format!("{block}_provenance").as_str())
                        .and_then(|v| v.as_str())
                        .unwrap_or(grounding_trust::PROV_UNAVAILABLE)
                        .to_string()
                };

                for m in &markets {
                    let verdict_prov = stamp(m);
                    let evidence_prov = stamp(&format!("{m}_evidence"));
                    let yaml = evaluation_yaml(
                        &doc,
                        &claim_id,
                        m,
                        action_id,
                        &verdict_prov,
                        &evidence_prov,
                    );
                    files.push((
                        format!("regulatory-lens/ontology/evaluated/{m}/{claim_id}.yaml"),
                        yaml,
                    ));
                }
                attempted.push(claim_id.clone());

                evaluated.push(json!({
                    "claim_id": claim_id,
                    "document": doc,
                    "grounding": {
                        "is_clean": report.is_clean(),
                        "violation_count": report.violations.len(),
                        "provenance": report.provenance.iter()
                            .map(|(b, v)| json!({"block": b, "verdict": v}))
                            .collect::<Vec<_>>(),
                    },
                }));
            }
            Ok(Err((claim_id, err))) => {
                all_clean = false;
                attempted.push(claim_id.clone());
                failed.push(json!({"claim_id": claim_id, "error": err}));
            }
            Err(join_err) => {
                all_clean = false;
                failed.push(json!({
                    "claim_id": Value::Null,
                    "error": format!("evaluation task panicked: {join_err}"),
                }));
            }
        }
    }

    // One commit for the whole batch. Individual commits from concurrent tasks
    // would contend on the repo lock and leave the workspace half-written if
    // one failed.
    let mut written: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    if !files.is_empty() {
        let git = state.workspace_git.clone();
        let slug_w = slug.clone();
        let msg = format!(
            "regulatory_lens_translator: {} claim evaluation(s) across {} market(s)",
            evaluated.len(),
            markets.len()
        );
        let commit =
            tokio::task::spawn_blocking(move || git.commit_files_as(&slug_w, &files, &msg, None))
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

        if let Err(e) = commit {
            // The evaluations are still returned. Say plainly that they were
            // not persisted, so the caller does not assume the cache is warm.
            written.clear();
            return Ok(Json(json!({
                "action_id": action_id,
                "action_type": "evaluate_claims",
                "evaluated": evaluated,
                "skipped": skipped,
                "failed": failed,
                "attempted": attempted,
                "remaining": remaining,
                "markets": markets,
                "written_paths": written,
                "persistence_error": format!(
                    "Evaluations completed but could not be committed to the \
                     workspace: {e}. They are returned here but will not be \
                     found by compare_lenses."
                ),
                "grounding_summary": {
                    "is_clean": false,
                    "violation_count": any_violations,
                },
            })));
        }
    }

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "evaluate_claims",
        "evaluated": evaluated,
        "skipped": skipped,
        "failed": failed,
        // Page on this, not on `remaining`. See the field's declaration.
        "attempted": attempted,
        "remaining": remaining,
        "markets": markets,
        "written_paths": written,
        "grounding_summary": {
            "is_clean": all_clean,
            "violation_count": any_violations,
        },
    })))
}
