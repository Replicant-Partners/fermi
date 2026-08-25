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
