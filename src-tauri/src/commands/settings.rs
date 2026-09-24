/// Settings commands.

use crate::error::AppResult;
use crate::services::config::Config;
use crate::AppState;
use std::sync::Mutex;
use tauri::State;

#[tauri::command]
pub fn get_settings(state: State<'_, Mutex<AppState>>) -> AppResult<Config> {
    let st = state.lock().unwrap();
    Ok(st.config.clone())
}

#[tauri::command]
pub fn update_settings(
    new_config: Config,
    state: State<'_, Mutex<AppState>>,
) -> AppResult<()> {
    let mut st = state.lock().unwrap();
    new_config.save(&st.vault_path)?;
    st.config = new_config;
    Ok(())
}
