use std::path::Path;

use dioxus::prelude::*;
use rfd::AsyncFileDialog;

use crate::local_files::display_local_path;
use crate::ui::app_context::use_app_context;
use crate::ui::icons::FolderIcon;

/// Shows where dumps are saved, with buttons to pick another folder or go back to the default.
#[component]
pub fn DownloadFolderControl() -> Element {
    let app = use_app_context();
    let (download_folder, is_custom_folder) = {
        let settings = app.settings.read();
        (
            settings.download_folder(),
            settings.custom_download_folder.is_some(),
        )
    };

    let choose_folder = {
        let current_folder = download_folder.clone();
        move |_| {
            let current_folder = current_folder.clone();
            spawn(async move {
                let picked = AsyncFileDialog::new()
                    .set_title("Choose where dumps are saved")
                    .set_directory(&current_folder)
                    .pick_folder()
                    .await;
                let Some(folder) = picked else {
                    return;
                };
                let mut settings = app.settings;
                settings.with_mut(|settings| {
                    settings.custom_download_folder = Some(folder.path().to_path_buf());
                });
            });
        }
    };
    let use_default_folder = move |_| {
        let mut settings = app.settings;
        settings.with_mut(|settings| settings.custom_download_folder = None);
    };

    let folder_name = name_folder(&download_folder);
    let folder_path = display_local_path(&download_folder);

    rsx! {
        div { class: "download-folder",
            span { class: "download-folder-label", "Save to" }
            button {
                class: "folder-chip",
                title: "{folder_path} · click to choose another folder",
                onclick: choose_folder,
                FolderIcon {}
                span { class: "folder-chip-name", "{folder_name}" }
                span { class: "folder-chip-chevron", "▾" }
            }
            if is_custom_folder {
                button {
                    class: "icon-button folder-reset",
                    title: "Save to ~/Downloads again",
                    onclick: use_default_folder,
                    "↺"
                }
            }
        }
    }
}

/// The folder's own name (`db-refresh-dumps`), or the whole path for `/`.
fn name_folder(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_is_named_by_its_last_component() {
        assert_eq!(name_folder(Path::new("/Users/me/Downloads/dumps")), "dumps");
        assert_eq!(name_folder(Path::new("/")), "/");
    }
}
