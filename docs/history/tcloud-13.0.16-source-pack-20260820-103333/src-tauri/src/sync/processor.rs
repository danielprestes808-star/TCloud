//! TCloud durable synchronization processor.
//!
//! Consumes `tcloud_sync_jobs` and performs Telegram operations using the
//! already authenticated grammers client managed by the application.

use crate::commands::utils::{map_error, resolve_peer};
use crate::db::DbConnection;
use crate::vpn_optimizer::{backoff_ms, NetworkConfig};
use crate::TelegramState;
use crate::sync::cache::{cached_path_for, mark_cached};
use crate::sync::controls::{job_control_state, mark_cancelled, JobControlState};
use crate::sync::progress::{finish_transfer, start_transfer, update_transfer_progress};
use crate::sync::scheduler::{signal_sync_work, wait_for_sync_work};
use crate::sync::throttle::BandwidthThrottle;
use crate::commands::tcloud_network_settings::load_network_settings;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncSeekExt, AsyncWriteExt, ReadBuf};
use grammers_client::InputMessage;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tauri::{AppHandle, Manager};

// TCloud 12.8 — process-wide transient network gate. When Windows changes
// Wi-Fi, wakes from sleep or loses internet, one failed transfer temporarily
// slows every worker instead of letting eight workers hammer Telegram and burn
// through each file's retry budget.
static NETWORK_COOLDOWN_UNTIL: AtomicI64 = AtomicI64::new(0);
static NETWORK_FAILURE_STREAK: AtomicU64 = AtomicU64::new(0);

pub fn network_cooldown_remaining_secs() -> i64 {
    let now = chrono::Utc::now().timestamp();
    (NETWORK_COOLDOWN_UNTIL.load(Ordering::Relaxed) - now).max(0)
}

/// TCloud 12.8 Phase 2 — called by the long-session heartbeat when the process
/// appears to have resumed from sleep/hibernate. A stale network backoff must
/// not delay the first real action after Windows wakes.
pub fn note_system_resume(db: &DbConnection) {
    NETWORK_FAILURE_STREAK.store(0, Ordering::Relaxed);
    NETWORK_COOLDOWN_UNTIL.store(0, Ordering::Relaxed);
    if let Ok(conn) = db.lock() {
        let now = chrono::Utc::now().timestamp();
        let _ = conn.execute(format!(
            "INSERT INTO tcloud_settings(key,value) VALUES('stability_last_resume_at','{now}') \
             ON CONFLICT(key) DO UPDATE SET value='{now}'"
        ));
    }
}

fn is_transient_network_error(error: &str) -> bool {
    let value = error.to_ascii_lowercase();
    [
        "timed out",
        "timeout",
        "connection reset",
        "connection refused",
        "connection aborted",
        "connection closed",
        "broken pipe",
        "read 0 bytes",
        "unexpected eof",
        "temporarily unavailable",
        "network is unreachable",
        "host is unreachable",
        "no route to host",
        "dns",
        "socket",
        "transport error",
        "temporarily unavailable",
        "os error 10051",
        "os error 10053",
        "os error 10054",
        "os error 10060",
        "os error 10061",
        "the semaphore timeout period has expired",
        "network name is no longer available",
    ]
    .iter()
    .any(|needle| value.contains(needle))
}

fn defer_job_for_network(db: &DbConnection, job_id: i64, error: &str) -> Result<u64, String> {
    crate::sync::progress::forget_transfer(job_id);
    let streak = NETWORK_FAILURE_STREAK.fetch_add(1, Ordering::Relaxed).saturating_add(1);
    let delay = match streak {
        1 => 5,
        2 => 10,
        3 => 20,
        4 => 30,
        _ => 60,
    };
    let now = chrono::Utc::now().timestamp();
    NETWORK_COOLDOWN_UNTIL.store(now + delay as i64, Ordering::Relaxed);

    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    let mut stmt = conn
        .prepare(
            "UPDATE tcloud_sync_jobs SET status='pending',error=?1,next_attempt_at=?2,\
             speed_bps=0,eta_seconds=NULL,started_at=NULL,updated_at=?3 WHERE id=?4",
        )
        .map_err(|e| e.to_string())?;
    let friendly = format!("Sem conexão. O TCloud tentará novamente automaticamente. ({error})");
    stmt.bind((1, friendly.as_str())).map_err(|e| e.to_string())?;
    stmt.bind((2, now + delay as i64)).map_err(|e| e.to_string())?;
    stmt.bind((3, now)).map_err(|e| e.to_string())?;
    stmt.bind((4, job_id)).map_err(|e| e.to_string())?;
    stmt.next().map_err(|e| e.to_string())?;
    Ok(delay)
}

struct ThrottledCountingReader<R> {
    inner: R,
    counter: Arc<AtomicU64>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    hasher: Arc<Mutex<Sha256>>,
    limit_bps: u64,
    started_at: std::time::Instant,
    throttle_sleep: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<R> ThrottledCountingReader<R> {
    fn new(
        inner: R,
        counter: Arc<AtomicU64>,
        cancel_flag: Arc<std::sync::atomic::AtomicBool>,
        hasher: Arc<Mutex<Sha256>>,
        limit_bps: u64,
    ) -> Self {
        Self {
            inner,
            counter,
            cancel_flag,
            hasher,
            limit_bps,
            started_at: std::time::Instant::now(),
            throttle_sleep: None,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ThrottledCountingReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        if this.cancel_flag.load(Ordering::Relaxed) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "TCloud transfer cancelled",
            )));
        }

        if this.limit_bps > 0 {
            let transferred = this.counter.load(Ordering::Relaxed);
            let expected = std::time::Duration::from_secs_f64(
                transferred as f64 / this.limit_bps as f64
            );
            let elapsed = this.started_at.elapsed();
            if elapsed < expected {
                let remaining = expected - elapsed;
                if this.throttle_sleep.is_none() {
                    this.throttle_sleep = Some(Box::pin(tokio::time::sleep(remaining)));
                }
                if let Some(sleep) = this.throttle_sleep.as_mut() {
                    if sleep.as_mut().poll(cx).is_pending() {
                        return Poll::Pending;
                    }
                }
                this.throttle_sleep = None;
            } else {
                this.throttle_sleep = None;
            }
        }

        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = &result {
            let after = buf.filled().len();
            let delta = after.saturating_sub(before) as u64;
            if delta > 0 {
                this.counter.fetch_add(delta, Ordering::Relaxed);
                // Hash exactly the bytes that were streamed to Telegram. This
                // gives the committed object an integrity identity without a
                // second disk pass over large files.
                if let Ok(mut hasher) = this.hasher.lock() {
                    hasher.update(&buf.filled()[before..after]);
                }
            }
        }

        result
    }
}

struct CountingReader<R> {
    inner: R,
    counter: Arc<AtomicU64>,
    cancel_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl<R> CountingReader<R> {
    fn new(
        inner: R,
        counter: Arc<AtomicU64>,
        cancel_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self { inner, counter, cancel_flag }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for CountingReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.cancel_flag.load(Ordering::Relaxed) {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "TCloud transfer cancelled",
            )));
        }
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let after = buf.filled().len();
            let delta = after.saturating_sub(before) as u64;
            if delta > 0 {
                this.counter.fetch_add(delta, Ordering::Relaxed);
            }
        }
        result
    }
}

#[derive(Debug, Clone)]
struct PendingJob {
    job_id: i64,
    file_id: i64,
    operation: String,
    path: String,
    telegram_chat_id: Option<i64>,
    telegram_message_id: Option<i32>,
    remote_commit_message_id: Option<i32>,
    remote_commit_fingerprint: Option<String>,
    remote_commit_sha256: Option<String>,
}

pub fn start_processor(app: AppHandle, db: DbConnection) {
    // 12.6.2 Part 7: jobs left as `running` belong to the previous process. A
    // Windows shutdown, update or crash must never leave the queue permanently
    // blocked after the next login. Recover them before any worker can claim work.
    {
        if let Ok(conn)=db.lock() {
            let now=chrono::Utc::now().timestamp();
            let _=conn.execute(&format!(
                "UPDATE tcloud_sync_jobs SET \
                 status=CASE WHEN COALESCE(cancel_requested,0)<>0 THEN 'cancelled' \
                             WHEN COALESCE(paused,0)<>0 THEN 'paused' ELSE 'pending' END, \
                 speed_bps=0,eta_seconds=NULL,started_at=NULL,updated_at={now}, \
                 error=CASE WHEN COALESCE(cancel_requested,0)<>0 THEN error ELSE NULL END \
                 WHERE status='running'"
            ));
        }
    }
    // 12.9.7: repair stale/duplicated work before workers start. A committed
    // Telegram message is more valuable than a newer speculative upload row, so
    // checkpointed jobs are promoted and duplicate non-checkpointed pending jobs
    // are cancelled. Local rows already moved to Trash must never be resurrected
    // by a leftover upload after an unclean shutdown.
    {
        if let Ok(conn)=db.lock() {
            let now=chrono::Utc::now().timestamp();
            let _=conn.execute(format!(
                "UPDATE tcloud_sync_jobs SET status='cancelled',cancel_requested=1,error='Cancelado na recuperação: o arquivo já estava removido ou na Lixeira.',updated_at={now}                  WHERE operation IN ('upload','refresh') AND status IN ('pending','paused') AND remote_commit_message_id IS NULL                    AND file_id IN (SELECT id FROM tcloud_files WHERE sync_status IN ('deleted','trashed'));"
            ));
            let _=conn.execute(format!(
                "UPDATE tcloud_sync_jobs SET status='cancelled',cancel_requested=1,error='Substituído por uma sincronização mais recente durante a recuperação.',updated_at={now}                  WHERE operation IN ('upload','refresh') AND status='pending' AND remote_commit_message_id IS NULL                    AND EXISTS (SELECT 1 FROM tcloud_sync_jobs newer WHERE newer.file_id=tcloud_sync_jobs.file_id                      AND newer.operation IN ('upload','refresh') AND newer.status='pending'                      AND (newer.remote_commit_message_id IS NOT NULL OR newer.id>tcloud_sync_jobs.id));"
            ));
            let _=conn.execute(
                "UPDATE tcloud_sync_jobs SET priority=MAX(COALESCE(priority,0),190),next_attempt_at=NULL                  WHERE status='pending' AND remote_commit_message_id IS NOT NULL"
            );
            // Elimina uploads/refresh antigos substituídos por uma solicitação mais nova.
            let _=conn.execute(format!(
                "UPDATE tcloud_sync_jobs
                 SET status='cancelled',
                     cancel_requested=1,
                     error='Substituído por uma sincronização mais recente.',
                     updated_at={now}
                 WHERE status='pending'
                   AND remote_commit_message_id IS NULL
                   AND operation IN ('upload','refresh')
                   AND EXISTS (
                       SELECT 1
                       FROM tcloud_sync_jobs newer
                       WHERE newer.file_id=tcloud_sync_jobs.file_id
                         AND newer.operation IN ('upload','refresh')
                         AND newer.id>tcloud_sync_jobs.id
                         AND newer.status IN ('pending','running')
                         AND COALESCE(newer.cancel_requested,0)=0
                   );"
            ));

            
            // TCloud sync history retention.
            // Mantém histórico recente para diagnóstico, mas impede que
            // dezenas de milhares de jobs antigos cresçam indefinidamente.
            let history_cutoff = now - (30 * 24 * 60 * 60);

            let _ = conn.execute(format!(
                "DELETE FROM tcloud_sync_jobs
                 WHERE status IN ('completed','cancelled')
                   AND updated_at < {history_cutoff}
                   AND remote_commit_message_id IS NULL
                   AND id NOT IN (
                       SELECT id FROM (
                           SELECT id
                           FROM tcloud_sync_jobs
                           WHERE status IN ('completed','cancelled')
                           ORDER BY id DESC
                           LIMIT 5000
                       )
                   );"
            ));// Não gastar workers com pendências antigas de arquivos já confirmados
            // como sincronizados. Jobs com checkpoint remoto ficam preservados.
            let _=conn.execute(format!(
                "UPDATE tcloud_sync_jobs
                 SET status='cancelled',
                     cancel_requested=1,
                     error='Pendência antiga ignorada: o arquivo já está sincronizado.',
                     updated_at={now}
                 WHERE status='pending'
                   AND remote_commit_message_id IS NULL
                   AND operation IN ('upload','refresh')
                   AND file_id IN (
                       SELECT id
                       FROM tcloud_files
                       WHERE COALESCE(sync_status,'')='synced'
                   );"
            ));
        }
    }

    // 12.3.1: real parallel workers. The scheduler still enforces the user-defined
    // max_concurrent_transfers limit; these workers simply make that limit real
    // instead of serializing every Telegram transfer through one async loop.
    {
        if let Ok(conn)=db.lock() {
            let _=conn.execute("UPDATE tcloud_sync_jobs SET priority=130 WHERE operation='upload' AND status='pending' AND COALESCE(priority,0)<130");
            let _=conn.execute("UPDATE tcloud_sync_jobs SET priority=115 WHERE operation='refresh' AND status='pending' AND COALESCE(priority,0)<115");
            let _=conn.execute("UPDATE tcloud_sync_jobs SET priority=125 WHERE operation IN ('delete','delete_remote') AND status='pending' AND COALESCE(priority,0)<125");
            let _=conn.execute("UPDATE tcloud_sync_jobs SET priority=150 WHERE operation='download' AND status='pending' AND COALESCE(priority,0)<150");
        }
    }
    for worker_id in 0..8u8 {
        let worker_app=app.clone();
        let worker_db=db.clone();
        tauri::async_runtime::spawn(async move {
            // Keep worker 0 responsive while letting the other seven workers sleep
            // progressively longer when the queue is empty. This removes dozens of
            // SQLite polls per second during multi-hour tray sessions without making
            // a newly-created Explorer file feel delayed.
            let mut idle_rounds: u32 = 0;
            loop {
                match process_next(&worker_app,&worker_db).await {
                    Ok(true) => {
                        idle_rounds = 0;
                        // Yield instead of sleeping long enough to make a burst of
                        // Explorer jobs feel serial. claim_next_job enforces the
                        // configured transfer limit atomically under the DB mutex.
                        tokio::task::yield_now().await;
                    }
                    Ok(false) => {
                        idle_rounds = idle_rounds.saturating_add(1).min(12);
                        // Event-driven idle wait: worker 0 remains the safety-net
                        // for retry timestamps/legacy producers; the remaining
                        // workers can sleep much longer with effectively zero idle
                        // SQLite churn, yet signal_sync_work wakes them immediately.
                        let timeout = if worker_id == 0 {
                            Duration::from_millis(900_u64.saturating_add((idle_rounds as u64) * 100).min(1800))
                        } else {
                            Duration::from_secs(6)
                        };
                        wait_for_sync_work(timeout).await;
                    }
                    Err(error) => {
                        idle_rounds = idle_rounds.saturating_add(1).min(12);
                        log::warn!("TCloud sync processor worker {}: {}",worker_id,error);
                        wait_for_sync_work(Duration::from_millis(1200)).await;
                    }
                }
            }
        });
    }
}

fn requeue_after_finalize_failure(db:&DbConnection,job_id:i64,error:&str)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let friendly=format!("Telegram confirmou a operação; finalização local será retomada automaticamente. ({error})");
    let mut stmt=conn.prepare(
        "UPDATE tcloud_sync_jobs SET status='pending',error=?1,next_attempt_at=?2,speed_bps=0,eta_seconds=NULL,updated_at=?3 WHERE id=?4"
    ).map_err(|e|e.to_string())?;
    stmt.bind((1,friendly.as_str())).map_err(|e|e.to_string())?;
    stmt.bind((2,now+2)).map_err(|e|e.to_string())?;
    stmt.bind((3,now)).map_err(|e|e.to_string())?;
    stmt.bind((4,job_id)).map_err(|e|e.to_string())?;
    stmt.next().map_err(|e|e.to_string())?;
    drop(stmt);drop(conn);signal_sync_work();Ok(())
}


fn handle_missing_local_upload(
    db: &DbConnection,
    job: &PendingJob,
    error: &str,
) -> Result<bool, String> {
    if !matches!(job.operation.as_str(), "upload" | "refresh") {
        return Ok(false);
    }

    let normalized = error.to_ascii_lowercase();

    if !normalized.contains("local file not found") {
        return Ok(false);
    }

    let conn = db
        .lock()
        .map_err(|_| "database lock poisoned".to_string())?;

    let mut query = conn
        .prepare(
            "SELECT telegram_message_id, telegram_file_id, sync_status
             FROM tcloud_files
             WHERE id=?1
             LIMIT 1"
        )
        .map_err(|e| e.to_string())?;

    query.bind((1, job.file_id)).map_err(|e| e.to_string())?;

    if !matches!(
        query.next().map_err(|e| e.to_string())?,
        sqlite::State::Row
    ) {
        return Ok(false);
    }

    let telegram_message_id =
        query.read::<Option<i64>, _>(0).unwrap_or(None);

    let telegram_file_id =
        query.read::<Option<String>, _>(1).unwrap_or(None);

    let current_status =
        query.read::<String, _>(2).unwrap_or_default();

    drop(query);

    // Se existe referência remota, não assumimos que o arquivo pode ser
    // descartado. Isso pode representar um placeholder, cache removido ou
    // arquivo do Telegram ainda não hidratado.
    if telegram_message_id.is_some() || telegram_file_id.is_some() {
        return Ok(false);
    }

    // Estados removidos já não precisam de qualquer upload.
    if matches!(
        current_status.as_str(),
        "deleted" | "trashed" | "remote_deleted" | "pending_delete"
    ) {
        let now = chrono::Utc::now().timestamp();

        let mut cancel = conn
            .prepare(
                "UPDATE tcloud_sync_jobs
                 SET status='cancelled',
                     cancel_requested=1,
                     error='Upload obsoleto: arquivo local já removido.',
                     speed_bps=0,
                     eta_seconds=NULL,
                     updated_at=?1
                 WHERE id=?2"
            )
            .map_err(|e| e.to_string())?;

        cancel.bind((1, now)).map_err(|e| e.to_string())?;
        cancel.bind((2, job.job_id)).map_err(|e| e.to_string())?;
        cancel.next().map_err(|e| e.to_string())?;

        return Ok(true);
    }

    // Arquivo criado localmente, nunca enviado ao Telegram e que já não
    // existe mais: encerra o registro obsoleto sem gerar "Precisa de atenção".
    let now = chrono::Utc::now().timestamp();

    let mut cancel = conn
        .prepare(
            "UPDATE tcloud_sync_jobs
             SET status='cancelled',
                 cancel_requested=1,
                 error='Upload cancelado automaticamente: arquivo local não existe mais.',
                 speed_bps=0,
                 eta_seconds=NULL,
                 updated_at=?1
             WHERE id=?2"
        )
        .map_err(|e| e.to_string())?;

    cancel.bind((1, now)).map_err(|e| e.to_string())?;
    cancel.bind((2, job.job_id)).map_err(|e| e.to_string())?;
    cancel.next().map_err(|e| e.to_string())?;

    drop(cancel);

    let mut file = conn
        .prepare(
            "UPDATE tcloud_files
             SET sync_status='deleted',
                 local_status='deleted',
                 updated_at=?1
             WHERE id=?2
               AND telegram_message_id IS NULL
               AND telegram_file_id IS NULL"
        )
        .map_err(|e| e.to_string())?;

    file.bind((1, now)).map_err(|e| e.to_string())?;
    file.bind((2, job.file_id)).map_err(|e| e.to_string())?;
    file.next().map_err(|e| e.to_string())?;

    Ok(true)
}
async fn process_next(app: &AppHandle, db: &DbConnection) -> Result<bool, String> {
    if network_cooldown_remaining_secs() > 0 {
        return Ok(false);
    }

    let telegram = app.state::<TelegramState>();
    let client = {
        telegram.client.lock().await.clone()
    };
    let Some(client) = client else {
        // Do not claim jobs while logged out/offline. They remain pending.
        return Ok(false);
    };

    let Some(job) = claim_next_job(db)? else {
        return Ok(false);
    };

    let result = match job.operation.as_str() {
        "upload" | "refresh" => {
            upload_job(app, &telegram, client, &job).await
        }
        "download" => {
            download_job(&telegram, client, db, &job).await
        }
        "delete" | "delete_remote" => {
            delete_job(&telegram, client, db, &job).await
        }
        unsupported => Err(format!("unsupported TCloud sync operation: {unsupported}")),
    };

    match result {
        Ok(remote_message_id) => {
            if let Err(error)=finish_job(app, db, &job, remote_message_id){
                requeue_after_finalize_failure(db,job.job_id,&error)?;
                return Err(format!("job {} remote commit saved; local finalize deferred: {}",job.job_id,error));
            }
            NETWORK_FAILURE_STREAK.store(0, Ordering::Relaxed);
            NETWORK_COOLDOWN_UNTIL.store(0, Ordering::Relaxed);
        }
        Err(error) => {
            // Pause/cancel are control-flow decisions, not transfer failures.
            // An upload is cooperatively interrupted when paused so it can be
            // restarted cleanly on resume; never feed that interruption into
            // the retry/backoff machinery.
            match job_control_state(db, job.job_id) {
                Ok(JobControlState::Cancelled) => {
                    mark_cancelled(db, job.job_id)?;
                    return Ok(true);
                }
                Ok(JobControlState::Paused) => {
                    return Ok(true);
                }
                _ if is_transient_network_error(&error) => {
                    let delay = defer_job_for_network(db, job.job_id, &error)?;
                    log::warn!(
                        "TCloud network temporarily unavailable; job {} deferred for {}s without consuming a retry: {}",
                        job.job_id, delay, error
                    );
                    return Ok(true);
                }
                _ if handle_missing_local_upload(db, &job, &error)? => {
                    log::info!(
                        "TCloud: upload obsoleto do job {} encerrado automaticamente.",
                        job.job_id
                    );
                    return Ok(true);
                }
                _ => {
                    fail_job(db, job.job_id, &error)?;
                    return Err(format!("job {} failed: {}", job.job_id, error));
                }
            }
        }
    }

    Ok(true)
}

fn claim_next_job(db: &DbConnection) -> Result<Option<PendingJob>, String> {
    let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
    // 12.9.6: one locked decision replaces the old scheduler preflight + claim
    // pair, cutting idle/busy SQLite round-trips while preserving global pause.
    if crate::commands::tcloud_transfer_controls::refresh_global_pause_locked(&conn)?.paused {
        return Ok(None);
    }
    // Enforce the concurrency limit while holding the same DB mutex used to
    // claim a job. This closes the small race between parallel workers checking
    // the scheduler at the same time.
    let max_concurrent={
        let mut q=conn.prepare("SELECT value FROM tcloud_settings WHERE key='max_concurrent_transfers' LIMIT 1").map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<String,_>(0).ok().and_then(|v|v.parse::<i64>().ok()).unwrap_or(8).clamp(1,8)}else{8}
    };
    let running={
        let mut q=conn.prepare("SELECT COUNT(*) FROM tcloud_sync_jobs WHERE status='running'").map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<i64,_>(0).unwrap_or(0)}else{0}
    };
    if running>=max_concurrent{return Ok(None);}
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "SELECT j.id,j.file_id,j.operation,f.path,f.telegram_chat_id,f.telegram_message_id,c.relative_path,
                j.remote_commit_message_id,j.remote_commit_fingerprint,j.remote_commit_sha256
         FROM tcloud_sync_jobs j
         JOIN tcloud_files f ON f.id=j.file_id
         LEFT JOIN tcloud_cfapi_items c ON c.file_id=f.id
         WHERE j.status='pending'
           AND j.paused=0
           AND j.cancel_requested=0
           AND (j.next_attempt_at IS NULL OR j.next_attempt_at <= ?1)
             AND NOT (
                 j.operation IN ('upload','refresh')
                 AND c.file_id IS NOT NULL
                 AND COALESCE(f.local_status,'')='cloud_only'
                 AND f.telegram_message_id IS NOT NULL
             )
         ORDER BY j.priority DESC,j.created_at ASC,j.id ASC LIMIT 1"
    ).map_err(|e|e.to_string())?;
    stmt.bind((1,now)).map_err(|e|e.to_string())?;
    if !matches!(stmt.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(None);}
    let stored_path=stmt.read::<String,_>(3).map_err(|e|e.to_string())?;
    let relative=stmt.read::<Option<String>,_>(6).unwrap_or(None);
    let actual_path=if let Some(relative)=relative{
        crate::commands::tcloud_cfapi::cfapi_root().map(|root|root.join(relative).to_string_lossy().to_string()).unwrap_or(stored_path)
    }else{stored_path};
    let job=PendingJob{
        job_id:stmt.read::<i64,_>(0).map_err(|e|e.to_string())?,
        file_id:stmt.read::<i64,_>(1).map_err(|e|e.to_string())?,
        operation:stmt.read::<String,_>(2).map_err(|e|e.to_string())?,
        path:actual_path,
        telegram_chat_id:stmt.read::<Option<i64>,_>(4).unwrap_or(None),
        telegram_message_id:stmt.read::<Option<i64>,_>(5).unwrap_or(None).and_then(|v|i32::try_from(v).ok()),
        remote_commit_message_id:stmt.read::<Option<i64>,_>(7).unwrap_or(None).and_then(|v|i32::try_from(v).ok()),
        remote_commit_fingerprint:stmt.read::<Option<String>,_>(8).unwrap_or(None),
        remote_commit_sha256:stmt.read::<Option<String>,_>(9).unwrap_or(None),
    };
    drop(stmt);
    let mut update=conn.prepare(
        "UPDATE tcloud_sync_jobs SET status='running',error=NULL,updated_at=?1
         WHERE id=?2 AND status='pending' AND COALESCE(paused,0)=0 AND COALESCE(cancel_requested,0)=0"
    ).map_err(|e|e.to_string())?;
    update.bind((1,now)).map_err(|e|e.to_string())?;
    update.bind((2,job.job_id)).map_err(|e|e.to_string())?;
    update.next().map_err(|e|e.to_string())?;
    drop(update);
    if conn.change_count()==0 {
        return Ok(None);
    }
    Ok(Some(job))
}

async fn resolve_sync_destination(
    client:&grammers_client::Client,
    telegram:&tauri::State<'_,TelegramState>,
    db:&DbConnection,
    chat_key:Option<i64>,
)->Result<(grammers_client::types::Peer,Option<i32>),String>{
    let key=chat_key.unwrap_or(0);
    if key<0{
        let topic=crate::commands::tcloud_forums::topic_target(db,Some(key))?
            .ok_or_else(||format!("Tópico TCloud {key} não encontrado."))?;
        let peer_ref:grammers_session::types::PeerRef=grammers_tl_types::types::InputPeerChannel{
            channel_id:topic.chat_id,access_hash:topic.access_hash,
        }.into();
        let peer=client.resolve_peer(peer_ref).await.map_err(|e|format!("Não foi possível abrir o fórum '{}' para sincronização: {e}",topic.name))?;
        {
            let mut cache=telegram.peer_cache.write().await;
            cache.insert(topic.chat_id,peer.clone());
            cache.insert(key,peer.clone());
        }
        Ok((peer,Some(topic.topic_id)))
    }else{
        let peer=resolve_peer(client,if key==0{None}else{Some(key)},&telegram.peer_cache).await?;
        Ok((peer,None))
    }
}

fn encrypted_remote_file(db:&DbConnection,chat_key:Option<i64>,message_id:Option<i32>)->Result<bool,String>{
    let Some(message_id)=message_id else{return Ok(false);};
    let folder_key=chat_key.filter(|v|*v!=0).map(|v|v.to_string()).unwrap_or_else(||"home".to_string());
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let mut q=conn.prepare("SELECT 1 FROM encrypted_files WHERE folder_key=?1 AND message_id=?2 AND record_state='active' LIMIT 1").map_err(|e|e.to_string())?;
    q.bind((1,folder_key.as_str())).map_err(|e|e.to_string())?;
    q.bind((2,i64::from(message_id))).map_err(|e|e.to_string())?;
    Ok(matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row))
}

fn local_fingerprint(path:&std::path::Path)->Result<String,String>{
    let meta=std::fs::metadata(path).map_err(|e|format!("failed to inspect {}: {e}",path.display()))?;
    let modified_ns=meta.modified().ok()
        .and_then(|value|value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value|value.as_nanos()).unwrap_or(0);
    Ok(format!("{}:{}",meta.len(),modified_ns))
}

async fn wait_until_file_stable(db:&DbConnection,job_id:i64,path:&std::path::Path)->Result<String,String>{
    let mut last:Option<String>=None;
    let mut stable_since:Option<std::time::Instant>=None;
    loop{
        wait_for_job_control(db,job_id).await?;
        if !path.exists() || !path.is_file(){return Err(format!("local file not found: {}",path.display()));}
        let metadata=std::fs::metadata(path).map_err(|e|format!("failed to inspect {}: {e}",path.display()))?;
        let size=metadata.len();
        // Many desktop applications save in bursts: write, pause briefly, then
        // append/finalize. Sampling only twice could mistake that pause for EOF.
        // Require a size-dependent quiet *duration* and also make sure the mtime
        // itself is old enough before opening the file for Telegram.
        let (quiet_ms,sample_ms)=if size<=8*1024*1024{(650u64,150u64)}else if size<=128*1024*1024{(950u64,250u64)}else{(1500u64,400u64)};
        let current=local_fingerprint(path)?;
        if last.as_deref()==Some(current.as_str()){
            let since=stable_since.get_or_insert_with(std::time::Instant::now);
            let modified_quiet=metadata.modified().ok()
                .and_then(|modified|std::time::SystemTime::now().duration_since(modified).ok())
                .map(|age|age>=Duration::from_millis(quiet_ms))
                .unwrap_or(true);
            if since.elapsed()>=Duration::from_millis(quiet_ms) && modified_quiet{return Ok(current);}
        }else{
            last=Some(current);
            stable_since=Some(std::time::Instant::now());
        }
        tokio::time::sleep(Duration::from_millis(sample_ms)).await;
    }
}

fn queue_followup_upload(db:&DbConnection,job:&PendingJob,fingerprint:&str)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let mut file=conn.prepare("UPDATE tcloud_files SET local_fingerprint=?1,sync_status='pending_upload',updated_at=?2 WHERE id=?3").map_err(|e|e.to_string())?;
    file.bind((1,fingerprint)).map_err(|e|e.to_string())?;file.bind((2,now)).map_err(|e|e.to_string())?;file.bind((3,job.file_id)).map_err(|e|e.to_string())?;file.next().map_err(|e|e.to_string())?;
    let mut existing=conn.prepare("SELECT 1 FROM tcloud_sync_jobs WHERE file_id=?1 AND id!=?2 AND operation IN ('upload','refresh') AND status IN ('pending','running') LIMIT 1").map_err(|e|e.to_string())?;
    existing.bind((1,job.file_id)).map_err(|e|e.to_string())?;existing.bind((2,job.job_id)).map_err(|e|e.to_string())?;
    if matches!(existing.next().map_err(|e|e.to_string())?,sqlite::State::Row){return Ok(());}
    let mut enqueue=conn.prepare("INSERT INTO tcloud_sync_jobs(file_id,operation,status,created_at,updated_at,priority,attempts,next_attempt_at) VALUES(?1,'refresh','pending',?2,?2,115,0,NULL)").map_err(|e|e.to_string())?;
    enqueue.bind((1,job.file_id)).map_err(|e|e.to_string())?;enqueue.bind((2,now)).map_err(|e|e.to_string())?;enqueue.next().map_err(|e|e.to_string())?;
    drop(enqueue);
    drop(existing);
    drop(file);
    drop(conn);
    signal_sync_work();
    Ok(())
}

fn upload_sha256_hex(hasher:&Arc<Mutex<Sha256>>)->Result<String,String>{
    let guard=hasher.lock().map_err(|_|"upload hash lock poisoned".to_string())?;
    let digest=guard.clone().finalize();
    Ok(digest.iter().map(|byte|format!("{byte:02x}")).collect::<String>())
}

fn checkpoint_remote_commit(
    db:&DbConnection,
    job_id:i64,
    message_id:i32,
    fingerprint:&str,
    sha256:&str,
)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let mut stmt=conn.prepare(
        "UPDATE tcloud_sync_jobs SET remote_commit_message_id=?1,remote_commit_fingerprint=?2,remote_commit_sha256=?3,remote_commit_at=?4,updated_at=?4 WHERE id=?5"
    ).map_err(|e|e.to_string())?;
    stmt.bind((1,i64::from(message_id))).map_err(|e|e.to_string())?;
    stmt.bind((2,fingerprint)).map_err(|e|e.to_string())?;
    stmt.bind((3,sha256)).map_err(|e|e.to_string())?;
    stmt.bind((4,now)).map_err(|e|e.to_string())?;
    stmt.bind((5,job_id)).map_err(|e|e.to_string())?;
    stmt.next().map_err(|e|e.to_string())?;
    Ok(())
}

fn clear_remote_commit(db:&DbConnection,job_id:i64)->Result<(),String>{
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let mut stmt=conn.prepare(
        "UPDATE tcloud_sync_jobs SET remote_commit_message_id=NULL,remote_commit_fingerprint=NULL,remote_commit_sha256=NULL,remote_commit_at=NULL WHERE id=?1"
    ).map_err(|e|e.to_string())?;
    stmt.bind((1,job_id)).map_err(|e|e.to_string())?;
    stmt.next().map_err(|e|e.to_string())?;
    Ok(())
}

async fn preserve_previous_remote_version(
    client:&grammers_client::Client,
    peer:&grammers_client::types::Peer,
    db:&DbConnection,
    job:&PendingJob,
    new_message_id:i32,
    file_name:&str,
    size:i64,
){
    let Some(old_message_id)=job.telegram_message_id else{return;};
    if old_message_id==new_message_id{return;}
    let retention=crate::commands::tcloud_sync_maturity::version_retention(db);
    if retention>0{
        match crate::commands::tcloud_sync_maturity::record_version_and_collect_prunable(
            db,job.file_id,old_message_id,job.telegram_chat_id,file_name,size,&job.path,retention
        ){
            Ok(prunable)=>{
                for old_version in prunable{
                    match client.delete_messages(peer,&[old_version]).await{
                        Ok(_)=>{let _=crate::commands::tcloud_sync_maturity::forget_version_message(db,job.file_id,old_version);},
                        Err(error)=>{
                            log::warn!("TCloud: versão antiga {} excedeu retenção, limpeza adiada: {}",old_version,error);
                            let _=crate::commands::tcloud_cfapi::queue_remote_cleanup(
                                db,job.file_id,job.telegram_chat_id.unwrap_or(0),old_version,size
                            );
                        }
                    }
                }
            }
            Err(error)=>log::warn!("TCloud: nova versão confirmada, mas o snapshot da anterior será reconciliado depois: {}",error),
        }
    }else if let Err(error)=client.delete_messages(peer,&[old_message_id]).await{
        log::warn!("TCloud: nova versão confirmada, mas a anterior {} será removida depois: {}",old_message_id,error);
        let _=crate::commands::tcloud_cfapi::queue_remote_cleanup(
            db,job.file_id,job.telegram_chat_id.unwrap_or(0),old_message_id,size
        );
    }
}

async fn recover_remote_commit_if_present(
    telegram:&tauri::State<'_,TelegramState>,
    client:&grammers_client::Client,
    db:&DbConnection,
    job:&PendingJob,
)->Result<Option<i32>,String>{
    let Some(message_id)=job.remote_commit_message_id else{return Ok(None);};
    let (peer,_topic)=resolve_sync_destination(client,telegram,db,job.telegram_chat_id).await?;
    let messages=client.get_messages_by_id(&peer,&[message_id]).await
        .map_err(|e|format!("Falha ao validar checkpoint remoto do upload: {}",map_error(e)))?;
    if messages.into_iter().flatten().next().is_none(){
        // The checkpoint points at an object that no longer exists. Clear it and
        // allow the normal upload path to recreate the remote object exactly once.
        clear_remote_commit(db,job.job_id)?;
        log::warn!("TCloud: checkpoint remoto do job {} não existe mais; upload será refeito.",job.job_id);
        return Ok(None);
    }

    let path=PathBuf::from(&job.path);
    let size=std::fs::metadata(&path).map(|m|m.len() as i64).unwrap_or_else(|_|{
        let Ok(conn)=db.lock() else{return 0;};
        let Ok(mut q)=conn.prepare("SELECT size FROM tcloud_files WHERE id=?1 LIMIT 1") else{return 0;};
        if q.bind((1,job.file_id)).is_ok() && matches!(q.next(),Ok(sqlite::State::Row)){q.read::<i64,_>(0).unwrap_or(0)}else{0}
    });
    let file_name=path.file_name().and_then(|v|v.to_str()).unwrap_or("arquivo").to_string();
    preserve_previous_remote_version(client,&peer,db,job,message_id,&file_name,size).await;

    if path.is_file(){
        if let Ok(current)=local_fingerprint(&path){
            if job.remote_commit_fingerprint.as_deref()!=Some(current.as_str()){
                queue_followup_upload(db,job,&current)?;
            }
        }
    }
    let hash_hint=job.remote_commit_sha256.as_deref().and_then(|v|v.get(..12)).unwrap_or("sem-hash");
    log::info!("TCloud: job {} recuperado do checkpoint remoto (mensagem {}, hash {}).",job.job_id,message_id,hash_hint);
    Ok(Some(message_id))
}

async fn upload_job(
    app: &AppHandle,
    telegram: &tauri::State<'_, TelegramState>,
    client: grammers_client::Client,
    job: &PendingJob,
) -> Result<Option<i32>, String> {
    let db=app.state::<DbConnection>();
    // If Telegram already confirmed this upload before a crash/update, recover
    // from the durable checkpoint before touching local bytes again. This is the
    // key guard against duplicate remote files after an unclean shutdown.
    if let Some(message_id)=recover_remote_commit_if_present(telegram,&client,db.inner(),job).await?{
        return Ok(Some(message_id));
    }

    let path = PathBuf::from(&job.path);
    if !path.exists() || !path.is_file() {
        return Err(format!("local file not found: {}", path.display()));
    }

    if encrypted_remote_file(db.inner(),job.telegram_chat_id,job.telegram_message_id)?{
        return Err("Este arquivo está protegido pelo Cofre. A edição direta pelo Explorer ainda não pode substituir o objeto criptografado sem reempacotar o TDENC2.".into());
    }

    // Explorer emits create/modify notifications while a copy is still being
    // written. Wait for a short stable window before opening the file so a large
    // copy can never become a partial Telegram object.
    let uploaded_fingerprint=wait_until_file_stable(db.inner(),job.job_id,&path).await?;
    let file_name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("file")
        .to_string();
    let size = tokio::fs::metadata(&path)
        .await
        .map_err(|e| format!("failed to inspect {}: {e}", path.display()))?
        .len();
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|e| format!("failed to open {}: {e}", path.display()))?;

    wait_for_job_control(db.inner(), job.job_id).await?;
    start_transfer(db.inner(), job.job_id, size as i64)?;

    let counter = Arc::new(AtomicU64::new(0));
    let cancel_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let upload_hasher = Arc::new(Mutex::new(Sha256::new()));
    let network_settings = load_network_settings(&app.state::<DbConnection>())?;
    let upload_limit = if network_settings.economy_mode && network_settings.upload_limit_bps == 0 {
        1024 * 1024
    } else {
        network_settings.upload_limit_bps
    };
    let mut reader = ThrottledCountingReader::new(
        file,
        counter.clone(),
        cancel_flag.clone(),
        upload_hasher.clone(),
        upload_limit,
    );
    let progress_db = app.state::<DbConnection>().inner().clone();
    let progress_job_id = job.job_id;
    let progress_counter = counter.clone();

    let progress_task = tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            match job_control_state(&progress_db, progress_job_id) {
                Ok(JobControlState::Cancelled) => {
                    cancel_flag.store(true, Ordering::Relaxed);
                    let _ = mark_cancelled(&progress_db, progress_job_id);
                    break;
                }
                Ok(JobControlState::Paused) => {
                    // Upload pause is cooperative between chunks. We stop new reads
                    // by signalling cancellation; resume will requeue from the start.
                    cancel_flag.store(true, Ordering::Relaxed);
                    break;
                }
                _ => {}
            }

            let transferred = progress_counter.load(Ordering::Relaxed) as i64;
            let _ = update_transfer_progress(&progress_db, progress_job_id, transferred);
            if transferred >= size as i64 {
                break;
            }
        }
    });

    let uploaded_result = client
        .upload_stream(&mut reader, size as usize, file_name.clone())
        .await;

    progress_task.abort();
    let final_count = counter.load(Ordering::Relaxed) as i64;
    let _ = update_transfer_progress(&app.state::<DbConnection>(), job.job_id, final_count);

    // The byte upload can finish slightly before the Telegram message is committed.
    // Honour pause/cancel here too, so a user cancellation never creates the remote
    // object after the UI already said the operation was stopping.
    wait_for_job_control(db.inner(), job.job_id).await?;
    let uploaded = uploaded_result.map_err(map_error)?;
    let uploaded_sha256=upload_sha256_hex(&upload_hasher)?;
    let (peer,topic_id)=resolve_sync_destination(&client,telegram,db.inner(),job.telegram_chat_id).await?;
    let mut outgoing = InputMessage::new().text("").file(uploaded);
    if let Some(topic_id)=topic_id{outgoing=outgoing.reply_to(Some(topic_id));}

    let network = app.state::<Arc<NetworkConfig>>();
    let max_retries = network.retry_attempts();
    let mut last_error = String::new();

    for attempt in 0..=max_retries {
        wait_for_job_control(db.inner(), job.job_id).await?;
        match client.send_message(&peer, outgoing.clone()).await {
            Ok(message) => {
                // Persist the immutable Telegram message id *before* version
                // cleanup, placeholder updates or UI bookkeeping. If the process
                // stops on the next instruction, startup can verify this message
                // and finalize the same job instead of uploading duplicate bytes.
                checkpoint_remote_commit(
                    db.inner(),job.job_id,message.id(),&uploaded_fingerprint,&uploaded_sha256
                )?;
                preserve_previous_remote_version(
                    &client,&peer,db.inner(),job,message.id(),&file_name,size as i64
                ).await;
                // If the user edited the file again while this upload was in
                // flight, preserve the newly created remote version but immediately
                // queue one more refresh. The current job must not mark newer local
                // bytes as synchronized by accident.
                if let Ok(current_fingerprint)=local_fingerprint(&path){
                    if current_fingerprint!=uploaded_fingerprint{
                        queue_followup_upload(db.inner(),job,&current_fingerprint)?;
                    }
                }
                finish_transfer(db.inner(), job.job_id)?;
                return Ok(Some(message.id()));
            },
            Err(error) => {
                last_error = map_error(error);
                if network.should_respect_flood_wait() && last_error.starts_with("FLOOD_WAIT_") {
                    if let Ok(seconds) = last_error.trim_start_matches("FLOOD_WAIT_").parse::<u64>() {
                        tokio::time::sleep(Duration::from_secs(seconds.min(300))).await;
                        continue;
                    }
                }
                if attempt < max_retries {
                    let wait = backoff_ms(
                        attempt,
                        network.retry_base_backoff_ms(),
                        network.retry_max_backoff_ms(),
                    );
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
            }
        }
    }

    Err(if last_error.is_empty() {
        "Telegram upload failed".to_string()
    } else {
        last_error
    })
}


async fn download_job(
    telegram: &tauri::State<'_, TelegramState>,
    client: grammers_client::Client,
    db: &DbConnection,
    job: &PendingJob,
) -> Result<Option<i32>, String> {
    let Some(message_id) = job.telegram_message_id else {
        return Err("download job has no Telegram message id".into());
    };

    let (peer,_topic_id)=resolve_sync_destination(&client,telegram,db,job.telegram_chat_id).await?;
    let messages = client
        .get_messages_by_id(&peer, &[message_id])
        .await
        .map_err(|e| format!("failed to fetch Telegram message: {}", map_error(e)))?;

    let message = messages
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| "Telegram message not found".to_string())?;

    let media = message
        .media()
        .ok_or_else(|| "Telegram message has no downloadable media".to_string())?;

    let file_name = PathBuf::from(&job.path)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("file")
        .to_string();

    let total = {
        let conn = db.lock().map_err(|_| "database lock poisoned".to_string())?;
        let mut stmt = conn
            .prepare("SELECT size FROM tcloud_files WHERE id = ?1 LIMIT 1")
            .map_err(|e| e.to_string())?;
        stmt.bind((1, job.file_id)).map_err(|e| e.to_string())?;
        match stmt.next().map_err(|e| e.to_string())? {
            sqlite::State::Row => stmt.read::<i64, _>(0).unwrap_or(0),
            _ => 0,
        }
    };

    start_transfer(db, job.job_id, total)?;

    let target = cached_path_for(job.file_id, &file_name)?;
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("failed to create cache directory: {e}"))?;
    }

    let temp = target.with_extension(format!(
        "{}.part",
        target.extension().and_then(|v| v.to_str()).unwrap_or("download")
    ));

    const DOWNLOAD_CHUNK_SIZE: i32 = 512 * 1024;

    let existing = tokio::fs::metadata(&temp)
        .await
        .map(|meta| meta.len() as i64)
        .unwrap_or(0);

    // grammers DownloadIter supports chunk skipping rather than an arbitrary
    // byte offset. Rewind the partial file to a Telegram chunk boundary, then
    // resume from that chunk without duplicating bytes.
    let resume_at = if existing > 0 {
        (existing / DOWNLOAD_CHUNK_SIZE as i64) * DOWNLOAD_CHUNK_SIZE as i64
    } else {
        0
    };

    let mut output = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&temp)
        .await
        .map_err(|e| format!("failed to open cache file: {e}"))?;

    output
        .set_len(resume_at as u64)
        .await
        .map_err(|e| format!("failed to prepare partial download: {e}"))?;
    output
        .seek(std::io::SeekFrom::Start(resume_at as u64))
        .await
        .map_err(|e| format!("failed to seek partial download: {e}"))?;

    let mut downloaded = resume_at;
    update_transfer_progress(db, job.job_id, downloaded)?;

    let network_settings = load_network_settings(db)?;
    let mut download_throttle = BandwidthThrottle::new(
        if network_settings.economy_mode && network_settings.download_limit_bps == 0 {
            2 * 1024 * 1024
        } else {
            network_settings.download_limit_bps
        }
    );
    if resume_at > 0 {
        download_throttle.set_transferred(resume_at as u64);
    }

    // 14.24: arquivos grandes sem limite de banda usam uma janela de
    // quatro requests Telegram em paralelo. ? a mesma concorr?ncia usada
    // internamente pelo grammers para arquivos grandes, mas mantemos aqui
    // o nosso .part, retomada, progresso e controles de pausa/cancelamento.
    const FAST_DOWNLOAD_MIN_SIZE: i64 = 10 * 1024 * 1024;
    const FAST_DOWNLOAD_WORKERS: i64 = 4;

    let fast_download =
        total >= FAST_DOWNLOAD_MIN_SIZE
        && !network_settings.economy_mode
        && network_settings.download_limit_bps == 0;

    if fast_download {
        let chunk_size = DOWNLOAD_CHUNK_SIZE as i64;
        let total_chunks = (total + chunk_size - 1) / chunk_size;
        let mut next_chunk = resume_at / chunk_size;

        while next_chunk < total_chunks {
            if let Err(error) = wait_for_job_control(db, job.job_id).await {
                drop(output);
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(error);
            }

            let end_chunk =
                (next_chunk + FAST_DOWNLOAD_WORKERS).min(total_chunks);

            let mut requests = Vec::new();

            for chunk_index in next_chunk..end_chunk {
                let mut part_stream = client
                    .iter_download(&media)
                    .chunk_size(DOWNLOAD_CHUNK_SIZE)
                    .skip_chunks(chunk_index as i32);

                requests.push(async move {
                    (chunk_index, part_stream.next().await)
                });
            }

            let results = futures::future::join_all(requests).await;

            for (_chunk_index, result) in results {
                let maybe_bytes = result.map_err(|e| map_error(&e))?;
                let bytes = maybe_bytes.ok_or_else(|| {
                    "Telegram encerrou o download antes do tamanho esperado.".to_string()
                })?;

                output
                    .write_all(&bytes)
                    .await
                    .map_err(|e| format!("failed to write cache file: {e}"))?;

                downloaded += bytes.len() as i64;
                update_transfer_progress(db, job.job_id, downloaded)?;
            }

            next_chunk = end_chunk;
        }
    } else {
        let mut stream = client
            .iter_download(&media)
            .chunk_size(DOWNLOAD_CHUNK_SIZE);

        if resume_at > 0 {
            stream =
                stream.skip_chunks((resume_at / DOWNLOAD_CHUNK_SIZE as i64) as i32);
        }

        while let Some(chunk) = stream.next().await.transpose() {
            if let Err(error) = wait_for_job_control(db, job.job_id).await {
                drop(output);
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(error);
            }

            let bytes = chunk.map_err(|e| map_error(&e))?;

            output
                .write_all(&bytes)
                .await
                .map_err(|e| format!("failed to write cache file: {e}"))?;

            downloaded += bytes.len() as i64;
            download_throttle.account(bytes.len() as u64).await;
            update_transfer_progress(db, job.job_id, downloaded)?;
        }
    }

    output
        .flush()
        .await
        .map_err(|e| format!("failed to flush cache file: {e}"))?;
    drop(output);

    tokio::fs::rename(&temp, &target)
        .await
        .map_err(|e| format!("failed to finalize cached file: {e}"))?;

    mark_cached(db, job.file_id, &target)?;
    finish_transfer(db, job.job_id)?;

    Ok(Some(message_id))
}

async fn delete_job(
    telegram: &tauri::State<'_, TelegramState>,
    client: grammers_client::Client,
    db: &DbConnection,
    job: &PendingJob,
) -> Result<Option<i32>, String> {
    let Some(message_id) = job.telegram_message_id else {
        // The local file may have been deleted before its first upload finished.
        return Ok(None);
    };

    let (peer,_topic_id)=resolve_sync_destination(&client,telegram,db,job.telegram_chat_id).await?;
    client
        .delete_messages(&peer, &[message_id])
        .await
        .map_err(map_error)?;
    Ok(None)
}

fn finish_job(app:&AppHandle,db:&DbConnection,job:&PendingJob,remote_message_id:Option<i32>)->Result<(),String>{
    let now=chrono::Utc::now().timestamp();
    {
        let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
        if matches!(job.operation.as_str(),"upload"|"refresh"){
            if let Some(message_id)=remote_message_id{
                let has_followup={
                    let mut q=conn.prepare("SELECT 1 FROM tcloud_sync_jobs WHERE file_id=?1 AND id!=?2 AND operation IN ('upload','refresh') AND status IN ('pending','running') LIMIT 1").map_err(|e|e.to_string())?;
                    q.bind((1,job.file_id)).map_err(|e|e.to_string())?;q.bind((2,job.job_id)).map_err(|e|e.to_string())?;
                    matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row)
                };
                let mut update=conn.prepare(
                    "UPDATE tcloud_files SET telegram_message_id=?1,
                     path=CASE
                         WHEN path LIKE 'telegram:%'
                         THEN 'telegram:' || COALESCE(telegram_chat_id,0) || ':' || ?1
                         ELSE path
                     END,
                     sha256=COALESCE((SELECT remote_commit_sha256 FROM tcloud_sync_jobs WHERE id=?5),sha256),
                     sync_status=CASE WHEN sync_status IN ('trashed','deleted','remote_deleted') THEN sync_status ELSE ?2 END,
                     local_status=CASE WHEN sync_status IN ('trashed','deleted','remote_deleted') THEN local_status ELSE 'local' END,
                     updated_at=?3 WHERE id=?4"
                ).map_err(|e|e.to_string())?;
                update.bind((1,i64::from(message_id))).map_err(|e|e.to_string())?;
                update.bind((2,if has_followup{"pending_upload"}else{"synced"})).map_err(|e|e.to_string())?;
                update.bind((3,now)).map_err(|e|e.to_string())?;
                update.bind((4,job.file_id)).map_err(|e|e.to_string())?;
                update.bind((5,job.job_id)).map_err(|e|e.to_string())?;
                update.next().map_err(|e|e.to_string())?;
            }
        }else if job.operation=="download"{
            let mut update=conn.prepare("UPDATE tcloud_files SET sync_status='synced',local_status='cached',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
            update.bind((1,now)).map_err(|e|e.to_string())?;update.bind((2,job.file_id)).map_err(|e|e.to_string())?;update.next().map_err(|e|e.to_string())?;
        }else if matches!(job.operation.as_str(),"delete"|"delete_remote"){
            let old_message=job.telegram_message_id;
            let old_chat=job.telegram_chat_id.unwrap_or(0);
            let mut update=conn.prepare("UPDATE tcloud_files SET telegram_message_id=NULL,sync_status='deleted',local_status='deleted',updated_at=?1 WHERE id=?2").map_err(|e|e.to_string())?;
            update.bind((1,now)).map_err(|e|e.to_string())?;update.bind((2,job.file_id)).map_err(|e|e.to_string())?;update.next().map_err(|e|e.to_string())?;
            if let Some(mid)=old_message{
                let folder_key=if old_chat==0{"home".to_string()}else{old_chat.to_string()};
                if let Ok(mut enc)=conn.prepare("DELETE FROM encrypted_files WHERE folder_key=?1 AND message_id=?2"){
                    let _=enc.bind((1,folder_key.as_str()));let _=enc.bind((2,i64::from(mid)));let _=enc.next();
                }
            }
        }
        let mut update_job=conn.prepare(
            "UPDATE tcloud_sync_jobs SET status='completed',error=NULL,attempts=0,next_attempt_at=NULL,
             bytes_transferred=CASE WHEN bytes_total>0 THEN bytes_total ELSE bytes_transferred END,
             speed_bps=0,eta_seconds=0,remote_commit_message_id=NULL,remote_commit_fingerprint=NULL,
             remote_commit_sha256=NULL,remote_commit_at=NULL,updated_at=?1 WHERE id=?2"
        ).map_err(|e|e.to_string())?;
        update_job.bind((1,now)).map_err(|e|e.to_string())?;update_job.bind((2,job.job_id)).map_err(|e|e.to_string())?;update_job.next().map_err(|e|e.to_string())?;
    }
    if matches!(job.operation.as_str(),"upload"|"refresh"){
        if let Some(message_id)=remote_message_id{
            let mark_in_sync={
                let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
                let mut q=conn.prepare("SELECT sync_status FROM tcloud_files WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
                q.bind((1,job.file_id)).map_err(|e|e.to_string())?;
                matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row) && q.read::<String,_>(0).unwrap_or_default()=="synced"
            };
            if let Err(error)=crate::commands::tcloud_cfapi::finalize_local_upload(app,db,job.file_id,message_id,mark_in_sync){
                log::warn!("TCloud: upload remoto concluído, mas o placeholder local precisa de reconciliação: {}",error);
            }
        }
    }
    Ok(())
}

fn fail_job(db:&DbConnection,job_id:i64,error:&str)->Result<(),String>{
    // Terminal/retry failures must not leave progress samples resident forever
    // during multi-hour sessions. A retry starts with a fresh sampler anyway.
    crate::sync::progress::forget_transfer(job_id);
    let conn=db.lock().map_err(|_|"database lock poisoned".to_string())?;
    let now=chrono::Utc::now().timestamp();
    let attempts={
        let mut q=conn.prepare("SELECT COALESCE(attempts,0) FROM tcloud_sync_jobs WHERE id=?1 LIMIT 1").map_err(|e|e.to_string())?;
        q.bind((1,job_id)).map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){q.read::<i64,_>(0).unwrap_or(0)+1}else{1}
    };
    let quiet_retries={
        let mut q=conn.prepare("SELECT value FROM tcloud_settings WHERE key='desktop_quiet_retries' LIMIT 1").map_err(|e|e.to_string())?;
        if matches!(q.next().map_err(|e|e.to_string())?,sqlite::State::Row){
            matches!(q.read::<String,_>(0).unwrap_or_else(|_|"1".into()).as_str(),"1"|"true"|"yes"|"on")
        }else{true}
    };
    let terminal=attempts>=if quiet_retries{8}else{3};
    let delay=match attempts{1=>5,2=>15,3=>45,4=>120,5=>300,6=>900,_=>1800};
    let next=now+delay;
    let mut stmt=conn.prepare(
        "UPDATE tcloud_sync_jobs SET status=?1,error=?2,attempts=?3,next_attempt_at=?4,updated_at=?5 WHERE id=?6"
    ).map_err(|e|e.to_string())?;
    stmt.bind((1,if terminal{"failed"}else{"pending"})).map_err(|e|e.to_string())?;
    stmt.bind((2,error)).map_err(|e|e.to_string())?;
    stmt.bind((3,attempts)).map_err(|e|e.to_string())?;
    if terminal{stmt.bind((4,sqlite::Value::Null)).map_err(|e|e.to_string())?;}else{stmt.bind((4,next)).map_err(|e|e.to_string())?;}
    stmt.bind((5,now)).map_err(|e|e.to_string())?;stmt.bind((6,job_id)).map_err(|e|e.to_string())?;stmt.next().map_err(|e|e.to_string())?;
    Ok(())
}

async fn wait_for_job_control(
    db: &DbConnection,
    job_id: i64,
) -> Result<(), String> {
    loop {
        match job_control_state(db, job_id)? {
            JobControlState::Continue => return Ok(()),
            JobControlState::Cancelled => {
                mark_cancelled(db, job_id)?;
                return Err("TCloud transfer cancelled".into());
            }
            JobControlState::Paused => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    }
}


