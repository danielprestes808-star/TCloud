use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudRecentFile {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub size: i64,
    pub sync_status: String,
    pub local_status: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudDashboardStats {
    pub files: i64,
    pub folders: i64,
    pub pending_jobs: i64,
    pub running_jobs: i64,
    pub failed_jobs: i64,
    pub conflicts: i64,
    pub synced_files: i64,
    pub online_only_files: i64,
    pub cached_files: i64,
    pub total_bytes: i64,
    pub recent_files: Vec<TCloudRecentFile>,
}

fn scalar_i64(conn: &sqlite::Connection, sql: &str) -> Result<i64, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => stmt.read::<i64, _>(0).map_err(|e| e.to_string()),
        _ => Ok(0),
    }
}

#[tauri::command]
pub fn tcloud_get_dashboard_stats(
    db: State<'_, DbConnection>,
) -> Result<TCloudDashboardStats, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    let files = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files")?;
    let folders = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_folders WHERE parent_id IS NOT NULL"
    )?;
    let pending_jobs = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status = 'pending'"
    )?;
    let running_jobs = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status = 'running'"
    )?;
    let failed_jobs = scalar_i64(
        &conn,
        "SELECT COUNT(DISTINCT j.file_id) FROM tcloud_sync_jobs j JOIN tcloud_files f ON f.id=j.file_id
         WHERE j.status='failed' AND COALESCE(f.sync_status,'') NOT IN ('synced','deleted','trashed')
           AND NOT EXISTS (SELECT 1 FROM tcloud_sync_jobs newer WHERE newer.file_id=j.file_id AND newer.operation=j.operation AND newer.id>j.id)"
    )?;
    let conflicts = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_sync_conflicts WHERE resolution = 'pending'"
    )?;
    let synced_files = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_files WHERE sync_status = 'synced'"
    )?;
    let online_only_files = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_files WHERE local_status = 'online'"
    )?;
    let cached_files = scalar_i64(
        &conn,
        "SELECT COUNT(*) FROM tcloud_cache"
    )?;
    let total_bytes = scalar_i64(
        &conn,
        "SELECT COALESCE(SUM(size), 0) FROM tcloud_files"
    )?;

    /*
     * Recentes reais:
     * 1. Arquivos efetivamente acessados vêm primeiro, usando
     *    tcloud_cache.last_accessed_at.
     * 2. Enquanto o usuário ainda não tiver histórico suficiente,
     *    completamos com arquivos atualizados recentemente.
     * 3. Não mostramos itens apagados/lixeira.
     */
    let mut recent_stmt = conn
        .prepare(
            "SELECT
                 f.id,
                 f.name,
                 f.path,
                 f.size,
                 f.sync_status,
                 f.local_status,
                 CASE
                     WHEN c.last_accessed_at IS NOT NULL
                         THEN c.last_accessed_at
                     ELSE f.updated_at
                 END AS recent_at
             FROM tcloud_files f
             LEFT JOIN tcloud_cache c
                    ON c.file_id = f.id
             WHERE COALESCE(f.sync_status,'') NOT IN ('deleted','trashed')
               AND COALESCE(f.local_status,'') != 'deleted'
             ORDER BY
                 CASE
                     WHEN c.last_accessed_at IS NOT NULL THEN 0
                     ELSE 1
                 END ASC,
                 recent_at DESC
             LIMIT 8"
        )
        .map_err(|e| e.to_string())?;

    let mut recent_files = Vec::new();
    while let sqlite::State::Row = recent_stmt.next().map_err(|e| e.to_string())? {
        recent_files.push(TCloudRecentFile {
            id: recent_stmt.read::<i64, _>(0).map_err(|e| e.to_string())?,
            name: recent_stmt.read::<String, _>(1).map_err(|e| e.to_string())?,
            path: recent_stmt.read::<String, _>(2).map_err(|e| e.to_string())?,
            size: recent_stmt.read::<i64, _>(3).unwrap_or(0),
            sync_status: recent_stmt.read::<String, _>(4).unwrap_or_else(|_| "unknown".into()),
            local_status: recent_stmt.read::<String, _>(5).unwrap_or_else(|_| "online".into()),
            updated_at: recent_stmt.read::<i64, _>(6).unwrap_or(0),
        });
    }

    Ok(TCloudDashboardStats {
        files,
        folders,
        pending_jobs,
        running_jobs,
        failed_jobs,
        conflicts,
        synced_files,
        online_only_files,
        cached_files,
        total_bytes,
        recent_files,
    })
}
