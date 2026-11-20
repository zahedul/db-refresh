use std::collections::BTreeSet;
use std::path::PathBuf;

use dioxus::prelude::*;

use crate::aws::s3::{DumpFile, SearchResult, find_latest_file_per_group};
use crate::constants::DUMP_FILES_PER_PAGE;
use crate::format::{
    format_relative_age, format_size, format_utc_timestamp, read_current_epoch_secs,
};
use crate::local_files::{has_complete_local_copy, reveal_in_finder};
use crate::ui::app_context::use_app_context;
use crate::ui::download_dialog::DownloadDialog;
use crate::ui::downloads::DownloadJobs;
use crate::ui::icons::DatabaseIcon;
use crate::ui::panel_header::PanelHeader;
use crate::ui::state::SearchState;
use crate::ui::state::{DownloadJob, DownloadStatus};
use crate::ui::tags::GroupTag;

#[component]
pub fn ResultsPanel(
    selected_profile: Signal<Option<String>>,
    search_state: Signal<SearchState>,
    downloads: DownloadJobs,
) -> Element {
    let subtitle = build_backups_subtitle(&search_state.read());
    let content = match &*search_state.read() {
        SearchState::Idle => {
            let hint = match selected_profile.read().as_deref() {
                Some(_) => "Choose a bucket and folder on the left, then press Search dumps.",
                None => "Select an AWS profile on the left to start.",
            };
            rsx! {
                div { class: "empty-state",
                    div { class: "empty-state-icon", DatabaseIcon {} }
                    p { class: "empty-state-title", "No backups listed yet" }
                    p { class: "muted", "{hint}" }
                }
            }
        }
        SearchState::Searching { .. } => rsx! {
            div { class: "empty-state",
                span { class: "spinner" }
                p { class: "muted", "Looking for dump files…" }
            }
        },
        SearchState::Failed(error) => rsx! {
            p { class: "error", "{error}" }
        },
        SearchState::Found(result) => rsx! {
            DumpFileList {
                key: "{result.request.s3_location()}|{result.request.name_filter}",
                result: result.clone(),
                downloads,
            }
        },
    };

    rsx! {
        section { class: "panel",
            PanelHeader {
                icon: rsx! { DatabaseIcon {} },
                title: "Backups",
                subtitle,
            }
            {content}
        }
    }
}

fn build_backups_subtitle(search_state: &SearchState) -> String {
    match search_state {
        SearchState::Idle => "Dump files found in S3 appear here".to_string(),
        SearchState::Searching { s3_location } => format!("Searching {s3_location}"),
        SearchState::Failed(_) => "Search failed".to_string(),
        SearchState::Found(result) => {
            let count = result.dump_files.len();
            let noun = if count == 1 { "dump" } else { "dumps" };
            format!("{count} {noun} in {}", result.request.s3_location())
        }
    }
}

#[component]
fn DumpFileList(result: SearchResult, downloads: DownloadJobs) -> Element {
    let mut group_filter = use_signal(|| None::<String>);
    let mut shown_limit = use_signal(|| DUMP_FILES_PER_PAGE);

    let location = result.request.s3_location();
    let total_count = result.dump_files.len();
    if total_count == 0 {
        return rsx! { p { class: "muted", "No matching files in {location}." } };
    }

    let latest_keys = find_latest_file_per_group(&result.dump_files);
    let groups: BTreeSet<&str> = result
        .dump_files
        .iter()
        .filter_map(DumpFile::group)
        .collect();
    let active_group = group_filter();
    let matching_files: Vec<&DumpFile> = result
        .dump_files
        .iter()
        .filter(|file| active_group.is_none() || file.group() == active_group.as_deref())
        .collect();
    let matching_count = matching_files.len();
    let shown_count = shown_limit().min(matching_count);
    let remaining_count = matching_count - shown_count;
    let next_page_count = remaining_count.min(DUMP_FILES_PER_PAGE);

    let summary = summarize_list(shown_count, matching_count, total_count);
    let now = read_current_epoch_secs();

    let mut show_group = move |group: Option<String>| {
        group_filter.set(group);
        shown_limit.set(DUMP_FILES_PER_PAGE);
    };

    rsx! {
        div { class: "results-toolbar",
            p { class: "muted results-summary", "{summary}" }
            if groups.len() > 1 {
                div { class: "chips",
                    FilterChip {
                        label: "All",
                        is_active: active_group.is_none(),
                        on_pick: move |_| show_group(None),
                    }
                    for group in groups {
                        FilterChip {
                            key: "{group}",
                            label: group.to_uppercase(),
                            is_active: active_group.as_deref() == Some(group),
                            on_pick: {
                                let group = group.to_string();
                                move |_| show_group(Some(group.clone()))
                            },
                        }
                    }
                }
            }
        }
        if result.is_truncated {
            p { class: "warning", "Stopped after scanning 50,000 objects. Pick a narrower folder." }
        }
        div { class: "dump-list",
            for file in matching_files.into_iter().take(shown_count) {
                DumpCard {
                    key: "{file.key}",
                    file: file.clone(),
                    is_latest: latest_keys.contains(file.key.as_str()),
                    now_epoch_secs: now,
                    profile: result.request.profile.clone(),
                    bucket: result.request.bucket.clone(),
                    downloads,
                }
            }
        }
        if remaining_count > 0 {
            div { class: "show-more",
                button {
                    class: "secondary",
                    onclick: move |_| shown_limit += DUMP_FILES_PER_PAGE,
                    "Show {next_page_count} more"
                }
                if remaining_count > DUMP_FILES_PER_PAGE {
                    button {
                        class: "link-button",
                        onclick: move |_| shown_limit.set(matching_count),
                        "Show all {matching_count}"
                    }
                }
            }
        }
    }
}

/// `"Showing 10 of 34 · newest first"`; adds the total when a group filter hides some.
/// (The S3 location is in the panel header.)
fn summarize_list(shown_count: usize, matching_count: usize, total_count: usize) -> String {
    let shown_part = if shown_count < matching_count {
        format!("Showing {shown_count} of {matching_count}")
    } else {
        format!("All {matching_count}")
    };
    let filtered_part = if matching_count < total_count {
        format!(" ({total_count} in total)")
    } else {
        String::new()
    };
    format!("{shown_part}{filtered_part} · newest first")
}

#[component]
fn FilterChip(label: String, is_active: bool, on_pick: EventHandler<()>) -> Element {
    rsx! {
        button {
            class: "chip",
            class: if is_active { "active" },
            onclick: move |_| on_pick.call(()),
            "{label}"
        }
    }
}

#[component]
fn DumpCard(
    file: DumpFile,
    is_latest: bool,
    now_epoch_secs: i64,
    profile: String,
    bucket: String,
    downloads: DownloadJobs,
) -> Element {
    let app = use_app_context();
    let mut is_naming_download = use_signal(|| false);
    let local_file_name = app
        .settings
        .read()
        .local_file_name(&file.key, file.file_name());
    let local_path = app.settings.read().download_path_for(&local_file_name);
    let download_fraction = find_download_fraction(&downloads.read(), &file.key);
    let is_on_disk =
        download_fraction.is_none() && has_complete_local_copy(&local_path, file.size_bytes);

    let age = file
        .last_modified_epoch_secs
        .map(|modified| format_relative_age(modified, now_epoch_secs));
    let exact_time = file
        .last_modified_epoch_secs
        .map(format_utc_timestamp)
        .unwrap_or_default();

    rsx! {
        div {
            class: "dump-card",
            class: if download_fraction.is_some() { "downloading" },
            div { class: "dump-icon", DatabaseIcon {} }
            div { class: "dump-main",
                div { class: "dump-title-row",
                    span { class: "dump-name", title: "{file.key}", "{file.file_name()}" }
                    if let Some(group) = file.group() {
                        GroupTag { group: group.to_string() }
                    }
                    if is_latest {
                        span { class: "badge-latest", "Latest" }
                    }
                }
                div { class: "dump-meta",
                    span { "{format_size(file.size_bytes)}" }
                    if let Some(age) = age {
                        span { class: "meta-dot", "·" }
                        span { title: "{exact_time}", "{age}" }
                    }
                    if is_on_disk {
                        span { class: "meta-dot", "·" }
                        if local_file_name == file.file_name() {
                            span { class: "on-disk", "✓ On disk" }
                        } else {
                            span { class: "on-disk", title: "{local_file_name}", "✓ On disk as {local_file_name}" }
                        }
                    }
                }
                if let Some(fraction) = download_fraction {
                    div { class: "progress-track",
                        div { class: "progress-fill", width: "{fraction * 100.0}%" }
                    }
                }
            }
            div { class: "dump-action",
                DumpCardAction {
                    download_fraction,
                    is_on_disk,
                    local_path: local_path.clone(),
                    on_download: move |_| is_naming_download.set(true),
                }
            }
        }
        if is_naming_download() {
            DownloadDialog {
                file: file.clone(),
                profile,
                bucket,
                downloads,
                on_close: move |_| is_naming_download.set(false),
            }
        }
    }
}

/// Progress of a running download of `s3_key` (0.0 ..= 1.0), if there is one.
fn find_download_fraction(jobs: &[DownloadJob], s3_key: &str) -> Option<f64> {
    jobs.iter()
        .find(|job| job.request.key == s3_key)
        .and_then(|job| match job.status {
            DownloadStatus::Running { progress, .. } => Some(progress.fraction_done()),
            _ => None,
        })
}

/// Right side of a dump card: progress, "Show in Finder" + "Sync…", or "Download".
#[component]
fn DumpCardAction(
    download_fraction: Option<f64>,
    is_on_disk: bool,
    local_path: PathBuf,
    on_download: EventHandler<()>,
) -> Element {
    let app = use_app_context();

    if let Some(fraction) = download_fraction {
        let percent = (fraction * 100.0).round();
        return rsx! { span { class: "dump-progress-label", "{percent}%" } };
    }
    if !is_on_disk {
        return rsx! {
            button { class: "small", onclick: move |_| on_download.call(()), "↓ Download" }
        };
    }

    let finder_path = local_path.clone();
    rsx! {
        div { class: "dump-action-buttons",
            button {
                class: "secondary small",
                onclick: move |_| reveal_in_finder(&finder_path),
                "Show in Finder"
            }
            button {
                class: "small",
                onclick: move |_| app.open_sync_for(local_path.clone()),
                "⇄ Sync…"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_summary_mentions_paging_and_filtering() {
        assert_eq!(
            summarize_list(10, 34, 34),
            "Showing 10 of 34 · newest first"
        );
        assert_eq!(summarize_list(5, 5, 5), "All 5 · newest first");
        assert_eq!(
            summarize_list(10, 17, 34),
            "Showing 10 of 17 (34 in total) · newest first"
        );
    }
}
