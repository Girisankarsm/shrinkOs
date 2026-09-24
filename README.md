# ShrinkOS

ShrinkOS is a macOS Tauri application that compresses application files into a local vault and restores them after integrity verification.

## Version Changes

### 0.1.0

- Added native macOS application discovery, analysis, optimization, verification, and restore.
- Added real Zstandard compression with measured savings and Blake3 integrity checks.
- Added Tauri desktop bundling and corrected native IPC state registration.

## How Space Reduction Works

ShrinkOS scans each selected application, classifies file content, samples larger files, and selects an efficient Zstandard level. Already-compressed or high-entropy files are kept in their original form. Reported savings equal the original file size minus the stored size. The original application is never replaced during optimization.

## Compression Architecture

1. Analyze the selected `.app` bundle and classify its files.
2. Sample larger files and estimate entropy and compressibility.
3. Benchmark Zstandard levels 1, 3, 6, and 9 on representative samples.
4. Compare the selected result with the configured baseline and store only the smaller safe representation.
5. Record size, ratio, level, timings, and strategy in the manifest.
6. Verify restored content with Blake3 hashes, then restore files to the original path on request.

The current MVP uses file-by-file adaptive compression. Chunking, deduplication, cloud storage, and filesystem virtualization are not implemented.

## Run and Test

```sh
npm install
npm run desktop
```

For a safe test copy:

```sh
mkdir -p "$HOME/Applications"
cp -R "/Applications/The Unarchiver.app" "$HOME/Applications/ShrinkOS Test.app"
```

In the native ShrinkOS window: scan, analyze, optimize, verify integrity, then restore the test copy. The scan checks `/Applications`, nested application folders, `/System/Applications`, and `~/Applications`.

Build the macOS application with:

```sh
npm run tauri build
```

Output: `src-tauri/target/release/bundle/macos/ShrinkOS.app`

## Next Steps

Complete the native end-to-end workflow test, then improve failure recovery and release packaging. Chunking, deduplication, cloud storage, Windows support, and filesystem virtualization are outside the current MVP.
