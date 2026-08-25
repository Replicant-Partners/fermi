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

use fermi::loop_api::STAGE_ACTIONS;
use fermi::loop_model::LOOPS;
use fermi::panel_absence::Reading;

/// Is `path` declared as a route in this router source?
///
/// Extracted so the detector can be shown a known-bad input without a
/// filesystem. Matches the quoted path exactly: axum routes are written as
/// string literals, and matching unquoted would let `/api/loops` satisfy
/// `/api/loops/actions`.
fn router_declares(router_src: &str, path: &str) -> bool {
    router_src.contains(&format!("\"{path}\""))
}

/// The detector must see a path the router does not have.
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

/// Every door a UI is told to use must exist.
#[test]
fn every_declared_action_path_exists_in_the_router() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let router = std::fs::read_to_string(repo.join("src/api_server.rs"))
        .expect("src/api_server.rs is unreadable");

    // A scan over an empty corpus passes for ever.
    assert!(
        router.len() > 10_000 && router.contains(".route("),
        "src/api_server.rs does not look like the router ({} bytes)",
        router.len()
    );
    assert!(
        !STAGE_ACTIONS.is_empty(),
        "no action is declared, so this check has nothing to verify"
    );

    let missing: Vec<String> = STAGE_ACTIONS
        .iter()
        .filter(|a| !router_declares(&router, a.path))
        .map(|a| format!("{}.{} → {} {}", a.loop_id, a.stage, a.method, a.path))
        .collect();

    assert!(
        missing.is_empty(),
        "\n{} declared action(s) name a path the router does not:\n  {}\n\n\
         A UI builds its buttons from `loop_api::STAGE_ACTIONS`. A wrong path \
         renders a button that 404s after a reviewer believed they had acted, \
         and `hitl_actions` holds zero rows so there is no traffic whose \
         disappearance would say so.\n",
        missing.len(),
        missing.join("\n  ")
    );
    println!(
        "  {} declared door(s), every path present in the router.",
        STAGE_ACTIONS.len()
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
    for path in ["/api/loops", "/api/loops/actions", "/api/loops/:loop_id"] {
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
        "\n  {} loop(s): {} turning, {} stalled, {} unmeasured\n",
        tally.total, tally.turning, tally.stalled, tally.unmeasured
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
                    .map(|a| format!("  {} {}", a.method, a.path))
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
