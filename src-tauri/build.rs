fn main() {
    // Tell cargo to re-run this build script if src-tauri/Cargo.toml changes.
    println!("cargo:rerun-if-changed=Cargo.toml");
    tauri_build::build()
}
