//! Temporary: does `sqlx::raw_sql` execute a multi-statement file atomically?
#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn raw_sql_multi_statement_atomicity() {
    let url = std::env::var("DATABASE_URL").unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await.unwrap();

    // Clean slate, committed on its own.
    sqlx::raw_sql("DROP TABLE IF EXISTS _atomicity_probe").execute(&pool).await.unwrap();
    sqlx::raw_sql("CREATE TABLE _atomicity_probe (v text)").execute(&pool).await.unwrap();
    sqlx::raw_sql("INSERT INTO _atomicity_probe VALUES ('bad')").execute(&pool).await.unwrap();
    sqlx::raw_sql("ALTER TABLE _atomicity_probe ADD CONSTRAINT probe_chk CHECK (v IN ('bad'))")
        .execute(&pool).await.unwrap();

    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE conname='probe_chk'")
        .fetch_one(&pool).await.unwrap();

    // The exact shape a migration uses: DROP then a failing ADD, one raw_sql call.
    let res = sqlx::raw_sql(
        "ALTER TABLE _atomicity_probe DROP CONSTRAINT IF EXISTS probe_chk;\n\
         ALTER TABLE _atomicity_probe ADD CONSTRAINT probe_chk CHECK (v IN ('good'));",
    ).execute(&pool).await;

    let after: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_constraint WHERE conname='probe_chk'")
        .fetch_one(&pool).await.unwrap();

    println!("\n  raw_sql failed as expected: {}", res.is_err());
    println!("  constraint before: {before}, after: {after}");
    println!("  => raw_sql multi-statement is {}",
             if after == before { "ATOMIC (DROP rolled back)" } else { "NOT ATOMIC (DROP committed, constraint LOST)" });

    sqlx::raw_sql("DROP TABLE IF EXISTS _atomicity_probe").execute(&pool).await.unwrap();
}
