//! `unprovided-consume` — an `IoConsume` (`kind == "http"`, `key` resolved `Some`) with no matching
//! `IoProvide` key in the same analysis. A single-tree, narrower cousin of
//! `zzop_core::link_cross_layer_io`'s `unprovided_consumes` set.
//!
//! ## The zero-provides veto
//! A pure front-end tree with ZERO `http` provides of its own legitimately consumes routes served by a
//! remote backend outside this analysis's scope, so this rule only runs when the tree has at least one
//! `http` provide itself — an unmatched consume there is more likely a typo'd path than "someone else's API".
//!
//! ## Single-tree vs multi-tree
//! This runs at the single-tree level (`analyze::assemble`), so it never sees a sibling tree's provides.
//! `MultiAnalyzeOutput::cross_layer.unprovided_consumes` matches every consume against every tree's provides
//! unconditionally (provides from ANY tree already cover the remote-backend case, so no zero-provides veto
//! is needed there) — prefer that cross-tree join for a split FE/BE repo pair.
//!
//! ## Static-asset veto
//! Static-asset fetches (`public/` JSON, `.svg` icons, ...) are not API consumption, so the veto has two
//! tiers. [`ALWAYS_VETO_EXTENSION_PATTERN`] vetoes static-asset-shaped extensions unconditionally, anchored
//! to end-of-path. [`ASSET_DIR_GATED_EXTENSION_PATTERN`] (`json`/`xml`) also legitimately names a real API
//! shape (`GET /api/users.json`), so it's gated on an API-ish path segment ([`API_SEGMENT_PATTERN`]) instead
//! of an asset-directory allowlist, since some frameworks strip the `public/` prefix from served asset URLs.
//! Tradeoff: an API route living outside any `/api`-ish segment is missed by this veto too.
//!
//! A related structural gap: the raw-Worker `export default { fetch }` pattern's provides are not yet
//! extracted. Severity starts at [`Severity::Info`] to absorb both gaps.
//!
//! ## Localhost absolute-URL veto
//! An absolute-URL `fetch()` call to `localhost`/`127.0.0.1` (a dev-mode self-reference to this app) is
//! extracted with a host-carrying key that can never string-match an internal, extension-free
//! `provided_keys` entry like `"GET /api/users"` — so without [`LOCALHOST_HOST_PATTERN`] every such call is
//! wrongly flagged. This is a deliberate SKIP rather than a fabricated join, since stripping the host risks
//! a false negative masking a real mismatch. A non-localhost absolute URL goes through existing logic as usual.
//!
//! [`Severity::Info`]: zzop_core::Severity::Info

use std::collections::HashSet;

use regex::Regex;

/// Always-veto extension vocabulary — see module doc "Static-asset veto". Anchored to end-of-path
/// (optionally followed by a query string or fragment), not merely appearing anywhere in the key.
const ALWAYS_VETO_EXTENSION_PATTERN: &str =
    r"(?i)\.(svg|png|jpe?g|gif|ico|css|txt|webp|woff2?|map|js)([?#]|$)";

/// API-segment-gated extension vocabulary — see module doc "Static-asset veto". Vetoed unless
/// [`API_SEGMENT_PATTERN`] also matches (inverted gate: absence of an API-ish segment is the veto signal).
const ASSET_DIR_GATED_EXTENSION_PATTERN: &str = r"(?i)\.(json|xml)([?#]|$)";

/// API-ish path-segment vocabulary — see module doc "Static-asset veto". `/`-delimited so it matches a
/// whole path segment, not a bare substring (e.g. `/apiary/` does not match `/api/`).
const API_SEGMENT_PATTERN: &str = r"(?i)/(api|graphql|rpc|v[0-9]+)(/|$)";

/// Localhost/loopback absolute-URL host vocabulary — see module doc "Localhost absolute-URL veto". Anchored
/// so `localhost`/`127.0.0.1` must be the URL's host, not merely a substring elsewhere in the key.
const LOCALHOST_HOST_PATTERN: &str = r"(?i)^\S+\s+https?://(localhost|127\.0\.0\.1)(:\d+)?(/|$)";

pub fn unprovided_consume_findings(
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
) -> Vec<zzop_core::Finding> {
    let has_http_provide = io_provides.iter().any(|p| p.kind == "http");
    if !has_http_provide {
        return Vec::new();
    }
    let provided_keys: HashSet<&str> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .map(|p| p.key.as_str())
        .collect();
    let always_veto_re = Regex::new(ALWAYS_VETO_EXTENSION_PATTERN).unwrap();
    let asset_dir_gated_re = Regex::new(ASSET_DIR_GATED_EXTENSION_PATTERN).unwrap();
    let api_segment_re = Regex::new(API_SEGMENT_PATTERN).unwrap();
    let localhost_host_re = Regex::new(LOCALHOST_HOST_PATTERN).unwrap();

    let mut findings: Vec<zzop_core::Finding> = io_consumes
        .iter()
        .filter(|c| c.kind == "http")
        .filter_map(|c| {
            let key = c.key.as_deref()?;
            if provided_keys.contains(key) {
                return None;
            }
            if localhost_host_re.is_match(key) {
                return None; // localhost/127.0.0.1 absolute-URL dev self-reference — see module doc
            }
            if always_veto_re.is_match(key) {
                return None; // static-asset fetch, not API consumption — see module doc
            }
            if asset_dir_gated_re.is_match(key) && !api_segment_re.is_match(key) {
                return None; // json/xml with no API-ish path segment — vetoed by default, see module doc
            }
            Some(zzop_core::Finding {
                rule_id: "unprovided-consume".to_string(),
                severity: zzop_core::Severity::Info,
                file: c.file.clone(),
                line: c.line,
                message: format!(
                    "This call consumes `{key}` but no HTTP route anywhere in this analysis provides that \
                     key — likely a typo'd path, a renamed/removed backend route, or a route defined in a \
                     file this analysis didn't parse. Verify the route still exists at that path and method. \
                     Veto: consumes whose key path ends in a static-asset extension (.js, .css, .svg, .png, \
                     .jpg/.jpeg, .gif, .ico, .txt, .webp, .woff/.woff2, .map) are never flagged. \
                     Consumes ending in .json or .xml are vetoed by default UNLESS the path contains an \
                     API-ish segment (/api/, /graphql/, /rpc/, or a version segment like /v1/) — e.g. \
                     `GET /i18n/ko.json` and `GET /public/recipes.json` are vetoed, but \
                     `GET /api/users.json` is real API consumption (Rails-style format-suffixed routes) \
                     and stays flaggable. Tradeoff: a Rails-style .json/.xml API route living outside any \
                     /api-ish path segment will also be missed by this veto. An absolute-URL consume whose \
                     host is localhost or 127.0.0.1 (with or without a port) is treated as a same-app \
                     dev self-reference and is never flagged — a deliberate skip rather than a fabricated \
                     host-stripped path join. Note: this only \
                     fires because this same source ALSO provides at least one HTTP route itself — a source with \
                     zero HTTP provides is assumed to be consuming a remote backend outside this analysis's \
                     scope and is never flagged by this rule (that veto avoids a systematic false-positive \
                     class for pure front-end sources). If you're analyzing a split FE/BE repo pair, prefer \
                     the multi-source `analyze_trees` cross-layer join \
                     (`MultiAnalyzeOutput::cross_layer.unprovided_consumes`), which matches consumes against every \
                     source's provides, not just this one. This finding starts at Info severity: raw-Worker \
                     route extraction (`export default {{ fetch }}`) is not yet covered by this analysis's \
                     provides-extraction, which remains a structural false-positive source until that \
                     extraction lands. Disable via rule config `disabled_rules: [\"unprovided-consume\"]` \
                     if intentional (native rules have no inline suppression marker)."
                ),
                data: Some(serde_json::json!({ "key": key })),
            })
        })
        .collect();
    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    findings
}

#[cfg(test)]
mod tests {
    //! Unit tests for `unprovided_consume_findings`'s join + veto logic in isolation (e2e coverage —
    //! real FE/BE fixtures — lives in `packages/engine/tests/analyze_io_natives.rs`).
    use super::*;

    fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        }
    }

    fn consume(kind: &str, key: Option<&str>, file: &str, line: u32) -> zzop_core::IoConsume {
        zzop_core::IoConsume {
            kind: kind.to_string(),
            key: key.map(str::to_string),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
        }
    }

    #[test]
    fn unmatched_consume_is_flagged_when_the_tree_has_a_provide() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /missing"), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "client.ts");
        assert_eq!(found[0].line, 3);
        assert_eq!(found[0].rule_id, "unprovided-consume");
        assert_eq!(found[0].severity, zzop_core::Severity::Info);
        assert!(found[0].message.contains("GET /missing"));
    }

    #[test]
    fn always_veto_static_asset_extension_consume_is_never_flagged() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /assets/icon.svg"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn always_veto_extension_followed_by_a_query_string_is_still_vetoed() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /assets/icon.svg?v=2"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn json_in_a_public_asset_directory_is_vetoed() {
        // /public/recipes.json — no API-ish segment anywhere in the path, so it's vetoed by default.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /public/recipes.json"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn xml_in_a_static_asset_directory_is_vetoed() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /static/sitemap.xml"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn next_js_public_prefix_stripped_json_path_is_vetoed() {
        // Some frameworks serve public/ files with the `public/` prefix stripped from the URL — no
        // asset-directory segment survives in the key, but the API-segment gate still catches this.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /i18n/ko.json"), "client.ts", 3)];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn rails_style_json_api_route_with_an_api_segment_still_fires() {
        // GET /api/users.json — Rails-style format-suffixed API route, real API consumption; the /api/
        // segment stops the default json/xml veto from applying.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /api/users.json"), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].severity, zzop_core::Severity::Info);
        assert!(found[0].message.contains("GET /api/users.json"));
    }

    #[test]
    fn xml_with_an_api_segment_still_fires() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /api/feed.xml"), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
    }

    #[test]
    fn versioned_api_segment_json_route_still_fires() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /v1/users.json"), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
    }

    #[test]
    fn graphql_segment_json_route_still_fires() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /graphql/schema.json"),
            "client.ts",
            3,
        )];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
    }

    #[test]
    fn json_path_with_no_api_segment_is_vetoed_regardless_of_directory_name() {
        // "/database/export.json" — not under a conventional asset directory either, but the inverted
        // gate vetoes it by default anyway since no /api/,/graphql/,/rpc/,/vN/ segment is present.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /database/export.json"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn api_segment_match_requires_a_whole_path_segment_not_a_substring() {
        // "/apiary/" contains "api" as a substring but not as a whole `/api/` path segment — this must
        // still be vetoed (no real API-ish segment present), not fooled by the substring.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /apiary/export.json"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn a_path_that_only_contains_an_asset_extension_mid_segment_is_not_vetoed() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET /api/json-export"),
            "client.ts",
            3,
        )];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
    }

    #[test]
    fn matched_consume_is_never_flagged() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /a"), "client.ts", 3)];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn zero_http_provides_vetoes_every_consume_pure_fe_tree() {
        let consumes = vec![consume("http", Some("GET /remote"), "client.ts", 3)];
        assert!(unprovided_consume_findings(&[], &consumes).is_empty());
    }

    #[test]
    fn unresolved_consume_key_none_is_never_flagged() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("http", None, "client.ts", 3)];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn non_http_consume_kind_is_ignored() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume("queue", Some("topic:x"), "client.ts", 3)];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn a_non_http_provide_does_not_satisfy_the_zero_provides_gate() {
        let provides = vec![zzop_core::IoProvide {
            kind: "queue".to_string(),
            key: "topic:x".to_string(),
            file: "worker.ts".to_string(),
            line: 1,
            symbol: None,
        }];
        let consumes = vec![consume("http", Some("GET /missing"), "client.ts", 3)];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn localhost_absolute_url_consume_is_vetoed() {
        // The host-carrying key can never string-match the internal, extension-free provided key ("GET
        // /a"), so it must be skipped rather than wrongly flagged.
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET https://localhost:3000/api/users"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn localhost_with_port_and_path_absolute_url_consume_is_vetoed() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("POST https://localhost:8080/api/orders/create"),
            "client.ts",
            7,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn loopback_ip_absolute_url_consume_is_vetoed() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET https://127.0.0.1:3000/api/users"),
            "client.ts",
            3,
        )];
        assert!(unprovided_consume_findings(&provides, &consumes).is_empty());
    }

    #[test]
    fn non_localhost_absolute_url_consume_is_still_flagged_when_unprovided() {
        // Negative control: a non-localhost absolute URL must NOT be swept up by the new veto — it keeps
        // going through the existing join/veto logic exactly as before (still flagged when unmatched).
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![consume(
            "http",
            Some("GET https://api.stripe.com/v1/charges"),
            "client.ts",
            3,
        )];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(found[0]
            .message
            .contains("https://api.stripe.com/v1/charges"));
    }

    #[test]
    fn results_sorted_by_file_then_line() {
        let provides = vec![provide("GET /a", "api.ts", 1)];
        let consumes = vec![
            consume("http", Some("GET /x"), "b.ts", 5),
            consume("http", Some("GET /y"), "a.ts", 9),
            consume("http", Some("GET /z"), "a.ts", 2),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 3);
        assert_eq!(
            found
                .iter()
                .map(|f| (f.file.as_str(), f.line))
                .collect::<Vec<_>>(),
            vec![("a.ts", 2), ("a.ts", 9), ("b.ts", 5)]
        );
    }
}
