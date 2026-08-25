//! `GET /api/loops` — the loop surface, for people.
//!
//! # Why a handler and not another observatory tab
//!
//! Everything served here already existed and was reachable two ways, both
//! wrong for a UI:
//!
//! * `/api/admin/schema-health` — the whole platform's diagnostics in one blob,
//!   admin-scoped, shaped for a triage session rather than a screen;
//! * `/api/observatory/agents/:id/loops` — 610 lines of bespoke per-loop SQL
//!   giving a **second answer** to the question `loop_model` answers from the
//!   contracts, which is the duplication §3.4 warns about and the audit's §9
//!   item 6.
//!
//! This is one shape, one implementation behind it, and it carries the three
//! things a surface needs that a row count cannot give: whether an empty panel
//! is idle / faulty / unknowable, what a person can do about it, and what a
//! green tick does **not** mean.
//!
//! # Read-only
//!
//! One walk of `loop_model::evaluate`, which is a `SELECT count(*)` per stage
//! and is asserted read-only by `loop_model::tests::every_stage_query_is_read_only`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;
use fermi_auth::AuthPrincipal;

/// GET /api/loops
///
/// Every declared feedback loop, its first empty link, and the door a person
/// has at that link if there is one.
pub async fn list_loops_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let states = fermi::loop_model::evaluate(&state.db).await;
    let views = fermi::loop_api::views(&states);
    let tally = fermi::loop_api::tally(&views);

    Ok(Json(json!({
        // Five buckets, not one number. "2 of 6 turning" invites a reader to
        // infer four are broken; "0 stalled" invites the opposite. The header
        // has to carry the same distinctions the panels do, and the one that
        // matters is `no_reading` — stopped, with no contract able to say why,
        // which is neither healthy nor broken and must not be coloured as
        // either.
        "tally": tally,
        "loops": views,
        // The vocabulary a client will branch on, served rather than hardcoded
        // on the far side. `panel_absence::every_stall_reason_is_classified`
        // holds the mapping; a client that copies this list is copying a
        // declaration instead of inventing a parallel one.
        "vocabulary": {
            "reading": ["idle", "fault", "unknown"],
            "status": ["turning", "stalled", "unmeasured"],
            "tally_bucket": [
                "turning", "stalled_by_fault", "stalled_idle", "no_reading",
                "unreadable"
            ],
            "trigger": [
                "request", "sweeper", "upstream", "manual", "prompted",
                "nothing_calls_it"
            ],
            "stall_reason": fermi::loop_model::STALL_REASONS,
        },
        // Stated in the payload because it is the whole point of the surface:
        // a client must not render `rows: 0` as a number. `measured` says
        // whether there is a reading at all, and `reading` says what the
        // absence means.
        "contract": "Never render a bare zero. `measured: false` means the \
                     probe did not run — show nothing, not zero. `reading` \
                     explains every empty panel: `idle` is correctly empty, \
                     `fault` is something that should have happened and did \
                     not, `unknown` is that no contract can say. `unknown` is \
                     not a pass.",
    })))
}

/// GET /api/loops/:loop_id
///
/// One loop, with the same shape as an element of `loops` above.
pub async fn get_loop_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(loop_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let states = fermi::loop_model::evaluate(&state.db).await;
    let Some(view) = fermi::loop_api::view_of(&states, &loop_id) else {
        // The declared set, so a 404 tells a client what it could have asked
        // for rather than only that it was wrong.
        let known: Vec<&str> = fermi::loop_model::LOOPS.iter().map(|l| l.id).collect();
        return Err((
            StatusCode::NOT_FOUND,
            format!("no loop `{loop_id}`; declared loops are {known:?}"),
        ));
    };
    Ok(Json(json!({ "loop": view })))
}

/// GET /api/agents/:agent_id/loops
///
/// One agent's chain, and — the point of it — **which stages can be asked about
/// an agent at all.**
///
/// Replaces `observatory::agent_loops_handler`, which was 610 lines of separate
/// per-loop SQL and a second answer to `loop_model`'s question. Its own comment
/// records the defect this shape is built against: *"two rows of which were
/// hardcoded constants rendered under a live status column"*.
///
/// Fifteen of the twenty-three stages have no agent dimension — a forecast
/// resolves, a workspace coheres, a sensor reads — and each says so in
/// `because`. A surface that showed the platform's figure for those under an
/// agent's name would be repeating the original defect exactly.
pub async fn agent_loops_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let states = fermi::loop_model::evaluate(&state.db).await;

    let mut loops = Vec::with_capacity(states.len());
    for s in &states {
        // One probe per answerable stage. A probe that errors yields `None`,
        // which renders as "not measured for this agent" rather than as zero —
        // the same sentinel discipline as `loop_model`'s `-1`, expressed in the
        // type instead of in a magic number.
        let mut counts: Vec<(&'static str, Option<i64>)> = Vec::new();
        for stage in &s.stages {
            if let Some(fermi::loop_api::SubjectScope::PerAgent { sql }) =
                fermi::loop_api::subject_scope(s.id, stage.id)
            {
                let n = sqlx::query_scalar::<_, i64>(sql)
                    .bind(agent_id)
                    .fetch_one(&state.db)
                    .await
                    .ok();
                counts.push((stage.id, n));
            }
        }
        loops.push(fermi::loop_api::agent_view(s, &counts));
    }

    let answerable: usize = loops.iter().map(|l| l.answerable).sum();
    let total: usize = loops.iter().map(|l| l.total).sum();

    Ok(Json(json!({
        "agent_id": agent_id,
        "coverage": {
            "answerable": answerable,
            "total": total,
            "note": "How much of the loop machinery can be asked about one \
                     agent. The rest is about forecasts, workspaces and \
                     sensors, and each stage says which it is in `because`.",
        },
        "loops": loops,
        "contract": "`rows: null` means the question does not apply to an agent \
                     — render nothing. `rows: 0` means it applies and the \
                     answer is none. `platform_rows` is context and is never \
                     this agent's figure; showing it as one is the defect this \
                     endpoint replaces.",
    })))
}

/// GET /api/agents/:agent_id/coordination-notes
///
/// The coherence notes a strategist wrote into this agent's memory, and whether
/// the agent has dreamt on them yet.
///
/// # Why this is its own endpoint
///
/// It is the one place Loop 3 and Loop 1 meet, and the only surface on which
/// the platform's central claim about coordination is visible at all: a
/// strategist's brief is *a document*, and
/// `record_coordination_observation` writing an episode is what makes it
/// something the agent learns from — because dreaming reads episodes, not
/// workspace git.
///
/// So the useful column is not the note, it is **whether consolidation has run
/// over it since**. A note received and never dreamt on is the same as a note
/// nobody sent, and nothing until now could tell them apart.
///
/// Honestly empty today: `coordinator_observation` stands at 0 of 3,576
/// episodes, so `record_coordination_observation` has never once been called.
/// The endpoint says that rather than returning `[]`.
pub async fn agent_coordination_notes_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // `consolidation_jobs` records a window over an agent's episodes rather than
    // a list of episode ids, so "has this note been dreamt on" is answered by
    // asking whether a completed job covered it in time. Stated because it is a
    // proxy: a job that ran after the note and processed zero episodes would
    // read as having consolidated it.
    let rows = sqlx::query(
        "SELECT e.episode_id, e.created_at, e.query, e.response, \
                EXISTS ( \
                    SELECT 1 FROM consolidation_jobs j \
                     WHERE j.agent_id = e.agent_id \
                       AND j.status = 'completed' \
                       AND j.completed_at > e.created_at \
                ) AS consolidated \
           FROM episodes e \
          WHERE e.agent_id = $1 \
            AND e.provenance = 'coordinator_observation' \
          ORDER BY e.created_at DESC \
          LIMIT 100",
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let notes: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
                "received_at": r
                    .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok()
                    .map(|t| t.to_rfc3339()),
                "about": r.try_get::<String, _>("query").ok(),
                "note": r.try_get::<Option<String>, _>("response").ok().flatten(),
                // The column that matters. A note not yet dreamt on has had no
                // effect on anything the agent does.
                "consolidated": r.try_get::<bool, _>("consolidated").unwrap_or(false),
            })
        })
        .collect();

    // Empty is never blank. Which of the two empties this is decides what a
    // reader should do about it, and they are not the same instruction.
    let platform_total: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM episodes WHERE provenance = 'coordinator_observation'",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(-1);

    let reading = if !notes.is_empty() {
        "idle"
    } else if platform_total == 0 {
        // Nobody has ever received one. Not this agent's problem.
        "unknown"
    } else {
        // Others have; this agent has not.
        "idle"
    };

    Ok(Json(json!({
        "agent_id": agent_id,
        "notes": notes,
        "reading": reading,
        "detail": if !notes.is_empty() {
            format!("{} coordination note(s) on file.", notes.len())
        } else if platform_total == 0 {
            "No agent anywhere has received a coordination note. \
             `record_coordination_observation` exists and is asked for by the \
             strategist's Stage 3 prompt, and has never been called — so this \
             is not a fact about this agent. Loop 3's `brief` stage is the one \
             to look at."
                .to_string()
        } else {
            format!(
                "This agent has received no coordination notes; {platform_total} \
                 exist across the platform, so the path works and has not \
                 reached this agent."
            )
        },
        "contract": "`consolidated: false` means the note is in the agent's \
                     memory and has not been dreamt on, so it has changed \
                     nothing yet. That is the field to surface — a note \
                     received and never consolidated is indistinguishable, in \
                     the agent's behaviour, from one nobody sent.",
    })))
}

/// GET /api/gates
///
/// Every declared gate, its reading, and whether its counters survive a
/// restart.
///
/// Served from the same module as the loops because they are the same pattern
/// over a different domain — [`fermi::surface`] declares the two parts they
/// share — and because a reader looking for "what is stopping things" should
/// not have to know whether the answer is a chain or a control.
///
/// No database walk: gate counters are in-memory.
pub async fn list_gates_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let views = fermi::gate_api::views();
    let tally = fermi::gate_api::tally(&views);
    Ok(Json(json!({
        "tally": tally,
        "gates": views,
        // Empty, and stated rather than omitted. Nothing anywhere lets a person
        // act on a gate; a client should render the absence rather than hide
        // it, because "no actions available" is the honest reading and a blank
        // panel is not.
        "doors": fermi::gate_api::GATE_DOORS,
        "caveats": fermi::gate_api::GATE_CAVEATS,
        "vocabulary": {
            "reading": ["idle", "fault", "unknown"],
            "token": [
                "discriminating", "refuses_everything", "admits_everything",
                "never_asked"
            ],
            "since": ["boot", "ledger"],
        },
        "contract": "`refused: 0` is not a pass. Two tokens map to `unknown` \
                     and mean different things: `never_asked` is a control \
                     nobody has exercised, `admits_everything` is one that has \
                     run and stopped nothing. `since: boot` means the counters \
                     reset on restart — do not render them as a lifetime \
                     total. The `caveats` array says what each `unknown` fails \
                     to establish; show it.",
    })))
}

/// GET /api/evaluators
///
/// What the platform concluded about its own machinery, and what those
/// conclusions do not mean.
///
/// One snapshot, so every verdict describes the same instant. The loop walk is
/// the expensive part and it is shared with `/api/loops` by construction rather
/// than by convention: both read the same `Observation`.
pub async fn list_evaluators_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Gathered once. Two evaluators disagreeing about which instant they
    // describe is the defect `native_evaluators::Observation` exists to prevent.
    let observation = fermi::native_evaluators::Observation::collect(&state.db).await;
    let views = fermi::evaluator_api::views(&observation);
    let tally = fermi::evaluator_api::tally(&views);

    Ok(Json(json!({
        "tally": tally,
        "evaluators": views,
        // Empty, and for a better reason than the gates'. An evaluator is a
        // pure function over a snapshot: there is nothing to approve or
        // override, and a verdict a person could wave away would not be worth
        // computing. Act on the subject a finding names, not on the finding.
        "doors": fermi::evaluator_api::EVALUATOR_DOORS,
        "caveats": fermi::evaluator_api::EVALUATOR_CAVEATS,
        "vocabulary": {
            "reading": ["idle", "fault", "unknown"],
            "token": ["healthy", "critical", "warning", "notice", "inconclusive"],
        },
        "contract": "`inconclusive` is NOT a pass, and three of the six are \
                     usually in it — most of these counters are process-local \
                     and reset on restart, so a cold snapshot honestly \
                     concludes nothing. `notice` is not a pass either: it is \
                     reported and never asserted. Both read `unknown` and mean \
                     different things, so branch on `token` and colour on \
                     `reading`. Where an evaluator carries a `caveat`, its \
                     passing verdict is narrower than it reads — show it.",
    })))
}

/// GET /api/loops/actions
///
/// Every human door into every loop, without walking the database.
///
/// Served separately because it is a **declaration, not a measurement**: a UI
/// can build its buttons from this at startup and does not need six `count(*)`
/// queries to know what actions exist. Keeping it out of the loop payload also
/// keeps the two honest — the actions are the same whether or not a loop is
/// turning, and a surface that only showed a door when a queue was non-empty
/// would hide the door precisely when someone wanted to ask why it was empty.
pub async fn list_loop_actions_handler(
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    Ok(Json(json!({
        "actions": fermi::loop_api::STAGE_ACTIONS,
        "note": "`why_manual` is required on every entry: a stage that cannot \
                 argue for being manual should be automated. Show it — a \
                 reviewer deciding whether a queue is worth working needs the \
                 argument, not just the button.",
    })))
}
