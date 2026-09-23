//! `scan_non_idempotent_write` — see the parent `http_scan` module doc for the shared BFS design.

use super::landing::UNIQUE_ENFORCEMENT_LANDING;
use zzop_core::callgraph::{bfs_reachable_in, Adjacency, SymbolGraph};
use zzop_core::{
    disable_hint, ApiEndpoint, Finding, NonIdempotentKind, Severity, SourceSymbol, WriteSite,
};

use super::{
    build_name_index, is_whitelisted, resolve_handler, with_ok_marker_near_miss,
    write_site_sightline, WRITE_HTTP_METHODS,
};

/// Input for [`scan_non_idempotent_write`].
pub struct ScanNonIdempotentWriteInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    /// Reads ONE file's full text by rel path, on demand, for the `idempotent-ok` whitelist
    /// lookback only (see [`ScanUnsafeReadEndpointInput::read_file`]'s doc).
    pub read_file: &'a dyn Fn(&str) -> Option<String>,
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
    let symbols_by_id = super::symbols_by_id(input.symbols);

    // Only classified sites (`kind` set) are relevant here — mirrors the old `symbol_bad_sites`, which
    // never emitted an unclassified write.
    let sites_at = |id: &str| -> Vec<&WriteSite> {
        symbols_by_id
            .get(id)
            .map(|s| s.write_sites.iter().filter(|w| w.kind.is_some()).collect())
            .unwrap_or_default()
    };

    // ONE adjacency index for the whole loop — same measurement as its sibling `unsafe_read`, and
    // `Adjacency`'s own doc for why building it per call was quadratic (review ledger V112).
    let adjacency = Adjacency::build(input.symbol_graph);
    let mut out = Vec::new();
    for e in writes {
        let method = e.method.to_uppercase();
        let allowed = flaggable_kinds(&method);
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue;
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.read_file) {
            continue;
        }
        let Some((id, depth)) = bfs_reachable_in(&adjacency, &handler_symbol, |id| {
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

        // Near-miss disclosure is appended AFTER the rule's own message so `data.hint` and `message`
        // stay the same string (both are `hint` below).
        let hint = with_ok_marker_near_miss(
            hint_for(&method, &e.path, &site, depth),
            &handler_symbol,
            input.symbols,
            input.read_file,
        );
        out.push(Finding {
            rule_id: "non-idempotent-write".to_string(),
            severity: Severity::Warning,
            file: site.file.clone(),
            line: site.line,
            message: hint.clone(),
            evidence_paths: Vec::new(),
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
                // 🔴 NO `hint` KEY HERE, and its absence is the repair. This rule used to emit
                // `"hint": hint` beside `message: hint.clone()` — the SAME string twice in one finding.
                // Harmless while both were inline; expensive once the prose fold landed, because the fold
                // shrinks `message` to a pointer and `data.hint` kept shipping the full text per finding,
                // cancelling the saving exactly. 📏 Measured 2026-09-13 (ledger V231) on
                // `analyze corpus/frameworks/fastapi --limit 1000`: `data.hint` was 624,774 of the reply's
                // 1,167,742 bytes (60%), and 117 of 117 hints were byte-identical to their own finding's
                // message. The text is not lost — it is in `message`, which is the field that carries it.
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
        "{method} {path} reaches {} {where_} ({}) — {why}; {contract}. If a retry is genuinely safe \
         here, this is a false positive: mark it with `// idempotent-ok: <reason>` on the {} and \
         nothing else has to change. {UNIQUE_ENFORCEMENT_LANDING} IF A RETRY IS NOT SAFE: add an \
         idempotency key, or a dedup check the DATABASE enforces, before the write. {} if this applies \
         more broadly. {}",
        site.sink,
        kind.as_str(),
        super::marker_window_phrase(),
        disable_hint("non-idempotent-write"),
        write_site_sightline()
    )
}
