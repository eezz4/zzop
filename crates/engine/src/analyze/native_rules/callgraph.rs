//! The call-graph-BFS HTTP native rules — see `run_callgraph_rules`'s doc for the engine-wiring route
//! (a second, uncached TS/Java re-parse off disk).

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
/// file's text off disk (`ts_paths`) and re-parses it with `zzop_parser_typescript::parse_calls`, AND —
/// symmetrically — every already-dispatched Java file's text off disk (`java_rels`), re-parsed with
/// `zzop_parser_java_21::parse_calls`/`parse_imports` (this is the "lift the exemption" wiring
/// `rules-http`'s `mutating_route_no_auth` module doc names as the completion of its own "Call-graph
/// language coverage" gap — see that doc). Neither re-parse ever consults `zzop_cache::AnalysisCache` — a
/// full per-file cache hit still re-reads and re-parses every TS/Java file here whenever any of the three
/// call-graph-BFS rules is enabled and at least one HTTP endpoint exists.
///
/// Java's imports are ALSO re-parsed fresh here (unlike TS's, which arrive pre-computed via
/// `ts_import_pairs` from the fused per-file pass) — no `java_import_pairs` equivalent is threaded into
/// this function, so re-parsing both calls and imports together keeps the Java side self-contained
/// rather than growing the caller's parameter list for a fact only this function needs.
///
/// `api_endpoints` is reconstructed from the per-file `IoProvide` facts already collected (`kind ==
/// "http"`) rather than a third route-extraction pass — `IoProvide::key` is the normalized
/// `http_interface_key(method, path)` form (path params collapsed to `{}`), so a finding's displayed
/// `path` is that normalized form, not the endpoint's literal source text. This only affects display;
/// BFS correctness never depends on exact path spelling.
///
/// ## Java call resolution: an opaque-specifier `resolve_file`, not real package resolution
/// The combined `resolve_file_fn` below dispatches on the CALLING file's own extension: a TS `from_file`
/// keeps using the real `zzop_parser_typescript::resolve_file` (relative-specifier, `ts_paths`-aware); a
/// Java `from_file` always resolves a specifier to itself (`Some(specifier.to_string())`) — Java import
/// specifiers are dotted package/class names (`io.spring.core.service.AuthorizationService`), not
/// relative paths, and no whole-corpus Java package/type index (`pipeline::JavaIndex`, used elsewhere for
/// the dep-graph) is threaded into this function. Treating the specifier as its own opaque, stable target
/// identity is sufficient for THIS graph's purpose — `bfs_reachable`'s predicate only needs a stable node
/// id to visit and vocabulary-match (`mutating_route_no_auth::is_guard_id`), not a real cross-file
/// resolution to another parsed Java file's own outgoing edges. Known limitation: a guard reachable only
/// through a SECOND hop through Java code (handler -> helper method in another Java file -> guard) won't
/// be found, since the first hop's target id is this opaque specifier string, not a real symbol id
/// anything else in the graph has outgoing edges from — single-hop (handler directly calls the guard, or
/// a same-file helper) is the coverage this wiring buys.
#[allow(clippy::too_many_arguments)]
pub(in crate::analyze) fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
    attribute_store: &zzop_core::AttributeStore,
    io_provides: &[zzop_core::IoProvide],
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    java_rels: &[String],
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
    let mut imports_by_file: HashMap<String, ImportMap> = ts_import_pairs.iter().cloned().collect();
    // Java's own re-parse — module doc "Engine-wiring route taken". Deliberately NOT folded into
    // `file_texts`: neither `unsafe-read-endpoint`/`non-idempotent-write`'s `is_whitelisted` lookback nor
    // `mutating-route-no-auth`'s NestJS `@UseGuards` decorator scan (`extract_controller_guarded_lines`,
    // below) has any Java-shaped signal to find, so adding Java text there would only be unread bytes.
    for rel in java_rels {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zzop_parser_java_21::parse_calls(rel, &text));
            imports_by_file.insert(rel.clone(), zzop_parser_java_21::parse_imports(&text));
        }
    }
    let mut local_symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for s in all_symbols {
        local_symbols_by_file
            .entry(s.file.clone())
            .or_default()
            .insert(s.name.clone());
    }
    // Combined resolver, dispatched by the CALLING file's own extension — module doc "Java call
    // resolution".
    let resolve_file_fn = |specifier: &str, from_file: &str| {
        if from_file.ends_with(".java") {
            Some(specifier.to_string())
        } else {
            zzop_parser_typescript::resolve_file(specifier, from_file, ts_paths)
        }
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
