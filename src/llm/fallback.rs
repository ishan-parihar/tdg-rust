use async_trait::async_trait;
use tracing::warn;

use crate::error::{TdgError, TdgResult};

use super::{LlmCompletionRequest, LlmCompletionResponse, LlmProvider};

/// Fallback provider that tries multiple LLM providers sequentially.
///
/// Each provider is tried in order; the first successful response is returned.
/// If all providers fail, an error combining all failure messages is returned.
///
/// # Example
///
/// ```ignore
/// let fallback = FallbackProvider::new(vec![
///     Box::new(openai_provider),
///     Box::new(anthropic_provider),
///     Box::new(ollama_provider),
/// ]);
/// ```
pub struct FallbackProvider {
    providers: Vec<Box<dyn LlmProvider>>,
}

impl FallbackProvider {
    /// Create a new fallback provider with the given ordered list of providers.
    ///
    /// Providers are tried in the order they appear in the vector.
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl LlmProvider for FallbackProvider {
    fn name(&self) -> &str {
        "fallback"
    }

    /// Returns `true` if **any** provider reports healthy.
    async fn health_check(&self) -> bool {
        for provider in &self.providers {
            if provider.health_check().await {
                return true;
            }
        }
        false
    }

    /// Try each provider in order, returning the first success.
    ///
    /// On failure, logs the error and proceeds to the next provider.
    /// If all providers fail, returns a `TdgError::Custom` containing all
    /// individual error messages.
    async fn complete(&self, request: &LlmCompletionRequest) -> TdgResult<LlmCompletionResponse> {
        if self.providers.is_empty() {
            return Err(TdgError::Custom(
                "Fallback provider has no providers configured".to_string(),
            ));
        }

        let mut errors: Vec<String> = Vec::with_capacity(self.providers.len());

        for provider in &self.providers {
            match provider.complete(request).await {
                Ok(response) => return Ok(response),
                Err(err) => {
                    let msg = format!("{}: {}", provider.name(), err);
                    warn!("Fallback: provider failed — {}", msg);
                    errors.push(msg);
                }
            }
        }

        Err(TdgError::Custom(format!(
            "All {} fallback providers failed: {}",
            self.providers.len(),
            errors.join(" | ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TdgError;
    use crate::llm::{LlmCompletionRequest, LlmCompletionResponse, LlmMessage, LlmUsage};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock provider that can be configured to succeed or fail.
    struct MockProvider {
        name: &'static str,
        succeed: bool,
        healthy: bool,
        call_count: AtomicUsize,
    }

    impl MockProvider {
        fn new(name: &'static str, succeed: bool, healthy: bool) -> Self {
            Self {
                name,
                succeed,
                healthy,
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            self.name
        }

        async fn health_check(&self) -> bool {
            self.healthy
        }

        async fn complete(
            &self,
            _request: &LlmCompletionRequest,
        ) -> TdgResult<LlmCompletionResponse> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            if self.succeed {
                Ok(LlmCompletionResponse {
                    content: format!("response-from-{}", self.name),
                    model: self.name.to_string(),
                    usage: LlmUsage::default(),
                })
            } else {
                Err(TdgError::Custom(format!("{} error", self.name)))
            }
        }
    }

    fn sample_request() -> LlmCompletionRequest {
        LlmCompletionRequest {
            messages: vec![LlmMessage {
                role: "user".to_string(),
                content: "test".to_string(),
            }],
            model: None,
            temperature: None,
            max_tokens: None,
        }
    }

    #[tokio::test]
    async fn test_first_provider_succeeds() {
        let ok = MockProvider::new("ok", true, true);
        let fail = MockProvider::new("fail", false, true);

        let fallback = FallbackProvider::new(vec![Box::new(ok), Box::new(fail)]);
        let result = fallback.complete(&sample_request()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "response-from-ok");
    }

    #[tokio::test]
    async fn test_first_fails_second_succeeds() {
        let fail = MockProvider::new("fail", false, true);
        let ok = MockProvider::new("ok", true, true);

        let fallback = FallbackProvider::new(vec![Box::new(fail), Box::new(ok)]);
        let result = fallback.complete(&sample_request()).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().content, "response-from-ok");
    }

    #[tokio::test]
    async fn test_all_providers_fail() {
        let a = MockProvider::new("a", false, true);
        let b = MockProvider::new("b", false, true);
        let c = MockProvider::new("c", false, true);

        let fallback = FallbackProvider::new(vec![Box::new(a), Box::new(b), Box::new(c)]);
        let result = fallback.complete(&sample_request()).await;

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("a: a error"),
            "error should mention provider a"
        );
        assert!(
            err.contains("b: b error"),
            "error should mention provider b"
        );
        assert!(
            err.contains("c: c error"),
            "error should mention provider c"
        );
        assert!(
            err.contains("All 3 fallback providers failed"),
            "error should mention count"
        );
    }

    #[tokio::test]
    async fn test_empty_provider_list() {
        let fallback = FallbackProvider::new(vec![]);
        let result = fallback.complete(&sample_request()).await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no providers configured"),
            "empty provider list should produce a clear error"
        );
    }

    #[tokio::test]
    async fn test_health_check_with_mixed_providers() {
        let unhealthy = MockProvider::new("unhealthy", false, false);
        let healthy = MockProvider::new("healthy", false, true);

        let fallback = FallbackProvider::new(vec![Box::new(unhealthy), Box::new(healthy)]);
        assert!(fallback.health_check().await);
    }

    #[tokio::test]
    async fn test_health_check_all_unhealthy() {
        let a = MockProvider::new("a", false, false);
        let b = MockProvider::new("b", false, false);

        let fallback = FallbackProvider::new(vec![Box::new(a), Box::new(b)]);
        assert!(!fallback.health_check().await);
    }

    #[tokio::test]
    async fn test_name_returns_fallback() {
        let provider = MockProvider::new("mock", true, true);
        let fallback = FallbackProvider::new(vec![Box::new(provider)]);
        assert_eq!(fallback.name(), "fallback");
    }
}
