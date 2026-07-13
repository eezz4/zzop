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
//! - No wildcard/ANY verb exists engine-wide. Verb-export conventions (app-router/Medusa
//!   `export const GET/POST/...`, Remix `loader`→GET / `action`→POST) are exact. `pages/api`
//!   default-export handlers serve any method, so their verb set comes from the lexical scan's
//!   method-literal hints, falling back to {GET, POST} when the file body names no verb.
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

/// `pages/api` fallback when the handler body names no method literal (see module doc).
const PAGES_API_FALLBACK_VERBS: [&str; 2] = ["GET", "POST"];

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
            PAGES_API_FALLBACK_VERBS.to_vec()
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
mod tests {
    //! Orchestration coverage: verb-export wiring, pages/api lexical-scan wiring, the Remix
    //! resource-route gate, and the test/fixture skip. Path→URL transforms are tested per-submodule.
    use super::*;
    use zzop_core::SourceSymbolKind;

    fn sym(file: &str, name: &str, line: u32, is_default: bool) -> SourceSymbol {
        SourceSymbol {
            id: format!("{file}#{name}"),
            file: file.into(),
            name: name.into(),
            kind: SourceSymbolKind::Const,
            line,
            exported: true,
            is_default,
            body_start: None,
            body_end: None,
            write_sites: Vec::new(),
        }
    }

    fn no_text(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn medusa_verb_exports_become_http_provides() {
        let rel = "packages/medusa/src/api/admin/campaigns/[id]/route.ts";
        let symbols = vec![sym(rel, "GET", 10, false), sym(rel, "POST", 40, false)];
        let out = compose_file_convention_provides([rel], &symbols, &no_text);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key, "GET /admin/campaigns/{}");
        assert_eq!(out[0].kind, "http");
        assert_eq!(out[0].line, 10);
        assert_eq!(out[0].symbol.as_deref(), Some("GET"));
        assert_eq!(out[1].key, "POST /admin/campaigns/{}");
    }

    #[test]
    fn non_verb_exports_on_route_modules_emit_nothing() {
        let rel = "packages/medusa/src/api/admin/campaigns/route.ts";
        let symbols = vec![
            sym(rel, "AUTHENTICATE", 3, false),
            sym(rel, "config", 5, false),
        ];
        let out = compose_file_convention_provides([rel], &symbols, &no_text);
        assert!(out.is_empty());
    }

    #[test]
    fn fixture_and_test_paths_are_skipped() {
        let fixture = "integration-tests/http/__fixtures__/x/src/api/admin/route.ts";
        let test = "apps/web/pages/api/book/recurring-event.test.ts";
        let symbols = vec![sym(fixture, "GET", 1, false)];
        let out = compose_file_convention_provides([fixture, test], &symbols, &|_| {
            Some("export default handler;".into())
        });
        assert!(out.is_empty());
    }

    #[test]
    fn app_router_verb_exports_become_http_provides() {
        let rel = "apps/web/app/api/cancel/route.ts";
        let symbols = vec![sym(rel, "POST", 7, false), sym(rel, "DELETE", 9, false)];
        let out = compose_file_convention_provides([rel], &symbols, &no_text);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].key, "DELETE /api/cancel");
        assert_eq!(out[1].key, "POST /api/cancel");
    }

    #[test]
    fn pages_api_uses_scanned_verb_hints() {
        let rel = "apps/web/pages/api/book/event.ts";
        let text = concat!(
            "async function handler(req, res) {\n",
            "  if (req.method !== \"POST\") return res.status(405).end();\n",
            "}\n",
            "export default handler;\n",
        );
        let out =
            compose_file_convention_provides([rel], &[], &|r| (r == rel).then(|| text.to_string()));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "POST /api/book/event");
        assert_eq!(out[0].line, 4);
        assert_eq!(out[0].symbol.as_deref(), Some("default"));
    }

    #[test]
    fn pages_api_without_method_literals_falls_back_to_get_post() {
        let rel = "apps/web/pages/api/auth/verify-email.ts";
        let out = compose_file_convention_provides([rel], &[], &|_| {
            Some("export default handler;\n".into())
        });
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["GET /api/auth/verify-email", "POST /api/auth/verify-email"]
        );
    }

    #[test]
    fn remix_resource_route_maps_loader_and_action() {
        let rel = "apps/remix/app/routes/api+/stripe.webhook.ts";
        let symbols = vec![sym(rel, "action", 12, false)];
        let out = compose_file_convention_provides([rel], &symbols, &no_text);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "POST /api/stripe/webhook");
        assert_eq!(out[0].symbol.as_deref(), Some("action"));
    }

    #[test]
    fn remix_ui_page_with_default_export_is_not_a_provide() {
        let rel = "apps/remix/app/routes/_authenticated+/dashboard.tsx";
        let symbols = vec![
            sym(rel, "loader", 5, false),
            sym(rel, "Dashboard", 20, true),
        ];
        let out = compose_file_convention_provides([rel], &symbols, &no_text);
        assert!(out.is_empty());
    }

    /// Policy-value equality pin (T2): `PAGES_API_FALLBACK_VERBS` here and
    /// `zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS` encode the same policy decision
    /// (the verb set assumed for a handler that names no method) but live on opposite sides of a
    /// crate boundary where symbol sharing is impossible. This pin forces a deliberate change on
    /// either side to be re-justified against the other rather than silently drifting apart.
    #[test]
    fn pathname_dispatch_fallback_verbs_pin() {
        assert_eq!(
            PAGES_API_FALLBACK_VERBS,
            zzop_parser_typescript::PATHNAME_DISPATCH_FALLBACK_VERBS,
            "PAGES_API_FALLBACK_VERBS and PATHNAME_DISPATCH_FALLBACK_VERBS must stay in lockstep \
             (policy-value equality pin, T2)"
        );
    }

    /// Policy-value set-equality pin (T2): `HTTP_VERB_EXPORTS` (which export NAMES count as verb
    /// handlers in file-convention routing — deliberately omits HEAD/OPTIONS, see its doc) and
    /// core's `HTTP_KEY_VERBS` (the name-inferred verb keying vocabulary) are DIFFERENT policy
    /// domains that today hold the same 5-verb set. If either grows or shrinks deliberately
    /// (e.g. core learns HEAD), this pin forces the divergence to be decided rather than drift.
    #[test]
    fn http_verb_exports_matches_core_key_verbs_set() {
        let mut exports: Vec<&str> = HTTP_VERB_EXPORTS.to_vec();
        let mut core: Vec<&str> = zzop_core::HTTP_KEY_VERBS.to_vec();
        exports.sort_unstable();
        core.sort_unstable();
        assert_eq!(
            exports, core,
            "HTTP_VERB_EXPORTS and zzop_core::HTTP_KEY_VERBS hold the same verb set today; a \
             deliberate change to either must be re-justified here (policy set-equality pin, T2)"
        );
    }

    /// Policy-value subset pin (T2): the no-method fallback verbs must always be drawn from the
    /// core verb vocabulary — a fallback verb core cannot key would mint unjoinable provides.
    #[test]
    fn pages_api_fallback_verbs_are_a_subset_of_core_key_verbs() {
        for v in PAGES_API_FALLBACK_VERBS {
            assert!(
                zzop_core::HTTP_KEY_VERBS.contains(&v),
                "fallback verb {v} is not in zzop_core::HTTP_KEY_VERBS (policy subset pin, T2)"
            );
        }
    }

    #[test]
    fn remix_default_expr_page_is_caught_by_lexical_fallback() {
        // `export default memo(Page)` produces no `parse_symbols` default symbol — the re-read
        // lexical check is what keeps this UI page out of the provide surface.
        let rel = "apps/remix/app/routes/api+/pretend.ts";
        let symbols = vec![sym(rel, "loader", 5, false)];
        let out = compose_file_convention_provides([rel], &symbols, &|_| {
            Some("export default memo(Page);".into())
        });
        assert!(out.is_empty());
    }
}
