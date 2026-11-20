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
            }
            if !targets.is_empty() {
                button {
                    class: "add-button full-width",
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
            div { class: "target-body",
                div { class: "target-icon", ServerIcon {} }
                div { class: "target-main",
                    div { class: "target-title-row",
                        span { class: "target-name", title: "{target.name}", "{target.name}" }
                        if !target.dump_group.is_empty() {
                            GroupTag { group: target.dump_group.clone() }
                        }
                    }
                    span { class: "target-database mono", "{target.database}" }
                    span { class: "target-connection mono", title: "{target.connection_label()}",
                        "{target.user}@{target.host}:{target.port}"
                    }
                    TargetBadges { target: target.clone() }
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
            div { class: "target-footer",
                TargetRefreshAction { target: target.clone(), is_restore_running, on_edit, on_refresh }
            }
        }
    }
}

/// The database's settings as small pills: password storage, Touch ID, restore mode.
#[component]
fn TargetBadges(target: DatabaseTarget) -> Element {
    rsx! {
        div { class: "target-badges",
            if target.is_password_saved {
                span { class: "target-badge", title: "The password is saved in the macOS Keychain", "🔑 Keychain" }
                if target.should_require_touch_id {
                    span { class: "target-badge", title: "Touch ID is asked before the saved password is used", "👆 Touch ID" }
                }
            }
            span { class: "target-badge", title: "{target.restore_mode.description()}",
                "🧹 {target.restore_mode.short_label()}"
            }
        }
    }
}

/// "Refresh latest" when a dump source is set, otherwise a link to set one up.
#[component]
fn TargetRefreshAction(
    target: DatabaseTarget,
    is_restore_running: bool,
    on_edit: EventHandler<()>,
    on_refresh: EventHandler<()>,
) -> Element {
    if !target.dump_source.is_configured() {
        return rsx! {
            button {
                class: "link-button setup-refresh",
                onclick: move |event| {
                    event.stop_propagation();
                    on_edit.call(());
                },
                "Set up one-click refresh →"
            }
        };
    }

    rsx! {
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
    }
}
