//! tsconfig `paths`/`baseUrl` alias collection.
//!
//! `tsconfig_scan` is this engine's filesystem-touching collection pass; the pure resolver logic it
//! feeds lives in `zzop_parser_typescript::resolve` instead (no I/O there).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use super::manifest::{
    is_tsconfig_json_path, join_and_normalize, package_json_dir, strip_jsonc_comments,
};

mod emission;
#[cfg(test)]
mod tests;

pub(crate) use emission::tsconfig_preserves_type_imports;

/// One tsconfig file's text as a JSON value, tolerating the two JSONC liberties `tsconfig.json` is
/// written with in practice (comments, trailing commas). `None` on any parse failure — every caller
/// degrades by skipping the file rather than guessing at its contents.
fn parse_jsonc(text: &str) -> Option<serde_json::Value> {
    static TRAILING_COMMA: OnceLock<Regex> = OnceLock::new();
    let stripped = strip_jsonc_comments(text);
    let cleaned = TRAILING_COMMA
        .get_or_init(|| Regex::new(r",(\s*[}\]])").unwrap())
        .replace_all(&stripped, "$1");
    serde_json::from_str(&cleaned).ok()
}

/// One tsconfig file's own (unmerged, un-`extends`-resolved) `compilerOptions.baseUrl`/`paths` plus the
/// two links out of it, as written: `extends` (inheritance) and `references` (project references).
struct RawTsconfig {
    base_url: Option<String>,
    paths: BTreeMap<String, Vec<String>>,
    extends: Option<String>,
    references: Vec<String>,
}

/// What one tsconfig contributes once its single local `extends` level is merged in. `base_url` is still
/// as-written, i.e. relative to that config file's own directory.
struct Effective {
    base_url: Option<String>,
    paths: BTreeMap<String, Vec<String>>,
    references: Vec<String>,
}

/// Parses one tsconfig file's text (JSONC-tolerant) into its own `compilerOptions.baseUrl`/`paths`/
/// `extends`/`references`, un-merged. `None` on any parse failure — callers degrade by skipping the file.
fn parse_raw_tsconfig(text: &str) -> Option<RawTsconfig> {
    let value = parse_jsonc(text)?;
    let extends = value
        .get("extends")
        .and_then(|v| v.as_str())
        .map(String::from);
    // Declaration order is preserved: it is the tie-breaker when two referenced projects claim the same
    // alias, and it is a byte-order property of the file, so the merge stays stable run to run.
    let references = value
        .get("references")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|r| r.get("path").and_then(|p| p.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let compiler_options = value.get("compilerOptions");
    let base_url = compiler_options
        .and_then(|c| c.get("baseUrl"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let mut paths = BTreeMap::new();
    if let Some(map) = compiler_options
        .and_then(|c| c.get("paths"))
        .and_then(|v| v.as_object())
    {
        for (pattern, targets) in map {
            if let Some(arr) = targets.as_array() {
                let targets: Vec<String> = arr
                    .iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect();
                if !targets.is_empty() {
                    paths.insert(pattern.clone(), targets);
                }
            }
        }
    }
    Some(RawTsconfig {
        base_url,
        paths,
        extends,
        references,
    })
}

/// Reads and parses the tsconfig at `rel`, merging one level of local relative `extends`
/// (parent-fills-gaps: the child's `paths` keys win, `baseUrl` is the child's if set else the parent's).
/// A second-level or non-local `extends` is left unresolved.
fn load_effective(root: &Path, rel: &str) -> Option<Effective> {
    let text = fs::read_to_string(root.join(rel)).ok()?;
    let raw = parse_raw_tsconfig(&text)?;
    let dir = package_json_dir(rel);
    let mut paths = raw.paths;
    let mut base_url = raw.base_url;
    if let Some(extends) = &raw.extends {
        if extends.starts_with("./") || extends.starts_with("../") {
            let mut parent_rel = join_and_normalize(dir, extends);
            if !parent_rel.ends_with(".json") {
                parent_rel.push_str(".json");
            }
            if let Ok(parent_text) = fs::read_to_string(root.join(&parent_rel)) {
                if let Some(parent_raw) = parse_raw_tsconfig(&parent_text) {
                    // `parent_raw.extends` (a 2nd extends level) is intentionally not chased further.
                    let mut merged = parent_raw.paths;
                    merged.extend(paths);
                    paths = merged;
                    if base_url.is_none() {
                        base_url = parent_raw.base_url;
                    }
                }
            }
        }
    }
    Some(Effective {
        base_url,
        paths,
        references: raw.references,
    })
}

/// The tsconfig file a `references[].path` entry names. TS accepts either the config file itself or a
/// DIRECTORY holding a `tsconfig.json` — a different rule from `extends`, which appends `.json` to an
/// extensionless path. `None` for an absolute path or one that climbs out of the analysis root: this scan
/// only ever reads inside the tree it was handed.
fn reference_target(dir: &str, path: &str) -> Option<String> {
    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return None; // drive-absolute, e.g. `C:/proj/tsconfig.json`
    }
    let joined = join_and_normalize(dir, path);
    if joined.is_empty() || joined == ".." || joined.starts_with("../") {
        return None;
    }
    Some(if joined.ends_with(".json") {
        joined
    } else {
        format!("{joined}/tsconfig.json")
    })
}

/// The root-relative POSIX directory a config's `paths` targets are written against: its own `baseUrl`
/// when it declares one, else the directory the config file sits in (TS 4.4+ resolves `paths` relative to
/// the config itself when `baseUrl` is absent).
fn effective_base(rel: &str, cfg: &Effective) -> String {
    let dir = package_json_dir(rel);
    match &cfg.base_url {
        Some(b) => join_and_normalize(dir, b),
        None => dir.to_string(),
    }
}

/// Rewrites a `paths` target written against `from_base` into one written against `to_base`, so several
/// configs' targets can share the single `base_url` one merged `TsconfigPaths` entry carries. Both bases
/// are root-relative POSIX directories (`""` = analysis root); the `*` placeholder rides along as an
/// ordinary path character. `None` when the rewrite collapses to nothing to point at.
fn rebase_target(target: &str, from_base: &str, to_base: &str) -> Option<String> {
    if from_base == to_base {
        return Some(target.to_string());
    }
    let absolute = join_and_normalize(from_base, target);
    let to: Vec<&str> = to_base.split('/').filter(|s| !s.is_empty()).collect();
    let abs: Vec<&str> = absolute.split('/').filter(|s| !s.is_empty()).collect();
    let common = to.iter().zip(&abs).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; to.len() - common];
    out.extend(&abs[common..]);
    if out.is_empty() {
        return None;
    }
    Some(out.join("/"))
}

/// Collects `compilerOptions.baseUrl`/`paths` from every `tsconfig.json` found during the same manifest
/// walk `package_json_entries` uses, keyed by the tsconfig's own directory (the directory a TypeScript
/// file's nearest ancestor tsconfig governs, per `zzop_parser_typescript::resolve::governing_tsconfig`).
///
/// Two links out of a config are followed, each exactly one level and each only inside the analysis root:
///
/// * `extends` — inheritance, merged parent-fills-gaps (see [`load_effective`]).
/// * `references` — project references. HEURISTIC, and deliberately not what `tsc` does: a reference is a
///   BUILD relationship, and a referenced config's `paths` govern the files THAT config includes, not the
///   referencing project's. zzop keys path-mapping by directory and never evaluates `include`/`files`
///   globs, so it cannot make that distinction. Following references anyway is right for the shape that
///   motivates it — the `npm create vite@latest` scaffold, where the tree's `tsconfig.json` is a
///   `"files": []` solution file owning no compilerOptions and every source file is included by the
///   referenced `tsconfig.app.json`. It is WRONG where a solution file references projects in sibling
///   directories with genuinely different mappings for the same alias: zzop applies one merged map to
///   every file under the solution file's directory, so files governed by project B can resolve an alias
///   through project A's target. The conflict rule below makes that outcome deterministic, not correct.
///
/// Alias precedence, first writer wins: the config's own `paths`, then its `extends` parent's, then each
/// referenced project's in `references` declaration order. `baseUrl` likewise — own, else `extends`
/// parent's, else the first reference declaring one, else the config's own directory. Because one entry
/// carries a single `base_url`, each reference's targets are rewritten into it by [`rebase_target`].
///
/// A directory whose merged tsconfig declares neither `baseUrl` nor `paths` anywhere in that chain is not
/// registered, so `governing_tsconfig`'s ancestor walk continues past it. Degrades gracefully on every
/// failure mode — never panics.
pub(crate) fn tsconfig_scan(
    root: &Path,
    node_paths: impl Iterator<Item = String>,
) -> BTreeMap<String, zzop_parser_typescript::TsconfigPaths> {
    let mut result = BTreeMap::new();
    for rel in node_paths.filter(|p| is_tsconfig_json_path(p)) {
        let Some(own) = load_effective(root, &rel) else {
            continue;
        };
        let dir = package_json_dir(&rel);

        // Referenced projects, in declaration order, self-reference and out-of-tree paths dropped.
        let refs: Vec<(String, Effective)> = own
            .references
            .iter()
            .filter_map(|p| reference_target(dir, p))
            .filter(|target| *target != rel)
            .filter_map(|target| load_effective(root, &target).map(|cfg| (target, cfg)))
            .collect();

        let contributes = |cfg: &Effective| !cfg.paths.is_empty() || cfg.base_url.is_some();
        if !contributes(&own) && !refs.iter().any(|(_, cfg)| contributes(cfg)) {
            continue;
        }

        let base_url = match &own.base_url {
            Some(b) => join_and_normalize(dir, b),
            None => refs
                .iter()
                .find(|(_, cfg)| cfg.base_url.is_some())
                .map(|(target, cfg)| effective_base(target, cfg))
                .unwrap_or_else(|| dir.to_string()),
        };

        // The entry's OWN targets need the same rewrite the references get, and for the same reason: when
        // this config declares no `baseUrl`, the `base_url` above may have been ADOPTED from a reference,
        // and these targets were written against this config's own frame rather than that one. Skipping
        // this was a regression the day `references` started being followed — before it, an entry with no
        // `baseUrl` of its own always kept `dir`, so its targets were already in the frame they were
        // written against and moving them verbatim was correct. `rebase_target` is a no-op when the two
        // frames are equal, which is every config that declares its own `baseUrl` and every one whose
        // references declare none, so the common case is untouched.
        let own_base = effective_base(&rel, &own);
        let mut paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (pattern, targets) in &own.paths {
            let rebased: Vec<String> = targets
                .iter()
                .filter_map(|t| rebase_target(t, &own_base, &base_url))
                .collect();
            if !rebased.is_empty() {
                paths.insert(pattern.clone(), rebased);
            }
        }
        for (target, cfg) in &refs {
            let from = effective_base(target, cfg);
            for (pattern, targets) in &cfg.paths {
                // The config's own (and `extends`-inherited) keys are already in the map, as are those of
                // every earlier reference — so `contains_key` IS the first-writer-wins rule.
                if paths.contains_key(pattern) {
                    continue;
                }
                let rebased: Vec<String> = targets
                    .iter()
                    .filter_map(|t| rebase_target(t, &from, &base_url))
                    .collect();
                if !rebased.is_empty() {
                    paths.insert(pattern.clone(), rebased);
                }
            }
        }

        result.insert(
            dir.to_string(),
            zzop_parser_typescript::TsconfigPaths { base_url, paths },
        );
    }
    result
}
