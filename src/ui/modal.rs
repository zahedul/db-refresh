use dioxus::prelude::*;

/// Centered dialog over a dimmed backdrop. Escape, the ✕ button or clicking the
/// backdrop calls `on_close`.
#[component]
pub fn Modal(
    title: String,
    #[props(default)] subtitle: Option<String>,
    #[props(default)] icon: Option<Element>,
    on_close: EventHandler<()>,
    children: Element,
) -> Element {
    let close_on_escape = move |event: KeyboardEvent| {
        if event.key() == Key::Escape {
            on_close.call(());
        }
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            div {
                class: "modal",
                onkeydown: close_on_escape,
                // Clicks inside the dialog must not reach the backdrop and close it.
                onclick: move |event| event.stop_propagation(),
                header { class: "modal-header",
                    if let Some(icon) = icon {
                        div { class: "modal-header-icon", {icon} }
                    }
                    div { class: "modal-header-text",
                        h3 { class: "modal-title", "{title}" }
                        if let Some(subtitle) = subtitle {
                            p { class: "modal-subtitle", "{subtitle}" }
                        }
                    }
                    button {
                        class: "modal-close",
                        title: "Close (Esc)",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                {children}
            }
        }
    }
}
