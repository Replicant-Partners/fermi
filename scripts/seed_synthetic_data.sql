-- Synthetic seed data for the 3 curated agents
-- Run with: psql "$DATABASE_URL" -f scripts/seed_synthetic_data.sql

-- Look up real agent UUIDs
DO $$
DECLARE
  v_mr_id UUID;
  v_sa_id UUID;
  v_ep_id UUID;
  v_ep1 UUID; v_ep2 UUID; v_ep3 UUID; v_ep4 UUID; v_ep5 UUID;
  v_ep6 UUID; v_ep7 UUID; v_ep8 UUID; v_ep9 UUID; v_ep10 UUID;
  v_ep11 UUID; v_ep12 UUID; v_ep13 UUID; v_ep14 UUID; v_ep15 UUID;
  v_sep1 UUID; v_sep2 UUID; v_sep3 UUID; v_sep4 UUID; v_sep5 UUID;
  v_sep6 UUID; v_sep7 UUID; v_sep8 UUID; v_sep9 UUID; v_sep10 UUID;
  v_eep1 UUID; v_eep2 UUID; v_eep3 UUID; v_eep4 UUID; v_eep5 UUID;
  -- entities
  v_ent1 UUID; v_ent2 UUID; v_ent3 UUID; v_ent4 UUID; v_ent5 UUID;
  v_ent6 UUID; v_ent7 UUID; v_ent8 UUID; v_ent9 UUID; v_ent10 UUID;
  v_ent11 UUID; v_ent12 UUID;
  -- rules
  v_rule1 UUID; v_rule2 UUID; v_rule3 UUID; v_rule4 UUID;
  v_rule5 UUID; v_rule6 UUID; v_rule7 UUID; v_rule8 UUID;
  -- consolidation jobs
  v_job1 UUID; v_job2 UUID; v_job3 UUID;
BEGIN
  SELECT agent_id INTO v_mr_id FROM agents WHERE agent_name = 'market_research';
  SELECT agent_id INTO v_sa_id FROM agents WHERE agent_name = 'sentiment_analyzer';
  SELECT agent_id INTO v_ep_id FROM agents WHERE agent_name = 'embedding_projector_guide';

  IF v_mr_id IS NULL OR v_sa_id IS NULL OR v_ep_id IS NULL THEN
    RAISE EXCEPTION 'Missing curated agents. Run the server first to seed them.';
  END IF;

  -- =========================================================================
  -- MARKET RESEARCH: 15 episodes across 45 days
  -- =========================================================================
  v_ep1  := gen_random_uuid(); v_ep2  := gen_random_uuid();
  v_ep3  := gen_random_uuid(); v_ep4  := gen_random_uuid();
  v_ep5  := gen_random_uuid(); v_ep6  := gen_random_uuid();
  v_ep7  := gen_random_uuid(); v_ep8  := gen_random_uuid();
  v_ep9  := gen_random_uuid(); v_ep10 := gen_random_uuid();
  v_ep11 := gen_random_uuid(); v_ep12 := gen_random_uuid();
  v_ep13 := gen_random_uuid(); v_ep14 := gen_random_uuid();
  v_ep15 := gen_random_uuid();

  INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context, execution_status, execution_time_ms, tokens_used, cost_usd, consolidated, created_at) VALUES
    (v_ep1,  v_mr_id, NOW() - INTERVAL '44 days', 'What is the current global semiconductor market size and growth rate?', '{"domain":"semiconductors","depth":"overview"}', 'success', 2340, 1850, 0.0037, TRUE, NOW() - INTERVAL '44 days'),
    (v_ep2,  v_mr_id, NOW() - INTERVAL '42 days', 'Analyze NVIDIA market share in datacenter GPUs vs AMD', '{"domain":"gpu","competitors":["NVIDIA","AMD"]}', 'success', 3120, 2450, 0.0049, TRUE, NOW() - INTERVAL '42 days'),
    (v_ep3,  v_mr_id, NOW() - INTERVAL '39 days', 'What are the key drivers of HBM (High Bandwidth Memory) demand?', '{"domain":"memory","focus":"HBM"}', 'success', 2890, 2100, 0.0042, TRUE, NOW() - INTERVAL '39 days'),
    (v_ep4,  v_mr_id, NOW() - INTERVAL '36 days', 'TSMC advanced node capacity allocation for 2026', '{"domain":"foundry","company":"TSMC"}', 'success', 4150, 3200, 0.0064, TRUE, NOW() - INTERVAL '36 days'),
    (v_ep5,  v_mr_id, NOW() - INTERVAL '33 days', 'Compare Intel Foundry Services competitive position vs TSMC and Samsung', '{"domain":"foundry","competitors":["Intel","TSMC","Samsung"]}', 'failure', 890, NULL, NULL, FALSE, NOW() - INTERVAL '33 days'),
    (v_ep6,  v_mr_id, NOW() - INTERVAL '30 days', 'Automotive chip shortage recovery timeline and remaining bottlenecks', '{"domain":"automotive","focus":"supply_chain"}', 'success', 3560, 2780, 0.0056, TRUE, NOW() - INTERVAL '30 days'),
    (v_ep7,  v_mr_id, NOW() - INTERVAL '27 days', 'AI accelerator market: custom silicon (Google TPU, AWS Trainium) vs merchant GPU', '{"domain":"ai_accelerators"}', 'success', 4200, 3400, 0.0068, TRUE, NOW() - INTERVAL '27 days'),
    (v_ep8,  v_mr_id, NOW() - INTERVAL '24 days', 'RISC-V adoption trends in edge computing and IoT', '{"domain":"architecture","focus":"RISC-V"}', 'success', 2670, 1950, 0.0039, FALSE, NOW() - INTERVAL '24 days'),
    (v_ep9,  v_mr_id, NOW() - INTERVAL '20 days', 'Qualcomm vs MediaTek mobile SoC market dynamics', '{"domain":"mobile","competitors":["Qualcomm","MediaTek"]}', 'success', 3100, 2300, 0.0046, FALSE, NOW() - INTERVAL '20 days'),
    (v_ep10, v_mr_id, NOW() - INTERVAL '17 days', 'Impact of US-China chip export controls on global supply chains', '{"domain":"geopolitics","focus":"export_controls"}', 'success', 3890, 3100, 0.0062, FALSE, NOW() - INTERVAL '17 days'),
    (v_ep11, v_mr_id, NOW() - INTERVAL '13 days', 'European Chips Act progress and ASML advanced lithography demand', '{"domain":"policy","region":"EU"}', 'success', 2950, 2200, 0.0044, FALSE, NOW() - INTERVAL '13 days'),
    (v_ep12, v_mr_id, NOW() - INTERVAL '10 days', 'Memory market cycle: DRAM pricing outlook and Samsung vs SK Hynix', '{"domain":"memory","focus":"DRAM"}', 'partial', 5100, 3800, 0.0076, FALSE, NOW() - INTERVAL '10 days'),
    (v_ep13, v_mr_id, NOW() - INTERVAL '7 days', 'ARM IPO valuation and licensing revenue model analysis', '{"domain":"ip_licensing","company":"ARM"}', 'success', 2400, 1700, 0.0034, FALSE, NOW() - INTERVAL '7 days'),
    (v_ep14, v_mr_id, NOW() - INTERVAL '4 days', 'Chiplet architecture adoption: AMD, Intel, and packaging technology trends', '{"domain":"packaging","focus":"chiplets"}', 'success', 3300, 2600, 0.0052, FALSE, NOW() - INTERVAL '4 days'),
    (v_ep15, v_mr_id, NOW() - INTERVAL '1 day',  'Photonics and silicon photonics market opportunity for datacenters', '{"domain":"photonics","application":"datacenter"}', 'success', 2780, 2050, 0.0041, FALSE, NOW() - INTERVAL '1 day');

  -- Add error details for the failed one
  UPDATE episodes SET error_details = 'LLM provider timeout: request exceeded 30s deadline' WHERE episode_id = v_ep5;

  -- =========================================================================
  -- SENTIMENT ANALYZER: 10 episodes
  -- =========================================================================
  v_sep1  := gen_random_uuid(); v_sep2  := gen_random_uuid();
  v_sep3  := gen_random_uuid(); v_sep4  := gen_random_uuid();
  v_sep5  := gen_random_uuid(); v_sep6  := gen_random_uuid();
  v_sep7  := gen_random_uuid(); v_sep8  := gen_random_uuid();
  v_sep9  := gen_random_uuid(); v_sep10 := gen_random_uuid();

  INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context, execution_status, execution_time_ms, tokens_used, cost_usd, consolidated, created_at) VALUES
    (v_sep1,  v_sa_id, NOW() - INTERVAL '40 days', 'Analyze Twitter/X sentiment around NVIDIA earnings Q4 2025', '{"platform":"twitter","topic":"NVIDIA","event":"earnings"}', 'success', 4500, 3800, 0.0076, TRUE, NOW() - INTERVAL '40 days'),
    (v_sep2,  v_sa_id, NOW() - INTERVAL '35 days', 'Reddit r/wallstreetbets sentiment on AMD stock', '{"platform":"reddit","subreddit":"wallstreetbets","ticker":"AMD"}', 'success', 3200, 2900, 0.0058, TRUE, NOW() - INTERVAL '35 days'),
    (v_sep3,  v_sa_id, NOW() - INTERVAL '30 days', 'News headline sentiment: US semiconductor policy announcements', '{"platform":"news","topic":"chip_policy"}', 'success', 2800, 2100, 0.0042, TRUE, NOW() - INTERVAL '30 days'),
    (v_sep4,  v_sa_id, NOW() - INTERVAL '25 days', 'GitHub developer sentiment toward Rust vs Go for systems programming', '{"platform":"github","topic":"programming_languages"}', 'success', 3600, 2700, 0.0054, TRUE, NOW() - INTERVAL '25 days'),
    (v_sep5,  v_sa_id, NOW() - INTERVAL '20 days', 'Hacker News sentiment on AI regulation proposals', '{"platform":"hackernews","topic":"ai_regulation"}', 'failure', 1200, NULL, NULL, FALSE, NOW() - INTERVAL '20 days'),
    (v_sep6,  v_sa_id, NOW() - INTERVAL '16 days', 'Crypto Twitter sentiment on Bitcoin ETF inflows', '{"platform":"twitter","topic":"bitcoin_etf"}', 'success', 3900, 3100, 0.0062, FALSE, NOW() - INTERVAL '16 days'),
    (v_sep7,  v_sa_id, NOW() - INTERVAL '12 days', 'LinkedIn professional sentiment on tech layoffs Q1 2026', '{"platform":"linkedin","topic":"tech_layoffs"}', 'success', 2600, 1900, 0.0038, FALSE, NOW() - INTERVAL '12 days'),
    (v_sep8,  v_sa_id, NOW() - INTERVAL '8 days',  'YouTube comment sentiment on Apple Vision Pro adoption', '{"platform":"youtube","topic":"apple_vision_pro"}', 'success', 4100, 3300, 0.0066, FALSE, NOW() - INTERVAL '8 days'),
    (v_sep9,  v_sa_id, NOW() - INTERVAL '4 days',  'Forum sentiment on open-source LLM models (Llama, Mistral, Gemma)', '{"platform":"forums","topic":"open_source_llm"}', 'success', 3400, 2500, 0.0050, FALSE, NOW() - INTERVAL '4 days'),
    (v_sep10, v_sa_id, NOW() - INTERVAL '1 day',   'Cross-platform sentiment analysis: Anthropic Claude perception vs OpenAI', '{"platform":"multi","topic":"llm_comparison"}', 'success', 3800, 2800, 0.0056, FALSE, NOW() - INTERVAL '1 day');

  UPDATE episodes SET error_details = 'Rate limit exceeded on HN API, retry after 60s' WHERE episode_id = v_sep5;

  -- =========================================================================
  -- EMBEDDING PROJECTOR GUIDE: 5 episodes
  -- =========================================================================
  v_eep1 := gen_random_uuid(); v_eep2 := gen_random_uuid();
  v_eep3 := gen_random_uuid(); v_eep4 := gen_random_uuid();
  v_eep5 := gen_random_uuid();

  INSERT INTO episodes (episode_id, agent_id, timestamp_ref, query, context, execution_status, execution_time_ms, tokens_used, cost_usd, consolidated, created_at) VALUES
    (v_eep1, v_ep_id, NOW() - INTERVAL '15 days', 'Interpret the PCA projection for market_research agent knowledge space', '{"target_agent":"market_research","method":"PCA"}', 'success', 2100, 1600, 0.0032, TRUE, NOW() - INTERVAL '15 days'),
    (v_eep2, v_ep_id, NOW() - INTERVAL '10 days', 'Explain temporal drift in sentiment_analyzer embeddings over last month', '{"target_agent":"sentiment_analyzer","method":"temporal"}', 'success', 2800, 2200, 0.0044, TRUE, NOW() - INTERVAL '10 days'),
    (v_eep3, v_ep_id, NOW() - INTERVAL '7 days',  'Identify semantic clusters in the bestiary-wide projection', '{"method":"PCA","scope":"bestiary"}', 'success', 3200, 2600, 0.0052, FALSE, NOW() - INTERVAL '7 days'),
    (v_eep4, v_ep_id, NOW() - INTERVAL '3 days',  'Compare explained variance between PCA and t-SNE for market_research', '{"target_agent":"market_research","comparison":["PCA","tSNE"]}', 'success', 2500, 1800, 0.0036, FALSE, NOW() - INTERVAL '3 days'),
    (v_eep5, v_ep_id, NOW() - INTERVAL '1 day',   'Detect anomalous points in the embedding space that may indicate knowledge gaps', '{"method":"PCA","focus":"anomalies"}', 'partial', 4800, 3500, 0.0070, FALSE, NOW() - INTERVAL '1 day');

  -- =========================================================================
  -- SEMANTIC RULES: 8 rules across agents
  -- =========================================================================
  v_rule1 := gen_random_uuid(); v_rule2 := gen_random_uuid();
  v_rule3 := gen_random_uuid(); v_rule4 := gen_random_uuid();
  v_rule5 := gen_random_uuid(); v_rule6 := gen_random_uuid();
  v_rule7 := gen_random_uuid(); v_rule8 := gen_random_uuid();

  INSERT INTO semantic_rules (rule_id, agent_id, rule_content, rule_description, confidence_score, verification_status, verification_method, source_episode_cluster, episode_count, application_count, successful_applications, failed_applications, is_active, created_at) VALUES
    -- Market Research rules
    (v_rule1, v_mr_id,
     'NVIDIA maintains >80% market share in datacenter GPUs due to CUDA ecosystem lock-in and superior software stack',
     'NVIDIA datacenter GPU dominance pattern',
     0.92, 'verified', 'cross_reference', ARRAY[v_ep2, v_ep7], 2, 5, 4, 1, TRUE, NOW() - INTERVAL '30 days'),
    (v_rule2, v_mr_id,
     'HBM demand is primarily driven by AI training workloads, with SK Hynix leading supply to NVIDIA',
     'HBM demand driver identification',
     0.87, 'verified', 'market_data', ARRAY[v_ep3, v_ep4], 2, 3, 3, 0, TRUE, NOW() - INTERVAL '28 days'),
    (v_rule3, v_mr_id,
     'US-China export controls create a bifurcated chip market with separate supply chains emerging',
     'Geopolitical supply chain bifurcation',
     0.78, 'pending', NULL, ARRAY[v_ep10, v_ep11], 2, 1, 1, 0, TRUE, NOW() - INTERVAL '15 days'),
    (v_rule4, v_mr_id,
     'Chiplet architecture will become the dominant packaging approach for high-performance computing by 2027',
     'Chiplet architecture adoption forecast',
     0.65, 'pending', NULL, ARRAY[v_ep14], 1, 0, 0, 0, TRUE, NOW() - INTERVAL '4 days'),
    -- Sentiment Analyzer rules
    (v_rule5, v_sa_id,
     'Earnings announcements create predictable sentiment spikes: positive pre-earnings hype followed by post-earnings mean reversion',
     'Earnings sentiment cycle pattern',
     0.88, 'verified', 'backtesting', ARRAY[v_sep1, v_sep2], 2, 8, 7, 1, TRUE, NOW() - INTERVAL '25 days'),
    (v_rule6, v_sa_id,
     'Reddit sentiment is a lagging indicator compared to Twitter for breaking tech news, but more persistent for trend analysis',
     'Platform sentiment timing hierarchy',
     0.72, 'verified', 'temporal_analysis', ARRAY[v_sep1, v_sep2, v_sep6], 3, 4, 3, 1, TRUE, NOW() - INTERVAL '20 days'),
    (v_rule7, v_sa_id,
     'Open-source AI model releases generate 3-5 day positive sentiment waves followed by critical evaluation phase',
     'OSS AI release sentiment cycle',
     0.60, 'pending', NULL, ARRAY[v_sep9], 1, 0, 0, 0, TRUE, NOW() - INTERVAL '3 days'),
    -- Embedding Projector Guide rules
    (v_rule8, v_ep_id,
     'Agents with more consolidation cycles show tighter, more distinct clusters in PCA space, indicating knowledge crystallization',
     'Consolidation-cluster correlation in embedding space',
     0.83, 'verified', 'visual_inspection', ARRAY[v_eep1, v_eep2, v_eep3], 3, 2, 2, 0, TRUE, NOW() - INTERVAL '6 days');

  -- =========================================================================
  -- ENTITIES: 12 knowledge graph nodes (market research domain)
  -- =========================================================================
  v_ent1  := gen_random_uuid(); v_ent2  := gen_random_uuid();
  v_ent3  := gen_random_uuid(); v_ent4  := gen_random_uuid();
  v_ent5  := gen_random_uuid(); v_ent6  := gen_random_uuid();
  v_ent7  := gen_random_uuid(); v_ent8  := gen_random_uuid();
  v_ent9  := gen_random_uuid(); v_ent10 := gen_random_uuid();
  v_ent11 := gen_random_uuid(); v_ent12 := gen_random_uuid();

  INSERT INTO entities (entity_id, agent_id, entity_name, entity_type, summary, t_valid, t_created, source_episodes, extraction_confidence) VALUES
    (v_ent1,  v_mr_id, 'NVIDIA',     'Company',      'Leading GPU and AI accelerator company, dominant in datacenter market', NOW() - INTERVAL '44 days', NOW() - INTERVAL '44 days', ARRAY[v_ep2, v_ep7], 0.98),
    (v_ent2,  v_mr_id, 'AMD',        'Company',      'Semiconductor company competing in CPU, GPU, and datacenter markets',    NOW() - INTERVAL '42 days', NOW() - INTERVAL '42 days', ARRAY[v_ep2, v_ep14], 0.95),
    (v_ent3,  v_mr_id, 'TSMC',       'Company',      'World largest semiconductor foundry, key supplier of advanced nodes',   NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep4], 0.97),
    (v_ent4,  v_mr_id, 'SK Hynix',   'Company',      'Major memory manufacturer, leading HBM supplier for AI workloads',      NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep3], 0.90),
    (v_ent5,  v_mr_id, 'Intel',      'Company',      'Legacy chip giant transitioning to foundry services model',             NOW() - INTERVAL '33 days', NOW() - INTERVAL '33 days', ARRAY[v_ep5, v_ep14], 0.93),
    (v_ent6,  v_mr_id, 'HBM',        'Technology',   'High Bandwidth Memory, critical for AI training accelerators',          NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep3], 0.92),
    (v_ent7,  v_mr_id, 'CUDA',       'Technology',   'NVIDIA proprietary GPU computing platform, major competitive moat',     NOW() - INTERVAL '42 days', NOW() - INTERVAL '42 days', ARRAY[v_ep2, v_ep7], 0.96),
    (v_ent8,  v_mr_id, 'RISC-V',     'Architecture', 'Open-source instruction set architecture, growing in edge/IoT',         NOW() - INTERVAL '24 days', NOW() - INTERVAL '24 days', ARRAY[v_ep8], 0.85),
    (v_ent9,  v_mr_id, 'Chiplets',   'Technology',   'Modular chip packaging approach using multiple smaller dies',            NOW() - INTERVAL '4 days',  NOW() - INTERVAL '4 days',  ARRAY[v_ep14], 0.88),
    (v_ent10, v_mr_id, 'Qualcomm',   'Company',      'Leading mobile SoC designer, Snapdragon platform',                      NOW() - INTERVAL '20 days', NOW() - INTERVAL '20 days', ARRAY[v_ep9], 0.91),
    (v_ent11, v_mr_id, 'ARM',        'Company',      'IP licensing company for mobile and embedded processor architectures',   NOW() - INTERVAL '7 days',  NOW() - INTERVAL '7 days',  ARRAY[v_ep13], 0.94),
    (v_ent12, v_mr_id, 'ASML',       'Company',      'Monopoly provider of EUV lithography equipment for advanced chips',     NOW() - INTERVAL '13 days', NOW() - INTERVAL '13 days', ARRAY[v_ep11], 0.96);

  -- =========================================================================
  -- FACTS: 10 knowledge graph edges
  -- =========================================================================
  INSERT INTO facts (fact_id, agent_id, source_entity_id, target_entity_id, relation_type, relation_cardinality, confidence, reasoning, t_valid, t_created, source_episodes) VALUES
    (gen_random_uuid(), v_mr_id, v_ent1,  v_ent2,  'competes_with',   '}o--o{', 0.95, 'Direct competition in datacenter GPU market, NVIDIA leads with ~80% share', NOW() - INTERVAL '42 days', NOW() - INTERVAL '42 days', ARRAY[v_ep2]),
    (gen_random_uuid(), v_mr_id, v_ent1,  v_ent3,  'manufactured_by', '||--o{', 0.98, 'NVIDIA GPUs fabricated at TSMC on advanced process nodes (4nm, 3nm)',      NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep4]),
    (gen_random_uuid(), v_mr_id, v_ent4,  v_ent6,  'supplies',        '||--o{', 0.93, 'SK Hynix is primary HBM supplier, especially HBM3E for NVIDIA H100/H200', NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep3]),
    (gen_random_uuid(), v_mr_id, v_ent1,  v_ent7,  'owns',            '||--||', 0.99, 'CUDA is NVIDIA proprietary technology, key ecosystem moat',               NOW() - INTERVAL '42 days', NOW() - INTERVAL '42 days', ARRAY[v_ep2, v_ep7]),
    (gen_random_uuid(), v_mr_id, v_ent5,  v_ent3,  'competes_with',   '}o--o{', 0.80, 'Intel Foundry Services competes with TSMC but trails significantly',      NOW() - INTERVAL '33 days', NOW() - INTERVAL '33 days', ARRAY[v_ep5]),
    (gen_random_uuid(), v_mr_id, v_ent2,  v_ent9,  'pioneers',        '||--||', 0.88, 'AMD pioneered chiplet architecture with Zen CPUs, now industry standard',  NOW() - INTERVAL '4 days',  NOW() - INTERVAL '4 days',  ARRAY[v_ep14]),
    (gen_random_uuid(), v_mr_id, v_ent10, v_ent11, 'licenses_from',   '}o--||', 0.92, 'Qualcomm licenses ARM architecture for Snapdragon mobile SoCs',           NOW() - INTERVAL '20 days', NOW() - INTERVAL '20 days', ARRAY[v_ep9, v_ep13]),
    (gen_random_uuid(), v_mr_id, v_ent12, v_ent3,  'supplies_to',     '||--o{', 0.97, 'ASML is sole EUV supplier to TSMC (and Samsung, Intel)',                   NOW() - INTERVAL '13 days', NOW() - INTERVAL '13 days', ARRAY[v_ep11]),
    (gen_random_uuid(), v_mr_id, v_ent8,  v_ent11, 'competes_with',   '}o--o{', 0.70, 'RISC-V open architecture is an emerging competitor to ARM licensing',      NOW() - INTERVAL '24 days', NOW() - INTERVAL '24 days', ARRAY[v_ep8]),
    (gen_random_uuid(), v_mr_id, v_ent6,  v_ent1,  'enables',         '||--||', 0.94, 'HBM technology enables NVIDIA AI accelerator performance leadership',     NOW() - INTERVAL '39 days', NOW() - INTERVAL '39 days', ARRAY[v_ep3, v_ep7]);

  -- =========================================================================
  -- COMMUNITIES: 3 entity clusters
  -- =========================================================================
  INSERT INTO communities (community_id, agent_id, community_name, summary, member_entity_ids, member_count, created_at) VALUES
    (gen_random_uuid(), v_mr_id, 'AI Accelerator Ecosystem',
     'Core companies and technologies driving AI hardware: NVIDIA, AMD, SK Hynix, HBM, CUDA',
     ARRAY[v_ent1, v_ent2, v_ent4, v_ent6, v_ent7], 5, NOW() - INTERVAL '28 days'),
    (gen_random_uuid(), v_mr_id, 'Semiconductor Manufacturing Chain',
     'Foundry and equipment suppliers: TSMC, Intel, ASML, and chiplet technology',
     ARRAY[v_ent3, v_ent5, v_ent9, v_ent12], 4, NOW() - INTERVAL '13 days'),
    (gen_random_uuid(), v_mr_id, 'Mobile & Edge Architecture',
     'Companies and architectures competing in mobile and edge computing: Qualcomm, ARM, RISC-V',
     ARRAY[v_ent8, v_ent10, v_ent11], 3, NOW() - INTERVAL '7 days');

  -- =========================================================================
  -- CONSOLIDATION JOBS: 3 jobs showing pipeline history
  -- =========================================================================
  v_job1 := gen_random_uuid(); v_job2 := gen_random_uuid(); v_job3 := gen_random_uuid();

  INSERT INTO consolidation_jobs (job_id, agent_id, status, started_at, completed_at, duration_ms, episode_range_start, episode_range_end, episodes_processed, clusters_identified, rules_extracted, rules_verified, rules_rejected, entities_created, facts_created) VALUES
    (v_job1, v_mr_id, 'completed', NOW() - INTERVAL '30 days', NOW() - INTERVAL '30 days' + INTERVAL '45 seconds',
     45000, v_ep1, v_ep7, 6, 2, 2, 2, 0, 7, 4),
    (v_job2, v_sa_id, 'completed', NOW() - INTERVAL '20 days', NOW() - INTERVAL '20 days' + INTERVAL '38 seconds',
     38000, v_sep1, v_sep4, 4, 1, 2, 2, 0, 0, 0),
    (v_job3, v_ep_id, 'completed', NOW() - INTERVAL '6 days', NOW() - INTERVAL '6 days' + INTERVAL '22 seconds',
     22000, v_eep1, v_eep2, 2, 1, 1, 1, 0, 0, 0);

  -- Link consolidated episodes to their jobs
  UPDATE episodes SET consolidation_job_id = v_job1 WHERE episode_id IN (v_ep1, v_ep2, v_ep3, v_ep4, v_ep6, v_ep7);
  UPDATE episodes SET consolidation_job_id = v_job2 WHERE episode_id IN (v_sep1, v_sep2, v_sep3, v_sep4);
  UPDATE episodes SET consolidation_job_id = v_job3 WHERE episode_id IN (v_eep1, v_eep2);

  -- =========================================================================
  -- UPDATE AGENT STATS
  -- =========================================================================
  UPDATE agents SET
    total_executions = 15,
    successful_executions = 13,
    failed_executions = 1,
    total_cost_usd = 0.0690,
    avg_execution_time_ms = 3100,
    dreaming_budget_credits = 10,
    dreaming_credits_used = 1,
    last_consolidated_at = NOW() - INTERVAL '30 days'
  WHERE agent_id = v_mr_id;

  UPDATE agents SET
    total_executions = 10,
    successful_executions = 9,
    failed_executions = 1,
    total_cost_usd = 0.0502,
    avg_execution_time_ms = 3310,
    dreaming_budget_credits = 5,
    dreaming_credits_used = 1,
    last_consolidated_at = NOW() - INTERVAL '20 days'
  WHERE agent_id = v_sa_id;

  UPDATE agents SET
    total_executions = 5,
    successful_executions = 4,
    failed_executions = 0,
    total_cost_usd = 0.0234,
    avg_execution_time_ms = 2880,
    dreaming_budget_credits = 3,
    dreaming_credits_used = 1,
    last_consolidated_at = NOW() - INTERVAL '6 days'
  WHERE agent_id = v_ep_id;

  RAISE NOTICE 'Seed data inserted: 30 episodes, 8 rules, 12 entities, 10 facts, 3 communities, 3 consolidation jobs';
END $$;
