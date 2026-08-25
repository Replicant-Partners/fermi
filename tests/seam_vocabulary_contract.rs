//! Do Rust and Postgres agree about what a column will accept?
//!
//! Run with:
//! ```text
//! cargo test --test seam_vocabulary_contract -- --ignored --nocapture
//! ```
//!
//! Read-only. Every query reads `pg_constraint`, `SELECT DISTINCT` on the
//! declared column, or `SELECT $1::text` with a bound value — no statement
//! writes.
//!
//! # What each failure means
//!
//! * **Rust declares a token the constraint rejects.** Every write of it is
//!   refused. If the write is on a swallowed path — and most of these are — the
//!   sink stays empty and the row count reads `Silent`, which is the shape of
//!   the `L1` defect: `severity = "L1"` against `('info','warning','critical')`,
//!   rejected in a spawned task for the life of the feature.
//! * **The constraint accepts a token Rust never writes.** A vocabulary was
//!   widened for a producer nobody finished. Migration 200 added `'grounding'`
//!   to `anomaly_events.kind` and no `AnomalyKind` variant was ever added, so
//!   the only kind actually written was the one no enum could express.
//! * **The column holds a value nobody declares.** Written before the
//!   constraint existed, or the constraint is `NOT VALID`, or the column has no
//!   constraint at all and the data is the only authority there is.

use fermi::seam_vocabulary::{
    tokens_in_constraint, ActorKind, DeltaDirection, EvaluatorTier, ResolutionMode, Vocabulary,
    VOCABULARIES,
};
use sqlx::PgPool;
use std::collections::BTreeSet;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

async fn constraint_def(pool: &PgPool, name: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conname = $1",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

fn declared(v: &Vocabulary) -> BTreeSet<String> {
    v.tokens.iter().map(|t| t.to_string()).collect()
}

/// Rust and the constraint must agree, in both directions.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_vocabulary_matches_its_check_constraint() {
    let pool = pool().await;
    let mut problems: Vec<String> = Vec::new();

    println!("\n  {:<52} {:>8} {:>8}", "column", "rust", "schema");
    println!("  {}", "-".repeat(72));

    for v in VOCABULARIES {
        let label = format!("{}.{}", v.table, v.column);
        let Some(name) = v.constraint else {
            println!("  {label:<52} {:>8} {:>8}", v.tokens.len(), "\u{2014}");
            continue;
        };

        let Some(def) = constraint_def(&pool, name).await else {
            // "The constraint is missing" has two very different causes and
            // they were reported identically. Migration 212 was the whole
            // lesson: a file on disk that nothing registers looks exactly like
            // a file that is merely waiting for the next boot.
            //
            // The table itself distinguishes them. No table means the migration
            // has not run — pending, or never registered, and
            // `schema_migrations` says which. A table with no constraint means
            // the migration ran and did not do what this registry believes it
            // did, which is the worse case and the one that would otherwise be
            // read as "probably just pending".
            let table_exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
                .bind(format!("public.{}", v.table))
                .fetch_one(&pool)
                .await
                .unwrap_or(false);

            problems.push(if table_exists {
                format!(
                    "{label}: the table exists and constraint `{name}` does not. \
                     The migration ran and did not create it, or it was dropped \
                     — an absent CHECK accepts everything, including the tokens \
                     this registry is asserting are impossible."
                )
            } else {
                format!(
                    "{label}: table `{}` does not exist, so `{name}` cannot. The \
                     migration that declares it has not run. Check \
                     `schema_migrations` for it: no row means never attempted \
                     (pending a deploy, or never registered in \
                     `run_migrations` — which is what happened to 212); a row \
                     with failures means it ran and could not apply.",
                    v.table
                )
            });
            continue;
        };

        let in_db: BTreeSet<String> = tokens_in_constraint(&def).into_iter().collect();
        if in_db.is_empty() {
            problems.push(format!(
                "{label}: no literals parsed out of `{name}`, so this check \
                 proved nothing. Definition: {def}"
            ));
            continue;
        }

        let in_rust = declared(v);
        println!("  {label:<52} {:>8} {:>8}", in_rust.len(), in_db.len());

        let rejected: Vec<_> = in_rust.difference(&in_db).cloned().collect();
        let unwritten: Vec<_> = in_db.difference(&in_rust).cloned().collect();

        if !rejected.is_empty() {
            problems.push(format!(
                "{label}: Rust declares {rejected:?}, which the database will \
                 REFUSE.\n         producers: {}\n         cost: {}",
                v.producers, v.why
            ));
        }
        if !unwritten.is_empty() {
            problems.push(format!(
                "{label}: the constraint accepts {unwritten:?}, which Rust never \
                 writes. Either a producer is missing or the vocabulary was \
                 widened for a feature nobody finished.\n         cost: {}",
                v.why
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "\n{} seam(s) disagree:\n\n  {}\n",
        problems.len(),
        problems.join("\n\n  ")
    );
}

/// Every value the column actually holds must be declared.
///
/// The check with no substitute. The constraint comparison above cannot see a
/// value written before the constraint existed, cannot see one admitted by a
/// `NOT VALID` constraint, and cannot run at all on a column that has no
/// constraint — where the data is the only authority there is.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn no_column_holds_a_value_nobody_declares() {
    let pool = pool().await;
    let mut problems: Vec<String> = Vec::new();
    let mut columns_with_data = 0usize;

    for v in VOCABULARIES {
        let label = format!("{}.{}", v.table, v.column);

        // Identifiers come from a compile-time const, not from input.
        let sql = format!(
            "SELECT DISTINCT {}::text AS val FROM {} WHERE {} IS NOT NULL",
            v.column, v.table, v.column
        );
        let rows = match sqlx::query_scalar::<_, String>(&sql).fetch_all(&pool).await {
            Ok(r) => r,
            // A missing table is `every_vocabulary_matches_its_check_constraint`'s
            // finding, and it says precisely why. Reporting it here too would
            // make one pending migration fail two tiers, which is how a suite
            // becomes noise — every finding is owned by exactly one tier.
            Err(e) if e.to_string().contains("does not exist") => {
                println!("  {label:<52} table absent — see the constraint tier");
                continue;
            }
            Err(e) => {
                problems.push(format!("{label}: could not read — {e}"));
                continue;
            }
        };
        if rows.is_empty() {
            continue;
        }
        columns_with_data += 1;

        let in_rust = declared(v);
        let present: BTreeSet<String> = rows.into_iter().collect();
        let undeclared: Vec<_> = present.difference(&in_rust).cloned().collect();

        println!("  {label:<52} {} distinct value(s) on file", present.len());

        if !undeclared.is_empty() {
            problems.push(format!(
                "{label}: holds {undeclared:?}, which Rust does not declare.\n\
                 \x20        producers: {}\n         cost: {}",
                v.producers, v.why
            ));
        }
    }

    // A suite over empty tables passes for ever. Same rule as the liveness
    // positive controls and the flag census: if nothing has been read, the
    // check has demonstrated nothing.
    assert!(
        columns_with_data > 0,
        "not one declared column holds a value, so this check ran over an empty \
         set and cannot fail. Establish that these tables have data before \
         believing it."
    );
    println!("\n  {columns_with_data} column(s) had data to check.");

    assert!(
        problems.is_empty(),
        "\n{} column(s) hold undeclared values:\n\n  {}\n",
        problems.len(),
        problems.join("\n\n  ")
    );
}

/// Does the *type* survive a real Postgres?
///
/// The three checks above compare declarations: Rust's token list against the
/// constraint, and both against the data. None of them binds anything. The four
/// registry-owned vocabularies are now types whose wire form comes from
/// `#[sqlx(type_name = "text")]`, and that attribute is resolved by **name at
/// bind time** — `SELECT $1::regtype::oid` on first use, per connection. So a
/// typo in it is not a compile error, is invisible to every declaration check
/// here, and surfaces as `TypeNotFound` on the first write, on paths that
/// swallow their errors.
///
/// `the_wire_form_is_the_declared_token` (offline) proves the right bytes go
/// into the buffer. This proves the server takes them and hands them back as
/// the same variant.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn every_owned_vocabulary_round_trips_through_postgres() {
    let pool = pool().await;
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;

    macro_rules! round_trip {
        ($ty:ty) => {
            for v in <$ty>::ALL {
                checked += 1;
                match sqlx::query_scalar::<_, $ty>("SELECT $1::text")
                    .bind(*v)
                    .fetch_one(&pool)
                    .await
                {
                    Ok(got) if got == *v => {}
                    Ok(got) => {
                        problems.push(format!("{}::{:?} came back as {got:?}", stringify!($ty), v))
                    }
                    Err(e) => problems.push(format!(
                        "{}::{:?} did not survive a round trip — {e}",
                        stringify!($ty),
                        v
                    )),
                }
            }
        };
    }

    round_trip!(DeltaDirection);
    round_trip!(ResolutionMode);
    round_trip!(EvaluatorTier);
    round_trip!(ActorKind);

    println!("  {checked} variant(s) bound and read back as themselves.");
    assert!(
        checked >= 11,
        "only {checked} variants were bound; a type was added to \
         `seam_vocabulary` and not to this probe, so its encoding is unproven \
         against a live server"
    );
    assert!(
        problems.is_empty(),
        "\n{} variant(s) do not round trip:\n\n  {}\n",
        problems.len(),
        problems.join("\n  ")
    );
}

/// A vocabulary with no constraint is reported, never asserted.
///
/// Adding a `CHECK` to a live column is a migration with a validation scan, and
/// it may be the wrong call for a hot table. What must not happen is for the
/// absence to be invisible: an unconstrained column accepts anything, so the
/// data check above is the only guard it has, and a reader should know which
/// columns are in that position.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_vocabularies_with_no_constraint() {
    let pool = pool().await;
    let mut bare = Vec::new();
    for v in VOCABULARIES {
        match v.constraint {
            None => bare.push(format!("{}.{} (none declared)", v.table, v.column)),
            Some(name) => {
                if constraint_def(&pool, name).await.is_none() {
                    bare.push(format!("{}.{} (`{name}` missing)", v.table, v.column));
                }
            }
        }
    }
    if bare.is_empty() {
        println!("  every declared vocabulary is enforced by a live constraint.");
    } else {
        println!(
            "\n  {} vocabular(ies) rest on the data check alone:\n  {}",
            bare.len(),
            bare.join("\n  ")
        );
    }
}
