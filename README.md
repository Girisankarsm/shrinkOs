# ShrinkOS

ShrinkOS is a macOS Tauri application that compresses application files into a local vault and restores them after integrity verification.

## Version Changes

### 0.1.0

- Added native macOS application discovery, analysis, optimization, verification, and restore.
- Added real Zstandard compression with measured savings and Blake3 integrity checks.
- Added Tauri desktop bundling and corrected native IPC state registration.
- Added macOS App Management permission onboarding and readable native error messages.
- Added automatic in-app data refresh, scan animation, duplicate-record cleanup, and repeat optimize/restore cycles.

## How Space Reduction Works

ShrinkOS scans each selected application, classifies file content, samples larger files, and selects an efficient Zstandard level. Already-compressed or high-entropy files are kept in their original form. Reported savings equal the original file size minus the stored size. After verification succeeds, the original application bundle is removed so the compressed vault produces real disk-space reduction. Restore recreates the original bundle and makes it eligible for optimization again.

## Compression Architecture

1. Analyze the selected `.app` bundle and classify its files.
2. Sample larger files and estimate entropy and compressibility.
3. Benchmark Zstandard levels 1, 3, 6, and 9 on representative samples.
4. Compare the selected result with the configured baseline and store only the smaller safe representation.
5. Record size, ratio, level, timings, and strategy in the manifest.
6. Verify every stored file with Blake3 hashes, then remove the original bundle.
7. Restore files to the original path on request; the same app can then be optimized repeatedly.

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

In the native ShrinkOS window: approve App Management in macOS System Settings when prompted, then scan, analyze, optimize, verify integrity, and restore the test copy. Restored applications appear separately and can be optimized again. The scan checks `/Applications`, nested application folders, `/System/Applications`, and `~/Applications`.

Build the macOS application with:

```sh
npm run tauri build
```

Output: `src-tauri/target/release/bundle/macos/ShrinkOS.app`

## Next Steps

Automatic background restore when ShrinkOS is closed, signed self-updates, chunking, deduplication, cloud storage, Windows support, and filesystem virtualization are outside the current MVP.
