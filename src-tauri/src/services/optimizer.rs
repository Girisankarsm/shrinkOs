/// Application optimization service.
///
/// Milestone 1: file-by-file Zstd compression with real measured savings.
/// Milestone 2 will layer in chunking and deduplication.
///
/// Safety contract:
///   1. Application must not be running.
///   2. Sufficient free space must exist for staging.
///   3. Original files are preserved until verification passes.
///   4. On any failure, the original application is fully recoverable.

use crate::core::compression::{engine_for, AdaptiveStorageEngine};
use crate::core::manifests::{AppManifest, AppStatus, CompatibilityRating, ManifestFile};
use crate::core::storage::analyze_application;
use crate::database::Database;
use crate::error::{AppError, AppResult};
use crate::security::{assert_path_within, assert_sufficient_space, atomic_write, hash_bytes, hash_file, verify_file_hash};
use crate::platform;
use crate::services::config::Config;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tracing::{error, info, warn};

// ── Progress tracking ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationProgress {
    pub app_id: String,
    pub phase: OptimizationPhase,
    pub files_total: u64,
    pub files_done: u64,
    pub bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_saved: u64,
    pub percent: f32,
    pub current_file: Option<String>,
    pub error: Option<String>,
    pub finished: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationPhase {
    Analyzing,
    CheckingSpace,
    CheckingRunning,
    Compressing,
    Verifying,
    Done,
    Failed,
    Cancelled,
}

impl OptimizationProgress {
    fn new(app_id: &str, files_total: u64, bytes_total: u64) -> Self {
        OptimizationProgress {
            app_id: app_id.to_string(),
            phase: OptimizationPhase::Analyzing,
            files_total,
            files_done: 0,
            bytes_total,
            bytes_done: 0,
            bytes_saved: 0,
            percent: 0.0,
            current_file: None,
            error: None,
            finished: false,
        }
    }

    fn update_percent(&mut self) {
        if self.bytes_total == 0 {
            self.percent = 0.0;
        } else {
            self.percent = (self.bytes_done as f32 / self.bytes_total as f32) * 100.0;
        }
    }
}

// ── Progress registry ─────────────────────────────────────────────────────

/// Shared in-process map from app_id → progress.
/// Tauri commands poll this via `get_optimization_progress`.
pub type ProgressRegistry = Arc<Mutex<std::collections::HashMap<String, OptimizationProgress>>>;

// ── Optimizer ─────────────────────────────────────────────────────────────

pub struct Optimizer {
    vault_path: PathBuf,
    config: Config,
}

impl Optimizer {
    pub fn new(vault_path: PathBuf, config: Config) -> Self {
        Optimizer { vault_path, config }
    }

    /// Main optimization entry point.
    ///
    /// Runs entirely in the calling async task. Progress updates are sent
    /// to `progress_tx` so the Tauri command layer can poll or stream them.
    pub async fn optimize(
        &self,
        app_path: PathBuf,
        db: Arc<Database>,
        progress_registry: ProgressRegistry,
        cancel_rx: watch::Receiver<bool>,
    ) -> AppResult<AppManifest> {
        // ── Phase 1: Analyze ─────────────────────────────────────
        info!(app = %app_path.display(), "Starting optimization");

        let platform_name = if cfg!(target_os = "macos") { "macos" } else { "windows" };
        let (display_name, version) = platform::app_display_info(&app_path);

        // Safety: classify application before touching anything.
        let compat = platform::classify_application(&app_path);
        if compat == CompatibilityRating::Unsupported {
            return Err(AppError::AppUnsupported(format!(
                "{} is a system/unsupported application that AppVault cannot safely optimize.",
                display_name
            )));
        }

        // Safety: check the application is not running.
        if platform::is_app_running(&app_path) {
            return Err(AppError::AppRunning(format!(
                "{} is currently running. Please quit it before optimizing.",
                display_name
            )));
        }

        let analysis = analyze_application(&app_path)?;

        let mut manifest = AppManifest::new(
            display_name.clone(),
            app_path.display().to_string(),
            self.vault_app_path(&app_path).display().to_string(),
            platform_name.to_string(),
        );
        manifest.version = version;
        manifest.compatibility = compat;
        manifest.status = AppStatus::Analyzing;

        let app_id = manifest.app_id.clone();
        let mut progress = OptimizationProgress::new(
            &app_id,
            analysis.file_count,
            analysis.total_size_bytes,
        );
        progress.phase = OptimizationPhase::Analyzing;
        self.emit_progress(&progress_registry, progress.clone());

        // ── Phase 2: Space check ──────────────────────────────────
        progress.phase = OptimizationPhase::CheckingSpace;
        self.emit_progress(&progress_registry, progress.clone());

        // We need temporary space equal to the original app size (worst case).
        assert_sufficient_space(&self.vault_path, analysis.total_size_bytes)?;

        // ── Phase 3: Create vault directory for this app ──────────
        let app_vault_dir = self.vault_app_path(&app_path);
        std::fs::create_dir_all(&app_vault_dir)
            .map_err(|e| AppError::Io(format!("create vault dir: {e}")))?;

        manifest.status = AppStatus::Optimizing;
        db.upsert_application(&manifest)?;

        // ── Phase 4: Compress each file ───────────────────────────
        progress.phase = OptimizationPhase::Compressing;
        self.emit_progress(&progress_registry, progress.clone());

        let strategy = AdaptiveStorageEngine::new(self.config.compression_level);
        let mut manifest_files: Vec<ManifestFile> = Vec::new();

        for file_entry in &analysis.files {
            // Check for cancellation.
            if *cancel_rx.borrow() {
                warn!(app_id, "Optimization cancelled by user");
                manifest.status = AppStatus::Error;
                manifest.notes.push("Cancelled by user".into());
                db.upsert_application(&manifest)?;
                progress.phase = OptimizationPhase::Cancelled;
                progress.finished = true;
                self.emit_progress(&progress_registry, progress.clone());
                return Err(AppError::Cancelled);
            }

            let src_path = app_path.join(&file_entry.relative_path);
            if !src_path.exists() {
                warn!(path = %src_path.display(), "File disappeared during optimization — skipping");
                continue;
            }

            progress.current_file = Some(file_entry.relative_path.clone());
            self.emit_progress(&progress_registry, progress.clone());

            // Hash original file for integrity verification.
            let original_hash = hash_file(&src_path)?;
            let original_data = std::fs::read(&src_path)
                .map_err(|e| AppError::Io(format!("read '{}': {e}", src_path.display())))?;

            // Compress (the engine auto-detects incompressible data).
            let compressed = strategy.compress(&original_data)?;

            // Write compressed file to vault.
            let dest_path = app_vault_dir.join(&file_entry.relative_path);
            atomic_write(&dest_path, &compressed.data)?;

            progress.bytes_done += file_entry.size_bytes;
            progress.bytes_saved += compressed.space_saved();
            progress.files_done += 1;
            progress.update_percent();

            manifest_files.push(ManifestFile {
                relative_path: file_entry.relative_path.clone(),
                original_size: compressed.original_size,
                stored_size: compressed.compressed_size,
                original_hash,
                compression: compressed.algorithm,
                compression_level: compressed.selected_level,
                compression_time_ms: compressed.compression_time_ms,
                decompression_time_ms: compressed.decompression_time_ms,
                strategy: compressed.strategy,
            });
        }

        // ── Phase 5: Verify stored files ──────────────────────────
        progress.phase = OptimizationPhase::Verifying;
        progress.current_file = None;
        self.emit_progress(&progress_registry, progress.clone());

        for mf in &manifest_files {
            let stored_path = app_vault_dir.join(&mf.relative_path);
            if !stored_path.exists() {
                return Err(AppError::IntegrityFailure(format!(
                    "stored file missing after compression: {}",
                    mf.relative_path
                )));
            }
            // Read back compressed data, decompress, verify hash.
            let stored_data = std::fs::read(&stored_path)
                .map_err(|e| AppError::Io(format!("verify read: {e}")))?;

            let engine = engine_for(mf.compression, mf.compression_level);
            let decompressed = engine.decompress(&stored_data, mf.original_size)
                .map_err(|e| AppError::IntegrityFailure(format!(
                    "decompression during verify failed for '{}': {e}",
                    mf.relative_path
                )))?;

            let actual_hash = hash_bytes(&decompressed);
            if actual_hash != mf.original_hash {
                return Err(AppError::IntegrityFailure(format!(
                    "hash mismatch after compression for '{}': expected {}, got {}",
                    mf.relative_path, mf.original_hash, actual_hash
                )));
            }
        }

        // ── Phase 6: Finalize manifest ────────────────────────────
        manifest.files = manifest_files;
        manifest.finalize();
        manifest.status = AppStatus::Managed;
        manifest.optimized_at = Some(Utc::now());

        // Write manifest JSON to vault directory.
        let manifest_path = app_vault_dir.join("manifest.json");
        let manifest_json = manifest.to_json()?;
        atomic_write(&manifest_path, &manifest_json)?;

        // The verified vault is now the source of truth. Remove the original
        // bundle so optimization produces an actual reduction on disk.
        std::fs::remove_dir_all(&app_path)
            .map_err(|e| AppError::Io(format!("remove original app '{}': {e}", app_path.display())))?;

        // Persist to database.
        db.upsert_application(&manifest)?;
        db.log_operation(
            Some(&manifest.app_id),
            "optimize",
            "success",
            Some(&format!(
                "Compressed {} files; saved {} bytes",
                manifest.file_count, manifest.space_saved
            )),
        )?;

        progress.phase = OptimizationPhase::Done;
        progress.percent = 100.0;
        progress.finished = true;
        self.emit_progress(&progress_registry, progress.clone());

        info!(
            app_id = %manifest.app_id,
            original_size = manifest.original_size,
            stored_size = manifest.stored_size,
            space_saved = manifest.space_saved,
            ratio = manifest.compression_ratio,
            "Optimization complete"
        );

        Ok(manifest)
    }

    // ── Restore ───────────────────────────────────────────────────

    /// Restore an application from the vault to its original location.
    pub async fn restore(
        &self,
        app_id: &str,
        db: Arc<Database>,
        progress_registry: ProgressRegistry,
    ) -> AppResult<()> {
        let mut manifest = db
            .get_application(app_id)?
            .ok_or_else(|| AppError::AppNotFound(app_id.to_string()))?;

        info!(app_id, app_name = %manifest.name, "Starting restore");

        let app_vault_dir = PathBuf::from(&manifest.vault_path);
        let original_path = PathBuf::from(&manifest.original_path);
        let files_total = manifest.files.len() as u64;
        let mut progress = OptimizationProgress::new(app_id, files_total, manifest.stored_size);
        progress.phase = OptimizationPhase::Compressing; // reuse for decompress phase
        self.emit_progress(&progress_registry, progress.clone());

        for mf in &manifest.files {
            let stored_path = app_vault_dir.join(&mf.relative_path);
            let restore_path = original_path.join(&mf.relative_path);

            if !stored_path.exists() {
                return Err(AppError::IntegrityFailure(format!(
                    "vault file missing: {}",
                    stored_path.display()
                )));
            }

            let stored_data = std::fs::read(&stored_path)
                .map_err(|e| AppError::Io(format!("read vault file: {e}")))?;

            let engine = engine_for(mf.compression, mf.compression_level);
            let restored_data = engine.decompress(&stored_data, mf.original_size)?;

            // Verify integrity before writing.
            let actual_hash = hash_bytes(&restored_data);
            if actual_hash != mf.original_hash {
                return Err(AppError::IntegrityFailure(format!(
                    "hash mismatch during restore for '{}': corrupted vault data",
                    mf.relative_path
                )));
            }

            atomic_write(&restore_path, &restored_data)?;

            progress.files_done += 1;
            progress.bytes_done += mf.stored_size;
            progress.update_percent();
            self.emit_progress(&progress_registry, progress.clone());
        }

        // Update manifest status.
        manifest.status = AppStatus::Unmanaged;
        manifest.last_accessed = Utc::now();
        db.upsert_application(&manifest)?;
        db.log_operation(Some(app_id), "restore", "success", None)?;

        progress.phase = OptimizationPhase::Done;
        progress.percent = 100.0;
        progress.finished = true;
        self.emit_progress(&progress_registry, progress.clone());

        info!(app_id, "Restore complete");
        Ok(())
    }

    // ── Helpers ───────────────────────────────────────────────────

    fn vault_app_path(&self, app_path: &Path) -> PathBuf {
        let app_name = app_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        self.vault_path.join("apps").join(app_name)
    }

    fn emit_progress(&self, registry: &ProgressRegistry, progress: OptimizationProgress) {
        if let Ok(mut map) = registry.lock() {
            map.insert(progress.app_id.clone(), progress);
        }
    }
}
