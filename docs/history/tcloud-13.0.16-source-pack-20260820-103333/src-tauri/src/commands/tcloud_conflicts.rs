use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TCloudConflict {
    pub id: i64,
    pub file_id: i64,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    pub resolution: String,
    pub created_at: i64,
    pub remote_message_id: Option<i64>,
    pub file_name: Option<String>,
    pub local_path: Option<String>,
    pub remote_name: Option<String>,
    pub local_size: Option<i64>,
    pub remote_size: Option<i64>,
}

fn ensure_schema(conn:&sqlite::Connection)->Result<(),String>{
    conn.execute("CREATE TABLE IF NOT EXISTS tcloud_sync_preferences (id INTEGER PRIMARY KEY CHECK(id=1), version_retention INTEGER NOT NULL DEFAULT 10, trash_retention_days INTEGER NOT NULL DEFAULT 30, conflict_strategy TEXT NOT NULL DEFAULT 'ask', updated_at INTEGER NOT NULL DEFAULT 0); INSERT OR IGNORE INTO tcloud_sync_preferences(id,version_retention,trash_retention_days,conflict_strategy,updated_at) VALUES(1,10,30,'ask',0);").map_err(|e|e.to_string())?;
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_message_id INTEGER");
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN file_name TEXT");
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN local_path TEXT");
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_name TEXT");
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN local_size INTEGER");
    let _=conn.execute("ALTER TABLE tcloud_sync_conflicts ADD COLUMN remote_size INTEGER");
    Ok(())
}

pub(crate) fn record_remote_metadata_conflict(
    conn:&sqlite::Connection,
    file_id:i64,
    remote_message_id:i64,
    file_name:&str,
    local_path:Option<&str>,
    local_size:i64,
    remote_name:&str,
    remote_size:i64,
    local_hash:Option<&str>,
)->Result<(),String>{
    ensure_schema(conn)?;
    let mut existing=conn.prepare("SELECT id FROM tcloud_sync_conflicts WHERE file_id=?1 AND resolution='pending' LIMIT 1").map_err(|e|e.to_string())?;
    existing.bind((1,file_id)).map_err(|e|e.to_string())?;
    if matches!(existing.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(());}
    let now=chrono::Utc::now().timestamp();
    let mut insert=conn.prepare(
        "INSERT INTO tcloud_sync_conflicts(file_id,local_hash,remote_hash,resolution,created_at,resolved_at,remote_message_id,file_name,local_path,remote_name,local_size,remote_size)
         VALUES(?1,?2,?3,'pending',?4,NULL,?5,?6,?7,?8,?9,?10)"
    ).map_err(|e|e.to_string())?;
    insert.bind((1,file_id)).map_err(|e|e.to_string())?;
    insert.bind((2,local_hash)).map_err(|e|e.to_string())?;
    insert.bind((3,format!("telegram:{}:{}",remote_message_id,remote_size).as_str())).map_err(|e|e.to_string())?;
    insert.bind((4,now)).map_err(|e|e.to_string())?;
    insert.bind((5,remote_message_id)).map_err(|e|e.to_string())?;
    insert.bind((6,file_name)).map_err(|e|e.to_string())?;
    insert.bind((7,local_path)).map_err(|e|e.to_string())?;
    insert.bind((8,remote_name)).map_err(|e|e.to_string())?;
    insert.bind((9,local_size)).map_err(|e|e.to_string())?;
    insert.bind((10,remote_size)).map_err(|e|e.to_string())?;
    insert.next().map_err(|e|e.to_string())?;

    // Stop the local replacement until the configured strategy (or the user)
    // makes a real decision. Previous builds marked "keep_both" as resolved
    // without actually preserving both versions.
    let mut stop=conn.prepare(
        "UPDATE tcloud_sync_jobs
         SET cancel_requested=1,status=CASE WHEN status='running' THEN status ELSE 'paused' END,paused=1,updated_at=?1
         WHERE file_id=?2 AND operation IN ('upload','refresh') AND status IN ('pending','running','paused')"
    ).map_err(|e|e.to_string())?;
    stop.bind((1,now)).map_err(|e|e.to_string())?;
    stop.bind((2,file_id)).map_err(|e|e.to_string())?;
    stop.next().map_err(|e|e.to_string())?;
    let mut mark=conn.prepare("UPDATE tcloud_files SET sync_status='conflict',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
    mark.bind((1,now)).map_err(|e|e.to_string())?;
    mark.bind((2,file_id)).map_err(|e|e.to_string())?;
    mark.next().map_err(|e|e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn tcloud_list_conflicts(db: State<'_, DbConnection>) -> Result<Vec<TCloudConflict>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let mut stmt = conn.prepare(
        "SELECT id,file_id,local_hash,remote_hash,resolution,created_at,remote_message_id,file_name,local_path,remote_name,local_size,remote_size
         FROM tcloud_sync_conflicts WHERE resolution='pending' ORDER BY created_at DESC"
    ).map_err(|e| e.to_string())?;
    let mut items=Vec::new();
    while let sqlite::State::Row=stmt.next().map_err(|e|e.to_string())?{
        items.push(TCloudConflict{
            id:stmt.read(0).map_err(|e|e.to_string())?,file_id:stmt.read(1).map_err(|e|e.to_string())?,
            local_hash:stmt.read::<Option<String>,_>(2).unwrap_or(None),remote_hash:stmt.read::<Option<String>,_>(3).unwrap_or(None),
            resolution:stmt.read(4).unwrap_or_else(|_|"pending".into()),created_at:stmt.read(5).unwrap_or(0),
            remote_message_id:stmt.read::<Option<i64>,_>(6).unwrap_or(None),file_name:stmt.read::<Option<String>,_>(7).unwrap_or(None),
            local_path:stmt.read::<Option<String>,_>(8).unwrap_or(None),remote_name:stmt.read::<Option<String>,_>(9).unwrap_or(None),
            local_size:stmt.read::<Option<i64>,_>(10).unwrap_or(None),remote_size:stmt.read::<Option<i64>,_>(11).unwrap_or(None),
        });
    }
    Ok(items)
}

fn resolve_conflict_internal(app:&AppHandle,db:&DbConnection,conflict_id:i64,resolution:&str)->Result<(),String>{
    if !matches!(resolution,"keep_local"|"keep_remote"|"keep_both"){return Err("invalid conflict resolution".into());}

    let (file_id,remote_mid,remote_name,remote_size)={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q=conn.prepare(
            "SELECT file_id,remote_message_id,remote_name,remote_size
             FROM tcloud_sync_conflicts WHERE id=?1 AND resolution='pending' LIMIT 1"
        ).map_err(|e|e.to_string())?;
        q.bind((1,conflict_id)).map_err(|e|e.to_string())?;
        if !matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
            return Err("Conflito não encontrado ou já resolvido.".into());
        }
        (
            q.read::<i64,_>(0).unwrap_or(0),
            q.read::<Option<i64>,_>(1).unwrap_or(None).filter(|value|*value>0),
            q.read::<Option<String>,_>(2).unwrap_or(None),
            q.read::<Option<i64>,_>(3).unwrap_or(None),
        )
    };

    // "Guardar as duas" now means exactly that: duplicate the PC bytes as a
    // fresh local file first, queue that copy for upload, then let the original
    // placeholder adopt the cloud version.
    let preserved_copy=if resolution=="keep_both"{
        Some(crate::commands::tcloud_cfapi::preserve_local_conflict_copy(db,file_id)?)
    }else{None};

    let now=chrono::Utc::now().timestamp();
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        if matches!(resolution,"keep_remote"|"keep_both"){
            let mut cancel=conn.prepare(
                "UPDATE tcloud_sync_jobs SET status='cancelled',cancel_requested=1,paused=0,updated_at=?1
                 WHERE file_id=?2 AND operation IN ('upload','refresh') AND status IN ('pending','running','paused')"
            ).map_err(|e|e.to_string())?;
            cancel.bind((1,now)).map_err(|e|e.to_string())?;
            cancel.bind((2,file_id)).map_err(|e|e.to_string())?;
            cancel.next().map_err(|e|e.to_string())?;

            if remote_mid.is_some(){
                let mut up=conn.prepare(
                    "UPDATE tcloud_files
                     SET telegram_message_id=?1,name=COALESCE(?2,name),size=COALESCE(?3,size),
                         sync_status='synced',local_status='cloud_only',updated_at=?4
                     WHERE id=?5"
                ).map_err(|e|e.to_string())?;
                up.bind((1,remote_mid)).map_err(|e|e.to_string())?;
                up.bind((2,remote_name.as_deref())).map_err(|e|e.to_string())?;
                up.bind((3,remote_size)).map_err(|e|e.to_string())?;
                up.bind((4,now)).map_err(|e|e.to_string())?;
                up.bind((5,file_id)).map_err(|e|e.to_string())?;
                up.next().map_err(|e|e.to_string())?;
            }else{
                let mut up=conn.prepare(
                    "UPDATE tcloud_files SET sync_status='remote_deleted',local_status='deleted',updated_at=?1 WHERE id=?2"
                ).map_err(|e|e.to_string())?;
                up.bind((1,now)).map_err(|e|e.to_string())?;
                up.bind((2,file_id)).map_err(|e|e.to_string())?;
                up.next().map_err(|e|e.to_string())?;
            }
        }else{
            let mut jobs=conn.prepare(
                "UPDATE tcloud_sync_jobs
                 SET status='pending',cancel_requested=0,paused=0,error=NULL,next_attempt_at=NULL,updated_at=?1
                 WHERE file_id=?2 AND operation IN ('upload','refresh') AND status IN ('paused','failed','cancelled')"
            ).map_err(|e|e.to_string())?;
            jobs.bind((1,now)).map_err(|e|e.to_string())?;
            jobs.bind((2,file_id)).map_err(|e|e.to_string())?;
            jobs.next().map_err(|e|e.to_string())?;
            if conn.change_count()==0{
                let mut add=conn.prepare(
                    "INSERT INTO tcloud_sync_jobs(file_id,operation,status,created_at,updated_at,priority,attempts,next_attempt_at)
                     VALUES(?1,'refresh','pending',?2,?2,110,0,NULL)"
                ).map_err(|e|e.to_string())?;
                add.bind((1,file_id)).map_err(|e|e.to_string())?;
                add.bind((2,now)).map_err(|e|e.to_string())?;
                add.next().map_err(|e|e.to_string())?;
                crate::sync::scheduler::signal_sync_work();
            } else {
                crate::sync::scheduler::signal_sync_work();
            }
            let mut up=conn.prepare("UPDATE tcloud_files SET sync_status='pending_upload',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
            up.bind((1,now)).map_err(|e|e.to_string())?;
            up.bind((2,file_id)).map_err(|e|e.to_string())?;
            up.next().map_err(|e|e.to_string())?;
        }

        let mut done=conn.prepare("UPDATE tcloud_sync_conflicts SET resolution=?1,resolved_at=?2 WHERE id=?3").map_err(|e|e.to_string())?;
        done.bind((1,resolution)).map_err(|e|e.to_string())?;
        done.bind((2,now)).map_err(|e|e.to_string())?;
        done.bind((3,conflict_id)).map_err(|e|e.to_string())?;
        done.next().map_err(|e|e.to_string())?;
    }

    if matches!(resolution,"keep_remote"|"keep_both"){
        let _=crate::commands::tcloud_cfapi::discard_local_file_for_conflict(db,file_id);
        let _=crate::commands::tcloud_cfapi::reconcile_remote_index(app,db);
    }
    if let Some(path)=preserved_copy{
        log::info!("TCloud conflito {}: versão do computador preservada como {}.",conflict_id,path);
    }
    Ok(())
}

pub(crate) fn auto_resolve_keep_both(app:&AppHandle,db:&DbConnection)->Result<usize,String>{
    let ids={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let strategy={
            let mut q=conn.prepare("SELECT conflict_strategy FROM tcloud_sync_preferences WHERE id=1").map_err(|e|e.to_string())?;
            if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                q.read::<String,_>(0).unwrap_or_else(|_|"ask".into())
            }else{"ask".into()}
        };
        if strategy!="keep_both"{return Ok(0);}
        let mut q=conn.prepare("SELECT id FROM tcloud_sync_conflicts WHERE resolution='pending' ORDER BY created_at ASC LIMIT 16").map_err(|e|e.to_string())?;
        let mut ids=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            ids.push(q.read::<i64,_>(0).unwrap_or(0));
        }
        ids
    };

    let mut resolved=0usize;
    for id in ids{
        match resolve_conflict_internal(app,db,id,"keep_both"){
            Ok(())=>resolved+=1,
            Err(error)=>log::warn!("TCloud conflito {}: Guardar as duas automático será tentado novamente: {}",id,error),
        }
    }
    Ok(resolved)
}

#[tauri::command]
pub fn tcloud_resolve_conflict(
    app:AppHandle,
    db:State<'_,DbConnection>,
    conflict_id:i64,
    resolution:String,
)->Result<(),String>{
    resolve_conflict_internal(&app,db.inner(),conflict_id,&resolution)
}
