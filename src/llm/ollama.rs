use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::error::TdgError;
use crate::error::TdgResult;

use super::config::OllamaConfig;
use super::{LlmCompletionRequest, LlmCompletionResponse, LlmProvider, LlmUsage};

/// Ollama LLM provider for local inference via the Ollama API.
///
/// Connects to a local Ollama server at `config.base_url` (default: `http://localhost:11434`).
/// No authentication required (local-only).
pub struct OllamaProvider {
    client: Client,
    config: OllamaConfig,
}

impl OllamaProvider {
    pub fn new(config: OllamaConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest Client");

        Self { client, config }
    }

    /// Build the request body for the `/api/chat` endpoint.
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
            "stream": false,
            "options": {
                "temperature": request.temperature.unwrap_or(self.config.temperature),
                "num_predict": request.max_tokens.unwrap_or(self.config.max_tokens),
            }
        })
    }

    /// Parse the Ollama API response into our response type.
    fn parse_response(&self, response_body: &Value) -> TdgResult<LlmCompletionResponse> {
        let content = response_body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                TdgError::Custom("Ollama response missing 'message.content'".to_string())
            })?
            .to_string();

        let model = response_body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string();

        let prompt_eval_count = response_body
            .get("prompt_eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let eval_count = response_body
            .get("eval_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;

        let usage = LlmUsage {
            prompt_tokens: prompt_eval_count,
            completion_tokens: eval_count,
            total_tokens: prompt_eval_count + eval_count,
        };

        Ok(LlmCompletionResponse {
            content,
            model,
            usage,
        })
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn health_check(&self) -> bool {
        let url = format!("{}/api/tags", self.config.base_url.trim_end_matches('/'));
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    async fn complete(&self, request: &LlmCompletionRequest) -> TdgResult<LlmCompletionResponse> {
        let url = format!("{}/api/chat", self.config.base_url.trim_end_matches('/'));
        let body = self.build_request_body(request);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TdgError::Custom(format!("Ollama request failed: {}", e)))?;

        let status = response.status();
        if !status.is_success() {
            let status_code = status.as_u16();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(TdgError::Custom(format!(
                "Ollama API error ({}): {}",
                status_code, error_text
            )));
        }

        let response_body: Value = response
            .json()
            .await
            .map_err(|e| TdgError::Custom(format!("Failed to parse Ollama response: {}", e)))?;

        self.parse_response(&response_body)
    }
}

#[cfg(test)]
mod tests {
    use super::super::LlmMessage;
    use super::*;

    fn test_config() -> OllamaConfig {
        OllamaConfig {
            base_url: "http://localhost:11434".to_string(),
            model: "llama3".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }

    #[test]
    fn test_build_request_body_defaults() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

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

        assert_eq!(body["model"], "llama3");
        assert!((body["options"]["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert_eq!(body["options"]["num_predict"], 4096);
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Hello");
    }

    #[test]
    fn test_build_request_body_overrides() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

        let request = LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
            }],
            model: Some("llama3.1".to_string()),
            temperature: Some(0.3),
            max_tokens: Some(1024),
        };

        let body = provider.build_request_body(&request);

        assert_eq!(body["model"], "llama3.1");
        assert!((body["options"]["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(body["options"]["num_predict"], 1024);
    }

    #[test]
    fn test_parse_response_success() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

        let response_json = json!({
            "model": "llama3",
            "created_at": "2024-01-15T12:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Hello! How can I help you today?"
            },
            "done": true,
            "prompt_eval_count": 10,
            "eval_count": 8
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Hello! How can I help you today?");
        assert_eq!(result.model, "llama3");
        assert_eq!(result.usage.prompt_tokens, 10);
        assert_eq!(result.usage.completion_tokens, 8);
        assert_eq!(result.usage.total_tokens, 18);
    }

    #[test]
    fn test_parse_response_missing_counts() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

        let response_json = json!({
            "model": "llama3",
            "created_at": "2024-01-15T12:00:00Z",
            "message": {
                "role": "assistant",
                "content": "Sure!"
            },
            "done": true
        });

        let result = provider.parse_response(&response_json).unwrap();

        assert_eq!(result.content, "Sure!");
        assert_eq!(result.model, "llama3");
        assert_eq!(result.usage.prompt_tokens, 0);
        assert_eq!(result.usage.completion_tokens, 0);
        assert_eq!(result.usage.total_tokens, 0);
    }

    #[test]
    fn test_parse_response_missing_message() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

        let response_json = json!({
            "model": "llama3",
            "done": true
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing 'message.content'"));
    }

    #[test]
    fn test_parse_response_missing_content() {
        let config = test_config();
        let provider = OllamaProvider::new(config);

        let response_json = json!({
            "model": "llama3",
            "message": {
                "role": "assistant"
            },
            "done": true
        });

        let result = provider.parse_response(&response_json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing 'message.content'"));
    }

    #[test]
    fn test_name() {
        let config = test_config();
        let provider = OllamaProvider::new(config);
        assert_eq!(provider.name(), "ollama");
    }

    #[tokio::test]
    async fn test_health_check_returns_false_when_no_server() {
        let config = OllamaConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            model: "llama3".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        };
        let provider = OllamaProvider::new(config);
        // Port 1 should not have a server running
        assert!(!provider.health_check().await);
    }
}
