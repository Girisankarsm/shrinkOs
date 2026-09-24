/// Security utilities.
///
/// Path validation, safe temp files, integrity verification.
/// Never trust user-supplied paths without sanitization.

use crate::error::{AppError, AppResult};
use blake3::Hasher;
use std::path::{Path, PathBuf};
use tracing::warn;

// ── Path safety ───────────────────────────────────────────────────────────

/// Verify that `child` is genuinely inside `base` (prevents path traversal).
/// Both paths are canonicalized before comparison.
pub fn assert_path_within(base: &Path, child: &Path) -> AppResult<PathBuf> {
    // We must ensure both paths exist for canonicalize to work.
    // If child doesn't exist yet, check the parent instead.
    let canonical_base = base
        .canonicalize()
        .map_err(|e| AppError::Security(format!("cannot canonicalize base '{}': {e}", base.display())))?;

    // Resolve the child as far as it exists.
    let existing_ancestor = find_existing_ancestor(child);
    let canonical_child = existing_ancestor
        .canonicalize()
        .map_err(|e| AppError::Security(format!("cannot canonicalize child '{}': {e}", child.display())))?;

    if !canonical_child.starts_with(&canonical_base) {
        warn!(
            base = %canonical_base.display(),
            child = %canonical_child.display(),
            "Path traversal attempt blocked"
        );
        return Err(AppError::Security(format!(
            "path '{}' is outside the allowed base '{}'",
            child.display(),
            base.display()
        )));
    }

    Ok(canonical_child)
}

fn find_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        if let Some(parent) = current.parent() {
            current = parent.to_path_buf();
        } else {
            return path.to_path_buf();
        }
    }
}

// ── Content integrity ─────────────────────────────────────────────────────

/// Compute the Blake3 hash of a file, returning it as a lowercase hex string.
pub fn hash_file(path: &Path) -> AppResult<String> {
    let data = std::fs::read(path)
        .map_err(|e| AppError::Io(format!("read '{}': {e}", path.display())))?;
    Ok(hash_bytes(&data))
}

/// Compute the Blake3 hash of a byte slice.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize().to_hex().to_string()
}

/// Verify that a file's Blake3 hash matches the expected value.
pub fn verify_file_hash(path: &Path, expected_hash: &str) -> AppResult<()> {
    let actual = hash_file(path)?;
    if actual != expected_hash {
        return Err(AppError::IntegrityFailure(format!(
            "hash mismatch for '{}': expected {expected_hash}, got {actual}",
            path.display()
        )));
    }
    Ok(())
}

/// Verify that a byte slice's Blake3 hash matches the expected value.
pub fn verify_bytes_hash(data: &[u8], expected_hash: &str) -> AppResult<()> {
    let actual = hash_bytes(data);
    if actual != expected_hash {
        return Err(AppError::IntegrityFailure(format!(
            "data hash mismatch: expected {expected_hash}, got {actual}"
        )));
    }
    Ok(())
}

// ── Atomic file writes ────────────────────────────────────────────────────

/// Write `data` to `dest` atomically via a same-filesystem temp file.
/// This prevents the destination from being left in a partial state
/// if the process is interrupted.
pub fn atomic_write(dest: &Path, data: &[u8]) -> AppResult<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| AppError::Io(format!("no parent for '{}'", dest.display())))?;

    std::fs::create_dir_all(parent)
        .map_err(|e| AppError::Io(format!("create dirs '{}': {e}", parent.display())))?;

    // Write to a temp file in the same directory (same filesystem → rename is atomic on POSIX).
    let tmp = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| AppError::Io(format!("create temp file: {e}")))?;

    std::io::Write::write_all(&mut tmp.as_file(), data)
        .map_err(|e| AppError::Io(format!("write temp file: {e}")))?;

    tmp.persist(dest)
        .map_err(|e| AppError::Io(format!("persist to '{}': {e}", dest.display())))?;

    Ok(())
}

// ── Storage space guard ────────────────────────────────────────────────────

/// Return an error if `path`'s filesystem has less than `required_bytes` free.
pub fn assert_sufficient_space(path: &Path, required_bytes: u64) -> AppResult<()> {
    let available = available_space(path)?;
    // Add 10% headroom to avoid filling the drive to the brim.
    let needed = (required_bytes as f64 * 1.10) as u64;
    if available < needed {
        return Err(AppError::InsufficientStorage {
            required: needed,
            available,
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn available_space(path: &Path) -> AppResult<u64> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;
    let cstr = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|e| AppError::Io(e.to_string()))?;
    let mut stat: MaybeUninit<libc::statvfs> = MaybeUninit::uninit();
    let ret = unsafe { libc::statvfs(cstr.as_ptr(), stat.as_mut_ptr()) };
    if ret != 0 {
        return Err(AppError::Io(std::io::Error::last_os_error().to_string()));
    }
    let s = unsafe { stat.assume_init() };
    Ok(s.f_bavail as u64 * s.f_frsize as u64)
}

#[cfg(target_os = "windows")]
fn available_space(path: &Path) -> AppResult<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::PCWSTR;
    let path_w: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut avail = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(PCWSTR(path_w.as_ptr()), Some(&mut avail), None, None)
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    Ok(avail)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn available_space(_path: &Path) -> AppResult<u64> {
    Ok(u64::MAX) // Placeholder for unsupported platforms in tests
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn hash_deterministic() {
        let data = b"hello appvault";
        let h1 = hash_bytes(data);
        let h2 = hash_bytes(data);
        assert_eq!(h1, h2);
        assert!(!h1.is_empty());
    }

    #[test]
    fn verify_hash_detects_corruption() {
        let data = b"original data";
        let good_hash = hash_bytes(data);
        assert!(verify_bytes_hash(data, &good_hash).is_ok());
        assert!(verify_bytes_hash(b"corrupted", &good_hash).is_err());
    }

    #[test]
    fn atomic_write_creates_file() {
        let dir = TempDir::new().unwrap();
        let dest = dir.path().join("output.bin");
        atomic_write(&dest, b"test content").unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"test content");
    }

    #[test]
    fn path_within_detects_traversal() {
        let dir = TempDir::new().unwrap();
        // Constructing /tmp/xyz/../../../etc/passwd style path
        let traversal = dir.path().join("../outside");
        let result = assert_path_within(dir.path(), &traversal);
        assert!(result.is_err(), "Expected traversal to be blocked");
    }
}
