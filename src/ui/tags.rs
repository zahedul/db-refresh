use crate::constants::TAG_COLOR_COUNT;
use dioxus::prelude::*;

/// Coloured pill for a dump group such as `prod` -> "PROD".
#[component]
pub fn GroupTag(group: String) -> Element {
    rsx! {
        span { class: "tag {pick_tag_color_class(&group)}", "{group.to_uppercase()}" }
    }
}

/// Same group always gets the same colour, so PROD and STG are easy to tell apart.
fn pick_tag_color_class(group: &str) -> String {
    let color_index = group.to_lowercase().bytes().map(u32::from).sum::<u32>() % TAG_COLOR_COUNT;
    format!("tag-color-{color_index}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_colors_are_stable_and_differ_between_groups() {
        assert_eq!(pick_tag_color_class("prod"), pick_tag_color_class("PROD"));
        assert_ne!(pick_tag_color_class("prod"), pick_tag_color_class("stg"));
    }
}
