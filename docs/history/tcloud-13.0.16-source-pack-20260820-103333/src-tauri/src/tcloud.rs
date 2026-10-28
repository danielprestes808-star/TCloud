//! TCloud storage index and synchronization primitives.
//! This layer deliberately stores metadata only; Telegram remains the remote blob store.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::DbConnection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudFolder {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub telegram_chat_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudFile {
    pub id: i64,
    pub folder_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub mime_type: Option<String>,
    pub sha256: Option<String>,
    pub telegram_chat_id: Option<i64>,
    pub telegram_message_id: Option<i64>,
    pub telegram_file_id: Option<String>,
    pub sync_status: String,
    pub local_status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudSyncJob {
    pub id: i64,
    pub file_id: i64,
    pub operation: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[tauri::command]
pub fn tcloud_get_storage_overview(db: State<'_, DbConnection>) -> Result<serde_json::Value, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let files: i64 = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files")?;
    let bytes: i64 = scalar_i64(&conn, "SELECT COALESCE(SUM(size), 0) FROM tcloud_files")?;
    let folders: i64 = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_folders")?;
    let pending: i64 = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status IN ('pending','running')")?;
    Ok(serde_json::json!({"files": files, "bytes": bytes, "folders": folders, "pending_sync": pending}))
}

#[tauri::command]
pub fn tcloud_list_folders(db: State<'_, DbConnection>, parent_id: Option<i64>) -> Result<Vec<TCloudFolder>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let sql = if parent_id.is_some() {
        "SELECT id, parent_id, name, path, telegram_chat_id, created_at, updated_at FROM tcloud_folders WHERE parent_id = ? ORDER BY name COLLATE NOCASE"
    } else {
        "SELECT id, parent_id, name, path, telegram_chat_id, created_at, updated_at FROM tcloud_folders WHERE parent_id IS NULL ORDER BY name COLLATE NOCASE"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    if let Some(id) = parent_id { stmt.bind((1, id)).map_err(|e| e.to_string())?; }
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(TCloudFolder {
            id: stmt.read(0).map_err(|e| e.to_string())?,
            parent_id: stmt.read(1).map_err(|e| e.to_string())?,
            name: stmt.read(2).map_err(|e| e.to_string())?,
            path: stmt.read(3).map_err(|e| e.to_string())?,
            telegram_chat_id: stmt.read(4).map_err(|e| e.to_string())?,
            created_at: stmt.read(5).map_err(|e| e.to_string())?,
            updated_at: stmt.read(6).map_err(|e| e.to_string())?,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn tcloud_create_folder(db: State<'_, DbConnection>, name: String, parent_id: Option<i64>) -> Result<TCloudFolder, String> {
    let name = name.trim().to_string();
    if name.is_empty() { return Err("Folder name cannot be empty".into()); }
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let parent_path: Option<String> = match parent_id {
        Some(id) => query_string(&conn, "SELECT path FROM tcloud_folders WHERE id = ?", id)?,
        None => None,
    };
    let path = match parent_path {
        Some(p) if p == "/" => format!("/{}", name),
        Some(p) => format!("{}/{}", p.trim_end_matches('/'), name),
        None => format!("/{}", name),
    };
    let mut stmt = conn.prepare("INSERT INTO tcloud_folders(parent_id,name,path,created_at,updated_at) VALUES(?1,?2,?3,?4,?5)").map_err(|e| e.to_string())?;
    match parent_id {
        Some(id) => stmt.bind((1, id)).map_err(|e| e.to_string())?,
        None => stmt.bind((1, sqlite::Value::Null)).map_err(|e| e.to_string())?,
    }
    stmt.bind((2, name.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, path.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((4, now)).map_err(|e| e.to_string())?;
    stmt.bind((5, now)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    let id = crate::db::last_insert_rowid(&conn)?;
    Ok(TCloudFolder { id, parent_id, name, path, telegram_chat_id: None, created_at: now, updated_at: now })
}

#[tauri::command]
pub fn tcloud_enqueue_sync(
    db: State<'_, DbConnection>,
    file_id: i64,
    operation: String,
) -> Result<i64, String> {
    let operation = operation.trim().to_lowercase();

    let priority = match operation.as_str() {
        "download" => 150,
        "upload" => 130,
        "delete" => 125,
        "refresh" => 115,
        _ => return Err("Unsupported sync operation".into()),
    };

    let conn = db
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    let now = chrono::Utc::now().timestamp();

    if matches!(operation.as_str(), "upload" | "refresh") {
        let mut existing = conn
            .prepare(
                "SELECT id
                 FROM tcloud_sync_jobs
                 WHERE file_id=?1
                   AND operation IN ('upload','refresh')
                   AND status IN ('pending','running','paused')
                   AND COALESCE(cancel_requested,0)=0
                 ORDER BY
                   CASE status
                     WHEN 'running' THEN 0
                     WHEN 'pending' THEN 1
                     ELSE 2
                   END,
                   priority DESC,
                   id DESC
                 LIMIT 1"
            )
            .map_err(|e| e.to_string())?;

        existing
            .bind((1, file_id))
            .map_err(|e| e.to_string())?;

        if let sqlite::State::Row =
            existing.next().map_err(|e| e.to_string())?
        {
            let existing_id = existing
                .read::<i64, _>(0)
                .map_err(|e| e.to_string())?;

            drop(existing);

            let mut promote = conn
                .prepare(
                    "UPDATE tcloud_sync_jobs
                     SET priority=MAX(COALESCE(priority,0),?1),
                         next_attempt_at=NULL,
                         updated_at=?2
                     WHERE id=?3"
                )
                .map_err(|e| e.to_string())?;

            promote
                .bind((1, priority))
                .map_err(|e| e.to_string())?;

            promote
                .bind((2, now))
                .map_err(|e| e.to_string())?;

            promote
                .bind((3, existing_id))
                .map_err(|e| e.to_string())?;

            promote
                .next()
                .map_err(|e| e.to_string())?;

            drop(promote);
            drop(conn);

            crate::sync::scheduler::signal_sync_work();

            return Ok(existing_id);
        }
    }
    else {
        let mut existing = conn
            .prepare(
                "SELECT id
                 FROM tcloud_sync_jobs
                 WHERE file_id=?1
                   AND operation=?2
                   AND status IN ('pending','running','paused')
                   AND COALESCE(cancel_requested,0)=0
                 ORDER BY id DESC
                 LIMIT 1"
            )
            .map_err(|e| e.to_string())?;

        existing
            .bind((1, file_id))
            .map_err(|e| e.to_string())?;

        existing
            .bind((2, operation.as_str()))
            .map_err(|e| e.to_string())?;

        if let sqlite::State::Row =
            existing.next().map_err(|e| e.to_string())?
        {
            let existing_id = existing
                .read::<i64, _>(0)
                .map_err(|e| e.to_string())?;

            drop(existing);
            drop(conn);

            crate::sync::scheduler::signal_sync_work();

            return Ok(existing_id);
        }
    }

    let mut stmt = conn
        .prepare(
            "INSERT INTO tcloud_sync_jobs(
                file_id,
                operation,
                status,
                created_at,
                updated_at,
                priority,
                attempts,
                next_attempt_at
             )
             VALUES(?1,?2,'pending',?3,?3,?4,0,NULL)"
        )
        .map_err(|e| e.to_string())?;

    stmt.bind((1, file_id))
        .map_err(|e| e.to_string())?;

    stmt.bind((2, operation.as_str()))
        .map_err(|e| e.to_string())?;

    stmt.bind((3, now))
        .map_err(|e| e.to_string())?;

    stmt.bind((4, priority))
        .map_err(|e| e.to_string())?;

    stmt.next().map_err(|e| e.to_string())?;

    let id = crate::db::last_insert_rowid(&conn)?;

    drop(stmt);
    drop(conn);

    crate::sync::scheduler::signal_sync_work();

    Ok(id)
}

fn scalar_i64(conn: &sqlite::Connection, sql: &str) -> Result<i64, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => stmt.read::<i64, _>(0).map_err(|e| e.to_string()),
        _ => Ok(0),
    }
}

fn query_string(conn: &sqlite::Connection, sql: &str, id: i64) -> Result<Option<String>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    stmt.bind((1, id)).map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => stmt.read::<String, _>(0).map(Some).map_err(|e| e.to_string()),
        _ => Ok(None),
    }
}
