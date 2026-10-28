use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudTransferLimits {
    pub max_concurrent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudGlobalPauseState {
    pub paused: bool,
    /// -1 = paused indefinitely, 0/None = running, positive = unix timestamp.
    pub until: Option<i64>,
}

fn set_setting(conn: &sqlite::Connection, key: &str, value: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO tcloud_settings(key, value)
             VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value"
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key)).map_err(|e| e.to_string())?;
    stmt.bind((2, value)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

fn read_setting(conn: &sqlite::Connection, key: &str) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare("SELECT value FROM tcloud_settings WHERE key=?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key)).map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => Ok(stmt.read::<String, _>(0).ok()),
        _ => Ok(None),
    }
}

/// Refresh an expired scheduled pause while the DB lock is already held.
/// Returns the effective pause state after any automatic resume.
pub(crate) fn refresh_global_pause_locked(conn: &sqlite::Connection) -> Result<TCloudGlobalPauseState, String> {
    let now = chrono::Utc::now().timestamp();
    let raw = read_setting(conn, "sync_global_pause_until")?
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    if raw > 0 && raw <= now {
        set_setting(conn, "sync_global_pause_until", "0")?;
        let mut resume = conn.prepare(
            "UPDATE tcloud_sync_jobs
             SET paused=0,
                 pause_scope=NULL,
                 status=CASE WHEN status='paused' THEN 'pending' ELSE status END,
                 updated_at=?1
             WHERE pause_scope='global'"
        ).map_err(|e| e.to_string())?;
        resume.bind((1, now)).map_err(|e| e.to_string())?;
        resume.next().map_err(|e| e.to_string())?;
        drop(resume);
        drop(conn);
        crate::sync::scheduler::signal_sync_work();
        return Ok(TCloudGlobalPauseState { paused: false, until: None });
    }

    if raw == -1 {
        Ok(TCloudGlobalPauseState { paused: true, until: Some(-1) })
    } else if raw > now {
        Ok(TCloudGlobalPauseState { paused: true, until: Some(raw) })
    } else {
        Ok(TCloudGlobalPauseState { paused: false, until: None })
    }
}

#[tauri::command]
pub fn tcloud_pause_sync_job(
    db: State<'_, DbConnection>,
    job_id: i64,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE tcloud_sync_jobs
             SET paused = 1,
                 pause_scope='job',
                 status = CASE WHEN status IN ('running','pending') THEN 'paused' ELSE status END,
                 speed_bps = 0,
                 eta_seconds = NULL,
                 updated_at = ?1
             WHERE id = ?2"
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, now)).map_err(|e| e.to_string())?;
    stmt.bind((2, job_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn tcloud_resume_sync_job(
    db: State<'_, DbConnection>,
    job_id: i64,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE tcloud_sync_jobs
             SET paused = 0,
                 pause_scope=NULL,
                 cancel_requested = 0,
                 status = CASE
                    WHEN status IN ('paused','cancelled','failed') THEN 'pending'
                    ELSE status
                 END,
                 error = NULL,
                 updated_at = ?1
             WHERE id = ?2"
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, now)).map_err(|e| e.to_string())?;
    stmt.bind((2, job_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    drop(stmt);
    drop(conn);
    crate::sync::scheduler::signal_sync_work();
    Ok(())
}

#[tauri::command]
pub fn tcloud_cancel_sync_job(
    db: State<'_, DbConnection>,
    job_id: i64,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE tcloud_sync_jobs
             SET cancel_requested = 1,
                 paused = 0,
                 pause_scope=NULL,
                 status = CASE
                    WHEN status IN ('pending','paused') THEN 'cancelled'
                    ELSE status
                 END,
                 speed_bps = 0,
                 eta_seconds = NULL,
                 updated_at = ?1
             WHERE id = ?2"
        )
        .map_err(|e| e.to_string())?;
    stmt.bind((1, now)).map_err(|e| e.to_string())?;
    stmt.bind((2, job_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn tcloud_get_transfer_limits(
    db: State<'_, DbConnection>,
) -> Result<TCloudTransferLimits, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT value FROM tcloud_settings
                 WHERE key = 'max_concurrent_transfers'
                 LIMIT 1"
            )
            .map_err(|e| e.to_string())?;

        let max_concurrent = match stmt.next().map_err(|e| e.to_string())? {
            sqlite::State::Row => stmt
                .read::<String, _>(0)
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(4),
            _ => 4,
        };

        Ok(TCloudTransferLimits {
            max_concurrent: max_concurrent.clamp(1, 8),
        })
    })
    .await
    .map_err(|e| format!("transfer limits worker failed: {e}"))?
}

#[tauri::command]
pub fn tcloud_set_transfer_limits(
    db: State<'_, DbConnection>,
    max_concurrent: i64,
) -> Result<TCloudTransferLimits, String> {
    let value = max_concurrent.clamp(1, 8);
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    set_setting(&conn, "max_concurrent_transfers", &value.to_string())?;
    drop(conn);
    crate::sync::scheduler::signal_sync_work();
    Ok(TCloudTransferLimits {
        max_concurrent: value,
    })
}

#[tauri::command]
pub async fn tcloud_get_global_pause(
    db: State<'_, DbConnection>,
) -> Result<TCloudGlobalPauseState, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        refresh_global_pause_locked(&conn)
    })
    .await
    .map_err(|e| format!("global pause worker failed: {e}"))?
}

/// `until` semantics: 0 = resume now, -1 = pause indefinitely, >now = scheduled resume.
#[tauri::command]
pub fn tcloud_set_global_pause(
    db: State<'_, DbConnection>,
    until: i64,
) -> Result<TCloudGlobalPauseState, String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    if until == 0 || (until > 0 && until <= now) {
        set_setting(&conn, "sync_global_pause_until", "0")?;
        let mut resume = conn.prepare(
            "UPDATE tcloud_sync_jobs
             SET paused=0,
                 pause_scope=NULL,
                 status=CASE WHEN status='paused' THEN 'pending' ELSE status END,
                 updated_at=?1
             WHERE pause_scope='global'"
        ).map_err(|e| e.to_string())?;
        resume.bind((1, now)).map_err(|e| e.to_string())?;
        resume.next().map_err(|e| e.to_string())?;
        drop(resume);
        drop(conn);
        crate::sync::scheduler::signal_sync_work();
        return Ok(TCloudGlobalPauseState { paused: false, until: None });
    }

    let effective = if until < 0 { -1 } else { until };
    set_setting(&conn, "sync_global_pause_until", &effective.to_string())?;
    let mut pause = conn.prepare(
        "UPDATE tcloud_sync_jobs
         SET paused=1,
             pause_scope='global',
             status=CASE WHEN status IN ('pending','running') THEN 'paused' ELSE status END,
             updated_at=?1
         WHERE COALESCE(paused,0)=0
           AND status NOT IN ('completed','cancelled','failed')"
    ).map_err(|e| e.to_string())?;
    pause.bind((1, now)).map_err(|e| e.to_string())?;
    pause.next().map_err(|e| e.to_string())?;

    Ok(TCloudGlobalPauseState { paused: true, until: Some(effective) })
}
