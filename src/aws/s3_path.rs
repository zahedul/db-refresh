//! Pure helpers for S3 "folder" prefixes such as `backups/gen/`.

const FOLDER_SEPARATOR: char = '/';
const WHOLE_BUCKET_LABEL: &str = "Whole bucket";

#[derive(Clone, Debug, PartialEq)]
pub struct Breadcrumb {
    pub label: String,
    /// Prefix to browse to when this crumb is clicked, e.g. `backups/gen/`.
    pub prefix: String,
}

/// `"backups/gen/"` -> `[backups -> "backups/", gen -> "backups/gen/"]`
pub fn breadcrumbs(prefix: &str) -> Vec<Breadcrumb> {
    let mut crumbs = Vec::new();
    let mut prefix_so_far = String::new();

    for piece in prefix.split_inclusive(FOLDER_SEPARATOR) {
        // Build from the raw pieces so unusual keys like `a//b/` keep their exact prefix.
        prefix_so_far.push_str(piece);
        let label = piece.trim_end_matches(FOLDER_SEPARATOR);
        if label.is_empty() {
            continue;
        }
        crumbs.push(Breadcrumb {
            label: label.to_string(),
            prefix: prefix_so_far.clone(),
        });
    }

    crumbs
}

/// `"backups/gen/"` -> `"backups / gen"`, `""` -> `"Whole bucket"`
pub fn display_path(prefix: &str) -> String {
    let labels: Vec<String> = breadcrumbs(prefix)
        .into_iter()
        .map(|crumb| crumb.label)
        .collect();

    if labels.is_empty() {
        return WHOLE_BUCKET_LABEL.to_string();
    }
    labels.join(" / ")
}

/// `("backups/gen/", "backups/")` -> `"gen"`
pub fn folder_name<'a>(folder_prefix: &'a str, parent_prefix: &str) -> &'a str {
    folder_prefix
        .strip_prefix(parent_prefix)
        .unwrap_or(folder_prefix)
        .trim_end_matches(FOLDER_SEPARATOR)
}

pub fn contains_ignoring_case(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crumb(label: &str, prefix: &str) -> Breadcrumb {
        Breadcrumb {
            label: label.to_string(),
            prefix: prefix.to_string(),
        }
    }

    #[test]
    fn breadcrumbs_build_cumulative_prefixes() {
        assert_eq!(
            breadcrumbs("backups/gen/"),
            vec![crumb("backups", "backups/"), crumb("gen", "backups/gen/")]
        );
        assert!(breadcrumbs("").is_empty());
    }

    #[test]
    fn breadcrumbs_keep_exact_prefix_for_double_slashes() {
        assert_eq!(
            breadcrumbs("a//b/"),
            vec![crumb("a", "a/"), crumb("b", "a//b/")]
        );
    }

    #[test]
    fn display_path_joins_labels_or_names_whole_bucket() {
        assert_eq!(display_path("backups/gen/"), "backups / gen");
        assert_eq!(display_path(""), "Whole bucket");
    }

    #[test]
    fn folder_name_strips_parent_and_trailing_slash() {
        assert_eq!(folder_name("backups/gen/", "backups/"), "gen");
        assert_eq!(folder_name("backups/", ""), "backups");
    }

    #[test]
    fn contains_ignoring_case_matches_any_case() {
        assert!(contains_ignoring_case("Backups/GEN_2026.dump", "gen_"));
        assert!(!contains_ignoring_case("mad_2026.dump", "gen_"));
    }
}
