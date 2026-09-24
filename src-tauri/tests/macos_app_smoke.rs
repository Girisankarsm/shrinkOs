use appvault_lib::core::compression::{CompressionEngine, ZstdEngine};
use appvault_lib::core::storage::{analyze_application, directory_size};
use appvault_lib::database::Database;
use appvault_lib::platform;
use appvault_lib::services::config::Config;
use appvault_lib::services::optimizer::{Optimizer, ProgressRegistry};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tempfile::TempDir;
use tokio::sync::watch;

#[cfg(target_os = "macos")]
#[tokio::test]
async fn real_macos_app_optimize_verify_restore_smoke() {
    let installed_app = Path::new("/Applications/The Unarchiver.app");
    assert!(installed_app.is_dir(), "fixture app is missing: {}", installed_app.display());

    let discovered = platform::discover_applications().expect("application discovery failed");
    assert!(discovered.iter().any(|path| path == installed_app), "fixture was not discovered");

    let workspace = TempDir::new().expect("temporary workspace failed");
    let source_app = workspace.path().join("The Unarchiver.app");
    copy_directory(installed_app, &source_app);
    let restore_app = workspace.path().join("restored").join("The Unarchiver.app");
    let vault_path = workspace.path().join("vault");
    fs::create_dir_all(&vault_path).expect("vault directory failed");

    let analysis = analyze_application(&source_app).expect("application analysis failed");
    let original_size = analysis.total_size_bytes;
    let file_count = analysis.file_count;

    let baseline_started = Instant::now();
    let mut baseline_stored_size = 0u64;
    let mut baseline_sizes = HashMap::new();
    for file in &analysis.files {
        let data = fs::read(source_app.join(&file.relative_path)).expect("baseline file read failed");
        let baseline_size = ZstdEngine::new(3)
            .compress(&data)
            .expect("baseline compression failed")
            .compressed_size;
        baseline_stored_size += baseline_size;
        baseline_sizes.insert(file.relative_path.clone(), baseline_size);
    }
    let baseline_time_ms = baseline_started.elapsed().as_millis();

    let db = Arc::new(Database::open(&vault_path).expect("database initialization failed"));
    let progress: ProgressRegistry = Arc::new(Mutex::new(HashMap::new()));
    let optimizer = Optimizer::new(vault_path.clone(), Config::default());
    let (_, cancel_rx) = watch::channel(false);

    let compression_started = Instant::now();
    let mut manifest = optimizer
        .optimize(source_app.clone(), Arc::clone(&db), Arc::clone(&progress), cancel_rx)
        .await
        .expect("optimization failed");
    let compression_time_ms = compression_started.elapsed().as_millis();
    let compressed_size = manifest.stored_size;
    let app_id = manifest.app_id.clone();
    let vault_size = directory_size(&vault_path).expect("vault size measurement failed");
    let adaptive_compression_time_ms: u64 = manifest.files.iter().map(|file| file.compression_time_ms).sum();
    let adaptive_decompression_time_ms: u64 = manifest.files.iter().map(|file| file.decompression_time_ms).sum();
    let mut selected_levels = HashMap::<i32, usize>::new();
    let mut adaptive_worse_files = 0usize;
    for file in &manifest.files {
        *selected_levels.entry(file.compression_level).or_default() += 1;
        if file.stored_size > baseline_sizes.get(&file.relative_path).copied().unwrap_or(file.stored_size) {
            adaptive_worse_files += 1;
        }
    }

    assert_eq!(manifest.original_size, original_size);
    assert_eq!(manifest.file_count, file_count);
    assert_eq!(manifest.files.len() as u64, file_count);

    let integrity_failures = verify_manifest(&manifest);
    assert!(integrity_failures.is_empty(), "vault integrity failed: {integrity_failures:?}");

    manifest.original_path = restore_app.display().to_string();
    db.upsert_application(&manifest).expect("test restore path update failed");
    let restore_started = Instant::now();
    optimizer
        .restore(&app_id, Arc::clone(&db), Arc::clone(&progress))
        .await
        .expect("restore failed");
    let restore_time_ms = restore_started.elapsed().as_millis();

    let restored_analysis = analyze_application(&restore_app).expect("restored analysis failed");
    assert_eq!(restored_analysis.total_size_bytes, original_size);
    assert_eq!(restored_analysis.file_count, file_count);
    assert_same_files(&source_app, &restore_app, &analysis.files);

    println!(
        "SMOKE_MEASUREMENTS original_size={} file_count={} baseline_fixed_zstd_size={} baseline_fixed_zstd_time_ms={} adaptive_size={} adaptive_vault_size={} adaptive_space_saved={} adaptive_vs_fixed_saved={} adaptive_worse_files={} adaptive_wall_time_ms={} adaptive_file_compression_time_ms={} adaptive_decompression_probe_time_ms={} restore_time_ms={} selected_levels={:?} integrity=PASS",
        original_size,
        file_count,
        baseline_stored_size,
        baseline_time_ms,
        compressed_size,
        vault_size,
        manifest.space_saved,
        baseline_stored_size.saturating_sub(compressed_size),
        adaptive_worse_files,
        compression_time_ms,
        adaptive_compression_time_ms,
        adaptive_decompression_time_ms,
        restore_time_ms,
        selected_levels,
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn real_macos_app_optimize_verify_restore_smoke() {}

fn copy_directory(source: &Path, destination: &Path) {
    let status = std::process::Command::new("cp")
        .args(["-R", &source.to_string_lossy(), &destination.to_string_lossy()])
        .status()
        .expect("failed to invoke cp");
    assert!(status.success(), "copy failed with status {status}");
}

fn verify_manifest(manifest: &appvault_lib::core::manifests::AppManifest) -> Vec<String> {
    let engine = appvault_lib::core::compression::ZstdEngine::default_level();
    let mut failures = Vec::new();
    for file in &manifest.files {
        let path = PathBuf::from(&manifest.vault_path).join(&file.relative_path);
        let data = fs::read(path).expect("stored file read failed");
        let restored = match file.compression {
            appvault_lib::core::compression::CompressionAlgorithm::None => data,
            appvault_lib::core::compression::CompressionAlgorithm::Zstd => engine
                .decompress(&data, file.original_size)
                .expect("stored file decompression failed"),
        };
        if appvault_lib::security::hash_bytes(&restored) != file.original_hash {
            failures.push(file.relative_path.clone());
        }
    }
    failures
}

fn assert_same_files(source: &Path, restored: &Path, files: &[appvault_lib::core::storage::FileEntry]) {
    for file in files {
        let source_data = fs::read(source.join(&file.relative_path)).expect("source file read failed");
        let restored_data = fs::read(restored.join(&file.relative_path)).expect("restored file read failed");
        assert_eq!(source_data, restored_data, "restored file differs: {}", file.relative_path);
    }
}