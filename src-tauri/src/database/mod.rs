/// SQLite database layer for AppVault.
///
/// Uses rusqlite with bundled SQLite (no external dependency).
/// All schema changes MUST go through the migration system.
/// Schema changes that destroy existing data are NOT acceptable.

use crate::error::{AppError, AppResult};
use crate::core::manifests::{AppManifest, AppStatus};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;
use tracing::{debug, info};

// ── Migration SQL ─────────────────────────────────────────────────────────

/// Schema migrations — append-only.
/// Each string is applied in order; version = index + 1.
const MIGRATIONS: &[&str] = &[
    // V1: Initial schema
    r#"
    CREATE TABLE IF NOT EXISTS schema_version (
        version INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS applications (
        app_id          TEXT PRIMARY KEY NOT NULL,
        name            TEXT NOT NULL,
        version         TEXT,
        platform        TEXT NOT NULL,
        original_path   TEXT NOT NULL,
        vault_path      TEXT NOT NULL,
        original_size   INTEGER NOT NULL DEFAULT 0,
        stored_size     INTEGER NOT NULL DEFAULT 0,
        cached_size     INTEGER NOT NULL DEFAULT 0,
        space_saved     INTEGER NOT NULL DEFAULT 0,
        compression_ratio REAL NOT NULL DEFAULT 1.0,
        compression     TEXT NOT NULL DEFAULT 'zstd',
        file_count      INTEGER NOT NULL DEFAULT 0,
        compatibility   TEXT NOT NULL DEFAULT 'SAFE',
        status          TEXT NOT NULL DEFAULT 'unmanaged',
        manifest_json   TEXT NOT NULL,
        created_at      TEXT NOT NULL,
        last_accessed   TEXT NOT NULL,
        optimized_at    TEXT
    );

    CREATE INDEX IF NOT EXISTS idx_apps_status ON applications(status);
    CREATE INDEX IF NOT EXISTS idx_apps_platform ON applications(platform);

    CREATE TABLE IF NOT EXISTS operation_log (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        app_id      TEXT,
        operation   TEXT NOT NULL,
        status      TEXT NOT NULL,
        message     TEXT,
        started_at  TEXT NOT NULL,
        finished_at TEXT
    );

    CREATE TABLE IF NOT EXISTS settings (
        key     TEXT PRIMARY KEY NOT NULL,
        value   TEXT NOT NULL
    );
    "#,
];

// ── Database handle ───────────────────────────────────────────────────────

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the AppVault database at `vault_path/metadata/appvault.db`.
    pub fn open(vault_path: &Path) -> AppResult<Self> {
        let meta_dir = vault_path.join("metadata");
        std::fs::create_dir_all(&meta_dir)
            .map_err(|e| AppError::Io(format!("create metadata dir: {e}")))?;

        let db_path = meta_dir.join("appvault.db");
        info!(db_path = %db_path.display(), "Opening database");

        let conn = Connection::open(&db_path)
            .map_err(|e| AppError::Database(format!("open: {e}")))?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| AppError::Database(format!("PRAGMA: {e}")))?;

        let db = Database {
            conn: Mutex::new(conn),
        };

        db.run_migrations()?;
        Ok(db)
    }

    // ── Migrations ────────────────────────────────────────────────

    fn run_migrations(&self) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();

        // Determine current schema version.
        let version: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let current = version as usize;
        info!(current_version = current, total = MIGRATIONS.len(), "Running database migrations");

        for (i, migration) in MIGRATIONS.iter().enumerate() {
            if i < current {
                continue; // already applied
            }
            conn.execute_batch(migration)
                .map_err(|e| AppError::Database(format!("migration {}: {e}", i + 1)))?;
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![i as i64 + 1],
            )
            .map_err(|e| AppError::Database(format!("schema version insert: {e}")))?;
            info!(version = i + 1, "Migration applied");
        }
        Ok(())
    }

    // ── Application CRUD ──────────────────────────────────────────

    pub fn upsert_application(&self, manifest: &AppManifest) -> AppResult<()> {
        let json = serde_json::to_string(manifest)
            .map_err(|e| AppError::Database(format!("serialize manifest: {e}")))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO applications
               (app_id, name, version, platform, original_path, vault_path,
                original_size, stored_size, cached_size, space_saved,
                compression_ratio, compression, file_count, compatibility,
                status, manifest_json, created_at, last_accessed, optimized_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)
               ON CONFLICT(app_id) DO UPDATE SET
                 name=excluded.name,
                 version=excluded.version,
                 original_size=excluded.original_size,
                 stored_size=excluded.stored_size,
                 cached_size=excluded.cached_size,
                 space_saved=excluded.space_saved,
                 compression_ratio=excluded.compression_ratio,
                 compression=excluded.compression,
                 file_count=excluded.file_count,
                 compatibility=excluded.compatibility,
                 status=excluded.status,
                 manifest_json=excluded.manifest_json,
                 last_accessed=excluded.last_accessed,
                 optimized_at=excluded.optimized_at"#,
            params![
                manifest.app_id,
                manifest.name,
                manifest.version,
                manifest.platform,
                manifest.original_path,
                manifest.vault_path,
                manifest.original_size as i64,
                manifest.stored_size as i64,
                manifest.cached_size as i64,
                manifest.space_saved as i64,
                manifest.compression_ratio,
                manifest.compression.to_string(),
                manifest.file_count as i64,
                manifest.compatibility.to_string(),
                serde_json::to_string(&manifest.status)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
                json,
                manifest.created_at.to_rfc3339(),
                manifest.last_accessed.to_rfc3339(),
                manifest.optimized_at.map(|t| t.to_rfc3339()),
            ],
        )
        .map_err(|e| AppError::Database(format!("upsert application: {e}")))?;

        debug!(app_id = %manifest.app_id, "Application upserted");
        Ok(())
    }

    pub fn get_all_applications(&self) -> AppResult<Vec<AppManifest>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT manifest_json, status FROM applications ORDER BY name")
            .map_err(|e| AppError::Database(format!("prepare: {e}")))?;

        let manifests: Result<Vec<AppManifest>, _> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::Database(format!("query: {e}")))?
            .map(|r| {
                let (json, status) = r.map_err(|e| AppError::Database(e.to_string()))?;
                let mut manifest = serde_json::from_str::<AppManifest>(&json)
                    .map_err(|e| AppError::Database(format!("deserialize: {e}")))?;
                manifest.status = serde_json::from_str(&format!("\"{status}\""))
                    .map_err(|e| AppError::Database(format!("deserialize status: {e}")))?;
                Ok(manifest)
            })
            .collect();

        manifests
    }

    pub fn get_application(&self, app_id: &str) -> AppResult<Option<AppManifest>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT manifest_json, status FROM applications WHERE app_id = ?1",
            params![app_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );

        match result {
            Ok(json) => {
                let (json, status) = json;
                let mut manifest = serde_json::from_str::<AppManifest>(&json)
                    .map_err(|e| AppError::Database(format!("deserialize: {e}")))?;
                manifest.status = serde_json::from_str(&format!("\"{status}\""))
                    .map_err(|e| AppError::Database(format!("deserialize status: {e}")))?;
                Ok(Some(manifest))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    pub fn update_status(&self, app_id: &str, status: AppStatus) -> AppResult<()> {
        let status_str = serde_json::to_string(&status)
            .map_err(|e| AppError::Database(format!("serialize status: {e}")))?
            .trim_matches('"')
            .to_string();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE applications SET status = ?1 WHERE app_id = ?2",
            params![status_str, app_id],
        )
        .map_err(|e| AppError::Database(format!("update status: {e}")))?;
        Ok(())
    }

    pub fn delete_application(&self, app_id: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM applications WHERE app_id = ?1",
            params![app_id],
        )
        .map_err(|e| AppError::Database(format!("delete: {e}")))?;
        Ok(())
    }

    // ── Settings ──────────────────────────────────────────────────

    pub fn get_setting(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let result = conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
        .map_err(|e| AppError::Database(format!("set setting: {e}")))?;
        Ok(())
    }

    // ── Operation log ─────────────────────────────────────────────

    pub fn log_operation(
        &self,
        app_id: Option<&str>,
        operation: &str,
        status: &str,
        message: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO operation_log
               (app_id, operation, status, message, started_at)
               VALUES (?1, ?2, ?3, ?4, datetime('now'))"#,
            params![app_id, operation, status, message],
        )
        .map_err(|e| AppError::Database(format!("log operation: {e}")))?;
        Ok(())
    }

    // ── Aggregate stats ───────────────────────────────────────────

    pub fn vault_totals(&self) -> AppResult<VaultTotals> {
        let conn = self.conn.lock().unwrap();
        let row = conn.query_row(
            r#"SELECT
                COUNT(*) as app_count,
                COALESCE(SUM(original_size), 0) as total_original,
                COALESCE(SUM(stored_size), 0) as total_stored,
                COALESCE(SUM(space_saved), 0) as total_saved
               FROM applications
               WHERE status = 'managed'"#,
            [],
            |row| {
                Ok(VaultTotals {
                    managed_app_count: row.get::<_, i64>(0)? as u64,
                    total_original_bytes: row.get::<_, i64>(1)? as u64,
                    total_stored_bytes: row.get::<_, i64>(2)? as u64,
                    total_saved_bytes: row.get::<_, i64>(3)? as u64,
                })
            },
        )
        .map_err(|e| AppError::Database(format!("vault totals: {e}")))?;
        Ok(row)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VaultTotals {
    pub managed_app_count: u64,
    pub total_original_bytes: u64,
    pub total_stored_bytes: u64,
    pub total_saved_bytes: u64,
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_test_db() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        (db, dir)
    }

    #[test]
    fn migrations_run_cleanly() {
        let (_db, _dir) = open_test_db();
        // If we got here, migrations succeeded.
    }

    #[test]
    fn upsert_and_retrieve_application() {
        let (db, _dir) = open_test_db();
        let manifest = AppManifest::new(
            "TestApp".into(),
            "/Applications/Test.app".into(),
            "/vault/test".into(),
            "macos".into(),
        );
        db.upsert_application(&manifest).unwrap();

        let retrieved = db.get_application(&manifest.app_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "TestApp");
    }

    #[test]
    fn settings_roundtrip() {
        let (db, _dir) = open_test_db();
        db.set_setting("compression_level", "5").unwrap();
        let val = db.get_setting("compression_level").unwrap();
        assert_eq!(val, Some("5".to_string()));
    }

    #[test]
    fn vault_totals_empty() {
        let (db, _dir) = open_test_db();
        let totals = db.vault_totals().unwrap();
        assert_eq!(totals.managed_app_count, 0);
    }
}
