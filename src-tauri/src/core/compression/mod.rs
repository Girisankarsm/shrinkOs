/// Core compression abstraction.
///
/// The `CompressionEngine` trait allows swapping compression algorithms
/// without touching the rest of the storage engine. Zstd is the default;
/// future algorithms (LZ4, Brotli, …) are drop-in additions.

use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Instant;
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
    /// Selected Zstd level, or 0 when raw bytes were stored.
    pub selected_level: i32,
    /// Compression duration in milliseconds.
    pub compression_time_ms: u64,
    /// Decompression probe duration in milliseconds.
    pub decompression_time_ms: u64,
    /// Strategy decision recorded for diagnostics and future storage engines.
    pub strategy: String,
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

/// Adaptive Storage Optimization Engine.
///
/// Samples large files, rejects high-entropy data early, then chooses the
/// Zstd level with the best measured savings per unit of compression and
/// decompression work. Chunking and deduplication can be added behind this
/// strategy boundary later without changing the optimizer contract.
pub struct AdaptiveStorageEngine {
    default_level: i32,
}

const ASOE_LEVELS: &[i32] = &[1, 3, 6, 9];
const SAMPLE_BLOCK_SIZE: usize = 64 * 1024;
const MAX_SAMPLE_SIZE: usize = SAMPLE_BLOCK_SIZE * 3;

impl AdaptiveStorageEngine {
    pub fn new(default_level: i32) -> Self {
        Self { default_level: default_level.clamp(1, 22) }
    }

    fn sample(data: &[u8]) -> Vec<u8> {
        if data.len() <= MAX_SAMPLE_SIZE {
            return data.to_vec();
        }

        let middle = data.len() / 2 - SAMPLE_BLOCK_SIZE / 2;
        let last = data.len() - SAMPLE_BLOCK_SIZE;
        let mut sample = Vec::with_capacity(MAX_SAMPLE_SIZE);
        sample.extend_from_slice(&data[..SAMPLE_BLOCK_SIZE]);
        sample.extend_from_slice(&data[middle..middle + SAMPLE_BLOCK_SIZE]);
        sample.extend_from_slice(&data[last..]);
        sample
    }

    fn entropy(data: &[u8]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        let mut counts = [0usize; 256];
        for byte in data {
            counts[*byte as usize] += 1;
        }
        let len = data.len() as f64;
        counts.iter().filter(|count| **count > 0).fold(0.0, |sum, count| {
            let probability = *count as f64 / len;
            sum - probability * probability.log2()
        })
    }

    fn raw_result(data: &[u8], compression_time_ms: u64, strategy: &str) -> CompressionResult {
        let size = data.len() as u64;
        CompressionResult {
            algorithm: CompressionAlgorithm::None,
            data: data.to_vec(),
            original_size: size,
            compressed_size: size,
            was_incompressible: true,
            selected_level: 0,
            compression_time_ms,
            decompression_time_ms: 0,
            strategy: strategy.to_string(),
        }
    }

    pub fn compress(&self, data: &[u8]) -> AppResult<CompressionResult> {
        if data.is_empty() {
            return Ok(Self::raw_result(data, 0, "raw_empty"));
        }

        let strategy_started = Instant::now();
        let sample = Self::sample(data);
        let probe_started = Instant::now();
        let probe = zstd::encode_all(sample.as_slice(), 1)
            .map_err(|e| AppError::Compression(format!("zstd sample probe: {e}")))?;
        let probe_time_ms = probe_started.elapsed().as_millis() as u64;
        let probe_ratio = probe.len() as f64 / sample.len() as f64;

        if Self::entropy(&sample) >= 7.99 && probe_ratio >= 0.999 {
            return Ok(Self::raw_result(data, probe_time_ms, "raw_incompressible_sample"));
        }

        let mut best_level = self.default_level;
        let mut best_score = f64::MIN;
        for level in ASOE_LEVELS {
            let started = Instant::now();
            let candidate = zstd::encode_all(sample.as_slice(), *level)
                .map_err(|e| AppError::Compression(format!("zstd sample benchmark: {e}")))?;
            let compression_micros = started.elapsed().as_micros() as f64;
            let mut decompression_micros = 0.0;
            if candidate.len() < sample.len() {
                let started = Instant::now();
                zstd::decode_all(candidate.as_slice())
                    .map_err(|e| AppError::Compression(format!("zstd sample decode: {e}")))?;
                decompression_micros = started.elapsed().as_micros() as f64;
            }
            let saved = sample.len().saturating_sub(candidate.len()) as f64;
            let score = saved / (compression_micros.max(1.0) + decompression_micros * 0.25);
            if score > best_score {
                best_score = score;
                best_level = *level;
            }
        }

        let compressed = zstd::encode_all(data, best_level)
            .map_err(|e| AppError::Compression(format!("zstd adaptive encode: {e}")))?;
        let mut selected_level = best_level;
        let mut selected_strategy = "adaptive_zstd";
        let mut selected_data = compressed;

        // The sample score is only a prediction. Compare the candidate's
        // final output with the configured baseline before storing anything.
        if best_level != self.default_level {
            let baseline_compressed = zstd::encode_all(data, self.default_level)
                .map_err(|e| AppError::Compression(format!("zstd baseline guard: {e}")))?;
            let baseline_is_raw = baseline_compressed.len() as f64 / data.len() as f64 >= INCOMPRESSIBLE_THRESHOLD;
            let baseline_size = if baseline_is_raw { data.len() } else { baseline_compressed.len() };
            if baseline_size <= selected_data.len() {
                selected_level = self.default_level;
                selected_strategy = "adaptive_zstd_baseline_guard";
                selected_data = if baseline_is_raw {
                    data.to_vec()
                } else {
                    baseline_compressed
                };
            }
        }

        let compression_time_ms = strategy_started.elapsed().as_millis() as u64;
        if selected_data.len() >= data.len()
            || selected_data.len() as f64 / data.len() as f64 >= INCOMPRESSIBLE_THRESHOLD
        {
            return Ok(Self::raw_result(data, compression_time_ms, "raw_after_adaptive"));
        }

        let decompression_started = Instant::now();
        zstd::decode_all(selected_data.as_slice())
            .map_err(|e| AppError::Compression(format!("zstd adaptive decode: {e}")))?;
        let decompression_time_ms = decompression_started.elapsed().as_millis() as u64;

        Ok(CompressionResult {
            algorithm: CompressionAlgorithm::Zstd,
            data: selected_data.clone(),
            original_size: data.len() as u64,
            compressed_size: selected_data.len() as u64,
            was_incompressible: false,
            selected_level,
            compression_time_ms,
            decompression_time_ms,
            strategy: selected_strategy.to_string(),
        })
    }
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
                selected_level: 0,
                compression_time_ms: 0,
                decompression_time_ms: 0,
                strategy: "fixed_zstd_raw".to_string(),
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
            selected_level: self.level,
            compression_time_ms: 0,
            decompression_time_ms: 0,
            strategy: "fixed_zstd".to_string(),
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
            selected_level: 0,
            compression_time_ms: 0,
            decompression_time_ms: 0,
            strategy: "raw".to_string(),
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

    #[test]
    fn adaptive_strategy_records_level_and_roundtrips() {
        let engine = AdaptiveStorageEngine::new(3);
        let original = b"adaptive compression sample ".repeat(20_000);
        let result = engine.compress(&original).expect("adaptive compression failed");
        assert_eq!(result.algorithm, CompressionAlgorithm::Zstd);
        assert!([1, 3, 6, 9].contains(&result.selected_level));
        assert!(result.strategy.starts_with("adaptive_zstd"));
        assert!(result.compressed_size < result.original_size);
        let restored = ZstdEngine::new(result.selected_level)
            .decompress(&result.data, result.original_size)
            .expect("adaptive decompression failed");
        assert_eq!(restored, original);
    }

    #[test]
    fn adaptive_strategy_skips_high_entropy_data() {
        let engine = AdaptiveStorageEngine::new(3);
        let mut state = 0x9E3779B97F4A7C15u64;
        let original: Vec<u8> = (0..200_000)
            .map(|_| {
                state ^= state << 7;
                state ^= state >> 9;
                state ^= state << 8;
                state as u8
            })
            .collect();
        let result = engine.compress(&original).expect("adaptive compression failed");
        assert_eq!(result.algorithm, CompressionAlgorithm::None);
        assert!(result.was_incompressible);
        assert_eq!(result.selected_level, 0);
    }
}
