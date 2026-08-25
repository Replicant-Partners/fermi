//! Where does each loop stop, and why?
//!
//! ```text
//! cargo test --test loop_model_contract -- --ignored --nocapture
//! ```
//!
//! Read-only; `every_stage_query_is_read_only` asserts that at the unit level.
//!
//! # What is asserted and what is only reported
//!
//! A stalled loop is **not** a test failure. A loop can be idle because nothing
//! has happened yet, and asserting otherwise would assert that anomalies must
//! occur, that forecasts must resolve, that owners must change their teams. That
//! is the same error as asserting on `anomaly_events`' row count.
//!
//! What **is** asserted is the subset of reasons that are facts about the code,
//! *and that are not already pinned somewhere cheaper*:
//!
//! * `writes_refused` — the writer runs and the database refuses it.
//! * `gate_refuses_everything` — a gate ran and approved nothing.
//!
//! Both are dynamic: they can begin at any deploy and no static check can see
//! them.
//!
//! # Why `no_trigger` is not asserted here
//!
//! It was, on the first run, and the suite went red on `loop3.intentions` — a
//! finding that is **already declared** in the model and **already pinned** by
//! `every_untriggered_stage_explains_itself`, which fixes the exact set and
//! insists it may only shrink.
//!
//! Asserting it twice buys nothing and costs the thing that matters: a suite
//! that is permanently red for a known state is a suite people stop reading,
//! and §5.2 is explicit that the deletion which follows will look like cleanup.
//! A static fault belongs to a static test. This tier is for what only a
//! database can see.
//!
//! `no_trigger` is still the loudest thing in the report — `NOTHING CALLS IT`,
//! in the trigger column — and
//! [`no_stage_declared_untriggered_has_started_producing`] holds the other
//! direction: a dead link that comes alive must lose its declaration.
//!
//! `awaiting_upstream` and `no_input` are reported and never asserted.

use fermi::loop_model::{evaluate, Trigger, LOOPS};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_where_every_loop_stops() {
    let pool = pool().await;
    let states = evaluate(&pool).await;

    let mut turning = 0usize;
    let mut code_faults: Vec<String> = Vec::new();
    let mut unrunnable: Vec<String> = Vec::new();

    for l in &states {
        println!("\n  {} — {} ({})", l.id, l.name, l.scope);
        for s in &l.stages {
            let mark = match (s.rows, l.stops_at == Some(s.id)) {
                (-1, _) => "?",
                (0, true) => "<",
                (0, false) => " ",
                _ => "+",
            };
            let trig = match s.trigger {
                Trigger::Request => "request".to_string(),
                Trigger::Scheduler { env, default_on } => {
                    format!("sweeper {env}{}", if default_on { "" } else { " (opt-in)" })
                }
                Trigger::Upstream => "upstream".to_string(),
                Trigger::Manual => "manual".to_string(),
                // Printed distinctly from `manual`: the button is manual, the
                // tool call is a model's decision, and a reader looking at a
                // zero needs to know which half is missing.
                Trigger::Prompted { .. } => "prompted".to_string(),
                Trigger::None { .. } => "NOTHING CALLS IT".to_string(),
            };
            println!(
                "   {mark} {:<16} {:>10}  {:<22} {}",
                s.id,
                if s.rows < 0 {
                    "unrunnable".to_string()
                } else {
                    s.rows.to_string()
                },
                trig,
                s.what
            );
            if s.rows < 0 {
                unrunnable.push(format!("{}.{}", l.id, s.id));
            }
        }
        match (l.stops_at, l.reason) {
            (None, _) => {
                turning += 1;
                println!("   -> turning");
            }
            (Some(at), Some(why)) => {
                println!("   -> stalled at `{at}`: {why}");
                if matches!(why, "writes_refused" | "gate_refuses_everything") {
                    code_faults.push(format!("{}.{at}: {why}\n         claim: {}", l.id, l.claim));
                }
            }
            (Some(at), None) => println!("   -> stalled at `{at}`"),
        }
    }

    println!(
        "\n  {turning} of {} loop(s) turning end to end.",
        states.len()
    );

    assert!(
        unrunnable.is_empty(),
        "\n{} stage count(s) could not run: {:?}\nAn unrunnable check reports \
         healthy for ever.\n",
        unrunnable.len(),
        unrunnable
    );

    assert!(
        code_faults.is_empty(),
        "\n{} loop(s) are stalled by something that started failing rather than \
         by something that has not happened:\n\n  {}\n\n\
         `writes_refused` means the writer runs and the database refuses it. \
         `gate_refuses_everything` means a gate ran and approved nothing. \
         Neither is a statement about whether anything has happened yet, and \
         neither is visible from the stage's row count — which reads zero for \
         both of them and for a perfectly healthy idle loop.\n",
        code_faults.len(),
        code_faults.join("\n\n  ")
    );
}

/// The model must be able to see something.
///
/// A chain walker over empty tables reports every loop stalled at stage one and
/// proves nothing. The same rule as the liveness positive controls: without a
/// known-good case, "all stalled" cannot be told apart from "the walker is
/// broken".
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn at_least_one_loop_turns_end_to_end() {
    let pool = pool().await;
    let states = evaluate(&pool).await;
    let turning: Vec<_> = states
        .iter()
        .filter(|l| l.stops_at.is_none())
        .map(|l| l.id)
        .collect();

    assert!(
        !turning.is_empty(),
        "no loop turns end to end, so this model has demonstrated nothing and \
         cannot distinguish a stalled platform from a broken walker. Every \
         stage count would have to be wrong in the same direction for this to \
         be a false alarm, which is exactly why it is worth asserting."
    );
    println!("  turning: {turning:?}");
}

/// A stage declared as having no caller must still have none.
///
/// The exemption discipline, applied to the model. `Trigger::None` is a
/// standing admission that a link is dead; if it starts producing, the
/// admission is stale and must be removed rather than left as a permanent
/// excuse nobody re-reads.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_stage_declared_untriggered_has_started_producing() {
    let pool = pool().await;
    let states = evaluate(&pool).await;

    let mut stale = Vec::new();
    for l in LOOPS {
        for s in l.stages {
            if !matches!(s.trigger, Trigger::None { .. }) {
                continue;
            }
            let rows = states
                .iter()
                .find(|st| st.id == l.id)
                .and_then(|st| st.stages.iter().find(|ss| ss.id == s.id))
                .map(|ss| ss.rows)
                .unwrap_or(0);
            if rows > 0 {
                stale.push(format!("{}.{} has {rows} row(s)", l.id, s.id));
            }
        }
    }

    assert!(
        stale.is_empty(),
        "\n{} stage(s) declared `Trigger::None` are producing:\n  {}\n\n\
         Something calls them now. Remove the declaration — a stale admission \
         that a link is dead will be believed the next time someone reads the \
         model.\n",
        stale.len(),
        stale.join("\n  ")
    );
}
