//! TCloud Sync Worker
//!
//! Consumes durable synchronization jobs and decides which transport action
//! should execute. The worker is transport-neutral by design.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncJobKind {
    Upload,
    Download,
    DeleteRemote,
    DeleteLocal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncJob {
    pub id: String,
    pub kind: SyncJobKind,
    pub local_path: Option<String>,
    pub remote_file_id: Option<String>,
    pub chat_id: Option<i64>,
    pub folder_id: Option<String>,
    pub attempts: u32,
    pub status: SyncJobStatus,
    pub last_error: Option<String>,
}

impl SyncJob {
    pub fn mark_running(&mut self) {
        self.status = SyncJobStatus::Running;
        self.last_error = None;
    }

    pub fn mark_completed(&mut self) {
        self.status = SyncJobStatus::Completed;
        self.last_error = None;
    }

    pub fn mark_failed(&mut self, message: impl Into<String>) {
        self.attempts = self.attempts.saturating_add(1);
        self.status = SyncJobStatus::Failed;
        self.last_error = Some(message.into());
    }

    pub fn mark_conflict(&mut self, message: impl Into<String>) {
        self.status = SyncJobStatus::Conflict;
        self.last_error = Some(message.into());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerResult {
    pub job_id: String,
    pub success: bool,
    pub retryable: bool,
    pub message: String,
}

/// Generic worker processor. Actual upload/download execution is injected
/// through callbacks so the worker is testable without a Telegram connection.
pub fn process_job<FU, FD>(
    job: &mut SyncJob,
    mut upload: FU,
    mut download: FD,
) -> WorkerResult
where
    FU: FnMut(&SyncJob) -> Result<(), String>,
    FD: FnMut(&SyncJob) -> Result<(), String>,
{
    job.mark_running();

    let result = match job.kind {
        SyncJobKind::Upload => upload(job),
        SyncJobKind::Download => download(job),
        SyncJobKind::DeleteRemote | SyncJobKind::DeleteLocal => {
            Err("delete handlers are not connected yet".to_string())
        }
    };

    match result {
        Ok(()) => {
            job.mark_completed();
            WorkerResult {
                job_id: job.id.clone(),
                success: true,
                retryable: false,
                message: "sync job completed".into(),
            }
        }
        Err(err) => {
            let retryable = job.attempts < 3;
            job.mark_failed(err.clone());
            WorkerResult {
                job_id: job.id.clone(),
                success: false,
                retryable,
                message: err,
            }
        }
    }
}
