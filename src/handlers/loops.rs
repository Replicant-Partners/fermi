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

/// GET /api/gates/:gate_id/decisions
///
/// What this gate actually refused, from the durable ledger.
///
/// # A read, not a door
///
/// `gate_api::GATE_DOORS` is empty and stays empty: this endpoint lets a person
/// **see** what a gate stopped, and nothing lets them act on it. That is the
/// honest split. Whether a refusal should be overridable is a real decision with
/// safety weight — a gate a person can wave through is not much of a gate — and
/// it has never been made. Seeing first is the part that needs no argument.
///
/// # `since: "ledger"` is a claim, and this is where it is checked
///
/// A gate declared `Retention::Recorded` tells the surface its counters survive
/// a restart. `ledger_claim` says whether the rows behind that exist. Until
/// migration 214 ran, the platform had a record of every request it served and
/// none of any it refused; a gate reporting `ledger` over an empty table is
/// making the same claim on the same evidence.
pub async fn gate_decisions_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(gate_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let Some(account) = fermi::gate_trust::accounts()
        .into_iter()
        .find(|a| a.id == gate_id)
    else {
        let known: Vec<&str> = fermi::gate_trust::GATES.iter().map(|g| g.id).collect();
        return Err((
            StatusCode::NOT_FOUND,
            format!("no gate `{gate_id}`; declared gates are {known:?}"),
        ));
    };

    let total: i64 = sqlx::query_scalar(fermi::gate_api::LEDGER_COUNT_SQL)
        .bind(&gate_id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(-1);

    let rows = sqlx::query(fermi::gate_api::LEDGER_SQL)
        .bind(&gate_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // The reviews, before the decisions are dressed, so each decision can carry
    // its own current verdict. `LATEST_REVIEWS_SQL` is `DISTINCT ON` over an
    // append-only log — derived current state, migration 205's pattern — so a
    // decision upheld and later overturned reads as overturned with the earlier
    // row still on file.
    let review_rows = sqlx::query(fermi::gate_review::LATEST_REVIEWS_SQL)
        .bind(&gate_id)
        // The priority token, bound rather than spelled in the SQL: the ordering
        // and the CHECK cannot then disagree about how the word is written.
        .bind(fermi::seam_vocabulary::GateReviewVerdict::Overturned)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut latest: std::collections::HashMap<i64, Value> = std::collections::HashMap::new();
    for r in &review_rows {
        let Ok(decision_id) = r.try_get::<i64, _>("decision_id") else {
            continue;
        };
        latest.insert(
            decision_id,
            json!({
                "verdict": r.try_get::<String, _>("verdict").ok(),
                "rationale": r.try_get::<Option<String>, _>("rationale").ok().flatten(),
                "actor": r.try_get::<String, _>("actor").ok(),
                "actor_kind": r.try_get::<String, _>("actor_kind").ok(),
                "reviewed_at": r
                    .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok()
                    .map(|t| t.to_rfc3339()),
            }),
        );
    }

    let decisions: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id = r.try_get::<i64, _>("id").ok();
            json!({
                // The handle the review door needs. A read whose rows a client
                // cannot then act on is how a door ends up unbuildable after
                // the endpoint is written.
                "id": id,
                // `null` means nobody has judged this decision, and that is not
                // the same as nobody having found anything wrong with it.
                "review": id.and_then(|i| latest.get(&i)).cloned(),
                "decision": r.try_get::<String, _>("decision").ok(),
                // What the gate was deciding about. Named so a refusal points
                // at a thing rather than starting an investigation.
                "subject": r.try_get::<Option<String>, _>("subject").ok().flatten(),
                "reason": r.try_get::<Option<String>, _>("reason").ok().flatten(),
                "decided_at": r
                    .try_get::<chrono::DateTime<chrono::Utc>, _>("decided_at")
                    .ok()
                    .map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    // Where the gate stands with its reviewers. A separate answer from the
    // ledger claim below and deliberately not folded into it: `Unbacked` is
    // about whether the durability the surface advertises has rows behind it,
    // and `Unreviewed` is about whether anyone has read them. A full ledger
    // nobody has looked at is a healthy `Backed` and an `Unreviewed` standing at
    // the same time, and both are true.
    let counts: Vec<(String, i64)> = sqlx::query_as(fermi::gate_review::STANDING_SQL)
        .bind(&gate_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    // A token no variant spells is surfaced, not bucketed: it means the CHECK is
    // wider than `seam_vocabulary::GateReviewVerdict`, which is half of the
    // `severity = 'L1'` shape, and the rows are the only place it appears.
    let (standing, standing_error) = match fermi::gate_review::tally_from_counts(&counts) {
        Ok(tally) => (
            Some(fermi::gate_review::standing(total.max(0), tally)),
            None,
        ),
        Err(unknown) => (None, Some(unknown.to_string())),
    };
    let review = standing.map(|s| {
        let (reading, token) = fermi::gate_review::reading(s);
        json!({
            "standing": s,
            "reading": reading.label(),
            "token": token,
        })
    });

    let claim = fermi::gate_api::ledger_claim(&account, total.max(0));
    let (reading, detail) = match claim {
        fermi::gate_api::LedgerClaim::Backed { rows } => {
            ("idle", format!("{rows} decision(s) on file, durably."))
        }
        fermi::gate_api::LedgerClaim::Unbacked { asked } => (
            "fault",
            format!(
                "This gate has been asked {asked} time(s) since boot and its \
                 ledger is empty. It is declared `Recorded`, so the surface \
                 reports its counters as surviving a restart — and nothing is \
                 behind that. Check `gate_trust::spawn_gate_recorder` is \
                 draining, and `ledger_status().dropped` for a full queue."
            ),
        ),
        fermi::gate_api::LedgerClaim::NothingToRecord => (
            "unknown",
            "This gate has not been asked since the counters started, so there \
             is nothing to have recorded. Not a pass — an unwired control looks \
             exactly like this."
                .to_string(),
        ),
        fermi::gate_api::LedgerClaim::NotClaimed => (
            "unknown",
            "This gate is counted in memory only and never claimed durability. \
             Its decisions vanish on restart by design, so an empty ledger here \
             is correct and says nothing about the gate."
                .to_string(),
        ),
    };

    Ok(Json(json!({
        "gate": gate_id,
        "ledger_total": total,
        "decisions": decisions,
        "reading": reading,
        "detail": detail,
        // The second, independent reading: has anyone judged these decisions?
        // `null` only when the column holds a verdict token no Rust variant
        // spells, which is a platform defect and is reported as one rather than
        // as an absence of reviews.
        "review": review,
        "review_error": standing_error,
        // The doors for THIS gate, not the whole list. Five of the seven gates
        // are `Retention::Counted`, their individual decisions never leave the
        // process, and offering a per-decision review on one of those is a
        // button pointing at a row that does not exist. Rendering the empty case
        // is the point: a reviewer looking at `rate_limit` should be told there
        // is nothing to review and why, not shown a control that 404s.
        "doors": fermi::gate_api::GATE_DOORS
            .iter()
            .filter(|d| d.subject == gate_id)
            .collect::<Vec<_>>(),
        "caveats": fermi::gate_api::GATE_CAVEATS,
        "contract": "Refusals are ordered first: what was stopped is what a \
                     reader came for. `reading: fault` here is about the LEDGER, \
                     not about the gate — it means the durability the surface \
                     advertises is not backed by rows. `review.reading` is a \
                     third thing again: whether anybody has said the decisions \
                     were right. No count can answer that, which is why it is \
                     here.",
    })))
}

/// POST /api/gates/:gate_id/decisions/:decision_id/review
///
/// Record whether one gate decision was right. **Records; does not override.**
///
/// # Why this endpoint is not a convenience
///
/// Every other reading on this surface is computed from approve/refuse counts,
/// and no arrangement of counts distinguishes a correct refusal from an incorrect
/// one. `refuses_everything` catches the extreme — asked, and approved nothing —
/// and a gate that approves 90% of what it sees and refuses the other 10%
/// *wrongly* reads `discriminating`, which `/api/gates` renders as healthy.
/// Correctness is a judgement about the subject, so this is the only instrument
/// that can see that failure. See `fermi::gate_review`.
///
/// # The rationale rule is Postgres's
///
/// `overturned` requires a rationale, enforced by
/// `gate_decision_reviews_rationale_check`. This handler does **not** re-check
/// it before the insert: two implementations of one trust rule is §3.4, and the
/// predictable end state is a Rust guard narrower than the constraint, an insert
/// that fails anyway, and a 500 where the reviewer was told their finding had
/// been filed. `gate_review::classify_write_error` translates the constraint's
/// refusal into a 400 instead.
pub async fn review_gate_decision_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((gate_id, decision_id)): Path<(String, i64)>,
    Json(body): Json<fermi::gate_review::ReviewRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    // The door is declared per gate, and the declaration is the authorisation.
    // Checking `GATE_DOORS` rather than `Retention` keeps one answer to "may a
    // person act here": `gate_api::a_review_door_only_exists_where_the_decisions
    // _do` already holds the door set to the recorded gates, and re-deriving the
    // rule here would be a second implementation that could disagree with the
    // door the client was shown.
    if !fermi::gate_api::GATE_DOORS
        .iter()
        .any(|d| d.subject == gate_id && d.path.contains("/decisions/"))
    {
        let open: Vec<&str> = fermi::gate_api::GATE_DOORS
            .iter()
            .filter(|d| d.path.contains("/decisions/"))
            .map(|d| d.subject)
            .collect();
        return Err((
            StatusCode::NOT_FOUND,
            format!(
                "`{gate_id}` has no review door. Only gates declared \
                 `Retention::Recorded` write a decision a review can point at; \
                 reviewable gates are {open:?}. If `{gate_id}` should be one, \
                 that argument belongs in `gate_trust::GATES`."
            ),
        ));
    }

    // The gate comes off the decision row, never off the path. A client-supplied
    // gate would let a review be filed against another gate's standing, and that
    // is the one field here whose corruption is silent — the review would be
    // written, the reviewer told it succeeded, and it would be counted against a
    // control it says nothing about.
    let found = sqlx::query(fermi::gate_review::DECISION_LOOKUP_SQL)
        .bind(decision_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some(row) = found else {
        return Err((
            StatusCode::NOT_FOUND,
            format!("no gate decision {decision_id}"),
        ));
    };
    let actual_gate: String = row
        .try_get("gate")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if actual_gate != gate_id {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "decision {decision_id} belongs to gate `{actual_gate}`, not \
                 `{gate_id}`. Refused rather than corrected: a review filed \
                 under the wrong gate counts against a control it says nothing \
                 about, and silently accepting the mismatch would make that \
                 undetectable."
            ),
        ));
    }

    // `real_user_id`, not `user_id`. Under impersonation those differ, and this
    // row is an audit record: attributing a judgement to the user an admin is
    // viewing as would let impersonation launder it. `fermi_auth` says of this
    // method "audit only — never use it for access control", and this is the
    // audit case it means.
    let actor = principal.real_user_id();
    let actor_kind = body
        .actor_kind
        .unwrap_or(fermi::seam_vocabulary::ActorKind::Human);

    let written = sqlx::query(fermi::gate_review::REVIEW_INSERT_SQL)
        .bind(decision_id)
        .bind(&actual_gate)
        .bind(body.verdict)
        .bind(body.rationale.as_deref())
        .bind(&actor)
        .bind(actor_kind)
        .bind(body.evidence.as_ref())
        .fetch_one(&state.db)
        .await;

    match written {
        Ok(row) => Ok(Json(json!({
            "review_id": row.try_get::<uuid::Uuid, _>("review_id").ok(),
            "decision_id": decision_id,
            "gate": actual_gate,
            "verdict": body.verdict,
            "actor": actor,
            "actor_kind": actor_kind,
            "recorded_at": row
                .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .ok()
                .map(|t| t.to_rfc3339()),
            "contract": "Recorded, not applied. This does not override the \
                         decision, re-run the gate, or admit what it refused. \
                         An `overturned` review is a finding for whoever can \
                         change the code, and the append-only log keeps every \
                         earlier verdict on the same decision.",
        }))),
        Err(e) => {
            let constraint = e.as_database_error().and_then(|d| d.constraint());
            let refusal = fermi::gate_review::classify_write_error(constraint, &e.to_string());
            let status = if fermi::gate_review::is_client_error(&refusal) {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            let detail = match &refusal {
                fermi::gate_review::Refusal::RationaleRequired => {
                    "`overturned` says the platform was wrong and should cause a \
                     change, so it requires a rationale. An uncited overturn is a \
                     complaint; the rationale is what makes it followable. \
                     `upheld` deliberately requires nothing — see migration 216."
                        .to_string()
                }
                fermi::gate_review::Refusal::NoSuchDecision => {
                    "That decision no longer exists. It was there a moment ago, \
                     so this is a race with a delete rather than a bad request."
                        .to_string()
                }
                fermi::gate_review::Refusal::UnknownToken { column } => format!(
                    "The database refused a `{column}` value the typed path \
                     cannot produce, which means migration 216's CHECK and \
                     `seam_vocabulary` have drifted. A platform defect, not \
                     yours. `tests/seam_vocabulary_contract.rs` names the \
                     mismatch."
                ),
                fermi::gate_review::Refusal::Rejected { error } => error.clone(),
            };
            Err((status, detail))
        }
    }
}

/// GET /api/episodes/:episode_id/trace
///
/// One artifact, and the checkpoints it passed.
///
/// # Why this endpoint and not another panel
///
/// Every other surface here is population-level: how many loops turn, how many
/// gates discriminate. That is the operator's question. *What happened to this
/// thing* is everyone else's, and until now the platform could not answer it.
///
/// # It recomputes nothing it does not own
///
/// The belt comes from `command_registry`, the clocks and refusal text from
/// `gate_trust::GATES`, the per-field grades from
/// `grounding_trust::graded_fields`, the weakest link from `grounding_trust
/// ::floor`, the routing from `assertions::Assertion::route`, and the reason an
/// empty trace is empty from `declaration_ladder::attribute`. This handler
/// assembles.
///
/// # Read-only, and it re-runs the contract rather than trusting a summary
///
/// `episodes.response_text` has been retained since migration 199, so the
/// grounding contract can be re-run over **the exact bytes the agent produced**.
/// That is the only reason a historical episode can be traced at all — and it is
/// why the retention was worth the storage: a digest is not a record.
///
/// # What it cannot show, said in the payload
///
/// `gate_decisions` carries no `episode_id`, so no recorded gate decision can be
/// joined to an artifact. Every rung is still listed, with
/// `outcome: not_recorded` and the reason — a belt that drops the checkpoints it
/// cannot report on looks shorter and safer than it is.
pub async fn episode_trace_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(episode_id): Path<uuid::Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT e.episode_id, e.parent_episode_id, e.agent_id, e.query, \
                e.response_text, e.provenance, e.model_used, e.provider_used, \
                e.persona_version_at_write, e.timestamp_ref, e.assertions, \
                a.agent_name, a.accepts, a.produces, a.output_contract \
           FROM episodes e \
           JOIN agents a ON a.agent_id = e.agent_id \
          WHERE e.episode_id = $1",
    )
    .bind(episode_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let Some(row) = row else {
        return Err((StatusCode::NOT_FOUND, format!("no episode {episode_id}")));
    };

    let agent_name: String = row.try_get("agent_name").unwrap_or_default();
    let response_text: Option<String> = row.try_get("response_text").ok().flatten();

    // Re-run the contract over the retained bytes. `enforce` mutates, so the
    // claimed values are read from a copy taken before it — the same rule the
    // execute path follows, and for the same reason: a nulled field has no
    // evidence in it.
    let claimed_doc = response_text
        .as_deref()
        .and_then(fermi::agent_backend::envelope::extract_json);
    let mut enforced = claimed_doc.clone();
    let report = match enforced.as_mut() {
        Some(doc) => fermi::grounding_trust::enforce(&agent_name, doc),
        None => fermi::grounding_trust::Report::default(),
    };
    let graded = match claimed_doc.as_ref() {
        Some(doc) => fermi::grounding_trust::graded_fields(&agent_name, doc, &report),
        None => Vec::new(),
    };
    let (fields, floor) = fermi::artifact_trace::fields(&graded);

    // What this agent has declared, so the empty case has a sourced cause rather
    // than this handler's guess.
    let mut rungs_declared: Vec<&'static str> = Vec::new();
    let accepts: Option<Vec<String>> = row.try_get("accepts").ok();
    let produces: Option<Vec<String>> = row.try_get("produces").ok();
    if accepts.as_ref().is_some_and(|v| !v.is_empty())
        && produces.as_ref().is_some_and(|v| !v.is_empty())
    {
        rungs_declared.push("ports");
    }
    let output_contract: Option<Value> = row.try_get("output_contract").ok();
    if let Some(oc) = output_contract.as_ref() {
        if oc.get("produces_schema").is_some() {
            rungs_declared.push("output_type");
        }
        if oc.get("schema").is_some_and(|s| s.is_object()) {
            rungs_declared.push("output_schema");
        }
    }
    if fermi::declaration_ladder::has_field_contract(&agent_name) {
        rungs_declared.push("field_contract");
    }
    let legibility = fermi::declaration_ladder::legibility(&rungs_declared);

    let (reading, token, silence, owner) =
        fermi::artifact_trace::reading(report.violations.len(), &graded, &legibility);

    // The belt, with the grounding rung's outcome filled in from this episode.
    // The route is the non-streaming command; both declare the same rungs and
    // `grounding_execute_coverage` holds them to it, so either is a correct
    // answer to "which checkpoints does this artifact's route have".
    let mut belt = fermi::artifact_trace::belt("agent.execute");
    for r in belt.iter_mut() {
        if r.rung == "grounding" {
            r.outcome = if graded.is_empty() {
                fermi::artifact_trace::Outcome::NotApplicable {
                    because: format!(
                        "`{agent_name}` declares no field contract, so this rung \
                         had nothing to grade. That is the agent author's \
                         declaration to make, not a check the platform skipped."
                    ),
                }
            } else {
                fermi::artifact_trace::Outcome::Graded {
                    fields: graded.len(),
                    violations: report.violations.len(),
                }
            };
        }
    }

    // What is queued about this episode's claims, and what settled.
    let verifications = sqlx::query(
        "SELECT assertion_id, verdict, actor, actor_kind::text AS actor_kind, \
                source_citation, evidence, created_at \
           FROM assertion_verifications \
          WHERE episode_id = $1 \
          ORDER BY created_at DESC",
    )
    .bind(episode_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let routed: Vec<Value> = verifications
        .iter()
        .map(|v| {
            json!({
                "assertion_id": v.try_get::<uuid::Uuid, _>("assertion_id").ok(),
                "verdict": v.try_get::<String, _>("verdict").ok(),
                "actor": v.try_get::<String, _>("actor").ok(),
                "actor_kind": v.try_get::<Option<String>, _>("actor_kind").ok().flatten(),
                "citation": v.try_get::<Option<String>, _>("source_citation").ok().flatten(),
                "evidence": v.try_get::<Option<Value>, _>("evidence").ok().flatten(),
                "at": v.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    let model_used: Option<String> = row.try_get("model_used").ok().flatten();
    Ok(Json(json!({
        "episode_id": episode_id,
        "parent_episode_id": row.try_get::<Option<uuid::Uuid>, _>("parent_episode_id").ok().flatten(),
        "agent": {
            "id": row.try_get::<uuid::Uuid, _>("agent_id").ok(),
            "name": agent_name,
        },
        "model": {
            "model_used": model_used,
            "provider_used": row.try_get::<Option<String>, _>("provider_used").ok().flatten(),
            "persona_version_at_write": row
                .try_get::<Option<i32>, _>("persona_version_at_write").ok().flatten(),
        },
        // `model_used IS NOT NULL` and not a test fixture. Both halves, though
        // the fixture filter currently removes nothing extra: every
        // `test_agent_*` episode already lacks `model_used`. Kept so that if a
        // fixture ever starts recording one, the corpus does not silently
        // acquire it.
        "corpus_eligible": model_used.is_some()
            && !fermi::declaration_ladder::is_test_cruft(&agent_name),
        "at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("timestamp_ref")
            .ok().map(|t| t.to_rfc3339()),
        "input": { "query": row.try_get::<String, _>("query").ok() },
        // Computed from the retained text, not stored. `query` and
        // `response_text` are both kept — the latter since migration 199,
        // deliberately — so a digest of them is a pure function of what the
        // platform already holds, and a computed one cannot drift from its
        // subject the way a stored one can.
        //
        // `enforced.as_ref()` and not `claimed_doc`: the third hash is over the
        // document AFTER the contract nulled what it refused, and the difference
        // between the two is precisely what grounding did. Handing the same value
        // twice would report that it changed nothing.
        "hashes": fermi::artifact_hash::of_episode(
            row.try_get::<String, _>("query").ok().as_deref(),
            response_text.as_deref(),
            enforced.as_ref(),
        ),
        "belt": belt,
        "fields": fields,
        "floor": floor,
        "floor_strength": fermi::grounding_trust::strength(floor),
        "routed": routed,
        "reading": reading.label(),
        "token": token,
        "silence": silence,
        "owner": owner,
        "declared": rungs_declared,
        "legibility": legibility,
        "caveats": fermi::artifact_trace::TRACE_CAVEATS,
        "contract": "The belt is DECLARED, from `command_registry`; only the \
                     grounding rung carries an outcome for this artifact, because \
                     `gate_decisions` has no `episode_id` and nothing else can be \
                     joined. `fields[].value` is what the agent actually claimed, \
                     never stripped. Read `floor_strength` rather than the token: \
                     `tool_no_match` and `unavailable_no_tool_source` are \
                     different words for the same amount of reliance. An empty \
                     `fields` is NOT a clean run - see `owner`. `hashes` are \
                     computed from retained text on every read, so they cannot \
                     drift from their subject; they do NOT support a seam check \
                     against a parent's output, because a delegated child \
                     receives a prompt built around its task rather than the \
                     parent's output verbatim.",
    })))
}

/// GET /api/declarations
///
/// What have the platform's agents declared about themselves, and who has to act
/// on the silence?
///
/// # Why this is a trust surface and not an admin report
///
/// Every other surface here reports `unknown` more often than anything else, and
/// until `fermi::declaration_ladder` existed the cause was one word. Measured: of
/// 206 agents that have produced an episode, **110 are `test_agent_*` rows
/// declaring nothing**, and of the 96 real ones 93 declare ports, 2 a checkable
/// schema and 7 a field contract. So the dominant cause of `unknown` across the
/// whole platform is *the subject declaring no structure to check against* —
/// which is neither a stalled loop, nor a cold counter, nor a contract the
/// platform failed to write.
///
/// That matters because it is a different backlog with a different owner.
/// `Unresolved` is ours; `Undeclared` is the agent author's. Collapsing them made
/// 89 real agents' missing declarations look like 89 contracts the platform owed,
/// and a backlog nobody can act on is one nobody does.
///
/// # The two worklists are served separately, on purpose
///
/// Pruning a `test_agent_<uuid>` row is a delete behind a safety gate.
/// Retrofitting `weather_oracle` is authoring work with someone who knows the
/// domain. Reported as one number the retrofit looks twice its size, and its
/// actual size is what decides whether it is worth doing.
///
/// # No target
///
/// Coverage is reported and never asserted against a figure. A threshold must be
/// a measurement or a two-way ratchet and never a target, and here even a ratchet
/// would fire on correct behaviour — new agents arrive undeclared by definition.
pub async fn list_declarations_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let rows = sqlx::query(fermi::declaration_ladder::CENSUS_SQL)
        .fetch_all(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // The three card-borne rungs come from the query; `field_contract` is owned
    // by `grounding_trust` and no SQL can see it. Asked here rather than
    // duplicated into the query, because a second answer to "does this agent have
    // a field contract" is the §3.4 violation and the const is the authority.
    let mut measured: Vec<(String, Vec<&'static str>)> = Vec::new();
    let mut retrofit: Vec<Value> = Vec::new();
    let mut prune = 0usize;

    for r in &rows {
        let Ok(name) = r.try_get::<String, _>("agent_name") else {
            continue;
        };
        let mut rungs: Vec<&'static str> = Vec::new();
        if r.try_get::<bool, _>("ports").unwrap_or(false) {
            rungs.push("ports");
        }
        if r.try_get::<bool, _>("output_type").unwrap_or(false) {
            rungs.push("output_type");
        }
        if r.try_get::<bool, _>("output_schema").unwrap_or(false) {
            rungs.push("output_schema");
        }
        if fermi::declaration_ladder::has_field_contract(&name) {
            rungs.push("field_contract");
        }

        let legibility = fermi::declaration_ladder::legibility(&rungs);
        match fermi::declaration_ladder::disposition(&name, &legibility) {
            fermi::declaration_ladder::Disposition::Prune => prune += 1,
            fermi::declaration_ladder::Disposition::Retrofit => {
                // The worklist item names the *cheapest* missing rung, because
                // the ladder is ordered by what it costs an author and telling
                // someone to write a field contract for an agent that has not
                // declared its ports is the most expensive step first.
                let silence = fermi::declaration_ladder::attribute(false, &legibility, 1);
                retrofit.push(json!({
                    "agent": name,
                    "legibility": legibility,
                    "next": silence,
                    "owner": fermi::declaration_ladder::whose_work(&silence),
                }));
            }
            fermi::declaration_ladder::Disposition::Legible => {}
        }
        measured.push((name, rungs));
    }

    let census = fermi::declaration_ladder::census(&measured);

    Ok(Json(json!({
        "census": census,
        // The ladder itself, served rather than hardcoded on the far side. Each
        // rung carries what it unlocks and what reads `unknown` without it, and
        // the second of those is the sentence a surface should show in place of a
        // blank panel.
        "ladder": fermi::declaration_ladder::LADDER,
        // Two lists, never one number.
        "retrofit": retrofit,
        "prune_count": prune,
        "contract": "`unknown` on every other surface here is USUALLY this: the \
                     agent declared no structure to check against. That is the \
                     author's work, not the platform's, and it is why \
                     `retrofit` and `prune_count` are separate — a delete behind \
                     a safety gate and an authoring task with a domain expert are \
                     different jobs. Coverage is reported and never compared to a \
                     target: new agents arrive undeclared by definition, so even a \
                     ratchet here would fire on correct behaviour.",
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
