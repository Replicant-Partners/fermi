//! Does what a loop produces carry the signal its claim needs?
//!
//! Run with:
//! ```text
//! cargo test --test outcome_contract -- --ignored --nocapture
//! ```
//!
//! Read-only. Every query is a `SELECT` from
//! [`fermi::outcome_trust::OUTCOME_CONTRACTS`], asserted read-only at the unit
//! level by `every_query_is_read_only`.
//!
//! # What each failure means
//!
//! * **`Uniform`** — subjects sharing an event never take different values, so
//!   the metric is about the *event* however it is named. This is the finding
//!   the module was written for and Loop 5.A is in it: the forecast's Brier,
//!   written once per name in `agents_used`, identical every time.
//! * **`Sparse`** — plenty of events, almost no variation. Evidence *for*
//!   uniformity, which is what separates it from `Underpowered`.
//! * **`Conflated`** — more than one producer writes the metric and nothing
//!   compares them. Loop 5.A again: one writes per resolved forecast and
//!   another an aggregate over N forecasts, both into
//!   `dimension = 'forecast_calibration'`.
//! * **`Underpowered` / `NoSharedEvents`** — reported, never asserted. Neither
//!   is a pass and neither is a defect; they are the states in which no reading
//!   is available, and calling either a verdict is the error this module
//!   inherited its vocabulary for.
//!
//! # Declared gaps
//!
//! Some findings are real and cannot be closed this week. They go in
//! [`fermi::outcome_trust::KNOWN_GAPS`] with what would clear them, are
//! reported rather than asserted, and are separately asserted to be **still
//! open** by [`every_declared_gap_is_still_open`] — so an entry cannot outlive
//! its reason. Same instrument as `liveness_trust::KNOWN_SILENT`, and for the
//! same reason: a suite permanently red for a known state is a suite people
//! stop reading.

use fermi::outcome_trust::{
    classify_discrimination, classify_producers, known_gap, shared_metric, Discrimination,
    EventSpread, Producers, KNOWN_GAPS, OUTCOME_CONTRACTS,
};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

async fn spread(pool: &PgPool, sql: &str) -> Vec<EventSpread> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(sql)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    rows.iter()
        .map(|(subjects, distinct)| EventSpread {
            subjects: *subjects as usize,
            distinct_values: *distinct as usize,
        })
        .collect()
}

/// Can each metric tell its subjects apart?
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_outcome_metric_can_tell_its_subjects_apart() {
    let pool = pool().await;
    let mut findings: Vec<String> = Vec::new();
    let mut read = 0usize;

    for c in OUTCOME_CONTRACTS {
        let label = format!("{}.{}", c.loop_id, c.stage);
        let spreads = spread(&pool, c.spread_sql).await;
        if spreads.is_empty() {
            println!("\n  {label}: the spread query returned nothing.");
            continue;
        }
        read += 1;

        let verdict = classify_discrimination(&spreads, c.min_events);
        println!("\n  {label}");
        println!("    claim       {}", c.claim);
        println!("    proposition {}", c.proposition);
        println!(
            "    events      {} ({} with >1 subject)",
            spreads.len(),
            spreads.iter().filter(|s| s.subjects > 1).count()
        );
        println!("    verdict     {verdict:?}");

        if verdict.is_finding() {
            if let Some(g) = known_gap(&label, "uniform") {
                println!("    DECLARED    cleared by: {}", g.cleared_by);
                continue;
            }
            findings.push(format!(
                "{label}: {verdict:?}\n         proposition: {}\n         cost: {}",
                c.proposition, c.why
            ));
        }
    }

    // A suite over an empty set passes for ever.
    assert!(
        read > 0,
        "not one spread query returned a row, so this check has demonstrated \
         nothing"
    );

    assert!(
        findings.is_empty(),
        "\n{} metric(s) cannot distinguish the subjects they are named for:\n\n  {}\n\n\
         A loop can be `turning` in `loop_model` and produce a number with no \
         subject-level information in it. Downstream readers — the MoE router \
         at Stage 0, composition evolution — cannot be reading agent skill from \
         such a metric whatever they believe, and a uniform number invites no \
         question the way a missing one would.\n\n\
         If it cannot be fixed now, declare it in `outcome_trust::KNOWN_GAPS` \
         with what would clear it — and expect `every_declared_gap_is_still_open` \
         to insist it is still there.\n",
        findings.len(),
        findings.join("\n\n  ")
    );
}

/// One metric, one producer — or a declared reason.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_outcome_metric_has_one_producer_or_a_stated_reason() {
    let pool = pool().await;
    let mut findings: Vec<String> = Vec::new();

    for c in OUTCOME_CONTRACTS {
        let label = format!("{}.{}", c.loop_id, c.stage);
        let rows: Vec<(String, i64)> = match sqlx::query_as(c.producer_sql).fetch_all(&pool).await {
            Ok(r) => r,
            Err(e) => {
                findings.push(format!("{label}: producer query could not run — {e}"));
                continue;
            }
        };

        println!("\n  {label} — {} producer(s)", rows.len());
        for (p, n) in &rows {
            println!("    {n:>6}  {p}");
        }

        let declared_ok = shared_metric(c.stage).is_some();
        match classify_producers(rows.len(), declared_ok) {
            Producers::Conflated { producers } => {
                if let Some(g) = known_gap(&label, "conflated") {
                    println!("    DECLARED    cleared by: {}", g.cleared_by);
                    continue;
                }
                findings.push(format!(
                    "{label}: {producers} undeclared producers of one metric:\n         {}\n         \
                     Nothing compares them, and they need not compute the same \
                     thing over the same denominator — a reader that averages \
                     the column is averaging incomparable numbers with equal \
                     weight.\n         cost: {}",
                    rows.iter()
                        .map(|(p, n)| format!("{p} ({n})"))
                        .collect::<Vec<_>>()
                        .join("\n         "),
                    c.why
                ))
            }
            // `liveness_trust`'s question, not this one. Reported so the spread
            // verdict is not read as meaningful over an empty set.
            Producers::None => {
                println!("    (nothing writes it — that is the liveness rung's finding)")
            }
            Producers::Single => {}
        }
    }

    assert!(
        findings.is_empty(),
        "\n{} metric(s) have more than one undeclared producer:\n\n  {}\n",
        findings.len(),
        findings.join("\n\n  ")
    );
}

/// A declared gap must still be a gap.
///
/// The half of the baseline that makes it a ratchet rather than a permission.
/// `KNOWN_SILENT` held one entry whose own reason named the condition for its
/// removal, and the first live run that met the condition failed — so the
/// exemption could not outlive it. Same instrument.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_declared_gap_is_still_open() {
    let pool = pool().await;
    let mut closed: Vec<String> = Vec::new();

    for g in KNOWN_GAPS {
        let Some(c) = OUTCOME_CONTRACTS
            .iter()
            .find(|c| format!("{}.{}", c.loop_id, c.stage) == g.metric)
        else {
            closed.push(format!("{}: names no declared contract", g.metric));
            continue;
        };

        let still_open = match g.gap {
            "uniform" => {
                let v = classify_discrimination(&spread(&pool, c.spread_sql).await, c.min_events);
                println!("  {} uniform    -> {v:?}", g.metric);
                // Open while it is a finding. `Underpowered` counts as open
                // too: the gap has not been shown to have closed, and removing
                // the entry on an unavailable reading would be the same
                // positive-claim-on-no-evidence this module exists to prevent.
                v.is_finding() || !v.is_reading()
            }
            "conflated" => {
                let rows: Vec<(String, i64)> = sqlx::query_as(c.producer_sql)
                    .fetch_all(&pool)
                    .await
                    .unwrap_or_default();
                println!("  {} conflated  -> {} producer(s)", g.metric, rows.len());
                rows.len() > 1
            }
            other => {
                closed.push(format!("{}: unknown gap kind `{other}`", g.metric));
                continue;
            }
        };

        if !still_open {
            closed.push(format!(
                "{} ({}): the gap has closed. Remove the entry — the list may \
                 only shrink, and this is the run it was waiting for.\n      \
                 cleared_by said: {}",
                g.metric, g.gap, g.cleared_by
            ));
        }
    }

    assert!(
        closed.is_empty(),
        "\n{} declared gap(s) are no longer gaps:\n\n  {}\n",
        closed.len(),
        closed.join("\n\n  ")
    );
    println!("\n  {} declared gap(s), all still open.", KNOWN_GAPS.len());
}
