//! Where downloaded dumps live on this Mac, and small helpers around those files.

use std::cmp::Reverse;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;
use std::{env, fs, io};

use crate::constants::DUMP_EXTENSION;
use dioxus::logger::tracing;

/// A dump file on this Mac that can be restored.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalDump {
    pub path: PathBuf,
    pub file_name: String,
    pub size_bytes: u64,
    pub modified_epoch_secs: Option<i64>,
}

impl LocalDump {
    /// `None` when the path is not an existing file.
    pub fn from_path(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path)
            .ok()
            .filter(|metadata| metadata.is_file())?;
        let modified_epoch_secs = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|since_epoch| since_epoch.as_secs() as i64);

        Some(Self {
            path: path.to_path_buf(),
            file_name: path.file_name()?.to_string_lossy().to_string(),
            size_bytes: metadata.len(),
            modified_epoch_secs,
        })
    }
}

/// `*.dump` files in `dir`, newest first. Unfinished `.dump.part` downloads are skipped.
pub fn list_local_dumps(dir: &Path) -> Vec<LocalDump> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                tracing::warn!("Could not list dumps in {}: {error}", dir.display());
            }
            return Vec::new();
        }
    };

    let mut dumps: Vec<LocalDump> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == DUMP_EXTENSION)
        })
        .filter_map(|path| LocalDump::from_path(&path))
        .collect();
    dumps.sort_by_key(|dump| Reverse(dump.modified_epoch_secs));
    dumps
}

/// `~/Downloads` - where dumps are saved unless the user picks another folder.
pub fn resolve_default_download_folder() -> PathBuf {
    let home = env::home_dir().unwrap_or_else(env::temp_dir);
    home.join("Downloads")
}

/// True when the file is already on disk with the expected size, so it need not be downloaded again.
pub fn has_complete_local_copy(path: &Path, expected_size_bytes: u64) -> bool {
    fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected_size_bytes)
}

/// `/Users/me/Downloads/x.dump` -> `~/Downloads/x.dump`
pub fn display_local_path(path: &Path) -> String {
    let full_path = path.display().to_string();
    let Some(home) = env::home_dir() else {
        return full_path;
    };
    match path.strip_prefix(&home) {
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => full_path,
    }
}

pub fn reveal_in_finder(path: &Path) {
    if let Err(error) = Command::new("open").arg("-R").arg(path).spawn() {
        tracing::warn!("Could not reveal {} in Finder: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_download_folder_is_downloads() {
        assert!(resolve_default_download_folder().ends_with("Downloads"));
    }

    #[test]
    fn complete_local_copy_requires_matching_size() {
        let path = env::temp_dir().join("db-refresh-size-check.dump");
        fs::write(&path, b"12345").expect("temp file should be writable");

        assert!(has_complete_local_copy(&path, 5));
        assert!(!has_complete_local_copy(&path, 6));

        fs::remove_file(&path).expect("temp file should be removable");
        assert!(!has_complete_local_copy(&path, 5));
    }

    #[test]
    fn list_local_dumps_skips_partial_and_other_files() {
        let dir = env::temp_dir().join("db-refresh-list-test");
        fs::create_dir_all(&dir).unwrap();
        for name in ["prod_1.dump", "stg_1.dump", "prod_2.dump.part", "notes.txt"] {
            fs::write(dir.join(name), b"x").unwrap();
        }

        let mut names: Vec<String> = list_local_dumps(&dir)
            .into_iter()
            .map(|dump| dump.file_name)
            .collect();
        names.sort();

        assert_eq!(names, vec!["prod_1.dump", "stg_1.dump"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_dump_folder_lists_nothing() {
        assert!(list_local_dumps(Path::new("/definitely/missing/folder")).is_empty());
    }

    #[test]
    fn display_local_path_shortens_home() {
        let Some(home) = env::home_dir() else {
            return;
        };
        let path = home.join("Downloads").join("x.dump");
        assert_eq!(display_local_path(&path), "~/Downloads/x.dump");
    }
}
