//! # Wild — the App, and the reason Rabble depends on it rather than the reverse
//!
//! `apps/kask_wild.json` declares Wild as a standalone App and says Rabble
//! creatures consume it: *"the creature provides spatial context; Wild provides
//! foraging intelligence."* Until this module existed the code did the opposite —
//! the only photographic-identification path was
//! `POST /api/creatures/:creature_id/forage`, with the creature mandatory, so
//! Wild's core capability was reachable only from inside the game.
//!
//! This module is Wild. It owns:
//!
//! - [`identify_specimen`] — what a photograph can and cannot establish, with no
//!   creature and no game in the signature
//! - [`identify_action_handler`] — `POST /api/workspaces/:id/actions/identify`
//! - the safety directive, which is a platform constant and not model output
//!
//! Rabble's creature route now calls [`identify_specimen`] and passes its
//! creature as *context*. The dependency arrow points the way the manifest
//! always said it did.
//!
//! ## Why the arrow matters more than tidiness
//!
//! The verification corpus (`src/verification.rs`) only works if determinations
//! accumulate across every submitter. `MIN_N_FOR_HEADLINE` is 30 and a useful
//! accuracy figure wants a few hundred. While identification required a creature,
//! every submission was bound to one, and a corpus partitioned that way never
//! reaches either threshold — each shard reporting "insufficient evidence"
//! indefinitely while the aggregate had been answerable for months. That failure
//! does not look like a bug. It looks like a quiet platform, which is why it
//! would have survived.
//!
//! ## What this module refuses to do, and on what evidence
//!
//! It does not say whether anything is safe to eat. Not as a hedge — as a
//! measured position:
//!
//! > Hodgson SE, McKenzie C, May TW, Greene SL. "A comparison of the accuracy of
//! > mushroom identification applications using digital photographs."
//! > Clin Toxicol (Phila). 2023 Mar;61(3):166-172. PMID 36794335.
//!
//! 78 specimens sent to the Victorian Poisons Information Centre and Royal
//! Botanic Gardens Victoria, each confirmed by an expert mycologist, through
//! three popular phone apps. Best accuracy 49%; iNaturalist 35%; *Amanita
//! phalloides* falsely identified by two of the three. The study exists because
//! its authors observed an increase in poisonings after poisonous species were
//! identified as edible using such apps.
//!
//! See `docs/specs/WILD_APP_DESIGN.md` for why that number shaped the whole App,
//! and `docs/specs/SOURCE_RELIABILITY.md` for what each source we call is
//! actually good for.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::handlers::workspace::actions::resolve_workspace;
use crate::AppState;
use fermi_auth::AuthPrincipal;

/// POST /api/creatures/:creature_id/forage
/// The safety warning returned by every `identify` response.
///
/// A platform constant, not model output, and that is the whole design. The
/// previous version asked the model for a `safety_note` "especially if toxic
/// look-alikes exist" — so the presence of a warning depended on the model
/// deciding a warning was warranted, on a call where it had already decided the
/// specimen was a `choice` edible. The call that omits the caution is
/// indistinguishable from the calls that include it, and it is the one that
/// matters.
///
/// Written as a plain statement rather than a hedge, and now a **measured** one.
///
/// Hodgson SE, McKenzie C, May TW, Greene SL. "A comparison of the accuracy of
/// mushroom identification applications using digital photographs."
/// Clin Toxicol (Phila). 2023 Mar;61(3):166-172. PMID 36794335.
///
/// 78 specimens sent to the Victorian Poisons Information Centre and Royal
/// Botanic Gardens Victoria over 2020-2021, each confirmed by an expert
/// mycologist, run through three popular phone identification apps. Best
/// accuracy 49% (Picture Mushroom); iNaturalist and Mushroom Identificator 35%
/// each. On the poisonous subset: 44%, 40%, 30%. *Amanita phalloides* was
/// **falsely identified** twice by Picture Mushroom and once by iNaturalist. The
/// paper's stated motivation is an observed increase in poisonings following
/// incorrect identification of poisonous species as edible using such apps.
///
/// Two things follow, and the second is the uncomfortable one.
///
/// First, the directive stops being an assertion about mycology and becomes a
/// citation. "Photographs cannot exclude lethal lookalikes" was reasonable
/// before; it is now measured, and a wearer can go and read the measurement.
///
/// Second, **this platform has no accuracy figure of its own.** The paper
/// measures three specific apps, not a general vision model, so 49% is not our
/// number and must not be quoted as one. What it is, is the best published
/// figure for the task, produced on precisely the population that matters —
/// specimens sent to a poisons centre, self-selected for being confusing. Until
/// someone measures this handler on a comparable set, the honest prior is that
/// it is no better than these, and the confidence interval on the best of them
/// spans [0-100].
pub const FORAGE_SAFETY_DIRECTIVE: &str = "This is a guess from one photograph. It is \
    not an identification and it says nothing about whether the specimen is safe \
    to eat. Photographs cannot exclude lethal lookalikes: several deadly species \
    are separated from edible ones only by spore print, cut-flesh reaction, stipe \
    base, or microscopy. This is measured, not cautionary — the best of three \
    popular phone identification apps was correct for 49% of 78 expert-confirmed \
    specimens sent to a poisons centre, and death caps were misidentified by two \
    of the three (Hodgson et al., Clin Toxicol 2023, PMID 36794335). No accuracy \
    figure exists for this system, so assume it is no better. Do not eat anything \
    on the strength of this response.";

/// What would actually answer the question this handler refuses.
///
/// Paired with the directive because a refusal that does not say where to go
/// next gets ignored, and the person ignoring it is then relying on the guess
/// alone. The refusal is only useful if it redirects.
pub const FORAGE_NEXT_STEPS: &[&str] = &[
    "Have the specimen checked in person by a local expert or mycological society.",
    "Work it through a regional key, with the specimen in hand and a spore print taken.",
    "If anyone has already eaten it, contact poison control immediately and keep the specimen.",
];

/// Resolve a guessed name against GBIF, and MycoBank when it is a fungus.
///
/// Returns an empty object when nothing resolved, which `grounding_trust` reads
/// as `tool_no_match` — "the databases were asked and did not recognise this
/// name". That is materially different from "no database was consulted", and for
/// a forager it is the more useful of the two: an unresolvable binomial on a
/// confident-sounding determination usually means the epithet was invented.
///
/// A lookup failure is never fatal to the response. The identification and the
/// safety directive stand on their own, and losing the taxonomy to a network
/// blip should not deny someone the warning.
pub(crate) async fn resolve_forage_taxonomy(name: &str, kingdom: &str) -> serde_json::Value {
    // `fermi::`, not `crate::`: this file is `#[path]`-included into the
    // api-server binary, so `crate::` is the binary. The `crate::grounding_trust`
    // paths elsewhere in this file work only because api_server.rs re-exports
    // that module with `pub(crate) use`. Reaching the tools directly avoids
    // adding another crate-level re-export for two functions.
    use fermi::agent_backend::tools::{execute_gbif_species_search, execute_mycobank_lookup};

    let mut out = serde_json::Map::new();

    // `gbif_species_search` defaults to Insecta, so an unscoped search for a
    // mushroom returns insects whose text matches. The scope is chosen from the
    // model's kingdom guess; a wrong guess surfaces as `tool_no_match` rather
    // than as a confident ladder for the wrong organism.
    let scope = match kingdom {
        "fungi" => Some("fungi"),
        "plantae" | "plant" | "plants" => Some("plantae"),
        "animalia" | "animal" | "animals" => Some("animalia"),
        _ => None,
    };

    if let Some(scope) = scope {
        let query = json!({ "query": name, "scope": scope, "limit": 1 });
        if let Ok(raw) = execute_gbif_species_search(&query).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(hit) = parsed.pointer("/species/0") {
                    for (from, to) in [
                        ("kingdom", "kingdom"),
                        ("phylum", "phylum"),
                        ("class", "class"),
                        ("order", "order"),
                        ("family", "family"),
                        ("genus", "genus"),
                        ("species", "species"),
                        ("scientificName", "matched_name"),
                        ("vernacularName", "vernacular_name"),
                        ("taxonomicStatus", "taxonomic_status"),
                        ("key", "gbif_usage_key"),
                    ] {
                        if let Some(v) = hit.get(from) {
                            if !v.is_null() {
                                out.insert(to.to_string(), v.clone());
                            }
                        }
                    }
                    out.insert("scope_searched".into(), json!(scope));
                }
            }
        }
    }

    if kingdom == "fungi" {
        let query = json!({ "name": name, "include_synonyms": true });
        if let Ok(raw) = execute_mycobank_lookup(&query).await {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(status) = parsed.get("status").filter(|v| !v.is_null()) {
                    out.insert("nomenclatural_status".into(), status.clone());
                }
                if let Some(accepted) = parsed.get("accepted_name").filter(|v| !v.is_null()) {
                    out.insert("accepted_name".into(), accepted.clone());
                }
                // Which database actually answered. The tool falls back to GBIF
                // when MYCOBANK_API_KEY is unset and says so; passing that
                // through is what keeps the value traceable rather than merely
                // present.
                if let Some(src) = parsed.get("source").filter(|v| !v.is_null()) {
                    out.insert("nomenclature_source".into(), src.clone());
                }
            }
        }
    }

    serde_json::Value::Object(out)
}

// ─── the determination itself ──────────────────────────────────────────

/// Ask what a photograph appears to show, ground the name, and refuse the safety
/// question.
///
/// **No creature in the signature, and that is the point.** This is Wild's
/// capability; Rabble is one caller.
///
/// Returns the enforced document: `identification` (a judgement), `taxonomy` (a
/// real retrieval, keyed on that judgement), and `safety` (a platform constant),
/// plus the `_provenance` stamps `grounding_trust` writes and a list of anything
/// it had to strip.
///
/// Errors are transport and configuration failures only. A specimen it cannot
/// place is a successful call returning nulls — "I could not tell" is an answer,
/// and the most useful one this function has.
pub async fn identify_specimen(
    photo_url: &str,
    locality: Option<&str>,
    habitat: Option<&str>,
    api_key: &str,
) -> Result<(Value, fermi::grounding_trust::Report), String> {
    let habitat_hint = habitat.unwrap_or("unknown habitat");
    let location_hint = locality.unwrap_or("unknown");

    // The prompt does not ask for edibility, look-alikes, a harvest window or a
    // self-rated confidence. Stripping those afterwards is the backstop, not the
    // fix: asking and then nulling spends the model's attention on the answer a
    // forager most wants and least ought to receive, and pushes the claim into
    // whatever prose field survives — which is exactly how genome_profiler's
    // summary ended up restating numbers already cleared from its fields.
    let request_body = json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1024,
        "system": "You are a field mycologist looking at a photograph. Your job is to \
                   say what the specimen appears to be and WHY, so the person holding \
                   it can check your reasoning against the specimen itself.\n\n\
                   You are not a safety authority and you must not act as one. Do NOT \
                   state or imply edibility, toxicity, whether something is safe to \
                   eat, which species it could be confused with, when to harvest it, \
                   or how to prepare it. You have no database for any of that; you \
                   have a photograph.\n\n\
                   This is measured, not cautionary. The best of three popular phone \
                   identification apps was correct for 49% of 78 expert-confirmed \
                   specimens sent to a poisons centre, and death caps were \
                   misidentified by two of the three (Hodgson et al., Clin Toxicol \
                   2023). Assume you are no better.\n\n\
                   Be precise about how far down the ladder the photograph actually \
                   supports. 'Genus Amanita, species undetermined' is a better answer \
                   than a binomial you cannot see enough to justify. Say what you \
                   cannot see: gills obscured, no stipe base, no spore print, no \
                   scale reference. Respond in JSON only.",
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image", "source": { "type": "url", "url": photo_url } },
                {
                    "type": "text",
                    "text": format!(
                        "What does this specimen appear to be? Location: {}. \
                         Habitat: {}.\n\n\
                         Respond with JSON only:\n\
                         {{\n\
                           \"species\": \"scientific name, or null if the photo does not support one\",\n\
                           \"common_name\": \"common name, or null\",\n\
                           \"kingdom\": \"fungi|plantae|animalia|null — used to scope the database lookup\",\n\
                           \"rank_reached\": \"species|genus|family|null — how far the photo actually supports\",\n\
                           \"visual_features\": \"the features in THIS photo that led you there\",\n\
                           \"not_visible\": \"what the photo does not show that would matter\",\n\
                           \"safety_note\": \"say only that a photograph cannot establish safety. No verdict.\"\n\
                         }}",
                        location_hint, habitat_hint
                    )
                }
            ]
        }]
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("Vision API request failed: {e}"))?;

    if !resp.status().is_success() {
        let err = resp.text().await.unwrap_or_default();
        return Err(format!("Vision API error: {err}"));
    }

    let claude_resp: Value = resp.json().await.map_err(|e| e.to_string())?;
    let raw_text = claude_resp
        .pointer("/content/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // `envelope::extract_json` rather than a local parser: it is the canonical
    // implementation in the lib, it already handles the shapes models actually
    // emit (bare JSON, a ```json fence, prose with an object inside), and reaching
    // into Rabble's private module for a copy would have Wild depending on the
    // game — the arrow this module exists to correct.
    let identification = fermi::agent_backend::envelope::extract_json(raw_text).unwrap_or(json!({
        "species": null,
        "common_name": null,
        "rank_reached": null,
        "visual_features": null,
        "safety_note": "A photograph cannot establish whether this is safe to eat.",
    }));

    // Ground the name. This is what moves the response from honest to checkable:
    // the model produced a name from pixels, and GBIF is now asked whether that
    // name resolves and to what. Neither confirms the identification — both are
    // keyed on the guess — but a forager can follow every value to a database,
    // and a name that fails to resolve signals an invented epithet.
    let guessed = identification
        .get("species")
        .and_then(|v: &Value| v.as_str())
        .map(str::trim)
        .filter(|s: &&str| !s.is_empty());
    let kingdom = identification
        .get("kingdom")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let taxonomy = match guessed {
        None => json!({}),
        Some(name) => resolve_forage_taxonomy(name, &kingdom).await,
    };

    let mut document = json!({
        "identification": identification,
        "taxonomy": taxonomy,
        // Written here, by Rust, and never by the model. A model-authored caution
        // can be softened or dropped on any given call, and the call where it is
        // dropped looks exactly like the ones where it is not.
        "safety": {
            "determination_basis": "photograph only",
            "edibility_source": null,
            "lookalike_check_performed": false,
            "directive": FORAGE_SAFETY_DIRECTIVE,
            "what_would_answer_it": FORAGE_NEXT_STEPS,
        },
    });

    // The backstop. If a future prompt edit reintroduces an edibility verdict, or
    // the model volunteers one unasked, it is cleared here and the removal is
    // reported rather than shipped.
    let grounding = fermi::grounding_trust::enforce("forage_identify", &mut document);

    if let Some(obj) = document.as_object_mut() {
        obj.insert(
            "grounding_violations".into(),
            json!(grounding
                .violations
                .iter()
                .map(|v| json!({ "path": v.path, "removed": v.removed }))
                .collect::<Vec<_>>()),
        );
    }

    // The report travels with the document.
    //
    // It used to be consumed here: the violations were summarised into a field
    // of the response body and the `Report` was dropped, so the only record
    // that the control had fired lived in a JSON blob nobody aggregates. This
    // is a pure helper with no store, so it cannot raise the anomaly itself —
    // it hands the report to callers that can.
    Ok((document, grounding))
}

// ─── the workspace action ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct IdentifyRequest {
    /// Raw URL of an already-uploaded photograph.
    photo_url: String,
    /// Free-text locality as the submitter gave it.
    ///
    /// Not a coordinate, deliberately. A precise location for a rare or
    /// over-collected species is a conservation risk, and this endpoint should not
    /// be the reason a patch gets stripped.
    #[serde(default)]
    locality: Option<String>,
    #[serde(default)]
    habitat: Option<String>,
    /// A Rabble creature that was present, if any.
    ///
    /// **Context, not ownership** — the same shape `log_observation` already uses.
    /// Echoed back so a caller can correlate, and never required.
    #[serde(default)]
    creature_context: Option<String>,
}

/// `POST /api/workspaces/:workspace_id/actions/identify`
///
/// Wild's identification capability, reachable without a creature. This is the
/// endpoint the glasses shell and any other surface should call; the Rabble
/// creature route is a second caller of the same function, not the way in.
pub async fn identify_action_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<IdentifyRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "ANTHROPIC_API_KEY not set".to_string(),
        )
    })?;

    let (document, grounding) = identify_specimen(
        &req.photo_url,
        req.locality.as_deref(),
        req.habitat.as_deref(),
        &api_key,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // Tell Loop 2. `None` for the episode: this path persists nothing.
    fermi::grounding_anomaly::spawn_raise(
        std::sync::Arc::clone(&state.memory_store),
        "forage_identify",
        None,
        grounding,
    );

    Ok(Json(json!({
        "workspace_id": ws_uuid,
        "photo_url": req.photo_url,
        // Echoed, not owned. The corpus is App-scoped; a creature is context.
        "creature_context": req.creature_context,
        "identification": document.get("identification"),
        "identification_provenance": document.get("identification_provenance"),
        "taxonomy": document.get("taxonomy"),
        "taxonomy_provenance": document.get("taxonomy_provenance"),
        "safety": document.get("safety"),
        "safety_provenance": document.get("safety_provenance"),
        "grounding_violations": document.get("grounding_violations"),
    })))
}
