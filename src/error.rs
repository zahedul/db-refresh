use thiserror::Error;

/// Every failure the app can show to the user, each with a message that says what to do next.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum AppError {
    #[error("Select an AWS profile first.")]
    MissingProfile,

    #[error("Enter a bucket name.")]
    MissingBucket,

    #[error(
        "AWS profile \"{profile}\" has no region. Add `region = <region>` to it in ~/.aws/config."
    )]
    MissingRegion { profile: String },

    #[error(
        "Could not get AWS credentials for profile \"{profile}\". \
         If it uses SSO, run in a terminal: aws sso login --profile {profile}\n\nDetails: {detail}"
    )]
    CredentialsUnavailable { profile: String, detail: String },

    #[error("Bucket \"{bucket}\" does not exist.")]
    BucketNotFound { bucket: String },

    #[error("Access denied: profile \"{profile}\" cannot access bucket \"{bucket}\".")]
    AccessDenied { profile: String, bucket: String },

    #[error("\"{key}\" no longer exists in the bucket. Search again.")]
    ObjectNotFound { key: String },

    #[error("Download stopped early: got {received_bytes} of {expected_bytes} bytes. Try again.")]
    IncompleteDownload {
        received_bytes: u64,
        expected_bytes: u64,
    },

    #[error("Download failed: {0}")]
    Download(String),

    #[error("Could not write {path}: {detail}")]
    FileSystem { path: String, detail: String },

    #[error("Bucket \"{bucket}\" is in a different region than profile \"{profile}\" uses.")]
    WrongRegion { profile: String, bucket: String },

    #[error("S3 request failed: {0}")]
    S3Request(String),

    #[error("{0}")]
    InvalidTarget(String),

    #[error("Keychain: {0}")]
    Keychain(String),

    #[error("Could not connect: {0}")]
    DatabaseConnection(String),

    #[error("{0}")]
    Restore(String),
}
