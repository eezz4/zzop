//! Does this tree's TypeScript configuration EMIT type-only imports?
//!
//! Three `compilerOptions` keys answer yes — `verbatimModuleSyntax`, its predecessor
//! `preserveValueImports`, and `importsNotUsedAsValues: "preserve"`. Under any of them `tsc` leaves
//! `import { X } from './y'` in the output even when `X` is a type, so the module really is loaded and
//! a cycle closed by that edge is REAL. The export-side arm of `zzop_core::NoncycleCandidates` (a
//! value-spelled import of an `export type` / `export interface` reads as erased) is therefore turned
//! off wholesale for such a tree.
//!
//! Deliberately a SEPARATE read from `tsconfig_scan`'s: that scan registers nothing for a config
//! declaring neither `baseUrl` nor `paths`, and a config whose only content is
//! `verbatimModuleSyntax` is exactly that shape. Widening `tsconfig_scan`'s admission instead would
//! insert an empty entry that `governing_tsconfig`'s nearest-ancestor walk would stop at, shadowing a
//! real ancestor's aliases — a resolution change, to answer a question that is not about resolution.
//!
//! Scope, stated rather than left to be discovered: this is a TREE-level verdict (any config anywhere
//! in the tree turns the arm off for the whole tree — the safe direction, since it only ever KEEPS
//! findings), and it follows one level of local relative `extends` (the same link and the same
//! one-level limit `load_effective` takes) but does NOT chase `references`, whose own doc calls the
//! link a heuristic about build order rather than about which files a config governs.

use std::fs;
use std::path::Path;

use super::super::manifest::{is_tsconfig_json_path, join_and_normalize, package_json_dir};
use super::parse_jsonc;

/// True when ANY tsconfig in `node_paths` — or the one config it locally `extends` — keeps type
/// imports in the emitted output.
pub(crate) fn tsconfig_preserves_type_imports(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
) -> bool {
    node_paths
        .filter(|p| is_tsconfig_json_path(p))
        .any(|rel| config_preserves(root, &rel))
}

fn config_preserves(root: &Path, rel: &str) -> bool {
    let Some(value) = fs::read_to_string(root.join(rel))
        .ok()
        .and_then(|t| parse_jsonc(&t))
    else {
        return false;
    };
    if declares_preservation(&value) {
        return true;
    }
    let Some(extends) = value.get("extends").and_then(|v| v.as_str()) else {
        return false;
    };
    if !extends.starts_with("./") && !extends.starts_with("../") {
        return false;
    }
    let mut parent_rel = join_and_normalize(package_json_dir(rel), extends);
    if !parent_rel.ends_with(".json") {
        parent_rel.push_str(".json");
    }
    fs::read_to_string(root.join(&parent_rel))
        .ok()
        .and_then(|t| parse_jsonc(&t))
        .is_some_and(|v| declares_preservation(&v))
}

fn declares_preservation(value: &serde_json::Value) -> bool {
    let Some(options) = value.get("compilerOptions") else {
        return false;
    };
    let flag = |key: &str| options.get(key).and_then(|v| v.as_bool()).unwrap_or(false);
    flag("verbatimModuleSyntax")
        || flag("preserveValueImports")
        || options
            .get("importsNotUsedAsValues")
            .and_then(|v| v.as_str())
            == Some("preserve")
}

#[cfg(test)]
mod tests;
