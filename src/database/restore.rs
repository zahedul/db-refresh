//! Runs `scripts/restore-postgres.sh` (compiled into the app) and streams its output.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{env, io};

use dioxus::logger::tracing;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::database::target::DatabaseTarget;
use crate::error::AppError;

const RESTORE_SCRIPT: &str = include_str!("../../scripts/restore-postgres.sh");
// `exec 2>&1` merges the script's stderr (where pg_restore writes) into stdout, so the
// log keeps the real order of messages and only one pipe has to be read.
const MERGE_STDERR_INTO_STDOUT: &str = "exec 2>&1\n";
const SCRIPT_NAME: &str = "restore-postgres";
const RESTORE_JOBS: &str = "4";
const CONNECT_TIMEOUT_SECONDS: &str = "10";
/// Where Homebrew puts the PostgreSQL client tools (libpq first, as the script expects).
const POSTGRES_TOOL_DIRS: [&str; 3] = [
    "/opt/homebrew/opt/libpq/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
];

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
    /// Only with "Start from an empty database": drop and create the database.
    RecreatingDatabase,
    RestoringData,
}

impl RestorePhase {
    pub const ALL: [Self; 5] = [
        Self::Downloading,
        Self::CheckingDump,
        Self::Connecting,
        Self::RecreatingDatabase,
        Self::RestoringData,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Downloading => "Download",
            Self::CheckingDump => "Check dump",
            Self::Connecting => "Connect",
            Self::RecreatingDatabase => "Recreate database",
            Self::RestoringData => "Restore data",
        }
    }
}

// Marker lines printed by `scripts/restore-postgres.sh`; keep in sync with the script.
const CONNECTING_MARKER: &str = "Testing connection";
const RECREATING_MARKER: &str = "Recreating database";
const RESTORING_MARKER: &str = "Starting database restoration";
const IGNORED_ERRORS_MARKER: &str = "errors ignored on restore:";

/// The phase a script output line starts, if it is one of the script's marker lines.
pub fn phase_started_by(line: &str) -> Option<RestorePhase> {
    if line.starts_with(CONNECTING_MARKER) {
        return Some(RestorePhase::Connecting);
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
pub fn ignored_error_count(line: &str) -> Option<u32> {
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
        .arg(SCRIPT_NAME)
        .arg(&request.dump_path)
        .arg(&request.target.database)
        .env("RESTORE_JOBS", RESTORE_JOBS)
        .env("RECREATE_DATABASE", recreate_flag(&request.target))
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

fn recreate_flag(target: &DatabaseTarget) -> &'static str {
    if target.recreate_database { "1" } else { "0" }
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
        .env("PATH", path_with_postgres_tools())
        .env("PGHOST", &target.host)
        .env("PGPORT", target.port.to_string())
        .env("PGUSER", &target.user)
        .env("PGCONNECT_TIMEOUT", CONNECT_TIMEOUT_SECONDS);

    // Passed only to the child process, never on its command line (visible in `ps`).
    match password.filter(|password| !password.is_empty()) {
        Some(password) => command.env("PGPASSWORD", password),
        None => command.env_remove("PGPASSWORD"),
    };
}

// Apps opened from Finder get a minimal PATH without Homebrew, so add it explicitly.
fn path_with_postgres_tools() -> String {
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
            phase_started_by("Testing connection..."),
            Some(RestorePhase::Connecting)
        );
        assert_eq!(
            phase_started_by("Starting database restoration..."),
            Some(RestorePhase::RestoringData)
        );
        assert_eq!(phase_started_by("pg_restore: creating TABLE x"), None);
        // The markers must exist in the embedded script, or phases would never advance.
        assert!(RESTORE_SCRIPT.contains(CONNECTING_MARKER));
        assert!(RESTORE_SCRIPT.contains(RESTORING_MARKER));
        assert!(RESTORE_SCRIPT.contains(RECREATING_MARKER));
        assert_eq!(
            phase_started_by("Recreating database app_db..."),
            Some(RestorePhase::RecreatingDatabase)
        );
    }

    #[test]
    fn ignored_error_count_reads_pg_restore_summary() {
        assert_eq!(
            ignored_error_count("pg_restore: warning: errors ignored on restore: 8"),
            Some(8)
        );
        assert_eq!(
            ignored_error_count("pg_restore: error: could not execute"),
            None
        );
    }

    #[test]
    fn path_puts_homebrew_postgres_tools_first() {
        assert!(path_with_postgres_tools().starts_with("/opt/homebrew/opt/libpq/bin:"));
    }

    /// Runs only on request, against a throwaway server, because it drops a database:
    /// `DB_REFRESH_TEST_PGPORT=55432 DB_REFRESH_TEST_PGPASSWORD=… DB_REFRESH_TEST_DUMP=/path/x.dump
    ///  cargo test recreate_mode -- --ignored`
    #[tokio::test]
    #[ignore = "needs a disposable PostgreSQL server; see the doc comment"]
    async fn recreate_mode_restores_into_an_empty_database() {
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name}"));
        let request = RestoreRequest {
            dump_path: PathBuf::from(env("DB_REFRESH_TEST_DUMP")),
            target: DatabaseTarget {
                host: "127.0.0.1".to_string(),
                port: env("DB_REFRESH_TEST_PGPORT").parse().unwrap(),
                user: "postgres".to_string(),
                database: "db_refresh_integration".to_string(),
                recreate_database: true,
                ..DatabaseTarget::default()
            },
            password: Some(env("DB_REFRESH_TEST_PGPASSWORD")),
        };
        let mut phases = Vec::new();
        let mut lines = Vec::new();

        let exit = run_restore(
            &request,
            |_| {},
            |line| {
                phases.extend(phase_started_by(&line));
                lines.push(line);
            },
        )
        .await
        .unwrap();

        assert_eq!(exit, RestoreExit::Succeeded, "log:\n{}", lines.join("\n"));
        assert_eq!(
            phases,
            vec![
                RestorePhase::Connecting,
                RestorePhase::RecreatingDatabase,
                RestorePhase::RestoringData
            ]
        );
        assert!(!lines.iter().any(|line| line.contains("error")));
    }

    #[tokio::test]
    async fn invalid_dump_fails_with_script_message() {
        let request = RestoreRequest {
            dump_path: PathBuf::from("/definitely/missing.dump"),
            target: DatabaseTarget {
                user: "nobody".to_string(),
                database: "nothing".to_string(),
                ..DatabaseTarget::default()
            },
            password: None,
        };
        let mut lines = Vec::new();

        let exit = run_restore(&request, |_| {}, |line| lines.push(line))
            .await
            .unwrap();

        assert_eq!(exit, RestoreExit::Failed { exit_code: 1 });
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Dump file not found"))
        );
    }
}
