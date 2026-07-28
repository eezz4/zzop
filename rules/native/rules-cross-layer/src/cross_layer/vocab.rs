//! The one helper that compiles a caller-declared vocabulary pattern for this crate's rules. Split out of
//! `cross_layer/mod.rs` purely to keep that file under the repo's per-file line cap; the pattern constant
//! it falls back to ([`super::VERSION_SEGMENT_PATTERN`]) stays in `mod.rs`, where both readers already
//! find it.

use regex::Regex;

/// Compiles the run's declared version-segment vocabulary (`vocabulary.apiVersionSegmentPattern`), or
/// `None` when the author declared none or declared one that will not parse.
///
/// Neither case may become [`super::VERSION_SEGMENT_PATTERN`]: how a project spells an API version is a
/// name it chooses, and putting ours back in its place is the guessing this vocabulary removes. Neither
/// may panic either — a config file is hand-written, so a malformed regex is a real input. `None` marks no
/// segment as a version, which for both readers means no pair of paths is treated as version-skewed. One
/// owner for both readers (`version_skew`, `external_version_inconsistent`) so the two can never drift on
/// what an absent or bad declaration means.
pub(crate) fn version_segment_re(declared: Option<&str>) -> Option<Regex> {
    declared.and_then(|d| Regex::new(d).ok())
}
