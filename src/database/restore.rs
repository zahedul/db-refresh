//! Runs `scripts/restore-postgres.sh` (compiled into the app) and streams its output.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{env, io};

use dioxus::logger::tracing;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::constants::{
    CONNECTING_MARKER, DATABASE_CONNECT_TIMEOUT_SECONDS, DROPPING_SCHEMAS_MARKER,
    IGNORED_ERRORS_MARKER, MERGE_STDERR_INTO_STDOUT, POSTGRES_TOOL_DIRS, RECREATING_MARKER,
    RESTORE_PARALLEL_JOBS, RESTORE_SCRIPT, RESTORE_SCRIPT_NAME, RESTORING_MARKER,
};
use crate::database::target::DatabaseTarget;
use crate::error::AppError;

#[derive(Clone, Debug, PartialEq)]
pub struct RestoreRequest {
    pub dump_path: PathBuf,
    pub target: DatabaseTarget,
    /// `None` lets the tools fall back to `~/.pgpass`.
    pub password: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreExit {
    Succeeded,
    Failed {
        exit_code: i32,
    },
    /// Ended by a signal, i.e. the user pressed Cancel.
    Stopped,
}

/// Stages of the restore script, in order. Shown as a step indicator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RestorePhase {
    /// Only in "Refresh latest": fetching the dump from S3 before the script runs.
    Downloading,
    CheckingDump,
    Connecting,
    /// Only in "Drop the dump's schemas" mode.
    DroppingSchemas,
    /// Only in "Recreate the database" mode.
    RecreatingDatabase,
    RestoringData,
}

impl RestorePhase {
    pub const ALL: [Self; 6] = [
        Self::Downloading,
        Self::CheckingDump,
        Self::Connecting,
        Self::DroppingSchemas,
        Self::RecreatingDatabase,
        Self::RestoringData,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Downloading => "Download",
            Self::CheckingDump => "Check dump",
            Self::Connecting => "Connect",
            Self::DroppingSchemas => "Drop schemas",
            Self::RecreatingDatabase => "Recreate database",
            Self::RestoringData => "Restore data",
        }
    }
}

/// The phase a script output line starts, if it is one of the script's marker lines.
pub fn detect_started_phase(line: &str) -> Option<RestorePhase> {
    if line.starts_with(CONNECTING_MARKER) {
        return Some(RestorePhase::Connecting);
    }
    if line.starts_with(DROPPING_SCHEMAS_MARKER) {
        return Some(RestorePhase::DroppingSchemas);
    }
    if line.starts_with(RECREATING_MARKER) {
        return Some(RestorePhase::RecreatingDatabase);
    }
    if line.starts_with(RESTORING_MARKER) {
        return Some(RestorePhase::RestoringData);
    }
    None
}

/// pg_restore keeps going past failing items and ends with
/// `pg_restore: warning: errors ignored on restore: 8` (and exit code 1).
pub fn parse_ignored_error_count(line: &str) -> Option<u32> {
    let (_, count) = line.split_once(IGNORED_ERRORS_MARKER)?;
    count.trim().parse().ok()
}

/// Runs the restore and calls `on_line` for every output line until it ends.
/// `on_started` receives the process group id that `stop_restore` needs.
pub async fn run_restore(
    request: &RestoreRequest,
    on_started: impl FnOnce(u32),
    mut on_line: impl FnMut(String),
) -> Result<RestoreExit, AppError> {
    let script = format!("{MERGE_STDERR_INTO_STDOUT}{RESTORE_SCRIPT}");
    let mut command = Command::new("/bin/bash");
    command
        .arg("-c")
        .arg(script)
        .arg(RESTORE_SCRIPT_NAME)
        .arg(&request.dump_path)
        .arg(&request.target.database)
        .env("RESTORE_PARALLEL_JOBS", RESTORE_PARALLEL_JOBS)
        .env("RESTORE_MODE", request.target.restore_mode.script_value())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Own process group, so Cancel also stops pg_restore's parallel workers.
        .process_group(0)
        .kill_on_drop(true);
    apply_connection_env(&mut command, &request.target, request.password.as_deref());

    let mut child = command
        .spawn()
        .map_err(|error| AppError::Restore(format!("Could not start the restore: {error}")))?;
    if let Some(process_id) = child.id() {
        on_started(process_id);
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::Restore("Restore output is not available.".to_string()))?;
    let mut output = BufReader::new(stdout);
    let mut line_bytes = Vec::new();
    loop {
        line_bytes.clear();
        let read_count = output
            .read_until(b'\n', &mut line_bytes)
            .await
            .map_err(|error| {
                AppError::Restore(format!("Could not read restore output: {error}"))
            })?;
        if read_count == 0 {
            break;
        }
        // Lossy: a stray non-UTF-8 byte in a log line must not abort the restore.
        let line = String::from_utf8_lossy(&line_bytes).trim_end().to_string();
        on_line(line);
    }

    let status = child
        .wait()
        .await
        .map_err(|error| AppError::Restore(format!("Could not wait for the restore: {error}")))?;
    Ok(match status.code() {
        Some(0) => RestoreExit::Succeeded,
        Some(exit_code) => RestoreExit::Failed { exit_code },
        None => RestoreExit::Stopped,
    })
}

/// Sends SIGTERM to the whole restore process group (bash, psql/pg_restore and its workers).
pub fn stop_restore(process_group_id: u32) {
    let result = std::process::Command::new("/bin/kill")
        .arg("-TERM")
        .arg(format!("-{process_group_id}"))
        .status();
    if let Err(error) = result {
        tracing::warn!("Could not stop restore process group {process_group_id}: {error}");
    }
}

/// Connects with `psql` and runs `SELECT 1`.
pub async fn test_connection(
    target: &DatabaseTarget,
    password: Option<&str>,
) -> Result<(), AppError> {
    let mut command = Command::new(find_postgres_tool("psql"));
    command.args(["-X", "-w", "-v", "ON_ERROR_STOP=1", "-c", "SELECT 1"]);
    command.arg("-d").arg(&target.database);
    apply_connection_env(&mut command, target, password);

    let output = command.output().await.map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            return AppError::DatabaseConnection(
                "psql is not installed. Run in a terminal: brew install libpq".to_string(),
            );
        }
        AppError::DatabaseConnection(error.to_string())
    })?;

    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(AppError::DatabaseConnection(detail))
}

fn apply_connection_env(command: &mut Command, target: &DatabaseTarget, password: Option<&str>) {
    command
        .env("PATH", build_path_with_postgres_tools())
        .env("PGHOST", &target.host)
        .env("PGPORT", target.port.to_string())
        .env("PGUSER", &target.user)
        .env("PGCONNECT_TIMEOUT", DATABASE_CONNECT_TIMEOUT_SECONDS);

    // Passed only to the child process, never on its command line (visible in `ps`).
    match password.filter(|password| !password.is_empty()) {
        Some(password) => command.env("PGPASSWORD", password),
        None => command.env_remove("PGPASSWORD"),
    };
}

// Apps opened from Finder get a minimal PATH without Homebrew, so add it explicitly.
fn build_path_with_postgres_tools() -> String {
    let current_path = env::var("PATH").unwrap_or_default();
    format!("{}:{current_path}", POSTGRES_TOOL_DIRS.join(":"))
}

fn find_postgres_tool(tool_name: &str) -> PathBuf {
    POSTGRES_TOOL_DIRS
        .iter()
        .map(|dir| Path::new(dir).join(tool_name))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(tool_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_script_takes_dump_and_database_arguments() {
        assert!(RESTORE_SCRIPT.contains("DUMP_FILE=\"$1\""));
        assert!(RESTORE_SCRIPT.contains("DATABASE=\"$2\""));
    }

    #[test]
    fn script_markers_start_phases() {
        assert_eq!(
            detect_started_phase("Testing connection..."),
            Some(RestorePhase::Connecting)
        );
        assert_eq!(
            detect_started_phase("Starting database restoration..."),
            Some(RestorePhase::RestoringData)
        );
        assert_eq!(detect_started_phase("pg_restore: creating TABLE x"), None);
        // The markers must exist in the embedded script, or phases would never advance.
        assert!(RESTORE_SCRIPT.contains(CONNECTING_MARKER));
        assert!(RESTORE_SCRIPT.contains(RESTORING_MARKER));
        assert!(RESTORE_SCRIPT.contains(RECREATING_MARKER));
        assert!(RESTORE_SCRIPT.contains(DROPPING_SCHEMAS_MARKER));
        assert_eq!(
            detect_started_phase("Dropping schemas: auth, core..."),
            Some(RestorePhase::DroppingSchemas)
        );
        assert_eq!(
            detect_started_phase("Recreating database app_db..."),
            Some(RestorePhase::RecreatingDatabase)
        );
    }

    #[test]
    fn ignored_error_count_reads_pg_restore_summary() {
        assert_eq!(
            parse_ignored_error_count("pg_restore: warning: errors ignored on restore: 8"),
            Some(8)
        );
        assert_eq!(
            parse_ignored_error_count("pg_restore: error: could not execute"),
            None
        );
    }

    #[test]
    fn path_puts_homebrew_postgres_tools_first() {
        assert!(build_path_with_postgres_tools().starts_with("/opt/homebrew/opt/libpq/bin:"));
    }
}
