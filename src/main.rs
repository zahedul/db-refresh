mod aws;
mod clipboard;
mod database;
mod dump_name;
mod error;
mod format;
mod legacy_migration;
mod local_files;
mod settings;
mod ui;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use dioxus::prelude::*;

use aws::profiles::choose_profile;
use settings::{load_settings, save_settings};
use ui::{
    AppContext, AppView, ConnectPanel, JobsPanel, RestoreState, ResultsPanel, SearchState,
    SyncView, load_profiles,
};

const MAIN_CSS: Asset = asset!("/assets/main.css");
const APP_TITLE: &str = "DB Refresh";

const WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(1200.0, 800.0);
const MIN_WINDOW_SIZE: LogicalSize<f64> = LogicalSize::new(960.0, 640.0);

fn main() {
    legacy_migration::migrate_legacy_folders();

    let window = WindowBuilder::new()
        .with_title(APP_TITLE)
        .with_inner_size(WINDOW_SIZE)
        .with_min_inner_size(MIN_WINDOW_SIZE);
    // Keep Dioxus' default menu: on macOS, ⌘C / ⌘V / ⌘Q in text fields only work
    // when the app has an Edit menu.
    let desktop_config = Config::new().with_window(window);

    LaunchBuilder::desktop()
        .with_cfg(desktop_config)
        .launch(App);
}

#[component]
fn App() -> Element {
    let settings = use_signal(load_settings);
    // Re-runs whenever settings change, from any view, so nothing needs an explicit save.
    use_effect(move || save_settings(&settings.read()));

    let app = use_context_provider(|| AppContext {
        view: Signal::new(AppView::Download),
        settings,
        selected_dump: Signal::new(None),
        selected_target_id: Signal::new(None),
        restore: Signal::new(RestoreState::Idle),
        restore_log: Signal::new(Vec::new()),
    });

    let profiles = use_signal(load_profiles);
    let selected_profile = use_signal(|| {
        let saved_profile = settings.peek().profile.clone();
        profiles
            .peek()
            .as_ref()
            .ok()
            .and_then(|names| choose_profile(names, saved_profile.as_deref()))
    });
    let search_state = use_signal(|| SearchState::Idle);
    let downloads = use_signal(Vec::new);

    rsx! {
        document::Title { "{APP_TITLE}" }
        document::Stylesheet { href: MAIN_CSS }
        div { class: "app",
            AppHeader {}
            match (app.view)() {
                AppView::Download => rsx! {
                    main { class: "layout",
                        ConnectPanel { profiles, selected_profile, search_state }
                        ResultsPanel { selected_profile, search_state, downloads }
                        JobsPanel { downloads }
                    }
                },
                AppView::Sync => rsx! { SyncView {} },
            }
        }
    }
}

#[component]
fn AppHeader() -> Element {
    let app = ui::use_app_context();
    let is_restoring = app.restore.read().is_running();

    rsx! {
        header { class: "app-header",
            span { class: "app-title", "{APP_TITLE}" }
            nav { class: "tabs",
                TabButton { label: "↓ Download", view: AppView::Download }
                TabButton { label: "⇄ Sync database", view: AppView::Sync, is_busy: is_restoring }
            }
        }
    }
}

#[component]
fn TabButton(label: String, view: AppView, #[props(default)] is_busy: bool) -> Element {
    let mut current_view = ui::use_app_context().view;

    rsx! {
        button {
            class: "tab",
            class: if current_view() == view { "active" },
            onclick: move |_| current_view.set(view),
            "{label}"
            if is_busy {
                span { class: "busy-dot", title: "Sync running" }
            }
        }
    }
}
