use dioxus::prelude::*;

const TAG_COLOR_COUNT: u32 = 5;

/// Coloured pill for a dump group such as `gen` -> "GEN".
#[component]
pub fn GroupTag(group: String) -> Element {
    rsx! {
        span { class: "tag {tag_color_class(&group)}", "{group.to_uppercase()}" }
    }
}

/// Same group always gets the same colour, so GEN and MAD are easy to tell apart.
fn tag_color_class(group: &str) -> String {
    let color_index = group.to_lowercase().bytes().map(u32::from).sum::<u32>() % TAG_COLOR_COUNT;
    format!("tag-color-{color_index}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_colors_are_stable_and_differ_for_gen_and_mad() {
        assert_eq!(tag_color_class("gen"), tag_color_class("GEN"));
        assert_ne!(tag_color_class("gen"), tag_color_class("mad"));
    }
}
