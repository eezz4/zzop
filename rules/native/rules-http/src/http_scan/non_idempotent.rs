//! `scan_non_idempotent_write` — see the parent `http_scan` module doc for the shared BFS design.

use std::collections::HashMap;

use zzop_core::callgraph::{bfs_reachable, SymbolGraph};
use zzop_core::{
    disable_hint, ApiEndpoint, Finding, NonIdempotentKind, Severity, SourceSymbol, WriteSite,
};

use super::{build_name_index, is_whitelisted, resolve_handler, WRITE_HTTP_METHODS};

/// Input for [`scan_non_idempotent_write`].
pub struct ScanNonIdempotentWriteInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    /// rel path -> full source text, for the `idempotent-ok` whitelist lookback only (see
    /// [`ScanUnsafeReadEndpointInput::files`]'s doc).
    pub files: &'a HashMap<String, String>,
}

/// Which finding kinds apply to a method: `create` only matters for PUT/DELETE (idempotency-promising); POST/PATCH are flagged only for accumulation.
fn flaggable_kinds(method: &str) -> &'static [NonIdempotentKind] {
    if method == "PUT" || method == "DELETE" {
        &[
            NonIdempotentKind::Create,
            NonIdempotentKind::AtomicAccumulate,
            NonIdempotentKind::Counter,
        ]
    } else {
        &[
            NonIdempotentKind::AtomicAccumulate,
            NonIdempotentKind::Counter,
        ]
    }
}

/// Flags a write handler that reaches a non-idempotent operation: `create`/`insert` on PUT/DELETE (a retry
/// duplicates), or an atomic accumulation/counter bump on any write method (a retry doubles the effect).
pub fn scan_non_idempotent_write(input: &ScanNonIdempotentWriteInput) -> Vec<Finding> {
    let writes: Vec<&ApiEndpoint> = input
        .api_endpoints
        .iter()
        .filter(|e| WRITE_HTTP_METHODS.contains(&e.method.to_uppercase().as_str()))
        .collect();
    if writes.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let symbols_by_id: HashMap<&str, &SourceSymbol> =
        input.symbols.iter().map(|s| (s.id.as_str(), s)).collect();

    // Only classified sites (`kind` set) are relevant here — mirrors the old `symbol_bad_sites`, which
    // never emitted an unclassified write.
    let sites_at = |id: &str| -> Vec<&WriteSite> {
        symbols_by_id
            .get(id)
            .map(|s| s.write_sites.iter().filter(|w| w.kind.is_some()).collect())
            .unwrap_or_default()
    };

    let mut out = Vec::new();
    for e in writes {
        let method = e.method.to_uppercase();
        let allowed = flaggable_kinds(&method);
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue;
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.files) {
            continue;
        }
        let Some((id, depth)) = bfs_reachable(input.symbol_graph, &handler_symbol, |id| {
            sites_at(id)
                .iter()
                .any(|s| allowed.contains(&s.kind.expect("filtered to Some above")))
        }) else {
            continue;
        };
        let site = sites_at(&id)
            .into_iter()
            .find(|s| allowed.contains(&s.kind.expect("filtered to Some above")))
            .cloned()
            .expect("predicate true implies a matching site exists");

        let hint = hint_for(&method, &e.path, &site, depth);
        out.push(Finding {
            rule_id: "non-idempotent-write".to_string(),
            severity: Severity::Warning,
            file: site.file.clone(),
            line: site.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "method": method,
                "path": e.path,
                "handler": e.handler,
                "handlerSymbol": handler_symbol,
                "writeSymbol": id,
                "writeFile": site.file,
                "writeLine": site.line,
                "sink": site.sink,
                "kind": site.kind.expect("filtered to Some above").as_str(),
                "depth": depth,
                "hint": hint,
            })),
        });
    }
    out
}

fn hint_for(method: &str, path: &str, site: &WriteSite, depth: u32) -> String {
    let where_ = if depth == 0 {
        "directly".to_string()
    } else {
        format!("{depth} call(s) deep")
    };
    let kind = site
        .kind
        .expect("hint_for is only called with a classified site");
    let why = match kind {
        NonIdempotentKind::Create => "a retry inserts a duplicate row",
        NonIdempotentKind::AtomicAccumulate => {
            "a retry applies the increment again (doubles the effect)"
        }
        NonIdempotentKind::Counter => "a retry bumps the counter again",
    };
    let contract = if method == "PUT" || method == "DELETE" {
        format!("{method} must be idempotent")
    } else {
        format!("a retried {method} must converge or carry an idempotency key")
    };
    format!(
        "{method} {path} reaches {} {where_} ({}) — {why}; {contract}. Add an idempotency key or a \
         dedup/uniqueness check before the write, or mark it with `// idempotent-ok: <reason>` above the \
         handler if a retry is genuinely safe here. {} if this applies more broadly.",
        site.sink,
        kind.as_str(),
        disable_hint("non-idempotent-write")
    )
}
