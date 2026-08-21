#[test]
fn git_half_of_snapshot_works_here() {
    use agent_bestiary_ontology::{GitConfig, GitManager};
    let dir = std::env::temp_dir().join(format!("ontotest-{}", std::process::id()));
    let cfg = GitConfig {
        base_path: dir.to_string_lossy().to_string(),
        author_name: "Fermi ADM".into(),
        author_email: "adm@fermi.ai".into(),
        branch: "main".into(),
        github_org: None,
        github_token: None,
        auto_push: false,
        remote_name: "origin".into(),
    };
    let gm = match GitManager::new(cfg) {
        Ok(g) => g,
        Err(e) => panic!("GitManager::new FAILED: {e}"),
    };
    let stats = agent_bestiary_ontology::OntologyStats::new(3, 2, 1, 0, None);
    match gm.commit_ontology("fermi", "erDiagram\n    THING {\n        string id\n    }\n", &stats) {
        Ok(c) => println!("commit_ontology OK sha={} pushed={}", c.sha, c.pushed_to_remote),
        Err(e) => panic!("commit_ontology FAILED: {e}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The bug that meant `create_snapshot` never once succeeded.
///
/// `SELECT MAX(version) ... WHERE agent_id = $1` has no GROUP BY, so it always
/// returns exactly one row — NULL when the agent has no snapshots. Decoding
/// that into `(i32,)` fails, `?` propagates, and the FIRST snapshot for any
/// agent errors. No agent ever reached a second, so the path never ran.
///
/// Requires a database: this is a decode contract, and a mock cannot fail the
/// way Postgres did. Skipped when DATABASE_URL_UNPOOLED is absent.
#[tokio::test]
async fn max_version_of_an_agent_with_no_snapshots_decodes() {
    let Ok(url) = std::env::var("DATABASE_URL_UNPOOLED") else {
        eprintln!("skipped: DATABASE_URL_UNPOOLED not set");
        return;
    };
    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipped: cannot connect: {e}");
            return;
        }
    };

    // An agent id that certainly has no snapshots.
    let orphan = uuid::Uuid::new_v4();

    // The OLD shape. Kept as the regression witness: if this ever stops
    // failing, Postgres changed and the comment above needs revisiting.
    let old: Result<Option<(i32,)>, _> =
        sqlx::query_as("SELECT MAX(version) FROM ontology_snapshots WHERE agent_id = $1")
            .bind(orphan)
            .fetch_optional(&pool)
            .await;
    assert!(
        old.is_err(),
        "the old (i32,) decode should fail on a NULL aggregate — that was the bug"
    );

    // The FIXED shape.
    let fixed: Option<i32> =
        sqlx::query_scalar("SELECT MAX(version) FROM ontology_snapshots WHERE agent_id = $1")
            .bind(orphan)
            .fetch_one(&pool)
            .await
            .expect("Option<i32> must decode a NULL aggregate");
    assert_eq!(fixed, None, "no snapshots yet");
    assert_eq!(fixed.unwrap_or(0) + 1, 1, "first snapshot must be version 1");
}
