//! zzop-engine — the fused execution engine: per-file parse + DSL-rule pass (`pipeline.rs`), then a
//! whole-graph assembly pass (`analyze.rs`), composed for public consumption by `analyze_tree`.
//!
//! Implements the two design principles this crate is built against:
//! - **Fused execution**: per-file DSL rules run **inside** the parse pass, before that file's AST is
//!   dropped; whole-graph rules run after, over the assembled `CommonIr`; a parse failure or oversized file
//!   degrades to a lexical fallback and is recorded, never crashes the pipeline.
//! - **The 2-layer IR**: a
//!   Normalized-AST layer that never leaves the parser's own call frame, projected into the
//!   language-neutral Common IR the engine and rules consume; oxlint-style single traversal (no per-rule
//!   re-walk) + rayon file parallelism.
//!
//! ## Deliberate implementation choices (see also doc comments at each cited site)
//! - **TS parse-failure signal** (`pipeline::parse_typescript`'s doc): `zzop-parser-typescript`'s public
//!   API swallows a swc syntax error into an empty, successful-looking result — there is no `Result`/
//!   panic this crate can observe to tell "broken file" apart from "legitimately empty file" without a
//!   direct swc dependency (rejected — this crate depends only on the parser crates, never swc itself).
//!   This crate adds one local, swc-independent heuristic (`looks_structurally_broken`: brace/paren/
//!   bracket balance, comment/string aware) as a pre-check ahead of the real parse call. `catch_unwind`
//!   still wraps the actual parse calls underneath as defense in depth.
//! - **IoFacts / io-scan wiring**: wired in `pipeline::process_file` via `crate::io::extract_file_io`,
//!   one call per well-formed TypeScript file, against a **single-file slice** of the two TS-side
//!   adapters that were designed for a project-wide call. File-local constant/const-map indirection
//!   still resolves; a shared constants module imported from ANOTHER file does not resolve at that
//!   one-file call site, but is no longer lost — `analyze::late_resolve_cross_file_consumes` merges
//!   every file's constant-map fragment and re-resolves the consume before assembly freezes
//!   `MinimalIr::io`. A sub-router mounted from a different file is the one shape still unresolved — the
//!   endpoint is simply absent from `provides` rather than crashing or fabricating a wrong key.
//! - **TS dep-graph resolution scope**: `resolve::build_dep` runs once, in the assembly phase
//!   (`analyze::assemble`), over every TypeScript-dispatched file's `rel` path and `ImportMap` — never
//!   per-file inside the fused pass. A file's own outgoing edges depend on which *other* files exist in
//!   the tree, so it cannot be resolved until every file has been walked and dispatched. Only
//!   TypeScript-dispatched files participate — Prisma has no import syntax, and lexical-only files were
//!   never parsed for imports in the first place.

mod analyze;
mod cache;
mod config;
mod coverage;
mod cross_layer_findings;
mod dead_exports;
mod disclosure;
mod dispatch;
mod envelope;
mod file_routes;
mod framework_silence;
mod generated_banner;
mod io;
mod output;
mod pipeline;
mod trees;

use std::path::Path;

use zzop_core::RuleRegistry;

pub use config::{EngineConfig, GitOptions, MountRule, PackSource};
pub use coverage::CoverageCensus;
pub use disclosure::{blindness_registry, BlindnessClass, DisclosureStatus};
pub use dispatch::{DispatchConfig, Language};
pub use envelope::analyze_envelope;
pub use io::IoOptions;
pub use output::{AnalyzeOutput, CacheStats, GitWindow, PackLoaded, RuleOverridesApplied};
pub use trees::{
    analyze_trees, MultiAnalyzeOutput, PackageImportSummary, MIN_PARALLEL_IMPL_SIGNALS,
};

/// Composes every crate's own `register_native_analyses` into one `RuleRegistry` — the engine aggregator
/// half of the extensibility contract (`rules/README.md`'s "Adding a rule" section). The kernel
/// (`zzop_core`) itself registers nothing; native analyses live in their owning crates.
pub fn register_all_native(registry: &mut RuleRegistry) {
    zzop_rules_graph::register_native_analyses(registry);
    zzop_rules_http::register_native_analyses(registry);
    zzop_rules_cross_layer::register_native_analyses(registry);
    zzop_rules_schema::register_native_analyses(registry);
    zzop_metrics::register_native_analyses(registry);
}

/// A size cap above which a file skips structural parsing entirely and falls back to a lexical count
/// (`pipeline::process_file`'s oversized branch) — the "graceful degrade on oversized file" half of the
/// fusion contract. ~1.5MB: large enough that no realistic hand-written source file hits it, small enough
/// that a generated/vendored/minified file does not blow up parse time or memory in the per-file pass.
pub const DEFAULT_SIZE_CAP: usize = 1_500_000;

/// Runs the fused engine over every file under `root`: per-file parse + DSL rules (`pipeline`), then
/// whole-graph assembly (`analyze`), including the optional git-history-dependent analyses. Two calls
/// over an unchanged tree (and unchanged git history) produce byte-for-byte identical output.
pub fn analyze_tree(root: &Path, config: &EngineConfig) -> AnalyzeOutput {
    // Input-scope self-report (`input-scope-error` in the disclosure registry): a mistyped root used
    // to be absorbed as an empty tree whose only trace was `files: 0` in the census — which reads as a
    // clean/empty repo. A root that does not exist (or is not a directory) is a structural fact about
    // the REQUEST, disclosed up front; a root that exists but yields zero artifacts self-reports after
    // the walk. A too-narrow root that still matches SOME files stays undetected (registry: partial).
    let mut scope_warnings = Vec::new();
    if !root.is_dir() {
        scope_warnings.push(format!(
            "root '{}' does not exist or is not a directory — analyzed as an empty tree (0 files). Check the path for a typo; every count and finding for this tree is a statement about nothing.",
            root.display()
        ));
    }

    let mut cache_warnings = Vec::new();
    let analysis_cache = cache::open_cache(config, &mut cache_warnings);
    let counters = analysis_cache
        .as_ref()
        .map(|_| cache::CacheCounters::default());

    let mut artifacts =
        pipeline::run_file_pass(root, config, analysis_cache.as_ref(), counters.as_ref());
    if scope_warnings.is_empty() && artifacts.is_empty() {
        scope_warnings.push(format!(
            "0 source files found under root '{}' — this tree contributes nothing to any analysis. If that is unexpected, the root points at the wrong directory or every file was filtered before parsing.",
            root.display()
        ));
    }
    let mut overlay_warnings = Vec::new();
    if !config.adapter_overlays.is_empty() {
        envelope::apply_adapter_overlays(
            &mut artifacts,
            &config.adapter_overlays,
            &config.source_id,
            &mut overlay_warnings,
        );
    }
    let mut output = analyze::assemble(root, artifacts, config);

    // Scope warnings lead: they qualify every other line ("about nothing"), so a reader hits them first.
    if !scope_warnings.is_empty() {
        scope_warnings.append(&mut output.warnings);
        output.warnings = scope_warnings;
    }
    output.warnings.extend(cache_warnings);
    output.warnings.extend(overlay_warnings);
    output.cache = counters.map(cache::CacheCounters::into_stats);
    output
}

#[cfg(test)]
mod tests;
