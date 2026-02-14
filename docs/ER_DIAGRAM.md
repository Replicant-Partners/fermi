Perfect! I've now read all 72 migration files (migrations 004-075). Let me generate the comprehensive Entity-Relationship diagram and summary.

## Canonical Entity-Relationship Diagram

```mermaid
erDiagram
    %% ═══════════════════════════════════════════════════════════════════════
    %% USERS & AUTHENTICATION
    %% ═══════════════════════════════════════════════════════════════════════
    
    users {
        TEXT user_id PK "Zitadel/Ethereum address"
        TEXT email UK
        TEXT display_name
        TEXT avatar_url
        TEXT role "admin | developer | viewer"
        TEXT auth_provider "email | github | google | ethereum | legacy"
        TEXT github_username
        TEXT github_id
        TEXT google_id
        TEXT ethereum_address UK
        TEXT ens_name
        TEXT stripe_customer_id
        TEXT bio
        TEXT personal_workspace_id FK
        TIMESTAMPTZ last_login_at
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    api_keys {
        UUID key_id PK
        TEXT user_id FK
        TEXT key_hash
        TEXT key_prefix UK
        TEXT name
        TEXT[] scopes
        BOOLEAN is_active
        TIMESTAMPTZ expires_at
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
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
        TIMESTAMPTZ created_at
    }

    users ||--o{ api_keys : "owns"
    users ||--o{ notifications : "receives"

    %% ═══════════════════════════════════════════════════════════════════════
    %% TEAMS & WORKSPACES
    %% ═══════════════════════════════════════════════════════════════════════

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
        TEXT avatar_url
        TEXT workflow_mermaid
        JSONB workflow_meta
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    team_members {
        UUID team_id PK_FK
        TEXT member_id PK
        TEXT member_type "user | agent"
        TEXT role "owner | admin | member | viewer"
        TEXT invited_by
        TIMESTAMPTZ joined_at
    }

    object_shares {
        UUID id PK
        TEXT object_type "agent | rabble | forecast | repo | file"
        TEXT object_id
        TEXT share_type "team | user"
        TEXT share_target
        TEXT permission "view | edit | admin"
        TEXT granted_by
        TIMESTAMPTZ created_at
    }

    workspace_messages {
        UUID message_id PK
        UUID workspace_id FK
        TEXT sender_type "user | agent | system"
        TEXT sender_id
        TEXT sender_name
        TEXT content
        TEXT message_type "chat | execution_result | system_event | agent_invocation | coherence_update"
        TEXT audio_url
        JSONB metadata
        TIMESTAMPTZ created_at
    }

    workspace_agents {
        UUID workspace_id PK_FK
        UUID agent_id PK_FK
        TEXT added_by
        TEXT relationship "hired | owned | created_here | system"
        TIMESTAMPTZ added_at
    }

    coherence_evaluations {
        UUID eval_id PK
        UUID workspace_id FK
        DOUBLE global_score
        TEXT quality_label
        JSONB principle_scores
        JSONB health_indicators
        INTEGER utterance_count
        JSONB message_window
        TIMESTAMPTZ created_at
    }

    users ||--o{ teams : "owns"
    teams ||--o{ team_members : "has"
    teams ||--o{ workspace_messages : "contains"
    teams ||--o{ workspace_agents : "includes"
    teams ||--o{ coherence_evaluations : "evaluates"
    teams ||--o{ object_shares : "subject of"

    %% ═══════════════════════════════════════════════════════════════════════
    %% AGENTS & REGISTRY
    %% ═══════════════════════════════════════════════════════════════════════

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
        TEXT[] tags
        TEXT[] sample_queries
        TEXT[] accepts
        TEXT[] produces
        TEXT visibility "private | unlisted | public"
        JSONB fork_pricing
        JSONB workflow_template
        TEXT prompt_template
        JSONB requires_secrets
        UUID forked_from FK
        INTEGER fork_count
        INTEGER auto_collect_pct
        INTEGER total_executions
        INTEGER successful_executions
        INTEGER failed_executions
        BIGINT avg_execution_time_ms
        INTEGER dreaming_budget_credits
        INTEGER dreaming_credits_used
        INTEGER education_budget_credits
        INTEGER education_credits_used
        TIMESTAMPTZ dreaming_budget_reset_at
        UUID current_ontology_snapshot_id FK
        TEXT current_ontology_commit
        TIMESTAMPTZ last_consolidated_at
        TIMESTAMPTZ created_at
    }

    agent_versions {
        UUID version_id PK
        UUID agent_id FK
        INTEGER version_number
        TEXT system_prompt
        TEXT[] tags
        TEXT model
        FLOAT temperature
        TEXT visibility
        TEXT display_alias
        TEXT changed_by
        TEXT description
        TIMESTAMPTZ created_at
    }

    agents ||--o{ agent_versions : "versioned"
    agents o|--o| agents : "forked_from"
    workspace_agents }o--|| agents : "references"

    %% ═══════════════════════════════════════════════════════════════════════
    %% ADM: EPISODIC MEMORY
    %% ═══════════════════════════════════════════════════════════════════════

    episodes {
        UUID episode_id PK
        UUID agent_id FK
        TEXT user_id FK
        TIMESTAMPTZ timestamp_ref
        TIMESTAMPTZ timestamp_created
        TEXT query
        JSONB context
        TEXT execution_status
        TEXT error_details
        BIGINT execution_time_ms
        INTEGER tokens_used
        DECIMAL cost_usd
        VECTOR embedding "1024-dim"
        TEXT[] tags
        BOOLEAN consolidated
        UUID consolidation_job_id FK
        UUID cluster_id
        TIMESTAMPTZ created_at
    }

    agents ||--o{ episodes : "generates"

    %% ═══════════════════════════════════════════════════════════════════════
    %% ADM: SEMANTIC MEMORY
    %% ═══════════════════════════════════════════════════════════════════════

    semantic_rules {
        UUID rule_id PK
        UUID agent_id FK
        TEXT user_id FK
        TEXT rule_content
        TEXT rule_description
        FLOAT confidence_score
        TEXT verification_status
        UUID[] source_episode_cluster
        INTEGER episode_count
        VECTOR embedding "1024-dim"
        BOOLEAN is_active
        INTEGER application_count
        INTEGER successful_applications
        INTEGER failed_applications
        TIMESTAMPTZ last_validated_at
        TIMESTAMPTZ invalidated_at
        TEXT invalidation_reason
        TIMESTAMPTZ created_at
    }

    agents ||--o{ semantic_rules : "learns"

    %% ═══════════════════════════════════════════════════════════════════════
    %% ADM: KNOWLEDGE GRAPH
    %% ═══════════════════════════════════════════════════════════════════════

    entities {
        UUID entity_id PK
        UUID agent_id FK
        TEXT entity_name
        TEXT entity_type
        TEXT summary
        FLOAT extraction_confidence
        VECTOR embedding "1024-dim"
        INTEGER version
        UUID replaces_entity_id FK
        TIMESTAMPTZ t_valid
        TIMESTAMPTZ t_invalid
        TIMESTAMPTZ t_created
        TIMESTAMPTZ t_expired
        UUID[] source_episodes
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
        INTEGER version
        UUID replaces_fact_id FK
        TIMESTAMPTZ t_valid
        TIMESTAMPTZ t_invalid
        TIMESTAMPTZ t_created
        TIMESTAMPTZ t_expired
        UUID[] source_episodes
    }

    communities {
        UUID community_id PK
        UUID agent_id FK
        TEXT community_name
        TEXT summary
        UUID[] member_entity_ids
        INTEGER member_count
        VECTOR embedding "1024-dim"
        TIMESTAMPTZ created_at
        TIMESTAMPTZ last_propagation_at
    }

    agents ||--o{ entities : "discovers"
    agents ||--o{ facts : "extracts"
    agents ||--o{ communities : "clusters into"
    entities ||--o{ facts : "source"
    entities ||--o{ facts : "target"
    entities o|--o| entities : "replaces"
    facts o|--o| facts : "replaces"

    %% ═══════════════════════════════════════════════════════════════════════
    %% ADM: CONSOLIDATION (DREAMING)
    %% ═══════════════════════════════════════════════════════════════════════

    consolidation_jobs {
        UUID job_id PK
        UUID agent_id FK
        TIMESTAMPTZ started_at
        TIMESTAMPTZ completed_at
        BIGINT duration_ms
        TEXT status "running | completed | failed"
        TEXT error_message
        UUID episode_range_start
        UUID episode_range_end
        INTEGER episodes_processed
        INTEGER clusters_identified
        INTEGER rules_extracted
        INTEGER rules_verified
        INTEGER rules_rejected
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
        TEXT git_repository
        TEXT git_path
        TEXT github_url
        BOOLEAN pushed_to_remote
        INTEGER entity_count
        INTEGER fact_count
        INTEGER community_count
        INTEGER rule_count
        TEXT mermaid_content
        INTEGER version
        TEXT dream_synopsis
        JSONB consolidation_stats
        TEXT audio_url
        UUID previous_snapshot_id FK
        TIMESTAMPTZ created_at
    }

    agents ||--o{ consolidation_jobs : "dreams"
    agents ||--o| consolidation_locks : "locked"
    agents ||--o{ ontology_snapshots : "snapshots"
    consolidation_jobs ||--o| ontology_snapshots : "produces"
    ontology_snapshots o|--o| ontology_snapshots : "previous"

    %% ═══════════════════════════════════════════════════════════════════════
    %% ECONOMICS: CREDITS & WALLETS
    %% ═══════════════════════════════════════════════════════════════════════

    wallets {
        UUID wallet_id PK
        TEXT owner_type "user | workspace | agent"
        TEXT owner_id UK
        INTEGER balance
        INTEGER granted_balance
        INTEGER purchased_balance
        INTEGER total_deposited
        INTEGER total_spent
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
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
        TIMESTAMPTZ created_at
    }

    users ||--o| wallets : "has"
    teams ||--o| wallets : "has"
    wallets ||--o{ credit_ledger : "records"

    %% ═══════════════════════════════════════════════════════════════════════
    %% MARKETPLACE
    %% ═══════════════════════════════════════════════════════════════════════

    shopping_profiles {
        UUID profile_id PK
        TEXT user_id FK
        UUID agent_id FK
        TEXT profile_name
        VECTOR composite_embedding "1024-dim"
        INTEGER embedding_version
        INTEGER episode_count
        TEXT[] category_tags
        FLOAT price_sensitivity
        FLOAT quality_bias
        JSONB brand_affinities
        JSONB metadata
        BOOLEAN is_listed
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    marketplace_listings {
        UUID listing_id PK
        UUID profile_id FK
        TEXT seller_id
        INTEGER price_credits
        INTEGER max_queries_per_buyer
        INTEGER total_queries
        INTEGER total_earned
        TEXT status "active | paused | delisted"
        TEXT[] category_tags
        TEXT description
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    marketplace_transactions {
        UUID tx_id PK
        UUID listing_id FK
        TEXT buyer_id
        TEXT seller_id
        FLOAT similarity_score
        TEXT product_embedding_hash
        INTEGER credits_charged
        INTEGER credits_to_seller
        INTEGER platform_fee
        TIMESTAMPTZ created_at
    }

    shopping_profiles ||--o{ marketplace_listings : "listed"
    marketplace_listings ||--o{ marketplace_transactions : "matched"
    agents ||--o{ shopping_profiles : "profiled"

    %% ═══════════════════════════════════════════════════════════════════════
    %% EVALUATION FRAMEWORK
    %% ═══════════════════════════════════════════════════════════════════════

    eval_test_cases {
        UUID test_case_id PK
        UUID agent_id FK
        TEXT query
        TEXT expected_output
        TEXT rubric
        TEXT[] tags
        BOOLEAN is_active
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    eval_runs {
        UUID run_id PK
        UUID agent_id FK
        TEXT triggered_by
        TEXT status "running | completed | failed"
        BOOLEAN judge_enabled
        INTEGER total_cases
        INTEGER passed
        INTEGER failed
        BIGINT avg_latency_ms
        INTEGER avg_tokens
        FLOAT avg_judge_score
        INTEGER total_cost_credits
        JSONB case_results
        BOOLEAN regression_detected
        JSONB regression_details
        TIMESTAMPTZ started_at
        TIMESTAMPTZ completed_at
        BIGINT duration_ms
    }

    agents ||--o{ eval_test_cases : "tested"
    agents ||--o{ eval_runs : "evaluated"

    %% ═══════════════════════════════════════════════════════════════════════
    %% UTILITIES & TELEMETRY
    %% ═══════════════════════════════════════════════════════════════════════

    waitlist {
        UUID id PK
        TEXT email UK
        TEXT source
        TEXT status
        TIMESTAMPTZ invited_at
        TEXT notes
        TIMESTAMPTZ created_at
    }

    user_secrets {
        UUID secret_id PK
        TEXT user_id FK
        TEXT secret_name
        BYTEA encrypted_value
        BYTEA nonce
        TEXT scope
        TEXT label
        TEXT description
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    secret_access_log {
        UUID log_id PK
        TEXT user_id FK
        TEXT secret_name
        TEXT agent_name
        UUID workspace_id FK
        TEXT action "read | used | created | updated | deleted"
        TEXT tool_name
        TEXT ip_address
        TIMESTAMPTZ created_at
    }

    ar_beacons {
        UUID beacon_id PK
        UUID workspace_id FK
        TEXT creator_id
        TEXT agent_name
        TEXT h3_cell
        INT h3_resolution
        DOUBLE center_lat
        DOUBLE center_lng
        TEXT asset_path
        TEXT asset_type
        DOUBLE azimuth_deg
        DOUBLE elevation_deg
        BOOLEAN billboard
        DOUBLE scale
        INT ttl_seconds
        TEXT decay_style
        TEXT visibility
        JSONB tags
        JSONB interaction
        JSONB metadata
        TIMESTAMPTZ expires_at
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    ar_choreographies {
        UUID choreo_id PK
        UUID beacon_id FK
        UUID workspace_id FK
        TEXT name
        TEXT description
        JSONB motion
        INT duration_total_ms
        BOOLEAN loop_motion
        BOOLEAN active
        INT priority
        JSONB triggers
        TIMESTAMPTZ created_at
    }

    ar_grid_maps {
        UUID map_id PK
        UUID workspace_id FK
        TEXT creator_id
        TEXT name
        TEXT description
        DOUBLE center_lat
        DOUBLE center_lng
        TEXT center_h3
        INT center_resolution
        INT grid_resolution
        INT radius_rings
        INT total_cells
        JSONB quadrants
        JSONB zones
        JSONB metadata
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    teams ||--o{ ar_beacons : "contains"
    ar_beacons ||--o{ ar_choreographies : "has"
    teams ||--o{ ar_grid_maps : "defines"

    %% ═══════════════════════════════════════════════════════════════════════
    %% CREATURES & FLIGHTS (RABBLE)
    %% ═══════════════════════════════════════════════════════════════════════

    creatures {
        UUID creature_id PK
        TEXT owner_id FK
        UUID workspace_id FK
        BIGINT gbif_key
        TEXT scientific_name
        TEXT common_name
        TEXT species_group
        JSONB taxonomy
        TEXT specimen_name
        TEXT asset_path
        TEXT flight_silhouette_path
        TEXT variation_notes
        JSONB generation_params
        INT mint_number
        INT total_flights
        BIGINT total_flight_time_seconds
        INT unique_locations
        TEXT status
        BOOLEAN flagged
        TEXT flag_reason
        TEXT presence "active | sleeping | parked"
        UUID parked_at_workspace FK
        TIMESTAMP presence_changed_at
        TEXT visibility "public | contacts | private"
        INT attraction_score
        BOOLEAN sosa_opt_in
        TEXT animation_status
        JSONB data_card
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    creature_flights {
        UUID flight_id PK
        UUID creature_id FK
        UUID beacon_id FK
        TEXT owner_id FK
        TEXT h3_cell
        INT h3_resolution
        DOUBLE center_lat
        DOUBLE center_lng
        TEXT location_name
        TEXT country_code
        TEXT flight_pattern
        UUID choreo_id FK
        UUID swarm_id FK
        UUID sub_flock_id FK
        UUID attracted_by_creature_id FK
        TEXT data_source "synthetic | device"
        TEXT visibility
        JSONB path_samples
        JSONB environment
        TIMESTAMPTZ started_at
        TIMESTAMPTZ ended_at
        INT duration_seconds
    }

    swarm_events {
        UUID swarm_id PK
        TEXT creator_id
        UUID workspace_id FK
        TEXT h3_cell
        INT h3_resolution
        DOUBLE center_lat
        DOUBLE center_lng
        TEXT location_name
        UUID grid_map_id FK
        TEXT name
        TEXT description
        TEXT species_filter
        INT max_participants
        TIMESTAMPTZ starts_at
        TIMESTAMPTZ ends_at
        TEXT status
        INT participant_count
        INT creature_count
        TEXT funding_mode
        INT invite_pool
        INT invite_pool_remaining
        INT suggested_contribution
        INT total_contributions
        TEXT qr_token UK
        TEXT visibility
        UUID anchor_creature_id FK
        TIMESTAMPTZ anchor_transferred_at
        INT walk_in_price
        INT walk_in_budget
        INT walk_in_budget_remaining
        JSONB metadata
        TIMESTAMPTZ created_at
    }

    swarm_sub_flocks {
        UUID sub_flock_id PK
        UUID swarm_id FK
        TEXT owner_id
        TEXT name
        TEXT species_filter
        UUID formation_algorithm_id FK
        TIMESTAMPTZ created_at
    }

    rabble_messages {
        UUID message_id PK
        UUID swarm_id FK
        TEXT sender_id
        UUID creature_id FK
        TEXT creature_name
        TEXT species_name
        TEXT species_group
        TEXT content
        TEXT message_type
        TIMESTAMPTZ created_at
    }

    creature_collections {
        UUID collection_id PK
        TEXT owner_id
        TEXT name
        TEXT description
        JSONB creature_ids
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    creature_images {
        UUID creature_id PK_FK
        BYTEA image_bytes
        TEXT mime_type
        INT file_size
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    creature_animation_layers {
        UUID creature_id PK_FK
        TEXT layer_name "body | left_wing | right_wing"
        BYTEA image_bytes
        TEXT mime_type
        INT file_size
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    creature_devices {
        UUID device_id PK
        UUID creature_id FK
        TEXT owner_id
        TEXT device_type
        TEXT device_identifier UK
        TEXT device_name
        BOOLEAN is_active
        DOUBLE last_lat
        DOUBLE last_lng
        TIMESTAMPTZ last_seen_at
        JSONB metadata
        TIMESTAMPTZ created_at
    }

    creature_tethers {
        UUID tether_id PK
        UUID creature_id FK
        TEXT owner_id
        TEXT tether_type "phone_gps | meshtastic | gps_tracker | fixed_sensor"
        TEXT device_label
        JSONB config
        BOOLEAN active
        TIMESTAMPTZ created_at
        TIMESTAMPTZ deactivated_at
    }

    telemetry_points {
        UUID point_id PK
        UUID tether_id FK
        UUID creature_id FK
        DOUBLE lat
        DOUBLE lng
        DOUBLE altitude
        DOUBLE accuracy
        DOUBLE speed
        DOUBLE heading
        JSONB metadata
        TIMESTAMPTZ recorded_at
    }

    users ||--o{ creatures : "owns"
    teams ||--o{ creatures : "contains"
    creatures ||--o{ creature_flights : "flies"
    creatures ||--o{ creature_images : "has_image"
    creatures ||--o{ creature_animation_layers : "has_layers"
    creatures ||--o{ creature_devices : "paired_with"
    creatures ||--o{ creature_tethers : "tethered_to"
    creatures ||--o{ creature_collections : "grouped_in"
    creature_flights }o--o| ar_beacons : "from"
    creature_flights }o--o| swarm_events : "part_of"
    creature_flights }o--o| swarm_sub_flocks : "grouped_in"
    swarm_events ||--o{ swarm_sub_flocks : "contains"
    swarm_events ||--o{ rabble_messages : "records"
    swarm_events }o--o| ar_grid_maps : "located_in"
    creature_tethers ||--o{ telemetry_points : "generates"

    %% ═══════════════════════════════════════════════════════════════════════
    %% SWARM ALGORITHMS & FLOCKING
    %% ═══════════════════════════════════════════════════════════════════════

    swarm_algorithms {
        UUID algorithm_id PK
        TEXT name UK
        TEXT display_name
        TEXT description
        TEXT category "formation | team_action"
        TEXT onto4mat_class
        JSONB formation_spec
        TEXT tier
        INT cost_credits
        TEXT icon
        TIMESTAMPTZ created_at
    }

    swarm_activations {
        UUID activation_id PK
        UUID algorithm_id FK
        TEXT user_id
        UUID swarm_id FK
        TIMESTAMPTZ activated_at
    }

    swarm_sessions {
        UUID session_id PK
        TEXT owner_id
        TEXT name
        TEXT description
        INT agent_count
        TEXT formation_type
        TEXT mission_type
        JSONB environment
        TEXT status
        TIMESTAMPTZ started_at
        TIMESTAMPTZ ended_at
        JSONB metadata
    }

    swarm_telemetry {
        UUID telemetry_id PK
        UUID session_id FK
        TEXT agent_label
        TEXT agent_type
        BIGINT timestamp_ms
        DOUBLE x_location
        DOUBLE y_location
        DOUBLE z_location
        DOUBLE heading
        DOUBLE speed
        DOUBLE energy
        DOUBLE distance_to_goal
        DOUBLE team_alignment
        DOUBLE team_cohesion
        DOUBLE team_separation
        DOUBLE influence
        TEXT action
        TEXT temperament
        JSONB extra
    }

    swarm_algorithms ||--o{ swarm_activations : "used_in"
    swarm_events ||--o{ swarm_activations : "activates"
    swarm_sessions ||--o{ swarm_telemetry : "contains"

    %% ═══════════════════════════════════════════════════════════════════════
    %% SOSA OBSERVATIONS (UNIVERSAL SENSOR)
    %% ═══════════════════════════════════════════════════════════════════════

    sosa_platforms {
        UUID platform_id PK
        TEXT owner_id
        TEXT name
        TEXT platform_type
        TEXT description
        JSONB location
        JSONB metadata
        TIMESTAMPTZ created_at
    }

    sosa_sensors {
        UUID sensor_id PK
        UUID platform_id FK
        TEXT name
        TEXT observable_property
        TEXT unit
        TEXT description
        JSONB metadata
        TIMESTAMPTZ created_at
    }

    observation_sessions {
        UUID session_id PK
        TEXT owner_id
        UUID platform_id FK
        TEXT name
        TEXT description
        TEXT status
        TIMESTAMPTZ started_at
        TIMESTAMPTZ ended_at
        JSONB metadata
    }

    sosa_observations {
        UUID observation_id PK
        UUID session_id FK
        UUID sensor_id FK
        UUID platform_id FK
        TEXT observable_property
        TEXT feature_of_interest
        DOUBLE result_value
        TEXT result_unit
        BIGINT phenomenon_time
        BIGINT result_time
        TEXT procedure
        JSONB extra
    }

    sosa_platforms ||--o{ sosa_sensors : "has"
    sosa_platforms ||--o{ observation_sessions : "hosts"
    observation_sessions ||--o{ sosa_observations : "contains"
    sosa_sensors ||--o{ sosa_observations : "measures"

    %% ═══════════════════════════════════════════════════════════════════════
    %% FERMI NOTEBOOKS & FORECASTING
    %% ═══════════════════════════════════════════════════════════════════════

    fermi_notebooks {
        TEXT id PK
        UUID owner_id FK
        UUID team_id FK
        TEXT title
        TEXT description
        TEXT visibility "private | shared | public"
        TEXT org_id
        JSONB cells
        TEXT execution_state
        TIMESTAMPTZ last_executed_at
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    fermi_portfolios {
        TEXT id PK
        UUID owner_id FK
        UUID team_id FK
        TEXT title
        TEXT description
        TEXT visibility "private | shared | public"
        TEXT org_id
        TEXT[] notebook_ids
        JSONB metadata
        TIMESTAMPTZ created_at
        TIMESTAMPTZ updated_at
    }

    fermi_forecasts {
        TEXT id PK
        TEXT notebook_id FK
        UUID owner_id FK
        TEXT question_text
        REAL predicted_probability
        REAL confidence_interval_low
        REAL confidence_interval_high
        TIMESTAMPTZ resolution_date
        BOOLEAN actual_outcome
        REAL brier_score
        JSONB metadata
        TIMESTAMPTZ created_at
        TIMESTAMPTZ resolved_at
    }

    fermi_portfolio_forecasts {
        TEXT portfolio_id PK_FK
        TEXT forecast_id PK_FK
        TIMESTAMPTZ added_at
    }

    fermi_notebooks ||--o{ fermi_forecasts : "contains"
    fermi_portfolios ||--o{ fermi_portfolio_forecasts : "includes"
    fermi_forecasts ||--o{ fermi_portfolio_forecasts : "part_of"

    %% ═══════════════════════════════════════════════════════════════════════
    %% AKP: AGENT KNOWLEDGE PROTOCOL
    %% ═══════════════════════════════════════════════════════════════════════

    agent_alignments {
        UUID alignment_id PK
        UUID source_agent_id FK
        UUID target_agent_id FK
        FLOAT alignment_score
        INT shared_entity_count
        INT divergent_entity_count
        JSONB shared_entities
        JSONB divergent_entities
        TIMESTAMPTZ last_computed_at
    }

    pairwise_coherence {
        UUID coherence_id PK
        UUID agent_a_id FK
        UUID agent_b_id FK
        UUID workspace_id FK
        FLOAT global_score
        JSONB principle_scores
        INT episode_count
        TIMESTAMPTZ computed_at
    }

    knowledge_transfers {
        UUID transfer_id PK
        UUID source_agent_id FK
        UUID target_agent_id FK
        TEXT transfer_type
        INT item_count
        INT accepted_count
        INT rejected_count
        INT conflict_count
        JSONB details
        TIMESTAMPTZ transferred_at
    }

    agent_interaction_policies {
        UUID policy_id PK
        UUID agent_id FK
        TEXT policy_type
        UUID target_agent_id FK
        BOOLEAN enabled
        TIMESTAMPTZ created_at
    }

    agents ||--o{ agent_alignments : "alignment_source"
    agents ||--o{ agent_alignments : "alignment_target"
    agents ||--o{ pairwise_coherence : "coherence_a"
    agents ||--o{ pairwise_coherence : "coherence_b"
    agents ||--o{ knowledge_transfers : "transfer_source"
    agents ||--o{ knowledge_transfers : "transfer_target"
    agents ||--o{ agent_interaction_policies : "subject"

    %% ═══════════════════════════════════════════════════════════════════════
    %% SOCIAL & CONTACT MANAGEMENT
    %% ═══════════════════════════════════════════════════════════════════════

    contacts {
        UUID id PK
        TEXT user_id FK
        TEXT contact_id FK
        TEXT nickname
        TIMESTAMPTZ created_at
    }

    voice_assets {
        UUID asset_id PK
        TEXT object_type "episode | message | creature | synopsis"
        TEXT object_id
        TEXT provider "cartesia | elevenlabs"
        TEXT voice_id
        INT duration_ms
        INT character_count
        TEXT storage_url
        TIMESTAMPTZ created_at
    }

    agent_avatars {
        TEXT agent_id PK
        JSONB avatar_json
        TIMESTAMPTZ created_at
    }

    %% ═══════════════════════════════════════════════════════════════════════
    %% AGENT PAYOUT TRACKING
    %% ═══════════════════════════════════════════════════════════════════════

    agent_episode_payouts {
        UUID payout_id PK
        UUID episode_id FK
        UUID agent_id FK
        UUID workspace_id FK
        INT amount
        TEXT contribution_tier
        TIMESTAMPTZ created_at
    }

    episodes ||--o{ agent_episode_payouts : "payouts"
    agents ||--o{ agent_episode_payouts : "earns"
```

---

## Plain-Text Schema Summary

### Total Table Count: 67 Tables

### Table Breakdown by Domain:

#### **Users & Authentication (6 tables)**
- `users` — Multi-provider auth (Zitadel, GitHub, Google, SIWE)
- `api_keys` — Programmatic access with Argon2 hashing
- `siwe_nonces` — Ethereum replay protection
- `notifications` — In-app alerts
- `user_secrets` — Encrypted credential storage
- `secret_access_log` — Audit trail for secret access

#### **Teams & Workspaces (6 tables)**
- `teams` — Workspaces with shared budget and git tracking
- `team_members` — Polymorphic membership (user/agent)
- `object_shares` — Polymorphic sharing (agent/file/rabble/etc)
- `workspace_messages` — Chat, execution results, system events
- `workspace_agents` — Agent availability in workspace
- `coherence_evaluations` — TEC evaluation results

#### **Agents & Registry (4 tables)**
- `agents` — Full agent spec with LLM provider, embedding config, fork tracking
- `agent_versions` — Mutable field history with rollback support
- `agent_avatars` — Cached avatar JSON

#### **ADM: Episodic Memory (1 table)**
- `episodes` — Wake-phase execution logs with 1024-dim embeddings

#### **ADM: Semantic Memory (1 table)**
- `semantic_rules` — Extracted patterns with confidence & verification status

#### **ADM: Knowledge Graph (3 tables)**
- `entities` — GBIF/domain concepts with temporal validity
- `facts` — Entity relationships (source→target) with type cardinality
- `communities` — Entity clusters from graph algorithms

#### **ADM: Consolidation/Dreaming (3 tables)**
- `consolidation_jobs` — Sleep-phase batch jobs with dream synopses
- `consolidation_locks` — Prevents concurrent consolidation
- `ontology_snapshots` — Versioned mermaid ER + dream narrative

#### **Economics: Credits (2 tables)**
- `wallets` — Dual balance (granted/purchased) for user/workspace/agent
- `credit_ledger` — Append-only transaction log (50+ tx_types)

#### **Marketplace (3 tables)**
- `shopping_profiles` — Consumer profiles with composite embeddings
- `marketplace_listings` — Profile listing for sale with price/tags
- `marketplace_transactions` — Match records with similarity scores

#### **Evaluation Framework (2 tables)**
- `eval_test_cases` — Enriched sample queries with rubrics
- `eval_runs` — Full evaluation batches with regression detection

#### **Utilities & Telemetry (5 tables)**
- `waitlist` — Early-access list
- `ar_beacons` — H3 hex AR asset placement
- `ar_choreographies` — Motion sequences on beacons
- `ar_grid_maps` — Named spatial grids with quadrants/zones
- `voice_assets` — TTS-generated audio for episodes/messages

#### **Creatures & Flights (12 tables)**
- `creatures` — Minted AR insects with GBIF taxonomy, mint #, flight stats
- `creature_flights` — Every flight log with H3 location, path samples, environment
- `creature_images` — Persisted PNG bytes (ephemeral FS workaround)
- `creature_animation_layers` — Segmented body/wing images for Chen animation
- `creature_devices` — GPS tracker/BLE beacon pairing
- `creature_tethers` — Live signal tethering (phone_gps, meshtastic, etc)
- `telemetry_points` — Timestamped position stream from tethered creatures
- `creature_collections` — Named groupings of creatures
- `swarm_events` — Rabble gathering with H3, visibility, funding modes, QR token
- `swarm_sub_flocks` — Named creature groups within rabble with formation assignment
- `rabble_messages` — Creature-attributed chat in rabble
- `creature_tethers` — Live tethering to GPS sources

#### **Swarm Algorithms & Flocking (4 tables)**
- `swarm_algorithms` — Onto4MAT formation/team_action specs (11 seeded)
- `swarm_activations` — Per-user algorithm activation for swarm
- `swarm_sessions` — Telemetry collection windows
- `swarm_telemetry` — High-frequency Onto4MAT data points

#### **SOSA Universal Sensor (4 tables)**
- `sosa_platforms` — Sensor hosts (drone, weather station, wearable, etc)
- `sosa_sensors` — Observable properties per platform
- `observation_sessions` — Collection windows (generic)
- `sosa_observations` — Timestamped W3C SSN/SOSA data points

#### **Fermi Notebooks & Forecasting (4 tables)**
- `fermi_notebooks` — Structured research notebooks with execution state
- `fermi_portfolios` — Named collections of notebooks
- `fermi_forecasts` — Brier-scored probability predictions
- `fermi_portfolio_forecasts` — Portfolio membership for aggregation

#### **AKP: Agent Knowledge Protocol (4 tables)**
- `agent_alignments` — Ontology similarity between agent pairs
- `pairwise_coherence` — TEC coherence from multi-agent interaction
- `knowledge_transfers` — Append-only transfer log (accept/reject/conflict counts)
- `agent_interaction_policies` — Socialization rules

#### **Agent Payout Tracking (1 table)**
- `agent_episode_payouts` — Per-agent earnings from workspace executions

#### **Social & Contacts (1 table)**
- `contacts` — Asymmetric follow model

---

## Key Relationships

### Primary Hierarchies
1. **User → Wallet → Credit Ledger** — Money flow
2. **Agent → Episodes → Consolidation Jobs → Ontology Snapshots** — ADM pipeline
3. **User → Teams → Workspace Agents** — Collaboration structure
4. **Creature → Creature Flights → Swarm Events** — Rabble ecosystem
5. **Agent → Agent Alignments/Pairwise Coherence** — AKP cross-agent knowledge

### Join Tables (Many-to-Many)
- `team_members` — Teams ↔ Users/Agents
- `workspace_agents` — Workspaces ↔ Agents
- `object_shares` — Objects ↔ Teams/Users
- `swarm_activations` — Swarm Algorithms ↔ Swarms
- `fermi_portfolio_forecasts` — Portfolios ↔ Forecasts

### Polymorphic Fields
- `object_shares.object_type` — agent | capability | forecast | index | repo | file | **rabble**
- `team_members.member_type` — user | agent
- `workspace_messages.message_type` — chat | execution_result | coherence_update | system_event | agent_invocation
- `wallets.owner_type` — user | workspace | agent
- `voice_assets.object_type` — episode | message | creature | synopsis
- `creature_tethers.tether_type` — phone_gps | meshtastic | gps_tracker | fixed_sensor

### Foreign Key Cascades
- `agents` → `episodes`, `semantic_rules`, `entities`, `facts`, `communities`, `consolidation_jobs`, `ontology_snapshots` (CASCADE)
- `teams` → `team_members`, `workspace_messages`, `workspace_agents`, `coherence_evaluations` (CASCADE)
- `creatures` → `creature_flights`, `creature_devices`, `creature_tethers` (CASCADE)
- `swarm_events` → `swarm_sub_flocks`, `rabble_messages` (CASCADE)
- `creature_tethers` → `telemetry_points` (CASCADE)

---

## Canonical tx_type List (from migration 075)

**50 transaction types** for credit ledger:

### Core Platform
- `deposit`, `withdrawal`
- `execution_fee`, `gas_fee`
- `education_alloc`, `education_spend`
- `transfer_out`, `transfer_in`
- `grant`, `refund`

### Agent Economics
- `fork_royalty`, `fork_fee`
- `publish_fee`, `eval_fee`
- `consolidation_fee`
- `execution_royalty`, `agent_collaboration_payout`

### Agent Admin
- `agent_collect_out`, `agent_collect_in`
- `agent_allocate_dream`, `agent_allocate_education`, `agent_allocate_coherence`

### Content Generation
- `avatar_generate`, `embedding_import`
- `ontology_generation`, `prompt_generation`, `file_write`
- `creature_art`

### Marketplace
- `marketplace_listing_fee`
- `marketplace_match_purchase`, `marketplace_match_payout`

### Rabble (Creature/Swarm)
- `creature_mint`, `creature_flight`, `creature_animate`
- `swarm_create`, `swarm_join`, `swarm_session_create`, `swarm_telemetry_ingest`
- `collection_create`, `rabble_chat`
- `gbif_contribution`, `rabble_platform_fee`
- `formation_activate`
- `attraction_reward`

### Ecosystem
- `akp_alignment`, `akp_transfer`, `akp_bootstrap`, `akp_diff`
- `observation_session_create`, `observation_ingest`
- `flight_plan`
- `perch`, `fly`, `walk_in_fee`, `walk_in_revenue`
- `tether`
- `platform_read`

---

## Constraints & Indexes Summary

### CHECK Constraints
- `users.role` IN ('admin', 'developer', 'viewer')
- `auth_provider` IN ('email', 'github', 'google', 'ethereum', 'legacy')
- `agents.status` IN ('draft', 'published', 'archived')
- `agents.tier` IN ('curated', 'community', 'system')
- `agents.visibility` IN ('private', 'unlisted', 'public')
- `team_members.member_type` IN ('user', 'agent')
- `team_members.role` IN ('owner', 'admin', 'member', 'viewer')
- `workspace_messages.message_type` IN (5 types)
- `workspace_agents.relationship` IN ('hired', 'owned', 'created_here', 'system')
- `object_shares.object_type` IN (7 types)
- `wallets.owner_type` IN ('user', 'workspace', 'agent')
- `wallets.balance` = `granted_balance` + `purchased_balance`
- `credit_ledger.tx_type` IN (50 types - see above)
- `creatures.presence` IN ('active', 'sleeping', 'parked')
- `creatures.visibility` IN ('public', 'contacts', 'private')
- `creature_flights.visibility` IN ('public', 'contacts', 'private')
- `swarm_events.visibility` IN ('public', 'shared', 'private')
- `swarm_events.status` IN ('scheduled', ...) [open]
- `creature_tethers.tether_type` IN (4 types)
- `fermi_notebooks.visibility` IN ('private', 'shared', 'public')
- `fermi_portfolios.visibility` IN ('private', 'shared', 'public')
- `fermi_forecasts.predicted_probability` BETWEEN 0 AND 1
- `eval_runs.status` IN ('running', 'completed', 'failed')
- `sosa_observations.phenomenon_time` IS NOT NULL

### Unique Indexes
- `users.email`, `users.ethereum_address` (conditional)
- `agents.agent_name`
- `teams.slug`
- `api_keys.key_prefix`
- `siwe_nonces.nonce`
- `swarm_events.qr_token` (conditional)
- `shopping_profiles(user_id, agent_id, profile_name)`
- `creature_devices(owner_id, device_identifier)`
- `contacts(user_id, contact_id)`
- `agent_alignments(source_agent_id, target_agent_id)`
- `agent_interaction_policies(agent_id, policy_type, target_agent_id)`
- **Partial**: `idx_one_active_flight_per_creature` — one active flight per creature

### GIN Indexes (Array/JSONB)
- `episodes.tags` (USING GIN)
- `marketplace_listings.category_tags` (USING GIN)

### Composite Indexes
- `idx_creature_flights_active_visible` — (owner_id, visibility) WHERE ended_at IS NULL
- `idx_episodes_user_agent` — (user_id, agent_id)
- `idx_teams_owner` — (owner_id)
- `idx_agents_user_visibility` — (user_id, visibility)

---

## Notable Schema Patterns

1. **Temporal Validity (ADM)**
   - Entities & Facts: `t_valid`, `t_invalid`, `t_created`, `t_expired`
   - Enables time-series reasoning and historical graph queries

2. **Versioning**
   - `agent_versions` — Snapshot mutable fields before update
   - `agents.version` — Current semantic version
   - `ontology_snapshots.version` — Snapshot increment
   - `entities.version`, `facts.version` — Replacement tracking

3. **Append-Only Ledgers**
   - `credit_ledger` — Every tx immutable; balance derived
   - `secret_access_log` — Audit trail
   - `knowledge_transfers` — Transfer history

4. **Dual Balance (Wallet)**
   - `granted_balance` — Non-transferable (signups, grants)
   - `purchased_balance` — Transferable (Stripe, revenue)
   - Spend priority: granted first

5. **Embedding Storage**
   - 1024-dim pgvector on: episodes, entities, facts, communities, shopping_profiles
   - Used for similarity search (marketplace, coherence)

6. **Ephemerality Workarounds**
   - Railway FS ephemeral → creature_images, creature_animation_layers as BYTEA
   - voice_assets.storage_url points to R2/S3

7. **H3 Hexagonal Geospatial**
   - creatures, creature_flights, swarm_events, ar_beacons, ar_grid_maps use H3 cells
   - Enables efficient geographic queries & multi-resolution clustering

8. **JSONB for Flexibility**
   - `agents.mcp_servers`, `agents.fork_pricing`, `agents.workflow_template`
   - `creatures.taxonomy`, `creatures.generation_params`
   - `swarm_algorithms.formation_spec` — Onto4MAT declarative JSON
   - `swarm_telemetry.extra`, `sosa_observations.extra`

---

## Summary Statistics

- **Total columns**: ~800
- **Total indexes**: ~120+
- **Total constraints**: 50+ CHECK + unique
- **pgvector dimensions**: 1024 (4 entity types)
- **H3 resolution**: Variable (9-12 typical)
- **Largest table**: credit_ledger (append-only, unbounded growth)
- **Most complex entity**: `agents` (30+ columns)
- **Most relationships**: agents (40+ outbound foreign keys)
- **Highest cardinality**: credit_ledger → wallets (N:1)

All migrations follow PgBouncer transaction mode safety: no BEGIN/COMMIT in migrations, all operations atomic via single ALTER/INSERT statements or DO $$ blocks.
