//! TCloud transfer scheduling helpers.

use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Notify;

// 12.9.6: Workers no longer need to hammer SQLite just to discover new work.
// Producers signal this Notify after committing a pending job; workers still use
// a bounded timeout as a safety net for delayed retries and legacy enqueue paths.
static SYNC_WORK_NOTIFY: OnceLock<Notify> = OnceLock::new();

fn sync_work_notify() -> &'static Notify {
    SYNC_WORK_NOTIFY.get_or_init(Notify::new)
}

pub fn signal_sync_work() {
    // Wake every currently sleeping worker so the configured concurrency can be
    // filled immediately. notify_one also stores one permit when no worker has
    // reached the wait yet, avoiding a lost wake during very fast Explorer edits.
    sync_work_notify().notify_waiters();
    sync_work_notify().notify_one();
}

pub async fn wait_for_sync_work(timeout: Duration) {
    tokio::select! {
        _ = sync_work_notify().notified() => {},
        _ = tokio::time::sleep(timeout) => {},
    }
}
