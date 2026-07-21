//! `cross-layer/unknown-verb-route` (info) — a route whose PATH is statically served but whose HTTP
//! method(s) could not be determined at parse time: a handler that serves every method (a Next.js
//! `pages/api` default export answering all verbs), a `pathname`-dispatch block that branches on the URL
//! path without pinning a method literal, or a Go `HandleFunc` registration with no method guard. The path
//! reaches a live handler, but the verb is a static unknown.
//!
//! This is the honest-disclosure half of the 1b fabrication removal: the parsers used to emit these
//! verb-unknown routes as a fabricated `[GET, POST]` pair, inventing a method surface that no code actually
//! pins. That fabrication is gone. Instead of guessing, the join now surfaces the route as a disclosure —
//! the path is served, the method is unknown, and inject-to-confirm is the remedy — so no verb-level
//! cross-layer check runs on a made-up method and no invented verb ever reaches a finding.
//!
//! Tone and inject-pointer phrasing mirror `cross-layer/unresolved-consume-ratio`: a self-report of a known
//! blind spot with a Mode B overlay (Normalized AST adapter) as the resolution, never an error.

use std::collections::BTreeSet;

use zzop_core::{disable_hint, Finding, Severity};

/// One route site whose path is served but whose HTTP method is a static unknown. `path` is already
/// normalized to `zzop_core::normalize_http_path`'s output shape (e.g. `/me/achievements`); the caller
/// (`zzop_engine::analyze_trees`) derives these from the same provide inputs it builds for the join.
#[derive(Debug, Clone)]
pub struct UnknownVerbRouteSite {
    pub source: String,
    pub path: String,
    pub file: String,
    pub line: u32,
}

/// One info `Finding` per unique `(source, path, file, line)` site: the endpoint's path is served but its
/// HTTP method(s) could not be determined statically, so exact verb-level cross-layer checks cannot run on
/// it. Deterministic: deduped on the full tuple, sorted by `(file, line, path)`.
pub fn unknown_verb_route_findings(sites: &[UnknownVerbRouteSite]) -> Vec<Finding> {
    let mut seen: BTreeSet<(&str, &str, &str, u32)> = BTreeSet::new();
    let mut out = Vec::new();
    for site in sites {
        if !seen.insert((
            site.source.as_str(),
            site.path.as_str(),
            site.file.as_str(),
            site.line,
        )) {
            continue;
        }

        let path = &site.path;
        // Paste-ready `routes` injection stub: the PATH is known, so only the VERB is a `<VERB>`
        // placeholder the user fills. The route is served by THIS site's own tree (`site.source`), so the
        // entry goes in that tree's own `routes` array — no cross-tree ambiguity (unlike the unprovided-*
        // stubs, whose serving tree is unknown).
        let injection_stub = format!("routes: [{{ \"key\": \"<VERB> {path}\" }}]");
        let message = format!(
            "route `{path}` is served but its HTTP method(s) could not be determined statically — the \
             handler answers every method (an all-verb `pages/api` handler), or the route comes from a \
             `pathname`-dispatch block or a Go `HandleFunc` registration that pins no method literal. The \
             PATH is confirmed served; only the VERB is unknown. Because the method is a static blind spot, \
             no exact verb-level cross-layer check (`cross-layer/unconsumed-endpoint`, \
             `cross-layer/path-near-miss`, `cross-layer/method-mismatch`, ...) can run against this route — \
             they would have to invent a method to compare, which this analysis deliberately does not do. \
             This is a disclosure, not an error: inject the concrete method(s) for this route and the exact \
             verb-level checks activate — the fastest way is this tree's `routes` field (`{injection_stub}`, \
             replacing `<VERB>` with the real method), or a full Normalized AST adapter for many routes. {} \
             if the route is intentionally method-agnostic and the resulting verb-level blindness is accepted.",
            disable_hint("cross-layer/unknown-verb-route"),
        );

        out.push(Finding {
            rule_id: "cross-layer/unknown-verb-route".to_string(),
            severity: Severity::Info,
            file: site.file.clone(),
            line: site.line,
            message,
            data: Some(serde_json::json!({
                "source": site.source,
                "servedPath": site.path,
                "injectionStub": injection_stub,
            })),
        });
    }

    out.sort_by(|a, b| {
        a.file.cmp(&b.file).then(a.line.cmp(&b.line)).then_with(|| {
            let ap = a.data.as_ref().and_then(|d| d["servedPath"].as_str());
            let bp = b.data.as_ref().and_then(|d| d["servedPath"].as_str());
            ap.cmp(&bp)
        })
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(source: &str, path: &str, file: &str, line: u32) -> UnknownVerbRouteSite {
        UnknownVerbRouteSite {
            source: source.to_string(),
            path: path.to_string(),
            file: file.to_string(),
            line,
        }
    }

    #[test]
    fn one_site_yields_one_finding_with_expected_fields() {
        let out = unknown_verb_route_findings(&[site("api", "/me/achievements", "handler.ts", 12)]);
        assert_eq!(out.len(), 1);
        let f = &out[0];
        assert_eq!(f.rule_id, "cross-layer/unknown-verb-route");
        assert_eq!(f.severity, Severity::Info);
        assert_eq!(f.file, "handler.ts");
        assert_eq!(f.line, 12);
        assert!(f.message.contains("/me/achievements"));
        assert!(f.message.contains("disabled_rules"));
        let data = f.data.as_ref().unwrap();
        assert_eq!(data["source"], "api");
        assert_eq!(data["servedPath"], "/me/achievements");
        // Paste-ready injection stub: path known, verb a `<VERB>` placeholder.
        assert_eq!(
            data["injectionStub"],
            "routes: [{ \"key\": \"<VERB> /me/achievements\" }]"
        );
        assert!(f
            .message
            .contains("routes: [{ \"key\": \"<VERB> /me/achievements\" }]"));
    }

    #[test]
    fn duplicate_source_path_file_line_dedupes_to_one() {
        let out = unknown_verb_route_findings(&[
            site("api", "/me/achievements", "handler.ts", 12),
            site("api", "/me/achievements", "handler.ts", 12),
        ]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(unknown_verb_route_findings(&[]).is_empty());
    }

    #[test]
    fn output_is_sorted_by_file_then_line_then_path() {
        let out = unknown_verb_route_findings(&[
            site("api", "/z", "b.ts", 3),
            site("api", "/a", "a.ts", 9),
            site("api", "/beta", "a.ts", 1),
            site("api", "/alpha", "a.ts", 1),
        ]);
        assert_eq!(out.len(), 4);
        // (a.ts, 1, /alpha), (a.ts, 1, /beta), (a.ts, 9, /a), (b.ts, 3, /z)
        assert_eq!((out[0].file.as_str(), out[0].line), ("a.ts", 1));
        assert_eq!(out[0].data.as_ref().unwrap()["servedPath"], "/alpha");
        assert_eq!(out[1].data.as_ref().unwrap()["servedPath"], "/beta");
        assert_eq!((out[2].file.as_str(), out[2].line), ("a.ts", 9));
        assert_eq!((out[3].file.as_str(), out[3].line), ("b.ts", 3));
    }
}
