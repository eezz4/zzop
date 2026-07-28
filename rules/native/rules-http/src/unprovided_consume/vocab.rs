//! The one DECLARABLE vocabulary this rule reads — which URL segments mark an API surface. Split out of
//! the parent module both to keep that file under the repo's per-file line cap and to put the declarable
//! default somewhere a reader can find it by name; the two-tier veto it gates is in the parent's
//! "Static-asset veto" section.
//!
//! What the gate covers, and what it does not. It lifts the `json`/`xml` veto for a key whose path
//! carries a declared API segment, rather than for a key under an asset directory — some frameworks strip
//! the `public/` prefix from served asset URLs, so a Next.js `public/i18n/*.json` fetch keyed
//! `GET /i18n/{}.json` is vetoed by the ABSENT API segment. Two consequences follow, both under-reporting
//! rather than fabricating: an API route living outside every declared segment is missed here, and a run
//! that declares NO `vocabulary.apiSegmentPattern` marks no path API-ish at all, so the veto is never
//! lifted for anything.

use regex::Regex;

/// API-ish path-segment vocabulary. `/`-delimited so it matches a whole path segment, not a bare
/// substring (`/apiary/` does not match `/api/`). This is the default behind the declarable config key
/// `vocabulary.apiSegmentPattern` — which segments mean "API here" is a name each project picks, so
/// `zzop_engine::VocabularyConfig` references THIS symbol rather than restating the value.
pub const API_SEGMENT_PATTERN: &str = r"(?i)/(api|graphql|rpc|v[0-9]+)(/|$)";

/// The API-segment matcher for one run, or `None` when there is no judgment to make: the caller declared
/// no pattern, or declared one that will not compile. Neither may quietly become [`API_SEGMENT_PATTERN`]
/// — substituting our value for the author's silence is the guessing this vocabulary exists to remove —
/// and neither may take the run down. A `None` marks no segment as API-ish, which for this rule's one
/// reader means the asset-directory veto stops being lifted.
pub(super) fn api_segment_re(declared: Option<&str>) -> Option<Regex> {
    declared.and_then(|d| Regex::new(d).ok())
}
