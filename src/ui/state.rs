use std::time::{Duration, Instant};

use dioxus::core::Task;

use crate::aws::download::{DownloadProgress, DownloadRequest};
use crate::aws::s3::SearchResult;
use crate::database::restore::RestorePhase;
use crate::error::AppError;

#[derive(Clone, Debug, PartialEq)]
pub enum SearchState {
    Idle,
    Searching { s3_location: String },
    Found(SearchResult),
    Failed(AppError),
}

impl SearchState {
    pub fn is_searching(&self) -> bool {
        matches!(self, Self::Searching { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloadStatus {
    Running {
        progress: DownloadProgress,
        started_at: Instant,
    },
    Completed,
    Cancelled,
    Failed(AppError),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadJob {
    pub request: DownloadRequest,
    pub file_name: String,
    /// Handle of the background task while it runs; used by Cancel.
    pub task: Option<Task>,
    pub status: DownloadStatus,
}

impl DownloadJob {
    pub fn is_running(&self) -> bool {
        matches!(self.status, DownloadStatus::Running { .. })
    }
}

/// What is being restored where, for status messages.
#[derive(Clone, Debug, PartialEq)]
pub struct RestoreJob {
    pub dump_name: String,
    pub target_name: String,
    pub database: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RestoreResult {
    Succeeded,
    /// pg_restore finished but skipped items it could not restore (it still exits with 1).
    CompletedWithErrors {
        ignored_error_count: u32,
    },
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RestoreState {
    Idle,
    Running {
        job: RestoreJob,
        started_at: Instant,
        phase: RestorePhase,
        /// Known once the process has started; needed by Cancel.
        process_group_id: Option<u32>,
    },
    Finished {
        job: RestoreJob,
        result: RestoreResult,
        elapsed: Duration,
        /// The last phase reached, so the step indicator can show where it stopped.
        reached_phase: RestorePhase,
    },
}

impl RestoreState {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }
}
