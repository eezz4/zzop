//! The call-graph-BFS HTTP native rules — see `run_callgraph_rules`'s doc for the engine-wiring route
//! (a second, uncached TS/Java/Python re-parse off disk).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use zzop_core::{is_enabled, Finding, ImportMap};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

mod cache_lane;
mod decorator_gate;
mod python_guard;
mod rust_guard;

use decorator_gate::{assemble_decorator_guarded, packs_read_io_scan_attrs};

/// Runs the three call-graph-BFS native rules — `zzop-rules-http`'s `scan_unsafe_read_endpoint` /
/// `scan_non_idempotent_write` / `scan_mutating_route_no_auth` — and extends `global_findings` in place.
/// Gated behind `is_enabled` per rule id and behind having at least one reconstructed `ApiEndpoint`, so
/// a tree with no HTTP routes never pays the cost below (`decorator_guarded_out` too stays empty then).
///
/// ## Engine-wiring route taken
/// `FileArtifact` carries no `RawCall`s — the fused pass's contract is "parse once, project, drop the
/// AST", and `SourceSymbol`/`ImportMap` alone do not encode call sites. Rather than widen that contract,
/// this function runs a **second, uncached pass** over three languages: every already-dispatched
/// TypeScript file (`ts_paths`, `zzop_parser_typescript::parse_calls`), every Java file (`java_rels`,
/// `zzop_parser_java_21::parse_calls`/`parse_imports`), and every Python file (`python_guard::
/// parse_calls_and_guards`, which owns that loop and its own resolution/guard docs). The two non-TS loops
/// are the "lift the exemption" wiring `rules-http`'s `mutating_route_no_auth` module doc names as the
/// completion of its own "Call-graph language coverage" gap. No re-parse ever consults
/// `zzop_cache::AnalysisCache` — a full per-file cache hit still re-reads and re-parses every one of
/// those files whenever any call-graph-BFS rule is enabled and at least one HTTP endpoint exists.
///
/// Java's imports are ALSO re-parsed fresh here (unlike TS's and Python's, which arrive pre-computed via
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
/// ## Call resolution is per-language: the combined `resolve_file_fn` dispatches on the CALLING file
/// A TS `from_file` keeps the real `zzop_parser_typescript::resolve_file` (relative-specifier,
/// `ts_paths`-aware); a PYTHON one uses the real module resolver (`python_guard::
/// resolve_python_call_target`); a JAVA one resolves a specifier to ITSELF (`Some(specifier.to_string())`)
/// — Java import specifiers are dotted package/class names (`io.spring.core.service.AuthorizationService`),
/// not relative paths, and no whole-corpus Java package/type index (`pipeline::JavaIndex`, used elsewhere
/// for the dep-graph) is threaded into this function. Treating the specifier as its own opaque, stable
/// target identity is sufficient for THIS graph's purpose — `bfs_reachable`'s predicate only needs a
/// stable node id to visit and vocabulary-match (`mutating_route_no_auth::is_guard_id`), not a real
/// cross-file resolution. Known limitation, shared with Python's own module-attribute case: a guard
/// reachable only through a SECOND hop (handler -> helper in another file -> guard) is not found, since
/// the first hop's target id is a node nothing else has outgoing edges from — single-hop (handler calls
/// the guard directly, or through a same-file helper) is the coverage this wiring buys.
#[allow(clippy::too_many_arguments)]
pub(in crate::analyze) fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
    attribute_store: &zzop_core::AttributeStore,
    io_provides: &[zzop_core::IoProvide],
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    java_rels: &[String],
    rust_workspace: &crate::pipeline::RustWorkspaceMap,
    all_symbols: &[zzop_core::SourceSymbol],
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
    decorator_guarded_out: &mut BTreeSet<(String, u32)>,
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

    // Convention vocabulary for this run: what the CONFIG declared, each key falling back to its built-in
    // (`VocabularyConfig::resolve`). Resolved once here rather than at each use so every consumer in this
    // pass reads the same answer.
    let vocab = config.vocabulary.resolve();
    // `cache-lane-file-read` is the first consumer of this pass that has NOTHING to do with HTTP, which is
    // why the "no endpoints, nothing to do" early return moved from the top of this function to here and
    // became conditional. Leaving it above would have made the rule structurally unreachable on exactly
    // the trees it is for — a library or a compiler with no routes at all — and it would have failed
    // SILENTLY, which is the defect class this repo spends the most effort on. Its own two vocabularies
    // gate it further inside the rule; this only decides whether the pass runs at all.
    let run_cache_lane = is_enabled(&config.rule_config, "cache-lane-file-read")
        && vocab.cache_lane_anchor_pattern.is_some();
    if api_endpoints.is_empty() && !run_cache_lane {
        return;
    }
    let run_unsafe_read = is_enabled(&config.rule_config, "unsafe-read-endpoint");
    let run_non_idempotent = is_enabled(&config.rule_config, "non-idempotent-write");
    let run_mutating_no_auth = is_enabled(&config.rule_config, "mutating-route-no-auth");
    // Decoupled from `run_mutating_no_auth` alone (A2 of the IoScan projection redesign — resolves the
    // COUPLING CAVEAT documented in `assemble/rules/io_scan.rs`): `decorator_guarded` evidence is produced
    // whenever EITHER consumer needs it — the native `mutating-route-no-auth` rule is enabled, OR some
    // loaded+enabled pack's `IoScan` rule reads it via `attr_present`/`attr_absent`
    // (`packs_read_io_scan_attrs`). Otherwise, disabling the native rule would silently empty the minted
    // `auth-guarded` attribute and false-positive every decorator-guarded route under such a pack. The
    // native rule's OWN gating — whether `scan_mutating_route_no_auth` itself runs, below — stays exactly
    // `run_mutating_no_auth`, unchanged.
    //
    // Cost note (scouted, then corrected by review): WITHIN an invocation every decorator-guard producer
    // below reads text already in memory — Java's `extract_spring_guarded_lines`/
    // `extract_spring_security_posture` re-parse the same `text` string `parse_calls`/`parse_imports`
    // consumed per `java_rels` entry, and the NestJS producers read from `file_texts` — so no producer
    // adds a per-file read on top of the pass. BUT the widened gate also makes the pass RUN in one config
    // it previously skipped outright: every callgraph-family rule off while a DSL pack reads auth attrs.
    // That config used to early-return with zero I/O and now pays this pass's own TS+Java file reads —
    // the honest price of producing evidence that config actually consumes. The union (not an
    // unconditional run) still skips everything when NEITHER consumer is active, via the early-return
    // below.
    let need_decorator_guarded = run_mutating_no_auth || packs_read_io_scan_attrs(config);
    if !run_unsafe_read && !run_non_idempotent && !need_decorator_guarded && !run_cache_lane {
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
    // Java's own re-parse — module doc "Engine-wiring route taken". Java text is NOT folded into
    // `file_texts` (the TS-shaped `is_whitelisted` lookback and `extract_controller_guarded_lines` find
    // nothing Java in it); its ONE `mutating-route-no-auth` signal — Spring method-security annotations —
    // is read HERE into `java_decorator_guarded`, the Java half of the framework-neutral decorator-guard
    // exemption set (the NestJS `@UseGuards` half is built from `file_texts` below).
    let mut java_decorator_guarded: std::collections::HashSet<(String, u32)> =
        std::collections::HashSet::new();
    // Spring Security global authorization postures (secure-by-default `authorizeRequests` chains). One
    // per config file; collected across the tree — applied below ONLY if exactly one exists (multiple =
    // ambiguous scoping, unsafe to reason about, so left unapplied).
    let mut spring_postures: Vec<(String, zzop_parser_java_21::SpringSecurityPosture)> = Vec::new();
    for rel in java_rels {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zzop_parser_java_21::parse_calls(rel, &text));
            imports_by_file.insert(rel.clone(), zzop_parser_java_21::parse_imports(&text));
            if need_decorator_guarded {
                for line in zzop_parser_java_21::extract_spring_guarded_lines(rel, &text) {
                    java_decorator_guarded.insert((rel.clone(), line));
                }
                if let Some(p) = zzop_parser_java_21::extract_spring_security_posture(rel, &text) {
                    spring_postures.push((rel.clone(), p));
                }
            }
        }
    }
    // Python's own re-parse + its two decorator-guard producers — module doc "Engine-wiring route taken",
    // and `python_guard`'s own doc for the two guard shapes and why they are gathered in two phases.
    let python_guards = python_guard::parse_calls_and_guards(
        root,
        ts_paths,
        need_decorator_guarded,
        &vocab.python_guard(),
        &mut raw_calls,
    );
    // Rust's own re-parse — calls PLUS handler-signature extractor evidence. Unconditional (not behind
    // `need_decorator_guarded`) because the signature edges are ordinary graph edges, not side-channel
    // guard evidence: see `rust_guard`'s module doc.
    rust_guard::parse_calls_and_guards(root, ts_paths, &vocab.rust_guard(), &mut raw_calls);
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
        } else if crate::analyze::assemble::helpers::is_python_source_ext(from_file) {
            python_guard::resolve_python_call_target(specifier, from_file, ts_paths)
        } else if crate::analyze::assemble::helpers::is_rust_source_ext(from_file) {
            rust_guard::resolve_rust_call_target(specifier, from_file, ts_paths, rust_workspace)
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
    if run_cache_lane {
        let t0 = profile.then(Instant::now);
        let found = cache_lane::run(
            &raw_calls,
            &symbol_graph,
            all_symbols,
            vocab.cache_lane_anchor_pattern,
            &vocab.file_read_callees,
        );
        record_native_timing(rule_time, t0, "cache-lane-file-read", found.len());
        global_findings.extend(found);
    }
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
    if need_decorator_guarded {
        // Reuses the same `symbol_graph` built above but reads `io_provides` directly rather than
        // `api_endpoints`, since the `Finding` anchors on the route's own registration `file`/`line`,
        // which `ApiEndpoint` cannot carry.
        //
        // `decorator_guarded`: framework-neutral decorator/annotation auth coverage the call-graph BFS
        // can't see (a decorator/annotation application is metadata, not a call edge). Every producer and
        // the load-bearing order they apply in live in `decorator_gate::assemble_decorator_guarded`.
        // This whole block runs whenever EITHER consumer needs it — see `need_decorator_guarded`'s doc —
        // not only when `run_mutating_no_auth` itself is on.
        let decorator_guarded = assemble_decorator_guarded(
            java_decorator_guarded,
            &python_guards,
            &spring_postures,
            &file_texts,
            io_provides,
            all_symbols,
            vocab.java_source_root,
        );
        *decorator_guarded_out = decorator_guarded.iter().cloned().collect();

        if run_mutating_no_auth {
            // Generic entity-attribute channel — injected auth-guard evidence for routes the call-graph
            // BFS can't see (middleware). Built once by `analyze::assemble` from every Mode-B adapter
            // overlay's `attributes` and threaded in (shared with `schema_usage_findings`). Empty unless
            // an adapter injects. The native rule's OWN gating — unchanged by the A2 decoupling above.
            let t0 = profile.then(Instant::now);
            let found = zzop_rules_http::scan_mutating_route_no_auth(
                &zzop_rules_http::ScanMutatingRouteNoAuthInput {
                    io_provides,
                    symbols: all_symbols,
                    symbol_graph: &symbol_graph,
                    auth_guard_pattern: vocab.auth_guard_pattern,
                    qualifier_guard_tokens: &vocab.auth_guard_qualifier_tokens,
                    auth_acquisition_standalone_pattern: vocab.auth_acquisition_standalone_pattern,
                    auth_acquisition_conditional_pattern: vocab
                        .auth_acquisition_conditional_pattern,
                    auth_family_path_pattern: vocab.auth_family_path_pattern,
                    decorator_guarded: &decorator_guarded,
                    route_attr_store: attribute_store,
                },
            );
            record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
            global_findings.extend(found);
        }
    }
}
