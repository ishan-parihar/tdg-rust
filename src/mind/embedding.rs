//! ONNX-based text embedding engine.
//!
//! Port of `core/mind/embedding_engine.py` — uses ONNX Runtime for text-to-vector
//! inference with HuggingFace tokenizers. Produces 384-dim float32 vectors via
//! mean pooling + L2 normalization.
//!
//! # Feature Gate
//!
//! This module requires the `onnx` feature flag. When disabled, only stub types
//! and constants are available, allowing the rest of the crate to compile without
//! ONNX Runtime.

use std::path::{Path, PathBuf};
#[cfg(feature = "onnx")]
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────

/// Default embedding dimension (all-MiniLM-L6-v2).
pub const DEFAULT_EMBEDDING_DIM: usize = 384;

/// Maximum sequence length for tokenizer.
pub const MAX_SEQUENCE_LENGTH: usize = 256;

/// Default padding token ID.
pub const DEFAULT_PAD_ID: i32 = 0;

// ── Public Types ──────────────────────────────────────────────────────

/// Configuration for the embedding engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Path to the ONNX model file.
    pub model_path: PathBuf,
    /// Path to the tokenizer JSON file.
    pub tokenizer_path: PathBuf,
    /// Embedding dimension (default: 384).
    pub embedding_dim: usize,
    /// Maximum sequence length (default: 256).
    pub max_length: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            tokenizer_path: PathBuf::new(),
            embedding_dim: DEFAULT_EMBEDDING_DIM,
            max_length: MAX_SEQUENCE_LENGTH,
        }
    }
}

/// A single embedding result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    /// The embedding vector.
    pub vector: Vec<f32>,
    /// Token count used for this embedding.
    pub token_count: usize,
}

// ── Feature-gated ONNX Implementation ────────────────────────────────

#[cfg(feature = "onnx")]
mod onnx_impl {
    use super::*;
    use anyhow::{Context, Result};
    use ndarray::{Array1, Array2, Axis};
    use ort::session::Session;
    use tokenizers::Tokenizer;

    /// Global model cache — loaded once, reused across calls.
    static MODEL_CACHE: OnceLock<Arc<Mutex<Option<CachedModel>>>> = OnceLock::new();

    /// Cached ONNX session + tokenizer pair.
    struct CachedModel {
        session: Session,
        tokenizer: Tokenizer,
        config: EmbeddingConfig,
    }

    /// Get or initialize the global model cache.
    fn get_cache() -> &'static Arc<Mutex<Option<CachedModel>>> {
        MODEL_CACHE.get_or_init(|| Arc::new(Mutex::new(None)))
    }

    /// Load configuration from `config/embeddings.json`.
    ///
    /// Resolves `{REPO_ROOT}` placeholders using the crate's repo root.
    pub fn load_config(config_path: &Path) -> Result<EmbeddingConfig> {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("Failed to read embeddings config: {}", config_path.display()))?;
        let repo_root = crate::config::Config::repo_root();
        let resolved = content.replace("{REPO_ROOT}", &repo_root.to_string_lossy());
        let config: EmbeddingConfig =
            serde_json::from_str(&resolved).context("Failed to parse embeddings config")?;
        Ok(config)
    }

    /// Initialize the embedding engine with an explicit config.
    ///
    /// Loads the ONNX model and tokenizer, caching them globally.
    pub fn init(config: EmbeddingConfig) -> Result<()> {
        let mut cache = get_cache().lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;

        // Already loaded with same config?
        if let Some(ref cached) = *cache {
            if cached.config.model_path == config.model_path {
                return Ok(());
            }
        }

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        // Load ONNX session
        let session = Session::builder()?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&config.model_path)
            .context("Failed to load ONNX model")?;

        *cache = Some(CachedModel {
            session,
            tokenizer,
            config,
        });
        Ok(())
    }

    /// Embed a single text string → 384-dim float32 vector.
    ///
    /// Steps: tokenize → ONNX inference → mean pooling → L2 normalize.
    pub fn embed(text: &str) -> Result<EmbeddingResult> {
        let mut cache = get_cache().lock().map_err(|e| anyhow::anyhow!("Lock poisoned: {e}"))?;
        let cached = cache
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Embedding engine not initialized. Call init() first."))?;

        // Configure tokenizer for this encoding
        let mut tok = cached.tokenizer.clone();
        tok.enable_truncation(cached.config.max_length)
            .map_err(|e| anyhow::anyhow!("Truncation config failed: {e}"))?;
        tok.enable_padding(
            tokenizers::PaddingParams::default()
                .with_length(cached.config.max_length)
                .with_pad_id(DEFAULT_PAD_ID as u32)
                .with_strategy(tokenizers::PaddingStrategy::Fixed),
        )
        .map_err(|e| anyhow::anyhow!("Padding config failed: {e}"))?;

        // Tokenize
        let encoding = tok
            .encode(text, true)
            .map_err(|e| anyhow::anyhow!("Tokenization failed: {e}"))?;

        let seq_len = encoding.len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> =
            encoding.get_attention_mask().iter().map(|&m| m as i64).collect();
        let token_type_ids: Vec<i64> =
            encoding.get_type_ids().iter().map(|&t| t as i64).collect();

        // Build tensors: shape [1, seq_len]
        let input_ids_tensor = Array2::from_shape_vec((1, seq_len), input_ids)
            .map_err(|e| anyhow::anyhow!("input_ids tensor error: {e}"))?;
        let attention_mask_tensor = Array2::from_shape_vec((1, seq_len), attention_mask)
            .map_err(|e| anyhow::anyhow!("attention_mask tensor error: {e}"))?;
        let token_type_ids_tensor = Array2::from_shape_vec((1, seq_len), token_type_ids)
            .map_err(|e| anyhow::anyhow!("token_type_ids tensor error: {e}"))?;

        // Run ONNX inference
        let outputs = cached.session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])?;

        // Extract token embeddings: shape [1, seq_len, hidden_dim]
        let token_embeddings = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .context("Failed to extract last_hidden_state")?;

        // Mean pooling: average over non-padding tokens
        let hidden_dim = token_embeddings.shape()[2];
        let mask_slice = attention_mask_tensor
            .index_axis(ndarray::Axis(0), 0)
            .to_owned();

        let mut pooled = Array1::<f32>::zeros(hidden_dim);
        let mut mask_sum: f32 = 0.0;

        for t in 0..seq_len {
            let m = mask_slice[t];
            if m > 0 {
                mask_sum += m as f32;
                for d in 0..hidden_dim {
                    pooled[d] += token_embeddings[[0, t, d]] * m as f32;
                }
            }
        }

        if mask_sum > 0.0 {
            pooled.mapv_inplace(|v| v / mask_sum);
        }

        // L2 normalize
        let norm: f32 = pooled.mapv(|v| v * v).sum().sqrt();
        if norm > 1e-12 {
            pooled.mapv_inplace(|v| v / norm);
        }

        Ok(EmbeddingResult {
            vector: pooled.to_vec(),
            token_count: seq_len,
        })
    }

    /// Embed a batch of texts → list of 384-dim vectors.
    pub fn embed_batch(texts: &[&str]) -> Result<Vec<EmbeddingResult>> {
        texts.iter().map(|t| embed(t)).collect()
    }

    /// Compute cosine similarity between two vectors.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a < 1e-12 || norm_b < 1e-12 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// Reset the global model cache (for testing).
    pub fn reset() {
        if let Ok(mut cache) = get_cache().lock() {
            *cache = None;
        }
    }
}

// ── Feature-disabled stubs ────────────────────────────────────────────

#[cfg(not(feature = "onnx"))]
pub mod onnx_impl {
    use super::*;

    /// Stub: returns error when ONNX feature is disabled.
    pub fn load_config(_config_path: &Path) -> anyhow::Result<EmbeddingConfig> {
        anyhow::bail!("ONNX feature not enabled. Enable with `--features onnx`.")
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn init(_config: EmbeddingConfig) -> anyhow::Result<()> {
        anyhow::bail!("ONNX feature not enabled. Enable with `--features onnx`.")
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn embed(_text: &str) -> anyhow::Result<EmbeddingResult> {
        anyhow::bail!("ONNX feature not enabled. Enable with `--features onnx`.")
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn embed_batch(_texts: &[&str]) -> anyhow::Result<Vec<EmbeddingResult>> {
        anyhow::bail!("ONNX feature not enabled. Enable with `--features onnx`.")
    }

    /// Cosine similarity between two vectors (works without ONNX).
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a < 1e-12 || norm_b < 1e-12 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    /// Stub: no-op when ONNX feature is disabled.
    pub fn reset() {}
}

// Re-export onnx_impl functions at module level for convenience.
#[allow(unused_imports)]
pub use onnx_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_basic() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn embedding_config_default() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.embedding_dim, 384);
        assert_eq!(config.max_length, 256);
    }

    #[test]
    fn embedding_result_serialization() {
        let result = EmbeddingResult {
            vector: vec![0.1, 0.2, 0.3],
            token_count: 5,
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: EmbeddingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.vector, result.vector);
        assert_eq!(deserialized.token_count, result.token_count);
    }

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn stub_embed_returns_error() {
        let result = embed("hello world");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("ONNX feature not enabled"));
    }

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn stub_init_returns_error() {
        let config = EmbeddingConfig::default();
        let result = init(config);
        assert!(result.is_err());
    }
}
