use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudNetworkSettings {
    pub economy_mode: bool,
    pub upload_limit_bps: u64,
    pub download_limit_bps: u64,
}

fn read_setting(conn: &sqlite::Connection, key: &str) -> Result<Option<String>, String> {
    let mut stmt = conn
        .prepare("SELECT value FROM tcloud_settings WHERE key = ?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    stmt.bind((1, key)).map_err(|e| e.to_string())?;
    match stmt.next().map_err(|e| e.to_string())? {
        sqlite::State::Row => Ok(Some(stmt.read::<String, _>(0).map_err(|e| e.to_string())?)),
        _ => Ok(None),
    }
}

fn write_setting(conn: &sqlite::Connection, key: &str, value: &str) -> Result<(), String> {
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

pub fn load_network_settings(db: &DbConnection) -> Result<TCloudNetworkSettings, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    let economy_mode = read_setting(&conn, "economy_mode")?
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    let upload_limit_bps = read_setting(&conn, "upload_limit_bps")?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let download_limit_bps = read_setting(&conn, "download_limit_bps")?
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    Ok(TCloudNetworkSettings {
        economy_mode,
        upload_limit_bps,
        download_limit_bps,
    })
}

#[tauri::command]
pub fn tcloud_get_network_settings(
    db: State<'_, DbConnection>,
) -> Result<TCloudNetworkSettings, String> {
    load_network_settings(db.inner())
}

#[tauri::command]
pub fn tcloud_set_network_settings(
    db: State<'_, DbConnection>,
    economy_mode: bool,
    upload_limit_bps: u64,
    download_limit_bps: u64,
) -> Result<TCloudNetworkSettings, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    write_setting(&conn, "economy_mode", if economy_mode { "1" } else { "0" })?;
    write_setting(&conn, "upload_limit_bps", &upload_limit_bps.to_string())?;
    write_setting(&conn, "download_limit_bps", &download_limit_bps.to_string())?;

    // 14.23: economia usa uma transfer?ncia; perfil normal usa os 8 workers.
    write_setting(
        &conn,
        "max_concurrent_transfers",
        if economy_mode { "1" } else { "8" },
    )?;

    drop(conn);
    load_network_settings(db.inner())
}
