// AppVault library crate root
// Coordinates: Tauri app, commands, core engine, database, logging

pub mod commands;
pub mod core;
pub mod database;
pub mod error;
pub mod platform;
pub mod security;
pub mod services;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use tracing::info;

pub use error::{AppError, AppResult};

/// Application-level shared state passed to every Tauri command.
pub struct AppState {
    pub db: Arc<database::Database>,
    pub vault_path: std::path::PathBuf,
    pub config: services::config::Config,
}

/// Library entry point — called from main.rs.
pub fn run() {
    // Initialise structured logging before anything else.
    logging::init();

    info!(version = env!("CARGO_PKG_VERSION"), "ShrinkOS starting");

    tauri::Builder::default()
        // ── Plugins ────────────────────────────────────────────────
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        // ── Setup hook — runs before any window is shown ───────────
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Determine vault storage path (platform-specific).
            let vault_path = platform::vault_storage_path(&app_handle)?;
            std::fs::create_dir_all(&vault_path)?;

            // Initialize database.
            let db = Arc::new(database::Database::open(&vault_path)?);

            // Load or create config.
            let config = services::config::Config::load_or_default(&vault_path)?;

            app.manage(Mutex::new(AppState {
                db: Arc::clone(&db),
                vault_path: vault_path.clone(),
                config,
            }));

            app.manage(Arc::new(Mutex::new(HashMap::<String, services::optimizer::OptimizationProgress>::new())));
            app.manage(Arc::new(Mutex::new(HashMap::<String, tokio::sync::watch::Sender<bool>>::new())));

            info!(vault_path = %vault_path.display(), "AppVault state initialised");
            Ok(())
        })
        // ── Tauri Commands ─────────────────────────────────────────
        .invoke_handler(tauri::generate_handler![
            // System info
            commands::system::get_system_info,
            commands::system::get_disk_stats,
            commands::system::get_permission_status,
            commands::system::open_permission_settings,
            // Application management
            commands::apps::discover_applications,
            commands::apps::analyze_application,
            commands::apps::get_managed_applications,
            commands::apps::get_application_detail,
            // Optimization
            commands::optimize::start_optimization,
            commands::optimize::get_optimization_progress,
            commands::optimize::cancel_optimization,
            // Restore
            commands::restore::restore_application,
            commands::restore::get_restore_progress,
            // Settings
            commands::settings::get_settings,
            commands::settings::update_settings,
            // Vault
            commands::vault::get_vault_stats,
            commands::vault::verify_vault_integrity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AppVault");
}

// ── Logging ────────────────────────────────────────────────────────────────

mod logging {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    pub fn init() {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("appvault=info,warn"));

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().compact())
            .init();
    }
}
