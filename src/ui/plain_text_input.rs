use dioxus::prelude::*;

/// Text field for technical values (bucket names, file filters): macOS must not
/// auto-capitalize, auto-correct or spell-check what the user types.
#[component]
pub fn PlainTextInput(
    value: Signal<String>,
    placeholder: String,
    /// Fires when the value is committed (Enter or leaving the field).
    on_commit: Option<EventHandler<String>>,
    /// Fires when Enter is pressed in the field.
    on_enter: Option<EventHandler<()>>,
    #[props(default)] focus_on_mount: bool,
) -> Element {
    rsx! {
        input {
            r#type: "text",
            onkeydown: move |event| {
                if let (Key::Enter, Some(handler)) = (event.key(), on_enter) {
                    handler.call(());
                }
            },
            onmounted: move |event| async move {
                if focus_on_mount {
                    // Focus failing only means the user has to click the field.
                    let _ = event.set_focus(true).await;
                }
            },
            placeholder,
            value: "{value}",
            spellcheck: "false",
            autocomplete: "off",
            "autocapitalize": "off",
            "autocorrect": "off",
            oninput: move |event| value.set(event.value()),
            onchange: move |event| {
                if let Some(handler) = on_commit {
                    handler.call(event.value());
                }
            },
        }
    }
}
