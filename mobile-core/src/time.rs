//! Timestamp normalization for the formats used by iOS and Android stores.
//!
//! Every conversion is a pure function of the stored integer. No timezone or
//! daylight-saving inference is made; the record keeps the original value too.

use lovelycarve_plugin::Timestamp;

/// Apple Core Data / Cocoa epoch (2001-01-01 UTC) in Unix seconds.
pub const COCOA_EPOCH: i64 = 978_307_200;

/// WebKit / Chromium epoch (1601-01-01 UTC) in Unix seconds.
pub const WEBKIT_EPOCH: i64 = 11_644_473_600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeKind {
    /// Seconds since the Unix epoch.
    UnixSeconds,
    /// Milliseconds since the Unix epoch.
    UnixMillis,
    /// Microseconds since the Unix epoch.
    UnixMicros,
    /// Nanoseconds since the Unix epoch.
    UnixNanos,
    /// Seconds since 2001-01-01 UTC.
    Cocoa,
    /// Cocoa for realistic values, Unix seconds above the 2032 crossover.
    CocoaOrUnix,
    /// Microseconds since 1601-01-01 UTC (Chromium `visit_time`).
    WebkitMicros,
    /// Infer seconds / milliseconds / microseconds / nanoseconds by magnitude.
    Auto,
}

/// Converts one stored integer to an RFC 3339 UTC projection.
///
/// Returns `None` for zero/negative values where the store uses them as a
/// null marker only when `kind` cannot represent the value; otherwise the
/// arithmetic result is returned even for pre-1970 dates.
pub fn convert(value: i64, kind: TimeKind) -> Option<Timestamp> {
    match kind {
        TimeKind::UnixSeconds => Some(Timestamp::from_unix(value)),
        TimeKind::UnixMillis => Some(Timestamp::from_unix_millis(value)),
        TimeKind::UnixMicros => Some(from_micros(value)),
        TimeKind::UnixNanos => Some(from_nanos(value)),
        TimeKind::Cocoa => Some(Timestamp::from_unix(value.checked_add(COCOA_EPOCH)?)),
        TimeKind::CocoaOrUnix => {
            if value > COCOA_EPOCH {
                Some(Timestamp::from_unix(value))
            } else {
                Some(Timestamp::from_unix(value.checked_add(COCOA_EPOCH)?))
            }
        }
        TimeKind::WebkitMicros => {
            let seconds = value.div_euclid(1_000_000) - WEBKIT_EPOCH;
            let micros = value.rem_euclid(1_000_000);
            Some(Timestamp::from_unix_nanos(
                seconds,
                (micros as u32) * 1_000,
            ))
        }
        TimeKind::Auto => {
            let magnitude = value.unsigned_abs();
            if magnitude >= 100_000_000_000_000_000 {
                Some(from_nanos(value))
            } else if magnitude >= 100_000_000_000_000 {
                Some(from_micros(value))
            } else if magnitude >= 100_000_000_000 {
                Some(Timestamp::from_unix_millis(value))
            } else {
                Some(Timestamp::from_unix(value))
            }
        }
    }
}

fn from_micros(value: i64) -> Timestamp {
    let seconds = value.div_euclid(1_000_000);
    let micros = value.rem_euclid(1_000_000);
    Timestamp::from_unix_nanos(seconds, (micros as u32) * 1_000)
}

fn from_nanos(value: i64) -> Timestamp {
    let seconds = value.div_euclid(1_000_000_000);
    let nanos = value.rem_euclid(1_000_000_000);
    Timestamp::from_unix_nanos(seconds, nanos as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cocoa_epoch_is_2001() {
        assert_eq!(
            convert(0, TimeKind::Cocoa).unwrap().as_str(),
            "2001-01-01T00:00:00Z"
        );
        assert_eq!(
            convert(631_152_000, TimeKind::Cocoa).unwrap().as_str(),
            "2021-01-01T00:00:00Z"
        );
    }

    #[test]
    fn chrome_epoch_is_1601() {
        assert_eq!(
            convert(0, TimeKind::WebkitMicros).unwrap().as_str(),
            "1601-01-01T00:00:00Z"
        );
        assert_eq!(
            convert(13_000_000_000_000_000, TimeKind::WebkitMicros)
                .unwrap()
                .as_str()
                .get(..4),
            Some("2012")
        );
    }

    #[test]
    fn android_milliseconds_are_not_read_as_seconds() {
        assert_eq!(
            convert(1_704_164_645_000, TimeKind::UnixMillis)
                .unwrap()
                .as_str()
                .get(..4),
            Some("2024")
        );
        assert_eq!(
            convert(1_704_164_645_000, TimeKind::Auto)
                .unwrap()
                .as_str()
                .get(..4),
            Some("2024")
        );
    }

    #[test]
    fn cocoa_or_unix_uses_the_crossover() {
        assert_eq!(
            convert(800_000_000, TimeKind::CocoaOrUnix)
                .unwrap()
                .as_str()
                .get(..4),
            Some("2026")
        );
        assert_eq!(
            convert(1_704_164_645, TimeKind::CocoaOrUnix)
                .unwrap()
                .as_str()
                .get(..4),
            Some("2024")
        );
    }
}
