//! Integration tests using the seed dataset.
//!
//! Run with: `cargo test -p agent-bestiary-memory --test test_seed -- --test-threads=1`
//! Requires DATABASE_URL environment variable.
//!
//! ## Why the fixture is shared rather than per-test
//!
//! `SeedData::seed` writes ~150 rows one statement at a time. Against a
//! remote database that is ~3 minutes, and re-running it in every test made
//! this file take 36 minutes for 14 tests — roughly 2,100 sequential round
//! trips, of which all but ~150 were rebuilding a fixture that had just been
//! torn down. A run that long also has real odds of catching one transient
//! connection error, and any test that died mid-seed stranded the fixture and
//! failed every later test on a duplicate key.
//!
//! So `setup` is idempotent: it seeds only when the fixture is missing and is
//! otherwise a single COUNT. The data is read-only for almost every test, and
//! the exceptions own their writes — the composition tests create their own
//! workspace rows, and `test_seed_and_cleanup` deliberately tears the fixture
//! down, after which the next `setup` simply rebuilds it.
//!
//! Cost, measured: 36 min → 7 min from cold, and ~3 min when the fixture is
//! already present.
//!
//! ## The fixture outlives the run, on purpose
//!
//! Nothing deletes it at the end, which is what makes a warm run cheap. The
//! rows are namespaced by deterministic ids (`^0000000[0-2]-`) and the agents
//! are named `seed_*`, so they are easy to identify and safe to leave. Remove
//! them with `scripts/clean_seed_fixtures.sh --apply` — worth doing before
//! anything that asserts a global agent count.

use agent_bestiary_memory::{CompositionVersion, MemoryStore, SeedData};
use chrono::Utc;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Serialises the seed check so two tests can't both decide the fixture is
/// missing and race to insert it. Ordinary runs are `--test-threads=1`, but a
/// parallel run should degrade to "slow", not "duplicate key".
static SEED_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn setup() -> (MemoryStore, SeedData) {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set for tests");
    let store = MemoryStore::new(&database_url).await.unwrap();
    let seed = SeedData::build();

    let _guard = SEED_LOCK.lock().await;

    let agent_ids: Vec<Uuid> = seed.agents.iter().map(|a| a.agent_id).collect();
    let present: i64 = sqlx::query_scalar("SELECT count(*) FROM agents WHERE agent_id = ANY($1)")
        .bind(&agent_ids)
        .fetch_one(store.pool())
        .await
        .expect("count seeded agents");

    if present != agent_ids.len() as i64 {
        // Absent, or a previous run died partway through. Either way the
        // fixture is not trustworthy: clear whatever survived, then rebuild.
        // Without the clear, a partial fixture fails the reseed on the first
        // row it already has.
        seed.cleanup(&store).await.expect("clear partial fixture");
        seed.seed(&store).await.expect("seed fixture");
    }

    (store, seed)
}

/// A real workspace row, plus the user that has to own it.
///
/// `composition_versions.workspace_id` is a foreign key to `teams(id)`, and
/// `teams.owner_id` is in turn a foreign key to `users(user_id)`. So a bare
/// `Uuid::new_v4()` cannot stand in for a workspace the way these tests
/// assumed — the insert fails with a 23503, and because the failure happens
/// after `setup()` has already seeded, it leaves the fixture behind and every
/// subsequent test in the file dies on a duplicate key instead.
///
/// Returns the ids so the caller can remove both afterwards.
async fn create_test_workspace(store: &MemoryStore) -> (Uuid, String) {
    let owner_id = format!("test_ws_owner_{}", Uuid::new_v4());

    sqlx::query(
        "INSERT INTO users (user_id, email, password_hash, password_salt)
         VALUES ($1, $2, 'test-fixture', 'test-fixture')",
    )
    .bind(&owner_id)
    .bind(format!("{}@test.invalid", owner_id))
    .execute(store.pool())
    .await
    .expect("create workspace owner");

    let workspace_id: Uuid = sqlx::query_scalar(
        "INSERT INTO teams (name, slug, owner_id, origin)
         VALUES ($1, $2, $3, 'test_fixture')
         RETURNING id",
    )
    .bind("Seed Test Workspace")
    .bind(format!("test-ws-{}", Uuid::new_v4()))
    .bind(&owner_id)
    .fetch_one(store.pool())
    .await
    .expect("create test workspace");

    (workspace_id, owner_id)
}

/// Tear down what `create_test_workspace` made. Best-effort: a failure here
/// must not mask the assertion that the test actually cares about.
async fn cleanup_test_workspace(store: &MemoryStore, workspace_id: Uuid, owner_id: &str) {
    let _ = sqlx::query("DELETE FROM composition_versions WHERE workspace_id = $1")
        .bind(workspace_id)
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(workspace_id)
        .execute(store.pool())
        .await;
    let _ = sqlx::query("DELETE FROM users WHERE user_id = $1")
        .bind(owner_id)
        .execute(store.pool())
        .await;
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

    // The other half of this test's name. `cleanup` deletes agents and leans
    // on ON DELETE CASCADE for everything else, so assert the children are
    // actually gone rather than trusting the cascade — an agents-only delete
    // that left episodes behind is exactly the failure mode that would strand
    // the fixture and poison later runs.
    //
    // This is the one test that removes the shared fixture. That is safe:
    // `setup` rebuilds it on demand, so whichever test runs next re-seeds.
    seed.cleanup(&store).await.unwrap();

    let agent_ids: Vec<Uuid> = seed.agents.iter().map(|a| a.agent_id).collect();
    let remaining_agents: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agents WHERE agent_id = ANY($1)")
            .bind(&agent_ids)
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(remaining_agents, 0, "cleanup should remove seed agents");

    let remaining_episodes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM episodes WHERE agent_id = ANY($1)")
            .bind(&agent_ids)
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(remaining_episodes, 0, "cleanup should cascade to episodes");
}

// ── Composition version tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_composition_version_create_and_list() {
    let (store, seed) = setup().await;
    let (workspace_id, ws_owner) = create_test_workspace(&store).await;

    let version = CompositionVersion {
        composition_version_id: Uuid::new_v4(),
        workspace_id,
        version_number: 0, // overwritten by create
        mission: None,
        coordination_strategist_id: None,
        member_agent_ids: Some(vec![
            seed.market_research_agent().agent_id,
            seed.geopolitical_risk_agent().agent_id,
        ]),
        member_weights: Some(serde_json::json!({
            seed.market_research_agent().agent_id.to_string(): 0.6,
            seed.geopolitical_risk_agent().agent_id.to_string(): 0.4,
        })),
        diff_summary: Some("Add geopolitical agent to balance high-arousal homophily".to_string()),
        proposed_by: Some("cohere_and_coordinate".to_string()),
        accepted_by: None,
        rejected_by: None,
        rejection_note: Some(
            "Rationale: arousal spread was 0.1, below 0.25 threshold.".to_string(),
        ),
        created_at: Utc::now(),
    };

    let version_id = store.create_composition_version(&version).await.unwrap();
    assert_ne!(version_id, Uuid::nil());

    // List should return 1 pending version
    let versions = store.list_composition_versions(workspace_id).await.unwrap();
    assert_eq!(versions.len(), 1, "Expected 1 composition version");

    let v = &versions[0];
    assert_eq!(v.version_number, 1, "First version should be numbered 1");
    assert_eq!(v.proposed_by.as_deref(), Some("cohere_and_coordinate"));
    assert!(v.accepted_by.is_none(), "Should be pending (not accepted)");
    assert!(v.rejected_by.is_none(), "Should be pending (not rejected)");
    assert_eq!(
        v.member_agent_ids.as_ref().unwrap().len(),
        2,
        "Should have 2 member agents"
    );

    println!("✅ CompositionVersion create and list works!");
    cleanup_test_workspace(&store, workspace_id, &ws_owner).await;
}

#[tokio::test]
async fn test_composition_version_reject_stores_note() {
    let (store, _seed) = setup().await;
    let (workspace_id, ws_owner) = create_test_workspace(&store).await;

    let version = CompositionVersion {
        composition_version_id: Uuid::new_v4(),
        workspace_id,
        version_number: 0,
        mission: None,
        coordination_strategist_id: None,
        member_agent_ids: None,
        member_weights: None,
        diff_summary: Some("Proposed: reduce team to 2 agents".to_string()),
        proposed_by: Some("cohere_and_coordinate".to_string()),
        accepted_by: None,
        rejected_by: None,
        rejection_note: None,
        created_at: Utc::now(),
    };

    let version_id = store.create_composition_version(&version).await.unwrap();

    // Reject with a note
    store
        .resolve_composition_version(
            version_id,
            "owner_user_123",
            false,
            Some("Team size is intentional — we need the diversity"),
        )
        .await
        .unwrap();

    let versions = store.list_composition_versions(workspace_id).await.unwrap();
    assert_eq!(versions.len(), 1);
    let v = &versions[0];
    assert_eq!(v.rejected_by.as_deref(), Some("owner_user_123"));
    assert_eq!(
        v.rejection_note.as_deref(),
        Some("Team size is intentional — we need the diversity")
    );
    assert!(v.accepted_by.is_none(), "Should not be accepted");

    println!("✅ CompositionVersion rejection with note works!");
    cleanup_test_workspace(&store, workspace_id, &ws_owner).await;
}

#[tokio::test]
async fn test_composition_version_sequential_numbering() {
    let (store, _seed) = setup().await;
    let (workspace_id, ws_owner) = create_test_workspace(&store).await;

    // Create 3 versions for the same workspace
    for i in 0..3u32 {
        let v = CompositionVersion {
            composition_version_id: Uuid::new_v4(),
            workspace_id,
            version_number: 0,
            mission: Some(format!("Mission iteration {}", i)),
            coordination_strategist_id: None,
            member_agent_ids: None,
            member_weights: None,
            diff_summary: Some(format!("Change {}", i)),
            proposed_by: Some("cohere_and_coordinate".to_string()),
            accepted_by: None,
            rejected_by: None,
            rejection_note: None,
            created_at: Utc::now(),
        };
        store.create_composition_version(&v).await.unwrap();
    }

    // List should be newest-first, numbered 1-3
    let versions = store.list_composition_versions(workspace_id).await.unwrap();
    assert_eq!(versions.len(), 3);
    // Newest first → version_number DESC
    assert_eq!(versions[0].version_number, 3);
    assert_eq!(versions[1].version_number, 2);
    assert_eq!(versions[2].version_number, 1);

    println!("✅ CompositionVersion sequential numbering works!");
    cleanup_test_workspace(&store, workspace_id, &ws_owner).await;
}

// ── Valence round-trip test ──────────────────────────────────────────────────

#[tokio::test]
async fn test_valence_persists_through_update() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    let valence = serde_json::json!({
        "primary_affect": "analytical",
        "arousal": 0.4,
        "valence": 0.65,
        "personality_traits": ["precise", "evidence-driven", "calibrated"]
    });

    // Apply valence via AgentUpdate.
    //
    // Spread the `Default` rather than listing every field as `None`. The
    // exhaustive form broke this test the moment `AgentUpdate` grew
    // `mcp_servers`, `mcp_tools`, `output_contract`, `valence` and
    // `taxonomy` (commit 8983c063), which is a compile error in a test that
    // does not care about any of those fields — it sets one and asserts it
    // round-trips. `AgentUpdate` derives `Default`, so this states the
    // intent directly and stays correct as the struct grows.
    let update = agent_bestiary_memory::AgentUpdate {
        valence: Some(valence.clone()),
        ..Default::default()
    };

    store.update_agent(agent_id, &update).await.unwrap();

    // Read back
    let retrieved = store.get_agent(agent_id).await.unwrap().unwrap();
    let stored_valence = retrieved.valence.expect("valence should be stored");

    assert_eq!(
        stored_valence
            .get("primary_affect")
            .and_then(|v| v.as_str()),
        Some("analytical")
    );
    assert_eq!(
        stored_valence.get("arousal").and_then(|v| v.as_f64()),
        Some(0.4)
    );
    assert_eq!(
        stored_valence.get("valence").and_then(|v| v.as_f64()),
        Some(0.65)
    );
    let traits = stored_valence
        .get("personality_traits")
        .and_then(|v| v.as_array())
        .expect("personality_traits should be an array");
    assert_eq!(traits.len(), 3);

    println!("✅ Agent valence round-trip through update works!");
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
}

#[tokio::test]
async fn test_rule_lifecycle() {
    let (store, seed) = setup().await;
    let agent_id = seed.market_research_agent().agent_id;

    // `get_agent_semantic_rules` returns *active* rules only, by design: a
    // deactivated rule must not influence an agent. The fixture writes 6 per
    // agent and deactivates 2 (one Rejected, one superseded), so the getter
    // is expected to yield 4.
    //
    // This previously asserted 6 and then filtered for active itself, which
    // only makes sense against a getter that returns everything. The other
    // three assertions below were already written for the active-only view,
    // so 368 was the single line out of step.
    let rules = store.get_agent_semantic_rules(agent_id).await.unwrap();
    assert_eq!(rules.len(), 4, "Expected 4 active rules per agent");

    // The getter's contract: everything it hands back is active.
    let active: Vec<_> = rules.iter().filter(|r| r.is_active).collect();
    assert_eq!(
        active.len(),
        4,
        "get_agent_semantic_rules must return only active rules"
    );

    // The deactivation half of the lifecycle is invisible through the getter,
    // so assert it against the table directly — otherwise "rejected and
    // superseded get deactivated" is untested, which is the behaviour this
    // test is named for.
    let (total, inactive): (i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE NOT is_active)
           FROM semantic_rules WHERE agent_id = $1",
    )
    .bind(agent_id)
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(total, 6, "Expected 6 rules per agent on disk");
    assert_eq!(
        inactive, 2,
        "Expected rejected + superseded to be deactivated"
    );

    // Verified rules among active
    let verified_active: Vec<_> = active
        .iter()
        .filter(|r| {
            matches!(
                r.verification_status,
                agent_bestiary_memory::VerificationStatus::Verified
            )
        })
        .collect();
    assert_eq!(verified_active.len(), 2, "Expected 2 active verified rules");

    // Rules with embeddings
    let with_embeddings: Vec<_> = rules.iter().filter(|r| r.embedding.is_some()).collect();
    assert_eq!(with_embeddings.len(), 4, "Expected 4 rules with embeddings");
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
    let no_summary: Vec<_> = seed_entities
        .iter()
        .filter(|e| e.summary.is_none())
        .collect();
    assert!(
        !no_summary.is_empty(),
        "Expected at least 1 entity with no summary in seed data"
    );
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
    assert_eq!(consolidated.len(), 10, "10 episodes should be consolidated");
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
}

#[tokio::test]
async fn test_dreaming_budget_states() {
    let (store, _seed) = setup().await;

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
}
