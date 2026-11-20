use dioxus::prelude::*;

use crate::aws::download::DownloadRequest;
use crate::aws::s3::DumpFile;
use crate::database::target::DatabaseTarget;
use crate::dump_name::{validated_download_name, with_group_prefix};
use crate::format::format_size;
use crate::local_files::{display_local_path, local_path_for};
use crate::ui::app_context::use_app_context;
use crate::ui::downloads::{DownloadJobs, start_download};
use crate::ui::icons::DownloadIcon;
use crate::ui::modal::Modal;
use crate::ui::plain_text_input::PlainTextInput;

/// A one-click file name, e.g. "GEN-Optimus" -> `gen_2026-11-20T040016.dump`.
#[derive(Clone, Debug, PartialEq)]
struct NameSuggestion {
    label: String,
    file_name: String,
}

/// Asks for the local file name before downloading `file`.
#[component]
pub fn DownloadDialog(
    file: DumpFile,
    profile: String,
    bucket: String,
    downloads: DownloadJobs,
    on_close: EventHandler<()>,
) -> Element {
    let app = use_app_context();
    let s3_file_name = file.file_name().to_string();
    let typed_name = use_signal(|| s3_file_name.clone());

    let validated_name = validated_download_name(&typed_name());
    let destination = validated_name.as_deref().ok().map(local_path_for);
    let is_replacing_file = destination.as_ref().is_some_and(|path| path.exists());
    let suggestions = name_suggestions(&app.settings.read().database_targets, &s3_file_name);
    let subtitle = format!(
        "{} · s3://{bucket}/{}",
        format_size(file.size_bytes),
        file.key
    );

    let start = move || {
        let Ok(file_name) = validated_download_name(&typed_name()) else {
            return;
        };
        let request = DownloadRequest {
            profile: profile.clone(),
            bucket: bucket.clone(),
            key: file.key.clone(),
            destination: local_path_for(&file_name),
        };
        start_download(downloads, app.settings, request, file_name);
        on_close.call(());
    };
    let start_from_enter = start.clone();
    let start_from_button = start;

    rsx! {
        Modal {
            title: "Download dump",
            subtitle,
            icon: rsx! { DownloadIcon {} },
            on_close: move |_| on_close.call(()),

            div { class: "form-field",
                span { class: "field-label", "Save as" }
                PlainTextInput {
                    value: typed_name,
                    placeholder: "{s3_file_name}",
                    focus_on_mount: true,
                    on_enter: move |_| start_from_enter(),
                }
                if let Some(path) = &destination {
                    span { class: "field-hint mono", "{display_local_path(path)}" }
                }
            }

            div { class: "name-suggestions",
                span { class: "field-label", "Quick names" }
                div { class: "chips wrap",
                    NameChip {
                        label: "Original name",
                        file_name: s3_file_name.clone(),
                        typed_name,
                    }
                    for suggestion in suggestions.iter() {
                        NameChip {
                            key: "{suggestion.file_name}",
                            label: suggestion.label.clone(),
                            file_name: suggestion.file_name.clone(),
                            typed_name,
                        }
                    }
                }
                if suggestions.is_empty() {
                    span { class: "field-hint",
                        "Tip: give a database a Dump prefix (e.g. gen) on the Sync tab. "
                        "You'll get a one-click name here, and dumps named that way select their database automatically."
                    }
                } else {
                    span { class: "field-hint",
                        "A database's prefix in the name lets the Sync tab pick that database automatically."
                    }
                }
            }

            match &validated_name {
                Err(message) => rsx! { p { class: "form-alert error", "{message}" } },
                Ok(name) if is_replacing_file => rsx! {
                    p { class: "form-alert warning", "A file named {name} already exists and will be replaced." }
                },
                Ok(_) => rsx! {},
            }

            div { class: "modal-footer",
                div { class: "modal-footer-spacer" }
                button { class: "secondary", onclick: move |_| on_close.call(()), "Cancel" }
                button {
                    class: "with-icon",
                    disabled: validated_name.is_err(),
                    onclick: move |_| start_from_button(),
                    DownloadIcon {}
                    "Download"
                }
            }
        }
    }
}

#[component]
fn NameChip(label: String, file_name: String, typed_name: Signal<String>) -> Element {
    let is_active = typed_name.read().trim() == file_name;

    rsx! {
        button {
            class: "chip",
            class: if is_active { "active" },
            title: "{file_name}",
            onclick: move |_| typed_name.set(file_name.clone()),
            "{label}"
        }
    }
}

/// One suggestion per distinct dump prefix among the saved databases.
fn name_suggestions(targets: &[DatabaseTarget], s3_file_name: &str) -> Vec<NameSuggestion> {
    let mut suggestions: Vec<NameSuggestion> = Vec::new();
    for target in targets
        .iter()
        .filter(|target| !target.dump_group.is_empty())
    {
        let file_name = with_group_prefix(&target.dump_group, s3_file_name);
        if file_name == s3_file_name || suggestions.iter().any(|s| s.file_name == file_name) {
            continue;
        }
        suggestions.push(NameSuggestion {
            label: format!("{} · {}_", target.name, target.dump_group),
            file_name,
        });
    }
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, dump_group: &str) -> DatabaseTarget {
        DatabaseTarget {
            id: name.to_string(),
            name: name.to_string(),
            dump_group: dump_group.to_string(),
            ..DatabaseTarget::default()
        }
    }

    #[test]
    fn suggestions_come_from_distinct_dump_prefixes() {
        let targets = vec![
            target("General", "gen"),
            target("General copy", "gen"),
            target("Madrasa", "mad"),
            target("No prefix", ""),
        ];

        let suggestions = name_suggestions(&targets, "2026-11-20.dump");

        let names: Vec<&str> = suggestions.iter().map(|s| s.file_name.as_str()).collect();
        assert_eq!(names, vec!["gen_2026-11-20.dump", "mad_2026-11-20.dump"]);
        assert_eq!(suggestions[0].label, "General · gen_");
    }

    #[test]
    fn no_suggestion_when_name_already_has_the_prefix() {
        let suggestions = name_suggestions(&[target("General", "gen")], "gen_2026.dump");
        assert!(suggestions.is_empty());
    }
}
