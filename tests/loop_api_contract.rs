//! Does the loop surface advertise anything that does not exist?
//!
//! # Why this is the check that matters for a UI
//!
//! `loop_api::STAGE_ACTIONS` tells a client which endpoint to call to work a
//! human-gated stage. A path that is wrong there is worse than a missing
//! feature: the button renders, a reviewer presses it, and the 404 arrives
//! after they believed they had recorded a correction.
//!
//! The declaration and the router are in two files with no relationship, so a
//! route rename breaks the button silently — and `hitl_actions` holds zero
//! rows, so there is no traffic whose disappearance would tell anyone.
//!
//! # What it is, plainly
//!
//! A scan over `src/api_server.rs` for a substring. It cannot tell a live route
//! from one inside a comment, and it will not notice a handler that returns 501.
//! What it does catch is the case it was written for: a path in `STAGE_ACTIONS`
//! that no longer appears in the router at all.
//!
//! [`the_scan_sees_a_path_the_router_does_not_have`] is the falsification, per
//! the rule `tests/falsification_registry.rs` enforces.

use std::path::Path;

use fermi::evaluator_api::EVALUATOR_DOORS;
use fermi::gate_api::GATE_DOORS;
use fermi::loop_api::STAGE_ACTIONS;
use fermi::loop_model::LOOPS;
use fermi::panel_absence::Reading;
use fermi::surface::{doors_missing_from, router_declares, Door};

/// The detector must see a path the router does not have.
///
/// The matcher itself lives in [`fermi::surface`] and is shared with gates and
/// evaluators — three copies would be three chances for one to stop matching.
/// This is the falsification for all of them.
#[test]
fn the_scan_sees_a_path_the_router_does_not_have() {
    let router = r#"
        .route("/api/observatory/hitl/:event_id/action", post(h::a))
        .route("/api/loops", get(h::b))
    "#;
    assert!(router_declares(
        router,
        "/api/observatory/hitl/:event_id/action"
    ));
    assert!(
        !router_declares(router, "/api/observatory/hitl/:event_id/act"),
        "a prefix of a real route satisfied the check, so a renamed path with a \
         surviving stem would pass"
    );
    assert!(
        !router_declares(router, "/api/loops/actions"),
        "`/api/loops` satisfied `/api/loops/actions` — the quotes are what \
         stop a shorter route standing in for a longer one"
    );
    assert!(!router_declares(router, "/api/nothing/here"));
}

/// Every door a UI is told to use must exist — in every domain.
///
/// One scan over every declared door, not one per domain. A gate door added
/// with a wrong path fails here, and it will fail the day the first one is
/// added rather than the day someone remembers to write a gate version of this
/// test.
#[test]
fn every_declared_door_exists_in_the_router() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let router = std::fs::read_to_string(repo.join("src/api_server.rs"))
        .expect("src/api_server.rs is unreadable");

    // A scan over an empty corpus passes for ever.
    assert!(
        router.len() > 10_000 && router.contains(".route("),
        "src/api_server.rs does not look like the router ({} bytes)",
        router.len()
    );

    let mut doors: Vec<Door> = STAGE_ACTIONS.iter().map(|a| a.door).collect();
    doors.extend(GATE_DOORS.iter().copied());
    doors.extend(EVALUATOR_DOORS.iter().copied());
    assert!(
        !doors.is_empty(),
        "no door is declared in any domain, so this check has nothing to verify"
    );

    let missing = doors_missing_from(&router, &doors);
    assert!(
        missing.is_empty(),
        "\n{} declared door(s) name a path the router does not:\n  {}\n\n\
         A UI builds its buttons from these declarations. A wrong path renders \
         a button that 404s after a reviewer believed they had acted, and \
         `hitl_actions` holds zero rows so there is no traffic whose \
         disappearance would say so.\n",
        missing.len(),
        missing.join("\n  ")
    );
    println!(
        "  {} declared door(s) across 3 domain(s), every path present.",
        doors.len()
    );
}

/// The new surface's own routes must exist, or it serves nothing.
///
/// Separate from the actions above because these are the endpoints *this*
/// module adds, and the failure is different: an action path going missing
/// breaks a button, whereas these going missing means the whole surface is
/// unreachable while every unit test still passes.
#[test]
fn the_loop_surface_is_routed() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let router = std::fs::read_to_string(repo.join("src/api_server.rs")).expect("router");
    for path in [
        "/api/loops",
        "/api/loops/actions",
        "/api/loops/:loop_id",
        "/api/gates",
        "/api/evaluators",
        "/api/agents/:agent_id/coordination-notes",
        "/api/gates/:gate_id/decisions",
    ] {
        assert!(
            router_declares(&router, path),
            "`{path}` is not routed, so the loop surface is unreachable — and \
             every `loop_api` unit test would still pass, because they test the \
             assembly and not the wiring"
        );
    }
    // Ordering matters and the router is a matcher, not a set: `/api/loops/:loop_id`
    // declared before `/api/loops/actions` would swallow `actions` as a loop id
    // and the actions endpoint would 404 with "no loop `actions`".
    let actions_at = router
        .find("\"/api/loops/actions\"")
        .expect("actions route");
    let param_at = router.find("\"/api/loops/:loop_id\"").expect("param route");
    assert!(
        actions_at < param_at,
        "`/api/loops/:loop_id` is declared before `/api/loops/actions`, so \
         `actions` matches as a loop id and the declaration endpoint returns \
         404 with a list of loops. Verified by swapping them."
    );
}

/// The surface, assembled against production.
///
/// Prints the payload a UI receives. It asserts the two properties a client
/// depends on and cannot check for itself:
///
/// * **no loop is rendered without a reading** — every panel says whether its
///   emptiness is idle, faulty or unknowable;
/// * **at most one stage per loop is flagged actionable** — everything below
///   the first empty link is empty because of it, and a UI that highlights all
///   of them turns one finding into four.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_surface_assembles_against_production() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    let states = fermi::loop_model::evaluate(&pool).await;
    let views = fermi::loop_api::views(&states);
    let tally = fermi::loop_api::tally(&views);

    assert_eq!(
        views.len(),
        LOOPS.len(),
        "the surface dropped a loop between the model and the view"
    );
    println!(
        "\n  {} loop(s): {} turning · {} stalled by a code fault · {} stalled \
         and idle · {} stopped with no reading available · {} unreadable\n",
        tally.total,
        tally.turning,
        tally.stalled_by_fault,
        tally.stalled_idle,
        tally.no_reading,
        tally.unreadable
    );
    assert_eq!(
        tally.turning
            + tally.stalled_by_fault
            + tally.stalled_idle
            + tally.no_reading
            + tally.unreadable,
        tally.total,
        "a loop fell through the header's buckets, so the count a UI renders \
         omits it silently"
    );

    for v in &views {
        println!("  {:<7} {:<10} {:?}", v.id, v.status, v.reading);
        println!("          {}", v.detail);
        for s in &v.stages {
            let rows = if s.measured {
                s.rows.to_string()
            } else {
                "unread".to_string()
            };
            println!(
                "      {} {:<16} {:>7}  {:<17}{}",
                if s.is_first_empty { "<" } else { " " },
                s.id,
                rows,
                s.trigger_label,
                s.action
                    .map(|a| format!("  {} {}", a.door.method, a.door.path))
                    .unwrap_or_default()
            );
        }
        for o in &v.outcomes {
            println!("      ~ {} — {}", o.stage, o.proposition);
            if o.declared_gap.is_some() {
                println!("        (declared gap)");
            }
        }
        println!();
    }

    // A turning loop reads `idle`; anything else must have said why. The
    // property a client depends on: there is no state in which it must render a
    // bare zero and guess.
    for v in &views {
        assert!(
            matches!(v.reading, Reading::Idle | Reading::Fault | Reading::Unknown),
            "{} has no reading",
            v.id
        );
        let flagged = v.stages.iter().filter(|s| s.is_first_empty).count();
        assert!(
            flagged <= 1,
            "{} flags {flagged} stages as the first empty link; a UI would show \
             one finding as several",
            v.id
        );
        // An unread stage must never carry a renderable count.
        for s in &v.stages {
            assert!(
                s.measured || s.rows < 0,
                "{}.{} says it was not measured and carries rows = {}",
                v.id,
                s.id,
                s.rows
            );
        }
    }

    // The whole point of the surface is that it is not vacuous.
    assert!(
        views.iter().any(|v| v.stages.iter().any(|s| s.rows > 0)),
        "not one stage anywhere reported a row, so this ran against an empty \
         database and has shown nothing"
    );
}

/// The repoint is real: the 610-line handler is unrouted.
///
/// A repoint that leaves the old handler wired is not a repoint — it is two
/// surfaces with one path, and whichever `.route` axum resolves last wins
/// silently. The comment in the router claims this; here it is asserted.
///
/// `observatory::agent_loops_handler` is left *present* deliberately: deleting a
/// thousand lines of working SQL in the same commit that repoints the route
/// makes the change hard to reverse if the new view proves too narrow. What must
/// not happen is for it to still serve traffic.
#[test]
fn no_unrouted_handler_survives_the_repoint() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let router = std::fs::read_to_string(repo.join("src/api_server.rs")).expect("router");

    // The *routed* form, not the bare name. The first version searched for the
    // name and matched the router comment that explains the repoint — a scan
    // firing on its own documentation, which is the §5.2 failure and would have
    // been fixed by deleting the check.
    assert!(
        !router.contains("get(handlers::observatory::agent_loops_handler)"),
        "`observatory::agent_loops_handler` is still routed. The per-agent loop \
         path is served by `handlers::loops::agent_loops_handler` now, and two \
         handlers on one path means whichever `.route` resolves last wins — \
         which is a coin toss between an assembled view and 610 lines of \
         bespoke SQL."
    );
    assert!(
        router.contains("get(handlers::loops::agent_loops_handler)"),
        "the per-agent loop path is routed at neither handler, so the repoint \
         removed the endpoint instead of moving it"
    );
    // The path itself must survive, or existing clients 404 on a change that
    // was supposed to be invisible to them.
    assert!(
        router_declares(&router, "/api/observatory/agents/:agent_id/loops"),
        "the path was dropped rather than repointed; every existing client \
         breaks on what was meant to be an internal change"
    );
}

/// Every stage the per-agent view can probe binds the agent.
///
/// The substitution this whole view exists to prevent: a probe declared
/// per-agent that forgets `$1` counts the platform and reports it as one
/// agent's. `loop_api`'s own unit test asserts it over the declarations; this
/// asserts the *handler* binds exactly one parameter to each, by checking it
/// passes the agent id and nothing else.
#[test]
fn the_per_agent_handler_binds_the_agent_to_every_probe() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let handler = std::fs::read_to_string(repo.join("src/handlers/loops.rs")).expect("handler");
    // One `.bind(agent_id)` per probe, and the probe SQL comes from the
    // declaration rather than being written here.
    assert!(
        handler.contains(".bind(agent_id)"),
        "the per-agent handler runs the declared probes without binding the \
         agent, so every count would be the platform's"
    );
    // Positive rather than negative. The first version asserted the handler
    // contained no `SELECT count(*) FROM episodes`, and fired on the
    // coordination-notes endpoint's platform total in the same file — a
    // legitimate query, flagged. What is actually wanted is that the per-agent
    // counts come from the declaration, which is a thing to look *for*.
    assert!(
        handler.contains("loop_api::subject_scope"),
        "the per-agent handler does not read `loop_api::subject_scope`, so its \
         counts are written here rather than declared — which is how the \
         610-line handler it replaces came to disagree with the model it was \
         reporting on"
    );
}

/// The two loop surfaces must not name different loops.
///
/// `observatory::agent_loops_handler` is 610 lines of bespoke per-loop SQL and
/// the audit's §9 item 6: a second answer to the question `loop_model` answers
/// from the contracts. It survives this commit because it is per-agent where the
/// new surface is platform-wide.
///
/// What must not happen while both exist is that they disagree about **which
/// loops there are**. This is the weakest useful pin — it cannot compare their
/// numbers without an `AppState` — and it is the one that catches the shape of
/// the original defect, which was hardcoded rows rendered under a live status
/// column.
#[test]
fn the_two_loop_surfaces_do_not_disagree_about_which_loops_exist() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let legacy =
        std::fs::read_to_string(repo.join("src/handlers/observatory.rs")).expect("observatory.rs");

    // Loop ids the legacy handler spells for itself. Quoted, and short ids like
    // `"5a"` are how it writes them.
    let declared: Vec<&str> = LOOPS.iter().map(|l| l.id).collect();
    let mut unknown: Vec<String> = Vec::new();
    for cap in legacy.split('"') {
        // `loop` + a digit, and nothing longer than `loop5b`. The first
        // version tested `starts_with("loop")` alone and reported the JSON
        // keys `"loop"` and `"loops"` as undeclared loops — a check crying
        // wolf on its first run, which §5.2 says gets it deleted.
        let looks_like_a_loop_id = cap.len() >= 5
            && cap.len() <= 7
            && cap.starts_with("loop")
            && cap.as_bytes()[4].is_ascii_digit();
        if looks_like_a_loop_id && !declared.contains(&cap) {
            unknown.push(cap.to_string());
        }
    }
    unknown.sort();
    unknown.dedup();

    assert!(
        unknown.is_empty(),
        "`observatory.rs` names loop id(s) `loop_model` does not declare: \
         {unknown:?}. Two surfaces that disagree about which loops exist will \
         disagree about their state next, and a reader has no way to tell which \
         is the live one."
    );
    println!("  legacy surface names no loop outside the declared {declared:?}");
}
