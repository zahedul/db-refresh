use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aws_sdk_s3::primitives::{DateTime, DateTimeFormat};

const SIZE_UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
// Decimal units (1 KB = 1000 B) to match what macOS Finder shows.
const SIZE_STEP: f64 = 1000.0;

pub fn format_size(size_bytes: u64) -> String {
    let mut value = size_bytes as f64;
    let mut unit_index = 0;

    while value >= SIZE_STEP && unit_index < SIZE_UNITS.len() - 1 {
        value /= SIZE_STEP;
        unit_index += 1;
    }

    if unit_index == 0 {
        return format!("{size_bytes} B");
    }
    format!("{value:.1} {}", SIZE_UNITS[unit_index])
}

pub fn format_speed(bytes_per_second: f64) -> String {
    format!("{}/s", format_size(bytes_per_second as u64))
}

/// `18s`, `2m 05s`, `1h 02m`
pub fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let (hours, minutes, seconds) = (
        total_seconds / 3600,
        (total_seconds % 3600) / 60,
        total_seconds % 60,
    );

    if hours > 0 {
        return format!("{hours}h {minutes:02}m");
    }
    if minutes > 0 {
        return format!("{minutes}m {seconds:02}s");
    }
    format!("{seconds}s")
}

const SECONDS_PER_MINUTE: i64 = 60;
const SECONDS_PER_HOUR: i64 = 60 * SECONDS_PER_MINUTE;
const SECONDS_PER_DAY: i64 = 24 * SECONDS_PER_HOUR;
const RELATIVE_AGE_MAX_DAYS: i64 = 30;

/// `"just now"`, `"5m ago"`, `"6h ago"`, `"2d ago"`; older than a month shows the date.
pub fn format_relative_age(epoch_secs: i64, now_epoch_secs: i64) -> String {
    let age_secs = (now_epoch_secs - epoch_secs).max(0);

    if age_secs < SECONDS_PER_MINUTE {
        return "just now".to_string();
    }
    if age_secs < SECONDS_PER_HOUR {
        return format!("{}m ago", age_secs / SECONDS_PER_MINUTE);
    }
    if age_secs < SECONDS_PER_DAY {
        return format!("{}h ago", age_secs / SECONDS_PER_HOUR);
    }
    if age_secs < RELATIVE_AGE_MAX_DAYS * SECONDS_PER_DAY {
        return format!("{}d ago", age_secs / SECONDS_PER_DAY);
    }
    format_utc_timestamp(epoch_secs)
        .get(..10)
        .unwrap_or_default()
        .to_string()
}

pub fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since_epoch| since_epoch.as_secs() as i64)
        .unwrap_or(0)
}

/// `1763611396` -> `"2025-11-20 04:03 UTC"`
pub fn format_utc_timestamp(epoch_secs: i64) -> String {
    let Ok(iso_timestamp) = DateTime::from_secs(epoch_secs).fmt(DateTimeFormat::DateTime) else {
        return epoch_secs.to_string();
    };
    // ISO form is `2025-11-20T04:03:16Z`; keep date + hours:minutes.
    let date_and_minutes = iso_timestamp.get(..16).unwrap_or(&iso_timestamp);
    format!("{} UTC", date_and_minutes.replacen('T', " ", 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_size_picks_readable_unit() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1_500), "1.5 KB");
        assert_eq!(format_size(947_541_969), "947.5 MB");
        assert_eq!(format_size(1_315_670_014), "1.3 GB");
    }

    #[test]
    fn format_duration_picks_largest_units() {
        assert_eq!(format_duration(Duration::from_secs(18)), "18s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 05s");
        assert_eq!(format_duration(Duration::from_secs(3720)), "1h 02m");
    }

    #[test]
    fn format_relative_age_uses_coarsest_unit() {
        let now = 1_763_611_396;
        assert_eq!(format_relative_age(now - 10, now), "just now");
        assert_eq!(format_relative_age(now - 5 * 60, now), "5m ago");
        assert_eq!(format_relative_age(now - 6 * 3600, now), "6h ago");
        assert_eq!(format_relative_age(now - 2 * 86_400, now), "2d ago");
        assert_eq!(format_relative_age(0, now), "1970-01-01");
        // A clock slightly behind S3 must not show a negative age.
        assert_eq!(format_relative_age(now + 30, now), "just now");
    }

    #[test]
    fn format_speed_appends_per_second() {
        assert_eq!(format_speed(48_200_000.0), "48.2 MB/s");
    }

    #[test]
    fn format_utc_timestamp_shows_date_and_minutes() {
        assert_eq!(format_utc_timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc_timestamp(1_763_611_396), "2025-11-20 04:03 UTC");
    }
}
