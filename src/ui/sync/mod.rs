mod refresh_dialog;
mod restore_runner;
mod sync_panel;
mod target_form;
mod targets_panel;

use dioxus::prelude::*;

use crate::database::target::{DatabaseTarget, RestoreMode};
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

/// What a restore into `target` destroys, shown before every restore.
pub(crate) fn describe_data_loss(target: &DatabaseTarget) -> String {
    let database = &target.database;
    match target.restore_mode {
        RestoreMode::DropSchemas => format!(
            "Every schema in the dump (auth, core, …) is dropped from {database} with CASCADE, \
             including tables that exist only locally in those schemas. Other schemas are kept."
        ),
        RestoreMode::RecreateDatabase => format!(
            "{database} is deleted and created again, empty, before the restore. Everything in it \
             is lost, including tables that exist only locally. Other connections to it are closed."
        ),
        RestoreMode::ReplaceObjects => format!(
            "Existing tables in {database} are dropped and recreated (pg_restore --clean). Any changes \
             you made in that database will be lost."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_matches_the_restore_mode() {
        let warning_for = |restore_mode| {
            describe_data_loss(&DatabaseTarget {
                database: "prod_db".to_string(),
                restore_mode,
                ..DatabaseTarget::default()
            })
        };
        assert!(warning_for(RestoreMode::ReplaceObjects).contains("pg_restore --clean"));
        assert!(
            warning_for(RestoreMode::DropSchemas).contains("dropped from prod_db with CASCADE")
        );
        assert!(
            warning_for(RestoreMode::RecreateDatabase)
                .starts_with("prod_db is deleted and created again")
        );
    }
}
