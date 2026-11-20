use dioxus::prelude::*;

use crate::aws::s3::{SearchRequest, search_dump_files};
use crate::error::AppError;
use crate::ui::SearchState;
use crate::ui::app_context::use_app_context;
use crate::ui::folder_browser::FolderBrowser;
use crate::ui::icons::{CloudIcon, SearchIcon};
use crate::ui::panel_header::PanelHeader;
use crate::ui::plain_text_input::PlainTextInput;
use crate::ui::profile_picker::{ProfileList, ProfilePicker};

#[component]
pub fn ConnectPanel(
    profiles: Signal<ProfileList>,
    selected_profile: Signal<Option<String>>,
    search_state: Signal<SearchState>,
) -> Element {
    let mut settings = use_app_context().settings;
    let mut bucket = use_signal(|| settings.peek().bucket.clone());
    // Updated only when the bucket field is committed (Enter / leaving the field),
    // so folders are not fetched on every keystroke.
    let mut committed_bucket = use_signal(|| settings.peek().bucket.clone());
    let mut prefix = use_signal(|| settings.peek().folder_prefix.clone());
    let name_filter = use_signal(String::new);

    // Re-runs whenever one of these signals changes, so every choice is remembered
    // (App saves `settings` to disk when it changes).
    use_effect(move || {
        let (profile, bucket_name, folder_prefix) =
            (selected_profile(), committed_bucket(), prefix());
        let is_unchanged = {
            let current = settings.peek();
            current.profile == profile
                && current.bucket == bucket_name
                && current.folder_prefix == folder_prefix
        };
        if is_unchanged {
            return;
        }
        settings.with_mut(|current| {
            current.profile = profile;
            current.bucket = bucket_name;
            current.folder_prefix = folder_prefix;
        });
    });

    let mut start_search = move || {
        if search_state.peek().is_searching() {
            return;
        }
        let Some(profile) = selected_profile() else {
            search_state.set(SearchState::Failed(AppError::MissingProfile));
            return;
        };
        let request = SearchRequest {
            profile,
            bucket: bucket(),
            prefix: prefix(),
            name_filter: name_filter(),
        };

        search_state.set(SearchState::Searching {
            s3_location: request.s3_location(),
        });
        spawn(async move {
            let next_state = match search_dump_files(request).await {
                Ok(result) => SearchState::Found(result),
                Err(error) => SearchState::Failed(error),
            };
            search_state.set(next_state);
        });
    };

    rsx! {
        section { class: "panel",
            PanelHeader {
                icon: rsx! { CloudIcon {} },
                title: "S3 source",
                subtitle: "Where your backups live",
            }

            p { class: "form-section-title", "Account" }
            label { class: "field-label", "AWS profile" }
            ProfilePicker { profiles, selected_profile }

            p { class: "form-section-title", "Location" }
            label { class: "field-label", "Bucket" }
            PlainTextInput {
                value: bucket,
                placeholder: "my-backup-bucket",
                on_commit: move |typed: String| {
                    // S3 bucket names are always lowercase.
                    let bucket_name = typed.trim().to_lowercase();
                    bucket.set(bucket_name.clone());
                    if bucket_name == *committed_bucket.peek() {
                        return;
                    }
                    // Folders belong to a bucket, so a new bucket starts at its root.
                    prefix.set(String::new());
                    committed_bucket.set(bucket_name);
                },
            }
            label { class: "field-label", "Folder" }
            match (selected_profile(), committed_bucket()) {
                (Some(profile), bucket_name) if !bucket_name.is_empty() => rsx! {
                    FolderBrowser {
                        key: "{profile}|{bucket_name}",
                        profile,
                        bucket: bucket_name,
                        prefix,
                    }
                },
                _ => rsx! { p { class: "muted", "Enter a bucket to load its folders." } },
            }
            label { class: "field-label", "Name contains (optional)" }
            PlainTextInput {
                value: name_filter,
                placeholder: "prod_",
                on_enter: move |_| start_search(),
            }
            SearchButton {
                is_searching: search_state.read().is_searching(),
                missing_input: missing_search_input(selected_profile.read().is_some(), &bucket()),
                location: search_location(&bucket(), &prefix()),
                on_search: move |_| start_search(),
            }
        }
    }
}

#[component]
fn SearchButton(
    is_searching: bool,
    /// Why searching is not possible yet, if it isn't.
    missing_input: Option<String>,
    location: String,
    on_search: EventHandler<()>,
) -> Element {
    let is_disabled = is_searching || missing_input.is_some();

    rsx! {
        div { class: "search-action",
            button {
                class: "search-button",
                disabled: is_disabled,
                onclick: move |_| on_search.call(()),
                if is_searching {
                    span { class: "spinner" }
                    "Searching…"
                } else {
                    SearchIcon {}
                    "Search dumps"
                }
            }
            match missing_input {
                Some(reason) => rsx! { span { class: "search-hint", "{reason}" } },
                None => rsx! {
                    span { class: "search-hint mono", title: "{location}", "in {location}" }
                },
            }
        }
    }
}

fn missing_search_input(has_profile: bool, bucket: &str) -> Option<String> {
    if !has_profile {
        return Some("Select an AWS profile to search.".to_string());
    }
    if bucket.trim().is_empty() {
        return Some("Enter a bucket to search.".to_string());
    }
    None
}

fn search_location(bucket: &str, prefix: &str) -> String {
    format!("s3://{}/{prefix}", bucket.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_needs_profile_then_bucket() {
        assert_eq!(
            missing_search_input(false, "bucket").as_deref(),
            Some("Select an AWS profile to search.")
        );
        assert_eq!(
            missing_search_input(true, "  ").as_deref(),
            Some("Enter a bucket to search.")
        );
        assert_eq!(missing_search_input(true, "bucket"), None);
    }

    #[test]
    fn search_location_shows_bucket_and_folder() {
        assert_eq!(
            search_location(" prod-db-backup ", "backups/"),
            "s3://prod-db-backup/backups/"
        );
    }
}
