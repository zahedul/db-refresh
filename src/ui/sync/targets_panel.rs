use dioxus::prelude::*;

use crate::database::target::DatabaseTarget;
use crate::ui::app_context::use_app_context;
use crate::ui::icons::{PencilIcon, PlusIcon, RefreshIcon, ServerIcon};
use crate::ui::panel_header::PanelHeader;
use crate::ui::sync::refresh_dialog::RefreshLatestDialog;
use crate::ui::sync::target_form::TargetFormDialog;
use crate::ui::tags::GroupTag;

/// Draft being edited, and whether it is a new target.
type EditingTarget = Option<(DatabaseTarget, bool)>;

#[component]
pub fn TargetsPanel() -> Element {
    let app = use_app_context();
    let mut editing = use_signal(|| EditingTarget::None);
    let mut refreshing = use_signal(|| None::<DatabaseTarget>);
    let is_restore_running = app.restore.read().is_running();
    let targets = app.settings.read().database_targets.clone();
    let selected_target_id = (app.selected_target_id)();
    let mut start_new_target = move || editing.set(Some((DatabaseTarget::new_draft(), true)));

    rsx! {
        section { class: "panel targets-panel",
            PanelHeader {
                icon: rsx! { ServerIcon {} },
                title: "Databases",
                subtitle: "Where dumps get restored",
                button {
                    class: "add-button",
                    title: "Add a database to restore into",
                    onclick: move |_| start_new_target(),
                    PlusIcon {}
                    "Add database"
                }
            }
            if targets.is_empty() {
                button {
                    class: "add-first-target",
                    onclick: move |_| start_new_target(),
                    span { class: "add-first-target-icon", PlusIcon {} }
                    span { class: "add-first-target-title", "Add your first database" }
                    span { class: "muted",
                        "For example a local copy of staging or production. You can add as many as you need."
                    }
                }
            }
            div { class: "target-list",
                for target in targets {
                    TargetCard {
                        key: "{target.id}",
                        is_selected: selected_target_id.as_deref() == Some(target.id.as_str()),
                        is_restore_running,
                        on_edit: {
                            let target = target.clone();
                            move |_| editing.set(Some((target.clone(), false)))
                        },
                        on_refresh: {
                            let target = target.clone();
                            move |_| refreshing.set(Some(target.clone()))
                        },
                        target,
                    }
                }
            }
        }
        if let Some((draft, is_new)) = editing() {
            TargetFormDialog { draft, is_new, on_close: move |_| editing.set(None) }
        }
        if let Some(target) = refreshing() {
            RefreshLatestDialog { target, on_close: move |_| refreshing.set(None) }
        }
    }
}

#[component]
fn TargetCard(
    target: DatabaseTarget,
    is_selected: bool,
    is_restore_running: bool,
    on_edit: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    let mut selected_target_id = use_app_context().selected_target_id;
    let target_id = target.id.clone();

    rsx! {
        div {
            class: "target-card",
            class: if is_selected { "selected" },
            onclick: move |_| selected_target_id.set(Some(target_id.clone())),
            div { class: "target-icon", ServerIcon {} }
            div { class: "target-main",
                div { class: "target-title-row",
                    span { class: "target-name", "{target.name}" }
                    if !target.dump_group.is_empty() {
                        GroupTag { group: target.dump_group.clone() }
                    }
                }
                span { class: "target-database mono", "{target.database}" }
                span { class: "target-connection", title: "{target.connection_label()}",
                    "{target.user} · {target.host}:{target.port}"
                }
                if target.is_password_saved {
                    span { class: "target-keychain",
                        if target.should_require_touch_id { "🔑 Password in Keychain · Touch ID" } else { "🔑 Password in Keychain" }
                    }
                }
                span { class: "target-keychain", title: "{target.restore_mode.description()}",
                    "🧹 {target.restore_mode.label()}"
                }
                if target.dump_source.is_configured() {
                    button {
                        class: "refresh-latest-button",
                        title: "Download the newest dump from {target.dump_source.s3_location()} and restore it",
                        disabled: is_restore_running,
                        onclick: move |event| {
                            event.stop_propagation();
                            on_refresh.call(());
                        },
                        RefreshIcon {}
                        "Refresh latest"
                    }
                } else {
                    button {
                        class: "link-button setup-refresh",
                        onclick: move |event| {
                            event.stop_propagation();
                            on_edit.call(());
                        },
                        "Set up one-click refresh →"
                    }
                }
            }
            button {
                class: "icon-button edit-button",
                title: "Edit {target.name}",
                onclick: move |event| {
                    // Editing must not also select the card underneath.
                    event.stop_propagation();
                    on_edit.call(());
                },
                PencilIcon {}
            }
        }
    }
}
