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
mod deep_stack;
mod degrade_cause;
pub(crate) mod findings;
mod fresh;
mod go_module;
mod io_projection;
mod java_index;
mod manifest;
mod package_json;
mod parsers;
mod rust_workspace;
#[cfg(test)]
mod testutil;
mod tsconfig;
pub(crate) use deep_stack::with_parse_stack;
mod walking;

pub(crate) use csharp_index::{scan_csharp_index, CSharpIndex};
pub(crate) use findings::schema_usage_findings;
pub(crate) use go_module::{
    governing_go_module, join_dir as go_module_join_dir, scan_go_modules, GoModuleMap,
};
pub(crate) use java_index::{scan_java_index, JavaIndex};
pub(crate) use package_json::package_json_entries;
// Re-exported so the pre-split `crate::pipeline::PackageJsonScan` path keeps resolving; `assemble`'s
// `dep_graph`/`provides`/`rules` all name it through here.
pub(crate) use degrade_cause::DegradeCause;
pub(crate) use fresh::{text_exceeds_recursion_caps, RecursionNeedle};
pub(crate) use package_json::PackageJsonScan;
pub(crate) use rust_workspace::{scan_rust_workspace, RustWorkspaceMap};
pub(crate) use tsconfig::{tsconfig_preserves_type_imports, tsconfig_scan};

/// One file's contribution to the tree-wide assembly (`analyze::assemble`) — plain data only.
/// `imports` is `Some` for files this engine can place in the shared dep graph (dispatched to a
/// structural parser, including degraded ones — an empty `ImportMap` still gives the file a graph
/// node); `None` for Prisma / lexical-only files, which never
/// participate in `resolve::build_dep`. Membership is decided in one place — `fresh::ts_slot`'s
/// `matches!` — and the roster is not restated here or in any sibling field doc. Below,
/// several fields share that "`None`/empty for a file outside that slot, or a degraded one"
/// convention; noted once here rather than repeated per field. Every non-TypeScript member's
/// dep-graph EDGES resolve through a separate engine-side pass of its own
/// (`analyze::assemble::dep_graph`'s `merge_*_dep_edges` family — that module's call order is the
/// roster), not `zzop_parser_typescript`'s resolver — see those functions' docs for why.
pub(crate) struct FileArtifact {
    pub rel: String,
    /// This file's `zzop-<rule>-ok` markers, set OUTSIDE the cache branches (see `suppress_marker_sites`).
    pub suppress_markers: Vec<zzop_core::dsl::SuppressMarkerSite>,
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
    /// the target's fan-in WITHOUT adding a dep node (mirroring the import pre-scan's fan-in bump), so a `public/*.js`
    /// worklet/worker loaded only by URL string is not a `dead-candidates` false positive. Empty for
    /// non-TypeScript/degraded files, same convention as `dynamic_imports`.
    pub asset_refs: Vec<String>,
    pub loc: u32,
    pub findings: Vec<zzop_core::Finding>,
    /// `Some(cause)` exactly when this file fell back to the lexical projection — the degraded flag and
    /// its reason are ONE field, deliberately, so no code path can report a degrade whose cause is unset
    /// or a cause on a file that parsed. `is_some()` is the old `degraded: bool`.
    pub degrade_cause: Option<DegradeCause>,
    /// Minified/generated classification — distinct from `degrade_cause`: a degraded file still runs
    /// line-scan DSL rules against raw text, but this flag skips ALL DSL rule-pack evaluation.
    /// Structural extraction below is unaffected; this only gates `findings`.
    pub minified_or_generated: bool,
    /// Projected HTTP-egress/route `IoFacts` (see `crate::io`'s module doc for the fusion tradeoff).
    pub io: Option<IoFacts>,
    /// Per-rule DSL timing; empty when profiling is off or on a full cache hit. `analyze::assemble`
    /// sums these into `AnalyzeOutput::rule_timings`.
    pub rule_timings: Vec<RuleTiming>,
    /// Identifiers referenced anywhere in this file, sorted — feeds `unimported-export`' per-file "used
    /// names" (in-file-only liveness, never cross-file).
    pub used_names: Vec<String>,
    /// Names appearing in the PUBLIC SIGNATURE of some exported declaration in this file, sorted —
    /// the position-aware companion `used_names` cannot be (it is a flat, position-blind set). Lets
    /// `unimported-export` exempt a type that is part of an exported value's public API. TypeScript-only;
    /// empty elsewhere, and an empty value simply means "no exemptions" (graceful degrade).
    pub exported_signature_names: Vec<String>,
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
    /// Per-file loop-body line spans (each producing parser's `extract_loop_spans`) — feeds
    /// `zzop_core::dsl::SourceFile::loop_spans`, `Matcher::MethodScan::trigger_in_loop`'s substrate. An
    /// AST-derived projection (unlike `field_usage_tokens`/`store_bound_models` above), so it follows the
    /// `symbols`-style convention: real spans only from a well-formed, non-degraded parse of a language
    /// that projects them; empty for degraded, oversized, or dispatch-`None` files (graceful degrade,
    /// never guessed). WHICH languages project them is deliberately not listed here: the per-language
    /// table is `pipeline/fresh/spans.rs`, which declares itself ground truth for the `loop_spans` column
    /// of `crates/engine/tests/rule_contracts/capability_matrix.rs` (the per-environment SSOT), and a
    /// list copied to this line would go stale on the next arm — it already did.
    pub loop_spans: Vec<(u32, u32)>,
    /// Per-file function line spans with promise-continuation callbacks merged into their call site
    /// (`zzop_parser_typescript::extract_function_spans`) — feeds
    /// `zzop_core::dsl::SourceFile::function_spans`, `Matcher::MethodScan::after_in_same_function`'s
    /// substrate. Same AST-derived convention as `loop_spans` above, but TypeScript ONLY — narrower than
    /// `loop_spans`' producer set, and that asymmetry is published (`docs/NORMALIZED_AST.md`,
    /// `FileIrSlice`'s module doc, and the table in `pipeline/fresh/spans.rs`), not hidden.
    pub function_spans: Vec<(u32, u32)>,
    /// Per-file TEST-ONLY line spans (`zzop_parser_rust::extract_test_spans`) — feeds
    /// `zzop_core::dsl::SourceFile::test_spans`, the SUBTRACTIVE gate every DSL matcher passes through
    /// (`zzop_core::dsl::eval`'s `TestRegions`). Same AST-derived convention as the two span facts above,
    /// but RUST only, and deliberately so: every other language names its tests in the PATH, where the
    /// packs' `${test-paths-stories}` fragment already excludes them, while Rust's `#[cfg(test)] mod
    /// tests` lives inside the shipping file where no path pattern can reach it.
    pub test_spans: Vec<(u32, u32)>,
    /// Per-file call sites (`zzop_core::call_sites::CallSite`) — feeds
    /// `zzop_core::dsl::SourceFile::call_sites`, `Matcher::CallScan`'s substrate. Same AST-derived
    /// convention as the three span facts above (real sites only from a well-formed, non-degraded parse
    /// of a language that projects them). WHICH languages project them is deliberately not listed here:
    /// producers land one dispatch arm at a time in `pipeline/fresh/call_sites.rs`, and the
    /// per-environment SSOT is `capability_matrix`'s `call_sites` column
    /// (`crates/engine/tests/rule_contracts/capability_matrix.rs`) — a language with no arm carries an
    /// empty vec, graceful degrade like every other fact on this struct.
    pub call_sites: Vec<zzop_core::CallSite>,
    /// Per-file bound string literals (`zzop_core::BoundStringLiteral`) — feeds
    /// `zzop_core::dsl::SourceFile::string_literals`, `Matcher::LiteralScan`'s substrate. Same
    /// AST-derived convention and the same "which languages is the capability matrix's business"
    /// stance as `call_sites` directly above; producers land one dispatch arm at a time in
    /// `pipeline/fresh/string_literals.rs`. Carries hash + entropy per entry, NEVER the literal's
    /// value — `zzop_core::string_literals`'s no-plaintext contract, load-bearing here because this
    /// struct is what the cache serializes.
    pub string_literals: Vec<zzop_core::BoundStringLiteral>,
    /// This file's contribution to the whole-tree call graph — call sites plus the NestJS guard
    /// evidence that used to be gathered by the call-graph pass's own read loop. Produced here
    /// because the parse memo is warm here and nowhere else; see
    /// `zzop_core::callgraph::CallGraphFacts` for the measurement that moved it.
    pub call_graph: zzop_core::callgraph::CallGraphFacts,
    /// `unimported-export`'s two remaining per-file inputs — see `FileIrSlice`'s note for why they
    /// are carried rather than re-read.
    pub export_aliases: Vec<(String, String)>,
    pub has_generated_banner: bool,
}

/// Runs the fused per-file pass over every file under `root` (skipping `config.dispatch.skip_dirs`) and
/// returns one `FileArtifact` per file, sorted by `rel`. `cache`/`counters` are `analyze_tree`'s
/// already-opened cache handle and shared hit/miss counters — both `None` when caching is off.
///
/// `skipped_dirs` is an out-param carrying the walk's prune list up to the caller's diagnostics (see
/// [`walking::Walked`]): the prune happens here but only `analyze_tree` assembles warnings, and a fact
/// that never leaves this function is a fact the reply cannot disclose.
pub(crate) fn run_file_pass(
    root: &Path,
    config: &EngineConfig,
    cache: Option<&AnalysisCache>,
    counters: Option<&CacheCounters>,
    skipped_dirs: &mut Vec<String>,
) -> Vec<FileArtifact> {
    // `config.cache_dir` is handed to the walk, not just to the store: the directory this run writes its
    // own entries into must not be walked as source by the NEXT run (`walk_files`'s doc has the growth
    // numbers). It is passed even when `cache` is `None` — a cache directory that failed to OPEN may still
    // hold entries an earlier run wrote, and those are no more source than this run's are.
    let walked = walking::walk_files(root, &config.dispatch, config.cache_dir.as_deref());
    let files = walked.files;
    skipped_dirs.extend(walked.skipped_dirs);
    // Pack-level and per-rule `disabled_rules` gating happen once here, outside the per-file loop
    // (`pack_loader::applies_to` below is the remaining per-file pre-filter). A bare pack id drops the
    // whole pack; a `"{pack}/{rule}"` id drops just that rule.
    let gated_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| registry::is_pack_enabled(&config.rule_config, &p.id))
        .map(|p| gate_pack_rules(p, config))
        .collect();
    let enabled_packs: Vec<&RulePackDef> = gated_packs.iter().collect();
    // Computed once per call (constant across every file in this pass), not per file. `None` when the
    // cache is off.
    let ruleset_fp = cache.map(|_| crate::cache::ruleset_fingerprint(&enabled_packs, config));
    // The run's declared convention vocabulary, resolved ONCE per pass rather than per file: several of
    // its keys reach the per-file projection (write sites, db-table consumes, router-mount guards), and
    // resolving inside the rayon body would rebuild the same lists for every file.
    let vocab = config.vocabulary.resolve();

    // Every parser here is recursive-descent, so source nesting becomes stack depth and a deep
    // enough file ABORTS the process — no JSON, no warning naming it, nothing for the other files in
    // the tree. `deep_stack` gives the pass (workers AND the thread that joins them) room that no
    // measured input comes near; its module doc carries the table of what used to die. Review ledger V99.
    let mut artifacts: Vec<FileArtifact> = deep_stack::with_parse_stack(|| {
        files
            .par_iter()
            .map(|(rel, abs)| {
                artifact::process_file(
                    rel,
                    abs,
                    config,
                    &vocab,
                    &enabled_packs,
                    cache,
                    ruleset_fp.as_deref(),
                    counters,
                )
            })
            .collect()
    });
    artifacts.sort_by(|a, b| a.rel.cmp(&b.rel));
    artifacts
}

/// Per-rule `disabled_rules` gating: returns a clone of `pack` with every rule whose full
/// `"{pack.id}/{rule.id}"` id is disabled removed from `rules`. Called once per call (not per file),
/// shared by both `analyze_tree` and `analyze_envelope`. A pack left with zero rules behaves like an
/// empty pack downstream (`pack_loader::applies_to` returns `false`).
pub(crate) fn gate_pack_rules(pack: &RulePackDef, config: &EngineConfig) -> RulePackDef {
    let mut gated = pack.clone();
    gated.rules.retain(|rule| {
        registry::is_enabled(&config.rule_config, &format!("{}/{}", pack.id, rule.id))
    });
    // The project's own EXTRA test-path spellings, ADDED to the language conventions the shared
    // `${test-paths…}` fragment already carries. Applied HERE, on the per-pass clone, rather than in
    // `zzop-facade` where the config is assembled: this is the one seam every lane funnels through
    // (fused file pass, whole-tree io-scan, envelope ingest, the decorator-gate predicate), so a direct
    // Rust embedder that never touches the facade gets the same behavior as a CLI run. Per tree by
    // construction — `config` is one tree's config and `pack` is one tree's pack.
    //
    // It also lands INSIDE the ruleset fingerprint for free: callers hash these gated packs
    // (`cache::ruleset_fingerprint`), so a changed declaration misses the warm entries written under
    // the old one instead of being served their answers.
    if let (Some(extra), _) = crate::vocabulary::extra_test_path_tail(&config.vocabulary) {
        gated.extend_test_path_exclusions(&extra);
    }
    // The project's own secret NAMES, replacing the seven `security/hardcoded-secret` compiles in.
    // Same seam and the same two reasons: every lane funnels through here, and the rewrite lands
    // inside the ruleset fingerprint for free, so a changed declaration misses the warm entries
    // written under the old one instead of being served their answers.
    //
    // 🔴 It runs UNCONDITIONALLY, `None` included, and that is the contract rather than an
    // optimization: `None` means the config declared nothing, and for a replacing key that is "do not
    // make this judgment", which the rewriter spells as a pattern nothing satisfies. Making the call
    // conditional on a declaration would silently restore the built-ins for an author who deleted the
    // key — the exact fallback this repo removed on 2026-07-27.
    //
    // A pack that carries none of it is rewritten zero times and that is correct — every pack but the
    // bundled `security` one is in exactly that position, as is any pack an author writes.
    //
    // 🔴 The count is NOT checked here, and the first draft's attempt to is worth recording: it read
    // `pack.id != "security" || rewritten == PACK_ARMS`, which treats a NAME as identity. Twenty-nine
    // tests across three binaries build their own fixture pack, some of them called `security`, and
    // every one of them fired the assertion — as would any user whose own pack picks that id. The
    // subject of that check is the SHIPPED pack text, not whatever is passing through this function,
    // so it lives where that text does: `the_pack_spells_the_owned_alternation_in_exactly_the_declared
    // _number_of_arms` in `rules/dsl/security/secrets.rs` reads `security.json` and holds it to
    // `secret_names::PACK_ARMS`.
    gated.rewrite_secret_names(
        zzop_core::dsl::secret_names::alternation_from(&config.vocabulary.secret_names).as_deref(),
    );
    if gated.rules.len() != pack.rules.len() {
        // The clone's rules vec changed shape, so it must not share the original's POSITIONAL
        // prefilter state — see `RegexCache::fork_for_mutated_rules` (pattern memo kept, prefilter
        // rebuilt for this shape). One loaded pack evaluated under two `disabled_rules` configs is
        // an ordinary embedder flow, and the shared prefilter mis-mapped rule indices there.
        gated.regex_cache = pack.regex_cache.fork_for_mutated_rules();
    }
    gated
}
