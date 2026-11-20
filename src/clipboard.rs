use std::io::Write;
use std::process::{Command, Stdio};

use dioxus::logger::tracing;

/// Puts `text` on the macOS clipboard via `pbcopy`. Failure is logged, not shown:
/// the user sees that nothing was pasted and can select the text by hand.
pub fn copy_to_clipboard(text: &str) {
    let result = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes())?;
            }
            child.wait()
        });
    if let Err(error) = result {
        tracing::warn!("Could not copy to the clipboard: {error}");
    }
}
