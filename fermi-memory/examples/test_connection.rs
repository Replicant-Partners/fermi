//! Test database connection without compile-time query checking
//!
//! Run with: cargo run -p fermi-memory --example test_connection

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::Row;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");

    // Neon uses PgBouncer in transaction mode — disable prepared statement cache
    let connect_options = PgConnectOptions::from_str(&database_url)?.statement_cache_capacity(0);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(connect_options)
        .await?;

    println!("Connected successfully!");

    // Test query
    let row = sqlx::query("SELECT 1 as health").fetch_one(&pool).await?;
    let health: i32 = row.get("health");

    println!("Health check passed: {}", health);

    // Check if episodes table exists
    let tables = sqlx::query(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'",
    )
    .fetch_all(&pool)
    .await?;

    println!("\nAvailable tables:");
    for table in &tables {
        let name: String = table.get("table_name");
        println!("  - {}", name);
    }

    Ok(())
}
