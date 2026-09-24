/// Optimization commands.

use crate::error::AppResult;
use crate::services::optimizer::{OptimizationProgress, Optimizer, ProgressRegistry};
use crate::AppState;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;
use tokio::sync::watch;
use tracing::info;

// ── Shared state for progress + cancellation ──────────────────────────────

/// One cancel sender per in-flight optimization, keyed by app_id.
pub type CancelRegistry = Arc<Mutex<HashMap<String, watch::Sender<bool>>>>;

// ── Commands ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn start_optimization(
    app_path: String,
    state: State<'_, Mutex<AppState>>,
    progress_reg: State<'_, ProgressRegistry>,
    cancel_reg: State<'_, CancelRegistry>,
) -> AppResult<String> {
    let (vault_path, config, db) = {
        let st = state.lock().unwrap();
        (st.vault_path.clone(), st.config.clone(), Arc::clone(&st.db))
    };

    let path = PathBuf::from(&app_path);
    let optimizer = Optimizer::new(vault_path, config);
    let prog_reg = Arc::clone(&progress_reg);

    let (cancel_tx, cancel_rx) = watch::channel(false);

    let manifest = optimizer
        .optimize(path, db, prog_reg, cancel_rx)
        .await?;

    let app_id = manifest.app_id.clone();

    // Register cancel sender.
    if let Ok(mut map) = cancel_reg.lock() {
        map.insert(app_id.clone(), cancel_tx);
    }

    info!(app_id, "Optimization started asynchronously");
    Ok(app_id)
}

#[tauri::command]
pub fn get_optimization_progress(
    app_id: String,
    progress_reg: State<'_, ProgressRegistry>,
) -> Option<OptimizationProgress> {
    progress_reg
        .lock()
        .ok()
        .and_then(|map| map.get(&app_id).cloned())
}

#[tauri::command]
pub fn cancel_optimization(
    app_id: String,
    cancel_reg: State<'_, CancelRegistry>,
) -> AppResult<()> {
    if let Ok(map) = cancel_reg.lock() {
        if let Some(tx) = map.get(&app_id) {
            let _ = tx.send(true);
        }
    }
    Ok(())
}
