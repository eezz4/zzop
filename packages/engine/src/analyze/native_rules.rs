//! Whole-graph native rule runners + thin delegates to `zzop_rules_graph`/`zzop_rules_http`/
//! `zzop_rules_schema`: circular/unreachable/dead-candidate graph analyses, the call-graph-BFS HTTP
//! rules (re-parses TS off disk — see `run_callgraph_rules`'s own doc for why), the whole-corpus Java
//! Spring provides pass, and the schema x usage JOIN rules.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use zzop_core::{is_enabled, DepGraph, DepStats, FileNode, Finding, ImportMap, IoProvide};

use crate::EngineConfig;

use super::record_native_timing;

/// Fan-in/fan-out/all-paths derived from a resolved dep graph — the minimal `DepStats`-shaped input
/// `build_file_nodes` needs. A local build since `zzop_core::file_nodes` has no standalone "DepStats
/// from a DepGraph" helper.
pub(crate) fn dep_stats_from_dep(dep: &DepGraph) -> DepStats {
    let mut fan_in = std::collections::BTreeMap::new();
    let mut fan_out = std::collections::BTreeMap::new();
    let mut all_paths = std::collections::BTreeSet::new();
    for (src, targets) in dep {
        all_paths.insert(src.clone());
        fan_out.insert(src.clone(), targets.len() as u32);
        for target in targets {
            all_paths.insert(target.clone());
            *fan_in.entry(target.clone()).or_insert(0) += 1;
        }
    }
    DepStats {
        fan_in,
        fan_out,
        all_paths,
    }
}

/// Thin delegate to `zzop_rules_graph::circular_findings`. Kept as a `crate::analyze` function (rather
/// than inlining the call at every call site) since `envelope::analyze_envelope` also imports it by
/// this name/path. `cycles` is passed in (rather than re-derived from `dep`) so this and the
/// scores/recommendations computations above share one `circular_from_dep` call.
pub(crate) fn circular_findings(cycles: &[Vec<String>]) -> Vec<Finding> {
    zzop_rules_graph::circular_findings(cycles)
}

/// Thin delegate to `zzop_rules_graph::unreachable_findings` — see `circular_findings`'s doc for why this
/// wrapper stays here rather than being inlined at its call sites.
pub(crate) fn unreachable_findings(nodes: &[FileNode], dep: &DepGraph) -> Vec<Finding> {
    zzop_rules_graph::unreachable_findings(nodes, dep)
}

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
pub(super) fn run_callgraph_rules(
    root: &std::path::Path,
    config: &EngineConfig,
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
        let t0 = profile.then(Instant::now);
        let found = zzop_rules_http::scan_mutating_route_no_auth(
            &zzop_rules_http::ScanMutatingRouteNoAuthInput {
                io_provides,
                symbols: all_symbols,
                symbol_graph: &symbol_graph,
                auth_guard_pattern: zzop_rules_http::DEFAULT_AUTH_GUARD_PATTERN,
                nest_guarded: &nest_guarded,
            },
        );
        record_native_timing(rule_time, t0, "mutating-route-no-auth", found.len());
        global_findings.extend(found);
    }
}

/// Whole-corpus Java Spring HTTP-provides pass — wires `zzop_parser_java::extract_http_provides_project`
/// (see that module's doc for the two per-file-invisible facts it resolves: CE-split `extends`-chain
/// gating, and constant/constant-concatenation class-level `@RequestMapping` prefixes) into `assemble`.
/// Runs once per `analyze_tree` call, over EVERY non-degraded java-dispatched file (`java_rels`),
/// reading each file's text fresh off disk — the fused per-file pass drops each file's text after
/// projecting its own slice, and folding a whole-corpus-dependent result into the per-file cache would
/// let an edit to one file (e.g. a prefix-constants-only file with no routes of its own) leave every
/// OTHER already-cached java file's provides silently stale. Recomputed in full on every call — never
/// consults `zzop_cache::AnalysisCache`.
///
/// **Merge semantics**: `io_provides` already carries the fused per-file pass's own java `http` provides
/// — same-file controllers with a literal (or absent) class-level `@RequestMapping`. The project pass
/// finds a superset of that, with one known exception: a controller whose simple class name is
/// duplicated across the corpus is skipped by the project pass's ambiguous-class guard even when its
/// prefix is literal, so its per-file provides are deleted by this replacement without a project-side
/// substitute (route loss). Accepted: duplicate controller class names are rare, and key-based dedupe
/// instead would leave a latent trap where the two passes silently disagree on one fact and both
/// entries survive. So this REPLACES the per-file java `http` provides wholesale with the project
/// pass's own output, for every file in `java_rels`: one source of truth.
pub(super) fn run_java_provides_project_pass(
    root: &std::path::Path,
    java_rels: &[String],
    io_provides: &mut Vec<IoProvide>,
) {
    let java_set: HashSet<&str> = java_rels.iter().map(String::as_str).collect();
    let mut files: Vec<(String, String)> = Vec::with_capacity(java_rels.len());
    for rel in java_rels {
        // Unreadable (deleted/permission race since the fused pass's own read) — same "treat as absent
        // rather than fail the whole analysis" convention `dead_export_findings` documents for its own
        // disk re-read.
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            files.push((rel.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    if files.is_empty() {
        return;
    }
    let report = zzop_parser_java::extract_http_provides_project(&files);
    io_provides.retain(|p| !(p.kind == "http" && java_set.contains(p.file.as_str())));
    io_provides.extend(report.provides);
}

/// Runs the three schema x usage JOIN native rules (`soft-delete-bypass` / `orderby-unindexed` /
/// `enum-string-drift` — `zzop_rules_schema::join`'s module doc) — a whole-tree pass over every
/// non-degraded Prisma file (`prisma_rels`, same eligibility as `schema-usage`) plus `sites`, every
/// file's Prisma query-call-site facts already collected by `assemble`'s per-artifact loop (parser
/// output, not a filesystem walk of this function's own — see `zzop_parser_typescript::
/// extract_query_call_sites`), gated per-id via `is_enabled` and timed via `record_native_timing`, the
/// same shape every other whole-tree native analysis in `assemble` uses.
///
/// `enum-string-drift` also collects `SchemaEnum`s (via `zzop_parser_prisma::parse_schema_enums`,
/// alongside the per-file `parse_schema` call for models) over the same `prisma_rels`, so
/// `enum_string_drift_issues` has both model and enum substrate to join call-site literals against.
///
/// All three rules need evidence spanning the whole BE source tree (every query call site for a model,
/// not just one file), so the model/enum parse is recomputed in full on every `assemble` call and never
/// enters the per-file findings cache (`sites` itself IS cached, per-file, via `FileIrSlice`).
pub(super) fn run_schema_join_rules(
    root: &std::path::Path,
    prisma_rels: &[String],
    sites: &[zzop_core::QueryCallSite],
    config: &EngineConfig,
    profile: bool,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) {
    if prisma_rels.is_empty() {
        return;
    }
    if !is_enabled(&config.rule_config, "soft-delete-bypass")
        && !is_enabled(&config.rule_config, "orderby-unindexed")
        && !is_enabled(&config.rule_config, "enum-string-drift")
    {
        return;
    }

    let mut models: Vec<zzop_core::SchemaModel> = Vec::new();
    let mut enums: Vec<zzop_core::SchemaEnum> = Vec::new();
    for rel in prisma_rels {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        models.extend(zzop_parser_prisma::parse_schema(&text, Some(rel), None));
        enums.extend(zzop_parser_prisma::parse_schema_enums(&text));
    }
    if models.is_empty() {
        return;
    }

    run_join_rule(
        "soft-delete-bypass",
        &config.rule_config,
        profile,
        &models,
        sites,
        zzop_rules_schema::soft_delete_bypass_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "orderby-unindexed",
        &config.rule_config,
        profile,
        &models,
        sites,
        zzop_rules_schema::orderby_unindexed_issues,
        rule_time,
        global_findings,
    );
    run_join_rule(
        "enum-string-drift",
        &config.rule_config,
        profile,
        &models,
        sites,
        |m, s| zzop_rules_schema::enum_string_drift_issues(m, &enums, s),
        rule_time,
        global_findings,
    );
}

/// Runs one schema x usage JOIN rule (`rule_fn`) under the `id` gate, appending its findings to
/// `global_findings` and timing the call. `rule_fn` is generic (not a bare `fn` pointer) so
/// `enum-string-drift`'s call site can close over its extra `enums` argument via a closure while the
/// other two rules' plain `fn` items keep coercing in unchanged.
#[allow(clippy::too_many_arguments)]
fn run_join_rule<F>(
    id: &str,
    rule_config: &zzop_core::RuleConfig,
    profile: bool,
    models: &[zzop_core::SchemaModel],
    sites: &[zzop_rules_schema::QueryCallSite],
    rule_fn: F,
    rule_time: &mut HashMap<String, (u128, usize)>,
    global_findings: &mut Vec<Finding>,
) where
    F: Fn(
        &[zzop_core::SchemaModel],
        &[zzop_rules_schema::QueryCallSite],
    ) -> Vec<zzop_rules_schema::JoinIssue>,
{
    if !is_enabled(rule_config, id) {
        return;
    }
    let t0 = profile.then(Instant::now);
    let issues = rule_fn(models, sites);
    let found: Vec<Finding> = issues.iter().map(join_issue_to_finding).collect();
    record_native_timing(rule_time, t0, id, found.len());
    global_findings.extend(found);
}

/// One `JoinIssue` -> one `Finding`. Unlike `schema_issue_to_finding` (`pipeline.rs`), no
/// `zzop_parser_prisma::model_decl_line` lookup is needed: `JoinIssue` already carries the exact BE
/// call-site `file`/`line` it fired at. `rule_id` is the bare id, not `"schema/{id}"` — each of these
/// three is a whole individually-gated toggle unit, matching `duplicate-route`'s convention rather than
/// `schema-usage`'s pack-namespace-prefixed sub-rule ids.
fn join_issue_to_finding(issue: &zzop_rules_schema::JoinIssue) -> Finding {
    Finding {
        rule_id: issue.rule.clone(),
        severity: issue.severity,
        file: issue.file.clone(),
        line: issue.line,
        message: zzop_rules_schema::join_issue_message(issue),
        data: serde_json::to_value(issue).ok(),
    }
}

/// Thin delegate to `zzop_rules_graph::dead_candidate_findings` — see `circular_findings`'s doc. `extra_entries`
/// forwards straight through (package.json-referenced entry files).
pub(crate) fn dead_candidate_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    zzop_rules_graph::dead_candidate_findings(nodes, dep, extra_entries)
}

#[cfg(test)]
mod prisma_client_getter_consistency_tests {
    //! `zzop_parser_typescript::PRISMA_CLIENT_GETTER` and `zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN`
    //! are twin recognizers of the same "Prisma client getter function name" convention, kept in two
    //! separate parser crates on purpose: `zzop_core` is vocabulary-free (no Prisma-specific concept
    //! belongs there), and a parser-typescript -> parser-prisma dependency edge for one string would be
    //! architecturally backwards. This guard — living here since `zzop_engine` already depends on both
    //! parsers — catches the two twins silently drifting apart without forcing either coupling.
    #[test]
    fn prisma_client_getter_twins_stay_in_sync() {
        assert_eq!(
            zzop_parser_typescript::PRISMA_CLIENT_GETTER,
            zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN
        );
    }
}
