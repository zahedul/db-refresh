//! The app was first called "Optimus DB Refresh". This moves data saved under the old
//! name to the current locations once, so settings and downloads carry over.
//! (Keychain passwords are moved lazily in `database::credentials`.)

use std::fs;
use std::path::Path;

use dioxus::logger::tracing;

use crate::local_files::download_dir;
use crate::settings::{app_data_dir, settings_dir};

const LEGACY_APP_FOLDER_NAME: &str = "optimus-db-refresh";
const LEGACY_DOWNLOAD_FOLDER_NAME: &str = "optimus-dumps";

pub fn migrate_legacy_folders() {
    if let (Some(legacy_settings), Some(current_settings)) =
        (app_data_dir(LEGACY_APP_FOLDER_NAME), settings_dir())
    {
        move_folder_if_unused(&legacy_settings, &current_settings);
    }

    let current_downloads = download_dir();
    if let Some(downloads_parent) = current_downloads.parent() {
        let legacy_downloads = downloads_parent.join(LEGACY_DOWNLOAD_FOLDER_NAME);
        move_folder_if_unused(&legacy_downloads, &current_downloads);
    }
}

/// Renames `from` to `to` when `from` exists and `to` does not. Never overwrites, and a
/// rename on the same disk is instant even for gigabytes of dumps.
fn move_folder_if_unused(from: &Path, to: &Path) {
    if !from.is_dir() || to.exists() {
        return;
    }
    match fs::rename(from, to) {
        Ok(()) => tracing::info!("Moved {} to {}", from.display(), to.display()),
        Err(error) => tracing::warn!(
            "Could not move {} to {}: {error}",
            from.display(),
            to.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn fresh_dir(name: &str) -> std::path::PathBuf {
        let dir = env::temp_dir().join(format!("db-refresh-migration-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn moves_legacy_folder_when_new_one_is_missing() {
        let root = fresh_dir("move");
        let (legacy, current) = (root.join("old"), root.join("new"));
        fs::create_dir_all(&legacy).unwrap();
        fs::write(legacy.join("settings.json"), "{}").unwrap();

        move_folder_if_unused(&legacy, &current);

        assert!(!legacy.exists());
        assert!(current.join("settings.json").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn never_overwrites_an_existing_new_folder() {
        let root = fresh_dir("keep");
        let (legacy, current) = (root.join("old"), root.join("new"));
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&current).unwrap();
        fs::write(current.join("settings.json"), "new").unwrap();

        move_folder_if_unused(&legacy, &current);

        assert!(legacy.exists());
        assert_eq!(
            fs::read_to_string(current.join("settings.json")).unwrap(),
            "new"
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
