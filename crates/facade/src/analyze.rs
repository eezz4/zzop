//! The `analyze`/`analyzeTrees` entry points: JSON string in, JSON string out.

use std::path::{Path, PathBuf};

use zzop_engine::EngineConfig;

use crate::config::build_engine_config;
use crate::output::{
    disclosure_views, AnalyzeOutputView, MultiAnalyzeOutputView, SingleTreeOutputView,
    TreeEntryView,
};
use crate::request::{AnalyzeRequest, AnalyzeTreesRequest};

/// The ONE `sourceId` default, applied to every tree-rooted request (`analyze` and each
/// `analyzeTrees` entry alike): a missing/empty `sourceId` names the tree after its root's directory
/// basename (lossy `to_str` fallback: the given root string, e.g. a bare drive root). An explicit
/// non-empty `sourceId` is never overridden.
///
/// Hoisted HERE — the shared chokepoint every host funnels through (Node addon, `zzop-mcp`, engine
/// examples via this facade) — so one naming rule holds everywhere (D14, 2026-07-17): before this, a
/// config-discovered single tree with no explicit `sourceId` reached the engine as `""`, so the
/// overlay source-mismatch warning said the envelope "declares a different source than the tree
/// \"\"" while `check_endpoint`'s matched objects showed the directory name for the SAME tree
/// (`zzop-mcp` defaulted the name on its endpoint path only). With the default at this chokepoint,
/// the mismatch warning and query output always agree, and an adapter author can read the right
/// `source` value off either. Applied AFTER `zzop-config`'s JS-parity mapping, which stays
/// shape-only (the mapper never invents a single-root `sourceId` — parity fixtures unaffected).
///
/// Cache note (why this default is safe to add): `source_id` is half of the per-file cache scope
/// (`crates/engine/src/cache.rs`'s `cache_scope`, see its "Scope" module-doc section). Renaming an
/// unnamed tree from `""` to its basename orphans that tree's old `""`-scoped entries ONCE — a
/// one-time re-analyze on upgrade, same magnitude as a fingerprint bump, never a wrong result. It
/// cannot introduce wrong-result aliasing either: the full key still includes file content, parser
/// and ruleset fingerprints, and the file's own `rel`, which fully determine the per-file result at
/// fixed fingerprints — the scope's actual job (same content, DIFFERENT rel) stays closed.
fn apply_source_id_default(req: &mut AnalyzeRequest) {
    if req.source_id.is_empty() {
        req.source_id = Path::new(&req.root)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&req.root)
            .to_string();
    }
}

/// Shared root-shape gate — the ONE chokepoint the host (the `zzop-mcp` binary's CLI/MCP surface)
/// funnels through via `analyze_json`/`analyze_trees_json`, so every entry gets the
/// same error for the same mistake. A root that names an EXISTING file (not a directory) used to fall
/// straight through to `zzop_engine::analyze_tree`'s walk, which treats a file path exactly like an
/// empty directory: the walk yields that one file as its sole entry, so the output was
/// self-contradictory (`analyzed as an empty tree (0 files)` in `warnings` while `coverage.files`/
/// `fileCount` read 1 — a blind-agent field test hit this passing an envelope JSON's own path as the
/// analysis root). Failing fast here instead is strictly better than the walk's own guess.
///
/// A root that simply does NOT exist is deliberately left untouched: `analyze_tree`'s own leading
/// scope-warning self-report (`crates/engine/src/lib.rs`) already handles that case sanely (`file_count:
/// 0`, one leading warning naming the path, no contradiction with `coverage`) — verified by
/// `zzop_engine`'s own `nonexistent_root_self_reports_as_the_leading_warning` pinned test — so there is
/// nothing to fix there and no reason to turn a typo'd path into a hard error instead of a warning.
fn reject_non_directory_root(root: &Path) -> Result<(), String> {
    if root.exists() && !root.is_dir() {
        return Err(format!(
            "zzop-facade: root is not a directory: {} — pass the tree's root directory (an envelope \
             JSON is validate_envelope's input, not an analysis root)",
            root.display()
        ));
    }
    Ok(())
}

/// `analyze(configJson)`: deserializes `configJson` into an `AnalyzeRequest`, builds
/// an `EngineConfig` (loading DSL packs from `packs_dir` via `zzop_core::load_dsl_packs` when given), runs
/// `zzop_engine::analyze_tree`, and serializes the result. Every failure mode (bad JSON, a pack directory
/// that doesn't exist) returns `Err(message)` — never a panic; the host maps `Err` to its own error
/// channel (the MCP `isError` reply / a nonzero CLI exit).
pub fn analyze_json(config_json: &str) -> Result<String, String> {
    let mut req: AnalyzeRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-facade: invalid analyze() config JSON: {e}"))?;
    if req.root.is_empty() {
        return Err("zzop-facade: analyze() config is missing required field \"root\"".to_string());
    }
    apply_source_id_default(&mut req);
    let root = PathBuf::from(&req.root);
    reject_non_directory_root(&root)?;

    let mut warnings = Vec::new();
    let config = build_engine_config(&req, &mut warnings);
    let mut output = zzop_engine::analyze_tree(&root, &config);
    warnings.append(&mut output.warnings);
    output.warnings = warnings;

    serde_json::to_string(&SingleTreeOutputView::of(&output))
        .map_err(|e| format!("zzop-facade: failed to serialize analyze() output: {e}"))
}

/// `analyzeTrees(configJson)`: deserializes `configJson` into an `AnalyzeTreesRequest`
/// (`{trees: AnalyzeRequest[]}`), runs `zzop_engine::analyze_tree` once per entry, then
/// `zzop_engine::analyze_trees`'s cross-layer join over every tree's `IoFacts`. Per-tree DSL-pack-load
/// warnings are attached to that same tree's own `AnalyzeOutput.warnings` (not globally pooled), so a
/// consumer can tell which tree a warning came from.
pub fn analyze_trees_json(config_json: &str) -> Result<String, String> {
    let mut req: AnalyzeTreesRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-facade: invalid analyzeTrees() config JSON: {e}"))?;
    if req.trees.is_empty() {
        return Err(
            "zzop-facade: analyzeTrees() config must include at least one entry in \"trees\""
                .to_string(),
        );
    }
    for (i, tree_req) in req.trees.iter().enumerate() {
        if tree_req.root.is_empty() {
            return Err(format!(
                "zzop-facade: analyzeTrees() trees[{i}] is missing required field \"root\""
            ));
        }
        reject_non_directory_root(Path::new(&tree_req.root))?;
    }
    for tree_req in &mut req.trees {
        apply_source_id_default(tree_req);
    }

    let mut per_tree_warnings: Vec<Vec<String>> = Vec::with_capacity(req.trees.len());
    let mut trees: Vec<(PathBuf, EngineConfig)> = Vec::with_capacity(req.trees.len());
    for tree_req in &req.trees {
        let mut warnings = Vec::new();
        let config = build_engine_config(tree_req, &mut warnings);
        per_tree_warnings.push(warnings);
        trees.push((PathBuf::from(&tree_req.root), config));
    }

    let mut result = zzop_engine::analyze_trees(&trees);
    for (warnings, (_, _, output)) in per_tree_warnings.into_iter().zip(result.trees.iter_mut()) {
        let mut merged = warnings;
        merged.append(&mut output.warnings);
        output.warnings = merged;
    }

    let view = MultiAnalyzeOutputView {
        trees: result
            .trees
            .iter()
            .map(|(root, source_id, output)| TreeEntryView {
                root: root.display().to_string(),
                source_id,
                output: AnalyzeOutputView::of(output),
            })
            .collect(),
        cross_layer: &result.cross_layer,
        cross_layer_findings: &result.cross_layer_findings,
        warnings: &result.warnings,
        disclosure: disclosure_views(),
    };

    serde_json::to_string(&view)
        .map_err(|e| format!("zzop-facade: failed to serialize analyzeTrees() output: {e}"))
}
