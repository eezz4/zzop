//! Clock/timestamp helpers for `build_file_nodes`'s lifecycle classification — `now_ms` (wall clock)
//! and the dependency-free ISO-8601 parser (no date/time crate is available in this workspace).

pub(super) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Minimal ISO-8601 date/date-time string -> epoch milliseconds (UTC). Handles the shapes emitted by git-log
/// timestamps: date-only "YYYY-MM-DD" (interpreted as UTC midnight, matching JS `Date.parse`) and full
/// "YYYY-MM-DDTHH:MM:SS(.sss)?Z". No date/time crate is available in this workspace (see Cargo.toml); returns None
/// for anything else, which `classify_lifecycle` treats the same as a null lastModified (infinitely old).
pub(super) fn parse_iso_to_ms(s: &str) -> Option<i64> {
    if s.len() < 10 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: i64 = s.get(5..7)?.parse().ok()?;
    let day: i64 = s.get(8..10)?.parse().ok()?;

    let mut hour: i64 = 0;
    let mut minute: i64 = 0;
    let mut second: i64 = 0;
    let mut millis: i64 = 0;
    if bytes.len() >= 19 && bytes[10] == b'T' {
        hour = s.get(11..13)?.parse().ok()?;
        minute = s.get(14..16)?.parse().ok()?;
        second = s.get(17..19)?.parse().ok()?;
        if bytes.len() > 19 && bytes[19] == b'.' {
            let frac: String = s[20..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !frac.is_empty() {
                let mut padded = frac;
                padded.truncate(3);
                while padded.len() < 3 {
                    padded.push('0');
                }
                millis = padded.parse().ok()?;
            }
        }
    }

    let days = days_from_civil(year, month, day);
    Some(days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1000 + millis)
}

/// Days since 1970-01-01 (UTC) for a proleptic-Gregorian civil date. Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}
