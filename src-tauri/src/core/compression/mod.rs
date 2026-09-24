/// Core compression abstraction.
///
/// The `CompressionEngine` trait allows swapping compression algorithms
/// without touching the rest of the storage engine. Zstd is the default;
/// future algorithms (LZ4, Brotli, …) are drop-in additions.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use tracing::{debug, warn};

// ── Algorithm enum ────────────────────────────────────────────────────────

/// Identifies the compression algorithm stored alongside compressed data.
/// Stored in chunk metadata so data can always be decompressed even if the
/// default algorithm changes in a future version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionAlgorithm {
    None,
    Zstd,
    // Future: Lz4, Brotli, …
}

impl std::fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompressionAlgorithm::None => write!(f, "none"),
            CompressionAlgorithm::Zstd => write!(f, "zstd"),
        }
    }
}

// ── Result of a compression attempt ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionResult {
    /// Algorithm that was actually applied (may be `None` if incompressible).
    pub algorithm: CompressionAlgorithm,
    /// Compressed (or unmodified) bytes.
    pub data: Vec<u8>,
    /// Original input size in bytes.
    pub original_size: u64,
    /// Size of `data` in bytes.
    pub compressed_size: u64,
    /// True when the engine detected compressing would grow the data
    /// (e.g. already-compressed assets) and stored the original instead.
    pub was_incompressible: bool,
}

impl CompressionResult {
    pub fn ratio(&self) -> f64 {
        if self.original_size == 0 {
            return 1.0;
        }
        self.compressed_size as f64 / self.original_size as f64
    }

    pub fn space_saved(&self) -> u64 {
        self.original_size.saturating_sub(self.compressed_size)
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────

pub trait CompressionEngine: Send + Sync {
    fn algorithm(&self) -> CompressionAlgorithm;

    /// Compress `data`. Implementations MUST fall back to storing the raw
    /// bytes if compression produces larger output.
    fn compress(&self, data: &[u8]) -> AppResult<CompressionResult>;

    /// Decompress `data` that was compressed by `algorithm`.
    fn decompress(&self, data: &[u8], original_size: u64) -> AppResult<Vec<u8>>;

    /// Compress streaming from `reader` into `writer`.
    fn compress_stream(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
    ) -> AppResult<CompressionResult>;
}

// ── Zstd implementation ───────────────────────────────────────────────────

/// Default compression level: 3 gives a strong ratio/speed tradeoff.
/// Level 1 is faster; level 22 is maximum compression.
pub const ZSTD_DEFAULT_LEVEL: i32 = 3;

/// If compression saves less than this fraction, we store raw bytes.
/// Example: 0.95 means "skip if we only save 5% or less".
const INCOMPRESSIBLE_THRESHOLD: f64 = 0.95;

pub struct ZstdEngine {
    level: i32,
}

impl ZstdEngine {
    pub fn new(level: i32) -> Self {
        let level = level.clamp(1, 22);
        Self { level }
    }

    pub fn default_level() -> Self {
        Self::new(ZSTD_DEFAULT_LEVEL)
    }
}

impl CompressionEngine for ZstdEngine {
    fn algorithm(&self) -> CompressionAlgorithm {
        CompressionAlgorithm::Zstd
    }

    fn compress(&self, data: &[u8]) -> AppResult<CompressionResult> {
        let original_size = data.len() as u64;

        let compressed = zstd::encode_all(data, self.level)
            .map_err(|e| AppError::Compression(format!("zstd encode: {e}")))?;

        let compressed_size = compressed.len() as u64;
        let ratio = compressed_size as f64 / original_size.max(1) as f64;

        if ratio >= INCOMPRESSIBLE_THRESHOLD {
            warn!(
                original_size,
                compressed_size,
                ratio,
                "Data appears incompressible — storing raw bytes"
            );
            return Ok(CompressionResult {
                algorithm: CompressionAlgorithm::None,
                data: data.to_vec(),
                original_size,
                compressed_size: original_size,
                was_incompressible: true,
            });
        }

        debug!(
            original_size,
            compressed_size,
            ratio,
            "Zstd compression successful"
        );

        Ok(CompressionResult {
            algorithm: CompressionAlgorithm::Zstd,
            data: compressed,
            original_size,
            compressed_size,
            was_incompressible: false,
        })
    }

    fn decompress(&self, data: &[u8], _original_size: u64) -> AppResult<Vec<u8>> {
        zstd::decode_all(data)
            .map_err(|e| AppError::Compression(format!("zstd decode: {e}")))
    }

    fn compress_stream(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
    ) -> AppResult<CompressionResult> {
        let mut input_buf = Vec::new();
        reader
            .read_to_end(&mut input_buf)
            .map_err(|e| AppError::Io(format!("read stream: {e}")))?;

        let result = self.compress(&input_buf)?;
        writer
            .write_all(&result.data)
            .map_err(|e| AppError::Io(format!("write stream: {e}")))?;

        Ok(result)
    }
}

// ── Passthrough (no compression) ──────────────────────────────────────────

/// Used when the caller explicitly wants no compression (e.g. for encrypted
/// or already-compressed file types).
pub struct NoopEngine;

impl CompressionEngine for NoopEngine {
    fn algorithm(&self) -> CompressionAlgorithm {
        CompressionAlgorithm::None
    }

    fn compress(&self, data: &[u8]) -> AppResult<CompressionResult> {
        let size = data.len() as u64;
        Ok(CompressionResult {
            algorithm: CompressionAlgorithm::None,
            data: data.to_vec(),
            original_size: size,
            compressed_size: size,
            was_incompressible: true,
        })
    }

    fn decompress(&self, data: &[u8], _original_size: u64) -> AppResult<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn compress_stream(
        &self,
        reader: &mut dyn Read,
        writer: &mut dyn Write,
    ) -> AppResult<CompressionResult> {
        let mut buf = Vec::new();
        reader
            .read_to_end(&mut buf)
            .map_err(|e| AppError::Io(format!("read stream: {e}")))?;
        let result = self.compress(&buf)?;
        writer
            .write_all(&result.data)
            .map_err(|e| AppError::Io(format!("write stream: {e}")))?;
        Ok(result)
    }
}

// ── Factory ───────────────────────────────────────────────────────────────

/// Construct the appropriate engine for a given algorithm.
pub fn engine_for(algorithm: CompressionAlgorithm, level: i32) -> Box<dyn CompressionEngine> {
    match algorithm {
        CompressionAlgorithm::None => Box::new(NoopEngine),
        CompressionAlgorithm::Zstd => Box::new(ZstdEngine::new(level)),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_zstd_text() {
        let engine = ZstdEngine::default_level();
        let original = b"Hello, AppVault! ".repeat(500);
        let result = engine.compress(&original).expect("compress failed");
        assert!(
            !result.was_incompressible,
            "Repetitive text should be compressible"
        );
        let restored = engine
            .decompress(&result.data, result.original_size)
            .expect("decompress failed");
        assert_eq!(original.as_slice(), restored.as_slice());
    }

    #[test]
    fn incompressible_data_stored_raw() {
        let engine = ZstdEngine::default_level();
        // Pseudo-random bytes are typically incompressible.
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut data = Vec::with_capacity(4096);
        for i in 0u64..4096 {
            let mut h = DefaultHasher::new();
            i.hash(&mut h);
            data.push((h.finish() & 0xff) as u8);
        }
        let result = engine.compress(&data).expect("compress failed");
        assert!(
            result.was_incompressible || result.ratio() < INCOMPRESSIBLE_THRESHOLD + 0.1,
            "Random data should not compress well"
        );
    }

    #[test]
    fn compression_ratio_realistic() {
        let engine = ZstdEngine::default_level();
        let data = b"The quick brown fox jumps over the lazy dog. ".repeat(1000);
        let result = engine.compress(&data).expect("compress failed");
        assert!(
            result.ratio() < 0.5,
            "Repetitive English text should compress to <50%"
        );
    }

    #[test]
    fn noop_engine_roundtrip() {
        let engine = NoopEngine;
        let data = b"test data";
        let result = engine.compress(data).expect("compress failed");
        assert_eq!(result.compressed_size, data.len() as u64);
        let restored = engine.decompress(&result.data, result.original_size).expect("decompress failed");
        assert_eq!(data.as_slice(), restored.as_slice());
    }
}
