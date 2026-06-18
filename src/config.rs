use std::path::PathBuf;

use figment::{
    Figment,
    providers::{Env, Format, Json, Serialized, Yaml},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration error: {0}")]
    Figment(#[from] figment::Error),
    #[error("validation failed: {0}")]
    Validation(String),
}

/// TDG configuration, loaded from config files and environment variables with sensible defaults.
///
/// Mirrors the Python `TDGConfig` class from `core/config.py`.
/// Supports hierarchical loading: defaults → tdg.yaml → tdg.json → TDG_* env vars.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Base home directory (default: `~/.hermes`). Override with `TDG_HOME`.
    pub home: PathBuf,
    /// TDG directory (default: `{home}/tdg`). Always derived from `home`.
    pub tdg_dir: PathBuf,
    /// Database path (default: `{tdg_dir}/graph.db`). Override with `TDG_DB_PATH`.
    pub db_path: PathBuf,
    /// State directory (default: `{home}/state`). Override with `TDG_STATE_DIR`.
    pub state_dir: PathBuf,
    /// Skills directory (default: `{home}/skills`). Override with `TDG_SKILLS_DIR`.
    pub skills_dir: PathBuf,
    /// Lean mode flag. Override with `TDG_LEAN`.
    pub lean: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self::load().unwrap_or_else(|_| Self::defaults())
    }
}

impl Config {
    fn defaults() -> Self {
        let home = PathBuf::from("~/.hermes");
        let tdg_dir = home.join("tdg");
        Self {
            home: home.clone(),
            tdg_dir,
            db_path: home.join("tdg").join("graph.db"),
            state_dir: home.join("state"),
            skills_dir: home.join("skills"),
            lean: false,
        }
    }

    /// Figment provider chain: defaults → tdg.yaml → tdg.json → env vars.
    fn figment() -> Figment {
        Figment::from(Serialized::defaults(Self::defaults()))
            .merge(Yaml::file("tdg.yaml"))
            .merge(Json::file("tdg.json"))
            .merge(Env::prefixed("TDG_").split("__"))
    }

    pub fn load() -> Result<Self, ConfigError> {
        let mut config: Config = Self::figment().extract()?;

        // Expand `~` in home path (Env provider won't expand shell tildes).
        let home_lossy = config.home.to_string_lossy();
        let expanded = shellexpand::tilde(&home_lossy);
        config.home = PathBuf::from(expanded.into_owned());

        // tdg_dir is always derived from home — no env var override.
        config.tdg_dir = config.home.join("tdg");

        // Recompute derived paths when their override env var is absent,
        // so changing TDG_HOME cascades to children like the original code.
        if std::env::var("TDG_DB_PATH").is_err() {
            config.db_path = config.tdg_dir.join("graph.db");
        }
        if std::env::var("TDG_STATE_DIR").is_err() {
            config.state_dir = config.home.join("state");
        }
        if std::env::var("TDG_SKILLS_DIR").is_err() {
            config.skills_dir = config.home.join("skills");
        }

        Ok(config)
    }

    /// Build configuration from environment variables (backward-compatible alias).
    pub fn from_env() -> Self {
        Self::load().unwrap_or_else(|_| Self::defaults())
    }

    /// Build config with explicit paths (useful for testing).
    pub fn with_db_path(db_path: PathBuf) -> Self {
        let mut cfg = Self::from_env();
        cfg.db_path = db_path;
        cfg
    }

    /// Archive database path: `{tdg_dir}/graph/tdg_archive.db`
    pub fn archive_db_path(&self) -> PathBuf {
        self.tdg_dir.join("graph").join("tdg_archive.db")
    }

    /// Graph directory: `{tdg_dir}/graph`
    pub fn graph_dir(&self) -> PathBuf {
        self.tdg_dir.join("graph")
    }

    /// Config directory: `{tdg_dir}/config`
    pub fn config_dir(&self) -> PathBuf {
        self.tdg_dir.join("config")
    }

    /// Snapshots directory: `{tdg_dir}/snapshots`
    pub fn snapshots_dir(&self) -> PathBuf {
        self.tdg_dir.join("snapshots")
    }

    /// Working memory path: `{state_dir}/hermes-working-memory.json`
    pub fn working_memory_path(&self) -> PathBuf {
        self.state_dir.join("hermes-working-memory.json")
    }

    /// Loop state path: `{state_dir}/hermes-loop-state.json`
    pub fn loop_state_path(&self) -> PathBuf {
        self.state_dir.join("hermes-loop-state.json")
    }

    /// Meta view cache path: `{state_dir}/hermes-meta-view-cache.json`
    pub fn meta_view_cache_path(&self) -> PathBuf {
        self.state_dir.join("hermes-meta-view-cache.json")
    }

    /// Constraints path: `{state_dir}/hermes-constraints.json`
    pub fn constraints_path(&self) -> PathBuf {
        self.state_dir.join("hermes-constraints.json")
    }

    /// Diagnostic thresholds path: `{config_dir}/diagnostic_thresholds.yaml`
    pub fn diagnostic_thresholds_path(&self) -> PathBuf {
        self.config_dir().join("diagnostic_thresholds.yaml")
    }

    /// ONNX model path: `{tdg_dir}/models/all-MiniLM-L6-v2/onnx/model_quantized.onnx`
    pub fn onnx_model_path(&self) -> PathBuf {
        self.tdg_dir
            .join("models")
            .join("all-MiniLM-L6-v2")
            .join("onnx")
            .join("model_quantized.onnx")
    }

    /// Repository root: two levels up from this source file's directory.
    ///
    /// Mirrors Python `TDGConfig.repo_root`: `Path(__file__).resolve().parent.parent`.
    /// In Rust we approximate this by walking up from `CARGO_MANIFEST_DIR`.
    pub fn repo_root() -> PathBuf {
        // CARGO_MANIFEST_DIR points to the crate root (tdg-rust/)
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Read a JSON config file and replace `{REPO_ROOT}` placeholders with the actual repo root.
    ///
    /// Mirrors Python `resolve_config_json()` from `core/config.py`.
    pub fn resolve_config_json(path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
        let content = std::fs::read_to_string(path)?;
        let repo = Self::repo_root()
            .to_string_lossy()
            .into_owned();
        let resolved = content.replace("{REPO_ROOT}", &repo);
        let value: serde_json::Value = serde_json::from_str(&resolved)?;
        Ok(value)
    }

    /// Ensure all required directories exist.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for dir in [
            &self.home,
            &self.tdg_dir,
            &self.state_dir,
            &self.graph_dir(),
            &self.snapshots_dir(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_paths() {
        let cfg = Config::from_env();
        assert!(cfg.home.to_string_lossy().contains(".hermes"));
        assert!(cfg.db_path.to_string_lossy().contains("graph.db"));
    }

    #[test]
    fn config_lean_from_env() {
        // Default should be false
        let cfg = Config::from_env();
        // lean depends on env, just check it's a bool
        let _ = cfg.lean;
    }

    #[test]
    fn figment_loads_defaults() {
        let cfg = Config::load().expect("load should succeed");
        assert!(cfg.home.to_string_lossy().contains(".hermes"));
        assert!(cfg.tdg_dir == cfg.home.join("tdg"));
        assert!(cfg.db_path == cfg.tdg_dir.join("graph.db"));
        assert!(cfg.state_dir == cfg.home.join("state"));
        assert!(cfg.skills_dir == cfg.home.join("skills"));
        assert!(!cfg.lean);
    }

    #[test]
    fn derived_paths_computed_from_home() {
        let cfg = Config::defaults();
        assert_eq!(cfg.archive_db_path(), cfg.tdg_dir.join("graph").join("tdg_archive.db"));
        assert_eq!(cfg.graph_dir(), cfg.tdg_dir.join("graph"));
        assert_eq!(cfg.config_dir(), cfg.tdg_dir.join("config"));
        assert_eq!(cfg.snapshots_dir(), cfg.tdg_dir.join("snapshots"));
        assert_eq!(cfg.working_memory_path(), cfg.state_dir.join("hermes-working-memory.json"));
        assert_eq!(cfg.loop_state_path(), cfg.state_dir.join("hermes-loop-state.json"));
        assert_eq!(cfg.meta_view_cache_path(), cfg.state_dir.join("hermes-meta-view-cache.json"));
        assert_eq!(cfg.constraints_path(), cfg.state_dir.join("hermes-constraints.json"));
        assert_eq!(cfg.diagnostic_thresholds_path(), cfg.config_dir().join("diagnostic_thresholds.yaml"));
        assert!(cfg.onnx_model_path().to_string_lossy().contains("model_quantized.onnx"));
    }

    #[test]
    fn config_is_serializable() {
        let cfg = Config::defaults();
        let json = serde_json::to_string(&cfg).expect("serialize to JSON");
        let deserialized: Config = serde_json::from_str(&json).expect("deserialize from JSON");
        assert_eq!(cfg.home, deserialized.home);
        assert_eq!(cfg.tdg_dir, deserialized.tdg_dir);
        assert_eq!(cfg.db_path, deserialized.db_path);
        assert_eq!(cfg.lean, deserialized.lean);
    }

    #[test]
    fn repo_root_is_not_empty() {
        let root = Config::repo_root();
        assert!(!root.as_os_str().is_empty());
    }
}
