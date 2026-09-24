/// System information commands.

use crate::core::storage::disk_stats;
use crate::error::AppResult;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub filesystem: String,
    pub vault_path: String,
    pub app_version: String,
    pub milestone: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionStatus {
    pub app_management: bool,
    pub requires_approval: bool,
    pub message: String,
}

#[tauri::command]
pub fn get_permission_status() -> AppResult<PermissionStatus> {
    #[cfg(target_os = "macos")]
    {
        let app_management = std::process::Command::new("/usr/bin/test")
            .args(["-w", "/Applications"])
            .status()
            .map(|status| status.success())
            .unwrap_or(false);

        return Ok(PermissionStatus {
            app_management,
            requires_approval: !app_management,
            message: if app_management {
                "ShrinkOS can update and restore applications.".to_string()
            } else {
                "Allow ShrinkOS in System Settings > Privacy & Security > App Management to update and restore applications.".to_string()
            },
        });
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(PermissionStatus {
            app_management: true,
            requires_approval: false,
            message: "Application management permissions are available.".to_string(),
        })
    }
}

#[tauri::command]
pub fn open_permission_settings() -> AppResult<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AppManagement")
            .status()
            .map_err(|e| crate::error::AppError::Io(format!("open permission settings: {e}")))?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_system_info(state: State<'_, Mutex<AppState>>) -> AppResult<SystemInfo> {
    let st = state.lock().unwrap();
    Ok(SystemInfo {
        os: std::env::consts::OS.to_string(),
        os_version: os_version(),
        filesystem: detect_filesystem(&st.vault_path),
        vault_path: st.vault_path.display().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        milestone: 1,
    })
}

#[tauri::command]
pub fn get_disk_stats(state: State<'_, Mutex<AppState>>) -> AppResult<crate::core::storage::DiskStats> {
    let st = state.lock().unwrap();
    disk_stats(&st.vault_path, &st.vault_path)
}

fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        format!("macOS {}", output.trim())
    }
    #[cfg(target_os = "windows")]
    {
        "Windows".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Unknown".to_string()
    }
}

fn detect_filesystem(path: &std::path::Path) -> String {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("diskutil")
            .args(["info", &path.display().to_string()])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        // Look for "File System Personality" line
        for line in output.lines() {
            if line.contains("File System Personality") {
                if let Some(fs) = line.split(':').nth(1) {
                    return fs.trim().to_string();
                }
            }
        }
        "APFS".to_string() // reasonable default on modern macOS
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Unknown".to_string()
    }
}
