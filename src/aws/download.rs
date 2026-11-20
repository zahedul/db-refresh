//! Downloads S3 objects in parallel byte ranges, straight to disk.

use std::cell::Cell;
use std::fs::{self, File};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use aws_sdk_s3::Client;
use aws_sdk_s3::config::http::HttpResponse;
use aws_sdk_s3::error::{DisplayErrorContext, ProvideErrorMetadata, SdkError};
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::operation::head_object::HeadObjectError;
use futures_util::{StreamExt, stream};

use crate::aws::s3::build_client;
use crate::constants::{
    DOWNLOAD_PART_SIZE_BYTES, DOWNLOAD_PROGRESS_INTERVAL, DOWNLOAD_WRITE_BATCH_BYTES,
    HTTP_FORBIDDEN, HTTP_PRECONDITION_FAILED, MAX_PARALLEL_DOWNLOAD_PARTS, MAX_PART_ATTEMPTS,
    PART_RETRY_BASE_DELAY, PARTIAL_FILE_SUFFIX,
};
use crate::error::AppError;

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

/// Size and version of the object, read once before the parts are fetched.
struct ObjectInfo {
    size_bytes: u64,
    /// Every part is requested with `If-Match: <etag>`, so a file replaced mid-download
    /// fails clearly instead of mixing two versions.
    e_tag: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ByteRange {
    start: u64,
    end_inclusive: u64,
}

impl ByteRange {
    fn len(self) -> u64 {
        self.end_inclusive - self.start + 1
    }

    /// HTTP `Range` header value, e.g. `bytes=0-8388607`.
    fn header_value(self) -> String {
        format!("bytes={}-{}", self.start, self.end_inclusive)
    }
}

/// Downloads an S3 object to `request.destination` in parallel byte ranges.
///
/// Data goes to `<destination>.part` first and is renamed only when complete, so a
/// half-finished file never looks like a real dump. If the returned future is dropped
/// (the user pressed Cancel), every request stops and the partial file is deleted.
pub async fn download_dump_file(
    request: &DownloadRequest,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<(), AppError> {
    let client = build_client(&request.profile).await?;
    let object = describe_object(&client, request).await?;
    let total_bytes = object.size_bytes;
    on_progress(DownloadProgress {
        received_bytes: 0,
        total_bytes,
    });

    let partial_file = PartialFile::create(&request.destination, total_bytes)?;
    // All parts run inside this one task, so a plain `Cell` can count their bytes.
    let received_bytes = Cell::new(0_u64);
    let part_target = PartTarget {
        client: &client,
        request,
        e_tag: object.e_tag.as_deref(),
        file: &partial_file,
        received_bytes: &received_bytes,
    };

    download_all_parts(part_target, total_bytes, &mut on_progress).await?;

    let final_progress = DownloadProgress {
        received_bytes: received_bytes.get(),
        total_bytes,
    };
    if final_progress.received_bytes != total_bytes {
        return Err(AppError::IncompleteDownload {
            received_bytes: final_progress.received_bytes,
            expected_bytes: total_bytes,
        });
    }
    partial_file.finish(&request.destination)?;
    on_progress(final_progress);
    Ok(())
}

/// Runs up to `MAX_PARALLEL_DOWNLOAD_PARTS` part downloads at a time, reporting progress on a timer.
async fn download_all_parts(
    part_target: PartTarget<'_>,
    total_bytes: u64,
    on_progress: &mut impl FnMut(DownloadProgress),
) -> Result<(), AppError> {
    let mut parts = pin!(
        stream::iter(split_into_part_ranges(
            total_bytes,
            DOWNLOAD_PART_SIZE_BYTES
        ))
        .map(|range| part_target.download_with_retries(range))
        .buffer_unordered(MAX_PARALLEL_DOWNLOAD_PARTS)
    );
    let mut progress_ticker = tokio::time::interval(DOWNLOAD_PROGRESS_INTERVAL);
    loop {
        tokio::select! {
            finished_part = parts.next() => match finished_part {
                Some(result) => result?,
                None => return Ok(()),
            },
            _ = progress_ticker.tick() => on_progress(DownloadProgress {
                received_bytes: part_target.received_bytes.get(),
                total_bytes,
            }),
        }
    }
}

/// Splits `total_bytes` into consecutive ranges of at most `part_size` bytes.
fn split_into_part_ranges(total_bytes: u64, part_size: u64) -> Vec<ByteRange> {
    (0..total_bytes)
        .step_by(part_size as usize)
        .map(|start| ByteRange {
            start,
            end_inclusive: (start + part_size).min(total_bytes) - 1,
        })
        .collect()
}

async fn describe_object(
    client: &Client,
    request: &DownloadRequest,
) -> Result<ObjectInfo, AppError> {
    let head = client
        .head_object()
        .bucket(&request.bucket)
        .key(&request.key)
        .send()
        .await
        .map_err(|error| to_head_error(error, request))?;

    let size_bytes = head
        .content_length()
        .and_then(|length| u64::try_from(length).ok())
        .ok_or_else(|| AppError::Download("S3 did not report the file size.".to_string()))?;
    Ok(ObjectInfo {
        size_bytes,
        e_tag: head.e_tag().map(str::to_string),
    })
}

/// What every part download shares.
#[derive(Clone, Copy)]
struct PartTarget<'a> {
    client: &'a Client,
    request: &'a DownloadRequest,
    e_tag: Option<&'a str>,
    file: &'a PartialFile,
    received_bytes: &'a Cell<u64>,
}

impl PartTarget<'_> {
    async fn download_with_retries(self, range: ByteRange) -> Result<(), AppError> {
        let mut attempt = 1;
        loop {
            let mut received_this_attempt = 0;
            let result = self.download(range, &mut received_this_attempt).await;
            let error = match result {
                Ok(()) => return Ok(()),
                Err(error) => error,
            };
            if attempt >= MAX_PART_ATTEMPTS || !is_retryable(&error) {
                return Err(error);
            }
            // The retry fetches the whole part again, so take back what this attempt counted.
            self.received_bytes
                .set(self.received_bytes.get() - received_this_attempt);
            tokio::time::sleep(PART_RETRY_BASE_DELAY * attempt).await;
            attempt += 1;
        }
    }

    async fn download(
        self,
        range: ByteRange,
        received_this_attempt: &mut u64,
    ) -> Result<(), AppError> {
        let mut response = self
            .client
            .get_object()
            .bucket(&self.request.bucket)
            .key(&self.request.key)
            .range(range.header_value())
            .set_if_match(self.e_tag.map(str::to_string))
            .send()
            .await
            .map_err(|error| to_download_error(error, self.request))?;

        let mut write_offset = range.start;
        let mut batch = Vec::with_capacity(DOWNLOAD_WRITE_BATCH_BYTES);
        while let Some(chunk) = response
            .body
            .try_next()
            .await
            .map_err(|error| AppError::Download(DisplayErrorContext(&error).to_string()))?
        {
            batch.extend_from_slice(&chunk);
            *received_this_attempt += chunk.len() as u64;
            self.received_bytes
                .set(self.received_bytes.get() + chunk.len() as u64);
            if batch.len() >= DOWNLOAD_WRITE_BATCH_BYTES {
                write_offset = self.file.write_batch(write_offset, &mut batch).await?;
            }
        }
        write_offset = self.file.write_batch(write_offset, &mut batch).await?;

        if write_offset - range.start != range.len() {
            return Err(AppError::Download(format!(
                "Part {} ended early.",
                range.header_value()
            )));
        }
        Ok(())
    }
}

/// Network hiccups are worth retrying; a missing file, no access or a replaced file is not.
fn is_retryable(error: &AppError) -> bool {
    matches!(error, AppError::Download(_))
}

/// A pre-sized `<name>.part` file that parts write into at their own offsets.
/// It deletes itself unless `finish` succeeds.
struct PartialFile {
    path: PathBuf,
    file: Arc<File>,
    is_finished: bool,
}

impl PartialFile {
    fn create(destination: &Path, size_bytes: u64) -> Result<Self, AppError> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| to_file_system_error(parent, &error))?;
        }
        let path = build_partial_path(destination);
        let file = File::create(&path).map_err(|error| to_file_system_error(&path, &error))?;
        // Reserve the full size up front so parts can be written in any order.
        file.set_len(size_bytes)
            .map_err(|error| to_file_system_error(&path, &error))?;

        Ok(Self {
            path,
            file: Arc::new(file),
            is_finished: false,
        })
    }

    /// Writes `batch` at `offset` on a blocking thread, empties it, and returns the next offset.
    async fn write_batch(&self, offset: u64, batch: &mut Vec<u8>) -> Result<u64, AppError> {
        if batch.is_empty() {
            return Ok(offset);
        }
        let data = std::mem::replace(batch, Vec::with_capacity(DOWNLOAD_WRITE_BATCH_BYTES));
        let next_offset = offset + data.len() as u64;
        let file = Arc::clone(&self.file);
        let write_result = tokio::task::spawn_blocking(move || file.write_all_at(&data, offset))
            .await
            .map_err(|join_error| {
                AppError::Download(format!("Disk write stopped: {join_error}"))
            })?;
        write_result.map_err(|error| to_file_system_error(&self.path, &error))?;
        Ok(next_offset)
    }

    fn finish(mut self, destination: &Path) -> Result<(), AppError> {
        fs::rename(&self.path, destination)
            .map_err(|error| to_file_system_error(destination, &error))?;
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
        let _ = fs::remove_file(&self.path);
    }
}

fn build_partial_path(destination: &Path) -> PathBuf {
    let mut partial_name = destination.as_os_str().to_owned();
    partial_name.push(PARTIAL_FILE_SUFFIX);
    PathBuf::from(partial_name)
}

fn to_file_system_error(path: &Path, error: &std::io::Error) -> AppError {
    AppError::FileSystem {
        path: path.display().to_string(),
        detail: error.to_string(),
    }
}

fn read_http_status<E>(error: &SdkError<E, HttpResponse>) -> Option<u16> {
    error
        .raw_response()
        .map(|response| response.status().as_u16())
}

fn to_head_error(
    error: SdkError<HeadObjectError, HttpResponse>,
    request: &DownloadRequest,
) -> AppError {
    if let Some(HeadObjectError::NotFound(_)) = error.as_service_error() {
        return AppError::ObjectNotFound {
            key: request.key.clone(),
        };
    }
    // HEAD responses have no body, so access errors only show as the HTTP status.
    if read_http_status(&error) == Some(HTTP_FORBIDDEN) {
        return AppError::AccessDenied {
            profile: request.profile.clone(),
            bucket: request.bucket.clone(),
        };
    }
    AppError::Download(DisplayErrorContext(&error).to_string())
}

fn to_download_error(
    error: SdkError<GetObjectError, HttpResponse>,
    request: &DownloadRequest,
) -> AppError {
    if let Some(GetObjectError::NoSuchKey(_)) = error.as_service_error() {
        return AppError::ObjectNotFound {
            key: request.key.clone(),
        };
    }
    if error.code() == Some("PreconditionFailed")
        || read_http_status(&error) == Some(HTTP_PRECONDITION_FAILED)
    {
        return AppError::ObjectChanged {
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
    fn part_ranges_cover_every_byte_exactly_once() {
        let ranges = split_into_part_ranges(25, 10);
        assert_eq!(
            ranges,
            vec![
                ByteRange {
                    start: 0,
                    end_inclusive: 9
                },
                ByteRange {
                    start: 10,
                    end_inclusive: 19
                },
                ByteRange {
                    start: 20,
                    end_inclusive: 24
                },
            ]
        );
        assert_eq!(ranges.iter().map(|range| range.len()).sum::<u64>(), 25);
        assert_eq!(ranges[0].header_value(), "bytes=0-9");
    }

    #[test]
    fn part_ranges_handle_exact_multiples_and_empty_files() {
        assert_eq!(split_into_part_ranges(20, 10).len(), 2);
        assert_eq!(
            split_into_part_ranges(1, 10),
            vec![ByteRange {
                start: 0,
                end_inclusive: 0
            }]
        );
        assert!(split_into_part_ranges(0, 10).is_empty());
    }

    #[test]
    fn a_one_gigabyte_dump_is_split_into_128_parts() {
        assert_eq!(
            split_into_part_ranges(1024 * 1024 * 1024, DOWNLOAD_PART_SIZE_BYTES).len(),
            128
        );
    }

    #[test]
    fn only_network_errors_are_retried() {
        assert!(is_retryable(&AppError::Download(
            "connection reset".to_string()
        )));
        assert!(!is_retryable(&AppError::ObjectChanged {
            key: "k".to_string()
        }));
        assert!(!is_retryable(&AppError::ObjectNotFound {
            key: "k".to_string()
        }));
    }

    #[test]
    fn partial_path_appends_suffix() {
        let destination = Path::new("/tmp/prod_2026.dump");
        assert_eq!(
            build_partial_path(destination),
            Path::new("/tmp/prod_2026.dump.part")
        );
    }

    #[tokio::test]
    async fn parts_written_out_of_order_produce_the_right_file() {
        let destination = std::env::temp_dir().join("db-refresh-parts-test.dump");
        let partial_file = PartialFile::create(&destination, 10).unwrap();

        // Later part first, as happens when parallel parts finish in any order.
        partial_file
            .write_batch(5, &mut b"world".to_vec())
            .await
            .unwrap();
        partial_file
            .write_batch(0, &mut b"hello".to_vec())
            .await
            .unwrap();
        partial_file.finish(&destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), b"helloworld");
        fs::remove_file(&destination).unwrap();
    }

    #[test]
    fn partial_file_is_deleted_when_dropped_unfinished() {
        let destination = std::env::temp_dir().join("db-refresh-drop-test.dump");
        let partial_file = PartialFile::create(&destination, 1024).unwrap();
        let partial_path = partial_file.path.clone();
        assert!(partial_path.exists());

        drop(partial_file);

        assert!(!partial_path.exists());
        assert!(!destination.exists());
    }
}
