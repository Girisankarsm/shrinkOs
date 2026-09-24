/// Vault commands.

use crate::database::VaultTotals;
use crate::error::AppResult;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct VaultStats {
    pub totals: VaultTotals,
    pub vault_path: String,
    pub vault_size_bytes: u64,
}

#[tauri::command]
pub fn get_vault_stats(state: State<'_, Mutex<AppState>>) -> AppResult<VaultStats> {
    let st = state.lock().unwrap();
    let totals = st.db.vault_totals()?;
    let vault_size = crate::core::storage::directory_size(&st.vault_path).unwrap_or(0);

    Ok(VaultStats {
        totals,
        vault_path: st.vault_path.display().to_string(),
        vault_size_bytes: vault_size,
    })
}

/// Verify integrity of all stored manifests and their files.
/// Returns a list of app_ids that failed verification.
#[tauri::command]
pub fn verify_vault_integrity(
    state: State<'_, Mutex<AppState>>,
) -> AppResult<Vec<String>> {
    let st = state.lock().unwrap();
    let manifests = st.db.get_all_applications()?;
    let engine = crate::core::compression::ZstdEngine::default_level();
    let mut failed = Vec::new();

    for manifest in manifests {
        let mut ok = true;
        for mf in &manifest.files {
            let stored_path = std::path::PathBuf::from(&manifest.vault_path).join(&mf.relative_path);
            if !stored_path.exists() {
                ok = false;
                break;
            }
            // Decompress and check hash.
            if let Ok(stored_data) = std::fs::read(&stored_path) {
                let decompressed = match mf.compression {
                    crate::core::compression::CompressionAlgorithm::None => stored_data,
                    crate::core::compression::CompressionAlgorithm::Zstd => {
                        match crate::core::compression::CompressionEngine::decompress(&engine, &stored_data, mf.original_size) {
                            Ok(d) => d,
                            Err(_) => { ok = false; break; }
                        }
                    }
                };
                let actual_hash = crate::security::hash_bytes(&decompressed);
                if actual_hash != mf.original_hash {
                    ok = false;
                    break;
                }
            } else {
                ok = false;
                break;
            }
        }
        if !ok {
            failed.push(manifest.app_id.clone());
        }
    }

    Ok(failed)
}
