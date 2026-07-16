//! The call-graph-BFS HTTP native rules — see `run_callgraph_rules`'s doc for the engine-wiring route
//! (a second, uncached TS re-parse off disk).

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use zzop_core::{is_enabled, Finding, ImportMap};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

/// Runs the three call-graph-BFS native rules — `zzop-rules-http`'s `scan_unsafe_read_endpoint` /
/// `scan_non_idempotent_write` / `scan_mutating_route_no_auth` — and extends `global_findings` in place.
/// Gated behind `is_enabled` per rule id and behind having at least one reconstructed `ApiEndpoint`, so
/// a tree with no HTTP routes never pays the cost below.
///
/// ## Engine-wiring route taken
/// `FileArtifact` carries no `RawCall`s — the fused pass's contract is "parse once, project, drop the
/// AST", and `SourceSymbol`/`ImportMap` alone do not encode call sites. Rather than widen that contract,
/// this function runs a **second, uncached pass**: it re-reads every already-dispatched TypeScript
/// file's text off disk (`ts_paths`) and re-parses it with `zzop_parser_typescript::parse_calls`. This
/// never consults `zzop_cache::AnalysisCache` — a full per-file cache hit still re-reads and re-parses
/// every TS file here whenever either rule is enabled and at least one HTTP endpoint exists.
///
/// `api_endpoints` is reconstructed from the per-file `IoProvide` facts already collected (`kind ==
/// "http"`) rather than a third route-extraction pass — `IoProvide::key` is the normalized
/// `http_interface_key(method, path)` form (path params collapsed to `{}`), so a finding's displayed
/// `path` is that normalized form, not the endpoint's literal source text. This only affects display;
/// BFS correctness never depends on exact path spelling.
#[allow(clippy::too_many_arguments)]
pub(in crate::analyze) fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
    attribute_store: &zzop_core::AttributeStore,
    io_provides: &[zzop_core::IoProvide],
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    all_symbols: &[zzop_core::SourceSymbol],
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    let api_endpoints: Vec<zzop_core::ApiEndpoint> = io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter_map(|p| {
            let (method, path) = p.key.split_once(' ')?;
            Some(zzop_core::ApiEndpoint {
                method: method.to_string(),
                path: path.to_string(),
                handler: p.symbol.clone().unwrap_or_default(),
            })
        })
        .collect();
    if api_endpoints.is_empty() {
        return;
    }

    let run_unsafe_read = is_enabled(&config.rule_config, "unsafe-read-endpoint");
    let run_non_idempotent = is_enabled(&config.rule_config, "non-idempotent-write");
    let run_mutating_no_auth = is_enabled(&config.rule_config, "mutating-route-no-auth");
    if !run_unsafe_read && !run_non_idempotent && !run_mutating_no_auth {
        return;
    }

    let mut raw_calls = Vec::new();
    let mut file_texts: HashMap<String, String> = HashMap::new();
    for rel in ts_paths {
        if !crate::dead_exports::is_ts_source_ext(rel) {
            continue; // non-TS overlay participant (e.g. .svelte) — re-parsing as TS would inject noise
        }
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zzop_parser_typescript::parse_calls(rel, &text));
            file_texts.insert(rel.clone(), text);
        }
    }
    let imports_by_file: HashMap<String, ImportMap> = ts_import_pairs.iter().cloned().collect();
    let mut local_symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for s in all_symbols {
        local_symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .insert(s.name.clone());
    }
    let resolve_file_fn = |specifier: &str, from_file: &str| {
        zzop_parser_typescript::resolve_file(specifier, from_file, ts_paths)
    };
    let symbol_graph = zzop_core::callgraph::build_symbol_graph(
        &raw_calls,
        &imports_by_file,
        &local_symbols_by_file,
        &resolve_file_fn,
    );
    if run_unsafe_read {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_unsafe_read_endpoint(
            &zzop_rules_http::ScanUnsafeReadEndpointInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
            },
        );
        record_native_timing(rule_time, t0, "unsafe-read-endpoint", found.len());
        global_findings.extend(found);
    }
    if run_non_idempotent {
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_non_idempotent_write(
            &zzop_rules_http::ScanNonIdempotentWriteInput {
                api_endpoints: &api_endpoints,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                files: &file_texts,
            },
        );
        record_native_timing(rule_time, t0, "non-idempotent-write", found.len());
        global_findings.extend(found);
    }
    if run_mutating_no_auth {
        // Reuses the same `symbol_graph` built above but reads `io_provides` directly rather than
        // `api_endpoints`, since the `Finding` anchors on the route's own registration `file`/`line`,
        // which `ApiEndpoint` cannot carry.
        //
        // `nest_guarded`: NestJS `@UseGuards(...)` decorator coverage, computed from the same
        // `file_texts` already read off disk — no extra file I/O. The BFS needs this side-channel
        // because a decorator application is metadata, not a call edge, so it's invisible to
        // `bfs_reachable`.
        let nest_guarded: std::collections::HashSet<(String, u32)> = file_texts
            .iter()
            .flat_map(|(rel, text)| {
                zzop_parser_typescript::extract_controller_guarded_lines(rel, text)
                    .into_iter()
                    .map(move |line| (rel.clone(), line))
            })
            .collect();
        // Generic entity-attribute channel — injected auth-guard evidence for routes the call-graph BFS
        // can't see (middleware). Built once by `analyze::assemble` from every Mode-B adapter overlay's
        // `attributes` and threaded in (shared with `schema_usage_findings`). Empty unless an adapter
        // injects; then old behavior.
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_mutating_route_no_auth(
            &zzop_rules_http::ScanMutatingRouteNoAuthInput {
                io_provides,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                auth_guard_pattern: zzop_rules_http::DEFAULT_AUTH_GUARD_PATTERN,
                nest_guarded: &nest_guarded,
                route_attr_store: attribute_store,
            },
        );
        record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
        global_findings.extend(found);
    }
}
