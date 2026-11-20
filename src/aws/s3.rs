use std::cmp::Reverse;
use std::collections::HashSet;

use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_s3::Client;
use aws_sdk_s3::config::ProvideCredentials;
use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::list_objects_v2::ListObjectsV2Error;
use aws_sdk_s3::types::Object;

use crate::aws::s3_path::contains_ignoring_case;
use crate::constants::{MAX_FOLDER_LIST_PAGES, MAX_SCANNED_OBJECTS, S3_FOLDER_SEPARATOR};
use crate::dump_name::extract_dump_group;
use crate::error::AppError;

#[derive(Clone, Debug, PartialEq)]
pub struct SearchRequest {
    pub profile: String,
    pub bucket: String,
    pub prefix: String,
    pub name_filter: String,
}

impl SearchRequest {
    fn trimmed(self) -> Self {
        Self {
            profile: self.profile.trim().to_string(),
            bucket: self.bucket.trim().to_string(),
            prefix: self.prefix.trim().to_string(),
            name_filter: self.name_filter.trim().to_string(),
        }
    }

    pub fn s3_location(&self) -> String {
        format!("s3://{}/{}", self.bucket, self.prefix)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DumpFile {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified_epoch_secs: Option<i64>,
}

impl DumpFile {
    fn from_s3_object(object: &Object) -> Option<Self> {
        let key = object.key()?;
        // Keys ending in '/' are "folder" placeholders, not files.
        if key.ends_with('/') {
            return None;
        }
        Some(Self {
            key: key.to_string(),
            size_bytes: object
                .size()
                .and_then(|size| u64::try_from(size).ok())
                .unwrap_or(0),
            last_modified_epoch_secs: object.last_modified().map(|time| time.secs()),
        })
    }

    pub fn file_name(&self) -> &str {
        self.key.rsplit('/').next().unwrap_or(&self.key)
    }

    /// See `extract_dump_group`: `prod_2026-11-20.dump` -> `prod`.
    pub fn group(&self) -> Option<&str> {
        extract_dump_group(self.file_name())
    }

    /// The full folder of the key: `backups/prod/x.dump` -> `backups/prod`.
    pub fn folder(&self) -> Option<&str> {
        let (folder, _) = self.key.rsplit_once(S3_FOLDER_SEPARATOR)?;
        Some(folder)
    }

    /// The folder relative to where the search started: (`backups/prod/x.dump`, `backups/`)
    /// -> `prod`. `None` for files directly in the searched folder.
    pub fn extract_relative_folder(&self, search_prefix: &str) -> Option<&str> {
        let relative_key = self.key.strip_prefix(search_prefix).unwrap_or(&self.key);
        let (folder, _) = relative_key.rsplit_once(S3_FOLDER_SEPARATOR)?;
        Some(folder).filter(|folder| !folder.is_empty())
    }
}

/// Keys of the newest file in each group. Expects files sorted newest first.
pub fn find_latest_file_per_group(dump_files: &[DumpFile]) -> HashSet<&str> {
    let mut seen_groups = HashSet::new();
    dump_files
        .iter()
        // Per folder too: when the whole bucket is searched, each folder has its own latest.
        .filter(|file| seen_groups.insert((file.folder(), file.group())))
        .map(|file| file.key.as_str())
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub request: SearchRequest,
    pub dump_files: Vec<DumpFile>,
    pub is_truncated: bool,
}

pub async fn search_dump_files(request: SearchRequest) -> Result<SearchResult, AppError> {
    let request = request.trimmed();
    if request.bucket.is_empty() {
        return Err(AppError::MissingBucket);
    }

    let client = build_client(&request.profile).await?;
    let (mut dump_files, is_truncated) = list_matching_dump_files(&client, &request).await?;
    sort_newest_first(&mut dump_files);

    Ok(SearchResult {
        request,
        dump_files,
        is_truncated,
    })
}

/// Direct sub-folders of `parent_prefix` (`""` = bucket root), e.g. `["backups/prod/", "backups/stg/"]`.
pub async fn list_folders(
    profile: &str,
    bucket: &str,
    parent_prefix: &str,
) -> Result<Vec<String>, AppError> {
    let bucket = bucket.trim();
    if bucket.is_empty() {
        return Err(AppError::MissingBucket);
    }

    let client = build_client(profile).await?;
    let mut pages = client
        .list_objects_v2()
        .bucket(bucket)
        .delimiter(S3_FOLDER_SEPARATOR)
        .set_prefix(filter_non_empty(parent_prefix))
        .into_paginator()
        .send();

    let mut folders = Vec::new();
    let mut fetched_page_count = 0;

    while let Some(page) = pages.next().await {
        let page = page.map_err(|error| to_app_error(error, profile, bucket))?;
        let page_folders = page
            .common_prefixes()
            .iter()
            .filter_map(|common_prefix| common_prefix.prefix())
            .map(str::to_string);
        folders.extend(page_folders);

        fetched_page_count += 1;
        if fetched_page_count >= MAX_FOLDER_LIST_PAGES {
            break;
        }
    }

    Ok(folders)
}

pub(crate) async fn build_client(profile: &str) -> Result<Client, AppError> {
    let sdk_config = aws_config::defaults(BehaviorVersion::latest())
        .profile_name(profile)
        .load()
        .await;

    if sdk_config.region().is_none() {
        return Err(AppError::MissingRegion {
            profile: profile.to_string(),
        });
    }
    ensure_credentials_available(&sdk_config, profile).await?;

    Ok(Client::new(&sdk_config))
}

// Resolving credentials up front turns "SSO token expired" into a clear message
// instead of a generic failure from the first S3 call.
async fn ensure_credentials_available(
    sdk_config: &SdkConfig,
    profile: &str,
) -> Result<(), AppError> {
    let credentials_unavailable = |detail: String| AppError::CredentialsUnavailable {
        profile: profile.to_string(),
        detail,
    };

    let Some(provider) = sdk_config.credentials_provider() else {
        return Err(credentials_unavailable(
            "no credentials configured".to_string(),
        ));
    };
    provider
        .provide_credentials()
        .await
        .map(|_| ())
        .map_err(|error| credentials_unavailable(DisplayErrorContext(&error).to_string()))
}

async fn list_matching_dump_files(
    client: &Client,
    request: &SearchRequest,
) -> Result<(Vec<DumpFile>, bool), AppError> {
    let mut pages = client
        .list_objects_v2()
        .bucket(&request.bucket)
        .set_prefix(filter_non_empty(&request.prefix))
        .into_paginator()
        .send();

    let mut dump_files = Vec::new();
    let mut scanned_object_count = 0;

    while let Some(page) = pages.next().await {
        let page = page.map_err(|error| to_app_error(error, &request.profile, &request.bucket))?;
        scanned_object_count += page.contents().len();

        let matching_files = page
            .contents()
            .iter()
            .filter_map(DumpFile::from_s3_object)
            .filter(|file| matches_name_filter(&file.key, &request.name_filter));
        dump_files.extend(matching_files);

        if scanned_object_count >= MAX_SCANNED_OBJECTS {
            return Ok((dump_files, true));
        }
    }

    Ok((dump_files, false))
}

fn to_app_error(error: SdkError<ListObjectsV2Error>, profile: &str, bucket: &str) -> AppError {
    if let Some(ListObjectsV2Error::NoSuchBucket(_)) = error.as_service_error() {
        return AppError::BucketNotFound {
            bucket: bucket.to_string(),
        };
    }

    match error.code() {
        Some("AccessDenied") => AppError::AccessDenied {
            profile: profile.to_string(),
            bucket: bucket.to_string(),
        },
        Some("PermanentRedirect" | "AuthorizationHeaderMalformed") => AppError::WrongRegion {
            profile: profile.to_string(),
            bucket: bucket.to_string(),
        },
        _ => AppError::S3Request(DisplayErrorContext(&error).to_string()),
    }
}

fn filter_non_empty(value: &str) -> Option<String> {
    Some(value.to_string()).filter(|value| !value.is_empty())
}

fn matches_name_filter(key: &str, name_filter: &str) -> bool {
    name_filter.is_empty() || contains_ignoring_case(key, name_filter)
}

fn sort_newest_first(dump_files: &mut [DumpFile]) {
    dump_files.sort_by_key(|file| Reverse(file.last_modified_epoch_secs));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump_file(key: &str, last_modified_epoch_secs: Option<i64>) -> DumpFile {
        DumpFile {
            key: key.to_string(),
            size_bytes: 0,
            last_modified_epoch_secs,
        }
    }

    #[test]
    fn name_filter_is_case_insensitive_and_empty_matches_all() {
        assert!(matches_name_filter("backups/PROD_2026.dump", "prod_"));
        assert!(matches_name_filter("anything", ""));
        assert!(!matches_name_filter("backups/stg_2026.dump", "prod_"));
    }

    #[test]
    fn sort_puts_newest_first_and_unknown_dates_last() {
        let mut files = vec![
            dump_file("old", Some(100)),
            dump_file("unknown", None),
            dump_file("new", Some(300)),
        ];
        sort_newest_first(&mut files);
        let keys: Vec<&str> = files.iter().map(|file| file.key.as_str()).collect();
        assert_eq!(keys, vec!["new", "old", "unknown"]);
    }

    #[test]
    fn non_empty_turns_blank_into_none() {
        assert_eq!(filter_non_empty(""), None);
        assert_eq!(filter_non_empty("backups/"), Some("backups/".to_string()));
    }

    #[test]
    fn group_is_file_name_prefix_before_underscore() {
        assert_eq!(
            dump_file("backups/prod_2026.dump", None).group(),
            Some("prod")
        );
        assert_eq!(dump_file("2026-11-02T040006.dump", None).group(), None);
        assert_eq!(dump_file("_odd.dump", None).group(), None);
    }

    #[test]
    fn latest_file_per_group_picks_first_of_each_group() {
        let files = vec![
            dump_file("prod_3.dump", Some(3)),
            dump_file("stg_3.dump", Some(3)),
            dump_file("prod_2.dump", Some(2)),
            dump_file("plain.dump", Some(1)),
        ];
        let latest = find_latest_file_per_group(&files);
        assert_eq!(
            latest,
            HashSet::from(["prod_3.dump", "stg_3.dump", "plain.dump"])
        );
    }

    #[test]
    fn relative_folder_is_measured_from_the_search_prefix() {
        let nested = dump_file("backups/prod/daily/x.dump", None);
        assert_eq!(
            nested.extract_relative_folder("backups/"),
            Some("prod/daily")
        );
        assert_eq!(
            nested.extract_relative_folder(""),
            Some("backups/prod/daily")
        );
        assert_eq!(nested.folder(), Some("backups/prod/daily"));

        let top_level = dump_file("backups/x.dump", None);
        assert_eq!(top_level.extract_relative_folder("backups/"), None);
        assert_eq!(dump_file("x.dump", None).folder(), None);
    }

    #[test]
    fn latest_is_picked_per_folder_and_group() {
        let files = vec![
            dump_file("prod/2026-11-20.dump", Some(2)),
            dump_file("stg/2026-11-20.dump", Some(2)),
            dump_file("prod/2026-11-19.dump", Some(1)),
        ];
        let latest = find_latest_file_per_group(&files);
        assert_eq!(
            latest,
            HashSet::from(["prod/2026-11-20.dump", "stg/2026-11-20.dump"])
        );
    }

    #[test]
    fn file_name_strips_folders() {
        assert_eq!(dump_file("a/b/prod.dump", None).file_name(), "prod.dump");
        assert_eq!(dump_file("prod.dump", None).file_name(), "prod.dump");
    }

    #[test]
    fn folder_placeholders_are_not_dump_files() {
        let folder = Object::builder().key("backups/").build();
        let file = Object::builder().key("backups/prod.dump").size(42).build();
        assert_eq!(DumpFile::from_s3_object(&folder), None);
        assert_eq!(
            DumpFile::from_s3_object(&file).map(|f| f.size_bytes),
            Some(42)
        );
    }

    #[test]
    fn trimmed_request_removes_surrounding_spaces() {
        let request = SearchRequest {
            profile: " default ".to_string(),
            bucket: "  my-bucket ".to_string(),
            prefix: " backups/ ".to_string(),
            name_filter: " prod_ ".to_string(),
        }
        .trimmed();
        assert_eq!(request.s3_location(), "s3://my-bucket/backups/");
        assert_eq!(request.name_filter, "prod_");
    }
}
