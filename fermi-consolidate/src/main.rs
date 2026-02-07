use anyhow::{bail, Result};
use clap::Parser;
use fermi_memory::{
    AnthropicEmbeddings, AnthropicProvider, ConsolidationLock, ConsolidationWorker,
    EmbeddingGenerator, MemoryStore, MistralEmbeddings, OpenAIEmbeddings, QwenEmbeddings,
};
use fermi_ontology::{GitConfig, GitManager, MermaidGenerator, SnapshotManager};
use std::sync::Arc;
use tracing::{error, info};
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

    /// Git repository path for ontologies
    #[arg(long, default_value = "./ontologies")]
    ontology_repo_path: String,

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
    info!("Ontology repo: {}", args.ontology_repo_path);

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
                .unwrap_or_else(|| "voyage-2".to_string());
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
        repo_path: args.ontology_repo_path.clone(),
        author_name: "Fermi ADM".to_string(),
        author_email: "adm@fermi.ai".to_string(),
        branch: "main".to_string(),
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
                agent.agent_id,
                args.epsilon,
                args.min_samples,
            )
            .await
            {
                Ok(result) => {
                    info!(
                        "  ✓ Success: {} episodes, {} rules, {} entities",
                        result.episodes_processed, result.rules_extracted, result.entities_created
                    );
                    if let Some(snapshot_id) = result.snapshot_id {
                        info!("  ✓ Snapshot: {}", snapshot_id);
                    }
                }
                Err(e) => {
                    error!("  ✗ Failed: {}", e);
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
    agent_id: Uuid,
    epsilon: f64,
    min_samples: usize,
) -> Result<ConsolidationResult> {
    // Run consolidation
    let base_result = worker
        .consolidate_agent(agent_id, epsilon, min_samples)
        .await?;

    // Convert to our extended result type
    let mut result = ConsolidationResult::from(base_result);

    // Create ontology snapshot
    match snapshot_manager.create_snapshot(agent_id, None).await {
        Ok(snapshot_id) => {
            result.snapshot_id = Some(snapshot_id);
            info!("Created ontology snapshot: {}", snapshot_id);
        }
        Err(e) => {
            error!("Failed to create ontology snapshot: {}", e);
            // Don't fail consolidation if snapshot fails
        }
    }

    Ok(result)
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
}

impl From<fermi_memory::ConsolidationResult> for ConsolidationResult {
    fn from(r: fermi_memory::ConsolidationResult) -> Self {
        Self {
            episodes_processed: r.episodes_processed,
            clusters_identified: r.clusters_identified,
            rules_extracted: r.rules_extracted,
            rules_verified: r.rules_verified,
            rules_rejected: r.rules_rejected,
            entities_created: r.entities_created,
            facts_created: r.facts_created,
            snapshot_id: None,
        }
    }
}
