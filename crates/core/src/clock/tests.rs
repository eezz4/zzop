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

/// The V90 defect, pinned as an equality rather than a magic number: an offset timestamp and the UTC
/// spelling of the SAME instant must parse to the same value. The deleted `file_nodes/time.rs` copy
/// failed this by exactly the offset, which is what made a 30-day lifecycle boundary movable by
/// whichever timezone the committer happened to sit in.
#[test]
fn offset_and_its_utc_equivalent_are_the_same_instant() {
    assert_eq!(
        parse_iso_to_ms("2026-09-07T09:00:00+09:00"),
        parse_iso_to_ms("2026-09-07T00:00:00Z")
    );
    assert_eq!(
        parse_iso_to_ms("2026-09-07T00:00:00-05:00"),
        parse_iso_to_ms("2026-09-07T05:00:00Z")
    );
    // Negative offsets can cross the date line backwards; the day must move with them.
    assert_eq!(
        parse_iso_to_ms("2026-09-07T22:00:00-05:00"),
        parse_iso_to_ms("2026-09-08T03:00:00Z")
    );
}

/// `git log --date=iso-strict` — the format `zzop_git::process` asks for — never emits `Z`, so the
/// offset branch is not an edge case here, it is the ONLY branch real git dates take.
#[test]
fn iso_strict_offset_shapes_git_actually_emits() {
    // `+HHMM` without the colon, the other spelling git can produce.
    assert_eq!(
        parse_iso_to_ms("2026-09-07T09:00:00+0900"),
        parse_iso_to_ms("2026-09-07T00:00:00Z")
    );
    // An offset with non-zero minutes (e.g. India, +05:30).
    assert_eq!(
        parse_iso_to_ms("2026-09-07T05:30:00+05:30"),
        parse_iso_to_ms("2026-09-07T00:00:00Z")
    );
}

#[test]
fn malformed_input_returns_none() {
    assert_eq!(parse_iso_to_ms("not-a-date"), None);
    assert_eq!(parse_iso_to_ms(""), None);
}

#[test]
fn now_ms_is_past_the_epoch() {
    // Not a clock assertion — just that the `unwrap_or(0)` fallback is not what callers get.
    assert!(now_ms() > 1_700_000_000_000);
}
