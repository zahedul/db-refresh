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

    rsx! {
        div { class: "download-folder",
            FolderIcon {}
            span {
                class: "download-folder-path mono",
                title: "{download_folder.display()}",
                "{display_local_path(&download_folder)}"
            }
            button { class: "link-button", onclick: choose_folder, "Change…" }
            if is_custom_folder {
                button {
                    class: "link-button",
                    title: "Save to ~/Downloads again",
                    onclick: use_default_folder,
                    "Use Downloads"
                }
            }
        }
    }
}
