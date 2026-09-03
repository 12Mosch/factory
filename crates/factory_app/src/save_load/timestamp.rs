use chrono::{DateTime, Local, Utc};
use std::time::{Duration, UNIX_EPOCH};

/// Converts a serialized Unix timestamp without overflowing `SystemTime` or
/// exceeding Chrono's representable date range.
pub(crate) fn local_datetime_from_unix_ms(unix_ms: u64) -> Option<DateTime<Local>> {
    UNIX_EPOCH.checked_add(Duration::from_millis(unix_ms))?;
    let unix_ms = i64::try_from(unix_ms).ok()?;
    DateTime::<Utc>::from_timestamp_millis(unix_ms).map(|timestamp| timestamp.with_timezone(&Local))
}
