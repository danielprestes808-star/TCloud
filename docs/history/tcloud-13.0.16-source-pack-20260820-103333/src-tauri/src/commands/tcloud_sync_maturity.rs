use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderSyncPolicy {
    pub relative_path: String,
    pub mode: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMaturitySettings {
    pub version_retention: i64,
    pub trash_retention_days: i64,
    pub conflict_strategy: String,
}

fn ensure_schema(conn: &sqlite::Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tcloud_folder_sync_policies (
            relative_path TEXT PRIMARY KEY,
            mode TEXT NOT NULL DEFAULT 'normal',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tcloud_sync_preferences (
            id INTEGER PRIMARY KEY CHECK(id=1),
            version_retention INTEGER NOT NULL DEFAULT 10,
            trash_retention_days INTEGER NOT NULL DEFAULT 30,
            conflict_strategy TEXT NOT NULL DEFAULT 'keep_both',
            updated_at INTEGER NOT NULL DEFAULT 0
        );
        INSERT OR IGNORE INTO tcloud_sync_preferences(id,version_retention,trash_retention_days,conflict_strategy,updated_at)
            VALUES(1,10,30,'keep_both',0);"
    ).map_err(|e| e.to_string())?;
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
    Ok(())
}

fn normalize_relative(value: &str) -> String {
    value.replace('\\', "/").trim_matches('/').to_string()
}

pub(crate) fn folder_policy_for_relative(conn: &sqlite::Connection, relative: &str) -> Result<String, String> {
    ensure_schema(conn)?;
    let relative = normalize_relative(relative);
    let mut stmt = conn.prepare(
        "SELECT relative_path,mode FROM tcloud_folder_sync_policies ORDER BY LENGTH(relative_path) DESC"
    ).map_err(|e| e.to_string())?;
    while let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        let prefix = stmt.read::<String,_>(0).unwrap_or_default();
        let mode = stmt.read::<String,_>(1).unwrap_or_else(|_| "normal".into());
        if prefix.is_empty() || relative.eq_ignore_ascii_case(&prefix) || relative.to_ascii_lowercase().starts_with(&(prefix.to_ascii_lowercase() + "/")) {
            return Ok(mode);
        }
    }
    Ok("normal".into())
}

pub(crate) fn version_retention(db: &DbConnection) -> i64 {
    let Ok(conn) = db.lock() else { return 10; };
    if ensure_schema(&conn).is_err() { return 10; }
    let Ok(mut stmt) = conn.prepare("SELECT version_retention FROM tcloud_sync_preferences WHERE id=1") else { return 10; };
    if matches!(stmt.next(), Ok(sqlite::State::Row)) { stmt.read::<i64,_>(0).unwrap_or(10).clamp(0,100) } else { 10 }
}

pub(crate) fn record_version_and_collect_prunable(
    db: &DbConnection,
    file_id: i64,
    message_id: i32,
    folder_id: Option<i64>,
    name: &str,
    size: i64,
    original_path: &str,
    retention: i64,
) -> Result<Vec<i32>, String> {
    if retention <= 0 { return Ok(Vec::new()); }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let now = chrono::Utc::now().timestamp();
    let mut insert = conn.prepare(
        "INSERT OR IGNORE INTO tcloud_versions(message_id,folder_id,name,size,note,created_at,file_id,original_path)
         VALUES(?1,?2,?3,?4,'Snapshot automático antes da substituição',?5,?6,?7)"
    ).map_err(|e| e.to_string())?;
    insert.bind((1, i64::from(message_id))).map_err(|e|e.to_string())?;
    insert.bind((2, folder_id)).map_err(|e|e.to_string())?;
    insert.bind((3, name)).map_err(|e|e.to_string())?;
    insert.bind((4, size)).map_err(|e|e.to_string())?;
    insert.bind((5, now)).map_err(|e|e.to_string())?;
    insert.bind((6, file_id)).map_err(|e|e.to_string())?;
    insert.bind((7, original_path)).map_err(|e|e.to_string())?;
    insert.next().map_err(|e|e.to_string())?;

    let mut q = conn.prepare(
        "SELECT message_id FROM tcloud_versions WHERE file_id=?1 ORDER BY created_at DESC,id DESC LIMIT -1 OFFSET ?2"
    ).map_err(|e|e.to_string())?;
    q.bind((1,file_id)).map_err(|e|e.to_string())?;
    q.bind((2,retention)).map_err(|e|e.to_string())?;
    let mut out=Vec::new();
    while let sqlite::State::Row=q.next().map_err(|e|e.to_string())? {
        let mid=q.read::<i64,_>(0).unwrap_or(0);
        if mid>0 && mid<=i32::MAX as i64 { out.push(mid as i32); }
    }
    Ok(out)
}

pub(crate) fn forget_version_message(db:&DbConnection,file_id:i64,message_id:i32)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut s=conn.prepare("DELETE FROM tcloud_versions WHERE file_id=?1 AND message_id=?2").map_err(|e|e.to_string())?;
    s.bind((1,file_id)).map_err(|e|e.to_string())?;s.bind((2,i64::from(message_id))).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;Ok(())
}

pub(crate) fn is_version_message(conn:&sqlite::Connection,folder_id:i64,message_id:i64)->bool{
    if ensure_schema(conn).is_err(){return false;}
    let Ok(mut q)=conn.prepare("SELECT 1 FROM tcloud_versions WHERE file_id IS NOT NULL AND message_id=?1 AND COALESCE(folder_id,0)=?2 LIMIT 1") else{return false;};
    if q.bind((1,message_id)).is_err() || q.bind((2,folder_id)).is_err(){return false;}
    matches!(q.next(),Ok(sqlite::State::Row))
}

pub(crate) fn is_trash_message(conn:&sqlite::Connection,folder_id:i64,message_id:i64)->bool{
    if ensure_schema(conn).is_err(){return false;}
    let Ok(mut q)=conn.prepare(
        "SELECT 1 FROM tcloud_trash WHERE message_id=?1 AND COALESCE(folder_id,0)=?2 AND COALESCE(status,'trashed')='trashed' LIMIT 1"
    ) else{return false;};
    if q.bind((1,message_id)).is_err() || q.bind((2,folder_id)).is_err(){return false;}
    matches!(q.next(),Ok(sqlite::State::Row))
}

pub(crate) fn move_file_to_trash(db:&DbConnection,file_id:i64)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
    let mut q=conn.prepare(
        "SELECT f.telegram_message_id,COALESCE(f.telegram_chat_id,0),f.name,f.size,c.relative_path
         FROM tcloud_files f LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id WHERE f.id=?1 LIMIT 1"
    ).map_err(|e|e.to_string())?;
    q.bind((1,file_id)).map_err(|e|e.to_string())?;
    if !matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Err("Arquivo não encontrado para mover à Lixeira.".into());}
    let message_id=q.read::<Option<i64>,_>(0).unwrap_or(None);
    let folder_id=q.read::<i64,_>(1).unwrap_or(0);
    let name=q.read::<String,_>(2).unwrap_or_else(|_|"arquivo".into());
    let size=q.read::<i64,_>(3).unwrap_or(0);
    let relative=q.read::<Option<String>,_>(4).unwrap_or(None);
    drop(q);
    let now=chrono::Utc::now().timestamp();
    let retention={let mut p=conn.prepare("SELECT trash_retention_days FROM tcloud_sync_preferences WHERE id=1").map_err(|e|e.to_string())?;if matches!(p.next().map_err(|e|e.to_string())?,sqlite::State::Row){p.read::<i64,_>(0).unwrap_or(30)}else{30}};
    if let Some(mid)=message_id{
        let expires=if retention<=0{None}else{Some(now+retention.clamp(1,3650)*86400)};
        let mut ins=conn.prepare(
            "INSERT OR REPLACE INTO tcloud_trash(message_id,folder_id,name,size,deleted_at,file_id,relative_path,expires_at,status)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'trashed')"
        ).map_err(|e|e.to_string())?;
        ins.bind((1,mid)).map_err(|e|e.to_string())?;ins.bind((2,if folder_id==0{None}else{Some(folder_id)})).map_err(|e|e.to_string())?;
        ins.bind((3,name.as_str())).map_err(|e|e.to_string())?;ins.bind((4,size)).map_err(|e|e.to_string())?;ins.bind((5,now)).map_err(|e|e.to_string())?;
        ins.bind((6,file_id)).map_err(|e|e.to_string())?;ins.bind((7,relative.as_deref())).map_err(|e|e.to_string())?;ins.bind((8,expires)).map_err(|e|e.to_string())?;ins.next().map_err(|e|e.to_string())?;
    }
    let mut jobs=conn.prepare("UPDATE tcloud_sync_jobs SET status='cancelled',cancel_requested=1,updated_at=?1 WHERE file_id=?2 AND status IN ('pending','running','paused')").map_err(|e|e.to_string())?;
    jobs.bind((1,now)).map_err(|e|e.to_string())?;jobs.bind((2,file_id)).map_err(|e|e.to_string())?;jobs.next().map_err(|e|e.to_string())?;
    let mut up=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
    up.bind((1,now)).map_err(|e|e.to_string())?;up.bind((2,file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
    let mut del=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;del.bind((1,file_id)).map_err(|e|e.to_string())?;del.next().map_err(|e|e.to_string())?;
    Ok(())
}

pub(crate) fn queue_permanent_delete(conn:&sqlite::Connection,file_id:i64)->Result<(),String>{
    let now=chrono::Utc::now().timestamp();
    let has_remote={let mut q=conn.prepare("SELECT telegram_message_id FROM tcloud_files WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;q.bind((1,file_id)).map_err(|e|e.to_string())?;matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row)&&q.read::<Option<i64>,_>(0).unwrap_or(None).is_some()};
    if has_remote{
        let mut dup=conn.prepare("SELECT 1 FROM tcloud_sync_jobs WHERE file_id=?1 AND operation='delete_remote' AND status IN ('pending','running') LIMIT 1").map_err(|e|e.to_string())?;dup.bind((1,file_id)).map_err(|e|e.to_string())?;
        if !matches!(dup.next().map_err(|e|e.to_string())?,sqlite::State::Row){
            let mut j=conn.prepare("INSERT INTO tcloud_sync_jobs(file_id,operation,status,created_at,updated_at,priority,attempts,next_attempt_at) VALUES(?1,'delete_remote','pending',?2,?2,125,0,NULL)").map_err(|e|e.to_string())?;j.bind((1,file_id)).map_err(|e|e.to_string())?;j.bind((2,now)).map_err(|e|e.to_string())?;j.next().map_err(|e|e.to_string())?;
            crate::sync::scheduler::signal_sync_work();
        }
        let mut u=conn.prepare("UPDATE tcloud_files SET sync_status='pending_delete',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;u.bind((1,now)).map_err(|e|e.to_string())?;u.bind((2,file_id)).map_err(|e|e.to_string())?;u.next().map_err(|e|e.to_string())?;
    }else{
        let mut u=conn.prepare("UPDATE tcloud_files SET sync_status='deleted',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;u.bind((1,now)).map_err(|e|e.to_string())?;u.bind((2,file_id)).map_err(|e|e.to_string())?;u.next().map_err(|e|e.to_string())?;
    }
    Ok(())
}

pub(crate) fn queue_expired_trash(db:&DbConnection)->Result<i64,String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let now=chrono::Utc::now().timestamp();let mut q=conn.prepare("SELECT id,file_id FROM tcloud_trash WHERE expires_at IS NOT NULL AND expires_at<=?1 AND status='trashed' LIMIT 100").map_err(|e|e.to_string())?;q.bind((1,now)).map_err(|e|e.to_string())?;let mut rows=Vec::new();while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{rows.push((q.read::<i64,_>(0).unwrap_or(0),q.read::<Option<i64>,_>(1).unwrap_or(None)));}drop(q);let mut count=0;for(id,file_id)in rows{if let Some(fid)=file_id{queue_permanent_delete(&conn,fid)?;}let mut d=conn.prepare("DELETE FROM tcloud_trash WHERE id=?1").map_err(|e|e.to_string())?;d.bind((1,id)).map_err(|e|e.to_string())?;d.next().map_err(|e|e.to_string())?;count+=1;}Ok(count)
}

#[tauri::command]
pub fn tcloud_sync_maturity_list_policies(db: State<'_,DbConnection>)->Result<Vec<FolderSyncPolicy>,String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut s=conn.prepare("SELECT relative_path,mode,updated_at FROM tcloud_folder_sync_policies ORDER BY relative_path COLLATE NOCASE").map_err(|e|e.to_string())?;let mut out=Vec::new();while let sqlite::State::Row=s.next().map_err(|e|e.to_string())?{out.push(FolderSyncPolicy{relative_path:s.read(0).unwrap_or_default(),mode:s.read(1).unwrap_or_else(|_|"normal".into()),updated_at:s.read(2).unwrap_or(0)});}Ok(out)
}

#[tauri::command]
pub fn tcloud_sync_maturity_set_policy(app:AppHandle,db:State<'_,DbConnection>,relative_path:String,mode:String)->Result<FolderSyncPolicy,String>{
    if !matches!(mode.as_str(),"normal"|"offline"|"online"){return Err("Política inválida.".into());}
    let relative=normalize_relative(&relative_path);let now=chrono::Utc::now().timestamp();{
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;
        if mode=="normal"{let mut d=conn.prepare("DELETE FROM tcloud_folder_sync_policies WHERE relative_path=?1").map_err(|e|e.to_string())?;d.bind((1,relative.as_str())).map_err(|e|e.to_string())?;d.next().map_err(|e|e.to_string())?;}
        else{let mut s=conn.prepare("INSERT INTO tcloud_folder_sync_policies(relative_path,mode,created_at,updated_at) VALUES(?1,?2,?3,?3) ON CONFLICT(relative_path) DO UPDATE SET mode=excluded.mode,updated_at=excluded.updated_at").map_err(|e|e.to_string())?;s.bind((1,relative.as_str())).map_err(|e|e.to_string())?;s.bind((2,mode.as_str())).map_err(|e|e.to_string())?;s.bind((3,now)).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;}
    }
    if let Err(error)=crate::commands::tcloud_cfapi::apply_folder_sync_policy(&app,db.inner(),&relative,&mode){log::warn!("TCloud política de pasta salva, mas aplicação CFAPI será retomada depois: {}",error);}
    Ok(FolderSyncPolicy{relative_path:relative,mode,updated_at:now})
}

#[tauri::command]
pub fn tcloud_sync_maturity_get_settings(db:State<'_,DbConnection>)->Result<SyncMaturitySettings,String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut s=conn.prepare("SELECT version_retention,trash_retention_days,conflict_strategy FROM tcloud_sync_preferences WHERE id=1").map_err(|e|e.to_string())?;if !matches!(s.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Err("Preferências de sincronização ausentes.".into());}Ok(SyncMaturitySettings{version_retention:s.read(0).unwrap_or(10),trash_retention_days:s.read(1).unwrap_or(30),conflict_strategy:s.read(2).unwrap_or_else(|_|"ask".into())})
}

#[tauri::command]
pub fn tcloud_sync_maturity_set_settings(db:State<'_,DbConnection>,version_retention:i64,trash_retention_days:i64,conflict_strategy:String)->Result<SyncMaturitySettings,String>{
    if !matches!(conflict_strategy.as_str(),"keep_both"|"ask"){return Err("Estratégia de conflito inválida.".into());}
    let vr=version_retention.clamp(0,100);let tr=trash_retention_days.clamp(0,3650);let now=chrono::Utc::now().timestamp();let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut s=conn.prepare("UPDATE tcloud_sync_preferences SET version_retention=?1,trash_retention_days=?2,conflict_strategy=?3,updated_at=?4 WHERE id=1").map_err(|e|e.to_string())?;s.bind((1,vr)).map_err(|e|e.to_string())?;s.bind((2,tr)).map_err(|e|e.to_string())?;s.bind((3,conflict_strategy.as_str())).map_err(|e|e.to_string())?;s.bind((4,now)).map_err(|e|e.to_string())?;s.next().map_err(|e|e.to_string())?;Ok(SyncMaturitySettings{version_retention:vr,trash_retention_days:tr,conflict_strategy})
}

#[tauri::command]
pub fn tcloud_sync_maturity_purge_trash(db:State<'_,DbConnection>,trash_id:Option<i64>)->Result<i64,String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;ensure_schema(&conn)?;let mut rows=Vec::new();if let Some(id)=trash_id{let mut q=conn.prepare("SELECT id,file_id,message_id,folder_id FROM tcloud_trash WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;q.bind((1,id)).map_err(|e|e.to_string())?;if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){rows.push((q.read::<i64,_>(0).unwrap_or(0),q.read::<Option<i64>,_>(1).unwrap_or(None),q.read::<i64,_>(2).unwrap_or(0),q.read::<Option<i64>,_>(3).unwrap_or(None)));}}
    else{let mut q=conn.prepare("SELECT id,file_id,message_id,folder_id FROM tcloud_trash").map_err(|e|e.to_string())?;while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{rows.push((q.read::<i64,_>(0).unwrap_or(0),q.read::<Option<i64>,_>(1).unwrap_or(None),q.read::<i64,_>(2).unwrap_or(0),q.read::<Option<i64>,_>(3).unwrap_or(None)));}}
    let mut count=0;for (id,file_id,mid,folder) in rows{let resolved=if let Some(fid)=file_id{Some(fid)}else{let mut q=conn.prepare("SELECT id FROM tcloud_files WHERE telegram_message_id=?1 AND COALESCE(telegram_chat_id,0)=?2 ORDER BY id DESC LIMIT 1").map_err(|e|e.to_string())?;q.bind((1,mid)).map_err(|e|e.to_string())?;q.bind((2,folder.unwrap_or(0))).map_err(|e|e.to_string())?;if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){Some(q.read::<i64,_>(0).unwrap_or(0))}else{None}};if let Some(fid)=resolved{queue_permanent_delete(&conn,fid)?;}let mut d=conn.prepare("DELETE FROM tcloud_trash WHERE id=?1").map_err(|e|e.to_string())?;d.bind((1,id)).map_err(|e|e.to_string())?;d.next().map_err(|e|e.to_string())?;count+=1;}Ok(count)
}
