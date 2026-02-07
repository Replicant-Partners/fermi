use anyhow::Result;
use clap::Parser;
use fermi_memory::{
    AnthropicProvider, ConsolidationLock, ConsolidationWorker, MemoryStore, OpenAIEmbeddings,
};
use fermi_ontology::{GitConfig, GitManager, MermaidConfig, MermaidGenerator, SnapshotManager};
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

    /// OpenAI API key for embeddings
    #[arg(long, env)]
    openai_api_key: String,

    /// Anthropic API key for LLM
    #[arg(long, env)]
    anthropic_api_key: String,

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

    // Initialize embedding generator
    let embedder = Arc::new(
        OpenAIEmbeddings::new(args.openai_api_key.clone())
            .with_model("text-embedding-3-small".to_string(), 1536),
    );
    info!("Initialized embedding generator");

    // Initialize LLM provider
    let llm = Arc::new(AnthropicProvider::new(
        args.anthropic_api_key.clone(),
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
