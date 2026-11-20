mod refresh_dialog;
mod restore_runner;
mod sync_panel;
mod target_form;
mod targets_panel;

use dioxus::prelude::*;

use crate::database::target::DatabaseTarget;
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
pub(crate) fn data_loss_warning(target: &DatabaseTarget) -> String {
    if target.recreate_database {
        return format!(
            "{database} is deleted and created again, empty, before the restore. Everything in it \
             is lost, including tables that exist only locally. Other connections to it are closed.",
            database = target.database
        );
    }
    format!(
        "Existing tables in {database} are dropped and recreated (pg_restore --clean). Any changes \
         you made in that database will be lost.",
        database = target.database
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warning_matches_the_restore_mode() {
        let mut target = DatabaseTarget {
            database: "gen_db".to_string(),
            ..DatabaseTarget::default()
        };
        assert!(data_loss_warning(&target).contains("pg_restore --clean"));

        target.recreate_database = true;
        let warning = data_loss_warning(&target);
        assert!(warning.starts_with("gen_db is deleted and created again"));
        assert!(warning.contains("Other connections to it are closed"));
    }
}
