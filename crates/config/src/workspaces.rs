//! `trees: "auto"` workspace expansion — the port of the removed JS CLI's `workspaces.js` (2026-07-20), specifically its
//! `expandAutoTrees`. Only activates when `trees` is EXACTLY the string `"auto"`; every other shape
//! passes through untouched. The JS algorithm is hand-rolled and must be ported as-is, not replaced
//! with a general glob/YAML crate (a "smarter" matcher would change which packages get discovered):
//! - Manifest precedence: `pnpm-workspace.yaml` first (block list or inline flow list forms only),
//!   else `package.json` `workspaces` (array or `{packages: [...]}`); NEITHER present → a
//!   `ConfigError` telling the user to write an explicit `trees` array — never a silent single-tree
//!   fallback.
//! - Glob expansion: segment-by-segment against real directories, `*`/`?`/`**` (depth cap 40),
//!   `node_modules` and `.git` never descended, `!`-negatives applied as a whole-path anchored
//!   filter; a match is kept only if it contains a `package.json`; results sorted alphabetically
//!   (the determinism guarantee).
//! - `sourceId` = the package's own `name` field, else the relative dir; duplicate sourceIds are a
//!   WARNING (cross-source joins key on sourceId), not an error.
//! - Always emits an informational expansion warning; an extra one when only 1 tree resulted (the
//!   cross-layer join needs 2+).
//!
//! Manifest readers live in `workspaces/manifest.rs`, the glob expansion + matching engine in
//! `workspaces/glob.rs`; this root keeps the census-pinned constants and the public entry points:
//! `expand_auto_trees` (the expansion itself) and `single_tree_workspace_warning` (the same manifest
//! detection read for its honesty value whenever a run resolved ONE tree over a multi-package
//! workspace root, so a monorepo never silently degrades to a single tree — and thus no cross-layer
//! join — without saying so).

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::ConfigError;

mod glob;
mod manifest;
#[cfg(test)]
mod tests;

use glob::resolve_workspace_dirs;
use manifest::{read_npm_workspace_packages, read_package_name, read_pnpm_workspace_packages};

/// Directories never descended into while expanding a `**` glob, and never returned as workspace
/// packages: scanning them is both wasteful and wrong for workspace detection.
const SKIP_DIRS: [&str; 2] = ["node_modules", ".git"];

/// Hard cap on `**` recursion depth — a backstop against a pathological symlink cycle or an
/// absurdly deep tree. Far below any real monorepo nesting.
const MAX_GLOB_DEPTH: u32 = 40;

/// Expands `trees: "auto"` in `config` against `base_dir` (the config file's directory — the JS CLI
/// passes cwd, which is the same directory in normal CLI use). Returns the possibly-rewritten config
/// plus the expansion warnings. A config without `trees: "auto"` comes back unchanged.
pub fn expand_auto_trees(
    config: serde_json::Value,
    base_dir: &Path,
) -> Result<(serde_json::Value, Vec<String>), ConfigError> {
    let trees_is_auto = config
        .as_object()
        .and_then(|m| m.get("trees"))
        .and_then(Value::as_str)
        == Some("auto");
    if !trees_is_auto {
        return Ok((config, Vec::new()));
    }

    // `trees_is_auto` can only be true when `config.as_object()` above succeeded.
    let mut map = match config {
        Value::Object(m) => m,
        _ => unreachable!("trees_is_auto implies config is a JSON object"),
    };

    let (patterns, source) = if let Some(found) = read_workspace_manifest(base_dir) {
        found
    } else {
        return Err(ConfigError(format!(
            "trees: \"auto\" found no workspace manifest in {} — expected a pnpm-workspace.yaml with a \"packages:\" list, or a package.json with a \"workspaces\" field. Write an explicit \"trees\": [{{ \"root\": ..., \"sourceId\": ... }}] array instead, or run zzop from the workspace root.",
            base_dir.display()
        )));
    };

    let dirs = resolve_workspace_dirs(base_dir, &patterns);
    if dirs.is_empty() {
        let joined = patterns.join(", ");
        let patterns_display = if joined.is_empty() {
            "(none)"
        } else {
            joined.as_str()
        };
        return Err(ConfigError(format!(
            "trees: \"auto\" matched no package directories from {source} (patterns: {patterns_display}). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
        )));
    }

    let mut warnings = Vec::new();

    // Shadowed-key honesty gap (D-something, blind field test): `roots` has zero effect once
    // `trees: "auto"` is in play — auto's workspace scan base is always `base_dir` (the config
    // file's directory), never anything `roots` names. Without this warning a config author sees
    // `roots` silently steer nothing. Remove the now-inert key so the generic `roots`+`trees`
    // check in `mapper::config_to_request` (which runs on this function's OUTPUT, after "auto" has
    // become a concrete array) doesn't also fire and double-warn about the same root cause.
    if map.contains_key("roots") {
        warnings.push(
            "config has both \"roots\" and \"trees\": \"auto\" — auto wins and scans the config \
             file's directory for workspace members; \"roots\" is ignored in auto mode (remove one)."
                .to_string(),
        );
        map.remove("roots");
    }

    let mut seen_source: HashMap<String, String> = HashMap::new();
    let mut trees: Vec<(String, String)> = Vec::with_capacity(dirs.len());
    for rel in &dirs {
        let name = read_package_name(&base_dir.join(rel));
        let source_id = name.unwrap_or_else(|| rel.clone());
        if let Some(prev_root) = seen_source.get(&source_id) {
            warnings.push(format!(
                "trees: \"auto\" derived a duplicate sourceId \"{source_id}\" for both \"{prev_root}\" and \"{rel}\". Cross-source joins key on sourceId; give one package a distinct \"name\" or use an explicit \"trees\" array to disambiguate."
            ));
        } else {
            seen_source.insert(source_id.clone(), rel.clone());
        }
        trees.push((rel.clone(), source_id));
    }

    let tree_desc = trees
        .iter()
        .map(|(root, source_id)| format!("{source_id} ({root})"))
        .collect::<Vec<_>>()
        .join(", ");
    warnings.push(format!(
        "trees: \"auto\" expanded to {} tree(s) from {source}: {tree_desc}.",
        trees.len()
    ));
    if trees.len() == 1 {
        warnings.push(
            "trees: \"auto\" resolved only one workspace package — the cross-layer join needs >= 2 trees with distinct sourceIds to fire, so this run behaves like a single-tree analysis."
                .to_string(),
        );
    }

    let trees_json: Vec<Value> = trees
        .into_iter()
        .map(|(root, source_id)| json!({ "root": root, "sourceId": source_id }))
        .collect();
    map.insert("trees".to_string(), Value::Array(trees_json));

    Ok((Value::Object(map), warnings))
}

/// The manifest precedence shared by [`expand_auto_trees`] and [`single_tree_workspace_warning`]:
/// `pnpm-workspace.yaml` first (a present file wins outright, even when its `packages:` list is
/// empty — pnpm's own precedence), else `package.json`'s `workspaces` field. Returns the glob
/// patterns plus the label every message about them uses, so the two callers can never disagree
/// about which manifest a repo "has".
fn read_workspace_manifest(base_dir: &Path) -> Option<(Vec<String>, &'static str)> {
    if let Some(patterns) = read_pnpm_workspace_packages(base_dir) {
        return Some((patterns, "pnpm-workspace.yaml"));
    }
    read_npm_workspace_packages(base_dir).map(|patterns| (patterns, "package.json \"workspaces\""))
}

/// Single-tree-over-a-monorepo disclosure: the run resolved exactly ONE tree, yet `base_dir` (the
/// analyzed root, or the config file's own directory) carries a workspace manifest naming 2+
/// packages. Without this, the run is silent about the biggest thing it did not do — the cross-layer
/// join (zzop's headline capability) needs >= 2 trees, so it never even ran, while the reply reads
/// exactly like a complete analysis (measured on a 22-package pnpm monorepo: `config = null,
/// configWarnings = []`).
///
/// `config_path` selects which of the two ways into that trap the caller hit, and only changes the
/// opener and the remedy — never the shared middle, so the two variants read as one message:
/// - `None` — no `zzop.config.jsonc` at all (remedy: create one with `{"trees": "auto"}`).
/// - `Some(p)` — a config EXISTS at `p` and declares no `trees` (remedy: add `"trees": "auto"` to
///   it). This second case is why the trigger is the resolved tree count rather than config absence:
///   a config carrying only `{"rules": {...}}` lands in the identical trap while its author
///   reasonably believes having a config means the analysis was configured, and — unlike
///   `trees: "auto"` — there is no expansion, hence no expansion report, hence pure silence.
///
/// DELIBERATELY SILENT for an explicit `trees: [...]` (the caller passes `Method::AnalyzeTrees`, so
/// this is never called): naming the tree set IS the author's answer to "which trees?", and zzop
/// must not second-guess a stated decision — a one-entry `trees` array is a legitimate choice
/// (analyze just this package), not an oversight. That case is not left unattended either: the
/// `trees: "auto"` path emits its own "resolved only one workspace package" warning above, which is
/// the only single-entry shape zzop itself produced rather than the author.
///
/// Every claim here is a structural fact this function actually observed, never a guess (the repo's
/// never-guess doctrine): the manifest label is the file that was really read, and the package count
/// is the output of the SAME `resolve_workspace_dirs` expansion `trees: "auto"` would perform, so
/// the number is exact and the suggested remedy is known to produce it.
///
/// Silent (returns `None`) when there is no manifest at all — an ordinary single-package repo must
/// never be nagged — and equally when the manifest resolves to fewer than 2 packages, because then
/// `{"trees": "auto"}` could not deliver the join either and the advice would be false.
pub fn single_tree_workspace_warning(
    base_dir: &Path,
    config_path: Option<&Path>,
) -> Option<String> {
    let (patterns, source) = read_workspace_manifest(base_dir)?;
    if patterns.is_empty() {
        return None;
    }
    let package_count = resolve_workspace_dirs(base_dir, &patterns).len();
    if package_count < 2 {
        return None;
    }
    let config_filename = crate::DEFAULT_CONFIG_FILENAME;
    let (opener, remedy) = match config_path {
        None => (
            format!(
                "no {config_filename} at {} — {source} is present there and resolves to \
                 {package_count} workspace packages, but this run analyzed the root as a SINGLE \
                 tree",
                base_dir.display()
            ),
            format!("Create a {config_filename} at that root containing {{\"trees\": \"auto\"}}"),
        ),
        // `base_dir` is named separately from the config path here: a config may point `roots` at a
        // subdirectory, so the analyzed tree is not necessarily the directory holding the manifest.
        Some(path) => (
            format!(
                "the config at {} declares no \"trees\" — {source} at {} resolves to \
                 {package_count} workspace packages, but this run analyzed a SINGLE tree",
                path.display(),
                base_dir.display()
            ),
            "Add \"trees\": \"auto\" to that config".to_string(),
        ),
    };
    Some(format!(
        "{opener}: the cross-layer join needs >= 2 trees with distinct sourceIds to fire, so it did \
         not run. {remedy} to analyze those {package_count} packages as separate trees."
    ))
}
