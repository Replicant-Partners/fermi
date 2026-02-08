//! Integration tests using the seed dataset.
//!
//! Run with: `cargo test -p agent-bestiary-memory --test test_seed -- --test-threads=1`
//! Requires DATABASE_URL environment variable.

use agent_bestiary_memory::{MemoryStore, SeedData};

async fn setup() -> (MemoryStore, SeedData) {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let store = MemoryStore::new(&database_url).await.unwrap();
    let seed = SeedData::build();
    seed.seed(&store).await.unwrap();
    (store, seed)
}

#[tokio::test]
async fn test_seed_and_cleanup() {
    let (store, seed) = setup().await;

    // Verify agents were created
    let agents = store.list_agents().await.unwrap();
    let seed_agents: Vec<_> = agents
        .iter()
        .filter(|a| a.agent_name.starts_with("seed_"))
        .collect();
    assert_eq!(seed_agents.len(), 3, "Expected 3 seed agents");

    // Verify agent names
    let names: Vec<&str> = seed_agents.iter().map(|a| a.agent_name.as_str()).collect();
    assert!(names.contains(&"seed_market_research"));
    assert!(names.contains(&"seed_geopolitical_risk"));
    assert!(names.contains(&"seed_crypto_sentiment"));

    // Cleanup
    seed.cleanup(&store).await.unwrap();

    // Verify cleanup
    let agents_after = store.list_agents().await.unwrap();
    let remaining: Vec<_> = agents_after
        .iter()
        .filter(|a| a.agent_name.starts_with("seed_"))
        .collect();
    assert_eq!(remaining.len(), 0, "All seed agents should be removed");
}

#[tokio::test]
async fn test_episode_queries() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    // Unconsolidated episodes: 25 total - 10 consolidated = 15
    let unconsolidated = store.get_unconsolidated_episodes(agent_id).await.unwrap();
    assert_eq!(
        unconsolidated.len(),
        15,
        "Expected 15 unconsolidated episodes"
    );

    // Episodes with embeddings: 18 success + 4 failure + 1 partial = 23
    let with_embeddings = store
        .get_all_episodes_with_embeddings(agent_id)
        .await
        .unwrap();
    assert_eq!(
        with_embeddings.len(),
        23,
        "Expected 23 episodes with embeddings"
    );

    // Failure episodes with embeddings: 4
    let failures = store
        .get_failure_episodes_with_embeddings(agent_id)
        .await
        .unwrap();
    // Only unconsolidated failures have embeddings (failures are indices 18-21, all unconsolidated)
    assert_eq!(
        failures.len(),
        4,
        "Expected 4 failure episodes with embeddings"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_episode_similarity_search() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    // Use the embedding from the first episode as query
    let first_ep = &seed.episodes_for(agent_id)[0];
    let query_embedding = first_ep.embedding.as_ref().unwrap();

    let results = store
        .search_similar_episodes(agent_id, query_embedding, 5)
        .await
        .unwrap();

    assert!(!results.is_empty(), "Should find similar episodes");
    assert!(results.len() <= 5, "Should respect limit");

    // First result should be the episode itself (distance ~0)
    let (closest, distance) = &results[0];
    assert_eq!(closest.episode_id, first_ep.episode_id);
    assert!(
        *distance < 0.01,
        "Self-similarity distance should be near 0"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_rule_lifecycle() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    let rules = store.get_agent_semantic_rules(agent_id).await.unwrap();
    assert_eq!(rules.len(), 6, "Expected 6 rules per agent");

    // Active rules: 4 (2 verified + 2 pending; rejected and superseded are deactivated)
    let active: Vec<_> = rules.iter().filter(|r| r.is_active).collect();
    assert_eq!(active.len(), 4, "Expected 4 active rules");

    // Verified rules among active
    let verified_active: Vec<_> = active
        .iter()
        .filter(|r| matches!(r.verification_status, agent_bestiary_memory::VerificationStatus::Verified))
        .collect();
    assert_eq!(verified_active.len(), 2, "Expected 2 active verified rules");

    // Rules with embeddings
    let with_embeddings: Vec<_> = rules.iter().filter(|r| r.embedding.is_some()).collect();
    assert_eq!(
        with_embeddings.len(),
        4,
        "Expected 4 rules with embeddings"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_entity_graph() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    let entities = store.get_agent_entities(agent_id).await.unwrap();
    // get_agent_entities filters for valid entities (t_invalid IS NULL OR > NOW())
    // 3 entities have t_invalid set in the past, so 7 valid
    assert!(
        entities.len() >= 7,
        "Expected at least 7 valid entities, got {}",
        entities.len()
    );

    // Check entity types are diverse
    let types: std::collections::HashSet<&str> =
        entities.iter().map(|e| e.entity_type.as_str()).collect();
    assert!(
        types.len() >= 3,
        "Expected at least 3 distinct entity types"
    );

    // Some entities should have no summary
    // The edge case entity with no summary might be invalidated, so check in seed data
    let seed_entities = seed.entities_for(agent_id);
    let no_summary: Vec<_> = seed_entities.iter().filter(|e| e.summary.is_none()).collect();
    assert!(
        !no_summary.is_empty(),
        "Expected at least 1 entity with no summary in seed data"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_fact_connectivity() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    let facts = store.get_agent_facts(agent_id).await.unwrap();
    // get_agent_facts filters for valid facts (t_invalid IS NULL OR > NOW())
    // 3 facts are invalidated, so 9 valid
    assert!(
        facts.len() >= 9,
        "Expected at least 9 valid facts, got {}",
        facts.len()
    );

    // Check cardinality diversity
    let cardinalities: std::collections::HashSet<String> = facts
        .iter()
        .map(|f| f.relation_cardinality.to_string())
        .collect();
    assert!(
        cardinalities.len() >= 3,
        "Expected at least 3 distinct cardinality types"
    );

    // Test entity-specific fact lookup
    let entities = seed.entities_for(agent_id);
    let first_entity_id = entities[0].entity_id;
    let entity_facts = store.get_entity_facts(first_entity_id).await.unwrap();
    assert!(
        !entity_facts.is_empty(),
        "First entity should have connected facts"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_community_membership() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    let communities = store.get_agent_communities(agent_id).await.unwrap();
    assert_eq!(communities.len(), 3, "Expected 3 communities per agent");

    // Verify member counts match member_entity_ids length
    for community in &communities {
        assert_eq!(
            community.member_count as usize,
            community.member_entity_ids.len(),
            "member_count should match member_entity_ids length"
        );
    }

    // Check edge cases
    let no_name: Vec<_> = communities
        .iter()
        .filter(|c| c.community_name.is_none())
        .collect();
    assert_eq!(no_name.len(), 1, "Expected 1 community with no name");

    let no_embedding: Vec<_> = communities
        .iter()
        .filter(|c| c.embedding.is_none())
        .collect();
    assert_eq!(
        no_embedding.len(),
        1,
        "Expected 1 community with no embedding"
    );

    // Verify member_entity_ids reference real entities
    let entity_ids: std::collections::HashSet<uuid::Uuid> = seed
        .entities_for(agent_id)
        .iter()
        .map(|e| e.entity_id)
        .collect();
    for community in &communities {
        for member_id in &community.member_entity_ids {
            assert!(
                entity_ids.contains(member_id),
                "Community member {} should reference a real entity",
                member_id
            );
        }
    }

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_consolidation_jobs() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    // We can't easily get jobs by agent_id (no such method), but we can verify
    // the seed created jobs by checking the agent's consolidation state
    // The jobs are created with create_consolidation_job which generates new UUIDs,
    // so we verify indirectly through the agent's episode consolidation state

    let unconsolidated = store.get_unconsolidated_episodes(agent_id).await.unwrap();
    assert_eq!(
        unconsolidated.len(),
        15,
        "15 episodes should remain unconsolidated"
    );

    // Verify consolidated episodes have consolidation_job_id set
    let all_episodes = store
        .get_all_episodes_with_embeddings(agent_id)
        .await
        .unwrap();
    let consolidated: Vec<_> = all_episodes.iter().filter(|e| e.consolidated).collect();
    assert_eq!(
        consolidated.len(),
        10,
        "10 episodes should be consolidated"
    );

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_cross_agent_data_isolation() {
    let (store, seed) = setup().await;

    // Each agent should have independent data
    for agent in &seed.agents {
        let episodes = seed.episodes_for(agent.agent_id);
        assert_eq!(episodes.len(), 25, "Each agent should have 25 episodes");

        let entities = seed.entities_for(agent.agent_id);
        assert_eq!(entities.len(), 10, "Each agent should have 10 entities");
    }

    // Verify DB queries also return isolated data
    let mr_episodes = store
        .get_all_episodes_with_embeddings(seed.market_research_agent().agent_id)
        .await
        .unwrap();
    let geo_episodes = store
        .get_all_episodes_with_embeddings(seed.geopolitical_risk_agent().agent_id)
        .await
        .unwrap();

    // No episode should appear in both
    let mr_ids: std::collections::HashSet<uuid::Uuid> =
        mr_episodes.iter().map(|e| e.episode_id).collect();
    for ep in &geo_episodes {
        assert!(
            !mr_ids.contains(&ep.episode_id),
            "Episodes should be isolated between agents"
        );
    }

    seed.cleanup(&store).await.unwrap();
}

#[tokio::test]
async fn test_dreaming_budget_states() {
    let (store, seed) = setup().await;

    // market_research: 10 budget, 3 used (has remaining)
    let mr = store
        .get_agent_by_name("seed_market_research")
        .await
        .unwrap();
    assert_eq!(mr.dreaming_budget_credits, 10);
    assert_eq!(mr.dreaming_credits_used, 3);

    // geopolitical_risk: 5 budget, 5 used (exhausted)
    let geo = store
        .get_agent_by_name("seed_geopolitical_risk")
        .await
        .unwrap();
    assert_eq!(geo.dreaming_budget_credits, 5);
    assert_eq!(geo.dreaming_credits_used, 5);

    // crypto_sentiment: 0 budget (free tier)
    let crypto = store
        .get_agent_by_name("seed_crypto_sentiment")
        .await
        .unwrap();
    assert_eq!(crypto.dreaming_budget_credits, 0);
    assert_eq!(crypto.dreaming_credits_used, 0);

    seed.cleanup(&store).await.unwrap();
}
