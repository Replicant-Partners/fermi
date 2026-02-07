//! Test database connection without compile-time query checking
//!
//! Run with: cargo run --example test_connection

use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    println!("Connecting to database...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("✅ Connected successfully!");

    // Test query
    let result = sqlx::query!("SELECT 1 as health")
        .fetch_one(&pool)
        .await?;

    println!("✅ Health check passed: {}", result.health.unwrap());

    // Check if episodes table exists
    let tables = sqlx::query!(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public'"
    )
    .fetch_all(&pool)
    .await?;

    println!("\n📋 Available tables:");
    for table in tables {
        println!("  - {}", table.table_name.unwrap_or_default());
    }

    Ok(())
}
