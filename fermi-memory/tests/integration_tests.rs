// Integration tests for fermi-memory
// Run with: cargo test --package fermi-memory --test integration_tests

use fermi_memory::{Episode, ExecutionStatus, MemoryStore, SemanticRule, VerificationStatus};
use uuid::Uuid;

// Helper to get test database URL
fn get_test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://localhost/fermi_test".to_string())
    })
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_store_and_retrieve_episode() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let agent_id = Uuid::new_v4();
    let episode = Episode::new(
        agent_id,
        "test_user".to_string(),
        "Test query: What is the AI market size?".to_string(),
        serde_json::json!({
            "result": "The AI market is worth $150B",
            "sources": ["source1", "source2"]
        }),
        ExecutionStatus::Success,
    );

    // Store episode
    let episode_id = store.store_episode(episode).await.unwrap();

    // Retrieve episode
    let retrieved = store.get_episode(episode_id).await.unwrap();

    assert_eq!(retrieved.agent_id, agent_id);
    assert_eq!(retrieved.query, "Test query: What is the AI market size?");
    assert!(matches!(
        retrieved.execution_status,
        ExecutionStatus::Success
    ));
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_get_unconsolidated_episodes() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let agent_id = Uuid::new_v4();

    // Store multiple episodes
    for i in 0..5 {
        let episode = Episode::new(
            agent_id,
            "test_user".to_string(),
            format!("Test query {}", i),
            serde_json::json!({"result": i}),
            ExecutionStatus::Success,
        );
        store.store_episode(episode).await.unwrap();
    }

    // Retrieve unconsolidated episodes
    let episodes = store
        .get_unconsolidated_episodes(agent_id, 10)
        .await
        .unwrap();

    assert_eq!(episodes.len(), 5);
    assert!(episodes.iter().all(|e| e.agent_id == agent_id));
    assert!(episodes.iter().all(|e| !e.consolidated));
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_store_and_retrieve_semantic_rule() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let agent_id = Uuid::new_v4();
    let episode_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

    let rule = SemanticRule::new(
        agent_id,
        "test_user".to_string(),
        "When analyzing market size, always check multiple sources".to_string(),
        0.85,
        episode_ids.clone(),
    );

    // Store rule
    let rule_id = store.store_semantic_rule(rule).await.unwrap();

    // Retrieve rule
    let retrieved = store.get_semantic_rule(rule_id).await.unwrap();

    assert_eq!(retrieved.agent_id, agent_id);
    assert_eq!(retrieved.confidence_score, 0.85);
    assert_eq!(retrieved.source_episode_cluster.len(), 2);
    assert!(matches!(
        retrieved.verification_status,
        VerificationStatus::Pending
    ));
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_mark_episodes_consolidated() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let agent_id = Uuid::new_v4();
    let consolidation_job_id = Uuid::new_v4();

    // Store episodes
    let mut episode_ids = Vec::new();
    for i in 0..3 {
        let episode = Episode::new(
            agent_id,
            "test_user".to_string(),
            format!("Test query {}", i),
            serde_json::json!({"result": i}),
            ExecutionStatus::Success,
        );
        let id = store.store_episode(episode).await.unwrap();
        episode_ids.push(id);
    }

    // Mark as consolidated
    store
        .mark_episodes_consolidated(&episode_ids, consolidation_job_id)
        .await
        .unwrap();

    // Verify they're marked consolidated
    for episode_id in episode_ids {
        let episode = store.get_episode(episode_id).await.unwrap();
        assert!(episode.consolidated);
        assert_eq!(episode.consolidation_job_id, Some(consolidation_job_id));
    }
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_get_active_semantic_rules() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let agent_id = Uuid::new_v4();

    // Store multiple rules
    for i in 0..3 {
        let rule = SemanticRule::new(
            agent_id,
            "test_user".to_string(),
            format!("Rule {}", i),
            0.5 + (i as f32 * 0.1),
            vec![Uuid::new_v4()],
        );
        store.store_semantic_rule(rule).await.unwrap();
    }

    // Retrieve active rules
    let rules = store.get_active_semantic_rules(agent_id).await.unwrap();

    assert_eq!(rules.len(), 3);
    assert!(rules.iter().all(|r| r.agent_id == agent_id));
    assert!(rules.iter().all(|r| r.is_active));

    // Check they're sorted by confidence score (descending)
    for i in 0..rules.len() - 1 {
        assert!(rules[i].confidence_score >= rules[i + 1].confidence_score);
    }
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_health_check() {
    let database_url = get_test_db_url();
    let store = MemoryStore::new(&database_url).await.unwrap();

    let result = store.health_check().await;
    assert!(result.is_ok(), "Database health check should pass");
}

#[tokio::test]
async fn test_episode_creation() {
    let agent_id = Uuid::new_v4();
    let episode = Episode::new(
        agent_id,
        "test_user".to_string(),
        "Test query".to_string(),
        serde_json::json!({"test": "data"}),
        ExecutionStatus::Success,
    );

    assert_eq!(episode.agent_id, agent_id);
    assert_eq!(episode.query, "Test query");
    assert!(!episode.consolidated);
    assert!(matches!(episode.execution_status, ExecutionStatus::Success));
}

#[tokio::test]
async fn test_semantic_rule_creation() {
    let agent_id = Uuid::new_v4();
    let episode_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

    let rule = SemanticRule::new(
        agent_id,
        "test_user".to_string(),
        "Test rule".to_string(),
        0.75,
        episode_ids.clone(),
    );

    assert_eq!(rule.agent_id, agent_id);
    assert_eq!(rule.confidence_score, 0.75);
    assert_eq!(rule.episode_count, 2);
    assert!(rule.is_active);
    assert_eq!(rule.application_count, 0);
}
