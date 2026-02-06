/// Test program to demonstrate the agent backend
///
/// Run with: cargo run --example test_agent_backend

use fermi::agent_backend::{AgentCard, AgentRegistry, ExecutionContext, MockExecutor, AgentExecutor};
use fermi::ast::{AgentStmt, Program, Schedule, TimeUnit};

fn main() {
    println!("🤖 Fermi Agent Backend Demo\n");

    // Create registry
    let registry = AgentRegistry::new();
    println!("✓ Created agent registry");

    // Create and register a market research agent
    let mut market_agent = AgentCard::new(
        "market_research".to_string(),
        "research".to_string(),
    );
    market_agent.metadata.description =
        "Researches market trends and competitive dynamics".to_string();
    market_agent.metadata.tags = vec!["market".to_string(), "research".to_string()];

    registry.register(market_agent.clone()).unwrap();
    println!("✓ Registered agent: {}", market_agent.agent_id);

    // Create an agent statement (from FPL)
    let agent_stmt = AgentStmt {
        name: "market_research".to_string(),
        agent_type: Some("research".to_string()),
        query: "What is AMD's datacenter market share?".to_string(),
        executor: None,
        schedule: Some(Schedule::Every {
            interval: 1,
            unit: TimeUnit::Week,
        }),
        driver_refs: vec!["market_share".to_string()],
        depends_on: vec![],
        confidence_threshold: Some(0.75),
    };

    println!("\n📝 Agent Query: {}", agent_stmt.query);

    // Create execution context
    let context = ExecutionContext {
        program: Program { statements: vec![] },
        agent_card: market_agent.clone(),
    };

    // Execute agent
    println!("\n⚙️  Executing agent with MockExecutor...");
    let result = registry.execute_agent(&agent_stmt, &context).unwrap();

    println!("\n✅ Execution Results:");
    println!("   Status: {:?}", result.status);
    println!("   Confidence: {:.2}", result.confidence);
    println!("   Execution time: {}ms", result.execution_time_ms);
    println!("   Tokens used: {:?}", result.tokens_used);
    println!("   Sources: {:?}", result.sources_consulted);

    println!("\n📚 Generated Evidence:");
    for (i, evidence) in result.evidence.iter().enumerate() {
        println!("   Evidence {}:", i + 1);
        println!("      ID: {}", evidence.id);
        println!("      Source: {}", evidence.source);
        if let Some(summary) = &evidence.summary {
            println!("      Summary: {}", summary);
        }
        println!("      Key Findings:");
        for finding in &evidence.key_findings {
            println!("         - {}", finding);
        }
    }

    // Record execution
    registry.record_execution("market_research", &result).unwrap();
    println!("\n✓ Recorded execution stats");

    // Get updated agent card
    let updated_card = registry.get("market_research").unwrap();
    println!("\n📊 Updated Agent Stats:");
    println!("   Total executions: {}", updated_card.usage.total_executions);
    println!("   Successful: {}", updated_card.usage.successful_executions);
    println!("   Failed: {}", updated_card.usage.failed_executions);
    println!("   Total tokens: {}", updated_card.usage.total_tokens_used);
    println!("   Total cost: ${:.4}", updated_card.usage.total_cost_usd);
    println!("   Avg execution time: {}ms", updated_card.usage.avg_execution_time_ms);

    // List all agents
    println!("\n📋 Registered Agents:");
    let agents = registry.list().unwrap();
    for agent_id in agents {
        println!("   - {}", agent_id);
    }

    println!("\n✅ Demo complete!");
}
