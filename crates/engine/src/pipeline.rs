//! Fused per-file pass: for each file, parse -> project this file's slice of Common IR
//! (symbols/loc) -> run every applicable DSL pack against that slice -> return plain data. The
//! parser's AST never leaves the function that calls the parser — only `zzop_core` types
//! (`SourceSymbol`, `ImportMap`, `Finding`, `u32` loc) cross back into this module.
//!
//! Files are processed via `rayon::par_iter` over a single-threaded, pre-sorted walk
//! (`walk_files`), and `run_file_pass` re-sorts the results by path afterward — belt-and-suspenders
//! so output order does not depend on `rayon`'s collect-order guarantee holding across versions.

use std::path::Path;

use rayon::prelude::*;

use zzop_cache::AnalysisCache;
use zzop_core::{dsl::RuleTiming, ir::SourceSymbol, registry, ImportMap, IoFacts, RulePackDef};

use crate::cache::CacheCounters;
use crate::EngineConfig;

mod artifact;
mod csharp_index;
pub(crate) mod findings;
mod fresh;
mod go_module;
mod java_index;
mod manifest;
mod package_json;
mod parsers;
mod rust_workspace;
#[cfg(test)]
mod testutil;
mod tsconfig;
mod walking;

pub(crate) use csharp_index::{scan_csharp_index, CSharpIndex};
pub(crate) use findings::schema_usage_findings;
pub(crate) use go_module::{
    governing_go_module, join_dir as go_module_join_dir, scan_go_modules, GoModuleMap,
};
pub(crate) use java_index::{scan_java_index, JavaIndex};
pub(crate) use package_json::package_json_entries;
// Not currently named outside `pipeline` (callers use field access on `package_json_entries`'
// return), but re-exported so the pre-split `crate::pipeline::PackageJsonScan` path keeps resolving.
#[allow(unused_imports)]
pub(crate) use package_json::PackageJsonScan;
pub(crate) use rust_workspace::{
    declared_rust_target_paths, scan_rust_workspace, RustWorkspaceMap,
};
pub(crate) use tsconfig::tsconfig_scan;

/// One file's contribution to the tree-wide assembly (`analyze::assemble`) — plain data only.
/// `imports` is `Some` for files this engine can place in the shared TS/Python/Rust/Go/Java dep graph
/// (TypeScript-, Python-, Rust-, Go-, or Java21-dispatched, including degraded ones — an empty
/// `ImportMap` still gives the file a graph node); `None` for Prisma / lexical-only files, which never
/// participate in `resolve::build_dep`. `.java` joined this `Some` set only once its dispatch target
/// became a real structural parser (`Language::Java21`) — the retired lexical brace-matcher never
/// produced imports at all, so every `imports: None`/"never participates in the TS dep graph" doc
/// elsewhere describing `.java` predates this and is now stale (updated alongside this one). Below,
/// several fields share that "`None`/empty for a non-TypeScript/Python/Rust/Go/Java21 or degraded file"
/// convention; noted once here rather than repeated per field. Python's, Rust's, Go's, and Java's own
/// dep-graph EDGES each resolve through a separate engine-side pass
/// (`analyze::assemble::dep_graph::merge_python_dep_edges` / `merge_rust_dep_edges` /
/// `merge_go_dep_edges` / `merge_java_dep_edges`), not `zzop_parser_typescript`'s resolver — see those
/// functions' docs for why.
pub(crate) struct FileArtifact {
    pub rel: String,
    pub symbols: Vec<SourceSymbol>,
    pub imports: Option<ImportMap>,
    /// This file's re-exports (`export { x } from './y'` / `export * from './y'`, each carrying its own
    /// `type_only`) — `analyze::assemble`'s substrate for merging non-type-only re-export specifiers into
    /// `build_dep_with_workspace`'s dep graph as real edges (Defect A: a barrel file re-exporting only,
    /// with no local import, used to be invisible to the dep graph, undercounting its target's fan-in and
    /// false-positiving `dead-candidates`). Empty for non-TypeScript/degraded files, same convention as
    /// `imports`.
    pub re_exports: Vec<zzop_core::ReExport>,
    /// This file's dynamic-`import()` specifiers (`parse_dynamic_imports`, which recurses into
    /// `dynamic(() => import('./x'))` / `lazy(() => import('./x'))` wrappers) — `analyze::assemble`'s
    /// substrate for merging them into `build_dep_with_workspace`'s dep graph as real edges that give the
    /// target fan-in but are excluded from circular detection (a code-split-only module used to be
    /// invisible to the dep graph and false-positived `dead-candidates`). Empty for non-TypeScript/degraded
    /// files, same convention as `re_exports`.
    pub dynamic_imports: Vec<String>,
    /// This file's runtime asset-URL references (`parse_asset_refs`: `AudioWorklet.addModule`,
    /// `new Worker`/`new SharedWorker`, `importScripts`, `new URL(<path>, import.meta.url)`) as RAW,
    /// unresolved path strings — `analyze::assemble`'s substrate for `merge_asset_ref_fan_in`, which
    /// resolves each against the tree's `public/`/`static/` root (or a relative module path) and bumps
    /// the target's fan-in WITHOUT adding a dep node (mirroring the SFC fan-in bump), so a `public/*.js`
    /// worklet/worker loaded only by URL string is not a `dead-candidates` false positive. Empty for
    /// non-TypeScript/degraded files, same convention as `dynamic_imports`.
    pub asset_refs: Vec<String>,
    pub loc: u32,
    pub findings: Vec<zzop_core::Finding>,
    pub degraded: bool,
    /// Minified/generated classification — distinct from `degraded`: a degraded file still runs
    /// line-scan DSL rules against raw text, but this flag skips ALL DSL rule-pack evaluation.
    /// Structural extraction below is unaffected; this only gates `findings`.
    pub minified_or_generated: bool,
    /// Projected HTTP-egress/route `IoFacts` (see `crate::io`'s module doc for the fusion tradeoff).
    pub io: Option<IoFacts>,
    /// Per-rule DSL timing; empty when profiling is off or on a full cache hit. `analyze::assemble`
    /// sums these into `AnalyzeOutput::rule_timings`.
    pub rule_timings: Vec<RuleTiming>,
    /// Identifiers referenced anywhere in this file, sorted — feeds `dead-exports`' per-file "used
    /// names" (in-file-only liveness, never cross-file).
    pub used_names: Vec<String>,
    /// Constant-map fragment (same parse, no second pass) — `analyze::assemble` merges every file's
    /// fragment into one project-wide map to re-resolve consumes left unresolved.
    pub const_map_fragment: std::collections::HashMap<String, String>,
    /// tRPC router shape fragment — `analyze::compose_trpc_provides`'s substrate.
    pub procedure_router_fragments: Vec<zzop_core::ProcedureRouterFragment>,
    /// Code-registered router-mount fragment (Hono chained builders / cross-file sub-router mounts) —
    /// provide-side sibling of `procedure_router_fragments`.
    pub router_mount_fragments: Vec<zzop_core::RouterMountFragment>,
    /// Wrapper-DEFINITION fragment — substrate for `analyze`'s assemble-time wrapper-consume join.
    pub wrapper_def_fragments: Vec<zzop_core::WrapperDefFragment>,
    /// Wrapper-CALL fragment — each call is resolved via its import specifier back to a def.
    pub wrapper_call_fragments: Vec<zzop_core::WrapperCallFragment>,
    /// Controller-prefix route fragment (`controller-prefix-ref-v1`) — a `@Controller(RouteKey.Asset)`
    /// dotted member-expression prefix this file alone cannot resolve; `analyze`'s assemble-time
    /// controller-prefix composer resolves `prefix_ref` against the same merged const map
    /// `const_map_fragment` feeds, and emits the real `IoProvide`s.
    pub controller_prefix_route_fragments: Vec<zzop_core::ControllerPrefixRouteFragment>,
    /// Class field-shape fragments (`body-shape-v1`) — `analyze::assemble` merges every file's
    /// fragments into one tree-wide `name -> shape` map to resolve `IoProvide::body.dto_ref`
    /// (the request-body DTO class usually lives in another file than the controller).
    pub class_shape_fragments: Vec<zzop_core::ClassShapeFragment>,
    /// This file's Prisma query-call-site facts (`<clientAccessor>().<model>.<method>(...)`, restricted
    /// to the 4 read-only query methods) — `analyze::assemble`'s substrate for `run_schema_join_rules`,
    /// replacing that pass's own filesystem re-walk (`zzop_rules_schema::join::scan_query_call_sites`,
    /// now removed).
    pub query_call_sites: Vec<zzop_core::QueryCallSite>,
    /// This file's comment/string-stripped identifier tokens (`zzop_rules_schema::field_usage_tokens`) —
    /// `analyze::assemble`'s substrate for `SchemaUsage.identifier_counts` (presence only), replacing
    /// `zzop_rules_schema::usage::scan_field_usage`'s own `<root>/src` filesystem re-walk (now removed).
    /// Unlike most fields on this struct, this one is populated for ANY `.ts`/`.tsx` file regardless of
    /// `language`/`degraded` — the removed `scan_field_usage` was a raw-text regex scan, never an AST
    /// parse, so it never cared whether swc could parse the file.
    pub field_usage_tokens: Vec<String>,
    /// Per-file loop-body line spans (`zzop_parser_typescript::extract_loop_spans`) — feeds
    /// `zzop_core::dsl::SourceFile::loop_spans`, `Matcher::MethodScan::trigger_in_loop`'s substrate. An
    /// AST-derived projection (unlike `field_usage_tokens`/`store_bound_models` above), so it follows the
    /// `symbols`-style convention: real spans only for a well-formed, non-degraded TypeScript file; empty
    /// for non-TypeScript, degraded, oversized, or dispatch-`None` files (graceful degrade, never guessed).
    pub loop_spans: Vec<(u32, u32)>,
}

/// Runs the fused per-file pass over every file under `root` (skipping `config.dispatch.skip_dirs`) and
/// returns one `FileArtifact` per file, sorted by `rel`. `cache`/`counters` are `analyze_tree`'s
/// already-opened cache handle and shared hit/miss counters — both `None` when caching is off.
pub(crate) fn run_file_pass(
    root: &Path,
    config: &EngineConfig,
    cache: Option<&AnalysisCache>,
    counters: Option<&CacheCounters>,
) -> Vec<FileArtifact> {
    let files = walking::walk_files(root, &config.dispatch);
    // Pack-level and per-rule `disabled_rules` gating happen once here, outside the per-file loop
    // (`pack_loader::applies_to` below is the remaining per-file pre-filter). A bare pack id drops the
    // whole pack; a `"{pack}/{rule}"` id drops just that rule.
    let gated_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| registry::is_enabled(&config.rule_config, &p.id))
        .map(|p| gate_pack_rules(p, &config.rule_config))
        .collect();
    let enabled_packs: Vec<&RulePackDef> = gated_packs.iter().collect();
    // Computed once per call (constant across every file in this pass), not per file. `None` when the
    // cache is off.
    let ruleset_fp = cache.map(|_| crate::cache::ruleset_fingerprint(&enabled_packs, config));

    let mut artifacts: Vec<FileArtifact> = files
        .par_iter()
        .map(|(rel, abs)| {
            artifact::process_file(
                rel,
                abs,
                config,
                &enabled_packs,
                cache,
                ruleset_fp.as_deref(),
                counters,
            )
        })
        .collect();
    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
    artifacts
}

/// Per-rule `disabled_rules` gating: returns a clone of `pack` with every rule whose full
/// `"{pack.id}/{rule.id}"` id is disabled removed from `rules`. Called once per call (not per file),
/// shared by both `analyze_tree` and `analyze_envelope`. A pack left with zero rules behaves like an
/// empty pack downstream (`pack_loader::applies_to` returns `false`).
pub(crate) fn gate_pack_rules(pack: &RulePackDef, config: &zzop_core::RuleConfig) -> RulePackDef {
    let mut gated = pack.clone();
    gated
        .rules
        .retain(|rule| registry::is_enabled(config, &format!("{}/{}", pack.id, rule.id)));
    gated
}
