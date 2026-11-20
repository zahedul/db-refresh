use dioxus::prelude::*;

use crate::aws::profiles::{choose_profile, list_profile_names};
use crate::aws::s3_path::contains_ignoring_case;
use crate::ui::icons::ProfileIcon;
use crate::ui::picker::{
    PickerFooter, PickerList, PickerMessage, PickerPopover, PickerRow, PickerTrigger,
};
use crate::ui::plain_text_input::PlainTextInput;

/// Profile names, or a message explaining why they could not be read.
pub type ProfileList = Result<Vec<String>, String>;

pub fn load_profiles() -> ProfileList {
    list_profile_names().map_err(|error| format!("Could not read AWS profiles: {error}"))
}

#[component]
pub fn ProfilePicker(
    profiles: Signal<ProfileList>,
    selected_profile: Signal<Option<String>>,
) -> Element {
    let mut is_open = use_signal(|| false);
    let mut profile_filter = use_signal(String::new);

    let toggle_popover = move |_| {
        if !is_open() {
            profile_filter.set(String::new());
        }
        is_open.set(!is_open());
    };
    let pick_profile = move |profile: String| {
        selected_profile.set(Some(profile));
        is_open.set(false);
    };
    let reload_profiles = move |_| {
        let reloaded_profiles = load_profiles();
        if let Ok(names) = &reloaded_profiles {
            let current = selected_profile.peek().clone();
            selected_profile.set(choose_profile(names, current.as_deref()));
        }
        profiles.set(reloaded_profiles);
    };

    let trigger_label = selected_profile().unwrap_or_else(|| "Select a profile".to_string());

    rsx! {
        div { class: "picker",
            PickerTrigger {
                icon: rsx! { ProfileIcon {} },
                label: trigger_label,
                is_open: is_open(),
                on_toggle: toggle_popover,
            }
            if is_open() {
                PickerPopover { on_close: move |_| is_open.set(false),
                    PlainTextInput {
                        value: profile_filter,
                        placeholder: "Filter profiles…",
                        should_focus_on_mount: true,
                    }
                    ProfileOptions {
                        profiles: profiles.read().clone(),
                        selected_profile: selected_profile(),
                        profile_filter: profile_filter(),
                        on_pick: pick_profile,
                    }
                    PickerFooter {
                        button { class: "secondary", onclick: reload_profiles, "↻ Reload profiles" }
                    }
                }
            }
        }
    }
}

#[component]
fn ProfileOptions(
    profiles: ProfileList,
    selected_profile: Option<String>,
    profile_filter: String,
    on_pick: EventHandler<String>,
) -> Element {
    let names = match profiles {
        Ok(names) => names,
        Err(message) => return rsx! { PickerMessage { text: message, is_error: true } },
    };
    if names.is_empty() {
        return rsx! {
            PickerMessage { text: "No profiles found in ~/.aws. Run `aws configure sso` in a terminal." }
        };
    }

    let visible_names: Vec<String> = names
        .into_iter()
        .filter(|name| contains_ignoring_case(name, &profile_filter))
        .collect();
    if visible_names.is_empty() {
        return rsx! { PickerMessage { text: format!("No profiles match \"{profile_filter}\".") } };
    }

    rsx! {
        PickerList {
            for name in visible_names {
                PickerRow {
                    key: "{name}",
                    icon: rsx! { ProfileIcon {} },
                    label: name.clone(),
                    is_selected: selected_profile.as_deref() == Some(name.as_str()),
                    trailing: if selected_profile.as_deref() == Some(name.as_str()) { "✓" } else { "" },
                    on_pick: {
                        let name = name.clone();
                        move |_| on_pick.call(name.clone())
                    },
                }
            }
        }
    }
}
