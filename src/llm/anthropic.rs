use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::error::TdgError;
use crate::error::TdgResult;
use super::config::AnthropicConfig;
use super::{LlmCompletionRequest, LlmCompletionResponse, LlmProvider, LlmUsage};

/// Anthropic LLM provider using the Messages API.
///
/// Authenticates via `x-api-key` header (not Bearer token).
/// System messages are placed at the top-level `system` field, not in the messages array.
pub struct AnthropicProvider {
    client: Client,
    config: AnthropicConfig,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given config.
    pub fn new(config: AnthropicConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest Client");

        Self { client, config }
    }

    /// Build the JSON request body for the Anthropic Messages API.
    ///
    /// Anthropic places the system prompt at the top level, not in the messages array.
    /// This extracts any message with role "system" and lifts it to the `system` field.
    fn build_request_body(&self, request: &LlmCompletionRequest) -> Value {
        let model = request
            .model
            .as_deref()
            .unwrap_or(&self.config.model);
        let max_tokens = request
            .max_tokens
            .unwrap_or(self.config.max_tokens);
        let temperature = request
            .temperature
            .unwrap_or(self.config.temperature);

        let mut system_message: Option<String> = None;
        let mut api_messages: Vec<Value> = Vec::new();

        for msg in &request.messages {
            if msg.role == "system" {
                system_message = Some(msg.content.clone());
            } else {
                api_messages.push(json!({
                    "role": msg.role,
                    "content": msg.content,
                }));
            }
        }

        // Ensure at least one message exists (Anthropic requires a non-empty messages array)
        if api_messages.is_empty() {
            api_messages.push(json!({
                "role": "user",
                "content": "...",
            }));
        }

        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "messages": api_messages,
        });

        if let Some(system) = system_message {
            body["system"] = json!(system);
        }

        body
    }

    /// Parse an Anthropic API response into our unified response type.
    ///
    /// Anthropic returns content as an array of blocks: `content[0].text`.
    /// Usage fields are `input_tokens` / `output_tokens`.
    fn parse_response(&self, response_body: &Value) -> TdgResult<LlmCompletionResponse> {
        let content = response_body["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .ok_or_else(|| {
                TdgError::Custom(
                    "Anthropic response missing content[0].text".to_string(),
                )
            })?
            .to_string();

        let model = response_body["model"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let input_tokens = response_body["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = response_body["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(LlmCompletionResponse {
            content,
            model,
            usage: LlmUsage {
                prompt_tokens: input_tokens,
                completion_tokens: output_tokens,
                total_tokens: input_tokens + output_tokens,
            },
        })
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn health_check(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn complete(&self, request: &LlmCompletionRequest) -> TdgResult<LlmCompletionResponse> {
        let api_key = self.config.api_key.as_deref().ok_or_else(|| {
            TdgError::Custom("Anthropic API key is not configured".to_string())
        })?;

        let url = format!(
            "{}/messages",
            self.config.base_url.trim_end_matches('/')
        );
        let body = self.build_request_body(request);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .map_err(|e| TdgError::Custom(format!("Anthropic request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(TdgError::Custom(format!(
                "Anthropic API error ({}): {}",
                status_code, error_text
            )));
        }

        let response_body: Value = response
            .json()
            .await
            .map_err(|e| TdgError::Custom(format!("Failed to parse Anthropic response: {}", e)))?;

        self.parse_response(&response_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::LlmMessage;

    fn test_config() -> AnthropicConfig {
        AnthropicConfig {
            api_key: Some("sk-ant-test-key".to_string()),
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }

    #[test]
    fn test_build_request_body_system_message_extracted() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "You are a helpful assistant.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: "Hello!".to_string(),
                },
            ],
            model: None,
            temperature: None,
            max_tokens: None,
        };

        let body = provider.build_request_body(&request);

        // System message should be top-level, not in messages array
        assert_eq!(body["system"], "You are a helpful assistant.");
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 4096);

        // Messages array should contain only the user message
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "Hello!");
    }

    #[test]
    fn test_build_request_body_without_system() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "user".to_string(),
                content: "Tell me a joke.".to_string(),
            }],
            model: Some("claude-opus-4-20250514".to_string()),
            temperature: Some(0.5),
            max_tokens: Some(2048),
        };

        let body = provider.build_request_body(&request);

        // No system field should be present
        assert!(body.get("system").is_none());
        assert_eq!(body["model"], "claude-opus-4-20250514");
        assert_eq!(body["max_tokens"], 2048);
        assert!((body["temperature"].as_f64().unwrap() - 0.5).abs() < f64::EPSILON);

        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
    }

    #[test]
    fn test_build_request_body_system_only_adds_placeholder() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        // Only a system message — no user message
        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "system".to_string(),
                content: "Be concise.".to_string(),
            }],
            model: None,
            temperature: None,
            max_tokens: None,
        };

        let body = provider.build_request_body(&request);

        // System should be top-level
        assert_eq!(body["system"], "Be concise.");
        // Messages should contain a placeholder user message
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "...");
    }

    #[test]
    fn test_parse_response_success() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        let response_json = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Hello! How can I help you today?"}
            ],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 25
            }
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Hello! How can I help you today?");
        assert_eq!(result.model, "claude-sonnet-4-20250514");
        assert_eq!(result.usage.prompt_tokens, 10);
        assert_eq!(result.usage.completion_tokens, 25);
        assert_eq!(result.usage.total_tokens, 35);
    }

    #[test]
    fn test_parse_response_usage_defaults_when_missing() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        let response_json = json!({
            "id": "msg_456",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Sure!"}
            ],
            "model": "claude-sonnet-4-20250514"
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Sure!");
        assert_eq!(result.model, "claude-sonnet-4-20250514");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }

    #[test]
    fn test_parse_response_missing_content() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);

        let response_json = json!({
            "id": "msg_789",
            "type": "message",
            "model": "claude-sonnet-4-20250514",
            "usage": {"input_tokens": 5, "output_tokens": 10}
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("content[0].text")
        );
    }

    #[tokio::test]
    async fn test_health_check_returns_true_when_key_set() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);
        assert!(provider.health_check().await);
    }

    #[tokio::test]
    async fn test_health_check_returns_false_when_key_missing() {
        let config = AnthropicConfig {
            api_key: None,
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = AnthropicProvider::new(config);
        assert!(!provider.health_check().await);
    }

    #[tokio::test]
    async fn test_complete_fails_without_api_key() {
        let config = AnthropicConfig {
            api_key: None,
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = AnthropicProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            model: None,
            temperature: None,
            max_tokens: None,
        };

        let result = provider.complete(&request).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("API key is not configured")
        );
    }

    #[test]
    fn test_name() {
        let config = test_config();
        let provider = AnthropicProvider::new(config);
        assert_eq!(provider.name(), "anthropic");
    }
}
