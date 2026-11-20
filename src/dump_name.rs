use crate::constants::{DUMP_EXTENSION, DUMP_GROUP_SEPARATOR};
/// The part of a dump file name before the first `_`: `prod_2026-11-20.dump` -> `prod`.
/// Dumps of the same database share it.
pub fn extract_dump_group(file_name: &str) -> Option<&str> {
    let (group, _) = file_name.split_once(DUMP_GROUP_SEPARATOR)?;
    Some(group).filter(|group| !group.is_empty())
}

/// `("prod", "2026-11-20.dump")` -> `"prod_2026-11-20.dump"`. Never strips anything from the
/// name (a leading `my_` may be part of the real name), and never adds the prefix twice.
pub fn add_group_prefix(group: &str, file_name: &str) -> String {
    if extract_dump_group(file_name) == Some(group) {
        return file_name.to_string();
    }
    format!("{group}{DUMP_GROUP_SEPARATOR}{file_name}")
}

/// Checks a name typed for a download and adds `.dump` when it is missing.
pub fn validate_download_name(typed_name: &str) -> Result<String, String> {
    let name = typed_name.trim();
    if name.is_empty() {
        return Err("Enter a file name.".to_string());
    }
    if name.contains('/') || name.contains(':') {
        return Err("The name cannot contain / or :.".to_string());
    }
    if name.starts_with('.') {
        return Err("The name cannot start with a dot.".to_string());
    }
    let dump_suffix = format!(".{DUMP_EXTENSION}");
    if name.ends_with(&dump_suffix) {
        return Ok(name.to_string());
    }
    Ok(format!("{name}{dump_suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_group_is_prefix_before_underscore() {
        assert_eq!(
            extract_dump_group("prod_2026-11-20T040016.dump"),
            Some("prod")
        );
        assert_eq!(extract_dump_group("2026-11-02T040006.dump"), None);
        assert_eq!(extract_dump_group("_odd.dump"), None);
    }

    #[test]
    fn with_group_prefix_adds_or_replaces_prefix() {
        assert_eq!(
            add_group_prefix("prod", "2026-11-20T040016.dump"),
            "prod_2026-11-20T040016.dump"
        );
        assert_eq!(
            add_group_prefix("prod", "my_backup.dump"),
            "prod_my_backup.dump"
        );
        assert_eq!(add_group_prefix("prod", "prod_2026.dump"), "prod_2026.dump");
    }

    #[test]
    fn validated_download_name_appends_extension_and_rejects_paths() {
        assert_eq!(
            validate_download_name(" prod_x "),
            Ok("prod_x.dump".to_string())
        );
        assert_eq!(
            validate_download_name("prod_x.dump"),
            Ok("prod_x.dump".to_string())
        );
        assert!(validate_download_name("  ").is_err());
        assert!(validate_download_name("../x.dump").is_err());
        assert!(validate_download_name(".hidden").is_err());
    }
}
