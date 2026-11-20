const DUMP_EXTENSION: &str = ".dump";
const GROUP_SEPARATOR: char = '_';

/// The part of a dump file name before the first `_`: `gen_2026-11-20.dump` -> `gen`.
/// Dumps of the same database share it.
pub fn dump_group(file_name: &str) -> Option<&str> {
    let (group, _) = file_name.split_once(GROUP_SEPARATOR)?;
    Some(group).filter(|group| !group.is_empty())
}

/// `("gen", "2026-11-20.dump")` -> `"gen_2026-11-20.dump"`. Never strips anything from the
/// name (a leading `my_` may be part of the real name), and never adds the prefix twice.
pub fn with_group_prefix(group: &str, file_name: &str) -> String {
    if dump_group(file_name) == Some(group) {
        return file_name.to_string();
    }
    format!("{group}{GROUP_SEPARATOR}{file_name}")
}

/// Checks a name typed for a download and adds `.dump` when it is missing.
pub fn validated_download_name(typed_name: &str) -> Result<String, String> {
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
    if name.ends_with(DUMP_EXTENSION) {
        return Ok(name.to_string());
    }
    Ok(format!("{name}{DUMP_EXTENSION}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_group_is_prefix_before_underscore() {
        assert_eq!(dump_group("gen_2026-11-20T040016.dump"), Some("gen"));
        assert_eq!(dump_group("2026-11-02T040006.dump"), None);
        assert_eq!(dump_group("_odd.dump"), None);
    }

    #[test]
    fn with_group_prefix_adds_or_replaces_prefix() {
        assert_eq!(
            with_group_prefix("gen", "2026-11-20T040016.dump"),
            "gen_2026-11-20T040016.dump"
        );
        assert_eq!(
            with_group_prefix("gen", "my_backup.dump"),
            "gen_my_backup.dump"
        );
        assert_eq!(with_group_prefix("gen", "gen_2026.dump"), "gen_2026.dump");
    }

    #[test]
    fn validated_download_name_appends_extension_and_rejects_paths() {
        assert_eq!(
            validated_download_name(" gen_x "),
            Ok("gen_x.dump".to_string())
        );
        assert_eq!(
            validated_download_name("gen_x.dump"),
            Ok("gen_x.dump".to_string())
        );
        assert!(validated_download_name("  ").is_err());
        assert!(validated_download_name("../x.dump").is_err());
        assert!(validated_download_name(".hidden").is_err());
    }
}
