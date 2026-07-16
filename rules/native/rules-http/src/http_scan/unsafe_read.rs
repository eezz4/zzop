//! `scan_unsafe_read_endpoint` — see the parent `http_scan` module doc for the shared BFS design.

use std::collections::HashMap;

use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{
    disable_hint, ApiEndpoint, Finding, NonIdempotentKind, Severity, SourceSymbol, WriteSite,
};

use super::{build_name_index, is_whitelisted, resolve_handler, SAFE_METHODS};

/// Input for [`scan_unsafe_read_endpoint`].
pub struct ScanUnsafeReadEndpointInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    /// rel path -> full source text, for the `idempotent-ok` whitelist lookback only (write-site
    /// detection reads `symbol.write_sites`, precomputed at parse time — see the module doc).
    pub files: &'a HashMap<String, String>,
}

/// The first (lowest-position) write site in `sym.write_sites` that counts as "any write" for this rule —
/// every kind qualifies EXCEPT a pure counter-bump (`Counter`), since the vocabulary
/// `unsafe-read-endpoint` always used (`create`/`createMany`/`update`/`updateMany`/`delete`/`deleteMany`/
/// `upsert`/`insert`/`save`/`remove`) never included the counter vocabulary
/// (`incr`/`incrby`/`decr`/`decrby`) that `non-idempotent-write` also inspects — see
/// `zzop_parser_typescript::write_site`'s module doc for why this reproduces the old two-scan split
/// exactly now that both rules share one `write_sites` list.
fn first_unsafe_write_site(sym: &SourceSymbol) -> Option<&WriteSite> {
    sym.write_sites
        .iter()
        .find(|w| w.kind != Some(NonIdempotentKind::Counter))
}

/// Flags a "safe" method endpoint (GET/HEAD) whose handler reaches a database write — per RFC 7231, GET/HEAD
/// must be safe and idempotent, so a mutating read is a crawler/prefetch/retry hazard.
pub fn scan_unsafe_read_endpoint(input: &ScanUnsafeReadEndpointInput) -> Vec<Finding> {
    let reads: Vec<&ApiEndpoint> = input
        .api_endpoints
        .iter()
        .filter(|e| SAFE_METHODS.contains(&e.method.to_uppercase().as_str()))
        .collect();
    if reads.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let symbols_by_id: HashMap<&str, &SourceSymbol> =
        input.symbols.iter().map(|s| (s.id.as_str(), s)).collect();

    let site_at = |id: &str| -> Option<WriteSite> {
        symbols_by_id
            .get(id)
            .and_then(|s| first_unsafe_write_site(s))
            .cloned()
    };

    let mut out = Vec::new();
    for e in reads {
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue; // unresolved handler — do not guess
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.files) {
            continue;
        }
        let Some((write_id, depth)) = bfs_reachable(input.symbol_graph, &handler_symbol, |id| {
            site_at(id).is_some()
        }) else {
            continue;
        };
        let site = site_at(&write_id).expect("predicate true implies a site exists");
        let method = e.method.to_uppercase();
        let where_ = if depth == 0 {
            format!("{method} {} writes directly ({})", e.path, site.sink)
        } else {
            format!(
                "{method} {} reaches a write ({}) {depth} call(s) deep",
                e.path, site.sink
            )
        };
        let hint = format!(
            "{where_} — GET/HEAD must be safe & idempotent. Move the write behind a mutating method \
             (POST/PUT/PATCH/DELETE), or make this endpoint genuinely read-only. If the write is \
             deliberate and safe to repeat (e.g. a fire-and-forget audit log), mark it with \
             `// idempotent-ok: <reason>` on the line above the handler, or disable {} if this applies \
             more broadly.",
            // `disable_hint` always starts with "Disable " — this site already supplies "disable"
            // mid-sentence (after "or"), so only the "via config ..." remainder is spliced in, same
            // technique `rules-schema/src/message.rs`'s `disable_hint_tail` uses.
            disable_hint("unsafe-read-endpoint")
                .strip_prefix("Disable ")
                .expect("disable_hint always starts with \"Disable \"")
        );
        out.push(Finding {
            rule_id: "unsafe-read-endpoint".to_string(),
            severity: Severity::Warning,
            file: site.file.clone(),
            line: site.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "method": method,
                "path": e.path,
                "handler": e.handler,
                "handlerSymbol": handler_symbol,
                "writeSymbol": write_id,
                "writeFile": site.file,
                "writeLine": site.line,
                "sink": site.sink,
                "depth": depth,
                "hint": hint,
            })),
        });
    }
    out
}
