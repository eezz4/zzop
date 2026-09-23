//! The call-graph-BFS HTTP native rules — see `run_callgraph_rules`'s doc for the engine-wiring route
//! (a second, uncached TS/Java/Python re-parse off disk).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use zzop_core::{is_enabled, Finding, ImportMap};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

mod cache_lane;
mod decorator_gate;
mod graph_build;
mod java_bridge;
mod java_guard;
mod marker_lookback_rules;
mod python_bridge;
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
/// this function runs a **second, uncached pass** over four languages: every already-dispatched
/// TypeScript file (`ts_paths`, `zzop_parser_typescript::parse_calls`) inline below, and Java, Python and
/// Rust each in a sibling module that owns its own loop and its own resolution/guard docs
/// (`java_guard`/`python_guard`/`rust_guard`, all `parse_calls_and_guards`). The three non-TS loops are
/// the "lift the exemption" wiring `rules-http`'s `mutating_route_no_auth` module doc names as the
/// completion of its own "Call-graph language coverage" gap. No re-parse ever consults
/// `zzop_cache::AnalysisCache` — a full per-file cache hit still re-reads and re-parses every one of
/// those files whenever any call-graph-BFS rule is enabled and at least one HTTP endpoint exists.
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
/// not relative paths, and one specifier can name MANY files (`com.ex.svc.*`), which this callback's
/// one-target shape cannot express. Treating the specifier as its own opaque, stable target identity is
/// what this callback can honestly return — `bfs_reachable_in`'s predicate only needs a stable node id to
/// visit and vocabulary-match (`mutating_route_no_auth::is_guard_id`).
///
/// 🔴 **That left Java single-hop, and single-hop was not a missing finding — it was a FALSE one.**
/// A cross-file Java edge LANDS on `<specifier>#<Class>.<method>` while the callee file's own edges
/// LEAVE from `<path>#<Class>.<method>`, so a guard one file further away (handler -> helper -> guard)
/// was unreachable and `mutating-route-no-auth` FIRED on a route that is guarded. Measured on a
/// three-controller fixture (2026-09-07, review ledger V30): 2 findings where 1 is real. This doc used
/// to record the limit as coverage this wiring "does not buy", which is the half of the truth that does
/// no harm to say.
///
/// It is now joined AFTER resolution by [`java_bridge`] — additive edges between the two id spaces,
/// using the same `pipeline::JavaIndex` the dep-graph already builds (`java_index`). See that module for
/// why the join belongs after the resolver rather than inside it.
///
/// **Python's module-attribute case is the analogous shape and is ALSO bridged**, by
/// [`python_bridge`] (2026-09-08, review ledger V100): `from pkg import mod` + `mod.f()` lands on
/// `pkg/__init__.py#mod.f` while `f`'s own edges leave from `pkg/mod.py#f`. Both bridges are called
/// unconditionally from `graph_build::build`.
///
/// ⚠ This paragraph said Python was "NOT covered — no measurement yet" for four days after the bridge
/// shipped (found 2026-09-12, ledger V169). A doc that reports a shipped capability as ABSENT is not a
/// harmless lag: the next reader scoring coverage counts it as an open gap, and the reviewer who found
/// this nearly did. When a second member joins a class, the sentence that named the first is part of
/// the change.
#[allow(clippy::too_many_arguments)]
pub(in crate::analyze) fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
    attribute_store: &zzop_core::AttributeStore,
    io_provides: &[zzop_core::IoProvide],
    ts_paths: &HashSet<String>,
    ts_import_pairs: &[(String, ImportMap)],
    // Produced by the per-file lane (`pipeline::fresh::call_graph`), not by this pass. That is the
    // whole of review ledger V108: this pass used to re-read and re-parse every source to get it.
    ts_call_graph_pairs: &[(String, zzop_core::callgraph::CallGraphFacts)],
    java_rels: &[String],
    // The whole-corpus Java package/type index the dep graph already builds. Threaded in for
    // `java_bridge` — see that module's doc for why a Java call graph needs it AFTER resolution rather
    // than inside the resolver.
    java_index: &crate::pipeline::JavaIndex,
    rust_workspace: &crate::pipeline::RustWorkspaceMap,
    all_symbols: &[zzop_core::SourceSymbol],
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
    decorator_guarded_out: &mut BTreeSet<(String, u32)>,
    // Files that LOOK like a Spring Security config and whose posture extraction bailed, with the bail's
    // stable name. An out-param for the same reason `decorator_guarded_out` is one: this pass computes it
    // as a by-product and the caller owns the reply channel it belongs in.
    posture_bails_out: &mut Vec<(String, &'static str, String)>,
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
    // consumed per `java_rels` entry, and the NestJS producers run inside the TS read loop below, on
    // the text it already holds (`TsGuardEvidence::collect`) — so no producer
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

    // Substrate first: four re-parses, one resolver, one graph — `graph_build`'s own doc for the seam.
    let graph_build::BuiltGraph {
        raw_calls,
        symbol_graph,
        unresolved_callees,
        ts_guards,
        java,
        python_guards,
    } = graph_build::build(
        root,
        ts_paths,
        ts_import_pairs,
        ts_call_graph_pairs,
        java_rels,
        java_index,
        rust_workspace,
        all_symbols,
        &vocab,
        need_decorator_guarded,
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
    marker_lookback_rules::run(
        &marker_lookback_rules::Args {
            root,
            api_endpoints: &api_endpoints,
            all_symbols,
            symbol_graph: &symbol_graph,
            run_unsafe_read,
            run_non_idempotent,
            profile,
        },
        rule_time,
        global_findings,
    );
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
            java.decorator_guarded,
            &python_guards,
            &java.postures,
            &ts_guards,
            io_provides,
            all_symbols,
            vocab.java_source_root,
        );
        *decorator_guarded_out = decorator_guarded.iter().cloned().collect();
        // Hand the named bails up. SCOPE, stated because the first version of this comment got it wrong:
        // collection happens inside `need_decorator_guarded`, so a tree whose route-auth rules are all off
        // (or that registered no route at all) produces no bail line even if it ships a security config.
        // That gating is correct rather than a gap — the disclosure exists because a route-auth rule is
        // about to judge routes without the posture, and with no such rule judging there is no silence to
        // break — but it does mean this channel answers "what did the rule that RAN fail to read", not
        // "what security configs does this tree have".
        posture_bails_out.extend(java.posture_bails.iter().cloned());

        if run_mutating_no_auth {
            // Generic entity-attribute channel — injected auth-guard evidence for routes the call-graph
            // BFS can't see (middleware). Built once by `analyze::assemble` from every Mode-B adapter
            // overlay's `attributes` and threaded in (shared with `schema_usage_findings`). Empty unless
            // an adapter injects. The native rule's OWN gating — unchanged by the A2 decoupling above.
            let t0 = profile.then(Instant::now);
            let found = zzop_rules_http::scan_mutating_route_no_auth(
                &zzop_rules_http::ScanMutatingRouteNoAuthInput {
                    unresolved_callees: &unresolved_callees,
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
