async fn try_llm_providers(
    _client: &reqwest::Client,
    cfg: &crate::llm::config::LlmConfig,
    prompt: &str,
) -> Option<(Value, String)> {
    // Build the provider chain, respecting the configured default_provider.
    //
    // Previously this function:
    // 1. Hardcoded the order openai -> anthropic -> ollama, ignoring
    //    cfg.default_provider (which defaults to "ollama"). A user who set
    //    TDG_LLM_DEFAULT_PROVIDER=ollama to avoid cloud egress would still
    //    hit OpenAI first if any key was present.
    // 2. Used inline call_openai/call_anthropic/call_ollama functions that
    //    swallowed ALL errors with .ok()? — a 401 (bad key) was silently
    //    treated the same as a network error, and the agent wasted 30s
    //    per provider before falling back.
    // 3. call_anthropic didn't extract system messages (Anthropic requires
    //    them top-level, not in the messages array).
    //
    // We now use the trait-based LlmProvider implementations which properly
    // surface errors, extract system messages for Anthropic, and support
    // token usage tracking.
    let request = crate::llm::LlmCompletionRequest {
        messages: vec![crate::llm::LlmMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
        model: None,
        temperature: None,
        max_tokens: None,
    };

    // Determine provider order based on default_provider config
    let order: Vec<&str> = match cfg.default_provider.as_str() {
        "openai" => vec!["openai", "anthropic", "ollama"],
        "anthropic" => vec!["anthropic", "openai", "ollama"],
        "ollama" => vec!["ollama", "openai", "anthropic"],
        _ => vec!["ollama", "openai", "anthropic"],
    };

    for provider_name in &order {
        if !cfg.provider_available(provider_name) {
            continue;
        }

        let provider: Box<dyn crate::llm::LlmProvider> = match *provider_name {
            "openai" => Box::new(crate::llm::openai::OpenAiProvider::new(cfg.openai.clone())),
            "anthropic" => {
                Box::new(crate::llm::anthropic::AnthropicProvider::new(cfg.anthropic.clone()))
            }
            "ollama" => Box::new(crate::llm::ollama::OllamaProvider::new(cfg.ollama.clone())),
            _ => continue,
        };

        match provider.complete(&request).await {
            Ok(response) => {
                tracing::info!(
                    "LLM provider '{}' succeeded (prompt_tokens={}, completion_tokens={})",
                    provider.name(),
                    response.usage.prompt_tokens,
                    response.usage.completion_tokens
                );
                if let Some(parsed) = parse_llm_output(&response.content) {
                    return Some((parsed, provider.name().to_string()));
                } else {
                    tracing::warn!(
                        "LLM provider '{}' returned unparseable output ({} chars)",
                        provider.name(),
                        response.content.len()
                    );
                }
            }
            Err(e) => {
                // Surface the error instead of silently swallowing it.
                // The previous .ok()? pattern made it impossible to distinguish
                // a 401 (bad key) from a network timeout from a malformed response.
                tracing::warn!(
                    "LLM provider '{}' failed: {} — trying next provider",
                    provider.name(),
                    e
                );
            }
        }
    }

    None
}

