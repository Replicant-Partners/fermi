//! The native evaluators, run against production.
//!
//! ```text
//! cargo test --test native_evaluator_contract -- --ignored --nocapture
//! ```
//!
//! Read-only.
//!
//! # What this tier adds
//!
//! The unit tests in [`fermi::native_evaluators`] prove each evaluator fires on
//! a world built to break it. That is §5.1, and it is not the same as proving
//! the evaluators say anything useful about *this* system.
//!
//! Two things can only be checked here:
//!
//! * the observation actually collects — a snapshot that silently comes back
//!   empty would make every evaluator `Inconclusive`, which is not a pass but is
//!   quiet;
//! * the verdicts about production are the ones a person should act on.
//!
//! # Only `Critical` fails here, and that is a rule about ownership
//!
//! Every finding should be asserted by exactly one tier. Assert it in three and
//! the suite goes red three times for one state, and §5.2 is explicit about
//! what happens next.
//!
//! | severity | means | asserted by |
//! |---|---|---|
//! | `Critical` | a control is inverted **right now** — a writer refused every time, a gate approving nothing | here; nothing else can see it |
//! | `Warning` | a known structural gap | the tier that owns it — `loop_model`'s static pin, the liveness script's `silent` assertion |
//! | `Notice` | reported, asserts nothing | nobody |
//!
//! So a `Warning` is printed here in full, with its remedy, and does not fail:
//! it is already pinned somewhere that fails faster and cheaper. This tier
//! exists for the states that only appear at runtime.
//!
//! The first version asserted `Warning` too and went red on
//! `loop3.intentions: no_trigger` — pinned by
//! `loop_model::every_untriggered_stage_explains_itself`, which fixes the exact
//! set and insists it may only shrink. The same mistake as the loop tier made
//! one step earlier, which is why it is written down as a rule here rather than
//! fixed a second time in silence.

use fermi::native_evaluators::{run, Observation, Severity, Verdict};
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
async fn the_machinery_scores_itself() {
    let pool = pool().await;

    // The liveness report lives in a process-local cell and is populated by the
    // hourly sweeper, which has not run in a test process. Sweeping here keeps
    // `undocumented_silence` and `positive_control` from being `Inconclusive`
    // for a reason that is an artefact of the harness rather than of the
    // system.
    let report = fermi::liveness_trust::sweep(&pool).await;
    fermi::liveness_trust::record_latest(report);

    let o = Observation::collect(&pool).await;
    assert!(
        !o.writes.is_empty() && !o.gates.is_empty() && !o.loops.is_empty(),
        "the observation came back empty, which would make every evaluator \
         `Inconclusive` — not a pass, but quiet. writes={} gates={} loops={}",
        o.writes.len(),
        o.gates.len(),
        o.loops.len()
    );

    let r = run(&o);
    println!("\n  native evaluators — status: {}\n", r.status);
    let mut failing: Vec<String> = Vec::new();

    for s in &r.scored {
        match &s.verdict {
            Verdict::Healthy { detail } => println!("  [ok]      {:<26} {detail}", s.id),
            Verdict::Inconclusive { why } => {
                println!("  [?]       {:<26} {why}", s.id)
            }
            Verdict::Finding {
                severity,
                detail,
                subjects,
                remedy,
            } => {
                let tag = match severity {
                    Severity::Notice => "[note]",
                    Severity::Warning => "[WARN]",
                    Severity::Critical => "[CRIT]",
                };
                println!("  {tag:<9} {:<26} {detail}", s.id);
                for sub in subjects {
                    println!("            · {sub}");
                }
                if *severity == Severity::Critical {
                    failing.push(format!(
                        "{}: {detail}\n         {:?}\n         {remedy}",
                        s.id, subjects
                    ));
                }
            }
        }
    }

    println!(
        "\n  {} healthy · {} notice · {} finding · {} inconclusive",
        r.healthy, r.notices, r.findings, r.inconclusive
    );

    // A registry that concluded nothing must not pass. Same rule as the
    // liveness positive control, one level up.
    assert_ne!(
        r.status, "inconclusive",
        "every evaluator declined to answer, so this suite has demonstrated \
         nothing about the machinery it exists to score."
    );

    assert!(
        failing.is_empty(),
        "\n{} CRITICAL native finding(s) — a control is inverted right now:\n\n  {}\n",
        failing.len(),
        failing.join("\n\n  ")
    );
}

/// Every evaluator must have something to look at.
///
/// A `Healthy` verdict reached over an empty input is the shape of check this
/// whole audit is about. Reported rather than asserted, because
/// `Inconclusive` is already not a pass and forcing all six to be conclusive
/// would fail the suite on a freshly booted process for no fault of the
/// system's.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_which_evaluators_had_nothing_to_look_at() {
    let pool = pool().await;
    let o = Observation::collect(&pool).await;
    let r = run(&o);

    let quiet: Vec<_> = r
        .scored
        .iter()
        .filter(|s| matches!(s.verdict, Verdict::Inconclusive { .. }))
        .map(|s| s.id)
        .collect();

    if quiet.is_empty() {
        println!("  every native evaluator had data to work with.");
    } else {
        println!(
            "  {} evaluator(s) could not conclude: {quiet:?}\n  \
             In a fresh process the counters start at zero, so this is expected \
             immediately after a deploy and is a finding if it persists.",
            quiet.len()
        );
    }
}
