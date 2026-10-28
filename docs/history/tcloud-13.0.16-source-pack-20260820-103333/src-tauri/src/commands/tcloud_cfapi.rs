use crate::db::DbConnection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfapiStatus {
    pub platform_supported: bool,
    pub registered: bool,
    pub root_path: String,
    pub fallback: String,
    pub detail: String,
    pub phase: String,
    pub placeholder_count: i64,
    pub hydration_ready: bool,
    pub provider_connected: bool,
    pub platform_build: Option<u32>,
    pub platform_integration: Option<u32>,
    pub active_fetches: u64,
    pub completed_fetches: u64,
    pub cancelled_fetches: u64,
    pub failed_fetches: u64,
    pub pending_operations: i64,
    pub failed_operations: i64,
    pub encrypted_hydration_ready: bool,
    pub last_provider_error: Option<String>,
    pub shell_registered: bool,
    pub shell_pinning_enabled: bool,
    pub shell_sync_root_id: Option<String>,
    pub shell_detail: String,
    pub package_identity: bool,
    pub package_family_name: Option<String>,
    pub health: String,
    pub native_offline_menu: bool,
    pub fallback_menu_installed: bool,
    pub offline_pinned_count: i64,
    pub cloud_only_count: i64,
    pub cached_count: i64,
    pub auto_repair_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfapiIntegrityResult {
    pub checked: i64,
    pub missing_placeholders: i64,
    pub stale_index: i64,
    pub repaired: i64,
    pub errors: Vec<String>,
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfapiPopulateResult {
    pub indexed: i64,
    pub created: i64,
    pub existing: i64,
    pub skipped: i64,
    pub errors: Vec<String>,
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CfapiFolderNamespaceResult {
    pub expected: i64,
    pub created: i64,
    pub renamed: i64,
    pub removed: i64,
    pub deferred_nonempty: i64,
    pub errors: Vec<String>,
    pub root_path: String,
}

#[derive(Debug, Clone)]
struct ManagedFolderSpec {
    source_key: String,
    kind: String,
    relative_path: String,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CfapiAvailabilityResult {
    pub action: String,
    pub accepted: bool,
    pub root_path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CfapiFileIdentity {
    version: u8,
    file_id: i64,
    chat_key: i64,
    message_id: i64,
    size: i64,
    updated_at: i64,
}

#[derive(Debug)]
struct IndexedFile {
    id: i64,
    #[allow(dead_code)]
    name: String,
    size: i64,
    chat_key: i64,
    message_id: i64,
    created_at: i64,
    updated_at: i64,
}

fn ensure_schema(conn: &sqlite::Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tcloud_cfapi_items (
            file_id INTEGER PRIMARY KEY,
            relative_path TEXT NOT NULL UNIQUE,
            identity_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_cfapi_items_path ON tcloud_cfapi_items(relative_path);
        CREATE TABLE IF NOT EXISTS tcloud_cfapi_pending_ops (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            op_type TEXT NOT NULL,
            file_id INTEGER NOT NULL,
            chat_key INTEGER NOT NULL,
            message_id INTEGER NOT NULL,
            payload_json TEXT,
            created_at INTEGER NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            completed_at INTEGER,
            last_attempt_at INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_cfapi_pending_ops_open
            ON tcloud_cfapi_pending_ops(completed_at, created_at);
        CREATE TABLE IF NOT EXISTS tcloud_offline_pins (
            file_id INTEGER PRIMARY KEY,
            pinned_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tcloud_cfapi_managed_dirs (
            relative_path TEXT PRIMARY KEY,
            source_key TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_tcloud_cfapi_managed_dirs_source
            ON tcloud_cfapi_managed_dirs(source_key);"
    ).map_err(|e| e.to_string())?;
    // Existing 11.x databases may already have the table without the backoff column.
    // Inspect the schema first so normal status refreshes do not intentionally trigger SQL errors.
    let mut has_last_attempt = false;
    if let Ok(mut info) = conn.prepare("PRAGMA table_info(tcloud_cfapi_pending_ops)") {
        while let Ok(sqlite::State::Row) = info.next() {
            if info.read::<String,_>(1).ok().as_deref() == Some("last_attempt_at") {
                has_last_attempt = true;
                break;
            }
        }
    }
    if !has_last_attempt {
        conn.execute("ALTER TABLE tcloud_cfapi_pending_ops ADD COLUMN last_attempt_at INTEGER").map_err(|e| e.to_string())?;
    }
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tcloud_cfapi_pending_ops_recent ON tcloud_cfapi_pending_ops(completed_at, last_attempt_at DESC, created_at DESC)"
    );
    let _ = conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tcloud_cfapi_pending_ops_file_type ON tcloud_cfapi_pending_ops(file_id, op_type, id DESC)"
    );
    Ok(())
}


#[cfg(target_os = "windows")]
fn notify_shell_directory_changed(path: &Path) {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    // SHChangeNotify(SHCNE_UPDATEDIR, SHCNF_PATHW, path, NULL) tells Explorer
    // that the directory contents changed. The filesystem watcher already keeps
    // the on-disk namespace coherent; this call only prevents Explorer windows
    // from waiting for F5 before repainting the tree.
    const SHCNE_UPDATEDIR: i32 = 0x0000_1000;
    const SHCNF_PATHW: u32 = 0x0005;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(event_id: i32, flags: u32, item1: *const c_void, item2: *const c_void);
    }

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<u16>>();
    wide.push(0);
    unsafe {
        SHChangeNotify(
            SHCNE_UPDATEDIR,
            SHCNF_PATHW,
            wide.as_ptr().cast::<c_void>(),
            std::ptr::null(),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn notify_shell_directory_changed(_path: &Path) {}

#[cfg(target_os = "windows")]
fn notify_shell_visual_refresh(path: &Path) {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;

    // 14.22A:
    // O Sync Root pode continuar funcional enquanto o Explorer mant?m
    // ?cone/associa??o visual em cache. Atualizamos o item e as associa??es
    // sem desmontar, cancelar registro ou tocar nos placeholders.
    const SHCNE_UPDATEDIR: i32 = 0x0000_1000;
    const SHCNE_UPDATEITEM: i32 = 0x0000_2000;
    const SHCNE_ASSOCCHANGED: i32 = 0x0800_0000;

    const SHCNF_IDLIST: u32 = 0x0000;
    const SHCNF_PATHW: u32 = 0x0005;
    const SHCNF_FLUSHNOWAIT: u32 = 0x2000;

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(
            event_id: i32,
            flags: u32,
            item1: *const c_void,
            item2: *const c_void,
        );
    }

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<u16>>();
    wide.push(0);

    unsafe {
        // Atualiza o diret?rio e o pr?prio item do Sync Root.
        SHChangeNotify(
            SHCNE_UPDATEDIR,
            SHCNF_PATHW | SHCNF_FLUSHNOWAIT,
            wide.as_ptr().cast::<c_void>(),
            std::ptr::null(),
        );

        SHChangeNotify(
            SHCNE_UPDATEITEM,
            SHCNF_PATHW | SHCNF_FLUSHNOWAIT,
            wide.as_ptr().cast::<c_void>(),
            std::ptr::null(),
        );

        // ASSOCCHANGED deve ser enviado sem item/path.
        SHChangeNotify(
            SHCNE_ASSOCCHANGED,
            SHCNF_IDLIST | SHCNF_FLUSHNOWAIT,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn notify_shell_visual_refresh(_path: &Path) {}

pub(crate) fn cfapi_root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Não foi possível localizar a pasta do usuário.".to_string())?;
    let root = home.join("TCloud");
    std::fs::create_dir_all(&root).map_err(|e| format!("Falha ao criar a raiz CFAPI: {e}"))?;
    Ok(root)
}

/// Short-lived suppression used by the local filesystem watcher. CFAPI itself
/// creates/updates/hydrates placeholders and those filesystem notifications must
/// never be uploaded back to Telegram as if they were user edits.
fn watcher_suppression() -> &'static std::sync::Mutex<HashMap<String, std::time::Instant>> {
    static STATE: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> = std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn normalized_watch_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase()
}

pub(crate) fn suppress_watcher_path(path: &Path, seconds: u64) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds.max(1));
    if let Ok(mut state) = watcher_suppression().lock() {
        state.retain(|_, expiry| *expiry > std::time::Instant::now());
        state.insert(normalized_watch_path(path), until);
    }
}

pub(crate) fn watcher_event_suppressed(path: &Path) -> bool {
    let key = normalized_watch_path(path);
    let now = std::time::Instant::now();
    let Ok(mut state) = watcher_suppression().lock() else { return false; };
    state.retain(|_, expiry| *expiry > now);
    state.iter().any(|(candidate, expiry)| {
        *expiry > now && (key == *candidate || key.starts_with(&(candidate.clone() + "\\")))
    })
}


fn identity_size_for_file(db:&DbConnection,file_id:i64)->Option<i64>{
    let conn=db.lock().ok()?;
    let mut q=conn.prepare("SELECT size FROM tcloud_files WHERE id=?1 LIMIT 1").ok()?;
    q.bind((1,file_id)).ok()?;
    if matches!(q.next().ok()?,sqlite::State::Row){q.read::<i64,_>(0).ok()}else{None}
}

fn local_path_fingerprint(path:&Path)->Option<String>{
    let meta=std::fs::metadata(path).ok()?;
    if !meta.is_file(){return None;}
    let modified_ns=meta.modified().ok()
        .and_then(|value|value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value|value.as_nanos()).unwrap_or(0);
    Some(format!("{}:{}",meta.len(),modified_ns))
}

fn store_local_fingerprint(conn:&sqlite::Connection,file_id:i64,path:&Path){
    let Some(fingerprint)=local_path_fingerprint(path) else{return;};
    if let Ok(mut q)=conn.prepare("UPDATE tcloud_files SET local_fingerprint=?1 WHERE id=?2"){
        let _=q.bind((1,fingerprint.as_str()));
        let _=q.bind((2,file_id));
        let _=q.next();
    }
}

fn marker_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("cfapi");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("registration.json"))
}

fn sanitize_segment(input: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || ch.is_control() {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    let trimmed = out.trim().trim_end_matches(&['.', ' '][..]).trim().to_string();
    let upper = trimmed.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" |
        "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" |
        "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    );
    let mut value = if trimmed.is_empty() { fallback.to_string() } else if reserved { format!("_{trimmed}") } else { trimmed };
    const MAX_SEGMENT: usize = 180;
    if value.chars().count() > MAX_SEGMENT {
        value = value.chars().take(MAX_SEGMENT).collect();
    }
    value
}


fn managed_relative(parts: &[String]) -> String {
    parts.iter()
        .map(|part| sanitize_segment(part, "Arquivos"))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn expected_managed_folders(conn: &sqlite::Connection) -> Result<Vec<ManagedFolderSpec>, String> {
    let mut out = Vec::new();
    out.push(ManagedFolderSpec {
        source_key: "saved:0".into(),
        kind: "saved".into(),
        relative_path: managed_relative(&["Mensagens Salvas".into()]),
    });

    let mut forum_names = HashMap::<i64, String>::new();
    let mut forum_chat_ids = std::collections::HashSet::<i64>::new();
    let mut forums = conn.prepare(
        "SELECT id,telegram_chat_id,name FROM tcloud_forums ORDER BY id"
    ).map_err(|e| e.to_string())?;
    while let sqlite::State::Row = forums.next().map_err(|e| e.to_string())? {
        let id = forums.read::<i64,_>(0).unwrap_or(0);
        let chat_id = forums.read::<i64,_>(1).unwrap_or(0);
        let name = forums.read::<String,_>(2).unwrap_or_else(|_| "TCloud".into());
        if id <= 0 { continue; }
        forum_names.insert(id, name.clone());
        if chat_id != 0 { forum_chat_ids.insert(chat_id); }
        out.push(ManagedFolderSpec {
            source_key: format!("forum:{id}"),
            kind: "forum".into(),
            relative_path: managed_relative(&[name]),
        });
    }
    drop(forums);

    let mut topics = conn.prepare(
        "SELECT t.id,t.forum_id,COALESCE(m.name,t.name) \
         FROM tcloud_forum_topics t \
         LEFT JOIN folder_metadata m ON m.channel_id=-t.id \
         ORDER BY t.forum_id,t.id"
    ).map_err(|e| e.to_string())?;
    while let sqlite::State::Row = topics.next().map_err(|e| e.to_string())? {
        let id = topics.read::<i64,_>(0).unwrap_or(0);
        let forum_id = topics.read::<i64,_>(1).unwrap_or(0);
        let topic_name = topics.read::<String,_>(2).unwrap_or_else(|_| format!("Tópico {id}"));
        let Some(forum_name) = forum_names.get(&forum_id).cloned() else { continue; };
        if id <= 0 { continue; }
        out.push(ManagedFolderSpec {
            source_key: format!("topic:{id}"),
            kind: "topic".into(),
            relative_path: managed_relative(&[forum_name, topic_name]),
        });
    }
    drop(topics);

    let mut legacy = conn.prepare(
        "SELECT channel_id,name FROM folder_metadata WHERE channel_id>0 ORDER BY channel_id"
    ).map_err(|e| e.to_string())?;
    while let sqlite::State::Row = legacy.next().map_err(|e| e.to_string())? {
        let channel_id = legacy.read::<i64,_>(0).unwrap_or(0);
        if channel_id == 0 || forum_chat_ids.contains(&channel_id) { continue; }
        let name = legacy.read::<String,_>(1).unwrap_or_else(|_| format!("Chat {channel_id}"));
        out.push(ManagedFolderSpec {
            source_key: format!("legacy:{channel_id}"),
            kind: "legacy".into(),
            relative_path: managed_relative(&[name]),
        });
    }
    Ok(out)
}

fn tree_contains_regular_file(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else { return true; };
    for entry in entries.flatten() {
        let p = entry.path();
        let Ok(kind) = entry.file_type() else { return true; };
        if kind.is_file() || kind.is_symlink() { return true; }
        if kind.is_dir() && tree_contains_regular_file(&p) { return true; }
    }
    false
}

fn remove_managed_tree_if_safe(path: &Path) -> Result<bool, String> {
    if !path.exists() { return Ok(true); }
    if tree_contains_regular_file(path) { return Ok(false); }
    std::fs::remove_dir_all(path).map_err(|e| format!("Não foi possível remover a pasta antiga {}: {e}", path.display()))?;
    Ok(true)
}

fn reconcile_managed_folder_namespace(app: &AppHandle, conn: &sqlite::Connection) -> Result<CfapiFolderNamespaceResult, String> {
    ensure_schema(conn)?;
    let root = cfapi_root()?;

    // Migration/repair for folders deleted before TCloud started tracking the
    // Explorer directory namespace explicitly. A negative chat key belongs to a
    // forum topic; if that topic no longer exists, already-synchronized files are
    // remote-deleted and must not recreate a fallback "Tópico N" directory.
    let mut orphan_rows=Vec::<(i64,String)>::new();
    if let Ok(mut orphan)=conn.prepare(
        "SELECT f.id,COALESCE(c.relative_path,'')          FROM tcloud_files f LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id          WHERE COALESCE(f.telegram_chat_id,0)<0            AND NOT EXISTS(SELECT 1 FROM tcloud_forum_topics t WHERE t.id=ABS(f.telegram_chat_id))            AND COALESCE(f.sync_status,'') NOT IN ('pending_upload','uploading','pending_move','conflict')"
    ){
        while let Ok(sqlite::State::Row)=orphan.next(){
            orphan_rows.push((orphan.read::<i64,_>(0).unwrap_or(0),orphan.read::<String,_>(1).unwrap_or_default()));
        }
    }
    for (file_id,relative) in orphan_rows{
        if !relative.is_empty(){
            let path=root.join(relative.replace('/',"\\"));
            suppress_watcher_path(&path,15);
            if path.is_file(){
                if let Err(error)=std::fs::remove_file(&path){
                    log::warn!("TCloud Explorer: arquivo órfão {} será removido depois: {}",path.display(),error);
                    continue;
                }
            }
        }
        if let Ok(mut del)=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1"){
            let _=del.bind((1,file_id)); let _=del.next();
        }
        if let Ok(mut done)=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted',updated_at=?1 WHERE id=?2"){
            let _=done.bind((1,chrono::Utc::now().timestamp())); let _=done.bind((2,file_id)); let _=done.next();
        }
    }

    let expected = expected_managed_folders(conn)?;
    let mut result = CfapiFolderNamespaceResult {
        expected: expected.len() as i64,
        root_path: root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let now = chrono::Utc::now().timestamp();

    let mut previous = HashMap::<String,(String,String)>::new();
    let mut q = conn.prepare(
        "SELECT source_key,relative_path,kind FROM tcloud_cfapi_managed_dirs"
    ).map_err(|e| e.to_string())?;
    while let sqlite::State::Row = q.next().map_err(|e| e.to_string())? {
        previous.insert(
            q.read::<String,_>(0).unwrap_or_default(),
            (q.read::<String,_>(1).unwrap_or_default(), q.read::<String,_>(2).unwrap_or_default()),
        );
    }
    drop(q);

    // Parents first. This makes a forum rename move the whole subtree once, after
    // which topic rows only need their metadata path refreshed.
    let mut ordered = expected.clone();
    ordered.sort_by_key(|item| item.relative_path.matches('/').count());
    let expected_keys: std::collections::HashSet<String> = ordered.iter().map(|item| item.source_key.clone()).collect();

    for item in &ordered {
        let target = root.join(item.relative_path.replace('/', "\\"));
        if let Some((old_relative,_)) = previous.get(&item.source_key) {
            if !old_relative.eq_ignore_ascii_case(&item.relative_path) {
                let old_path = root.join(old_relative.replace('/', "\\"));
                if old_path.exists() && !target.exists() {
                    if let Some(parent) = target.parent() { let _ = std::fs::create_dir_all(parent); }
                    suppress_watcher_path(&old_path, 15);
                    suppress_watcher_path(&target, 15);
                    match std::fs::rename(&old_path, &target) {
                        Ok(()) => result.renamed += 1,
                        Err(error) => result.errors.push(format!("{}: não foi possível refletir a renomeação no Explorer: {error}", item.relative_path)),
                    }
                } else if old_path.exists() && target.exists() {
                    match remove_managed_tree_if_safe(&old_path) {
                        Ok(true) => result.removed += 1,
                        Ok(false) => result.deferred_nonempty += 1,
                        Err(error) => result.errors.push(error),
                    }
                }
            }
        }
        if !target.exists() {
            suppress_watcher_path(&target, 12);
            match std::fs::create_dir_all(&target) {
                Ok(()) => result.created += 1,
                Err(error) => {
                    result.errors.push(format!("{}: não foi possível criar a pasta no Explorer: {error}", item.relative_path));
                    continue;
                }
            }
        }
        let mut up = conn.prepare(
            "INSERT INTO tcloud_cfapi_managed_dirs(relative_path,source_key,kind,created_at,updated_at) \
             VALUES(?1,?2,?3,?4,?4) \
             ON CONFLICT(source_key) DO UPDATE SET relative_path=excluded.relative_path,kind=excluded.kind,updated_at=excluded.updated_at"
        ).map_err(|e| e.to_string())?;
        up.bind((1,item.relative_path.as_str())).map_err(|e|e.to_string())?;
        up.bind((2,item.source_key.as_str())).map_err(|e|e.to_string())?;
        up.bind((3,item.kind.as_str())).map_err(|e|e.to_string())?;
        up.bind((4,now)).map_err(|e|e.to_string())?;
        if let Err(error)=up.next(){ result.errors.push(format!("{}: índice de pasta não pôde ser atualizado: {error}",item.relative_path)); }
    }

    // Remove destinations which TCloud itself managed and that no longer exist in
    // Telegram/local metadata. Never delete a tree containing a regular file: that
    // protects hydrated or not-yet-uploaded user data during races.
    for (source_key,(relative,_kind)) in &previous {
        if expected_keys.contains(source_key) { continue; }
        let path = root.join(relative.replace('/', "\\"));
        suppress_watcher_path(&path, 15);
        match remove_managed_tree_if_safe(&path) {
            Ok(true) => {
                let mut del = conn.prepare("DELETE FROM tcloud_cfapi_managed_dirs WHERE source_key=?1").map_err(|e|e.to_string())?;
                del.bind((1,source_key.as_str())).map_err(|e|e.to_string())?;
                let _=del.next();
                result.removed += 1;
            },
            Ok(false) => result.deferred_nonempty += 1,
            Err(error) => result.errors.push(error),
        }
    }

    // Migration cleanup for installations created before managed-dir tracking.
    // A direct child of a forum is always a topic destination in TCloud. Empty
    // children not present in the current topic table are therefore safe stale dirs.
    let mut topics_by_forum = HashMap::<String,std::collections::HashSet<String>>::new();
    for item in &ordered {
        if item.kind != "topic" { continue; }
        let mut parts=item.relative_path.splitn(2,'/');
        let Some(forum)=parts.next() else { continue; };
        let Some(topic)=parts.next() else { continue; };
        topics_by_forum.entry(forum.to_ascii_lowercase()).or_default().insert(topic.to_ascii_lowercase());
    }
    for forum in ordered.iter().filter(|item| item.kind=="forum") {
        let forum_path=root.join(forum.relative_path.replace('/', "\\"));
        let expected_children=topics_by_forum.get(&forum.relative_path.to_ascii_lowercase()).cloned().unwrap_or_default();
        let Ok(children)=std::fs::read_dir(&forum_path) else { continue; };
        for child in children.flatten() {
            let Ok(kind)=child.file_type() else { continue; };
            if !kind.is_dir() { continue; }
            let name=child.file_name().to_string_lossy().to_string();
            if expected_children.contains(&name.to_ascii_lowercase()) { continue; }
            let child_path=child.path();
            suppress_watcher_path(&child_path,15);
            match remove_managed_tree_if_safe(&child_path) {
                Ok(true)=>result.removed+=1,
                Ok(false)=>result.deferred_nonempty+=1,
                Err(error)=>result.errors.push(error),
            }
        }
    }

    if result.created>0 || result.renamed>0 || result.removed>0 || !result.errors.is_empty() {
        log::info!(
            "TCloud Explorer pastas: {} esperadas, {} criadas, {} renomeadas, {} removidas, {} adiadas, {} erro(s).",
            result.expected,result.created,result.renamed,result.removed,result.deferred_nonempty,result.errors.len()
        );
    }
    // The filesystem changes above are immediate, but an already-open Explorer
    // window may cache the directory view. Notify the Shell so the same tree is
    // visible without requiring F5. Refresh the root and first-level locations.
    notify_shell_directory_changed(&root);
    for forum in ordered.iter().filter(|item| item.kind == "forum") {
        notify_shell_directory_changed(&root.join(forum.relative_path.replace('/', "\\")));
    }

    let _ = app.emit("tcloud-explorer-folders-changed", serde_json::json!({
        "created":result.created,"renamed":result.renamed,"removed":result.removed
    }));
    Ok(result)
}

pub(crate) async fn refresh_folder_namespace_now(app: AppHandle) -> Result<CfapiFolderNamespaceResult, String> {
    let marker = marker_path(&app)?;
    if !marker.exists() {
        return Ok(CfapiFolderNamespaceResult { root_path: cfapi_root()?.to_string_lossy().into_owned(), ..Default::default() });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.try_state::<DbConnection>().ok_or_else(|| "Banco do TCloud indisponível".to_string())?;
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        reconcile_managed_folder_namespace(&app,&conn)
    }).await.map_err(|e|format!("Falha ao atualizar pastas do Explorer: {e}"))?
}

pub(crate) fn schedule_folder_namespace_refresh(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(error)=refresh_folder_namespace_now(app).await {
            log::warn!("TCloud Explorer: atualização de pastas adiada: {}",error);
        }
    });
}

fn folder_for_chat(conn: &sqlite::Connection, chat_key: i64, cache: &mut HashMap<i64, Vec<String>>) -> Vec<String> {
    if let Some(value) = cache.get(&chat_key) { return value.clone(); }
    let result = if chat_key == 0 {
        vec!["Mensagens Salvas".to_string()]
    } else if chat_key < 0 {
        let local_id = chat_key.abs();
        let mut q = match conn.prepare(
            "SELECT f.name,t.name FROM tcloud_forum_topics t JOIN tcloud_forums f ON f.id=t.forum_id WHERE t.id=?1 LIMIT 1"
        ) { Ok(q) => q, Err(_) => return vec!["Fóruns".into(), format!("Tópico {local_id}")] };
        if q.bind((1, local_id)).is_ok() && matches!(q.next(), Ok(sqlite::State::Row)) {
            vec![q.read::<String,_>(0).unwrap_or_else(|_| "TCloud".into()), q.read::<String,_>(1).unwrap_or_else(|_| format!("Tópico {local_id}"))]
        } else {
            vec!["Fóruns".into(), format!("Tópico {local_id}")]
        }
    } else {
        let mut q = match conn.prepare("SELECT name FROM folder_metadata WHERE channel_id=?1 LIMIT 1") {
            Ok(q) => q,
            Err(_) => return vec![format!("Chat {chat_key}")],
        };
        if q.bind((1, chat_key)).is_ok() && matches!(q.next(), Ok(sqlite::State::Row)) {
            vec![q.read::<String,_>(0).unwrap_or_else(|_| format!("Chat {chat_key}"))]
        } else {
            vec![format!("Chat {chat_key}")]
        }
    };
    cache.insert(chat_key, result.clone());
    result
}


pub(crate) fn chat_key_for_relative_path(conn: &sqlite::Connection, relative: &Path) -> Result<i64, String> {
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let parts: Vec<String> = parent.components()
        .filter_map(|component| component.as_os_str().to_str().map(|v| v.to_string()))
        .filter(|value| !value.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("Arquivos devem ficar dentro de uma pasta TCloud (por exemplo, Mensagens Salvas ou um tópico).".into());
    }
    // TCloud 12.6.2 Parte 6: subpastas locais são permitidas dentro de um
    // destino TCloud conhecido. A primeira parte do caminho continua definindo
    // o chat (ou as duas primeiras partes no caso de fórum/tópico); o restante
    // é organização local preservada pelo índice CFAPI.
    if parts[0].eq_ignore_ascii_case(&sanitize_segment("Mensagens Salvas", "Mensagens Salvas")) {
        return Ok(0);
    }

    if parts.len() >= 2 {
        let mut q = conn.prepare(
            "SELECT f.name,t.name,t.id FROM tcloud_forum_topics t JOIN tcloud_forums f ON f.id=t.forum_id"
        ).map_err(|e| e.to_string())?;
        while let sqlite::State::Row = q.next().map_err(|e| e.to_string())? {
            let forum_name = q.read::<String,_>(0).unwrap_or_else(|_| "TCloud".into());
            let topic_name = q.read::<String,_>(1).unwrap_or_else(|_| "Arquivos".into());
            let local_id = q.read::<i64,_>(2).unwrap_or(0);
            if sanitize_segment(&forum_name, "TCloud").eq_ignore_ascii_case(&parts[0])
                && sanitize_segment(&topic_name, "Arquivos").eq_ignore_ascii_case(&parts[1])
                && local_id > 0
            {
                return Ok(-local_id);
            }
        }
    }

    if !parts.is_empty() {
        let mut q = conn.prepare("SELECT channel_id,name FROM folder_metadata WHERE channel_id IS NOT NULL")
            .map_err(|e| e.to_string())?;
        while let sqlite::State::Row = q.next().map_err(|e| e.to_string())? {
            let channel_id = q.read::<i64,_>(0).unwrap_or(0);
            let name = q.read::<String,_>(1).unwrap_or_default();
            if channel_id != 0 && sanitize_segment(&name, "Arquivos").eq_ignore_ascii_case(&parts[0]) {
                return Ok(channel_id);
            }
        }
        if let Some(value) = parts[0].strip_prefix("Chat ").and_then(|v| v.parse::<i64>().ok()) {
            return Ok(value);
        }
    }

    Err(format!(
        "A pasta '{}' não corresponde a um destino TCloud conhecido. Mova o arquivo para Mensagens Salvas, um chat ou um tópico existente.",
        parent.to_string_lossy()
    ))
}

fn read_indexed_files(conn: &sqlite::Connection, limit: i64) -> Result<Vec<IndexedFile>, String> {
    let mut q = conn.prepare(
        "SELECT id,name,size,COALESCE(telegram_chat_id,0),telegram_message_id,created_at,updated_at
         FROM tcloud_files
         WHERE telegram_message_id IS NOT NULL AND COALESCE(sync_status,'') NOT IN ('trashed','pending_delete','remote_deleted','applying_remote_delete')
         ORDER BY updated_at DESC LIMIT ?1"
    ).map_err(|e| e.to_string())?;
    q.bind((1, limit)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = q.next().map_err(|e| e.to_string())? {
        let id: i64 = q.read(0).map_err(|e| e.to_string())?;
        let name = q.read(1).unwrap_or_else(|_| "arquivo".into());
        let stored_size = q.read::<i64,_>(2).unwrap_or(0).max(0);
        let chat_key = q.read::<i64,_>(3).unwrap_or(0);
        let message_id = q.read::<i64,_>(4).unwrap_or(0);
        let folder_key = if chat_key == 0 { "home".to_string() } else { chat_key.to_string() };
        let logical_size = (|| -> Result<i64, String> {
            let mut encrypted = conn.prepare(
                "SELECT plaintext_size FROM encrypted_files WHERE folder_key=?1 AND message_id=?2 AND record_state='active' LIMIT 1"
            ).map_err(|e| e.to_string())?;
            encrypted.bind((1, folder_key.as_str())).map_err(|e| e.to_string())?;
            encrypted.bind((2, message_id)).map_err(|e| e.to_string())?;
            if let sqlite::State::Row = encrypted.next().map_err(|e| e.to_string())? {
                Ok(encrypted.read::<i64,_>(0).unwrap_or(stored_size).max(0))
            } else { Ok(stored_size) }
        })().unwrap_or(stored_size);
        out.push(IndexedFile {
            id, name, size: logical_size, chat_key, message_id,
            created_at: q.read::<i64,_>(5).unwrap_or(0),
            updated_at: q.read::<i64,_>(6).unwrap_or(0),
        });
    }
    Ok(out)
}

fn read_indexed_files_for_chat(conn: &sqlite::Connection, chat_key_filter: i64, limit: i64) -> Result<Vec<IndexedFile>, String> {
    let mut q = conn.prepare(
        "SELECT id,name,size,COALESCE(telegram_chat_id,0),telegram_message_id,created_at,updated_at
         FROM tcloud_files
         WHERE telegram_message_id IS NOT NULL
           AND COALESCE(telegram_chat_id,0)=?1
           AND COALESCE(sync_status,'') NOT IN ('trashed','pending_delete','remote_deleted','applying_remote_delete')
         ORDER BY updated_at DESC LIMIT ?2"
    ).map_err(|e| e.to_string())?;
    q.bind((1, chat_key_filter)).map_err(|e| e.to_string())?;
    q.bind((2, limit)).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let sqlite::State::Row = q.next().map_err(|e| e.to_string())? {
        let id: i64 = q.read(0).map_err(|e| e.to_string())?;
        let name = q.read(1).unwrap_or_else(|_| "arquivo".into());
        let stored_size = q.read::<i64,_>(2).unwrap_or(0).max(0);
        let chat_key = q.read::<i64,_>(3).unwrap_or(0);
        let message_id = q.read::<i64,_>(4).unwrap_or(0);
        let folder_key = if chat_key == 0 { "home".to_string() } else { chat_key.to_string() };
        let logical_size = (|| -> Result<i64, String> {
            let mut encrypted = conn.prepare(
                "SELECT plaintext_size FROM encrypted_files WHERE folder_key=?1 AND message_id=?2 AND record_state='active' LIMIT 1"
            ).map_err(|e| e.to_string())?;
            encrypted.bind((1, folder_key.as_str())).map_err(|e| e.to_string())?;
            encrypted.bind((2, message_id)).map_err(|e| e.to_string())?;
            if let sqlite::State::Row = encrypted.next().map_err(|e| e.to_string())? {
                Ok(encrypted.read::<i64,_>(0).unwrap_or(stored_size).max(0))
            } else { Ok(stored_size) }
        })().unwrap_or(stored_size);
        out.push(IndexedFile {
            id, name, size: logical_size, chat_key, message_id,
            created_at: q.read::<i64,_>(5).unwrap_or(0),
            updated_at: q.read::<i64,_>(6).unwrap_or(0),
        });
    }
    Ok(out)
}

fn unique_target(base: &Path, name: &str, message_id: i64, known_relative: Option<&str>, root: &Path) -> (String, PathBuf) {
    let clean = sanitize_segment(name, "arquivo");
    let direct = base.join(&clean);
    if !direct.exists() || known_relative.map(|r| root.join(r) == direct).unwrap_or(false) {
        return (clean, direct);
    }
    let path = Path::new(&clean);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("arquivo");
    let ext = path.extension().and_then(|s| s.to_str());
    for suffix in [message_id.to_string(), format!("msg-{message_id}")] {
        let candidate = match ext { Some(ext) if !ext.is_empty() => format!("{stem} ({suffix}).{ext}"), _ => format!("{stem} ({suffix})") };
        let target = base.join(&candidate);
        if !target.exists() { return (candidate, target); }
    }
    let candidate = format!("{stem} ({message_id}-2){}", ext.map(|x| format!(".{x}")).unwrap_or_default());
    (candidate.clone(), base.join(candidate))
}

fn relative_string(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

fn preserved_parent_for_chat(conn:&sqlite::Connection, root:&Path, relative:&str, chat_key:i64)->Option<PathBuf>{
    use std::path::Component;
    let rel=Path::new(relative);
    // Stored CFAPI paths must always remain lexical descendants of the Sync Root.
    // Refuse absolute, parent ("..") or prefix components before reusing a path.
    if rel.components().any(|component| !matches!(component,Component::Normal(_))){return None;}
    if chat_key_for_relative_path(conn,rel).ok()?!=chat_key{return None;}
    let parent=rel.parent()?;
    Some(root.join(parent))
}

fn known_cfapi_item_for_file(conn:&sqlite::Connection,file:&IndexedFile)->Result<Option<(i64,String,String)>,String>{
    let mut direct=conn.prepare("SELECT file_id,relative_path,identity_json FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;
    direct.bind((1,file.id)).map_err(|e|e.to_string())?;
    if let sqlite::State::Row=direct.next().map_err(|e|e.to_string())?{
        return Ok(Some((direct.read::<i64,_>(0).map_err(|e|e.to_string())?,direct.read::<String,_>(1).map_err(|e|e.to_string())?,direct.read::<String,_>(2).unwrap_or_default())));
    }
    drop(direct);

    let mut scan=conn.prepare("SELECT file_id,relative_path,identity_json FROM tcloud_cfapi_items").map_err(|e|e.to_string())?;
    while let sqlite::State::Row=scan.next().map_err(|e|e.to_string())?{
        let old_id=scan.read::<i64,_>(0).unwrap_or(0);
        let relative=scan.read::<String,_>(1).unwrap_or_default();
        let json=scan.read::<String,_>(2).unwrap_or_default();
        if let Ok(identity)=serde_json::from_str::<CfapiFileIdentity>(&json){
            if identity.chat_key==file.chat_key && identity.message_id==file.message_id{
                return Ok(Some((old_id,relative,json)));
            }
        }
    }
    Ok(None)
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::mem::size_of;
    use std::os::windows::ffi::OsStrExt;
    use std::sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, AtomicU64, Ordering}};

    type HResult = i32;
    type NtStatus = i32;
    type CfRegisterFlags = i32;
    type CfCreateFlags = i32;
    type CfConnectFlags = i32;
    type CfCallbackType = i32;
    type CfOperationType = i32;
    type CfOpenFileFlags = i32;
    type CfPinState = i32;
    type CfSetPinFlags = i32;
    type CfHydrateFlags = i32;
    type CfDehydrateFlags = i32;
    type CfUpdateFlags = i32;
    type CfInSyncState = i32;
    type CfSetInSyncFlags = i32;

    const S_OK: HResult = 0;
    const STATUS_SUCCESS: NtStatus = 0;
    const STATUS_UNSUCCESSFUL: NtStatus = 0xC000_0001u32 as i32;
    const CF_HYDRATION_POLICY_FULL: u16 = 2;
    const CF_HYDRATION_POLICY_MODIFIER_AUTO_DEHYDRATION_ALLOWED: u16 = 0x0004;
    const CF_POPULATION_POLICY_ALWAYS_FULL: u16 = 3;
    const CF_INSYNC_POLICY_TRACK_ALL: u32 = 0x00ff_ffff;
    const CF_HARDLINK_POLICY_NONE: u32 = 0;
    const CF_PLACEHOLDER_MANAGEMENT_POLICY_DEFAULT: u32 = 0;
    const CF_REGISTER_FLAG_NONE: CfRegisterFlags = 0;
    const CF_REGISTER_FLAG_UPDATE: CfRegisterFlags = 1;
    const CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT: CfRegisterFlags = 4;
    const CF_PLACEHOLDER_CREATE_FLAG_MARK_IN_SYNC: i32 = 2;
    const CF_CREATE_FLAG_NONE: CfCreateFlags = 0;
    const CF_CONNECT_FLAG_REQUIRE_FULL_FILE_PATH: CfConnectFlags = 0x0000_0004;
    const CF_CALLBACK_TYPE_NONE: CfCallbackType = -1;
    const CF_CALLBACK_TYPE_FETCH_DATA: CfCallbackType = 0;
    const CF_CALLBACK_TYPE_CANCEL_FETCH_DATA: CfCallbackType = 2;
    const CF_CALLBACK_TYPE_NOTIFY_DEHYDRATE: CfCallbackType = 7;
    const CF_CALLBACK_TYPE_NOTIFY_DEHYDRATE_COMPLETION: CfCallbackType = 8;
    const CF_CALLBACK_TYPE_NOTIFY_DELETE: CfCallbackType = 9;
    const CF_CALLBACK_TYPE_NOTIFY_DELETE_COMPLETION: CfCallbackType = 10;
    const CF_CALLBACK_TYPE_NOTIFY_RENAME: CfCallbackType = 11;
    const CF_CALLBACK_TYPE_NOTIFY_RENAME_COMPLETION: CfCallbackType = 12;
    const CF_OPERATION_TYPE_TRANSFER_DATA: CfOperationType = 0;
    const CF_OPERATION_TYPE_ACK_DEHYDRATE: CfOperationType = 5;
    const CF_OPERATION_TYPE_ACK_DELETE: CfOperationType = 6;
    const CF_OPERATION_TYPE_ACK_RENAME: CfOperationType = 7;
    const CF_OPERATION_TRANSFER_DATA_FLAG_NONE: i32 = 0;
    const CF_OPEN_FILE_FLAG_NONE: CfOpenFileFlags = 0;
    const CF_OPEN_FILE_FLAG_EXCLUSIVE: CfOpenFileFlags = 0x0000_0001;
    const CF_OPEN_FILE_FLAG_WRITE_ACCESS: CfOpenFileFlags = 0x0000_0002;
    const CF_HYDRATE_FLAG_NONE: CfHydrateFlags = 0;
    const CF_DEHYDRATE_FLAG_NONE: CfDehydrateFlags = 0;
    const CF_EOF: i64 = -1;
    const CF_UPDATE_FLAG_MARK_IN_SYNC: CfUpdateFlags = 0x0000_0002;
    const CF_UPDATE_FLAG_ALLOW_PARTIAL: CfUpdateFlags = 0x0000_0400;
    const CF_PIN_STATE_PINNED: CfPinState = 1;
    const CF_PIN_STATE_UNPINNED: CfPinState = 2;
    const CF_SET_PIN_FLAG_NONE: CfSetPinFlags = 0;
    const CF_SET_PIN_FLAG_RECURSE: CfSetPinFlags = 0x0000_0001;
    const CF_SET_PIN_FLAG_RECURSE_STOP_ON_ERROR: CfSetPinFlags = 0x0000_0004;
    const CF_IN_SYNC_STATE_IN_SYNC: CfInSyncState = 1;
    const CF_SET_IN_SYNC_FLAG_NONE: CfSetInSyncFlags = 0;
    const FILE_ATTRIBUTE_ARCHIVE: u32 = 0x20;

    #[repr(transparent)]
    #[derive(Clone, Copy, Default)]
    struct CfConnectionKey(i64);

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Guid { data1: u32, data2: u16, data3: u16, data4: [u8; 8] }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct FileBasicInfo {
        creation_time: i64,
        last_access_time: i64,
        last_write_time: i64,
        change_time: i64,
        file_attributes: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfFsMetadata { basic_info: FileBasicInfo, file_size: i64 }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfHydrationPolicy { primary: u16, modifier: u16 }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfPopulationPolicy { primary: u16, modifier: u16 }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfSyncPolicies {
        struct_size: u32,
        hydration: CfHydrationPolicy,
        population: CfPopulationPolicy,
        in_sync: u32,
        hard_link: u32,
        placeholder_management: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfSyncRegistration {
        struct_size: u32,
        provider_name: *const u16,
        provider_version: *const u16,
        sync_root_identity: *const c_void,
        sync_root_identity_length: u32,
        file_identity: *const c_void,
        file_identity_length: u32,
        provider_id: Guid,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CfPlatformInfo { build_number: u32, revision_number: u32, integration_number: u32 }

    #[repr(C)]
    struct CfPlaceholderCreateInfo {
        relative_file_name: *const u16,
        fs_metadata: CfFsMetadata,
        file_identity: *const c_void,
        file_identity_length: u32,
        flags: i32,
        result: HResult,
        create_usn: i64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfCallbackInfo {
        struct_size: u32,
        connection_key: CfConnectionKey,
        callback_context: *mut c_void,
        volume_guid_name: *const u16,
        volume_dos_name: *const u16,
        volume_serial_number: u32,
        sync_root_file_id: i64,
        sync_root_identity: *const c_void,
        sync_root_identity_length: u32,
        file_id: i64,
        file_size: i64,
        file_identity: *const c_void,
        file_identity_length: u32,
        normalized_path: *const u16,
        transfer_key: i64,
        priority_hint: u8,
        correlation_vector: *mut c_void,
        process_info: *mut c_void,
        request_key: i64,
    }

    #[repr(C)]
    struct CfCallbackParameters { param_size: u32 }

    // Minimal FETCH_DATA prefix used by TCloud. CF_CALLBACK_PARAMETERS is
    // versioned through ParamSize, so only read the fields we actually need.
    // The callback union begins at offset 8 and RequiredFileOffset is 8-byte aligned.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfCallbackFetchDataRequiredParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        _flags_alignment: u32,
        required_file_offset: i64,
        required_length: i64,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfCallbackRenameParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        _flags_alignment: u32,
        target_path: *const u16,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfCallbackRenameCompletionParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        _flags_alignment: u32,
        source_path: *const u16,
    }

    #[repr(C)]
    struct CfOperationAckDehydrateParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        completion_status: NtStatus,
        file_identity: *const c_void,
        file_identity_length: u32,
        _padding: u32,
    }

    #[repr(C)]
    struct CfOperationAckSimpleParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        completion_status: NtStatus,
    }

    #[derive(Debug, Clone, Default)]
    pub struct ProviderDiagnostics {
        pub active_fetches: u64,
        pub completed_fetches: u64,
        pub cancelled_fetches: u64,
        pub failed_fetches: u64,
        pub last_error: Option<String>,
    }

    struct ProviderMetrics {
        active_fetches: AtomicU64,
        completed_fetches: AtomicU64,
        cancelled_fetches: AtomicU64,
        failed_fetches: AtomicU64,
        last_error: Mutex<Option<String>>,
    }

    fn metrics() -> &'static ProviderMetrics {
        static METRICS: OnceLock<ProviderMetrics> = OnceLock::new();
        METRICS.get_or_init(|| ProviderMetrics {
            active_fetches: AtomicU64::new(0),
            completed_fetches: AtomicU64::new(0),
            cancelled_fetches: AtomicU64::new(0),
            failed_fetches: AtomicU64::new(0),
            last_error: Mutex::new(None),
        })
    }

    fn cancellations() -> &'static Mutex<HashMap<i64, Arc<AtomicBool>>> {
        static CANCELLATIONS: OnceLock<Mutex<HashMap<i64, Arc<AtomicBool>>>> = OnceLock::new();
        CANCELLATIONS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    fn begin_fetch(request_key: i64) -> Arc<AtomicBool> {
        let token = Arc::new(AtomicBool::new(false));
        cancellations().lock().unwrap_or_else(|p| p.into_inner()).insert(request_key, token.clone());
        metrics().active_fetches.fetch_add(1, Ordering::Relaxed);
        token
    }

    fn finish_fetch(request_key: i64, success: bool, cancelled: bool, error: Option<&str>) {
        cancellations().lock().unwrap_or_else(|p| p.into_inner()).remove(&request_key);
        metrics().active_fetches.fetch_sub(1, Ordering::Relaxed);
        if cancelled {
            metrics().cancelled_fetches.fetch_add(1, Ordering::Relaxed);
        } else if success {
            metrics().completed_fetches.fetch_add(1, Ordering::Relaxed);
        } else {
            metrics().failed_fetches.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(error) = error {
            *metrics().last_error.lock().unwrap_or_else(|p| p.into_inner()) = Some(error.to_string());
        }
    }

    pub fn cancellation_token(request_key: i64) -> Option<Arc<AtomicBool>> {
        cancellations().lock().ok().and_then(|m| m.get(&request_key).cloned())
    }

    fn cancel_all_fetches() {
        let guard = cancellations().lock().unwrap_or_else(|p| p.into_inner());
        for token in guard.values() {
            token.store(true, Ordering::Relaxed);
        }
    }

    pub fn diagnostics() -> ProviderDiagnostics {
        ProviderDiagnostics {
            active_fetches: metrics().active_fetches.load(Ordering::Relaxed),
            completed_fetches: metrics().completed_fetches.load(Ordering::Relaxed),
            cancelled_fetches: metrics().cancelled_fetches.load(Ordering::Relaxed),
            failed_fetches: metrics().failed_fetches.load(Ordering::Relaxed),
            last_error: metrics().last_error.lock().ok().and_then(|v| v.clone()),
        }
    }

    fn wide_ptr_to_string(ptr: *const u16) -> Option<String> {
        if ptr.is_null() { return None; }
        unsafe {
            let mut len = 0usize;
            while *ptr.add(len) != 0 && len < 32768 { len += 1; }
            Some(String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)))
        }
    }

    type CfCallback = unsafe extern "system" fn(*const CfCallbackInfo, *const CfCallbackParameters);

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CfCallbackRegistration {
        callback_type: CfCallbackType,
        callback: Option<CfCallback>,
    }

    #[repr(C)]
    struct CfOperationInfo {
        struct_size: u32,
        operation_type: CfOperationType,
        connection_key: CfConnectionKey,
        transfer_key: i64,
        correlation_vector: *const c_void,
        sync_status: *const c_void,
        request_key: i64,
    }

    // TRANSFER_DATA occupies 32 bytes and the operation union starts at offset 8.
    // ParamSize must therefore be 40 bytes on the supported Windows ABI.
    #[repr(C)]
    struct CfOperationTransferDataParameters {
        param_size: u32,
        _union_alignment: u32,
        flags: i32,
        completion_status: NtStatus,
        buffer: *const c_void,
        offset: i64,
        length: i64,
    }

    // Guard the hand-written cfapi.h ABI used by the Windows provider.
    // If a field/layout is changed accidentally, the x64 build must fail here
    // instead of producing a corrupted callback at runtime.
    #[cfg(target_pointer_width = "64")]
    const _: [(); 152] = [(); size_of::<CfCallbackInfo>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 32] = [(); size_of::<CfCallbackFetchDataRequiredParameters>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 48] = [(); size_of::<CfOperationInfo>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 40] = [(); size_of::<CfOperationTransferDataParameters>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 24] = [(); size_of::<CfCallbackRenameParameters>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 32] = [(); size_of::<CfOperationAckDehydrateParameters>()];
    #[cfg(target_pointer_width = "64")]
    const _: [(); 16] = [(); size_of::<CfOperationAckSimpleParameters>()];

    #[derive(Debug, Clone)]
    pub struct FetchRequest {
        pub connection_key: i64,
        pub transfer_key: i64,
        pub request_key: i64,
        pub required_offset: i64,
        pub required_length: i64,
        pub file_size: i64,
        pub file_identity: Vec<u8>,
    }

    struct ProviderContext { app: AppHandle }

    #[derive(Clone, Copy)]
    struct ConnectionState {
        key: CfConnectionKey,
        context: usize,
    }

    fn connection_state() -> &'static Mutex<Option<ConnectionState>> {
        static STATE: OnceLock<Mutex<Option<ConnectionState>>> = OnceLock::new();
        STATE.get_or_init(|| Mutex::new(None))
    }

    #[link(name = "CldApi")]
    extern "system" {
        fn CfGetPlatformInfo(platform_version: *mut CfPlatformInfo) -> HResult;
        fn CfRegisterSyncRoot(sync_root_path: *const u16, registration: *const CfSyncRegistration, policies: *const CfSyncPolicies, register_flags: CfRegisterFlags) -> HResult;
        fn CfUnregisterSyncRoot(sync_root_path: *const u16) -> HResult;
        fn CfCreatePlaceholders(base_directory_path: *const u16, placeholder_array: *mut CfPlaceholderCreateInfo, placeholder_count: u32, create_flags: CfCreateFlags, entries_processed: *mut u32) -> HResult;
        fn CfConnectSyncRoot(sync_root_path: *const u16, callback_table: *const CfCallbackRegistration, callback_context: *const c_void, connect_flags: CfConnectFlags, connection_key: *mut CfConnectionKey) -> HResult;
        fn CfDisconnectSyncRoot(connection_key: CfConnectionKey) -> HResult;
        fn CfExecute(op_info: *const CfOperationInfo, op_params: *mut c_void) -> HResult;
        fn CfReportProviderProgress(connection_key: CfConnectionKey, transfer_key: i64, provider_progress_total: i64, provider_progress_completed: i64) -> HResult;
        fn CfOpenFileWithOplock(file_path: *const u16, flags: CfOpenFileFlags, protected_handle: *mut *mut c_void) -> HResult;
        fn CfCloseHandle(file_handle: *mut c_void);
        fn CfSetPinState(file_handle: *mut c_void, pin_state: CfPinState, pin_flags: CfSetPinFlags, overlapped: *mut c_void) -> HResult;
        fn CfSetInSyncState(file_handle: *mut c_void, in_sync_state: CfInSyncState, in_sync_flags: CfSetInSyncFlags, in_sync_usn: *mut i64) -> HResult;
        fn CfHydratePlaceholder(file_handle: *mut c_void, starting_offset: i64, length: i64, hydrate_flags: CfHydrateFlags, overlapped: *mut c_void) -> HResult;
        fn CfDehydratePlaceholder(file_handle: *mut c_void, starting_offset: i64, length: i64, dehydrate_flags: CfDehydrateFlags, overlapped: *mut c_void) -> HResult;
        fn CfConvertToPlaceholder(file_handle: *mut c_void, file_identity: *const c_void, file_identity_length: u32, convert_flags: u32, convert_usn: *mut i64, overlapped: *mut c_void) -> HResult;
        fn CfUpdatePlaceholder(file_handle: *mut c_void, fs_metadata: *const CfFsMetadata, file_identity: *const c_void, file_identity_length: u32, dehydrate_range_array: *const c_void, dehydrate_range_count: u32, update_flags: CfUpdateFlags, update_usn: *mut i64, overlapped: *mut c_void) -> HResult;
    }

    fn wide_path(path: &Path) -> Vec<u16> { path.as_os_str().encode_wide().chain(std::iter::once(0)).collect() }
    fn wide_text(text: &str) -> Vec<u16> { std::ffi::OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect() }
    fn ok(hr: HResult) -> bool { hr >= 0 }
    fn hr_error(action: &str, hr: HResult) -> String { format!("{action} falhou (HRESULT 0x{:08X}).", hr as u32) }

    fn unix_to_filetime(seconds: i64) -> i64 {
        let safe = seconds.max(0) as i128;
        ((safe + 11_644_473_600i128) * 10_000_000i128).min(i64::MAX as i128) as i64
    }

    unsafe fn identity_bytes(info: &CfCallbackInfo) -> Vec<u8> {
        if info.file_identity.is_null() || info.file_identity_length == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(info.file_identity as *const u8, info.file_identity_length as usize).to_vec()
        }
    }

    fn op_info(info: &CfCallbackInfo, operation_type: CfOperationType) -> CfOperationInfo {
        CfOperationInfo {
            struct_size: size_of::<CfOperationInfo>() as u32,
            operation_type,
            connection_key: info.connection_key,
            transfer_key: info.transfer_key,
            correlation_vector: std::ptr::null(),
            sync_status: std::ptr::null(),
            request_key: info.request_key,
        }
    }

    unsafe fn ack_simple(info: &CfCallbackInfo, operation_type: CfOperationType, status: NtStatus) -> Result<(), String> {
        let op = op_info(info, operation_type);
        let mut params = CfOperationAckSimpleParameters {
            param_size: size_of::<CfOperationAckSimpleParameters>() as u32,
            _union_alignment: 0,
            flags: 0,
            completion_status: status,
        };
        let hr = CfExecute(&op, &mut params as *mut _ as *mut c_void);
        if ok(hr) { Ok(()) } else { Err(hr_error("CfExecute/ACK", hr)) }
    }

    unsafe fn ack_dehydrate(info: &CfCallbackInfo, status: NtStatus) -> Result<(), String> {
        let identity = identity_bytes(info);
        let op = op_info(info, CF_OPERATION_TYPE_ACK_DEHYDRATE);
        let mut params = CfOperationAckDehydrateParameters {
            param_size: size_of::<CfOperationAckDehydrateParameters>() as u32,
            _union_alignment: 0,
            flags: 0,
            completion_status: status,
            file_identity: if identity.is_empty() { std::ptr::null() } else { identity.as_ptr() as *const c_void },
            file_identity_length: identity.len() as u32,
            _padding: 0,
        };
        let hr = CfExecute(&op, &mut params as *mut _ as *mut c_void);
        if ok(hr) { Ok(()) } else { Err(hr_error("CfExecute/ACK_DEHYDRATE", hr)) }
    }

    unsafe extern "system" fn fetch_data_callback(
        callback_info: *const CfCallbackInfo,
        callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() || callback_parameters.is_null() { return; }
        let info = *callback_info;
        if ((*callback_parameters).param_size as usize) < size_of::<CfCallbackFetchDataRequiredParameters>() {
            log::error!("CFAPI FETCH_DATA recebeu parâmetros menores que o prefixo suportado.");
            return;
        }
        let params = *(callback_parameters as *const CfCallbackFetchDataRequiredParameters);
        let required_offset = params.required_file_offset.max(0);
        let required_length = if params.required_length > 0 { params.required_length } else { info.file_size.saturating_sub(required_offset).max(0) };
        let request = FetchRequest {
            connection_key: info.connection_key.0,
            transfer_key: info.transfer_key,
            request_key: info.request_key,
            required_offset,
            required_length,
            file_size: info.file_size.max(0),
            file_identity: identity_bytes(&info),
        };
        if info.callback_context.is_null() {
            let _ = fail_fetch(&request);
            return;
        }
        let cancel = begin_fetch(info.request_key);
        let app = (&*(info.callback_context as *const ProviderContext)).app.clone();
        tauri::async_runtime::spawn(async move {
            let result = super::service_fetch_data(app, request.clone(), cancel.clone()).await;
            match result {
                Ok(()) => finish_fetch(request.request_key, true, false, None),
                Err(error) if error.starts_with("[CFAPI_CANCELLED]") || cancel.load(Ordering::Relaxed) => {
                    log::info!("CFAPI FETCH_DATA cancelado: {}", error);
                    finish_fetch(request.request_key, false, true, None);
                }
                Err(error) => {
                    log::error!("CFAPI FETCH_DATA failed: {}", error);
                    let _ = fail_fetch(&request);
                    finish_fetch(request.request_key, false, false, Some(&error));
                }
            }
        });
    }

    unsafe extern "system" fn cancel_fetch_data_callback(
        callback_info: *const CfCallbackInfo,
        _callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() { return; }
        let request_key = (*callback_info).request_key;
        if let Some(token) = cancellation_token(request_key) {
            token.store(true, Ordering::Relaxed);
        }
    }

    unsafe extern "system" fn notify_dehydrate_callback(
        callback_info: *const CfCallbackInfo,
        _callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() { return; }
        if let Err(error) = ack_dehydrate(&*callback_info, STATUS_SUCCESS) {
            log::warn!("CFAPI ACK_DEHYDRATE falhou: {}", error);
        }
    }

    unsafe extern "system" fn notify_dehydrate_completion_callback(
        callback_info: *const CfCallbackInfo,
        _callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() { return; }
        let info = *callback_info;
        if info.callback_context.is_null() { return; }
        let app = (&*(info.callback_context as *const ProviderContext)).app.clone();
        let identity = identity_bytes(&info);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = super::service_dehydrate_completion(app, identity).await {
                log::warn!("CFAPI pós-desidratação: {}", error);
            }
        });
    }

    unsafe extern "system" fn notify_delete_callback(
        callback_info: *const CfCallbackInfo,
        _callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() { return; }
        let info = &*callback_info;
        let valid_identity = serde_json::from_slice::<CfapiFileIdentity>(&identity_bytes(info))
            .map(|identity| identity.version == 1 && identity.file_id > 0 && identity.message_id > 0)
            .unwrap_or(false);
        let status = if valid_identity { STATUS_SUCCESS } else { STATUS_UNSUCCESSFUL };
        if let Err(error) = ack_simple(info, CF_OPERATION_TYPE_ACK_DELETE, status) {
            log::warn!("CFAPI ACK_DELETE falhou: {}", error);
        }
    }

    unsafe extern "system" fn notify_delete_completion_callback(
        callback_info: *const CfCallbackInfo,
        _callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() { return; }
        let info = *callback_info;
        if info.callback_context.is_null() { return; }
        let app = (&*(info.callback_context as *const ProviderContext)).app.clone();
        let identity = identity_bytes(&info);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = super::service_delete_completion(app, identity).await {
                log::error!("CFAPI delete completion: {}", error);
            }
        });
    }

    unsafe extern "system" fn notify_rename_callback(
        callback_info: *const CfCallbackInfo,
        callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() || callback_parameters.is_null() { return; }
        let info = *callback_info;
        let mut status = STATUS_SUCCESS;
        if ((*callback_parameters).param_size as usize) >= size_of::<CfCallbackRenameParameters>() {
            let params = *(callback_parameters as *const CfCallbackRenameParameters);
            let target = wide_ptr_to_string(params.target_path);
            if info.callback_context.is_null() || !super::rename_allowed_preflight(
                (&*(info.callback_context as *const ProviderContext)).app.clone(),
                identity_bytes(&info),
                target,
            ) {
                status = STATUS_UNSUCCESSFUL;
            }
        } else {
            status = STATUS_UNSUCCESSFUL;
        }
        if let Err(error) = ack_simple(&info, CF_OPERATION_TYPE_ACK_RENAME, status) {
            log::warn!("CFAPI ACK_RENAME falhou: {}", error);
        }
    }

    unsafe extern "system" fn notify_rename_completion_callback(
        callback_info: *const CfCallbackInfo,
        callback_parameters: *const CfCallbackParameters,
    ) {
        if callback_info.is_null() || callback_parameters.is_null() { return; }
        let info = *callback_info;
        if info.callback_context.is_null() { return; }
        let params = if ((*callback_parameters).param_size as usize) >= size_of::<CfCallbackRenameCompletionParameters>() {
            Some(*(callback_parameters as *const CfCallbackRenameCompletionParameters))
        } else { None };
        let source = params.and_then(|p| wide_ptr_to_string(p.source_path));
        let target = wide_ptr_to_string(info.normalized_path);
        let app = (&*(info.callback_context as *const ProviderContext)).app.clone();
        let identity = identity_bytes(&info);
        tauri::async_runtime::spawn(async move {
            if let Err(error) = super::service_rename_completion(app, identity, source, target).await {
                log::error!("CFAPI rename completion: {}", error);
            }
        });
    }

    static CALLBACK_TABLE: [CfCallbackRegistration; 9] = [
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_FETCH_DATA, callback: Some(fetch_data_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_CANCEL_FETCH_DATA, callback: Some(cancel_fetch_data_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_DEHYDRATE, callback: Some(notify_dehydrate_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_DEHYDRATE_COMPLETION, callback: Some(notify_dehydrate_completion_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_DELETE, callback: Some(notify_delete_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_DELETE_COMPLETION, callback: Some(notify_delete_completion_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_RENAME, callback: Some(notify_rename_callback) },
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NOTIFY_RENAME_COMPLETION, callback: Some(notify_rename_completion_callback) },
        // CfConnectSyncRoot requires the callback table to terminate with CF_CALLBACK_TYPE_NONE.
        CfCallbackRegistration { callback_type: CF_CALLBACK_TYPE_NONE, callback: None },
    ];

    fn transfer_data(request: &FetchRequest, offset: i64, data: Option<&[u8]>, length: i64, status: NtStatus) -> Result<(), String> {
        let op_info = CfOperationInfo {
            struct_size: size_of::<CfOperationInfo>() as u32,
            operation_type: CF_OPERATION_TYPE_TRANSFER_DATA,
            connection_key: CfConnectionKey(request.connection_key),
            transfer_key: request.transfer_key,
            correlation_vector: std::ptr::null(),
            sync_status: std::ptr::null(),
            request_key: request.request_key,
        };
        let buffer = data.map(|slice| slice.as_ptr() as *const c_void).unwrap_or(std::ptr::null());
        let mut params = CfOperationTransferDataParameters {
            param_size: size_of::<CfOperationTransferDataParameters>() as u32,
            _union_alignment: 0,
            flags: CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
            completion_status: status,
            buffer,
            offset,
            length,
        };
        let hr = unsafe { CfExecute(&op_info, &mut params as *mut _ as *mut c_void) };
        if ok(hr) { Ok(()) } else { Err(hr_error("CfExecute/TRANSFER_DATA", hr)) }
    }

    pub fn transfer_chunk(request: &FetchRequest, offset: i64, data: &[u8]) -> Result<(), String> {
        if data.is_empty() { return Ok(()); }
        transfer_data(request, offset, Some(data), data.len() as i64, STATUS_SUCCESS)
    }

    pub fn fail_fetch(request: &FetchRequest) -> Result<(), String> {
        const ALIGN: i64 = 4096;
        let file_size = request.file_size.max(0);
        if file_size == 0 {
            return transfer_data(request, 0, None, 0, STATUS_UNSUCCESSFUL);
        }
        let requested = request.required_offset.clamp(0, file_size);
        let offset = (requested / ALIGN) * ALIGN;
        let requested_end = if request.required_length > 0 {
            requested.saturating_add(request.required_length).min(file_size)
        } else {
            file_size
        };
        let mut end = requested_end;
        if end < file_size { end = ((end + ALIGN - 1) / ALIGN) * ALIGN; }
        end = end.min(file_size).max(offset);
        transfer_data(request, offset, None, end.saturating_sub(offset), STATUS_UNSUCCESSFUL)
    }

    pub fn report_progress(request: &FetchRequest, total: i64, completed: i64) {
        let _ = unsafe {
            CfReportProviderProgress(
                CfConnectionKey(request.connection_key),
                request.transfer_key,
                total.max(0),
                completed.clamp(0, total.max(0)),
            )
        };
    }

    pub fn platform_info() -> Option<(u32, u32)> {
        let mut info = CfPlatformInfo::default();
        let hr = unsafe { CfGetPlatformInfo(&mut info) };
        if ok(hr) {
            Some((info.build_number, info.integration_number))
        } else {
            // CfGetPlatformInfo can fail in a freshly registered loose-layout process even
            // when CldApi is present and the same machine already has a working TCloud Sync Root.
            // Do not hide/disable the CFAPI controls solely because this diagnostic probe failed;
            // the real register/connect operations below return their own HRESULTs.
            log::warn!("CfGetPlatformInfo falhou (HRESULT 0x{:08X}); mantendo tentativa CFAPI habilitada.", hr as u32);
            Some((0, 0))
        }
    }

    pub fn register(root: &Path, marker_exists: bool) -> Result<(), String> {
        let root_w = wide_path(root);
        let provider = wide_text("TCloud");
        let version = wide_text(env!("CARGO_PKG_VERSION"));
        let sync_identity = b"tcloud-cfapi-v2";
        let registration = CfSyncRegistration {
            struct_size: size_of::<CfSyncRegistration>() as u32,
            provider_name: provider.as_ptr(),
            provider_version: version.as_ptr(),
            sync_root_identity: sync_identity.as_ptr() as *const c_void,
            sync_root_identity_length: sync_identity.len() as u32,
            file_identity: std::ptr::null(),
            file_identity_length: 0,
            provider_id: Guid::default(),
        };
        let policies = CfSyncPolicies {
            struct_size: size_of::<CfSyncPolicies>() as u32,
            hydration: CfHydrationPolicy { primary: CF_HYDRATION_POLICY_FULL, modifier: CF_HYDRATION_POLICY_MODIFIER_AUTO_DEHYDRATION_ALLOWED },
            population: CfPopulationPolicy { primary: CF_POPULATION_POLICY_ALWAYS_FULL, modifier: 0 },
            in_sync: CF_INSYNC_POLICY_TRACK_ALL,
            hard_link: CF_HARDLINK_POLICY_NONE,
            placeholder_management: CF_PLACEHOLDER_MANAGEMENT_POLICY_DEFAULT,
        };
        // The marker is only TCloud's local memory. A development package refresh can
        // remove the Windows Sync Root while leaving the marker/database intact. Try the
        // expected mode first, then the opposite mode so a stale marker can self-heal.
        let preferred = if marker_exists { CF_REGISTER_FLAG_UPDATE } else { CF_REGISTER_FLAG_NONE };
        let alternate = if marker_exists { CF_REGISTER_FLAG_NONE } else { CF_REGISTER_FLAG_UPDATE };
        let mut hr = unsafe {
            CfRegisterSyncRoot(
                root_w.as_ptr(),
                &registration,
                &policies,
                preferred | CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT,
            )
        };
        if !ok(hr) {
            let first_hr = hr;
            hr = unsafe {
                CfRegisterSyncRoot(
                    root_w.as_ptr(),
                    &registration,
                    &policies,
                    alternate | CF_REGISTER_FLAG_MARK_IN_SYNC_ON_ROOT,
                )
            };
            if ok(hr) {
                log::warn!(
                    "CFAPI: registro recuperado usando modo alternativo após HRESULT 0x{:08X}.",
                    first_hr as u32
                );
            }
        }
        if ok(hr) { Ok(()) } else { Err(hr_error("Registro CFAPI", hr)) }
    }

    pub fn connect(root: &Path, app: AppHandle) -> Result<(), String> {
        let mut state = connection_state().lock().map_err(|_| "CFAPI connection state poisoned".to_string())?;
        if state.is_some() { return Ok(()); }

        let context = Box::new(ProviderContext { app });
        let context_ptr = Box::into_raw(context);
        let root_w = wide_path(root);
        let mut key = CfConnectionKey::default();
        let hr = unsafe {
            CfConnectSyncRoot(
                root_w.as_ptr(),
                CALLBACK_TABLE.as_ptr(),
                context_ptr as *const c_void,
                CF_CONNECT_FLAG_REQUIRE_FULL_FILE_PATH,
                &mut key,
            )
        };
        if !ok(hr) {
            unsafe { drop(Box::from_raw(context_ptr)); }
            return Err(hr_error("Conexão CFAPI/FETCH_DATA", hr));
        }
        *state = Some(ConnectionState { key, context: context_ptr as usize });
        Ok(())
    }

    pub fn disconnect() -> Result<(), String> {
        let mut state = connection_state().lock().map_err(|_| "CFAPI connection state poisoned".to_string())?;
        let Some(current) = *state else { return Ok(()); };
        cancel_all_fetches();
        let hr = unsafe { CfDisconnectSyncRoot(current.key) };
        if !ok(hr) { return Err(hr_error("Desconexão CFAPI", hr)); }
        *state = None;
        unsafe { drop(Box::from_raw(current.context as *mut ProviderContext)); }
        Ok(())
    }

    pub fn provider_connected() -> bool {
        connection_state().lock().map(|state| state.is_some()).unwrap_or(false)
    }

    pub fn unregister(root: &Path) -> Result<(), String> {
        let root_w = wide_path(root);
        let hr = unsafe { CfUnregisterSyncRoot(root_w.as_ptr()) };
        if ok(hr) { Ok(()) } else { Err(hr_error("Desregistro CFAPI", hr)) }
    }

    fn set_pin_state(path: &Path, pinned: bool, recurse: bool) -> Result<(), String> {
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_NONE, &mut handle) };
        if !ok(open_hr) {
            return Err(hr_error("Abertura CFAPI para alterar disponibilidade", open_hr));
        }
        if handle.is_null() {
            return Err("CFAPI retornou um handle nulo ao alterar disponibilidade.".into());
        }
        let pin_state = if pinned { CF_PIN_STATE_PINNED } else { CF_PIN_STATE_UNPINNED };
        let pin_flags = if recurse {
            CF_SET_PIN_FLAG_RECURSE | CF_SET_PIN_FLAG_RECURSE_STOP_ON_ERROR
        } else {
            CF_SET_PIN_FLAG_NONE
        };
        let hr = unsafe { CfSetPinState(handle, pin_state, pin_flags, std::ptr::null_mut()) };
        unsafe { CfCloseHandle(handle); }
        if ok(hr) { Ok(()) } else { Err(hr_error("CfSetPinState", hr)) }
    }

    pub fn set_root_pin_state(root: &Path, pinned: bool) -> Result<(), String> {
        set_pin_state(root, pinned, true)
    }

    pub fn set_item_pin_state(path: &Path, pinned: bool) -> Result<(), String> {
        set_pin_state(path, pinned, path.is_dir())
    }

    /// Force the full contents of one placeholder file onto local disk.
    /// CfSetPinState records user intent; CfHydratePlaceholder is what actually
    /// causes the platform to request missing bytes from the connected provider.
    pub fn hydrate_item(path: &Path) -> Result<(), String> {
        if path.is_dir() { return Ok(()); }
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_WRITE_ACCESS, &mut handle) };
        if !ok(open_hr) { return Err(hr_error("Abertura CFAPI para hidratação explícita", open_hr)); }
        if handle.is_null() { return Err("CFAPI retornou um handle nulo para hidratação explícita.".into()); }
        let hydrate_hr = unsafe { CfHydratePlaceholder(handle, 0, CF_EOF, CF_HYDRATE_FLAG_NONE, std::ptr::null_mut()) };
        if !ok(hydrate_hr) { unsafe { CfCloseHandle(handle); } return Err(hr_error("CfHydratePlaceholder", hydrate_hr)); }
        // Hydration and sync state are independent. Older placeholders can have all
        // bytes locally but still keep Explorer's blue "sync pending" arrows.
        let sync_hr = unsafe { CfSetInSyncState(handle, CF_IN_SYNC_STATE_IN_SYNC, CF_SET_IN_SYNC_FLAG_NONE, std::ptr::null_mut()) };
        unsafe { CfCloseHandle(handle); }
        if ok(sync_hr) { Ok(()) } else { Err(hr_error("CfSetInSyncState após hidratação", sync_hr)) }
    }

    /// Clear stale Explorer "sync pending" state without changing pin/hydration.
    pub fn mark_item_in_sync(path: &Path) -> Result<(), String> {
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_WRITE_ACCESS, &mut handle) };
        if !ok(open_hr) { return Err(hr_error("Abertura CFAPI para reparar estado de sincronização", open_hr)); }
        if handle.is_null() { return Err("CFAPI retornou handle nulo ao reparar estado de sincronização.".into()); }
        let hr = unsafe { CfSetInSyncState(handle, CF_IN_SYNC_STATE_IN_SYNC, CF_SET_IN_SYNC_FLAG_NONE, std::ptr::null_mut()) };
        unsafe { CfCloseHandle(handle); }
        if ok(hr) { Ok(()) } else { Err(hr_error("CfSetInSyncState", hr)) }
    }

    /// Explicitly remove local contents after the placeholder has been unpinned.
    /// An exclusive protected handle is required by CfDehydratePlaceholder.
    pub fn dehydrate_item(path: &Path) -> Result<(), String> {
        if path.is_dir() { return Ok(()); }
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_EXCLUSIVE, &mut handle) };
        if !ok(open_hr) { return Err(hr_error("Abertura CFAPI para liberar espaço", open_hr)); }
        if handle.is_null() { return Err("CFAPI retornou um handle nulo para liberar espaço.".into()); }
        let dehydrate_hr = unsafe { CfDehydratePlaceholder(handle, 0, CF_EOF, CF_DEHYDRATE_FLAG_NONE, std::ptr::null_mut()) };
        if !ok(dehydrate_hr) { unsafe { CfCloseHandle(handle); } return Err(hr_error("CfDehydratePlaceholder", dehydrate_hr)); }
        let sync_hr = unsafe { CfSetInSyncState(handle, CF_IN_SYNC_STATE_IN_SYNC, CF_SET_IN_SYNC_FLAG_NONE, std::ptr::null_mut()) };
        unsafe { CfCloseHandle(handle); }
        if ok(sync_hr) { Ok(()) } else { Err(hr_error("CfSetInSyncState após liberar espaço", sync_hr)) }
    }

    pub fn set_item_availability(path: &Path, pinned: bool) -> Result<(), String> {
        set_item_pin_state(path, pinned)?;
        if path.is_file() {
            if pinned { hydrate_item(path) } else { dehydrate_item(path) }
        } else {
            Ok(())
        }
    }

    pub fn convert_existing_file(path: &Path, identity: &[u8]) -> Result<(), String> {
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_NONE, &mut handle) };
        if !ok(open_hr) {
            return Err(hr_error("Abertura CFAPI para conversão", open_hr));
        }
        if handle.is_null() {
            return Err("CFAPI retornou um handle nulo ao converter arquivo local.".into());
        }
        const CF_CONVERT_FLAG_MARK_IN_SYNC: u32 = 0x00000001;
        let hr = unsafe {
            CfConvertToPlaceholder(
                handle,
                if identity.is_empty() { std::ptr::null() } else { identity.as_ptr() as *const c_void },
                identity.len() as u32,
                CF_CONVERT_FLAG_MARK_IN_SYNC,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        unsafe { CfCloseHandle(handle); }
        if !ok(hr) {
            return Err(hr_error("CfConvertToPlaceholder", hr));
        }
        // A regular local file that survived a shell-registration migration should
        // remain available offline until the user explicitly chooses to free space.
        let _ = set_pin_state(path, true, false);
        Ok(())
    }

    pub fn update_placeholder(path: &Path, size: i64, updated_at: i64, identity: &[u8]) -> Result<(), String> {
        let path_w = wide_path(path);
        let mut handle: *mut c_void = std::ptr::null_mut();
        let open_hr = unsafe { CfOpenFileWithOplock(path_w.as_ptr(), CF_OPEN_FILE_FLAG_WRITE_ACCESS, &mut handle) };
        if !ok(open_hr) { return Err(hr_error("Abertura CFAPI para atualizar placeholder", open_hr)); }
        if handle.is_null() { return Err("CFAPI retornou handle nulo ao atualizar placeholder.".into()); }
        let updated = unix_to_filetime(updated_at);
        let metadata = CfFsMetadata {
            basic_info: FileBasicInfo {
                creation_time: 0,
                last_access_time: 0,
                last_write_time: updated,
                change_time: updated,
                file_attributes: FILE_ATTRIBUTE_ARCHIVE,
            },
            file_size: size.max(0),
        };
        let hr = unsafe {
            CfUpdatePlaceholder(
                handle,
                &metadata,
                identity.as_ptr() as *const c_void,
                identity.len() as u32,
                std::ptr::null(),
                0,
                CF_UPDATE_FLAG_MARK_IN_SYNC | CF_UPDATE_FLAG_ALLOW_PARTIAL,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        unsafe { CfCloseHandle(handle); }
        if ok(hr) { Ok(()) } else { Err(hr_error("CfUpdatePlaceholder", hr)) }
    }

    pub fn create_placeholder(base: &Path, file_name: &str, size: i64, created_at: i64, updated_at: i64, identity: &[u8]) -> Result<(), String> {
        let base_w = wide_path(base);
        let name_w = wide_text(file_name);
        let created = unix_to_filetime(created_at);
        let updated = unix_to_filetime(updated_at);
        let metadata = CfFsMetadata {
            basic_info: FileBasicInfo {
                creation_time: created,
                last_access_time: updated,
                last_write_time: updated,
                change_time: updated,
                file_attributes: FILE_ATTRIBUTE_ARCHIVE,
            },
            file_size: size.max(0),
        };
        let mut info = CfPlaceholderCreateInfo {
            relative_file_name: name_w.as_ptr(),
            fs_metadata: metadata,
            file_identity: identity.as_ptr() as *const c_void,
            file_identity_length: identity.len() as u32,
            flags: CF_PLACEHOLDER_CREATE_FLAG_MARK_IN_SYNC,
            result: S_OK,
            create_usn: 0,
        };
        let mut processed = 0u32;
        let hr = unsafe { CfCreatePlaceholders(base_w.as_ptr(), &mut info, 1, CF_CREATE_FLAG_NONE, &mut processed) };
        if ok(hr) && processed == 1 && ok(info.result) {
            let target = base.join(file_name);
            if let Err(error) = set_pin_state(&target, false, false) {
                log::warn!("Placeholder criado, mas não foi possível marcá-lo como online-only: {}", error);
            }
            Ok(())
        }
        else if !ok(info.result) { Err(hr_error("Criação do placeholder", info.result)) }
        else { Err(hr_error("Criação do placeholder", hr)) }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::*;
    #[derive(Debug, Clone)]
    pub struct FetchRequest {
        pub connection_key:i64,
        pub transfer_key:i64,
        pub request_key:i64,
        pub required_offset:i64,
        pub required_length:i64,
        pub file_size:i64,
        pub file_identity:Vec<u8>,
    }
    #[derive(Debug, Clone, Default)]
    pub struct ProviderDiagnostics {
        pub active_fetches:u64,pub completed_fetches:u64,pub cancelled_fetches:u64,pub failed_fetches:u64,pub last_error:Option<String>,
    }
    pub fn diagnostics()->ProviderDiagnostics{ProviderDiagnostics::default()}
    pub fn platform_info() -> Option<(u32,u32)> { None }
    pub fn register(_root: &Path, _marker_exists: bool) -> Result<(),String> { Err("CFAPI está disponível apenas no Windows.".into()) }
    pub fn connect(_root:&Path,_app:AppHandle)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn disconnect()->Result<(),String>{Ok(())}
    pub fn provider_connected()->bool{false}
    pub fn unregister(_root: &Path) -> Result<(),String> { Err("CFAPI está disponível apenas no Windows.".into()) }
    pub fn create_placeholder(_base:&Path,_file_name:&str,_size:i64,_created_at:i64,_updated_at:i64,_identity:&[u8])->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn update_placeholder(_path:&Path,_size:i64,_updated_at:i64,_identity:&[u8])->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn convert_existing_file(_path:&Path,_identity:&[u8])->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn set_root_pin_state(_root:&Path,_pinned:bool)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn set_item_pin_state(_path:&Path,_pinned:bool)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn hydrate_item(_path:&Path)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn dehydrate_item(_path:&Path)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn set_item_availability(_path:&Path,_pinned:bool)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn transfer_chunk(_request:&FetchRequest,_offset:i64,_data:&[u8])->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}
    pub fn fail_fetch(_request:&FetchRequest)->Result<(),String>{Ok(())}
    pub fn report_progress(_request:&FetchRequest,_total:i64,_completed:i64){}
}

#[cfg(target_os = "windows")]
fn sync_root_missing_error(error: &str) -> bool {
    // HRESULT_FROM_WIN32(ERROR_CLOUD_FILE_NOT_UNDER_SYNC_ROOT). Windows renders
    // this as 0x80070186: the path is no longer registered as a cloud Sync Root.
    error.contains("0x80070186") || error.contains("0X80070186")
}

#[cfg(target_os = "windows")]
fn ensure_provider_connection(app: &AppHandle, root: &Path, marker_exists: bool) -> Result<bool, String> {
    match platform::connect(root, app.clone()) {
        Ok(()) => {
            // 14.21D:
            // O Sync Root pode continuar funcional enquanto o registro visual
            // moderno do Explorer desaparece ou fica inconsistente.
            //
            // Não fazemos unregister aqui para preservar placeholders.
            let shell = shell_integration::query(root);

            if !shell.registered && shell.package_identity {
                match shell_integration::register_full(root) {
                    Ok(_) => {
                        log::info!(
                            "CFAPI self-heal: registro moderno do Explorer restaurado."
                        );
                        notify_shell_visual_refresh(root);
                        return Ok(true);
                    }
                    Err(error) => {
                        log::warn!(
                            "CFAPI self-heal: registro visual do Explorer nao pode ser restaurado agora: {}",
                            error
                        );
                    }
                }
            }

            // Mesmo com o Sync Root registrado, o Explorer pode manter
            // o ?cone antigo em cache. O refresh visual ? seguro e n?o
            // altera registro, hidrata??o ou placeholders.
            if shell.registered {
                notify_shell_visual_refresh(root);
            }

            Ok(false)
        },
        Err(first_error) if sync_root_missing_error(&first_error) => {
            log::warn!(
                "CFAPI: o Windows perdeu o registro do Sync Root; iniciando autorreparo sem apagar arquivos: {}",
                first_error
            );
            // Do not unregister here. CfUnregisterSyncRoot can turn hydrated placeholders
            // into normal files and remove online-only placeholders. When running with
            // Package Identity, first restore the StorageProvider registration so native
            // Explorer pinning/icons return together with the CldApi connection.
            let shell_before = shell_integration::query(root);
            let mut registered_by_shell = shell_before.registered;
            if !registered_by_shell && shell_before.package_identity {
                match shell_integration::register_full(root) {
                    Ok(_) => {
                        registered_by_shell = true;
                        log::info!("CFAPI autorreparo: registro moderno do Explorer restaurado.");
                    }
                    Err(shell_error) => {
                        log::warn!(
                            "CFAPI autorreparo: Shell moderno não pôde ser restaurado imediatamente; usando registro CldApi compatível: {}",
                            shell_error
                        );
                    }
                }
            }
            if !registered_by_shell {
                platform::register(root, marker_exists).map_err(|register_error| {
                    format!(
                        "O Sync Root deixou de estar registrado ({first_error}) e o autorreparo falhou: {register_error}"
                    )
                })?;
            }
            platform::connect(root, app.clone()).map_err(|connect_error| {
                format!(
                    "O Sync Root foi registrado novamente, mas o provider não reconectou: {connect_error}"
                )
            })?;
            log::info!("CFAPI: Sync Root recuperado e provider reconectado em {}.", root.display());
            Ok(true)
        }
        Err(error) => Err(error),
    }
}

#[cfg(not(target_os = "windows"))]
fn ensure_provider_connection(_app: &AppHandle, _root: &Path, _marker_exists: bool) -> Result<bool, String> {
    Err("CFAPI está disponível apenas no Windows.".into())
}

#[cfg(target_os = "windows")]
mod shell_integration {
    use super::*;
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    fn hidden_command(program: &str) -> Command {
        let mut command = Command::new(program);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    use windows::{
        core::{HSTRING, Interface},
        ApplicationModel::Package,
        Storage::{IStorageFolder, StorageFolder},
        Storage::Provider::{
            StorageProviderHardlinkPolicy,
            StorageProviderHydrationPolicy,
            StorageProviderHydrationPolicyModifier,
            StorageProviderInSyncPolicy,
            StorageProviderPopulationPolicy,
            StorageProviderSyncRootInfo,
            StorageProviderSyncRootManager,
        },
    };

    #[derive(Debug, Clone, Default)]
    pub struct Snapshot {
        pub api_supported: bool,
        pub registered: bool,
        pub pinning_enabled: bool,
        pub sync_root_id: Option<String>,
        pub package_identity: bool,
        pub package_family_name: Option<String>,
        pub detail: String,
    }

    #[link(name = "ole32")]
    unsafe extern "system" {
        fn CoInitializeEx(reserved: *mut std::ffi::c_void, coinit: u32) -> i32;
        fn CoUninitialize();
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentPackageFamilyName(package_family_name_length: *mut u32, package_family_name: *mut u16) -> i32;
    }

    struct ComApartment(bool);
    impl Drop for ComApartment {
        fn drop(&mut self) { if self.0 { unsafe { CoUninitialize(); } } }
    }

    fn ensure_com() -> Result<ComApartment, String> {
        const COINIT_MULTITHREADED: u32 = 0;
        const RPC_E_CHANGED_MODE: i32 = 0x80010106u32 as i32;
        let hr=unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED) };
        if hr >= 0 { Ok(ComApartment(true)) }
        else if hr == RPC_E_CHANGED_MODE { Ok(ComApartment(false)) }
        else { Err(format!("CoInitializeEx falhou (HRESULT 0x{:08X}).",hr as u32)) }
    }

    fn error_text(prefix: &str, error: windows::core::Error) -> String {
        format!("{prefix}: {} ({:?})", error.message(), error.code())
    }

    fn storage_folder(root: &Path) -> Result<StorageFolder, String> {
        let path = HSTRING::from(root.to_string_lossy().as_ref());
        StorageFolder::GetFolderFromPathAsync(&path)
            .map_err(|e| error_text("StorageFolder::GetFolderFromPathAsync", e))?
            .get()
            .map_err(|e| error_text("Acesso WinRT ao Sync Root", e))
    }

    fn current_user_sid() -> Result<String, String> {
        let output = hidden_command("whoami.exe")
            .args(["/user", "/fo", "csv", "/nh"])
            .output()
            .map_err(|e| format!("Falha ao consultar o SID do Windows: {e}"))?;
        if !output.status.success() {
            return Err("O Windows não retornou o SID da conta atual.".into());
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let Some(start) = text.find("S-1-") else {
            return Err(format!("SID do Windows não encontrado na resposta do whoami: {}", text.trim()));
        };
        let tail = &text[start..];
        let end = tail
            .find(|c: char| c == '"' || c == ',' || c == '\r' || c == '\n' || c.is_whitespace())
            .unwrap_or(tail.len());
        let sid = tail[..end].trim().to_string();
        if sid.is_empty() { Err("SID do Windows vazio.".into()) } else { Ok(sid) }
    }

    pub fn desired_sync_root_id() -> Result<String, String> {
        Ok(format!("TCloud!{}!Personal", current_user_sid()?))
    }

    fn package_identity() -> (bool, Option<String>) {
        // Prefer the Win32 AppModel API for classic desktop apps. It does not depend on
        // WinRT apartment state and works for loose-layout/MSIX package identity.
        const ERROR_SUCCESS: i32 = 0;
        const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
        const APPMODEL_ERROR_NO_PACKAGE: i32 = 15700;
        let mut length: u32 = 0;
        let first = unsafe { GetCurrentPackageFamilyName(&mut length, std::ptr::null_mut()) };
        if first == APPMODEL_ERROR_NO_PACKAGE {
            return (false, None);
        }
        if (first == ERROR_INSUFFICIENT_BUFFER || first == ERROR_SUCCESS) && length > 0 {
            let mut buffer = vec![0u16; length as usize];
            let second = unsafe { GetCurrentPackageFamilyName(&mut length, buffer.as_mut_ptr()) };
            if second == ERROR_SUCCESS {
                if let Some(pos) = buffer.iter().position(|&v| v == 0) { buffer.truncate(pos); }
                let value = String::from_utf16_lossy(&buffer).trim().to_string();
                if !value.is_empty() { return (true, Some(value)); }
                return (true, None);
            }
        }

        // WinRT fallback keeps compatibility with environments where the Win32 query is
        // unexpectedly unavailable but Package::Current still resolves.
        match Package::Current() {
            Ok(package) => match package.Id() {
                Ok(id) => match id.FamilyName() {
                    Ok(name) => {
                        let value=name.to_string();
                        if value.trim().is_empty() {(true,None)} else {(true,Some(value))}
                    }
                    Err(_) => (true,None),
                },
                Err(_) => (true,None),
            },
            Err(_) => (false,None),
        }
    }

    fn icon_resource() -> String {
        std::env::current_exe()
            .map(|path| format!("{},0", path.to_string_lossy()))
            .unwrap_or_else(|_| "TCloud.exe,0".to_string())
    }

    pub fn query(root: &Path) -> Snapshot {
        let _com=match ensure_com(){Ok(value)=>value,Err(detail)=>return Snapshot{detail,..Snapshot::default()}};
        let (package_identity, package_family_name)=package_identity();
        let api_supported = StorageProviderSyncRootManager::IsSupported().unwrap_or(false);
        if !api_supported {
            return Snapshot {
                api_supported: false,
                registered: false,
                pinning_enabled: false,
                sync_root_id: None,
                package_identity,
                package_family_name,
                detail: "Windows.Storage.Provider não está disponível neste Windows.".into(),
            };
        }
        let folder = match storage_folder(root) {
            Ok(folder) => folder,
            Err(error) => return Snapshot { api_supported: true, detail: error, ..Snapshot::default() },
        };
        let folder_iface = match folder.cast::<IStorageFolder>() {
            Ok(value) => value,
            Err(error) => return Snapshot { api_supported: true, detail: error_text("IStorageFolder", error), ..Snapshot::default() },
        };
        match StorageProviderSyncRootManager::GetSyncRootInformationForFolder(&folder_iface) {
            Ok(info) => {
                let id = info.Id().ok().map(|value| value.to_string()).filter(|value| !value.is_empty());
                let allow_pinning = info.AllowPinning().unwrap_or(false);
                let effective_pinning = allow_pinning;
                Snapshot {
                    api_supported: true,
                    registered: true,
                    pinning_enabled: effective_pinning,
                    sync_root_id: id,
                    package_identity,
                    package_family_name,
                    detail: if allow_pinning && package_identity {
                        "Integração Windows.Storage.Provider ativa com Package Identity e pinning/offline habilitado.".into()
                    } else if allow_pinning {
                        "Integração Windows.Storage.Provider ativa com AllowPinning. Package Identity não é usada como bloqueio para o offline; o menu compatível do TCloud também é instalado como fallback.".into()
                    } else if package_identity {
                        "O TCloud possui Package Identity, mas o Sync Root ainda não confirmou AllowPinning; execute a finalização do Explorer.".into()
                    } else {
                        "O Sync Root está registrado, mas ainda não confirmou AllowPinning; execute a finalização do Explorer.".into()
                    },
                }
            }
            Err(error) => Snapshot {
                api_supported: true,
                registered: false,
                pinning_enabled: false,
                sync_root_id: None,
                package_identity,
                package_family_name,
                detail: error_text("Sync Root ainda não registrado no Shell moderno", error),
            },
        }
    }

    pub fn register_full(root: &Path) -> Result<Snapshot, String> {
        let _com=ensure_com()?;
        let (has_identity, family_name)=package_identity();
        log::info!(
            "Registro Shell moderno: Package Identity={} ({})",
            has_identity,
            family_name.unwrap_or_else(|| "sem identidade de pacote".into())
        );
        if !StorageProviderSyncRootManager::IsSupported().unwrap_or(false) {
            return Err("Este Windows não oferece StorageProviderSyncRootManager.".into());
        }
        std::fs::create_dir_all(root).map_err(|e| format!("Falha ao preparar Sync Root: {e}"))?;
        let folder = storage_folder(root)?;
        let folder_iface = folder.cast::<IStorageFolder>().map_err(|e| error_text("IStorageFolder", e))?;
        let info = StorageProviderSyncRootInfo::new().map_err(|e| error_text("StorageProviderSyncRootInfo", e))?;
        info.SetPath(&folder_iface).map_err(|e| error_text("SetPath", e))?;
        info.SetId(&HSTRING::from(desired_sync_root_id()?)).map_err(|e| error_text("SetId", e))?;
        info.SetDisplayNameResource(&HSTRING::from("TCloud")).map_err(|e| error_text("SetDisplayNameResource", e))?;
        info.SetIconResource(&HSTRING::from(icon_resource())).map_err(|e| error_text("SetIconResource", e))?;
        info.SetHydrationPolicy(StorageProviderHydrationPolicy::Full).map_err(|e| error_text("SetHydrationPolicy", e))?;
        info.SetHydrationPolicyModifier(StorageProviderHydrationPolicyModifier::AutoDehydrationAllowed).map_err(|e| error_text("SetHydrationPolicyModifier", e))?;
        info.SetPopulationPolicy(StorageProviderPopulationPolicy::AlwaysFull).map_err(|e| error_text("SetPopulationPolicy", e))?;
        info.SetInSyncPolicy(StorageProviderInSyncPolicy::PreserveInsyncForSyncEngine).map_err(|e| error_text("SetInSyncPolicy", e))?;
        info.SetHardlinkPolicy(StorageProviderHardlinkPolicy::None).map_err(|e| error_text("SetHardlinkPolicy", e))?;
        info.SetVersion(&HSTRING::from(env!("CARGO_PKG_VERSION"))).map_err(|e| error_text("SetVersion", e))?;
        info.SetAllowPinning(true).map_err(|e| error_text("SetAllowPinning", e))?;
        info.SetShowSiblingsAsGroup(false).map_err(|e| error_text("SetShowSiblingsAsGroup", e))?;
        StorageProviderSyncRootManager::Register(&info).map_err(|e| error_text("StorageProviderSyncRootManager::Register", e))?;
        let snapshot = query(root);
        if !snapshot.registered || !snapshot.pinning_enabled {
            return Err(format!("O Windows aceitou o registro, mas o Shell ainda não confirmou AllowPinning. {}", snapshot.detail));
        }
        Ok(snapshot)
    }

    pub fn unregister(root: &Path) -> Result<(), String> {
        let _com=ensure_com()?;
        let snapshot = query(root);
        let id = snapshot.sync_root_id.or_else(|| desired_sync_root_id().ok());
        let Some(id) = id else { return Ok(()); };
        StorageProviderSyncRootManager::Unregister(&HSTRING::from(id))
            .map_err(|e| error_text("StorageProviderSyncRootManager::Unregister", e))
    }
}

#[cfg(not(target_os = "windows"))]
mod shell_integration {
    use super::*;
    #[derive(Debug, Clone, Default)]
    pub struct Snapshot {
        pub api_supported: bool,
        pub registered: bool,
        pub pinning_enabled: bool,
        pub sync_root_id: Option<String>,
        pub package_identity: bool,
        pub package_family_name: Option<String>,
        pub detail: String,
    }
    pub fn query(_root: &Path) -> Snapshot { Snapshot { detail: "Integração Shell disponível apenas no Windows.".into(), ..Snapshot::default() } }
    pub fn register_full(_root: &Path) -> Result<Snapshot, String> { Err("Integração Shell disponível apenas no Windows.".into()) }
    pub fn unregister(_root: &Path) -> Result<(), String> { Ok(()) }
}


#[cfg(target_os = "windows")]
fn hidden_reg_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = std::process::Command::new("reg.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(target_os = "windows")]
fn reg_add_value(key: &str, name: Option<&str>, value: &str) -> Result<(), String> {
    let mut command=hidden_reg_command();
    command.args(["add", key]);
    match name {
        Some(name) => { command.args(["/v", name]); }
        None => { command.arg("/ve"); }
    }
    let output=command.args(["/t","REG_SZ","/d",value,"/f"]).output()
        .map_err(|e|format!("Falha ao iniciar reg.exe: {e}"))?;
    if output.status.success(){Ok(())}else{
        let text=String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if text.is_empty(){"reg.exe não conseguiu atualizar o menu do Explorer.".into()}else{text})
    }
}

#[cfg(target_os = "windows")]
fn install_explorer_pin_verbs(_root: &Path) -> Result<(), String> {
    let exe=std::env::current_exe().map_err(|e|format!("Executável TCloud não localizado: {e}"))?;
    let exe_text=exe.to_string_lossy();
    // Do not use the registry AppliesTo AQS filter here. On some Windows 11
    // builds it suppresses classic verbs for CFAPI placeholders even though the
    // selected item belongs to the sync root. shell_pin_path() performs the
    // authoritative path check before changing any pin state.
    let commands=[
        ("TCloudKeepOffline","Disponível offline (TCloud)","--cfapi-pin"),
        ("TCloudFreeSpace","Liberar espaço (TCloud)","--cfapi-unpin"),
    ];
    for class in ["*", "Directory"] {
        for &(verb,label,arg) in &commands {
            let key=format!(r"HKCU\Software\Classes\{}\shell\{}",class,verb);
            reg_add_value(&key,None,label)?;
            reg_add_value(&key,Some("Icon"),exe_text.as_ref())?;
            // Remove stale filtering left by previous hotfixes.
            let _=hidden_reg_command().args(["delete", key.as_str(), "/v", "AppliesTo", "/f"]).output();
            let command_key=format!(r"{}\command",key);
            let command_value=format!(r#""{}" {} "%1""#,exe_text,arg);
            reg_add_value(&command_key,None,&command_value)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_explorer_pin_verbs() -> Result<(), String> {
    let mut errors=Vec::new();
    for class in ["*", "Directory"] {
        for verb in ["TCloudKeepOffline", "TCloudFreeSpace"] {
            let key=format!(r"HKCU\Software\Classes\{}\shell\{}",class,verb);
            match hidden_reg_command().args(["delete",key.as_str(),"/f"]).output(){
                Ok(output) if output.status.success()=>{},
                Ok(output)=>{
                    // ERROR_FILE_NOT_FOUND is harmless; reg.exe localizes stderr, so
                    // only retain a diagnostic when the key still exists afterwards.
                    let check=hidden_reg_command().args(["query",key.as_str()]).output();
                    if matches!(check,Ok(value) if value.status.success()){
                        let text=String::from_utf8_lossy(&output.stderr).trim().to_string();
                        errors.push(if text.is_empty(){format!("Não foi possível remover {key}")}else{text});
                    }
                },
                Err(error)=>errors.push(error.to_string()),
            }
        }
    }
    if errors.is_empty(){Ok(())}else{Err(errors.join(" | "))}
}

#[cfg(target_os = "windows")]
fn explorer_pin_verbs_installed() -> bool {
    hidden_reg_command()
        .args(["query",r"HKCU\Software\Classes\*\shell\TCloudKeepOffline"])
        .output()
        .map(|value|value.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn sync_explorer_pin_verbs(root:&Path,native_pinning:bool){
    if native_pinning{
        if let Err(error)=remove_explorer_pin_verbs(){
            log::debug!("Menus compatíveis antigos do Explorer não puderam ser totalmente removidos: {}",error);
        }
    }else if let Err(error)=install_explorer_pin_verbs(root){
        log::warn!("Falha ao instalar menu compatível de offline no Explorer: {}",error);
    }
}

#[cfg(not(target_os = "windows"))]
fn install_explorer_pin_verbs(_root: &Path) -> Result<(), String> { Ok(()) }
#[cfg(not(target_os = "windows"))]
fn remove_explorer_pin_verbs() -> Result<(), String> { Ok(()) }
#[cfg(not(target_os = "windows"))]
fn explorer_pin_verbs_installed() -> bool { false }
#[cfg(not(target_os = "windows"))]
fn sync_explorer_pin_verbs(_root:&Path,_native_pinning:bool){}

/// Entry point used by the Explorer fallback verbs. It intentionally avoids
/// booting Tauri/WebView and only changes the CFAPI pin state of the selected item.
pub fn shell_pin_path(path: &Path, pinned: bool) -> Result<(), String> {
    let root=cfapi_root()?;
    let selected=if path.is_absolute(){path.to_path_buf()}else{
        std::env::current_dir().map_err(|e|e.to_string())?.join(path)
    };
    let root_norm=root.to_string_lossy().replace('/',"\\").to_ascii_lowercase();
    let selected_norm=selected.to_string_lossy().replace('/',"\\").to_ascii_lowercase();
    if selected_norm != root_norm && !selected_norm.starts_with(&(root_norm.clone()+"\\")) {
        return Err("O comando offline do TCloud só pode atuar dentro da pasta TCloud.".into());
    }
    platform::set_item_availability(&selected,pinned)
}

#[cfg(target_os = "windows")]
struct CfapiTempGuard { path: PathBuf, armed: bool }
#[cfg(target_os = "windows")]
impl CfapiTempGuard { fn new(path:PathBuf)->Self{Self{path,armed:true}} fn disarm(&mut self){self.armed=false;} }
#[cfg(target_os = "windows")]
impl Drop for CfapiTempGuard { fn drop(&mut self){ if self.armed { let _=std::fs::remove_file(&self.path); } } }

#[cfg(target_os = "windows")]
fn hydration_lock(file_id: i64) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = locks.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.entry(file_id).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct ResolvedLocalFile {
    id: i64,
    name: String,
    indexed: bool,
}

#[cfg(target_os = "windows")]
fn resolve_local_file_for_identity(db: &DbConnection, identity: &CfapiFileIdentity) -> Result<ResolvedLocalFile, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;

    // Fast path: the local id embedded in the placeholder is still valid.
    let mut by_id = conn.prepare(
        "SELECT id,name FROM tcloud_files
         WHERE id=?1 AND COALESCE(telegram_chat_id,0)=?2 AND telegram_message_id=?3
           AND COALESCE(sync_status,'') NOT IN ('trashed','pending_delete') LIMIT 1"
    ).map_err(|e|e.to_string())?;
    by_id.bind((1,identity.file_id)).map_err(|e|e.to_string())?;
    by_id.bind((2,identity.chat_key)).map_err(|e|e.to_string())?;
    by_id.bind((3,identity.message_id)).map_err(|e|e.to_string())?;
    if let sqlite::State::Row=by_id.next().map_err(|e|e.to_string())? {
        return Ok(ResolvedLocalFile {
            id: by_id.read::<i64,_>(0).map_err(|e|e.to_string())?,
            name: by_id.read::<String,_>(1).map_err(|e|e.to_string())?,
            indexed: true,
        });
    }
    drop(by_id);

    // Stable fallback: local ids can change when the Telegram index is rebuilt.
    // chat_key + telegram_message_id identify the remote object used by the
    // placeholder and therefore survive a local reindex.
    let mut by_remote = conn.prepare(
        "SELECT id,name FROM tcloud_files
         WHERE COALESCE(telegram_chat_id,0)=?1 AND telegram_message_id=?2
           AND COALESCE(sync_status,'') NOT IN ('trashed','pending_delete')
         ORDER BY updated_at DESC LIMIT 1"
    ).map_err(|e|e.to_string())?;
    by_remote.bind((1,identity.chat_key)).map_err(|e|e.to_string())?;
    by_remote.bind((2,identity.message_id)).map_err(|e|e.to_string())?;
    if let sqlite::State::Row=by_remote.next().map_err(|e|e.to_string())? {
        let current_id=by_remote.read::<i64,_>(0).map_err(|e|e.to_string())?;
        let name=by_remote.read::<String,_>(1).map_err(|e|e.to_string())?;
        log::info!(
            "CFAPI reconciliou FileIdentity obsoleta: file_id {} -> {} (chat {}, message {}).",
            identity.file_id,current_id,identity.chat_key,identity.message_id
        );
        return Ok(ResolvedLocalFile { id: current_id, name, indexed: true });
    }

    log::warn!(
        "CFAPI usando identidade remota sem registro local: file_id {}, chat {}, message {}.",
        identity.file_id,identity.chat_key,identity.message_id
    );
    Ok(ResolvedLocalFile {
        id: identity.file_id,
        name: format!("telegram-{}.cache",identity.message_id),
        indexed: false,
    })
}

#[cfg(target_os = "windows")]
async fn valid_cached_file(db: &DbConnection, file_id: i64, expected_size: i64) -> Result<Option<PathBuf>, String> {
    let Some(path)=crate::sync::cache::get_cached_path(db,file_id)? else { return Ok(None); };
    let metadata=match tokio::fs::metadata(&path).await { Ok(v)=>v, Err(_)=>return Ok(None) };
    let len=metadata.len() as i64;
    if metadata.is_file() && (expected_size <= 0 || len == expected_size) {
        let _=crate::sync::cache::touch_cache(db,file_id);
        return Ok(Some(path));
    }
    Ok(None)
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
struct EncryptedCfapiInfo {
    protection_mode: String,
    plaintext_size: i64,
    ciphertext_size: i64,
}

#[cfg(target_os = "windows")]
fn encrypted_info_for_identity(db: &DbConnection, identity: &CfapiFileIdentity) -> Result<Option<EncryptedCfapiInfo>, String> {
    let folder_key = if identity.chat_key == 0 { "home".to_string() } else { identity.chat_key.to_string() };
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn.prepare(
        "SELECT protection_mode,COALESCE(plaintext_size,0),ciphertext_size FROM encrypted_files WHERE folder_key=?1 AND message_id=?2 AND record_state='active' LIMIT 1"
    ).map_err(|e| e.to_string())?;
    stmt.bind((1, folder_key.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, identity.message_id)).map_err(|e| e.to_string())?;
    if let sqlite::State::Row = stmt.next().map_err(|e| e.to_string())? {
        Ok(Some(EncryptedCfapiInfo {
            protection_mode: stmt.read::<String,_>(0).unwrap_or_else(|_| "vault".to_string()),
            plaintext_size: stmt.read::<i64,_>(1).unwrap_or(0).max(0),
            ciphertext_size: stmt.read::<i64,_>(2).unwrap_or(0).max(0),
        }))
    } else { Ok(None) }
}

#[cfg(target_os = "windows")]
fn check_cancel(cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<(), String> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        Err("[CFAPI_CANCELLED] Solicitação de hidratação cancelada pelo Windows.".into())
    } else { Ok(()) }
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
async fn download_cfapi_cache(
    db: &DbConnection,
    telegram: &crate::commands::TelegramState,
    identity: &CfapiFileIdentity,
    local_file: &ResolvedLocalFile,
    request: &platform::FetchRequest,
    cancel: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<PathBuf,String> {
    use tokio::io::AsyncWriteExt;

    let client={ telegram.client.lock().await.clone() }.ok_or_else(|| "Telegram não está conectado; não é possível hidratar o arquivo.".to_string())?;
    let folder_id = if identity.chat_key < 0 {
        let target=crate::commands::tcloud_forums::topic_target(db,Some(identity.chat_key))?
            .ok_or_else(|| format!("Tópico TCloud {} não encontrado.",identity.chat_key))?;
        Some(target.chat_id)
    } else if identity.chat_key == 0 { None } else { Some(identity.chat_key) };
    let peer=crate::commands::utils::resolve_peer(&client,folder_id,&telegram.peer_cache).await?;
    let message_id=i32::try_from(identity.message_id).map_err(|_| "ID de mensagem Telegram inválido para hidratação.".to_string())?;
    let message=client.get_messages_by_id(&peer,&[message_id]).await.map_err(|e|format!("Falha ao localizar mídia no Telegram: {e}"))?
        .into_iter().flatten().next().ok_or_else(|| "Mensagem do arquivo não foi encontrada no Telegram.".to_string())?;
    if message.text() == "TDENC2" {
        return Err("Arquivo TDENC2 detectado sem registro criptográfico local. A hidratação segura foi bloqueada para impedir que ciphertext seja entregue ao Explorer.".to_string());
    }
    let media=message.media().ok_or_else(|| "A mensagem Telegram não contém mídia baixável.".to_string())?;

    let target=crate::sync::cache::cached_path_for(local_file.id,&local_file.name)?;
    if let Some(parent)=target.parent(){tokio::fs::create_dir_all(parent).await.map_err(|e|format!("Falha ao criar cache CFAPI: {e}"))?;}
    let temp=target.with_extension(format!("{}.cfapi.part",target.extension().and_then(|v|v.to_str()).unwrap_or("download")));
    let _=tokio::fs::remove_file(&temp).await;
    let mut output=tokio::fs::File::create(&temp).await.map_err(|e|format!("Falha ao criar download temporário CFAPI: {e}"))?;
    const CHUNK:i32=512*1024;
    let mut stream=client.iter_download(&media).chunk_size(CHUNK);
    let mut downloaded=0i64;
    let expected=identity.size.max(request.file_size).max(0);
    platform::report_progress(request,expected,0);
    loop {
        check_cancel(cancel)?;
        match stream.next().await {
            Ok(Some(bytes))=>{
                output.write_all(&bytes).await.map_err(|e|format!("Falha ao gravar cache CFAPI: {e}"))?;
                downloaded=downloaded.saturating_add(bytes.len() as i64);
                platform::report_progress(request,expected.max(downloaded),downloaded);
            }
            Ok(None)=>break,
            Err(e)=>{let _=tokio::fs::remove_file(&temp).await;return Err(format!("Download Telegram para CFAPI falhou: {e}"));}
        }
    }
    output.flush().await.map_err(|e|format!("Falha ao finalizar cache CFAPI: {e}"))?;
    drop(output);
    if downloaded == 0 && expected > 0 {let _=tokio::fs::remove_file(&temp).await;return Err("Download CFAPI retornou zero bytes.".into());}
    if expected > 0 && downloaded != expected {let _=tokio::fs::remove_file(&temp).await;return Err(format!("Tamanho baixado ({downloaded}) difere do placeholder ({expected}). Atualize os placeholders e tente novamente."));}
    if target.exists(){let _=tokio::fs::remove_file(&target).await;}
    tokio::fs::rename(&temp,&target).await.map_err(|e|format!("Falha ao finalizar cache CFAPI: {e}"))?;
    if local_file.indexed {
        crate::sync::cache::mark_cached(db,local_file.id,&target)?;
    }
    Ok(target)
}

#[cfg(target_os = "windows")]
async fn download_encrypted_cfapi_plaintext(
    app:&AppHandle,
    db:&DbConnection,
    telegram:&crate::commands::TelegramState,
    crypto_state:&crate::crypto::state::CryptoState,
    identity:&CfapiFileIdentity,
    encrypted:&EncryptedCfapiInfo,
    request:&platform::FetchRequest,
    cancel:&std::sync::Arc<std::sync::atomic::AtomicBool>,
)->Result<(PathBuf,i64),String>{
    use tokio::io::AsyncWriteExt;
    check_cancel(cancel)?;
    let client={telegram.client.lock().await.clone()}.ok_or_else(||"Telegram não está conectado; não é possível hidratar o arquivo criptografado.".to_string())?;
    let folder_id=if identity.chat_key<0{
        let target=crate::commands::tcloud_forums::topic_target(db,Some(identity.chat_key))?.ok_or_else(||format!("Tópico TCloud {} não encontrado.",identity.chat_key))?;
        Some(target.chat_id)
    }else if identity.chat_key==0{None}else{Some(identity.chat_key)};
    let peer=crate::commands::utils::resolve_peer(&client,folder_id,&telegram.peer_cache).await?;
    let message_id=i32::try_from(identity.message_id).map_err(|_|"ID de mensagem Telegram inválido para hidratação criptografada.".to_string())?;
    let message=client.get_messages_by_id(&peer,&[message_id]).await.map_err(|e|format!("Falha ao localizar mídia criptografada: {e}"))?.into_iter().flatten().next().ok_or_else(||"Mensagem criptografada não encontrada no Telegram.".to_string())?;
    let media=message.media().ok_or_else(||"A mensagem criptografada não contém mídia.".to_string())?;
    let ciphertext_size=crate::commands::utils::media_size(&media) as i64;
    if encrypted.ciphertext_size>0 && ciphertext_size!=encrypted.ciphertext_size{return Err(format!("Tamanho cifrado mudou: registro {}, Telegram {}.",encrypted.ciphertext_size,ciphertext_size));}

    let temp_dir=app.path().app_data_dir().map_err(|e|e.to_string())?.join("cfapi").join("hydration-temp");
    tokio::fs::create_dir_all(&temp_dir).await.map_err(|e|format!("Falha ao criar temporário CFAPI seguro: {e}"))?;
    let temp=temp_dir.join(format!(".cfapi-{}-{}.plain.part",identity.file_id,crate::crypto::random::random_u64()));
    let mut guard=CfapiTempGuard::new(temp.clone());
    let std_file=crate::commands::fs::create_private_partial_file(&temp).map_err(|e|format!("Falha ao criar arquivo temporário privado: {e}"))?;
    let mut output=tokio::fs::File::from_std(std_file);

    let vault_key=crypto_state.get_current_wrapping_key().ok();
    let mut stream=client.iter_download(&media).chunk_size(512*1024);
    let mut header_bytes=Vec::with_capacity(crate::crypto::policy::MAX_HEADER_LENGTH);
    let mut expected_header_length:Option<usize>=None;
    let mut decryptor:Option<crate::crypto::envelope::decrypt_reader::DecryptReader>=None;
    let mut downloaded=0i64;
    let mut plaintext_written=0i64;
    platform::report_progress(request,encrypted.plaintext_size.max(identity.size).max(0),0);

    loop{
        check_cancel(cancel)?;
        let next=stream.next().await.map_err(|e|format!("Download cifrado CFAPI falhou: {e}"))?;
        let Some(bytes)=next else{break;};
        downloaded=downloaded.saturating_add(bytes.len() as i64);
        if downloaded>ciphertext_size{return Err("Download cifrado excedeu o tamanho declarado.".into());}
        let mut remaining=bytes.as_slice();
        if decryptor.is_none(){
            while !remaining.is_empty() && decryptor.is_none(){
                let target=expected_header_length.unwrap_or(crate::crypto::policy::CORE_HEADER_SIZE);
                let needed=target.saturating_sub(header_bytes.len());
                let take=needed.min(remaining.len());
                header_bytes.extend_from_slice(&remaining[..take]);
                remaining=&remaining[take..];
                if expected_header_length.is_none() && header_bytes.len()==crate::crypto::policy::CORE_HEADER_SIZE{
                    let core=crate::crypto::envelope::CoreHeader::parse(&header_bytes).map_err(|e|format!("Preâmbulo TDENC2 inválido: {e}"))?;
                    let expected=core.header_length as usize;
                    if expected<crate::crypto::policy::CORE_HEADER_SIZE || expected>crate::crypto::policy::MAX_HEADER_LENGTH{return Err("Cabeçalho TDENC2 fora dos limites permitidos.".into());}
                    expected_header_length=Some(expected);
                }
                if let Some(expected)=expected_header_length{
                    if header_bytes.len()==expected{
                        let parsed=crate::crypto::envelope::EnvelopeHeader::parse(&header_bytes).map_err(|e|format!("Cabeçalho TDENC2 inválido: {e}"))?;
                        let expected_total=crate::crypto::envelope::calculate_ciphertext_length(parsed.core.total_plaintext_length,parsed.core.chunk_size,parsed.core.header_length).map_err(|e|format!("Comprimento TDENC2 inválido: {e}"))? as i64;
                        if expected_total!=ciphertext_size{return Err(format!("Comprimento TDENC2 autenticado ({expected_total}) difere do Telegram ({ciphertext_size})."));}
                        let reader=crate::commands::fs::initialize_tdenc2_decryptor(&header_bytes,vault_key.as_ref(),None)?;
                        let logical=reader.plaintext_length() as i64;
                        if encrypted.plaintext_size>0 && encrypted.plaintext_size!=logical{return Err(format!("Tamanho plaintext autenticado ({logical}) difere do registro ({}).",encrypted.plaintext_size));}
                        if identity.size>0 && identity.size!=logical{return Err(format!("Placeholder antigo possui {} bytes, mas o arquivo protegido autentica {logical}. Execute Sincronizar placeholders.",identity.size));}
                        decryptor=Some(reader);
                    }
                }
            }
        }
        if !remaining.is_empty(){
            let reader=decryptor.as_mut().ok_or_else(||"Cabeçalho TDENC2 não foi inicializado.".to_string())?;
            let mut plaintext=reader.feed(remaining).map_err(|e|format!("Autenticação de bloco TDENC2 falhou: {e}"))?;
            if !plaintext.is_empty(){
                output.write_all(&plaintext).await.map_err(|e|format!("Falha ao gravar plaintext autenticado: {e}"))?;
                plaintext_written=plaintext_written.saturating_add(plaintext.len() as i64);
                zeroize::Zeroize::zeroize(&mut plaintext);
                platform::report_progress(request,reader.plaintext_length() as i64,plaintext_written);
            }
        }
    }
    check_cancel(cancel)?;
    if downloaded!=ciphertext_size{return Err(format!("Download cifrado incompleto: {downloaded}/{ciphertext_size}."));}
    let reader=decryptor.as_ref().ok_or_else(||"Arquivo TDENC2 terminou antes do cabeçalho completo.".to_string())?;
    reader.finish().map_err(|e|format!("Registro final TDENC2 inválido: {e}"))?;
    let logical=reader.plaintext_length() as i64;
    if plaintext_written!=logical{return Err(format!("Plaintext autenticado incompleto: {plaintext_written}/{logical}."));}
    output.flush().await.map_err(|e|format!("Falha ao flush do temporário seguro: {e}"))?;
    output.sync_all().await.map_err(|e|format!("Falha ao sincronizar temporário seguro: {e}"))?;
    drop(output);
    crypto_state.touch_activity();
    guard.disarm();
    Ok((temp,logical))
}

#[cfg(target_os = "windows")]
async fn transfer_cached_file(
    path:&Path,
    request:&platform::FetchRequest,
    logical_size:i64,
    cancel:&std::sync::Arc<std::sync::atomic::AtomicBool>,
)->Result<(),String>{
    use tokio::io::{AsyncReadExt,AsyncSeekExt};
    check_cancel(cancel)?;
    let mut file=tokio::fs::File::open(path).await.map_err(|e|format!("Falha ao abrir cache para hidratação: {e}"))?;
    let physical=tokio::fs::metadata(path).await.map_err(|e|format!("Falha ao consultar cache CFAPI: {e}"))?.len() as i64;
    if logical_size>0 && physical!=logical_size{return Err(format!("Arquivo de hidratação possui {physical} bytes, mas o placeholder espera {logical_size}."));}
    if logical_size==0{return Ok(());}
    const ALIGN:i64=4096;
    const TRANSFER_CHUNK:usize=1024*1024;
    let requested_start=request.required_offset.clamp(0,logical_size);
    let requested_end=if request.required_length>0{requested_start.saturating_add(request.required_length).min(logical_size)}else{logical_size};
    let start=(requested_start/ALIGN)*ALIGN;
    let mut end=requested_end;
    if end<logical_size{end=((end+ALIGN-1)/ALIGN)*ALIGN;}
    end=end.min(logical_size).max(start);
    file.seek(std::io::SeekFrom::Start(start as u64)).await.map_err(|e|format!("Falha ao posicionar cache CFAPI: {e}"))?;
    let mut buffer=vec![0u8;TRANSFER_CHUNK];
    let mut offset=start;
    platform::report_progress(request,logical_size,offset.min(logical_size));
    while offset<end{
        check_cancel(cancel)?;
        let want=((end-offset) as usize).min(buffer.len());
        let read=file.read(&mut buffer[..want]).await.map_err(|e|format!("Falha ao ler cache para hidratação: {e}"))?;
        if read==0{return Err("Cache CFAPI terminou antes do intervalo solicitado.".into());}
        let is_final=offset.saturating_add(read as i64)>=logical_size;
        if offset%ALIGN!=0 || (!is_final && read as i64%ALIGN!=0){return Err("Bloco CFAPI intermediário não está alinhado a 4 KB.".into());}
        platform::transfer_chunk(request,offset,&buffer[..read])?;
        offset=offset.saturating_add(read as i64);
        platform::report_progress(request,logical_size,offset.min(logical_size));
    }
    Ok(())
}


#[cfg(target_os = "windows")]
async fn transfer_cfapi_range_from_telegram(
    db:&DbConnection,
    telegram:&crate::commands::TelegramState,
    identity:&CfapiFileIdentity,
    request:&platform::FetchRequest,
    logical_size:i64,
    cancel:&std::sync::Arc<std::sync::atomic::AtomicBool>,
)->Result<(),String>{
    const ALIGN:i64=4096;
    const TELEGRAM_CHUNK:i32=512*1024;

    check_cancel(cancel)?;

    if logical_size<=0{
        return Ok(());
    }

    let requested_start=request.required_offset.clamp(0,logical_size);

    let requested_end=if request.required_length>0{
        requested_start
            .saturating_add(request.required_length)
            .min(logical_size)
    }else{
        logical_size
    };

    let start=(requested_start/ALIGN)*ALIGN;

    let mut end=requested_end;
    if end<logical_size{
        end=((end+ALIGN-1)/ALIGN)*ALIGN;
    }
    end=end.min(logical_size).max(start);

    if end<=start{
        return Ok(());
    }

    let client={
        telegram.client.lock().await.clone()
    }.ok_or_else(||
        "Telegram nao esta conectado; nao e possivel hidratar o intervalo solicitado.".to_string()
    )?;

    let folder_id=if identity.chat_key<0{
        let target=crate::commands::tcloud_forums::topic_target(
            db,
            Some(identity.chat_key)
        )?.ok_or_else(||
            format!("Topico TCloud {} nao encontrado.",identity.chat_key)
        )?;

        Some(target.chat_id)
    }else if identity.chat_key==0{
        None
    }else{
        Some(identity.chat_key)
    };

    let peer=crate::commands::utils::resolve_peer(
        &client,
        folder_id,
        &telegram.peer_cache
    ).await?;

    let message_id=i32::try_from(identity.message_id)
        .map_err(|_|"ID de mensagem Telegram invalido para hidratacao parcial.".to_string())?;

    let message=client
        .get_messages_by_id(&peer,&[message_id])
        .await
        .map_err(|e|format!("Falha ao localizar midia para hidratacao parcial: {e}"))?
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(||"Mensagem do arquivo nao foi encontrada no Telegram.".to_string())?;

    if message.text()=="TDENC2"{
        return Err(
            "Arquivo protegido deve usar o caminho de hidratacao criptografada.".into()
        );
    }

    let media=message
        .media()
        .ok_or_else(||"A mensagem Telegram nao contem midia baixavel.".to_string())?;

    // Grammers permite posicionar DownloadIter em um offset:
    //
    // 1. reduzimos o request para o alinhamento minimo de 4 KB;
    // 2. pulamos a quantidade exata de blocos ate `start`;
    // 3. restauramos 512 KB para throughput normal.
    //
    // Assim nenhum byte anterior ao range solicitado precisa ser baixado.
    let skip_blocks=start/ALIGN;

    let skip_blocks=i32::try_from(skip_blocks)
        .map_err(|_|"Offset remoto grande demais para o iterador Telegram atual.".to_string())?;

    let mut stream=client
        .iter_download(&media)
        .chunk_size(ALIGN as i32)
        .skip_chunks(skip_blocks)
        .chunk_size(TELEGRAM_CHUNK);

    let mut offset=start;

    platform::report_progress(
        request,
        logical_size,
        offset.min(logical_size)
    );

    while offset<end{
        check_cancel(cancel)?;

        let bytes=stream
            .next()
            .await
            .map_err(|e|format!("Download parcial Telegram falhou: {e}"))?
            .ok_or_else(||format!(
                "Telegram terminou o download parcial antes do esperado em {offset}/{end}."
            ))?;

        if bytes.is_empty(){
            return Err(
                "Telegram retornou bloco vazio durante hidratacao parcial.".into()
            );
        }

        let remaining=(end-offset) as usize;
        let take=remaining.min(bytes.len());

        if take==0{
            break;
        }

        let next_offset=offset.saturating_add(take as i64);
        let is_file_end=next_offset>=logical_size;

        if offset%ALIGN!=0{
            return Err(
                "Offset CFAPI parcial nao esta alinhado a 4 KB.".into()
            );
        }

        if !is_file_end && (take as i64)%ALIGN!=0{
            return Err(
                "Bloco CFAPI parcial intermediario nao esta alinhado a 4 KB.".into()
            );
        }

        platform::transfer_chunk(
            request,
            offset,
            &bytes[..take]
        )?;

        offset=next_offset;

        platform::report_progress(
            request,
            logical_size,
            offset.min(logical_size)
        );
    }

    if offset<end{
        return Err(format!(
            "Hidratacao parcial incompleta: {offset}/{end} bytes."
        ));
    }

    log::debug!(
        "CFAPI range Telegram concluido: arquivo={}, mensagem={}, range={}..{}, solicitado={}+{}.",
        identity.file_id,
        identity.message_id,
        start,
        end,
        request.required_offset,
        request.required_length
    );

    Ok(())
}

#[cfg(target_os = "windows")]
async fn service_fetch_data(
    app:AppHandle,
    request:platform::FetchRequest,
    cancel:std::sync::Arc<std::sync::atomic::AtomicBool>,
)->Result<(),String>{
    check_cancel(&cancel)?;
    if request.file_identity.is_empty(){return Err("Placeholder CFAPI sem FileIdentity TCloud.".into());}
    let identity:CfapiFileIdentity=serde_json::from_slice(&request.file_identity).map_err(|e|format!("FileIdentity CFAPI inválida: {e}"))?;
    if identity.version!=1{return Err(format!("Versão de FileIdentity CFAPI não suportada: {}",identity.version));}
    if identity.file_id<=0 || identity.message_id<=0{return Err("FileIdentity CFAPI incompleta.".into());}
    let db=app.try_state::<DbConnection>().ok_or_else(||"Banco TCloud indisponível para CFAPI.".to_string())?.inner().clone();
    let telegram=app.try_state::<crate::commands::TelegramState>().ok_or_else(||"Estado Telegram indisponível para CFAPI.".to_string())?.inner().clone();
    // Hydration writes are provider-owned filesystem activity. Keep a short
    // suppression window and record the resulting local fingerprint; a delayed
    // watcher recheck can then distinguish hydration from a real user edit.
    let placeholder_path={
        let mut value=None;
        if let Ok(conn)=db.lock(){
            if let Ok(mut q)=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1"){
                let _=q.bind((1,identity.file_id));
                if matches!(q.next(),Ok(sqlite::State::Row)){
                    if let Ok(relative)=q.read::<String,_>(0){ if let Ok(root)=cfapi_root(){value=Some(root.join(relative));} }
                }
            }
        }
        if let Some(path)=value.as_ref(){suppress_watcher_path(path,3);}
        value
    };
    let local_file=resolve_local_file_for_identity(&db,&identity)?;
    let lock=hydration_lock(local_file.id);
    let _guard=lock.lock().await;
    check_cancel(&cancel)?;

    if let Some(encrypted)=encrypted_info_for_identity(&db,&identity)?{
        let crypto_state=app.try_state::<crate::crypto::state::CryptoState>().ok_or_else(||"Estado de criptografia indisponível para CFAPI.".to_string())?.inner().clone();
        let (path,logical_size)=download_encrypted_cfapi_plaintext(&app,&db,&telegram,&crypto_state,&identity,&encrypted,&request,&cancel).await
            .map_err(|e|format!("Hidratação protegida ({}) falhou: {e}",encrypted.protection_mode))?;
        let transfer=transfer_cached_file(&path,&request,logical_size,&cancel).await;
        let _=tokio::fs::remove_file(&path).await;
        if transfer.is_ok(){if let Some(local)=placeholder_path.as_ref(){if let Ok(conn)=db.lock(){store_local_fingerprint(&conn,identity.file_id,local);}}}
        return transfer;
    }

    let logical_size=identity.size.max(request.file_size).max(0);

    // Fast path: a complete cache can satisfy arbitrary CFAPI ranges locally.
    if local_file.indexed {
        if let Some(path)=valid_cached_file(&db,local_file.id,logical_size).await?{
            let transfer=transfer_cached_file(
                &path,
                &request,
                logical_size,
                &cancel
            ).await;

            if transfer.is_ok(){
                if let Some(local)=placeholder_path.as_ref(){
                    if let Ok(conn)=db.lock(){
                        store_local_fingerprint(
                            &conn,
                            identity.file_id,
                            local
                        );
                    }
                }
            }

            return transfer;
        }
    }

    // No complete cache: do NOT download the whole Telegram object first.
    // Serve only the range Windows requested. This is especially important
    // for video playback and seeking, where the player may ask independently
    // for the MP4 header, moov atom and later media ranges.
    let transfer=transfer_cfapi_range_from_telegram(
        &db,
        &telegram,
        &identity,
        &request,
        logical_size,
        &cancel
    ).await;

    if transfer.is_ok(){
        if let Some(local)=placeholder_path.as_ref(){
            if let Ok(conn)=db.lock(){
                store_local_fingerprint(
                    &conn,
                    identity.file_id,
                    local
                );
            }
        }
    }

    transfer
}

#[cfg(not(target_os = "windows"))]
async fn service_fetch_data(_app:AppHandle,_request:platform::FetchRequest,_cancel:std::sync::Arc<std::sync::atomic::AtomicBool>)->Result<(),String>{Err("CFAPI está disponível apenas no Windows.".into())}

#[cfg(target_os = "windows")]
fn callback_target_relative(root:&Path,target:&str)->Option<String>{
    let norm=|v:&str|v.replace('/',"\\").trim_end_matches('\\').to_string();
    let strip_drive=|v:String| -> String { if v.as_bytes().get(1)==Some(&b':'){v[2..].to_string()}else{v} };
    let target_norm=strip_drive(norm(target));
    let root_norm=strip_drive(norm(&root.to_string_lossy()));
    let t_lower=target_norm.to_ascii_lowercase();
    let r_lower=root_norm.to_ascii_lowercase();
    let pos=t_lower.find(&r_lower)?;
    let rel=target_norm[pos+root_norm.len()..].trim_matches('\\').replace('\\',"/");
    if rel.is_empty(){None}else{Some(rel)}
}

#[cfg(target_os = "windows")]
fn rename_allowed_preflight(app:AppHandle,identity_bytes:Vec<u8>,target:Option<String>)->bool{
    let Ok(identity)=serde_json::from_slice::<CfapiFileIdentity>(&identity_bytes) else{return false;};
    let Some(target)=target else{return false;};
    let Some(db)=app.try_state::<DbConnection>() else{return false;};
    if encrypted_info_for_identity(db.inner(),&identity).ok().flatten().is_some(){
        // Protected files need TDENC2 metadata re-wrapping before a remote filename
        // can change. Keep that cryptographic boundary explicit for now.
        return false;
    }
    let Ok(root)=cfapi_root() else{return false;};
    let Some(target_rel)=callback_target_relative(&root,&target) else{return false;};
    let Ok(conn)=db.inner().lock() else{return false;};
    if chat_key_for_relative_path(&conn,Path::new(&target_rel)).is_err(){return false;}
    // Windows is case-insensitive. Refuse a rename/move when another tracked
    // placeholder already owns the target path, before the filesystem commit can
    // leave the SQLite index in a UNIQUE(relative_path) failure state.
    let Ok(mut occupied)=conn.prepare(
        "SELECT 1 FROM tcloud_cfapi_items WHERE lower(relative_path)=lower(?1) AND file_id<>?2 LIMIT 1"
    ) else{return false;};
    if occupied.bind((1,target_rel.as_str())).is_err() || occupied.bind((2,identity.file_id)).is_err(){return false;}
    !matches!(occupied.next(),Ok(sqlite::State::Row))
}

#[cfg(not(target_os = "windows"))]
fn rename_allowed_preflight(_app:AppHandle,_identity_bytes:Vec<u8>,_target:Option<String>)->bool{false}

#[cfg(target_os = "windows")]
async fn service_dehydrate_completion(app:AppHandle,identity_bytes:Vec<u8>)->Result<(),String>{
    let identity:CfapiFileIdentity=serde_json::from_slice(&identity_bytes).map_err(|e|e.to_string())?;
    let db=app.try_state::<DbConnection>().ok_or_else(||"Banco TCloud indisponível.".to_string())?.inner().clone();
    let cached=crate::sync::cache::get_cached_path(&db,identity.file_id)?;
    if let Some(path)=cached{let _=tokio::fs::remove_file(path).await;}
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let mut del=conn.prepare("DELETE FROM tcloud_cache WHERE file_id=?1").map_err(|e|e.to_string())?;
    del.bind((1,identity.file_id)).map_err(|e|e.to_string())?;del.next().map_err(|e|e.to_string())?;
    let mut up=conn.prepare("UPDATE tcloud_files SET local_status='cloud_only' WHERE id=?1").map_err(|e|e.to_string())?;
    up.bind((1,identity.file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn cfapi_pending_notify()->&'static tokio::sync::Notify{
    use std::sync::OnceLock;
    static NOTIFY:OnceLock<tokio::sync::Notify>=OnceLock::new();
    NOTIFY.get_or_init(tokio::sync::Notify::new)
}

#[cfg(target_os = "windows")]
pub(crate) fn signal_cfapi_pending_work(){
    cfapi_pending_notify().notify_waiters();
    cfapi_pending_notify().notify_one();
}

#[cfg(target_os = "windows")]
fn queue_pending_op(db:&DbConnection,op_type:&str,identity:&CfapiFileIdentity,payload:Option<&str>)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    ensure_schema(&conn)?;
    let now=chrono::Utc::now().timestamp();
    // Explorer can emit several rename/move callbacks in quick succession (for
    // example drag to folder A and immediately rename, or move A -> B -> C). Keep
    // only the newest path transaction that has not started remotely yet. This
    // prevents intermediate cloud moves from creating ghosts/duplicates.
    if matches!(op_type,"rename"|"relocate"){
        let mut supersede=conn.prepare(
            "UPDATE tcloud_cfapi_pending_ops SET completed_at=?1,last_error='Substituída por uma ação mais recente no Explorer.'
             WHERE file_id=?2 AND completed_at IS NULL AND op_type IN ('rename','relocate')"
        ).map_err(|e|e.to_string())?;
        supersede.bind((1,now)).map_err(|e|e.to_string())?;
        supersede.bind((2,identity.file_id)).map_err(|e|e.to_string())?;
        supersede.next().map_err(|e|e.to_string())?;
    }
    let mut q=conn.prepare(
        "INSERT INTO tcloud_cfapi_pending_ops(op_type,file_id,chat_key,message_id,payload_json,created_at)
         SELECT ?1,?2,?3,?4,?5,?6
         WHERE NOT EXISTS (
           SELECT 1 FROM tcloud_cfapi_pending_ops
           WHERE completed_at IS NULL AND op_type=?1 AND file_id=?2 AND message_id=?4
             AND COALESCE(payload_json,'')=COALESCE(?5,'')
         )"
    ).map_err(|e|e.to_string())?;
    q.bind((1,op_type)).map_err(|e|e.to_string())?;
    q.bind((2,identity.file_id)).map_err(|e|e.to_string())?;
    q.bind((3,identity.chat_key)).map_err(|e|e.to_string())?;
    q.bind((4,identity.message_id)).map_err(|e|e.to_string())?;
    match payload { Some(v)=>q.bind((5,v)).map_err(|e|e.to_string())?, None=>q.bind((5,sqlite::Value::Null)).map_err(|e|e.to_string())? };
    q.bind((6,now)).map_err(|e|e.to_string())?;
    q.next().map_err(|e|e.to_string())?;
    drop(q);
    drop(conn);
    signal_cfapi_pending_work();
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn queue_remote_cleanup(db:&DbConnection,file_id:i64,chat_key:i64,message_id:i32,size:i64)->Result<(),String>{
    let identity=CfapiFileIdentity{version:1,file_id,chat_key,message_id:i64::from(message_id),size,updated_at:chrono::Utc::now().timestamp()};
    queue_pending_op(db,"cleanup_delete",&identity,None)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn queue_remote_cleanup(_db:&DbConnection,_file_id:i64,_chat_key:i64,_message_id:i32,_size:i64)->Result<(),String>{Ok(())}

#[cfg(target_os = "windows")]
fn queue_sync_refresh(db:&DbConnection,file_id:i64,fingerprint:&str)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let mut update=conn.prepare("UPDATE tcloud_files SET local_fingerprint=?1,sync_status='pending_upload',local_status='local',updated_at=?2 WHERE id=?3").map_err(|e|e.to_string())?;
    update.bind((1,fingerprint)).map_err(|e|e.to_string())?;update.bind((2,now)).map_err(|e|e.to_string())?;update.bind((3,file_id)).map_err(|e|e.to_string())?;update.next().map_err(|e|e.to_string())?;
    let mut existing=conn.prepare("SELECT 1 FROM tcloud_sync_jobs WHERE file_id=?1 AND operation IN ('upload','refresh') AND status IN ('pending','running') LIMIT 1").map_err(|e|e.to_string())?;
    existing.bind((1,file_id)).map_err(|e|e.to_string())?;
    if matches!(existing.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(());}
    let mut job=conn.prepare("INSERT INTO tcloud_sync_jobs(file_id,operation,status,created_at,updated_at,priority,attempts,next_attempt_at) VALUES(?1,'refresh','pending',?2,?2,115,0,NULL)").map_err(|e|e.to_string())?;
    job.bind((1,file_id)).map_err(|e|e.to_string())?;job.bind((2,now)).map_err(|e|e.to_string())?;job.next().map_err(|e|e.to_string())?;
    drop(job);
    drop(existing);
    drop(update);
    drop(conn);
    crate::sync::scheduler::signal_sync_work();
    Ok(())
}

#[cfg(target_os = "windows")]
async fn stable_local_fingerprint(path:&Path)->Result<String,String>{
    let mut last:Option<String>=None;let mut stable=0u8;
    loop{
        let current=local_path_fingerprint(path).ok_or_else(||format!("Arquivo local não está disponível para sincronização: {}",path.display()))?;
        if last.as_deref()==Some(current.as_str()){stable=stable.saturating_add(1);if stable>=3{return Ok(current);}}else{last=Some(current);stable=0;}
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

#[cfg(target_os = "windows")]
async fn service_delete_completion(app:AppHandle,identity_bytes:Vec<u8>)->Result<(),String>{
    let identity:CfapiFileIdentity=serde_json::from_slice(&identity_bytes).map_err(|e|format!("Identidade de delete inválida: {e}"))?;
    let db=app.try_state::<DbConnection>().ok_or_else(||"Banco TCloud indisponível.".to_string())?.inner().clone();
    let (remote_apply,relative_path)={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let mut q=conn.prepare(
            "SELECT f.sync_status,c.relative_path FROM tcloud_files f LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id WHERE f.id=?1 LIMIT 1"
        ).map_err(|e|e.to_string())?;
        q.bind((1,identity.file_id)).map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
            let status=q.read::<String,_>(0).unwrap_or_default();
            (matches!(status.as_str(),"remote_deleted"|"applying_remote_delete"),q.read::<Option<String>,_>(1).unwrap_or(None))
        }else{(false,None)}
    };
    if !remote_apply{
        // Atomic-save protection: Office/editors often implement Save as
        // delete(old) + create(new) at the same path. Wait briefly before moving
        // the cloud identity to Trash. If materialized bytes reappear, keep the
        // immutable remote identity and queue a replacement upload instead.
        if let (Some(relative),Ok(root))=(relative_path.as_deref(),cfapi_root()){
            let local_path=root.join(relative);
            for _ in 0..4{
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                if crate::sync::watcher_runtime::is_materialized_user_file(&local_path){
                    crate::sync::watcher_runtime::queue_local_file_now(&db,&root,&local_path)?;
                    log::info!("TCloud CFAPI: delete + recriação detectados em {}; tratado como substituição segura.",local_path.display());
                    return Ok(());
                }
            }
        }
        // A delayed delete callback can race a rename/move callback for the same
        // immutable identity. Re-read the current relative path after the grace
        // window; if it changed, this delete belongs to the obsolete pathname.
        if let Some(original_relative)=relative_path.as_deref(){
            let current_relative={
                let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                let mut q=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;
                q.bind((1,identity.file_id)).map_err(|e|e.to_string())?;
                if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<String,_>(0).ok()}else{None}
            };
            if current_relative.as_deref().is_some_and(|current|!current.eq_ignore_ascii_case(original_relative)){
                log::info!("TCloud CFAPI: callback de delete antigo ignorado; identidade {} agora está em {:?}.",identity.file_id,current_relative);
                return Ok(());
            }
        }
        // SAFETY:
        // A file-level CFAPI delete can be emitted while an entire managed
        // directory temporarily disappears during namespace refresh, rename
        // or placeholder reconciliation.
        //
        // A genuine single-file delete normally leaves its parent directory
        // intact. If the parent disappeared too, treat this callback as
        // ambiguous namespace churn instead of destructive user intent.
        if let (Some(relative), Ok(root)) = (relative_path.as_deref(), cfapi_root()) {
            let local_path = root.join(relative);

            if let Some(parent) = local_path.parent() {
                if !parent.exists() {
                    log::warn!(
                        "TCloud CFAPI: delete de arquivo {} ignorado porque a pasta pai também desapareceu: {}",
                        identity.file_id,
                        parent.display()
                    );
                    return Ok(());
                }
            }
        }

        // TCloud 12.5+: Explorer delete is non-destructive. Keep the Telegram
        // message intact and move only the local projection into TCloud Trash.
        crate::commands::tcloud_sync_maturity::move_file_to_trash(&db,identity.file_id)?;
        return Ok(());
    }
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let mut up=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
        up.bind((1,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;up.bind((2,identity.file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
        let mut del=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;
        del.bind((1,identity.file_id)).map_err(|e|e.to_string())?;del.next().map_err(|e|e.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn service_rename_completion(app:AppHandle,identity_bytes:Vec<u8>,_source:Option<String>,target:Option<String>)->Result<(),String>{
    let identity:CfapiFileIdentity=serde_json::from_slice(&identity_bytes).map_err(|e|format!("Identidade de rename inválida: {e}"))?;
    let target=target.ok_or_else(||"Windows não forneceu caminho final do rename.".to_string())?;
    let root=cfapi_root()?;
    let relative=callback_target_relative(&root,&target).ok_or_else(||format!("Caminho final fora do Sync Root: {target}"))?;
    let new_name=Path::new(&relative).file_name().and_then(|v|v.to_str()).ok_or_else(||"Novo nome CFAPI inválido.".to_string())?.to_string();
    let db=app.try_state::<DbConnection>().ok_or_else(||"Banco TCloud indisponível.".to_string())?.inner().clone();
    let target_chat_key={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        chat_key_for_relative_path(&conn,Path::new(&relative))?
    };
    let remote_apply={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let mut q=conn.prepare("SELECT sync_status FROM tcloud_files WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,identity.file_id)).map_err(|e|e.to_string())?;
        matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row)
            && q.read::<String,_>(0).unwrap_or_default().starts_with("applying_remote_")
    };
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let now=chrono::Utc::now().timestamp();
        let mut up=conn.prepare("UPDATE tcloud_cfapi_items SET relative_path=?1,updated_at=?2 WHERE file_id=?3").map_err(|e|e.to_string())?;
        up.bind((1,relative.as_str())).map_err(|e|e.to_string())?;up.bind((2,now)).map_err(|e|e.to_string())?;up.bind((3,identity.file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
        // Keep the committed Telegram source while a cross-destination move is
        // pending. Changing telegram_chat_id before the replacement message exists
        // would make the next remote listing rediscover the source as a duplicate.
        let committed_chat_key=if remote_apply || target_chat_key==identity.chat_key{target_chat_key}else{identity.chat_key};
        let mut file=conn.prepare("UPDATE tcloud_files SET name=?1,telegram_chat_id=?2,sync_status=?3,updated_at=?4 WHERE id=?5").map_err(|e|e.to_string())?;
        file.bind((1,new_name.as_str())).map_err(|e|e.to_string())?;file.bind((2,committed_chat_key)).map_err(|e|e.to_string())?;
        file.bind((3,if remote_apply{"synced"}else{"pending_move"})).map_err(|e|e.to_string())?;
        file.bind((4,now)).map_err(|e|e.to_string())?;file.bind((5,identity.file_id)).map_err(|e|e.to_string())?;file.next().map_err(|e|e.to_string())?;
    }
    if remote_apply{return Ok(());}
    let target_local_path=root.join(&relative);
    let baseline_fingerprint=local_path_fingerprint(&target_local_path);
    if target_chat_key==identity.chat_key{
        // A rename inside the same Telegram destination only needs to update the
        // message caption. cmd_get_files already treats the caption as the display
        // filename, so avoid downloading/reuploading the entire object.
        let payload=serde_json::json!({"new_name":new_name,"target_relative":relative,"baseline_fingerprint":baseline_fingerprint}).to_string();
        queue_pending_op(&db,"rename",&identity,Some(&payload))?;
    }else{
        // Cross-folder move is create-new-then-delete-old, durable and resumable.
        let payload=serde_json::json!({"new_name":new_name,"target_chat_key":target_chat_key,"target_relative":relative,"baseline_fingerprint":baseline_fingerprint}).to_string();
        queue_pending_op(&db,"relocate",&identity,Some(&payload))?;
    }
    let _=process_pending_ops_once(&app).await;
    Ok(())
}

#[cfg(target_os = "windows")]
async fn resolve_identity_peer(client:&grammers_client::Client,telegram:&crate::commands::TelegramState,db:&DbConnection,chat_key:i64)->Result<grammers_client::types::Peer,String>{
    if chat_key<0{
        let target=crate::commands::tcloud_forums::topic_target(db,Some(chat_key))?.ok_or_else(||format!("Tópico TCloud {chat_key} não encontrado."))?;
        let peer_ref:grammers_session::types::PeerRef=grammers_tl_types::types::InputPeerChannel{channel_id:target.chat_id,access_hash:target.access_hash}.into();
        let peer=client.resolve_peer(peer_ref).await.map_err(|e|format!("Não foi possível abrir o fórum '{}' para operação CFAPI: {e}",target.name))?;
        {
            let mut cache=telegram.peer_cache.write().await;
            cache.insert(target.chat_id,peer.clone());
            cache.insert(chat_key,peer.clone());
        }
        Ok(peer)
    }else{
        let folder_id=if chat_key==0{None}else{Some(chat_key)};
        crate::commands::utils::resolve_peer(client,folder_id,&telegram.peer_cache).await
    }
}

#[cfg(target_os = "windows")]
async fn process_pending_ops_once(app:&AppHandle)->Result<(),String>{
    #[derive(Clone)] struct Pending{id:i64,op:String,file_id:i64,chat_key:i64,message_id:i64,payload:Option<String>}
    let db=app.try_state::<DbConnection>().ok_or_else(||"Banco TCloud indisponível.".to_string())?.inner().clone();
    let telegram=app.try_state::<crate::commands::TelegramState>().ok_or_else(||"Estado Telegram indisponível.".to_string())?.inner().clone();
    let client={telegram.client.lock().await.clone()};
    let Some(client)=client else{return Ok(());};
    let pending={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let now=chrono::Utc::now().timestamp();
        let mut q=conn.prepare(
            "SELECT id,op_type,file_id,chat_key,message_id,payload_json
             FROM tcloud_cfapi_pending_ops
             WHERE completed_at IS NULL AND attempts < 12
               AND (last_attempt_at IS NULL OR last_attempt_at <= ?1 - CASE
                    WHEN attempts <= 1 THEN 20
                    WHEN attempts = 2 THEN 60
                    WHEN attempts = 3 THEN 180
                    WHEN attempts = 4 THEN 600
                    ELSE 1800 END)
             ORDER BY created_at LIMIT 20"
        ).map_err(|e|e.to_string())?;
        q.bind((1,now)).map_err(|e|e.to_string())?;
        let mut rows=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            rows.push(Pending{id:q.read(0).map_err(|e|e.to_string())?,op:q.read(1).map_err(|e|e.to_string())?,file_id:q.read(2).map_err(|e|e.to_string())?,chat_key:q.read(3).map_err(|e|e.to_string())?,message_id:q.read(4).map_err(|e|e.to_string())?,payload:q.read::<String,_>(5).ok()});
        }
        rows
    };
    for item in pending{
        // The queue snapshot can race with a newer Explorer action. Re-check the
        // row before touching Telegram so a superseded rename/move is skipped.
        let still_active={
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let mut q=conn.prepare("SELECT completed_at FROM tcloud_cfapi_pending_ops WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
            q.bind((1,item.id)).map_err(|e|e.to_string())?;
            matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row) && q.read::<Option<i64>,_>(0).unwrap_or(None).is_none()
        };
        if !still_active{continue;}
        let result:Result<(),String>=async{
            let peer=resolve_identity_peer(&client,&telegram,&db,item.chat_key).await?;
            let message_id=i32::try_from(item.message_id).map_err(|_|"ID Telegram inválido na fila CFAPI.".to_string())?;
            match item.op.as_str(){
                "delete"=>{client.delete_messages(&peer,&[message_id]).await.map_err(|e|e.to_string())?;}
                "cleanup_delete"=>{client.delete_messages(&peer,&[message_id]).await.map_err(|e|e.to_string())?;}
                "relocate"=>{
                    let payload=item.payload.as_deref().ok_or_else(||"Move/rename pendente sem payload.".to_string())?;
                    let mut value:serde_json::Value=serde_json::from_str(payload).map_err(|e|e.to_string())?;
                    let new_name=value.get("new_name").and_then(|v|v.as_str()).ok_or_else(||"Move/rename sem nome final.".to_string())?.to_string();
                    let target_chat_key=value.get("target_chat_key").and_then(|v|v.as_i64()).ok_or_else(||"Move/rename sem destino remoto.".to_string())?;
                    let target_relative=value.get("target_relative").and_then(|v|v.as_str()).ok_or_else(||"Move/rename sem caminho final.".to_string())?.to_string();
                    let root=cfapi_root()?;
                    let local_path=root.join(&target_relative);

                    // Part 7: a cross-destination move is checkpointed immediately after
                    // the replacement Telegram message exists. If Windows closes, TCloud
                    // updates or the process crashes between "send new" and "delete old",
                    // the next startup resumes from this message id instead of uploading a
                    // second copy to the destination.
                    let checkpoint_mid=value.get("replacement_message_id").and_then(|v|v.as_i64()).filter(|v|*v>0);
                    let (sent_id,size,uploaded_fingerprint)=if let Some(mid)=checkpoint_mid{
                        let sent_id=i32::try_from(mid).map_err(|_|"Checkpoint de movimento possui ID Telegram inválido.".to_string())?;
                        let size=value.get("replacement_size").and_then(|v|v.as_u64())
                            .or_else(||std::fs::metadata(&local_path).ok().map(|m|m.len()))
                            .unwrap_or(identity_size_for_file(&db,item.file_id).unwrap_or(0).max(0) as u64);
                        let fingerprint=value.get("replacement_fingerprint").and_then(|v|v.as_str()).map(str::to_string)
                            .or_else(||local_path_fingerprint(&local_path)).unwrap_or_default();
                        log::info!("TCloud move: retomando checkpoint remoto {} para arquivo {}.",sent_id,item.file_id);
                        (sent_id,size,fingerprint)
                    }else{
                        if !local_path.exists(){return Err(format!("Arquivo movido não está mais em {}",local_path.display()));}
                        suppress_watcher_path(&local_path,30);
                        let hydrate_path=local_path.clone();
                        tokio::task::spawn_blocking(move||platform::hydrate_item(&hydrate_path)).await.map_err(|e|e.to_string())??;
                        let uploaded_fingerprint=stable_local_fingerprint(&local_path).await?;
                        let size=tokio::fs::metadata(&local_path).await.map_err(|e|e.to_string())?.len();
                        let mut file=tokio::fs::File::open(&local_path).await.map_err(|e|e.to_string())?;
                        let uploaded=client.upload_stream(&mut file,size as usize,new_name.clone()).await.map_err(|e|format!("Upload do move/rename falhou: {e}"))?;
                        let target_peer=resolve_identity_peer(&client,&telegram,&db,target_chat_key).await?;
                        let mut outgoing=grammers_client::InputMessage::new().text("").file(uploaded);
                        if target_chat_key<0{
                            let topic=crate::commands::tcloud_forums::topic_target(&db,Some(target_chat_key))?.ok_or_else(||format!("Tópico {target_chat_key} não encontrado."))?;
                            outgoing=outgoing.reply_to(Some(topic.topic_id));
                        }
                        let sent=client.send_message(&target_peer,outgoing).await.map_err(|e|format!("Publicação no destino falhou: {e}"))?;
                        let sent_id=sent.id();

                        if let Some(object)=value.as_object_mut(){
                            object.insert("replacement_message_id".into(),serde_json::json!(sent_id));
                            object.insert("replacement_size".into(),serde_json::json!(size));
                            object.insert("replacement_fingerprint".into(),serde_json::json!(uploaded_fingerprint.clone()));
                            object.insert("replacement_created_at".into(),serde_json::json!(chrono::Utc::now().timestamp()));
                        }
                        let checkpoint_json=serde_json::to_string(&value).map_err(|e|e.to_string())?;
                        {
                            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                            let mut checkpoint=conn.prepare(
                                "UPDATE tcloud_cfapi_pending_ops SET payload_json=?1 WHERE id=?2 AND completed_at IS NULL"
                            ).map_err(|e|e.to_string())?;
                            checkpoint.bind((1,checkpoint_json.as_str())).map_err(|e|e.to_string())?;
                            checkpoint.bind((2,item.id)).map_err(|e|e.to_string())?;
                            checkpoint.next().map_err(|e|e.to_string())?;
                        }
                        (sent_id,size,uploaded_fingerprint)
                    };

                    if let Err(error)=client.delete_messages(&peer,&[message_id]).await{
                        log::warn!("TCloud move: novo objeto {} criado, mas o antigo {} será limpo depois: {}",sent_id,message_id,error);
                        let cleanup=CfapiFileIdentity{version:1,file_id:item.file_id,chat_key:item.chat_key,message_id:i64::from(message_id),size:size as i64,updated_at:chrono::Utc::now().timestamp()};
                        let _=queue_pending_op(&db,"cleanup_delete",&cleanup,None);
                    }
                    let updated_at=chrono::Utc::now().timestamp();
                    let new_identity=CfapiFileIdentity{version:1,file_id:item.file_id,chat_key:target_chat_key,message_id:i64::from(sent_id),size:size as i64,updated_at};
                    let new_identity_json=serde_json::to_string(&new_identity).map_err(|e|e.to_string())?;
                    {
                        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                        let mut f=conn.prepare("UPDATE tcloud_files SET name=?1,size=?2,telegram_chat_id=?3,telegram_message_id=?4,sync_status='synced',local_status='local',local_fingerprint=?5,updated_at=?6 WHERE id=?7").map_err(|e|e.to_string())?;
                        f.bind((1,new_name.as_str())).map_err(|e|e.to_string())?;f.bind((2,size as i64)).map_err(|e|e.to_string())?;f.bind((3,target_chat_key)).map_err(|e|e.to_string())?;f.bind((4,i64::from(sent_id))).map_err(|e|e.to_string())?;f.bind((5,uploaded_fingerprint.as_str())).map_err(|e|e.to_string())?;f.bind((6,updated_at)).map_err(|e|e.to_string())?;f.bind((7,item.file_id)).map_err(|e|e.to_string())?;f.next().map_err(|e|e.to_string())?;
                        let mut c=conn.prepare("UPDATE tcloud_cfapi_items SET identity_json=?1,relative_path=?2,updated_at=?3 WHERE file_id=?4").map_err(|e|e.to_string())?;
                        c.bind((1,new_identity_json.as_str())).map_err(|e|e.to_string())?;c.bind((2,target_relative.as_str())).map_err(|e|e.to_string())?;c.bind((3,updated_at)).map_err(|e|e.to_string())?;c.bind((4,item.file_id)).map_err(|e|e.to_string())?;c.next().map_err(|e|e.to_string())?;
                        // If another rename/move was queued while this relocation was
                        // already in flight, rebase that newer operation onto the new
                        // Telegram identity. Without this it would retry the deleted
                        // source message and eventually appear as a stuck sync error.
                        let mut rebase=conn.prepare(
                            "UPDATE tcloud_cfapi_pending_ops SET chat_key=?1,message_id=?2
                             WHERE file_id=?3 AND id<>?4 AND completed_at IS NULL AND op_type IN ('rename','relocate')"
                        ).map_err(|e|e.to_string())?;
                        rebase.bind((1,target_chat_key)).map_err(|e|e.to_string())?;
                        rebase.bind((2,i64::from(sent_id))).map_err(|e|e.to_string())?;
                        rebase.bind((3,item.file_id)).map_err(|e|e.to_string())?;
                        rebase.bind((4,item.id)).map_err(|e|e.to_string())?;
                        rebase.next().map_err(|e|e.to_string())?;
                    }
                    if local_path.exists(){
                        suppress_watcher_path(&local_path,15);
                        platform::update_placeholder(&local_path,size as i64,updated_at,new_identity_json.as_bytes())?;
                        let current_fingerprint=local_path_fingerprint(&local_path);
                        if current_fingerprint.as_deref()!=Some(uploaded_fingerprint.as_str()){
                            if let Some(current)=current_fingerprint{queue_sync_refresh(&db,item.file_id,&current)?;}
                        }else{
                            let _=platform::mark_item_in_sync(&local_path);
                        }
                    }
                }
                "rename"=>{
                    use grammers_client::types::Peer;
                    use grammers_tl_types as tl;
                    let payload=item.payload.as_deref().ok_or_else(||"Rename pendente sem payload.".to_string())?;
                    let value:serde_json::Value=serde_json::from_str(payload).map_err(|e|e.to_string())?;
                    let new_name=value.get("new_name").and_then(|v|v.as_str()).ok_or_else(||"Rename pendente sem nome.".to_string())?;
                    let target_relative=value.get("target_relative").and_then(|v|v.as_str()).map(str::to_string);
                    let baseline=value.get("baseline_fingerprint").and_then(|v|v.as_str()).map(str::to_string);
                    let input_peer=match &peer{
                        Peer::User(u)=>{let(id,access_hash)=match &u.raw{tl::enums::User::User(usr)=>(usr.id,usr.access_hash.unwrap_or(0)),tl::enums::User::Empty(usr)=>(usr.id,0)};tl::enums::InputPeer::User(tl::types::InputPeerUser{user_id:id,access_hash})}
                        Peer::Channel(c)=>tl::enums::InputPeer::Channel(tl::types::InputPeerChannel{channel_id:c.raw.id,access_hash:c.raw.access_hash.ok_or("No access hash for channel")?}),
                        _=>return Err("Tipo de peer não suportado para rename CFAPI.".into()),
                    };
                    client.invoke(&tl::functions::messages::EditMessage{peer:input_peer,id:message_id,no_webpage:false,invert_media:false,message:Some(new_name.to_string()),media:None,reply_markup:None,entities:None,schedule_date:None,quick_reply_shortcut_id:None,schedule_repeat_period:None}).await.map_err(|e|format!("Rename Telegram falhou: {e}"))?;
                    let relative=if target_relative.is_some(){target_relative}else{
                        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                        let mut q=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;
                        q.bind((1,item.file_id)).map_err(|e|e.to_string())?;
                        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<String,_>(0).ok()}else{None}
                    };
                    if let Some(relative)=relative{
                        if let Ok(root)=cfapi_root(){
                            let path=root.join(relative);suppress_watcher_path(&path,3);
                            let current=local_path_fingerprint(&path);
                            if current.is_some() && current!=baseline{queue_sync_refresh(&db,item.file_id,current.as_deref().unwrap_or_default())?;}
                            else{let _=platform::mark_item_in_sync(&path);}
                        }
                    }
                }
                _=>return Err(format!("Operação CFAPI desconhecida: {}",item.op)),
            }
            Ok(())
        }.await;
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        match result{
            Ok(())=>{
                let now=chrono::Utc::now().timestamp();let mut done=conn.prepare("UPDATE tcloud_cfapi_pending_ops SET completed_at=?1,last_error=NULL WHERE id=?2").map_err(|e|e.to_string())?;done.bind((1,now)).map_err(|e|e.to_string())?;done.bind((2,item.id)).map_err(|e|e.to_string())?;done.next().map_err(|e|e.to_string())?;
                if item.op=="delete"{let mut up=conn.prepare("UPDATE tcloud_files SET sync_status='trashed' WHERE id=?1").map_err(|e|e.to_string())?;up.bind((1,item.file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;let folder_key=if item.chat_key==0{"home".to_string()}else{item.chat_key.to_string()};let mut enc=conn.prepare("DELETE FROM encrypted_files WHERE folder_key=?1 AND message_id=?2").map_err(|e|e.to_string())?;enc.bind((1,folder_key.as_str())).map_err(|e|e.to_string())?;enc.bind((2,item.message_id)).map_err(|e|e.to_string())?;enc.next().map_err(|e|e.to_string())?;}
                if item.op=="rename"{let mut up=conn.prepare("UPDATE tcloud_files SET sync_status=CASE WHEN sync_status='pending_upload' THEN sync_status ELSE 'synced' END,updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;up.bind((1,now)).map_err(|e|e.to_string())?;up.bind((2,item.file_id)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;}
            }
            Err(error)=>{
                let now=chrono::Utc::now().timestamp();
                let mut fail=conn.prepare("UPDATE tcloud_cfapi_pending_ops SET attempts=attempts+1,last_attempt_at=?1,last_error=?2 WHERE id=?3").map_err(|e|e.to_string())?;
                fail.bind((1,now)).map_err(|e|e.to_string())?;
                fail.bind((2,error.as_str())).map_err(|e|e.to_string())?;
                fail.bind((3,item.id)).map_err(|e|e.to_string())?;
                fail.next().map_err(|e|e.to_string())?;
                if matches!(item.op.as_str(),"rename"|"relocate"){
                    let exhausted={
                        let mut q=conn.prepare("SELECT attempts FROM tcloud_cfapi_pending_ops WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
                        q.bind((1,item.id)).map_err(|e|e.to_string())?;
                        matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row) && q.read::<i64,_>(0).unwrap_or(0)>=12
                    };
                    if exhausted{
                        let mut mark=conn.prepare("UPDATE tcloud_files SET sync_status='failed_cfapi',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
                        mark.bind((1,now)).map_err(|e|e.to_string())?;
                        mark.bind((2,item.file_id)).map_err(|e|e.to_string())?;
                        mark.next().map_err(|e|e.to_string())?;
                        log::warn!("Operação CFAPI {} atingiu o limite automático e agora precisa de atenção do usuário: {}",item.id,error);
                    }else{
                        log::warn!("Operação CFAPI pendente {} será tentada novamente com backoff: {}",item.id,error);
                    }
                }else{
                    log::warn!("Operação CFAPI pendente {} será tentada novamente com backoff: {}",item.id,error);
                }
            }
        }
    }
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let cutoff=chrono::Utc::now().timestamp().saturating_sub(7*24*60*60);
        if let Ok(mut prune)=conn.prepare("DELETE FROM tcloud_cfapi_pending_ops WHERE completed_at IS NOT NULL AND completed_at < ?1") {
            let _=prune.bind((1,cutoff));
            let _=prune.next();
        };
    }
    Ok(())
}


#[cfg(target_os = "windows")]
fn cleanup_cfapi_temp(app:&AppHandle){if let Ok(dir)=app.path().app_data_dir(){let path=dir.join("cfapi").join("hydration-temp");if let Ok(entries)=std::fs::read_dir(path){for e in entries.flatten(){let _=std::fs::remove_file(e.path());}}}}

#[cfg(target_os = "windows")]
fn start_pending_worker(app:AppHandle){
    use std::sync::{OnceLock,atomic::{AtomicBool,Ordering}};static STARTED:OnceLock<AtomicBool>=OnceLock::new();let started=STARTED.get_or_init(||AtomicBool::new(false));if started.swap(true,Ordering::SeqCst){return;}
    tauri::async_runtime::spawn(async move{
        loop{
            let _=process_pending_ops_once(&app).await;
            if let Some(db)=app.try_state::<DbConnection>(){
                // Conflitos aguardam decis?o expl?cita do usu?rio.
                // Nenhuma resolu??o keep_both autom?tica pelo CFAPI.
            }
            // New Explorer transactions wake this loop immediately. The timeout
            // remains as a retry safety-net for operations under backoff.
            tokio::select!{
                _=cfapi_pending_notify().notified()=>{},
                _=tokio::time::sleep(std::time::Duration::from_secs(20))=>{},
            }
        }
    });
}

/// Keeps the Windows cloud provider alive without requiring the settings screen to
/// be opened. Startup can race with Explorer/Cloud Files services after logon, so a
/// failed first connection is retried silently with bounded backoff.
#[cfg(target_os = "windows")]
fn start_provider_health_worker(app:AppHandle){
    use std::sync::{OnceLock,atomic::{AtomicBool,Ordering}};
    static STARTED:OnceLock<AtomicBool>=OnceLock::new();
    let started=STARTED.get_or_init(||AtomicBool::new(false));
    if started.swap(true,Ordering::SeqCst){return;}
    tauri::async_runtime::spawn(async move{
        let mut failures:u32=0;
        // Give Explorer and Cloud Files services time to settle after Windows logon.
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        loop{
            let marker=match marker_path(&app){Ok(value)=>value,Err(_)=>{
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }};
            if !marker.exists(){
                failures=0;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
            let root=match cfapi_root(){Ok(value)=>value,Err(_)=>{
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }};
            if platform::platform_info().is_none(){
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
            if platform::provider_connected(){
                failures=0;
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                continue;
            }
            match ensure_provider_connection(&app,&root,true){
                Ok(repaired)=>{
                    failures=0;
                    log::info!("CFAPI watchdog: provider reconectado automaticamente em {}.",root.display());
                    if repaired{
                        if let Some(db)=app.try_state::<DbConnection>(){
                            if let Ok(conn)=db.lock(){
                                match populate_inner(&app,&conn,100_000){
                                    Ok(result)=>log::info!(
                                        "CFAPI watchdog: namespace reparado ({} criados, {} existentes, {} ignorados).",
                                        result.created,result.existing,result.skipped
                                    ),
                                    Err(error)=>log::warn!("CFAPI watchdog: reconectado, mas o namespace será reparado depois: {}",error),
                                }
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
                Err(error)=>{
                    failures=failures.saturating_add(1);
                    let delay=(10u64.saturating_mul(1u64 << failures.min(3))).min(120);
                    log::warn!("CFAPI watchdog: reconexão adiada (tentativa {}): {}",failures,error);
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                }
            }
        }
    });
}

/// Explorer's native "Always keep on this device" verb changes the PINNED
/// attribute. The sync provider must notice that intent and ensure the file is
/// fully present. Polling the small local namespace is intentionally used here
/// instead of shell hooks so native and fallback verbs share the same behavior.
#[cfg(target_os = "windows")]
fn sync_local_availability_snapshot(app:&AppHandle,root:&Path,updates:&[(PathBuf,&'static str,bool)]){
    if updates.is_empty(){return;}
    let Some(db)=app.try_state::<DbConnection>() else{return;};
    let Ok(conn)=db.lock() else{return;};
    for (path,status,pinned) in updates{
        let Ok(relative)=path.strip_prefix(root) else{continue;};
        let relative=relative.to_string_lossy().replace('\\',"/");
        if relative.is_empty(){continue;}
        let Ok(mut lookup)=conn.prepare("SELECT file_id FROM tcloud_cfapi_items WHERE relative_path=?1 LIMIT 1") else{continue;};
        if lookup.bind((1,relative.as_str())).is_err(){continue;}
        if !matches!(lookup.next(),Ok(sqlite::State::Row)){continue;}
        let Ok(file_id)=lookup.read::<i64,_>(0) else{continue;};
        drop(lookup);
        let Ok(mut update)=conn.prepare("UPDATE tcloud_files SET local_status=?1 WHERE id=?2") else{continue;};
        if update.bind((1,*status)).is_err(){continue;}
        if update.bind((2,file_id)).is_err(){continue;}
        let _=update.next();
        if *pinned{
            if let Ok(mut pin)=conn.prepare("INSERT OR REPLACE INTO tcloud_offline_pins(file_id,pinned_at) VALUES(?1,?2)"){
                let _=pin.bind((1,file_id));
                let _=pin.bind((2,chrono::Utc::now().timestamp()));
                let _=pin.next();
            }
        }else if let Ok(mut pin)=conn.prepare("DELETE FROM tcloud_offline_pins WHERE file_id=?1"){
            let _=pin.bind((1,file_id));
            let _=pin.next();
        }
    }
}

#[cfg(target_os = "windows")]
fn start_pin_hydration_worker(app:AppHandle,root:PathBuf){
    use std::collections::HashMap;
    use std::sync::{OnceLock,atomic::{AtomicBool,Ordering}};
    static STARTED:OnceLock<AtomicBool>=OnceLock::new();
    let started=STARTED.get_or_init(||AtomicBool::new(false));
    if started.swap(true,Ordering::SeqCst){return;}
    tauri::async_runtime::spawn(async move{
        let mut seen:HashMap<PathBuf,u32>=HashMap::new();
        let mut baseline_ready=false;
        let mut quiet_passes:u8=0;
        loop{
            let scan_root=root.clone();
            let entries=tokio::task::spawn_blocking(move||{
                use std::os::windows::fs::MetadataExt;
                let mut result=Vec::new();
                if !scan_root.exists(){return result;}
                for entry in walkdir::WalkDir::new(&scan_root).follow_links(false).into_iter().filter_map(Result::ok){
                    if !entry.file_type().is_file(){continue;}
                    let Ok(meta)=entry.metadata() else{continue;};
                    result.push((entry.path().to_path_buf(),meta.file_attributes()));
                }
                result
            }).await.unwrap_or_default();

            const FILE_ATTRIBUTE_OFFLINE:u32=0x0000_1000;
            const FILE_ATTRIBUTE_PINNED:u32=0x0008_0000;
            const FILE_ATTRIBUTE_UNPINNED:u32=0x0010_0000;
            const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS:u32=0x0040_0000;
            let mut repaired_this_pass=0usize;
            let mut status_updates:Vec<(PathBuf,&'static str,bool)>=Vec::new();
            if !baseline_ready{
                // Build the UI/local database state from Windows itself after restart.
                // This prevents a file pinned from Explorer from showing as cloud-only in TCloud.
                for (path,attrs) in entries{
                    seen.insert(path.clone(),attrs);
                    let pinned=attrs & FILE_ATTRIBUTE_PINNED != 0;
                    let needs_data=attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS|FILE_ATTRIBUTE_OFFLINE) != 0;
                    if pinned && needs_data{
                        match platform::hydrate_item(&path){
                            Ok(())=>{
                                status_updates.push((path.clone(),"cached",true));
                                log::info!("CFAPI startup: hidratação offline retomada {}",path.display());
                            },
                            Err(error)=>{
                                status_updates.push((path.clone(),"cloud_only",true));
                                log::debug!("CFAPI startup: hidratação adiada {}: {}",path.display(),error);
                            },
                        }
                    }else{
                        status_updates.push((path.clone(),if needs_data{"cloud_only"}else{"cached"},pinned));
                    }
                }
                sync_local_availability_snapshot(&app,&root,&status_updates);
                baseline_ready=true;
                tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                continue;
            }
            for (path,attrs) in entries{
                if seen.get(&path).copied()==Some(attrs){continue;}
                if repaired_this_pass>=64{continue;}
                let pinned=attrs & FILE_ATTRIBUTE_PINNED != 0;
                let unpinned=attrs & FILE_ATTRIBUTE_UNPINNED != 0;
                let needs_data=attrs & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS|FILE_ATTRIBUTE_OFFLINE) != 0;
                let action=if pinned{
                    platform::hydrate_item(&path)
                }else if unpinned && !needs_data{
                    platform::dehydrate_item(&path)
                }else{
                    platform::mark_item_in_sync(&path)
                };
                match action{
                    Ok(())=>{
                        seen.insert(path.clone(),attrs);
                        repaired_this_pass+=1;
                        let local_status=if pinned{"cached"}else if unpinned{"cloud_only"}else if needs_data{"cloud_only"}else{"cached"};
                        status_updates.push((path.clone(),local_status,pinned));
                        log::info!("CFAPI state repair: placeholder estabilizado {}",path.display());
                    },
                    Err(error)=>log::debug!("CFAPI state repair ignorou {}: {}",path.display(),error),
                }
            }
            let changed=!status_updates.is_empty();
            sync_local_availability_snapshot(&app,&root,&status_updates);
            if changed{quiet_passes=0;}else{quiet_passes=quiet_passes.saturating_add(1).min(12);}
            let window_visible=app.get_webview_window("main")
                .and_then(|window|window.is_visible().ok())
                .unwrap_or(false);
            // Full attribute scans are useful after a pin/offline change, but they
            // are expensive on large roots. Back off progressively once Windows
            // reports a stable namespace, with a moderate cap so native Explorer
            // pin changes are still noticed promptly.
            let delay=if quiet_passes>=7{
                if window_visible{20}else{30}
            }else if quiet_passes>=3{
                if window_visible{10}else{18}
            }else if window_visible{
                4
            }else{
                8
            };
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }
    });
}

fn integrity_check_inner(app:&AppHandle,conn:&sqlite::Connection,repair:bool)->Result<CfapiIntegrityResult,String>{
    ensure_schema(conn)?;
    let root=cfapi_root()?;
    let marker=marker_path(app)?;
    if !marker.exists(){
        return Err("Conecte este computador ao TCloud antes de verificar os arquivos do Explorer.".into());
    }

    if repair {
        if let Err(error)=reconcile_managed_folder_namespace(app,conn){
            log::warn!("TCloud Explorer: reparo de pastas adiado durante verificação: {}",error);
        }
    }

    let mut rows=Vec::<(i64,String,Option<String>)>::new();
    let mut query=conn.prepare(
        "SELECT c.file_id,c.relative_path,f.sync_status FROM tcloud_cfapi_items c LEFT JOIN tcloud_files f ON f.id=c.file_id"
    ).map_err(|e|e.to_string())?;
    while let sqlite::State::Row=query.next().map_err(|e|e.to_string())?{
        rows.push((
            query.read::<i64,_>(0).unwrap_or(0),
            query.read::<String,_>(1).unwrap_or_default(),
            query.read::<Option<String>,_>(2).unwrap_or(None),
        ));
    }
    drop(query);

    let mut missing=Vec::<i64>::new();
    let mut stale=Vec::<i64>::new();
    for (file_id,relative,status) in &rows{
        let inactive=status.as_deref().map(|value|matches!(value,"deleted"|"trashed"|"remote_deleted"|"applying_remote_delete")).unwrap_or(true);
        if inactive{
            stale.push(*file_id);
        }else if !root.join(relative.replace('/',"\\")).exists(){
            missing.push(*file_id);
        }
    }

    let mut repaired=0i64;
    let mut errors=Vec::new();
    if repair{
        for file_id in &stale{
            match conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1") {
                Ok(mut stmt)=>{
                    if stmt.bind((1,*file_id)).is_ok() && stmt.next().is_ok(){repaired+=1;}
                },
                Err(error)=>errors.push(error.to_string()),
            }
            if let Ok(mut pin)=conn.prepare("DELETE FROM tcloud_offline_pins WHERE file_id=?1"){
                let _=pin.bind((1,*file_id));
                let _=pin.next();
            }
        }
        if !missing.is_empty(){
            match populate_inner(app,conn,100_000){
                Ok(result)=>{
                    repaired+=result.created+result.existing;
                    errors.extend(result.errors);
                },
                Err(error)=>errors.push(error),
            }
        }
    }

    let mut missing_after=0i64;
    if repair && !missing.is_empty(){
        for file_id in &missing{
            let mut q=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;
            q.bind((1,*file_id)).map_err(|e|e.to_string())?;
            if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                let relative=q.read::<String,_>(0).unwrap_or_default();
                if !root.join(relative.replace('/',"\\")).exists(){missing_after+=1;}
            }
        }
    }else{
        missing_after=missing.len() as i64;
    }

    Ok(CfapiIntegrityResult{
        checked:rows.len() as i64,
        missing_placeholders:missing_after,
        stale_index:if repair{0}else{stale.len() as i64},
        repaired,errors,root_path:root.to_string_lossy().into_owned(),
    })
}

#[cfg(target_os = "windows")]
fn start_namespace_integrity_worker(app:AppHandle){
    use std::sync::{OnceLock,atomic::{AtomicBool,Ordering}};
    static STARTED:OnceLock<AtomicBool>=OnceLock::new();
    let started=STARTED.get_or_init(||AtomicBool::new(false));
    if started.swap(true,Ordering::SeqCst){return;}
    tauri::async_runtime::spawn(async move{
        // Startup population already runs on demand. The integrity pass is delayed so
        // login, hydration and user-initiated transfers always win.
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        loop{
            if provider_expected_fast(&app) && provider_connected_fast() && platform::diagnostics().active_fetches==0{
                if let Some(db)=app.try_state::<DbConnection>(){
                    if let Ok(conn)=db.lock(){
                        let busy=conn.prepare("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts<12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id) AND attempts<12")
                            .ok().and_then(|mut q|if matches!(q.next(),Ok(sqlite::State::Row)){q.read::<i64,_>(0).ok()}else{Some(0)}).unwrap_or(0);
                        if busy==0{
                            match integrity_check_inner(&app,&conn,true){
                                Ok(result) if result.repaired>0 || result.missing_placeholders>0 || !result.errors.is_empty()=>log::info!(
                                    "CFAPI integrity: {} verificados, {} reparados, {} ausentes, {} erro(s).",
                                    result.checked,result.repaired,result.missing_placeholders,result.errors.len()
                                ),
                                Ok(_)=>{},
                                Err(error)=>log::debug!("CFAPI integrity adiada: {}",error),
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(20*60)).await;
        }
    });
}

#[cfg(not(target_os = "windows"))]
fn start_namespace_integrity_worker(_app:AppHandle){}

#[cfg(target_os = "windows")]
pub(crate) fn provider_connected_fast()->bool{platform::provider_connected()}

#[cfg(not(target_os = "windows"))]
pub(crate) fn provider_connected_fast()->bool{true}

#[cfg(target_os = "windows")]
pub(crate) fn provider_expected_fast(app:&AppHandle)->bool{
    marker_path(app).map(|path|path.exists()).unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn provider_expected_fast(_app:&AppHandle)->bool{false}

pub(crate) fn start_provider_if_registered(app:&AppHandle){
    #[cfg(target_os="windows")]
    {
        let Ok(marker)=marker_path(app) else{return;};
        if !marker.exists(){return;}
        let Ok(root)=cfapi_root() else{return;};
        if platform::platform_info().is_none(){return;}
        let shell=shell_integration::query(&root);
        sync_explorer_pin_verbs(&root,shell.pinning_enabled);
        cleanup_cfapi_temp(app);
        start_pending_worker(app.clone());
        start_provider_health_worker(app.clone());
        start_pin_hydration_worker(app.clone(),root.clone());
        start_namespace_integrity_worker(app.clone());
        match ensure_provider_connection(app, &root, marker.exists()) {
            Ok(repaired) => {
                log::info!("CFAPI final: provider conectado com hidratação, cancelamento, operações e recuperação em {}",root.display());
                // Empty topics/folders must exist in Explorer even when they contain no
                // files. Reconcile this lightweight directory namespace every startup.
                schedule_folder_namespace_refresh(app.clone());
                if repaired {
                    // A package-development unregister can have removed every online-only
                    // placeholder while preserving hydrated files as normal files. Rebuild
                    // only what is missing/needs conversion from the current TCloud index.
                    let app_repair=app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _=tokio::task::spawn_blocking(move||{
                            let Some(db)=app_repair.try_state::<DbConnection>() else{return;};
                            let Ok(conn)=db.lock() else{return;};
                            match populate_inner(&app_repair,&conn,100_000){
                                Ok(result)=>log::info!(
                                    "CFAPI startup repair: {} criado(s), {} existente(s), {} ignorado(s).",
                                    result.created,result.existing,result.skipped
                                ),
                                Err(error)=>log::warn!("CFAPI startup repair falhou: {}",error),
                            }
                        }).await;
                    });
                }
            }
            Err(error)=>log::warn!("CFAPI provider não foi conectado na inicialização: {}",error),
        }
    }
}

pub(crate) fn shutdown_provider(){
    #[cfg(target_os="windows")]
    if let Err(error)=platform::disconnect(){log::warn!("Falha ao desconectar provider CFAPI: {}",error);}
}

pub(crate) fn status_inner(app: &AppHandle, conn: Option<&sqlite::Connection>) -> Result<CfapiStatus, String> {
    let root = cfapi_root()?;
    let marker = marker_path(app)?;
    let platform_info = platform::platform_info();
    let shell = shell_integration::query(&root);
    let mut registered = marker.exists() || shell.registered;

    // Package Identity/loose-layout runs can initially observe a different AppData view.
    // The actual Windows Sync Root is authoritative: if CldApi accepts a connection to the
    // existing TCloud root, recover the marker automatically instead of reporting 0/disabled.
    #[cfg(target_os="windows")]
    if !registered && platform_info.is_some() && root.exists() {
        match platform::connect(&root, app.clone()) {
            Ok(()) => {
                registered = true;
                if let Some(parent)=marker.parent(){let _=std::fs::create_dir_all(parent);}
                let recovered=serde_json::json!({
                    "version":env!("CARGO_PKG_VERSION"),"root":root,"recovered_from_sync_root":true,
                    "shell_package_identity":shell.package_identity,"package_family_name":shell.package_family_name.clone(),
                    "recovered_at":chrono::Utc::now().timestamp()
                });
                if let Ok(bytes)=serde_json::to_vec_pretty(&recovered){let _=std::fs::write(&marker,bytes);}
                log::info!("CFAPI: registro existente recuperado diretamente do Sync Root do Windows.");
            }
            Err(error) => log::debug!("CFAPI: raiz ainda não conectável durante recuperação automática: {}", error),
        }
    }

    #[cfg(target_os="windows")]
    if registered {
        start_pending_worker(app.clone());
        start_provider_health_worker(app.clone());
        start_pin_hydration_worker(app.clone(),root.clone());
        start_namespace_integrity_worker(app.clone());
    }
    if registered && platform_info.is_some() && !platform::provider_connected() {
        match ensure_provider_connection(app, &root, marker.exists()) {
            Ok(repaired) => {
                if repaired {
                    log::info!("CFAPI status: Sync Root perdido foi registrado novamente automaticamente.");
                    // Package refreshes can remove online-only placeholders. Restore the
                    // namespace in the background after the provider is connected again.
                    let app_repair = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = tokio::task::spawn_blocking(move || {
                            let Some(db) = app_repair.try_state::<DbConnection>() else { return; };
                            let Ok(conn) = db.lock() else { return; };
                            match populate_inner(&app_repair, &conn, 100_000) {
                                Ok(result) => log::info!(
                                    "CFAPI autorreparo de namespace: {} criados, {} existentes, {} ignorados.",
                                    result.created, result.existing, result.skipped
                                ),
                                Err(error) => log::warn!("CFAPI autorreparo de namespace falhou: {}", error),
                            }
                        }).await;
                    });
                }
            }
            Err(error) => log::warn!("Autorreconexão CFAPI falhou: {}",error),
        }
    }
    let mut placeholder_count=0i64;
    let mut pending_operations=0i64;
    let mut failed_operations=0i64;
    let mut offline_pinned_count=0i64;
    let mut cloud_only_count=0i64;
    let mut cached_count=0i64;
    if let Some(conn)=conn{
        ensure_schema(conn)?;
        let mut q=conn.prepare("SELECT COUNT(*) FROM tcloud_cfapi_items").map_err(|e|e.to_string())?;
        if matches!(q.next(),Ok(sqlite::State::Row)){placeholder_count=q.read::<i64,_>(0).unwrap_or(0);}
        let mut p=conn.prepare("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts<12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id)").map_err(|e|e.to_string())?;
        if matches!(p.next(),Ok(sqlite::State::Row)){pending_operations=p.read::<i64,_>(0).unwrap_or(0);}
        let mut failed=conn.prepare("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts>=12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id)").map_err(|e|e.to_string())?;
        if matches!(failed.next(),Ok(sqlite::State::Row)){failed_operations=failed.read::<i64,_>(0).unwrap_or(0);}
        let mut pins=conn.prepare("SELECT COUNT(*) FROM tcloud_offline_pins").map_err(|e|e.to_string())?;
        if matches!(pins.next(),Ok(sqlite::State::Row)){offline_pinned_count=pins.read::<i64,_>(0).unwrap_or(0);}
        let mut cloud=conn.prepare("SELECT COUNT(*) FROM tcloud_files WHERE local_status='cloud_only' AND sync_status NOT IN ('deleted','trashed')").map_err(|e|e.to_string())?;
        if matches!(cloud.next(),Ok(sqlite::State::Row)){cloud_only_count=cloud.read::<i64,_>(0).unwrap_or(0);}
        let mut cached=conn.prepare("SELECT COUNT(*) FROM tcloud_files WHERE local_status='cached' AND sync_status NOT IN ('deleted','trashed')").map_err(|e|e.to_string())?;
        if matches!(cached.next(),Ok(sqlite::State::Row)){cached_count=cached.read::<i64,_>(0).unwrap_or(0);}
    }
    let provider_connected=registered && platform::provider_connected();
    let diag=platform::diagnostics();
    let encrypted_hydration_ready=app.try_state::<crate::crypto::state::CryptoState>().map(|s|!s.inner().is_locked()).unwrap_or(false);
    let detail=if platform_info.is_none(){
        "CFAPI não está disponível neste Windows. O TCloud requer suporte nativo a Cloud Files para integração com o Explorer.".to_string()
    }else if registered && provider_connected && shell.pinning_enabled{
        format!("CFAPI + Explorer final ativos: {placeholder_count} placeholder(s), menu offline/pinning habilitado, {} hidratação(ões) concluída(s), {} cancelada(s), {} falha(s), {pending_operations} operação(ões) remota(s) pendente(s) e {failed_operations} precisando de atenção.",diag.completed_fetches,diag.cancelled_fetches,diag.failed_fetches)
    }else if registered && provider_connected{
        format!("CFAPI está hidratando arquivos, mas o Shell moderno do Explorer ainda não confirmou AllowPinning. Use ‘Finalizar integração do Explorer’. {}", shell.detail)
    }else if registered{
        format!("Sync Root registrado com {placeholder_count} item(ns), mas o provider ainda não reconectou. {}", shell.detail)
    }else{
        "CFAPI disponível. Ative o Sync Root; novas instalações usam o registro moderno do Explorer com suporte a pinning/offline.".to_string()
    };
    let native_offline_menu=shell.pinning_enabled;
    let fallback_menu_installed=explorer_pin_verbs_installed();
    let health=if platform_info.is_none(){"unsupported"}
        else if !registered{"setup_needed"}
        else if !provider_connected{"reconnecting"}
        else if failed_operations>0{"needs_attention"}
        else if native_offline_menu{"ready_native"}
        else{"ready_compatible"}.to_string();
    Ok(CfapiStatus{
        platform_supported:platform_info.is_some(),registered,root_path:root.to_string_lossy().into_owned(),fallback:if native_offline_menu{"Menu nativo do Windows".into()}else if fallback_menu_installed{"Menu compatível do TCloud".into()}else{"Nenhum".into()},detail,
        phase:"CFAPI + Explorer final — Shell nativo, pinning/offline, hidratação, criptografia, operações e recuperação".into(),
        placeholder_count,hydration_ready:provider_connected,provider_connected,
        platform_build:platform_info.map(|v|v.0),platform_integration:platform_info.map(|v|v.1),
        active_fetches:diag.active_fetches,completed_fetches:diag.completed_fetches,cancelled_fetches:diag.cancelled_fetches,failed_fetches:diag.failed_fetches,
        pending_operations,failed_operations,encrypted_hydration_ready,last_provider_error:diag.last_error,
        shell_registered:shell.registered,shell_pinning_enabled:shell.pinning_enabled,shell_sync_root_id:shell.sync_root_id,shell_detail:shell.detail,
        package_identity:shell.package_identity,package_family_name:shell.package_family_name,
        health,native_offline_menu,fallback_menu_installed,offline_pinned_count,cloud_only_count,cached_count,auto_repair_active:registered&&platform_info.is_some(),
    })
}


fn refresh_status_db_counters(status: &mut CfapiStatus, conn: &sqlite::Connection) -> Result<(), String> {
    ensure_schema(conn)?;
    let scalar = |sql: &str| -> Result<i64, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        if matches!(stmt.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            Ok(stmt.read::<i64,_>(0).unwrap_or(0))
        } else {
            Ok(0)
        }
    };
    status.placeholder_count = scalar("SELECT COUNT(*) FROM tcloud_cfapi_items")?;
    status.pending_operations = scalar("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts<12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id)")?;
    status.failed_operations = scalar("SELECT COUNT(*) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts>=12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id)")?;
    status.offline_pinned_count = scalar("SELECT COUNT(*) FROM tcloud_offline_pins")?;
    status.cloud_only_count = scalar("SELECT COUNT(*) FROM tcloud_files WHERE local_status='cloud_only' AND sync_status NOT IN ('deleted','trashed')")?;
    status.cached_count = scalar("SELECT COUNT(*) FROM tcloud_files WHERE local_status='cached' AND sync_status NOT IN ('deleted','trashed')")?;

    status.health = if !status.platform_supported { "unsupported" }
        else if !status.registered { "setup_needed" }
        else if !status.provider_connected { "reconnecting" }
        else if status.failed_operations > 0 { "needs_attention" }
        else if status.native_offline_menu { "ready_native" }
        else { "ready_compatible" }.to_string();

    if status.platform_supported {
        status.detail = if status.registered && status.provider_connected && status.shell_pinning_enabled {
            format!(
                "Integração do Explorer ativa: {} item(ns), {} sempre offline, {} neste computador, {} somente na nuvem, {} operação(ões) pendente(s) e {} precisando de atenção.",
                status.placeholder_count,
                status.offline_pinned_count,
                status.cached_count,
                status.cloud_only_count,
                status.pending_operations,
                status.failed_operations,
            )
        } else if status.registered && status.provider_connected {
            format!("TCloud conectado ao Explorer. O Windows ainda está finalizando os recursos nativos de disponibilidade. {}", status.shell_detail)
        } else if status.registered {
            format!("TCloud está reconectando ao Explorer. {}", status.shell_detail)
        } else {
            "Este computador ainda precisa ser conectado ao TCloud para usar arquivos sob demanda no Explorer.".into()
        };
    }
    Ok(())
}

#[tauri::command]
pub async fn tcloud_cfapi_status(app: AppHandle, db: State<'_, DbConnection>) -> Result<CfapiStatus, String> {
    // Windows.Storage.Provider / WinRT and registry probing can occasionally take
    // hundreds of milliseconds (or longer while Explorer is recovering). Never run
    // those blocking calls on Tauri's window/IPC thread. Also do not hold the SQLite
    // mutex while Windows/Explorer is answering: sync workers must remain free to make
    // progress even if the Shell is temporarily slow.
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let mut status = status_inner(&app, None)?;
        {
            let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
            refresh_status_db_counters(&mut status, &conn)?;
        }
        let elapsed = started.elapsed();
        if elapsed >= std::time::Duration::from_secs(2) {
            log::warn!("TCloud: consulta de status do Explorer levou {:?}; executada fora da thread da interface.", elapsed);
        }
        Ok::<CfapiStatus, String>(status)
    })
    .await
    .map_err(|e| format!("CFAPI status worker failed: {e}"))?
}

#[tauri::command]
pub async fn tcloud_cfapi_retry_failed_operations(app: AppHandle, db: State<'_, DbConnection>) -> Result<CfapiStatus, String> {
    let db_clone=db.inner().clone();
    drop(db);
    {
        let conn=db_clone.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let now=chrono::Utc::now().timestamp();
        let mut files=conn.prepare(
            "UPDATE tcloud_files SET sync_status='pending_move',updated_at=?1 \
             WHERE id IN (SELECT p.file_id FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts>=12 AND p.op_type IN ('rename','relocate') AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id))"
        ).map_err(|e|e.to_string())?;
        files.bind((1,now)).map_err(|e|e.to_string())?;
        files.next().map_err(|e|e.to_string())?;
        let mut retry=conn.prepare(
            "UPDATE tcloud_cfapi_pending_ops SET attempts=0,last_attempt_at=NULL,last_error=NULL \
             WHERE completed_at IS NULL AND attempts>=12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=tcloud_cfapi_pending_ops.file_id AND newer.op_type=tcloud_cfapi_pending_ops.op_type AND newer.id>tcloud_cfapi_pending_ops.id)"
        ).map_err(|e|e.to_string())?;
        retry.next().map_err(|e|e.to_string())?;
    }
    #[cfg(target_os="windows")]
    {
        let _=process_pending_ops_once(&app).await;
    }
    let conn=db_clone.lock().map_err(|_|"database lock poisoned".to_string())?;
    status_inner(&app,Some(&conn))
}

#[tauri::command]
pub fn tcloud_cfapi_set(app: AppHandle, db: State<'_, DbConnection>, enabled: bool) -> Result<CfapiStatus, String> {
    let root = cfapi_root()?;
    let marker = marker_path(&app)?;
    if enabled {
        if platform::platform_info().is_none() { return Err("Este Windows não oferece a integração de arquivos sob demanda necessária pelo TCloud.".into()); }
        let shell_before = shell_integration::query(&root);
        if !marker.exists() && !shell_before.registered {
            // Fresh installs prefer the modern StorageProvider registration. This is what
            // makes the Sync Root a first-class Explorer cloud provider and exposes the
            // standard pin/offline semantics instead of only registering with CldApi.
            if let Err(shell_error)=shell_integration::register_full(&root) {
                log::warn!("Registro Shell moderno falhou; mantendo CFAPI legado como fallback: {}",shell_error);
                platform::register(&root, false)?;
            }
        } else if !shell_before.registered {
            // Existing/legacy roots stay operational here. The explicit finalizer performs
            // the destructive unregister/re-register migration and repairs placeholders.
            platform::register(&root, marker.exists())?;
        }
        let shell_after=shell_integration::query(&root);
        #[cfg(target_os="windows")]
        sync_explorer_pin_verbs(&root,shell_after.pinning_enabled);
        let value = serde_json::json!({
            "version":env!("CARGO_PKG_VERSION"),"root":root,"phase":6,"final_cfapi":true,"shell_target":true,
            "shell_registered":shell_after.registered,"shell_pinning_enabled":shell_after.pinning_enabled,
            "shell_package_identity":shell_after.package_identity,"package_family_name":shell_after.package_family_name,
            "registered_at":chrono::Utc::now().timestamp()
        });
        std::fs::write(&marker, serde_json::to_vec_pretty(&value).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        // Use the same self-healing connection path used at startup. A stale Windows
        // Sync Root registration should not force the user into technical recovery.
        let _ = ensure_provider_connection(&app, &root, marker.exists())?;
        #[cfg(target_os="windows")]
        {
            cleanup_cfapi_temp(&app);
            start_pending_worker(app.clone());
            start_provider_health_worker(app.clone());
            start_pin_hydration_worker(app.clone(),root.clone());
            start_namespace_integrity_worker(app.clone());
        }
    } else if marker.exists() {
        // Disconnect first so no FETCH_DATA callback can outlive its TCloud context.
        platform::disconnect()?;
        // Prefer the registration mechanism used by the modern Shell and keep CldApi as
        // a compatibility cleanup for roots created by older TCloud versions.
        let _=shell_integration::unregister(&root);
        let _=platform::unregister(&root);
        let _=remove_explorer_pin_verbs();
        let _=std::fs::remove_file(&marker);
    }
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    status_inner(&app, Some(&conn))
}

fn populate_inner_filtered(app: &AppHandle, conn: &sqlite::Connection, limit: i64, chat_filter: Option<i64>) -> Result<CfapiPopulateResult, String> {
    let root = cfapi_root()?;
    let marker = marker_path(app)?;
    if !marker.exists() { return Err("Ative a CFAPI antes de gerar placeholders.".into()); }
    if platform::platform_info().is_none() { return Err("Este Windows não oferece a integração de arquivos sob demanda necessária pelo TCloud.".into()); }
    let repaired_registration = ensure_provider_connection(app, &root, marker.exists())?;
    if repaired_registration {
        log::info!("CFAPI populate: registro do Sync Root foi autorreparado antes da reconciliação.");
    }
    ensure_schema(conn)?;
    if let Err(error)=reconcile_managed_folder_namespace(app,conn){
        log::warn!("TCloud Explorer: pastas não puderam ser reconciliadas antes dos placeholders: {}",error);
    }
    let files = match chat_filter {
        Some(chat_key)=>read_indexed_files_for_chat(conn,chat_key,limit.clamp(1,100_000))?,
        None=>read_indexed_files(conn,limit.clamp(1,100_000))?,
    };
    let mut folders: HashMap<i64, Vec<String>> = HashMap::new();
    let mut result = CfapiPopulateResult { indexed: files.len() as i64, created:0, existing:0, skipped:0, errors:Vec::new(), root_path:root.to_string_lossy().into_owned() };

    for file in files {
        let known_item=known_cfapi_item_for_file(conn,&file)?;
        let known_relative=known_item.as_ref().map(|(_,relative,_)|relative.clone());

        let segments = folder_for_chat(conn, file.chat_key, &mut folders);
        let mut base = root.clone();
        for segment in segments {
            base.push(sanitize_segment(&segment, "Arquivos"));
        }
        // Preserve user-created subfolders and the original Trash restore location
        // whenever that path still belongs to the same Telegram destination.
        if let Some(relative)=known_relative.as_deref(){
            if let Some(preserved)=preserved_parent_for_chat(conn,&root,relative,file.chat_key){
                base=preserved;
            }
        }
        if let Err(e)=std::fs::create_dir_all(&base) { result.skipped+=1; result.errors.push(format!("{}: falha ao criar pasta: {e}",file.name)); continue; }

        let (final_name,target)=unique_target(&base,&file.name,file.message_id,known_relative.as_deref(),&root);
        let relative=relative_string(&root,&target);
        let identity = CfapiFileIdentity{version:1,file_id:file.id,chat_key:file.chat_key,message_id:file.message_id,size:file.size,updated_at:file.updated_at};
        let identity_json=serde_json::to_string(&identity).map_err(|e|e.to_string())?;

        // Telegram-side rename: move the existing placeholder instead of creating
        // a second local entry. `applying_remote_rename` tells the CFAPI completion
        // callback that this mutation originated from the provider, preventing a
        // rename -> upload -> rename feedback loop.
        if let Some(old_relative)=known_relative.as_ref(){
            if !old_relative.eq_ignore_ascii_case(&relative){
                let old_path=root.join(old_relative);
                if old_path.exists() && !target.exists(){
                    let mut status=conn.prepare("UPDATE tcloud_files SET sync_status='applying_remote_rename' WHERE id=?1 AND sync_status NOT IN ('pending_upload','uploading','pending_move','conflict')").map_err(|e|e.to_string())?;
                    status.bind((1,file.id)).map_err(|e|e.to_string())?;status.next().map_err(|e|e.to_string())?;
                    suppress_watcher_path(&old_path,15);suppress_watcher_path(&target,15);
                    if let Err(error)=std::fs::rename(&old_path,&target){
                        if result.errors.len()<50{result.errors.push(format!("{}: rename remoto não pôde ser aplicado localmente: {error}",file.name));}
                    }
                }
            }
        }

        if let Some((known_id,known_path,known_identity))=known_item.as_ref(){
            if *known_id==file.id && known_path.eq_ignore_ascii_case(&relative) && known_identity==&identity_json && target.exists(){
                apply_saved_policy_to_target(conn,&relative,&target);
                result.existing+=1;
                continue;
            }
        }
        suppress_watcher_path(&target, 12);
        if target.exists() {
            result.existing += 1;
            if let Err(update_error)=platform::update_placeholder(&target,file.size,file.updated_at,identity_json.as_bytes()){
                // During the WinRT/Shell migration Windows can preserve hydrated files as
                // normal files. Convert them back to full cloud placeholders without
                // deleting local data, then keep them pinned until the user frees space.
                if let Err(convert_error)=platform::convert_existing_file(&target,identity_json.as_bytes()){
                    if result.errors.len()<50{result.errors.push(format!("{}: atualização falhou ({update_error}); conversão local também falhou ({convert_error})",file.name));}
                }
            }
            let now=chrono::Utc::now().timestamp();
            if let Some((old_id,_,_))=&known_item{
                if *old_id!=file.id{
                    let mut stale=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;
                    stale.bind((1,*old_id)).map_err(|e|e.to_string())?;stale.next().map_err(|e|e.to_string())?;
                }
            }
            let mut up=conn.prepare("INSERT INTO tcloud_cfapi_items(file_id,relative_path,identity_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(file_id) DO UPDATE SET relative_path=excluded.relative_path,identity_json=excluded.identity_json,updated_at=excluded.updated_at").map_err(|e|e.to_string())?;
            up.bind((1,file.id)).map_err(|e|e.to_string())?;up.bind((2,relative.as_str())).map_err(|e|e.to_string())?;up.bind((3,identity_json.as_str())).map_err(|e|e.to_string())?;up.bind((4,now)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
            store_local_fingerprint(conn,file.id,&target);
            apply_saved_policy_to_target(conn,&relative,&target);
            continue;
        }
        match platform::create_placeholder(&base,&final_name,file.size,file.created_at,file.updated_at,identity_json.as_bytes()) {
            Ok(()) => {
                result.created += 1;
                let now=chrono::Utc::now().timestamp();
                if let Some((old_id,_,_))=&known_item{
                    if *old_id!=file.id{
                        let mut stale=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;
                        stale.bind((1,*old_id)).map_err(|e|e.to_string())?;stale.next().map_err(|e|e.to_string())?;
                    }
                }
                let mut up=conn.prepare("INSERT INTO tcloud_cfapi_items(file_id,relative_path,identity_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?4) ON CONFLICT(file_id) DO UPDATE SET relative_path=excluded.relative_path,identity_json=excluded.identity_json,updated_at=excluded.updated_at").map_err(|e|e.to_string())?;
                up.bind((1,file.id)).map_err(|e|e.to_string())?;up.bind((2,relative.as_str())).map_err(|e|e.to_string())?;up.bind((3,identity_json.as_str())).map_err(|e|e.to_string())?;up.bind((4,now)).map_err(|e|e.to_string())?;up.next().map_err(|e|e.to_string())?;
                store_local_fingerprint(conn,file.id,&target);
                apply_saved_policy_to_target(conn,&relative,&target);
            }
            Err(e)=>{result.skipped+=1;if result.errors.len()<50{result.errors.push(format!("{}: {e}",file.name));}}
        }
    }
    Ok(result)
}



#[cfg(target_os = "windows")]
fn apply_saved_policy_to_target(conn:&sqlite::Connection, relative:&str, target:&Path) {
    let Ok(mode)=crate::commands::tcloud_sync_maturity::folder_policy_for_relative(conn,relative) else{return;};
    if mode=="normal" || !target.exists(){return;}
    suppress_watcher_path(target,15);
    let result=if mode=="offline"{platform::set_item_availability(target,true)}else{platform::set_item_availability(target,false)};
    if let Err(error)=result{log::warn!("CFAPI política '{}' não pôde ser aplicada agora a {}: {}",mode,target.display(),error);}
}

#[cfg(target_os = "windows")]
pub(crate) fn apply_folder_sync_policy(_app:&AppHandle, _db:&DbConnection, relative_path:&str, mode:&str)->Result<usize,String>{
    let root=cfapi_root()?;
    let base=if relative_path.trim_matches('/').is_empty(){root.clone()}else{root.join(relative_path.replace('/',"\\"))};
    if !base.exists(){return Ok(0);}
    if mode=="normal"{return Ok(0);}
    let mut files=Vec::new();
    fn collect(path:&Path,out:&mut Vec<PathBuf>){
        if path.is_file(){out.push(path.to_path_buf());return;}
        let Ok(read)=std::fs::read_dir(path) else{return;};
        for entry in read.flatten(){let p=entry.path();if p.is_dir(){collect(&p,out)}else if p.is_file(){out.push(p)}}
    }
    collect(&base,&mut files);
    let mut applied=0usize;
    for file in files{
        suppress_watcher_path(&file,15);
        let result=if mode=="offline"{platform::set_item_availability(&file,true)}else{platform::set_item_availability(&file,false)};
        match result{Ok(())=>applied+=1,Err(error)=>log::warn!("TCloud política de pasta: {}: {}",file.display(),error)}
    }
    Ok(applied)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn apply_folder_sync_policy(_app:&AppHandle,_db:&DbConnection,_relative_path:&str,_mode:&str)->Result<usize,String>{Ok(0)}

#[cfg(target_os = "windows")]
pub(crate) fn prepare_restore_location(db:&DbConnection,file_id:i64,original_relative:Option<&str>)->Result<Option<String>,String>{
    use std::path::Component;
    let Some(original)=original_relative.map(str::trim).filter(|value|!value.is_empty()) else{return Ok(None);};
    let root=cfapi_root()?;
    let normalized=original.replace('\\',"/").trim_matches('/').to_string();
    let rel=Path::new(&normalized);
    if rel.components().any(|component|!matches!(component,Component::Normal(_))){return Ok(None);}
    let file_name=rel.file_name().and_then(|value|value.to_str()).filter(|value|!value.is_empty()).ok_or_else(||"Local original da Lixeira é inválido.".to_string())?;
    let parent=rel.parent().unwrap_or_else(||Path::new(""));

    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let chat_key={
        let mut q=conn.prepare("SELECT COALESCE(telegram_chat_id,0) FROM tcloud_files WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,file_id)).map_err(|e|e.to_string())?;
        if !matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(None);}
        q.read::<i64,_>(0).unwrap_or(0)
    };
    if chat_key_for_relative_path(&conn,rel).ok()!=Some(chat_key){return Ok(None);}

    let occupied=|candidate:&str,conn:&sqlite::Connection,root:&Path,file_id:i64|->bool{
        if root.join(candidate.replace('/',"\\")).exists(){return true;}
        let Ok(mut q)=conn.prepare("SELECT 1 FROM tcloud_cfapi_items WHERE lower(relative_path)=lower(?1) AND file_id<>?2 LIMIT 1") else{return true;};
        if q.bind((1,candidate)).is_err() || q.bind((2,file_id)).is_err(){return true;}
        matches!(q.next(),Ok(sqlite::State::Row))
    };

    let mut candidate=normalized.clone();
    if occupied(&candidate,&conn,&root,file_id){
        let name_path=Path::new(file_name);
        let stem=name_path.file_stem().and_then(|value|value.to_str()).unwrap_or("arquivo");
        let ext=name_path.extension().and_then(|value|value.to_str());
        for index in 1..=999u32{
            let suffix=if index==1{"Restaurado".to_string()}else{format!("Restaurado {index}")};
            let next_name=match ext{
                Some(ext) if !ext.is_empty()=>format!("{stem} ({suffix}).{ext}"),
                _=>format!("{stem} ({suffix})"),
            };
            let next=if parent.as_os_str().is_empty(){PathBuf::from(next_name)}else{parent.join(next_name)};
            let next_string=next.to_string_lossy().replace('\\',"/");
            if !occupied(&next_string,&conn,&root,file_id){candidate=next_string;break;}
        }
    }

    if let Some(parent)=Path::new(&candidate).parent(){
        std::fs::create_dir_all(root.join(parent)).map_err(|e|format!("Não foi possível recriar a pasta original no Explorer: {e}"))?;
    }
    let now=chrono::Utc::now().timestamp();
    let mut up=conn.prepare(
        "INSERT INTO tcloud_cfapi_items(file_id,relative_path,identity_json,created_at,updated_at)
         VALUES(?1,?2,'',?3,?3)
         ON CONFLICT(file_id) DO UPDATE SET relative_path=excluded.relative_path,identity_json='',updated_at=excluded.updated_at"
    ).map_err(|e|e.to_string())?;
    up.bind((1,file_id)).map_err(|e|e.to_string())?;
    up.bind((2,candidate.as_str())).map_err(|e|e.to_string())?;
    up.bind((3,now)).map_err(|e|e.to_string())?;
    up.next().map_err(|e|e.to_string())?;
    Ok(Some(candidate))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn prepare_restore_location(_db:&DbConnection,_file_id:i64,_original_relative:Option<&str>)->Result<Option<String>,String>{Ok(None)}

#[cfg(target_os = "windows")]
pub(crate) fn preserve_local_conflict_copy(db:&DbConnection,file_id:i64)->Result<String,String>{
    let root=cfapi_root()?;
    let relative={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let mut q=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,file_id)).map_err(|e|e.to_string())?;
        if !matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Err("A versão do computador não está mais disponível no Explorer.".into());}
        q.read::<String,_>(0).map_err(|e|e.to_string())?
    };
    let source=root.join(relative.replace('/',"\\"));
    if !source.is_file(){return Err("A versão do computador não está disponível para ser preservada.".into());}
    let parent=source.parent().ok_or_else(||"Pasta local do conflito é inválida.".to_string())?;
    let original=source.file_name().and_then(|value|value.to_str()).unwrap_or("arquivo");
    let name_path=Path::new(original);
    let stem=name_path.file_stem().and_then(|value|value.to_str()).unwrap_or("arquivo");
    let ext=name_path.extension().and_then(|value|value.to_str());
    let stamp=chrono::Utc::now().format("%Y-%m-%d %H%M").to_string();
    let mut target=None;
    for index in 0..1000u32{
        let marker=if index==0{format!("Conflito deste computador {stamp}")}else{format!("Conflito deste computador {stamp} {index}")};
        let name=match ext{
            Some(ext) if !ext.is_empty()=>format!("{stem} ({marker}).{ext}"),
            _=>format!("{stem} ({marker})"),
        };
        let candidate=parent.join(name);
        if !candidate.exists(){target=Some(candidate);break;}
    }
    let target=target.ok_or_else(||"Não foi possível gerar um nome seguro para guardar as duas versões.".to_string())?;
    std::fs::copy(&source,&target).map_err(|e|format!("Não foi possível preservar a versão do computador: {e}"))?;
    // Do not suppress the new file. Explicitly queue it as a fresh upload so
    // "Guardar as duas" remains reliable even if Windows coalesces notify events.
    crate::sync::watcher_runtime::queue_local_file_now(db,&root,&target)?;
    Ok(relative_string(&root,&target))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn preserve_local_conflict_copy(_db:&DbConnection,_file_id:i64)->Result<String,String>{Err("Guardar as duas pelo Explorer requer Windows.".into())}


#[cfg(target_os = "windows")]
pub(crate) fn rebind_managed_directory_prefix(
    db: &DbConnection,
    old_relative: &str,
    new_relative: &str,
) -> Result<usize, String> {
    let normalize = |value: &str| value.replace('\\', "/").trim_matches('/').to_string();
    let old_prefix = normalize(old_relative);
    let new_prefix = normalize(new_relative);
    if old_prefix.is_empty() || new_prefix.is_empty() { return Ok(0); }
    let root = cfapi_root()?;
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let like = format!("{}/%", old_prefix);
    let mut query = conn.prepare(
        "SELECT file_id,relative_path FROM tcloud_cfapi_items
         WHERE lower(relative_path)=lower(?1) OR lower(relative_path) LIKE lower(?2)"
    ).map_err(|e| e.to_string())?;
    query.bind((1, old_prefix.as_str())).map_err(|e| e.to_string())?;
    query.bind((2, like.as_str())).map_err(|e| e.to_string())?;
    let mut rows = Vec::<(i64,String,String)>::new();
    while let sqlite::State::Row = query.next().map_err(|e| e.to_string())? {
        let file_id = query.read::<i64,_>(0).unwrap_or(0);
        let old_path = query.read::<String,_>(1).unwrap_or_default();
        let suffix = old_path.get(old_prefix.len()..).unwrap_or("").trim_start_matches('/');
        let target = if suffix.is_empty() { new_prefix.clone() } else { format!("{new_prefix}/{suffix}") };
        rows.push((file_id, old_path, target));
    }
    drop(query);
    let now = chrono::Utc::now().timestamp();
    for (file_id, _old_path, target) in &rows {
        let mut update = conn.prepare("UPDATE tcloud_cfapi_items SET relative_path=?1,updated_at=?2 WHERE file_id=?3")
            .map_err(|e| e.to_string())?;
        update.bind((1, target.as_str())).map_err(|e| e.to_string())?;
        update.bind((2, now)).map_err(|e| e.to_string())?;
        update.bind((3, *file_id)).map_err(|e| e.to_string())?;
        update.next().map_err(|e| e.to_string())?;

        let full = root.join(target.replace('/', "\\")).to_string_lossy().to_string();
        let mut file = conn.prepare("UPDATE tcloud_files SET path=?1,updated_at=?2 WHERE id=?3")
            .map_err(|e| e.to_string())?;
        file.bind((1, full.as_str())).map_err(|e| e.to_string())?;
        file.bind((2, now)).map_err(|e| e.to_string())?;
        file.bind((3, *file_id)).map_err(|e| e.to_string())?;
        file.next().map_err(|e| e.to_string())?;
    }

    // Files created locally but not uploaded yet have no CFAPI row. Rebind them
    // too so an Explorer rename of a managed directory cannot leave an upload job
    // pointing at the old physical path.
    let old_full = root.join(old_prefix.replace('/', "\\"));
    let new_full = root.join(new_prefix.replace('/', "\\"));
    let mut pending = conn.prepare(
        "SELECT id,path FROM tcloud_files WHERE telegram_message_id IS NULL
         AND sync_status IN ('pending_upload','uploading','pending_move')"
    ).map_err(|e| e.to_string())?;
    let mut pending_rows = Vec::new();
    while let sqlite::State::Row = pending.next().map_err(|e| e.to_string())? {
        let id = pending.read::<i64,_>(0).unwrap_or(0);
        let stored = pending.read::<String,_>(1).unwrap_or_default();
        let stored_path = Path::new(&stored);
        if let Ok(suffix) = stored_path.strip_prefix(&old_full) {
            pending_rows.push((id, new_full.join(suffix).to_string_lossy().to_string()));
        }
    }
    drop(pending);
    for (id, full) in pending_rows {
        let mut update = conn.prepare("UPDATE tcloud_files SET path=?1,updated_at=?2 WHERE id=?3")
            .map_err(|e| e.to_string())?;
        update.bind((1, full.as_str())).map_err(|e| e.to_string())?;
        update.bind((2, now)).map_err(|e| e.to_string())?;
        update.bind((3, id)).map_err(|e| e.to_string())?;
        update.next().map_err(|e| e.to_string())?;
    }
    Ok(rows.len())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn rebind_managed_directory_prefix(_db:&DbConnection,_old_relative:&str,_new_relative:&str)->Result<usize,String>{Ok(0)}

#[cfg(target_os = "windows")]
pub(crate) fn handle_local_folder_move(db:&DbConnection,old_relative:&str,new_relative:&str)->Result<usize,String>{
    let normalize=|value:&str|value.replace('\\',"/").trim_matches('/').to_string();
    let old_prefix=normalize(old_relative);
    let new_prefix=normalize(new_relative);
    if old_prefix.is_empty() || new_prefix.is_empty(){return Ok(0);}
    let root=cfapi_root()?;

    #[derive(Clone)]
    struct MoveItem{
        file_id:i64,
        new_relative:String,
        identity:CfapiFileIdentity,
        status:String,
        target_chat_key:i64,
        baseline:Option<String>,
    }

    let items={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        let like=format!("{}/%",&old_prefix);
        let mut q=conn.prepare(
            "SELECT c.file_id,c.relative_path,c.identity_json,COALESCE(f.sync_status,'synced')
             FROM tcloud_cfapi_items c JOIN tcloud_files f ON f.id=c.file_id
             WHERE lower(c.relative_path) LIKE lower(?1)"
        ).map_err(|e|e.to_string())?;
        q.bind((1,like.as_str())).map_err(|e|e.to_string())?;
        let mut rows=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            let file_id=q.read::<i64,_>(0).unwrap_or(0);
            let old_path=q.read::<String,_>(1).unwrap_or_default();
            let identity_json=q.read::<String,_>(2).unwrap_or_default();
            let status=q.read::<String,_>(3).unwrap_or_else(|_|"synced".into());
            let Ok(identity)=serde_json::from_str::<CfapiFileIdentity>(&identity_json) else{continue;};
            let suffix=old_path.get(old_prefix.len()..).unwrap_or("").trim_start_matches('/');
            let new_path=if suffix.is_empty(){new_prefix.clone()}else{format!("{new_prefix}/{suffix}")};
            let target_chat_key=chat_key_for_relative_path(&conn,Path::new(&new_path))?;
            let baseline=local_path_fingerprint(&root.join(new_path.replace('/',"\\")));
            rows.push(MoveItem{file_id,new_relative:new_path,identity,status,target_chat_key,baseline});
        }
        drop(q);

        // Do the collision preflight before changing any row. Explorer already
        // validated the physical move; this protects the SQLite projection from
        // an unrelated stale item owning one of the new paths.
        let moved_ids:std::collections::HashSet<i64>=rows.iter().map(|item|item.file_id).collect();
        for item in &rows{
            let mut occupied=conn.prepare(
                "SELECT file_id FROM tcloud_cfapi_items WHERE lower(relative_path)=lower(?1) LIMIT 1"
            ).map_err(|e|e.to_string())?;
            occupied.bind((1,item.new_relative.as_str())).map_err(|e|e.to_string())?;
            if matches!(occupied.next().map_err(|e|e.to_string())?,sqlite::State::Row){
                let owner=occupied.read::<i64,_>(0).unwrap_or(0);
                if owner!=item.file_id && !moved_ids.contains(&owner){
                    return Err(format!("A pasta movida contém um arquivo cujo destino já pertence a outro item TCloud: {}",item.new_relative));
                }
            }
        }

        conn.execute("BEGIN IMMEDIATE").map_err(|e|e.to_string())?;
        let update_result:Result<(),String>=(||{
            let now=chrono::Utc::now().timestamp();
            for item in &rows{
                let mut path=conn.prepare("UPDATE tcloud_cfapi_items SET relative_path=?1,updated_at=?2 WHERE file_id=?3").map_err(|e|e.to_string())?;
                path.bind((1,item.new_relative.as_str())).map_err(|e|e.to_string())?;
                path.bind((2,now)).map_err(|e|e.to_string())?;
                path.bind((3,item.file_id)).map_err(|e|e.to_string())?;
                path.next().map_err(|e|e.to_string())?;

                let needs_remote=item.target_chat_key!=item.identity.chat_key || item.status=="pending_move";
                let mut file=conn.prepare(
                    "UPDATE tcloud_files SET sync_status=?1,updated_at=?2 WHERE id=?3"
                ).map_err(|e|e.to_string())?;
                file.bind((1,if needs_remote{"pending_move"}else{item.status.as_str()})).map_err(|e|e.to_string())?;
                file.bind((2,now)).map_err(|e|e.to_string())?;
                file.bind((3,item.file_id)).map_err(|e|e.to_string())?;
                file.next().map_err(|e|e.to_string())?;
            }
            Ok(())
        })();
        if let Err(error)=update_result{let _=conn.execute("ROLLBACK");return Err(error);}
        conn.execute("COMMIT").map_err(|e|e.to_string())?;
        rows
    };

    // Queue cross-destination moves after releasing the SQLite transaction.
    // Same-destination subfolder renames are local organization only.
    for item in &items{
        let needs_remote=item.target_chat_key!=item.identity.chat_key || item.status=="pending_move";
        if !needs_remote{continue;}
        let new_name=Path::new(&item.new_relative).file_name().and_then(|value|value.to_str()).unwrap_or("arquivo").to_string();
        let payload=serde_json::json!({
            "new_name":new_name,
            "target_chat_key":item.target_chat_key,
            "target_relative":item.new_relative,
            "baseline_fingerprint":item.baseline,
        }).to_string();
        queue_pending_op(db,"relocate",&item.identity,Some(&payload))?;
    }
    Ok(items.len())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn handle_local_folder_move(_db:&DbConnection,_old_relative:&str,_new_relative:&str)->Result<usize,String>{Ok(0)}

#[cfg(target_os = "windows")]
pub(crate) fn discard_local_file_for_conflict(db:&DbConnection,file_id:i64)->Result<(),String>{
    let root=cfapi_root()?;let relative={let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;let mut q=conn.prepare("SELECT relative_path FROM tcloud_cfapi_items WHERE file_id=?1 LIMIT 1").map_err(|e|e.to_string())?;q.bind((1,file_id)).map_err(|e|e.to_string())?;if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<String,_>(0).ok()}else{None}};
    if let Some(relative)=relative{let path=root.join(relative);if path.exists(){suppress_watcher_path(&path,15);platform::set_item_availability(&path,false)?;}}
    Ok(())
}
#[cfg(not(target_os = "windows"))]
pub(crate) fn discard_local_file_for_conflict(_db:&DbConnection,_file_id:i64)->Result<(),String>{Ok(())}

fn populate_inner(app: &AppHandle, conn: &sqlite::Connection, limit: i64) -> Result<CfapiPopulateResult, String> {
    populate_inner_filtered(app,conn,limit,None)
}

#[cfg(target_os = "windows")]
pub(crate) fn reconcile_remote_chat(app:&AppHandle,db:&DbConnection,chat_key:i64)->Result<(),String>{
    let root=cfapi_root()?;
    let pending_deletes={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q=conn.prepare(
            "SELECT f.id,c.relative_path,f.sync_status FROM tcloud_files f
             LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id
             WHERE COALESCE(f.telegram_chat_id,0)=?1 AND f.sync_status IN ('remote_deleted','applying_remote_delete')"
        ).map_err(|e|e.to_string())?;
        q.bind((1,chat_key)).map_err(|e|e.to_string())?;
        let mut rows=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            rows.push((q.read::<i64,_>(0).unwrap_or(0),q.read::<Option<String>,_>(1).unwrap_or(None),q.read::<String,_>(2).unwrap_or_default()));
        }
        rows
    };
    for (file_id,relative,status) in pending_deletes{
        let Some(relative)=relative else{
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let mut done=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted' WHERE id=?1").map_err(|e|e.to_string())?;
            done.bind((1,file_id)).map_err(|e|e.to_string())?;done.next().map_err(|e|e.to_string())?;
            continue;
        };
        let path=root.join(&relative);
        if !path.exists(){
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let mut del=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;del.bind((1,file_id)).map_err(|e|e.to_string())?;del.next().map_err(|e|e.to_string())?;
            let mut done=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;done.bind((1,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;done.bind((2,file_id)).map_err(|e|e.to_string())?;done.next().map_err(|e|e.to_string())?;
            continue;
        }
        if status=="remote_deleted"{
            {
                let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                let mut mark=conn.prepare("UPDATE tcloud_files SET sync_status='applying_remote_delete' WHERE id=?1").map_err(|e|e.to_string())?;mark.bind((1,file_id)).map_err(|e|e.to_string())?;mark.next().map_err(|e|e.to_string())?;
            }
            suppress_watcher_path(&path,15);
            if let Err(error)=std::fs::remove_file(&path){
                log::warn!("TCloud: exclusão remota ainda não pôde ser refletida em {}: {}",path.display(),error);
            }
        }
    }
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let _=populate_inner_filtered(app,&conn,50_000,Some(chat_key))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reconcile_remote_chat(_app:&AppHandle,_db:&DbConnection,_chat_key:i64)->Result<(),String>{Ok(())}

pub(crate) async fn refresh_after_folder_delete_now(app: AppHandle) -> Result<CfapiFolderNamespaceResult, String> {
    let marker = marker_path(&app)?;
    if !marker.exists() {
        return Ok(CfapiFolderNamespaceResult { root_path: cfapi_root()?.to_string_lossy().into_owned(), ..Default::default() });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.try_state::<DbConnection>().ok_or_else(|| "Banco do TCloud indisponível".to_string())?;
        // Apply file-level remote deletions first. populate_inner invoked by the
        // reconcile also refreshes the managed directory namespace.
        reconcile_remote_index(&app, db.inner())?;
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        reconcile_managed_folder_namespace(&app,&conn)
    }).await.map_err(|e|format!("Falha ao remover pasta do Explorer: {e}"))?
}

#[cfg(target_os = "windows")]
pub(crate) fn reconcile_remote_index(app:&AppHandle,db:&DbConnection)->Result<(),String>{
    let root=cfapi_root()?;
    let pending_deletes={
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q=conn.prepare(
            "SELECT f.id,c.relative_path,f.sync_status FROM tcloud_files f
             LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id
             WHERE f.sync_status IN ('remote_deleted','applying_remote_delete')"
        ).map_err(|e|e.to_string())?;
        let mut rows=Vec::new();
        while let sqlite::State::Row=q.next().map_err(|e|e.to_string())?{
            rows.push((q.read::<i64,_>(0).unwrap_or(0),q.read::<Option<String>,_>(1).unwrap_or(None),q.read::<String,_>(2).unwrap_or_default()));
        }
        rows
    };
    for (file_id,relative,status) in pending_deletes{
        let Some(relative)=relative else{
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let mut done=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted' WHERE id=?1").map_err(|e|e.to_string())?;done.bind((1,file_id)).map_err(|e|e.to_string())?;done.next().map_err(|e|e.to_string())?;
            continue;
        };
        let path=root.join(&relative);
        if !path.exists(){
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let mut del=conn.prepare("DELETE FROM tcloud_cfapi_items WHERE file_id=?1").map_err(|e|e.to_string())?;del.bind((1,file_id)).map_err(|e|e.to_string())?;del.next().map_err(|e|e.to_string())?;
            let mut done=conn.prepare("UPDATE tcloud_files SET sync_status='trashed',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;done.bind((1,chrono::Utc::now().timestamp())).map_err(|e|e.to_string())?;done.bind((2,file_id)).map_err(|e|e.to_string())?;done.next().map_err(|e|e.to_string())?;
            continue;
        }
        if status=="remote_deleted"{
            {
                let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                let mut mark=conn.prepare("UPDATE tcloud_files SET sync_status='applying_remote_delete' WHERE id=?1").map_err(|e|e.to_string())?;mark.bind((1,file_id)).map_err(|e|e.to_string())?;mark.next().map_err(|e|e.to_string())?;
            }
            suppress_watcher_path(&path,15);
            if let Err(error)=std::fs::remove_file(&path){
                log::warn!("TCloud: exclusão remota ainda não pôde ser refletida em {}: {}",path.display(),error);
            }
        }
    }
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let _=populate_inner(app,&conn,100_000)?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn reconcile_remote_index(_app:&AppHandle,_db:&DbConnection)->Result<(),String>{Ok(())}

#[cfg(target_os = "windows")]
pub(crate) fn finalize_local_upload(_app: &AppHandle, db: &DbConnection, file_id: i64, message_id: i32, mark_in_sync: bool) -> Result<(), String> {
    let root = cfapi_root()?;
    let (name,size,chat_key,updated_at,stored_path,known_relative) = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        let mut q = conn.prepare(
            "SELECT f.name,f.size,COALESCE(f.telegram_chat_id,0),f.updated_at,f.path,c.relative_path
             FROM tcloud_files f LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id WHERE f.id=?1 LIMIT 1"
        ).map_err(|e| e.to_string())?;
        q.bind((1,file_id)).map_err(|e| e.to_string())?;
        if !matches!(q.next().map_err(|e| e.to_string())?,sqlite::State::Row){return Err(format!("Arquivo TCloud {file_id} não encontrado após upload."));}
        (
            q.read::<String,_>(0).unwrap_or_else(|_| "arquivo".into()),
            q.read::<i64,_>(1).unwrap_or(0),
            q.read::<i64,_>(2).unwrap_or(0),
            q.read::<i64,_>(3).unwrap_or_else(|_| chrono::Utc::now().timestamp()),
            q.read::<String,_>(4).unwrap_or_default(),
            q.read::<Option<String>,_>(5).unwrap_or(None),
        )
    };
    let identity = CfapiFileIdentity { version:1,file_id,chat_key,message_id:i64::from(message_id),size,updated_at };
    let identity_json = serde_json::to_string(&identity).map_err(|e|e.to_string())?;
    let target = if let Some(relative)=known_relative.as_ref(){ root.join(relative) } else { PathBuf::from(&stored_path) };
    if !target.exists(){return Ok(());}
    if !target.starts_with(&root){return Ok(());}
    suppress_watcher_path(&target,15);
    if known_relative.is_some(){
        platform::update_placeholder(&target,size,updated_at,identity_json.as_bytes())?;
    }else{
        platform::convert_existing_file(&target,identity_json.as_bytes())?;
    }
    if mark_in_sync { let _=platform::mark_item_in_sync(&target); }
    let relative=relative_string(&root,&target);
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        apply_saved_policy_to_target(&conn,&relative,&target);
    }
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let mut up=conn.prepare(
        "INSERT INTO tcloud_cfapi_items(file_id,relative_path,identity_json,created_at,updated_at)
         VALUES(?1,?2,?3,?4,?4)
         ON CONFLICT(file_id) DO UPDATE SET relative_path=excluded.relative_path,identity_json=excluded.identity_json,updated_at=excluded.updated_at"
    ).map_err(|e|e.to_string())?;
    up.bind((1,file_id)).map_err(|e|e.to_string())?;
    up.bind((2,relative.as_str())).map_err(|e|e.to_string())?;
    up.bind((3,identity_json.as_str())).map_err(|e|e.to_string())?;
    up.bind((4,now)).map_err(|e|e.to_string())?;
    up.next().map_err(|e|e.to_string())?;
    log::info!("TCloud Explorer: {} sincronizado com Telegram (mensagem {}).",name,message_id);
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn finalize_local_upload(_app:&AppHandle,_db:&DbConnection,_file_id:i64,_message_id:i32,_mark_in_sync:bool)->Result<(),String>{Ok(())}

#[tauri::command]
pub async fn tcloud_cfapi_refresh_folders(app: AppHandle) -> Result<CfapiFolderNamespaceResult, String> {
    refresh_folder_namespace_now(app).await
}

#[tauri::command]
pub fn tcloud_cfapi_populate(app: AppHandle, db: State<'_, DbConnection>, limit: Option<i64>) -> Result<CfapiPopulateResult, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    populate_inner(&app, &conn, limit.unwrap_or(10_000))
}

#[tauri::command]
pub fn tcloud_cfapi_finalize_explorer(app: AppHandle, db: State<'_, DbConnection>) -> Result<CfapiStatus, String> {
    let root=cfapi_root()?;
    let marker=marker_path(&app)?;
    if !marker.exists(){return Err("Ative a CFAPI antes de finalizar a integração do Explorer.".into());}
    if platform::platform_info().is_none(){return Err("A plataforma CFAPI não está disponível neste Windows.".into());}

    let before=shell_integration::query(&root);
    if before.registered && before.pinning_enabled{
        #[cfg(target_os="windows")]
        sync_explorer_pin_verbs(&root,true);
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        return status_inner(&app,Some(&conn));
    }

    // Prefer an in-place upgrade. If Windows accepts the modern registration while
    // the legacy root is still present, no placeholder is removed or converted.
    match shell_integration::register_full(&root) {
        Ok(_) => {
            let _ = ensure_provider_connection(&app, &root, marker.exists());
            #[cfg(target_os="windows")]
            sync_explorer_pin_verbs(&root,true);
            let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
            let _=populate_inner(&app,&conn,100_000);
            let shell=shell_integration::query(&root);
            let value=serde_json::json!({
                "version":env!("CARGO_PKG_VERSION"),"root":root,"phase":7,"final_cfapi":true,"shell_target":true,
                "shell_registered":shell.registered,"shell_pinning_enabled":shell.pinning_enabled,
                "shell_package_identity":shell.package_identity,"package_family_name":shell.package_family_name,
                "shell_in_place_repaired_at":chrono::Utc::now().timestamp()
            });
            let _=std::fs::write(&marker,serde_json::to_vec_pretty(&value).map_err(|e|e.to_string())?);
            return status_inner(&app,Some(&conn));
        }
        Err(error) => {
            log::warn!("Upgrade Shell em lugar não foi aceito; iniciando migração controlada com reparo imediato: {}",error);
        }
    }

    platform::disconnect()?;
    // StorageProviderSyncRootManager is the authoritative registration for Explorer.
    // A legacy CfRegisterSyncRoot may still exist, so try both cleanup paths.
    let _=shell_integration::unregister(&root);
    let _=platform::unregister(&root);

    if let Err(shell_error)=shell_integration::register_full(&root){
        // Roll back to the known-working legacy registration so a Shell migration failure
        // never leaves the user's cloud folder unusable.
        let rollback=platform::register(&root,false)
            .and_then(|_|platform::connect(&root,app.clone()));
        let rollback_text=match rollback{Ok(())=>"O registro CFAPI legado foi restaurado e o provider reconectado.".to_string(),Err(e)=>format!("A restauração do registro legado também falhou: {e}")};
        let fallback=install_explorer_pin_verbs(&root).map(|_|" O menu compatível do TCloud foi instalado no Explorer como fallback.").unwrap_or("");
        return Err(format!("Não foi possível registrar o TCloud no Shell moderno do Explorer: {shell_error} {rollback_text}{fallback}"));
    }

    platform::connect(&root,app.clone())?;
    #[cfg(target_os="windows")]
    {
        cleanup_cfapi_temp(&app);
        start_pending_worker(app.clone());
        start_provider_health_worker(app.clone());
        start_pin_hydration_worker(app.clone(),root.clone());
        start_namespace_integrity_worker(app.clone());
        sync_explorer_pin_verbs(&root,true);
    }

    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let repaired=populate_inner(&app,&conn,100_000)?;
    let shell=shell_integration::query(&root);
    let value=serde_json::json!({
        "version":env!("CARGO_PKG_VERSION"),"root":root,"phase":6,"final_cfapi":true,"shell_target":true,
        "shell_registered":shell.registered,"shell_pinning_enabled":shell.pinning_enabled,
        "shell_package_identity":shell.package_identity,"package_family_name":shell.package_family_name,
        "shell_migrated_at":chrono::Utc::now().timestamp(),"repair_created":repaired.created,"repair_existing":repaired.existing,"repair_skipped":repaired.skipped
    });
    std::fs::write(&marker,serde_json::to_vec_pretty(&value).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    status_inner(&app,Some(&conn))
}

#[tauri::command]
pub fn tcloud_cfapi_integrity_check(app:AppHandle,db:State<'_,DbConnection>,repair:Option<bool>)->Result<CfapiIntegrityResult,String>{
    let root=cfapi_root()?;
    let marker=marker_path(&app)?;
    if !marker.exists(){return Err("Conecte este computador ao TCloud antes de verificar a integração do Explorer.".into());}
    let _=ensure_provider_connection(&app,&root,true)?;
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    integrity_check_inner(&app,&conn,repair.unwrap_or(true))
}

#[tauri::command]
pub fn tcloud_cfapi_set_item_availability(
    app: AppHandle,
    db: State<'_, DbConnection>,
    message_id: i64,
    folder_id: Option<i64>,
    offline: bool,
) -> Result<CfapiAvailabilityResult, String> {
    let root = cfapi_root()?;
    let marker = marker_path(&app)?;
    if !marker.exists() {
        return Err("Conecte este computador ao TCloud antes de alterar a disponibilidade do arquivo.".into());
    }

    let _ = ensure_provider_connection(&app, &root, marker.exists())?;
    let chat_key = folder_id.unwrap_or(0);

    fn find_item(conn: &sqlite::Connection, message_id: i64, chat_key: i64) -> Result<Option<(i64, String)>, String> {
        let mut q = conn.prepare(
            "SELECT f.id,c.relative_path FROM tcloud_files f \n             JOIN tcloud_cfapi_items c ON c.file_id=f.id \n             WHERE f.telegram_message_id=?1 AND COALESCE(f.telegram_chat_id,0)=?2 \n               AND f.sync_status NOT IN ('deleted','trashed') \n             ORDER BY f.id DESC LIMIT 1"
        ).map_err(|e| e.to_string())?;
        q.bind((1, message_id)).map_err(|e| e.to_string())?;
        q.bind((2, chat_key)).map_err(|e| e.to_string())?;
        if matches!(q.next().map_err(|e| e.to_string())?, sqlite::State::Row) {
            Ok(Some((
                q.read::<i64,_>(0).map_err(|e| e.to_string())?,
                q.read::<String,_>(1).map_err(|e| e.to_string())?,
            )))
        } else {
            Ok(None)
        }
    }

    let mut item = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        ensure_schema(&conn)?;
        find_item(&conn, message_id, chat_key)?
    };

    if item.is_none() {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let _ = populate_inner(&app, &conn, 100_000)?;
        item = find_item(&conn, message_id, chat_key)?;
    }

    let (file_id, relative) = item.ok_or_else(||
        "Este arquivo ainda não está pronto no Explorer. Sincronize a pasta e tente novamente.".to_string()
    )?;
    let target = root.join(relative.replace('/', "\\"));
    if !target.exists() {
        return Err("O arquivo ainda não foi projetado na pasta TCloud do Explorer.".into());
    }

    suppress_watcher_path(&target, 20);
    platform::set_item_availability(&target, offline)?;

    {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let local_status = if offline { "cached" } else { "cloud_only" };
        let mut update = conn.prepare("UPDATE tcloud_files SET local_status=?1,updated_at=?2 WHERE id=?3")
            .map_err(|e| e.to_string())?;
        update.bind((1, local_status)).map_err(|e| e.to_string())?;
        update.bind((2, chrono::Utc::now().timestamp())).map_err(|e| e.to_string())?;
        update.bind((3, file_id)).map_err(|e| e.to_string())?;
        update.next().map_err(|e| e.to_string())?;
        if offline {
            let mut pin = conn.prepare("INSERT OR REPLACE INTO tcloud_offline_pins(file_id,pinned_at) VALUES(?1,?2)")
                .map_err(|e| e.to_string())?;
            pin.bind((1, file_id)).map_err(|e| e.to_string())?;
            pin.bind((2, chrono::Utc::now().timestamp())).map_err(|e| e.to_string())?;
            pin.next().map_err(|e| e.to_string())?;
        } else {
            let mut pin = conn.prepare("DELETE FROM tcloud_offline_pins WHERE file_id=?1")
                .map_err(|e| e.to_string())?;
            pin.bind((1, file_id)).map_err(|e| e.to_string())?;
            pin.next().map_err(|e| e.to_string())?;
        }
    }

    Ok(CfapiAvailabilityResult {
        action: if offline { "keep_on_device".into() } else { "free_space".into() },
        accepted: true,
        root_path: target.to_string_lossy().to_string(),
        detail: if offline {
            "Arquivo disponível neste computador e mantido pelo Windows.".into()
        } else {
            "Espaço liberado. O arquivo continua visível e será baixado novamente quando necessário.".into()
        },
    })
}

#[tauri::command]
pub fn tcloud_cfapi_keep_on_device(app: AppHandle) -> Result<CfapiAvailabilityResult, String> {
    let root = cfapi_root()?;
    let marker = marker_path(&app)?;
    if !marker.exists() { return Err("Conecte este computador ao TCloud antes de alterar a disponibilidade dos arquivos.".into()); }
    if platform::platform_info().is_none() { return Err("Este Windows não oferece a integração de arquivos sob demanda necessária pelo TCloud.".into()); }
    platform::set_root_pin_state(&root, true)?;
    Ok(CfapiAvailabilityResult {
        action: "keep_on_device".into(),
        accepted: true,
        root_path: root.to_string_lossy().into_owned(),
        detail: "O Windows recebeu o estado PINNED recursivo. Os placeholders serão hidratados pelo provider TCloud e permanecerão protegidos contra desidratação enquanto estiverem fixados.".into(),
    })
}

#[tauri::command]
pub fn tcloud_cfapi_free_space(app: AppHandle) -> Result<CfapiAvailabilityResult, String> {
    let root = cfapi_root()?;
    let marker = marker_path(&app)?;
    if !marker.exists() { return Err("Conecte este computador ao TCloud antes de alterar a disponibilidade dos arquivos.".into()); }
    if platform::platform_info().is_none() { return Err("Este Windows não oferece a integração de arquivos sob demanda necessária pelo TCloud.".into()); }
    platform::set_root_pin_state(&root, false)?;
    Ok(CfapiAvailabilityResult {
        action: "free_space".into(),
        accepted: true,
        root_path: root.to_string_lossy().into_owned(),
        detail: "O Windows recebeu o estado UNPINNED recursivo. Como o Sync Root permite auto-desidratação, arquivos em sincronia podem voltar ao estado somente online e liberar espaço local de forma assíncrona.".into(),
    })
}

#[tauri::command]
pub fn tcloud_cfapi_open_root() -> Result<String, String> {
    let root = cfapi_root()?;
    #[cfg(target_os="windows")]
    {
        std::process::Command::new("explorer.exe").arg(&root).spawn().map_err(|e|e.to_string())?;
    }
    Ok(root.to_string_lossy().into_owned())
}
