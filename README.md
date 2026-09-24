# AppVault

AppVault is a macOS Tauri application that compresses application files into a local vault and restores them after integrity verification.

## Version Changes

### 0.1.0

- Added native macOS application discovery, analysis, optimization, verification, and restore.
- Added real Zstandard compression with measured savings and Blake3 integrity checks.
- Added Tauri desktop bundling and corrected native IPC state registration.

## How Space Reduction Works

AppVault scans each selected application, compresses eligible files with Zstandard, and stores the compressed files in its vault. Already-compressed files are kept in their original form. Reported savings equal the original file size minus the stored size. The original application is never replaced during optimization.

## Run and Test

```sh
npm install
npm run desktop
```

For a safe test copy:

```sh
mkdir -p "$HOME/Applications"
cp -R "/Applications/The Unarchiver.app" "$HOME/Applications/AppVault Test.app"
```

In the native AppVault window: scan, analyze, optimize, verify integrity, then restore the test copy. The scan checks `/Applications`, nested application folders, `/System/Applications`, and `~/Applications`.

Build the macOS application with:

```sh
npm run tauri build
```

Output: `src-tauri/target/release/bundle/macos/AppVault.app`

## Next Steps

Complete the native end-to-end workflow test, then improve failure recovery and release packaging. Chunking, deduplication, cloud storage, Windows support, and filesystem virtualization are outside the current MVP.
