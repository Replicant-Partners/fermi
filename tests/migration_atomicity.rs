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
//! `ensure_critical_schema` either. An admitted gap is worth more than an
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
//! 2. **`ensure_critical_schema`.** It runs each statement as its own
//!    `sqlx::query`, deliberately, and it contains `DROP CONSTRAINT` /
//!    `ADD CONSTRAINT` pairs. Those *are* two transactions. That is the genuinely
//!    non-atomic drop/add in this codebase.
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

/// The path that really is two transactions per pair.
///
/// `ensure_critical_schema` executes each statement as its own `sqlx::query`, on
/// purpose, and it holds `DROP CONSTRAINT` / `ADD CONSTRAINT` pairs. Offline scan,
/// because the hazard is in the shape of the code rather than in any current row.
#[test]
fn the_single_statement_schema_path_has_no_unguarded_drop_add_pair() {
    let src = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/api_server.rs"),
    )
    .expect("api_server.rs");

    let start = src
        .find("async fn ensure_critical_schema")
        .expect("ensure_critical_schema must exist");
    // Bounded to the `alters` array. Without this, the tuple scan below runs past
    // the closing bracket into the rest of the file and slices out of range.
    let body = {
        let after = &src[start..];
        let end = after.find("\n    ];").map(|x| x + 1).unwrap_or(after.len());
        &after[..end]
    };

    /// Leading SQL identifier only.
    ///
    /// The first version of this trimmed trailing quotes and backslashes and took
    /// the first whitespace-delimited token — which left
    /// `fermi_forecast_updates_revision_trigger_check"),` intact, matched nothing,
    /// and made the whole test pass while detecting none of the pairs it exists to
    /// find. Scanning forward over identifier characters cannot do that.
    fn ident(s: &str) -> &str {
        let t = s.trim_start();
        let end = t
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(t.len());
        &t[..end]
    }

    // Entry-aware, because "is a DROP present" is the wrong question. The list is
    // a sequence of `("label", "SQL")` tuples and each tuple becomes one query, so
    // what matters is whether a DROP and its matching ADD sit in the SAME entry.
    // A line-based version of this scan reported both pairs as unguarded after
    // they had been correctly combined into one `DO $$ ... $$` statement each.
    let entries: Vec<&str> = {
        let mut v = Vec::new();
        let mut idxs: Vec<usize> = body
            .match_indices("(\"")
            .filter(|(i, _)| body[..*i].ends_with("        "))
            .map(|(i, _)| i)
            .collect();
        idxs.push(body.len());
        for w in idxs.windows(2) {
            v.push(&body[w[0]..w[1]]);
        }
        v
    };

    let mut dropped_total = 0usize;
    let mut added_total = 0usize;
    let mut unguarded: Vec<String> = Vec::new();

    for e in &entries {
        let drops: Vec<&str> = e
            .match_indices("DROP CONSTRAINT IF EXISTS ")
            .map(|(i, m)| ident(&e[i + m.len()..]))
            .filter(|s| !s.is_empty())
            .collect();
        let adds: Vec<&str> = e
            .match_indices("ADD CONSTRAINT ")
            .map(|(i, m)| ident(&e[i + m.len()..]))
            .filter(|s| !s.is_empty())
            .collect();
        dropped_total += drops.len();
        added_total += adds.len();
        for d in drops {
            // Safe when the same entry puts it back. A DROP with no ADD anywhere is
            // a deliberate removal and not this defect.
            if !adds.contains(&d) && body.contains(&format!("ADD CONSTRAINT {d}")) {
                unguarded.push(d.to_string());
            }
        }
    }

    // A scan that matches nothing passes for the wrong reason. This function is
    // known to contain DROP CONSTRAINT statements; if it stops, the scan has lost
    // its target and must say so rather than going green.
    assert!(
        dropped_total > 0 && added_total > 0,
        "found {dropped_total} DROP and {added_total} ADD constraint statements \
         across {} entries in ensure_critical_schema — the scan no longer matches \
         the code it is meant to police",
        entries.len()
    );

    assert!(
        unguarded.is_empty(),
        "`ensure_critical_schema` drops and re-adds {} constraint(s) as separate \
         single-statement queries: {:?}\n\n\
         Unlike the migration files, this path really is one transaction per \
         statement — that is its stated purpose. So if the ADD fails the DROP \
         stays committed and the constraint is gone, with nothing to restore it. \
         Combine each pair into a single statement (a `DO $$ ... $$` block) so the \
         two move together.",
        unguarded.len(),
        unguarded
    );
}
