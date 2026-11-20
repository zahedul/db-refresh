//! Starting, cancelling and dismissing downloads. Jobs are identified by their S3 key.

use std::time::Instant;

use dioxus::core::spawn_forever;
use dioxus::prelude::*;

use crate::aws::download::{DownloadProgress, DownloadRequest, download_dump_file};
use crate::settings::Settings;
use crate::ui::state::{DownloadJob, DownloadStatus};

pub type DownloadJobs = Signal<Vec<DownloadJob>>;

/// Downloads `request.key` to `request.destination`; `file_name` is the local name
/// (which may differ from the S3 name) and is remembered once the download completes.
pub fn start_download(
    mut downloads: DownloadJobs,
    settings: Signal<Settings>,
    request: DownloadRequest,
    file_name: String,
) {
    let key = request.key.clone();
    let job = DownloadJob {
        request: request.clone(),
        file_name: file_name.clone(),
        task: None,
        status: DownloadStatus::Running {
            progress: DownloadProgress::NOT_STARTED,
            started_at: Instant::now(),
        },
    };
    {
        let mut jobs = downloads.write();
        jobs.retain(|existing| existing.request.key != key);
        jobs.insert(0, job);
    }

    // `spawn_forever` ties the download to the app, not to the component that
    // started it, so a new search (which rebuilds the results table) won't kill it.
    let task_key = key.clone();
    let task = spawn_forever(async move {
        let outcome = download_dump_file(&request, |progress| {
            update_job(downloads, &task_key, |job| {
                if let DownloadStatus::Running {
                    progress: current, ..
                } = &mut job.status
                {
                    *current = progress;
                }
            });
        })
        .await;

        let final_status = match outcome {
            Ok(()) => {
                remember_local_name(settings, &task_key, file_name);
                DownloadStatus::Completed
            }
            Err(error) => DownloadStatus::Failed(error),
        };
        update_job(downloads, &task_key, |job| {
            job.status = final_status;
            job.task = None;
        });
    });

    update_job(downloads, &key, |job| job.task = Some(task));
}

pub fn retry_download(downloads: DownloadJobs, settings: Signal<Settings>, job: &DownloadJob) {
    start_download(
        downloads,
        settings,
        job.request.clone(),
        job.file_name.clone(),
    );
}

fn remember_local_name(mut settings: Signal<Settings>, s3_key: &str, file_name: String) {
    settings.with_mut(|settings| {
        settings
            .downloaded_files
            .insert(s3_key.to_string(), file_name)
    });
}

/// Stopping the task drops the download future, which deletes the `.part` file.
pub fn cancel_download(downloads: DownloadJobs, key: &str) {
    update_job(downloads, key, |job| {
        if let Some(task) = job.task.take() {
            task.cancel();
        }
        job.status = DownloadStatus::Cancelled;
    });
}

pub fn dismiss_download(mut downloads: DownloadJobs, key: &str) {
    downloads
        .write()
        .retain(|job| job.request.key != key || job.is_running());
}

fn update_job(mut downloads: DownloadJobs, key: &str, change: impl FnOnce(&mut DownloadJob)) {
    let mut jobs = downloads.write();
    if let Some(job) = jobs.iter_mut().find(|job| job.request.key == key) {
        change(job);
    }
}
