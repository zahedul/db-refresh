//! Runs manual syncs and one-click "Refresh latest", keeping `AppContext.restore`
//! and its log up to date.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use dioxus::core::{Task, spawn_forever};
use dioxus::prelude::*;

use crate::aws::download::{DownloadProgress, DownloadRequest, download_dump_file};
use crate::aws::s3::DumpFile;
use crate::database::log_line::{LogLevel, LogLine};
use crate::database::restore::{
    RestoreExit, RestorePhase, RestoreRequest, ignored_error_count, phase_started_by, run_restore,
    stop_restore,
};
use crate::database::target::{DatabaseTarget, DumpSource};
use crate::error::AppError;
use crate::format::format_duration;
use crate::local_files::{display_local_path, local_path_for};
use crate::ui::app_context::AppContext;
use crate::ui::downloads::remember_local_name;
use crate::ui::state::{RestoreJob, RestoreResult, RestoreState};

// pg_restore --verbose prints a line per table/index; keep memory bounded on huge dumps.
const MAX_LOG_LINES: usize = 5_000;

/// Everything "Refresh latest" needs, decided before the user confirms.
#[derive(Clone, Debug, PartialEq)]
pub struct RefreshPlan {
    pub target: DatabaseTarget,
    pub source: DumpSource,
    pub latest_dump: DumpFile,
    pub local_file_name: String,
    /// False when the latest dump is already on disk with the right size.
    pub needs_download: bool,
    pub password: Option<String>,
}

/// Restores a dump that is already on disk.
pub fn start_restore(app: AppContext, request: RestoreRequest, job: RestoreJob) {
    let started_at = begin_job(
        app,
        &job,
        RestorePhase::CheckingDump,
        format!(
            "Restoring {} into {} ({})",
            job.dump_name,
            job.database,
            request.target.connection_label()
        ),
    );
    // Tied to the app, not a component: switching tabs must not stop a restore.
    let task = spawn_forever(run_restore_stage(app, request, job, started_at));
    set_task(app.restore, task);
}

/// Downloads the latest dump if needed, then restores it: one click, one job.
pub fn start_refresh(app: AppContext, plan: RefreshPlan) {
    let job = RestoreJob {
        dump_name: plan.local_file_name.clone(),
        target_name: plan.target.name.clone(),
        database: plan.target.database.clone(),
        includes_download: true,
    };
    let first_phase = if plan.needs_download {
        RestorePhase::Downloading
    } else {
        RestorePhase::CheckingDump
    };
    let started_at = begin_job(
        app,
        &job,
        first_phase,
        format!(
            "Refreshing {} with the latest dump from {}",
            job.database,
            plan.source.s3_location()
        ),
    );

    let task = spawn_forever(async move {
        let destination = local_path_for(&plan.local_file_name);
        if plan.needs_download {
            if let Err(error) = download_latest(app, &plan, destination.clone()).await {
                finish_job(
                    app,
                    job,
                    RestoreResult::Failed(error.to_string()),
                    started_at,
                    RestorePhase::Downloading,
                );
                return;
            }
        } else {
            append_log_line(
                app.restore_log,
                LogLine::note(
                    LogLevel::Info,
                    format!(
                        "Latest dump is already on disk: {}",
                        display_local_path(&destination)
                    ),
                ),
            );
        }

        let request = RestoreRequest {
            dump_path: destination,
            target: plan.target,
            password: plan.password,
        };
        run_restore_stage(app, request, job, started_at).await;
    });
    set_task(app.restore, task);
}

/// Stops whatever is running: the restore script (by signal) or the download (by
/// dropping its task, which also deletes the partial file).
pub fn cancel_restore(app: AppContext) {
    let (process_group_id, task) = match &*app.restore.peek() {
        RestoreState::Running {
            process_group_id,
            task,
            ..
        } => (*process_group_id, *task),
        _ => return,
    };

    if let Some(process_group_id) = process_group_id {
        // The script exits, and the running task records the result as Cancelled.
        stop_restore(process_group_id);
        return;
    }
    let Some(task) = task else {
        return;
    };
    task.cancel();

    let cancelled = match &*app.restore.peek() {
        RestoreState::Running {
            job,
            started_at,
            phase,
            ..
        } => Some((job.clone(), *started_at, *phase)),
        _ => None,
    };
    if let Some((job, started_at, phase)) = cancelled {
        finish_job(app, job, RestoreResult::Cancelled, started_at, phase);
    }
}

async fn download_latest(
    app: AppContext,
    plan: &RefreshPlan,
    destination: PathBuf,
) -> Result<(), AppError> {
    append_log_line(
        app.restore_log,
        LogLine::note(
            LogLevel::Info,
            format!(
                "Downloading {} → {}",
                plan.latest_dump.key,
                display_local_path(&destination)
            ),
        ),
    );
    let request = DownloadRequest {
        profile: plan.source.profile.clone(),
        bucket: plan.source.bucket.clone(),
        key: plan.latest_dump.key.clone(),
        destination,
    };
    download_dump_file(&request, |progress| {
        set_download_progress(app.restore, progress);
    })
    .await?;

    remember_local_name(
        app.settings,
        &plan.latest_dump.key,
        plan.local_file_name.clone(),
    );
    append_log_line(
        app.restore_log,
        LogLine::note(
            LogLevel::Success,
            format!("Downloaded {}", plan.local_file_name),
        ),
    );
    Ok(())
}

/// Runs the restore script and records the final result.
async fn run_restore_stage(
    app: AppContext,
    request: RestoreRequest,
    job: RestoreJob,
    started_at: Instant,
) {
    set_phase(app.restore, RestorePhase::CheckingDump);
    let mut reached_phase = RestorePhase::CheckingDump;
    let mut ignored_errors = None;

    let outcome = run_restore(
        &request,
        |process_group_id| set_process_group(app.restore, process_group_id),
        |line| {
            if let Some(phase) = phase_started_by(&line) {
                reached_phase = phase;
                set_phase(app.restore, phase);
            }
            ignored_errors = ignored_errors.or(ignored_error_count(&line));
            append_log_line(app.restore_log, LogLine::from_output(line));
        },
    )
    .await;

    let result = restore_result(outcome, ignored_errors);
    finish_job(app, job, result, started_at, reached_phase);
}

fn begin_job(
    app: AppContext,
    job: &RestoreJob,
    first_phase: RestorePhase,
    first_log_line: String,
) -> Instant {
    let (mut restore, mut restore_log) = (app.restore, app.restore_log);
    restore_log.set(vec![LogLine::note(LogLevel::Info, first_log_line)]);
    let started_at = Instant::now();
    restore.set(RestoreState::Running {
        job: job.clone(),
        started_at,
        phase: first_phase,
        download_progress: None,
        process_group_id: None,
        task: None,
    });
    started_at
}

fn finish_job(
    app: AppContext,
    job: RestoreJob,
    result: RestoreResult,
    started_at: Instant,
    reached_phase: RestorePhase,
) {
    let elapsed = started_at.elapsed();
    append_log_line(app.restore_log, summary_log_line(&result, &job, elapsed));
    let mut restore = app.restore;
    restore.set(RestoreState::Finished {
        job,
        result,
        elapsed,
        reached_phase,
    });
}

fn set_phase(mut restore: Signal<RestoreState>, next_phase: RestorePhase) {
    if let RestoreState::Running {
        phase,
        download_progress,
        ..
    } = &mut *restore.write()
    {
        *phase = next_phase;
        if next_phase != RestorePhase::Downloading {
            *download_progress = None;
        }
    }
}

fn set_download_progress(mut restore: Signal<RestoreState>, progress: DownloadProgress) {
    if let RestoreState::Running {
        download_progress, ..
    } = &mut *restore.write()
    {
        *download_progress = Some(progress);
    }
}

fn set_process_group(mut restore: Signal<RestoreState>, id: u32) {
    if let RestoreState::Running {
        process_group_id, ..
    } = &mut *restore.write()
    {
        *process_group_id = Some(id);
    }
}

fn set_task(mut restore: Signal<RestoreState>, running_task: Task) {
    if let RestoreState::Running { task, .. } = &mut *restore.write() {
        *task = Some(running_task);
    }
}

fn restore_result(
    outcome: Result<RestoreExit, AppError>,
    ignored_error_count: Option<u32>,
) -> RestoreResult {
    match (outcome, ignored_error_count) {
        (Ok(RestoreExit::Succeeded), _) => RestoreResult::Succeeded,
        // pg_restore exits with 1 when it skipped failing items but restored the rest.
        (Ok(RestoreExit::Failed { exit_code: 1 }), Some(count)) => {
            RestoreResult::CompletedWithErrors {
                ignored_error_count: count,
            }
        }
        (Ok(RestoreExit::Failed { exit_code }), _) => RestoreResult::Failed(format!(
            "The restore script stopped with exit code {exit_code}. Filter the log by Errors to see why."
        )),
        (Ok(RestoreExit::Stopped), _) => RestoreResult::Cancelled,
        (Err(error), _) => RestoreResult::Failed(error.to_string()),
    }
}

fn append_log_line(mut restore_log: Signal<Vec<LogLine>>, line: LogLine) {
    let mut lines = restore_log.write();
    lines.push(line);
    trim_log(&mut lines, MAX_LOG_LINES);
}

/// Drops the oldest info lines first, so errors and warnings are never lost from the log.
fn trim_log(lines: &mut Vec<LogLine>, max_lines: usize) {
    while lines.len() > max_lines {
        let oldest_info = lines.iter().position(|line| line.level == LogLevel::Info);
        lines.remove(oldest_info.unwrap_or(0));
    }
}

fn summary_log_line(result: &RestoreResult, job: &RestoreJob, elapsed: Duration) -> LogLine {
    let took = format_duration(elapsed);
    match result {
        RestoreResult::Succeeded => LogLine::note(
            LogLevel::Success,
            format!(
                "✓ {} refreshed from {} in {took}",
                job.database, job.dump_name
            ),
        ),
        RestoreResult::CompletedWithErrors {
            ignored_error_count,
        } => LogLine::note(
            LogLevel::Warning,
            format!(
                "Finished in {took}, but pg_restore skipped {ignored_error_count} item(s) that failed."
            ),
        ),
        RestoreResult::Failed(message) => LogLine::note(LogLevel::Error, format!("✗ {message}")),
        RestoreResult::Cancelled => LogLine::note(
            LogLevel::Warning,
            format!(
                "Cancelled after {took}. {} may be partly restored.",
                job.database
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_one_with_ignored_errors_is_partial_success() {
        let result = restore_result(Ok(RestoreExit::Failed { exit_code: 1 }), Some(8));
        assert_eq!(
            result,
            RestoreResult::CompletedWithErrors {
                ignored_error_count: 8
            }
        );
    }

    #[test]
    fn exit_one_without_summary_is_failure() {
        let result = restore_result(Ok(RestoreExit::Failed { exit_code: 1 }), None);
        assert!(matches!(result, RestoreResult::Failed(_)));
        assert_eq!(
            restore_result(Ok(RestoreExit::Stopped), None),
            RestoreResult::Cancelled
        );
    }

    #[test]
    fn trim_log_keeps_errors_and_warnings() {
        let mut lines = vec![
            LogLine::note(LogLevel::Error, "first error"),
            LogLine::note(LogLevel::Info, "old info"),
            LogLine::note(LogLevel::Warning, "warning"),
            LogLine::note(LogLevel::Info, "new info"),
        ];

        trim_log(&mut lines, 3);

        let texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, vec!["first error", "warning", "new info"]);
    }
}
