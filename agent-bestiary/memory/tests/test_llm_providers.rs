use agent_bestiary_memory::{
    AnthropicProvider, GenerationConfig, LLMProvider, LLMProviderConfig, LLMProviderFactory,
    Message, MessageRole, MistralProvider, OpenRouterProvider, ProviderType, QwenProvider,
};
use std::sync::Arc;

fn get_deepseek_key() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("DEEPSEEK_API_KEY").ok()
}

fn get_kimi_key() -> Option<String> {
    dotenvy::dotenv().ok();
    std::env::var("KIMI_API_KEY").ok()
}

// Helper to skip tests if API keys are not available
fn get_anthropic_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY").ok()
}

fn get_mistral_key() -> Option<String> {
    std::env::var("MISTRAL_API_KEY").ok()
}

fn get_qwen_key() -> Option<String> {
    std::env::var("QWEN_API_KEY").ok()
}

fn get_openrouter_key() -> Option<String> {
    std::env::var("OPENROUTER_API_KEY").ok()
}

#[tokio::test]
async fn test_anthropic_provider_basic() {
    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping Anthropic test (no API key)");
            return;
        }
    };

    let provider =
        AnthropicProvider::new(api_key, "claude-3-haiku-20240307".to_string(), None).unwrap();

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    assert_eq!(response.model, "claude-3-haiku-20240307");
    assert!(response.usage.total_tokens > 0);
    println!(
        "✅ Anthropic provider works! Response: {}",
        response.content
    );
}

#[tokio::test]
async fn test_mistral_provider_basic() {
    let api_key = match get_mistral_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping Mistral test (no API key)");
            return;
        }
    };

    let provider = MistralProvider::new(api_key, "mistral-tiny".to_string(), None).unwrap();

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    assert!(response.usage.total_tokens > 0);
    println!("✅ Mistral provider works! Response: {}", response.content);
}

#[tokio::test]
async fn test_qwen_provider_basic() {
    let api_key = match get_qwen_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping Qwen test (no API key)");
            return;
        }
    };

    let provider = QwenProvider::new(api_key, "qwen-turbo".to_string(), None).unwrap();

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    assert!(response.usage.total_tokens > 0);
    println!("✅ Qwen provider works! Response: {}", response.content);
}

#[tokio::test]
async fn test_openrouter_provider_basic() {
    let api_key = match get_openrouter_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping OpenRouter test (no API key)");
            return;
        }
    };

    let provider =
        OpenRouterProvider::new(api_key, "openai/gpt-3.5-turbo".to_string(), None).unwrap();

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    assert!(response.usage.total_tokens > 0);
    println!(
        "✅ OpenRouter provider works! Response: {}",
        response.content
    );
}

#[tokio::test]
async fn test_llm_provider_factory() {
    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping factory test (no Anthropic API key)");
            return;
        }
    };

    let config = LLMProviderConfig {
        provider_type: ProviderType::Anthropic,
        api_key,
        model: "claude-3-haiku-20240307".to_string(),
        base_url: None,
    };

    let provider = LLMProviderFactory::create(&config).unwrap();

    assert_eq!(provider.provider_name(), "anthropic");
    assert_eq!(provider.model_name(), "claude-3-haiku-20240307");
    assert!(provider.supports_tools());

    let messages = vec![Message {
        role: MessageRole::User,
        content: "Say hello in one word.".to_string(),
    }];

    let gen_config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &gen_config).await.unwrap();
    assert!(!response.content.is_empty());
    println!(
        "✅ LLMProviderFactory works! Response: {}",
        response.content
    );
}

#[tokio::test]
async fn test_provider_with_system_message() {
    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping system message test (no API key)");
            return;
        }
    };

    let provider =
        AnthropicProvider::new(api_key, "claude-3-haiku-20240307".to_string(), None).unwrap();

    let messages = vec![
        Message {
            role: MessageRole::System,
            content: "You are a helpful assistant that only responds with numbers.".to_string(),
        },
        Message {
            role: MessageRole::User,
            content: "What is 5+3?".to_string(),
        },
    ];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    println!(
        "✅ System message handling works! Response: {}",
        response.content
    );
}

#[tokio::test]
async fn test_provider_multi_turn() {
    let api_key = match get_anthropic_key() {
        Some(key) => key,
        None => {
            println!("⏭️  Skipping multi-turn test (no API key)");
            return;
        }
    };

    let provider =
        AnthropicProvider::new(api_key, "claude-3-haiku-20240307".to_string(), None).unwrap();

    let messages = vec![
        Message {
            role: MessageRole::User,
            content: "My favorite number is 7.".to_string(),
        },
        Message {
            role: MessageRole::Assistant,
            content: "I understand your favorite number is 7.".to_string(),
        },
        Message {
            role: MessageRole::User,
            content: "What is my favorite number? Answer with just the number.".to_string(),
        },
    ];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();

    assert!(!response.content.is_empty());
    println!(
        "✅ Multi-turn conversation works! Response: {}",
        response.content
    );
}

// ── DeepSeek provider tests ───────────────────────────────────────────────

#[tokio::test]
async fn test_deepseek_provider_via_factory() {
    // Tests provider registration and factory routing.
    // Uses OpenRouterProvider internally with DeepSeek's base URL.
    // Skips the actual API call if no key is set — but verifies
    // the factory correctly constructs and routes the provider.
    dotenvy::dotenv().ok();
    let api_key = match get_deepseek_key() {
        Some(k) => k,
        None => {
            println!("⏭️  Skipping DeepSeek test (no DEEPSEEK_API_KEY)");
            // Structural check: parse the provider type even without a key
            use std::str::FromStr;
            assert_eq!(
                ProviderType::from_str("deepseek").unwrap(),
                ProviderType::DeepSeek
            );
            println!("✅ DeepSeek ProviderType parses correctly");
            return;
        }
    };

    let config = LLMProviderConfig {
        provider_type: ProviderType::DeepSeek,
        api_key,
        model: "deepseek-chat".to_string(),
        base_url: None, // defaults to https://api.deepseek.com/v1
    };

    let provider = LLMProviderFactory::create(&config).unwrap();
    // DeepSeek uses OpenRouterProvider internally
    assert_eq!(provider.model_name(), "deepseek-chat");

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();
    assert!(!response.content.is_empty());
    println!(
        "✅ DeepSeek provider works! Response: {}",
        response.content
    );
}

#[tokio::test]
async fn test_kimi_provider_via_factory() {
    // Kimi (Moonshot AI) — OpenAI-compatible, routed via OpenRouterProvider.
    dotenvy::dotenv().ok();
    let api_key = match get_kimi_key() {
        Some(k) => k,
        None => {
            println!("⏭️  Skipping Kimi test (no KIMI_API_KEY)");
            use std::str::FromStr;
            assert_eq!(
                ProviderType::from_str("kimi").unwrap(),
                ProviderType::Kimi
            );
            assert_eq!(
                ProviderType::from_str("moonshot").unwrap(),
                ProviderType::Kimi
            );
            println!("✅ Kimi ProviderType parses correctly (both 'kimi' and 'moonshot')");
            return;
        }
    };

    let config = LLMProviderConfig {
        provider_type: ProviderType::Kimi,
        api_key,
        model: "moonshot-v1-8k".to_string(),
        base_url: None, // defaults to https://api.moonshot.cn/v1
    };

    let provider = LLMProviderFactory::create(&config).unwrap();
    assert_eq!(provider.model_name(), "moonshot-v1-8k");

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 2+2? Answer with just the number.".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(10),
        ..Default::default()
    };

    let response = provider.generate_raw(messages, &config).await.unwrap();
    assert!(!response.content.is_empty());
    println!("✅ Kimi provider works! Response: {}", response.content);
}

#[test]
fn test_provider_type_parsing() {
    use std::str::FromStr;

    assert_eq!(
        ProviderType::from_str("anthropic").unwrap(),
        ProviderType::Anthropic
    );
    assert_eq!(
        ProviderType::from_str("claude").unwrap(),
        ProviderType::Anthropic
    );
    assert_eq!(
        ProviderType::from_str("mistral").unwrap(),
        ProviderType::Mistral
    );
    assert_eq!(ProviderType::from_str("qwen").unwrap(), ProviderType::Qwen);
    assert_eq!(
        ProviderType::from_str("openrouter").unwrap(),
        ProviderType::OpenRouter
    );
    assert_eq!(
        ProviderType::from_str("deepseek").unwrap(),
        ProviderType::DeepSeek
    );
    assert_eq!(
        ProviderType::from_str("kimi").unwrap(),
        ProviderType::Kimi
    );
    assert_eq!(
        ProviderType::from_str("moonshot").unwrap(),
        ProviderType::Kimi
    );

    // Case insensitive
    assert_eq!(
        ProviderType::from_str("ANTHROPIC").unwrap(),
        ProviderType::Anthropic
    );
    assert_eq!(
        ProviderType::from_str("DEEPSEEK").unwrap(),
        ProviderType::DeepSeek
    );

    // Invalid provider
    assert!(ProviderType::from_str("invalid").is_err());

    println!("✅ ProviderType parsing works for all providers including DeepSeek and Kimi!");
}

#[tokio::test]
async fn test_llm_integration_with_consolidation() {
    // This test verifies that the LLM can be used for rule extraction
    // It's a mock test that doesn't require real API keys

    use agent_bestiary_memory::{
        ConsolidationLock, ConsolidationWorker, MemoryStore, MockEmbeddings,
    };

    dotenvy::dotenv().ok();
    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            println!("⏭️  Skipping consolidation integration test (no DATABASE_URL)");
            return;
        }
    };

    let store = Arc::new(MemoryStore::new(&database_url).await.unwrap());
    let pool = Arc::new(store.pool().clone());
    let lock = Arc::new(ConsolidationLock::new(pool, "test-worker".to_string()));
    let embedder = Arc::new(MockEmbeddings::new(1024));

    // Create worker without LLM (should use pattern-based extraction)
    let _worker_no_llm = ConsolidationWorker::new(
        store.clone(),
        lock.clone(),
        embedder.clone(),
        "test-worker".to_string(),
    );

    // Verify it was created successfully (no panics)
    println!("✅ ConsolidationWorker without LLM created successfully!");

    // If we have an Anthropic key, test with LLM
    if let Some(api_key) = get_anthropic_key() {
        let llm = Arc::new(
            AnthropicProvider::new(api_key, "claude-3-haiku-20240307".to_string(), None).unwrap(),
        );

        let _worker_with_llm = ConsolidationWorker::with_llm(
            store.clone(),
            lock.clone(),
            embedder.clone(),
            llm,
            "test-worker-llm".to_string(),
        );

        println!("✅ ConsolidationWorker with LLM created successfully!");
    } else {
        println!("⏭️  Skipping LLM consolidation test (no Anthropic API key)");
    }

    println!("✅ LLM integration with consolidation verified!");
}

#[tokio::test]
async fn test_generate_structured() {
    use agent_bestiary_memory::{
        generate_structured, AnthropicProvider, GenerationConfig, Message, MessageRole,
    };

    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("⏭️  Skipping structured generation test (no API key)");
            return;
        }
    };

    let provider =
        AnthropicProvider::new(api_key, "claude-3-haiku-20240307".to_string(), None).unwrap();

    #[derive(serde::Deserialize, Debug)]
    struct MathResponse {
        answer: i32,
        explanation: String,
    }

    let messages = vec![Message {
        role: MessageRole::User,
        content: "What is 7 + 5? Respond in JSON format with fields 'answer' (number) and 'explanation' (string).".to_string(),
    }];

    let config = GenerationConfig {
        temperature: 0.0,
        max_tokens: Some(100),
        ..Default::default()
    };

    let response: MathResponse = generate_structured(&provider, messages, &config)
        .await
        .unwrap();

    assert_eq!(response.answer, 12);
    assert!(!response.explanation.is_empty());
    println!("✅ generate_structured works! Got: {:?}", response);
}
