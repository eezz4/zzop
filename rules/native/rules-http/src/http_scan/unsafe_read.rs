//! `scan_unsafe_read_endpoint` — see the parent `http_scan` module doc for the shared BFS design.

use super::landing::SAFE_METHOD_MOVE_LANDING;
use zzop_core::callgraph::{bfs_reachable_in, Adjacency, SymbolGraph};
use zzop_core::{
    disable_hint, ApiEndpoint, Finding, NonIdempotentKind, Severity, SourceSymbol, WriteSite,
};

use super::{
    build_name_index, is_whitelisted, resolve_handler, with_ok_marker_near_miss,
    write_site_sightline, SAFE_METHODS,
};

/// Input for [`scan_unsafe_read_endpoint`].
pub struct ScanUnsafeReadEndpointInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    /// Reads ONE file's full text by rel path, on demand, for the `idempotent-ok` whitelist
    /// lookback only (write-site detection reads `symbol.write_sites`, precomputed at parse time).
    ///
    /// A reader rather than a map of the whole tree, and that is a measurement: see
    /// [`super::scan_marker_window`] for what the map cost. `None` means "could not read", which
    /// suppresses nothing — a file that cannot be read cannot be shown to carry a marker.
    pub read_file: &'a dyn Fn(&str) -> Option<String>,
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
    let symbols_by_id = super::symbols_by_id(input.symbols);

    let site_at = |id: &str| -> Option<WriteSite> {
        symbols_by_id
            .get(id)
            .and_then(|s| first_unsafe_write_site(s))
            .cloned()
    };

    // ONE adjacency index for the whole loop. Building it inside `bfs_reachable` — which is where it
    // used to live — made this rule and its sibling **91% of a 22.9s run on a 3,000-route tree while
    // reporting nothing**, because the index is O(edges) and the loop is O(routes). See `Adjacency`
    // (review ledger V112).
    let adjacency = Adjacency::build(input.symbol_graph);
    let mut out = Vec::new();
    for e in reads {
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue; // unresolved handler — do not guess
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.read_file) {
            continue;
        }
        let Some((write_id, depth)) =
            bfs_reachable_in(&adjacency, &handler_symbol, |id| site_at(id).is_some())
        else {
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
        // Near-miss disclosure is appended AFTER the rule's own message so `data.hint` and `message`
        // stay the same string (both are `hint` below).
        let hint = with_ok_marker_near_miss(
            format!(
            "{where_} — GET/HEAD must be safe & idempotent. If the write is deliberate and safe to \
             repeat (e.g. a fire-and-forget audit log), this is a false positive here: mark it with \
             `// idempotent-ok: <reason>` on the {} and nothing else has to change. \
             {SAFE_METHOD_MOVE_LANDING} IF IT IS NOT: move the write behind a mutating method \
             (POST/PUT/PATCH/DELETE), or make this endpoint genuinely read-only. Or disable {} if this \
             applies more broadly. {sightline}",
                super::marker_window_phrase(),
            // `disable_hint` always starts with "Disable " — this site already supplies "disable"
            // mid-sentence (after "or"), so only the "via config ..." remainder is spliced in, same
            // technique `rules-schema/src/message.rs`'s `disable_hint_tail` uses.
                disable_hint("unsafe-read-endpoint")
                    .strip_prefix("Disable ")
                    .expect("disable_hint always starts with \"Disable \""),
                sightline = write_site_sightline()
            ),
            &handler_symbol,
            input.symbols,
            input.read_file,
        );
        out.push(Finding {
            rule_id: "unsafe-read-endpoint".to_string(),
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
                "writeSymbol": write_id,
                "writeFile": site.file,
                "writeLine": site.line,
                "sink": site.sink,
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
