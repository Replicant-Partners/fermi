//! One specimen, three tabs.
//!
//! # What this replaces
//!
//! `templates/agent_detail.html` is 6,426 lines and eight tabs — Overview,
//! Activity, Knowledge, Economics, Field Notes, plus owner-only Eval,
//! Intelligence and Manage. An inventory of it found **thirteen metrics
//! rendered in more than one place**, several under different names for the
//! same number:
//!
//! | number | called |
//! |---|---|
//! | executions | "Runs" on the card, "Total" in Performance Statistics |
//! | ontology relationships | "facts" in the cognition panel, "Relationships" in Knowledge |
//! | tools | an identical name→description grid in Overview *and* Field Notes |
//! | Brier | twice on the same tab, from two endpoints |
//!
//! Field Notes is ~100% duplication of the header and Overview.
//!
//! # The duplication was caused by the fetching
//!
//! Those pages compose from a dozen endpoints, so the same quantity arrives
//! under whatever name its producer chose, and a reader cannot tell whether two
//! numbers that disagree are two measurements or one measurement twice. Worse,
//! Performance Statistics mixes **measured** `execution_stats` with
//! **hand-authored** `agent_card.json` values and renders both as `0.0%`, so a
//! measured zero and an absent source are indistinguishable.
//!
//! So this composes one payload server-side. **One producer per number means
//! one name per number**, and where a value cannot be measured it is absent
//! rather than zero.
//!
//! # Three tabs
//!
//! | tab | question | was |
//! |---|---|---|
//! | Profile | what is it? | Overview + Field Notes + the static half of Knowledge |
//! | Record | what has it done? | Activity + Economics + eval history + ontology counts |
//! | Health | is it working? | a link to the Observatory, and now the scoped absence readings |
//!
//! Editing is a **mode**, not a tab: Manage, Intelligence and eval authoring
//! belong in a drawer, because they are a different activity from reading.
//!
//! # Health is where the substrate surfaces
//!
//! [`crate::panel_absence::resolve_for_subject`] has existed since the scoped
//! probes landed and has had no UI. This is it: for this agent, what can the
//! platform say, and what can it not. An empty panel here carries the contract
//! that produced the emptiness and the opportunity count behind it.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

/// `GET /api/episodes/recent`
///
/// Artifacts you can open a trace on. Exists so the loop surface can lead with
/// something concrete rather than with a census — a loop is a path an artifact
/// takes, and until there is an artifact to point at, the path is a diagram.
///
/// It grades nothing. Whether a trace has content is decided by whether the
/// agent declares a field contract, which is a membership test against
/// `grounding_trust::FIELD_CONTRACTS`; the grading itself belongs to the trace
/// endpoint and is not repeated here.
pub async fn recent_episodes_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    let contracted: std::collections::HashSet<&str> = fermi::grounding_trust::FIELD_CONTRACTS
        .iter()
        .map(|c| c.agent_id)
        .collect();
    let mut names: Vec<&str> = contracted.iter().copied().collect();
    names.sort_unstable();
    let owned: Vec<String> = names.iter().map(|s| s.to_string()).collect();

    // Two lists, because they answer different questions, and merging them
    // would answer the second one dishonestly. `recent` is unsorted by
    // contract: it is what actually ran, and most of it is ungraded, which is
    // the true state of the platform. Sorting contracted rows to the top of a
    // single list made every visible artifact graded and quietly implied that
    // graded is the norm. `graded` is the separate short list, so there is
    // always an artifact with a populated belt to open even when none is
    // recent.
    let sql = "SELECT e.episode_id, a.agent_name, e.created_at, e.query,
                      (a.agent_name = ANY($1)) AS contracted
                 FROM episodes e
                 JOIN agents a ON a.agent_id = e.agent_id
                WHERE e.response_text IS NOT NULL
                  AND a.agent_name NOT LIKE 'test\\_agent\\_%'";

    let recent_rows = sqlx::query(&format!("{sql} ORDER BY e.created_at DESC LIMIT 18"))
        .bind(&owned)
        .fetch_all(db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("recent: {e}")))?;

    let graded_rows = sqlx::query(&format!(
        "{sql} AND a.agent_name = ANY($1) ORDER BY e.created_at DESC LIMIT 6"
    ))
    .bind(&owned)
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("graded: {e}")))?;

    let shape = |r: &sqlx::postgres::PgRow| {
        json!({
            "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
            "agent": r.get::<String, _>("agent_name"),
            "at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok().map(|t| t.to_rfc3339()),
            "query": r.try_get::<Option<String>, _>("query").ok().flatten(),
            "contracted": r.try_get::<Option<bool>, _>("contracted").ok().flatten()
                            .unwrap_or(false),
        })
    };

    let episodes: Vec<Value> = recent_rows.iter().map(shape).collect();
    let graded: Vec<Value> = graded_rows.iter().map(shape).collect();
    let graded_in_recent = episodes
        .iter()
        .filter(|e| e["contracted"].as_bool().unwrap_or(false))
        .count();

    Ok(Json(json!({
        "episodes": episodes,
        "graded": graded,
        "graded_in_recent": graded_in_recent,
        "contracted_agents": names,
        "note": "`contracted` means the agent declares a field contract, so its \
                 trace has graded checkpoints. The rest have a belt with nothing \
                 to grade — which is the default, is the majority, and is not an \
                 error. `episodes` is what actually ran, in order, ungraded rows \
                 included; `graded` is a separate short list so there is always \
                 one with a populated belt to open.",
    })))
}

/// `GET /api/episodes/:episode_id/lineage`
///
/// Who called this pulse, who it called, and where its output was consumed.
///
/// The trace could name its parent and nothing else, so the collaboration half
/// of an artifact's journey was a paragraph asserting the data did not exist.
/// It does: `parent_episode_id` is queryable in both directions, and since
/// migration 222 a workspace message carries the episode it delivered.
///
/// Agents hire agents — `weather_oracle` calls `weather_ensemble_forecaster` and
/// `weather_calibrator`, one pulse fanning out to two — and a trace that stops at
/// the parent cannot show it.
pub async fn episode_lineage_handler(
    State(state): State<AppState>,
    Path(episode_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // LEFT JOIN, not JOIN, and the difference is a finding.
    //
    // Measured: 12 episodes name a parent and **6 of them name one that does
    // not exist**. An inner join reports those as "not delegated", which is the
    // opposite of the truth - they were delegated and the parent's row never
    // landed. Migration 220 predicted exactly this race for `gate_decisions`:
    // the child is written from inside the tool loop while the parent persists
    // later, so an id can be minted, handed down, and never followed by a row.
    let parent_row = sqlx::query(
        "SELECT e.parent_episode_id, pe.episode_id AS resolved,
                pa.agent_name, pe.created_at
           FROM episodes e
           LEFT JOIN episodes pe ON pe.episode_id = e.parent_episode_id
           LEFT JOIN agents  pa ON pa.agent_id   = pe.agent_id
          WHERE e.episode_id = $1",
    )
    .bind(episode_id)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("parent: {e}")))?;

    let parent = parent_row.as_ref().and_then(|r| {
        let named: Option<uuid::Uuid> = r.try_get("parent_episode_id").ok().flatten();
        let resolved: Option<uuid::Uuid> = r.try_get("resolved").ok().flatten();
        named.map(|id| match resolved {
            Some(_) => json!({
                "state": "resolved",
                "episode_id": id,
                "agent": r.try_get::<Option<String>, _>("agent_name").ok().flatten(),
                "at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("created_at")
                        .ok().flatten().map(|t| t.to_rfc3339()),
            }),
            // Delegated by something whose episode was never written. Not "no
            // parent" - a broken chain, and the only place it is visible.
            None => json!({
                "state": "dangling",
                "episode_id": id,
                "because": "This pulse names a parent whose episode row was never \
                            written. The id is minted inside the tool loop and handed \
                            to the child before the parent persists, so a parent run \
                            that failed leaves the reference pointing at nothing.",
            }),
        })
    });

    // The direction the trace never looked. An agent that hired another is only
    // visible from here.
    let child_rows = sqlx::query(
        "SELECT e.episode_id, a.agent_name, e.created_at, e.query, e.execution_status
           FROM episodes e
           JOIN agents a ON a.agent_id = e.agent_id
          WHERE e.parent_episode_id = $1
          ORDER BY e.created_at ASC",
    )
    .bind(episode_id)
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("children: {e}")))?;

    let children: Vec<Value> = child_rows
        .iter()
        .map(|r| {
            json!({
                "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
                "agent": r.try_get::<String, _>("agent_name").unwrap_or_default(),
                "at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                        .ok().map(|t| t.to_rfc3339()),
                "query": r.try_get::<Option<String>, _>("query").ok().flatten(),
                "status": r.try_get::<Option<String>, _>("execution_status").ok().flatten(),
            })
        })
        .collect();

    // Where this pulse was delivered to a team. Possible since migration 222;
    // absent on anything that ran before it, which is stated rather than
    // rendered as "nobody read it".
    let use_rows = sqlx::query(
        "SELECT m.message_id, m.workspace_id, m.message_type, m.created_at, t.name
           FROM workspace_messages m
           LEFT JOIN teams t ON t.id = m.workspace_id
          WHERE m.episode_id = $1
          ORDER BY m.created_at ASC",
    )
    .bind(episode_id)
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("uses: {e}")))?;

    let delivered: Vec<Value> = use_rows
        .iter()
        .map(|r| {
            json!({
                "workspace_id": r.try_get::<Option<uuid::Uuid>, _>("workspace_id").ok().flatten(),
                "workspace": r.try_get::<Option<String>, _>("name").ok().flatten(),
                "message_type": r.try_get::<Option<String>, _>("message_type").ok().flatten(),
                "at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                        .ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "parent": parent,
        "children": children,
        "delivered": delivered,
        "contract": "`parent.state` is `resolved` or `dangling`, and the second is a \
                     finding: the pulse WAS delegated and the parent's episode row was \
                     never written. 6 of the 12 delegation edges on the platform are in \
                     that state, and an inner join reports them as \"not delegated\", \
                     which is the opposite of the truth. `parent: null` means genuinely \
                     not delegated. \
                     `children` is who this pulse hired, read from \
                     `episodes.parent_episode_id` in the direction the trace never \
                     looked. An empty `children` means this pulse delegated to nobody - \
                     that is a real answer, not a missing one. `delivered` is where the \
                     output reached a team, and it is empty for anything that ran before \
                     migration 222 added the join: absent because it predates the column, \
                     NOT because nobody read it. Those are different and must not render \
                     alike.",
    })))
}

/// One pulse, as every surface that lists pulses reads it.
///
/// The stream and a specimen's Record tab list the same object and listed it two
/// different ways: the stream gave you the hop (who addressed whom, with a glyph
/// for each), whether grounding graded it, and whether any checkpoint recorded a
/// decision; the specimen gave you a date, a truncated query and a cost. Same
/// rows, one of them stripped of everything that makes a pulse legible.
///
/// So the projection lives here once and both handlers select it. The `WHERE`
/// clause is the caller's — that is the only thing that actually differs between
/// "every pulse" and "this agent's pulses".
const PULSE_SELECT: &str = "SELECT e.episode_id, e.created_at, e.query, e.execution_status,
                e.cost_usd::float8 AS cost_usd, e.user_id, e.parent_episode_id,
                e.error_details,
                a.agent_name,
                pa.agent_name AS parent_agent,
                u.name AS user_name, u.email AS user_email,
                ('grounding:enforced'   = ANY(e.tags)) AS clean,
                ('grounding:violations' = ANY(e.tags)) AS dirty,
                (SELECT count(*) FROM gate_decisions gd
                  WHERE gd.episode_id = e.episode_id)  AS decisions
           FROM episodes e
           JOIN agents a  ON a.agent_id = e.agent_id
           LEFT JOIN episodes pe ON pe.episode_id = e.parent_episode_id
           LEFT JOIN agents  pa ON pa.agent_id   = pe.agent_id
           -- A person is a name, not a uuid prefix. The stream showed eight
           -- characters of a hash, which is unreadable and makes the human
           -- indistinguishable from any other human.
           LEFT JOIN users u ON u.id::text = e.user_id";

/// Turn a [`PULSE_SELECT`] row into the pulse object both surfaces render.
///
/// `parent_episode_id` names an agent addresser; `user_id` names a human.
/// Neither means the invoker was not recorded — 514 of 3,651 — and that is
/// reported as `unattributed` rather than guessed at.
fn pulse_row(r: &sqlx::postgres::PgRow) -> Value {
    let parent_agent: Option<String> = r.try_get("parent_agent").ok().flatten();
    let user_id: Option<String> = r.try_get("user_id").ok().flatten();
    let user_name: Option<String> = r.try_get("user_name").ok().flatten();
    let user_email: Option<String> = r.try_get("user_email").ok().flatten();
    // Ordered: a delegating agent is the addresser even when a human started the
    // chain, because the hop this row records is the agent-to-agent one.
    let (kind, who) = match (&parent_agent, &user_id) {
        (Some(a), _) => ("agent", a.clone()),
        (None, Some(u)) => (
            "human",
            // Name, then the local part of the email, and only then a short id.
            // An unresolvable id still says something, but it is the last resort
            // rather than the default.
            user_name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| {
                    user_email
                        .as_deref()
                        .and_then(|e| e.split('@').next())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| u.chars().take(8).collect()),
        ),
        _ => ("unattributed", String::new()),
    };
    let clean: bool = r.try_get("clean").unwrap_or(false);
    let dirty: bool = r.try_get("dirty").unwrap_or(false);
    let decisions: i64 = r.try_get("decisions").unwrap_or(0);
    json!({
        "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
        "at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .ok().map(|t| t.to_rfc3339()),
        "from": { "kind": kind, "name": who },
        "to":   { "kind": "agent", "name": r.get::<String, _>("agent_name") },
        "query": r.try_get::<Option<String>, _>("query").ok().flatten(),
        "status": r.try_get::<Option<String>, _>("execution_status").ok().flatten(),
        "error": r.try_get::<Option<String>, _>("error_details").ok().flatten(),
        "cost_usd": r.try_get::<Option<f64>, _>("cost_usd").ok().flatten(),
        // Three states, never two: graded clean, graded and violating, or not
        // graded at all - which is not a pass.
        "grounding": if clean { "clean" } else if dirty { "violations" } else { "ungraded" },
        "recorded": decisions > 0,
    })
}

/// `GET /api/stream`
///
/// Every exchange, newest first, across every agent.
///
/// The object is a **pulse with its addresser made explicit**. That subsumes
/// workspace hops, direct API calls, delegations and scheduled runs as one list,
/// and it needs no new data: who invoked a pulse is already derivable.
///
/// An earlier version of this was bound to a workspace, because that is where
/// the typed hops live in `workspace_messages`. That was the wrong scope - the
/// workspace already shows its own flow, and what is missing is the aggregated
/// stream across everything. `workspace_messages` becomes an enrichment rather
/// than the source.

pub async fn stream_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    let rows = sqlx::query(&format!(
        "{PULSE_SELECT}
          WHERE a.agent_name NOT LIKE 'test\\_agent\\_%'
          ORDER BY e.created_at DESC
          LIMIT 200"
    ))
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("stream: {e}")))?;

    let exchanges: Vec<Value> = rows.iter().map(pulse_row).collect();

    Ok(Json(json!({
        "exchanges": exchanges,
        "contract": "One row is one exchange: a pulse with its addresser named. `from.kind` \
                     is `agent` when the pulse was delegated, `human` when a person or \
                     script invoked it, and `unattributed` when neither was recorded - which \
                     is 514 of 3,651 and is a gap, not a system actor. `grounding: ungraded` \
                     means no contract applied or no path enforced one; it is NOT a pass. \
                     There is no workspace filter because `episodes` carries no workspace \
                     column.",
    })))
}

/// `GET /api/bestiary/cards`
///
/// The stat line for every specimen, in one read.
///
/// A card carries a small fixed set of managed quantities rather than prose,
/// because a number against a threshold can be managed and a paragraph cannot.
/// Four positions, and each is a door into the surface that owns its detail.
///
/// Kept separate from `/api/bestiary` deliberately: that handler's query is
/// large and actively edited, and these are aggregates over different tables.
pub async fn bestiary_cards_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // Who declares a field contract at all. Needed to tell "nobody typed this
    // agent" from "this agent is typed and nothing graded it".
    let contracted: std::collections::HashSet<&str> = fermi::grounding_trust::FIELD_CONTRACTS
        .iter()
        .map(|c| c.agent_id)
        .collect();

    // PULSE, and the economics. One grouped pass over the episode log rather
    // than a query per card.
    //
    // FIDELITY comes from the grounding tags, and they already encode the
    // distinction the card needs: `stamp_grounding` deliberately writes NOTHING
    // for an agent with no contract, because "an agent with no contract has not
    // been found compliant, and marking it so would be the original defect".
    // So `graded = 0` means *not declared*, which is authoring work, and is not
    // a score of zero. Given 98 of 206 producing agents are undeclared, getting
    // that backwards would paint the whole bestiary red.
    let rows = sqlx::query(
        "SELECT a.agent_name,
                count(*)                                             AS pulses,
                max(e.created_at)                                    AS last_at,
                sum(e.cost_usd)::float8                              AS cost_usd,
                count(*) FILTER (WHERE e.execution_status = 'success') AS ok,
                count(*) FILTER (WHERE 'grounding:enforced'   = ANY(e.tags)) AS clean,
                count(*) FILTER (WHERE 'grounding:violations' = ANY(e.tags)) AS dirty
           FROM episodes e
           JOIN agents a ON a.agent_id = e.agent_id
          WHERE a.agent_name NOT LIKE 'test\\_agent\\_%'
          GROUP BY a.agent_name",
    )
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("cards: {e}")))?;

    // LEARNED. The rule count is real; how often a rule is retrieved is not
    // recorded anywhere, so the card shows what it holds and says nothing about
    // use. Claiming a retrieval figure we do not have would be the more
    // expensive error.
    let rule_rows = sqlx::query(
        "SELECT a.agent_name, count(*) AS rules
           FROM semantic_rules r
           JOIN agents a ON a.agent_id = r.agent_id
          GROUP BY a.agent_name",
    )
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("rules: {e}")))?;

    let mut rules: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in &rule_rows {
        if let Ok(n) = r.try_get::<String, _>("agent_name") {
            rules.insert(n, r.try_get::<i64, _>("rules").unwrap_or(0));
        }
    }

    // The sparkline. Daily counts over a fortnight: the shape IS the measured
    // rate, which is the only thing on a card allowed to move.
    let series_rows = sqlx::query(
        "SELECT a.agent_name, (e.created_at AT TIME ZONE 'UTC')::date AS d, count(*) AS n
           FROM episodes e
           JOIN agents a ON a.agent_id = e.agent_id
          WHERE e.created_at > now() - interval '14 days'
            AND a.agent_name NOT LIKE 'test\\_agent\\_%'
          GROUP BY 1, 2",
    )
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("series: {e}")))?;

    let today = chrono::Utc::now().date_naive();
    let mut series: std::collections::HashMap<String, Vec<i64>> = std::collections::HashMap::new();
    for r in &series_rows {
        let Ok(name) = r.try_get::<String, _>("agent_name") else {
            continue;
        };
        let Ok(d) = r.try_get::<chrono::NaiveDate, _>("d") else {
            continue;
        };
        let n = r.try_get::<i64, _>("n").unwrap_or(0);
        let idx = (d - (today - chrono::Duration::days(13))).num_days();
        if (0..14).contains(&idx) {
            series
                .entry(name)
                .or_insert_with(|| vec![0; 14])
                .get_mut(idx as usize)
                .map(|slot| *slot = n);
        }
    }

    let mut cards = serde_json::Map::new();
    for r in &rows {
        let name: String = match r.try_get("agent_name") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let pulses: i64 = r.try_get("pulses").unwrap_or(0);
        let ok: i64 = r.try_get("ok").unwrap_or(0);
        let clean: i64 = r.try_get("clean").unwrap_or(0);
        let dirty: i64 = r.try_get("dirty").unwrap_or(0);
        let graded = clean + dirty;
        let cost: Option<f64> = r.try_get::<Option<f64>, _>("cost_usd").ok().flatten();
        let last_at = r
            .try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_at")
            .ok()
            .flatten()
            .map(|t| t.to_rfc3339());

        cards.insert(
            name.clone(),
            json!({
                "pulses": pulses,
                "last_at": last_at,
                "success_rate": if pulses > 0 { Some(ok as f64 / pulses as f64) } else { None },
                "cost_usd": cost,
                "cost_per_pulse": match (cost, pulses) {
                    (Some(c), p) if p > 0 => Some(c / p as f64),
                    _ => None,
                },
                "rules": rules.get(&name).copied().unwrap_or(0),
                "series": series.get(&name).cloned().unwrap_or_else(|| vec![0; 14]),
                // Three states, not two. Collapsing the middle one into
                // `not_declared` reported `football_analyst` and `prey_locator`
                // as undeclared when both declare a field contract in
                // `grounding_trust::FIELD_CONTRACTS` - and hid the actual
                // finding, which is that only 1 of the 10 contracted agents has
                // a single graded pulse. Whose problem it is differs per state:
                //
                //   not_declared     no contract exists   -> the author's work
                //   declared_ungraded contract exists, no pulse carries a
                //                     grounding tag       -> the PLATFORM's, and
                //                     it means grounding never ran on the route
                //                     those pulses travelled
                //   measured          graded pulses exist -> a real reading
                "fidelity": if graded == 0 {
                    if contracted.contains(name.as_str()) {
                        json!({ "state": "declared_ungraded", "graded": 0 })
                    } else {
                        json!({ "state": "not_declared", "graded": 0 })
                    }
                } else {
                    json!({
                        "state": "measured",
                        "graded": graded,
                        "clean": clean,
                        "violations": dirty,
                        "clean_rate": clean as f64 / graded as f64,
                    })
                },
            }),
        );
    }

    Ok(Json(json!({
        "cards": cards,
        "series_days": 14,
        "contract": "`fidelity.state` has THREE values and they have different owners. \
                     `not_declared`: no field contract exists, so nothing could be graded - \
                     authoring work, and it must NOT render as a score of zero, because most \
                     of the catalogue is undeclared and reading that as failure paints the \
                     whole register red. `declared_ungraded`: the agent DOES declare a \
                     contract and not one pulse carries a grounding tag - that is a platform \
                     finding, and it means grounding never ran on the route those pulses \
                     travelled. `measured`: a real reading. `rules` is what the agent holds; \
                     retrieval is not recorded anywhere, so no retrieval figure is served. \
                     `series` is daily pulse counts over `series_days`, oldest first.",
    })))
}

/// `GET /api/workspaces/:workspace_id/flow`
///
/// A workspace as its **seams**: who called whom, with what task, and whether
/// anything verified the artifact that crossed.
///
/// Exists because the same interaction is recorded twice and joined never. The
/// workflow diagram is generated from `workspace_messages`; the gates, grades
/// and ledger act on `episodes`; and `workspace_messages` carries no
/// `episode_id`. So the platform can say what happened, or whether it was
/// verified, but not both about one hop.
///
/// Every arrow in a workflow diagram is an artifact crossing a seam, and every
/// seam should pass through the gates. This endpoint serves the arrows and
/// reports, per arrow, that nothing joins it to a verdict — which is the honest
/// state and the reason the column is worth one migration.
pub async fn workspace_flow_handler(
    State(state): State<AppState>,
    Path(workspace_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // The typed hops only. `chat` is the human conversation around the work and
    // is deliberately excluded -- it is not a seam an artifact crosses.
    let rows = sqlx::query(
        "SELECT message_id, sender_type, sender_name, message_type, content,
                created_at, episode_id
           FROM workspace_messages
          WHERE workspace_id = $1
            AND message_type IN ('agent_invocation', 'execution_result')
          ORDER BY created_at ASC",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("flow: {e}")))?;

    // Pair each invocation with the next result after it. Order is the only
    // available pairing key, because nothing correlates the two rows -- which is
    // itself part of what this endpoint is reporting.
    let mut seams: Vec<Value> = Vec::new();
    let mut pending: Option<(String, String, String, String, String)> = None;

    for r in &rows {
        let mtype: String = r.try_get("message_type").unwrap_or_default();
        let content: String = r.try_get("content").unwrap_or_default();
        let mid = r
            .try_get::<uuid::Uuid, _>("message_id")
            .map(|v| v.to_string())
            .unwrap_or_default();
        let at = r
            .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();
        let sender = r
            .try_get::<Option<String>, _>("sender_name")
            .ok()
            .flatten()
            .unwrap_or_else(|| "unknown".into());
        let skind: String = r.try_get("sender_type").unwrap_or_default();

        if mtype == "agent_invocation" {
            // `@target {"task": "..."}` -- the callee and the task are in the text.
            let target = content
                .strip_prefix('@')
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("unknown")
                .to_string();
            let task = content
                .find("\"task\"")
                .and_then(|i| content[i..].split('"').nth(3))
                .unwrap_or("")
                .to_string();
            pending = Some((mid, sender, skind, target, task));
            if let Some((ref m, ref s, ref sk, ref t, ref task)) = pending {
                seams.push(json!({
                    "seq": seams.len() + 1,
                    "from": s, "from_kind": sk,
                    "to": t, "to_kind": "agent",
                    "task": if task.is_empty() { Value::Null } else { json!(task) },
                    "at": at,
                    "invocation_message_id": m,
                    "payload_bytes": content.len(),
                    "returned": false,
                    "result_message_id": Value::Null,
                    "result_bytes": Value::Null,
                    // The join that does not exist. Named rather than omitted:
                    // "this hop was not verified" and "this hop's join was never
                    // written" are different facts and must not collapse.
                    "episode_id": Value::Null,
                    "governed": false,
                    "why_ungoverned": "`workspace_messages` carries no `episode_id`, so this \
                                       hop cannot be joined to the episode the gates acted on. \
                                       The artifact was checked; this arrow cannot prove it.",
                }));
            }
        } else if mtype == "execution_result" {
            // The join, written on the result because that is the message that
            // has an episode (migration 222). Absent on rows written before the
            // column existed, which is why its absence is reported with a
            // reason rather than as a bare `false`.
            let eid = r
                .try_get::<Option<uuid::Uuid>, _>("episode_id")
                .ok()
                .flatten();
            if let Some(last) = seams.last_mut() {
                if last["returned"] == json!(false) {
                    last["returned"] = json!(true);
                    last["result_message_id"] = json!(mid);
                    last["result_bytes"] = json!(content.len());
                    match eid {
                        Some(e) => {
                            last["episode_id"] = json!(e);
                            last["governed"] = json!(true);
                            last["why_ungoverned"] = Value::Null;
                        }
                        None => {
                            last["why_ungoverned"] = json!(
                                "This hop predates `workspace_messages.episode_id` \
                                 (migration 222), so it cannot be joined to the episode the \
                                 gates acted on. Nothing is backfilled: guessing which \
                                 episode a historical arrow carried would make the join look \
                                 answered. Hops from here on carry it."
                            );
                        }
                    }
                }
            }
            pending = None;
        }
    }

    let returned = seams
        .iter()
        .filter(|s| s["returned"] == json!(true))
        .count();
    let governed = seams
        .iter()
        .filter(|s| s["governed"] == json!(true))
        .count();
    let participants: std::collections::BTreeSet<String> = seams
        .iter()
        .flat_map(|s| {
            [
                s["from"].as_str().unwrap_or("").to_string(),
                s["to"].as_str().unwrap_or("").to_string(),
            ]
        })
        .filter(|p| !p.is_empty())
        .collect();

    // Chat volume, for scale only. A workspace whose seams are a rounding error
    // beside its conversation is coordinating by prose, and that is worth seeing.
    let chat: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM workspace_messages
          WHERE workspace_id = $1 AND message_type = 'chat'",
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await
    .unwrap_or(-1);

    Ok(Json(json!({
        "workspace_id": workspace_id,
        "participants": participants,
        "seams": seams,
        "tally": {
            "seams": seams.len(),
            "returned": returned,
            "unreturned": seams.len().saturating_sub(returned),
            "governed": governed,
            "chat_messages": chat,
        },
        "contract": "Every arrow is an artifact crossing a seam, and every seam should pass \
                     through the gates. A hop with `governed: false` is NOT a hop whose \
                     artifact went unchecked - it is a hop that predates \
                     `workspace_messages.episode_id` and so cannot be joined to the episode \
                     the gates acted on. Read `why_ungoverned`, and never render it as a \
                     failure of the agents. `chat_messages` is context: a workspace with many \
                     chats and few seams is coordinating by prose, and prose is not a seam.",
    })))
}

/// `GET /api/specimen/:agent_name`
pub async fn specimen_handler(
    State(state): State<AppState>,
    Path(agent_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // ── Profile: what it is ──────────────────────────────────────────────
    let row = sqlx::query(
        "SELECT a.agent_id, a.agent_name,
                COALESCE(a.display_alias, a.agent_name) AS label,
                a.description, a.agent_type, a.tier, a.min_tier,
                a.llm_provider, a.model, a.executor_type, a.temperature,
                -- The ladder decides which model actually runs. `model` and
                -- `llm_provider` are only the fallback used when no rung matches
                -- the caller's tier (see AgentCard::apply_tier_resolution), and a
                -- surface presenting them as THE model is wrong for every agent
                -- that has a ladder.
                a.model_ladder, a.valence, a.system_prompt,
                -- Competition declaration (migration 228) — domain tags, price,
                -- support tier. The shelf edits these; fidelity and selection rate
                -- are platform-computed from gate history and select_agent_decisions.
                a.competition,
                -- Typed interface schema IDs (migrations 117/227) — used by the
                -- shelf to render schema-ID chips differently from bare labels.
                a.input_contract->>'accepts_schema'  AS input_schema_id,
                a.output_contract->>'produces_schema' AS output_schema_id,
                -- The contract itself, not only the booleans derived from it.
                -- `produces_schema` is what the prompt has to name, and reading
                -- it through `try_get` on an unselected column returns None
                -- silently, which would have answered NO for every agent on
                -- the question of whether its prompt names its own type.
                a.output_contract,
                a.status, a.visibility, a.tags, a.accepts, a.produces,
                a.taxonomy, a.fork_count, a.forked_from, a.persona_version,
                a.system_prompt, a.sample_queries, a.mcp_tools,
                a.dreaming_budget_credits, a.dreaming_credits_used,
                (a.output_contract IS NOT NULL)             AS declares_contract,
                (a.output_contract -> 'schema' IS NOT NULL) AS typed,
                om.source        AS provenance,
                ev.current_level AS level,
                ev.peak_level    AS peak_level
           FROM agents a
           LEFT JOIN orchestra_members om
                  ON om.agent_id = a.agent_id AND om.orchestra_name = 'fermi'
           LEFT JOIN agent_evolution ev ON ev.agent_id = a.agent_id
          WHERE a.agent_name = $1",
    )
    .bind(&agent_name)
    .fetch_optional(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("specimen: {e}")))?
    .ok_or((StatusCode::NOT_FOUND, format!("no specimen `{agent_name}`")))?;

    let agent_id: uuid::Uuid = row.get("agent_id");

    // ── Record: what it has done ─────────────────────────────────────────
    //
    // Every count from one place, so no two names can disagree. `episodes` is
    // the measured source; the `agents.total_executions` rollup is deliberately
    // NOT read — `rollup_trust` exists because that column was added with the
    // table and never wired, and 3 of 743 rows carry a non-zero value while the
    // episode log recorded every run faithfully.
    let rec = sqlx::query(
        "SELECT (SELECT count(*) FROM episodes WHERE agent_id = $1)                       AS runs,
                (SELECT count(*) FROM episodes
                  WHERE agent_id = $1 AND execution_status = 'success')                   AS succeeded,
                (SELECT count(*) FROM episodes
                  WHERE agent_id = $1 AND execution_status <> 'success')                  AS failed,
                (SELECT sum(cost_usd)::float8 FROM episodes WHERE agent_id = $1) AS cost_usd,
                (SELECT max(created_at) FROM episodes WHERE agent_id = $1)                AS last_run,
                (SELECT count(*) FROM entities WHERE agent_id = $1)                       AS entities,
                (SELECT count(*) FROM facts WHERE agent_id = $1)                          AS facts,
                (SELECT count(*) FROM semantic_rules WHERE agent_id = $1)                 AS rules,
                (SELECT count(*) FROM semantic_rules
                  WHERE agent_id = $1 AND application_count > 0)                          AS rules_retrieved,
                (SELECT count(*) FROM consolidation_jobs
                  WHERE agent_id = $1 AND status = 'completed')                           AS dream_cycles,
                (SELECT max(completed_at) FROM consolidation_jobs
                  WHERE agent_id = $1 AND status = 'completed')                           AS last_dreamt,
                (SELECT count(*) FROM eval_runs WHERE agent_id = $1)                      AS eval_runs,
                (SELECT max(started_at) FROM eval_runs WHERE agent_id = $1)               AS last_eval",
    )
    .bind(agent_id)
    .fetch_one(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("record: {e}")))?;

    let runs: i64 = rec.try_get("runs").unwrap_or(0);
    let cost: Option<f64> = rec.try_get("cost_usd").ok().flatten();

    // ── Recent episodes ──────────────────────────────────────────────────
    // The same projection the stream reads, filtered to this agent.
    //
    // It was its own five-column select, so the Record tab could show a date, a
    // query and a cost and nothing else — no addresser, no grounding state, no
    // "did any checkpoint record a decision". The stream has shown all three for
    // months. Same object, two renderings, and the stripped one was on the page
    // an agent's owner actually opens.
    let episodes = sqlx::query(&format!(
        "{PULSE_SELECT}
          WHERE e.agent_id = $1
          ORDER BY e.created_at DESC LIMIT 15"
    ))
    .bind(agent_id)
    .fetch_all(db)
    .await;

    // A failed read must not render as an empty list.
    //
    // This was `.unwrap_or_default()`, and it hid a real defect for a whole
    // deploy: the projection named `e.error_message` and the column is
    // `error_details`, so the query failed on every request and the Record tab
    // said **"No pulse recorded"** for an agent with 218 of them. The stream,
    // which propagates its error, went 502 and was noticed immediately; this one
    // lied quietly, which is the worse of the two behaviours and the exact rule
    // this codebase keeps re-learning: absent must look different from broken.
    //
    // Not fatal to the page, though — profile and record are worth serving
    // without the list. So the error travels as a value and the surface can say
    // which of the two it met.
    let (episodes, episodes_error) = match episodes {
        Ok(rows) => (rows, None),
        Err(e) => {
            tracing::error!("specimen pulse list failed for {agent_name}: {e}");
            (Vec::new(), Some(e.to_string()))
        }
    };

    // ── Health: what the platform can and cannot say about THIS agent ────
    // `collect`, not a hand-built literal. This site reassembled the snapshot
    // field by field and was byte-for-byte identical to `Observation::collect`;
    // adding `declarations` is what exposed it, because the copy silently
    // omitted the new field and every declaration-resolved panel here would have
    // reported `no_census` while the endpoint looked fine.
    let observation = fermi::native_evaluators::Observation::collect(db).await;

    let mut health = Vec::new();
    for p in fermi::panel_absence::PANELS {
        if p.scope != fermi::panel_absence::Scope::Agent {
            continue;
        }
        let a = fermi::panel_absence::resolve_for_subject(db, p, agent_id, &observation).await;
        let s = fermi::panel_contract::stamp_absence(p, &a, fermi::panel_contract::Density::Scan);
        health.push(json!({
            "panel": p.id,
            "shows": p.shows,
            "reading": s.reading,
            "marker": s.marker,
            "marker_word": s.marker_word,
            "token": s.token,
            "detail": a.detail,
            "answered_by": a.answered_by,
            "remediation": a.remediation,
        }));
    }

    // ── The declaration ladder, for this agent ───────────────────────────
    //
    // Served per agent rather than fetched from `/api/declarations`, which is a
    // fleet read. A page about one agent computing a fleet-wide anything is the
    // defect that made this endpoint take 46 seconds, and it is not going to be
    // reintroduced for a config panel.
    //
    // The rungs are the answer to "why can nothing say anything about this
    // agent's pulses" — each carries what it unlocks and what reads `unknown`
    // without it — so this is the one config group the platform can rank, and
    // the recommended next action is the first absent rung.
    let rungs_row = sqlx::query(
        "SELECT COALESCE(array_length(a.accepts, 1), 0) > 0
                  AND COALESCE(array_length(a.produces, 1), 0) > 0        AS ports,
                a.output_contract ? 'produces_schema'                     AS output_type,
                jsonb_typeof(a.output_contract -> 'schema') = 'object'    AS output_schema,
                jsonb_typeof(a.output_contract -> 'grounding') = 'object'
                  AND (a.output_contract -> 'grounding') != '{}'          AS field_contract
           FROM agents a WHERE a.agent_id = $1",
    )
    .bind(agent_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let present: Vec<&'static str> = fermi::declaration_ladder::LADDER
        .iter()
        .filter(|d| {
            // `field_contract` has two paths and the SQL covers only one: the
            // card's `output_contract.grounding`. The Rust const table is the
            // other, and an agent declared there is declared.
            if d.rung == "field_contract"
                && fermi::declaration_ladder::has_field_contract(&agent_name)
            {
                return true;
            }
            rungs_row
                .as_ref()
                .and_then(|r| r.try_get::<Option<bool>, _>(d.rung).ok().flatten())
                .unwrap_or(false)
        })
        .map(|d| d.rung)
        .collect();

    let ladder: Vec<Value> = fermi::declaration_ladder::LADDER
        .iter()
        .map(|d| {
            json!({
                "rung": d.rung,
                "declares": d.declares,
                "owner": d.owner,
                "unlocks": d.unlocks,
                "without_it": d.without_it,
                "present": present.contains(&d.rung),
            })
        })
        .collect();

    // ── The contract's fields, in the three states a compile has ─────────
    //
    // The ladder says whether a contract EXISTS. This says whether the one that
    // exists resolves, and it is the distinction that decides whether an agent
    // can ever read as finished.
    //
    //   resolved  a tool is named and the platform can dispatch it
    //   error     a tool is named and no such tool exists — the author's, now
    //   pending   no tool exists for this field YET, by declaration. Nobody's
    //             fault, and NOT a lesser `resolved`
    //
    // `pending` is the load-bearing one. `Grounding::Unsourced` has meant "no
    // tool exists, so the field must be null, and a value here is the violation"
    // since migration 200 — `ratings.elo_current` even carries the note "returns
    // when a ClubElo or equivalent tool is added". Surfaces have been collapsing
    // it into failure, which makes the only route to a healthy agent deleting
    // its ambition: `genome_profiler` declares seven such fields and is the
    // best-declared agent on the platform.
    //
    // **Green means zero errors, not zero pending.**
    //
    // The half of `error` that needs a tool's declared RESPONSE — does the named
    // tool actually return this field — is not checkable yet and is not guessed
    // at. See docs/plans/AGENT_COMPILE_AND_TOOL_REGISTRY.md.
    let dispatchable: std::collections::HashSet<&'static str> =
        fermi::agent_backend::tools::dispatchable_tool_names()
            .into_iter()
            .collect();

    let mut counts = std::collections::BTreeMap::<&'static str, usize>::new();
    let contract_fields: Vec<Value> = fermi::grounding_trust::contracts_for(&agent_name)
        .map(|c| {
            let (state, tool, why) = match c.grounding {
                fermi::grounding_trust::Grounding::Sourced { tool, .. } => {
                    if dispatchable.contains(tool) {
                        ("resolved", Some(tool), "a tool is named and the platform can run it")
                    } else {
                        (
                            "error",
                            Some(tool),
                            "the contract names a tool the platform cannot dispatch, so                              nothing can ever settle this field",
                        )
                    }
                }
                fermi::grounding_trust::Grounding::Unsourced => (
                    "pending",
                    None,
                    "no tool exists for this yet. The field must be null and a value                      here is the violation — this is a standing request for an                      integration, not a defect",
                ),
                fermi::grounding_trust::Grounding::Derived { .. } => (
                    "derived",
                    None,
                    "computed by the platform from other fields, so it is reproducible                      by construction",
                ),
                fermi::grounding_trust::Grounding::Inferred { .. } => (
                    "inferred",
                    None,
                    "a judgement the agent is commissioned to make. An endorsement is                      the terminal verdict, not a weak citation",
                ),
                fermi::grounding_trust::Grounding::Narrative => {
                    ("narrative", None, "prose. There is no proposition to settle")
                }
            };
            *counts.entry(state).or_default() += 1;
            json!({ "path": c.path, "state": state, "tool": tool, "why": why })
        })
        .collect();

    // ── The prompt, checked against the contract ─────────────────────────
    //
    // The first thing an author needs and the last thing any surface showed. The
    // system prompt is not documentation: `prompt_demands_structured_output`
    // substring-matches it and, when it matches, `ToolAwareExecutor::execute`
    // returns `self.inner.execute(..)` — **the agent gets no tool loop**.
    //
    // So a prompt saying "output valid JSON only" and a contract with `Sourced`
    // fields are a contradiction the platform can detect and has never
    // displayed. Three typed agents are in that state today: genome_profiler,
    // supply_chain_oracle and video_analyst.
    //
    // The trigger phrase travels, not just the boolean. "Your prompt removes the
    // tool loop" is a verdict; naming the phrase is something an author can act
    // on. Computed from the one Rust implementation rather than re-matched on a
    // client, because two copies of a decision that changes execution is the
    // drift this repo keeps finding.
    let system_prompt: Option<String> = row.try_get("system_prompt").ok().flatten();
    let trigger = system_prompt
        .as_deref()
        .and_then(fermi::agent_backend::tool_executor::structured_output_trigger);
    let sourced_fields = fermi::grounding_trust::contracts_for(&agent_name)
        .filter(|c| {
            matches!(
                c.grounding,
                fermi::grounding_trust::Grounding::Sourced { .. }
            )
        })
        .count();
    let produces_schema: Option<String> = row
        .try_get::<Option<Value>, _>("output_contract")
        .ok()
        .flatten()
        .and_then(|oc| {
            oc.get("produces_schema")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        });
    // Absent is not bad: an agent with no prompt is unconfigured, and one with no
    // type name cannot fail to mention it.
    let names_its_type = match (&system_prompt, &produces_schema) {
        (Some(p), Some(t)) => Some(p.contains(t.as_str())),
        _ => None,
    };

    let taxonomy: Option<Value> = row.try_get("taxonomy").ok().flatten();
    let succeeded: i64 = rec.try_get("succeeded").unwrap_or(0);

    // ── Compile-state computation ──────────────────────────────────────────────
    //
    // Computed before the json! macro because json! does not support let
    // bindings inside field values.
    //
    // has_any_contract: FIELD_CONTRACTS entries (the Rust table, checked by
    //   contracts_for) count as a contract for this purpose. So does a
    //   compiled output_contract in the DB row (declares_contract = true).
    //
    // is_exempt: on TYPED_TIER_EXEMPT. Predates the typed tier; allowed to
    //   remain published without a contract until the burn-down list empties.
    //   Grandfathered, not compiled.
    //
    // compiles: true only when a contract is present AND zero errors.
    //   An absent contract is not zero errors; it is an unwritten promise,
    //   and an unwritten promise is not a kept one. The previous computation
    //   (error_count == 0) was vacuously true when no contract existed.
    let has_any_contract = !contract_fields.is_empty()
        || row
            .try_get::<Option<bool>, _>("declares_contract")
            .ok()
            .flatten()
            .unwrap_or(false);
    let is_type_tier_exempt = fermi::workflows::agent_contract::is_typed_tier_exempt(&agent_name);
    let compile_error_count = counts.get("error").copied().unwrap_or(0);
    let compiles = if !has_any_contract && !is_type_tier_exempt {
        // No contract, not grandfathered: compile failure.
        // The absence of a contract is the finding; it is not counted in
        // `counts` (which only counts field-level states) but it is
        // reflected in the boolean.
        false
    } else {
        compile_error_count == 0
    };
    let grandfathered = is_type_tier_exempt && !has_any_contract;

    Ok(Json(json!({
        "profile": {
            "agent_name": row.get::<String, _>("agent_name"),
            "label": row.get::<String, _>("label"),
            "description": row.try_get::<Option<String>, _>("description").ok().flatten(),
            "agent_type": row.try_get::<Option<String>, _>("agent_type").ok().flatten(),
            "taxonomy": taxonomy,
            "tier": row.try_get::<Option<String>, _>("tier").ok().flatten(),
            "min_tier": row.try_get::<Option<String>, _>("min_tier").ok().flatten(),
            "status": row.try_get::<Option<String>, _>("status").ok().flatten(),
            "visibility": row.try_get::<Option<String>, _>("visibility").ok().flatten(),
            "tags": row.try_get::<Option<Vec<String>>, _>("tags").ok().flatten().unwrap_or_default(),
            "accepts": row.try_get::<Option<Vec<String>>, _>("accepts").ok().flatten().unwrap_or_default(),
            "produces": row.try_get::<Option<Vec<String>>, _>("produces").ok().flatten().unwrap_or_default(),
            "declares_contract": row.try_get::<Option<bool>, _>("declares_contract").ok().flatten().unwrap_or(false),
            "typed": row.try_get::<Option<bool>, _>("typed").ok().flatten().unwrap_or(false),
            "provenance": row.try_get::<Option<String>, _>("provenance").ok().flatten(),
            "level": row.try_get::<Option<i32>, _>("level").ok().flatten(),
            "peak_level": row.try_get::<Option<i32>, _>("peak_level").ok().flatten(),
            "forked_from": row.try_get::<Option<String>, _>("forked_from").ok().flatten(),
            "fork_count": row.try_get::<Option<i32>, _>("fork_count").ok().flatten().unwrap_or(0),
            "sample_queries": row.try_get::<Option<Vec<String>>, _>("sample_queries").ok().flatten().unwrap_or_default(),
            "substrate": {
                "provider": row.try_get::<Option<String>, _>("llm_provider").ok().flatten(),
                "model": row.try_get::<Option<String>, _>("model").ok().flatten(),
                "executor": row.try_get::<Option<String>, _>("executor_type").ok().flatten(),
                "temperature": row.try_get::<Option<f64>, _>("temperature").ok().flatten(),
                "persona_version": row.try_get::<Option<i32>, _>("persona_version").ok().flatten(),
                // Read `model`/`provider` as the FALLBACK. `apply_tier_resolution`
                // picks the highest rung whose tier is at or below the caller's
                // and overwrites both; the defaults stand only when no rung
                // matches. An empty array means there is no ladder and the
                // defaults are what runs.
                "model_ladder": row.try_get::<Option<Value>, _>("model_ladder").ok().flatten(),
            },
            // The affective signature: primary_affect, arousal, valence,
            // personality_traits. Served so the shelf can edit the four fields it
            // actually has rather than offering a JSON box.
            "valence": row.try_get::<Option<Value>, _>("valence").ok().flatten(),
            // Competition declaration — domains, price, support_tier (editable).
            // Fidelity and selection rate are platform-computed; not included here.
            "competition": row.try_get::<Option<Value>, _>("competition").ok().flatten(),
            // Typed port schema IDs — rendered as schema-id chips on the shelf.
            "input_schema_id":  row.try_get::<Option<String>, _>("input_schema_id").ok().flatten(),
            "output_schema_id": row.try_get::<Option<String>, _>("output_schema_id").ok().flatten(),
            "system_prompt": system_prompt.clone(),
            // What the prompt does to execution, and whether that agrees with
            // what the contract promises.
            "prompt_check": {
                "gets_tools": trigger.is_none(),
                "trigger": trigger,
                "sourced_fields": sourced_fields,
                // The one state that is an ERROR rather than a reading: the
                // contract names tools for N fields and the prompt has removed
                // the loop that could call them.
                "contradicts_contract": trigger.is_some() && sourced_fields > 0,
                "produces_schema": produces_schema,
                "names_its_type": names_its_type,
                "why": "`prompt_demands_structured_output` substring-matches the \
                        system prompt, and a match makes `ToolAwareExecutor` skip \
                        the tool loop entirely. The prompt is not documentation.",
            },
        },
        "record": {
            "runs": runs,
            "succeeded": succeeded,
            "failed": rec.try_get::<Option<i64>, _>("failed").ok().flatten().unwrap_or(0),
            // Absent rather than zero when there is nothing to divide. A success
            // rate of 0% and "never run" are different facts and the old page
            // rendered both as 0.0%.
            "success_rate": if runs > 0 { json!(succeeded as f64 / runs as f64) } else { Value::Null },
            "cost_usd": cost,
            "cost_per_run": match (cost, runs) { (Some(c), r) if r > 0 => json!(c / r as f64), _ => Value::Null },
            "last_run": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_run").ok().flatten().map(|t| t.to_rfc3339()),
            // One name each. The old page called `facts` "Relationships" in one
            // panel and "facts" in another, for the same number.
            "entities": rec.try_get::<Option<i64>, _>("entities").ok().flatten().unwrap_or(0),
            "facts": rec.try_get::<Option<i64>, _>("facts").ok().flatten().unwrap_or(0),
            "rules": rec.try_get::<Option<i64>, _>("rules").ok().flatten().unwrap_or(0),
            "rules_retrieved": rec.try_get::<Option<i64>, _>("rules_retrieved").ok().flatten().unwrap_or(0),
            "dream_cycles": rec.try_get::<Option<i64>, _>("dream_cycles").ok().flatten().unwrap_or(0),
            "last_dreamt": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_dreamt").ok().flatten().map(|t| t.to_rfc3339()),
            "dream_budget": row.try_get::<Option<i32>, _>("dreaming_budget_credits").ok().flatten().unwrap_or(0),
            "dream_used": row.try_get::<Option<i32>, _>("dreaming_credits_used").ok().flatten().unwrap_or(0),
            "eval_runs": rec.try_get::<Option<i64>, _>("eval_runs").ok().flatten().unwrap_or(0),
            "last_eval": rec.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_eval").ok().flatten().map(|t| t.to_rfc3339()),
            // One mapper, so a pulse means the same thing here as in the stream.
            "episodes": episodes.iter().map(pulse_row).collect::<Vec<_>>(),
            // `null` when the read succeeded. A string here means the list is
            // unknown rather than empty, and the surface must say so.
            "episodes_error": episodes_error,
        },
        "health": health,
        // What this agent has declared, rung by rung, with what each unlocks and
        // what reads `unknown` without it. The configuration shelf's one
        // rankable group: `next` is the cheapest absent rung, which is the only
        // configuration question the platform can answer for the author rather
        // than the other way round.
        "declaration": {
            "rungs": ladder,
            "declared": present.len(),
            "total": fermi::declaration_ladder::LADDER.len(),
            "next": fermi::declaration_ladder::LADDER
                .iter()
                .find(|d| !present.contains(&d.rung))
                .map(|d| d.rung),
            // The contract's own fields, and whether each resolves.
            "fields": contract_fields,
            "counts": counts,
            // Three compile states, not two. See the comment above the
            // json! call for the full explanation.
            "compiles": compiles,
            "grandfathered": grandfathered,
            "no_contract": !has_any_contract,
            "green_means": "zero errors AND a contract is present. `pending` fields are \
                            declared gaps with no source yet — an agent with pending fields \
                            and no errors compiles. `grandfathered` means the agent predates \
                            the typed tier: not blocked, not clean.",
        },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pulse projection must resolve against the live schema.
    ///
    /// It named `e.error_message`, and the column is `error_details`. Both
    /// readers broke: `/api/stream` propagated and went 502, and the specimen
    /// swallowed the error with `.unwrap_or_default()` and rendered **"No pulse
    /// recorded"** for an agent with 218 of them — the quieter failure, and the
    /// worse one.
    ///
    /// The repo's schema lint passed on that commit. It resolves a qualified
    /// name against a **global** set of 968 known columns rather than against
    /// the table the alias refers to, and `error_message` exists on another
    /// table, so `e.error_message` looked fine. Postgres can answer this
    /// question exactly, so this asks Postgres: `LIMIT 0` executes the plan,
    /// resolves every column, and returns no rows.
    ///
    /// Skips rather than fails without a database, because the unit suite runs
    /// where there is none — and CI has one.
    #[tokio::test]
    async fn the_pulse_projection_resolves_against_the_schema() {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!("skipped: needs DATABASE_URL");
            return;
        };
        let Ok(pool) = sqlx::PgPool::connect(&url).await else {
            eprintln!("skipped: could not connect");
            return;
        };
        for (what, sql) in [
            (
                "agent-scoped",
                format!("{PULSE_SELECT} WHERE e.agent_id = $1 LIMIT 0"),
            ),
            (
                "the whole stream",
                format!("{PULSE_SELECT} WHERE a.agent_name NOT LIKE 'x' LIMIT 0"),
            ),
        ] {
            let q = sqlx::query(&sql);
            let q = if what == "agent-scoped" {
                q.bind(uuid::Uuid::nil())
            } else {
                q
            };
            if let Err(e) = q.fetch_all(&pool).await {
                panic!(
                    "`PULSE_SELECT` does not resolve for {what}: {e}\n\n\
                     Every surface that lists pulses reads this projection. A \
                     column that does not exist takes the stream to 502 and the \
                     specimen to a silent empty list."
                );
            }
        }
    }
}

#[cfg(test)]
mod compile_state_tests {
    /// `pending` is not a lesser `resolved`, and an agent full of it compiles.
    ///
    /// The case this is written from: `genome_profiler` declares fifteen fields,
    /// seven of which say *no tool exists for this yet*. It is the best-declared
    /// agent on the platform, and every surface reported it as not-green —
    /// making the only route to a healthy agent deleting the ambition the
    /// contract exists to record.
    ///
    /// Checked against the real `FIELD_CONTRACTS` rather than a fixture, because
    /// the thing that would break this is somebody re-grading a field, and a
    /// fixture would keep passing while the fleet changed underneath it.
    #[test]
    fn an_agent_of_pending_fields_still_compiles() {
        use fermi::grounding_trust::{contracts_for, Grounding};

        let mut pending = 0;
        let mut sourced = 0;
        let mut unknown_tool = 0;
        let dispatchable: std::collections::HashSet<&'static str> =
            fermi::agent_backend::tools::dispatchable_tool_names()
                .into_iter()
                .collect();

        for c in contracts_for("genome_profiler") {
            match c.grounding {
                Grounding::Unsourced => pending += 1,
                Grounding::Sourced { tool, .. } => {
                    sourced += 1;
                    if !dispatchable.contains(tool) {
                        unknown_tool += 1;
                    }
                }
                _ => {}
            }
        }

        assert!(
            pending >= 5,
            "genome_profiler declares {pending} unsourced fields; this test is \
             reading the wrong agent or the contract was pruned — which is the \
             thing it exists to make unnecessary"
        );
        assert!(sourced > 0, "no sourced fields, so nothing could resolve");
        assert_eq!(
            unknown_tool, 0,
            "genome_profiler names {unknown_tool} tool(s) the platform cannot \
             dispatch. Those are ERRORS and the agent does not compile — which \
             is a real finding, not a test to relax"
        );
    }

    /// Every tool named by any contract must be dispatchable, or the field it
    /// claims to settle can never be settled by anything.
    ///
    /// This is the `error` state, computed over the whole fleet. It is empty
    /// today; if it stops being empty, some agent is carrying a promise nothing
    /// can keep.
    #[test]
    fn no_contract_names_a_tool_the_platform_cannot_dispatch() {
        use fermi::grounding_trust::{Grounding, FIELD_CONTRACTS};

        let dispatchable: std::collections::HashSet<&'static str> =
            fermi::agent_backend::tools::dispatchable_tool_names()
                .into_iter()
                .collect();

        let broken: Vec<String> = FIELD_CONTRACTS
            .iter()
            .filter_map(|c| match c.grounding {
                Grounding::Sourced { tool, .. } if !dispatchable.contains(tool) => {
                    Some(format!("{}.{} -> {tool}", c.agent_id, c.path))
                }
                _ => None,
            })
            .collect();

        assert!(
            broken.is_empty(),
            "these contracts name tools the platform cannot dispatch, so the \
             fields can never be settled by anything: {broken:#?}"
        );
    }
}
