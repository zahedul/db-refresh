use std::cmp::Ordering;
use std::time::Duration;

use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::aws::download::DownloadProgress;
use crate::clipboard::copy_to_clipboard;
use crate::database::credentials::password_for;
use crate::database::log_line::{LogLevel, LogLine};
use crate::database::restore::{RestorePhase, RestoreRequest};
use crate::database::target::DatabaseTarget;
use crate::format::{
    format_duration, format_relative_age, format_size, format_speed, now_epoch_secs,
};
use crate::local_files::{LocalDump, display_local_path, download_dir, list_local_dumps};
use crate::ui::app_context::{AppContext, use_app_context};
use crate::ui::icons::{DatabaseIcon, RefreshIcon, ServerIcon};
use crate::ui::modal::Modal;
use crate::ui::panel_header::PanelHeader;
use crate::ui::picker::{
    PickerFooter, PickerList, PickerMessage, PickerPopover, PickerRow, PickerTrigger,
};
use crate::ui::state::{RestoreJob, RestoreResult, RestoreState};
use crate::ui::sync::restore_runner::{cancel_restore, start_restore};

// The DOM only shows the tail of the log; the full (bounded) log stays in memory.
const VISIBLE_LOG_LINES: usize = 1_000;
const DUMP_FILE_EXTENSIONS: [&str; 2] = ["dump", "backup"];

#[component]
pub fn SyncPanel() -> Element {
    let app = use_app_context();
    let mut is_confirming = use_signal(|| false);

    let selected_dump = app
        .selected_dump
        .read()
        .as_deref()
        .and_then(LocalDump::from_path);
    let selected_target = selected_target(app);
    let is_running = app.restore.read().is_running();

    let missing_choice_hint = match (&selected_dump, &selected_target) {
        (None, _) => Some("Choose the dump file to restore."),
        (_, None) => Some("Choose the database to refresh."),
        _ => None,
    };
    let can_sync = missing_choice_hint.is_none() && !is_running;

    rsx! {
        section { class: "panel sync-panel",
            PanelHeader {
                icon: rsx! { RefreshIcon {} },
                title: "Restore",
                subtitle: "Replace a database with the contents of a dump",
            }
            div { class: "sync-route",
                div { class: "sync-step",
                    span { class: "field-label", "1 · Dump file" }
                    DumpPicker { selected_dump: selected_dump.clone() }
                    if let Some(dump) = &selected_dump {
                        span {
                            class: "sync-step-detail",
                            title: "{display_local_path(&dump.path)}",
                            "{dump_summary(dump, now_epoch_secs())}"
                        }
                    }
                }
                div { class: "sync-arrow", "→" }
                div { class: "sync-step",
                    span { class: "field-label", "2 · Database" }
                    TargetPicker { selected_target: selected_target.clone() }
                    if let Some(target) = &selected_target {
                        span {
                            class: "sync-step-detail",
                            title: "{target.connection_label()}",
                            span { class: "mono", "{target.database}" }
                            " · {target.host}:{target.port}"
                        }
                    }
                }
            }
            div { class: "sync-actions",
                button {
                    class: "danger",
                    disabled: !can_sync,
                    onclick: move |_| is_confirming.set(true),
                    "Sync now"
                }
                if let Some(hint) = missing_choice_hint {
                    span { class: "muted", "{hint}" }
                }
            }
            RestoreStatus {}
            RestoreLog {}
        }
        if let (true, Some(dump), Some(target)) = (is_confirming(), selected_dump, selected_target) {
            ConfirmRestoreDialog { dump, target, on_close: move |_| is_confirming.set(false) }
        }
    }
}

fn selected_target(app: AppContext) -> Option<DatabaseTarget> {
    let selected_id = app.selected_target_id.read().clone()?;
    app.settings
        .read()
        .database_targets
        .iter()
        .find(|target| target.id == selected_id)
        .cloned()
}

#[component]
fn DumpPicker(selected_dump: Option<LocalDump>) -> Element {
    let app = use_app_context();
    let mut is_open = use_signal(|| false);

    let choose_other_file = move |_| {
        is_open.set(false);
        spawn(async move {
            let picked = AsyncFileDialog::new()
                .set_title("Choose a PostgreSQL dump")
                .add_filter("PostgreSQL dump", &DUMP_FILE_EXTENSIONS)
                .set_directory(download_dir())
                .pick_file()
                .await;
            if let Some(file) = picked {
                app.select_dump(file.path().to_path_buf());
            }
        });
    };

    let label = selected_dump
        .as_ref()
        .map(|dump| dump.file_name.clone())
        .unwrap_or_else(|| "Choose a dump file".to_string());

    rsx! {
        div { class: "picker",
            PickerTrigger {
                icon: rsx! { DatabaseIcon {} },
                label,
                is_open: is_open(),
                on_toggle: move |_| is_open.set(!is_open()),
            }
            if is_open() {
                PickerPopover { on_close: move |_| is_open.set(false),
                    LocalDumpOptions {
                        selected_path: selected_dump.as_ref().map(|dump| dump.path.display().to_string()),
                        on_pick: move |dump: LocalDump| {
                            app.select_dump(dump.path);
                            is_open.set(false);
                        },
                    }
                    PickerFooter {
                        button { class: "secondary", onclick: choose_other_file, "Choose another file…" }
                    }
                }
            }
        }
    }
}

#[component]
fn LocalDumpOptions(selected_path: Option<String>, on_pick: EventHandler<LocalDump>) -> Element {
    let dumps = list_local_dumps(&download_dir());
    if dumps.is_empty() {
        let folder = display_local_path(&download_dir());
        return rsx! { PickerMessage { text: format!("No downloaded dumps in {folder} yet.") } };
    }
    let now = now_epoch_secs();

    rsx! {
        PickerList {
            for dump in dumps {
                PickerRow {
                    key: "{dump.path.display()}",
                    icon: rsx! { DatabaseIcon {} },
                    label: dump.file_name.clone(),
                    trailing: dump_summary(&dump, now),
                    is_selected: selected_path.as_deref() == Some(dump.path.display().to_string().as_str()),
                    on_pick: {
                        let dump = dump.clone();
                        move |_| on_pick.call(dump.clone())
                    },
                }
            }
        }
    }
}

fn dump_summary(dump: &LocalDump, now_epoch_secs: i64) -> String {
    let size = format_size(dump.size_bytes);
    match dump.modified_epoch_secs {
        Some(modified) => format!("{size} · {}", format_relative_age(modified, now_epoch_secs)),
        None => size,
    }
}

#[component]
fn TargetPicker(selected_target: Option<DatabaseTarget>) -> Element {
    let app = use_app_context();
    let mut is_open = use_signal(|| false);
    let targets = app.settings.read().database_targets.clone();
    let selected_id = selected_target.as_ref().map(|target| target.id.clone());
    let label = selected_target
        .as_ref()
        .map(|target| target.name.clone())
        .unwrap_or_else(|| "Choose a database".to_string());

    rsx! {
        div { class: "picker",
            PickerTrigger {
                icon: rsx! { ServerIcon {} },
                label,
                is_open: is_open(),
                on_toggle: move |_| is_open.set(!is_open()),
            }
            if is_open() {
                PickerPopover { on_close: move |_| is_open.set(false),
                    if targets.is_empty() {
                        PickerMessage { text: "Add a database in the Databases list first." }
                    } else {
                        PickerList {
                            for target in targets {
                                PickerRow {
                                    key: "{target.id}",
                                    icon: rsx! { ServerIcon {} },
                                    label: target.name.clone(),
                                    trailing: target.database.clone(),
                                    is_selected: selected_id.as_deref() == Some(target.id.as_str()),
                                    on_pick: {
                                        let target_id = target.id.clone();
                                        let mut selected_target_id = app.selected_target_id;
                                        move |_| {
                                            selected_target_id.set(Some(target_id.clone()));
                                            is_open.set(false);
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ConfirmRestoreDialog(
    dump: LocalDump,
    target: DatabaseTarget,
    on_close: EventHandler<()>,
) -> Element {
    let app = use_app_context();
    let mut typed_password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let confirm = {
        let (dump, target) = (dump.clone(), target.clone());
        move |_| {
            let password = match password_for(&target, &typed_password()) {
                Ok(password) => password,
                Err(password_error) => {
                    error.set(Some(password_error.to_string()));
                    return;
                }
            };
            let request = RestoreRequest {
                dump_path: dump.path.clone(),
                target: target.clone(),
                password,
            };
            let job = RestoreJob {
                dump_name: dump.file_name.clone(),
                target_name: target.name.clone(),
                database: target.database.clone(),
                includes_download: false,
            };
            start_restore(app, request, job);
            on_close.call(());
        }
    };

    rsx! {
        Modal { title: format!("Replace {}?", target.database), on_close: move |_| on_close.call(()),
            p { class: "modal-text",
                "Restore "
                strong { "{dump.file_name}" }
                " into "
                strong { "{target.database}" }
                " on {target.host}:{target.port}."
            }
            div { class: "warning-box",
                "Existing tables in {target.database} are dropped and recreated (pg_restore --clean). "
                "Any changes you made in that database will be lost."
            }
            if !target.is_password_saved {
                div { class: "form-field",
                    span { class: "field-label", "Password for {target.user}" }
                    input {
                        r#type: "password",
                        placeholder: "Leave empty to use ~/.pgpass",
                        value: "{typed_password}",
                        oninput: move |event| typed_password.set(event.value()),
                    }
                }
            }
            if let Some(message) = error() {
                p { class: "error form-message", "{message}" }
            }
            div { class: "modal-footer",
                div { class: "modal-footer-spacer" }
                button { class: "secondary", onclick: move |_| on_close.call(()), "Cancel" }
                button { class: "danger", onclick: confirm, "Replace data" }
            }
        }
    }
}

#[component]
fn RestoreStatus() -> Element {
    let app = use_app_context();
    let mut clock_tick = use_signal(|| 0_u64);
    // pg_restore can be silent for minutes while building indexes; tick once a second
    // so the elapsed time keeps moving.
    use_future(move || async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            if app.restore.peek().is_running() {
                clock_tick += 1;
            }
        }
    });
    clock_tick.read();

    match &*app.restore.read() {
        RestoreState::Idle => rsx! {},
        RestoreState::Running {
            job,
            started_at,
            phase,
            download_progress,
            process_group_id,
            task,
        } => {
            let elapsed = started_at.elapsed();
            let status_text = running_status_text(job, *phase, *download_progress, elapsed);
            let can_cancel = process_group_id.is_some() || task.is_some();
            rsx! {
                PhaseSteps {
                    current: *phase,
                    outcome: StepsOutcome::InProgress,
                    includes_download: job.includes_download,
                }
                div { class: "restore-status running",
                    span { class: "spinner" }
                    span { class: "restore-status-text", "{status_text}" }
                    button {
                        class: "secondary small",
                        title: "Stops the refresh. The database may be left partly restored.",
                        disabled: !can_cancel,
                        onclick: move |_| cancel_restore(app),
                        "Cancel"
                    }
                }
            }
        }
        RestoreState::Finished {
            job,
            result,
            elapsed,
            reached_phase,
        } => {
            let took = format_duration(*elapsed);
            let (status_class, message, steps_outcome) = match result {
                RestoreResult::Succeeded => (
                    "success",
                    format!(
                        "✓ {} is now a copy of {} (took {took})",
                        job.database, job.dump_name
                    ),
                    StepsOutcome::Done,
                ),
                RestoreResult::CompletedWithErrors {
                    ignored_error_count,
                } => (
                    "partial",
                    format!(
                        "⚠ {} restored in {took}, but {ignored_error_count} item(s) failed and were skipped. \
                         Use the Errors filter below to see which.",
                        job.database
                    ),
                    StepsOutcome::Done,
                ),
                RestoreResult::Failed(message) => (
                    "failed",
                    format!("✗ Sync failed: {message}"),
                    StepsOutcome::StoppedAt,
                ),
                RestoreResult::Cancelled => (
                    "cancelled",
                    format!(
                        "Cancelled after {took}. {} may be partly restored. Run the sync again to fix it.",
                        job.database
                    ),
                    StepsOutcome::StoppedAt,
                ),
            };
            rsx! {
                PhaseSteps {
                    current: *reached_phase,
                    outcome: steps_outcome,
                    includes_download: job.includes_download,
                }
                div { class: "restore-status {status_class}", "{message}" }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StepsOutcome {
    InProgress,
    Done,
    StoppedAt,
}

/// `Check dump ✓ ─ Connect ✓ ─ Restore data ● ─ Done`
#[component]
fn PhaseSteps(current: RestorePhase, outcome: StepsOutcome, includes_download: bool) -> Element {
    let phases = RestorePhase::ALL
        .into_iter()
        .filter(move |phase| includes_download || *phase != RestorePhase::Downloading);
    let step_class = |phase: RestorePhase| match (phase.cmp(&current), outcome) {
        (Ordering::Less, _) | (Ordering::Equal, StepsOutcome::Done) => "done",
        (Ordering::Equal, StepsOutcome::InProgress) => "active",
        (Ordering::Equal, StepsOutcome::StoppedAt) => "stopped",
        (Ordering::Greater, _) => "pending",
    };
    let final_class = if outcome == StepsOutcome::Done {
        "done"
    } else {
        "pending"
    };

    rsx! {
        ol { class: "phase-steps",
            for phase in phases {
                li { key: "{phase.label()}", class: "phase-step {step_class(phase)}",
                    span { class: "phase-dot" }
                    "{phase.label()}"
                }
            }
            li { class: "phase-step {final_class}",
                span { class: "phase-dot" }
                "Done"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogFilter {
    All,
    Errors,
    Warnings,
}

impl LogFilter {
    fn shows(self, level: LogLevel) -> bool {
        match self {
            Self::All => true,
            Self::Errors => level == LogLevel::Error,
            Self::Warnings => level == LogLevel::Warning,
        }
    }
}

#[component]
fn RestoreLog() -> Element {
    let app = use_app_context();
    let mut filter = use_signal(|| LogFilter::All);
    let mut is_copied = use_signal(|| false);

    let lines = app.restore_log.read();
    if lines.is_empty() {
        return rsx! {
            div { class: "log-view empty", "The restore log appears here when a sync runs." }
        };
    }

    let count_of = |level: LogLevel| lines.iter().filter(|line| line.level == level).count();
    let (error_count, warning_count) = (count_of(LogLevel::Error), count_of(LogLevel::Warning));
    let active_filter = filter();
    let matching_lines: Vec<&LogLine> = lines
        .iter()
        .filter(|line| active_filter.shows(line.level))
        .collect();
    let first_visible = matching_lines.len().saturating_sub(VISIBLE_LOG_LINES);

    let copy_log = move |_| {
        let full_log = app
            .restore_log
            .peek()
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        copy_to_clipboard(&full_log);
        is_copied.set(true);
    };

    rsx! {
        div { class: "log-toolbar",
            span { class: "log-title", "Log" }
            div { class: "chips",
                LogFilterChip { label: format!("All {}", lines.len()), filter_value: LogFilter::All, filter }
                LogFilterChip { label: format!("Errors {error_count}"), filter_value: LogFilter::Errors, filter, tone: "error" }
                LogFilterChip { label: format!("Warnings {warning_count}"), filter_value: LogFilter::Warnings, filter, tone: "warning" }
            }
            div { class: "modal-footer-spacer" }
            button { class: "link-button", onclick: copy_log,
                if is_copied() { "✓ Copied" } else { "Copy log" }
            }
        }
        // `column-reverse` keeps the view scrolled to the newest line as lines arrive.
        div { class: "log-view",
            div { class: "log-lines",
                if matching_lines.is_empty() {
                    div { class: "log-line info", "No matching lines." }
                }
                for (offset, line) in matching_lines[first_visible..].iter().enumerate() {
                    div {
                        key: "{first_visible + offset}",
                        class: "log-line {log_level_class(line.level)}",
                        "{line.text}"
                    }
                }
            }
        }
        if active_filter != LogFilter::All {
            p { class: "muted log-filter-note",
                "Showing {matching_lines.len()} of {lines.len()} lines. "
                button { class: "link-button", onclick: move |_| filter.set(LogFilter::All), "Show all" }
            }
        }
    }
}

#[component]
fn LogFilterChip(
    label: String,
    filter_value: LogFilter,
    filter: Signal<LogFilter>,
    #[props(default)] tone: Option<String>,
) -> Element {
    rsx! {
        button {
            class: "chip",
            class: if filter() == filter_value { "active" },
            class: if let Some(tone) = &tone { "{tone}" },
            onclick: move |_| filter.set(filter_value),
            "{label}"
        }
    }
}

fn log_level_class(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warning => "warning",
        LogLevel::Success => "success",
        LogLevel::Info => "info",
    }
}

/// `Download · 412.0 MB of 1.0 GB · 48.2 MB/s · 18s left · 0m 09s` while downloading,
/// `Restore data · gen_x.dump → gen_db · 1m 05s` afterwards.
fn running_status_text(
    job: &RestoreJob,
    phase: RestorePhase,
    download_progress: Option<DownloadProgress>,
    elapsed: Duration,
) -> String {
    let took = format_duration(elapsed);
    let Some(progress) = download_progress.filter(|_| phase == RestorePhase::Downloading) else {
        return format!(
            "{} · {} → {} · {took}",
            phase.label(),
            job.dump_name,
            job.database
        );
    };

    let mut parts = vec![
        phase.label().to_string(),
        format!(
            "{} of {}",
            format_size(progress.received_bytes),
            format_size(progress.total_bytes)
        ),
    ];
    if progress.received_bytes > 0 {
        parts.push(format_speed(progress.bytes_per_second(elapsed)));
    }
    if let Some(remaining) = progress.time_remaining(elapsed) {
        parts.push(format!("{} left", format_duration(remaining)));
    }
    parts.join(" · ")
}
