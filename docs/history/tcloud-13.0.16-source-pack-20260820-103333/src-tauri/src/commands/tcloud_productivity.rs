use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProIndexStats {
    pub files: i64,
    pub folders: i64,
    pub indexed_bytes: i64,
    pub cached_files: i64,
    pub offline_files: i64,
    pub pending_jobs: i64,
    pub conflicts: i64,
    pub trash_items: i64,
    pub versions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProTrashItem {
    pub id: i64,
    pub message_id: i64,
    pub folder_id: Option<i64>,
    pub name: String,
    pub size: i64,
    pub deleted_at: i64,
    pub relative_path: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProTrashBatchResult {
    pub restored: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProVersionItem {
    pub id: i64,
    pub message_id: i64,
    pub folder_id: Option<i64>,
    pub name: String,
    pub size: i64,
    pub note: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProRule {
    pub id: i64,
    pub name: String,
    pub extensions: String,
    pub target_folder_id: Option<i64>,
    pub enabled: bool,
    pub name_contains: Option<String>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProBackupSource {
    pub id: i64,
    pub local_path: String,
    pub target_folder_id: Option<i64>,
    pub enabled: bool,
    pub interval_minutes: i64,
    pub last_run_at: Option<i64>,
    pub last_scan_at: Option<i64>,
    pub last_success_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProBackupScanResult {
    pub files_found: usize,
    pub total_bytes: u64,
}

fn ensure_schema(conn: &sqlite::Connection) -> Result<(), String> {
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
        CREATE INDEX IF NOT EXISTS idx_tcloud_trash_deleted ON tcloud_trash(deleted_at DESC);

        CREATE TABLE IF NOT EXISTS tcloud_versions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id INTEGER NOT NULL,
            folder_id INTEGER,
            name TEXT NOT NULL,
            size INTEGER NOT NULL DEFAULT 0,
            note TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_versions_name ON tcloud_versions(name, created_at DESC);

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

        CREATE TABLE IF NOT EXISTS tcloud_offline_pins (
            file_id INTEGER PRIMARY KEY,
            pinned_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tcloud_index_meta (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        CREATE TABLE IF NOT EXISTS tcloud_backup_state (
            backup_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            modified_at INTEGER NOT NULL DEFAULT 0,
            uploaded_at INTEGER NOT NULL,
            PRIMARY KEY(backup_id, file_path)
        );
        CREATE TABLE IF NOT EXISTS tcloud_backup_jobs(
            backup_id INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            file_id INTEGER NOT NULL,
            sync_job_id INTEGER NOT NULL,
            file_size INTEGER NOT NULL DEFAULT 0,
            modified_at INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            PRIMARY KEY(backup_id, file_path),
            UNIQUE(sync_job_id)
        );

        CREATE INDEX IF NOT EXISTS idx_tcloud_backup_jobs_sync
            ON tcloud_backup_jobs(sync_job_id);"
    ).map_err(|e| e.to_string())?;
    // Backward-compatible rule filters introduced in TCloud 10. SQLite returns
    // a duplicate-column error on subsequent launches, which is safe to ignore.
    let _ = conn.execute("ALTER TABLE tcloud_auto_rules ADD COLUMN name_contains TEXT");
    let _ = conn.execute("ALTER TABLE tcloud_auto_rules ADD COLUMN min_size INTEGER");
    let _ = conn.execute("ALTER TABLE tcloud_auto_rules ADD COLUMN max_size INTEGER");
    let _ = conn.execute("ALTER TABLE tcloud_versions ADD COLUMN file_id INTEGER");
    let _ = conn.execute("ALTER TABLE tcloud_versions ADD COLUMN original_path TEXT");
    let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN file_id INTEGER");
    let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN relative_path TEXT");
    let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN expires_at INTEGER");
    let _ = conn.execute("ALTER TABLE tcloud_trash ADD COLUMN status TEXT NOT NULL DEFAULT 'trashed'");
    Ok(())
}

fn scalar_i64(conn: &sqlite::Connection, sql: &str) -> i64 {
    let Ok(mut stmt) = conn.prepare(sql) else { return 0; };
    match stmt.next() {
        Ok(sqlite::State::Row) => stmt.read::<i64, _>(0).unwrap_or(0),
        _ => 0,
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProIndexFileInput {
    pub message_id: i64,
    pub name: String,
    pub size: i64,
    pub mime_type: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProIndexFileRow {
    pub message_id: i64,
    pub name: String,
    pub size: i64,
    pub created_at: Option<String>,
    pub sync_status: String,
    pub local_status: String,
    pub is_pinned: bool,
}

fn tcloud_pro_index_upsert_files_inner(
    app: &tauri::AppHandle,
    db: &DbConnection,
    folder_id: Option<i64>,
    files: Vec<ProIndexFileInput>,
) -> Result<usize, String> {
    use std::collections::HashSet;
    let now = chrono::Utc::now().timestamp();
    let chat_key = folder_id.unwrap_or(0);
    let remote_ids: HashSet<i64> = files.iter().map(|file| file.message_id).collect();
    let mut written = 0usize;
    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        conn.execute("BEGIN IMMEDIATE").map_err(|e| e.to_string())?;
        let result: Result<(), String> = (|| {
            for file in &files {
                // Version snapshots remain as real Telegram messages, but are hidden
                // from the live Explorer projection and surfaced only in Versionamento.
                if crate::commands::tcloud_sync_maturity::is_version_message(&conn,chat_key,file.message_id)
                    || crate::commands::tcloud_sync_maturity::is_trash_message(&conn,chat_key,file.message_id)
                {
                    continue;
                }
                let existing = {
                    let mut q = conn.prepare(
                        "SELECT id,name,size,sync_status,sha256,path,local_status FROM tcloud_files
                         WHERE telegram_chat_id=?1 AND telegram_message_id=?2 AND sync_status!='trashed'
                         ORDER BY CASE WHEN path LIKE 'telegram:%' THEN 1 ELSE 0 END,id LIMIT 1"
                    ).map_err(|e|e.to_string())?;
                    q.bind((1,chat_key)).map_err(|e|e.to_string())?;
                    q.bind((2,file.message_id)).map_err(|e|e.to_string())?;
                    if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                        Some((
    q.read::<i64,_>(0).map_err(|e|e.to_string())?,
    q.read::<String,_>(1).unwrap_or_default(),
    q.read::<i64,_>(2).unwrap_or(0),
    q.read::<String,_>(3).unwrap_or_default(),
    q.read::<Option<String>,_>(4).unwrap_or(None),
    q.read::<String,_>(5).unwrap_or_default(),
    q.read::<String,_>(6).unwrap_or_default(),
))
                    }else{None}
                };
                let created=file.created_at.as_deref().and_then(|value|value.parse::<i64>().ok()).unwrap_or(now);
                if let Some((file_id,old_name,old_size,status,local_hash,local_path,local_status))=existing{
                    let real_local_edit =
    !local_path.is_empty()
    && !local_path.starts_with("telegram:")
    && !matches!(
        local_status.as_str(),
        "cloud_only" | "online" | "deleted"
    )
    && crate::sync::watcher_runtime::is_materialized_user_file(
        std::path::Path::new(&local_path)
    );

if matches!(status.as_str(),"pending_upload"|"uploading")
    && old_name!=file.name
    && real_local_edit
{
                        // Same remote identity changed while local bytes are waiting
                        // to upload: this is a real PC x Telegram metadata conflict.
                        crate::commands::tcloud_conflicts::record_remote_metadata_conflict(
                            &conn,file_id,file.message_id,&old_name,Some(local_path.as_str()),old_size,&file.name,file.size,local_hash.as_deref()
                        )?;
                    }else if matches!(status.as_str(),"pending_upload"|"uploading"|"pending_move"|"pending_delete"|"conflict"){
                        // The remote listing still contains the last committed
                        // Telegram version while a local operation is in flight.
                        // Preserve the local intent instead of turning the expected
                        // old snapshot into a false conflict.
                    }else{
                        let mut update=conn.prepare(
                            "UPDATE tcloud_files SET name=CASE WHEN TRIM(?1)<>'' THEN ?1 ELSE name END,size=?2,mime_type=COALESCE(?3,mime_type),telegram_chat_id=?4,telegram_message_id=?5,
                             sync_status='synced',updated_at=?6 WHERE id=?7"
                        ).map_err(|e|e.to_string())?;
                        update.bind((1,file.name.as_str())).map_err(|e|e.to_string())?;update.bind((2,file.size)).map_err(|e|e.to_string())?;
                        update.bind((3,file.mime_type.as_deref())).map_err(|e|e.to_string())?;update.bind((4,chat_key)).map_err(|e|e.to_string())?;update.bind((5,file.message_id)).map_err(|e|e.to_string())?;
                        update.bind((6,now)).map_err(|e|e.to_string())?;update.bind((7,file_id)).map_err(|e|e.to_string())?;update.next().map_err(|e|e.to_string())?;
                    }
                }else{
                    let path=format!("telegram:{chat_key}:{}",file.message_id);
                    let mut insert=conn.prepare(
                        "INSERT INTO tcloud_files(name,path,size,mime_type,telegram_chat_id,telegram_message_id,sync_status,local_status,created_at,updated_at)
                         VALUES(?1,?2,?3,?4,?5,?6,'synced','cloud_only',?7,?8)
                         ON CONFLICT(path) DO UPDATE SET name=CASE WHEN TRIM(excluded.name)<>'' THEN excluded.name ELSE tcloud_files.name END,size=excluded.size,mime_type=COALESCE(excluded.mime_type,tcloud_files.mime_type),sync_status='synced',local_status='cloud_only',updated_at=excluded.updated_at"
                    ).map_err(|e|e.to_string())?;
                    insert.bind((1,file.name.as_str())).map_err(|e|e.to_string())?;insert.bind((2,path.as_str())).map_err(|e|e.to_string())?;insert.bind((3,file.size)).map_err(|e|e.to_string())?;
                    insert.bind((4,file.mime_type.as_deref())).map_err(|e|e.to_string())?;insert.bind((5,chat_key)).map_err(|e|e.to_string())?;insert.bind((6,file.message_id)).map_err(|e|e.to_string())?;insert.bind((7,created)).map_err(|e|e.to_string())?;insert.bind((8,now)).map_err(|e|e.to_string())?;insert.next().map_err(|e|e.to_string())?;
                }
                written+=1;
            }

            // A complete folder listing is also our remote-delete signal. The
            // Telegram loader has a 50k safety cap; at that exact cap we skip
            // destructive absence reconciliation rather than risk a false delete.
            // SAFETY:
            // Uma leitura vazia ou parcial do Telegram nao prova exclusao remota.
            // Nunca marque arquivos como remote_deleted apenas porque nao apareceram
            // em uma unica varredura.
            if false && remote_ids.len()<50_000{
                let mut q=conn.prepare(
                    "SELECT id,telegram_message_id,sync_status FROM tcloud_files
                     WHERE telegram_chat_id=?1 AND telegram_message_id IS NOT NULL AND sync_status NOT IN ('trashed','deleted','pending_delete')"
                ).map_err(|e|e.to_string())?;
                q.bind((1,chat_key)).map_err(|e|e.to_string())?;
                let mut stale=Vec::new();
                while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
                    let id=q.read::<i64,_>(0).unwrap_or(0);let mid=q.read::<i64,_>(1).unwrap_or(0);let status=q.read::<String,_>(2).unwrap_or_default();
                    if mid>0 && !remote_ids.contains(&mid){stale.push((id,status));}
                }
                drop(q);
                for (id,status) in stale{
                    if matches!(status.as_str(),"pending_upload"|"uploading"){
                        let mut meta=conn.prepare("SELECT telegram_message_id,name,size,local_fingerprint,path,local_status FROM tcloud_files WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
                        meta.bind((1,id)).map_err(|e|e.to_string())?;
                        if matches!(meta.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                            let _mid=meta.read::<Option<i64>,_>(0).unwrap_or(None).unwrap_or(0);
                            let name=meta.read::<String,_>(1).unwrap_or_else(|_|"arquivo".into());
                            let size=meta.read::<i64,_>(2).unwrap_or(0);
                            let hash=meta.read::<Option<String>,_>(3).unwrap_or(None);
                            let local_path=meta.read::<String,_>(4).unwrap_or_default();
                            let local_status=meta.read::<String,_>(5).unwrap_or_default();

                            let real_local_edit =
                                !local_path.is_empty()
                                && !local_path.starts_with("telegram:")
                                && !matches!(
                                    local_status.as_str(),
                                    "cloud_only" | "online" | "deleted"
                                )
                                && crate::sync::watcher_runtime::is_materialized_user_file(
                                    std::path::Path::new(&local_path)
                                );
                            if real_local_edit {
                                crate::commands::tcloud_conflicts::record_remote_metadata_conflict(
                                    &conn,
                                    id,
                                    0,
                                    &name,
                                    Some(local_path.as_str()),
                                    size,
                                    "(excluído no Telegram)",
                                    0,
                                    hash.as_deref()
                                )?;
                            } else {
                                log::debug!(
                                    "TCloud: ausência remota ignorada como conflito para placeholder/projeção file_id={}",
                                    id
                                );
                            }
                        }
                        continue;
                    }
                    if matches!(status.as_str(),"pending_move"|"pending_delete"|"conflict"){continue;}
                    let mut mark=conn.prepare("UPDATE tcloud_files SET sync_status='remote_deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
                    mark.bind((1,now)).map_err(|e|e.to_string())?;mark.bind((2,id)).map_err(|e|e.to_string())?;mark.next().map_err(|e|e.to_string())?;
                }
            }
            let scan_key=format!("remote_scan:{}",chat_key);
            let mut scan=conn.prepare(
                "INSERT INTO tcloud_index_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value"
            ).map_err(|e|e.to_string())?;
            let scan_value=now.to_string();
            scan.bind((1,scan_key.as_str())).map_err(|e|e.to_string())?;
            scan.bind((2,scan_value.as_str())).map_err(|e|e.to_string())?;
            scan.next().map_err(|e|e.to_string())?;
            Ok(())
        })();
        if let Err(error)=result{let _=conn.execute("ROLLBACK");return Err(error);}
        conn.execute("COMMIT").map_err(|e|e.to_string())?;
    }
    // Resolve the default "Guardar as duas" strategy immediately after the
    // SQLite transaction is committed. This preserves PC bytes as a new file
    // before the original placeholder adopts the cloud version.
    // Conflitos aguardam decis?o expl?cita do usu?rio.
    // "Manter os dois" nunca ? executado automaticamente.

    // Keep the Explorer projection current after Telegram changes. This call is
    // incremental: unchanged identities are skipped without touching the file.
    if let Err(error)=crate::commands::tcloud_cfapi::reconcile_remote_chat(app,db,chat_key){
        log::warn!("TCloud: índice Telegram atualizado, mas a projeção CFAPI será reconciliada depois: {}",error);
    }
    Ok(written)
}

#[tauri::command]
pub async fn tcloud_pro_index_upsert_files(
    app: tauri::AppHandle,
    db: State<'_, DbConnection>,
    folder_id: Option<i64>,
    files: Vec<ProIndexFileInput>,
) -> Result<usize, String> {
    let db = db.inner().clone();
    let file_count = files.len();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = tcloud_pro_index_upsert_files_inner(&app, &db, folder_id, files);
        let elapsed = started.elapsed();
        if elapsed >= std::time::Duration::from_secs(2) {
            log::warn!("TCloud: indexação de {} arquivo(s) levou {:?}; executada em worker de fundo.", file_count, elapsed);
        }
        result
    })
        .await
        .map_err(|e| format!("index update worker failed: {e}"))?
}

fn known_remote_destinations(db:&DbConnection)->Vec<Option<i64>>{
    use std::collections::HashSet;
    let Ok(conn)=db.lock() else{return vec![None];};
    let mut forum_chats=HashSet::new();
    if let Ok(mut q)=conn.prepare("SELECT chat_id FROM tcloud_forums WHERE chat_id IS NOT NULL"){
        while let Ok(sqlite::State::Row)=q.next(){if let Ok(id)=q.read::<i64,_>(0){forum_chats.insert(id);}}
    }
    let mut values=Vec::new();values.push(None);
    if let Ok(mut q)=conn.prepare("SELECT channel_id FROM folder_metadata WHERE channel_id IS NOT NULL ORDER BY display_order,name"){
        while let Ok(sqlite::State::Row)=q.next(){
            if let Ok(id)=q.read::<i64,_>(0){if id!=0 && !forum_chats.contains(&id){values.push(Some(id));}}
        }
    }
    if let Ok(mut q)=conn.prepare("SELECT id FROM tcloud_forum_topics WHERE id>0 ORDER BY id"){
        while let Ok(sqlite::State::Row)=q.next(){if let Ok(id)=q.read::<i64,_>(0){values.push(Some(-id));}}
    }
    let mut seen=HashSet::new();
    values.into_iter().filter(|value|seen.insert(value.unwrap_or(0))).take(256).collect()
}

fn remote_scan_due(db:&DbConnection,folder_id:Option<i64>,min_age_seconds:i64)->bool{
    let chat_key=folder_id.unwrap_or(0);
    let Ok(conn)=db.lock() else{return false;};
    let key=format!("remote_scan:{}",chat_key);
    let Ok(mut q)=conn.prepare("SELECT value FROM tcloud_index_meta WHERE key=?1 LIMIT 1") else{return true;};
    if q.bind((1,key.as_str())).is_err(){return true;}
    if !matches!(q.next(),Ok(sqlite::State::Row)){return true;}
    let last=q.read::<String,_>(0).ok().and_then(|value|value.parse::<i64>().ok()).unwrap_or(0);
    chrono::Utc::now().timestamp().saturating_sub(last)>=min_age_seconds
}

fn has_recent_foreground_activity(db:&DbConnection,seconds:i64)->bool{
    let Ok(conn)=db.lock() else{return true;};
    let threshold=chrono::Utc::now().timestamp().saturating_sub(seconds.max(1));
    if let Ok(mut q)=conn.prepare(
        "SELECT 1 FROM tcloud_sync_jobs
         WHERE updated_at>=?1 AND COALESCE(priority,0)>=100
           AND status IN ('pending','running','completed') LIMIT 1"
    ){
        if q.bind((1,threshold)).is_ok() && matches!(q.next(),Ok(sqlite::State::Row)){return true;}
    }
    if let Ok(mut exists)=conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='tcloud_cfapi_pending_ops' LIMIT 1"){
        if matches!(exists.next(),Ok(sqlite::State::Row)){
            if let Ok(mut q)=conn.prepare("SELECT 1 FROM tcloud_cfapi_pending_ops WHERE created_at>=?1 LIMIT 1"){
                if q.bind((1,threshold)).is_ok() && matches!(q.next(),Ok(sqlite::State::Row)){return true;}
            }
        }
    }
    false
}

fn has_foreground_sync_work(db:&DbConnection)->bool{
    let Ok(conn)=db.lock() else{return true;};
    let Ok(mut q)=conn.prepare(
        "SELECT COUNT(*) FROM tcloud_sync_jobs
         WHERE status IN ('pending','running') AND COALESCE(paused,0)=0 AND COALESCE(cancel_requested,0)=0
           AND COALESCE(priority,0)>=80"
    ) else{return true;};
    if matches!(q.next(),Ok(sqlite::State::Row)) && q.read::<i64,_>(0).unwrap_or(0)>0{return true;}
    // Rename/move/delete callbacks use the dedicated CFAPI durable queue. Yield
    // remote inventory while those user actions are still pending, but do not let
    // an old terminally-failed file status starve background reconciliation forever.
    if let Ok(mut exists)=conn.prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='tcloud_cfapi_pending_ops' LIMIT 1"){
        if matches!(exists.next(),Ok(sqlite::State::Row)){
            if let Ok(mut q)=conn.prepare("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops WHERE completed_at IS NULL AND attempts<12"){
                if matches!(q.next(),Ok(sqlite::State::Row)) && q.read::<i64,_>(0).unwrap_or(0)>0{return true;}
            }
        }
    }
    false
}

/// TCloud 12.3 remote -> Explorer reconciler. It scans one known Telegram
/// destination per pass instead of hammering every chat at startup. Normal UI
/// folder loads still reconcile immediately; this worker covers background use.
pub fn start_remote_reconcile_worker(app:AppHandle){
    tauri::async_runtime::spawn(async move{
        let mut cursor=0usize;
        let mut maintenance_tick=0u32;
        // 12.7.1: startup is cache-first. Give CFAPI, the watcher and the visible
        // folder a quiet window; the UI can still refresh the folder the user opens.
        // 14.25: inicializacao mais rapida da sincronizacao Telegram -> Explorer.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        loop{
            maintenance_tick=maintenance_tick.wrapping_add(1);
            if maintenance_tick%60==1{
                let db=app.state::<DbConnection>();
                if let Err(error)=crate::commands::tcloud_sync_maturity::queue_expired_trash(db.inner()){log::warn!("TCloud Trash maintenance: {}",error);}
            }
            let foreground={let db=app.state::<DbConnection>();has_foreground_sync_work(db.inner())};
            let recent_foreground={let db=app.state::<DbConnection>();has_recent_foreground_activity(db.inner(),10)};
            if foreground || recent_foreground{
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                continue;
            }
            let connected={
                let state=app.state::<crate::TelegramState>();
                let connected=state.client.lock().await.is_some();
                connected
            };
            if connected{
                let destinations={let db=app.state::<DbConnection>();known_remote_destinations(db.inner())};
                if !destinations.is_empty(){
                    // At most two due destinations per idle pass; very large accounts
                    // scan one at a time. A successful UI folder refresh writes the same
                    // timestamp, so this worker does not immediately request it again.
                    let pass_limit=if destinations.len()>24{1usize}else{2usize};
                    let mut scanned=0usize;
                    let mut inspected=0usize;
                    while scanned<pass_limit && inspected<destinations.len(){
                        if {let db=app.state::<DbConnection>();has_foreground_sync_work(db.inner()) || has_recent_foreground_activity(db.inner(),10)}{break;}
                        let folder_id=destinations[cursor%destinations.len()];
                        cursor=cursor.wrapping_add(1);
                        inspected+=1;
                        let due={let db=app.state::<DbConnection>();remote_scan_due(db.inner(),folder_id,15)};
                        if !due{continue;}
                        let result=crate::commands::fs::cmd_get_files(
                            folder_id,Some(false),Some(true),app.clone(),app.state::<crate::TelegramState>(),app.state::<DbConnection>(),app.state::<crate::crypto::state::CryptoState>()
                        ).await;
                        match result{
                            Ok(files)=>{
                                let inputs=files.into_iter().map(|file|ProIndexFileInput{
                                    message_id:file.id,name:file.name,size:i64::try_from(file.size).unwrap_or(i64::MAX),mime_type:file.mime_type,created_at:None,
                                }).collect();
                                let db=app.state::<DbConnection>();
                                if let Err(error)=tcloud_pro_index_upsert_files(app.clone(),db,folder_id,inputs).await{
                                    log::warn!("TCloud remote reconcile failed for {:?}: {}",folder_id,error);
                                }
                            }
                            Err(error)=>log::debug!("TCloud remote scan skipped for {:?}: {}",folder_id,error),
                        }
                        scanned+=1;
                    }
                }
            }
            // 14.25: reduz a latencia de alteracoes remotas sem polling agressivo.
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    });
}

fn tcloud_pro_index_list_files_inner(
    db: &DbConnection,
    folder_id: Option<i64>,
) -> Result<Vec<ProIndexFileRow>, String> {
    let chat_key = folder_id.unwrap_or(0);
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT telegram_message_id,name,size,created_at,sync_status,local_status,
                EXISTS(SELECT 1 FROM tcloud_offline_pins p WHERE p.file_id=tcloud_files.id) AS is_pinned
         FROM tcloud_files
         WHERE telegram_chat_id=?1 AND telegram_message_id IS NOT NULL AND sync_status!='trashed'
         ORDER BY created_at DESC LIMIT 10000"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, chat_key)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(ProIndexFileRow {
            message_id: stmt.read(0).map_err(|e| e.to_string())?,
            name: stmt.read(1).map_err(|e| e.to_string())?,
            size: stmt.read(2).map_err(|e| e.to_string())?,
            created_at: stmt.read::<i64,_>(3).ok().map(|value| value.to_string()),
            sync_status: stmt.read::<String,_>(4).unwrap_or_else(|_| "synced".into()),
            local_status: stmt.read::<String,_>(5).unwrap_or_else(|_| "online".into()),
            is_pinned: stmt.read::<i64,_>(6).unwrap_or(0) != 0,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn tcloud_pro_index_list_files(
    db: State<'_, DbConnection>,
    folder_id: Option<i64>,
) -> Result<Vec<ProIndexFileRow>, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = tcloud_pro_index_list_files_inner(&db, folder_id);
        let elapsed = started.elapsed();
        if elapsed >= std::time::Duration::from_secs(1) {
            log::warn!("TCloud: leitura do índice local levou {:?}; executada em worker de fundo.", elapsed);
        }
        result
    })
        .await
        .map_err(|e| format!("index read worker failed: {e}"))?
}

#[tauri::command]
pub fn tcloud_pro_index_stats(db: State<'_, DbConnection>) -> Result<ProIndexStats, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    Ok(ProIndexStats {
        files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files"),
        folders: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_folders"),
        indexed_bytes: scalar_i64(&conn, "SELECT COALESCE(SUM(size),0) FROM tcloud_files"),
        cached_files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_cache"),
        offline_files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files WHERE local_status!='online'"),
        pending_jobs: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status IN ('pending','running','uploading','downloading')"),
        conflicts: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_conflicts WHERE resolution='pending'"),
        trash_items: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_trash"),
        versions: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_versions"),
    })
}

#[tauri::command]
pub fn tcloud_pro_mark_index_rebuilt(db: State<'_, DbConnection>) -> Result<i64, String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare("INSERT OR REPLACE INTO tcloud_index_meta(key,value) VALUES('last_rebuild',?1)").map_err(|e| e.to_string())?;
    stmt.bind((1, now.to_string().as_str())).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(now)
}

#[tauri::command]
pub fn tcloud_pro_trash_add(db: State<'_, DbConnection>, message_id: i64, folder_id: Option<i64>, name: String, size: i64) -> Result<i64, String> {
    let file_id={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
        let mut q=conn.prepare("SELECT id FROM tcloud_files WHERE telegram_message_id=?1 AND COALESCE(telegram_chat_id,0)=?2 AND sync_status NOT IN ('deleted','trashed') ORDER BY id DESC LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,message_id)).map_err(|e|e.to_string())?;q.bind((2,folder_id.unwrap_or(0))).map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){Some(q.read::<i64,_>(0).unwrap_or(0))}else{None}
    };
    if let Some(fid)=file_id{
        crate::commands::tcloud_sync_maturity::move_file_to_trash(db.inner(),fid)?;
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("SELECT id FROM tcloud_trash WHERE file_id=?1 ORDER BY deleted_at DESC LIMIT 1").map_err(|e|e.to_string())?;q.bind((1,fid)).map_err(|e|e.to_string())?;if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(q.read::<i64,_>(0).unwrap_or(0));}
    }
    // Compatibility fallback for messages that have not yet been indexed locally.
    let now=chrono::Utc::now().timestamp();let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut stmt=conn.prepare("INSERT OR REPLACE INTO tcloud_trash(message_id,folder_id,name,size,deleted_at) VALUES(?1,?2,?3,?4,?5)").map_err(|e|e.to_string())?;
    stmt.bind((1,message_id)).map_err(|e|e.to_string())?;stmt.bind((2,folder_id)).map_err(|e|e.to_string())?;stmt.bind((3,name.as_str())).map_err(|e|e.to_string())?;stmt.bind((4,size)).map_err(|e|e.to_string())?;stmt.bind((5,now)).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;Ok(crate::db::last_insert_rowid(&conn)?)
}

#[tauri::command]
pub fn tcloud_pro_trash_list(db: State<'_, DbConnection>) -> Result<Vec<ProTrashItem>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare("SELECT id,message_id,folder_id,name,size,deleted_at,relative_path,expires_at FROM tcloud_trash ORDER BY deleted_at DESC").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(ProTrashItem {
            id: stmt.read(0).map_err(|e| e.to_string())?,
            message_id: stmt.read(1).map_err(|e| e.to_string())?,
            folder_id: stmt.read::<Option<i64>,_>(2).unwrap_or(None),
            name: stmt.read(3).map_err(|e| e.to_string())?,
            size: stmt.read(4).map_err(|e| e.to_string())?,
            deleted_at: stmt.read(5).map_err(|e| e.to_string())?,
            relative_path: stmt.read::<Option<String>,_>(6).unwrap_or(None),
            expires_at: stmt.read::<Option<i64>,_>(7).unwrap_or(None),
        });
    }
    Ok(out)
}

fn restore_trash_item(app:&AppHandle,db:&DbConnection,trash_id:i64)->Result<bool,String>{
    let (file_id,original_relative)={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
        let mut q=conn.prepare("SELECT file_id,message_id,COALESCE(folder_id,0),relative_path FROM tcloud_trash WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,trash_id)).map_err(|e|e.to_string())?;
        if !matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Err("Item da Lixeira não encontrado.".into());}
        let stored=q.read::<Option<i64>,_>(0).unwrap_or(None);
        let mid=q.read::<i64,_>(1).unwrap_or(0);
        let folder=q.read::<i64,_>(2).unwrap_or(0);
        let relative=q.read::<Option<String>,_>(3).unwrap_or(None);
        drop(q);
        let fid=if let Some(fid)=stored{
            fid
        }else{
            let mut f=conn.prepare("SELECT id FROM tcloud_files WHERE telegram_message_id=?1 AND COALESCE(telegram_chat_id,0)=?2 ORDER BY id DESC LIMIT 1").map_err(|e|e.to_string())?;
            f.bind((1,mid)).map_err(|e|e.to_string())?;
            f.bind((2,folder)).map_err(|e|e.to_string())?;
            if !matches!(f.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Err("Arquivo remoto da Lixeira não está mais no índice.".into());}
            f.read::<i64,_>(0).unwrap_or(0)
        };
        (fid,relative)
    };

    // Remember the old Explorer path before re-projecting the remote placeholder.
    // If another file occupied the same name while this item was in Trash, the
    // CFAPI layer reserves an alternate path so the newer file is never touched.
    match crate::commands::tcloud_cfapi::prepare_restore_location(db,file_id,original_relative.as_deref()){
        Ok(Some(path))=>log::info!("TCloud Lixeira: restauração de {} preparada em {}.",file_id,path),
        Ok(None)=>{},
        Err(error)=>log::warn!("TCloud Lixeira: local original não pôde ser preparado, usando destino padrão: {}",error),
    }

    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let now=chrono::Utc::now().timestamp();
        let mut up=conn.prepare("UPDATE tcloud_files SET sync_status='synced',local_status='cloud_only',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
        up.bind((1,now)).map_err(|e|e.to_string())?;
        up.bind((2,file_id)).map_err(|e|e.to_string())?;
        up.next().map_err(|e|e.to_string())?;
        let mut d=conn.prepare("DELETE FROM tcloud_trash WHERE id=?1").map_err(|e|e.to_string())?;
        d.bind((1,trash_id)).map_err(|e|e.to_string())?;
        d.next().map_err(|e|e.to_string())?;
    }
    let _=crate::commands::tcloud_cfapi::reconcile_remote_index(app,db);
    log::info!("TCloud Lixeira: arquivo {} restaurado ao Explorer.",file_id);
    Ok(true)
}

#[tauri::command]
pub fn tcloud_pro_trash_restore(app: AppHandle, db: State<'_, DbConnection>, trash_id: i64) -> Result<bool, String> {
    restore_trash_item(&app,db.inner(),trash_id)
}

#[tauri::command]
pub fn tcloud_pro_trash_restore_many(app:AppHandle,db:State<'_,DbConnection>,trash_ids:Vec<i64>)->Result<ProTrashBatchResult,String>{
    let mut result=ProTrashBatchResult{restored:0,failed:0,errors:Vec::new()};
    for id in trash_ids.into_iter().take(500){
        match restore_trash_item(&app,db.inner(),id){
            Ok(true)=>result.restored+=1,
            Ok(false)=>result.failed+=1,
            Err(error)=>{
                result.failed+=1;
                if result.errors.len()<20{result.errors.push(error);}
            }
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn tcloud_pro_trash_clear(db: State<'_, DbConnection>) -> Result<bool, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    conn.execute("DELETE FROM tcloud_trash").map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn tcloud_pro_version_add(db: State<'_, DbConnection>, message_id: i64, folder_id: Option<i64>, name: String, size: i64, note: Option<String>) -> Result<i64, String> {
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare("INSERT INTO tcloud_versions(message_id,folder_id,name,size,note,created_at) VALUES(?1,?2,?3,?4,?5,?6)").map_err(|e| e.to_string())?;
    stmt.bind((1, message_id)).map_err(|e| e.to_string())?;
    stmt.bind((2, folder_id)).map_err(|e| e.to_string())?;
    stmt.bind((3, name.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((4, size)).map_err(|e| e.to_string())?;
    stmt.bind((5, note.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((6, now)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(crate::db::last_insert_rowid(&conn)?)
}

#[tauri::command]
pub fn tcloud_pro_version_list(db: State<'_, DbConnection>, name: Option<String>) -> Result<Vec<ProVersionItem>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let sql = if name.is_some() {
        "SELECT id,message_id,folder_id,name,size,note,created_at FROM tcloud_versions WHERE name=?1 ORDER BY created_at DESC"
    } else {
        "SELECT id,message_id,folder_id,name,size,note,created_at FROM tcloud_versions ORDER BY created_at DESC LIMIT 200"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    if let Some(ref value) = name { stmt.bind((1, value.as_str())).map_err(|e| e.to_string())?; }
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(ProVersionItem {
            id: stmt.read(0).map_err(|e| e.to_string())?,
            message_id: stmt.read(1).map_err(|e| e.to_string())?,
            folder_id: stmt.read(2).ok(),
            name: stmt.read(3).map_err(|e| e.to_string())?,
            size: stmt.read(4).map_err(|e| e.to_string())?,
            note: stmt.read(5).ok(),
            created_at: stmt.read(6).map_err(|e| e.to_string())?,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn tcloud_pro_rules_list(db: State<'_, DbConnection>) -> Result<Vec<ProRule>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare("SELECT id,name,extensions,target_folder_id,enabled,name_contains,min_size,max_size FROM tcloud_auto_rules ORDER BY id").map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(ProRule {
            id: stmt.read(0).map_err(|e|e.to_string())?,
            name: stmt.read(1).map_err(|e|e.to_string())?,
            extensions: stmt.read(2).map_err(|e|e.to_string())?,
            target_folder_id: stmt.read::<Option<i64>,_>(3).unwrap_or(None),
            enabled: stmt.read::<i64,_>(4).unwrap_or(1) != 0,
            name_contains: stmt.read::<Option<String>,_>(5).unwrap_or(None),
            min_size: stmt.read::<Option<i64>,_>(6).unwrap_or(None),
            max_size: stmt.read::<Option<i64>,_>(7).unwrap_or(None),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn tcloud_pro_rule_save(
    db: State<'_, DbConnection>,
    id: Option<i64>,
    name: String,
    extensions: String,
    target_folder_id: Option<i64>,
    enabled: bool,
    name_contains: Option<String>,
    min_size: Option<i64>,
    max_size: Option<i64>,
) -> Result<i64, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    if let Some(id) = id {
        let mut stmt = conn.prepare("UPDATE tcloud_auto_rules SET name=?1,extensions=?2,target_folder_id=?3,enabled=?4,name_contains=?5,min_size=?6,max_size=?7 WHERE id=?8").map_err(|e|e.to_string())?;
        stmt.bind((1,name.as_str())).map_err(|e|e.to_string())?;
        stmt.bind((2,extensions.as_str())).map_err(|e|e.to_string())?;
        stmt.bind((3,target_folder_id)).map_err(|e|e.to_string())?;
        stmt.bind((4, if enabled {1_i64}else{0_i64})).map_err(|e|e.to_string())?;
        stmt.bind((5,name_contains.as_deref())).map_err(|e|e.to_string())?;
        stmt.bind((6,min_size)).map_err(|e|e.to_string())?;
        stmt.bind((7,max_size)).map_err(|e|e.to_string())?;
        stmt.bind((8,id)).map_err(|e|e.to_string())?;
        stmt.next().map_err(|e|e.to_string())?;
        Ok(id)
    } else {
        let mut stmt = conn.prepare("INSERT INTO tcloud_auto_rules(name,extensions,target_folder_id,enabled,name_contains,min_size,max_size) VALUES(?1,?2,?3,?4,?5,?6,?7)").map_err(|e|e.to_string())?;
        stmt.bind((1,name.as_str())).map_err(|e|e.to_string())?;
        stmt.bind((2,extensions.as_str())).map_err(|e|e.to_string())?;
        stmt.bind((3,target_folder_id)).map_err(|e|e.to_string())?;
        stmt.bind((4, if enabled {1_i64}else{0_i64})).map_err(|e|e.to_string())?;
        stmt.bind((5,name_contains.as_deref())).map_err(|e|e.to_string())?;
        stmt.bind((6,min_size)).map_err(|e|e.to_string())?;
        stmt.bind((7,max_size)).map_err(|e|e.to_string())?;
        stmt.next().map_err(|e|e.to_string())?;
        Ok(crate::db::last_insert_rowid(&conn)?)
    }
}

#[tauri::command]
pub fn tcloud_pro_rule_delete(db: State<'_, DbConnection>, id: i64) -> Result<bool,String>{let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut stmt=conn.prepare("DELETE FROM tcloud_auto_rules WHERE id=?1").map_err(|e|e.to_string())?;stmt.bind((1,id)).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;Ok(true)}


fn backup_file_metadata(path: &Path) -> Option<(i64, i64)> {
    let meta = std::fs::metadata(path).ok()?;

    if !meta.is_file() {
        return None;
    }

    let modified = meta
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0);

    Some((meta.len() as i64, modified))
}

fn collect_backup_files(path: &Path, output: &mut Vec<String>, limit: usize) {
    if output.len() >= limit {
        return;
    }

    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        if output.len() >= limit {
            break;
        }

        let path = entry.path();

        if path.is_dir() {
            collect_backup_files(&path, output, limit);
        } else if path.is_file() {
            output.push(path.to_string_lossy().to_string());
        }
    }
}

fn enqueue_backup_file(
    db: &DbConnection,
    backup_id: i64,
    file_path: &str,
    target_folder_id: Option<i64>,
) -> Result<bool, String> {
    let path = Path::new(file_path);

    let Some((size, modified)) = backup_file_metadata(path) else {
        return Ok(false);
    };

    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("arquivo")
        .to_string();

    let fingerprint = format!("{}:{}", size, modified);
    let now = chrono::Utc::now().timestamp();

    let conn = db
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    ensure_schema(&conn)?;

    // Arquivo já confirmado por backup anterior?
    {
        let mut q = conn
            .prepare(
                "SELECT file_size,modified_at
                 FROM tcloud_backup_state
                 WHERE backup_id=?1 AND file_path=?2
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        q.bind((1, backup_id)).map_err(|e| e.to_string())?;
        q.bind((2, file_path)).map_err(|e| e.to_string())?;

        if matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            let old_size = q.read::<i64, _>(0).unwrap_or(-1);
            let old_modified = q.read::<i64, _>(1).unwrap_or(-1);

            if old_size == size && old_modified == modified {
                return Ok(false);
            }
        }
    }

    // Já existe um job de backup pendente para este mesmo estado?
    {
        let mut q = conn
            .prepare(
                "SELECT 1
                 FROM tcloud_backup_jobs
                 WHERE backup_id=?1
                   AND file_path=?2
                   AND file_size=?3
                   AND modified_at=?4
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        q.bind((1, backup_id)).map_err(|e| e.to_string())?;
        q.bind((2, file_path)).map_err(|e| e.to_string())?;
        q.bind((3, size)).map_err(|e| e.to_string())?;
        q.bind((4, modified)).map_err(|e| e.to_string())?;

        if matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            return Ok(false);
        }
    }

    // Reutilizar tcloud_files quando o caminho já existe.
    let file_id = {
        let mut q = conn
            .prepare(
                "SELECT id
                 FROM tcloud_files
                 WHERE path=?1
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        q.bind((1, file_path)).map_err(|e| e.to_string())?;

        if matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            let id = q.read::<i64, _>(0).map_err(|e| e.to_string())?;

            drop(q);

            let mut update = conn
                .prepare(
                    "UPDATE tcloud_files
                     SET name=?1,
                         size=?2,
                         telegram_chat_id=?3,
                         local_fingerprint=?4,
                         sync_status='pending_upload',
                         local_status='local',
                         updated_at=?5
                     WHERE id=?6",
                )
                .map_err(|e| e.to_string())?;

            update.bind((1, name.as_str())).map_err(|e| e.to_string())?;
            update.bind((2, size)).map_err(|e| e.to_string())?;
            update.bind((3, target_folder_id)).map_err(|e| e.to_string())?;
            update.bind((4, fingerprint.as_str())).map_err(|e| e.to_string())?;
            update.bind((5, now)).map_err(|e| e.to_string())?;
            update.bind((6, id)).map_err(|e| e.to_string())?;
            update.next().map_err(|e| e.to_string())?;

            id
        } else {
            drop(q);

            let mut insert = conn
                .prepare(
                    "INSERT INTO tcloud_files(
                        folder_id,
                        name,
                        path,
                        size,
                        telegram_chat_id,
                        sync_status,
                        local_status,
                        local_fingerprint,
                        created_at,
                        updated_at
                     )
                     VALUES(
                        1,
                        ?1,
                        ?2,
                        ?3,
                        ?4,
                        'pending_upload',
                        'local',
                        ?5,
                        ?6,
                        ?6
                     )",
                )
                .map_err(|e| e.to_string())?;

            insert.bind((1, name.as_str())).map_err(|e| e.to_string())?;
            insert.bind((2, file_path)).map_err(|e| e.to_string())?;
            insert.bind((3, size)).map_err(|e| e.to_string())?;
            insert.bind((4, target_folder_id)).map_err(|e| e.to_string())?;
            insert.bind((5, fingerprint.as_str())).map_err(|e| e.to_string())?;
            insert.bind((6, now)).map_err(|e| e.to_string())?;
            insert.next().map_err(|e| e.to_string())?;

            crate::db::last_insert_rowid(&conn)?
        }
    };

    // Evitar job de upload duplicado.
    {
        let mut existing = conn
            .prepare(
                "SELECT id
                 FROM tcloud_sync_jobs
                 WHERE file_id=?1
                   AND operation IN ('upload','refresh')
                   AND status IN ('pending','running')
                 LIMIT 1",
            )
            .map_err(|e| e.to_string())?;

        existing.bind((1, file_id)).map_err(|e| e.to_string())?;

        if matches!(
            existing.next().map_err(|e| e.to_string())?,
            sqlite::State::Row
        ) {
            let sync_job_id =
                existing.read::<i64, _>(0).map_err(|e| e.to_string())?;

            drop(existing);

            let mut link = conn
                .prepare(
                    "INSERT OR REPLACE INTO tcloud_backup_jobs(
                        backup_id,
                        file_path,
                        file_id,
                        sync_job_id,
                        file_size,
                        modified_at,
                        created_at
                     )
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                )
                .map_err(|e| e.to_string())?;

            link.bind((1, backup_id)).map_err(|e| e.to_string())?;
            link.bind((2, file_path)).map_err(|e| e.to_string())?;
            link.bind((3, file_id)).map_err(|e| e.to_string())?;
            link.bind((4, sync_job_id)).map_err(|e| e.to_string())?;
            link.bind((5, size)).map_err(|e| e.to_string())?;
            link.bind((6, modified)).map_err(|e| e.to_string())?;
            link.bind((7, now)).map_err(|e| e.to_string())?;
            link.next().map_err(|e| e.to_string())?;

            return Ok(false);
        }
    }

    let sync_job_id = {
        let mut job = conn
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
                 VALUES(?1,'upload','pending',?2,?2,90,0,NULL)",
            )
            .map_err(|e| e.to_string())?;

        job.bind((1, file_id)).map_err(|e| e.to_string())?;
        job.bind((2, now)).map_err(|e| e.to_string())?;
        job.next().map_err(|e| e.to_string())?;

        crate::db::last_insert_rowid(&conn)?
    };

    {
        let mut link = conn
            .prepare(
                "INSERT OR REPLACE INTO tcloud_backup_jobs(
                    backup_id,
                    file_path,
                    file_id,
                    sync_job_id,
                    file_size,
                    modified_at,
                    created_at
                 )
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
            )
            .map_err(|e| e.to_string())?;

        link.bind((1, backup_id)).map_err(|e| e.to_string())?;
        link.bind((2, file_path)).map_err(|e| e.to_string())?;
        link.bind((3, file_id)).map_err(|e| e.to_string())?;
        link.bind((4, sync_job_id)).map_err(|e| e.to_string())?;
        link.bind((5, size)).map_err(|e| e.to_string())?;
        link.bind((6, modified)).map_err(|e| e.to_string())?;
        link.bind((7, now)).map_err(|e| e.to_string())?;
        link.next().map_err(|e| e.to_string())?;
    }

    drop(conn);

    crate::sync::scheduler::signal_sync_work();

    Ok(true)
}

fn reconcile_backup_jobs(db: &DbConnection) -> Result<usize, String> {
    let conn = db
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    ensure_schema(&conn)?;

    let mut completed = Vec::<(i64, String, i64, i64)>::new();
    let mut touched_backups = Vec::<i64>::new();

    {
        let mut q = conn
            .prepare(
                "SELECT b.backup_id,
                        b.file_path,
                        b.file_size,
                        b.modified_at
                 FROM tcloud_backup_jobs b
                 JOIN tcloud_sync_jobs j
                   ON j.id=b.sync_job_id
                 WHERE j.status='completed'",
            )
            .map_err(|e| e.to_string())?;

        while matches!(
            q.next().map_err(|e| e.to_string())?,
            sqlite::State::Row
        ) {
            completed.push((
                q.read::<i64, _>(0).map_err(|e| e.to_string())?,
                q.read::<String, _>(1).map_err(|e| e.to_string())?,
                q.read::<i64, _>(2).unwrap_or(0),
                q.read::<i64, _>(3).unwrap_or(0),
            ));
        }
    }

    let now = chrono::Utc::now().timestamp();

    for (backup_id, file_path, size, modified) in &completed {
        if !touched_backups.contains(backup_id) {
            touched_backups.push(*backup_id);
        }

        let mut state = conn
            .prepare(
                "INSERT OR REPLACE INTO tcloud_backup_state(
                    backup_id,
                    file_path,
                    file_size,
                    modified_at,
                    uploaded_at
                 )
                 VALUES(?1,?2,?3,?4,?5)",
            )
            .map_err(|e| e.to_string())?;

        state.bind((1, *backup_id)).map_err(|e| e.to_string())?;
        state.bind((2, file_path.as_str())).map_err(|e| e.to_string())?;
        state.bind((3, *size)).map_err(|e| e.to_string())?;
        state.bind((4, *modified)).map_err(|e| e.to_string())?;
        state.bind((5, now)).map_err(|e| e.to_string())?;
        state.next().map_err(|e| e.to_string())?;

        let mut clear = conn
            .prepare(
                "DELETE FROM tcloud_backup_jobs
                 WHERE backup_id=?1 AND file_path=?2",
            )
            .map_err(|e| e.to_string())?;

        clear.bind((1, *backup_id)).map_err(|e| e.to_string())?;
        clear.bind((2, file_path.as_str())).map_err(|e| e.to_string())?;
        clear.next().map_err(|e| e.to_string())?;
    }

    // Um backup so e considerado concluido quando nao resta nenhum
    // arquivo daquele backup aguardando o sync processor.
    for backup_id in touched_backups {
        let remaining = {
            let mut q = conn
                .prepare(
                    "SELECT COUNT(*)
                     FROM tcloud_backup_jobs
                     WHERE backup_id=?1",
                )
                .map_err(|e| e.to_string())?;

            q.bind((1, backup_id)).map_err(|e| e.to_string())?;

            if matches!(
                q.next().map_err(|e| e.to_string())?,
                sqlite::State::Row
            ) {
                q.read::<i64, _>(0).unwrap_or(0)
            } else {
                0
            }
        };

        if remaining == 0 {
            let mut success = conn
                .prepare(
                    "UPDATE tcloud_backup_sources
                     SET last_success_at=?1
                     WHERE id=?2",
                )
                .map_err(|e| e.to_string())?;

            success.bind((1, now)).map_err(|e| e.to_string())?;
            success.bind((2, backup_id)).map_err(|e| e.to_string())?;
            success.next().map_err(|e| e.to_string())?;
        }
    }

    Ok(completed.len())
}

fn run_backup_scheduler_tick(db: &DbConnection) -> Result<usize, String> {
    let _ = reconcile_backup_jobs(db)?;

    let now = chrono::Utc::now().timestamp();

    let backups = {
        let conn = db
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;

        ensure_schema(&conn)?;

        let mut q = conn
            .prepare(
                "SELECT id,
                        local_path,
                        target_folder_id,
                        interval_minutes,
                        COALESCE(last_scan_at,last_run_at)
                 FROM tcloud_backup_sources
                 WHERE enabled=1
                 ORDER BY id",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = Vec::new();

        while matches!(
            q.next().map_err(|e| e.to_string())?,
            sqlite::State::Row
        ) {
            rows.push((
                q.read::<i64, _>(0).map_err(|e| e.to_string())?,
                q.read::<String, _>(1).map_err(|e| e.to_string())?,
                q.read::<Option<i64>, _>(2).unwrap_or(None),
                q.read::<i64, _>(3).unwrap_or(1440).max(15),
                q.read::<Option<i64>, _>(4).unwrap_or(None),
            ));
        }

        rows
    };

    let mut queued = 0usize;

    for (backup_id, local_path, target_folder_id, interval_minutes, last_scan_at) in backups {
        let due = last_scan_at
            .map(|last| now.saturating_sub(last) >= interval_minutes.saturating_mul(60))
            .unwrap_or(true);

        if !due {
            continue;
        }

        let root = Path::new(&local_path);

        if !root.exists() || !root.is_dir() {
            log::warn!(
                "TCloud backup {} ignorado: pasta inexistente {}",
                backup_id,
                local_path
            );
            continue;
        }

        let mut files = Vec::new();
        collect_backup_files(root, &mut files, 10_000);

        for file_path in files {
            match enqueue_backup_file(
                db,
                backup_id,
                &file_path,
                target_folder_id,
            ) {
                Ok(true) => queued += 1,
                Ok(false) => {}
                Err(error) => {
                    log::warn!(
                        "TCloud backup {}: falha ao enfileirar {}: {}",
                        backup_id,
                        file_path,
                        error
                    );
                }
            }
        }

        let conn = db
            .lock()
            .map_err(|_| "database lock poisoned".to_string())?;

        let mut update = conn
            .prepare(
                "UPDATE tcloud_backup_sources
                 SET last_scan_at=?1,
                     last_run_at=?1
                 WHERE id=?2",
            )
            .map_err(|e| e.to_string())?;

        update.bind((1, now)).map_err(|e| e.to_string())?;
        update.bind((2, backup_id)).map_err(|e| e.to_string())?;
        update.next().map_err(|e| e.to_string())?;
    }

    Ok(queued)
}

pub fn start_backup_scheduler(db: DbConnection) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        loop {
            let scheduler_db = db.clone();

            match tokio::task::spawn_blocking(move || {
                run_backup_scheduler_tick(&scheduler_db)
            })
            .await
            {
                Ok(Ok(queued)) if queued > 0 => {
                    log::info!(
                        "TCloud backup scheduler: {} arquivo(s) enfileirado(s).",
                        queued
                    );
                }
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    log::warn!("TCloud backup scheduler: {}", error);
                }
                Err(error) => {
                    log::warn!(
                        "TCloud backup scheduler worker interrompido: {}",
                        error
                    );
                }
            }

            tokio::time::sleep(
                std::time::Duration::from_secs(60)
            )
            .await;
        }
    });
}

#[tauri::command]
pub fn tcloud_pro_backups_list(db: State<'_, DbConnection>) -> Result<Vec<ProBackupSource>, String> {
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut stmt=conn.prepare("SELECT id,local_path,target_folder_id,enabled,interval_minutes,last_run_at,last_scan_at,last_success_at FROM tcloud_backup_sources ORDER BY id").map_err(|e|e.to_string())?;let mut out=Vec::new();
    while let sqlite::State::Row=stmt.next().map_err(|e|e.to_string())?{out.push(ProBackupSource{id:stmt.read(0).map_err(|e|e.to_string())?,local_path:stmt.read(1).map_err(|e|e.to_string())?,target_folder_id:stmt.read(2).ok(),enabled:stmt.read::<i64,_>(3).unwrap_or(1)!=0,interval_minutes:stmt.read(4).unwrap_or(1440),last_run_at:stmt.read(5).ok(),last_scan_at:stmt.read(6).ok(),last_success_at:stmt.read(7).ok()});}Ok(out)
}

#[tauri::command]
pub fn tcloud_pro_backup_save(db: State<'_, DbConnection>, id: Option<i64>, local_path: String, target_folder_id: Option<i64>, enabled: bool, interval_minutes: i64) -> Result<i64,String>{
    if !Path::new(&local_path).exists(){return Err("A pasta de backup não existe".to_string());}
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    if let Some(id)=id{let mut stmt=conn.prepare("UPDATE tcloud_backup_sources SET local_path=?1,target_folder_id=?2,enabled=?3,interval_minutes=?4 WHERE id=?5").map_err(|e|e.to_string())?;stmt.bind((1,local_path.as_str())).map_err(|e|e.to_string())?;stmt.bind((2,target_folder_id)).map_err(|e|e.to_string())?;stmt.bind((3,if enabled{1_i64}else{0_i64})).map_err(|e|e.to_string())?;stmt.bind((4,interval_minutes.max(15))).map_err(|e|e.to_string())?;stmt.bind((5,id)).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;Ok(id)}else{let mut stmt=conn.prepare("INSERT INTO tcloud_backup_sources(local_path,target_folder_id,enabled,interval_minutes) VALUES(?1,?2,?3,?4)").map_err(|e|e.to_string())?;stmt.bind((1,local_path.as_str())).map_err(|e|e.to_string())?;stmt.bind((2,target_folder_id)).map_err(|e|e.to_string())?;stmt.bind((3,if enabled{1_i64}else{0_i64})).map_err(|e|e.to_string())?;stmt.bind((4,interval_minutes.max(15))).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;Ok(crate::db::last_insert_rowid(&conn)?)}
}

#[tauri::command]
pub fn tcloud_pro_backup_delete(db:State<'_,DbConnection>,id:i64)->Result<bool,String>{let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut stmt=conn.prepare("DELETE FROM tcloud_backup_sources WHERE id=?1").map_err(|e|e.to_string())?;stmt.bind((1,id)).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;Ok(true)}

fn scan_dir(path: &Path, files: &mut usize, bytes: &mut u64) {
    let Ok(read) = std::fs::read_dir(path) else { return; };
    for entry in read.flatten() {
        let p = entry.path();
        if p.is_dir() { scan_dir(&p, files, bytes); }
        else if let Ok(meta) = entry.metadata() { *files += 1; *bytes += meta.len(); }
    }
}

#[tauri::command]
pub fn tcloud_pro_backup_scan(db: State<'_, DbConnection>, id: i64) -> Result<ProBackupScanResult, String> {
    let path = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q = conn.prepare("SELECT local_path FROM tcloud_backup_sources WHERE id=?1").map_err(|e| e.to_string())?;
        q.bind((1, id)).map_err(|e| e.to_string())?;
        if q.next().map_err(|e| e.to_string())? != sqlite::State::Row {
            return Err("Backup não encontrado".to_string());
        }
        q.read::<String, _>(0).map_err(|e| e.to_string())?
    };
    let mut files = 0usize;
    let mut bytes = 0u64;
    scan_dir(Path::new(&path), &mut files, &mut bytes);
    Ok(ProBackupScanResult { files_found: files, total_bytes: bytes })
}

#[tauri::command]
pub fn tcloud_pro_offline_pin(db:State<'_,DbConnection>,file_id:i64,pinned:bool)->Result<bool,String>{let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;if pinned{let mut s=conn.prepare("INSERT OR REPLACE INTO tcloud_offline_pins(file_id,pinned_at) VALUES(?1,?2)").map_err(|e|e.to_string())?;s.bind((1,file_id)).map_err(|e|e.to_string())?;s.bind((2,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;}else{let mut s=conn.prepare("DELETE FROM tcloud_offline_pins WHERE file_id=?1").map_err(|e|e.to_string())?;s.bind((1,file_id)).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;}Ok(pinned)}


fn collect_file_metadata(path: &Path, output: &mut Vec<(String, i64, i64)>, limit: usize) {
    if output.len() >= limit { return; }
    let Ok(read) = std::fs::read_dir(path) else { return; };
    for entry in read.flatten() {
        if output.len() >= limit { break; }
        let p = entry.path();
        if p.is_dir() {
            collect_file_metadata(&p, output, limit);
        } else if p.is_file() {
            if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0);
                output.push((p.to_string_lossy().to_string(), meta.len() as i64, modified));
            }
        }
    }
}

#[tauri::command]
pub fn tcloud_pro_backup_files(db: State<'_, DbConnection>, id: i64) -> Result<Vec<String>, String> {
    let (path, previous) = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q = conn.prepare("SELECT local_path FROM tcloud_backup_sources WHERE id=?1 AND enabled=1").map_err(|e| e.to_string())?;
        q.bind((1, id)).map_err(|e| e.to_string())?;
        if q.next().map_err(|e| e.to_string())? != sqlite::State::Row {
            return Err("Backup não encontrado ou desativado".to_string());
        }
        let path = q.read::<String, _>(0).map_err(|e| e.to_string())?;
        drop(q);
        let mut previous = std::collections::HashMap::<String, (i64, i64)>::new();
        let mut state = conn.prepare("SELECT file_path,file_size,modified_at FROM tcloud_backup_state WHERE backup_id=?1").map_err(|e| e.to_string())?;
        state.bind((1, id)).map_err(|e| e.to_string())?;
        while let sqlite::State::Row = state.next().map_err(|e| e.to_string())? {
            let file_path: String = state.read(0).map_err(|e| e.to_string())?;
            let size: i64 = state.read(1).unwrap_or(-1);
            let modified: i64 = state.read(2).unwrap_or(-1);
            previous.insert(file_path, (size, modified));
        }
        (path, previous)
    };
    let mut discovered = Vec::new();
    collect_file_metadata(Path::new(&path), &mut discovered, 10000);
    let mut changed = Vec::new();
    for (file_path, size, modified) in discovered {
        if previous.get(&file_path).copied() != Some((size, modified)) {
            changed.push(file_path);
        }
    }
    Ok(changed)
}

#[tauri::command]
pub fn tcloud_pro_backup_mark_file(db:State<'_,DbConnection>,id:i64,file_path:String)->Result<bool,String>{
    let meta=std::fs::metadata(&file_path).map_err(|e|e.to_string())?;
    let modified=meta.modified().ok().and_then(|time|time.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_secs() as i64).unwrap_or(0);
    let now=chrono::Utc::now().timestamp();let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut s=conn.prepare("INSERT OR REPLACE INTO tcloud_backup_state(backup_id,file_path,file_size,modified_at,uploaded_at) VALUES(?1,?2,?3,?4,?5)").map_err(|e|e.to_string())?;
    s.bind((1,id)).map_err(|e|e.to_string())?;s.bind((2,file_path.as_str())).map_err(|e|e.to_string())?;s.bind((3,meta.len() as i64)).map_err(|e|e.to_string())?;s.bind((4,modified)).map_err(|e|e.to_string())?;s.bind((5,now)).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;Ok(true)
}

#[tauri::command]
pub fn tcloud_pro_backup_mark_run(db:State<'_,DbConnection>,id:i64)->Result<i64,String>{
    let now=chrono::Utc::now().timestamp();let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut s=conn.prepare("UPDATE tcloud_backup_sources SET last_run_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;s.bind((1,now)).map_err(|e|e.to_string())?;s.bind((2,id)).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;Ok(now)
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProDuplicateFile {
    pub file_id: i64,
    pub telegram_message_id: Option<i64>,
    pub telegram_chat_id: Option<i64>,
    pub name: String,
    pub size: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProDuplicateGroup {
    pub sha256: String,
    pub size: i64,
    pub reclaimable_bytes: i64,
    pub files: Vec<ProDuplicateFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProCleanupStats {
    pub cached_files: i64,
    pub cached_bytes: i64,
    pub duplicate_groups: i64,
    pub duplicate_reclaimable_bytes: i64,
    pub trash_items: i64,
    pub versions: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProContentHit {
    pub file_id: i64,
    pub telegram_message_id: Option<i64>,
    pub telegram_chat_id: Option<i64>,
    pub name: String,
    pub size: i64,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProActivityItem {
    pub id: i64,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
    pub created_at: i64,
}

fn ensure_pro9_schema(conn: &sqlite::Connection) -> Result<(), String> {
    ensure_schema(conn)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tcloud_content_index (
            file_id INTEGER PRIMARY KEY,
            content TEXT NOT NULL DEFAULT '',
            indexed_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_content_index_time ON tcloud_content_index(indexed_at DESC);

        CREATE TABLE IF NOT EXISTS tcloud_activity (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            detail TEXT,
            created_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_activity_time ON tcloud_activity(created_at DESC);"
    ).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn tcloud_pro_duplicates(db: State<'_, DbConnection>) -> Result<Vec<ProDuplicateGroup>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;

    let mut hashes = conn.prepare(
        "SELECT sha256, MAX(size), COUNT(*), COALESCE(SUM(size)-MAX(size),0)
         FROM tcloud_files
         WHERE sha256 IS NOT NULL AND sha256 != '' AND sync_status != 'trashed'
         GROUP BY sha256 HAVING COUNT(*) > 1
         ORDER BY COALESCE(SUM(size)-MAX(size),0) DESC LIMIT 100"
    ).map_err(|e| e.to_string())?;

    let mut groups = Vec::new();
    while let sqlite::State::Row = hashes.next().map_err(|e| e.to_string())? {
        let sha256 = hashes.read::<String,_>(0).map_err(|e| e.to_string())?;
        let size = hashes.read::<i64,_>(1).unwrap_or(0);
        let reclaimable_bytes = hashes.read::<i64,_>(3).unwrap_or(0);
        let mut files_stmt = conn.prepare(
            "SELECT id,telegram_message_id,telegram_chat_id,name,size,sha256
             FROM tcloud_files WHERE sha256=?1 AND sync_status != 'trashed' ORDER BY updated_at DESC"
        ).map_err(|e| e.to_string())?;
        files_stmt.bind((1, sha256.as_str())).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        while let sqlite::State::Row = files_stmt.next().map_err(|e| e.to_string())? {
            files.push(ProDuplicateFile {
                file_id: files_stmt.read(0).map_err(|e| e.to_string())?,
                telegram_message_id: files_stmt.read::<Option<i64>,_>(1).unwrap_or(None),
                telegram_chat_id: files_stmt.read::<Option<i64>,_>(2).unwrap_or(None),
                name: files_stmt.read(3).map_err(|e| e.to_string())?,
                size: files_stmt.read(4).unwrap_or(0),
                sha256: files_stmt.read(5).map_err(|e| e.to_string())?,
            });
        }
        groups.push(ProDuplicateGroup { sha256, size, reclaimable_bytes, files });
    }
    Ok(groups)
}

#[tauri::command]
pub fn tcloud_pro_cleanup_stats(db: State<'_, DbConnection>) -> Result<ProCleanupStats, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;
    let duplicate_groups = scalar_i64(&conn,
        "SELECT COUNT(*) FROM (SELECT sha256 FROM tcloud_files WHERE sha256 IS NOT NULL AND sha256 != '' AND sync_status != 'trashed' GROUP BY sha256 HAVING COUNT(*) > 1)"
    );
    let duplicate_reclaimable_bytes = scalar_i64(&conn,
        "SELECT COALESCE(SUM(reclaimable),0) FROM (SELECT SUM(size)-MAX(size) AS reclaimable FROM tcloud_files WHERE sha256 IS NOT NULL AND sha256 != '' AND sync_status != 'trashed' GROUP BY sha256 HAVING COUNT(*) > 1)"
    );
    Ok(ProCleanupStats {
        cached_files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_cache"),
        cached_bytes: scalar_i64(&conn, "SELECT COALESCE(SUM(f.size),0) FROM tcloud_cache c JOIN tcloud_files f ON f.id=c.file_id"),
        duplicate_groups,
        duplicate_reclaimable_bytes,
        trash_items: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_trash"),
        versions: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_versions"),
    })
}

#[tauri::command]
pub fn tcloud_pro_reindex_text_content(db: State<'_, DbConnection>) -> Result<usize, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT f.id,f.name,c.local_path FROM tcloud_cache c JOIN tcloud_files f ON f.id=c.file_id"
    ).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        rows.push((
            stmt.read::<i64,_>(0).map_err(|e| e.to_string())?,
            stmt.read::<String,_>(1).map_err(|e| e.to_string())?,
            stmt.read::<String,_>(2).map_err(|e| e.to_string())?,
        ));
    }
    drop(stmt);

    let allowed = ["txt","md","csv","json","log","xml","html","htm","ini","yaml","yml","toml","rs","ts","tsx","js","jsx","css","py","sql"];
    let now = chrono::Utc::now().timestamp();
    let mut indexed = 0usize;
    for (file_id, name, local_path) in rows {
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !allowed.contains(&ext.as_str()) { continue; }
        let Ok(bytes) = std::fs::read(&local_path) else { continue; };
        let limited = if bytes.len() > 2_000_000 { &bytes[..2_000_000] } else { &bytes[..] };
        let content = String::from_utf8_lossy(limited).to_string();
        let mut upsert = conn.prepare(
            "INSERT INTO tcloud_content_index(file_id,content,indexed_at) VALUES(?1,?2,?3)
             ON CONFLICT(file_id) DO UPDATE SET content=excluded.content,indexed_at=excluded.indexed_at"
        ).map_err(|e| e.to_string())?;
        upsert.bind((1, file_id)).map_err(|e| e.to_string())?;
        upsert.bind((2, content.as_str())).map_err(|e| e.to_string())?;
        upsert.bind((3, now)).map_err(|e| e.to_string())?;
        upsert.next().map_err(|e| e.to_string())?;
        indexed += 1;
    }
    Ok(indexed)
}

fn tcloud_pro_content_search_inner(db: &DbConnection, query: &str) -> Result<Vec<ProContentHit>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;
    let trimmed = query.trim();
    let needle = format!("%{}%", trimmed.replace('%', "\\%").replace('_', "\\_"));
    let normalized_query = trimmed.to_lowercase();
    let mut stmt = conn.prepare(
        "SELECT f.id,f.telegram_message_id,f.telegram_chat_id,f.name,f.size,COALESCE(ci.content,'')
         FROM tcloud_files f LEFT JOIN tcloud_content_index ci ON ci.file_id=f.id
         WHERE f.sync_status!='trashed' AND (f.name LIKE ?1 ESCAPE '\\' OR ci.content LIKE ?1 ESCAPE '\\')
         ORDER BY f.updated_at DESC LIMIT 200"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, needle.as_str())).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let content = stmt.read::<String,_>(5).unwrap_or_default();
        let lower = content.to_lowercase();
        let pos = lower.find(&normalized_query).unwrap_or(0);
        let start = pos.saturating_sub(80);
        let end = std::cmp::min(content.len(), pos.saturating_add(normalized_query.len()).saturating_add(120));
        let snippet = content.get(start..end).unwrap_or("").replace('\n', " ").replace('\r', " ");
        out.push(ProContentHit {
            file_id: stmt.read(0).map_err(|e| e.to_string())?,
            telegram_message_id: stmt.read::<Option<i64>,_>(1).unwrap_or(None),
            telegram_chat_id: stmt.read::<Option<i64>,_>(2).unwrap_or(None),
            name: stmt.read(3).map_err(|e| e.to_string())?,
            size: stmt.read(4).unwrap_or(0),
            snippet,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn tcloud_pro_content_search(
    db: State<'_, DbConnection>,
    query: String,
) -> Result<Vec<ProContentHit>, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let result = tcloud_pro_content_search_inner(&db, &query);
        let elapsed = started.elapsed();
        if elapsed >= std::time::Duration::from_millis(500) {
            log::debug!("TCloud: busca no índice local levou {:?}; executada fora do runtime/UI.", elapsed);
        }
        result
    })
        .await
        .map_err(|e| format!("content search worker failed: {e}"))?
}

#[tauri::command]
pub fn tcloud_pro_activity_add(db: State<'_, DbConnection>, kind: String, title: String, detail: Option<String>) -> Result<i64, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare("INSERT INTO tcloud_activity(kind,title,detail,created_at) VALUES(?1,?2,?3,?4)").map_err(|e| e.to_string())?;
    stmt.bind((1, kind.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, title.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((3, detail.as_deref())).map_err(|e| e.to_string())?;
    stmt.bind((4, now)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(crate::db::last_insert_rowid(&conn).unwrap_or(0))
}

#[tauri::command]
pub fn tcloud_pro_activity_list(db: State<'_, DbConnection>, limit: Option<i64>) -> Result<Vec<ProActivityItem>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro9_schema(&conn)?;
    let limit = limit.unwrap_or(100).clamp(1, 500);
    let mut stmt = conn.prepare("SELECT id,kind,title,detail,created_at FROM tcloud_activity ORDER BY created_at DESC LIMIT ?1").map_err(|e| e.to_string())?;
    stmt.bind((1, limit)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        out.push(ProActivityItem {
            id: stmt.read(0).map_err(|e| e.to_string())?,
            kind: stmt.read(1).map_err(|e| e.to_string())?,
            title: stmt.read(2).map_err(|e| e.to_string())?,
            detail: stmt.read::<Option<String>,_>(3).unwrap_or(None),
            created_at: stmt.read(4).unwrap_or(0),
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn tcloud_pro_pause_all_jobs(db: State<'_, DbConnection>, paused: bool) -> Result<usize, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let now = chrono::Utc::now().timestamp();
    let sql = if paused {
        "UPDATE tcloud_sync_jobs SET paused=1,status=CASE WHEN status IN ('running','uploading','downloading','pending') THEN 'paused' ELSE status END,updated_at=?1 WHERE status NOT IN ('completed','cancelled')"
    } else {
        "UPDATE tcloud_sync_jobs SET paused=0,status=CASE WHEN status='paused' THEN 'pending' ELSE status END,updated_at=?1 WHERE status!='completed'"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    stmt.bind((1, now)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(conn.change_count() as usize)
}

#[tauri::command]
pub fn tcloud_pro_notify(title: String, body: String) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        fn ps_escape(value: &str) -> String { value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('\\', "\\\\").replace('"', "`\"").replace('\'', "''") }
        let title = ps_escape(&title);
        let body = ps_escape(&body);
        let script = format!(r#"
[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] > $null
$xml = New-Object Windows.Data.Xml.Dom.XmlDocument
$xml.LoadXml('<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>')
$toast = New-Object Windows.UI.Notifications.ToastNotification $xml
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('TCloud').Show($toast)
"#, title, body);
        let mut command = Command::new("powershell.exe");
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let output = command.args(["-NoProfile","-NonInteractive","-Command", &script]).output().map_err(|e| e.to_string())?;
        if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).to_string()); }
        return Ok(true);
    }
    #[cfg(not(target_os = "windows"))]
    { let _ = (title, body); Ok(false) }
}

// ── TCloud 10 metadata, diagnostics and storage analytics ─────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProFileMetadata {
    pub message_id: i64,
    pub folder_id: Option<i64>,
    pub tags: Vec<String>,
    pub note: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProStorageBucket {
    pub category: String,
    pub files: i64,
    pub bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProDuplicateSource {
    pub name: String,
    pub message_id: i64,
    pub folder_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProDuplicateMatch {
    pub sha256: String,
    pub size: i64,
    pub matches: i64,
    pub names: Vec<String>,
    pub sources: Vec<ProDuplicateSource>,
}

fn ensure_pro10_schema(conn: &sqlite::Connection) -> Result<(), String> {
    ensure_pro9_schema(conn)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tcloud_file_metadata (
            file_key TEXT PRIMARY KEY,
            message_id INTEGER NOT NULL,
            folder_id INTEGER,
            tags TEXT NOT NULL DEFAULT '',
            note TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_file_metadata_message ON tcloud_file_metadata(message_id, folder_id);"
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn metadata_key(message_id: i64, folder_id: Option<i64>) -> String {
    format!("{}:{}", folder_id.unwrap_or(0), message_id)
}

#[tauri::command]
pub fn tcloud_pro_metadata_get(
    db: State<'_, DbConnection>,
    message_id: i64,
    folder_id: Option<i64>,
) -> Result<ProFileMetadata, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro10_schema(&conn)?;
    let key = metadata_key(message_id, folder_id);
    let mut stmt = conn.prepare(
        "SELECT tags,note,updated_at FROM tcloud_file_metadata WHERE file_key=?1 LIMIT 1"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let tags_raw = stmt.read::<String,_>(0).unwrap_or_default();
        let tags = tags_raw.split(',').map(str::trim).filter(|v| !v.is_empty()).map(str::to_string).collect();
        return Ok(ProFileMetadata {
            message_id,
            folder_id,
            tags,
            note: stmt.read::<String,_>(1).unwrap_or_default(),
            updated_at: stmt.read::<i64,_>(2).unwrap_or(0),
        });
    }
    Ok(ProFileMetadata { message_id, folder_id, tags: Vec::new(), note: String::new(), updated_at: 0 })
}

#[tauri::command]
pub fn tcloud_pro_metadata_set(
    db: State<'_, DbConnection>,
    message_id: i64,
    folder_id: Option<i64>,
    tags: Vec<String>,
    note: String,
) -> Result<ProFileMetadata, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro10_schema(&conn)?;
    let key = metadata_key(message_id, folder_id);
    let now = chrono::Utc::now().timestamp();
    let normalized: Vec<String> = tags.into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(20)
        .collect();
    let tags_raw = normalized.join(",");
    let mut stmt = conn.prepare(
        "INSERT INTO tcloud_file_metadata(file_key,message_id,folder_id,tags,note,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6)
         ON CONFLICT(file_key) DO UPDATE SET tags=excluded.tags,note=excluded.note,updated_at=excluded.updated_at"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, message_id)).map_err(|e| e.to_string())?;
    stmt.bind((3, folder_id)).map_err(|e| e.to_string())?;
    stmt.bind((4, tags_raw.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((5, note.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((6, now)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(ProFileMetadata { message_id, folder_id, tags: normalized, note, updated_at: now })
}

fn storage_category(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg"|"jpeg"|"png"|"gif"|"webp"|"bmp"|"heic"|"avif" => "Imagens",
        "mp4"|"mkv"|"mov"|"avi"|"webm"|"m4v" => "Vídeos",
        "mp3"|"flac"|"wav"|"m4a"|"aac"|"ogg"|"opus" => "Áudio",
        "pdf"|"doc"|"docx"|"xls"|"xlsx"|"ppt"|"pptx"|"txt"|"md"|"csv" => "Documentos",
        "zip"|"rar"|"7z"|"tar"|"gz"|"bz2"|"xz" => "Compactados",
        _ => "Outros",
    }
}

#[tauri::command]
pub fn tcloud_pro_storage_breakdown(db: State<'_, DbConnection>) -> Result<Vec<ProStorageBucket>, String> {
    use std::collections::BTreeMap;
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro10_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT name,size FROM tcloud_files WHERE sync_status!='trashed' ORDER BY updated_at DESC"
    ).map_err(|e| e.to_string())?;
    let mut buckets: BTreeMap<String,(i64,i64)> = BTreeMap::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let name = stmt.read::<String,_>(0).unwrap_or_default();
        let size = stmt.read::<i64,_>(1).unwrap_or(0).max(0);
        let entry = buckets.entry(storage_category(&name).to_string()).or_insert((0,0));
        entry.0 += 1;
        entry.1 += size;
    }
    let order = ["Vídeos","Imagens","Áudio","Documentos","Compactados","Outros"];
    let mut out = Vec::new();
    for category in order {
        let (files, bytes) = buckets.remove(category).unwrap_or((0,0));
        out.push(ProStorageBucket { category: category.to_string(), files, bytes });
    }
    Ok(out)
}

#[tauri::command]
pub fn tcloud_pro_local_duplicate_check(
    db: State<'_, DbConnection>,
    path: String,
) -> Result<ProDuplicateMatch, String> {
    let path_ref = std::path::Path::new(&path);
    let size = std::fs::metadata(path_ref).map_err(|e| format!("Could not inspect local file: {e}"))?.len() as i64;
    let sha256 = crate::sync::hash::sha256_file(path_ref)?;
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro10_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT name,telegram_message_id,telegram_chat_id FROM tcloud_files WHERE sha256=?1 AND sync_status!='trashed' AND telegram_message_id IS NOT NULL ORDER BY updated_at DESC LIMIT 10"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, sha256.as_str())).map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    let mut sources = Vec::new();
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let name=stmt.read::<String,_>(0).unwrap_or_default();
        let message_id=stmt.read::<i64,_>(1).unwrap_or(0);
        let folder_raw=stmt.read::<Option<i64>,_>(2).unwrap_or(None);
        names.push(name.clone());
        if message_id>0 { sources.push(ProDuplicateSource{name,message_id,folder_id:folder_raw.and_then(|v| if v==0{None}else{Some(v)})}); }
    }
    Ok(ProDuplicateMatch { sha256, size, matches: names.len() as i64, names, sources })
}

#[tauri::command]
pub fn tcloud_pro_export_diagnostics(
    db: State<'_, DbConnection>,
    output_path: String,
) -> Result<String, String> {
    use std::io::Write;
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_pro10_schema(&conn)?;
    let stats = ProIndexStats {
        files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files"),
        folders: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_folders"),
        indexed_bytes: scalar_i64(&conn, "SELECT COALESCE(SUM(size),0) FROM tcloud_files"),
        cached_files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_cache"),
        offline_files: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files WHERE local_status!='online'"),
        pending_jobs: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status IN ('pending','running','uploading','downloading')"),
        conflicts: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_conflicts WHERE resolution='pending'"),
        trash_items: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_trash"),
        versions: scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_versions"),
    };
    let quick_check = conn.prepare("PRAGMA quick_check(1)").ok().and_then(|mut stmt| {
        if matches!(stmt.next(), Ok(sqlite::State::Row)) { stmt.read::<String,_>(0).ok() } else { None }
    }).unwrap_or_else(|| "unavailable".into());
    let current_failures = scalar_i64(&conn, "SELECT COUNT(DISTINCT j.file_id) FROM tcloud_sync_jobs j JOIN tcloud_files f ON f.id=j.file_id WHERE j.status='failed' AND COALESCE(f.sync_status,'') NOT IN ('synced','deleted','trashed')");
    let stalled_transfers = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status='running' AND COALESCE(updated_at,0) < strftime('%s','now') - 21600");
    let cfapi_pending = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_cfapi_pending_ops WHERE completed_at IS NULL");
    let cfapi_failed = scalar_i64(&conn, "SELECT COUNT(DISTINCT file_id) FROM tcloud_cfapi_pending_ops WHERE completed_at IS NULL AND attempts>=12");
    let cloud_only = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files WHERE local_status='cloud_only' AND sync_status NOT IN ('deleted','trashed')");
    let cached = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_files WHERE local_status='cached' AND sync_status NOT IN ('deleted','trashed')");
    let offline_pinned = scalar_i64(&conn, "SELECT COUNT(*) FROM tcloud_offline_pins");
    let previous_unclean = {
        let mut value = String::new();
        if let Ok(mut stmt) = conn.prepare("SELECT value FROM tcloud_settings WHERE key='stability_previous_unclean' LIMIT 1") {
            if matches!(stmt.next(), Ok(sqlite::State::Row)) { value = stmt.read::<String,_>(0).unwrap_or_default(); }
        }
        value == "1"
    };
    drop(conn);
    let updater_configured = option_env!("TCLOUD_UPDATER_CONFIGURED") == Some("1");
    let updater_channel = option_env!("TCLOUD_UPDATER_CHANNEL").unwrap_or("not-configured");
    let updater_endpoint_host = option_env!("TCLOUD_UPDATER_ENDPOINT_HOST").unwrap_or("");
    let summary = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "stats": stats,
        "health": {
            "sqlite_quick_check": quick_check,
            "previous_unclean_shutdown": previous_unclean,
            "current_failed_files": current_failures,
            "stalled_transfers_over_6h": stalled_transfers,
            "cfapi_pending_operations": cfapi_pending,
            "cfapi_failed_operations": cfapi_failed
        },
        "availability": {
            "cloud_only_files": cloud_only,
            "cached_files": cached,
            "offline_pinned_files": offline_pinned
        },
        "updater": {
            "configured_at_build": updater_configured,
            "channel": updater_channel,
            "endpoint_host": updater_endpoint_host,
            "signature_verification": "required"
        },
        "privacy": "API credentials, Telegram session data, passwords, file names, paths and file contents are intentionally excluded"
    });
    let file = std::fs::File::create(&output_path).map_err(|e| format!("Could not create diagnostics ZIP: {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("summary.json", options).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    zip.start_file("README.txt", options).map_err(|e| e.to_string())?;
    zip.write_all(b"TCloud diagnostics export. Credentials, Telegram session data, file names, paths and file contents are not included.\n").map_err(|e| e.to_string())?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(output_path)
}

#[tauri::command]
pub fn tcloud_pro_export_config(output_path: String, json: String) -> Result<String, String> {
    serde_json::from_str::<serde_json::Value>(&json).map_err(|e| format!("Invalid configuration JSON: {e}"))?;
    std::fs::write(&output_path, json.as_bytes()).map_err(|e| format!("Could not save configuration: {e}"))?;
    Ok(output_path)
}

#[tauri::command]
pub fn tcloud_pro_import_config(path: String) -> Result<String, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("Could not read configuration: {e}"))?;
    serde_json::from_str::<serde_json::Value>(&raw).map_err(|e| format!("Invalid configuration JSON: {e}"))?;
    Ok(raw)
}
