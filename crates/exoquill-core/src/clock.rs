//! Time helpers for ExoQuill.

use chrono::{Local, SecondsFormat, Utc};

/// Current UTC timestamp as an RFC 3339 string (millisecond precision).
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Local timestamp formatted as `YYYY-MM-DD HH:mm`, used in auto-generated
/// titles for OCR and dictation notes.
pub fn title_timestamp() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}
