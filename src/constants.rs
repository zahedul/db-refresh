//! Every constant of the app in one place, grouped by area.
//! Tests keep their own fixtures next to the tests.

use std::time::Duration;

use dioxus::desktop::LogicalSize;
// The whole prelude: `asset!` expands to paths that need it in scope.
use dioxus::prelude::*;
use objc2_local_authentication::LAError;

// ===== App window =====

pub const APP_TITLE: &str = "DB Refresh";
pub const WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1200.0, 800.0);
pub const MIN_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 640.0);
pub const MAIN_CSS: Asset = asset!("/assets/main.css");
/// The user guide is compiled in, so Help works offline and matches the installed version.
pub const USER_GUIDE_MARKDOWN: &str = include_str!("../docs/USER_GUIDE.md");

// ===== Settings =====

/// Folder under `~/Library/Application Support`.
pub const APP_FOLDER_NAME: &str = "db-refresh";
pub const SETTINGS_FILE_NAME: &str = "settings.json";

// ===== Keychain =====

pub const KEYCHAIN_SERVICE: &str = "dev.dbrefresh.app";
/// One Keychain item holds every database password (see `database::credentials`).
pub const KEYCHAIN_VAULT_ACCOUNT: &str = "database-passwords";
/// `errSecItemNotFound` from Apple's Security framework.
pub const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;

// ===== Touch ID =====

/// LocalAuthentication codes meaning "the user (or system) dismissed the prompt".
pub const TOUCH_ID_CANCELLATION_CODES: [LAError; 3] = [
    LAError::UserCancel,
    LAError::SystemCancel,
    LAError::AppCancel,
];

// ===== AWS / S3 =====

pub const DEFAULT_AWS_PROFILE: &str = "default";
pub const S3_FOLDER_SEPARATOR: char = '/';
pub const WHOLE_BUCKET_LABEL: &str = "Whole bucket";
/// Stops runaway listings on huge buckets: 50 pages of 1000 keys.
pub const MAX_SCANNED_OBJECTS: usize = 50_000;
/// Folder listing also pages through the files at that level; capped so a folder
/// holding thousands of dumps does not make the folder picker slow.
pub const MAX_FOLDER_LIST_PAGES: usize = 10;
pub const HTTP_FORBIDDEN: u16 = 403;
pub const HTTP_PRECONDITION_FAILED: u16 = 412;

// ===== Downloads =====

/// 8 MiB parts × 8 at a time: a 1 GB dump is 128 parts with 8 requests in flight.
/// Several connections fill the bandwidth far better than one long stream.
pub const DOWNLOAD_PART_SIZE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_PARALLEL_DOWNLOAD_PARTS: usize = 8;
/// A failed part (e.g. a dropped connection) is retried alone, not the whole file.
pub const MAX_PART_ATTEMPTS: u32 = 3;
pub const PART_RETRY_BASE_DELAY: Duration = Duration::from_millis(500);
/// Received data is written in batches to keep disk writes few and large.
pub const DOWNLOAD_WRITE_BATCH_BYTES: usize = 1024 * 1024;
/// Progress callbacks drive UI redraws; a few per second is smooth without wasting CPU.
pub const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
pub const PARTIAL_FILE_SUFFIX: &str = ".part";

// ===== Dump files =====

pub const DUMP_EXTENSION: &str = "dump";
/// Extensions offered by "Choose another file…".
pub const PICKABLE_DUMP_EXTENSIONS: [&str; 2] = ["dump", "backup"];
/// `gen_2026-11-20.dump` belongs to group `gen`.
pub const DUMP_GROUP_SEPARATOR: char = '_';

// ===== Databases and restore =====

pub const DEFAULT_DATABASE_HOST: &str = "localhost";
pub const DEFAULT_DATABASE_PORT: u16 = 5432;
pub const POSTGRES_URL_SCHEMES: [&str; 2] = ["postgresql://", "postgres://"];
pub const RESTORE_SCRIPT: &str = include_str!("../scripts/restore-postgres.sh");
/// Merges the script's stderr (where pg_restore writes) into stdout, so the log keeps
/// the real order of messages and only one pipe has to be read.
pub const MERGE_STDERR_INTO_STDOUT: &str = "exec 2>&1\n";
pub const RESTORE_SCRIPT_NAME: &str = "restore-postgres";
pub const RESTORE_PARALLEL_JOBS: &str = "4";
pub const DATABASE_CONNECT_TIMEOUT_SECONDS: &str = "10";
/// Where Homebrew puts the PostgreSQL client tools (libpq first, as the script expects).
pub const POSTGRES_TOOL_DIRS: [&str; 3] = [
    "/opt/homebrew/opt/libpq/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
];

// Marker lines printed by `scripts/restore-postgres.sh`; keep in sync with the script.
pub const CONNECTING_MARKER: &str = "Testing connection";
pub const DROPPING_SCHEMAS_MARKER: &str = "Dropping schemas";
pub const RECREATING_MARKER: &str = "Recreating database";
pub const RESTORING_MARKER: &str = "Starting database restoration";
pub const IGNORED_ERRORS_MARKER: &str = "errors ignored on restore:";

// ===== Restore log =====

/// pg_restore --verbose prints a line per object; keep memory bounded on huge dumps.
pub const MAX_KEPT_LOG_LINES: usize = 5_000;
/// The page shows only the tail of the log; the kept log stays in memory.
pub const MAX_VISIBLE_LOG_LINES: usize = 1_000;
/// Tools that start their lines with a severity label ("pg_restore: warning: …").
pub const LOG_TOOL_PREFIXES: [&str; 2] = ["pg_restore: ", "psql: "];

// ===== Lists and UI =====

pub const DUMP_FILES_PER_PAGE: usize = 10;
pub const TAG_COLOR_COUNT: u32 = 5;
pub const ICON_SIZE: &str = "16";
pub const ICON_VIEW_BOX: &str = "0 0 20 20";

// ===== Formatting =====

pub const SIZE_UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
/// Decimal units (1 KB = 1000 B) to match what macOS Finder shows.
pub const SIZE_STEP: f64 = 1000.0;
pub const SECONDS_PER_MINUTE: i64 = 60;
pub const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
pub const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
/// Older than this, ages are shown as a date instead of "12d ago".
pub const RELATIVE_AGE_MAX_DAYS: i64 = 30;
