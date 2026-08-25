//! The Bestiary — one register, three lenses.
//!
//! # Why one endpoint
//!
//! There are three registers of the same agents in the product today, each
//! separately implemented: the catalogue grid (`templates/index.html`), the
//! ecology register (`templates/ecology.html`), and the Observatory's patient
//! register. They answer three real but different questions —
//!
//! | lens | question |
//! |---|---|
//! | Discover | which agents exist, and which do I want? |
//! | Population | how is the population distributed, and where did it come from? |
//! | Health | which of these needs attention? |
//!
//! — over **one list**. Three code paths, three card grammars, three sets of
//! column names, and a reader who cannot tell which list they are on. So this
//! serves the rows once and the client changes columns and sort. A lens is a
//! view, not a page.
//!
//! # The card grammar
//!
//! Four zones in fixed positions, so the grammar is learned once and every card
//! is readable — the Magic test: *the card is sufficient to decide with, and
//! there is no separate manual.*
//!
//! | zone | content | note |
//! |---|---|---|
//! | cost | `tier`, `min_tier` | always the same corner, so a register is scannable by cost alone |
//! | type line | `agent_type` — `genus species` | the seven-rank `taxonomy` has been on the row since migration 186 and **has never reached the client** |
//! | body | `accepts` → `produces`, as studs | hollow when asserted, filled when a schema resolves |
//! | evidence | evolution level · runs | with `Untried, not failing` preserved for the unranked |
//!
//! plus a provenance mark, which Ecology already draws and nothing else does.
//!
//! # Studs are labels, not types, and the register says so
//!
//! Measured live rather than quoted from a comment: of the labels declared
//! across published agents, only a small minority appear on **both** an
//! `accepts` and a `produces`, so the rest cannot form a seam with anything.
//! `DESIGN_UX_PANEL_ARCHITECTURE.md` §4.2 records why this must ship before any
//! drag-and-snap surface: a Lego rendering of a bin of bricks that mostly do not
//! connect reads as a broken panel rather than as a fragmented vocabulary. **The
//! composability problem is a naming problem, and the register's job is to say
//! so.**

use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

/// `GET /api/bestiary`
///
/// One payload for all three lenses. The client switches lens without a refetch,
/// which is the point: same rows, different columns.
pub async fn bestiary_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // One row per publicly visible specimen, carrying every zone of the card.
    //
    // `episodes` is a correlated subquery rather than a join: joining it
    // alongside anomaly_events would multiply rows, which is the classic way
    // these counts silently inflate.
    let rows = sqlx::query(
        "SELECT a.agent_name,
                COALESCE(a.display_alias, a.agent_name) AS label,
                a.description,
                a.agent_type,
                a.tier,
                a.min_tier,
                a.llm_provider,
                a.model,
                a.tags,
                a.accepts,
                a.produces,
                a.taxonomy,
                a.fork_count,
                a.forked_from,
                (a.output_contract IS NOT NULL)              AS has_contract,
                (a.output_contract -> 'schema' IS NOT NULL)  AS has_schema,
                om.source                                    AS provenance,
                ev.current_level                             AS level,
                ev.peak_level                                AS peak_level,
                (SELECT count(*) FROM episodes e
                  WHERE e.agent_id = a.agent_id)             AS runs,
                -- What it has actually cost, and the mean per run. `cost_usd` is
                -- NUMERIC, so the cast is at the query: decoding a numeric as
                -- f64 fails and the usual `.ok()` turns a real number into a
                -- silent absence.
                (SELECT sum(e.cost_usd)::float8 FROM episodes e
                  WHERE e.agent_id = a.agent_id)             AS cost_usd,
                (SELECT avg(e.cost_usd)::float8 FROM episodes e
                  WHERE e.agent_id = a.agent_id)             AS cost_per_run,
                (SELECT max(e.created_at) FROM episodes e
                  WHERE e.agent_id = a.agent_id)             AS last_run,
                (SELECT count(*) FROM anomaly_events ae
                  WHERE ae.agent_id = a.agent_id
                    AND ae.requires_review
                    AND ae.resolved_at IS NULL)              AS open_flags
           FROM agents a
           LEFT JOIN orchestra_members om
                  ON om.agent_id = a.agent_id AND om.orchestra_name = 'fermi'
           LEFT JOIN agent_evolution ev ON ev.agent_id = a.agent_id
          WHERE a.status = 'published' AND a.visibility = 'public'
          ORDER BY runs DESC",
    )
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("specimens: {e}")))?;

    let specimens: Vec<Value> = rows
        .iter()
        .map(|r| {
            let taxonomy: Option<Value> = r.try_get("taxonomy").ok().flatten();
            let accepts: Vec<String> = r.try_get("accepts").unwrap_or_default();
            let produces: Vec<String> = r.try_get("produces").unwrap_or_default();
            let last_run: Option<chrono::DateTime<chrono::Utc>> =
                r.try_get("last_run").ok().flatten();

            json!({
                // identity
                "agent_name": r.get::<String, _>("agent_name"),
                "label": r.get::<String, _>("label"),
                "description": r.try_get::<Option<String>, _>("description").ok().flatten(),
                "provider": r.try_get::<Option<String>, _>("llm_provider").ok().flatten(),
                "model": r.try_get::<Option<String>, _>("model").ok().flatten(),
                "tags": r.try_get::<Option<Vec<String>>, _>("tags").ok().flatten()
                          .unwrap_or_default(),
                // cost corner
                "tier": r.try_get::<Option<String>, _>("tier").ok().flatten(),
                "min_tier": r.try_get::<Option<String>, _>("min_tier").ok().flatten(),
                // type line
                "agent_type": r.try_get::<Option<String>, _>("agent_type").ok().flatten(),
                "genus": taxonomy.as_ref().and_then(|t| t.get("genus")).cloned(),
                "species": taxonomy.as_ref().and_then(|t| t.get("species")).cloned(),
                "taxonomy": taxonomy,
                // studs
                "accepts": accepts,
                "produces": produces,
                // A stud is filled only when a schema resolves. `has_contract`
                // without `has_schema` is the common case and renders hollow:
                // the block names a schema rather than containing one, and a
                // name is a contract only once something resolves it.
                "typed": r.try_get::<Option<bool>, _>("has_schema").ok().flatten().unwrap_or(false),
                "declares_contract": r.try_get::<Option<bool>, _>("has_contract").ok().flatten()
                                       .unwrap_or(false),
                // evidence
                "level": r.try_get::<Option<i32>, _>("level").ok().flatten(),
                "peak_level": r.try_get::<Option<i32>, _>("peak_level").ok().flatten(),
                "runs": r.try_get::<Option<i64>, _>("runs").ok().flatten().unwrap_or(0),
                "cost_usd": r.try_get::<Option<f64>, _>("cost_usd").ok().flatten(),
                "cost_per_run": r.try_get::<Option<f64>, _>("cost_per_run").ok().flatten(),
                "last_run_days": last_run.map(|t| {
                    (chrono::Utc::now() - t).num_seconds() as f64 / 86400.0
                }),
                "open_flags": r.try_get::<Option<i64>, _>("open_flags").ok().flatten().unwrap_or(0),
                // provenance mark
                "provenance": r.try_get::<Option<String>, _>("provenance").ok().flatten(),
                "fork_count": r.try_get::<Option<i32>, _>("fork_count").ok().flatten().unwrap_or(0),
                "forked_from": r.try_get::<Option<String>, _>("forked_from").ok().flatten(),
            })
        })
        .collect();

    // ── Seam health ──────────────────────────────────────────────────────
    //
    // The Population lens's headline. Measured, not quoted: a census in a
    // comment is not a contract, and this number has to move when the
    // vocabulary converges.
    let seams = sqlx::query(
        "WITH acc  AS (SELECT DISTINCT unnest(accepts)  AS lbl FROM agents WHERE status='published'),
              prod AS (SELECT DISTINCT unnest(produces) AS lbl FROM agents WHERE status='published'),
              allx AS (SELECT lbl FROM acc UNION SELECT lbl FROM prod)
         SELECT (SELECT count(*) FROM allx)::bigint AS distinct_labels,
                (SELECT count(*) FROM (SELECT lbl FROM acc INTERSECT SELECT lbl FROM prod) s)::bigint
                  AS seam_forming,
                (SELECT count(*) FROM agents
                  WHERE status='published' AND output_contract IS NOT NULL)::bigint
                  AS declares_contract,
                (SELECT count(*) FROM agents
                  WHERE status='published' AND output_contract -> 'schema' IS NOT NULL)::bigint
                  AS carries_schema",
    )
    .fetch_one(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("seams: {e}")))?;

    let distinct_labels: i64 = seams.try_get("distinct_labels").unwrap_or(0);
    let seam_forming: i64 = seams.try_get("seam_forming").unwrap_or(0);

    // ── Census ───────────────────────────────────────────────────────────
    let described = specimens
        .iter()
        .filter(|s| !s["taxonomy"].is_null())
        .count();

    fn tally(specimens: &[Value], key: &str) -> Value {
        let mut m: std::collections::BTreeMap<String, usize> = Default::default();
        for s in specimens {
            let k = s[key].as_str().unwrap_or("unclassified").to_string();
            *m.entry(k).or_default() += 1;
        }
        json!(m)
    }

    Ok(Json(json!({
        "specimens": specimens,
        "census": {
            "total": specimens.len(),
            "described": described,
            // Named rather than derived on the client, because "undescribed" is
            // a real state with a real rendering (`Incertae sedis`) and not the
            // absence of a number.
            "undescribed": specimens.len() - described,
            "by_niche": tally(&specimens, "agent_type"),
            "by_stratum": tally(&specimens, "tier"),
            "by_provider": tally(&specimens, "provider"),
            "by_provenance": tally(&specimens, "provenance"),
        },
        "seams": {
            "distinct_labels": distinct_labels,
            "seam_forming": seam_forming,
            "orphans": distinct_labels - seam_forming,
            "declares_contract": seams.try_get::<i64, _>("declares_contract").unwrap_or(0),
            "carries_schema": seams.try_get::<i64, _>("carries_schema").unwrap_or(0),
        },
    })))
}
