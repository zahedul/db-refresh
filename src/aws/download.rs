use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::get_object::GetObjectError;
use tokio::fs::{self, File};
use tokio::io::{AsyncWriteExt, BufWriter};

use crate::aws::s3::build_client;
use crate::error::AppError;

const WRITE_BUFFER_BYTES: usize = 1024 * 1024;
// Progress callbacks drive UI redraws; a few per second is smooth without wasting CPU.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);
const PARTIAL_FILE_SUFFIX: &str = ".part";

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadRequest {
    pub profile: String,
    pub bucket: String,
    pub key: String,
    pub destination: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DownloadProgress {
    pub received_bytes: u64,
    pub total_bytes: u64,
}

impl DownloadProgress {
    pub const NOT_STARTED: Self = Self {
        received_bytes: 0,
        total_bytes: 0,
    };

    /// 0.0 ..= 1.0; 0.0 while the size is still unknown.
    pub fn fraction_done(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.received_bytes as f64 / self.total_bytes as f64).min(1.0)
    }

    pub fn bytes_per_second(&self, elapsed: Duration) -> f64 {
        let seconds = elapsed.as_secs_f64();
        if seconds <= 0.0 {
            return 0.0;
        }
        self.received_bytes as f64 / seconds
    }

    /// `None` until there is enough data to estimate a speed.
    pub fn time_remaining(&self, elapsed: Duration) -> Option<Duration> {
        let speed = self.bytes_per_second(elapsed);
        if speed <= 0.0 || self.total_bytes == 0 {
            return None;
        }
        let remaining_bytes = self.total_bytes.saturating_sub(self.received_bytes) as f64;
        Some(Duration::from_secs_f64(remaining_bytes / speed))
    }
}

/// Streams an S3 object to `request.destination` without holding it in memory.
///
/// Data goes to `<destination>.part` first and is renamed only when complete, so a
/// half-finished file never looks like a real dump. If the returned future is dropped
/// (the user pressed Cancel), the partial file is deleted.
pub async fn download_dump_file(
    request: &DownloadRequest,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<(), AppError> {
    let client = build_client(&request.profile).await?;
    let mut object = client
        .get_object()
        .bucket(&request.bucket)
        .key(&request.key)
        .send()
        .await
        .map_err(|error| to_download_error(error, request))?;

    let total_bytes = object
        .content_length()
        .and_then(|length| u64::try_from(length).ok())
        .unwrap_or(0);
    let mut progress = DownloadProgress {
        received_bytes: 0,
        total_bytes,
    };
    on_progress(progress);

    let mut partial_file = PartialFile::create(&request.destination).await?;
    let mut last_reported_at = Instant::now();

    while let Some(chunk) = object
        .body
        .try_next()
        .await
        .map_err(|error| AppError::Download(DisplayErrorContext(&error).to_string()))?
    {
        partial_file.write(&chunk).await?;
        progress.received_bytes += chunk.len() as u64;

        if last_reported_at.elapsed() >= PROGRESS_INTERVAL {
            on_progress(progress);
            last_reported_at = Instant::now();
        }
    }

    if total_bytes > 0 && progress.received_bytes != total_bytes {
        return Err(AppError::IncompleteDownload {
            received_bytes: progress.received_bytes,
            expected_bytes: total_bytes,
        });
    }

    partial_file.finish(&request.destination).await?;
    on_progress(progress);
    Ok(())
}

/// A `<name>.part` file that deletes itself unless `finish` succeeds.
struct PartialFile {
    path: PathBuf,
    writer: BufWriter<File>,
    is_finished: bool,
}

impl PartialFile {
    async fn create(destination: &Path) -> Result<Self, AppError> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|error| file_system_error(parent, &error))?;
        }

        let path = partial_path_for(destination);
        let file = File::create(&path)
            .await
            .map_err(|error| file_system_error(&path, &error))?;

        Ok(Self {
            writer: BufWriter::with_capacity(WRITE_BUFFER_BYTES, file),
            path,
            is_finished: false,
        })
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), AppError> {
        self.writer
            .write_all(bytes)
            .await
            .map_err(|error| file_system_error(&self.path, &error))
    }

    async fn finish(mut self, destination: &Path) -> Result<(), AppError> {
        self.writer
            .flush()
            .await
            .map_err(|error| file_system_error(&self.path, &error))?;
        fs::rename(&self.path, destination)
            .await
            .map_err(|error| file_system_error(destination, &error))?;
        self.is_finished = true;
        Ok(())
    }
}

impl Drop for PartialFile {
    fn drop(&mut self) {
        if self.is_finished {
            return;
        }
        // Runs on cancel or error. Nothing useful to do if removal fails; the next
        // download of the same file overwrites the leftover `.part` anyway.
        let _ = std::fs::remove_file(&self.path);
    }
}

fn partial_path_for(destination: &Path) -> PathBuf {
    let mut partial_name = destination.as_os_str().to_owned();
    partial_name.push(PARTIAL_FILE_SUFFIX);
    PathBuf::from(partial_name)
}

fn file_system_error(path: &Path, error: &std::io::Error) -> AppError {
    AppError::FileSystem {
        path: path.display().to_string(),
        detail: error.to_string(),
    }
}

fn to_download_error(error: SdkError<GetObjectError>, request: &DownloadRequest) -> AppError {
    if let Some(GetObjectError::NoSuchKey(_)) = error.as_service_error() {
        return AppError::ObjectNotFound {
            key: request.key.clone(),
        };
    }
    match error.code() {
        Some("AccessDenied") => AppError::AccessDenied {
            profile: request.profile.clone(),
            bucket: request.bucket.clone(),
        },
        _ => AppError::Download(DisplayErrorContext(&error).to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reports_fraction_speed_and_time_left() {
        let progress = DownloadProgress {
            received_bytes: 250,
            total_bytes: 1000,
        };
        let elapsed = Duration::from_secs(5);

        assert_eq!(progress.fraction_done(), 0.25);
        assert_eq!(progress.bytes_per_second(elapsed), 50.0);
        assert_eq!(
            progress.time_remaining(elapsed),
            Some(Duration::from_secs(15))
        );
    }

    #[test]
    fn progress_without_data_has_no_estimate() {
        let progress = DownloadProgress::NOT_STARTED;
        assert_eq!(progress.fraction_done(), 0.0);
        assert_eq!(progress.time_remaining(Duration::from_secs(1)), None);
    }

    #[test]
    fn partial_path_appends_suffix() {
        let destination = Path::new("/tmp/gen_2026.dump");
        assert_eq!(
            partial_path_for(destination),
            Path::new("/tmp/gen_2026.dump.part")
        );
    }

    #[tokio::test]
    async fn partial_file_is_deleted_when_dropped_unfinished() {
        let destination = std::env::temp_dir().join("db-refresh-drop-test.dump");
        let mut partial_file = PartialFile::create(&destination).await.unwrap();
        partial_file.write(b"half a dump").await.unwrap();
        let partial_path = partial_file.path.clone();
        assert!(partial_path.exists());

        drop(partial_file);

        assert!(!partial_path.exists());
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn finished_partial_file_becomes_destination() {
        let destination = std::env::temp_dir().join("db-refresh-finish-test.dump");
        let mut partial_file = PartialFile::create(&destination).await.unwrap();
        partial_file.write(b"full dump").await.unwrap();
        let partial_path = partial_file.path.clone();

        partial_file.finish(&destination).await.unwrap();

        assert!(!partial_path.exists());
        assert_eq!(std::fs::read(&destination).unwrap(), b"full dump");
        std::fs::remove_file(&destination).unwrap();
    }
}
