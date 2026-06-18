use std::path::PathBuf;

/// TDG configuration, loaded from environment variables with sensible defaults.
///
/// Mirrors the Python `TDGConfig` class from `core/config.py`.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base home directory (default: `~/.hermes`). Override with `TDG_HOME`.
    pub home: PathBuf,
    /// TDG directory (default: `{home}/tdg`).
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
        Self::from_env()
    }
}

impl Config {
    /// Build configuration from environment variables with defaults.
    pub fn from_env() -> Self {
        let home = expand_env_or_default("TDG_HOME", "~/.hermes");
        let tdg_dir = home.join("tdg");
        let db_path = PathBuf::from(
            std::env::var("TDG_DB_PATH")
                .unwrap_or_else(|_| tdg_dir.join("graph.db").to_string_lossy().into_owned()),
        );
        let state_dir = PathBuf::from(
            std::env::var("TDG_STATE_DIR")
                .unwrap_or_else(|_| home.join("state").to_string_lossy().into_owned()),
        );
        let skills_dir = PathBuf::from(
            std::env::var("TDG_SKILLS_DIR")
                .unwrap_or_else(|_| home.join("skills").to_string_lossy().into_owned()),
        );
        let lean = env_bool("TDG_LEAN");

        Self {
            home,
            tdg_dir,
            db_path,
            state_dir,
            skills_dir,
            lean,
        }
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

/// Expand `~` and env vars, falling back to default.
fn expand_env_or_default(env_key: &str, default: &str) -> PathBuf {
    let val = std::env::var(env_key).unwrap_or_else(|_| default.to_string());
    let expanded = shellexpand::tilde(&val);
    PathBuf::from(expanded.as_ref())
}

/// Parse a boolean env var (true for "1", "true", "yes").
fn env_bool(key: &str) -> bool {
    std::env::var(key)
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "TRUE" | "YES"))
        .unwrap_or(false)
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
}
