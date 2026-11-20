//! Building blocks shared by every popover picker (AWS profile, S3 folder), so
//! they look and behave the same: a trigger button, a popover that closes on
//! outside click or Escape, list rows and status messages.

use dioxus::prelude::*;

/// Button showing the current choice; opens/closes the picker's popover.
#[component]
pub fn PickerTrigger(
    icon: Element,
    label: String,
    is_open: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: "picker-trigger",
            class: if is_open { "open" },
            onclick: move |_| on_toggle.call(()),
            {icon}
            span { class: "picker-trigger-label", "{label}" }
            span { class: "picker-trigger-chevron", "▾" }
        }
    }
}

/// Floating panel below the trigger. Clicking outside or pressing Escape calls `on_close`.
/// Must be rendered inside an element with class `picker` (which positions it).
#[component]
pub fn PickerPopover(on_close: EventHandler<()>, children: Element) -> Element {
    let close_on_escape = move |event: KeyboardEvent| {
        if event.key() == Key::Escape {
            on_close.call(());
        }
    };

    rsx! {
        div { class: "backdrop", onclick: move |_| on_close.call(()) }
        div { class: "picker-popover", onkeydown: close_on_escape, {children} }
    }
}

#[component]
pub fn PickerList(children: Element) -> Element {
    rsx! {
        div { class: "picker-list", {children} }
    }
}

#[component]
pub fn PickerRow(
    icon: Element,
    label: String,
    /// Short marker on the right, e.g. "›" (opens) or "✓" (selected).
    trailing: String,
    #[props(default)] is_selected: bool,
    on_pick: EventHandler<()>,
) -> Element {
    rsx! {
        button {
            class: "picker-row",
            class: if is_selected { "selected" },
            onclick: move |_| on_pick.call(()),
            {icon}
            span { class: "picker-row-label", "{label}" }
            span { class: "picker-row-trailing", "{trailing}" }
        }
    }
}

#[component]
pub fn PickerMessage(text: String, #[props(default)] is_error: bool) -> Element {
    rsx! {
        p {
            class: "picker-message",
            class: if is_error { "error" } else { "muted" },
            "{text}"
        }
    }
}

#[component]
pub fn PickerFooter(children: Element) -> Element {
    rsx! {
        div { class: "picker-popover-footer", {children} }
    }
}
