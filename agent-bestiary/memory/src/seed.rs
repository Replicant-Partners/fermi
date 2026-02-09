//! Regression test seed data for all ADM pipeline tables.
//!
//! Provides deterministic, interconnected test data across agents, episodes,
//! rules, entities, facts, communities, and consolidation jobs.
//!
//! Usage:
//! ```no_run
//! let seed = SeedData::build();
//! seed.seed(&store).await?;
//! // ... run tests ...
//! seed.cleanup(&store).await?;
//! ```

use crate::{
    Agent, Cardinality, Community, Entity, Episode, ExecutionStatus, Fact, SemanticRule,
    VerificationStatus,
};
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

/// Deterministic UUID from (agent_index, table_code, item_index).
/// Table codes: Agent=0, Episode=1, Rule=2, Entity=3, Fact=4, Community=5, Job=6
fn make_uuid(agent_idx: u8, table_code: u8, item_idx: u8) -> Uuid {
    Uuid::from_u128((agent_idx as u128) << 96 | (table_code as u128) << 64 | (item_idx as u128))
}

/// Deterministic 1024-dim embedding from a seed value.
/// Different seeds produce different but reproducible vectors with real variance for PCA.
fn make_embedding(seed: u64) -> Vec<f32> {
    (0..1024)
        .map(|i| ((seed as f64 * 0.1 + i as f64 * 0.01).sin() as f32))
        .collect()
}

/// All seed data held in memory for insertion and cleanup.
pub struct SeedData {
    pub agents: Vec<Agent>,
    pub episodes: Vec<Episode>,
    pub rules: Vec<SemanticRule>,
    pub entities: Vec<Entity>,
    pub facts: Vec<Fact>,
    pub communities: Vec<Community>,
    /// (agent_id, episode_range_start, episode_range_end, is_completed, stats, error_msg)
    pub consolidation_jobs: Vec<ConsolidationJobSeed>,
    /// Episode IDs to mark consolidated (with their job_id)
    pub consolidated_episodes: Vec<(Vec<Uuid>, Uuid)>,
    /// Rule IDs to deactivate after insertion
    pub rules_to_deactivate: Vec<Uuid>,
}

pub struct ConsolidationJobSeed {
    pub agent_id: Uuid,
    pub episode_range_start: Uuid,
    pub episode_range_end: Uuid,
    pub is_completed: bool,
    pub episodes_processed: i32,
    pub clusters_identified: i32,
    pub rules_extracted: i32,
    pub rules_verified: i32,
    pub rules_rejected: i32,
    pub entities_created: i32,
    pub facts_created: i32,
    pub error_message: Option<String>,
}

impl SeedData {
    /// Build all seed data in memory. Pure function, no DB access.
    pub fn build() -> Self {
        let base_time = Utc::now() - Duration::days(30);

        let agents = Self::build_agents();
        let mut episodes = Vec::new();
        let mut rules = Vec::new();
        let mut entities = Vec::new();
        let mut facts = Vec::new();
        let mut communities = Vec::new();
        let mut consolidation_jobs = Vec::new();
        let mut consolidated_episodes = Vec::new();
        let mut rules_to_deactivate = Vec::new();

        for (ai, agent) in agents.iter().enumerate() {
            let ai = ai as u8;

            let (agent_episodes, consol_ep_ids) =
                Self::build_episodes(ai, agent.agent_id, base_time);
            let agent_rules = Self::build_rules(ai, agent.agent_id, &agent_episodes, base_time);
            let agent_entities =
                Self::build_entities(ai, agent.agent_id, &agent_episodes, base_time);
            let agent_facts = Self::build_facts(
                ai,
                agent.agent_id,
                &agent_entities,
                &agent_episodes,
                base_time,
            );
            let agent_communities =
                Self::build_communities(ai, agent.agent_id, &agent_entities, base_time);
            let agent_jobs = Self::build_consolidation_jobs(ai, agent.agent_id, &agent_episodes);

            // Track which episodes to consolidate (first 10 per agent)
            let job_uuid = make_uuid(ai, 6, 0); // completed job
            consolidated_episodes.push((consol_ep_ids, job_uuid));

            // Track rules to deactivate (rejected + superseded)
            for rule in &agent_rules {
                if !rule.is_active {
                    rules_to_deactivate.push(rule.rule_id);
                }
            }

            episodes.extend(agent_episodes);
            rules.extend(agent_rules);
            entities.extend(agent_entities);
            facts.extend(agent_facts);
            communities.extend(agent_communities);
            consolidation_jobs.extend(agent_jobs);
        }

        SeedData {
            agents,
            episodes,
            rules,
            entities,
            facts,
            communities,
            consolidation_jobs,
            consolidated_episodes,
            rules_to_deactivate,
        }
    }

    fn build_agents() -> Vec<Agent> {
        vec![
            Agent {
                agent_id: make_uuid(0, 0, 0),
                agent_name: "seed_market_research".to_string(),
                agent_type: "research".to_string(),
                version: "2.1.0".to_string(),
                tier: "professional".to_string(),
                executor_type: "llm".to_string(),
                model: "claude-sonnet-4-5-20250929".to_string(),
                temperature: 0.3,
                mcp_servers: Some(
                    json!([{"name": "market-data", "url": "https://api.marketdata.example"}]),
                ),
                description: Some("Semiconductor market analysis and forecasting".to_string()),
                author: "fermi-lab".to_string(),
                current_ontology_commit: None,
                current_ontology_snapshot_id: None,
                last_consolidated_at: None,
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                total_cost_usd: None,
                avg_execution_time_ms: 0,
                dreaming_budget_credits: 10,
                dreaming_credits_used: 3,
                dreaming_budget_reset_at: None,
                system_prompt: None,
                visibility: "public".to_string(),
                owner_id: None,
                tags: vec![],
                education_budget_credits: 0,
                education_credits_used: 0,
                display_alias: None,
                llm_provider: "anthropic".to_string(),
                embedding_provider: "anthropic".to_string(),
                embedding_model: "voyage-2".to_string(),
                embedding_dimension: 1024,
                sample_queries: vec![],
            },
            Agent {
                agent_id: make_uuid(1, 0, 0),
                agent_name: "seed_geopolitical_risk".to_string(),
                agent_type: "risk".to_string(),
                version: "1.5.0".to_string(),
                tier: "enterprise".to_string(),
                executor_type: "llm".to_string(),
                model: "mistral-large".to_string(),
                temperature: 0.2,
                mcp_servers: Some(json!([
                    {"name": "news-feed", "url": "https://news.example"},
                    {"name": "sanctions-db", "url": "https://sanctions.example"}
                ])),
                description: Some(
                    "Geopolitical conflict and sanctions risk assessment".to_string(),
                ),
                author: "fermi-lab".to_string(),
                current_ontology_commit: None,
                current_ontology_snapshot_id: None,
                last_consolidated_at: None,
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                total_cost_usd: None,
                avg_execution_time_ms: 0,
                dreaming_budget_credits: 5,
                dreaming_credits_used: 5,
                dreaming_budget_reset_at: None,
                system_prompt: None,
                visibility: "public".to_string(),
                owner_id: None,
                tags: vec![],
                education_budget_credits: 0,
                education_credits_used: 0,
                display_alias: None,
                llm_provider: "anthropic".to_string(),
                embedding_provider: "anthropic".to_string(),
                embedding_model: "voyage-2".to_string(),
                embedding_dimension: 1024,
                sample_queries: vec![],
            },
            Agent {
                agent_id: make_uuid(2, 0, 0),
                agent_name: "seed_crypto_sentiment".to_string(),
                agent_type: "sentiment".to_string(),
                version: "1.0.0".to_string(),
                tier: "free".to_string(),
                executor_type: "llm".to_string(),
                model: "qwen-max".to_string(),
                temperature: 0.5,
                mcp_servers: None,
                description: Some("Crypto and DeFi sentiment tracking".to_string()),
                author: "community".to_string(),
                current_ontology_commit: None,
                current_ontology_snapshot_id: None,
                last_consolidated_at: None,
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                total_cost_usd: None,
                avg_execution_time_ms: 0,
                dreaming_budget_credits: 0,
                dreaming_credits_used: 0,
                dreaming_budget_reset_at: None,
                system_prompt: None,
                visibility: "public".to_string(),
                owner_id: None,
                tags: vec![],
                education_budget_credits: 0,
                education_credits_used: 0,
                display_alias: None,
                llm_provider: "anthropic".to_string(),
                embedding_provider: "anthropic".to_string(),
                embedding_model: "voyage-2".to_string(),
                embedding_dimension: 1024,
                sample_queries: vec![],
            },
        ]
    }

    /// Build 25 episodes per agent. Returns (episodes, ids_to_consolidate).
    fn build_episodes(
        ai: u8,
        agent_id: Uuid,
        base_time: DateTime<Utc>,
    ) -> (Vec<Episode>, Vec<Uuid>) {
        let queries: &[&[&str]] = &[
            // Agent 0: market_research
            &[
                "AMD Q4 earnings impact on datacenter GPU market",
                "NVIDIA H100 supply chain constraints analysis",
                "Intel Gaudi3 competitive positioning vs AMD MI300X",
                "TSMC 3nm yield rates and capacity allocation",
                "Samsung HBM3E production timeline for AI accelerators",
                "Qualcomm Snapdragon X Elite laptop market penetration",
                "Broadcom VMware acquisition synergy forecast",
                "ARM IPO valuation relative to semiconductor peers",
                "ASML EUV lithography demand from foundries",
                "Micron HBM revenue contribution projections",
                "AMD EPYC Turin server market share trajectory",
                "SK Hynix HBM3 pricing power analysis",
                "Apple M4 chip impact on Intel laptop revenue",
                "GlobalFoundries specialty node demand from automotive",
                "Semiconductor inventory cycle Q1 2026 outlook",
                "China domestic GPU development Huawei Ascend 910C",
                "NVIDIA Blackwell B200 ramp timeline",
                "AMD Instinct MI350 architectural advantages",
                "Memory bandwidth bottleneck in LLM inference",
                "Marvell custom silicon TAM expansion",
                "Lattice Semiconductor FPGA market niche analysis",
                "Semiconductor capex trends 2026 forecast",
                "Advanced packaging CoWoS capacity constraints",
                "Wolfspeed SiC wafer cost reduction trajectory",
                "Synopsys Ansys merger EDA market consolidation",
            ],
            // Agent 1: geopolitical_risk
            &[
                "Russia sanctions evasion through crypto exchanges",
                "Taiwan Strait military escalation probability",
                "EU carbon border adjustment impact on trade flows",
                "Iran nuclear deal collapse scenario analysis",
                "South China Sea freedom of navigation tensions",
                "India-China LAC border standoff risk assessment",
                "OPEC+ production cut geopolitical dynamics",
                "North Korea missile test frequency escalation",
                "Ethiopia Tigray conflict humanitarian impact",
                "Myanmar civil war spillover to ASEAN neighbors",
                "Arctic resource competition Russia-NATO dynamics",
                "Red Sea Houthi shipping disruption duration",
                "Venezuela Guyana Essequibo territorial dispute",
                "Sudan RSF-SAF conflict resolution prospects",
                "Moldova Transnistria Russian influence operations",
                "Sahel region military coup contagion risk",
                "Lebanon Hezbollah-Israel ceasefire durability",
                "Pakistan-Afghanistan TTP cross-border dynamics",
                "Niger uranium supply disruption to France",
                "Libya oil production factional control analysis",
                "Turkey-Greece Aegean maritime boundary tensions",
                "Chile lithium nationalization foreign investment impact",
                "DRC cobalt mining regulation enforcement gaps",
                "Kazakhstan Russian minority political risk",
                "Georgia EU accession path obstruction by Russia",
            ],
            // Agent 2: crypto_sentiment
            &[
                "Bitcoin ETF net flow sentiment after halving",
                "Ethereum L2 TVL migration from mainnet analysis",
                "Solana MEV extraction impact on retail sentiment",
                "Tether USDT reserve transparency sentiment shift",
                "Coinbase regulatory clarity impact on listings",
                "Uniswap v4 hook ecosystem developer sentiment",
                "Aave GHO stablecoin adoption trajectory",
                "MakerDAO endgame SubDAO governance sentiment",
                "Arbitrum ARB token unlock selling pressure",
                "Polygon zkEVM vs Optimism Bedrock performance debate",
                "Lido stETH dominance centralization concerns",
                "Chainlink CCIP cross-chain adoption metrics",
                "Blur NFT marketplace wash trading sentiment",
                "Friend.tech SocialFi sustainability skepticism",
                "Celestia modular blockchain thesis validation",
                "EigenLayer restaking systematic risk sentiment",
                "Worldcoin iris scanning privacy backlash",
                "Jupiter DEX aggregator Solana ecosystem dominance",
                "Pyth Network oracle expansion to EVM chains",
                "Jito MEV rewards SOL staking attractiveness",
                "StarkNet STRK airdrop community reception",
                "ZKSync Era token distribution fairness debate",
                "Base L2 Coinbase institutional onboarding",
                "Aptos Move language developer adoption velocity",
                "Sui object-centric model vs account model sentiment",
            ],
        ];

        let errors: &[&str] = &[
            "API rate limit exceeded: 429 Too Many Requests",
            "Model context window overflow: input 145000 tokens exceeds 128000 limit",
            "External data source timeout after 30s: market-data API unreachable",
            "JSON parse error: unexpected token in structured output response",
        ];

        let mut episodes = Vec::new();
        let mut consolidated_ids = Vec::new();
        let agent_queries = queries[ai as usize];

        for i in 0..25u8 {
            let episode_id = make_uuid(ai, 1, i);
            let day_offset = (i as i64) * 30 / 25; // spread across 30 days
            let timestamp = base_time + Duration::days(day_offset) + Duration::hours(i as i64 % 12);

            // Status distribution: 18 success, 4 failure, 3 partial
            let (status, error_details) = if i < 18 {
                (ExecutionStatus::Success, None)
            } else if i < 22 {
                (
                    ExecutionStatus::Failure,
                    Some(errors[(i - 18) as usize].to_string()),
                )
            } else {
                (
                    ExecutionStatus::Partial,
                    Some("Partial results: 2 of 5 data sources responded".to_string()),
                )
            };

            // Embeddings: all success + failures have embeddings, partials: only first one
            let has_embedding = i < 22 || i == 22;
            let embedding = if has_embedding {
                Some(make_embedding(ai as u64 * 1000 + i as u64))
            } else {
                None
            };

            // First 10 are consolidated
            let consolidated = i < 10;
            if consolidated {
                consolidated_ids.push(episode_id);
            }

            episodes.push(Episode {
                episode_id,
                agent_id,
                timestamp_ref: timestamp,
                query: agent_queries[i as usize].to_string(),
                context: json!({
                    "source": "seed",
                    "agent_index": ai,
                    "episode_index": i,
                }),
                execution_status: status,
                error_details,
                execution_time_ms: 500 + (i as i64 * 100),
                tokens_used: Some(1000 + i as i32 * 200),
                cost_usd: Some(Decimal::new(1 + i as i64, 3)),
                embedding,
                consolidated,
            });
        }

        (episodes, consolidated_ids)
    }

    /// Build 6 rules per agent.
    fn build_rules(
        ai: u8,
        agent_id: Uuid,
        episodes: &[Episode],
        base_time: DateTime<Utc>,
    ) -> Vec<SemanticRule> {
        let rule_templates: &[&[(&str, &str)]] = &[
            // Agent 0: market_research
            &[
                ("When AMD releases datacenter products, stock price increases within 2 weeks", "Observed in MI300X and EPYC Turin launches"),
                ("TSMC capacity allocation favors Apple and NVIDIA over smaller fabless firms", "Consistent pattern across 3nm and 5nm nodes"),
                ("HBM supply constraints correlate with AI accelerator revenue beats", "SK Hynix and Micron earnings confirm"),
                ("Intel foundry delays push customers to TSMC, increasing lead times", "Seen in Qualcomm and MediaTek ordering patterns"),
                ("Semiconductor inventory cycles lag end-demand by 1-2 quarters", "Historical pattern validated 2023-2025"),
                ("China domestic chip development accelerates under export controls", "Huawei Ascend 910C contradicts containment thesis"),
            ],
            // Agent 1: geopolitical_risk
            &[
                ("Sanctions evasion routes shift to crypto within 3 months of new restrictions", "Russia-Iran-DPRK pattern observed"),
                ("Military buildup in Taiwan Strait increases before political transitions", "Pre-election pattern 2024, 2020"),
                ("Red Sea shipping disruptions last longer than initial 90-day estimates", "Houthi attacks persisted beyond ceasefire attempts"),
                ("Sahel military coups trigger cascading instability in neighboring states", "Mali-Burkina Faso-Niger sequence"),
                ("Arctic resource claims escalate during periods of NATO-Russia tension", "Correlation with Ukraine conflict intensity"),
                ("Lithium nationalization rhetoric increases before elections in producing countries", "Chile, Mexico, Bolivia pattern"),
            ],
            // Agent 2: crypto_sentiment
            &[
                ("Bitcoin ETF inflows correlate with positive sentiment 24-48 hours before price moves", "Grayscale GBTC vs BlackRock IBIT flow analysis"),
                ("L2 TVL growth inversely correlates with mainnet gas sentiment", "Arbitrum and Optimism migration accelerates during fee spikes"),
                ("Token unlock events create negative sentiment 1-2 weeks before actual unlock", "ARB, OP, STRK patterns"),
                ("DEX aggregator dominance shifts sentiment away from individual DEX governance tokens", "Jupiter effect on Raydium, Orca"),
                ("Restaking protocols generate fear sentiment proportional to TVL growth rate", "EigenLayer systemic risk concerns"),
                ("Privacy-focused project backlash increases with regulatory clarity", "Worldcoin, Tornado Cash spillover sentiment"),
            ],
        ];

        let templates = rule_templates[ai as usize];
        let source_eps: Vec<Uuid> = episodes[..3].iter().map(|e| e.episode_id).collect();

        templates
            .iter()
            .enumerate()
            .map(|(i, (content, description))| {
                let i = i as u8;
                // 0,1=verified+active, 2,3=pending+active, 4=rejected+inactive, 5=verified+inactive(superseded)
                let (status, is_active, confidence) = match i {
                    0 => (VerificationStatus::Verified, true, 0.92),
                    1 => (VerificationStatus::Verified, true, 0.87),
                    2 => (VerificationStatus::Pending, true, 0.65),
                    3 => (VerificationStatus::Pending, true, 0.55),
                    4 => (VerificationStatus::Rejected, false, 0.22),
                    5 => (VerificationStatus::Verified, false, 0.88),
                    _ => unreachable!(),
                };

                let has_embedding = i < 4; // verified and pending have embeddings
                SemanticRule {
                    rule_id: make_uuid(ai, 2, i),
                    agent_id,
                    rule_content: content.to_string(),
                    rule_description: Some(description.to_string()),
                    confidence_score: confidence,
                    verification_status: status,
                    verification_method: if i < 2 {
                        Some("historical_backtest".to_string())
                    } else {
                        None
                    },
                    source_episode_cluster: source_eps.clone(),
                    episode_count: 3,
                    embedding: if has_embedding {
                        Some(make_embedding(100 + ai as u64 * 100 + i as u64))
                    } else {
                        None
                    },
                    is_active,
                    created_at: base_time + Duration::days(10 + i as i64 * 3),
                }
            })
            .collect()
    }

    /// Build 10 entities per agent.
    fn build_entities(
        ai: u8,
        agent_id: Uuid,
        episodes: &[Episode],
        base_time: DateTime<Utc>,
    ) -> Vec<Entity> {
        let entity_templates: &[&[(&str, &str, Option<&str>)]] = &[
            // Agent 0: market_research (name, type, summary)
            &[
                (
                    "AMD",
                    "Company",
                    Some("Advanced Micro Devices — datacenter GPU and CPU manufacturer"),
                ),
                (
                    "NVIDIA",
                    "Company",
                    Some("GPU market leader, dominant in AI training accelerators"),
                ),
                (
                    "TSMC",
                    "Company",
                    Some("Taiwan Semiconductor Manufacturing Company, leading-edge foundry"),
                ),
                (
                    "HBM3E",
                    "Technology",
                    Some("High Bandwidth Memory 3E, next-gen memory for AI chips"),
                ),
                (
                    "Datacenter GPU Market",
                    "Market",
                    Some("$80B+ TAM for AI/ML accelerator hardware"),
                ),
                ("Lisa Su", "Person", Some("CEO of AMD")),
                ("MI300X Launch", "Event", None), // no summary edge case
                (
                    "Intel",
                    "Company",
                    Some("x86 processor and foundry company"),
                ),
                (
                    "CoWoS Packaging",
                    "Technology",
                    Some("Chip-on-Wafer-on-Substrate advanced packaging by TSMC"),
                ),
                (
                    "MI350 Architecture",
                    "Technology",
                    Some("Next-gen AMD Instinct GPU architecture"),
                ),
            ],
            // Agent 1: geopolitical_risk
            &[
                (
                    "Russian Federation",
                    "Country",
                    Some("Subject of extensive Western sanctions regime"),
                ),
                (
                    "Taiwan",
                    "Territory",
                    Some("Self-governing island, semiconductor manufacturing hub"),
                ),
                (
                    "Houthi Movement",
                    "Organization",
                    Some("Yemen-based militia disrupting Red Sea shipping"),
                ),
                (
                    "OPEC+",
                    "Organization",
                    Some("Oil production cartel including Russia"),
                ),
                (
                    "Wagner Group",
                    "Organization",
                    Some("Russian private military company active in Africa"),
                ),
                (
                    "Sahel Region",
                    "Region",
                    Some("Sub-Saharan belt experiencing military coup cascade"),
                ),
                (
                    "Nord Stream Pipeline",
                    "Infrastructure",
                    Some("Sabotaged Russia-EU gas pipeline"),
                ),
                ("LAC Border", "Region", None), // no summary
                (
                    "Suez Canal",
                    "Infrastructure",
                    Some("Critical shipping chokepoint affected by Houthi attacks"),
                ),
                (
                    "SWIFT System",
                    "Technology",
                    Some("Global financial messaging network used for sanctions"),
                ),
            ],
            // Agent 2: crypto_sentiment
            &[
                (
                    "Bitcoin",
                    "Protocol",
                    Some("Largest cryptocurrency by market cap"),
                ),
                (
                    "Ethereum",
                    "Protocol",
                    Some("Smart contract platform, transitioning to L2-centric roadmap"),
                ),
                ("Solana", "Protocol", Some("High-throughput L1 blockchain")),
                (
                    "Arbitrum",
                    "Protocol",
                    Some("Ethereum L2 optimistic rollup"),
                ),
                (
                    "EigenLayer",
                    "Protocol",
                    Some("Restaking protocol on Ethereum"),
                ),
                (
                    "Coinbase",
                    "Company",
                    Some("Largest US crypto exchange, Base L2 operator"),
                ),
                ("Tether", "Company", Some("USDT stablecoin issuer")),
                ("BlackRock", "Company", None), // no summary
                (
                    "DeFi Lending",
                    "Market",
                    Some("Decentralized lending protocols: Aave, Compound, MakerDAO"),
                ),
                (
                    "MEV Extraction",
                    "Technology",
                    Some("Maximal extractable value in transaction ordering"),
                ),
            ],
        ];

        let source_eps: Vec<Uuid> = episodes[..2].iter().map(|e| e.episode_id).collect();
        let templates = entity_templates[ai as usize];

        templates
            .iter()
            .enumerate()
            .map(|(i, (name, etype, summary))| {
                let i = i as u8;
                // 7 valid, 3 invalidated (indices 7,8,9)
                let t_invalid = if i >= 7 {
                    Some(base_time + Duration::days(20))
                } else {
                    None
                };

                let has_embedding = i < 8; // most have embeddings, last 2 don't
                Entity {
                    entity_id: make_uuid(ai, 3, i),
                    agent_id,
                    entity_name: name.to_string(),
                    entity_type: etype.to_string(),
                    summary: summary.map(|s| s.to_string()),
                    t_valid: base_time + Duration::days(i as i64),
                    t_invalid,
                    source_episodes: source_eps.clone(),
                    extraction_confidence: 0.5 + (i as f64) * 0.05,
                    embedding: if has_embedding {
                        Some(make_embedding(200 + ai as u64 * 100 + i as u64))
                    } else {
                        None
                    },
                }
            })
            .collect()
    }

    /// Build 12 facts per agent.
    fn build_facts(
        ai: u8,
        agent_id: Uuid,
        entities: &[Entity],
        episodes: &[Episode],
        base_time: DateTime<Utc>,
    ) -> Vec<Fact> {
        // (source_idx, target_idx, relation, cardinality, confidence, invalidated)
        let fact_specs: &[(usize, usize, &str, Cardinality, f64, bool)] = &[
            (0, 4, "competes_in", Cardinality::ManyToMany, 0.95, false),
            (1, 4, "dominates", Cardinality::OneToMany, 0.92, false),
            (0, 2, "manufactured_by", Cardinality::ManyToOne, 0.98, false),
            (1, 2, "manufactured_by", Cardinality::ManyToOne, 0.97, false),
            (3, 4, "enables", Cardinality::OneToMany, 0.88, false),
            (5, 0, "leads", Cardinality::OneToOne, 0.99, false),
            (0, 1, "competes_with", Cardinality::ManyToMany, 0.90, false),
            (7, 2, "manufactured_by", Cardinality::ManyToOne, 0.75, false),
            (6, 0, "milestone_for", Cardinality::OneToOne, 0.85, false),
            // Invalidated facts
            (8, 1, "supplies_to", Cardinality::ManyToMany, 0.70, true),
            (9, 0, "successor_of", Cardinality::OneToOne, 0.60, true),
            (7, 1, "partners_with", Cardinality::ManyToMany, 0.45, true),
        ];

        let source_eps: Vec<Uuid> = episodes[..2].iter().map(|e| e.episode_id).collect();

        fact_specs
            .iter()
            .enumerate()
            .map(|(i, (src, tgt, relation, card, conf, invalidated))| {
                let t_invalid = if *invalidated {
                    Some(base_time + Duration::days(22))
                } else {
                    None
                };

                Fact {
                    fact_id: make_uuid(ai, 4, i as u8),
                    agent_id,
                    source_entity_id: entities[*src].entity_id,
                    target_entity_id: entities[*tgt].entity_id,
                    relation_type: relation.to_string(),
                    relation_cardinality: card.clone(),
                    confidence: *conf,
                    reasoning: Some(format!("Extracted from {} episodes", source_eps.len())),
                    t_valid: base_time + Duration::days(5),
                    t_invalid,
                    source_episodes: source_eps.clone(),
                }
            })
            .collect()
    }

    /// Build 3 communities per agent.
    fn build_communities(
        ai: u8,
        agent_id: Uuid,
        entities: &[Entity],
        base_time: DateTime<Utc>,
    ) -> Vec<Community> {
        vec![
            // Fully populated community
            Community {
                community_id: make_uuid(ai, 5, 0),
                agent_id,
                community_name: Some(format!("Core {} Cluster", entities[0].entity_name)),
                summary: Some(format!(
                    "Primary cluster around {} and related entities",
                    entities[0].entity_name
                )),
                member_entity_ids: vec![
                    entities[0].entity_id,
                    entities[1].entity_id,
                    entities[2].entity_id,
                    entities[4].entity_id,
                ],
                member_count: 4,
                embedding: Some(make_embedding(300 + ai as u64 * 100)),
                created_at: base_time + Duration::days(15),
            },
            // Community with no name (edge case)
            Community {
                community_id: make_uuid(ai, 5, 1),
                agent_id,
                community_name: None,
                summary: Some("Secondary cluster of peripheral entities".to_string()),
                member_entity_ids: vec![
                    entities[3].entity_id,
                    entities[5].entity_id,
                    entities[6].entity_id,
                ],
                member_count: 3,
                embedding: Some(make_embedding(301 + ai as u64 * 100)),
                created_at: base_time + Duration::days(18),
            },
            // Community with no embedding (edge case)
            Community {
                community_id: make_uuid(ai, 5, 2),
                agent_id,
                community_name: Some("Emerging pattern".to_string()),
                summary: None,
                member_entity_ids: vec![entities[7].entity_id, entities[8].entity_id],
                member_count: 2,
                embedding: None,
                created_at: base_time + Duration::days(25),
            },
        ]
    }

    /// Build 2 consolidation jobs per agent.
    fn build_consolidation_jobs(
        _ai: u8,
        agent_id: Uuid,
        episodes: &[Episode],
    ) -> Vec<ConsolidationJobSeed> {
        vec![
            // Completed job
            ConsolidationJobSeed {
                agent_id,
                episode_range_start: episodes[0].episode_id,
                episode_range_end: episodes[9].episode_id,
                is_completed: true,
                episodes_processed: 10,
                clusters_identified: 3,
                rules_extracted: 4,
                rules_verified: 2,
                rules_rejected: 1,
                entities_created: 6,
                facts_created: 8,
                error_message: None,
            },
            // Failed job
            ConsolidationJobSeed {
                agent_id,
                episode_range_start: episodes[10].episode_id,
                episode_range_end: episodes[17].episode_id,
                is_completed: false,
                episodes_processed: 3,
                clusters_identified: 1,
                rules_extracted: 0,
                rules_verified: 0,
                rules_rejected: 0,
                entities_created: 0,
                facts_created: 0,
                error_message: Some(
                    "LLM provider timeout during rule extraction: Anthropic API 504".to_string(),
                ),
            },
        ]
    }

    /// Insert all seed data into the database.
    pub async fn seed(&self, store: &crate::store::MemoryStore) -> crate::Result<()> {
        // 1. Agents
        for agent in &self.agents {
            store.upsert_agent(agent.clone()).await?;
        }

        // 2. Episodes
        for episode in &self.episodes {
            store.store_episode(episode.clone()).await?;
        }

        // 3. Rules (insert all as active first, deactivate later)
        for rule in &self.rules {
            // Store with is_active = true initially; we'll deactivate rejected ones after
            let mut active_rule = rule.clone();
            active_rule.is_active = true;
            store.store_semantic_rule(active_rule).await?;
        }

        // 4. Entities
        for entity in &self.entities {
            store.store_entity(entity.clone()).await?;
        }

        // 5. Facts
        for fact in &self.facts {
            store.store_fact(fact.clone()).await?;
        }

        // 6. Communities
        for community in &self.communities {
            store.store_community(community.clone()).await?;
        }

        // 7. Consolidation jobs
        for job in &self.consolidation_jobs {
            let job_id = store
                .create_consolidation_job(
                    job.agent_id,
                    job.episode_range_start,
                    job.episode_range_end,
                )
                .await?;

            store
                .update_consolidation_job(
                    job_id,
                    job.episodes_processed,
                    job.clusters_identified,
                    job.rules_extracted,
                    job.rules_verified,
                    job.rules_rejected,
                    job.entities_created,
                    job.facts_created,
                )
                .await?;

            if job.is_completed {
                store
                    .complete_consolidation_job(job_id, "completed", None)
                    .await?;
            } else {
                store
                    .complete_consolidation_job(job_id, "failed", job.error_message.clone())
                    .await?;
            }
        }

        // 8. Mark consolidated episodes
        for (ep_ids, _job_id) in &self.consolidated_episodes {
            // We can't use the stored job_id (create_consolidation_job generates its own),
            // so we use mark_episodes_consolidated which just sets consolidated=true
            // The episodes are already created with consolidated=true, so this is a no-op
            // But we call it to exercise the method
            let dummy_job_id = Uuid::new_v4();
            store
                .mark_episodes_consolidated(ep_ids, dummy_job_id)
                .await?;
        }

        // 9. Deactivate rejected/superseded rules
        for rule_id in &self.rules_to_deactivate {
            store.deactivate_rule(*rule_id).await?;
        }

        Ok(())
    }

    /// Remove all seed data from the database.
    /// Uses CASCADE: deleting agents removes all child records.
    pub async fn cleanup(&self, store: &crate::store::MemoryStore) -> crate::Result<()> {
        for agent in &self.agents {
            sqlx::query("DELETE FROM agents WHERE agent_id = $1")
                .bind(agent.agent_id)
                .execute(store.pool())
                .await?;
        }
        Ok(())
    }

    // === Accessors for tests ===

    /// Get the first agent (seed_market_research)
    pub fn market_research_agent(&self) -> &Agent {
        &self.agents[0]
    }

    /// Get the second agent (seed_geopolitical_risk)
    pub fn geopolitical_risk_agent(&self) -> &Agent {
        &self.agents[1]
    }

    /// Get the third agent (seed_crypto_sentiment)
    pub fn crypto_sentiment_agent(&self) -> &Agent {
        &self.agents[2]
    }

    /// Get episodes for a specific agent
    pub fn episodes_for(&self, agent_id: Uuid) -> Vec<&Episode> {
        self.episodes
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect()
    }

    /// Get entities for a specific agent
    pub fn entities_for(&self, agent_id: Uuid) -> Vec<&Entity> {
        self.entities
            .iter()
            .filter(|e| e.agent_id == agent_id)
            .collect()
    }
}
