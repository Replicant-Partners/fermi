use agent_bestiary_memory::{
    AnthropicEmbeddings, AnthropicProvider, ConsolidationLock, ConsolidationWorker,
    EmbeddingGenerator, GenerationConfig, LLMProvider, MemoryStore, Message, MessageRole,
    MistralEmbeddings, OpenAIEmbeddings, QwenEmbeddings,
};
use agent_bestiary_ontology::{GitConfig, GitManager, MermaidGenerator, SnapshotManager};
use anyhow::{bail, Result};
use clap::Parser;
use sqlx::Row;
use std::sync::Arc;
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Agent ID to consolidate (if not provided, consolidates all agents)
    #[arg(short, long)]
    agent_id: Option<Uuid>,

    /// Database URL
    #[arg(long, env)]
    database_url: String,

    /// Embedding provider: anthropic, openai, mistral, qwen
    #[arg(long, env, default_value = "anthropic")]
    embedding_provider: String,

    /// Anthropic API key (for Voyage embeddings or LLM)
    #[arg(long, env)]
    anthropic_api_key: Option<String>,

    /// OpenAI API key (for OpenAI embeddings)
    #[arg(long, env)]
    openai_api_key: Option<String>,

    /// Mistral API key (for Mistral embeddings)
    #[arg(long, env)]
    mistral_api_key: Option<String>,

    /// Qwen API key (for Qwen embeddings)
    #[arg(long, env)]
    qwen_api_key: Option<String>,

    /// Embedding model (provider-specific)
    #[arg(long, env)]
    embedding_model: Option<String>,

    /// Embedding dimensions
    #[arg(long, env, default_value = "1024")]
    embedding_dimensions: usize,

    /// Base path for agent git repositories
    #[arg(long, env, default_value = "./agents")]
    agents_base_path: String,

    /// GitHub organization (e.g., "Replicant-Partners")
    #[arg(long, env)]
    github_org: Option<String>,

    /// GitHub personal access token
    #[arg(long, env)]
    github_token: Option<String>,

    /// Auto-push to GitHub after each commit
    #[arg(long, env)]
    auto_push_github: bool,

    /// DBSCAN epsilon parameter
    #[arg(long, default_value = "0.3")]
    epsilon: f64,

    /// DBSCAN min samples parameter
    #[arg(long, default_value = "2")]
    min_samples: usize,

    /// Worker ID
    #[arg(long, default_value = "worker-1")]
    worker_id: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env if available
    let _ = dotenvy::dotenv();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Parse arguments
    let args = Args::parse();

    info!("Starting Fermi consolidation worker");
    info!("Worker ID: {}", args.worker_id);
    info!("Database: {}", args.database_url);
    info!("Agent repos base: {}", args.agents_base_path);
    if let Some(ref org) = args.github_org {
        info!("GitHub org: {}", org);
        info!("Auto-push: {}", args.auto_push_github);
    }

    // Validate embedding dimensions match schema
    if args.embedding_dimensions != 1024 {
        error!(
            "⚠️  WARNING: Embedding dimensions set to {}, but PostgreSQL schema uses 1024d vectors",
            args.embedding_dimensions
        );
        error!("   This will cause database errors unless you've migrated the schema.");
        error!("   See docs/guides/EMBEDDING_MIGRATION.md for schema migration instructions.");
        bail!(
            "Embedding dimension mismatch: specified {}d, schema requires 1024d",
            args.embedding_dimensions
        );
    }

    // Initialize memory store
    let store = Arc::new(MemoryStore::new(&args.database_url).await?);
    info!("Connected to database");

    // Initialize embedding generator based on provider
    let embedder: Arc<dyn EmbeddingGenerator> = match args.embedding_provider.as_str() {
        "anthropic" => {
            let api_key = args.anthropic_api_key.as_ref().ok_or_else(|| {
                anyhow::anyhow!("ANTHROPIC_API_KEY required for anthropic provider")
            })?;
            let model = args
                .embedding_model
                .clone()
                .unwrap_or_else(|| "voyage-2".to_string());

            // Warn about models with non-1024d native dimensions
            if (model == "voyage-large-2" || model == "voyage-code-2")
                && args.embedding_dimensions == 1024
            {
                error!(
                    "⚠️  WARNING: Model {} natively produces 1536d embeddings",
                    model
                );
                error!("   Anthropic API doesn't support dimension reduction.");
                error!("   Results may be truncated or padded. Consider using voyage-2 (1024d native).");
            }

            info!(
                "Using Anthropic embeddings: model={}, dims={}",
                model, args.embedding_dimensions
            );
            Arc::new(
                AnthropicEmbeddings::new(api_key.clone())
                    .with_model(model, args.embedding_dimensions),
            )
        }
        "openai" => {
            let api_key = args
                .openai_api_key
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("OPENAI_API_KEY required for openai provider"))?;
            let model = args
                .embedding_model
                .unwrap_or_else(|| "text-embedding-3-large".to_string());
            info!(
                "Using OpenAI embeddings: model={}, dims={}",
                model, args.embedding_dimensions
            );
            Arc::new(
                OpenAIEmbeddings::new(api_key.clone()).with_model(model, args.embedding_dimensions),
            )
        }
        "mistral" => {
            let api_key = args
                .mistral_api_key
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("MISTRAL_API_KEY required for mistral provider"))?;
            let model = args
                .embedding_model
                .unwrap_or_else(|| "mistral-embed".to_string());
            info!(
                "Using Mistral embeddings: model={}, dims={}",
                model, args.embedding_dimensions
            );
            Arc::new(
                MistralEmbeddings::new(api_key.clone())
                    .with_model(model, args.embedding_dimensions),
            )
        }
        "qwen" => {
            let api_key = args
                .qwen_api_key
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("QWEN_API_KEY required for qwen provider"))?;
            let model = args
                .embedding_model
                .unwrap_or_else(|| "text-embedding-v3".to_string());
            info!(
                "Using Qwen embeddings: model={}, dims={}",
                model, args.embedding_dimensions
            );
            Arc::new(
                QwenEmbeddings::new(api_key.clone()).with_model(model, args.embedding_dimensions),
            )
        }
        _ => {
            bail!(
                "Unknown embedding provider: {}. Supported: anthropic, openai, mistral, qwen",
                args.embedding_provider
            );
        }
    };
    info!("Initialized embedding generator");

    // Initialize LLM provider (requires Anthropic API key)
    let anthropic_key = args
        .anthropic_api_key
        .ok_or_else(|| anyhow::anyhow!("ANTHROPIC_API_KEY required for LLM provider"))?;
    let llm = Arc::new(AnthropicProvider::new(
        anthropic_key,
        "claude-sonnet-4-5".to_string(),
        None,
    )?);
    info!("Initialized LLM provider");

    // Initialize consolidation lock
    let pool = Arc::new(store.pool().clone());
    let lock = Arc::new(ConsolidationLock::new(pool, args.worker_id.clone()));

    // Initialize consolidation worker
    let worker =
        ConsolidationWorker::with_llm(store.clone(), lock, embedder, llm, args.worker_id.clone());
    info!("Initialized consolidation worker");

    // Initialize ontology components
    // MemoryStore doesn't implement Clone, so we create a new connection
    let ontology_store = MemoryStore::new(&args.database_url).await?;
    let mermaid_generator = MermaidGenerator::new(ontology_store);

    let ontology_store2 = MemoryStore::new(&args.database_url).await?;
    let git_config = GitConfig {
        base_path: args.agents_base_path.clone(),
        author_name: "Fermi ADM".to_string(),
        author_email: "adm@fermi.ai".to_string(),
        branch: "main".to_string(),
        github_org: args.github_org.clone(),
        github_token: args.github_token.clone(),
        auto_push: args.auto_push_github,
        remote_name: "origin".to_string(),
    };
    let git_manager = GitManager::new(git_config)?;
    let snapshot_manager = Arc::new(SnapshotManager::new(
        ontology_store2,
        mermaid_generator,
        git_manager,
    ));
    info!("Initialized snapshot manager");

    // Consolidate agents
    if let Some(agent_id) = args.agent_id {
        // Consolidate specific agent
        info!("Consolidating agent: {}", agent_id);
        match consolidate_with_snapshot(
            &worker,
            &snapshot_manager,
            &store,
            llm.as_ref(),
            agent_id,
            args.epsilon,
            args.min_samples,
        )
        .await
        {
            Ok(result) => {
                info!("Consolidation completed successfully");
                info!("  Episodes processed: {}", result.episodes_processed);
                info!("  Clusters identified: {}", result.clusters_identified);
                info!("  Rules extracted: {}", result.rules_extracted);
                info!("  Rules verified: {}", result.rules_verified);
                info!("  Entities created: {}", result.entities_created);
                info!("  Facts created: {}", result.facts_created);
                if let Some(snapshot_id) = result.snapshot_id {
                    info!("  Snapshot ID: {}", snapshot_id);
                }
                if let Some(ref synopsis) = result.dream_synopsis {
                    info!(
                        "  Dream synopsis: {}...",
                        &synopsis[..synopsis.len().min(100)]
                    );
                }
            }
            Err(e) => {
                error!("Consolidation failed: {}", e);
                return Err(e);
            }
        }
    } else {
        // Consolidate all agents
        info!("Consolidating all agents");
        let agents = store.list_agents().await?;
        info!("Found {} agents", agents.len());

        for agent in agents {
            info!(
                "Consolidating agent: {} ({})",
                agent.agent_name, agent.agent_id
            );
            match consolidate_with_snapshot(
                &worker,
                &snapshot_manager,
                &store,
                llm.as_ref(),
                agent.agent_id,
                args.epsilon,
                args.min_samples,
            )
            .await
            {
                Ok(result) => {
                    info!(
                        "  Success: {} episodes, {} rules, {} entities",
                        result.episodes_processed, result.rules_extracted, result.entities_created
                    );
                    if let Some(snapshot_id) = result.snapshot_id {
                        info!("  Snapshot: {}", snapshot_id);
                    }
                    if result.dream_synopsis.is_some() {
                        info!("  Dream synopsis: generated");
                    }
                }
                Err(e) => {
                    error!("  Failed: {}", e);
                    // Continue with next agent
                }
            }
        }
    }

    info!("Consolidation worker completed");
    Ok(())
}

/// Consolidate an agent and create an ontology snapshot
async fn consolidate_with_snapshot(
    worker: &ConsolidationWorker,
    snapshot_manager: &SnapshotManager,
    store: &MemoryStore,
    llm: &dyn LLMProvider,
    agent_id: Uuid,
    epsilon: f64,
    min_samples: usize,
) -> Result<ConsolidationResult> {
    // Budget check: ensure agent has remaining dreaming credits
    let budget_row = sqlx::query(
        "SELECT dreaming_budget_credits, dreaming_credits_used FROM agents WHERE agent_id = $1",
    )
    .bind(agent_id)
    .fetch_optional(store.pool())
    .await?;

    if let Some(row) = &budget_row {
        let budget: i32 = row.try_get("dreaming_budget_credits")?;
        let used: i32 = row.try_get("dreaming_credits_used")?;
        if budget > 0 && used >= budget {
            warn!(
                "Agent {} has exhausted dreaming budget ({}/{}), skipping consolidation",
                agent_id, used, budget
            );
            anyhow::bail!("Dreaming budget exhausted ({}/{})", used, budget);
        }
        info!("Dreaming budget: {}/{} credits used", used, budget);
    }

    // Run consolidation
    let base_result = worker
        .consolidate_agent(agent_id, epsilon, min_samples)
        .await?;

    // Convert to our extended result type
    let mut result = ConsolidationResult::from(base_result);

    // Build consolidation stats JSON
    let stats = serde_json::json!({
        "episodes_processed": result.episodes_processed,
        "clusters_identified": result.clusters_identified,
        "rules_extracted": result.rules_extracted,
        "rules_verified": result.rules_verified,
        "rules_rejected": result.rules_rejected,
        "entities_created": result.entities_created,
        "facts_created": result.facts_created,
    });

    // Create ontology snapshot
    match snapshot_manager.create_snapshot(agent_id, None).await {
        Ok(snapshot_id) => {
            result.snapshot_id = Some(snapshot_id);
            info!("Created ontology snapshot: {}", snapshot_id);

            // Generate dream synopsis via LLM
            match generate_dream_synopsis(llm, &result, agent_id).await {
                Ok(synopsis) => {
                    info!("Generated dream synopsis ({} chars)", synopsis.len());
                    result.dream_synopsis = Some(synopsis.clone());

                    // Store synopsis on the snapshot
                    if let Err(e) = snapshot_manager
                        .update_snapshot_synopsis(snapshot_id, &synopsis, Some(&stats))
                        .await
                    {
                        error!("Failed to store dream synopsis on snapshot: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to generate dream synopsis: {}", e);
                    // Still store the stats even without synopsis
                    if let Err(e) = snapshot_manager
                        .update_snapshot_synopsis(snapshot_id, "", Some(&stats))
                        .await
                    {
                        error!("Failed to store consolidation stats on snapshot: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            error!("Failed to create ontology snapshot: {}", e);
            // Don't fail consolidation if snapshot fails
        }
    }

    // Increment dreaming credits used
    sqlx::query(
        "UPDATE agents SET dreaming_credits_used = dreaming_credits_used + 1 WHERE agent_id = $1",
    )
    .bind(agent_id)
    .execute(store.pool())
    .await?;

    Ok(result)
}

/// Generate a narrative dream synopsis from consolidation results
async fn generate_dream_synopsis(
    llm: &dyn LLMProvider,
    result: &ConsolidationResult,
    agent_id: Uuid,
) -> Result<String> {
    let prompt = format!(
        r#"You are an AI agent reflecting on a consolidation cycle — a "dream" where you processed
episodic memories and distilled them into semantic knowledge.

During this dreaming cycle for agent {agent_id}, the following occurred:
- Episodes processed: {episodes}
- Semantic clusters identified: {clusters}
- Rules extracted from patterns: {rules_extracted}
- Rules verified as reliable: {rules_verified}
- Rules rejected as unreliable: {rules_rejected}
- New entities discovered: {entities}
- New facts established: {facts}

Write a 2-3 paragraph narrative synopsis of this dreaming cycle. Describe what was learned,
what patterns emerged, what knowledge was consolidated. Write in first person as the agent
reflecting on its own cognitive process. Be specific about the nature of the knowledge gained
where possible — the clusters of related experiences, the rules that proved reliable vs those
that were rejected, the new entities and relationships discovered.

Keep the tone contemplative and precise. This synopsis will accompany the ontology snapshot
as a record of the agent's evolving understanding."#,
        agent_id = agent_id,
        episodes = result.episodes_processed,
        clusters = result.clusters_identified,
        rules_extracted = result.rules_extracted,
        rules_verified = result.rules_verified,
        rules_rejected = result.rules_rejected,
        entities = result.entities_created,
        facts = result.facts_created,
    );

    let messages = vec![Message {
        role: MessageRole::User,
        content: prompt,
    }];

    let config = GenerationConfig {
        temperature: 0.7,
        max_tokens: Some(1024),
        top_p: None,
        stop_sequences: vec![],
    };

    let response = llm.generate_raw(messages, &config).await?;
    Ok(response.content)
}

/// Extended consolidation result with snapshot info
#[derive(Debug, Clone)]
struct ConsolidationResult {
    pub episodes_processed: usize,
    pub clusters_identified: usize,
    pub rules_extracted: usize,
    pub rules_verified: usize,
    pub rules_rejected: usize,
    pub entities_created: usize,
    pub facts_created: usize,
    pub snapshot_id: Option<Uuid>,
    pub dream_synopsis: Option<String>,
}

impl From<agent_bestiary_memory::ConsolidationResult> for ConsolidationResult {
    fn from(r: agent_bestiary_memory::ConsolidationResult) -> Self {
        Self {
            episodes_processed: r.episodes_processed,
            clusters_identified: r.clusters_identified,
            rules_extracted: r.rules_extracted,
            rules_verified: r.rules_verified,
            rules_rejected: r.rules_rejected,
            entities_created: r.entities_created,
            facts_created: r.facts_created,
            snapshot_id: None,
            dream_synopsis: None,
        }
    }
}
