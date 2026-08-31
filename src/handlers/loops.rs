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
/// How much of the agent's answer the trace carries.
///
/// Generous, because the screen exists to let someone decide whether to trust
/// the document and a reader who has to leave to finish reading it will not
/// come back. Bounded anyway, because `response_text` is unbounded in the
/// database and this is the heaviest read on the platform.
///
/// Counted in characters and not bytes: the answers contain £, — and accented
/// club names, and slicing a `String` by byte offset panics mid-codepoint.
const RESPONSE_CHARS: usize = 20_000;

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
    let (fields, floor) = fermi::artifact_trace::fields(&agent_name, &graded);

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

    // Axis 1: is the AGENT on the substrate, and if not, whose work is it and
    // which worklist? Distinct from the belt, which is axis 2 and is about this
    // ARTIFACT. Keeping them apart is the whole separation: a rung that said
    // "not applicable, the author declared no field contract" forced a reader to
    // understand field contracts before they could read a checkpoint, and put an
    // agent-level backlog inside a per-artifact diagram.
    //
    // `Prune` is the one that most changes what a surface should do. 110 of 206
    // producing agents are `test_agent_*` fixtures awaiting deletion, not
    // retrofit targets; rendering them beside real agents makes the authoring
    // backlog look twice its true size and buries the agents worth fixing.
    let disposition = fermi::declaration_ladder::disposition(&agent_name, &legibility);
    let substrate_because = match disposition {
        fermi::declaration_ladder::Disposition::Prune =>
            "This is test cruft, not an agent anyone is going to declare. It is a              delete behind `/api/admin/agents/cleanup-test-cruft`'s safety gate,              not a retrofit target, and its belt is about a fixture."
                .to_string(),
        fermi::declaration_ladder::Disposition::Retrofit => format!(
            "`{agent_name}` is a real agent that has not been fully declared onto              the substrate, so the platform cannot say much about its output that              is not a row count. The grounding rung will read `undetermined` - the              check ran and had nothing to grade - which is a missing declaration              by the agent's author, not a check the platform skipped and not a              pass. This is authoring work, per agent, needing someone who knows              the domain."
        ),
        fermi::declaration_ladder::Disposition::Legible =>
            "Every rung on the declaration ladder is present, so every checkpoint              on the belt can say something specific about this artifact."
                .to_string(),
    };

    // The belt, with the grounding rung's recomputation filled in from this
    // episode.
    //
    // `agent.execute` is assumed, and that assumption is WRONG for a streamed
    // artifact -- it is disclosed in the payload rather than buried here. The
    // two routes declare different belts: `agent.execute` has four rungs,
    // `agent.execute_stream` has two (`credit` and `grounding`). An earlier
    // comment in this position claimed they declared the same rungs and that
    // either was therefore a correct answer. It was not true, and
    // `grounding_execute_coverage` only ever held them to both declaring
    // grounding.
    //
    // It is not fixable here: `episodes` carries no route discriminator, so the
    // route is not recoverable from the artifact. Serving the wider belt is the
    // deliberate choice of the two errors -- a streamed artifact shows
    // `attachment` and `input_binding` as rungs its route never had, which reads
    // as *unrecorded*, whereas serving the narrower belt would silently drop two
    // real checkpoints for the majority of artifacts, and a belt that omits
    // checkpoints looks shorter and safer than it is. Both are wrong; this one
    // is wrong in the direction that shows more rather than less, and
    // `belt_route.recoverable` tells the client not to trust it.
    let belt_command = "agent.execute";
    let mut belt = fermi::artifact_trace::belt(belt_command);

    // What the ledger recorded for THIS artifact. Migrations 220 and 221 exist
    // for this query: without it the column and the retention promotion have no
    // observable effect, which is exactly what the UX team reported.
    let ledger = sqlx::query(
        "SELECT id, gate::text AS gate, decision::text AS decision, reason, decided_at \
           FROM gate_decisions WHERE episode_id = $1",
    )
    .bind(episode_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    // When each recording gate FIRST recorded anything. The only evidence of when
    // a gate was promoted -- the date is not written down anywhere -- and it is
    // what separates "this artifact predates the promotion", which is permanent
    // and not a finding, from "the recorder dropped it", which is.
    let first_recorded: Vec<(String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as("SELECT gate::text, min(decided_at) FROM gate_decisions GROUP BY 1")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();

    let episode_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("timestamp_ref").ok();

    for r in belt.iter_mut() {
        // The ledger first: a recorded decision outranks anything derived.
        if let Some(hit) = ledger
            .iter()
            .find(|d| d.try_get::<String, _>("gate").ok().as_deref() == Some(r.rung))
        {
            r.decided = Some(fermi::artifact_trace::Decided {
                decision: hit.try_get::<String, _>("decision").unwrap_or_default(),
                reason: hit.try_get::<Option<String>, _>("reason").ok().flatten(),
                at: hit
                    .try_get::<chrono::DateTime<chrono::Utc>, _>("decided_at")
                    .ok()
                    .map(|t| t.to_rfc3339()),
                // So a reviewer can judge the decision from the artifact rather
                // than having to find it again in the gate list.
                decision_id: hit.try_get::<i64, _>("id").ok(),
            });
            // Invariant 2: exactly one of the two is set, so the absence the
            // registry filled in has to be cleared here rather than left beside
            // the verdict that supersedes it.
            r.decided_absent = None;
        } else {
            // No row. `belt()` has already decided the two reasons that need no
            // query; this narrows the remaining one by age.
            let first = first_recorded
                .iter()
                .find(|(g, _)| g == r.rung)
                .map(|(_, t)| *t);
            r.decided_absent = r
                .decided_absent
                .take()
                .map(|absent| fermi::artifact_trace::narrow_by_age(absent, episode_at, first));
        }

        // `recomputed` is a SIBLING of `outcome`, never merged into it. Both
        // reach the client unreconciled, because a recorded `approved` beside a
        // recomputed `2 violations` is the platform's only finding about its own
        // drift -- the contract having been tightened after the episode ran --
        // and any reconciliation here deletes it.
        //
        // There is deliberately no branch here for "the agent declares no field
        // contract". That was an `Outcome::NotApplicable` and it is gone: it
        // compensated for an empty ledger, and `execution.rs` already records
        // `Decision::Undetermined` for exactly that case, so the ledger answers
        // it honestly the moment the recorder has run. Keeping the branch would
        // have shadowed a real recorded verdict with a guess -- and made the
        // agent-level question ("is this agent on the substrate at all") into a
        // belt state, which is the mixing of axes the `substrate` block exists
        // to stop.
        if r.rung == "grounding" && !graded.is_empty() {
            r.recomputed = Some(fermi::artifact_trace::Recomputed {
                fields: graded.len(),
                violations: report.violations.len(),
            });
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
        // The answer, which this endpoint held and never served.
        //
        // `response_text` has been read here since the handler was written, to
        // compute the hashes and to re-run the contract, and then dropped. So
        // the one screen whose whole purpose is deciding whether to trust a
        // document could not show the document: its payload block rendered the
        // QUERY under the heading "The payload", and a comment beside the claim
        // values promised the full value was "one click away in the payload",
        // which was true of nothing on the page.
        //
        // Both forms, because the difference between them is what grounding did.
        // `text` is the bytes as the agent produced them, because retention is a
        // precondition for every later form of verification and a digest is not
        // a record. `document` is the JSON pulled out of those bytes, which is
        // what a reader can scan field by field. `document` is null for most
        // episodes: one agent on the platform answers in JSON most of the time,
        // and that absence is a finding rather than a gap here.
        //
        // Bounded, and it says so when it bounds. Unbounded would push a
        // megabyte of prose through the encoder on every read of the heaviest
        // screen on the platform; truncating silently would be worse than
        // either, because a reader deciding on a claim would be shown a
        // shortened document with nothing marking it short.
        "response": {
            "text": response_text.as_deref().map(|t| {
                t.chars().take(RESPONSE_CHARS).collect::<String>()
            }),
            "truncated": response_text
                .as_deref()
                .is_some_and(|t| t.chars().count() > RESPONSE_CHARS),
            "chars": response_text.as_deref().map(|t| t.chars().count()),
            "document": claimed_doc,
        },
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
        // Which command's belt this is, and the fact that we cannot know
        // whether it is the right one. Served rather than assumed: a surface
        // that draws four checkpoints for an artifact that passed two is making
        // a safety claim on the platform's behalf, and the client is entitled to
        // know the claim is unverified.
        "belt_route": {
            "assumed": belt_command,
            "recoverable": false,
            "because": "`episodes` records no route discriminator, so whether \
                        this artifact came from POST /execute or \
                        POST /execute/stream cannot be recovered. The two \
                        declare DIFFERENT belts - 4 rungs and 2 - so if this \
                        was a streamed artifact then `attachment` and \
                        `input_binding` are shown here and its route never had \
                        them. Fixing it needs a column on `episodes`, not a \
                        change to this handler.",
        },
        "fields": fields,
        "floor": floor,
        "floor_strength": fermi::grounding_trust::strength(floor),
        "routed": routed,
        "reading": reading.label(),
        "token": token,
        "silence": silence,
        "owner": owner,
        // Axis 1. One object, because these three answer one question and
        // serving them loose invited a client to read `legibility` without
        // `disposition` and put a fixture on somebody's worklist.
        "substrate": {
            "disposition": disposition,
            "legibility": legibility,
            "declared": rungs_declared,
            "because": substrate_because,
        },
        "caveats": fermi::artifact_trace::TRACE_CAVEATS,
        "contract": "TWO AXES, deliberately not mixed. `substrate` is about the \
                     AGENT - whether it has been declared onto the platform at \
                     all, and whether it is a retrofit target or a `prune`. \
                     `belt` is about THIS ARTIFACT. A legacy agent is not a \
                     degraded belt; it is an agent that is not on the substrate \
                     yet, and `substrate.disposition` is the field to branch on. \
                     \
                     The belt is DECLARED, from `command_registry`, and every \
                     declared rung always appears. On each rung EXACTLY ONE of \
                     `decided` and `decided_absent` is present - never both, \
                     never neither. `decided` is what the LEDGER recorded; \
                     `recomputed` is what re-running the contract says now, and \
                     they are separate fields on purpose - a recorded `approved` \
                     beside a recomputed `2 violations` means the contract was \
                     tightened after this episode ran, which is the only finding \
                     the platform has about its own drift, and it survives only \
                     unreconciled. Do not reconcile them. \
                     \
                     `decided.decision` is one of exactly three: `approved`, \
                     `refused`, `undetermined`. `undetermined` means the gate ran \
                     and COULD NOT DECIDE; it is never folded into either \
                     neighbour, and it is the expected reading of the grounding \
                     rung for any agent with no field contract. \
                     `decided.decision_id` makes \
                     POST /api/gates/:gate_id/decisions/:decision_id/review \
                     reachable from the artifact. \
                     \
                     `decided_absent.token` is a closed set of four: \
                     `fires_before_artifact` is permanent and nobody\'s work, \
                     `retention_counted` is a design choice, `predates_retention` \
                     is permanent for this artifact, and `retained_but_absent` is \
                     the only one that is a finding. \
                     `fields[].value` is what the agent actually claimed, \
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

/// GET /api/verification-queue
///
/// Claims awaiting a verdict, and what a reviewer can do about each.
///
/// # Why this is the last dependency of the rejection rate
///
/// *"An agent refuted four times in ten is measurably different from one refuted
/// twice in a hundred."* That number needs settled verdicts, and until something
/// could write one, **"nobody checked" and "checked and fine" rendered
/// identically.** `verification_queue::enqueue` fills the queue; this and the
/// settle below drain it.
pub async fn verification_queue_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Latest row per assertion, so a claim queued and then settled shows its
    // verdict rather than both events. `DISTINCT ON` over the append-only log is
    // the derived current state -- migration 205's pattern, and the reason the
    // earlier rows stay on file.
    let rows = sqlx::query(
        "SELECT DISTINCT ON (assertion_id) \
                assertion_id, episode_id, verdict, source_citation, actor, \
                actor_kind::text AS actor_kind, evidence, created_at \
           FROM assertion_verifications \
          ORDER BY assertion_id, created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut pending = 0usize;
    let mut settled = 0usize;
    let mut refuted = 0usize;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let verdict: String = r.try_get("verdict").unwrap_or_default();
            let is_pending = verdict.starts_with("pending_");
            if is_pending {
                pending += 1;
            } else {
                settled += 1;
            }
            if verdict == fermi::grounding_trust::PROV_REJECTED {
                refuted += 1;
            }
            json!({
                "assertion_id": r.try_get::<uuid::Uuid, _>("assertion_id").ok(),
                "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
                "verdict": verdict,
                "state": if is_pending { "pending" } else { "settled" },
                "citation": r.try_get::<Option<String>, _>("source_citation").ok().flatten(),
                "actor": r.try_get::<String, _>("actor").ok(),
                "actor_kind": r.try_get::<Option<String>, _>("actor_kind").ok().flatten(),
                "evidence": r.try_get::<Option<Value>, _>("evidence").ok().flatten(),
                "at": r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .ok().map(|t| t.to_rfc3339()),
            })
        })
        .collect();

    Ok(Json(json!({
        "items": items,
        "tally": { "pending": pending, "settled": settled, "refuted": refuted },
        // What a reviewer may write, served rather than hardcoded on the far
        // side. A client copying this list is copying a declaration instead of
        // inventing a parallel one.
        "settleable_verdicts": fermi::verification_queue::SETTLEABLE_BY_A_REVIEWER,
        "reading": if !items.is_empty() { "idle" } else { "unknown" },
        "detail": if !items.is_empty() {
            format!("{pending} awaiting a verdict, {settled} settled.")
        } else {
            "No claim has ever been queued for verification. The queue is filled \
             by the grounding contract at the execute boundary, and only a \
             contracted field produces an item - 10 of 96 real producing agents \
             declare a field contract, so this fills slowly and its emptiness is \
             about coverage rather than about the queue."
                .to_string()
        },
        "contract": "`state` is derived from the verdict, not stored: anything \
                     `pending_*` is awaiting a verdict and everything else is \
                     settled. The log is append-only, so this is the LATEST row \
                     per assertion and the earlier ones remain - two reviewers \
                     disagreeing about one claim is a disagreement, not a \
                     correction. `refuted` is the numerator of the rejection \
                     rate; the denominator is `settled`, and a rate without it is \
                     a lie at low volume.",
    })))
}

/// POST /api/verification-queue/:assertion_id/settle
///
/// Record a verdict on a queued claim. **Appends; never updates.**
///
/// # The citation rule is Postgres's
///
/// `human_sourced` requires a `source_citation`, enforced by migration 205's
/// `assertion_verifications_citation_check`. This handler does not re-check it:
/// two implementations of one trust rule is §3.4, and the end state is a Rust
/// guard narrower than the constraint with a 500 where the reviewer was told
/// their verdict had been recorded. `classify_settle_error` translates instead.
///
/// `human_sourced` scores as high as `tool_verified` in `grounding_trust::strength`
/// **because** someone else can follow the citation to the same source. The
/// citation is what earns the score, which is why it is enforced rather than
/// encouraged, and `human_endorsed` is the honest uncited alternative.
/// POST `/api/episodes/:episode_id/probe` — run the tool a field contract names.
///
/// The trace prints `call_football_api` beside a row and, until this existed,
/// offered no way to run it. A name the platform can print and cannot offer is a
/// description, not an affordance.
///
/// # It decides nothing
///
/// It runs the tool and returns what came out. The contract does not say **where
/// in the response** the value lives — `response_field` is prose as often as a
/// path — so comparing the answer to the claim automatically would be
/// string-matching dressed as verification. The platform performs the retrieval;
/// a person performs the comparison and records it through the settle form,
/// which is on the same row.
///
/// # The tool comes from the contract, never from the request
///
/// The caller names a **field**. The tool is looked up. A handler that ran the
/// tool the request asked for would be an authenticated outbound HTTP proxy
/// whose audit trail said "field verification".
///
/// The tool's *input* does come from the caller, and has to: `call_football_api`
/// needs a league id, a season and a team id, and those come from what the
/// episode was about rather than from the contract. That is a real limit and the
/// surface says so rather than pretending to know.
pub async fn probe_field_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(episode_id): Path<uuid::Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let path = body
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or((StatusCode::BAD_REQUEST, "`path` is required".to_string()))?;

    // The agent is read from the episode, so a caller cannot borrow one agent's
    // contract to run a tool against another's field.
    let agent_name: String = sqlx::query_scalar(
        "SELECT a.agent_name FROM episodes e \
         JOIN agents a ON a.agent_id = e.agent_id \
         WHERE e.episode_id = $1",
    )
    .bind(episode_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, format!("no episode {episode_id}")))?;

    let Some(tool) = fermi::field_probe::declared_tool(&agent_name, path) else {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "`{agent_name}` declares no tool that could settle `{path}`. A \
                 field with no tool in its contract is one only a person can \
                 settle, or one whose gap is a request for an integration that \
                 does not exist yet."
            ),
        ));
    };

    if !fermi::field_probe::is_runnable(tool) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "`{tool}` is the tool this field's contract names, and it cannot \
                 be run from here: it needs a workspace, a memory store or \
                 credentials of its own. Five of the sixteen tools named across \
                 the contracts are in that position."
            ),
        ));
    }

    let input = body.get("input").cloned().unwrap_or(json!({}));
    let probe = fermi::field_probe::run(tool, &input).await;

    Ok(Json(json!({
        "tool": probe.tool,
        "ok": probe.ok,
        "response": probe.response,
        "truncated": probe.truncated,
        "chars": probe.chars,
        "hint": fermi::field_probe::response_hint(&agent_name, path),
        // Stated in the payload, not only in this doc comment: a client that
        // read `ok: true` as "the field is verified" would be making exactly the
        // claim this endpoint refuses to make.
        "decides": "nothing. `ok` means the tool answered, not that the claim is \
                    true and not that this field is settled. Read the response, \
                    then record what you concluded through the settle form.",
    })))
}

pub async fn settle_verification_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(assertion_id): Path<uuid::Uuid>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let verdict = body
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    // Narrower than the column, and deliberately not in the database: the same
    // column is written by the platform's own enqueue with `pending_*`, so the
    // CHECK cannot express "what a PERSON may assert" without refusing the
    // writer that fills the queue.
    if !fermi::verification_queue::reviewer_may_write(&verdict) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "`{verdict}` is not a verdict a reviewer may write. Allowed: {:?}. \
                 The pending tier is what a claim is queued AS, so accepting it \
                 would let an item be resolved by re-queueing it; `tool_verified` \
                 and `derived` mean a tool call or a transform reproduces the \
                 value, which a person cannot assert by saying so.",
                fermi::verification_queue::SETTLEABLE_BY_A_REVIEWER
            ),
        ));
    }

    // `real_user_id`, not `user_id`. Under impersonation those differ and this
    // row is an audit record: attributing a verdict to the user an admin is
    // viewing as would let impersonation launder it.
    let actor = principal.real_user_id();
    let citation = body.get("source_citation").and_then(|v| v.as_str());

    let written = sqlx::query(fermi::verification_queue::SETTLE_SQL)
        .bind(assertion_id)
        .bind(&verdict)
        .bind(citation)
        .bind(&actor)
        .bind(fermi::seam_vocabulary::ActorKind::Human)
        .bind(body.get("evidence"))
        .fetch_optional(&state.db)
        .await;

    match written {
        // `fetch_optional` and not `fetch_one`: the INSERT selects from the
        // pending row, so no pending row means no row inserted and no error.
        // That is a 404 rather than a silent success, because settling a claim
        // nobody queued would put a verdict in the ledger with no record of what
        // was asked.
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            format!(
                "no queued claim for assertion {assertion_id}. A verdict can only \
                 settle something that was asked."
            ),
        )),
        Ok(Some(r)) => Ok(Json(json!({
            "verification_id": r.try_get::<uuid::Uuid, _>("verification_id").ok(),
            "assertion_id": assertion_id,
            "episode_id": r.try_get::<uuid::Uuid, _>("episode_id").ok(),
            "verdict": verdict,
            "actor": actor,
            "contract": "Appended, not updated. The earlier pending row remains, \
                         so this claim now reads as queued and then settled - and \
                         a second reviewer disagreeing appends again rather than \
                         overwriting you.",
        }))),
        Err(e) => {
            let constraint = e.as_database_error().and_then(|d| d.constraint());
            let refusal =
                fermi::verification_queue::classify_settle_error(constraint, &e.to_string());
            let status = if fermi::verification_queue::settle_is_client_error(&refusal) {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            let detail = match &refusal {
                fermi::verification_queue::SettleRefusal::CitationRequired => {
                    "`human_sourced` requires a `source_citation`. It scores as high \
                     as a tool call precisely BECAUSE someone else can follow the \
                     citation to the same source, so the citation is what earns the \
                     score. `human_endorsed` is the honest uncited alternative, at \
                     the strength of a model inference."
                        .to_string()
                }
                fermi::verification_queue::SettleRefusal::NotQueued => {
                    "That claim is not queued.".to_string()
                }
                fermi::verification_queue::SettleRefusal::UnknownVerdict => {
                    "The database refused a verdict the ladder declares, so \
                     `grounding_trust::PROVENANCE_VALUES` and migration 205's \
                     CHECK have drifted. A platform defect, not yours."
                        .to_string()
                }
                fermi::verification_queue::SettleRefusal::Rejected { error } => error.clone(),
            };
            Err((status, detail))
        }
    }
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
