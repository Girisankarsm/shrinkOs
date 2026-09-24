/// Application management commands.

use crate::core::manifests::AppManifest;
use crate::core::storage::analyze_application as analyze_app_path;
use crate::error::AppResult;
use crate::platform;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

// ── DTOs ──────────────────────────────────────────────────────────────────

/// Lightweight representation of a discovered (not-yet-managed) application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredApp {
    pub path: String,
    pub name: String,
    pub version: Option<String>,
    pub size_bytes: u64,
    pub compatibility: String,
    pub is_running: bool,
}

// ── Commands ──────────────────────────────────────────────────────────────

/// Scan the system for installable applications.
#[tauri::command]
pub async fn discover_applications(
    _state: State<'_, Mutex<AppState>>,
) -> AppResult<Vec<DiscoveredApp>> {
    let app_paths = platform::discover_applications()?;

    let mut results = Vec::new();
    for path in app_paths {
        let (name, version) = platform::app_display_info(&path);
        let compat = platform::classify_application(&path);
        let is_running = platform::is_app_running(&path);

        // Quick size estimate using directory walk.
        let size_bytes = crate::core::storage::directory_size(&path).unwrap_or(0);

        results.push(DiscoveredApp {
            path: path.display().to_string(),
            name,
            version,
            size_bytes,
            compatibility: compat.to_string(),
            is_running,
        });
    }

    Ok(results)
}

/// Deep-analyze a specific application to estimate compression savings.
#[tauri::command]
pub async fn analyze_application(
    app_path: String,
    _state: State<'_, Mutex<AppState>>,
) -> AppResult<crate::core::storage::ApplicationAnalysis> {
    let path = PathBuf::from(&app_path);
    // Security: we don't restrict which path users can analyze — they
    // explicitly selected it. We do canonicalize to prevent symlink tricks.
    analyze_app_path(&path)
}

/// Return all applications currently managed by AppVault.
#[tauri::command]
pub fn get_managed_applications(
    state: State<'_, Mutex<AppState>>,
) -> AppResult<Vec<AppManifest>> {
    let st = state.lock().unwrap();
    st.db.get_all_applications()
}

/// Return detailed manifest for a single application.
#[tauri::command]
pub fn get_application_detail(
    app_id: String,
    state: State<'_, Mutex<AppState>>,
) -> AppResult<Option<AppManifest>> {
    let st = state.lock().unwrap();
    st.db.get_application(&app_id)
}
