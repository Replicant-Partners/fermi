-- Migration 061: Swarm Algorithm Marketplace
-- Purchasable Onto4MAT formation algorithms for rabble creatures.
-- Algorithms are declarative JSON specs that the Flutter client downloads and
-- applies to the ring attractor simulation at 60fps.

CREATE TABLE IF NOT EXISTS swarm_algorithms (
    algorithm_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    description TEXT,
    category TEXT NOT NULL,
    onto4mat_class TEXT NOT NULL,
    formation_spec JSONB NOT NULL,
    tier TEXT NOT NULL DEFAULT 'premium',
    cost_credits INTEGER NOT NULL DEFAULT 3,
    icon TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS swarm_activations (
    activation_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    algorithm_id UUID NOT NULL REFERENCES swarm_algorithms(algorithm_id),
    user_id TEXT NOT NULL,
    swarm_id UUID NOT NULL,
    activated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_swarm_activations_user ON swarm_activations(user_id);
CREATE INDEX IF NOT EXISTS idx_swarm_activations_swarm ON swarm_activations(swarm_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_swarm_activations_unique
    ON swarm_activations(user_id, swarm_id, algorithm_id);

-- Add formation_activate tx_type
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee',
        'publish_fee', 'eval_fee',
        'consolidation_fee',
        'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
        'avatar_generate', 'embedding_import',
        'ontology_generation', 'prompt_generation', 'file_write',
        'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
        'collection_create', 'rabble_chat',
        'gbif_contribution', 'rabble_platform_fee',
        'akp_alignment', 'akp_transfer', 'akp_bootstrap', 'akp_diff',
        'swarm_session_create', 'swarm_telemetry_ingest',
        'observation_session_create', 'observation_ingest',
        'execution_royalty', 'agent_collaboration_payout',
        'creature_art',
        'platform_read',
        'agent_collect_out', 'agent_collect_in',
        'agent_allocate_dream', 'agent_allocate_education', 'agent_allocate_coherence',
        'formation_activate'
    ));

-- Seed 11 Onto4MAT algorithms: 5 formations + 6 team actions
-- Free-tier formations (matching existing SwarmEngine enum) are seeded as 'free'
-- so the client can display them in the picker alongside premium ones.

-- Formations (Onto4MAT Formation class)
INSERT INTO swarm_algorithms (name, display_name, description, category, onto4mat_class, tier, cost_credits, icon, formation_spec)
VALUES
('v_formation', 'V-Formation', 'Classic migratory V. Leader at apex, followers trail at symmetric angles. Efficient for long-distance coordinated movement.', 'formation', 'V-Formation', 'premium', 3, 'arrow_upward',
 '{"influence":{"mode":"v_formation","anchor_strength":0.3,"v_angle_deg":35.0,"spacing_m":8.0,"leader_index":0},"params_override":{"attraction_strength":1.3,"noise_strength":0.08,"max_turn_rate":2.5},"species_modifiers":{"butterfly":{"noise_strength":0.15},"dragonfly":{"base_speed":6.0}},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}'),

('echelon', 'Echelon', 'Staggered diagonal line. Military step formation — each creature offset behind and to one side of the creature ahead.', 'formation', 'Echelon', 'premium', 3, 'trending_flat',
 '{"influence":{"mode":"echelon","anchor_strength":0.3,"echelon_angle_deg":45.0,"spacing_m":6.0,"leader_index":0},"params_override":{"attraction_strength":1.2,"noise_strength":0.1,"max_turn_rate":2.0},"species_modifiers":{"butterfly":{"noise_strength":0.18}},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}'),

('wedge', 'Wedge', 'Wide-angle attack V. Like V-Formation but broader — aggressive spread for coverage and intimidation.', 'formation', 'Wedge', 'premium', 3, 'change_history',
 '{"influence":{"mode":"wedge","anchor_strength":0.35,"v_angle_deg":55.0,"spacing_m":10.0,"leader_index":0},"params_override":{"attraction_strength":1.1,"noise_strength":0.12,"max_turn_rate":2.2},"species_modifiers":{"dragonfly":{"base_speed":7.0}},"transition":{"blend_duration_ms":2500,"entry_formation":"gathering"}}'),

('arc', 'Arc', 'Semicircular sweep formation. Creatures spread along a curved front, useful for area scanning.', 'formation', 'Arc', 'premium', 3, 'panorama_horizontal',
 '{"influence":{"mode":"arc","anchor_strength":0.25,"arc_angle_deg":180.0,"arc_radius_m":25.0},"params_override":{"attraction_strength":1.0,"noise_strength":0.1,"max_turn_rate":1.8},"species_modifiers":{},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}'),

('line', 'Line', 'Single-file line. Tight alignment, minimum lateral spread. Trail formation for narrow passages.', 'formation', 'Line', 'premium', 3, 'linear_scale',
 '{"influence":{"mode":"line","anchor_strength":0.4,"spacing_m":5.0,"leader_index":0},"params_override":{"attraction_strength":1.4,"noise_strength":0.06,"max_turn_rate":2.8},"species_modifiers":{"butterfly":{"noise_strength":0.12},"locust":{"noise_strength":0.04}},"transition":{"blend_duration_ms":1500,"entry_formation":"gathering"}}')
ON CONFLICT (name) DO NOTHING;

-- Team Actions (Onto4MAT TeamAction class)
INSERT INTO swarm_algorithms (name, display_name, description, category, onto4mat_class, tier, cost_credits, icon, formation_spec)
VALUES
('encircle', 'Encircle', 'Orbit a target point with creatures facing outward. Defensive perimeter — surround and watch.', 'team_action', 'Encircle', 'premium', 5, 'radio_button_unchecked',
 '{"influence":{"mode":"encircle","orbit_radius_m":30.0,"tangential_strength":0.4,"outward_facing":true},"params_override":{"attraction_strength":1.0,"noise_strength":0.08,"max_turn_rate":2.0},"species_modifiers":{"dragonfly":{"base_speed":5.0}},"transition":{"blend_duration_ms":3000,"entry_formation":"gathering"}}'),

('patrol', 'Patrol', 'Circuit through waypoints with even spacing. Creatures maintain regular intervals along the route.', 'team_action', 'Patrol', 'premium', 5, 'route',
 '{"influence":{"mode":"patrol","waypoint_advance_threshold_m":5.0,"spacing_m":15.0,"loop":true},"params_override":{"attraction_strength":1.1,"noise_strength":0.1,"max_turn_rate":2.0,"base_speed":2.0},"species_modifiers":{"locust":{"base_speed":1.8}},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}'),

('search', 'Search', 'Expanding spiral from center. Maximum area coverage — creatures spiral outward to scan terrain.', 'team_action', 'Search', 'premium', 5, 'radar',
 '{"influence":{"mode":"search","expansion_rate_m_per_rev":8.0,"spiral_speed":0.5},"params_override":{"attraction_strength":0.8,"noise_strength":0.15,"max_turn_rate":1.5},"species_modifiers":{"butterfly":{"noise_strength":0.25}},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}'),

('surround', 'Surround', 'Converge to a ring around target and hold position. Tighter than Encircle — containment formation.', 'team_action', 'Surround', 'premium', 5, 'adjust',
 '{"influence":{"mode":"surround","ring_radius_m":15.0,"hold_strength":0.6},"params_override":{"attraction_strength":1.3,"noise_strength":0.05,"max_turn_rate":2.5,"base_speed":1.5},"species_modifiers":{},"transition":{"blend_duration_ms":2500,"entry_formation":"gathering"}}'),

('form_up', 'Form Up', 'Rapid gathering into tight cluster. Emergency regroup — all creatures converge fast to a single point.', 'team_action', 'FormUp', 'premium', 3, 'compress',
 '{"influence":{"mode":"form_up","convergence_strength":0.8,"target_radius_m":5.0},"params_override":{"attraction_strength":1.5,"noise_strength":0.05,"max_turn_rate":3.0,"base_speed":3.0},"species_modifiers":{"locust":{"base_speed":2.5}},"transition":{"blend_duration_ms":1000,"entry_formation":"gathering"}}'),

('herd', 'Herd', 'Push targets toward a goal point. Shepherding behavior — creatures position behind targets and drive them forward.', 'team_action', 'Herd', 'premium', 5, 'pets',
 '{"influence":{"mode":"herd","push_strength":0.5,"goal_bearing_bias":0.3,"spacing_behind_m":10.0},"params_override":{"attraction_strength":1.0,"noise_strength":0.1,"max_turn_rate":2.0},"species_modifiers":{"dragonfly":{"base_speed":6.0}},"transition":{"blend_duration_ms":2000,"entry_formation":"gathering"}}')
ON CONFLICT (name) DO NOTHING;
