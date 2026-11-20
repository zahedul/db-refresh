use dioxus::prelude::*;

use crate::aws::s3_path::build_breadcrumbs;
use crate::ui::icons::{CloudIcon, FolderIcon};

/// `(☁ my-bucket) › (📁 backups) › (📁 prod)`: an S3 location as pills.
#[component]
pub fn S3LocationTrail(
    bucket: String,
    prefix: String,
    /// Let pills move to a new line in narrow places instead of being cut off.
    #[props(default)]
    is_wrapping: bool,
) -> Element {
    let folders = build_breadcrumbs(&prefix);

    rsx! {
        span {
            class: "s3-trail",
            class: if is_wrapping { "wrap" },
            title: "s3://{bucket}/{prefix}",
            span { class: "s3-segment bucket",
                CloudIcon {}
                "{bucket}"
            }
            for folder in folders {
                span { key: "{folder.prefix}", class: "s3-step",
                    span { class: "s3-separator", "›" }
                    span { class: "s3-segment folder",
                        FolderIcon {}
                        "{folder.label}"
                    }
                }
            }
        }
    }
}
