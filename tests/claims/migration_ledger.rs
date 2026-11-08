//! SPEC 36 G16 (#120): the boot migration runner skips applied files, fails
//! on a changed checksum, and fails on any failure outside the known-failing
//! list. Against the real DB, in a throwaway schema dropped at the end, so
//! no shared table is touched.

use fermi::migration_runner::{run, BootError, KnownFailing, Outcome};
use sqlx::Connection;
use std::collections::HashMap;

use crate::support;

#[tokio::test]
async fn skip_checksum_change_and_failure_abort() {
    let Some(_pool) = support::pool().await else {
        return;
    };
    let url = std::env::var("ABW_TEST_DATABASE_URL").unwrap();
    let mut conn = sqlx::PgConnection::connect(&url).await.expect("connect");
    // A panic below skips the final drop; sweep schemas a failed earlier
    // run left behind first. Only this test's own prefix is touched.
    let stale: Vec<String> = sqlx::query_scalar(
        "SELECT nspname::text FROM pg_namespace WHERE nspname LIKE 'spec36\\_mig\\_%'",
    )
    .fetch_all(&mut conn)
    .await
    .expect("list stale schemas");
    for st in stale {
        sqlx::raw_sql(&format!("DROP SCHEMA {st} CASCADE"))
            .execute(&mut conn)
            .await
            .expect("drop stale schema");
    }
    let schema = support::tag("mig");
    sqlx::raw_sql(&format!("CREATE SCHEMA {schema}"))
        .execute(&mut conn)
        .await
        .expect("schema");
    scenario(&mut conn, &schema).await;
    sqlx::raw_sql(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&mut conn)
        .await
        .expect("drop schema");
}

async fn scenario(conn: &mut sqlx::PgConnection, schema: &str) {
    let ledger = format!("{schema}.schema_migrations");
    let mut files: HashMap<&'static str, String> = HashMap::new();
    files.insert("m/001.sql", format!("CREATE TABLE {schema}.t1 (x int);"));
    // Not idempotent: a second execution would fail.
    files.insert("m/002.sql", format!("CREATE TABLE {schema}.t2 (x int);"));
    let none = KnownFailing::default();
    let list = ["m/001.sql", "m/002.sql"];

    // First boot applies both.
    fn read(f: &HashMap<&'static str, String>) -> impl Fn(&str) -> std::io::Result<String> {
        let f = f.clone();
        move |name: &str| {
            f.get(name)
                .cloned()
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, name.to_string()))
        }
    }
    let out = run(conn, &ledger, &list, read(&files), &none)
        .await
        .expect("first boot");
    assert!(out.iter().all(|(_, o)| *o == Outcome::Applied), "{out:?}");

    // Second boot skips both. Re-running 002 would fail, so a pass proves
    // the skip.
    let out = run(conn, &ledger, &list, read(&files), &none)
        .await
        .expect("second boot");
    assert!(out.iter().all(|(_, o)| *o == Outcome::Skipped), "{out:?}");

    // An applied file edited: boot fails, and the recorded checksum is kept.
    let mut edited = files.clone();
    edited.insert("m/001.sql", format!("CREATE TABLE {schema}.t1b (x int);"));
    let err = run(conn, &ledger, &list, read(&edited), &none)
        .await
        .unwrap_err();
    assert!(matches!(err, BootError::ChecksumChanged { .. }), "{err}");
    let out = run(conn, &ledger, &list, read(&files), &none)
        .await
        .expect("original is still applied");
    assert!(out.iter().all(|(_, o)| *o == Outcome::Skipped));

    // A new failing file: boot fails, and the file after it does not run.
    let mut failing = files.clone();
    failing.insert("m/003.sql", "SELECT * FROM no_such_table_spec36;".into());
    failing.insert("m/004.sql", format!("CREATE TABLE {schema}.t4 (x int);"));
    let list4 = ["m/001.sql", "m/002.sql", "m/003.sql", "m/004.sql"];
    let err = run(conn, &ledger, &list4, read(&failing), &none)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, BootError::Failed { file, .. } if file == "m/003.sql"),
        "{err}"
    );
    let t4: Option<String> = sqlx::query_scalar("SELECT to_regclass($1)::text")
        .bind(format!("{schema}.t4"))
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert!(t4.is_none(), "nothing after a failure runs");

    // The same failure, listed as known with its checksum: tolerated, and the
    // rest runs.
    let sha = fermi::migration_runner::sha256_hex(&failing["m/003.sql"]);
    let known = KnownFailing::parse(&format!("m/003.sql {sha} test_class")).unwrap();
    let out = run(conn, &ledger, &list4, read(&failing), &known)
        .await
        .expect("tolerated");
    assert_eq!(
        out.iter().map(|(_, o)| *o).collect::<Vec<_>>(),
        vec![
            Outcome::Skipped,
            Outcome::Skipped,
            Outcome::KnownFailure,
            Outcome::Applied
        ]
    );

    // Listed, but the file changed and still fails: not tolerated.
    let mut changed = failing.clone();
    changed.insert(
        "m/003.sql",
        "SELECT * FROM still_no_such_table_spec36;".into(),
    );
    let err = run(conn, &ledger, &list4, read(&changed), &known)
        .await
        .unwrap_err();
    assert!(matches!(err, BootError::Failed { .. }), "{err}");

    // An unreadable listed file fails boot.
    let err = run(conn, &ledger, &["m/missing.sql"], read(&files), &none)
        .await
        .unwrap_err();
    assert!(matches!(err, BootError::Unreadable { .. }), "{err}");

    // An applied file that goes missing fails boot, and once it is back the
    // next boot skips it: its recorded checksum was not overwritten.
    let mut gone = files.clone();
    gone.remove("m/001.sql");
    let err = run(conn, &ledger, &list, read(&gone), &none)
        .await
        .unwrap_err();
    assert!(matches!(err, BootError::Unreadable { .. }), "{err}");
    let out = run(conn, &ledger, &list, read(&files), &none)
        .await
        .expect("restored");
    assert!(out.iter().all(|(_, o)| *o == Outcome::Skipped), "{out:?}");
}

/// The committed known-failing list parses, and every entry's checksum is
/// the file's current checksum (otherwise the tolerance silently lapses into
/// a boot failure, or names a file that no longer exists).
#[test]
fn known_failing_list_matches_the_files() {
    let root = env!("CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!(
        "{root}/{}",
        fermi::migration_runner::KNOWN_FAILING_PATH
    ))
    .unwrap();
    let k = KnownFailing::parse(&text).expect("parses");
    assert!(!k.is_empty());
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
    {
        let mut p = line.split_whitespace();
        let (f, sha) = (p.next().unwrap(), p.next().unwrap());
        let sql = std::fs::read_to_string(format!("{root}/{f}")).expect(f);
        assert_eq!(fermi::migration_runner::sha256_hex(&sql), sha, "{f}");
    }
}
