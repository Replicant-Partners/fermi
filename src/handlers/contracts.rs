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

use axum::{http::StatusCode, Json};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::Value;

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

    let parsed: Value = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("compiler emitted unparseable output: {e}"),
        )
    })?;

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
    Json(serde_json::json!({ "tools": names }))
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
