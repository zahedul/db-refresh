//! Integration tests for the restore pipeline, using only the public API.
//!
//! The PostgreSQL tests drop schemas and databases, so they run only on request,
//! against a throwaway server:
//!
//! ```text
//! docker run -d --name db-refresh-test -e POSTGRES_PASSWORD=test -p 127.0.0.1:55432:5432 postgres
//! DB_REFRESH_TEST_PGPORT=55432 DB_REFRESH_TEST_PGPASSWORD=test \
//! DB_REFRESH_TEST_DUMP=/path/to/any.dump cargo test --test restore -- --ignored
//! ```

use std::path::PathBuf;

use db_refresh::database::restore::{
    RestoreExit, RestorePhase, RestoreRequest, detect_started_phase, run_restore,
};
use db_refresh::database::target::{DatabaseTarget, RestoreMode};

/// Everything a restore printed, and the phases its marker lines started.
struct RestoreRun {
    exit: RestoreExit,
    phases: Vec<RestorePhase>,
    lines: Vec<String>,
}

impl RestoreRun {
    fn has_errors(&self) -> bool {
        self.lines.iter().any(|line| line.contains("error"))
    }

    fn log(&self) -> String {
        self.lines.join("\n")
    }
}

async fn run_restore_collecting_output(request: RestoreRequest) -> RestoreRun {
    let (mut phases, mut lines) = (Vec::new(), Vec::new());
    let exit = run_restore(
        &request,
        |_| {},
        |line| {
            phases.extend(detect_started_phase(&line));
            lines.push(line);
        },
    )
    .await
    .expect("the restore script should start");
    RestoreRun {
        exit,
        phases,
        lines,
    }
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}; see the top of tests/restore.rs"))
}

fn test_server_request(restore_mode: RestoreMode, database: &str) -> RestoreRequest {
    RestoreRequest {
        dump_path: PathBuf::from(required_env("DB_REFRESH_TEST_DUMP")),
        target: DatabaseTarget {
            host: "127.0.0.1".to_string(),
            port: required_env("DB_REFRESH_TEST_PGPORT")
                .parse()
                .expect("DB_REFRESH_TEST_PGPORT should be a port number"),
            user: "postgres".to_string(),
            database: database.to_string(),
            restore_mode,
            ..DatabaseTarget::default()
        },
        password: Some(required_env("DB_REFRESH_TEST_PGPASSWORD")),
    }
}

#[tokio::test]
async fn missing_dump_fails_with_the_script_message() {
    let request = RestoreRequest {
        dump_path: PathBuf::from("/definitely/missing.dump"),
        target: DatabaseTarget {
            user: "nobody".to_string(),
            database: "nothing".to_string(),
            ..DatabaseTarget::default()
        },
        password: None,
    };

    let run = run_restore_collecting_output(request).await;

    assert_eq!(run.exit, RestoreExit::Failed { exit_code: 1 });
    assert!(
        run.lines
            .iter()
            .any(|line| line.contains("Dump file not found"))
    );
}

#[tokio::test]
#[ignore = "needs a disposable PostgreSQL server; see the top of this file"]
async fn drop_schemas_mode_replaces_the_dumps_schemas() {
    let run = run_restore_collecting_output(test_server_request(
        RestoreMode::DropSchemas,
        "db_refresh_drop_schemas",
    ))
    .await;

    assert_eq!(run.exit, RestoreExit::Succeeded, "log:\n{}", run.log());
    let expected_phases = [
        RestorePhase::Connecting,
        RestorePhase::DroppingSchemas,
        RestorePhase::RestoringData,
    ];
    assert_eq!(run.phases, expected_phases);
    assert!(!run.has_errors(), "log:\n{}", run.log());
}

#[tokio::test]
#[ignore = "needs a disposable PostgreSQL server; see the top of this file"]
async fn recreate_mode_restores_into_an_empty_database() {
    let run = run_restore_collecting_output(test_server_request(
        RestoreMode::RecreateDatabase,
        "db_refresh_recreate",
    ))
    .await;

    assert_eq!(run.exit, RestoreExit::Succeeded, "log:\n{}", run.log());
    let expected_phases = [
        RestorePhase::Connecting,
        RestorePhase::RecreatingDatabase,
        RestorePhase::RestoringData,
    ];
    assert_eq!(run.phases, expected_phases);
    assert!(!run.has_errors(), "log:\n{}", run.log());
}
