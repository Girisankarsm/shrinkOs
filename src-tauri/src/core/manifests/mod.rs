/// Application manifest — the ground truth record for every managed app.
///
/// Versioned schema so future migrations can be written without data loss.

use crate::core::compression::CompressionAlgorithm;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MANIFEST_SCHEMA_VERSION: u32 = 2;

// ── Compatibility rating ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CompatibilityRating {
    /// Safe to optimize; AppVault has verified this application type.
    Safe,
    /// Proceed with caution; user should be informed of risks.
    Caution,
    /// AppVault will not optimize this application.
    Unsupported,
}

impl std::fmt::Display for CompatibilityRating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompatibilityRating::Safe => write!(f, "SAFE"),
            CompatibilityRating::Caution => write!(f, "CAUTION"),
            CompatibilityRating::Unsupported => write!(f, "UNSUPPORTED"),
        }
    }
}

// ── Application status lifecycle ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppStatus {
    /// Discovered but not yet optimized.
    Unmanaged,
    /// Currently being analyzed.
    Analyzing,
    /// Currently being optimized.
    Optimizing,
    /// Optimization complete — stored in vault.
    Managed,
    /// Currently being restored.
    Restoring,
    /// Restore failed; user intervention required.
    Error,
}

// ── File entry inside a manifest ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    /// Path relative to the application root.
    pub relative_path: String,
    /// Original size in bytes.
    pub original_size: u64,
    /// Stored size (compressed) in bytes.
    pub stored_size: u64,
    /// Blake3 hash of the *original* file content (hex string).
    pub original_hash: String,
    /// Compression algorithm applied to this file.
    pub compression: CompressionAlgorithm,
    /// Compression level used (0 = not compressed).
    pub compression_level: i32,
    /// Compression duration measured for this file.
    #[serde(default)]
    pub compression_time_ms: u64,
    /// Decompression probe duration measured for this file.
    #[serde(default)]
    pub decompression_time_ms: u64,
    /// Adaptive strategy selected for this file.
    #[serde(default)]
    pub strategy: String,
}

// ── The manifest itself ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppManifest {
    /// Monotonically increasing schema version — never decrease this.
    pub schema_version: u32,
    /// Unique identifier for this managed application.
    pub app_id: String,
    /// Human-readable application name.
    pub name: String,
    /// Application version string, if detectable.
    pub version: Option<String>,
    /// Target platform: "macos" | "windows"
    pub platform: String,
    /// Absolute path to the *original* application location.
    pub original_path: String,
    /// Absolute path to the vault storage directory for this app.
    pub vault_path: String,
    /// Total original size of all files (bytes).
    pub original_size: u64,
    /// Total stored size after compression (bytes).
    pub stored_size: u64,
    /// Bytes currently held in the warm cache.
    pub cached_size: u64,
    /// Bytes actually saved (original_size − stored_size).
    pub space_saved: u64,
    /// True actual compression ratio: stored_size / original_size.
    pub compression_ratio: f64,
    /// Primary compression algorithm used.
    pub compression: CompressionAlgorithm,
    /// Number of files in this application.
    pub file_count: u64,
    /// Per-file entries.
    pub files: Vec<ManifestFile>,
    /// Compatibility assessment.
    pub compatibility: CompatibilityRating,
    /// Current lifecycle status.
    pub status: AppStatus,
    /// ISO-8601 timestamp when this manifest was created.
    pub created_at: DateTime<Utc>,
    /// ISO-8601 timestamp of the last access.
    pub last_accessed: DateTime<Utc>,
    /// ISO-8601 timestamp of the last successful optimization.
    pub optimized_at: Option<DateTime<Utc>>,
    /// Free-form notes, e.g. "restored after corruption".
    pub notes: Vec<String>,
}

impl AppManifest {
    /// Create a new blank manifest for an application being ingested.
    pub fn new(
        name: String,
        original_path: String,
        vault_path: String,
        platform: String,
    ) -> Self {
        let now = Utc::now();
        AppManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            app_id: Uuid::new_v4().to_string(),
            name,
            version: None,
            platform,
            original_path,
            vault_path,
            original_size: 0,
            stored_size: 0,
            cached_size: 0,
            space_saved: 0,
            compression_ratio: 1.0,
            compression: CompressionAlgorithm::Zstd,
            file_count: 0,
            files: Vec::new(),
            compatibility: CompatibilityRating::Safe,
            status: AppStatus::Unmanaged,
            created_at: now,
            last_accessed: now,
            optimized_at: None,
            notes: Vec::new(),
        }
    }

    /// Recalculate derived fields after files are populated.
    pub fn finalize(&mut self) {
        self.original_size = self.files.iter().map(|f| f.original_size).sum();
        self.stored_size = self.files.iter().map(|f| f.stored_size).sum();
        self.file_count = self.files.len() as u64;
        self.space_saved = self.original_size.saturating_sub(self.stored_size);
        self.compression_ratio = if self.original_size == 0 {
            1.0
        } else {
            self.stored_size as f64 / self.original_size as f64
        };
    }

    /// Serialize to JSON bytes.
    pub fn to_json(&self) -> crate::error::AppResult<Vec<u8>> {
        serde_json::to_vec_pretty(self).map_err(|e| {
            crate::error::AppError::Other(format!("manifest serialization: {e}"))
        })
    }

    /// Deserialize from JSON bytes.
    pub fn from_json(data: &[u8]) -> crate::error::AppResult<Self> {
        serde_json::from_slice(data).map_err(|e| {
            crate::error::AppError::Other(format!("manifest deserialization: {e}"))
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest() -> AppManifest {
        let mut m = AppManifest::new(
            "TestApp".into(),
            "/Applications/TestApp.app".into(),
            "/vault/TestApp".into(),
            "macos".into(),
        );
        m.files.push(ManifestFile {
            relative_path: "Contents/MacOS/TestApp".into(),
            original_size: 10_000_000,
            stored_size: 6_000_000,
            original_hash: "deadbeef".into(),
            compression: CompressionAlgorithm::Zstd,
            compression_level: 3,
            compression_time_ms: 0,
            decompression_time_ms: 0,
            strategy: "fixed_zstd".into(),
        });
        m.finalize();
        m
    }

    #[test]
    fn finalize_computes_space_saved() {
        let m = make_manifest();
        assert_eq!(m.space_saved, 4_000_000);
    }

    #[test]
    fn compression_ratio_correct() {
        let m = make_manifest();
        assert!((m.compression_ratio - 0.6).abs() < 0.001);
    }

    #[test]
    fn roundtrip_json() {
        let m = make_manifest();
        let json = m.to_json().expect("to_json failed");
        let restored = AppManifest::from_json(&json).expect("from_json failed");
        assert_eq!(m.app_id, restored.app_id);
        assert_eq!(m.space_saved, restored.space_saved);
    }
}
