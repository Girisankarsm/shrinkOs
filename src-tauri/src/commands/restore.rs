/// Restore commands.

use crate::error::AppResult;
use crate::services::optimizer::{OptimizationProgress, Optimizer, ProgressRegistry};
use crate::AppState;
use std::sync::{Arc, Mutex};
use tauri::State;

#[tauri::command]
pub async fn restore_application(
    app_id: String,
    state: State<'_, Mutex<AppState>>,
    progress_reg: State<'_, ProgressRegistry>,
) -> AppResult<()> {
    let (vault_path, config, db) = {
        let st = state.lock().unwrap();
        (st.vault_path.clone(), st.config.clone(), Arc::clone(&st.db))
    };

    let optimizer = Optimizer::new(vault_path, config);
    let prog_reg = Arc::clone(&progress_reg);

    optimizer.restore(&app_id, db, prog_reg).await
}

#[tauri::command]
pub fn get_restore_progress(
    app_id: String,
    progress_reg: State<'_, ProgressRegistry>,
) -> Option<OptimizationProgress> {
    progress_reg
        .lock()
        .ok()
        .and_then(|map| map.get(&app_id).cloned())
}
