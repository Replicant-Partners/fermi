//! # Compiling an output-contract sketch, over HTTP
//!
//! The builder in `templates/agent_create.html` needs to turn a sketch into a
//! contract, and it must get **the same answer the publish gate will give**.
//!
//! That is the whole reason this is a server round-trip rather than a few
//! hundred lines of JavaScript. A browser-side compiler would be a second
//! implementation of `contract_sketch::Sketch::compile` and
//! `card_contract::validate`, and the two would drift — at which point the
//! wizard cheerfully produces contracts that fail at publish, which is worse
//! than no wizard, because the author now has a green tick to argue with.
//!
//! Same argument as `card_contract::execute_validate_tool`, which exists so
//! `xaman_ek`'s advice and the gate are one piece of code. Here it is again
//! for the human surface.
//!
//! One endpoint, no database, no side effects: `POST /api/contracts/compile`.

use axum::{extract::State, http::StatusCode, Json};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::{resolve_agent, resolve_agent_card, AppState};

#[derive(Deserialize)]
pub struct CompileRequest {
    /// The authored sketch. See `contract_sketch::Sketch`.
    pub sketch: Value,
    /// Tools the agent declares. A `sourced` block must name one of these —
    /// the cross-reference that stops the builder emitting a plausible
    /// contract for a tool the agent cannot call.
    #[serde(default)]
    pub tool_names: Vec<String>,
    /// Optional ontology, so `@entity` field types resolve against a
    /// vocabulary the agent already uses.
    #[serde(default)]
    pub ontology: Option<Value>,
    /// The card's current `produces`, so the returned one is the MERGE rather
    /// than the bare declared type.
    ///
    /// `ContractBuilder.saveTo` PUTs `produces` straight from this response.
    /// Without this the builder saved one label over a card that had seven,
    /// deleting six that other agents match on — the defect measured in
    /// `docs/plans/AGENT_COMPILE_AND_TOOL_REGISTRY.md` §6.8. Absent means
    /// "nothing to preserve", which is right for a card being created.
    #[serde(default)]
    pub existing_produces: Vec<String>,
}

/// `POST /api/contracts/compile`
///
/// Always `200` when the request is well-formed, including when the sketch
/// does not compile: a sketch with findings is a normal answer to a normal
/// question, not a client error. The body's `compiles` field carries the
/// verdict and `findings` carries the fixes.
///
/// Authenticated because it is a compute endpoint on a public origin, not
/// because the result is sensitive — it reveals nothing the corpus does not.
pub async fn compile_handler(
    _principal: AuthPrincipal,
    Json(req): Json<CompileRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if !req.sketch.is_object() {
        return Err((
            StatusCode::BAD_REQUEST,
            "`sketch` must be an object".to_string(),
        ));
    }

    let mut input = serde_json::json!({
        "sketch": req.sketch,
        "tool_names": req.tool_names,
    });
    if let Some(ont) = req.ontology.filter(|o| o.is_object()) {
        input["ontology"] = ont;
    }

    // The same function the `build_output_contract` MCP tool dispatches to,
    // so an agent drafting a card and a human filling in the wizard cannot
    // be told different things.
    let body = fermi::contract_sketch::execute_build_tool(&input)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let mut parsed: Value = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("compiler emitted unparseable output: {e}"),
        )
    })?;

    // Merge the port labels back in before the builder ever sees them, so the
    // save path needs no rule of its own. Two implementations of "which labels
    // survive a compile" is how one of them ends up deleting the other's.
    if let Some(arr) = parsed.get("produces").and_then(|v| v.as_array()) {
        let compiled: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        parsed["produces"] = serde_json::json!(fermi::contract_sketch::merge_produces(
            &compiled,
            &req.existing_produces
        ));
    }

    Ok(Json(parsed))
}

/// `GET /api/contracts/tools`
///
/// The dispatchable builtin tool names, so the builder can autocomplete
/// against what actually exists instead of letting an author type a plausible
/// name and discover at publish that nothing answers it.
///
/// Same list `invalid_tool_declarations` checks against, for the same
/// no-second-implementation reason as above.
pub async fn tool_names_handler(_principal: AuthPrincipal) -> Json<Value> {
    let mut names = fermi::agent_backend::tools::platform_tool_names();
    names.sort_unstable();

    // Each tool's declared response shape, where anyone has read it. This is
    // what turns the field picker from a text box into a choice among fields
    // that exist: pick one and you get its name AND its type, having guessed
    // neither.
    //
    // `evidence` travels with it because a `vendor` shape is a weaker claim
    // than a `constructed` one, and the UI should not present them
    // identically. A tool absent from the table has simply not been read, and
    // the builder falls back to extracting candidates from its description
    // and marking them unconfirmed.
    //
    // Serialised by the table's own module, not here, so the headless check of
    // the builder can be handed the same bytes the browser gets rather than a
    // fixture that drifts.
    let shapes: Vec<Value> = fermi::tool_response_shapes::declared_shapes_json();

    Json(json!({
        "tools": names,
        "response_shapes": shapes,
        "note": "`response_shapes` covers the tools someone has read. A tool \
                 absent from it has an unknown response, which is not the same \
                 as an empty one.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The body `cbCompile()` in `templates/agent_create.html` sends,
    /// including `ontology: null` — which `fetch` produces whenever the
    /// author has not pasted one.
    ///
    /// This is the realistic breakage on this seam. Not the compiler, which
    /// has its own tests, but a key renamed on one side of three (browser,
    /// handler, compiler) and not the others. A wizard posting `toolNames`
    /// and receiving an empty `tool_names` would accept a `sourced` block
    /// naming a tool the agent cannot call — the one failure the
    /// cross-check exists to prevent, reintroduced by a typo.
    #[test]
    fn the_browsers_payload_deserialises_and_compiles() {
        let body = serde_json::json!({
            "sketch": {
                "domain": "testing",
                "produces_schema": "demo/doc",
                "blocks": [{
                    "name": "ratios",
                    "source": {
                        "status": "sourced",
                        "tool": "fmp_ratios",
                        "response_field": "priceToEarningsRatio",
                        "coverage": "complete"
                    },
                    "why": "FMP computes this ratio server-side, so the agent reads it rather than deriving it from statements it also fetched.",
                    "fields": { "price_to_earnings": "number?" }
                }]
            },
            "tool_names": ["fmp_ratios"],
            "ontology": null
        });

        let req: CompileRequest =
            serde_json::from_value(body).expect("the browser's payload deserialises");
        assert_eq!(req.tool_names, vec!["fmp_ratios".to_string()]);

        // The handler's body, minus the auth extractor.
        let mut input = serde_json::json!({
            "sketch": req.sketch,
            "tool_names": req.tool_names,
        });
        if let Some(ont) = req.ontology.filter(|o| o.is_object()) {
            input["ontology"] = ont;
        }

        let out = fermi::contract_sketch::execute_build_tool(&input).expect("compiles");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(v["compiles"], serde_json::json!(true), "{v:#}");
        // The exact keys the page reads back. Renaming one of these breaks
        // the builder silently, which is why they are named here.
        for key in ["output_contract", "produces", "generated_properties"] {
            assert!(v.get(key).is_some(), "response is missing `{key}`");
        }
    }

    /// The invariant that keeps `suggest` a proposer rather than a generator:
    /// it never writes `why`. Asserted over the whole catalogue, not one
    /// example, because this is the property that would erode quietly — a
    /// helpful default here is the fabrication the contract exists to catch,
    /// wearing the costume of developer experience.
    #[test]
    fn no_proposal_ever_invents_a_why() {
        for (name, desc) in fermi::agent_backend::tools::builtin_tool_catalogue() {
            let p = tool_block_proposal(name, desc);
            assert_eq!(
                p["why"].as_str(),
                Some(""),
                "`{name}` came back with a why the author did not write"
            );
            for f in p["candidate_fields"].as_array().unwrap() {
                assert_eq!(
                    f["unconfirmed"],
                    serde_json::json!(true),
                    "`{name}`: a field lifted from prose must be marked unconfirmed"
                );
            }
        }
    }

    /// A proposal is a real starting point: fill in the `why` and it compiles.
    /// Without that, "suggest" would be a button that produces something the
    /// next button rejects.
    #[test]
    fn a_proposal_plus_an_authored_why_compiles() {
        let (name, desc) = fermi::agent_backend::tools::builtin_tool_catalogue()
            .into_iter()
            .find(|(n, _)| *n == "fmp_company_profile")
            .expect("fmp_company_profile is a builtin");

        let mut block = tool_block_proposal(name, desc);
        block["why"] = serde_json::json!(
            "FMP's profile endpoint returns these for a resolved ticker, or an \
             empty array for a symbol it does not carry."
        );

        // Candidate fields become real fields once the author keeps them.
        let fields: serde_json::Map<String, Value> = block["candidate_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["name"].as_str().unwrap().to_string(), f["type"].clone()))
            .collect();
        assert!(!fields.is_empty(), "the description yielded no candidates");
        block
            .as_object_mut()
            .unwrap()
            .insert("fields".into(), Value::Object(fields));

        let input = json!({
            "sketch": {
                "domain": "testing",
                "produces_schema": "demo/doc",
                "blocks": [block],
            },
            "tool_names": [name],
        });
        let out = fermi::contract_sketch::execute_build_tool(&input).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["compiles"], json!(true), "{v:#}");
    }

    /// And the same proposal, untouched, does NOT compile — so the button
    /// cannot be used to skip the one decision that matters.
    #[test]
    fn an_untouched_proposal_does_not_compile() {
        let (name, desc) = fermi::agent_backend::tools::builtin_tool_catalogue()
            .into_iter()
            .find(|(n, _)| *n == "fmp_company_profile")
            .unwrap();
        let mut block = tool_block_proposal(name, desc);
        block
            .as_object_mut()
            .unwrap()
            .insert("fields".into(), json!({ "price": "number?" }));

        let input = json!({
            "sketch": {
                "domain": "testing",
                "produces_schema": "demo/doc",
                "blocks": [block],
            },
            "tool_names": [name],
        });
        let out = fermi::contract_sketch::execute_build_tool(&input).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["compiles"], json!(false));
        assert!(v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["check"] == "sketch_why"));
    }

    /// A `sourced` block naming a tool the agent does not declare must not
    /// come back with a contract. Checked through the HTTP request shape
    /// rather than the compiler, because this is the property the builder's
    /// green tick is asserting on the author's behalf.
    #[test]
    fn a_phantom_tool_gets_no_contract_through_this_endpoint() {
        let body = serde_json::json!({
            "sketch": {
                "domain": "testing",
                "produces_schema": "demo/doc",
                "blocks": [{
                    "name": "ratios",
                    "source": {
                        "status": "sourced",
                        "tool": "fmp_imaginary",
                        "response_field": "x",
                        "coverage": "complete"
                    },
                    "why": "A plausible sentence about a tool that does not exist, long enough to clear the minimum.",
                    "fields": { "pe": "number?" }
                }]
            },
            "tool_names": ["fmp_ratios"]
        });

        let req: CompileRequest = serde_json::from_value(body).unwrap();
        let input = serde_json::json!({
            "sketch": req.sketch,
            "tool_names": req.tool_names,
        });
        let out = fermi::contract_sketch::execute_build_tool(&input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();

        assert_eq!(v["compiles"], serde_json::json!(false));
        assert!(
            v.get("output_contract").is_none(),
            "a partial contract reads exactly like a complete one"
        );
        assert!(
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["check"] == "grounding_sourced_names_tool"),
            "{v:#}"
        );
    }
}

// ─── Compositional clues ───────────────────────────────────────────────
//
// The first version of the builder was a form in a vacuum: it asked what
// document you return without telling you what documents already exist, who
// would consume yours, or that your own declared tools already imply most of
// the answer. A contract is a COMPOSITION artefact — `produces_schema` exists
// so another agent can match on identity — so authoring one with no view of
// the ecosystem is the one context in which it makes least sense.
//
// Two endpoints, both answering questions the author actually has:
//
//   /api/contracts/types    what types exist, who produces them, who could
//                           consume mine
//   /api/contracts/suggest  my tools, turned into candidate blocks
//
// `suggest` is a PROPOSER, in the sense `scripts/port_migrate.py` uses the
// word: it fills in what the tool declaration is evidence for and refuses to
// fill in what it is not. Specifically it never writes `why`, so nothing it
// returns can be compiled without the author having said something. See
// `tool_block_proposal`.

/// `GET /api/contracts/types`
///
/// The type registry: every `produces_schema` declared anywhere, its
/// producers, and its block names.
///
/// The block names are the useful part and the reason this is not just a list
/// of strings. "What do agents like mine actually return?" is answerable from
/// the corpus, and answering it from the corpus is strictly better than an
/// author guessing at structure from an empty form — which is the one thing
/// `port_migrate.py` measured as impossible to do well.
pub async fn types_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT agent_name, agent_type, accepts, produces, output_contract \
         FROM agents \
         WHERE output_contract IS NOT NULL \
           AND output_contract -> 'produces_schema' IS NOT NULL \
         ORDER BY agent_name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}")))?;

    // Every label any agent accepts, so "who could consume this" is answered
    // from declarations rather than from optimism.
    let consumer_rows =
        sqlx::query("SELECT agent_name, accepts FROM agents WHERE accepts IS NOT NULL")
            .fetch_all(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}")))?;

    let mut consumers_by_label: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for r in &consumer_rows {
        let name: String = r.try_get("agent_name").unwrap_or_default();
        let accepts: Vec<String> = r.try_get("accepts").unwrap_or_default();
        for a in accepts {
            consumers_by_label.entry(a).or_default().push(name.clone());
        }
    }

    let mut types: Vec<Value> = Vec::new();
    for r in &rows {
        let name: String = r.try_get("agent_name").unwrap_or_default();
        let agent_type: String = r.try_get("agent_type").unwrap_or_default();
        let oc: Option<Value> = r.try_get("output_contract").ok();
        let Some(oc) = oc else { continue };
        let Some(ty) = oc.get("produces_schema").and_then(|v| v.as_str()) else {
            continue;
        };

        let blocks: Vec<String> = oc
            .pointer("/schema/properties")
            .and_then(|p| p.as_object())
            .map(|o| {
                o.keys()
                    // The derived stamps are noise in a "what shape is this"
                    // answer: they are one per block and always the same idea.
                    .filter(|k| !k.ends_with(fermi::contract_sketch::PROVENANCE_SUFFIX))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();

        types.push(json!({
            "type": ty,
            "producer": name,
            "agent_type": agent_type,
            "domain": oc.get("domain").and_then(|d| d.as_str()),
            "blocks": blocks,
            "consumers": consumers_by_label.get(ty).cloned().unwrap_or_default(),
        }));
    }

    Ok(Json(json!({
        "types": types,
        // Stated so the builder can be honest about how thin this is rather
        // than presenting three entries as an ecosystem.
        "note": "Types declared across the corpus. `consumers` lists agents whose \
                 `accepts` names this type exactly. Most agents still accept \
                 free-text labels rather than types, so an empty `consumers` \
                 usually means the consumer side has not been typed yet — not \
                 that nothing wants this document.",
    })))
}

#[derive(Deserialize)]
pub struct SuggestRequest {
    /// Tools the agent declares. Only these are proposed — a block sourced
    /// from a tool the agent lacks is the defect the whole contract exists to
    /// catch, so it must not be reachable even as a suggestion.
    #[serde(default)]
    pub tool_names: Vec<String>,
}

/// `POST /api/contracts/suggest`
///
/// Turn declared tools into candidate blocks.
///
/// This is the single biggest ease-of-use lever, and it is legitimate where a
/// label-derived schema is not. `port_migrate.py`'s finding was that ~95% of
/// port LABELS have no corroborating evidence in the card — a name is not
/// evidence of anything. A declared tool is different: it has an author, a
/// description of what it returns, and an input schema. Proposing a block for
/// it is extraction from a real declaration.
///
/// Two invariants make it safe:
///
/// 1. **`why` is never proposed.** It is the field whose subject is the
///    author's own reasoning, so `compile` refuses without it and this returns
///    an empty string deliberately rather than a plausible sentence.
/// 2. **Field names are marked unconfirmed.** They are nouns lifted from the
///    tool's own description, which is the tool author's prose and not the
///    response keys. Useful as a starting point, dishonest as a fact, so they
///    come back under `candidate_fields` for the author to accept or rename.
pub async fn suggest_handler(
    _principal: AuthPrincipal,
    Json(req): Json<SuggestRequest>,
) -> Json<Value> {
    let defs = fermi::agent_backend::tools::builtin_tool_catalogue();
    let proposals: Vec<Value> = req
        .tool_names
        .iter()
        .filter_map(|want| {
            defs.iter()
                .find(|(name, _)| name == want)
                .map(|(name, desc)| {
                    // Prefer the declared response shape. Prose extraction is
                    // the fallback for a tool nobody has read, and its output
                    // is marked `unconfirmed` because it is nouns from a
                    // sentence rather than response keys.
                    match fermi::tool_response_shapes::response_for(name) {
                        Some(decl) => tool_block_proposal_from_shape(name, decl),
                        None => tool_block_proposal(name, desc),
                    }
                })
        })
        .collect();

    Json(json!({
        "blocks": proposals,
        "note": "One candidate block per declared tool. `why` is deliberately empty: \
                 its subject is where YOUR data comes from, and a generated \
                 justification for that is the fabrication this contract exists to \
                 catch. `candidate_fields` are nouns lifted from the tool's own \
                 description — a starting point, not the response keys. Rename them.",
    }))
}

/// A candidate block for a tool whose response shape has been read.
///
/// The difference from [`tool_block_proposal`] is the whole point of the
/// response-shape table: these fields are not `unconfirmed`, because they are
/// not nouns lifted from a sentence. They are the keys the tool emits, with
/// the types they emit them as, and `response_field` is a real path rather
/// than the tool author's prose.
///
/// `why` is still empty. Nothing about knowing a tool's response tells anyone
/// why this agent's block has the status it has.
fn tool_block_proposal_from_shape(
    name: &str,
    decl: &fermi::tool_response_shapes::ToolResponse,
) -> Value {
    let block = name
        .split_once('_')
        .map(|(_, rest)| rest)
        .unwrap_or(name)
        .to_string();

    json!({
        "name": block,
        "source": {
            "status": "sourced",
            "tool": name,
            // Real paths, comma-joined, so the contract's `response_field`
            // claim is checkable against the tool's actual output instead of
            // being the tool author's sentence about it.
            "response_field": decl
                .fields
                .iter()
                .map(|f| f.path)
                .collect::<Vec<_>>()
                .join(", "),
            "coverage": "complete",
        },
        "why": "",
        "candidate_fields": decl.fields.iter().map(|f| json!({
            "name": f.field,
            "type": f.ty,
            "path": f.path,
            "note": f.note,
            // Not a guess: this came from reading the tool.
            "unconfirmed": false,
        })).collect::<Vec<_>>(),
        "evidence": decl.evidence.kind(),
        "evidence_from": decl.evidence.where_from(),
    })
}

/// A candidate block for one tool.
fn tool_block_proposal(name: &str, description: &str) -> Value {
    // `fmp_company_profile` -> `company_profile`. The vendor prefix is noise
    // in a field name: the document says what it holds, the grounding entry
    // says who supplied it.
    let block = name
        .split_once('_')
        .map(|(_, rest)| rest)
        .unwrap_or(name)
        .to_string();

    json!({
        "name": block,
        "source": {
            "status": "sourced",
            "tool": name,
            // The tool's own sentence, for the author to cut down to the
            // fields that matter. Better than an empty box and explicitly not
            // a claim about response keys.
            "response_field": description.trim(),
            "coverage": "complete",
        },
        "why": "",
        "candidate_fields": candidate_fields(description),
    })
}

/// Nouns from a tool description, as candidate field names.
///
/// Crude on purpose. A cleverer extractor would produce more confident-looking
/// output for the same evidence, which is the wrong direction: the author has
/// to read these either way, and a list that obviously needs editing gets
/// edited.
fn candidate_fields(description: &str) -> Vec<Value> {
    // Tool descriptions in this corpus follow "…including a, b, c." or
    // "…data: a, b, c." — take the clause after the first colon or
    // "including", then split on commas.
    let tail = description
        .split_once(':')
        .map(|(_, t)| t)
        .or_else(|| description.split_once("including").map(|(_, t)| t))
        .unwrap_or("");

    tail.split(',')
        .map(|s| s.trim().trim_end_matches('.').trim())
        .filter(|s| !s.is_empty() && s.len() < 40 && !s.contains(" the "))
        .take(10)
        .map(|s| {
            let snake = s
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '_' })
                .collect::<String>();
            let snake = snake
                .split('_')
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join("_");
            json!({
                "name": snake,
                // Nullable by default: a retrieved field that the tool did
                // not return must be able to say so, and the corpus
                // convention is a type union rather than an absent key.
                "type": "number?",
                "unconfirmed": true,
            })
        })
        .filter(|f| !f["name"].as_str().unwrap_or("").is_empty())
        .collect()
}

// ─── managing a contract on an existing agent ──────────────────────────
//
// The builder started life inside the create wizard, which meant a contract
// could only be authored at birth. That is the wrong lifecycle: 90 of 101
// agents already exist, `genome_profiler` has had a schema and no grounding
// map since before any of this was written, and the interesting work is
// modifying a contract rather than minting one.
//
// So: read any agent's contract back into an editable sketch.

/// `GET /api/contracts/decompile/:agent_id`
///
/// The agent's current contract as a sketch, its declared tools, and what the
/// sketch is still missing.
///
/// `findings` is the useful half. For a card with a complete contract it is
/// empty. For `genome_profiler` it is five `sketch_why` findings — one per
/// block — which is exactly the information that card has never had, named per
/// field instead of as "publish refused".
pub async fn decompile_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    axum::extract::Path(agent_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // The RESOLVED card, not the raw `agents` row.
    //
    // This read the `mcp_tools` column directly and reported "no tools
    // declared" for agents that plainly have them: `genome_profiler` shows
    // four on its own page and the builder showed none. For a curated agent
    // the tools live in the on-disk card and the column is NULL, because
    // `resolve_agent_card` prefers the registry and only falls back to the
    // database. Every other surface uses that resolution; reading the column
    // was a second answer to a question the platform had already settled.
    //
    // The consequence was not cosmetic. With no tool names, the palette had
    // nothing to offer, the response-shape picker never appeared, and an
    // author had to type field names from memory — which is precisely the
    // failure this whole feature was built to remove, reintroduced by reading
    // the wrong column.
    let db_agent = resolve_agent(&state, &agent_id)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, format!("no agent `{agent_id}`")))?;
    let card = resolve_agent_card(&state, &db_agent);

    let produces: Vec<String> = card.produces.clone();
    let tool_names: Vec<String> = card
        .capabilities
        .mcp_tools
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let oc: Option<Value> = card.capabilities.output_contract.clone();

    // No contract yet: an empty sketch seeded with the agent's tools is a
    // better starting point than an error, because "this agent has nothing"
    // is the common case and the next action is the same either way.
    let Some(oc) = oc.filter(|v| v.is_object()) else {
        return Ok(Json(json!({
            "agent_id": agent_id,
            "has_contract": false,
            "sketch": Value::Null,
            "tool_names": tool_names,
            "produces": produces,
            "findings": [],
            "note": "This agent declares no output contract. Start from its \
                     tools: each one is a candidate retrieved block, and a \
                     block sourced from a tool the agent really has passes the \
                     hardest check by construction.",
        })));
    };

    let sketch = match fermi::contract_sketch::sketch_from_contract(&oc) {
        Ok(s) => s,
        Err(findings) => {
            // The shape itself could not be read. Distinguished from "reads
            // fine but is incomplete", because the fixes are different: this
            // one needs a hand-edit, that one needs a `why`.
            return Ok(Json(json!({
                "agent_id": agent_id,
                "has_contract": true,
                "readable": false,
                "sketch": Value::Null,
                "tool_names": tool_names,
                "produces": produces,
                "findings": findings
                    .iter()
                    .map(|f| json!({ "check": f.check, "fix": f.message }))
                    .collect::<Vec<_>>(),
                "note": "The contract exists but uses schema shapes this \
                         compiler cannot express, so editing it here would \
                         change fields you did not touch. Edit the card \
                         directly.",
            })));
        }
    };

    // What it is missing, phrased per field. Not fatal — the point is to load
    // the thing and show the to-do list.
    let findings = sketch
        .compile(&tool_names)
        .err()
        .unwrap_or_default()
        .iter()
        .map(|f| json!({ "check": f.check, "fix": f.message }))
        .collect::<Vec<_>>();

    // Which of each sourced block's fields the tool can actually supply.
    //
    // This is the original bug, stated mechanically and while the author is
    // looking at the block: `genome_profiler.genome` declares `notable_genes`
    // and `ncbi_genome_search` returns no such field. A field in a retrieved
    // block with no possible source is indistinguishable from its neighbours
    // until something says so.
    let mut field_coverage: Vec<Value> = Vec::new();
    for b in &sketch.blocks {
        let fermi::contract_sketch::Source::Sourced { tool, .. } = &b.source else {
            continue;
        };
        let names: Vec<&str> = b.fields.keys().map(|s| s.as_str()).collect();
        if names.is_empty() {
            continue;
        }
        match fermi::tool_response_shapes::coverage(tool, &names) {
            Some((covered, uncovered)) => field_coverage.push(json!({
                "block": b.name,
                "tool": tool,
                "known": true,
                "covered": covered,
                "uncovered": uncovered,
            })),
            // Absence of a declaration is absence of information, and must
            // not render the same as "all covered".
            None => field_coverage.push(json!({
                "block": b.name,
                "tool": tool,
                "known": false,
                "covered": [],
                "uncovered": [],
            })),
        }
    }

    Ok(Json(json!({
        "agent_id": agent_id,
        "has_contract": true,
        "readable": true,
        "field_coverage": field_coverage,
        "sketch": serde_json::to_value(&sketch).unwrap_or(Value::Null),
        "tool_names": tool_names,
        "produces": produces,
        "findings": findings,
        "note": if findings.is_empty() {
            "This contract is complete and would publish."
        } else {
            "Loaded. The findings are what this contract is still missing, per \
             field. A `why` is never recovered or invented — if the card had no \
             grounding map, every block needs one written."
        },
    })))
}

/// `POST /api/contracts/tool-request`
///
/// Turn an `unavailable` block into a specification for the tool that would
/// make it `sourced`.
///
/// The question this answers: an author declares a block `unavailable` because
/// nothing can supply it, writes `would_need: "the IUCN Red List API"`, and
/// then what? Today: nothing. The refusal is honest and permanently stuck,
/// because the gap is recorded in a card nobody reads as a backlog.
///
/// This emits a brief a coding agent can act on. It deliberately does NOT
/// build the tool: it states what the field needs, what the response must
/// carry, and — the part an author would forget — that the new tool has to be
/// added to `capabilities.mcp_tools` and the block flipped from `unavailable`
/// to `sourced` afterwards, or the contract still refuses.
pub async fn tool_request_handler(
    _principal: AuthPrincipal,
    Json(req): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = req
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed agent)");
    let block = req
        .get("block")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "`block` is required".to_string()))?;
    let would_need = req
        .get("would_need")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    let fields: Vec<String> = req
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if would_need.is_empty() {
        return Ok(Json(json!({
            "ready": false,
            "why": format!(
                "`{block}` is declared unavailable with no `would_need`. A tool \
                 cannot be specified from the absence alone — say what source \
                 would supply it, then this can be written."
            ),
        })));
    }

    let suggested = format!(
        "{}_{}",
        would_need
            .split_whitespace()
            .next()
            .unwrap_or("source")
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>(),
        block
    );

    let brief = format!(
        "Add a platform tool so `{agent}` can source its `{block}` block.\n\
         \n\
         WHY IT DOES NOT EXIST YET\n\
         The block is declared `unavailable` in the agent's output contract, \
         which is the honest status: no tool the agent declares returns this. \
         The author recorded what would be needed:\n\
         \n  {would_need}\n\
         \n\
         WHAT THE TOOL MUST RETURN\n\
         The block's fields, so the contract can be flipped to `sourced`:\n\
         {}\n\
         \n\
         WHERE IT GOES\n\
         - A `BuiltinToolDef` in src/agent_backend/ (see weather_tools.rs or \
         the fmp_* tools in tools_legacy.rs for the shape), plus a dispatch arm \
         in `ToolRegistry::execute`. A name with no arm is a phantom tool: \
         advertised to the model, called, and answered `Unknown tool`. \
         `invalid_tool_declarations` rejects the card on any write, so this \
         cannot be half-done.\n\
         - Return real fields, not prose. The contract's `response_field` names \
         which part of the response supplies each value, and that claim is \
         supposed to be checkable against the tool's actual output.\n\
         \n\
         AFTER IT LANDS — do not skip this\n\
         1. Add `{}` to `{agent}`'s `capabilities.mcp_tools`, or a `sourced` \
         claim against it is refused as naming a tool the agent cannot call.\n\
         2. Change the `{block}` block from `unavailable` to `sourced`, naming \
         the tool and the response field.\n\
         3. Set `coverage`: `complete` if the tool answers for every field or \
         honestly reports no match; `partial` if some fields have no source \
         even when it answers.\n\
         4. Rewrite the block's `why`. The existing one explains why nothing \
         could supply it, and that will no longer be true.\n\
         \n\
         Until step 1 happens the contract is unchanged and the block is still \
         correctly refusing.",
        if fields.is_empty() {
            "  (the block declares no fields yet)".to_string()
        } else {
            fields
                .iter()
                .map(|f| format!("  - {f}"))
                .collect::<Vec<_>>()
                .join("\n")
        },
        suggested
    );

    Ok(Json(json!({
        "ready": true,
        "suggested_tool_name": suggested,
        "brief": brief,
        "note": "A brief, not a tool. Paste it to a coding agent, or to \
                 xaman_ek, which knows the card contract and can check the \
                 result against it.",
    })))
}
