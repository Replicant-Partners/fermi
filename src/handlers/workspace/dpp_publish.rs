//! Releasing a product passport to people with no ABW account.
//!
//! ```text
//! POST   /api/workspaces/:id/dpp/publish   admin  — release (or re-release) a snapshot
//! GET    /api/workspaces/:id/dpp/publish   member — is it public, and at what link
//! DELETE /api/workspaces/:id/dpp/publish   admin  — withdraw it
//! GET    /api/dpp/p/:token                 PUBLIC — the released snapshot
//! GET    /api/dpp/p/:token/qr              PUBLIC — the data carrier for it
//! GET    /dpp/:token                       PUBLIC — short link for the QR
//! ```
//!
//! # The allowlist is the design
//!
//! The snapshot is built field by field from an explicit allowlist, never by
//! copying a document and deleting what looks sensitive. A denylist fails open:
//! the day someone adds `supplier_contract_price` to the composition, a
//! denylist publishes it. An allowlist fails closed — a new field is private
//! until someone decides otherwise, here, in a diff a reviewer can see.
//!
//! Private, always, whatever sections are chosen:
//!   - pricing (`dpp/bom_pricing.yaml`) — commercially sensitive, never offered
//!   - ingredient `part_number`s — supplier codes, which name suppliers
//!   - claim `notes` — internal working text, e.g. "CFU count substantiated by lot testing"
//!   - the workspace id, members, agents, wallet, and the action log
//!
//! # Verification is carried, not implied
//!
//! A public page is exactly where an unverified agent output gets read as
//! settled. So every released claim verdict keeps its `needs_expert` flag and
//! its provenance stamps, the carbon section keeps `verification_status`, and
//! the snapshot carries a fixed statement of what produced it. The public page
//! renders those first.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect},
    Json,
};
use fermi_auth::{teams, AuthPrincipal, TeamRole};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use sqlx::Row;
use uuid::Uuid;

use crate::AppState;

/// Sections a publisher may release. Identity is always included: a passport
/// that does not say what product it is for is not a passport.
pub const PUBLISHABLE_SECTIONS: &[&str] = &["claims", "composition", "carbon"];

const CLAIMS_PATH: &str = "dpp/claims.yaml";
const COMPOSITION_PATH: &str = "dpp/composition.yaml";
const STATEMENT_PATH: &str = "dpp/carbon/statement.yaml";
const EVALUATED_PREFIX: &str = "regulatory-lens/ontology/evaluated/";

/// The fixed statement every release carries, rendered above the content.
const PROVENANCE_NOTICE: &str = "Regulatory readings and the carbon figures in \
    this passport were produced by AI agents on Agent Bestiary from public \
    sources, and are marked with how each was arrived at. They are not \
    independently verified unless an endorsement is shown beside them, and \
    nothing here is legal or compliance advice.";

#[derive(Debug, Deserialize, Default)]
pub struct PublishRequest {
    /// Which of [`PUBLISHABLE_SECTIONS`] to release. Absent = all of them.
    #[serde(default)]
    pub sections: Option<Vec<String>>,
    /// Mint a new token, so links and printed codes already out stop working.
    #[serde(default)]
    pub rotate: bool,
}

fn base_url() -> String {
    std::env::var("APP_BASE_URL").unwrap_or_else(|_| "https://agent-bestiary.world".to_string())
}

fn public_url(token: &str) -> String {
    format!("{}/dpp/{token}", base_url())
}

/// ~124 bits. Long enough not to be enumerable on an endpoint with no login.
fn mint_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..24)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

/// Membership with a minimum role, returning the git slug.
async fn require_role(
    state: &AppState,
    workspace_id: &str,
    user_id: &str,
    min: TeamRole,
) -> Result<(Uuid, String), (StatusCode, String)> {
    let ws: Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;
    let role = teams::get_member_role(&state.db, ws, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;
    if role < min {
        return Err((
            StatusCode::FORBIDDEN,
            format!(
                "Releasing a passport publicly needs the {min:?} role or above; \
                 you are {role:?}. It is a statement made to people outside \
                 this workspace, so it is not an ordinary member action."
            ),
        ));
    }
    let slug: String = sqlx::query_scalar("SELECT slug FROM teams WHERE id = $1")
        .bind(ws)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".to_string()))?;
    Ok((ws, slug))
}

// ─── snapshot: an allowlist over the workspace's documents ──────────────────

fn yaml(bytes: &[u8]) -> Option<Value> {
    serde_yaml::from_slice::<Value>(bytes).ok()
}

/// Copy only `keys` from `src`, dropping nulls. The function every section
/// goes through, so "what is public" is always a literal list of names.
fn pick(src: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for k in keys {
        if let Some(v) = src.get(*k) {
            if !v.is_null() {
                out.insert((*k).to_string(), v.clone());
            }
        }
    }
    Value::Object(out)
}

fn identity_section(composition: Option<&Value>) -> Value {
    composition
        .map(|c| {
            pick(
                c,
                &[
                    "product_id",
                    "display_name",
                    "part_number",
                    "bom_version",
                    "serving",
                ],
            )
        })
        .unwrap_or_else(|| json!({}))
}

/// Ingredients without their supplier codes. `part_number` on an ingredient
/// line names who supplies it, which is commercial information; the product's
/// own `part_number` is on the label already and is in the identity section.
fn composition_section(composition: &Value) -> Value {
    let lines: Vec<Value> = composition
        .get("consists_of")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|l| {
                    pick(
                        l,
                        &[
                            "item_id",
                            "display_name",
                            "role",
                            "quantity",
                            "category",
                            "origin",
                        ],
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "ingredients": lines, "bom_source": composition.get("bom_source") })
}

/// One evaluated verdict, reduced to what a reader needs to judge it.
fn verdict(v: &Value) -> Value {
    let citations: Vec<Value> = v
        .get("citations")
        .and_then(|c| c.as_array())
        .map(|a| {
            a.iter()
                .map(|c| pick(c, &["url", "title", "provision"]))
                .collect()
        })
        .unwrap_or_default();
    let mut out = pick(
        v,
        &[
            "market",
            "status",
            "basis",
            "rendered_text",
            "needs_expert",
            "provenance",
            "endorsement",
        ],
    );
    if let Value::Object(m) = &mut out {
        m.insert("citations".into(), Value::Array(citations));
    }
    out
}

/// `files` is (path, parsed document) for everything under the evaluated
/// prefix. Taken as data rather than read here so the reduction is testable
/// without a repository.
fn claims_section(claims: &Value, evaluated: &[(String, Value)]) -> Value {
    let list: Vec<Value> = claims
        .get("source_claims")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|c| {
                    let id = c.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let mut row = pick(c, &["id", "candidate_text", "status"]);
                    let verdicts: Vec<Value> = evaluated
                        .iter()
                        .filter(|(path, _)| {
                            path.rsplit('/')
                                .next()
                                .and_then(|f| f.strip_suffix(".yaml"))
                                == Some(id)
                        })
                        .map(|(path, doc)| {
                            let mut v = verdict(doc);
                            // The market is the directory, which is the
                            // authoritative key even if the body omits it.
                            if let (Value::Object(m), Some(mk)) = (
                                &mut v,
                                path.strip_prefix(EVALUATED_PREFIX)
                                    .and_then(|r| r.split('/').next()),
                            ) {
                                m.insert("market".into(), Value::String(mk.to_uppercase()));
                            }
                            v
                        })
                        .collect();
                    if let Value::Object(m) = &mut row {
                        // An unevaluated claim is released as such, not
                        // omitted: "nobody has read this against EU law" is a
                        // fact a reader of a label is entitled to.
                        m.insert("evaluations".into(), Value::Array(verdicts));
                    }
                    row
                })
                .collect()
        })
        .unwrap_or_default();
    json!({ "claims": list })
}

fn carbon_section(statement: &Value) -> Value {
    let inv = statement.get("inventory").cloned().unwrap_or(Value::Null);
    let items: Vec<Value> = inv
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|i| {
                    pick(
                        i,
                        &[
                            "item_id",
                            "material",
                            "factor_kg_co2e_per_kg",
                            "factor_unit",
                            "reference_flow",
                            "geography",
                            "reference_year",
                            "dataset",
                            "source_url",
                            "kg_co2e",
                            "corroboration",
                            "basis_refusal",
                        ],
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "total_kg_co2e": inv.get("total_kg_co2e"),
        "coverage": inv.get("coverage"),
        "unpriced_items": inv.get("unpriced_items"),
        "items": items,
        "boundary": statement.get("boundary").map(|b| pick(b, &["declared", "excluded", "standard_followed"])),
        "assurance": statement.get("assurance").map(|a| pick(a, &["needs_expert", "verification_status", "regulatory_fitness"])),
        "provenance": statement.get("provenance"),
        "calculated_at": statement.get("calculated_at"),
        "endorsement": statement.get("endorsement"),
    })
}

/// The whole public document. Pure over already-read inputs.
fn build_snapshot(
    sections: &[String],
    composition: Option<&Value>,
    claims: Option<&Value>,
    evaluated: &[(String, Value)],
    statement: Option<&Value>,
) -> Value {
    let has = |s: &str| sections.iter().any(|x| x == s);
    let mut out = Map::new();
    out.insert("identity".into(), identity_section(composition));
    out.insert("notice".into(), Value::String(PROVENANCE_NOTICE.into()));
    out.insert("sections".into(), json!(sections));
    // A section that was chosen but has no document is released as absent,
    // not dropped, for the same reason an unevaluated claim is kept.
    if has("composition") {
        out.insert(
            "composition".into(),
            composition.map(composition_section).unwrap_or(Value::Null),
        );
    }
    if has("claims") {
        out.insert(
            "claims".into(),
            claims
                .map(|c| claims_section(c, evaluated))
                .unwrap_or(Value::Null),
        );
    }
    if has("carbon") {
        out.insert(
            "carbon".into(),
            statement.map(carbon_section).unwrap_or(Value::Null),
        );
    }
    Value::Object(out)
}

/// Read the four documents and build the snapshot.
async fn snapshot_from_workspace(
    state: &AppState,
    slug: &str,
    sections: &[String],
) -> Result<Value, (StatusCode, String)> {
    let git = state.workspace_git.clone();
    let slug = slug.to_string();
    let sections = sections.to_vec();
    tokio::task::spawn_blocking(move || {
        let read = |p: &str| git.read_file_bytes(&slug, p).ok().and_then(|b| yaml(&b));
        let composition = read(COMPOSITION_PATH);
        let claims = read(CLAIMS_PATH);
        let statement = read(STATEMENT_PATH);
        let evaluated: Vec<(String, Value)> = git
            .list_files(&slug, Some(EVALUATED_PREFIX))
            .unwrap_or_default()
            .into_iter()
            .filter(|e| !e.is_dir && e.path.ends_with(".yaml"))
            .filter_map(|e| {
                git.read_file_bytes(&slug, &e.path)
                    .ok()
                    .and_then(|b| yaml(&b))
                    .map(|v| (e.path, v))
            })
            .collect();
        build_snapshot(
            &sections,
            composition.as_ref(),
            claims.as_ref(),
            &evaluated,
            statement.as_ref(),
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

fn normalise_sections(req: Option<Vec<String>>) -> Result<Vec<String>, (StatusCode, String)> {
    let chosen =
        req.unwrap_or_else(|| PUBLISHABLE_SECTIONS.iter().map(|s| s.to_string()).collect());
    if let Some(bad) = chosen
        .iter()
        .find(|s| !PUBLISHABLE_SECTIONS.contains(&s.as_str()))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "`{bad}` is not a publishable section. Publishable: {}. Pricing \
                 is never offered — it is commercially sensitive and there is no \
                 switch for it here on purpose.",
                PUBLISHABLE_SECTIONS.join(", ")
            ),
        ));
    }
    let mut out: Vec<String> = Vec::new();
    for s in chosen {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    Ok(out)
}

fn status_json(row: &sqlx::postgres::PgRow) -> Value {
    let token: String = row.get("token");
    json!({
        "published": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at").is_none(),
        "token": token,
        "url": public_url(&token),
        "qr": format!("{}/api/dpp/p/{token}/qr", base_url()),
        "release": row.get::<i32, _>("release"),
        "sections": row.get::<Vec<String>, _>("sections"),
        "published_at": row.get::<chrono::DateTime<chrono::Utc>, _>("published_at"),
        "first_published_at": row.get::<chrono::DateTime<chrono::Utc>, _>("first_published_at"),
        "revoked_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at"),
    })
}

// ─── handlers ───────────────────────────────────────────────────────────────

/// POST /api/workspaces/:workspace_id/dpp/publish
pub async fn publish_dpp_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    // Required, not `Option<Json<_>>`. An optional extractor turns a malformed
    // body into `None`, which would fall through to the default of releasing
    // EVERY section — so a publisher who narrowed the scope and sent a typo
    // would publish more than they chose. Send `{}` for the defaults.
    Json(req): Json<PublishRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws, slug) = require_role(&state, &workspace_id, &user_id, TeamRole::Admin).await?;
    let sections = normalise_sections(req.sections)?;
    let snapshot = snapshot_from_workspace(&state, &slug, &sections).await?;

    let existing =
        sqlx::query("SELECT publication_id, token FROM dpp_publications WHERE workspace_id = $1")
            .bind(ws)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = match existing {
        None => {
            sqlx::query(
                "INSERT INTO dpp_publications (workspace_id, token, snapshot, sections)
             VALUES ($1, $2, $3, $4) RETURNING *",
            )
            .bind(ws)
            .bind(mint_token())
            .bind(&snapshot)
            .bind(&sections)
            .fetch_one(&state.db)
            .await
        }
        Some(r) => {
            let token: String = if req.rotate {
                mint_token()
            } else {
                r.get("token")
            };
            sqlx::query(
                "UPDATE dpp_publications
                    SET snapshot = $2, sections = $3, token = $4, release = release + 1,
                        published_at = NOW(), revoked_at = NULL
                  WHERE publication_id = $1 RETURNING *",
            )
            .bind(r.get::<Uuid, _>("publication_id"))
            .bind(&snapshot)
            .bind(&sections)
            .bind(token)
            .fetch_one(&state.db)
            .await
        }
    }
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // History. Soft-fail would lose the one record an auditor needs, so this
    // propagates: a release that cannot be recorded is not made.
    sqlx::query(
        "INSERT INTO dpp_publication_releases (publication_id, release, snapshot, sections, released_by)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(row.get::<Uuid, _>("publication_id"))
    .bind(row.get::<i32, _>("release"))
    .bind(&snapshot)
    .bind(&sections)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("release history not recorded: {e}")))?;

    let mut out = status_json(&row);
    if let Value::Object(m) = &mut out {
        m.insert("snapshot".into(), snapshot);
        m.insert("rotated".into(), Value::Bool(req.rotate));
    }
    Ok(Json(out))
}

/// GET /api/workspaces/:workspace_id/dpp/publish
pub async fn publication_status_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws, _) = require_role(&state, &workspace_id, &user_id, TeamRole::Viewer).await?;
    let row = sqlx::query("SELECT * FROM dpp_publications WHERE workspace_id = $1")
        .bind(ws)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(match row {
        Some(r) => status_json(&r),
        None => json!({ "published": false, "never_published": true,
                        "publishable_sections": PUBLISHABLE_SECTIONS }),
    }))
}

/// DELETE /api/workspaces/:workspace_id/dpp/publish
pub async fn unpublish_dpp_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws, _) = require_role(&state, &workspace_id, &user_id, TeamRole::Admin).await?;
    let row = sqlx::query(
        "UPDATE dpp_publications SET revoked_at = NOW()
          WHERE workspace_id = $1 AND revoked_at IS NULL RETURNING *",
    )
    .bind(ws)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "This passport is not currently public.".to_string(),
    ))?;
    Ok(Json(status_json(&row)))
}

/// GET /api/dpp/p/:token — PUBLIC.
pub async fn public_dpp_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT snapshot, release, published_at, first_published_at, revoked_at
           FROM dpp_publications WHERE token = $1",
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((
        StatusCode::NOT_FOUND,
        "No passport at this link.".to_string(),
    ))?;

    if row
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("revoked_at")
        .is_some()
    {
        return Err((
            StatusCode::GONE,
            "This passport was published and has since been withdrawn by its \
             publisher."
                .to_string(),
        ));
    }

    let body = json!({
        "passport": row.get::<Value, _>("snapshot"),
        "release": row.get::<i32, _>("release"),
        "published_at": row.get::<chrono::DateTime<chrono::Utc>, _>("published_at"),
        "first_published_at": row.get::<chrono::DateTime<chrono::Utc>, _>("first_published_at"),
    });
    Ok((
        [
            (header::CACHE_CONTROL, "public, max-age=60"),
            // Readable from a page on any origin — a retailer's or a
            // regulator's — without credentials. It is public by definition.
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        Json(body),
    ))
}

/// GET /api/dpp/p/:token/qr — PUBLIC. The code to print.
pub async fn public_dpp_qr_handler(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let live: Option<bool> =
        sqlx::query_scalar("SELECT revoked_at IS NULL FROM dpp_publications WHERE token = $1")
            .bind(&token)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if live != Some(true) {
        return Err((
            StatusCode::NOT_FOUND,
            "No live passport at this link.".to_string(),
        ));
    }
    let png = crate::handlers::qr_codes::qr_png(&public_url(&token), 360)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
        ],
        png,
    ))
}

/// GET /dpp/:token — PUBLIC. A short link, so the QR stays small and scans
/// from further away.
pub async fn public_dpp_redirect(Path(token): Path<String>) -> Redirect {
    // Tokens are [a-z0-9]; anything else is not ours and should not be
    // reflected into a Location header.
    let clean: String = token
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(64)
        .collect();
    Redirect::temporary(&format!("/static/adaptogen-lab/passport.html?t={clean}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition() -> Value {
        json!({
            "product_id": "pk", "display_name": "Precision Kombucha", "part_number": "PKH-F2-330",
            "consists_of": [
                { "item_id": "black_tea", "display_name": "Black tea", "role": "consumable",
                  "quantity": "0.8%", "part_number": "TEA-BLK-SL", "origin": "Sri Lanka",
                  "unit_cost_eur": 0.04 }
            ]
        })
    }

    /// Supplier codes and anything priced stay private, including fields
    /// nobody has thought of yet — the allowlist fails closed.
    #[test]
    fn ingredient_supplier_codes_and_unknown_fields_are_not_released() {
        let snap = build_snapshot(
            &["composition".into()],
            Some(&composition()),
            None,
            &[],
            None,
        );
        let line = &snap["composition"]["ingredients"][0];
        assert_eq!(line["display_name"], "Black tea");
        assert_eq!(line["origin"], "Sri Lanka");
        assert!(
            line.get("part_number").is_none(),
            "an ingredient part_number names its supplier"
        );
        assert!(
            line.get("unit_cost_eur").is_none(),
            "a field the allowlist does not name is private, which is the point of an allowlist"
        );
        // The product's own code is on its label, so it is in the identity.
        assert_eq!(snap["identity"]["part_number"], "PKH-F2-330");
    }

    #[test]
    fn internal_claim_notes_are_not_released_and_unevaluated_claims_are_kept() {
        let claims = json!({ "source_claims": [
            { "id": "low_sugar", "candidate_text": "Low in sugar", "status": "draft",
              "notes": "internal: margin is thin on this one" },
            { "id": "live_cultures", "candidate_text": "Contains live cultures" }
        ]});
        let evaluated = vec![(
            format!("{EVALUATED_PREFIX}eu/low_sugar.yaml"),
            json!({ "status": "not_allowed", "needs_expert": true,
                    "provenance": { "verdict": "model_inference", "evidence": "tool_verified" },
                    "citations": [{ "url": "https://x", "title": "Reg 1924", "snippet": "long" }],
                    "queries_run": ["internal search terms"] }),
        )];
        let snap = build_snapshot(&["claims".into()], None, Some(&claims), &evaluated, None);
        let list = snap["claims"]["claims"].as_array().unwrap();
        assert!(
            list[0].get("notes").is_none(),
            "claim notes are internal working text"
        );

        let v = &list[0]["evaluations"][0];
        assert_eq!(v["market"], "EU", "market comes from the directory");
        assert_eq!(
            v["needs_expert"], true,
            "the expert flag must survive publication"
        );
        assert_eq!(v["provenance"]["verdict"], "model_inference");
        assert!(v.get("queries_run").is_none());
        assert!(v["citations"][0].get("snippet").is_none());

        assert_eq!(
            list[1]["evaluations"].as_array().unwrap().len(),
            0,
            "an unevaluated claim is released as unevaluated, not omitted"
        );
    }

    #[test]
    fn a_chosen_section_with_no_document_is_released_as_absent() {
        let snap = build_snapshot(&["carbon".into()], None, None, &[], None);
        assert!(snap.get("carbon").is_some_and(|v| v.is_null()));
        assert!(
            snap.get("claims").is_none(),
            "an unchosen section is not mentioned at all"
        );
        assert!(snap["notice"]
            .as_str()
            .unwrap()
            .contains("not independently verified"));
    }

    #[test]
    fn pricing_cannot_be_requested() {
        assert!(normalise_sections(Some(vec!["pricing".into()])).is_err());
        assert_eq!(
            normalise_sections(None).unwrap().len(),
            PUBLISHABLE_SECTIONS.len()
        );
    }

    #[test]
    fn tokens_are_long_enough_not_to_be_enumerated() {
        let t = mint_token();
        assert_eq!(t.len(), 24);
        assert!(t
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_ne!(t, mint_token());
    }
}
