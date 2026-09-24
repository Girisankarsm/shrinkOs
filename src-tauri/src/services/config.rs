/// Application configuration.
///
/// Loaded from a TOML file in the vault directory.
/// Defaults are always applied so missing keys never crash the app.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::info;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Zstd compression level (1–22). Default: 3 (fast, good ratio).
    pub compression_level: i32,
    /// Maximum cache size in bytes. Default: 2 GiB.
    pub max_cache_bytes: u64,
    /// Whether to auto-detect new application installations.
    pub auto_detect: bool,
    /// Dark mode preference (true = dark, false = light, None = system).
    pub dark_mode: Option<bool>,
    /// Show notifications for newly detected apps.
    pub show_notifications: bool,
    /// Maximum parallel compression tasks.
    pub parallel_tasks: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            compression_level: 3,
            max_cache_bytes: 2 * 1024 * 1024 * 1024, // 2 GiB
            auto_detect: true,
            dark_mode: None,
            show_notifications: true,
            parallel_tasks: 2,
        }
    }
}

impl Config {
    pub fn load_or_default(vault_path: &Path) -> AppResult<Self> {
        let config_dir = vault_path.join("config");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| AppError::Io(format!("create config dir: {e}")))?;

        let config_path = config_dir.join(CONFIG_FILE_NAME);

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)
                .map_err(|e| AppError::Io(format!("read config: {e}")))?;
            let cfg: Config = toml::from_str(&contents)
                .map_err(|e| AppError::Config(format!("parse config: {e}")))?;
            info!(path = %config_path.display(), "Loaded config");
            Ok(cfg)
        } else {
            let cfg = Config::default();
            cfg.save(vault_path)?;
            info!(path = %config_path.display(), "Created default config");
            Ok(cfg)
        }
    }

    pub fn save(&self, vault_path: &Path) -> AppResult<()> {
        let config_dir = vault_path.join("config");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| AppError::Io(format!("create config dir: {e}")))?;

        let config_path = config_dir.join(CONFIG_FILE_NAME);
        let contents = toml::to_string_pretty(self)
            .map_err(|e| AppError::Config(format!("serialize config: {e}")))?;

        std::fs::write(&config_path, contents)
            .map_err(|e| AppError::Io(format!("write config: {e}")))?;

        Ok(())
    }
}
