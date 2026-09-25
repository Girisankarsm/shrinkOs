# ShrinkOS

ShrinkOS is a native macOS Tauri application that reduces application storage locally. It stores a verified compressed copy in a private vault, removes the original application only after verification, and restores the application to its original path on request.

## Download and Install

Download the installer for your Mac with one click:

- [Download for Apple silicon](https://github.com/Girisankarsm/shrinkOs/releases/download/v0.1.3/ShrinkOS_0.1.3_aarch64.dmg)
- [Download for Intel](https://github.com/Girisankarsm/shrinkOs/releases/download/v0.1.3/ShrinkOS_0.1.3_x64.dmg)
- [View all GitHub Releases](https://github.com/Girisankarsm/shrinkOs/releases)

Open the DMG and drag **ShrinkOS** into **Applications**, then launch it from
Launchpad or Finder. ShrinkOS supports macOS 11.0 and later on Apple silicon
and Intel Macs.

## How to Use ShrinkOS

1. Launch the native app, not the browser preview.
2. Approve **App Management** in macOS System Settings when prompted. This lets ShrinkOS update and restore applications with your approval.
3. Open **Applications** and select **Scan for Applications**.
4. Select an eligible application and review its analysis.
5. Confirm optimization. ShrinkOS compresses and verifies the vault copy before removing the original bundle.
6. Open **Restore** to restore a managed application. Restored applications are listed separately and can be optimized again.

Scan results persist across navigation and app restarts. Native statistics refresh automatically while ShrinkOS is open.

## Previous Version vs Current Version

The earlier MVP proved the Rust compression engine and basic Tauri IPC flow. It created manifests and compressed vault files, but the desktop workflow had duplicate records, stale scan state, weak native error display, and no reliable repeat optimize/restore lifecycle.

The current version adds:

- Native macOS application discovery, analysis, optimization, verification, and restore.
- App Management permission onboarding with a direct System Settings link.
- Clear structured native error messages instead of `[object Object]`.
- Real disk reduction: the original bundle is removed only after every stored file passes verification.
- Restore state that is persisted correctly and shown separately from active managed apps.
- Repeat cycles: optimize, restore, optimize again without duplicate records blocking the app.
- Startup cleanup for stale duplicate records, preferring the restored state.
- Persistent scan results, automatic data refresh, and a minimal scan animation.
- Adaptive compression benchmarking and a real macOS application smoke test.

## How Shrinking Works

For each selected `.app` bundle, ShrinkOS:

1. Walks the application file tree and records file sizes and content types.
2. Samples larger files from the beginning, middle, and end of the file.
3. Estimates entropy and runs a quick compressibility probe.
4. Benchmarks candidate Zstandard levels and scores the savings against compression and decompression cost.
5. Compresses the complete file using the selected level.
6. Keeps the original bytes when compression would make a file larger or save too little.
7. Writes the result to the local vault and records the algorithm, level, sizes, timings, and hashes.
8. Decompresses every stored file and compares its Blake3 hash with the original.
9. Removes the original application bundle only after all verification succeeds.

`Space saved = original application size - verified stored size`.

The vault is local. ShrinkOS does not upload application contents or use cloud storage.

## Compression Algorithms and Strategies

### Zstandard (Zstd)

Zstd is the default and primary algorithm. It provides a strong balance between compression ratio and speed, which makes it suitable for large application bundles and repeated optimize/restore cycles.

ShrinkOS benchmarks levels **1, 3, 6, and 9** on representative samples. Lower levels are faster; higher levels may save more space but take longer. The adaptive engine chooses the best measured trade-off and compares it with the configured baseline before storing the result.

### Raw storage (`None`)

Some files are already compressed, encrypted, or high entropy: images, videos, archives, binaries, and packaged assets are common examples. If Zstd would increase the size or provide negligible savings, ShrinkOS stores the original bytes unchanged and records `None` for that file.

This avoids wasting CPU time and prevents “compression” from increasing storage usage.

### Why file-by-file compression?

Application bundles contain mixed content. Compressing each file independently lets ShrinkOS compress text, metadata, scripts, and resources effectively while leaving unsuitable files untouched. The manifest also allows every file to be verified and restored independently.

Chunking, deduplication, filesystem virtualization, cloud storage, Windows support, signed self-updates, and automatic restore while ShrinkOS is closed are not part of the current MVP.

## Development

```sh
npm install
npm run desktop
```

Build the macOS application:

```sh
npm run tauri build
```

Outputs:

- `src-tauri/target/release/bundle/macos/ShrinkOS.app`
- `src-tauri/target/release/bundle/dmg/ShrinkOS_<version>_<architecture>.dmg`

Pushing a version tag such as `v0.1.3` runs the GitHub Actions release workflow,
builds Apple silicon and Intel DMGs, and attaches them to a GitHub Release.

Run tests:

```sh
cd src-tauri
cargo test
```

The test suite includes compression, database, integrity, and a real macOS `.app` optimize/verify/restore smoke test.

For a safe fixture copy:

```sh
mkdir -p "$HOME/Applications"
cp -R "/Applications/The Unarchiver.app" "$HOME/Applications/ShrinkOS Test.app"
```
