use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")?;

    println!("Connecting to database...");
    let connect_options = PgConnectOptions::from_str(&database_url)?.statement_cache_capacity(0);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    // Count before
    let count_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool)
        .await?;
    println!("Agents before cleanup: {}", count_before.0);

    // Delete test agents
    let result = sqlx::query("DELETE FROM agents WHERE agent_name LIKE 'test_agent_%'")
        .execute(&pool)
        .await?;
    println!("Deleted {} test agents", result.rows_affected());

    // Count after
    let count_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents")
        .fetch_one(&pool)
        .await?;
    println!("Agents remaining: {}", count_after.0);

    Ok(())
}
