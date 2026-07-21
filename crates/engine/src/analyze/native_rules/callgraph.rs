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
            if run_mutating_no_auth {
                for line in zzop_parser_java_21::extract_spring_guarded_lines(rel, &text) {
                    java_decorator_guarded.insert((rel.clone(), line));
                }
                if let Some(p) = zzop_parser_java_21::extract_spring_security_posture(rel, &text) {
                    spring_postures.push((rel.clone(), p));
                }
            }
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
        // `decorator_guarded`: framework-neutral decorator/annotation auth coverage the call-graph BFS
        // can't see (a decorator/annotation application is metadata, not a call edge). Two producers feed
        // the one `(file, line)` set: NestJS `@UseGuards(...)` from the TS `file_texts` already read off
        // disk (no extra I/O), and Spring method-security annotations gathered above into
        // `java_decorator_guarded`. Both key routes by the same `(file, line)` the provide anchors on.
        let mut decorator_guarded = java_decorator_guarded;
        for (rel, text) in &file_texts {
            for line in zzop_parser_typescript::extract_controller_guarded_lines(rel, text) {
                decorator_guarded.insert((rel.clone(), line));
            }
        }
        // NestJS route-scoped auth middleware: `consumer.apply(AuthX).forRoutes({path, method})` in a
        // module names its covered routes by (method, path) PATTERN, not a (file, line). Match each
        // pattern against the actual route provides and exempt every match by its own registration line.
        let forroutes: Vec<zzop_parser_typescript::ForRoutesPattern> = file_texts
            .iter()
            .flat_map(|(rel, text)| {
                zzop_parser_typescript::extract_nest_forroutes_guarded(rel, text)
            })
            .collect();
        if !forroutes.is_empty() {
            // The app's NestJS global prefix (`app.setGlobalPrefix('api')`), if any — a controller route
            // provide's key already carries it (applied at assembly) but a forRoutes `path` is written
            // WITHOUT it, so exact matching needs to prepend it. A non-literal / absent prefix leaves it
            // `None` (exact match against the unprefixed pattern) — a miss then only fails to exempt.
            let global_prefix: Option<String> = file_texts
                .iter()
                .find_map(|(rel, text)| {
                    zzop_parser_typescript::extract_global_prefix_marker(rel, text)
                })
                .map(|p| p.key);
            for p in io_provides.iter().filter(|p| p.kind == "http") {
                let Some((method, path)) = p.key.split_once(' ') else {
                    continue;
                };
                let covered = forroutes.iter().any(|(m, pat)| {
                    (m == "*" || m == method)
                        && forroutes_path_matches(path, pat, global_prefix.as_deref())
                });
                if covered {
                    decorator_guarded.insert((p.file.clone(), p.line));
                }
            }
        }
        // Spring Security global posture — a secure-by-default chain governs its app's Java routes: one is
        // authenticated (exempt) iff it escapes every `.permitAll()` matcher. Applied only when EXACTLY one
        // posture exists tree-wide (else config-vs-config scoping is ambiguous), and SCOPED to the config's
        // own source root (`spring_app_root`) so it never false-clears a sibling module's open routes.
        if let [(config_file, posture)] = spring_postures.as_slice() {
            let app_root = spring_app_root(config_file);
            for p in io_provides.iter().filter(|p| {
                p.kind == "http" && p.file.ends_with(".java") && p.file.starts_with(app_root)
            }) {
                let Some((method, path)) = p.key.split_once(' ') else {
                    continue;
                };
                if posture.route_is_authenticated(method, path) {
                    decorator_guarded.insert((p.file.clone(), p.line));
                }
            }
        }
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
                decorator_guarded: &decorator_guarded,
                route_attr_store: attribute_store,
            },
        );
        record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
        global_findings.extend(found);
    }
}

/// Whether a route provide's path (leading-slash, `http_interface_key`-normalized, and already carrying
/// the app's NestJS global prefix if one exists — `/api/articles/{}/comments`) is EXACTLY the route a
/// NestJS `forRoutes` PATTERN covers (controller-relative, no leading slash, no global prefix —
/// `articles/{}/comments`). The pattern is reconciled to the provide's key space by prepending
/// `global_prefix` (when a literal one was found) and comparing for EQUALITY — not a suffix match, which
/// would over-clear (a `{path:'articles'}` pattern must not exempt an unrelated `/api/admin/articles`
/// route in another module). Both sides already share the `{}` param normalization. When `global_prefix`
/// is `None` (no `setGlobalPrefix`, or a non-literal one that can't be read), the pattern is matched
/// unprefixed; if the app truly has a prefix we failed to read, the exemption is simply MISSED (the
/// finding stays) — never an over-clear, the safe direction for a security rule.
fn forroutes_path_matches(provide_path: &str, pattern: &str, global_prefix: Option<&str>) -> bool {
    let pat = pattern.trim_start_matches('/');
    let expected = match global_prefix {
        Some(p) if !p.trim_matches('/').is_empty() => format!("/{}/{}", p.trim_matches('/'), pat),
        _ => format!("/{pat}"),
    };
    provide_path == expected
}

/// The Java source-root prefix a Spring security config governs — everything up to and including the first
/// `src/main/java/` segment (the Maven/Gradle convention), so a posture only exempts routes in its OWN
/// module. A monorepo module lives at `<module>/src/main/java/...`, so `service-a`'s config yields prefix
/// `service-a/src/main/java/` and can never match `service-b/src/main/java/...`. When the config isn't
/// under a recognizable source root (unusual layout), falls back to the config file's own directory — the
/// most conservative scope (only same-directory routes), never the whole tree.
fn spring_app_root(config_file: &str) -> &str {
    const SRC_ROOT: &str = "src/main/java/";
    if let Some(idx) = config_file.find(SRC_ROOT) {
        &config_file[..idx + SRC_ROOT.len()]
    } else {
        match config_file.rfind('/') {
            Some(i) => &config_file[..=i],
            None => "",
        }
    }
}
