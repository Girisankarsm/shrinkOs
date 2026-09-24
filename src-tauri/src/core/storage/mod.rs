/// Storage accounting and analysis.
///
/// Measures real on-disk footprints using OS-reported sizes so the UI
/// never displays fabricated numbers.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;
use walkdir::WalkDir;

// ── Disk statistics ───────────────────────────────────────────────────────

/// System-wide disk statistics for the volume containing `path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub appvault_bytes: u64,
}

/// Retrieve free/used/total for the filesystem containing `path`.
pub fn disk_stats(path: &Path, vault_path: &Path) -> AppResult<DiskStats> {
    let stat = fs_stat(path)?;
    let vault_bytes = if vault_path.exists() {
        directory_size(vault_path)?
    } else {
        0
    };

    Ok(DiskStats {
        total_bytes: stat.total,
        used_bytes: stat.used,
        available_bytes: stat.available,
        appvault_bytes: vault_bytes,
    })
}

// ── Application analysis ──────────────────────────────────────────────────

/// A single file in an application bundle, including its raw size and
/// pre-classified compressibility hint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub compressibility: Compressibility,
    pub content_type: ContentType,
}

/// Our compression effort hint per file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compressibility {
    /// High probability of meaningful size reduction.
    High,
    /// Some reduction likely, but limited.
    Medium,
    /// Already compressed / encrypted — skip or minimal gain.
    Low,
}

/// Broad content category used for compatibility classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Binary,
    Text,
    Image,
    Video,
    Audio,
    Archive,
    Data,
    Unknown,
}

/// Full analysis result for one application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationAnalysis {
    pub app_path: String,
    pub total_size_bytes: u64,
    pub file_count: u64,
    /// Conservative estimate of how many bytes can be saved.
    /// This is an *estimate*; real compression must be measured.
    pub estimated_compressible_bytes: u64,
    /// Fraction (0.0 – 1.0) of the app that is likely compressible.
    pub compressibility_ratio: f64,
    pub files: Vec<FileEntry>,
}

impl ApplicationAnalysis {
    pub fn estimated_saved_bytes(&self) -> u64 {
        // Assume Zstd achieves ~50% on compressible data (conservative).
        (self.estimated_compressible_bytes as f64 * 0.50) as u64
    }
}

/// Walk an application bundle and classify every file.
pub fn analyze_application(app_path: &Path) -> AppResult<ApplicationAnalysis> {
    if !app_path.exists() {
        return Err(AppError::AppNotFound(
            app_path.display().to_string(),
        ));
    }

    let mut files = Vec::new();
    let mut total_size: u64 = 0;
    let mut compressible: u64 = 0;

    for entry in WalkDir::new(app_path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let size = entry
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        let rel_path = entry
            .path()
            .strip_prefix(app_path)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .to_string();

        let ext = entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let (compress, ctype) = classify_extension(&ext);

        if compress == Compressibility::High || compress == Compressibility::Medium {
            compressible += size;
        }
        total_size += size;

        files.push(FileEntry {
            relative_path: rel_path,
            size_bytes: size,
            compressibility: compress,
            content_type: ctype,
        });
    }

    let ratio = if total_size == 0 {
        0.0
    } else {
        compressible as f64 / total_size as f64
    };

    debug!(
        app = %app_path.display(),
        total_size,
        files = files.len(),
        compressible,
        ratio,
        "Application analysis complete"
    );

    Ok(ApplicationAnalysis {
        app_path: app_path.display().to_string(),
        total_size_bytes: total_size,
        file_count: files.len() as u64,
        estimated_compressible_bytes: compressible,
        compressibility_ratio: ratio,
        files,
    })
}

/// Compute real directory size (sum of file sizes, not blocks).
pub fn directory_size(path: &Path) -> AppResult<u64> {
    let mut total: u64 = 0;
    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        total += entry.metadata().map(|m| m.len()).unwrap_or(0);
    }
    Ok(total)
}

// ── Extension classification ──────────────────────────────────────────────

fn classify_extension(ext: &str) -> (Compressibility, ContentType) {
    // Already-compressed or encrypted formats — do NOT waste CPU compressing.
    const LOW: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "webp", "avif", "heic", "heif",
        "mp4", "mov", "avi", "mkv", "webm", "m4v",
        "mp3", "aac", "flac", "ogg", "m4a",
        "zip", "gz", "bz2", "xz", "7z", "rar", "zst",
        "woff", "woff2",
        "pdf",          // usually compressed internally
        "docx", "xlsx", "pptx", // Office XML, already zipped
        "ipa", "apk",
        "pak",          // Chromium/game data — often pre-compressed
    ];

    // High-compressibility text / executable formats.
    const HIGH: &[&str] = &[
        "exe", "dll", "so", "dylib",      // native binaries — good zstd ratio
        "js", "mjs", "cjs",
        "ts", "tsx", "jsx",
        "html", "css", "svg",
        "xml", "json", "yaml", "toml",
        "txt", "md", "log",
        "py", "rb", "go", "rs", "c", "cpp", "h", "hpp", "swift", "kt",
        "sh", "bash", "zsh",
        "plist",
        "nib", "xib",
        "ttf", "otf",                     // font bytecode — compresses well
    ];

    if LOW.contains(&ext) {
        let ctype = if matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "heic" | "heif") {
            ContentType::Image
        } else if matches!(ext, "mp4" | "mov" | "avi" | "mkv" | "webm" | "m4v") {
            ContentType::Video
        } else if matches!(ext, "mp3" | "aac" | "flac" | "ogg" | "m4a") {
            ContentType::Audio
        } else {
            ContentType::Archive
        };
        return (Compressibility::Low, ctype);
    }

    if HIGH.contains(&ext) {
        let ctype = if matches!(ext, "js" | "ts" | "tsx" | "jsx" | "mjs" | "cjs" | "py" | "rb" | "go" | "rs" | "c" | "cpp" | "h" | "hpp" | "swift" | "kt" | "sh" | "bash" | "zsh") {
            ContentType::Text
        } else if matches!(ext, "jpg" | "jpeg" | "png" | "gif" | "webp") {
            ContentType::Image
        } else {
            ContentType::Binary
        };
        return (Compressibility::High, ctype);
    }

    (Compressibility::Medium, ContentType::Unknown)
}

// ── Platform free-space helpers ───────────────────────────────────────────

struct FsStat {
    total: u64,
    available: u64,
    used: u64,
}

#[cfg(target_os = "macos")]
fn fs_stat(path: &Path) -> AppResult<FsStat> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let path_cstr = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| AppError::Io(format!("CString: {e}")))?;

    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), stat.as_mut_ptr()) };
    if ret != 0 {
        return Err(AppError::Io(std::io::Error::last_os_error().to_string()));
    }
    let stat = unsafe { stat.assume_init() };

    let block = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block;
    let available = stat.f_bavail as u64 * block;
    let used = total.saturating_sub(stat.f_bfree as u64 * block);

    Ok(FsStat { total, available, used })
}

#[cfg(target_os = "windows")]
fn fs_stat(path: &Path) -> AppResult<FsStat> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW};
    use windows::core::PCWSTR;

    let path_w: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut avail = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;

    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(path_w.as_ptr()),
            Some(&mut avail),
            Some(&mut total),
            Some(&mut free),
        )
        .map_err(|e| AppError::Io(e.to_string()))?;
    }

    Ok(FsStat {
        total,
        available: avail,
        used: total.saturating_sub(free),
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn fs_stat(_path: &Path) -> AppResult<FsStat> {
    // Linux / other: use statvfs via libc if needed in the future.
    Err(AppError::Other("Unsupported platform for disk stats".into()))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_test_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}".repeat(500)).unwrap();
        fs::write(dir.path().join("data.json"), r#"{"key":"value"}"#.repeat(200)).unwrap();
        // Simulate a "pre-compressed" file by using PNG extension.
        fs::write(dir.path().join("icon.png"), vec![0xFFu8; 1024]).unwrap();
        dir
    }

    #[test]
    fn analyze_returns_correct_file_count() {
        let dir = make_test_dir();
        let analysis = analyze_application(dir.path()).expect("analysis failed");
        assert_eq!(analysis.file_count, 3);
    }

    #[test]
    fn png_classified_as_low_compressibility() {
        let (c, _) = classify_extension("png");
        assert_eq!(c, Compressibility::Low);
    }

    #[test]
    fn rs_classified_as_high_compressibility() {
        let (c, _) = classify_extension("rs");
        assert_eq!(c, Compressibility::High);
    }

    #[test]
    fn directory_size_nonzero() {
        let dir = make_test_dir();
        let size = directory_size(dir.path()).expect("size failed");
        assert!(size > 0);
    }
}
