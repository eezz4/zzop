//! `trees: "auto"` workspace expansion — the port of `packages/cli/lib/workspaces.js`'s
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
//! `workspaces/glob.rs`; this root keeps the census-pinned constants and the single public entry point.

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

    let (patterns, source): (Vec<String>, &str) = if let Some(p) =
        read_pnpm_workspace_packages(base_dir)
    {
        (p, "pnpm-workspace.yaml")
    } else if let Some(p) = read_npm_workspace_packages(base_dir) {
        (p, "package.json \"workspaces\"")
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
