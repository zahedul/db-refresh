use std::path::PathBuf;

use dioxus::prelude::*;

use crate::database::log_line::RestoreLog;
use crate::database::target::suggest_target;
use crate::settings::Settings;
use crate::ui::state::RestoreState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppView {
    Download,
    Sync,
    Help,
}

/// App-wide state, provided once by `App` and read anywhere with `use_app_context()`.
/// Every field is a `Signal`, so the whole struct is a cheap `Copy` handle.
#[derive(Clone, Copy)]
pub struct AppContext {
    pub view: Signal<AppView>,
    /// Remembered between launches; saved automatically whenever it changes.
    pub settings: Signal<Settings>,
    pub selected_dump: Signal<Option<PathBuf>>,
    pub selected_target_id: Signal<Option<String>>,
    pub restore: Signal<RestoreState>,
    pub restore_log: Signal<RestoreLog>,
}

impl AppContext {
    /// Selects the dump and, if one matches its name (`gen_…` -> target with group `gen`),
    /// the database it should go to.
    pub fn select_dump(mut self, dump_path: PathBuf) {
        let file_name = dump_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let suggested_target_id =
            suggest_target(&self.settings.read().database_targets, &file_name)
                .map(|target| target.id.clone());

        if let Some(target_id) = suggested_target_id {
            self.selected_target_id.set(Some(target_id));
        }
        self.selected_dump.set(Some(dump_path));
    }

    /// Jumps from a downloaded file to the Sync view with that file selected.
    pub fn open_sync_for(mut self, dump_path: PathBuf) {
        self.select_dump(dump_path);
        self.view.set(AppView::Sync);
    }
}

pub fn use_app_context() -> AppContext {
    use_context::<AppContext>()
}
