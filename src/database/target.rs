use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dump_name::dump_group;
use crate::error::AppError;

const DEFAULT_HOST: &str = "localhost";
const DEFAULT_PORT: u16 = 5432;

/// A PostgreSQL database that dumps can be restored into. The password is never
/// stored here; it lives in the macOS Keychain under `id` (see `credentials`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseTarget {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub database: String,
    /// Dumps named `<dump_group>_…` suggest this target, e.g. `gen`.
    pub dump_group: String,
    pub is_password_saved: bool,
    /// Where "Refresh latest" looks for this database's newest dump.
    pub dump_source: DumpSource,
}

/// An S3 location holding a database's dumps, for one-click "Refresh latest".
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DumpSource {
    pub profile: String,
    pub bucket: String,
    pub folder_prefix: String,
    /// Optional: only dumps whose key contains this text (e.g. `gen_`).
    pub name_filter: String,
}

impl DumpSource {
    pub fn is_configured(&self) -> bool {
        !self.profile.trim().is_empty() && !self.bucket.trim().is_empty()
    }

    pub fn s3_location(&self) -> String {
        format!("s3://{}/{}", self.bucket.trim(), self.folder_prefix.trim())
    }
}

impl Default for DatabaseTarget {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            host: DEFAULT_HOST.to_string(),
            port: DEFAULT_PORT,
            user: String::new(),
            database: String::new(),
            dump_group: String::new(),
            is_password_saved: false,
            dump_source: DumpSource::default(),
        }
    }
}

impl DatabaseTarget {
    pub fn new_draft() -> Self {
        Self {
            id: new_target_id(),
            ..Self::default()
        }
    }

    /// `postgres@localhost:5432/app_db`
    pub fn connection_label(&self) -> String {
        format!(
            "{}@{}:{}/{}",
            self.user, self.host, self.port, self.database
        )
    }

    pub fn trimmed(self) -> Self {
        Self {
            name: self.name.trim().to_string(),
            host: self.host.trim().to_string(),
            user: self.user.trim().to_string(),
            database: self.database.trim().to_string(),
            dump_group: self.dump_group.trim().to_string(),
            ..self
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        let required_fields = [
            ("a name", &self.name),
            ("the host", &self.host),
            ("the user", &self.user),
            ("the database name", &self.database),
        ];
        for (description, value) in required_fields {
            if value.is_empty() {
                return Err(AppError::InvalidTarget(format!("Enter {description}.")));
            }
        }
        if self.port == 0 {
            return Err(AppError::InvalidTarget("Enter a valid port.".to_string()));
        }
        Ok(())
    }
}

/// Replaces the target with the same id, or adds it at the end.
pub fn upsert_target(targets: &mut Vec<DatabaseTarget>, target: DatabaseTarget) {
    match targets.iter_mut().find(|existing| existing.id == target.id) {
        Some(existing) => *existing = target,
        None => targets.push(target),
    }
}

/// The target whose `dump_group` matches the dump's file-name prefix, if any.
pub fn suggest_target<'a>(
    targets: &'a [DatabaseTarget],
    dump_file_name: &str,
) -> Option<&'a DatabaseTarget> {
    let group = dump_group(dump_file_name)?;
    targets.iter().find(|target| {
        !target.dump_group.is_empty() && target.dump_group.eq_ignore_ascii_case(group)
    })
}

fn new_target_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_nanos())
        .unwrap_or(0);
    format!("target-{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str, dump_group: &str) -> DatabaseTarget {
        DatabaseTarget {
            id: name.to_string(),
            name: name.to_string(),
            user: "app_user".to_string(),
            database: format!("{name}_db"),
            dump_group: dump_group.to_string(),
            ..DatabaseTarget::default()
        }
    }

    #[test]
    fn suggest_target_matches_dump_prefix_ignoring_case() {
        let targets = vec![target("general", "gen"), target("madrasa", "MAD")];
        let suggested = suggest_target(&targets, "mad_2026-11-20.dump").map(|t| t.name.as_str());
        assert_eq!(suggested, Some("madrasa"));
        assert_eq!(suggest_target(&targets, "other_2026.dump"), None);
        assert_eq!(suggest_target(&targets, "2026-11-02.dump"), None);
    }

    #[test]
    fn suggest_target_ignores_targets_without_group() {
        let targets = vec![target("anything", "")];
        assert_eq!(suggest_target(&targets, "gen_2026.dump"), None);
    }

    #[test]
    fn validate_names_the_first_missing_field() {
        let mut draft = target("general", "gen");
        assert_eq!(draft.validate(), Ok(()));

        draft.user = String::new();
        assert_eq!(
            draft.validate(),
            Err(AppError::InvalidTarget("Enter the user.".to_string()))
        );
    }

    #[test]
    fn connection_label_shows_where_data_goes() {
        let general = target("general", "gen");
        assert_eq!(
            general.connection_label(),
            "app_user@localhost:5432/general_db"
        );
    }

    #[test]
    fn upsert_replaces_same_id_and_appends_new() {
        let mut targets = vec![target("general", "gen")];

        let mut renamed = target("general", "gen");
        renamed.name = "General (local)".to_string();
        upsert_target(&mut targets, renamed);
        upsert_target(&mut targets, target("madrasa", "mad"));

        let names: Vec<&str> = targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["General (local)", "madrasa"]);
    }

    #[test]
    fn dump_source_needs_profile_and_bucket() {
        let mut source = DumpSource::default();
        assert!(!source.is_configured());

        source.profile = "default".to_string();
        source.bucket = " my-backups ".to_string();
        source.folder_prefix = "prod/".to_string();
        assert!(source.is_configured());
        assert_eq!(source.s3_location(), "s3://my-backups/prod/");
    }

    #[test]
    fn new_drafts_get_distinct_ids() {
        assert_ne!(DatabaseTarget::new_draft().id, "");
    }
}
