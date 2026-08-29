//! The reservation mechanism, against a real database.
//!
//! # What this proves
//!
//! Six of the platform's twelve delegation edges named a parent whose episode
//! row was never written. The cause was ordering: an id was minted, handed to a
//! child as `parent_episode_id`, and the row was written only after the run
//! finished — so anything the child delegated in that window referenced an id
//! with nothing behind it, permanently if the final write never landed.
//!
//! `MemoryStore::reserve_episode` closes it by writing the row before the id
//! leaves the process. That is only a fix if three things hold, and all three
//! are properties of SQL rather than of Rust, so they need a real database:
//!
//!   1. a reservation is immediately resolvable by a child;
//!   2. the normal write **completes** the reservation rather than failing on
//!      its own row;
//!   3. a write against an id that is **not** a reservation is refused, so a
//!      genuine duplicate cannot silently overwrite a finished episode.
//!
//! Point 3 is the one worth a test. The `ON CONFLICT ... DO UPDATE` that makes
//! point 2 work would, without its `WHERE execution_status = 'running'`
//! predicate, turn a loud error into lost data — and no unit test can see that,
//! because the predicate lives in a SQL string.
//!
//! # Self-cleaning
//!
//! Runs against whatever `DATABASE_URL` points at, which is production. Every
//! row it creates is deleted at the end and the delete count is asserted, so a
//! failure that skips cleanup fails loudly rather than leaving litter.
//!
//! `#[ignore]` because it needs a database; run with
//! `cargo test --test episode_reservation -- --ignored --nocapture`.

use agent_bestiary_memory::MemoryStore;
use sqlx::PgPool;
use uuid::Uuid;

async fn pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("connect")
}

/// Any real agent, because `episodes.agent_id` is a foreign key. Read rather
/// than created: this test adds episodes and nothing else.
async fn some_agent(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT agent_id FROM agents LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("the fleet is empty, so there is nothing to attach an episode to")
}

const MARK: &str = "__reservation_contract_test__";

/// Clear anything a previous failure left behind.
///
/// A test that panics mid-way skips its own cleanup, and the next test's
/// delete then counts rows it did not create — which is how the second
/// assertion here first failed for a reason that had nothing to do with the
/// code under test.
async fn clear(pool: &PgPool) {
    sqlx::query("DELETE FROM episodes WHERE query = $1")
        .bind(MARK)
        .execute(pool)
        .await
        .expect("pre-clean");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn a_reservation_is_resolvable_before_the_run_finishes() {
    let pool = pool().await;
    clear(&pool).await;
    let store = MemoryStore::from_pool(pool.clone());
    let agent = some_agent(&pool).await;

    let parent = Uuid::new_v4();
    let child = Uuid::new_v4();

    store
        .reserve_episode(parent, agent, MARK)
        .await
        .expect("reserve");

    // The whole point: a child written DURING the parent's run resolves.
    sqlx::query(
        "INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context,
                               execution_status, execution_time_ms, parent_episode_id)
         VALUES ($1, $2, now(), $3, '{}'::jsonb, 'success', 1, $4)",
    )
    .bind(child)
    .bind(agent)
    .bind(MARK)
    .bind(parent)
    .execute(&pool)
    .await
    .expect("child insert");

    let resolves: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM episodes c
                          JOIN episodes p ON p.episode_id = c.parent_episode_id
                         WHERE c.episode_id = $1)",
    )
    .bind(child)
    .fetch_one(&pool)
    .await
    .expect("resolve check");

    // Reserving twice must not fail: a retry cannot be allowed to trip over
    // its own earlier reservation.
    let twice = store.reserve_episode(parent, agent, MARK).await;

    let deleted = sqlx::query("DELETE FROM episodes WHERE query = $1")
        .bind(MARK)
        .execute(&pool)
        .await
        .expect("cleanup")
        .rows_affected();

    assert!(
        resolves,
        "a child written during the parent's run could not resolve its parent — \
         the reservation did not land, which is the whole defect"
    );
    assert!(twice.is_ok(), "reserving twice must be a no-op: {twice:?}");
    assert_eq!(
        deleted, 2,
        "cleanup must remove exactly the two rows it made"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL"]
async fn the_normal_write_completes_a_reservation_and_refuses_anything_else() {
    let pool = pool().await;
    clear(&pool).await;
    let store = MemoryStore::from_pool(pool.clone());
    let agent = some_agent(&pool).await;

    let id = Uuid::new_v4();
    store
        .reserve_episode(id, agent, MARK)
        .await
        .expect("reserve");

    let mut ep = fermi::episodes::agent_output_to_episode(
        agent,
        MARK,
        &fermi::agent_backend::executor::AgentOutput {
            raw_response: Some("{\"ok\":true}".into()),
            agent_name: "reservation_test".into(),
            agent_type: "test".into(),
            timestamp: chrono::Utc::now(),
            status: fermi::agent_backend::executor::AgentStatus::Success,
            evidence: vec![],
            confidence: 1.0,
            sources_consulted: vec![],
            execution_time_ms: 1,
            tokens_used: Some(1),
            input_tokens: Some(1),
            output_tokens: Some(1),
            metadata: Default::default(),
            tool_invocations: vec![],
            loop_iterations: 0,
        },
    );
    ep.episode_id = id;
    ep.query = MARK.to_string();

    // 2. The completing write must succeed against its own reservation.
    let completed = store
        .store_episode_with_provenance(ep.clone(), None, None)
        .await;

    let status: Option<String> =
        sqlx::query_scalar("SELECT execution_status FROM episodes WHERE episode_id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await
            .expect("status read")
            .flatten();

    // 3. A second write against a FINISHED row must be refused, not silently
    //    applied. Without the `WHERE execution_status = 'running'` guard this
    //    would overwrite a real episode and lose it.
    let duplicate = store.store_episode_with_provenance(ep, None, None).await;

    let deleted = sqlx::query("DELETE FROM episodes WHERE query = $1")
        .bind(MARK)
        .execute(&pool)
        .await
        .expect("cleanup")
        .rows_affected();

    assert!(
        completed.is_ok(),
        "the normal write must complete its own reservation, not collide with it: {completed:?}"
    );
    assert_ne!(
        status.as_deref(),
        Some("running"),
        "the reservation was never completed — it is still marked running"
    );
    assert!(
        duplicate.is_err(),
        "a write against a finished episode must be refused. Silently overwriting \
         it would turn a duplicate id into a lost record."
    );
    assert_eq!(
        deleted, 1,
        "cleanup must remove exactly the one row it made"
    );
}
