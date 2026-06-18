use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::error::TdgError;
use crate::error::TdgResult;

use super::config::OpenAiConfig;
use super::{LlmCompletionRequest, LlmCompletionResponse, LlmProvider, LlmUsage};
#[cfg(test)]
use super::LlmMessage;

/// OpenAI-compatible LLM provider (OpenAI, Azure OpenAI, etc.).
pub struct OpenAiProvider {
    client: Client,
    config: OpenAiConfig,
}

impl OpenAiProvider {
    pub fn new(config: OpenAiConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest Client");

        Self { client, config }
    }

    /// Build the request body for the chat completions endpoint.
    fn build_request_body(&self, request: &LlmCompletionRequest) -> Value {
        let messages: Vec<Value> = request
            .messages
            .iter()
            .map(|msg| {
                json!({
                    "role": msg.role,
                    "content": msg.content,
                })
            })
            .collect();

        json!({
            "model": request.model.as_deref().unwrap_or(&self.config.model),
            "messages": messages,
            "temperature": request.temperature.unwrap_or(self.config.temperature),
            "max_tokens": request.max_tokens.unwrap_or(self.config.max_tokens),
        })
    }

    /// Parse the OpenAI API response into our response type.
    fn parse_response(&self, response_body: &Value) -> TdgResult<LlmCompletionResponse> {
        let choices = response_body
            .get("choices")
            .and_then(|c| c.as_array())
            .ok_or_else(|| {
                TdgError::Custom("OpenAI response missing 'choices' array".to_string())
            })?;

        let first_choice = choices.first().ok_or_else(|| {
            TdgError::Custom("OpenAI response returned zero choices".to_string())
        })?;

        let content = first_choice
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                TdgError::Custom(
                    "OpenAI response missing 'choices[0].message.content'".to_string(),
                )
            })?
            .to_string();

        let model = response_body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = response_body
            .get("usage")
            .map(|u| LlmUsage {
                prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                completion_tokens: u
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
                total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            })
            .unwrap_or_default();

        Ok(LlmCompletionResponse {
            content,
            model,
            usage,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn health_check(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn complete(&self, request: &LlmCompletionRequest) -> TdgResult<LlmCompletionResponse> {
        let api_key = self.config.api_key.as_deref().ok_or_else(|| {
            TdgError::Custom("OpenAI API key is not configured".to_string())
        })?;

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = self.build_request_body(request);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TdgError::Custom(format!("OpenAI request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(TdgError::Custom(format!(
                "OpenAI API error ({}): {}",
                status_code, error_text
            )));
        }

        let response_body: Value = response
            .json()
            .await
            .map_err(|e| TdgError::Custom(format!("Failed to parse OpenAI response: {}", e)))?;

        self.parse_response(&response_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> OpenAiConfig {
        OpenAiConfig {
            api_key: Some("sk-test-key".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }

    #[test]
    fn test_build_request_body_defaults() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            }],
            model: None,
            temperature: None,
            max_tokens: None,
        };

        let body = provider.build_request_body(&request);

        assert_eq!(body["model"], "gpt-4o");
        assert!((body["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello");
    }

    #[test]
    fn test_build_request_body_overrides() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            }],
            model: Some("gpt-4-turbo".to_string()),
            temperature: Some(0.3),
            max_tokens: Some(1024),
        };

        let body = provider.build_request_body(&request);

        assert_eq!(body["model"], "gpt-4-turbo");
        assert!((body["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "You are helpful.");
    }

    #[test]
    fn test_build_request_body_multiple_messages() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![
                LlmMessage {
                    role: "system".to_string(),
                    content: "Be concise.".to_string(),
                },
                LlmMessage {
                    role: "user".to_string(),
                    content: "Tell me a joke.".to_string(),
                },
            ],
            model: None,
            temperature: None,
            max_tokens: None,
        };

        let body = provider.build_request_body(&request);

        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    #[test]
    fn test_parse_response_success() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let response_json = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Hello! How can I help you today?"
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Hello! How can I help you today?");
        assert_eq!(result.model, "gpt-4o");
        assert_eq!(result.usage.prompt_tokens, 10);
        assert_eq!(result.usage.completion_tokens, 8);
        assert_eq!(result.usage.total_tokens, 18);
    }

    #[test]
    fn test_parse_response_usage_defaults() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let response_json = json!({
            "id": "chatcmpl-456",
            "object": "chat.completion",
            "model": "gpt-4o-mini",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Sure!"
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Sure!");
        assert_eq!(result.model, "gpt-4o-mini");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }

    #[test]
    fn test_parse_response_missing_choices() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let response_json = json!({
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "model": "gpt-4o"
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing 'choices' array")
        );
    }

    #[test]
    fn test_parse_response_empty_choices() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let response_json = json!({
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": []
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("returned zero choices")
        );
    }

    #[test]
    fn test_parse_response_missing_content() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);

        let response_json = json!({
            "id": "chatcmpl-789",
            "object": "chat.completion",
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant"
                    },
                    "finish_reason": "stop"
                }
            ]
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("missing 'choices[0].message.content'")
        );
    }

    #[tokio::test]
    async fn test_health_check_returns_true_when_key_set() {
        let config = test_config();
        let provider = OpenAiProvider::new(config);
        assert!(provider.health_check().await);
    }

    #[tokio::test]
    async fn test_health_check_returns_false_when_key_missing() {
        let config = OpenAiConfig {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = OpenAiProvider::new(config);
        assert!(!provider.health_check().await);
    }

    #[tokio::test]
    async fn test_complete_fails_without_api_key() {
        let config = OpenAiConfig {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = OpenAiProvider::new(config);

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
        let provider = OpenAiProvider::new(config);
        assert_eq!(provider.name(), "openai");
    }
}
