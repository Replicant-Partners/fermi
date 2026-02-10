-- Seed Xaman Ek's ontology snapshot with the full system ER diagram.
-- This gives the platform navigator a live knowledge graph of the entire schema.

INSERT INTO ontology_snapshots (
    snapshot_id, agent_id, git_commit_sha, git_repository, git_path,
    pushed_to_remote, mermaid_content,
    entity_count, fact_count, community_count, rule_count,
    version, created_at
)
SELECT
    gen_random_uuid(),
    a.agent_id,
    'seed-034',
    'local',
    'ontology.mermaid',
    FALSE,
    $mermaid$
erDiagram

    %% ═══════════════════════════════════════════
    %% AUTHENTICATION DOMAIN
    %% ═══════════════════════════════════════════

    users {
        TEXT user_id PK
        TEXT email UK
        TEXT display_name
        TEXT avatar_url
        TEXT role "admin | developer | viewer"
        TEXT auth_provider "email | github | google | ethereum | legacy"
        TEXT github_username
        TEXT google_id
        TEXT ethereum_address UK
        TEXT stripe_customer_id
        TEXT bio
        TIMESTAMPTZ last_login_at
        TIMESTAMPTZ created_at
    }

    api_keys {
        UUID key_id PK
        UUID user_id FK
        TEXT key_hash
        TEXT key_prefix UK
        TEXT name
        TEXT_ARRAY scopes
        BOOLEAN is_active
        TIMESTAMPTZ expires_at
    }

    siwe_nonces {
        TEXT nonce PK
        TIMESTAMPTZ expires_at
    }

    notifications {
        UUID id PK
        TEXT user_id FK
        TEXT type
        TEXT title
        TEXT message
        BOOLEAN read
    }

    users ||--o{ api_keys : "authenticates via"
    users ||--o{ notifications : "receives"

    %% ═══════════════════════════════════════════
    %% WORKSPACE DOMAIN (teams = workspaces)
    %% ═══════════════════════════════════════════

    teams {
        UUID id PK
        TEXT name
        TEXT slug UK
        TEXT description
        TEXT owner_id FK
        INTEGER workspace_budget
        INTEGER workspace_spent
        TEXT git_repo_path
        TEXT git_latest_commit
        INTEGER git_commit_count
    }

    team_members {
        UUID team_id PK_FK
        TEXT member_id PK
        TEXT member_type "user | agent"
        TEXT role "owner | admin | member | viewer"
        TEXT invited_by
    }

    object_shares {
        UUID id PK
        TEXT object_type "agent | forecast | repo | file"
        TEXT object_id
        TEXT share_type "team | user"
        TEXT share_target
        TEXT permission "view | edit | admin"
        TEXT granted_by
    }

    workspace_messages {
        UUID message_id PK
        UUID workspace_id FK
        TEXT sender_type "user | agent | system"
        TEXT sender_id
        TEXT sender_name
        TEXT content
        TEXT message_type "chat | execution_result | system_event"
        JSONB metadata
    }

    workspace_agents {
        UUID workspace_id PK_FK
        UUID agent_id PK_FK
        TEXT added_by
        TEXT relationship "hired | owned | created_here"
    }

    coherence_evaluations {
        UUID eval_id PK
        UUID workspace_id FK
        FLOAT global_score
        TEXT quality_label
        JSONB principle_scores
        JSONB health_indicators
        INTEGER utterance_count
    }

    users ||--o{ teams : "owns"
    teams ||--o{ team_members : "has"
    teams ||--o{ workspace_messages : "contains"
    teams ||--o{ workspace_agents : "includes"
    teams ||--o{ coherence_evaluations : "evaluated by"

    %% ═══════════════════════════════════════════
    %% AGENT REGISTRY
    %% ═══════════════════════════════════════════

    agents {
        UUID agent_id PK
        TEXT agent_name UK
        TEXT display_alias
        TEXT agent_type
        TEXT version
        TEXT tier "curated | community | system"
        TEXT status "draft | published | archived"
        TEXT executor_type
        TEXT model
        FLOAT temperature
        TEXT llm_provider
        TEXT embedding_provider
        TEXT embedding_model
        INTEGER embedding_dimension
        TEXT system_prompt
        TEXT_ARRAY tags
        TEXT_ARRAY sample_queries
        TEXT visibility "private | unlisted | public"
        UUID forked_from FK
        INTEGER fork_count
        JSONB fork_pricing
        INTEGER total_executions
        INTEGER successful_executions
        INTEGER failed_executions
        BIGINT avg_execution_time_ms
        INTEGER dreaming_budget_credits
        INTEGER dreaming_credits_used
    }

    agent_versions {
        UUID version_id PK
        UUID agent_id FK
        INTEGER version_number
        TEXT system_prompt
        TEXT_ARRAY tags
        TEXT model
        FLOAT temperature
        TEXT changed_by
    }

    agents ||--o{ agent_versions : "versioned as"
    agents o|--o| agents : "forked from"
    workspace_agents }o--|| agents : "references"

    %% ═══════════════════════════════════════════
    %% ADM: EPISODIC MEMORY (Wake Phase)
    %% ═══════════════════════════════════════════

    episodes {
        UUID episode_id PK
        UUID agent_id FK
        TIMESTAMPTZ timestamp_ref
        TEXT query
        JSONB context
        TEXT execution_status
        TEXT error_details
        BIGINT execution_time_ms
        INTEGER tokens_used
        DECIMAL cost_usd
        VECTOR_1024 embedding
        TEXT_ARRAY tags
        BOOLEAN consolidated
        UUID consolidation_job_id FK
    }

    agents ||--o{ episodes : "generates"

    %% ═══════════════════════════════════════════
    %% ADM: SEMANTIC MEMORY (Sleep Phase)
    %% ═══════════════════════════════════════════

    semantic_rules {
        UUID rule_id PK
        UUID agent_id FK
        TEXT rule_content
        TEXT rule_description
        FLOAT confidence_score
        TEXT verification_status
        UUID_ARRAY source_episode_cluster
        INTEGER episode_count
        VECTOR_1024 embedding
        BOOLEAN is_active
        INTEGER application_count
        INTEGER successful_applications
    }

    agents ||--o{ semantic_rules : "learns"

    %% ═══════════════════════════════════════════
    %% ADM: KNOWLEDGE GRAPH
    %% ═══════════════════════════════════════════

    entities {
        UUID entity_id PK
        UUID agent_id FK
        TEXT entity_name
        TEXT entity_type
        TEXT summary
        TIMESTAMPTZ t_valid
        TIMESTAMPTZ t_invalid
        FLOAT extraction_confidence
        VECTOR_1024 embedding
        INTEGER version
        UUID replaces_entity_id FK
    }

    facts {
        UUID fact_id PK
        UUID agent_id FK
        UUID source_entity_id FK
        UUID target_entity_id FK
        TEXT relation_type
        TEXT relation_cardinality
        FLOAT confidence
        TEXT reasoning
        TIMESTAMPTZ t_valid
        TIMESTAMPTZ t_invalid
        INTEGER version
        UUID replaces_fact_id FK
    }

    communities {
        UUID community_id PK
        UUID agent_id FK
        TEXT community_name
        TEXT summary
        UUID_ARRAY member_entity_ids
        INTEGER member_count
        VECTOR_1024 embedding
    }

    agents ||--o{ entities : "discovers"
    agents ||--o{ facts : "extracts"
    agents ||--o{ communities : "clusters into"
    entities ||--o{ facts : "source of"
    entities ||--o{ facts : "target of"

    %% ═══════════════════════════════════════════
    %% ADM: CONSOLIDATION (Dreaming)
    %% ═══════════════════════════════════════════

    consolidation_jobs {
        UUID job_id PK
        UUID agent_id FK
        TIMESTAMPTZ started_at
        TIMESTAMPTZ completed_at
        BIGINT duration_ms
        TEXT status "running | completed | failed"
        INTEGER episodes_processed
        INTEGER clusters_identified
        INTEGER rules_extracted
        INTEGER entities_created
        INTEGER facts_created
        TEXT dream_synopsis
        UUID ontology_snapshot_id FK
    }

    consolidation_locks {
        UUID agent_id PK_FK
        TIMESTAMPTZ locked_at
        TEXT locked_by
        TIMESTAMPTZ expires_at
    }

    ontology_snapshots {
        UUID snapshot_id PK
        UUID agent_id FK
        TEXT git_commit_sha
        TEXT mermaid_content
        INTEGER entity_count
        INTEGER fact_count
        INTEGER community_count
        INTEGER rule_count
        INTEGER version
        TEXT dream_synopsis
        JSONB consolidation_stats
        UUID previous_snapshot_id FK
    }

    agents ||--o{ consolidation_jobs : "dreams via"
    agents ||--o| consolidation_locks : "locked by"
    agents ||--o{ ontology_snapshots : "snapshots"
    consolidation_jobs ||--o| ontology_snapshots : "produces"
    episodes }o--o| consolidation_jobs : "processed in"

    %% ═══════════════════════════════════════════
    %% ECONOMICS: CREDITS
    %% ═══════════════════════════════════════════

    wallets {
        UUID wallet_id PK
        TEXT owner_type "user | workspace"
        TEXT owner_id UK
        INTEGER balance
        INTEGER total_deposited
        INTEGER total_spent
    }

    credit_ledger {
        UUID tx_id PK
        UUID wallet_id FK
        INTEGER amount
        INTEGER balance_after
        TEXT tx_type
        TEXT description
        TEXT related_id
        TEXT stripe_session_id
    }

    users ||--o| wallets : "has wallet"
    teams ||--o| wallets : "has budget"
    wallets ||--o{ credit_ledger : "records"

    %% ═══════════════════════════════════════════
    %% MARKETPLACE
    %% ═══════════════════════════════════════════

    shopping_profiles {
        UUID profile_id PK
        TEXT user_id FK
        UUID agent_id FK
        TEXT profile_name
        VECTOR_1024 composite_embedding
        INTEGER episode_count
        TEXT_ARRAY category_tags
        FLOAT price_sensitivity
        FLOAT quality_bias
        JSONB brand_affinities
        BOOLEAN is_listed
    }

    marketplace_listings {
        UUID listing_id PK
        UUID profile_id FK
        TEXT seller_id
        INTEGER price_credits
        INTEGER total_queries
        INTEGER total_earned
        TEXT status "active | paused | delisted"
        TEXT_ARRAY category_tags
    }

    marketplace_transactions {
        UUID tx_id PK
        UUID listing_id FK
        TEXT buyer_id
        TEXT seller_id
        FLOAT similarity_score
        INTEGER credits_charged
        INTEGER credits_to_seller
        INTEGER platform_fee
    }

    shopping_profiles ||--o{ marketplace_listings : "listed as"
    marketplace_listings ||--o{ marketplace_transactions : "matched in"
    agents ||--o{ shopping_profiles : "profiled for"

    %% ═══════════════════════════════════════════
    %% EVALUATION FRAMEWORK
    %% ═══════════════════════════════════════════

    eval_test_cases {
        UUID test_case_id PK
        UUID agent_id FK
        TEXT query
        TEXT expected_output
        TEXT rubric
        TEXT_ARRAY tags
        BOOLEAN is_active
    }

    eval_runs {
        UUID run_id PK
        UUID agent_id FK
        TEXT triggered_by
        TEXT status "running | completed | failed"
        INTEGER total_cases
        INTEGER passed
        INTEGER failed
        FLOAT avg_judge_score
        INTEGER total_cost_credits
        JSONB case_results
        BOOLEAN regression_detected
        BIGINT duration_ms
    }

    agents ||--o{ eval_test_cases : "tested by"
    agents ||--o{ eval_runs : "evaluated in"

    %% ═══════════════════════════════════════════
    %% UTILITIES
    %% ═══════════════════════════════════════════

    waitlist {
        UUID id PK
        TEXT email UK
        TEXT source
        TEXT status
        TIMESTAMPTZ invited_at
    }
$mermaid$,
    28,   -- entity_count (tables)
    42,   -- fact_count (relationships)
    6,    -- community_count (domains)
    0,    -- rule_count
    1,    -- version
    NOW()
FROM agents a
WHERE a.agent_name = 'xaman_ek'
  AND NOT EXISTS (
    SELECT 1 FROM ontology_snapshots os WHERE os.agent_id = a.agent_id
  );

-- Point agent to this snapshot
UPDATE agents
SET current_ontology_snapshot_id = (
    SELECT snapshot_id FROM ontology_snapshots os
    JOIN agents a ON os.agent_id = a.agent_id
    WHERE a.agent_name = 'xaman_ek'
    ORDER BY os.created_at DESC LIMIT 1
),
current_ontology_commit = 'seed-034'
WHERE agent_name = 'xaman_ek'
  AND current_ontology_snapshot_id IS NULL;
