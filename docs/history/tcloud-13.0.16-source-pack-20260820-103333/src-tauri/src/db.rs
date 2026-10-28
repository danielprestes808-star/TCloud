use tauri::{AppHandle, Manager};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub type DbConnection = Arc<Mutex<sqlite::Connection>>;

pub fn last_insert_rowid(conn: &sqlite::Connection) -> Result<i64, String> {
    let mut stmt = conn
        .prepare("SELECT last_insert_rowid()")
        .map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => stmt.read::<i64, _>(0).map_err(|e| e.to_string()),
        _ => Err("SQLite did not return last_insert_rowid()".to_string()),
    }
}

/// Maximum number of retry attempts for database initialization
const MAX_DB_INIT_RETRIES: u32 = 5;

const STABILITY_MIGRATION_BACKUP_REVISION: &str = "12.9.7-sync-reliability";
const STABILITY_BACKUP_RETENTION: usize = 5;

fn ensure_stability_install_id(app_data_dir: &std::path::Path, recovery_dir: &std::path::Path) -> Result<String, String> {
    std::fs::create_dir_all(recovery_dir).map_err(|error| error.to_string())?;
    let app_id_path = app_data_dir.join("stability-install-id.txt");
    let recovery_id_path = recovery_dir.join("stability-install-id.txt");
    for source in [&app_id_path, &recovery_id_path] {
        if let Ok(existing) = std::fs::read_to_string(source) {
            let trimmed = existing.trim();
            if !trimmed.is_empty() {
                let value = trimmed.to_string();
                let _ = std::fs::write(&app_id_path, format!("{}\n", value));
                let _ = std::fs::write(&recovery_id_path, format!("{}\n", value));
                return Ok(value);
            }
        }
    }
    let nanos = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let id = format!("{:x}-{}-{:016x}", nanos, std::process::id(), rand::random::<u64>());
    std::fs::write(&app_id_path, format!("{}\n", id)).map_err(|error| error.to_string())?;
    std::fs::write(&recovery_id_path, format!("{}\n", id)).map_err(|error| error.to_string())?;
    Ok(id)
}

fn verify_sqlite_backup(path: &std::path::Path) -> Result<(), String> {
    let backup = sqlite::open(path).map_err(|error| error.to_string())?;
    let mut statement = backup.prepare("PRAGMA quick_check(1)").map_err(|error| error.to_string())?;
    if matches!(statement.next().map_err(|error| error.to_string())?, sqlite::State::Row) {
        let status = statement.read::<String, _>(0).map_err(|error| error.to_string())?;
        if status.eq_ignore_ascii_case("ok") { return Ok(()); }
        return Err(format!("SQLite backup quick_check returned: {}", status));
    }
    Err("SQLite backup quick_check did not return a result".to_string())
}

fn prune_stability_backups(directory: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(directory) else { return; };
    let mut files = entries.filter_map(Result::ok).filter_map(|entry| {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("db") { return None; }
        let modified = entry.metadata().ok()?.modified().ok()?;
        Some((modified, path))
    }).collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    while files.len() > STABILITY_BACKUP_RETENTION {
        let (_, path) = files.remove(0);
        if let Err(error) = std::fs::remove_file(&path) {
            log::warn!("Could not prune old TCloud stability backup {}: {}", path.display(), error);
        }
    }
}

fn create_pre_migration_backup(conn: &sqlite::Connection, db_path: &std::path::Path, recovery_dir: &std::path::Path, database_preexisted: bool) -> Result<Option<std::path::PathBuf>, String> {
    if !database_preexisted || !db_path.is_file() { return Ok(None); }
    std::fs::create_dir_all(recovery_dir).map_err(|error| error.to_string())?;
    let marker = recovery_dir.join(format!("stability-backup-{}.done", STABILITY_MIGRATION_BACKUP_REVISION));
    if marker.is_file() {
        let recorded = std::fs::read_to_string(&marker).unwrap_or_default();
        if std::path::Path::new(recorded.trim()).is_file() { return Ok(None); }
        let _ = std::fs::remove_file(&marker);
    }
    conn.execute("PRAGMA wal_checkpoint(FULL)").map_err(|error| error.to_string())?;
    let backup_dir = recovery_dir.join("stability-backups");
    std::fs::create_dir_all(&backup_dir).map_err(|error| error.to_string())?;
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let backup_path = backup_dir.join(format!("shares-pre-{}-{}.db", STABILITY_MIGRATION_BACKUP_REVISION, stamp));
    std::fs::copy(db_path, &backup_path).map_err(|error| error.to_string())?;
    if let Err(error) = verify_sqlite_backup(&backup_path) {
        let _ = std::fs::remove_file(&backup_path);
        return Err(format!("pre-migration backup verification failed: {}", error));
    }
    std::fs::write(&marker, format!("{}\n", backup_path.display())).map_err(|error| error.to_string())?;
    prune_stability_backups(&backup_dir);
    Ok(Some(backup_path))
}

pub fn init_db(app: &AppHandle) -> Result<DbConnection, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let db_path = dir.join("shares.db");
    let database_preexisted = db_path.is_file();
    let recovery_dir = dirs::data_local_dir().unwrap_or_else(|| dir.clone()).join("TCloudRecovery");
    let stability_install_id = match ensure_stability_install_id(&dir, &recovery_dir) {
        Ok(id) => id,
        Err(error) => {
            log::warn!("Could not prepare the persistent TCloud install id: {}", error);
            format!("temporary-{}-{:016x}", std::process::id(), rand::random::<u64>())
        }
    };
    
    // Retry opening the database with exponential backoff.
    // SQLite may report "database is locked" if another process or a stale
    // wal/shm journal hasn't been cleaned up yet (e.g., after a crash).
    let conn = {
        let mut last_err = String::new();
        let mut opened = None;
        for attempt in 0..MAX_DB_INIT_RETRIES {
            match sqlite::open(&db_path) {
                Ok(c) => {
                    opened = Some(c);
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < MAX_DB_INIT_RETRIES - 1 {
                        let wait_ms = 100 * 2u64.pow(attempt);
                        log::warn!(
                            "Failed to open SQLite database (attempt {}/{}): {}. Retrying in {}ms...",
                            attempt + 1, MAX_DB_INIT_RETRIES, last_err, wait_ms
                        );
                        std::thread::sleep(Duration::from_millis(wait_ms));
                    }
                }
            }
        }
        opened.ok_or_else(|| {
            format!(
                "Failed to open SQLite database after {} attempts: {}",
                MAX_DB_INIT_RETRIES, last_err
            )
        })?
    };

    // TCloud 12.7.1 — favor responsive local reads while background sync is busy.
    // WAL keeps readers from waiting on short metadata writes and NORMAL remains
    // crash-safe for WAL while avoiding a full fsync on every tiny index update.
    let _ = conn.execute(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000; PRAGMA temp_store=MEMORY;",
    );
    match create_pre_migration_backup(&conn, &db_path, &recovery_dir, database_preexisted) {
        Ok(Some(path)) => log::info!("TCloud pre-migration database backup created: {}", path.display()),
        Ok(None) => {}
        Err(error) => log::warn!("TCloud pre-migration backup was not created: {}", error),
    }
    
    // Run migration (also with retry for locked-database scenarios)
    {
        let mut last_err = String::new();
        for attempt in 0..MAX_DB_INIT_RETRIES {
            match conn.execute(
                "CREATE TABLE IF NOT EXISTS shared_links (
                    id TEXT PRIMARY KEY,
                    folder_id INTEGER,
                    message_id INTEGER NOT NULL,
                    file_name TEXT NOT NULL,
                    file_size INTEGER NOT NULL DEFAULT 0,
                    password_hash TEXT,
                    password_salt TEXT,
                    expires_at INTEGER,
                    revoked INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    max_downloads INTEGER,
                    download_count INTEGER NOT NULL DEFAULT 0,
                    last_access_at INTEGER
                );
                CREATE TABLE IF NOT EXISTS groups (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    color_hex TEXT DEFAULT '#3B82F6',
                    display_order INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE IF NOT EXISTS folder_metadata (
                    channel_id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    username TEXT,
                    is_public INTEGER NOT NULL DEFAULT 0,
                    display_order INTEGER NOT NULL DEFAULT 0,
                    group_id INTEGER,
                    FOREIGN KEY(group_id) REFERENCES groups(id) ON DELETE SET NULL
                );"
            ) {
                Ok(_) => {
                    last_err.clear();
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < MAX_DB_INIT_RETRIES - 1 {
                        let wait_ms = 100 * 2u64.pow(attempt);
                        log::warn!(
                            "Failed to run SQLite migration (attempt {}/{}): {}. Retrying in {}ms...",
                            attempt + 1, MAX_DB_INIT_RETRIES, last_err, wait_ms
                        );
                        std::thread::sleep(Duration::from_millis(wait_ms));
                    }
                }
            }
        }
        if !last_err.is_empty() {
            return Err(format!(
                "Failed to run SQLite migration after {} attempts: {}",
                MAX_DB_INIT_RETRIES, last_err
            ));
        }
    }

    // TCloud 10 share analytics / download limit migration for existing databases.
    let _ = conn.execute("ALTER TABLE shared_links ADD COLUMN max_downloads INTEGER");
    let _ = conn.execute("ALTER TABLE shared_links ADD COLUMN download_count INTEGER NOT NULL DEFAULT 0");
    let _ = conn.execute("ALTER TABLE shared_links ADD COLUMN last_access_at INTEGER");

    // TCloud storage index migration. This metadata layer is independent of
    // Telegram messages so the local drive can track folders, hashes and sync state.
    {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tcloud_folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                parent_id INTEGER REFERENCES tcloud_folders(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                telegram_chat_id INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_folders_parent ON tcloud_folders(parent_id);
            CREATE TABLE IF NOT EXISTS tcloud_files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER REFERENCES tcloud_folders(id) ON DELETE SET NULL,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                size INTEGER NOT NULL DEFAULT 0,
                mime_type TEXT,
                sha256 TEXT,
                telegram_chat_id INTEGER,
                telegram_message_id INTEGER,
                telegram_file_id TEXT,
                sync_status TEXT NOT NULL DEFAULT 'pending',
                local_status TEXT NOT NULL DEFAULT 'online',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_files_folder ON tcloud_files(folder_id);
            CREATE INDEX IF NOT EXISTS idx_tcloud_files_sha256 ON tcloud_files(sha256);
            CREATE TABLE IF NOT EXISTS tcloud_sync_jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES tcloud_files(id) ON DELETE CASCADE,
                operation TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_sync_jobs_status ON tcloud_sync_jobs(status, created_at);
            CREATE TABLE IF NOT EXISTS tcloud_sync_conflicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_id INTEGER NOT NULL REFERENCES tcloud_files(id) ON DELETE CASCADE,
                local_hash TEXT,
                remote_hash TEXT,
                resolution TEXT NOT NULL DEFAULT 'pending',
                created_at INTEGER NOT NULL,
                resolved_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS tcloud_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT
            );

            CREATE TABLE IF NOT EXISTS tcloud_cache (
                file_id INTEGER PRIMARY KEY REFERENCES tcloud_files(id) ON DELETE CASCADE,
                local_path TEXT NOT NULL,
                cached_at INTEGER NOT NULL,
                last_accessed_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_cache_last_accessed
                ON tcloud_cache(last_accessed_at);",
        ).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO tcloud_folders(id,name,path,created_at,updated_at) VALUES(1,'TCloud','/',strftime('%s','now'),strftime('%s','now'))",
        ).map_err(|e| e.to_string())?;
        let app_version = env!("CARGO_PKG_VERSION");
        conn.execute(format!(
            "INSERT INTO tcloud_settings(key,value) VALUES('stability_install_id','{stability_install_id}') \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value; \
             INSERT INTO tcloud_settings(key,value) VALUES('stability_last_app_version','{app_version}') \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value; \
             INSERT INTO tcloud_settings(key,value) VALUES('stability_migration_revision','{STABILITY_MIGRATION_BACKUP_REVISION}') \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value;"
        )).map_err(|e| e.to_string())?;
    }

    // TCloud 7.0 — Telegram forum storage.
    {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tcloud_forums (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                telegram_chat_id INTEGER NOT NULL UNIQUE,
                telegram_access_hash INTEGER NOT NULL DEFAULT 0,
                name TEXT NOT NULL,
                is_default INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_forums_default ON tcloud_forums(is_default);
            CREATE TABLE IF NOT EXISTS tcloud_forum_topics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                forum_id INTEGER NOT NULL REFERENCES tcloud_forums(id) ON DELETE CASCADE,
                telegram_topic_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(forum_id, telegram_topic_id)
            );
            CREATE INDEX IF NOT EXISTS idx_tcloud_forum_topics_forum ON tcloud_forum_topics(forum_id);"
        ).map_err(|e| e.to_string())?;
    }

    // TCloud 8.0 — productivity suite (trash, versions, automation, backups, offline pins).
    {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tcloud_trash (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id INTEGER NOT NULL,
                folder_id INTEGER,
                name TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                deleted_at INTEGER NOT NULL,
                UNIQUE(message_id, folder_id)
            );
            CREATE TABLE IF NOT EXISTS tcloud_versions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id INTEGER NOT NULL,
                folder_id INTEGER,
                name TEXT NOT NULL,
                size INTEGER NOT NULL DEFAULT 0,
                note TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tcloud_auto_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                extensions TEXT NOT NULL DEFAULT '',
                target_folder_id INTEGER,
                enabled INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS tcloud_backup_sources (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                local_path TEXT NOT NULL UNIQUE,
                target_folder_id INTEGER,
                enabled INTEGER NOT NULL DEFAULT 1,
                interval_minutes INTEGER NOT NULL DEFAULT 1440,
                last_run_at INTEGER,
            last_scan_at INTEGER,
            last_success_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS tcloud_backup_state (
                backup_id INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                modified_at INTEGER NOT NULL DEFAULT 0,
                uploaded_at INTEGER NOT NULL,
                PRIMARY KEY(backup_id, file_path)
            );
            CREATE TABLE IF NOT EXISTS tcloud_offline_pins (
                file_id INTEGER PRIMARY KEY,
                pinned_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tcloud_index_meta (
                key TEXT PRIMARY KEY,
                value TEXT
            );"
        ).map_err(|e| e.to_string())?;
    }

    // TCloud backup runtime timestamps.
    {
        let _ = conn.execute(
            "ALTER TABLE tcloud_backup_sources ADD COLUMN last_scan_at INTEGER"
        );

        let _ = conn.execute(
            "ALTER TABLE tcloud_backup_sources ADD COLUMN last_success_at INTEGER"
        );

        let _ = conn.execute(
            "UPDATE tcloud_backup_sources
             SET last_scan_at=last_run_at
             WHERE last_scan_at IS NULL
               AND last_run_at IS NOT NULL"
        );
    }
    // TCloud transfer progress columns (best-effort compatibility migration).
    {
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN bytes_total INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN bytes_transferred INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN started_at INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN speed_bps INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN eta_seconds INTEGER");
    }

    // TCloud transfer controls / scheduling (compatibility migration).
    {
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN paused INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN priority INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN pause_scope TEXT");
        // TCloud 12.3 — durable Explorer -> Telegram retry/backoff metadata.
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN next_attempt_at INTEGER");
        // TCloud 12.9.7 — crash-safe remote commit checkpoint. Once Telegram
        // confirms a message, persist its immutable id before any cleanup/final
        // UI bookkeeping. A process crash can then finish the same job on restart
        // instead of uploading a duplicate remote object.
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN remote_commit_message_id INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN remote_commit_fingerprint TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN remote_commit_sha256 TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_sync_jobs ADD COLUMN remote_commit_at INTEGER");
        // Fast local change detection. This deliberately lives outside `sha256` so
        // large files do not need to be hashed just to debounce Explorer events.
        let _ = conn.execute("ALTER TABLE tcloud_files ADD COLUMN local_fingerprint TEXT");
        // TCloud 12.5 — mature sync policies, real trash, automatic versions and richer conflicts.
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS tcloud_folder_sync_policies (relative_path TEXT PRIMARY KEY, mode TEXT NOT NULL DEFAULT 'normal', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)");
        let _ = conn.execute("CREATE TABLE IF NOT EXISTS tcloud_sync_preferences (id INTEGER PRIMARY KEY CHECK(id=1), version_retention INTEGER NOT NULL DEFAULT 10, trash_retention_days INTEGER NOT NULL DEFAULT 30, conflict_strategy TEXT NOT NULL DEFAULT 'ask', updated_at INTEGER NOT NULL DEFAULT 0)");
        let _ = conn.execute("INSERT OR IGNORE INTO tcloud_sync_preferences(id,version_retention,trash_retention_days,conflict_strategy,updated_at) VALUES(1,10,30,'ask',0)");
        let _ = conn.execute("ALTER TABLE tcloud_versions ADD COLUMN file_id INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_versions ADD COLUMN original_path TEXT");
        let _ = conn.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_tcloud_versions_file_message ON tcloud_versions(file_id,message_id) WHERE file_id IS NOT NULL");
        let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN file_id INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN relative_path TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN expires_at INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN status TEXT NOT NULL DEFAULT 'trashed'");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_message_id INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN file_name TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN local_path TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_name TEXT");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN local_size INTEGER");
        let _ = conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_size INTEGER");
        // 12.7.1 hot-path indexes: queue claim and cached folder projection are
        // touched constantly while the Explorer watcher and remote reconciler run.
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_sync_jobs_ready ON tcloud_sync_jobs(status,paused,cancel_requested,priority DESC,next_attempt_at,created_at)");
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_sync_jobs_updated ON tcloud_sync_jobs(updated_at DESC)");
        // 13.0.4: attention counters compare the newest attempt for the same file/operation.
        // Keep this lookup indexed because the overview is polled while the app is visible.
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_sync_jobs_file_operation ON tcloud_sync_jobs(file_id,operation,id DESC)");
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_sync_jobs_remote_commit ON tcloud_sync_jobs(status,remote_commit_message_id,updated_at)");
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_files_chat_status_created ON tcloud_files(telegram_chat_id,sync_status,created_at DESC)");
        let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_tcloud_files_remote_identity ON tcloud_files(telegram_chat_id,telegram_message_id)");
    }

    // Encryption tables migration. Run the complete migration in one explicit
    // transaction so a crash cannot leave a partially upgraded registry.
    {
        let mut last_err = String::new();
        for attempt in 0..MAX_DB_INIT_RETRIES {
            match run_encryption_migration(&conn) {
                Ok(_) => {
                    last_err.clear();
                    break;
                }
                Err(e) => {
                    last_err = e.to_string();
                    if attempt < MAX_DB_INIT_RETRIES - 1 {
                        let wait_ms = 100 * 2u64.pow(attempt);
                        log::warn!(
                            "Failed to run encryption migration (attempt {}/{}): {}. Retrying in {}ms...",
                            attempt + 1, MAX_DB_INIT_RETRIES, last_err, wait_ms
                        );
                        std::thread::sleep(Duration::from_millis(wait_ms));
                    }
                }
            }
        }
        if !last_err.is_empty() {
            return Err(format!(
                "Failed to run encryption migration after {} attempts: {}",
                MAX_DB_INIT_RETRIES, last_err
            ));
        }
    }
    
    log::info!("SQLite database initialized successfully using sqlite crate.");
    Ok(Arc::new(Mutex::new(conn)))
}

fn encryption_column_exists(
    conn: &sqlite::Connection,
    column_name: &str,
) -> Result<bool, String> {
    let mut statement = conn
        .prepare("SELECT COUNT(*) FROM pragma_table_info('encrypted_files') WHERE name = ?")
        .map_err(|error| error.to_string())?;
    statement
        .bind((1, column_name))
        .map_err(|error| error.to_string())?;
    if let sqlite::State::Row = statement.next().map_err(|error| error.to_string())? {
        return statement
            .read::<i64, _>(0)
            .map(|count| count > 0)
            .map_err(|error| error.to_string());
    }
    Ok(false)
}

fn run_encryption_migration(conn: &sqlite::Connection) -> Result<(), String> {
    conn.execute("BEGIN IMMEDIATE TRANSACTION")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS encrypted_files (
                folder_key TEXT NOT NULL,
                message_id INTEGER NOT NULL,
                file_uuid BLOB NOT NULL,
                envelope_version INTEGER NOT NULL,
                cipher_suite INTEGER NOT NULL,
                ciphertext_size INTEGER NOT NULL,
                plaintext_size INTEGER,
                remote_name TEXT NOT NULL,
                key_profile_id TEXT,
                protection_mode TEXT NOT NULL DEFAULT 'vault',
                metadata_protected INTEGER NOT NULL DEFAULT 0,
                header_blob BLOB,
                header_sha256 BLOB,
                record_state TEXT NOT NULL DEFAULT 'active',
                reconciliation_state TEXT NOT NULL DEFAULT 'ok',
                created_at INTEGER NOT NULL,
                last_verified_at INTEGER,
                PRIMARY KEY(folder_key, message_id)
            );
            CREATE TABLE IF NOT EXISTS encryption_profiles (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                kind TEXT NOT NULL,
                vault_locator TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                is_deleted INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );",
        )
        .map_err(|error| error.to_string())?;

        let additions = [
            ("plaintext_size", "INTEGER"),
            ("protection_mode", "TEXT NOT NULL DEFAULT 'vault'"),
            ("metadata_protected", "INTEGER NOT NULL DEFAULT 0"),
            ("reconciliation_state", "TEXT NOT NULL DEFAULT 'ok'"),
        ];
        for (name, declaration) in additions {
            if !encryption_column_exists(conn, name)? {
                conn.execute(format!(
                    "ALTER TABLE encrypted_files ADD COLUMN {name} {declaration}"
                ))
                .map_err(|error| error.to_string())?;
            }
        }
        conn.execute(format!(
            "INSERT OR IGNORE INTO schema_version (version, applied_at) VALUES (3, {})",
            chrono::Utc::now().timestamp()
        ))
        .map_err(|error| error.to_string())?;
        Ok(())
    })();

    match result {
        Ok(()) => conn
            .execute("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = conn.execute("ROLLBACK");
            Err(error)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StartupRecoveryReport {
    pub previous_unclean_shutdown: bool,
    pub quick_check_ok: bool,
}

/// TCloud 12.8 — mark this process as active and inspect the database only
/// when the previous process did not record a clean shutdown. This keeps the
/// common startup path fast while still validating WAL/database health after
/// Windows updates, crashes or forced staging replacements.
pub fn mark_runtime_started(db: &DbConnection) -> Result<StartupRecoveryReport, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let previous_clean = {
        let mut stmt = conn
            .prepare("SELECT value FROM tcloud_settings WHERE key='stability_clean_shutdown' LIMIT 1")
            .map_err(|e| e.to_string())?;
        if matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            stmt.read::<String, _>(0).unwrap_or_else(|_| "1".to_string()) == "1"
        } else {
            true
        }
    };

    let previous_unclean_shutdown = !previous_clean;
    let previous_db_attention = {
        let mut stmt = conn
            .prepare("SELECT value FROM tcloud_settings WHERE key='stability_db_check' LIMIT 1")
            .map_err(|e| e.to_string())?;
        matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row)
            && stmt.read::<String, _>(0).unwrap_or_default() == "attention"
    };
    let needs_check = previous_unclean_shutdown || previous_db_attention;
    let mut quick_check_ok = !previous_db_attention;
    if needs_check {
        // PASSIVE never blocks active readers/writers and is safe after an
        // interrupted process. quick_check is read-only. If a previous check
        // needed attention, keep checking on later starts until it passes.
        let _ = conn.execute("PRAGMA wal_checkpoint(PASSIVE)");
        quick_check_ok = match conn.prepare("PRAGMA quick_check(1)") {
            Ok(mut stmt) => match stmt.next() {
                Ok(sqlite::State::Row) => stmt
                    .read::<String, _>(0)
                    .unwrap_or_default()
                    .eq_ignore_ascii_case("ok"),
                _ => false,
            },
            Err(_) => false,
        };
    }

    let now = chrono::Utc::now().timestamp();
    let status = if quick_check_ok { "ok" } else { "attention" };
    conn.execute(format!(
        "INSERT INTO tcloud_settings(key,value) VALUES('stability_clean_shutdown','0') \n         ON CONFLICT(key) DO UPDATE SET value='0'; \n         INSERT INTO tcloud_settings(key,value) VALUES('stability_last_startup_at','{now}') \n         ON CONFLICT(key) DO UPDATE SET value='{now}'; \n         INSERT INTO tcloud_settings(key,value) VALUES('stability_previous_unclean','{}') \n         ON CONFLICT(key) DO UPDATE SET value='{}'; \n         INSERT INTO tcloud_settings(key,value) VALUES('stability_db_check','{status}') \n         ON CONFLICT(key) DO UPDATE SET value='{status}';",
        if previous_unclean_shutdown { "1" } else { "0" },
        if previous_unclean_shutdown { "1" } else { "0" },
    ))
    .map_err(|e| e.to_string())?;

    Ok(StartupRecoveryReport { previous_unclean_shutdown, quick_check_ok })
}

/// Record a clean application shutdown only after background services have been
/// stopped. If Windows terminates the process first, the value remains `0` and
/// the next startup automatically enters recovery mode.
pub fn mark_runtime_clean_shutdown(db: &DbConnection) -> Result<(), String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let now = chrono::Utc::now().timestamp();
    conn.execute(format!(
        "INSERT INTO tcloud_settings(key,value) VALUES('stability_clean_shutdown','1') \n         ON CONFLICT(key) DO UPDATE SET value='1'; \n         INSERT INTO tcloud_settings(key,value) VALUES('stability_last_clean_shutdown_at','{now}') \n         ON CONFLICT(key) DO UPDATE SET value='{now}';"
    ))
    .map_err(|e| e.to_string())
}

/// Lightweight housekeeping used by the long-running desktop service. It only
/// runs while no transfer is active, keeps the WAL bounded and lets SQLite
/// refresh planner statistics without interrupting Explorer operations.
pub fn run_idle_maintenance(db: &DbConnection) -> Result<bool, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let running = {
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status='running'")
            .map_err(|e| e.to_string())?;
        if matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            stmt.read::<i64, _>(0).unwrap_or(0)
        } else {
            0
        }
    };
    if running > 0 {
        return Ok(false);
    }

    let now = chrono::Utc::now().timestamp();
    let history_cutoff = now - 7 * 24 * 60 * 60;

    // Long-running clients used to accumulate every completed transfer forever.
    // Keep one week in the activity center, then prune only completed history.
    // Failed/pending jobs are intentionally preserved until the user or TCloud
    // resolves them.
    let _ = conn.execute(format!(
        "DELETE FROM tcloud_sync_jobs WHERE status='completed' AND updated_at < {history_cutoff}"
    ));

    // Cache metadata can outlive a deleted database file row after a crash or an
    // old migration. Removing those orphan rows keeps cache scans bounded.
    let _ = conn.execute(
        "DELETE FROM tcloud_cache WHERE file_id NOT IN (SELECT id FROM tcloud_files)"
    );

    let cfapi_ops_exists = {
        let mut stmt = conn
            .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='tcloud_cfapi_pending_ops' LIMIT 1")
            .map_err(|e| e.to_string())?;
        matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row)
    };
    if cfapi_ops_exists {
        let _ = conn.execute(format!(
            "DELETE FROM tcloud_cfapi_pending_ops WHERE completed_at IS NOT NULL AND completed_at < {history_cutoff}"
        ));
    }

    let _ = conn.execute("PRAGMA wal_checkpoint(PASSIVE)");
    let _ = conn.execute("PRAGMA optimize");

    // Every six hours, while completely idle, ask SQLite to return unused WAL
    // pages to Windows. Failure is harmless (for example if Explorer briefly
    // opened a read transaction); the next maintenance window will retry.
    let last_deep = {
        let mut stmt = conn
            .prepare("SELECT value FROM tcloud_settings WHERE key='stability_last_deep_maintenance_at' LIMIT 1")
            .map_err(|e| e.to_string())?;
        if matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            stmt.read::<String, _>(0)
                .ok()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0)
        } else {
            0
        }
    };
    if now.saturating_sub(last_deep) >= 6 * 60 * 60 {
        if conn.execute("PRAGMA wal_checkpoint(TRUNCATE)").is_ok() {
            let _ = conn.execute(format!(
                "INSERT INTO tcloud_settings(key,value) VALUES('stability_last_deep_maintenance_at','{now}') \
                 ON CONFLICT(key) DO UPDATE SET value='{now}'"
            ));
        }
    }

    let _ = conn.execute(format!(
        "INSERT INTO tcloud_settings(key,value) VALUES('stability_last_maintenance_at','{now}') \
         ON CONFLICT(key) DO UPDATE SET value='{now}'"
    ));
    Ok(true)
}
