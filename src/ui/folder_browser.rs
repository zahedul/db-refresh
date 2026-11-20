use dioxus::prelude::*;

use crate::aws::s3::list_folders;
use crate::aws::s3_path::{
    build_breadcrumbs, contains_ignoring_case, display_path, extract_folder_name,
};
use crate::error::AppError;
use crate::ui::icons::FolderIcon;
use crate::ui::picker::{
    PickerFooter, PickerList, PickerMessage, PickerPopover, PickerRow, PickerTrigger,
};
use crate::ui::plain_text_input::PlainTextInput;

/// The prefix a listing was made for, plus its result. Keeping the prefix lets the
/// popover show "Loading…" instead of a stale list while a new folder loads.
type FolderListing = (String, Result<Vec<String>, AppError>);

/// Button showing the chosen folder; opens a popover to browse into sub-folders.
///
/// Callers must key it by profile and bucket so it starts fresh when either changes.
#[component]
pub fn FolderBrowser(profile: String, bucket: String, prefix: Signal<String>) -> Element {
    let mut is_open = use_signal(|| false);
    let mut browsing_prefix = use_signal(String::new);
    let mut folder_filter = use_signal(String::new);

    let listing_bucket = bucket.clone();
    let listing = use_resource(move || {
        let (profile, bucket) = (profile.clone(), listing_bucket.clone());
        let parent_prefix = browsing_prefix();
        async move {
            let result = list_folders(&profile, &bucket, &parent_prefix).await;
            (parent_prefix, result)
        }
    });

    let mut browse_to = move |target_prefix: String| {
        browsing_prefix.set(target_prefix);
        folder_filter.set(String::new());
    };
    let toggle_popover = move |_| {
        if is_open() {
            is_open.set(false);
            return;
        }
        browse_to(prefix());
        is_open.set(true);
    };
    let use_browsing_folder = move |_| {
        prefix.set(browsing_prefix());
        is_open.set(false);
    };

    let current_prefix = browsing_prefix();

    rsx! {
        div { class: "picker",
            PickerTrigger {
                icon: rsx! { FolderIcon {} },
                label: display_path(&prefix()),
                is_open: is_open(),
                on_toggle: toggle_popover,
            }
            if is_open() {
                PickerPopover { on_close: move |_| is_open.set(false),
                    nav { class: "breadcrumbs",
                        button { class: "crumb", onclick: move |_| browse_to(String::new()), "{bucket}" }
                        for crumb in build_breadcrumbs(&current_prefix) {
                            span { key: "{crumb.prefix}", class: "crumb-item",
                                span { class: "crumb-separator", "›" }
                                button {
                                    class: "crumb",
                                    onclick: move |_| browse_to(crumb.prefix.clone()),
                                    "{crumb.label}"
                                }
                            }
                        }
                    }
                    PlainTextInput {
                        value: folder_filter,
                        placeholder: "Filter folders…",
                        should_focus_on_mount: true,
                    }
                    FolderOptions {
                        listing: listing.read().clone(),
                        current_prefix: current_prefix.clone(),
                        folder_filter: folder_filter(),
                        on_open_folder: move |folder: String| browse_to(folder),
                    }
                    PickerFooter {
                        button { onclick: use_browsing_folder,
                            "Use \"{display_path(&current_prefix)}\""
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn FolderOptions(
    listing: Option<FolderListing>,
    current_prefix: String,
    folder_filter: String,
    on_open_folder: EventHandler<String>,
) -> Element {
    let folders = match listing {
        Some((listed_prefix, result)) if listed_prefix == current_prefix => result,
        _ => return rsx! { PickerMessage { text: "Loading folders…" } },
    };
    let folders = match folders {
        Ok(folders) => folders,
        Err(error) => return rsx! { PickerMessage { text: error.to_string(), is_error: true } },
    };
    if folders.is_empty() {
        return rsx! { PickerMessage { text: "No sub-folders here." } };
    }

    let visible_folders: Vec<String> = folders
        .into_iter()
        .filter(|folder| {
            contains_ignoring_case(extract_folder_name(folder, &current_prefix), &folder_filter)
        })
        .collect();
    if visible_folders.is_empty() {
        return rsx! { PickerMessage { text: format!("No folders match \"{folder_filter}\".") } };
    }

    rsx! {
        PickerList {
            for folder in visible_folders {
                PickerRow {
                    key: "{folder}",
                    icon: rsx! { FolderIcon {} },
                    label: extract_folder_name(&folder, &current_prefix).to_string(),
                    trailing: "›",
                    on_pick: {
                        let folder = folder.clone();
                        move |_| on_open_folder.call(folder.clone())
                    },
                }
            }
        }
    }
}
