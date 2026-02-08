use crate::Result;
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use uuid::Uuid;

/// Distributed lock for consolidation operations
pub struct ConsolidationLock {
    pool: Arc<PgPool>,
    worker_id: String,
}

impl ConsolidationLock {
    /// Create a new consolidation lock
    pub fn new(pool: Arc<PgPool>, worker_id: String) -> Self {
        Self { pool, worker_id }
    }

    /// Attempt to acquire a lock for an agent
    /// Returns true if lock acquired, false if already locked by another worker
    pub async fn acquire(&self, agent_id: Uuid, timeout_minutes: i32) -> Result<bool> {
        let expires_at = Utc::now() + Duration::minutes(timeout_minutes as i64);

        // Try to insert lock
        let result = sqlx::query(
            r#"
            INSERT INTO consolidation_locks (agent_id, locked_by, locked_at, expires_at)
            VALUES ($1, $2, NOW(), $3)
            ON CONFLICT (agent_id) DO NOTHING
            "#,
        )
        .bind(agent_id)
        .bind(&self.worker_id)
        .bind(expires_at)
        .execute(&*self.pool)
        .await?;

        // If we inserted a row, we got the lock
        if result.rows_affected() > 0 {
            return Ok(true);
        }

        // Check if existing lock is ours or expired
        let row = sqlx::query(
            r#"
            SELECT locked_by, expires_at
            FROM consolidation_locks
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            let locked_by: String = row.try_get("locked_by")?;
            let expires_at: DateTime<Utc> = row.try_get("expires_at")?;

            // Check if it's our lock
            if locked_by == self.worker_id {
                return Ok(true);
            }

            // Check if expired
            if expires_at < Utc::now() {
                // Try to steal expired lock
                let result = sqlx::query(
                    r#"
                    UPDATE consolidation_locks
                    SET locked_by = $1, locked_at = NOW(), expires_at = $2
                    WHERE agent_id = $3 AND expires_at < NOW()
                    "#,
                )
                .bind(&self.worker_id)
                .bind(expires_at)
                .bind(agent_id)
                .execute(&*self.pool)
                .await?;

                return Ok(result.rows_affected() > 0);
            }
        }

        Ok(false)
    }

    /// Release a lock for an agent
    pub async fn release(&self, agent_id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM consolidation_locks
            WHERE agent_id = $1 AND locked_by = $2
            "#,
        )
        .bind(agent_id)
        .bind(&self.worker_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    /// Check if a lock exists and who holds it
    pub async fn check(&self, agent_id: Uuid) -> Result<Option<LockInfo>> {
        let row = sqlx::query(
            r#"
            SELECT locked_by, locked_at, expires_at
            FROM consolidation_locks
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&*self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(LockInfo {
                locked_by: row.try_get("locked_by")?,
                locked_at: row.try_get("locked_at")?,
                expires_at: row.try_get("expires_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Clean up all expired locks
    pub async fn cleanup_expired() -> Result<usize> {
        // This is a static method, needs a pool passed in
        // Will be called by a maintenance task
        Ok(0) // Placeholder
    }

    /// Clean up expired locks with pool
    pub async fn cleanup_expired_locks(pool: &PgPool) -> Result<usize> {
        let result = sqlx::query(
            r#"
            DELETE FROM consolidation_locks
            WHERE expires_at < NOW()
            "#,
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() as usize)
    }

    /// Extend lock expiry
    pub async fn extend(&self, agent_id: Uuid, additional_minutes: i32) -> Result<bool> {
        let new_expiry = Utc::now() + Duration::minutes(additional_minutes as i64);

        let result = sqlx::query(
            r#"
            UPDATE consolidation_locks
            SET expires_at = $1
            WHERE agent_id = $2 AND locked_by = $3
            "#,
        )
        .bind(new_expiry)
        .bind(agent_id)
        .bind(&self.worker_id)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }
}

/// Information about a lock
#[derive(Debug, Clone)]
pub struct LockInfo {
    pub locked_by: String,
    pub locked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl LockInfo {
    pub fn is_expired(&self) -> bool {
        self.expires_at < Utc::now()
    }

    pub fn time_remaining(&self) -> Duration {
        self.expires_at.signed_duration_since(Utc::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    async fn get_test_pool() -> PgPool {
        dotenvy::dotenv().ok();
        let database_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
        let connect_options = database_url
            .parse::<sqlx::postgres::PgConnectOptions>()
            .expect("Invalid DATABASE_URL")
            .statement_cache_capacity(0);
        sqlx::pool::PoolOptions::new()
            .max_connections(5)
            .connect_with(connect_options)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_lock_acquire_and_release() {
        let pool = Arc::new(get_test_pool().await);
        let lock = ConsolidationLock::new(pool.clone(), "test-worker-1".to_string());

        // Create test agent first
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
             VALUES ($1, $2, 'test', '1.0.0', 'test', 'llm', 'test-model', 0.3, 'test')"
        )
        .bind(agent_id)
        .bind(format!("test_agent_{}", Uuid::new_v4()))
        .execute(&*pool)
        .await
        .unwrap();

        // Should acquire lock
        let acquired = lock.acquire(agent_id, 5).await.unwrap();
        assert!(acquired, "Should acquire lock");

        // Check lock exists
        let info = lock.check(agent_id).await.unwrap();
        assert!(info.is_some(), "Lock should exist");
        assert_eq!(info.unwrap().locked_by, "test-worker-1");

        // Release lock
        lock.release(agent_id).await.unwrap();

        // Check lock removed
        let info = lock.check(agent_id).await.unwrap();
        assert!(info.is_none(), "Lock should be removed");

        println!("✅ Lock acquire and release works!");
    }

    #[tokio::test]
    async fn test_lock_prevents_concurrent_access() {
        let pool = Arc::new(get_test_pool().await);

        // Create test agent
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
             VALUES ($1, $2, 'test', '1.0.0', 'test', 'llm', 'test-model', 0.3, 'test')"
        )
        .bind(agent_id)
        .bind(format!("test_agent_{}", Uuid::new_v4()))
        .execute(&*pool)
        .await
        .unwrap();

        let lock1 = ConsolidationLock::new(pool.clone(), "worker-1".to_string());
        let lock2 = ConsolidationLock::new(pool.clone(), "worker-2".to_string());

        // Worker 1 acquires lock
        let acquired1 = lock1.acquire(agent_id, 5).await.unwrap();
        assert!(acquired1, "Worker 1 should acquire lock");

        // Worker 2 tries to acquire same lock
        let acquired2 = lock2.acquire(agent_id, 5).await.unwrap();
        assert!(!acquired2, "Worker 2 should NOT acquire lock");

        // Worker 1 releases
        lock1.release(agent_id).await.unwrap();

        // Now worker 2 can acquire
        let acquired2 = lock2.acquire(agent_id, 5).await.unwrap();
        assert!(acquired2, "Worker 2 should now acquire lock");

        // Cleanup
        lock2.release(agent_id).await.unwrap();

        println!("✅ Lock prevents concurrent access!");
    }

    #[tokio::test]
    async fn test_lock_expiry() {
        let pool = Arc::new(get_test_pool().await);

        // Create test agent
        let agent_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
             VALUES ($1, $2, 'test', '1.0.0', 'test', 'llm', 'test-model', 0.3, 'test')"
        )
        .bind(agent_id)
        .bind(format!("test_agent_{}", Uuid::new_v4()))
        .execute(&*pool)
        .await
        .unwrap();

        // Create expired lock by setting very short timeout
        let lock1 = ConsolidationLock::new(pool.clone(), "worker-1".to_string());

        // Manually insert expired lock
        sqlx::query(
            r#"
            INSERT INTO consolidation_locks (agent_id, locked_by, locked_at, expires_at)
            VALUES ($1, 'worker-1', NOW() - INTERVAL '10 minutes', NOW() - INTERVAL '5 minutes')
            ON CONFLICT (agent_id) DO UPDATE
            SET locked_by = 'worker-1', expires_at = NOW() - INTERVAL '5 minutes'
            "#,
        )
        .bind(agent_id)
        .execute(&*pool)
        .await
        .unwrap();

        // Worker 2 should be able to steal expired lock
        let lock2 = ConsolidationLock::new(pool.clone(), "worker-2".to_string());
        let acquired = lock2.acquire(agent_id, 5).await.unwrap();
        assert!(acquired, "Should steal expired lock");

        // Verify worker 2 owns the lock now
        let info = lock2.check(agent_id).await.unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().locked_by, "worker-2");

        // Cleanup
        lock2.release(agent_id).await.unwrap();

        println!("✅ Lock expiry works!");
    }

    #[tokio::test]
    async fn test_cleanup_expired_locks() {
        let pool = get_test_pool().await;

        // Create test agents
        let agent_id1 = Uuid::new_v4();
        let agent_id2 = Uuid::new_v4();

        for agent_id in &[agent_id1, agent_id2] {
            sqlx::query(
                "INSERT INTO agents (agent_id, agent_name, agent_type, version, tier, executor_type, model, temperature, author)
                 VALUES ($1, $2, 'test', '1.0.0', 'test', 'llm', 'test-model', 0.3, 'test')"
            )
            .bind(agent_id)
            .bind(format!("test_agent_{}", Uuid::new_v4()))
            .execute(&pool)
            .await
            .unwrap();
        }

        // Insert expired locks
        sqlx::query(
            r#"
            INSERT INTO consolidation_locks (agent_id, locked_by, locked_at, expires_at)
            VALUES
                ($1, 'worker-1', NOW() - INTERVAL '20 minutes', NOW() - INTERVAL '10 minutes'),
                ($2, 'worker-2', NOW() - INTERVAL '15 minutes', NOW() - INTERVAL '5 minutes')
            ON CONFLICT (agent_id) DO UPDATE
            SET expires_at = EXCLUDED.expires_at
            "#,
        )
        .bind(agent_id1)
        .bind(agent_id2)
        .execute(&pool)
        .await
        .unwrap();

        // Clean up expired locks
        let cleaned = ConsolidationLock::cleanup_expired_locks(&pool)
            .await
            .unwrap();
        assert!(cleaned >= 2, "Should clean at least 2 expired locks");

        println!("✅ Cleanup expired locks works! Cleaned: {}", cleaned);
    }
}
