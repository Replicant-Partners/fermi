//! Does a constraint the code depends on actually exist?
//!
//! # The failure
//!
//! `credit_ledger_tx_type_check` is declared by seventeen migrations and does
//! not exist in production. Three of those migrations exist for no other
//! purpose than to fix it. Nothing ever checked whether the fix worked, so the
//! repair was performed, believed, and repeated for the life of the project.
//!
//! `schema_trust` would have caught a missing *column* at boot. A missing
//! constraint was invisible to it, and constraints are where the interesting
//! guarantees live: a column's absence 500s a request loudly, while a
//! constraint's absence changes nothing until the day bad data arrives, and
//! then changes nothing visible either.
//!
//! # Why the live tier is the only real one
//!
//! Whether a constraint exists is a fact about the deployed database and
//! nothing else. It cannot be inferred from the migration files — that is
//! precisely the inference that was wrong for seventeen migrations. The
//! offline tests here only guard the shape of the list.
//!
//! Run with: `cargo test --test constraint_trust -- --ignored --nocapture`

use fermi::schema_trust::SCHEMA_CONSTRAINTS;

/// Migrations whose `DROP CONSTRAINT` + `ADD CONSTRAINT` pair sits outside a DO
/// block, and which therefore cannot be applied atomically through PgBouncer.
///
/// Grandfathered: these are already deployed, or already permanently failing,
/// and rewriting history would not change the database. The list exists to
/// **ratchet** — a new migration with this shape must not join it. `scripts/
/// lint-migrations.sh` rejects the pattern on staged files, which protects
/// anyone who commits through the hook; this protects the repository from
/// anyone who does not.
///
/// 15, not the 25 the linter reports as errors: the other 10 are Rule 1
/// (`BEGIN`/`COMMIT`), a different failure with a different remedy. Counting
/// the headline number would have made this ratchet move for reasons unrelated
/// to constraints, which is how a ratchet stops meaning anything.
const NON_ATOMIC_CONSTRAINT_MIGRATIONS: usize = 15;

#[test]
fn every_declared_constraint_names_a_table_and_says_why() {
    assert!(
        !SCHEMA_CONSTRAINTS.is_empty(),
        "an empty constraint contract is a contract that cannot fail"
    );
    for (table, name, why) in SCHEMA_CONSTRAINTS {
        assert!(!table.is_empty(), "constraint {name} names no table");
        assert!(
            name.starts_with(table),
            "constraint `{name}` does not begin with its table `{table}`. \
             Postgres' default naming is `<table>_<column>_<kind>`, and a name \
             that departs from it is usually a sign the entry was typed from \
             memory rather than copied from `pg_constraint`."
        );
        assert!(
            why.len() > 80,
            "constraint `{name}` has no real justification. The next person \
             deciding whether to drop it needs to know what it was protecting, \
             or they will decide from the name."
        );
    }
}

/// The lint's grandfathered set may only shrink.
///
/// A count rather than a list of names, because the names are not the point:
/// the point is that the number of migrations which cannot apply their own
/// constraints does not grow. Deliberately an equality assertion in the upward
/// direction only — fixing one should make this test tell you to lower the
/// number, not fail silently.
#[test]
fn no_new_migration_declares_a_constraint_it_cannot_apply() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = std::process::Command::new("bash")
        .arg("scripts/lint-migrations.sh")
        .args(
            glob_migrations(&root)
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>(),
        )
        .current_dir(&root)
        .output()
        .expect("run scripts/lint-migrations.sh");

    let text = String::from_utf8_lossy(&out.stdout);
    let errors = text
        .lines()
        .filter(|l| l.contains("DROP+ADD CONSTRAINT outside DO block"))
        .count();

    assert!(
        errors <= NON_ATOMIC_CONSTRAINT_MIGRATIONS,
        "{errors} migration(s) declare a constraint through a non-atomic \
         DROP+ADD, up from the grandfathered {NON_ATOMIC_CONSTRAINT_MIGRATIONS}. \
         Through PgBouncer the DROP commits and the ADD does not, so the net \
         effect of the migration is to DELETE the constraint — and \
         `run_migrations` logs the failure and continues. Wrap the pair in a \
         `DO $$ BEGIN ... END $$;` block.\n\n{text}"
    );
    assert_eq!(
        errors, NON_ATOMIC_CONSTRAINT_MIGRATIONS,
        "only {errors} migration(s) now have the non-atomic shape, down from \
         {NON_ATOMIC_CONSTRAINT_MIGRATIONS}. Lower the constant so the ratchet \
         holds at the new floor."
    );
}

fn glob_migrations(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(root.join("migrations"))
        .expect("migrations/")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "sql"))
        .collect();
    v.sort();
    v
}

// ─── live tier ─────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn declared_constraints_exist_in_the_database() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    let mut missing: Vec<String> = Vec::new();

    for (table, name, why) in SCHEMA_CONSTRAINTS {
        let found: Option<String> = sqlx::query_scalar(
            "SELECT pg_get_constraintdef(c.oid) \
               FROM pg_constraint c \
               JOIN pg_class t ON t.oid = c.conrelid \
              WHERE t.relname = $1 AND c.conname = $2",
        )
        .bind(table)
        .bind(name)
        .fetch_optional(&pool)
        .await
        .expect("query pg_constraint")
        .flatten();

        match found {
            Some(def) => println!("  ok   {name}\n         {def}"),
            None => missing.push(format!("{table}.{name}\n         why: {why}")),
        }
    }

    assert!(
        missing.is_empty(),
        "\n{} declared constraint(s) do not exist in the database:\n  {}\n\n\
         A constraint that is declared, documented, repeatedly repaired, and \
         absent protects nothing. Check whether the ADD is failing against \
         existing rows — `run_migrations` logs migration errors and continues, \
         so a permanently-failing ADD looks exactly like a successful one.\n",
        missing.len(),
        missing.join("\n  ")
    );
}

/// What the constraint would reject if it were applied today.
///
/// Not an assertion about the values — that is a decision about the economy,
/// not a fact about the schema. It exists so the number is printed rather than
/// rediscovered: the reason migration 075 can never apply is that the code
/// grew 22 new transaction types and the list did not, and nothing reported
/// that because the failure is a log line in a boot sequence.
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn report_ledger_values_no_declared_constraint_would_accept() {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect");

    // The canonical list is the one migration 075 declares. Read from the file
    // rather than restated, so this cannot drift from the migration it audits.
    let sql = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("migrations/075_fix_tx_type_constraint.sql"),
    )
    .expect("migration 075");
    let start = sql.find("tx_type IN (").expect("the IN list");
    let end = sql[start..].find("))").expect("end of IN list") + start;
    let declared: std::collections::BTreeSet<&str> =
        sql[start..end].split('\'').skip(1).step_by(2).collect();

    let live: Vec<(String, i64)> = sqlx::query_as(
        "SELECT tx_type, count(*)::bigint FROM credit_ledger GROUP BY 1 ORDER BY 2 DESC",
    )
    .fetch_all(&pool)
    .await
    .expect("ledger census");

    let mut rejected: Vec<(String, i64)> = Vec::new();
    let mut rows_rejected = 0i64;
    for (t, n) in &live {
        if !declared.contains(t.as_str()) {
            rejected.push((t.clone(), *n));
            rows_rejected += n;
        }
    }

    println!(
        "\n  migration 075 declares {} tx_types; the ledger holds {}.",
        declared.len(),
        live.len()
    );
    println!(
        "  {} type(s) / {rows_rejected} row(s) would be rejected, which is why \
         the ADD can never succeed:",
        rejected.len()
    );
    for (t, n) in &rejected {
        println!("    {n:>6}  {t}");
    }
    let unused: Vec<&&str> = declared
        .iter()
        .filter(|d| !live.iter().any(|(t, _)| t == **d))
        .collect();
    println!("  {} declared type(s) never used: {unused:?}", unused.len());
}
