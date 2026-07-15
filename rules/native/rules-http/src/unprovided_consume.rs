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
//! A related residual gap: raw-Worker manual dispatch (`export default { fetch }` comparing
//! `url.pathname` against literals) IS extracted by the parser's evidence-gated `pathname_dispatch`
//! adapter, but shapes outside its never-guess gate (dynamic/`startsWith` paths, const-indirected
//! literals, functions without Request evidence) remain invisible on the provide side. Severity
//! starts at [`Severity::Info`] to absorb both this residue and the extension-veto tradeoff above.
//!
//! ## Localhost absolute-URL veto
//! An absolute-URL `fetch()` call to `localhost`/`127.0.0.1` (a dev-mode self-reference to this app) is
//! extracted with a host-carrying key that can never string-match an internal, extension-free
//! `provided_keys` entry like `"GET /api/users"` — so without [`LOCALHOST_HOST_PATTERN`] every such call is
//! wrongly flagged. This is a deliberate SKIP rather than a fabricated join, since stripping the host risks
//! a false negative masking a real mismatch. A non-localhost absolute URL goes through existing logic as usual.
//!
//! ## Foreign-vs-overlapping fold (partial-provider trees)
//! Field measurement (a monorepo analyzed as ONE tree): one app in the tree contributed a handful of `http`
//! provides, which opened the zero-provides veto above, and a batch of keyed consumes from SIBLING apps —
//! served outside this analysis's scope, none matching any provided key — each fired an individual
//! [`Severity::Info`] finding. That's tone noise, not signal: this tree is only a *partial* provider, so a
//! wall of independently-worded "no route provides this" findings reads as N broken routes when it's really
//! one root cause (a monorepo where only one app's routes are visible to this analysis).
//!
//! [`unprovided_consume_findings`] splits unmatched consumes by FIRST PATH SEGMENT overlap with the tree's
//! own provided key space ([`first_path_segment`]): an unmatched consume whose first segment IS one of the
//! tree's own provided first segments ("overlapping") keeps today's individual finding unchanged — it's
//! still plausibly a typo'd or removed route under a family this tree actually serves. An unmatched consume
//! whose first segment is NOT in that space ("foreign") is folded into ONE aggregate finding once
//! [`MIN_FOREIGN_UNPROVIDED_GROUP`] or more foreign consumes accumulate, following the same
//! replace-not-silently-suppress contract as `cross-layer/prefix-drift`
//! (`rules/native/rules-cross-layer/src/cross_layer/prefix_drift.rs`): the aggregate enumerates every folded
//! key in `data.routes` and the message body, so no information is lost, only N findings replaced by one.
//! Below the fold threshold, foreign consumes still get today's individual findings — 1-2 foreign consumes
//! could be coincidence, not a partial-provider pattern.
//!
//! [`Severity::Info`]: zzop_core::Severity::Info

use std::collections::{BTreeSet, HashSet};

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

/// Fold threshold for "foreign" unprovided consumes (first path segment
/// outside the tree's provided key space). Same rationale as
/// `MIN_PREFIX_DRIFT_GROUP` in the cross-layer crate: 2 can be coincidence,
/// 3+ is a pattern (here: a partial-provider tree, e.g. a monorepo where only
/// one app's routes are extracted). Crate boundary prevents symbol sharing —
/// the relationship is pinned by an equality test in the engine crate.
pub const MIN_FOREIGN_UNPROVIDED_GROUP: usize = 3;

/// First `/`-delimited non-empty path segment of a `"METHOD /path"` (or `"METHOD <absolute-url>"`) key —
/// the unit "foreign-vs-overlapping" grouping compares (module doc). Returns `None` when the path carries
/// no segment at all (`"GET /"`), which the caller treats as foreign (nothing to overlap with).
fn first_path_segment(key: &str) -> Option<&str> {
    let path = key.split_once(' ').map(|(_, p)| p).unwrap_or(key);
    path.split('/').find(|segment| !segment.is_empty())
}

/// Builds today's individual `unprovided-consume` finding for one unmatched consume — shared by both the
/// "overlapping" leg (always individual) and the "foreign, below fold threshold" leg (module doc).
fn individual_finding(key: &str, file: &str, line: u32) -> zzop_core::Finding {
    zzop_core::Finding {
        rule_id: "unprovided-consume".to_string(),
        severity: zzop_core::Severity::Info,
        file: file.to_string(),
        line,
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
             source's provides, not just this one. This finding starts at Info severity: provide \
             extraction is evidence-gated, so route shapes it cannot prove (dynamic or \
             `startsWith` path matching, const-indirected path literals, raw-Worker dispatch \
             outside the `pathname_dispatch` adapter's Request-evidence gate) remain a \
             structural false-positive source. {} if intentional (this rule has no inline suppression marker).",
            zzop_core::disable_hint("unprovided-consume")
        ),
        data: Some(serde_json::json!({ "key": key })),
    }
}

/// One unmatched (post-veto, no provided-key match) consume, carried through the foreign/overlapping split.
struct UnmatchedConsume<'a> {
    key: &'a str,
    file: &'a str,
    line: u32,
}

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
    // See module doc "Foreign-vs-overlapping fold". A provide whose path is `/` has no segment
    // (`first_path_segment` returns `None`) and contributes nothing to this tree's provided key space.
    let provide_first_segments: BTreeSet<&str> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter_map(|p| first_path_segment(&p.key))
        .collect();
    let contributing_provide_count = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter(|p| first_path_segment(&p.key).is_some())
        .count();

    let always_veto_re = Regex::new(ALWAYS_VETO_EXTENSION_PATTERN).unwrap();
    let asset_dir_gated_re = Regex::new(ASSET_DIR_GATED_EXTENSION_PATTERN).unwrap();
    let api_segment_re = Regex::new(API_SEGMENT_PATTERN).unwrap();
    let localhost_host_re = Regex::new(LOCALHOST_HOST_PATTERN).unwrap();

    let mut overlapping: Vec<UnmatchedConsume> = Vec::new();
    let mut foreign: Vec<UnmatchedConsume> = Vec::new();

    for c in io_consumes.iter().filter(|c| c.kind == "http") {
        let Some(key) = c.key.as_deref() else {
            continue;
        };
        if provided_keys.contains(key) {
            continue;
        }
        if localhost_host_re.is_match(key) {
            continue; // localhost/127.0.0.1 absolute-URL dev self-reference — see module doc
        }
        if always_veto_re.is_match(key) {
            continue; // static-asset fetch, not API consumption — see module doc
        }
        if asset_dir_gated_re.is_match(key) && !api_segment_re.is_match(key) {
            continue; // json/xml with no API-ish path segment — vetoed by default, see module doc
        }

        let item = UnmatchedConsume {
            key,
            file: c.file.as_str(),
            line: c.line,
        };
        let is_foreign = match first_path_segment(key) {
            Some(segment) => !provide_first_segments.contains(segment),
            None => true, // no path segment at all — nothing to overlap with, see module doc
        };
        if is_foreign {
            foreign.push(item);
        } else {
            overlapping.push(item);
        }
    }

    let mut findings: Vec<zzop_core::Finding> = overlapping
        .iter()
        .map(|u| individual_finding(u.key, u.file, u.line))
        .collect();

    if foreign.len() >= MIN_FOREIGN_UNPROVIDED_GROUP {
        let mut anchor_order: Vec<&UnmatchedConsume> = foreign.iter().collect();
        anchor_order.sort_by(|a, b| {
            a.file
                .cmp(b.file)
                .then(a.line.cmp(&b.line))
                .then(a.key.cmp(b.key))
        });
        let anchor = anchor_order[0];

        let mut routes: Vec<&str> = foreign.iter().map(|u| u.key).collect();
        routes.sort_unstable();
        routes.dedup();

        let n = foreign.len();
        let m = contributing_provide_count;
        let example_segments: Vec<&str> = provide_first_segments.iter().copied().take(3).collect();
        // Only the first 3 provided first-segments are rendered inline; when more exist, append an
        // ellipsis so the message doesn't imply the tree provides only these 3 path families.
        let example_segments_str = if provide_first_segments.len() > 3 {
            format!("{}, …", example_segments.join(", "))
        } else {
            example_segments.join(", ")
        };
        // Edge case (release-audit v0.14.0, F4): a tree whose only http provides are root-path (e.g.
        // `GET /`) contributes zero first-segments (`first_path_segment` returns `None` for `/` — see
        // this fn's own doc), so `example_segments_str` is empty and `m` is 0. Rendering the normal
        // "{m} provide(s) under {segments}" clause in that case would dangle a trailing "under" with
        // nothing after it. Reword just that clause when there are no segments to show; the normal,
        // test-pinned wording below is unchanged whenever at least one segment exists.
        let path_space_clause = if provide_first_segments.is_empty() {
            "provides at least one route, but none under a named path prefix (e.g. only `GET /`)"
                .to_string()
        } else {
            format!("{m} provide(s) under {example_segments_str}")
        };

        let message = format!(
            "{n} calls in this tree consume HTTP keys that no route in this analysis provides, and none \
             of them fall under this tree's own provided path space ({path_space_clause}) — this tree \
             looks like a partial provider (e.g. a monorepo where only one app's routes are visible), so \
             these calls are most likely served by something outside this analysis rather than being {n} \
             independent broken routes. Affected keys: {}. This replaces {n} individual \
             `unprovided-consume` findings. If these should have local providers, each key above is a real \
             gap. {} if intentional (this rule has no inline suppression marker).",
            routes.join(", "),
            zzop_core::disable_hint("unprovided-consume"),
        );

        findings.push(zzop_core::Finding {
            rule_id: "unprovided-consume".to_string(),
            severity: zzop_core::Severity::Info,
            file: anchor.file.to_string(),
            line: anchor.line,
            message,
            data: Some(serde_json::json!({
                "callCount": n,
                "routes": routes,
                "provideFirstSegments": provide_first_segments.iter().collect::<Vec<_>>(),
            })),
        });
    } else {
        findings.extend(
            foreign
                .iter()
                .map(|u| individual_finding(u.key, u.file, u.line)),
        );
    }

    findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    findings
}

#[cfg(test)]
mod tests {
    //! Unit tests for `unprovided_consume_findings`'s join + veto logic in isolation (e2e coverage —
    //! real FE/BE fixtures — lives in `crates/engine/tests/analyze_io_natives.rs`).
    use super::*;

    fn provide(key: &str, file: &str, line: u32) -> zzop_core::IoProvide {
        zzop_core::IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        }
    }

    fn consume(kind: &str, key: Option<&str>, file: &str, line: u32) -> zzop_core::IoConsume {
        zzop_core::IoConsume {
            client: None,
            body: None,
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
            body: None,
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
        // All three consumes share the provide's first segment ("a") so they stay individual (overlapping,
        // never folded) — this test is only about the final sort order, not the fold behavior below.
        let provides = vec![provide("GET /a/base", "api.ts", 1)];
        let consumes = vec![
            consume("http", Some("GET /a/x"), "b.ts", 5),
            consume("http", Some("GET /a/y"), "a.ts", 9),
            consume("http", Some("GET /a/z"), "a.ts", 2),
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

    // -----------------------------------------------------------------------------------------
    // Foreign-vs-overlapping fold (see module doc "Foreign-vs-overlapping fold").
    // -----------------------------------------------------------------------------------------

    #[test]
    fn field_case_nine_foreign_unmatched_consumes_fold_into_one_aggregate() {
        // Mirrors the mono-hub field measurement: a tree that provides a handful of routes under one
        // family (/settle) but whose sibling apps' routes (served outside this analysis) leak in as
        // consumes spread across several foreign first segments.
        let provides = vec![
            provide("GET /settle/a", "settle.ts", 1),
            provide("GET /settle/b", "settle.ts", 2),
            provide("GET /settle/c", "settle.ts", 3),
            provide("GET /settle/d", "settle.ts", 4),
            provide("GET /settle/e", "settle.ts", 5),
        ];
        let consumes = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
            consume("http", Some("GET /users/1"), "client.ts", 13),
            consume("http", Some("GET /users/2"), "client.ts", 14),
            consume("http", Some("GET /users/3"), "client.ts", 15),
            consume("http", Some("GET /billing/1"), "client.ts", 16),
            consume("http", Some("GET /billing/2"), "client.ts", 17),
            consume("http", Some("GET /billing/3"), "client.ts", 18),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        let f = &found[0];
        assert_eq!(f.rule_id, "unprovided-consume");
        assert_eq!(f.severity, zzop_core::Severity::Info);
        let data = f.data.as_ref().unwrap();
        assert_eq!(data["callCount"], 9);
        let routes = data["routes"].as_array().unwrap();
        assert_eq!(routes.len(), 9);
        for c in &consumes {
            let key = c.key.as_ref().unwrap();
            assert!(
                routes.iter().any(|r| r.as_str() == Some(key.as_str())),
                "missing {key} in routes: {routes:?}"
            );
            assert!(f.message.contains(key.as_str()), "missing {key} in message");
        }
        assert!(f.message.contains("9 calls"));
        assert!(f.message.contains("settle"));
        assert!(f.message.contains("This replaces 9 individual"));
    }

    #[test]
    fn overlapping_unmatched_consume_keeps_the_individual_finding_shape() {
        // First-segment overlap ("api") preserves today's individual, byte-for-byte finding — this is the
        // typo/removed-route signal the fold must not swallow.
        let provides = vec![provide("GET /api/users", "api.ts", 1)];
        let consumes = vec![consume("http", Some("GET /api/missing"), "client.ts", 3)];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].file, "client.ts");
        assert_eq!(found[0].line, 3);
        assert!(found[0].message.contains("GET /api/missing"));
        assert!(found[0]
            .message
            .starts_with("This call consumes `GET /api/missing`"));
        assert_eq!(
            found[0].data.as_ref().unwrap()["key"].as_str(),
            Some("GET /api/missing")
        );
    }

    #[test]
    fn fires_at_threshold_not_below() {
        // Mirrors `cross-layer/prefix-drift`'s `fires_at_threshold_not_below` pin naming/shape.
        let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
        let three_foreign = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &three_foreign);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].data.as_ref().unwrap()["callCount"], 3);

        // Below threshold (only 2 foreign consumes): must stay individual, not fold.
        let two_foreign = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
        ];
        let below = unprovided_consume_findings(&provides, &two_foreign);
        assert_eq!(below.len(), 2, "{:?}", below);
        assert!(below
            .iter()
            .all(|f| f.data.as_ref().unwrap().get("callCount").is_none()));
    }

    #[test]
    fn mixed_overlapping_and_foreign_consumes_split_correctly() {
        // 1 overlapping (stays individual) + 3 foreign (fold into 1 aggregate) => 2 total findings.
        let provides = vec![provide("GET /api/users", "api.ts", 1)];
        let consumes = vec![
            consume("http", Some("GET /api/missing"), "client.ts", 3),
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 2, "{:?}", found);

        let individual = found
            .iter()
            .find(|f| f.data.as_ref().unwrap().get("key").is_some())
            .expect("individual finding for the overlapping consume");
        assert_eq!(
            individual.data.as_ref().unwrap()["key"].as_str(),
            Some("GET /api/missing")
        );

        let aggregate = found
            .iter()
            .find(|f| f.data.as_ref().unwrap().get("callCount").is_some())
            .expect("aggregate finding for the 3 foreign consumes");
        assert_eq!(aggregate.data.as_ref().unwrap()["callCount"], 3);
    }

    #[test]
    fn all_slot_consume_with_no_path_segment_overlap_counts_as_foreign() {
        // GET /{} — the all-slot placeholder is compared as a literal token ("{}"), which is not in the
        // /settle-only provide space, so it counts as foreign like any other non-overlapping segment.
        let provides = vec![provide("GET /settle/a", "settle.ts", 1)];
        let consumes = vec![
            consume("http", Some("GET /{}"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert_eq!(found[0].data.as_ref().unwrap()["callCount"], 3);
        let routes = found[0].data.as_ref().unwrap()["routes"]
            .as_array()
            .unwrap();
        assert!(routes.iter().any(|r| r.as_str() == Some("GET /{}")));
    }

    #[test]
    fn aggregate_message_appends_ellipsis_when_more_than_three_provide_segments() {
        // 4 distinct provided first segments — only the first 3 (alphabetical, via BTreeSet) are
        // rendered inline, so the message must append an ellipsis to avoid implying the tree only
        // provides those 3 path families.
        let provides = vec![
            provide("GET /alpha/a", "api.ts", 1),
            provide("GET /beta/a", "api.ts", 2),
            provide("GET /gamma/a", "api.ts", 3),
            provide("GET /delta/a", "api.ts", 4),
        ];
        let consumes = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(
            found[0].message.contains("alpha, beta, delta, …"),
            "{}",
            found[0].message
        );
    }

    #[test]
    fn aggregate_message_does_not_dangle_when_the_only_provides_are_root_path() {
        // F4 (release-audit v0.14.0): a tree whose only http provides are `GET /` contributes zero
        // first-segments (`first_path_segment` returns `None` for `/`), so the normal "{m} provide(s)
        // under {segments}" clause would render as a dangling "under " with nothing after it. The
        // reworded clause must not contain that dangling construct.
        let provides = vec![provide("GET /", "app.ts", 1)];
        let consumes = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        let message = &found[0].message;
        assert!(
            !message.contains("under (")
                && !message.contains("under )")
                && !message.contains("under  "),
            "message must not dangle a trailing \"under\" with nothing after it: {message}"
        );
        assert!(
            message.contains("none under a named path prefix"),
            "expected the reworded no-segments clause: {message}"
        );
    }

    #[test]
    fn aggregate_message_omits_ellipsis_when_three_or_fewer_provide_segments() {
        // Exactly 3 distinct provided first segments — all of them fit inline, so no ellipsis should
        // be appended (negative control for the ellipsis added above).
        let provides = vec![
            provide("GET /alpha/a", "api.ts", 1),
            provide("GET /beta/a", "api.ts", 2),
            provide("GET /gamma/a", "api.ts", 3),
        ];
        let consumes = vec![
            consume("http", Some("GET /orders/1"), "client.ts", 10),
            consume("http", Some("GET /orders/2"), "client.ts", 11),
            consume("http", Some("GET /orders/3"), "client.ts", 12),
        ];
        let found = unprovided_consume_findings(&provides, &consumes);
        assert_eq!(found.len(), 1, "{:?}", found);
        assert!(
            found[0].message.contains("alpha, beta, gamma"),
            "{}",
            found[0].message
        );
        assert!(!found[0].message.contains('…'), "{}", found[0].message);
    }
}
