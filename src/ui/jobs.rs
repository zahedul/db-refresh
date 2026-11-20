use dioxus::prelude::*;

use crate::format::{format_duration, format_size, format_speed};
use crate::local_files::{display_local_path, reveal_in_finder};
use crate::ui::app_context::use_app_context;
use crate::ui::download_folder::DownloadFolderControl;
use crate::ui::downloads::{DownloadJobs, cancel_download, dismiss_download, retry_download};
use crate::ui::icons::DownloadIcon;
use crate::ui::panel_header::PanelHeader;
use crate::ui::state::{DownloadJob, DownloadStatus};

#[component]
pub fn JobsPanel(downloads: DownloadJobs) -> Element {
    let jobs = downloads.read().clone();
    let running_count = jobs.iter().filter(|job| job.is_running()).count();
    let subtitle = match running_count {
        0 => "All downloads finished".to_string(),
        1 => "1 download running".to_string(),
        count => format!("{count} downloads running"),
    };

    if jobs.is_empty() {
        return rsx! {
            section { class: "panel jobs-panel empty",
                PanelHeader {
                    icon: rsx! { DownloadIcon {} },
                    title: "Downloads",
                    subtitle: "Nothing downloading",
                    is_compact: true,
                    DownloadFolderControl {}
                }
            }
        };
    }

    rsx! {
        section { class: "panel jobs-panel",
            PanelHeader {
                icon: rsx! { DownloadIcon {} },
                title: "Downloads",
                subtitle,
                DownloadFolderControl {}
            }
            for job in jobs {
                DownloadRow { key: "{job.request.key}", job, downloads }
            }
        }
    }
}

#[component]
fn DownloadRow(job: DownloadJob, downloads: DownloadJobs) -> Element {
    let key = job.request.key.clone();
    let destination = job.request.destination.clone();
    let app = use_app_context();

    let (status_line, fraction_done) = match &job.status {
        DownloadStatus::Running {
            progress,
            started_at,
        } => {
            let elapsed = started_at.elapsed();
            let mut parts = vec![format!(
                "{} of {}",
                format_size(progress.received_bytes),
                format_size(progress.total_bytes)
            )];
            if progress.received_bytes > 0 {
                parts.push(format_speed(progress.bytes_per_second(elapsed)));
            }
            if let Some(remaining) = progress.time_remaining(elapsed) {
                parts.push(format!("{} left", format_duration(remaining)));
            }
            (parts.join(" · "), Some(progress.fraction_done()))
        }
        DownloadStatus::Completed => (
            format!("Saved to {}", display_local_path(&destination)),
            None,
        ),
        DownloadStatus::Cancelled => ("Cancelled".to_string(), None),
        DownloadStatus::Failed(error) => (error.to_string(), None),
    };
    let is_failed = matches!(job.status, DownloadStatus::Failed(_));

    rsx! {
        div { class: "download-row",
            div { class: "download-header",
                span { class: "download-name", title: "{job.request.key}", "{job.file_name}" }
                div { class: "download-actions",
                    match job.status {
                        DownloadStatus::Running { .. } => rsx! {
                            button {
                                class: "secondary small",
                                onclick: move |_| cancel_download(downloads, &key),
                                "Cancel"
                            }
                        },
                        DownloadStatus::Completed => rsx! {
                            button {
                                class: "small",
                                onclick: {
                                    let destination = destination.clone();
                                    move |_| app.open_sync_for(destination.clone())
                                },
                                "⇄ Sync to database…"
                            }
                            button {
                                class: "secondary small",
                                onclick: move |_| reveal_in_finder(&destination),
                                "Show in Finder"
                            }
                            DismissButton { downloads, job_key: key }
                        },
                        DownloadStatus::Cancelled | DownloadStatus::Failed(_) => rsx! {
                            button {
                                class: "secondary small",
                                onclick: {
                                    let job = job.clone();
                                    move |_| retry_download(downloads, app.settings, &job)
                                },
                                "Retry"
                            }
                            DismissButton { downloads, job_key: key }
                        },
                    }
                }
            }
            if let Some(fraction) = fraction_done {
                div { class: "progress-track",
                    div {
                        class: "progress-fill",
                        width: "{fraction * 100.0}%",
                    }
                }
            }
            p {
                class: "download-status",
                class: if is_failed { "error" } else { "muted" },
                "{status_line}"
            }
        }
    }
}

#[component]
fn DismissButton(downloads: DownloadJobs, job_key: String) -> Element {
    rsx! {
        button {
            class: "icon-button",
            title: "Remove from list",
            onclick: move |_| dismiss_download(downloads, &job_key),
            "✕"
        }
    }
}
