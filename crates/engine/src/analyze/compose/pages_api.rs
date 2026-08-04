//! Next.js `pages/api` fragment composition — turns one candidate file's
//! `zzop_core::PagesApiHandlerScan` (the parser's default-export handler scan) into `http` PROVIDEs.
//!
//! Extracted from the call site in `crate::file_routes` (2026-08-03), BEHAVIOR-UNCHANGED, so the
//! pages-api path crosses the same typed `compose_*(zzop_core::…Fragment) -> Io…` seam every other
//! recognizer fragment does — the seam `rule_contracts::recognizer_channels`' type hop derives the
//! fragment→channel map from. Until then this was the one composition that happened inline at a
//! call site, and the `next.js` row had to live in that contract's `CHANNEL_NOT_CODE_DERIVED` pin
//! as a mechanism limit. The caller (`file_routes::compose_file_convention_provides`) still owns
//! the convention gates: which rels are candidates, the path→URL mapping, and the disk re-read.

use zzop_core::{http_interface_key, IoProvide};

/// One scanned `pages/api` file's provides. No default export (or no parse) composes nothing; a
/// handler whose scan witnessed no method literal emits ONE `zzop_core::UNKNOWN_VERB` sentinel
/// provide (`"? <path>"`) rather than a fabricated GET+POST — the assemble partition lifts that key
/// out of the exact-key join into the path-level `cross-layer/unknown-verb-route` disclosure (see
/// `file_routes`' module doc for the v1 decision trail).
pub(crate) fn compose_pages_api_provides(
    rel: &str,
    url: &str,
    scan: &zzop_core::PagesApiHandlerScan,
) -> Vec<IoProvide> {
    let Some(line) = scan.default_export_line else {
        return Vec::new();
    };
    let verbs: Vec<&str> = if scan.verbs.is_empty() {
        vec![zzop_core::UNKNOWN_VERB]
    } else {
        scan.verbs.iter().map(String::as_str).collect()
    };
    verbs
        .into_iter()
        .map(|verb| IoProvide {
            response: None,
            body: None,
            kind: "http".into(),
            key: http_interface_key(verb, url),
            file: rel.to_string(),
            line,
            symbol: Some("default".to_string()),
        })
        .collect()
}
