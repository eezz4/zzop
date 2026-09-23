//! Wall clock and ISO-8601 timestamp parsing — the one home for both, shared by every crate that
//! turns a git date into a number.
//!
//! ## Why this module exists (2026-09-07, review ledger V90)
//!
//! There were two `parse_iso_to_ms` implementations: a private one in `file_nodes/time.rs` and an
//! offset-aware one in `crates/git/src/iso_date.rs`. The git copy's own header said the core copy
//! "silently ignores any numeric timezone offset", and duplicated itself anyway "because the core
//! helper is private". So one crate knew the defect, wrote it down, and routed around it — while the
//! dates the core copy actually parses come from that same git crate, which runs
//! `git log --date=iso-strict` (see `zzop_git::process`) and therefore emits the committer's local
//! offset and NEVER a `Z`. The core copy read `2026-09-07T09:00:00+09:00` as 09:00 UTC: nine hours
//! late, up to ±14h wrong, straight into `classify_lifecycle`'s 30-day boundary via
//! `file_nodes.rs`'s `now - lastModified` subtraction.
//!
//! The fix is subtraction, not addition: the offset-aware version moved here and the private copy was
//! deleted, so there is nothing left to drift. `now_ms` came along for the same reason — it had been
//! copied a second time into `crates/git/src/lib.rs`, byte for byte.

/// Wall-clock milliseconds since the Unix epoch. Zero if the system clock predates the epoch, which
/// callers treat as "unknown" rather than a date.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Minimal ISO-8601 date/date-time string -> epoch milliseconds (UTC).
///
/// Handles the shapes git emits: bare `YYYY-MM-DD` (UTC midnight, matching JS `Date.parse`),
/// `…THH:MM:SS(.sss)?` with an optional `Z` or `+HH:MM`/`-HH:MM`/`+HHMM` offset. A numeric offset is
/// applied (`utc = local - offset`), which is the whole point of this function existing once instead
/// of twice. Returns `None` for anything else — `classify_lifecycle` treats that the same as a null
/// `lastModified` (infinitely old). Hand-rolled because no date/time crate is a workspace dependency.
pub fn parse_iso_to_ms(s: &str) -> Option<i64> {
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

/// Days since 1970-01-01 (UTC) for a proleptic-Gregorian civil date. Howard Hinnant's
/// `days_from_civil` algorithm.
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
mod tests;
