pub mod models;
pub mod crypto;

/// Minimal startup diagnostics that work even before Tauri/logging is fully ready.
/// `npm run tauri:shell` points this at app/stability-results/startup-latest.log.
fn startup_diag(message: &str) {
    use std::io::Write;
    let path = std::env::var_os("TCLOUD_STARTUP_LOG")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("TCloud").join("logs").join("startup.log"))
        });
    let Some(path) = path else { return; };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let _ = writeln!(file, "[{epoch_ms}] {message}");
        let _ = file.flush();
    }
}

fn install_startup_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        startup_diag(&format!("PANIC: {info}"));
        previous(info);
    }));
}

/// Initialize COM in Multi-Threaded Apartment mode on Windows worker threads.
/// Tauri's main thread uses STA (required for WebView2/DragDrop), so any spawned
/// background threads that touch COM APIs (e.g., Actix, Tokio, networking)
/// must explicitly init COM as MTA to avoid OLE_E_WRONGCOMPOBJ / RPC_E_CHANGED_MODE
/// errors during startup and teardown.
#[cfg(target_os = "windows")]
fn init_com_on_worker_thread() {
    extern "system" {
        fn CoInitializeEx(reserved: *mut std::ffi::c_void, coinit: u32) -> i32;
    }
    const COINIT_MULTITHREADED: u32 = 0x0;
    // HRESULT codes
    const S_OK: i32 = 0;
    const S_FALSE: i32 = 1;
    const RPC_E_CHANGED_MODE: i32 = -2147417850; // 0x80010106

    let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED) };
    match hr {
        S_OK | S_FALSE => {
            log::info!("COM MTA initialized on worker thread (hr=0x{:x})", hr as u32);
        }
        RPC_E_CHANGED_MODE => {
            // Thread was already initialized with a different apartment model.
            // This is non-fatal; the existing mode will be used.
            log::warn!(
                "COM already initialized in a different mode on this worker thread (hr=0x{:x})",
                hr as u32
            );
        }
        _ => {
            log::error!(
                "Failed to initialize COM on worker thread (hr=0x{:x})",
                hr as u32
            );
        }
    }
}

pub mod commands;
pub mod bandwidth;
pub mod vpn_optimizer;
pub mod socks5_bridge;

use tauri::{Emitter, Manager};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};


use tokio::sync::Mutex;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use commands::TelegramState;
use commands::streaming::StreamConfig;
use rand::Rng;


pub mod server;
pub mod api_routes;
pub mod db;
pub mod share_routes;
pub mod transcode;
pub mod fmp4_remux;
pub mod mp4_utils;
pub mod crypto_commands;
pub mod tcloud;
pub mod sync;


/// Single source of truth for the Actix streaming server port.
/// Referenced in lib.rs (server startup) and exposed to the frontend
/// via cmd_get_stream_info so no component ever hardcodes the port.
pub const STREAM_PORT: u16 = 14201;

/// Generate a random 32-character hex token for streaming server auth
fn generate_stream_token() -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.random()).collect();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Holds the Actix-web server stop handle so we can shut it down
/// from the RunEvent::Exit handler for graceful Ctrl+C termination.
pub struct ActixServerHandle(pub Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>>);

/// Tracks whether the API server is currently running (for the frontend status dot)
pub struct ApiServerRunning(pub Arc<std::sync::atomic::AtomicBool>);

/// Holds the API server stop handle separately so we can restart it independently
pub struct ApiServerHandle(pub Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>>);


/// Restart (or stop) the API server based on current settings.
/// Called from Tauri commands when the user changes API settings.
pub fn restart_api_server(app: &tauri::AppHandle) {
    // Stop existing API server if running
    let api_handle_arc = app.state::<ApiServerHandle>().0.clone();
    let old_handle = api_handle_arc.lock().ok().and_then(|mut g| g.take());
    if let Some(handle) = old_handle {
        log::info!("Stopping existing API server...");
        drop(handle.stop(true));
        // Give it a moment to release the port
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let settings = commands::api_settings::load_settings(app);
    let running_flag = app.state::<ApiServerRunning>().0.clone();

    if !settings.enabled {
        running_flag.store(false, std::sync::atomic::Ordering::Relaxed);
        log::info!("API server disabled");
        return;
    }

    // Need TelegramState to share with the API server
    let tg_state = Arc::new(app.state::<TelegramState>().inner().clone());
    let bw_manager = app.state::<Arc<bandwidth::BandwidthManager>>().inner().clone();
    let net_config = app.state::<Arc<vpn_optimizer::NetworkConfig>>().inner().clone();
    let db_pool = app.state::<db::DbConnection>().inner().clone();
    let api_port = settings.port;
    let key_hash = settings.key_hash.clone();
    let handle_for_thread = api_handle_arc.clone();

    // Resolve cache dirs before the thread spawn since app is a reference
    let preview_dir = app.path().app_cache_dir().unwrap_or_default().join("previews");
    let thumbnail_dir = app.path().app_data_dir().unwrap_or_default().join("thumbnails");

    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        init_com_on_worker_thread();
        let sys = actix_rt::System::new();
        sys.block_on(async move {
            let api_state_data = actix_web::web::Data::new(tg_state);
            let api_state = actix_web::web::Data::new(api_routes::ApiState {
                key_hash,
            });
            let cache_dirs = actix_web::web::Data::new(api_routes::CacheDirs {
                thumbnail_dir,
                preview_dir,
            });
            let api_bw = actix_web::web::Data::new(bw_manager);
            let api_net = actix_web::web::Data::new(net_config);
            let api_db = actix_web::web::Data::new(db_pool);

            log::info!("Starting REST API server on port {}", api_port);

            match actix_web::HttpServer::new(move || {
                let cors = actix_cors::Cors::default()
                    .allowed_origin_fn(|origin, _req_head| {
                        let origin_bytes = origin.as_bytes();
                        origin_bytes.starts_with(b"tauri://")
                            || origin_bytes.starts_with(b"http://tauri.localhost")
                            || origin_bytes.starts_with(b"https://tauri.localhost")
                            || origin_bytes.starts_with(b"http://localhost")
                            || origin_bytes.starts_with(b"http://127.0.0.1")
                            || origin_bytes.starts_with(b"https://asset.localhost")
                            || origin_bytes.starts_with(b"http://asset.localhost")
                            || origin_bytes == b"null"
                    })
                    .allow_any_method()
                    .allow_any_header();

                actix_web::App::new()
                    .wrap(cors)
                    .app_data(api_state_data.clone())
                    .app_data(api_state.clone())
                    .app_data(cache_dirs.clone())
                    .app_data(api_bw.clone())
                    .app_data(api_net.clone())
                    .app_data(api_db.clone())
                    .configure(api_routes::configure_api)
            })
            .bind(("127.0.0.1", api_port)) {
                Ok(bound) => {
                    let server = bound.run();
                    *handle_for_thread.lock().unwrap() = Some(server.handle());
                    running_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    log::info!("REST API server started on http://127.0.0.1:{}", api_port);
                    server.await.ok();
                }
                Err(e) => {
                    running_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    log::error!("Failed to start API server on port {}: {}", api_port, e);
                }
            }
        });
    });
}


#[tauri::command]
async fn tcloud_get_attention_center(
    db: tauri::State<'_, db::DbConnection>,
) -> Result<commands::tcloud_sync_center::TCloudAttentionCenter, String> {
    let db = db.inner().clone();
    tokio::task::spawn_blocking(move || {
        commands::tcloud_sync_center::tcloud_attention_center_snapshot(&db)
    })
    .await
    .map_err(|e| format!("attention center worker failed: {e}"))?
}

#[tauri::command]
fn cmd_open_file_externally(path: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app_handle.opener().open_path(&path, None::<&str>).map_err(|e| e.to_string())
}

/// Gather system diagnostics and environment info for debugging.
/// Returns a formatted string suitable for copying to clipboard.
#[tauri::command]
fn cmd_get_system_diagnostics(
    app: tauri::AppHandle,
) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::new();

    lines.push("=== Telegram Drive Diagnostics ===".into());
    lines.push(format!("Package: {}", env!("CARGO_PKG_NAME")));
    lines.push(format!("Version: {}", env!("CARGO_PKG_VERSION")));

    // OS info
    lines.push(format!("OS: {} {}", std::env::consts::OS, std::env::consts::ARCH));

    #[cfg(target_os = "linux")]
    {
        lines.push(format!("XDG_SESSION_TYPE: {}",
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into())));
        lines.push(format!("XDG_CURRENT_DESKTOP: {}",
            std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "unknown".into())));
        lines.push(format!("WEBKIT_DISABLE_DMABUF_RENDERER: {}",
            std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").unwrap_or_else(|_| "unset".into())));
    }

    #[cfg(target_os = "macos")]
    {
        lines.push("Package Type: macOS bundle".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        lines.push("Package Type: Windows installer".to_string());
    }

    // App data dir
    if let Ok(dir) = app.path().app_data_dir() {
        lines.push(format!("App Data: {}", dir.display()));
    }

    // Check for FFmpeg
    #[cfg(unix)]
    {
        let which = std::process::Command::new("which")
            .arg("ffmpeg")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        lines.push(format!("FFmpeg: {}", which.unwrap_or_else(|| "not found".into())));
    }

    lines.push("==================================".into());

    Ok(lines.join("\n"))
}

fn tcloud_tray_status_snapshot(app: &tauri::AppHandle) -> (String, String) {
    let mut running = 0i64;
    let mut pending = 0i64;
    let mut failed = 0i64;
    let mut bytes_total = 0i64;
    let mut bytes_done = 0i64;
    let mut globally_paused = false;

    if let Some(db) = app.try_state::<db::DbConnection>() {
        if let Ok(conn) = db.lock() {
            let scalar = |sql: &str| -> i64 {
                let Ok(mut stmt) = conn.prepare(sql) else { return 0; };
                if matches!(stmt.next(), Ok(sqlite::State::Row)) {
                    stmt.read::<i64, _>(0).unwrap_or(0)
                } else {
                    0
                }
            };
            running = scalar("SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status='running'");
            pending = scalar("SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status='pending' AND COALESCE(paused,0)=0 AND COALESCE(cancel_requested,0)=0");
            failed = scalar("SELECT COUNT(DISTINCT j.file_id) FROM tcloud_sync_jobs j JOIN tcloud_files f ON f.id=j.file_id WHERE j.status='failed' AND COALESCE(f.sync_status,'') NOT IN ('synced','deleted','trashed') AND NOT EXISTS (SELECT 1 FROM tcloud_sync_jobs newer WHERE newer.file_id=j.file_id AND newer.operation=j.operation AND newer.id>j.id)");
            failed += scalar("SELECT COUNT(DISTINCT p.file_id) FROM tcloud_cfapi_pending_ops p WHERE p.completed_at IS NULL AND p.attempts>=12 AND NOT EXISTS (SELECT 1 FROM tcloud_cfapi_pending_ops newer WHERE newer.file_id=p.file_id AND newer.op_type=p.op_type AND newer.id>p.id)");
            bytes_total = scalar("SELECT COALESCE(SUM(bytes_total),0) FROM tcloud_sync_jobs WHERE status='running'");
            bytes_done = scalar("SELECT COALESCE(SUM(bytes_transferred),0) FROM tcloud_sync_jobs WHERE status='running'");
            globally_paused = scalar("SELECT CASE WHEN value='-1' OR CAST(value AS INTEGER)>strftime('%s','now') THEN 1 ELSE 0 END FROM tcloud_settings WHERE key='sync_global_pause_until' LIMIT 1") > 0;
        }
    }

    #[cfg(target_os = "windows")]
    let reconnecting = commands::tcloud_cfapi::provider_expected_fast(app)
        && !commands::tcloud_cfapi::provider_connected_fast();
    #[cfg(not(target_os = "windows"))]
    let reconnecting = false;

    if globally_paused && (running > 0 || pending > 0) {
        return (
            "TCloud — Sincronização pausada".into(),
            "Sincronização — Pausada".into(),
        );
    }
    if failed > 0 {
        return (
            format!("TCloud — {} {} precisam de atenção", failed, if failed == 1 { "item" } else { "itens" }),
            format!("Sincronização — {} {}", failed, if failed == 1 { "problema" } else { "problemas" }),
        );
    }
    if running > 0 {
        let progress = if bytes_total > 0 {
            ((bytes_done.max(0) as f64 / bytes_total.max(1) as f64) * 100.0).round().clamp(0.0,100.0) as i64
        } else { 0 };
        let detail = if progress > 0 {
            format!("{} em andamento · {}%", running, progress)
        } else {
            format!("{} em andamento", running)
        };
        return (
            format!("TCloud — Sincronizando {} {}{}", running, if running == 1 { "arquivo" } else { "arquivos" }, if progress > 0 { format!(" · {}%", progress) } else { String::new() }),
            format!("Sincronização — {}", detail),
        );
    }
    if reconnecting {
        return (
            "TCloud — Reconectando ao Explorer".into(),
            "Sincronização — Reconectando".into(),
        );
    }
    let network_wait = sync::processor::network_cooldown_remaining_secs();
    if pending > 0 && network_wait > 0 {
        return (
            "TCloud — Sem conexão, tentando novamente".into(),
            "Sincronização — Aguardando internet".into(),
        );
    }
    if pending > 0 {
        return (
            format!("TCloud — Preparando {} {}", pending, if pending == 1 { "arquivo" } else { "arquivos" }),
            format!("Sincronização — {} aguardando", pending),
        );
    }
    (
        "TCloud — Tudo atualizado".into(),
        "Sincronização — Tudo atualizado".into(),
    )
}


/// Reveal the existing desktop window with as little work as possible.
///
/// TCloud can spend hours hidden/minimized while the Rust sync/CFAPI workers keep
/// running. When the user returns, showing the native window must win over any
/// frontend refresh or thumbnail work. The frontend receives a resume event and
/// gives the first frame a short grace period before decorative/background work.
fn tcloud_reveal_main_window(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // Showing a hidden window first is noticeably more reliable on Windows than
    // asking a hidden window to unminimize before it has been made visible.
    if window.is_visible().ok() != Some(true) {
        let _ = window.show();
    }
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.set_focus();
    let _ = app.emit("tcloud-window-resumed", ());
}

pub fn run() {
    install_startup_panic_hook();
    startup_diag("run: entered");
    let _ = env_logger::try_init();

    let stream_token = generate_stream_token();

    // Shared handle for stopping the Actix streaming server during shutdown
    let server_handle: Arc<std::sync::Mutex<Option<actix_web::dev::ServerHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let server_handle_for_setup = server_handle.clone();

    let builder = tauri::Builder::default();

    // TCloud 12.7.1 Part 4 — only one desktop process may own the tray,
    // Telegram session and CFAPI callbacks. A second normal launch simply
    // restores the existing window; a --background launch stays silent.
    // This plugin must be registered before every other plugin.
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
        let background = args.iter().any(|arg| arg == "--background");
        if !background {
            tcloud_reveal_main_window(app);
        }
        log::info!("TCloud single-instance: second launch redirected to the running process.");
    }));

    let builder = builder
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Development builds terminate normally so Cargo can replace the exe.
                if cfg!(debug_assertions) {
                    log::info!("Development build closing completely (background tray disabled on X).");
                    return;
                }
                let app_handle = window.app_handle();
                let keep_running = app_handle
                    .try_state::<db::DbConnection>()
                    .map(|db| commands::tcloud_desktop_experience::should_keep_running_on_close(db.inner()))
                    .unwrap_or(true);
                if !keep_running {
                    log::info!("TCloud close preference is 'exit'; allowing process shutdown.");
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                if let Some(db) = app_handle.try_state::<db::DbConnection>() {
                    if matches!(commands::tcloud_desktop_experience::mark_close_notice_seen(db.inner()), Ok(true)) {
                        tauri::async_runtime::spawn_blocking(|| {
                            let _ = commands::tcloud_productivity::tcloud_pro_notify(
                                "TCloud continua sincronizando".into(),
                                "A janela foi fechada, mas seus arquivos continuam sendo sincronizados em segundo plano. Use o ícone do TCloud perto do relógio para abrir novamente ou encerrar.".into(),
                            );
                        });
                    }
                }
                log::info!("TCloud window hidden; background drive service remains active.");
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_updater::Builder::new().build());

    // The updater plugin is always registered. Production builds receive their HTTPS
    // endpoint and public verification key through a generated release-only Tauri config.
    // Development builds intentionally have no endpoint/key; frontend checks handle that
    // state without making startup depend on the update service.

    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::default()
            // TCloud owns hide-to-tray visibility itself. Persisting VISIBLE can save
            // a hidden window during shutdown and fight the next normal launch.
            .with_state_flags(
                tauri_plugin_window_state::StateFlags::all()
                    & !tauri_plugin_window_state::StateFlags::VISIBLE,
            )
            .build(),
    );

    let app = builder
        .setup(move |app| {
            startup_diag("setup: entered");
            app.manage(TelegramState {
                client: Arc::new(Mutex::new(None)),
                login_token: Arc::new(Mutex::new(None)),
                password_token: Arc::new(Mutex::new(None)),
                api_id: Arc::new(Mutex::new(None)),
                runner_shutdown: Arc::new(std::sync::Mutex::new(None)),
                runner_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
                peer_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
                cancelled_transfers: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            });
            app.manage(Arc::new(bandwidth::BandwidthManager::new(app.handle())));
            app.manage(StreamConfig { token: stream_token.clone(), port: STREAM_PORT });

            // Initialize the passphrase-protected persistent production vault.
            // Test-only MemoryVault must never be constructed by the app.
            let crypto_data_dir = app.path().app_data_dir().map_err(|e| {
                log::error!("Failed to get app data dir for encryption vault: {}", e);
                e
            })?;
            let crypto_vault_path = crypto_data_dir.join("encryption").join("vault.v2");
            let crypto_vault = Box::new(crypto::vault::file::FileVault::new(crypto_vault_path));
            let crypto_state = crypto::state::CryptoState::new(crypto_vault);
            let crypto_state_for_timer = crypto_state.clone();
            app.manage(crypto_state);
            let crypto_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(15));
                loop {
                    interval.tick().await;
                    if crypto_state_for_timer.check_auto_lock() {
                        crypto_state_for_timer.lock();
                        let _ = crypto_app_handle.emit("vault-locked", "auto_lock");
                    }
                }
            });

            app.manage(ActixServerHandle(server_handle_for_setup.clone()));
            app.manage(ApiServerHandle(Arc::new(std::sync::Mutex::new(None))));
            app.manage(ApiServerRunning(Arc::new(std::sync::atomic::AtomicBool::new(false))));
            
            // Initialize TranscodeManager for HLS streaming
            let app_data_dir = app.path().app_data_dir().map_err(|e| {
                log::error!("Failed to get app data dir: {}", e);
                e
            })?;
            let cache_root = app_data_dir.join("streaming");
            let transcode_manager = transcode::TranscodeManager::new(cache_root);
            // Detect FFmpeg (non-blocking spawn)
            let app_handle = app.handle().clone();
            let ffmpeg_path_arc = transcode_manager.ffmpeg_path.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(ffmpeg) = transcode::detect_ffmpeg(&app_handle).await {
                    *ffmpeg_path_arc.lock().await = Some(ffmpeg);
                }
            });
            let transcode_arc = Arc::new(transcode_manager);
            app.manage(transcode_arc.clone());
            app.manage(fmp4_remux::Fmp4RemuxState::new());
            let loaded_config = vpn_optimizer::load_network_config(app.handle());
            let net_config = Arc::new(vpn_optimizer::NetworkConfig::new_with_config(loaded_config));
            app.manage(net_config.clone());

            // Auto-start SOCKS5 bridge on startup if HTTP/HTTPS proxy is configured
            {
                let start_config = net_config.clone();
                tauri::async_runtime::spawn(async move {
                    let (enabled, is_http_or_https) = {
                        let proxy = start_config.proxy.read().unwrap();
                        (proxy.enabled, proxy.proxy_type == "http" || proxy.proxy_type == "https")
                    };
                    if enabled && is_http_or_https {
                        if let Err(e) = start_config.start_http_bridge().await {
                            log::error!("Failed to auto-start SOCKS5 bridge on startup: {}", e);
                        }
                    }
                });
            }
            
            // Initialize SQLite Database
            let db_pool = db::init_db(app.handle()).map_err(|e| {
                log::error!("Failed to initialize SQLite database: {}", e);
                e
            })?;
            app.manage(db_pool.clone());
            // Housekeeping can inspect many cache files. Never hold the Tauri setup
            // thread for it: the window should become usable first, then maintenance
            // quietly cleans stale history/old cache in a blocking worker.
            {
                let housekeeping_db = db_pool.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let _ = tokio::task::spawn_blocking(move || {
                        commands::tcloud_desktop_experience::run_startup_housekeeping(&housekeeping_db);
                    }).await;
                });
            }
            startup_diag("setup: database ready");
            app.manage(sync::watcher_runtime::TCloudWatcherState::default());
            // Explorer Logic Stage 4: process durable folder namespace changes
            // generated directly by Windows Explorer (create/rename/move/delete).
            commands::tcloud_forums::start_explorer_namespace_worker(app.handle().clone());

            // TCloud 12.8 Stability Release: remember whether the previous process
            // reached a clean shutdown. After a forced Windows/update termination,
            // validate SQLite/WAL before the CFAPI and queue workers resume.
            match db::mark_runtime_started(&db_pool) {
                Ok(report) if report.previous_unclean_shutdown => {
                    if report.quick_check_ok {
                        log::warn!("TCloud recovered from an interrupted previous session; database quick_check passed.");
                    } else {
                        log::error!("TCloud detected an interrupted previous session and SQLite quick_check needs attention.");
                    }
                }
                Ok(_) => {}
                Err(error) => log::warn!("TCloud stability startup marker could not be updated: {}", error),
            }

            // Keep WAL/planner state healthy during multi-hour background sessions.
            // Maintenance is skipped whenever a transfer is active.
            {
                let maintenance_db = db_pool.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                    loop {
                        match db::run_idle_maintenance(&maintenance_db) {
                            Ok(true) => log::debug!("TCloud idle SQLite maintenance completed."),
                            Ok(false) => {}
                            Err(error) => log::warn!("TCloud idle SQLite maintenance: {}", error),
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(900)).await;
                    }
                });
            }

            // CFAPI final: the provider remains an early background service, but a normal
            // foreground launch must not block Tauri's setup thread on shell registration,
            // cleanup or provider connection. Give the WebView its first paint first.
            // Windows logon/background launches still start the provider immediately.
            {
                let cfapi_app = app.handle().clone();
                let background_start = std::env::args().any(|arg| arg == "--background");
                if background_start {
                    commands::tcloud_cfapi::start_provider_if_registered(&cfapi_app);
                } else {
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(350)).await;
                        let _ = tokio::task::spawn_blocking(move || {
                            commands::tcloud_cfapi::start_provider_if_registered(&cfapi_app);
                        }).await;
                    });
                }
            }

            // Start the durable TCloud queue processor. Jobs remain pending while
            // Telegram is disconnected and are processed after authentication.
            sync::processor::start_processor(app.handle().clone(), db_pool.clone());

            // TCloud 14.18 - backups automaticos pertencem ao backend.
            // A janela React apenas configura e exibe o estado.
            commands::tcloud_productivity::start_backup_scheduler(
                db_pool.clone()
            );

            // TCloud 12.8 Phase 2 — suspension/hibernation detector. Tokio timers
            // resume after Windows wakes, so a large gap between ticks is a reliable
            // low-cost signal that the machine slept. Reset transient network backoff
            // immediately instead of making the first Explorer action wait for an old
            // Wi-Fi failure window to expire.
            {
                let resume_db = db_pool.clone();
                tauri::async_runtime::spawn(async move {
                    let mut last_tick = std::time::Instant::now();
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                        let elapsed = last_tick.elapsed();
                        last_tick = std::time::Instant::now();
                        if elapsed > std::time::Duration::from_secs(75) {
                            log::info!("TCloud: retorno de suspensão/hibernação detectado após {:?}.", elapsed);
                            sync::processor::note_system_resume(&resume_db);
                        }
                    }
                });
            }

            #[cfg(target_os = "windows")]
            commands::tcloud_productivity::start_remote_reconcile_worker(app.handle().clone());

            // TCloud 12.3 — the CFAPI Sync Root is the authoritative local
            // workspace. Bind the watcher to it automatically instead of keeping a
            // second configurable Windows sync folder that can drift from Explorer.
            #[cfg(target_os = "windows")]
            {
                let db_for_watcher=db_pool.clone();
                let app_handle=app.handle().clone();
                tauri::async_runtime::spawn(async move{
                    match commands::tcloud_cfapi::cfapi_root(){
                        Ok(root)=>{
                            if let Ok(conn)=db_for_watcher.lock(){
                                if let Ok(mut local)=conn.prepare("INSERT INTO tcloud_settings(key,value) VALUES('local_root',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value"){
                                    let value=root.to_string_lossy().to_string();let _=local.bind((1,value.as_str()));let _=local.next();
                                }
                                let _=conn.execute("INSERT INTO tcloud_settings(key,value) VALUES('watcher_enabled','1') ON CONFLICT(key) DO UPDATE SET value='1'");
                            }
                            let state=app_handle.state::<sync::watcher_runtime::TCloudWatcherState>();
                            if let Err(error)=state.start(root,db_for_watcher){log::warn!("Failed to auto-start CFAPI Explorer watcher: {}",error);}
                        }
                        Err(error)=>log::warn!("TCloud CFAPI root unavailable for watcher: {}",error),
                    }
                });
            }
            #[cfg(not(target_os = "windows"))]
            {
                let db_for_watcher=db_pool.clone();
                let app_handle=app.handle().clone();
                tauri::async_runtime::spawn(async move{
                    let (root,enabled)={
                        let Ok(conn)=db_for_watcher.lock() else{return;};
                        let read=|key:&str|->Option<String>{let mut stmt=conn.prepare("SELECT value FROM tcloud_settings WHERE key=?1 LIMIT 1").ok()?;stmt.bind((1,key)).ok()?;if matches!(stmt.next().ok()?,sqlite::State::Row){stmt.read::<String,_>(0).ok()}else{None}};
                        let root=read("local_root");let enabled=read("watcher_enabled").map(|v|matches!(v.as_str(),"1"|"true"|"yes"|"on")).unwrap_or(false);(root,enabled)
                    };
                    if enabled{if let Some(root)=root{let state=app_handle.state::<sync::watcher_runtime::TCloudWatcherState>();let _=state.start(std::path::PathBuf::from(root),db_for_watcher);}}
                });
            }

            // Start Streaming Server on a dedicated thread (Actix owns its runtime).
            {
                let state = Arc::new(app.state::<TelegramState>().inner().clone());
                let token_for_server = stream_token.clone();
                let handle_for_thread = server_handle_for_setup.clone();
                let db_pool_for_server = db_pool.clone();
                let transcode_for_server = transcode_arc.clone();
                std::thread::spawn(move || {
                    #[cfg(target_os = "windows")]
                    init_com_on_worker_thread();
                    let sys = actix_rt::System::new();
                    sys.block_on(async move {
                        match server::start_server(state, STREAM_PORT, token_for_server, db_pool_for_server, transcode_for_server).await {
                            Ok(server) => {
                                if let Ok(mut handle) = handle_for_thread.lock() {
                                    *handle = Some(server.handle());
                                }
                                // Now await the server — blocks until stopped
                                server.await.ok();
                            }
                            Err(e) => log::error!("Streaming server failed: {}", e),
                        }
                    });
                });
            }
            // Start API server if enabled in settings
            restart_api_server(app.handle());

            {
                // Keep TCloud running independently from the visible window.

                #[cfg(target_os = "windows")]
                {
                    // Keep an existing startup registration pointing at the current
                    // executable after an update, but respect a user who disabled it.
                    if !cfg!(debug_assertions) {
                        commands::tcloud_windows_startup::repair_startup_if_configured();
                    }
                }

                // Tray menu is the normal way to reopen or fully terminate TCloud.
                let open_item = MenuItem::with_id(
                    app,
                    "tcloud_open",
                    "Abrir TCloud",
                    true,
                    None::<&str>,
                )?;
                let drive_item = MenuItem::with_id(
                    app,
                    "tcloud_open_drive",
                    "Abrir pasta TCloud",
                    true,
                    None::<&str>,
                )?;
                let upload_item = MenuItem::with_id(app, "tcloud_upload", "Upload rápido", true, None::<&str>)?;
                let sync_item = MenuItem::with_id(app, "tcloud_sync", "Sincronização", true, None::<&str>)?;
                let settings_item = MenuItem::with_id(app, "tcloud_settings", "Configurações", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(
                    app,
                    "tcloud_quit",
                    "Encerrar TCloud",
                    true,
                    None::<&str>,
                )?;
                let menu = Menu::with_items(app, &[&open_item, &drive_item, &upload_item, &sync_item, &settings_item, &quit_item])?;

                // Windows does not reliably infer a tray image when TrayIconBuilder is
                // created without `.icon(...)`. Use Tauri's bundled application icon
                // explicitly so the notification area never renders an empty slot.
                let mut tray_builder = TrayIconBuilder::with_id("tcloud-main-tray")
                    .tooltip("TCloud — seus arquivos conectados")
                    .menu(&menu)
                    // Explorer-like behavior: left click opens TCloud, right click opens
                    // the context menu. This also avoids a menu flashing on startup.
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event {
                            let app = tray.app_handle();
                            tcloud_reveal_main_window(app);
                        }
                    })
                    .on_menu_event(|app, event| {
                        match event.id().as_ref() {
                            "tcloud_open" => {
                                tcloud_reveal_main_window(app);
                            }
                            "tcloud_open_drive" => {
                                let _ = commands::tcloud_cfapi::tcloud_cfapi_open_root();
                            }
                            "tcloud_upload" => {
                                tcloud_reveal_main_window(app);
                                let _ = app.emit("tcloud-tray-action", "upload");
                            }
                            "tcloud_sync" => {
                                tcloud_reveal_main_window(app);
                                let _ = app.emit("tcloud-tray-action", "sync");
                            }
                            "tcloud_settings" => {
                                tcloud_reveal_main_window(app);
                                let _ = app.emit("tcloud-tray-action", "settings");
                            }
                            "tcloud_quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    });

                if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                } else {
                    // This should not happen in release builds because icon.ico is bundled,
                    // but keep the tray alive even if a custom package omits the icon.
                    log::warn!("TCloud tray: bundled application icon was not available; using platform fallback.");
                }
                let _tray = tray_builder.build(app)?;
                startup_diag("setup: tray ready");

                // Keep the tray useful even with the main window closed. The tooltip
                // and Sync menu entry mirror the durable queue and CFAPI health, so
                // users can understand the current state without reopening TCloud.
                let tray_status_app = app.handle().clone();
                let tray_sync_item = sync_item.clone();
                tauri::async_runtime::spawn(async move {
                    let mut previous_tooltip = String::new();
                    let mut previous_menu_text = String::new();
                    loop {
                        let (tooltip, menu_text) = tcloud_tray_status_snapshot(&tray_status_app);
                        let active = tooltip.contains("Sincronizando")
                            || tooltip.contains("Reconectando")
                            || tooltip.contains("Sem conexão")
                            || tooltip.contains("atenção");
                        if tooltip != previous_tooltip {
                            if let Some(tray) = tray_status_app.tray_by_id("tcloud-main-tray") {
                                let _ = tray.set_tooltip(Some(tooltip.as_str()));
                            }
                            previous_tooltip = tooltip;
                        }
                        if menu_text != previous_menu_text {
                            let _ = tray_sync_item.set_text(menu_text.as_str());
                            previous_menu_text = menu_text;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(if active { 3 } else { 10 })).await;
                    }
                });

                // Startup via HKCU Run uses --background: no window flashes at login.
                if std::env::args().any(|arg| arg == "--background") {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }

            // Start VPN keep-alive background task.
            {
                let ka_config = net_config.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        let interval = ka_config.keep_alive_interval_sec();
                        if interval == 0 {
                            // Disabled — check again in 10s
                            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                            continue;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(interval as u64)).await;
                        // TCP ping to Telegram DC2 (best-effort)
                        let _ = tauri::async_runtime::spawn_blocking(|| {
                            use std::net::TcpStream;
                            let addr: std::net::SocketAddr = match "149.154.167.50:443".parse() {
                            Ok(a) => a,
                            Err(e) => {
                                log::error!("VPN keep-alive: failed to parse DC2 address: {}", e);
                                return;
                            }
                        };
                        let _ = TcpStream::connect_timeout(
                                &addr,
                                std::time::Duration::from_secs(5),
                            );
                        }).await;
                    }
                });
            }

            startup_diag("setup: completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cmd_auth_app_credentials_status,
            commands::cmd_auth_request_code_simple,
            commands::cmd_auth_qr_login_simple,
            commands::cmd_auth_request_code,
            commands::cmd_auth_sign_in,
            commands::cmd_auth_check_password,
            commands::cmd_get_files,
            commands::cmd_upload_file,
            commands::cmd_validate_upload_destination,
            commands::cmd_validate_dropped_paths,
            commands::initiate_upload,
            commands::cmd_upload_from_url,
            cmd_open_file_externally,
            commands::cmd_has_saved_session,
            commands::cmd_connect_simple,
            commands::cmd_connect,
            commands::cmd_log,
            commands::cmd_delete_file,
            commands::cmd_download_file,
            commands::cmd_move_files,
            commands::cmd_copy_files,
            commands::cmd_create_folder,
            commands::cmd_delete_folder,
            commands::cmd_rename_folder,
            commands::cmd_rename_file,
            commands::cmd_get_bandwidth,
            commands::cmd_delete_preview_for_message,
            commands::cmd_get_preview,
            commands::cmd_clean_preview_cache,
            commands::cmd_logout,
            commands::cmd_scan_folders,
            commands::cmd_search_global,
            commands::cmd_check_connection,
            commands::cmd_is_network_available,
            commands::cmd_test_proxy_traffic,
            commands::cmd_reconnect_with_network_settings,
            commands::cmd_clean_cache,
            commands::cmd_get_thumbnail,
            commands::cmd_get_stream_info,
            commands::cmd_cancel_transfer,
            commands::cmd_auth_qr_login,
            commands::cmd_auth_qr_poll,
            commands::cmd_get_api_settings,
            commands::cmd_update_api_settings,
            commands::cmd_regenerate_api_key,
            commands::cmd_delete_image_thumbnail,
            commands::cmd_zip_folder,
            commands::cmd_delete_temp_zip,
            commands::cmd_apply_proxy_settings,
            commands::cmd_get_proxy_status,
            commands::cmd_apply_vpn_settings,
            commands::cmd_get_network_config,
            commands::cmd_check_latency,
            commands::cmd_detect_vpn,
            commands::cmd_create_share,
            commands::cmd_list_shares,
            commands::cmd_list_share_history,
            commands::cmd_revoke_share,
            commands::cmd_toggle_folder_visibility,
            commands::cmd_export_folder_invite,
            cmd_get_system_diagnostics,
            commands::cmd_get_video_metadata,
            commands::cmd_get_video_metadata_batch,
            transcode::cmd_get_transcode_capabilities,
            transcode::cmd_prepare_transcoded_stream,
            transcode::cmd_get_transcode_status,
            transcode::cmd_cancel_transcode,
            transcode::cmd_get_master_playlist_info,
            transcode::cmd_get_transcode_cache_info,
            transcode::cmd_set_transcode_cache_limit,
            transcode::cmd_get_cached_variants,
            transcode::cmd_get_detailed_transcode_cache,
            transcode::cmd_clear_transcode_cache,
            fmp4_remux::cmd_prepare_fmp4_stream,
            fmp4_remux::cmd_get_fmp4_status,
            commands::cmd_list_archive_contents,
            commands::cmd_extract_archive_entry,
            commands::cmd_get_enriched_folders,
            commands::cmd_update_folder_order,
            commands::cmd_create_group,
            commands::cmd_update_group,
            commands::cmd_delete_group,
            commands::cmd_assign_folder_to_group,
            commands::cmd_update_group_order,
            commands::cmd_get_groups,
            tcloud::tcloud_get_storage_overview,
            tcloud::tcloud_list_folders,
            tcloud::tcloud_create_folder,
            tcloud::tcloud_enqueue_sync,
            commands::tcloud_sync_settings::tcloud_set_sync_folder,
            commands::tcloud_sync_settings::tcloud_set_watcher_enabled,
            commands::tcloud_sync_settings::tcloud_get_sync_config,
            commands::tcloud_sync_settings::tcloud_get_watcher_status,
            commands::tcloud_windows_startup::tcloud_startup_enabled,
            commands::tcloud_windows_startup::tcloud_set_startup_enabled,
            commands::tcloud_conflicts::tcloud_list_conflicts,
            commands::tcloud_conflicts::tcloud_resolve_conflict,
            commands::tcloud_cache::tcloud_request_file,
            commands::tcloud_cache::tcloud_get_cache_info,
            commands::tcloud_cache::tcloud_remove_cached_copy,
            commands::tcloud_download::tcloud_get_download_state,
            commands::tcloud_productivity::tcloud_pro_index_stats,
            commands::tcloud_productivity::tcloud_pro_index_upsert_files,
            commands::tcloud_productivity::tcloud_pro_index_list_files,
            commands::tcloud_productivity::tcloud_pro_mark_index_rebuilt,
            commands::tcloud_productivity::tcloud_pro_trash_add,
            commands::tcloud_productivity::tcloud_pro_trash_list,
            commands::tcloud_productivity::tcloud_pro_trash_restore,
            commands::tcloud_productivity::tcloud_pro_trash_restore_many,
            commands::tcloud_productivity::tcloud_pro_trash_clear,
            commands::tcloud_productivity::tcloud_pro_version_add,
            commands::tcloud_productivity::tcloud_pro_version_list,
            commands::tcloud_productivity::tcloud_pro_rules_list,
            commands::tcloud_productivity::tcloud_pro_rule_save,
            commands::tcloud_productivity::tcloud_pro_rule_delete,
            commands::tcloud_productivity::tcloud_pro_backups_list,
            commands::tcloud_productivity::tcloud_pro_backup_save,
            commands::tcloud_productivity::tcloud_pro_backup_delete,
            commands::tcloud_productivity::tcloud_pro_backup_scan,
            commands::tcloud_productivity::tcloud_pro_backup_files,
            commands::tcloud_productivity::tcloud_pro_backup_mark_file,
            commands::tcloud_productivity::tcloud_pro_backup_mark_run,
            commands::tcloud_productivity::tcloud_pro_offline_pin,
            commands::tcloud_productivity::tcloud_pro_duplicates,
            commands::tcloud_productivity::tcloud_pro_cleanup_stats,
            commands::tcloud_productivity::tcloud_pro_reindex_text_content,
            commands::tcloud_productivity::tcloud_pro_content_search,
            commands::tcloud_productivity::tcloud_pro_activity_add,
            commands::tcloud_productivity::tcloud_pro_activity_list,
            commands::tcloud_productivity::tcloud_pro_pause_all_jobs,
            commands::tcloud_productivity::tcloud_pro_notify,
            commands::tcloud_productivity::tcloud_pro_metadata_get,
            commands::tcloud_productivity::tcloud_pro_metadata_set,
            commands::tcloud_productivity::tcloud_pro_storage_breakdown,
            commands::tcloud_productivity::tcloud_pro_local_duplicate_check,
            commands::tcloud_productivity::tcloud_pro_export_diagnostics,
            commands::tcloud_productivity::tcloud_pro_export_config,
            commands::tcloud_productivity::tcloud_pro_import_config,
            commands::tcloud_completion::tcloud_completion_folder_crypto_get,
            commands::tcloud_completion::tcloud_completion_folder_crypto_set,
            commands::tcloud_completion::tcloud_completion_reindex_content,
            commands::tcloud_completion::tcloud_completion_queue_indexable_downloads,
            commands::tcloud_cfapi::tcloud_cfapi_status,
            commands::tcloud_cfapi::tcloud_cfapi_retry_failed_operations,
            commands::tcloud_cfapi::tcloud_cfapi_set,
            commands::tcloud_cfapi::tcloud_cfapi_populate,
            commands::tcloud_cfapi::tcloud_cfapi_refresh_folders,
            commands::tcloud_cfapi::tcloud_cfapi_finalize_explorer,
            commands::tcloud_cfapi::tcloud_cfapi_integrity_check,
            commands::tcloud_cfapi::tcloud_cfapi_set_item_availability,
            commands::tcloud_cfapi::tcloud_cfapi_keep_on_device,
            commands::tcloud_cfapi::tcloud_cfapi_free_space,
            commands::tcloud_cfapi::tcloud_cfapi_open_root,
            commands::tcloud_completion::tcloud_completion_windows_hello_status,
            commands::tcloud_completion::tcloud_completion_windows_hello_enroll,
            commands::tcloud_completion::tcloud_completion_windows_hello_remove,
            commands::tcloud_completion::tcloud_completion_windows_hello_unlock,
            commands::tcloud_completion::tcloud_completion_music_library,
            commands::tcloud_completion::tcloud_completion_music_playlists,
            commands::tcloud_completion::tcloud_completion_music_playlist_save,
            commands::tcloud_completion::tcloud_completion_music_playlist_delete,
            commands::tcloud_completion::tcloud_completion_cleanup_apply,
            commands::tcloud_completion::tcloud_completion_deep_diagnostics,
            commands::tcloud_completion::tcloud_completion_share_audit,
            commands::tcloud_completion::tcloud_completion_favorites_pull,
            commands::tcloud_completion::tcloud_completion_favorites_push,
            commands::tcloud_completion::tcloud_completion_prepare_rollback,
            commands::tcloud_completion::tcloud_completion_mark_update_healthy,
            commands::tcloud_completion::tcloud_completion_cancel_rollback,
            commands::tcloud_completion::tcloud_completion_rollback_status,
            commands::tcloud_completion::tcloud_completion_force_rollback,
            commands::tcloud_completion::tcloud_completion_update_startup_guard,
            commands::tcloud_completion::tcloud_completion_zip_files,
            commands::tcloud_cache_manager::tcloud_get_cache_status,
            commands::tcloud_cache_manager::tcloud_set_cache_limit,
            commands::tcloud_cache_manager::tcloud_cleanup_cache,
            commands::tcloud_dashboard::tcloud_get_dashboard_stats,
            commands::tcloud_preview::tcloud_prepare_preview,
            commands::tcloud_folder_manager::tcloud_list_folder_tree,
            commands::tcloud_folder_manager::tcloud_create_logical_folder,
            commands::tcloud_folder_manager::tcloud_update_folder_group,
            commands::tcloud_folder_manager::tcloud_delete_logical_folder,
            commands::tcloud_telegram_chats::tcloud_list_telegram_chats,
            commands::tcloud_storage_chat::tcloud_create_storage_chat,
            commands::tcloud_forums::tcloud_forum_ensure_default,
            commands::tcloud_forums::tcloud_forum_create,
            commands::tcloud_forums::tcloud_forum_list,
            commands::tcloud_forums::tcloud_forum_list_tree,
            commands::tcloud_forums::tcloud_forum_list_tree_cached,
            commands::tcloud_forums::tcloud_forum_set_default,
            commands::tcloud_forums::tcloud_forum_rename,
            commands::tcloud_forums::tcloud_forum_delete_empty,
            commands::tcloud_forums::tcloud_forum_create_topic,
            commands::tcloud_forums::tcloud_forum_transfer_topic,
            commands::tcloud_forums::tcloud_forum_list_legacy_channels,
            commands::tcloud_forums::tcloud_forum_migrate_channel,
            commands::tcloud_setup::tcloud_get_setup_state,
            commands::tcloud_setup::tcloud_create_default_folders,
            commands::tcloud_setup::tcloud_reset_setup_state,
            commands::tcloud_sync_center::tcloud_get_sync_center,
            commands::tcloud_sync_center::tcloud_get_sync_overview,
            tcloud_get_attention_center,
            commands::tcloud_sync_center::tcloud_retry_sync_job,
            commands::tcloud_sync_center::tcloud_retry_all_sync_failures,
            commands::tcloud_sync_center::tcloud_cleanup_stale_attention,
            commands::tcloud_sync_center::tcloud_clear_completed_jobs,
            commands::tcloud_desktop_experience::tcloud_get_desktop_preferences,
            commands::tcloud_desktop_experience::tcloud_set_desktop_preferences,
            commands::tcloud_desktop_experience::tcloud_run_health_check,
            commands::tcloud_sync_maturity::tcloud_sync_maturity_list_policies,
            commands::tcloud_sync_maturity::tcloud_sync_maturity_set_policy,
            commands::tcloud_sync_maturity::tcloud_sync_maturity_get_settings,
            commands::tcloud_sync_maturity::tcloud_sync_maturity_set_settings,
            commands::tcloud_sync_maturity::tcloud_sync_maturity_purge_trash,
            commands::tcloud_transfer_progress::tcloud_transfer_start,
            commands::tcloud_transfer_progress::tcloud_transfer_progress,
            commands::tcloud_transfer_progress::tcloud_transfer_finish,
            commands::tcloud_transfer_controls::tcloud_pause_sync_job,
            commands::tcloud_transfer_controls::tcloud_resume_sync_job,
            commands::tcloud_transfer_controls::tcloud_cancel_sync_job,
            commands::tcloud_transfer_controls::tcloud_get_transfer_limits,
            commands::tcloud_transfer_controls::tcloud_set_transfer_limits,
            commands::tcloud_transfer_controls::tcloud_get_global_pause,
            commands::tcloud_transfer_controls::tcloud_set_global_pause,
            commands::tcloud_job_priority::tcloud_set_job_priority,
            commands::tcloud_network_settings::tcloud_get_network_settings,
            commands::tcloud_network_settings::tcloud_set_network_settings,
            crypto_commands::cmd_get_encryption_capabilities,
            crypto_commands::cmd_get_crypto_inventory,
            crypto_commands::cmd_get_encryption_settings,
            crypto_commands::cmd_update_encryption_settings,
            crypto_commands::cmd_create_vault,
            crypto_commands::cmd_unlock_vault,
            crypto_commands::cmd_change_vault_passphrase,
            crypto_commands::cmd_lock_vault,
            crypto_commands::cmd_stage_file_passphrase,
            crypto_commands::cmd_get_vault_status,
            crypto_commands::cmd_export_vault_recovery,
            crypto_commands::cmd_import_vault_recovery,
            crypto_commands::cmd_generate_recovery_key,
            crypto_commands::cmd_get_file_encryption_info,
            crypto_commands::cmd_verify_encrypted_file,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    startup_diag("build: application ready; entering event loop");
    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            startup_diag("run: Exit event received");
            log::info!("Application exiting — shutting down background services...");

            if let Some(crypto_state) = app_handle.try_state::<crypto::state::CryptoState>() {
                crypto_state.lock();
            }

            // Stop CFAPI callbacks before tearing down Telegram/database-backed services.
            commands::tcloud_cfapi::shutdown_provider();

            // 1. Shutdown the grammers network runner
            let shutdown_arc = app_handle.state::<TelegramState>().runner_shutdown.clone();
            let runner_tx = shutdown_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(tx) = runner_tx {
                log::info!("Signaling network runner shutdown...");
                let _ = tx.send(());
            }

            // 2. Stop the Actix streaming server (graceful)
            let server_arc = app_handle.state::<ActixServerHandle>().0.clone();
            let server_handle = server_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(handle) = server_handle {
                log::info!("Stopping Actix streaming server...");
                drop(handle.stop(true));
            }

            // 3. Stop the API server (graceful)
            let api_arc = app_handle.state::<ApiServerHandle>().0.clone();
            let api_handle = api_arc.lock().ok().and_then(|mut g| g.take());
            if let Some(handle) = api_handle {
                log::info!("Stopping API server...");
                drop(handle.stop(true));
            }

            // 5. Stop local SOCKS5 proxy bridge (if running)
            if let Some(net_config) = app_handle.try_state::<Arc<vpn_optimizer::NetworkConfig>>() {
                log::info!("Stopping SOCKS5 bridge...");
                net_config.stop_http_bridge();
            }

            // Mark clean only after the background services were asked to stop. If
            // Windows kills us before this point, the next startup enters recovery.
            if let Some(db_state) = app_handle.try_state::<db::DbConnection>() {
                if let Err(error) = db::mark_runtime_clean_shutdown(db_state.inner()) {
                    log::warn!("TCloud could not record clean shutdown: {}", error);
                }
            }
        }
    });
    startup_diag("run: event loop returned");
}
