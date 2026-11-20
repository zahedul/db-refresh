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
    /// "Refresh latest" downloads first; a manual sync starts at the restore script.
    pub includes_download: bool,
    /// The database is dropped and created before restoring.
    pub recreates_database: bool,
}

impl RestoreJob {
    /// Download and Recreate database are optional stages; the rest always run.
    pub fn includes_phase(&self, phase: RestorePhase) -> bool {
        match phase {
            RestorePhase::Downloading => self.includes_download,
            RestorePhase::RecreatingDatabase => self.recreates_database,
            RestorePhase::CheckingDump | RestorePhase::Connecting | RestorePhase::RestoringData => {
                true
            }
        }
    }
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
        /// Live progress while `phase` is `Downloading`.
        download_progress: Option<DownloadProgress>,
        /// Known once the restore script has started; Cancel stops it with a signal.
        process_group_id: Option<u32>,
        /// The background task; Cancel drops it while no process is running yet (download).
        task: Option<Task>,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn job(includes_download: bool, recreates_database: bool) -> RestoreJob {
        RestoreJob {
            dump_name: "gen.dump".to_string(),
            target_name: "General".to_string(),
            database: "gen_db".to_string(),
            includes_download,
            recreates_database,
        }
    }

    #[test]
    fn optional_phases_follow_the_job() {
        let shown = |job: &RestoreJob| {
            RestorePhase::ALL
                .into_iter()
                .filter(|phase| job.includes_phase(*phase))
                .count()
        };
        assert_eq!(shown(&job(false, false)), 3);
        assert_eq!(shown(&job(true, false)), 4);
        assert_eq!(shown(&job(true, true)), 5);
        assert!(!job(true, false).includes_phase(RestorePhase::RecreatingDatabase));
    }
}
