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
mod tests;
