//! The `analyze`/`analyzeTrees` entry points: JSON string in, JSON string out.

use std::path::PathBuf;

use zzop_engine::EngineConfig;

use crate::config::build_engine_config;
use crate::output::{
    disclosure_views, AnalyzeOutputView, MultiAnalyzeOutputView, SingleTreeOutputView,
    TreeEntryView,
};
use crate::request::{AnalyzeRequest, AnalyzeTreesRequest};

/// `analyze(configJson)`: deserializes `configJson` into an `AnalyzeRequest`, builds
/// an `EngineConfig` (loading DSL packs from `packs_dir` via `zzop_core::load_dsl_packs` when given), runs
/// `zzop_engine::analyze_tree`, and serializes the result. Every failure mode (bad JSON, a pack directory
/// that doesn't exist) returns `Err(message)` — never a panic; `addon.rs` maps `Err` to a napi `Error`.
pub fn analyze_json(config_json: &str) -> Result<String, String> {
    let req: AnalyzeRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-facade: invalid analyze() config JSON: {e}"))?;
    if req.root.is_empty() {
        return Err("zzop-facade: analyze() config is missing required field \"root\"".to_string());
    }
    let root = PathBuf::from(&req.root);

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
    let req: AnalyzeTreesRequest = serde_json::from_str(config_json)
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
        disclosure: disclosure_views(),
    };

    serde_json::to_string(&view)
        .map_err(|e| format!("zzop-facade: failed to serialize analyzeTrees() output: {e}"))
}
