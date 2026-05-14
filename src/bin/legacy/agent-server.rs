/// Fermi Agent Backend REST API Server
///
/// Provides HTTP API for agent management and execution.
///
/// Usage:
///   cargo run --bin agent-server
///
/// Endpoints:
///   GET  /health                 - Health check
///   GET  /agents                 - List all agents
///   POST /agents                 - Register new agent
///   GET  /agents/:id             - Get agent details
///   POST /agents/:id/execute     - Execute agent
use fermi::agent_backend::{AgentCard, AgentRegistry, LLMExecutor, MockExecutor};
use fermi::api::create_app;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "agent_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    println!("🤖 Fermi Agent Backend Server");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Create registry with appropriate executor
    let registry = if let Ok(llm_executor) = LLMExecutor::from_env() {
        println!("✓ Using LLM Executor (Claude API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(llm_executor)))
    } else {
        println!("⚠️  ANTHROPIC_API_KEY not set - using Mock Executor");
        println!("   Set ANTHROPIC_API_KEY environment variable to use real Claude API");
        Arc::new(AgentRegistry::new())
    };

    // Load agents from filesystem
    let agents_dir = std::env::var("AGENTS_DIR").unwrap_or_else(|_| "agents/curated".to_string());
    match registry.load_from_directory(&agents_dir) {
        Ok(count) if count > 0 => {
            println!("✓ Loaded {} agent(s) from {}", count, agents_dir);
        }
        _ => {
            println!("⚠️  No agents loaded from {}", agents_dir);
            println!("   Creating default agent...");

            // Register a default market research agent
            let market_agent =
                AgentCard::new("market_research".to_string(), "research".to_string());

            if let Err(e) = registry.register(market_agent) {
                eprintln!("Warning: Failed to register default agent: {}", e);
            } else {
                println!("✓ Registered default agent: market_research");
            }
        }
    }

    // Create app
    let app = create_app(registry);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .unwrap();

    let addr = listener.local_addr().unwrap();
    println!("\n🚀 Server running on http://{}", addr);
    println!("\nAvailable endpoints:");
    println!("  GET  http://{}/health", addr);
    println!("  GET  http://{}/agents", addr);
    println!("  POST http://{}/agents", addr);
    println!("  GET  http://{}/agents/:id", addr);
    println!("  POST http://{}/agents/:id/execute", addr);
    println!("\n📝 Try: curl http://{}/health\n", addr);

    axum::serve(listener, app).await.unwrap();
}
