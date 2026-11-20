mod restore_runner;
mod sync_panel;
mod target_form;
mod targets_panel;

use dioxus::prelude::*;

use sync_panel::SyncPanel;
use targets_panel::TargetsPanel;

/// Restore a downloaded dump into one of the saved databases.
#[component]
pub fn SyncView() -> Element {
    rsx! {
        main { class: "sync-layout",
            TargetsPanel {}
            SyncPanel {}
        }
    }
}
