use crate::constants::USER_GUIDE_MARKDOWN;
use dioxus::prelude::*;
use pulldown_cmark::{Options, Parser, html};

#[component]
pub fn HelpView() -> Element {
    let guide_html = use_hook(|| render_markdown(USER_GUIDE_MARKDOWN));

    rsx! {
        main { class: "help-layout",
            // Safe: the HTML comes only from our own guide file, never from user input.
            article { class: "panel help-article", dangerous_inner_html: "{guide_html}" }
        }
    }
}

fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    let mut rendered = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut rendered, parser);
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guide_renders_headings_and_tables() {
        let rendered = render_markdown(USER_GUIDE_MARKDOWN);
        assert!(rendered.contains("<h1>DB Refresh - User Guide</h1>"));
        assert!(rendered.contains("<h2>4. Refresh a database in one click</h2>"));
        assert!(rendered.contains("<table>"));
    }

    #[test]
    fn guide_has_no_web_links() {
        // A clicked web link would navigate the app's only window away from the UI.
        assert!(!render_markdown(USER_GUIDE_MARKDOWN).contains("<a href"));
    }
}
