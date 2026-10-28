//! Runtime TCloud file watcher powered by `notify`.
//!
//! TCloud 12.3 binds this watcher to the CFAPI Sync Root. It only turns
//! user-owned filesystem changes into durable SQLite jobs. Provider-owned
//! changes (placeholder creation, hydration, dehydration and reconciliation)
//! are suppressed by the CFAPI layer so they cannot loop back as uploads.

use crate::db::DbConnection;
use notify::{event::ModifyKind, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};

pub struct TCloudWatcherState {
    watcher: Mutex<Option<RecommendedWatcher>>,
    watched_root: Mutex<Option<PathBuf>>,
}

impl Default for TCloudWatcherState {
    fn default() -> Self {
        Self { watcher: Mutex::new(None), watched_root: Mutex::new(None) }
    }
}

impl TCloudWatcherState {
    pub fn is_running(&self) -> bool {
        self.watcher.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn root(&self) -> Option<PathBuf> {
        self.watched_root.lock().ok().and_then(|g| g.clone())
    }

    pub fn stop(&self) -> Result<(), String> {
        *self.watcher.lock().map_err(|_| "watcher lock poisoned".to_string())? = None;
        *self.watched_root.lock().map_err(|_| "watcher root lock poisoned".to_string())? = None;
        Ok(())
    }

    pub fn start(&self, root: PathBuf, db: DbConnection) -> Result<(), String> {
        if !root.exists() {
            std::fs::create_dir_all(&root)
                .map_err(|e| format!("failed to create TCloud sync folder: {e}"))?;
        }
        if !root.is_dir() { return Err("TCloud sync path must be a directory".into()); }

        self.stop()?;
        // Stability 12.8 Phase 3: keep the notify callback lightweight even
        // during copies with thousands of files. SQLite work runs on a dedicated
        // consumer so the Windows watcher backend can keep accepting events.
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let db_for_events = db.clone();
        let root_for_events = root.clone();
        std::thread::Builder::new()
            .name("tcloud-watcher-events".to_string())
            .spawn(move || {
                while let Ok(first) = event_rx.recv() {
                    // Windows may emit several Modify notifications for the same
                    // file during one copy/write burst. Batch what is already in
                    // the channel and collapse duplicate single-path modifies;
                    // persist_event reads the current filesystem state, so no user
                    // bytes are lost and SQLite does far less duplicate work.
                    let mut batch = Vec::with_capacity(64);
                    batch.push(first);
                    for _ in 0..255 {
                        match event_rx.try_recv() {
                            Ok(event) => batch.push(event),
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                    let mut seen_modifies: HashSet<PathBuf> = HashSet::new();
                    for event in batch {
                        let coalescible = matches!(&event.kind, EventKind::Modify(kind) if !matches!(kind, ModifyKind::Name(_)))
                            && event.paths.len() == 1;
                        if coalescible {
                            if let Some(path) = event.paths.first() {
                                if !seen_modifies.insert(path.clone()) { continue; }
                            }
                        }
                        if let Err(err) = persist_event(&db_for_events, &root_for_events, event) {
                            log::warn!("TCloud Explorer watcher: {}", err);
                        }
                    }
                }
            })
            .map_err(|e| format!("failed to start TCloud watcher event worker: {e}"))?;

        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            match result {
                Ok(event) => {
                    if event_tx.send(event).is_err() {
                        log::warn!("TCloud file watcher event worker stopped unexpectedly");
                    }
                }
                Err(err) => log::warn!("TCloud file watcher error: {}", err),
            }
        }).map_err(|e| format!("failed to create file watcher: {e}"))?;

        watcher.watch(&root, RecursiveMode::Recursive)
            .map_err(|e| format!("failed to watch {}: {e}", root.display()))?;
        *self.watcher.lock().map_err(|_| "watcher lock poisoned".to_string())? = Some(watcher);
        *self.watched_root.lock().map_err(|_| "watcher root lock poisoned".to_string())? = Some(root);
        Ok(())
    }
}

struct SuppressedRecheckRequest {
    db: DbConnection,
    root: PathBuf,
    path: PathBuf,
    due: std::time::Instant,
}

fn suppressed_recheck_sender() -> &'static mpsc::Sender<SuppressedRecheckRequest> {
    static SENDER: OnceLock<mpsc::Sender<SuppressedRecheckRequest>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<SuppressedRecheckRequest>();
        std::thread::Builder::new()
            .name("tcloud-watcher-recheck".to_string())
            .spawn(move || {
                let mut pending: HashMap<PathBuf, SuppressedRecheckRequest> = HashMap::new();
                loop {
                    // 12.9.6: do not wake this OS thread every 100 ms while the
                    // TCloud is idle. With no pending paths it blocks indefinitely;
                    // with work queued it sleeps exactly until the nearest due time
                    // (or until a newer Explorer event arrives and coalesces it).
                    let received = if pending.is_empty() {
                        match rx.recv() {
                            Ok(request) => Some(request),
                            Err(_) => break,
                        }
                    } else {
                        let now = std::time::Instant::now();
                        let next_due = pending.values().map(|request| request.due).min().unwrap_or(now);
                        let wait = next_due.saturating_duration_since(now);
                        match rx.recv_timeout(wait) {
                            Ok(request) => Some(request),
                            Err(mpsc::RecvTimeoutError::Timeout) => None,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    };
                    if let Some(request) = received {
                        pending.insert(request.path.clone(), request);
                    }
                    while let Ok(request) = rx.try_recv() {
                        pending.insert(request.path.clone(), request);
                    }
                    let now = std::time::Instant::now();
                    let due = pending.iter().filter_map(|(path, request)| (request.due <= now).then_some(path.clone())).collect::<Vec<_>>();
                    for path in due {
                        let Some(request) = pending.remove(&path) else { continue; };
                        if request.path.is_file() {
                            if let Err(error) = persist_upload_job(&request.db, &request.root, &request.path) {
                                log::warn!("TCloud Explorer watcher (recheck): {}", error);
                            }
                        }
                    }
                }
            })
            .expect("failed to start TCloud watcher recheck worker");
        tx
    })
}

fn recheck_suppressed_change(db: DbConnection, root: PathBuf, path: PathBuf) {
    // One coalescing worker replaces one sleeping OS thread per event.
    let request = SuppressedRecheckRequest {
        db,
        root,
        path,
        due: std::time::Instant::now() + std::time::Duration::from_millis(1500),
    };
    if suppressed_recheck_sender().send(request).is_err() {
        log::warn!("TCloud watcher recheck worker stopped unexpectedly");
    }
}

fn is_managed_namespace_directory(db: &DbConnection, relative_path: &str) -> bool {
    let Ok(conn) = db.lock() else { return false; };
    let Ok(mut query) = conn.prepare(
        "SELECT 1 FROM tcloud_cfapi_managed_dirs WHERE lower(relative_path)=lower(?1) LIMIT 1"
    ) else { return false; };
    if query.bind((1, relative_path)).is_err() { return false; }
    matches!(query.next(), Ok(sqlite::State::Row))
}

fn persist_event(db: &DbConnection, root: &Path, event: Event) -> Result<(), String> {
    // Rename events with both paths are special. Existing cloud placeholders are
    // handled by the CFAPI rename callback; this path only keeps not-yet-uploaded
    // local files coherent while Windows is still generating their first job.
    if matches!(&event.kind, EventKind::Modify(ModifyKind::Name(_))) && event.paths.len() >= 2 {
        let old = &event.paths[0];
        let new = &event.paths[event.paths.len() - 1];
        if !crate::commands::tcloud_cfapi::watcher_event_suppressed(old)
            && !crate::commands::tcloud_cfapi::watcher_event_suppressed(new)
        {
            if new.is_dir() {
                let old_relative = relative(root, old)?;
                let new_relative = relative(root, new)?;
                let managed = is_managed_namespace_directory(db, &old_relative);
                crate::commands::tcloud_forums::enqueue_explorer_folder_op(
                    db,
                    crate::commands::tcloud_forums::ExplorerFolderOp::Rename {
                        old_relative_path: old_relative,
                        new_relative_path: new_relative,
                    },
                )?;
                // Managed first/second-level directories are rebound only after
                // the Telegram namespace operation commits. Local-only deeper
                // directories can update file paths immediately.
                if !managed { persist_local_rename(db, root, old, new)?; }
            } else {
                persist_local_rename(db, root, old, new)?;
            }
        }
        return Ok(());
    }

    for path in event.paths {
        if should_ignore(&path) { continue; }
        if crate::commands::tcloud_cfapi::watcher_event_suppressed(&path) {
            if matches!(&event.kind,EventKind::Create(_) | EventKind::Modify(_)){
                recheck_suppressed_change(db.clone(),root.to_path_buf(),path.clone());
            }
            continue;
        }
        match &event.kind {
            EventKind::Create(_) => {
                if path.is_dir() {
                    crate::commands::tcloud_forums::enqueue_explorer_folder_op(
                        db,
                        crate::commands::tcloud_forums::ExplorerFolderOp::Create {
                            relative_path: relative(root, &path)?,
                        },
                    )?;
                } else if path.is_file() {
                    persist_upload_job(db, root, &path)?;
                }
            }
            EventKind::Modify(_) => {
                if path.is_file() { persist_upload_job(db, root, &path)?; }
            }
            EventKind::Remove(_) => {
                // The path no longer exists, so notify cannot reliably tell us if
                // it was a file or directory. Queue a namespace removal anyway;
                // the durable worker only acts on paths that are known managed
                // forum/topic directories, making this harmless for file events.
                let relative_path = relative(root, &path)?;
                let managed = is_managed_namespace_directory(db, &relative_path);
                crate::commands::tcloud_forums::enqueue_explorer_folder_op(
                    db,
                    crate::commands::tcloud_forums::ExplorerFolderOp::Remove {
                        relative_path,
                    },
                )?;
                if !managed { persist_remove(db, root, &path)?; }
            }
            _ => {}
        }
    }
    Ok(())
}

fn should_ignore(path: &Path) -> bool {
    let name = path.file_name().and_then(|v| v.to_str()).unwrap_or_default().to_ascii_lowercase();
    name.is_empty()
        || name == "thumbs.db"
        || name == "desktop.ini"
        || name.starts_with("~$")
        || name.ends_with(".tmp")
        || name.ends_with(".part")
        || name.ends_with(".crdownload")
        || name.ends_with(".tcloud-part")
}

fn relative(root: &Path, path: &Path) -> Result<String, String> {
    let rel = path.strip_prefix(root)
        .map_err(|_| format!("{} está fora da raiz TCloud", path.display()))?;
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

fn fingerprint(path: &Path) -> Result<(i64, String), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?;
    let modified_ns = metadata.modified().ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    Ok((metadata.len() as i64, format!("{}:{}", metadata.len(), modified_ns)))
}

#[cfg(target_os = "windows")]
fn is_cloud_only(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x00001000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x00040000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x00400000;
    let attrs = metadata.file_attributes();
    attrs & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0
}

#[cfg(not(target_os = "windows"))]
fn is_cloud_only(_metadata: &std::fs::Metadata) -> bool { false }

#[cfg(target_os = "windows")]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x00000400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_reparse_point(_metadata: &std::fs::Metadata) -> bool { false }

pub(crate) fn is_materialized_user_file(path:&Path)->bool{
    std::fs::metadata(path).map(|metadata|metadata.is_file() && !is_cloud_only(&metadata)).unwrap_or(false)
}

fn lookup_file_for_path(
    conn: &sqlite::Connection,
    root: &Path,
    path: &Path,
) -> Result<Option<(i64, Option<String>, Option<i64>, String)>, String> {
    let rel = relative(root, path)?;
    let mut cfapi = conn.prepare(
        "SELECT f.id,f.local_fingerprint,f.telegram_message_id,f.sync_status
         FROM tcloud_cfapi_items c JOIN tcloud_files f ON f.id=c.file_id
         WHERE c.relative_path=?1 LIMIT 1"
    ).map_err(|e| e.to_string())?;
    cfapi.bind((1, rel.as_str())).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = cfapi.next().map_err(|e| e.to_string())? {
        return Ok(Some((
            cfapi.read::<i64,_>(0).map_err(|e| e.to_string())?,
            cfapi.read::<Option<String>,_>(1).unwrap_or(None),
            cfapi.read::<Option<i64>,_>(2).unwrap_or(None),
            cfapi.read::<String,_>(3).unwrap_or_else(|_| "synced".into()),
        )));
    }
    drop(cfapi);

    let full = path.to_string_lossy();
    let mut local = conn.prepare(
        "SELECT id,local_fingerprint,telegram_message_id,sync_status FROM tcloud_files WHERE path=?1 LIMIT 1"
    ).map_err(|e| e.to_string())?;
    local.bind((1, full.as_ref())).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = local.next().map_err(|e| e.to_string())? {
        return Ok(Some((
            local.read::<i64,_>(0).map_err(|e| e.to_string())?,
            local.read::<Option<String>,_>(1).unwrap_or(None),
            local.read::<Option<i64>,_>(2).unwrap_or(None),
            local.read::<String,_>(3).unwrap_or_else(|_| "pending".into()),
        )));
    }
    Ok(None)
}

pub(crate) fn queue_local_file_now(db:&DbConnection, root:&Path, path:&Path)->Result<(),String>{
    persist_upload_job(db,root,path)
}

fn persist_upload_job(db: &DbConnection, root: &Path, path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?;
    if !metadata.is_file() { return Ok(()); }

    let rel_string = relative(root, path)?;
    let rel_path = Path::new(&rel_string);
    let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("arquivo").to_string();
    let (size, local_fingerprint) = fingerprint(path)?;
    let now = chrono::Utc::now().timestamp();
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    let mut existing = lookup_file_for_path(&conn, root, path)?;

    /*
     * Última barreira contra falso upload CFAPI.
     *
     * Um reparse point desconhecido nunca representa um arquivo novo criado
     * pelo usuário. É uma projeção/place­holder do próprio provedor que ainda
     * não foi associado ao SQLite. Deixar esse item seguir para a criação de
     * tcloud_files + pending_upload produz loops como:
     *
     *   Photo (msg-5900).jpg -> pending_upload
     *
     * mesmo quando a identidade CFAPI aponta para uma mensagem Telegram já
     * existente.
     */
    if existing.is_none() && is_reparse_point(&metadata) {
        log::debug!(
            "TCloud watcher: bloqueando novo upload para reparse point CFAPI não associado: {}",
            path.display()
        );
        return Ok(());
    }

    // CFAPI upload-loop guard.
    //
    // Online-only placeholders are provider-owned state and never represent
    // a new local upload.
    if is_cloud_only(&metadata) {
        log::debug!(
            "TCloud watcher: ignorando placeholder cloud-only em {}",
            path.display()
        );
        return Ok(());
    }

    // A hydrated CFAPI placeholder remains a Windows reparse point even when
    // OFFLINE/RECALL flags disappear. If SQLite does not know this physical
    // path yet, do NOT invent a second local tcloud_files row: this is the
    // race that caused remote files to be downloaded and uploaded again.
    //
    // Known reparse points are deliberately allowed through because the user
    // may have edited an already-associated hydrated TCloud file.
    if existing.is_none() && is_reparse_point(&metadata) {
        log::debug!(
            "TCloud watcher: ignorando reparse point CFAPI ainda não associado em {}",
            path.display()
        );
        return Ok(());
    }

    let chat_key = crate::commands::tcloud_cfapi::chat_key_for_relative_path(&conn, rel_path)?;
    // Some Windows notify backends report rename as separate FROM/TO events. If
    // this is the TO side of a not-yet-uploaded file, rebind the old local row by
    // fingerprint instead of creating a duplicate database object.
    if existing.is_none(){
        let full=path.to_string_lossy().to_string();
        let candidate={
            let mut q=conn.prepare(
                "SELECT id,local_fingerprint,telegram_message_id,sync_status,path FROM tcloud_files
                 WHERE telegram_message_id IS NULL AND local_fingerprint=?1 AND path!=?2
                   AND sync_status IN ('pending_upload','uploading') ORDER BY updated_at DESC LIMIT 1"
            ).map_err(|e|e.to_string())?;
            q.bind((1,local_fingerprint.as_str())).map_err(|e|e.to_string())?;
            q.bind((2,full.as_str())).map_err(|e|e.to_string())?;
            if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                Some((q.read::<i64,_>(0).map_err(|e|e.to_string())?,q.read::<Option<String>,_>(1).unwrap_or(None),q.read::<Option<i64>,_>(2).unwrap_or(None),q.read::<String,_>(3).unwrap_or_else(|_|"pending_upload".into()),q.read::<String,_>(4).unwrap_or_default()))
            }else{None}
        };
        if let Some((id,fp,msg,status,old_path))=candidate{
            if old_path.is_empty() || !Path::new(&old_path).exists(){
                let mut rebind=conn.prepare("UPDATE tcloud_files SET path=?1,name=?2,telegram_chat_id=?3,updated_at=?4 WHERE id=?5").map_err(|e|e.to_string())?;
                rebind.bind((1,full.as_str())).map_err(|e|e.to_string())?;rebind.bind((2,name.as_str())).map_err(|e|e.to_string())?;rebind.bind((3,chat_key)).map_err(|e|e.to_string())?;rebind.bind((4,now)).map_err(|e|e.to_string())?;rebind.bind((5,id)).map_err(|e|e.to_string())?;rebind.next().map_err(|e|e.to_string())?;
                existing=Some((id,fp,msg,status));
            }
        }
    }
    let (file_id, previous_fingerprint, _message_id, previous_status) = if let Some(row) = existing {
        row
    } else {
        let full = path.to_string_lossy().to_string();
        let mut insert = conn.prepare(
            "INSERT INTO tcloud_files(folder_id,name,path,size,telegram_chat_id,sync_status,local_status,local_fingerprint,created_at,updated_at)
             VALUES(1,?1,?2,?3,?4,'pending_upload','local',?5,?6,?7)"
        ).map_err(|e| e.to_string())?;
        insert.bind((1, name.as_str())).map_err(|e| e.to_string())?;
        insert.bind((2, full.as_str())).map_err(|e| e.to_string())?;
        insert.bind((3, size)).map_err(|e| e.to_string())?;
        insert.bind((4, chat_key)).map_err(|e| e.to_string())?;
        insert.bind((5, local_fingerprint.as_str())).map_err(|e| e.to_string())?;
        insert.bind((6, now)).map_err(|e| e.to_string())?;
        insert.bind((7, now)).map_err(|e| e.to_string())?;
        insert.next().map_err(|e| e.to_string())?;
        (crate::db::last_insert_rowid(&conn)?, None, None, "pending_upload".to_string())
    };

    // Rename/move has its own CFAPI transaction. Do not let a content watcher job
    // race it using the old Telegram identity. Remember newer local bytes and let
    // the rename/move worker queue a refresh after its remote commit if necessary.
    if previous_status=="pending_move" {
        let mut update=conn.prepare("UPDATE tcloud_files SET local_fingerprint=?1,local_status='local',updated_at=?2 WHERE id=?3").map_err(|e|e.to_string())?;
        update.bind((1,local_fingerprint.as_str())).map_err(|e|e.to_string())?;update.bind((2,now)).map_err(|e|e.to_string())?;update.bind((3,file_id)).map_err(|e|e.to_string())?;update.next().map_err(|e|e.to_string())?;
        return Ok(());
    }
    if previous_status=="pending_delete" { return Ok(()); }

    if previous_fingerprint.as_deref() == Some(local_fingerprint.as_str())
        && matches!(previous_status.as_str(), "synced" | "pending_upload" | "uploading")
    {
        return Ok(());
    }

    let full = path.to_string_lossy().to_string();
    let mut update = conn.prepare(
        "UPDATE tcloud_files SET name=?1,size=?2,telegram_chat_id=?3,local_fingerprint=?4,
             sync_status='pending_upload',local_status='local',updated_at=?5,
             path=CASE WHEN telegram_message_id IS NULL THEN ?6 ELSE path END
         WHERE id=?7"
    ).map_err(|e| e.to_string())?;
    update.bind((1, name.as_str())).map_err(|e| e.to_string())?;
    update.bind((2, size)).map_err(|e| e.to_string())?;
    update.bind((3, chat_key)).map_err(|e| e.to_string())?;
    update.bind((4, local_fingerprint.as_str())).map_err(|e| e.to_string())?;
    update.bind((5, now)).map_err(|e| e.to_string())?;
    update.bind((6, full.as_str())).map_err(|e| e.to_string())?;
    update.bind((7, file_id)).map_err(|e| e.to_string())?;
    update.next().map_err(|e| e.to_string())?;

    let mut duplicate = conn.prepare(
        "SELECT id FROM tcloud_sync_jobs WHERE file_id=?1 AND operation IN ('upload','refresh') AND status IN ('pending','running') LIMIT 1"
    ).map_err(|e| e.to_string())?;
    duplicate.bind((1, file_id)).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = duplicate.next().map_err(|e| e.to_string())? { return Ok(()); }

    let mut job = conn.prepare(
        "INSERT INTO tcloud_sync_jobs(file_id,operation,status,created_at,updated_at,priority,attempts,next_attempt_at)
         VALUES(?1,'upload','pending',?2,?3,130,0,NULL)"
    ).map_err(|e| e.to_string())?;
    job.bind((1, file_id)).map_err(|e| e.to_string())?;
    job.bind((2, now)).map_err(|e| e.to_string())?;
    job.bind((3, now)).map_err(|e| e.to_string())?;
    job.next().map_err(|e| e.to_string())?;
    drop(job);
    drop(duplicate);
    drop(update);
    drop(conn);
    crate::sync::scheduler::signal_sync_work();
    log::info!("TCloud Explorer: upload enfileirado para {}", path.display());
    Ok(())
}

fn persist_local_rename(db: &DbConnection, root: &Path, old: &Path, new: &Path) -> Result<(), String> {
    if should_ignore(old) || should_ignore(new) { return Ok(()); }
    let old_rel = relative(root, old)?;

    // Normal directories are not CFAPI placeholders, so Windows may report only
    // one directory rename instead of a rename callback for every child file.
    // Rebind the tracked subtree here and queue cross-destination relocations in
    // one pass. This makes dragging/renaming folders behave like Explorer users
    // expect while preserving local-only subfolders inside the same destination.
    if new.is_dir(){
        let new_rel=relative(root,new)?;
        let cloud_count=crate::commands::tcloud_cfapi::handle_local_folder_move(db,&old_rel,&new_rel)?;

        // Pending local uploads have no CFAPI identity yet. Keep their path/chat
        // binding coherent so their existing jobs upload from the new location.
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let mut q=conn.prepare(
            "SELECT id,path FROM tcloud_files
             WHERE telegram_message_id IS NULL AND sync_status IN ('pending_upload','uploading')"
        ).map_err(|e|e.to_string())?;
        let mut local_rows=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            let id=q.read::<i64,_>(0).unwrap_or(0);
            let path=q.read::<String,_>(1).unwrap_or_default();
            if Path::new(&path).starts_with(old){local_rows.push((id,path));}
        }
        drop(q);
        let now=chrono::Utc::now().timestamp();
        let mut local_count=0usize;
        for (id,old_path) in local_rows{
            let Ok(suffix)=Path::new(&old_path).strip_prefix(old) else{continue;};
            let target=new.join(suffix);
            let target_rel=relative(root,&target)?;
            let chat_key=crate::commands::tcloud_cfapi::chat_key_for_relative_path(&conn,Path::new(&target_rel))?;
            let target_string=target.to_string_lossy().to_string();
            let mut update=conn.prepare(
                "UPDATE tcloud_files SET path=?1,telegram_chat_id=?2,updated_at=?3 WHERE id=?4"
            ).map_err(|e|e.to_string())?;
            update.bind((1,target_string.as_str())).map_err(|e|e.to_string())?;
            update.bind((2,chat_key)).map_err(|e|e.to_string())?;
            update.bind((3,now)).map_err(|e|e.to_string())?;
            update.bind((4,id)).map_err(|e|e.to_string())?;
            update.next().map_err(|e|e.to_string())?;
            local_count+=1;
        }
        log::info!("TCloud Explorer: pasta movida/renomeada; {} arquivo(s) da nuvem e {} upload(s) local(is) reagrupados.",cloud_count,local_count);
        return Ok(());
    }

    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    // Existing placeholders are authoritative through NOTIFY_RENAME, including
    // cross-folder moves. Avoid duplicating that durable CFAPI operation here.
    let mut cloud = conn.prepare("SELECT file_id FROM tcloud_cfapi_items WHERE relative_path=?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    cloud.bind((1, old_rel.as_str())).map_err(|e| e.to_string())?;
    if matches!(cloud.next(), Ok(sqlite::State::Row)) { return Ok(()); }
    drop(cloud);

    let old_full = old.to_string_lossy().to_string();
    let new_full = new.to_string_lossy().to_string();
    let new_rel = relative(root, new)?;
    let chat_key = crate::commands::tcloud_cfapi::chat_key_for_relative_path(&conn, Path::new(&new_rel))?;
    let new_name = new.file_name().and_then(|v| v.to_str()).unwrap_or("arquivo");
    let mut update = conn.prepare(
        "UPDATE tcloud_files SET path=?1,name=?2,telegram_chat_id=?3,updated_at=?4 WHERE path=?5 AND telegram_message_id IS NULL"
    ).map_err(|e| e.to_string())?;
    update.bind((1, new_full.as_str())).map_err(|e| e.to_string())?;
    update.bind((2, new_name)).map_err(|e| e.to_string())?;
    update.bind((3, chat_key)).map_err(|e| e.to_string())?;
    update.bind((4, chrono::Utc::now().timestamp())).map_err(|e| e.to_string())?;
    update.bind((5, old_full.as_str())).map_err(|e| e.to_string())?;
    update.next().map_err(|e| e.to_string())?;
    Ok(())
}

struct DeferredRemoveRequest {
    db: DbConnection,
    root: PathBuf,
    path: PathBuf,
    due: std::time::Instant,
}

fn deferred_remove_sender() -> &'static mpsc::Sender<DeferredRemoveRequest> {
    static SENDER: OnceLock<mpsc::Sender<DeferredRemoveRequest>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DeferredRemoveRequest>();
        std::thread::Builder::new()
            .name("tcloud-watcher-delete-grace".to_string())
            .spawn(move || {
                let mut pending: HashMap<PathBuf, DeferredRemoveRequest> = HashMap::new();
                loop {
                    let received = if pending.is_empty() {
                        match rx.recv() { Ok(request)=>Some(request), Err(_)=>break }
                    } else {
                        let now=std::time::Instant::now();
                        let next_due=pending.values().map(|request|request.due).min().unwrap_or(now);
                        match rx.recv_timeout(next_due.saturating_duration_since(now)) {
                            Ok(request)=>Some(request),
                            Err(mpsc::RecvTimeoutError::Timeout)=>None,
                            Err(mpsc::RecvTimeoutError::Disconnected)=>break,
                        }
                    };
                    if let Some(request)=received {
                        pending.insert(request.path.clone(),request);
                    }
                    let now=std::time::Instant::now();
                    let ready:Vec<PathBuf>=pending.iter()
                        .filter_map(|(path,request)|(request.due<=now).then_some(path.clone()))
                        .collect();
                    for key in ready {
                        let Some(request)=pending.remove(&key) else{continue;};
                        // Office, editors and compilers commonly save atomically by
                        // deleting the old entry and recreating the same path. Give
                        // that replacement a brief grace period before turning the
                        // removal into cloud Trash. If bytes reappear, treat it as a
                        // content edit instead of a destructive delete.
                        if request.path.is_file() {
                            if let Err(error)=persist_upload_job(&request.db,&request.root,&request.path) {
                                log::warn!("TCloud Explorer: arquivo reapareceu após delete, mas o refresh falhou: {}",error);
                            }
                            continue;
                        }
                        if request.path.exists() { continue; }
                        if let Err(error)=persist_remove_now(&request.db,&request.root,&request.path) {
                            log::warn!("TCloud Explorer delete diferido: {}",error);
                        }
                    }
                }
            })
            .expect("failed to start TCloud deferred delete worker");
        tx
    })
}

fn persist_remove(db: &DbConnection, root: &Path, path: &Path) -> Result<(), String> {
    let request=DeferredRemoveRequest{
        db:db.clone(),root:root.to_path_buf(),path:path.to_path_buf(),
        due:std::time::Instant::now()+std::time::Duration::from_millis(1600),
    };
    deferred_remove_sender().send(request).map_err(|_|"TCloud deferred delete worker stopped".to_string())
}

fn persist_remove_now(db: &DbConnection, root: &Path, path: &Path) -> Result<(), String> {
    if should_ignore(path) { return Ok(()); }
    let rel = relative(root, path)?;
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    // A directory itself is not a CFAPI placeholder. Some Windows watcher
    // backends therefore report a single folder removal instead of one event for
    // every child. Detect a tracked subtree before the individual file path path
    // lookup and move all remote children to TCloud Trash in one durable batch.
    let prefix = format!("{}/%", rel.trim_matches('/'));
    let mut subtree = conn.prepare(
        "SELECT c.file_id FROM tcloud_cfapi_items c
         JOIN tcloud_files f ON f.id=c.file_id
         WHERE lower(c.relative_path) LIKE lower(?1) AND f.sync_status!='trashed'"
    ).map_err(|e| e.to_string())?;
    subtree.bind((1, prefix.as_str())).map_err(|e| e.to_string())?;
    let mut subtree_ids = Vec::new();
    while let sqlite::State::Row = subtree.next().map_err(|e| e.to_string())? {
        subtree_ids.push(subtree.read::<i64,_>(0).unwrap_or(0));
    }
    drop(subtree);

    // Pending local uploads have no CFAPI row yet, so detect them from their
    // stored physical paths as well. This also makes deleting a newly-created
    // folder reliable before its first upload has finished.
    let mut local = conn.prepare(
        "SELECT id,path FROM tcloud_files WHERE telegram_message_id IS NULL
         AND sync_status IN ('pending_upload','uploading')"
    ).map_err(|e| e.to_string())?;
    let mut local_ids=Vec::new();
    while let sqlite::State::Row=local.next().map_err(|e|e.to_string())?{
        let file_id=local.read::<i64,_>(0).unwrap_or(0);
        let stored_path=local.read::<String,_>(1).unwrap_or_default();
        if Path::new(&stored_path).starts_with(path){local_ids.push(file_id);}
    }
    drop(local);

    if !subtree_ids.is_empty() {
        // SAFETY: A directory removal event is ambiguous on Windows/CFAPI.
        // Refreshes, placeholder reconciliation, renames and namespace rebuilds
        // can make a whole directory disappear temporarily.
        //
        // Never convert a directory-level watcher event into bulk cloud Trash.
        // Real CFAPI file deletes are handled individually by NOTIFY_DELETE,
        // carrying the immutable file identity.
        log::warn!(
            "TCloud Explorer: remoção de pasta detectada em {} com {} arquivo(s) remoto(s). Exclusão em massa bloqueada por segurança.",
            rel,
            subtree_ids.len()
        );

        return Ok(());
    }

    if !local_ids.is_empty() {
        // These files do not exist remotely yet. A genuine local removal may
        // cancel their pending uploads without affecting Telegram files.
        let now=chrono::Utc::now().timestamp();
        for file_id in local_ids{
            let mut jobs=conn.prepare(
                "UPDATE tcloud_sync_jobs SET status='cancelled',cancel_requested=1,updated_at=?1
                 WHERE file_id=?2 AND status IN ('pending','running','paused')"
            ).map_err(|e|e.to_string())?;
            jobs.bind((1,now)).map_err(|e|e.to_string())?;
            jobs.bind((2,file_id)).map_err(|e|e.to_string())?;
            jobs.next().map_err(|e|e.to_string())?;
            let mut file=conn.prepare("UPDATE tcloud_files
SET sync_status='deleted',
    local_status='deleted',
    updated_at=?1
WHERE id=?2
  AND NOT EXISTS (
      SELECT 1
      FROM tcloud_sync_jobs j
      WHERE j.file_id=tcloud_files.id
        AND j.operation IN ('upload','refresh')
        AND (
            j.status IN ('pending','running')
            OR (
                j.status='completed'
                AND j.updated_at >= ?1 - 10
            )
        )
  )").map_err(|e|e.to_string())?;
            file.bind((1,now)).map_err(|e|e.to_string())?;
            file.bind((2,file_id)).map_err(|e|e.to_string())?;
            file.next().map_err(|e|e.to_string())?;
        }
        drop(conn);
        let mut moved=0usize;
        for file_id in subtree_ids{
            if crate::commands::tcloud_sync_maturity::move_file_to_trash(db,file_id).is_ok(){moved+=1;}
        }
        log::info!("TCloud Explorer: pasta removida; {} arquivo(s) remoto(s) enviados para a Lixeira.",moved);
        return Ok(());
    }

    // Cloud placeholders are deleted through CFAPI NOTIFY_DELETE, which carries
    // the immutable remote identity even after the local directory entry is gone.
    let mut cloud = conn.prepare("SELECT file_id FROM tcloud_cfapi_items WHERE relative_path=?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    cloud.bind((1, rel.as_str())).map_err(|e| e.to_string())?;
    if matches!(cloud.next(), Ok(sqlite::State::Row)) { return Ok(()); }
    drop(cloud);

    let full = path.to_string_lossy().to_string();
    let mut lookup = conn.prepare("SELECT id FROM tcloud_files WHERE path=?1 LIMIT 1")
        .map_err(|e| e.to_string())?;
    lookup.bind((1, full.as_str())).map_err(|e| e.to_string())?;
    if !matches!(lookup.next().map_err(|e| e.to_string())?, sqlite::State::Row) { return Ok(()); }
    let file_id = lookup.read::<i64,_>(0).map_err(|e| e.to_string())?;
    drop(lookup);
    // A rename may have moved the immutable CFAPI identity to another relative
    // path during the delete grace period. Never trash that same file merely
    // because the stale physical `path` column still points at the old name.
    let mut tracked=conn.prepare("SELECT 1 FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1")
        .map_err(|e|e.to_string())?;
    tracked.bind((1,file_id)).map_err(|e|e.to_string())?;
    if matches!(tracked.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(());}
    drop(tracked);

    drop(conn);
    // TCloud 12.5: deleting from the watched Explorer tree is a soft delete.
    // The remote Telegram object remains recoverable until Trash is purged.
    crate::commands::tcloud_sync_maturity::move_file_to_trash(db,file_id)?;
    Ok(())
}
