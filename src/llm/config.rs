use serde::{Deserialize, Serialize};

/// LLM provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default provider: "openai", "anthropic", "ollama"
    pub default_provider: String,
    /// OpenAI config
    pub openai: OpenAiConfig,
    /// Anthropic config
    pub anthropic: AnthropicConfig,
    /// Ollama config
    pub ollama: OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl LlmConfig {
    /// Build LLM config from `TDG_LLM_*` environment variables with sensible defaults.
    ///
    /// Default provider is `ollama` (local, no API key required).
    /// Env vars:
    /// - `TDG_LLM_DEFAULT_PROVIDER` — "openai", "anthropic", or "ollama" (default: "ollama")
    /// - `TDG_LLM_OPENAI_API_KEY` — OpenAI API key
    /// - `TDG_LLM_OPENAI_BASE_URL` — OpenAI base URL (default: "https://api.openai.com/v1")
    /// - `TDG_LLM_OPENAI_MODEL` — OpenAI model (default: "gpt-4o")
    /// - `TDG_LLM_OPENAI_MAX_TOKENS` — max tokens (default: 4096)
    /// - `TDG_LLM_OPENAI_TEMPERATURE` — temperature (default: 0.7)
    /// - `TDG_LLM_ANTHROPIC_API_KEY` — Anthropic API key
    /// - `TDG_LLM_ANTHROPIC_BASE_URL` — Anthropic base URL (default: "https://api.anthropic.com/v1")
    /// - `TDG_LLM_ANTHROPIC_MODEL` — Anthropic model (default: "claude-sonnet-4-20250514")
    /// - `TDG_LLM_ANTHROPIC_MAX_TOKENS` — max tokens (default: 4096)
    /// - `TDG_LLM_ANTHROPIC_TEMPERATURE` — temperature (default: 0.7)
    /// - `TDG_LLM_OLLAMA_URL` — Ollama server URL (default: "http://localhost:11434")
    /// - `TDG_LLM_OLLAMA_MODEL` — Ollama model (default: "llama3")
    /// - `TDG_LLM_OLLAMA_MAX_TOKENS` — max tokens (default: 4096)
    /// - `TDG_LLM_OLLAMA_TEMPERATURE` — temperature (default: 0.7)
    pub fn from_env() -> Self {
        let default_provider =
            std::env::var("TDG_LLM_DEFAULT_PROVIDER").unwrap_or_else(|_| "ollama".to_string());

        let openai = OpenAiConfig {
            api_key: std::env::var("TDG_LLM_OPENAI_API_KEY").ok(),
            base_url: std::env::var("TDG_LLM_OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            model: std::env::var("TDG_LLM_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string()),
            max_tokens: parse_env_u32("TDG_LLM_OPENAI_MAX_TOKENS", 4096),
            temperature: parse_env_f32("TDG_LLM_OPENAI_TEMPERATURE", 0.7),
        };

        let anthropic = AnthropicConfig {
            api_key: std::env::var("TDG_LLM_ANTHROPIC_API_KEY").ok(),
            base_url: std::env::var("TDG_LLM_ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string()),
            model: std::env::var("TDG_LLM_ANTHROPIC_MODEL")
                .unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string()),
            max_tokens: parse_env_u32("TDG_LLM_ANTHROPIC_MAX_TOKENS", 4096),
            temperature: parse_env_f32("TDG_LLM_ANTHROPIC_TEMPERATURE", 0.7),
        };

        let ollama = OllamaConfig {
            base_url: std::env::var("TDG_LLM_OLLAMA_URL")
                .unwrap_or_else(|_| "http://localhost:11434".to_string()),
            model: std::env::var("TDG_LLM_OLLAMA_MODEL").unwrap_or_else(|_| "llama3".to_string()),
            max_tokens: parse_env_u32("TDG_LLM_OLLAMA_MAX_TOKENS", 4096),
            temperature: parse_env_f32("TDG_LLM_OLLAMA_TEMPERATURE", 0.7),
        };

        Self {
            default_provider,
            openai,
            anthropic,
            ollama,
        }
    }

    /// Check if a provider has its API key configured (or doesn't need one).
    ///
    /// - `"openai"`: requires `openai.api_key` to be `Some`
    /// - `"anthropic"`: requires `anthropic.api_key` to be `Some`
    /// - `"ollama"`: always available (local, no key needed)
    pub fn provider_available(&self, provider: &str) -> bool {
        match provider {
            "openai" => self.openai.api_key.is_some(),
            "anthropic" => self.anthropic.api_key.is_some(),
            "ollama" => true,
            _ => false,
        }
    }
}

/// Parse a u32 from an env var, falling back to a default.
fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

/// Parse an f32 from an env var, falling back to a default.
fn parse_env_f32(key: &str, default: f32) -> f32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_ollama() {
        let cfg = LlmConfig::from_env();
        assert_eq!(cfg.default_provider, "ollama");
        assert_eq!(cfg.ollama.model, "llama3");
        assert_eq!(cfg.ollama.base_url, "http://localhost:11434");
    }

    #[test]
    fn default_openai_model() {
        let cfg = LlmConfig::from_env();
        assert_eq!(cfg.openai.model, "gpt-4o");
    }

    #[test]
    fn default_anthropic_model() {
        let cfg = LlmConfig::from_env();
        assert_eq!(cfg.anthropic.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn provider_availability() {
        let cfg = LlmConfig::from_env();
        // ollama always available
        assert!(cfg.provider_available("ollama"));
        // openai/anthropic only if key is set
        assert_eq!(
            cfg.provider_available("openai"),
            cfg.openai.api_key.is_some()
        );
        assert_eq!(
            cfg.provider_available("anthropic"),
            cfg.anthropic.api_key.is_some()
        );
        // unknown provider
        assert!(!cfg.provider_available("nonexistent"));
    }

    #[test]
    fn default_token_and_temp() {
        let cfg = LlmConfig::from_env();
        assert_eq!(cfg.openai.max_tokens, 4096);
        assert!((cfg.openai.temperature - 0.7).abs() < f32::EPSILON);
    }
}
