//! Every VOCABULARY `unprovided-consume` reads — the two static-asset extension tiers and the one
//! DECLARABLE segment pattern that gates the second of them. Split out of the parent module both to keep
//! that file under the repo's per-file line cap and to put each value beside the prose that explains it.
//!
//! **The two-tier static-asset veto.** A static-asset fetch (`public/` JSON, `.svg` icons, ...) is not API
//! consumption. [`ALWAYS_VETO_EXTENSION_PATTERN`] vetoes asset-shaped extensions unconditionally, anchored
//! to end-of-path. [`ASSET_DIR_GATED_EXTENSION_PATTERN`] (`json`/`xml`) also legitimately names a real API
//! shape (`GET /api/users.json`), so it is gated on the run's DECLARED API-ish path segment instead — what
//! that gate covers, and what it does not, is the next paragraph.
//!
//! What the gate covers, and what it does not. It lifts the `json`/`xml` veto for a key whose path
//! carries a declared API segment, rather than for a key under an asset directory — some frameworks strip
//! the `public/` prefix from served asset URLs, so a Next.js `public/i18n/*.json` fetch keyed
//! `GET /i18n/{}.json` is vetoed by the ABSENT API segment. Two consequences follow, both under-reporting
//! rather than fabricating: an API route living outside every declared segment is missed here, and a run
//! that declares NO `vocabulary.apiSegmentPattern` marks no path API-ish at all, so the veto is never
//! lifted for anything.

use regex::Regex;

/// Always-veto extension vocabulary — see the parent module's "Static-asset veto". Anchored to
/// end-of-path (optionally followed by a query string or fragment), not merely appearing anywhere in the
/// key. Members complete the families already present (images/fonts/scripts) rather than opening a new
/// class — none of them can name an API route shape (unlike `json`/`xml`, gated below).
pub(super) const ALWAYS_VETO_EXTENSION_PATTERN: &str =
    r"(?i)\.(svg|png|jpe?g|gif|ico|bmp|avif|css|txt|webp|woff2?|ttf|otf|eot|map|[mc]?js)([?#]|$)";

/// API-segment-gated extension vocabulary — see the parent module's "Static-asset veto". Vetoed unless
/// [`API_SEGMENT_PATTERN`] also matches (inverted gate: absence of an API-ish segment is the veto signal).
pub(super) const ASSET_DIR_GATED_EXTENSION_PATTERN: &str = r"(?i)\.(json|xml)([?#]|$)";

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
