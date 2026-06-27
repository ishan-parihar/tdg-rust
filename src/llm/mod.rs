pub mod anthropic;
pub mod config;
pub mod ollama;
pub mod openai;

use crate::error::TdgResult;
use async_trait::async_trait;

/// LLM message for chat completions
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmMessage {
    pub role: String, // "system", "user", "assistant"
    pub content: String,
}

/// LLM completion request
#[derive(Debug, Clone)]
pub struct LlmCompletionRequest {
    pub messages: Vec<LlmMessage>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

/// LLM completion response
#[derive(Debug, Clone)]
pub struct LlmCompletionResponse {
    pub content: String,
    pub model: String,
    pub usage: LlmUsage,
}

#[derive(Debug, Clone, Default)]
pub struct LlmUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Provider trait — implemented by OpenAI, Anthropic, Ollama
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name for logging
    fn name(&self) -> &str;

    /// Check if provider is available (API key set, service reachable)
    async fn health_check(&self) -> bool;

    /// Generate completion
    async fn complete(&self, request: &LlmCompletionRequest) -> TdgResult<LlmCompletionResponse>;

    /// Generate completion with streaming (optional, default impl blocks)
    async fn complete_stream(&self, request: &LlmCompletionRequest) -> TdgResult<String> {
        // Default: just call complete and return content
        Ok(self.complete(request).await?.content)
    }
}
