//! Minimal ISO-8601 date/date-time -> epoch milliseconds (UTC), used to compute the recent-window
//! cutoff. Unlike `packages/core/src/file_nodes.rs`'s private `parse_iso_to_ms` (which only handles a
//! bare date or a `Z`-suffixed date-time and silently ignores any numeric timezone offset), this
//! version also applies a `+HH:MM` / `-HH:MM` offset — required because `git log --date=iso-strict`
//! (this crate's date format, see `process.rs`) emits the committer's local offset, never `Z`.
//! Duplicated rather than shared because the core helper is private; the day-counting algorithm
//! (Howard Hinnant's `days_from_civil`) is identical.

pub(crate) fn parse_iso_to_ms(s: &str) -> Option<i64> {
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
    let mut offset_ms: i64 = 0; // local - UTC; applied as `utc = local - offset`.
    let mut idx = 10;

    if bytes.len() > idx && bytes[idx] == b'T' {
        if bytes.len() < idx + 9 {
            return None;
        }
        hour = s.get(idx + 1..idx + 3)?.parse().ok()?;
        minute = s.get(idx + 4..idx + 6)?.parse().ok()?;
        second = s.get(idx + 7..idx + 9)?.parse().ok()?;
        idx += 9;
        if bytes.len() > idx && bytes[idx] == b'.' {
            let frac_start = idx + 1;
            let frac: String = s[frac_start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            idx = frac_start + frac.len();
            if !frac.is_empty() {
                let mut padded = frac;
                padded.truncate(3);
                while padded.len() < 3 {
                    padded.push('0');
                }
                millis = padded.parse().ok()?;
            }
        }
        if bytes.len() > idx {
            match bytes[idx] {
                b'Z' | b'z' => offset_ms = 0,
                b'+' | b'-' => {
                    let sign: i64 = if bytes[idx] == b'+' { 1 } else { -1 };
                    let rest = &s[idx + 1..];
                    let (oh, om) = if rest.len() >= 5 && rest.as_bytes()[2] == b':' {
                        (
                            rest.get(0..2)?.parse::<i64>().ok()?,
                            rest.get(3..5)?.parse::<i64>().ok()?,
                        )
                    } else if rest.len() >= 4 {
                        (
                            rest.get(0..2)?.parse::<i64>().ok()?,
                            rest.get(2..4)?.parse::<i64>().ok()?,
                        )
                    } else {
                        (0, 0)
                    };
                    offset_ms = sign * (oh * 3_600_000 + om * 60_000);
                }
                _ => {}
            }
        }
    }

    let days = days_from_civil(year, month, day);
    let local_ms = days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1000 + millis;
    Some(local_ms - offset_ms)
}

/// Days since 1970-01-01 (UTC) for a proleptic-Gregorian civil date. Howard Hinnant's `days_from_civil`
/// algorithm (same as `file_nodes.rs`'s copy).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_only_is_utc_midnight() {
        assert_eq!(parse_iso_to_ms("1970-01-01"), Some(0));
        assert_eq!(parse_iso_to_ms("1970-01-02"), Some(86_400_000));
    }

    #[test]
    fn z_suffixed_datetime_is_utc() {
        assert_eq!(parse_iso_to_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_to_ms("1970-01-01T01:00:00.500Z"), Some(3_600_500));
    }

    #[test]
    fn numeric_offset_is_converted_to_utc() {
        // 09:00 local at +09:00 offset is 00:00 UTC the same day.
        assert_eq!(parse_iso_to_ms("1970-01-01T09:00:00+09:00"), Some(0));
        // 00:00 local at -05:00 offset is 05:00 UTC the same day.
        assert_eq!(
            parse_iso_to_ms("1970-01-01T00:00:00-05:00"),
            Some(5 * 3_600_000)
        );
    }

    #[test]
    fn malformed_input_returns_none() {
        assert_eq!(parse_iso_to_ms("not-a-date"), None);
        assert_eq!(parse_iso_to_ms(""), None);
    }
}
