//! Choices remembered between app launches, stored as JSON in
//! `~/Library/Application Support/db-refresh/settings.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::{env, fs, io};

use dioxus::logger::tracing;
use serde::{Deserialize, Serialize};

use crate::constants::{APP_FOLDER_NAME, SETTINGS_FILE_NAME};
use crate::database::target::DatabaseTarget;
use crate::local_files::resolve_default_download_folder;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
// Missing fields fall back to defaults, so adding a setting later does not break old files.
#[serde(default)]
pub struct Settings {
    pub profile: Option<String>,
    pub bucket: String,
    pub folder_prefix: String,
    pub database_targets: Vec<DatabaseTarget>,
    /// S3 key -> local file name, for dumps saved under a different name than in S3.
    pub downloaded_files: BTreeMap<String, String>,
    /// Where dumps are saved; `None` means the default, `~/Downloads`.
    pub custom_download_folder: Option<PathBuf>,
}

impl Settings {
    pub fn download_folder(&self) -> PathBuf {
        self.custom_download_folder
            .clone()
            .unwrap_or_else(resolve_default_download_folder)
    }

    pub fn download_path_for(&self, file_name: &str) -> PathBuf {
        self.download_folder().join(file_name)
    }

    /// The local file name a dump was (or will be) saved under.
    pub fn local_file_name(&self, s3_key: &str, s3_file_name: &str) -> String {
        self.downloaded_files
            .get(s3_key)
            .cloned()
            .unwrap_or_else(|| s3_file_name.to_string())
    }
}

/// Saved settings, or defaults when there are none yet (or the file is unreadable).
pub fn load_settings() -> Settings {
    let Some(path) = settings_path() else {
        return Settings::default();
    };
    match read_settings(&path) {
        Ok(settings) => settings,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Settings::default(),
        Err(error) => {
            tracing::warn!(
                "Ignoring unreadable settings at {}: {error}",
                path.display()
            );
            Settings::default()
        }
    }
}

/// Saving is best effort: a failure is logged, the app keeps working with the values in memory.
pub fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else {
        tracing::warn!("No home folder; settings are not saved.");
        return;
    };
    if let Err(error) = write_settings(&path, settings) {
        tracing::warn!("Could not save settings to {}: {error}", path.display());
    }
}

/// `~/Library/Application Support/db-refresh/settings.json`
fn settings_path() -> Option<PathBuf> {
    let home = env::home_dir()?;
    Some(
        home.join("Library")
            .join("Application Support")
            .join(APP_FOLDER_NAME)
            .join(SETTINGS_FILE_NAME),
    )
}

fn read_settings(path: &Path) -> io::Result<Settings> {
    let contents = fs::read_to_string(path)?;
    serde_json::from_str(&contents).map_err(io::Error::other)
}

// Write to a temp file and rename it into place, so a crash mid-write can't leave a
// half-written settings file behind.
fn write_settings(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let contents = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, contents)?;
    fs::rename(&temp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_settings_path(test_name: &str) -> PathBuf {
        env::temp_dir()
            .join(format!("db-refresh-{test_name}"))
            .join(SETTINGS_FILE_NAME)
    }

    #[test]
    fn settings_round_trip_through_file() {
        let path = temp_settings_path("round-trip");
        let settings = Settings {
            profile: Some("default".to_string()),
            bucket: "my-backups".to_string(),
            folder_prefix: "backups/gen/".to_string(),
            database_targets: vec![DatabaseTarget {
                id: "target-1".to_string(),
                name: "General local".to_string(),
                user: "app_user".to_string(),
                database: "gen_app_db".to_string(),
                dump_group: "gen".to_string(),
                ..DatabaseTarget::default()
            }],
            downloaded_files: BTreeMap::from([(
                "backups/2026-11-20.dump".to_string(),
                "gen_2026-11-20.dump".to_string(),
            )]),
            custom_download_folder: Some(PathBuf::from("/Volumes/Backups/dumps")),
        };

        write_settings(&path, &settings).unwrap();
        assert_eq!(read_settings(&path).unwrap(), settings);

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn local_file_name_prefers_the_name_used_when_downloading() {
        let settings = Settings {
            downloaded_files: BTreeMap::from([(
                "backups/2026.dump".to_string(),
                "gen_2026.dump".to_string(),
            )]),
            ..Settings::default()
        };
        assert_eq!(
            settings.local_file_name("backups/2026.dump", "2026.dump"),
            "gen_2026.dump"
        );
        assert_eq!(
            settings.local_file_name("backups/other.dump", "other.dump"),
            "other.dump"
        );
    }

    #[test]
    fn download_folder_defaults_to_downloads_and_can_be_changed() {
        let mut settings = Settings::default();
        assert_eq!(
            settings.download_folder(),
            resolve_default_download_folder()
        );

        settings.custom_download_folder = Some(PathBuf::from("/Volumes/Backups/dumps"));
        assert_eq!(
            settings.download_path_for("app.dump"),
            PathBuf::from("/Volumes/Backups/dumps/app.dump")
        );
    }

    #[test]
    fn missing_fields_use_defaults() {
        let settings: Settings = serde_json::from_str(r#"{ "bucket": "my-backups" }"#).unwrap();
        assert_eq!(settings.bucket, "my-backups");
        assert_eq!(settings.profile, None);
        assert_eq!(settings.folder_prefix, "");
    }

    #[test]
    fn corrupt_file_is_an_error_not_a_panic() {
        let path = temp_settings_path("corrupt");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();

        assert!(read_settings(&path).is_err());

        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
