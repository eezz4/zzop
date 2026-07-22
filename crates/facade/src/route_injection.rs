//! Lightweight route-fact injection: `AnalyzeRequest::routes` -> a synthetic adapter overlay. See
//! `crate::request::RouteInjectionRequest`'s doc for the caller-facing contract; this is the expansion
//! `config::build_engine_config` calls, kept in its own module so `config.rs` stays under the file-size
//! guard.

use crate::request::{RouteInjectionRequest, RouteRole};

/// Synthetic file marker every injected route is attributed to — a caller-declared fact is NOT extracted
/// source, and this non-source path (matched by no real file, and in no language's whole-corpus provide set
/// so a language pass never replaces it) also makes the injection survive every provide transform.
pub(crate) const INJECTED_ROUTES_FILE: &str = "<injected-routes>";

/// Expands `AnalyzeRequest::routes` into a single synthetic adapter overlay — the lightweight route-fact
/// injection sugar over the proven overlay-`io` join path (`RouteInjectionRequest`'s doc). Each entry
/// becomes one `http` provide or consume, keyed through the SAME normalization the native extractors use
/// for that side — `http_interface_key` for a provide, the query/fragment-dropping
/// `http_consume_interface_key` for a consume — so an injected `"get /api/users"` (or a consume
/// `"GET /articles?limit=10"`) joins the native route exactly. An entry whose `key` is not a `METHOD`+`PATH`
/// pair is skipped with a warning
/// — an injected fact that can never join is surfaced, never silently dropped. Returns `None` when no valid
/// route survives, so no empty overlay is appended. `source` is the tree's own `source_id`, so the overlay
/// makes no intra-source-mismatch claim.
pub(crate) fn routes_overlay(
    source_id: &str,
    routes: &[RouteInjectionRequest],
    warnings: &mut Vec<String>,
) -> Option<zzop_core::NormalizedEnvelope> {
    let mut provides = Vec::new();
    let mut consumes = Vec::new();
    for (i, r) in routes.iter().enumerate() {
        let line = (i + 1) as u32;
        let trimmed = r.key.trim();
        let parsed = trimmed
            .split_once(' ')
            .map(|(m, p)| (m.trim(), p.trim()))
            .filter(|(m, p)| !m.is_empty() && !p.is_empty());
        let Some((method, path)) = parsed else {
            warnings.push(format!(
                "injected route \"{}\" is not a \"METHOD PATH\" pair (e.g. \"GET /api/users\") — skipped",
                r.key
            ));
            continue;
        };
        match r.role {
            // Provides keep the raw path: a route TEMPLATE's `?` can be a wildcard, never a query string
            // (`zzop_core::io::key`'s contract), so provides use `http_interface_key` verbatim.
            RouteRole::Provide => provides.push(zzop_core::IoProvide {
                body: None,
                kind: "http".to_string(),
                key: zzop_core::http_interface_key(method, path),
                file: INJECTED_ROUTES_FILE.to_string(),
                line,
                symbol: None,
            }),
            // Consumes are call URLs: `http_consume_interface_key` drops any `?query`/`#fragment` before
            // keying, exactly as every native egress extractor does — so an injected
            // `GET /articles?limit=10` joins the native `GET /articles` provide.
            RouteRole::Consume => consumes.push(zzop_core::IoConsume {
                client: None,
                body: None,
                kind: "http".to_string(),
                key: Some(zzop_core::http_consume_interface_key(method, path)),
                file: INJECTED_ROUTES_FILE.to_string(),
                line,
                raw: None,
                method: Some(method.to_uppercase()),
                retry_configured: None,
            }),
        }
    }
    if provides.is_empty() && consumes.is_empty() {
        return None;
    }

    let io = zzop_core::IoFacts { provides, consumes };
    let file = zzop_core::FileProjection {
        path: INJECTED_ROUTES_FILE.to_string(),
        loc: routes.len() as u32,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: std::collections::HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        class_shape_fragments: Vec::new(),
        io,
        attributes: Vec::new(),
        loop_spans: Vec::new(),
        degraded: false,
        is_entry: false,
    };
    Some(zzop_core::NormalizedEnvelope {
        format: zzop_core::NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: "zzop-route-injection/1".to_string(),
        source: source_id.to_string(),
        files: vec![file],
    })
}
