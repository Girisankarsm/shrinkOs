/// macOS platform integration module.
///
/// Isolated behind #[cfg(target_os = "macos")] so no macOS code leaks
/// into the cross-platform core.

#[cfg(target_os = "macos")]
pub mod macos {
    use crate::core::manifests::{AppManifest, CompatibilityRating};
    use crate::error::{AppError, AppResult};
    use std::path::{Path, PathBuf};
    use tracing::{debug, warn};

    // ── Application bundle discovery ──────────────────────────────

    /// Standard macOS application search paths.
    const APP_SEARCH_PATHS: &[&str] = &[
        "/Applications",
        "/System/Applications",
        // ~/Applications is resolved at runtime via dirs::home_dir()
    ];

    /// Discover all .app bundles in the standard macOS application directories.
    /// Returns the list of bundle paths found.
    pub fn discover_app_bundles() -> AppResult<Vec<PathBuf>> {
        let mut paths = Vec::new();

        // Standard system paths
        for search in APP_SEARCH_PATHS {
            let dir = Path::new(search);
            if dir.exists() {
                collect_app_bundles(dir, &mut paths);
            }
        }

        // User ~/Applications
        if let Some(home) = dirs::home_dir() {
            let user_apps = home.join("Applications");
            if user_apps.exists() {
                collect_app_bundles(&user_apps, &mut paths);
            }
        }

        debug!(found = paths.len(), "macOS app bundle discovery complete");
        Ok(paths)
    }

    fn collect_app_bundles(dir: &Path, out: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("app") {
                    out.push(p);
                }
            }
        }
    }

    // ── Bundle metadata extraction ────────────────────────────────

    /// Extract display name and version from an app bundle's Info.plist.
    pub fn read_bundle_info(bundle_path: &Path) -> (String, Option<String>) {
        let plist_path = bundle_path.join("Contents").join("Info.plist");
        if !plist_path.exists() {
            // Fall back to directory name without .app suffix.
            let name = bundle_name(bundle_path);
            return (name, None);
        }

        // Parse the plist using the `plist` command-line tool on macOS
        // (avoids linking a heavy plist library for MVP).
        // In M2/M3, switch to the `plist` crate for proper parsing.
        let name = read_plist_string_key(&plist_path, "CFBundleDisplayName")
            .or_else(|| read_plist_string_key(&plist_path, "CFBundleName"))
            .unwrap_or_else(|| bundle_name(bundle_path));

        let version = read_plist_string_key(&plist_path, "CFBundleShortVersionString");

        (name, version)
    }

    fn bundle_name(path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    /// Attempt to read a string value from a plist file via `plutil -p`.
    /// Returns None if the key is missing or parsing fails.
    fn read_plist_string_key(plist_path: &Path, key: &str) -> Option<String> {
        // Attempt simple text scan for MVP — look for key in XML plist.
        if let Ok(content) = std::fs::read_to_string(plist_path) {
            // XML plist format: <key>CFBundleName</key>\n<string>VALUE</string>
            if let Some(pos) = content.find(&format!("<key>{key}</key>")) {
                let after = &content[pos + format!("<key>{key}</key>").len()..];
                if let Some(start) = after.find("<string>") {
                    let value_start = start + "<string>".len();
                    if let Some(end) = after[value_start..].find("</string>") {
                        return Some(after[value_start..value_start + end].to_string());
                    }
                }
            }
        }
        None
    }

    // ── Compatibility classification ──────────────────────────────

    /// Known macOS system/driver/extension bundles that must NOT be optimized.
    const UNSUPPORTED_BUNDLE_IDS: &[&str] = &[
        "com.apple.",           // Any Apple system app — never optimize
        "com.google.Keystone",  // Google auto-updater
    ];

    /// Classify a macOS app bundle for compatibility.
    pub fn classify_bundle(bundle_path: &Path) -> CompatibilityRating {
        // System extensions, kernel extensions are always UNSUPPORTED.
        if bundle_path.extension().and_then(|s| s.to_str()) == Some("kext") {
            return CompatibilityRating::Unsupported;
        }

        // Check bundle identifier against known-bad prefixes.
        if let Some(bundle_id) = read_plist_string_key(
            &bundle_path.join("Contents").join("Info.plist"),
            "CFBundleIdentifier",
        ) {
            for prefix in UNSUPPORTED_BUNDLE_IDS {
                if bundle_id.starts_with(prefix) {
                    warn!(bundle_id, "Bundle is a system app — marking UNSUPPORTED");
                    return CompatibilityRating::Unsupported;
                }
            }
        }

        // Applications in /System are never safe to optimize.
        if bundle_path.starts_with("/System") {
            return CompatibilityRating::Unsupported;
        }

        CompatibilityRating::Safe
    }

    // ── Process detection ─────────────────────────────────────────

    /// Return true if any process is running with an executable inside
    /// `bundle_path`. Uses `lsof` (available on all macOS versions).
    pub fn is_app_running(bundle_path: &Path) -> bool {
        let bundle_str = bundle_path.to_string_lossy();
        // lsof -t -c <bundle_name> would work for some apps, but checking
        // open files inside the bundle is more reliable.
        let output = std::process::Command::new("lsof")
            .arg("+D")
            .arg(bundle_path)
            .output();

        match output {
            Ok(o) => {
                let out = String::from_utf8_lossy(&o.stdout);
                // lsof returns lines for every open file; if any line references
                // a process, the app is running.
                out.lines().count() > 1
            }
            Err(e) => {
                warn!(error = %e, path = %bundle_str, "lsof failed — assuming app is not running");
                false
            }
        }
    }

    // ── Vault storage path ────────────────────────────────────────

    /// Return the AppVault data directory on macOS:
    /// ~/Library/Application Support/AppVault/
    pub fn vault_storage_path() -> AppResult<PathBuf> {
        let base = dirs::data_dir()
            .ok_or_else(|| AppError::Io("Could not determine macOS data directory".into()))?;
        Ok(base.join("AppVault"))
    }
}

// ── Windows platform module ───────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub mod windows {
    use crate::core::manifests::CompatibilityRating;
    use crate::error::{AppError, AppResult};
    use std::path::PathBuf;
    use tracing::debug;

    /// Standard Windows application search paths.
    const APP_SEARCH_PATHS: &[&str] = &[
        r"C:\Program Files",
        r"C:\Program Files (x86)",
    ];

    pub fn discover_apps() -> AppResult<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for base in APP_SEARCH_PATHS {
            let dir = std::path::Path::new(base);
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            // Look for the presence of an .exe to confirm it's an app dir.
                            if contains_exe(&p) {
                                paths.push(p);
                            }
                        }
                    }
                }
            }
        }
        debug!(found = paths.len(), "Windows app discovery complete");
        Ok(paths)
    }

    fn contains_exe(dir: &std::path::Path) -> bool {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
            })
    }

    pub fn classify_app(path: &std::path::Path) -> CompatibilityRating {
        // Windows system directories are never safe to optimize.
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.contains("\\windows\\") || path_str.contains("\\system32\\") {
            return CompatibilityRating::Unsupported;
        }
        CompatibilityRating::Safe
    }

    pub fn is_app_running(path: &std::path::Path) -> bool {
        // MVP: use tasklist to check for processes with executables in the path.
        // M4 will use Windows process enumeration APIs directly.
        let output = std::process::Command::new("tasklist")
            .args(["/FO", "CSV", "/NH"])
            .output();

        match output {
            Ok(o) => {
                let out = String::from_utf8_lossy(&o.stdout);
                let dir_name = path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                out.to_lowercase().contains(&dir_name)
            }
            Err(_) => false,
        }
    }

    pub fn vault_storage_path() -> AppResult<PathBuf> {
        let base = dirs::data_local_dir()
            .ok_or_else(|| AppError::Io("Could not determine Windows AppData directory".into()))?;
        Ok(base.join("AppVault"))
    }
}

// ── Cross-platform facade ─────────────────────────────────────────────────

use crate::error::AppResult;
use std::path::PathBuf;
use tauri::AppHandle;

/// Return the AppVault vault storage path for the current platform.
pub fn vault_storage_path(_app: &AppHandle) -> AppResult<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        macos::vault_storage_path()
    }
    #[cfg(target_os = "windows")]
    {
        windows::vault_storage_path()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Fallback for CI/development on Linux.
        let base = dirs::data_local_dir()
            .ok_or_else(|| crate::error::AppError::Io("No data dir".into()))?;
        Ok(base.join("AppVault"))
    }
}

/// Discover application paths on the current platform.
pub fn discover_applications() -> AppResult<Vec<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        macos::discover_app_bundles()
    }
    #[cfg(target_os = "windows")]
    {
        windows::discover_apps()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(Vec::new())
    }
}

/// Check whether the application at `path` is currently running.
pub fn is_app_running(path: &std::path::Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::is_app_running(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::is_app_running(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

/// Classify an application for compatibility.
pub fn classify_application(path: &std::path::Path) -> crate::core::manifests::CompatibilityRating {
    #[cfg(target_os = "macos")]
    {
        macos::classify_bundle(path)
    }
    #[cfg(target_os = "windows")]
    {
        windows::classify_app(path)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        crate::core::manifests::CompatibilityRating::Safe
    }
}

/// Extract the human-readable name and version for an application at `path`.
pub fn app_display_info(path: &std::path::Path) -> (String, Option<String>) {
    #[cfg(target_os = "macos")]
    {
        macos::read_bundle_info(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown")
            .to_string();
        (name, None)
    }
}
