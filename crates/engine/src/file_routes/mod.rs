//! File-convention route PROVIDEs for frameworks whose HTTP surface is the file tree itself, not
//! code the other extractors can see: Next.js `pages/api/**`, Next.js app-router `app/**/route.ts`,
//! Remix flat routes (`app/routes/**`), and Medusa-style `src/api/**/route.ts`.
//!
//! Wiring shape: a whole-tree composition pass in `analyze::assemble`, NOT a per-file
//! `extract_file_io` adapter — the route is a function of the file's *path*, over data `assemble`
//! already holds (`all_symbols`, the rel list), so nothing new is cached. One exception: Next.js
//! `pages/api` handlers are `export default <expr>`, invisible to `parse_symbols`, so those
//! candidate files are re-read from disk and lexically scanned
//! (`zzop_parser_typescript::scan_pages_api_handler`).
//!
//! v1 scope decisions (deliberate; revisit with field data):
//! - Emitted kind is plain `"http"` with `http_interface_key` keys, so these provides join the
//!   existing `http` cross-layer rules with zero rule-layer changes.
//! - Verb-export conventions (app-router/Medusa `export const GET/POST/...`, Remix `loader`→GET /
//!   `action`→POST) are exact. A `pages/api` default-export handler serves any method, so its verb set
//!   comes from the lexical scan's method-literal hints; a handler that names NO method literal emits
//!   ONE `zzop_core::UNKNOWN_VERB` sentinel provide (`"? <path>"`) rather than a fabricated {GET, POST}
//!   — the `super::assemble` partition lifts it out of the exact-key join into the path-level
//!   verb-unknown disclosure (`cross-layer/unknown-verb-route`). This reverses the former engine-wide
//!   "no ANY/wildcard verb" stance (a deliberate v1 decision, superseded 2026-07-21): fabricating two
//!   real verbs both mis-provided unwitnessed methods AND lost the real one.
//! - Catch-all segments (`[...x]`, `[[...x]]`, Remix bare-`$` splat) produce NO provide: the
//!   cross-layer join is an exact key match, and a phantom never-joined provide would misreport a
//!   consumed route (e.g. `[...nextauth]`) as unconsumed.
//! - Remix emits resource routes only (a `loader`/`action` export and no default export). UI pages
//!   are navigated, not fetched; their loader endpoints are framework plumbing, not an API surface.
//! - Test/fixture trees are skipped up front (`is_test_or_fixture_path`) rather than left for the
//!   rule layer, since a fixture route is not a deployed surface. Known cost: a genuine deployed
//!   route under a literal `test`/`tests`/`testing` URL segment is dropped with it — accepted.

mod medusa;
mod next;
mod remix;

use std::collections::BTreeMap;

use zzop_core::{http_interface_key, IoProvide, SourceSymbol};

/// Verb exports recognized by the verb-export conventions (mirrors the engine-wide 5-verb
/// vocabulary). HEAD/OPTIONS are omitted — real but never fetch targets, so emitting them would
/// only mint dead provides.
const HTTP_VERB_EXPORTS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

/// Filename test for the `route.<ext>` module conventions (Next app router, Medusa).
pub(super) fn is_route_module_filename(name: &str) -> bool {
    matches!(
        name,
        "route.ts" | "route.tsx" | "route.js" | "route.jsx" | "route.mjs" | "route.cjs"
    )
}

/// Convention-local test/fixture gate, kept separate from `rules-graph`'s `test_patterns` (that one
/// is rule-layer policy and doesn't know `__fixtures__`, which this convention needs).
fn is_test_or_fixture_path(rel: &str) -> bool {
    if rel.ends_with(".d.ts") {
        return true;
    }
    let lowered = rel.to_ascii_lowercase();
    for needle in [
        ".test.",
        ".spec.",
        "/__tests__/",
        "/__test__/",
        "/__mocks__/",
        "/__fixtures__/",
        "/fixtures/",
        "/e2e/",
        "/cypress/",
        "/playwright/",
        "/testing/",
        "/tests/",
        "/test/",
    ] {
        if lowered.contains(needle) {
            return true;
        }
    }
    lowered.starts_with("test/") || lowered.starts_with("tests/")
}

/// Cheap pre-filter so the symbol grouping below never allocates for the overwhelmingly common
/// non-candidate file. Anything that survives still has to pass a convention's own exact gate.
fn quick_candidate(rel: &str) -> bool {
    rel.contains("api/")
        || rel.contains("app/routes/")
        || rel
            .rsplit('/')
            .next()
            .is_some_and(|f| f.starts_with("route."))
}

/// True when `rel` contains the exact adjacent path segments `a/b`.
pub(super) fn has_segment_pair(rel: &str, a: &str, b: &str) -> bool {
    let segs: Vec<&str> = rel.split('/').collect();
    segs.windows(2).any(|w| w[0] == a && w[1] == b)
}

/// Shared `[param]`/catch-all handling for one path segment, used by every bracket-param
/// convention so catch-all policy can't drift between them: `[param]` → `Some("{param}")`; a
/// catch-all (`[...x]`/`[[...x]]`) → `None` (not statically routable, propagated via `?`). A plain
/// segment passes through unchanged.
pub(super) fn convert_dynamic_segment(seg: &str) -> Option<String> {
    if let Some(inner) = seg.strip_prefix('[') {
        let inner = inner.strip_suffix(']')?;
        if inner.starts_with("...") || inner.starts_with('[') || inner.is_empty() {
            return None;
        }
        Some(format!("{{{inner}}}"))
    } else {
        Some(seg.to_string())
    }
}

/// Whole-tree composition entry point (see module doc). `rels` is every analyzed file's rel path;
/// `all_symbols` powers the verb-export conventions; `read_text` is the convention-gated disk
/// re-read used for `pages/api` handler scans and the Remix default-export disambiguation. Output
/// is sorted deterministically by key/file/line, independent of input order.
pub(crate) fn compose_file_convention_provides<'a>(
    rels: impl IntoIterator<Item = &'a str>,
    all_symbols: &[SourceSymbol],
    read_text: &dyn Fn(&str) -> Option<String>,
) -> Vec<IoProvide> {
    let mut out: Vec<IoProvide> = Vec::new();

    // Verb-export conventions (symbols-driven): Remix resource routes, Next app router, Medusa.
    let mut by_file: BTreeMap<&str, Vec<&SourceSymbol>> = BTreeMap::new();
    for s in all_symbols {
        if !s.exported && !s.is_default {
            continue;
        }
        if !quick_candidate(&s.file) || is_test_or_fixture_path(&s.file) {
            continue;
        }
        by_file.entry(s.file.as_str()).or_default().push(s);
    }
    for (rel, symbols) in &by_file {
        if has_segment_pair(rel, "app", "routes") {
            let Some(url) = remix::remix_flat_route(rel) else {
                continue;
            };
            // Resource routes only; fall back to a lexical check since `export default <expr>`
            // isn't symbol-visible.
            if symbols.iter().any(|s| s.is_default) {
                continue;
            }
            let has_loader_or_action = symbols
                .iter()
                .any(|s| s.name == "loader" || s.name == "action");
            if !has_loader_or_action {
                continue;
            }
            if read_text(rel).is_some_and(|t| t.contains("export default")) {
                continue;
            }
            for s in symbols {
                let verb = match s.name.as_str() {
                    "loader" => "GET",
                    "action" => "POST",
                    _ => continue,
                };
                out.push(provide(verb, &url, rel, s.line, &s.name));
            }
        } else if let Some(url) = next::app_router_route(rel).or_else(|| medusa::medusa_route(rel))
        {
            for s in symbols {
                if s.is_default {
                    continue;
                }
                if HTTP_VERB_EXPORTS.contains(&s.name.as_str()) {
                    out.push(provide(&s.name, &url, rel, s.line, &s.name));
                }
            }
        }
    }

    // Path-driven convention: Next.js `pages/api` (default-export handlers, symbol-invisible).
    let mut page_rels: Vec<&str> = rels
        .into_iter()
        .filter(|rel| quick_candidate(rel) && !is_test_or_fixture_path(rel))
        .collect();
    page_rels.sort_unstable();
    page_rels.dedup();
    for rel in page_rels {
        let Some(url) = next::pages_api_route(rel) else {
            continue;
        };
        let Some(text) = read_text(rel) else { continue };
        let scan = zzop_parser_typescript::scan_pages_api_handler(&text);
        let Some(line) = scan.default_export_line else {
            continue;
        };
        let verbs: Vec<&str> = if scan.verbs.is_empty() {
            // Serve-all handler naming no method literal: ONE UNKNOWN_VERB sentinel provide (not a
            // fabricated GET+POST) — the assemble partition lifts the `"? <path>"` key into the
            // path-level `cross-layer/unknown-verb-route` disclosure channel.
            vec![zzop_core::UNKNOWN_VERB]
        } else {
            scan.verbs.iter().map(String::as_str).collect()
        };
        for verb in verbs {
            out.push(provide(verb, &url, rel, line, "default"));
        }
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    out
}

fn provide(verb: &str, url: &str, rel: &str, line: u32, symbol: &str) -> IoProvide {
    IoProvide {
        body: None,
        kind: "http".into(),
        key: http_interface_key(verb, url),
        file: rel.to_string(),
        line,
        symbol: Some(symbol.to_string()),
    }
}

#[cfg(test)]
mod tests;
