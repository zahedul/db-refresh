use dioxus::prelude::*;

use crate::aws::profiles::choose_profile;
use crate::constants::{APP_TITLE, MAIN_CSS};
use crate::database::log_line::RestoreLog;
use crate::settings::{load_settings, save_settings};
use crate::ui::app_context::{AppContext, AppView, use_app_context};
use crate::ui::connect::ConnectPanel;
use crate::ui::help::HelpView;
use crate::ui::jobs::JobsPanel;
use crate::ui::profile_picker::load_profiles;
use crate::ui::results::ResultsPanel;
use crate::ui::state::{RestoreState, SearchState};
use crate::ui::sync::SyncView;

/// Root component: shared state, the tab bar and the active view.
#[component]
pub fn App() -> Element {
    let settings = use_signal(load_settings);
    // Re-runs whenever settings change, from any view, so nothing needs an explicit save.
    use_effect(move || save_settings(&settings.read()));

    let app = use_context_provider(|| AppContext {
        view: Signal::new(AppView::Download),
        settings,
        selected_dump: Signal::new(None),
        selected_target_id: Signal::new(None),
        restore: Signal::new(RestoreState::Idle),
        restore_log: Signal::new(RestoreLog::default()),
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
                AppView::Help => rsx! { HelpView {} },
            }
        }
    }
}

#[component]
fn AppHeader() -> Element {
    let app = use_app_context();
    let is_restoring = app.restore.read().is_running();

    rsx! {
        header { class: "app-header",
            span { class: "app-title", "{APP_TITLE}" }
            nav { class: "tabs",
                TabButton { label: "↓ Download", view: AppView::Download }
                TabButton { label: "⇄ Sync database", view: AppView::Sync, is_busy: is_restoring }
                TabButton { label: "? Help", view: AppView::Help }
            }
        }
    }
}

#[component]
fn TabButton(label: String, view: AppView, #[props(default)] is_busy: bool) -> Element {
    let mut current_view = use_app_context().view;

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
