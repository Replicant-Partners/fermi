//! Is any migration currently failing, and would we know?
//!
//! # The gap this closes
//!
//! `run_migrations` replays every registered file on every boot, logs failures
//! with `eprintln!`, and continues. That continue is correct — most failures
//! here are benign replays of already-applied DDL, and a migration able to take
//! the service down would be a worse problem. What was missing is that nothing
//! was **written down**.
//!
//! `credit_ledger_tx_type_check` is the cost of that. Seventeen migrations
//! declared it. Each dropped the constraint, failed to re-add it because rows
//! already violated the new list, and left one line in a boot log. Three of
//! those migrations exist for no purpose other than repairing it, so the repair
//! was performed, believed, and repeated for the life of the project — while the
//! net effect of every attempt was to *delete* the thing being repaired.
//!
//! A failure that is only ever printed is a failure nobody can be asked about.
//! The ledger makes it a row, and this is the query that would have caught it on
//! the first boot.
//!
//! # The second thing the ledger unblocks
//!
//! `first_succeeded_at` is the field the rest of the verification work has been
//! missing. `liveness_trust` asks whether a write path ever ran, and answers it
//! by comparing rows in a sink against the number of opportunities the writer
//! had. With no record of when a migration landed, "opportunity" can only mean
//! *all time* — so every newly deployed writer reports as broken, because
//! history is always full of chances it could not have taken. That is currently
//! handled by a documented exemption. Once this ledger has data, the opportunity
//! window can start at the deploy and the exemption can go.
//!
//! Run with: `cargo test --test migration_ledger -- --ignored --nocapture`

use sqlx::{PgPool, Row};

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

async fn ledger_exists(pool: &PgPool) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT count(*)::bigint FROM information_schema.tables \
          WHERE table_name = 'schema_migrations'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// Every file `run_migrations` registers, read out of the source.
///
/// Parsed rather than restated, so the two cannot drift. A migration that is
/// registered but never recorded, or recorded but no longer registered, is a
/// discrepancy worth seeing.
fn registered_migrations() -> Vec<String> {
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/api_server.rs"),
    )
    .expect("api_server.rs");
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("\"migrations/") {
            if let Some(name) = rest.split('"').next() {
                out.push(format!("migrations/{name}"));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[test]
fn the_registered_list_is_parseable_and_non_trivial() {
    // If the scan stops matching the code it is meant to police, it would go
    // quietly green while enforcing nothing.
    let files = registered_migrations();
    assert!(
        files.len() > 50,
        "only {} migration(s) found in run_migrations — the scan has lost its \
         target",
        files.len()
    );
    assert!(files.iter().all(|f| f.ends_with(".sql")));
    assert!(
        files
            .iter()
            .any(|f| f.contains("204_restore_credit_ledger")),
        "expected the tx_type restoration among the registered files"
    );
}

/// Each registered migration must exist on disk.
///
/// Cheap, offline, and it catches a real class: a file renamed or deleted while
/// its registration stays. At runtime that surfaces as `unreadable`, meaning the
/// deploy is not carrying what the code believes it is.
#[test]
fn every_registered_migration_exists_on_disk() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let missing: Vec<String> = registered_migrations()
        .into_iter()
        .filter(|f| !root.join(f).exists())
        .collect();
    assert!(
        missing.is_empty(),
        "registered in run_migrations but absent from the repository:\n  {}",
        missing.join("\n  ")
    );
}

// ─── live tier ─────────────────────────────────────────────────────────

/// No registered migration may be failing.
///
/// The check that seventeen migrations went without.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_migration_is_currently_failing() {
    let pool = pool().await;

    if !ledger_exists(&pool).await {
        println!(
            "  ledger not deployed yet — nothing recorded. This is NOT a pass: \
             until a boot has written to `schema_migrations`, a failing migration \
             is still invisible."
        );
        return;
    }

    let rows = sqlx::query(
        "SELECT filename, last_status, consecutive_failures, attempts, successes, \
                first_succeeded_at IS NOT NULL AS ever_succeeded, \
                left(coalesce(last_error, ''), 200) AS err \
           FROM schema_migrations \
          ORDER BY filename",
    )
    .fetch_all(&pool)
    .await
    .expect("read ledger");

    if rows.is_empty() {
        println!("  ledger exists but is empty — no boot has recorded anything yet.");
        return;
    }

    let mut failing: Vec<String> = Vec::new();
    let mut never_succeeded: Vec<String> = Vec::new();
    let mut ok = 0usize;

    for r in &rows {
        let name: String = r.get("filename");
        let status: String = r.get("last_status");
        let consecutive: i32 = r.get("consecutive_failures");
        let ever: bool = r.get("ever_succeeded");
        let err: String = r.get("err");

        if status != "ok" {
            failing.push(format!(
                "{name}\n         status: {status}, {consecutive} consecutive failure(s)\n\
                 \x20        error:  {err}"
            ));
        } else {
            ok += 1;
        }
        // Distinct and worse: a migration that has run many times and has never
        // once applied. That is the tx_type shape exactly, and it can be true
        // while the LAST attempt looks fine if the failure is intermittent.
        if !ever {
            never_succeeded.push(name);
        }
    }

    println!(
        "\n  {} migration(s) recorded, {ok} last applied cleanly",
        rows.len()
    );

    assert!(
        never_succeeded.is_empty(),
        "\n{} migration(s) have been attempted and have NEVER once succeeded:\n  {}\n\n\
         This is the `credit_ledger_tx_type_check` shape: registered, replayed \
         every boot, failing every time, and reported only to a log. Whatever \
         they declare does not exist in the database.\n",
        never_succeeded.len(),
        never_succeeded.join("\n  ")
    );

    assert!(
        failing.is_empty(),
        "\n{} migration(s) failed on the most recent boot:\n  {}\n\n\
         `run_migrations` deliberately continues past a failure, so the service \
         is up and the schema is not what the code believes it is.\n",
        failing.len(),
        failing.join("\n  ")
    );
}

/// The ledger must know about every migration the code registers.
///
/// A registered file that has no ledger row was never attempted — which means
/// the deployed binary's list differs from this checkout's, and the schema is
/// being managed by code nobody is currently reading.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_ledger_covers_every_registered_migration() {
    let pool = pool().await;
    if !ledger_exists(&pool).await {
        println!("  ledger not deployed yet.");
        return;
    }

    let recorded: Vec<String> = sqlx::query_scalar("SELECT filename FROM schema_migrations")
        .fetch_all(&pool)
        .await
        .expect("read ledger");
    if recorded.is_empty() {
        println!("  ledger empty — no boot has recorded anything yet.");
        return;
    }

    let registered = registered_migrations();
    let unrecorded: Vec<&String> = registered
        .iter()
        .filter(|f| !recorded.contains(f))
        .collect();
    let orphaned: Vec<&String> = recorded
        .iter()
        .filter(|f| !registered.contains(f))
        .collect();

    println!(
        "  {} registered, {} recorded, {} unrecorded, {} orphaned",
        registered.len(),
        recorded.len(),
        unrecorded.len(),
        orphaned.len()
    );

    // Reported rather than asserted, in both directions, because a checkout
    // ahead of the deploy is the normal state of a repository and would make
    // this permanently red for a reason nobody can fix from here. What matters
    // is that the difference is visible and named.
    if !unrecorded.is_empty() {
        println!(
            "  registered here but never attempted on this database (this \
             checkout is ahead of the deploy, or the deploy is not carrying \
             them):\n    {}",
            unrecorded
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n    ")
        );
    }
    if !orphaned.is_empty() {
        println!(
            "  recorded on this database but no longer registered (deleted or \
             renamed since):\n    {}",
            orphaned
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n    ")
        );
    }
}

/// What the ledger unblocks, reported so the next step is obvious.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_deploy_times_that_liveness_windows_could_use() {
    let pool = pool().await;
    if !ledger_exists(&pool).await {
        println!("  ledger not deployed yet.");
        return;
    }

    let rows = sqlx::query(
        "SELECT filename, first_succeeded_at \
           FROM schema_migrations \
          WHERE first_succeeded_at IS NOT NULL \
            AND filename ~ '(199|203|205)' \
          ORDER BY filename",
    )
    .fetch_all(&pool)
    .await
    .expect("read ledger");

    if rows.is_empty() {
        println!(
            "  no landing times recorded yet for the migrations that create \
             liveness sinks. Until there are, `liveness_trust` must keep counting \
             opportunities over all time and keep its documented exemption."
        );
        return;
    }

    println!("\n  first-success times available for opportunity windowing:");
    for r in &rows {
        let f: String = r.get("filename");
        let at: chrono::DateTime<chrono::Utc> = r.get("first_succeeded_at");
        println!("    {at}  {f}");
    }
    println!(
        "  `liveness_trust` can now scope `episodes.assertions` opportunities to \
         episodes created after 205 landed, which removes the KNOWN_SILENT entry \
         explaining why it could not."
    );
}
