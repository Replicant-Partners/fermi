//! Is a multi-statement migration applied atomically?
//!
//! # Why this test exists
//!
//! This repository carried a belief — stated in `scripts/lint-migrations.sh`, in
//! several migration headers, and in a whitepaper — that a `DROP CONSTRAINT` and
//! `ADD CONSTRAINT` pair written as two top-level statements would execute in two
//! separate transactions through PgBouncer, so a failing `ADD` would leave the
//! `DROP` committed and silently delete the constraint.
//!
//! **That is not true of the path this platform uses**, and nobody had checked.
//! `run_migrations` reads each file whole and hands it to
//! `sqlx::raw_sql(&sql).execute(db)`, which sends it as one simple query.
//! Postgres wraps a multi-statement simple query in a single implicit
//! transaction, so a failure anywhere rolls the whole file back. The pooler
//! cannot split it, because it is one message.
//!
//! The belief was load-bearing. It was the stated cause of
//! `credit_ledger_tx_type_check` being absent, it justified an `ERROR`-level lint
//! rule, and it was repeated as established mechanism in commit messages. The
//! real reason that constraint was missing is **still unknown**: the migration
//! replay path could not have deleted it, and it was never managed by
//! `ensure_critical_schema` (removed in SPEC 36 G16) either. An admitted gap is worth more than an
//! unverified causal story, and this test is the part that can be verified.
//!
//! # What remains a real hazard
//!
//! Two things, and the lint is justified by these rather than by the pooler:
//!
//! 1. **Statement-at-a-time application.** `psql -f file.sql` executes each
//!    statement in its own implicit transaction. That is how migrations get
//!    validated by hand, so a `DROP`+`ADD` pair really can half-apply — just not
//!    on the deploy path.
//! 2. **`ensure_critical_schema`**, until SPEC 36 G16. It ran each statement as
//!    its own `sqlx::query` and held `DROP CONSTRAINT` / `ADD CONSTRAINT` pairs,
//!    which were two transactions. Its statements are now
//!    `migrations/245_critical_schema.sql`, one file, so one implicit
//!    transaction on the deploy path; the test below pins that it stays gone.
//!
//! Read-only apart from a scratch table created and dropped under a reserved
//! name.
//!
//! Run with: `cargo test --test migration_atomicity -- --ignored --nocapture`

use sqlx::PgPool;

const SCRATCH: &str = "_migration_atomicity_scratch";

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

async fn constraint_count(p: &PgPool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pg_constraint WHERE conname = 'scratch_chk'")
        .fetch_one(p)
        .await
        .expect("count")
}

/// The property the lint's original justification depended on, measured.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn raw_sql_applies_a_multi_statement_file_atomically() {
    let pool = pool().await;

    // Dropped at both ends, so a previous aborted run cannot skew the result.
    sqlx::raw_sql(&format!("DROP TABLE IF EXISTS {SCRATCH}"))
        .execute(&pool)
        .await
        .expect("clean");
    sqlx::raw_sql(&format!(
        "CREATE TABLE {SCRATCH} (v text); \
         INSERT INTO {SCRATCH} VALUES ('bad'); \
         ALTER TABLE {SCRATCH} ADD CONSTRAINT scratch_chk CHECK (v IN ('bad'));"
    ))
    .execute(&pool)
    .await
    .expect("setup");

    let before = constraint_count(&pool).await;
    assert_eq!(before, 1, "setup should have installed the constraint");

    // Exactly the shape the lint calls an error: a DROP, then an ADD that cannot
    // succeed because an existing row violates the narrower list.
    let res = sqlx::raw_sql(&format!(
        "ALTER TABLE {SCRATCH} DROP CONSTRAINT IF EXISTS scratch_chk; \
         ALTER TABLE {SCRATCH} ADD CONSTRAINT scratch_chk CHECK (v IN ('good'));"
    ))
    .execute(&pool)
    .await;
    assert!(
        res.is_err(),
        "the ADD must fail for this test to mean anything"
    );

    let after = constraint_count(&pool).await;
    let _ = sqlx::raw_sql(&format!("DROP TABLE IF EXISTS {SCRATCH}"))
        .execute(&pool)
        .await;

    assert_eq!(
        after, before,
        "the DROP committed even though the ADD failed, so the constraint was \
         lost. If this fires, `sqlx::raw_sql` is no longer applying a file as one \
         implicit transaction, and every DROP+ADD pair in migrations/ becomes a \
         live hazard on the DEPLOY path rather than only under `psql -f`."
    );

    println!(
        "\n  multi-statement raw_sql is ATOMIC through this pooler: the failing ADD \
         rolled the DROP back, constraint count {before} -> {after}."
    );
}

/// The path that was two transactions per pair is gone.
///
/// `ensure_critical_schema` executed each statement as its own `sqlx::query`
/// and held `DROP CONSTRAINT` / `ADD CONSTRAINT` pairs. SPEC 36 G16 moved its
/// statements into `migrations/245_critical_schema.sql`, which the runner sends
/// as one simple query, so the pairs are atomic. Offline scan.
#[test]
fn the_single_statement_schema_path_is_gone() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let src = std::fs::read_to_string(root.join("src/api_server.rs")).expect("api_server.rs");
    assert!(
        !src.contains("async fn ensure_critical_schema"),
        "a per-statement schema path is back; put the statements in a migration"
    );
    assert!(src.contains("\"migrations/245_critical_schema.sql\""));
    let sql =
        std::fs::read_to_string(root.join("migrations/245_critical_schema.sql")).expect("245");
    assert!(
        sql.contains("ADD CONSTRAINT"),
        "the pairs moved with the statements"
    );
}
