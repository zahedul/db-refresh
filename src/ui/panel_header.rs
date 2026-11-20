use dioxus::prelude::*;

/// Title row used by every panel: icon, title, a subtitle with live context,
/// and optional actions (buttons, filter chips) on the right.
#[component]
pub fn PanelHeader(
    icon: Element,
    title: String,
    #[props(default)] subtitle: Option<String>,
    /// Formatted subtitle; shown instead of `subtitle` when given.
    #[props(default)]
    subtitle_content: Option<Element>,
    #[props(default)] is_compact: bool,
    children: Element,
) -> Element {
    rsx! {
        header {
            class: "panel-header",
            class: if is_compact { "compact" },
            div { class: "panel-header-icon", {icon} }
            div { class: "panel-header-text",
                h2 { "{title}" }
                if let Some(content) = subtitle_content {
                    div { class: "panel-subtitle rich", {content} }
                } else if let Some(subtitle) = subtitle {
                    p { class: "panel-subtitle", title: "{subtitle}", "{subtitle}" }
                }
            }
            div { class: "panel-header-actions", {children} }
        }
    }
}
