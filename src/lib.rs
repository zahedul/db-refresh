//! DB Refresh: find PostgreSQL dumps in S3, download them, and restore them into
//! local databases. `main.rs` only calls [`run`]; everything else lives here so the
//! integration tests in `tests/` can use it.

pub mod aws;
pub mod clipboard;
pub mod constants;
pub mod database;
pub mod dump_name;
pub mod error;
pub mod format;
pub mod local_files;
pub mod settings;
pub mod ui;

use dioxus::desktop::{Config, WindowBuilder};
use dioxus::prelude::*;

use crate::constants::{APP_TITLE, MIN_WINDOW_SIZE, WINDOW_SIZE};

/// Opens the app window and runs until it is closed.
pub fn run() {
    // Warnings and errors only: at INFO the AWS SDK logs credential details such as
    // the access key id. `init` fails only if a logger already exists, which is fine.
    let _ = dioxus::logger::init(dioxus::logger::tracing::Level::WARN);
    let window = WindowBuilder::new()
        .with_title(APP_TITLE)
        .with_inner_size(WINDOW_SIZE)
        .with_min_inner_size(MIN_WINDOW_SIZE);
    // Keep Dioxus' default menu: on macOS, ⌘C / ⌘V / ⌘Q in text fields only work
    // when the app has an Edit menu.
    let desktop_config = Config::new().with_window(window);

    LaunchBuilder::desktop()
        .with_cfg(desktop_config)
        .launch(ui::app::App);
}
