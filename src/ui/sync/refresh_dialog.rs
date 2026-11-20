use dioxus::prelude::*;

use crate::aws::s3::{DumpFile, SearchRequest, search_dump_files};
use crate::database::credentials::password_for;
use crate::database::target::{DatabaseTarget, DumpSource};
use crate::dump_name::with_group_prefix;
use crate::error::AppError;
use crate::format::{format_relative_age, format_size, now_epoch_secs};
use crate::local_files::{has_complete_local_copy, local_path_for};
use crate::ui::app_context::use_app_context;
use crate::ui::icons::RefreshIcon;
use crate::ui::modal::Modal;
use crate::ui::sync::restore_runner::{RefreshPlan, start_refresh};

/// Finds the newest dump in the target's source and asks before replacing the data.
#[component]
pub fn RefreshLatestDialog(target: DatabaseTarget, on_close: EventHandler<()>) -> Element {
    let app = use_app_context();
    let mut typed_password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let source = target.dump_source.clone();
    let latest_dump = use_resource(move || {
        let source = source.clone();
        async move { find_latest_dump(&source).await }
    });

    // Only a found dump can be refreshed; everything else shows a message instead.
    let found_dump = match &*latest_dump.read() {
        Some(Ok(Some(dump))) => Some(dump.clone()),
        _ => None,
    };
    let plan_parts = found_dump.as_ref().map(|dump| {
        let previous_name = app.settings.read().downloaded_files.get(&dump.key).cloned();
        let local_file_name = refresh_file_name(
            previous_name.as_deref(),
            dump.file_name(),
            &target.dump_group,
        );
        let is_on_disk =
            has_complete_local_copy(&local_path_for(&local_file_name), dump.size_bytes);
        (dump.clone(), local_file_name, is_on_disk)
    });

    let confirm = {
        let (target, plan_parts) = (target.clone(), plan_parts.clone());
        move |_| {
            let Some((latest_dump, local_file_name, is_on_disk)) = plan_parts.clone() else {
                return;
            };
            let password = match password_for(&target, &typed_password()) {
                Ok(password) => password,
                Err(password_error) => {
                    error.set(Some(password_error.to_string()));
                    return;
                }
            };
            start_refresh(
                app,
                RefreshPlan {
                    target: target.clone(),
                    source: target.dump_source.clone(),
                    latest_dump,
                    local_file_name,
                    needs_download: !is_on_disk,
                    password,
                },
            );
            on_close.call(());
        }
    };

    rsx! {
        Modal {
            title: format!("Refresh {}", target.database),
            subtitle: format!("Newest dump from {}", target.dump_source.s3_location()),
            icon: rsx! { RefreshIcon {} },
            on_close: move |_| on_close.call(()),

            match &*latest_dump.read() {
                None => rsx! {
                    div { class: "empty-state compact",
                        span { class: "spinner" }
                        p { class: "muted", "Finding the newest dump…" }
                    }
                },
                Some(Err(search_error)) => rsx! {
                    p { class: "form-alert error", "{search_error}" }
                },
                Some(Ok(None)) => rsx! {
                    p { class: "form-alert warning",
                        "No dump files found in {target.dump_source.s3_location()}. "
                        "Edit the database to check its source."
                    }
                },
                Some(Ok(Some(_))) => rsx! {},
            }

            if let Some((dump, local_file_name, is_on_disk)) = &plan_parts {
                RefreshPreview {
                    dump: dump.clone(),
                    local_file_name: local_file_name.clone(),
                    is_on_disk: *is_on_disk,
                    target: target.clone(),
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
            }
            if let Some(message) = error() {
                p { class: "form-alert error", "{message}" }
            }

            div { class: "modal-footer",
                div { class: "modal-footer-spacer" }
                button { class: "secondary", onclick: move |_| on_close.call(()), "Cancel" }
                button {
                    class: "danger with-icon",
                    disabled: plan_parts.is_none(),
                    onclick: confirm,
                    RefreshIcon {}
                    "Refresh now"
                }
            }
        }
    }
}

/// `2026-11-20T040016.dump · 1.0 GB · 6h ago` → `gen_optimus_db`, with what will happen.
#[component]
fn RefreshPreview(
    dump: DumpFile,
    local_file_name: String,
    is_on_disk: bool,
    target: DatabaseTarget,
) -> Element {
    let age = dump
        .last_modified_epoch_secs
        .map(|modified| format_relative_age(modified, now_epoch_secs()))
        .unwrap_or_default();
    let download_step = if is_on_disk {
        "Already on disk, no download needed".to_string()
    } else {
        format!(
            "Download {} as {local_file_name}",
            format_size(dump.size_bytes)
        )
    };

    rsx! {
        div { class: "refresh-preview",
            div { class: "refresh-preview-dump",
                span { class: "field-label", "Latest dump" }
                span { class: "dump-name", title: "{dump.key}", "{dump.file_name()}" }
                span { class: "muted", "{format_size(dump.size_bytes)} · {age}" }
            }
            ol { class: "refresh-steps",
                li { class: if is_on_disk { "skipped" }, "{download_step}" }
                li { "Restore into " strong { "{target.database}" } " on {target.host}:{target.port}" }
            }
        }
    }
}

/// Newest dump in the source (search results are sorted newest first).
async fn find_latest_dump(source: &DumpSource) -> Result<Option<DumpFile>, AppError> {
    let request = SearchRequest {
        profile: source.profile.clone(),
        bucket: source.bucket.clone(),
        prefix: source.folder_prefix.clone(),
        name_filter: source.name_filter.clone(),
    };
    let result = search_dump_files(request).await?;
    Ok(result.dump_files.into_iter().next())
}

/// The name used before for this dump wins (so it isn't downloaded twice); otherwise
/// the target's prefix is added, so the file is recognised as this database's dump.
fn refresh_file_name(previous_name: Option<&str>, s3_file_name: &str, dump_group: &str) -> String {
    if let Some(previous_name) = previous_name {
        return previous_name.to_string();
    }
    if dump_group.is_empty() {
        return s3_file_name.to_string();
    }
    with_group_prefix(dump_group, s3_file_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_file_name_reuses_earlier_name_then_adds_prefix() {
        assert_eq!(
            refresh_file_name(Some("mine.dump"), "2026.dump", "gen"),
            "mine.dump"
        );
        assert_eq!(refresh_file_name(None, "2026.dump", "gen"), "gen_2026.dump");
        assert_eq!(refresh_file_name(None, "2026.dump", ""), "2026.dump");
    }
}
