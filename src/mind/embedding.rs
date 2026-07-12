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

#[cfg(feature = "onnx")]
use crate::error::{TdgError, TdgResult};
use std::path::{Path, PathBuf};
#[cfg(feature = "onnx")]
use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────

pub const DEFAULT_EMBEDDING_DIM: usize = 384;
pub const GEMMA_EMBEDDING_DIM: usize = 768;
pub const MAX_SEQUENCE_LENGTH: usize = 256;
pub const DEFAULT_PAD_ID: i32 = 0;

#[cfg(feature = "onnx")]
pub const MINILM_MODEL_DIR: &str = "all-MiniLM-L6-v2";
#[cfg(feature = "onnx")]
pub const MINILM_ONNX_FILE: &str = "model_quantized.onnx";
#[cfg(feature = "onnx")]
pub const GEMMA_MODEL_DIR: &str = "embeddinggemma-300m";
#[cfg(feature = "onnx")]
pub const GEMMA_Q4_FILE: &str = "embeddinggemma-300m-Q4_0.onnx";
#[cfg(feature = "onnx")]
pub const GEMMA_Q8_FILE: &str = "embeddinggemma-300m-Q8_0.onnx";


#[cfg(feature = "onnx")]
const MINILM_REPO_URL: &str =
    "https://huggingface.co/xenova/all-MiniLM-L6-v2/resolve/main/onnx/model_quantized.onnx";

#[cfg(feature = "onnx")]
const Q4_DOWNLOAD_URL: &str =
    "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/onnx/model_q4.onnx";
#[cfg(feature = "onnx")]
const Q8_DOWNLOAD_URL: &str =
    "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/onnx/model_quantized.onnx";

// ── Public Types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub embedding_dim: usize,
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

#[cfg(feature = "onnx")]
impl EmbeddingConfig {
    /// Create config from the application's embedding section.
    pub fn from_app_config(config: &crate::config::Config) -> Self {
        let model_dir = config.models_dir().join(config.embedding.model_dir_name());
        let onnx_file = config.embedding.onnx_filename();

        Self {
            model_path: model_dir.join(onnx_file),
            tokenizer_path: model_dir.join("tokenizer.json"),
            embedding_dim: config.embedding.effective_dimension(),
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
    use crate::error::{TdgError, TdgResult};
    use ort::session::Session;
    use ort::value::Tensor;
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
    pub fn load_config(config_path: &Path) -> TdgResult<EmbeddingConfig> {
        let content = std::fs::read_to_string(config_path).map_err(|e| {
            TdgError::Custom(format!(
                "Failed to read embeddings config {}: {e}",
                config_path.display()
            ))
        })?;
        let repo_root = crate::config::Config::repo_root();
        let resolved = content.replace("{REPO_ROOT}", &repo_root.to_string_lossy());
        let config: EmbeddingConfig = serde_json::from_str(&resolved)
            .map_err(|e| TdgError::Custom(format!("Failed to parse embeddings config: {e}")))?;
        Ok(config)
    }

    /// Initialize the embedding engine with an explicit config.
    ///
    /// Loads the ONNX model and tokenizer, caching them globally.
    pub fn init(config: EmbeddingConfig) -> TdgResult<()> {
        let mut cache = get_cache()
            .lock()
            .map_err(|e| TdgError::Custom(format!("Lock poisoned: {e}")))?;

        // Already loaded with same config?
        if let Some(ref cached) = *cache {
            if cached.config.model_path == config.model_path {
                return Ok(());
            }
        }

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| TdgError::Custom(format!("Failed to load tokenizer: {e}")))?;

        // Load ONNX session (default optimization = All; Level3/layout not in ORT 1.20.1)
        let session = Session::builder()
            .map_err(|e| TdgError::Custom(format!("ONNX session builder: {e}")))?
            .with_intra_threads(4)
            .map_err(|e| TdgError::Custom(format!("ONNX thread config: {e}")))?
            .commit_from_file(&config.model_path)
            .map_err(|e| TdgError::Custom(format!("Failed to load ONNX model: {e}")))?;

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
    pub fn embed(text: &str) -> TdgResult<EmbeddingResult> {
        let mut cache = get_cache()
            .lock()
            .map_err(|e| TdgError::Custom(format!("Lock poisoned: {e}")))?;

        if cache.is_none() {
            let config = crate::config::Config::from_env();
            let emb_config = EmbeddingConfig::from_app_config(&config);
            if let Err(e) = ensure_model_files(&config) {
                return Err(TdgError::Custom(format!(
                    "Embedding engine auto-init failed to ensure model files: {e}"
                )));
            }
            let tokenizer = Tokenizer::from_file(&emb_config.tokenizer_path)
                .map_err(|e| TdgError::Custom(format!("Failed to load tokenizer: {e}")))?;
            let session = Session::builder()
                .map_err(|e| TdgError::Custom(format!("ONNX session builder: {e}")))?
                .with_intra_threads(4)
                .map_err(|e| TdgError::Custom(format!("ONNX thread config: {e}")))?
                .commit_from_file(&emb_config.model_path)
                .map_err(|e| TdgError::Custom(format!("Failed to load ONNX model: {e}")))?;

            *cache = Some(CachedModel {
                session,
                tokenizer,
                config: emb_config,
            });
        }

        let cached = cache.as_mut().unwrap();

        // Configure tokenizer for this encoding
        let mut tok = cached.tokenizer.clone();
        tok.with_truncation(Some(tokenizers::TruncationParams {
            max_length: cached.config.max_length,
            ..Default::default()
        }))
        .map_err(|e| TdgError::Custom(format!("Truncation config failed: {e}")))?;
        tok.with_padding(Some(tokenizers::PaddingParams {
            strategy: tokenizers::PaddingStrategy::Fixed(cached.config.max_length),
            pad_id: 0,
            pad_type_id: 0,
            pad_token: "[PAD]".to_string(),
            pad_to_multiple_of: None,
            direction: tokenizers::PaddingDirection::Right,
        }));
        let encoding = tok
            .encode(text, true)
            .map_err(|e| TdgError::Custom(format!("Tokenization failed: {e}")))?;
        let seq_len = encoding.get_ids().len();
        let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let attention_mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&m| m as i64)
            .collect();
        // Keep a copy for mean pooling (tensor consumes the original)
        let mask_for_pooling = attention_mask.clone();

        // Build tensors: shape [1, seq_len]
        let input_ids_tensor = Tensor::from_array(([1, seq_len], input_ids))
            .map_err(|e| TdgError::Custom(format!("input_ids tensor error: {e}")))?;
        let attention_mask_tensor = Tensor::from_array(([1, seq_len], attention_mask))
            .map_err(|e| TdgError::Custom(format!("attention_mask tensor error: {e}")))?;

        // Run ONNX inference
        let outputs = if cached.config.embedding_dim == 384 {
            let token_type_ids = vec![0i64; seq_len];
            let token_type_ids_tensor = Tensor::from_array(([1, seq_len], token_type_ids))
                .map_err(|e| TdgError::Custom(format!("token_type_ids tensor error: {e}")))?;
            let inputs = ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
                "token_type_ids" => token_type_ids_tensor,
            ];
            cached
                .session
                .run(inputs)
                .map_err(|e| TdgError::Custom(format!("ONNX inference failed: {e}")))?
        } else {
            let inputs = ort::inputs![
                "input_ids" => input_ids_tensor,
                "attention_mask" => attention_mask_tensor,
            ];
            cached
                .session
                .run(inputs)
                .map_err(|e| TdgError::Custom(format!("ONNX inference failed: {e}")))?
        };

        // Extract token embeddings: shape [1, seq_len, hidden_dim]
        let (shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| TdgError::Custom(format!("Failed to extract last_hidden_state: {e}")))?;

        let seq_len = shape[1] as usize;
        let hidden_dim = shape[2] as usize;

        // Mean pooling: average over non-padding tokens
        let mut pooled = vec![0.0f32; hidden_dim];
        let mut mask_sum: f32 = 0.0;

        for t in 0..seq_len {
            let m = mask_for_pooling[t] as f32;
            if m > 0.0 {
                mask_sum += m;
                let offset = t * hidden_dim;
                for d in 0..hidden_dim {
                    pooled[d] += data[offset + d] * m;
                }
            }
        }

        if mask_sum > 0.0 {
            for v in &mut pooled {
                *v /= mask_sum;
            }
        }

        // L2 normalize
        let norm: f32 = pooled.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 1e-12 {
            for v in &mut pooled {
                *v /= norm;
            }
        }

        Ok(EmbeddingResult {
            vector: pooled,
            token_count: seq_len,
        })
    }

    /// Embed a batch of texts → list of 384-dim vectors.
    pub fn embed_batch(texts: &[&str]) -> TdgResult<Vec<EmbeddingResult>> {
        texts.iter().map(|t| embed(t)).collect()
    }

    /// Reset the global model cache (for testing).
    pub fn reset() {
        if let Ok(mut cache) = get_cache().lock() {
            *cache = None;
        }
    }
}

pub use crate::util::math::cosine_similarity;

// ── Feature-gated GGUF Implementation ────────────────────────────────

/// GGUF embedding dimension (EmbeddingGemma-300M).
pub const GGUF_EMBEDDING_DIM: usize = 768;

#[cfg(feature = "gguf")]
pub mod gguf {
    use super::*;
    use crate::error::{TdgError, TdgResult};
    use llama_cpp::{EmbeddingsParams, LlamaModel, LlamaParams};
    use std::sync::{Arc, Mutex, OnceLock};

    static MODEL_CACHE: OnceLock<Arc<Mutex<Option<LlamaModel>>>> = OnceLock::new();

    fn get_model() -> TdgResult<std::sync::Arc<std::sync::Mutex<Option<LlamaModel>>>> {
        Ok(MODEL_CACHE
            .get_or_init(|| Arc::new(Mutex::new(None)))
            .clone())
    }

    pub fn init(model_path: Option<&str>) -> TdgResult<()> {
        let path = model_path.map(String::from).unwrap_or_else(|| {
            std::env::var("TDG_GGUF_MODEL_PATH").unwrap_or_else(|_| {
                "models/embeddinggemma-300m/embeddinggemma-300m-Q4_0.gguf".into()
            })
        });

        let cache = get_model()?;
        let mut guard = cache
            .lock()
            .map_err(|e| TdgError::Custom(format!("Lock: {e}")))?;

        if guard.is_some() {
            return Ok(());
        }

        let model = LlamaModel::load_from_file(&path, LlamaParams::default())
            .map_err(|e| TdgError::Custom(format!("Failed to load GGUF model: {e}")))?;

        *guard = Some(model);
        Ok(())
    }

    pub fn embed(text: &str) -> TdgResult<EmbeddingResult> {
        // Auto-initialize if the model cache is empty, mirroring the ONNX
        // lazy-init pattern (onnx_impl::embed, line ~183).
        {
            let cache = get_model()?;
            let guard = cache
                .lock()
                .map_err(|e| TdgError::Custom(format!("Lock: {e}")))?;
            if guard.is_none() {
                drop(guard);
                init(None)?;
            }
        }

        let cache = get_model()?;
        let guard = cache
            .lock()
            .map_err(|e| TdgError::Custom(format!("Lock: {e}")))?;
        let model = guard
            .as_ref()
            .ok_or_else(|| TdgError::Custom("GGUF model not loaded after auto-init.".into()))?;

        let embeddings = model
            .embeddings(
                &[text],
                EmbeddingsParams {
                    n_threads: 4,
                    n_threads_batch: 4,
                },
            )
            .map_err(|e| TdgError::Custom(format!("GGUF embedding failed: {e}")))?;

        let vector = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| TdgError::Custom("No embedding returned".into()))?;

        Ok(EmbeddingResult {
            vector,
            token_count: text.split_whitespace().count(),
        })
    }

    pub fn embed_batch(texts: &[&str]) -> TdgResult<Vec<EmbeddingResult>> {
        // Auto-initialize if the model cache is empty, mirroring the ONNX
        // lazy-init pattern (onnx_impl::embed, line ~183).
        {
            let cache = get_model()?;
            let guard = cache
                .lock()
                .map_err(|e| TdgError::Custom(format!("Lock: {e}")))?;
            if guard.is_none() {
                drop(guard);
                init(None)?;
            }
        }

        let cache = get_model()?;
        let guard = cache
            .lock()
            .map_err(|e| TdgError::Custom(format!("Lock: {e}")))?;
        let model = guard
            .as_ref()
            .ok_or_else(|| TdgError::Custom("GGUF model not loaded after auto-init.".into()))?;

        let all_embeddings = model
            .embeddings(
                texts,
                EmbeddingsParams {
                    n_threads: 4,
                    n_threads_batch: 4,
                },
            )
            .map_err(|e| TdgError::Custom(format!("GGUF batch embedding failed: {e}")))?;

        Ok(all_embeddings
            .into_iter()
            .zip(texts.iter())
            .map(|(vector, text)| EmbeddingResult {
                vector,
                token_count: text.split_whitespace().count(),
            })
            .collect())
    }

    pub fn reset() {
        if let Ok(mut guard) = MODEL_CACHE
            .get_or_init(|| Arc::new(Mutex::new(None)))
            .lock()
        {
            *guard = None;
        }
    }
}

// Re-export GGUF only when ONNX is not also enabled (avoids symbol collision).
#[cfg(all(feature = "gguf", not(feature = "onnx")))]
pub use gguf::*;

// ── Feature-disabled stubs ────────────────────────────────────────────

#[cfg(not(feature = "onnx"))]
pub mod onnx_impl {
    use super::*;
    use crate::error::{TdgError, TdgResult};

    /// Stub: returns error when ONNX feature is disabled.
    pub fn load_config(_config_path: &Path) -> TdgResult<EmbeddingConfig> {
        Err(TdgError::Custom(
            "ONNX feature not enabled. Enable with `--features onnx`.".into(),
        ))
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn init(_config: EmbeddingConfig) -> TdgResult<()> {
        Err(TdgError::Custom(
            "ONNX feature not enabled. Enable with `--features onnx`.".into(),
        ))
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn embed(_text: &str) -> TdgResult<EmbeddingResult> {
        Err(TdgError::Custom(
            "ONNX feature not enabled. Enable with `--features onnx`.".into(),
        ))
    }

    /// Stub: returns error when ONNX feature is disabled.
    pub fn embed_batch(_texts: &[&str]) -> TdgResult<Vec<EmbeddingResult>> {
        Err(TdgError::Custom(
            "ONNX feature not enabled. Enable with `--features onnx`.".into(),
        ))
    }

    /// Stub: no-op when ONNX feature is disabled.
    pub fn reset() {}
}

pub use onnx_impl::*;

/// Build contextual text for embedding.
/// Combines node name, description, and top-K edge relationships.
pub fn build_embedding_text(
    conn: &Connection,
    node_id: &str,
    node_name: &str,
    node_description: &str,
    max_edges: usize,
) -> String {
    let mut parts = Vec::new();

    parts.push(node_name.to_string());

    if !node_description.is_empty() {
        parts.push(node_description.to_string());
    }

    if let Ok(edges) =
        crate::db::crud::get_edges(conn, Some(node_id), None, None, None, max_edges as i64)
    {
        let edge_texts: Vec<String> = edges
            .iter()
            .take(max_edges)
            .map(|e| {
                if let Ok(Some(target)) = crate::db::crud::get_node(conn, &e.target_id) {
                    format!("{}: {}", e.edge_type, target.name)
                } else {
                    format!("{}: {}", e.edge_type, e.target_id)
                }
            })
            .collect();

        if !edge_texts.is_empty() {
            parts.push(format!("Relationships: {}", edge_texts.join("; ")));
        }
    }

    parts.join(" | ")
}

#[cfg(feature = "onnx")]
pub fn ensure_model_files(config: &crate::config::Config) -> TdgResult<()> {
    use std::fs;

    let models_dir = config.models_dir();
    let model_dir = models_dir.join(config.embedding.model_dir_name());

    fs::create_dir_all(&model_dir)
        .map_err(|e| TdgError::Custom(format!("Failed to create models dir: {e}")))?;

    let onnx_path = model_dir.join(config.embedding.onnx_filename());
    if !onnx_path.exists() {
        eprintln!(
            "Downloading {} ({})...",
            config.embedding.model_dir_name(),
            config.embedding.onnx_filename()
        );
        download_file(&model_download_url(config), &onnx_path)?;
    }

    // Q4 ONNX external data format: weights in separate .onnx_data file (Gemma only)
    if config.embedding.model == crate::config::EmbeddingModel::Gemma 
        && config.embedding.quantization == crate::config::EmbeddingQuantization::Q4
    {
        let onnx_filename = config.embedding.onnx_filename();
        let data_filename = format!("{onnx_filename}_data");
        let data_path = model_dir.join(&data_filename);
        if !data_path.exists() {
            let data_url = format!(
                "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/onnx/{data_filename}"
            );
            eprintln!("Downloading {data_filename}...");
            download_file(&data_url, &data_path)?;
        }
    }

    let tokenizer_path = model_dir.join("tokenizer.json");
    if !tokenizer_path.exists() {
        eprintln!("Downloading tokenizer.json...");
        download_file(&model_tokenizer_url(config), &tokenizer_path)?;
    }

    Ok(())
}

#[cfg(feature = "onnx")]
fn model_download_url(config: &crate::config::Config) -> String {
    match config.embedding.model {
        crate::config::EmbeddingModel::Minilm => MINILM_REPO_URL.to_string(),
        crate::config::EmbeddingModel::Gemma => match config.embedding.quantization {
            crate::config::EmbeddingQuantization::Q4 => Q4_DOWNLOAD_URL.to_string(),
            crate::config::EmbeddingQuantization::Q8 => Q8_DOWNLOAD_URL.to_string(),
        },
    }
}

#[cfg(feature = "onnx")]
fn model_tokenizer_url(config: &crate::config::Config) -> String {
    match config.embedding.model {
        crate::config::EmbeddingModel::Minilm => {
            "https://huggingface.co/xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json".to_string()
        }
        crate::config::EmbeddingModel::Gemma => {
            "https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/resolve/main/tokenizer.json"
                .to_string()
        }
    }
}

#[cfg(feature = "onnx")]
fn download_file(url: &str, dest: &Path) -> TdgResult<()> {
    use std::io::Write;

    let response = reqwest::blocking::get(url)
        .map_err(|e| TdgError::Custom(format!("Download failed: {e}")))?;

    if !response.status().is_success() {
        return Err(TdgError::Custom(format!(
            "Download failed with status: {}",
            response.status()
        )));
    }

    let mut file = std::fs::File::create(dest)
        .map_err(|e| TdgError::Custom(format!("Failed to create file: {e}")))?;

    let bytes = response
        .bytes()
        .map_err(|e| TdgError::Custom(format!("Failed to read response: {e}")))?;

    file.write_all(&bytes)
        .map_err(|e| TdgError::Custom(format!("Failed to write file: {e}")))?;

    Ok(())
}

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
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("ONNX feature not enabled"));
    }

    #[cfg(not(feature = "onnx"))]
    #[test]
    fn stub_init_returns_error() {
        let config = EmbeddingConfig::default();
        let result = init(config);
        assert!(result.is_err());
    }
}
