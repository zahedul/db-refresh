use crate::constants::DEFAULT_AWS_PROFILE;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::{env, fs, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AwsFileKind {
    Config,
    Credentials,
}

/// Profile names from `~/.aws/config` and `~/.aws/credentials`, sorted and de-duplicated.
pub fn list_profile_names() -> io::Result<Vec<String>> {
    let mut profile_names = BTreeSet::new();

    for file_kind in [AwsFileKind::Config, AwsFileKind::Credentials] {
        let Some(contents) = read_aws_file(file_kind)? else {
            continue;
        };
        profile_names.extend(parse_profile_names(&contents, file_kind));
    }

    Ok(profile_names.into_iter().collect())
}

/// Keeps the current profile if it still exists, otherwise falls back to `default`, then the first one.
pub fn choose_profile(profile_names: &[String], current_profile: Option<&str>) -> Option<String> {
    let has_profile = |name: &str| profile_names.iter().any(|profile| profile == name);

    if let Some(current) = current_profile.filter(|name| has_profile(name)) {
        return Some(current.to_string());
    }
    if has_profile(DEFAULT_AWS_PROFILE) {
        return Some(DEFAULT_AWS_PROFILE.to_string());
    }
    profile_names.first().cloned()
}

fn read_aws_file(file_kind: AwsFileKind) -> io::Result<Option<String>> {
    let Some(path) = resolve_aws_file_path(file_kind) else {
        return Ok(None);
    };

    match fs::read_to_string(&path) {
        Ok(contents) => Ok(Some(contents)),
        // A missing file is normal (e.g. SSO users often have no credentials file).
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn resolve_aws_file_path(file_kind: AwsFileKind) -> Option<PathBuf> {
    let (override_env_var, default_file_name) = match file_kind {
        AwsFileKind::Config => ("AWS_CONFIG_FILE", "config"),
        AwsFileKind::Credentials => ("AWS_SHARED_CREDENTIALS_FILE", "credentials"),
    };

    if let Some(custom_path) = env::var_os(override_env_var) {
        return Some(PathBuf::from(custom_path));
    }
    env::home_dir().map(|home| home.join(".aws").join(default_file_name))
}

fn parse_profile_names(contents: &str, file_kind: AwsFileKind) -> Vec<String> {
    contents
        .lines()
        .filter_map(parse_section_header)
        .filter_map(|section| extract_profile_name(section, file_kind))
        .collect()
}

fn parse_section_header(line: &str) -> Option<&str> {
    let section = line.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    Some(section).filter(|name| !name.is_empty())
}

// In `config`, named profiles are written `[profile name]`; other sections
// (`[sso-session x]`, `[services x]`) are not profiles. In `credentials`, every section is a profile.
fn extract_profile_name(section: &str, file_kind: AwsFileKind) -> Option<String> {
    if file_kind == AwsFileKind::Credentials || section == DEFAULT_AWS_PROFILE {
        return Some(section.to_string());
    }

    let profile_name = section
        .strip_prefix("profile")
        .filter(|rest| rest.starts_with(char::is_whitespace))?
        .trim();
    Some(profile_name.to_string()).filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_CONFIG: &str = "\
[default]
region = eu-west-1

[profile staging]
sso_session = company

[profile   prod  ]
region = us-east-1

[sso-session company]
sso_start_url = https://example.awsapps.com/start

[services local]
# [profile commented-out] is not a header because the line starts with '#'
";

    #[test]
    fn config_keeps_default_and_named_profiles_only() {
        let names = parse_profile_names(SAMPLE_CONFIG, AwsFileKind::Config);
        assert_eq!(names, vec!["default", "staging", "prod"]);
    }

    #[test]
    fn credentials_treats_every_section_as_profile() {
        let contents = "[default]\naws_access_key_id = x\n[ci-user]\n";
        let names = parse_profile_names(contents, AwsFileKind::Credentials);
        assert_eq!(names, vec!["default", "ci-user"]);
    }

    #[test]
    fn config_ignores_malformed_profile_headers() {
        let contents = "[profile]\n[profileX]\n[]\n[profile ]\n";
        assert!(parse_profile_names(contents, AwsFileKind::Config).is_empty());
    }

    #[test]
    fn choose_profile_keeps_current_when_still_present() {
        let names = vec!["default".to_string(), "prod".to_string()];
        assert_eq!(
            choose_profile(&names, Some("prod")),
            Some("prod".to_string())
        );
    }

    #[test]
    fn choose_profile_falls_back_to_default_then_first() {
        let with_default = vec!["a".to_string(), "default".to_string()];
        assert_eq!(
            choose_profile(&with_default, Some("gone")),
            Some("default".to_string())
        );

        let without_default = vec!["b".to_string(), "c".to_string()];
        assert_eq!(
            choose_profile(&without_default, None),
            Some("b".to_string())
        );

        assert_eq!(choose_profile(&[], None), None);
    }
}
