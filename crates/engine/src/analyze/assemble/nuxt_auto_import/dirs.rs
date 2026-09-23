//! WHICH directories an app auto-imports, and which files inside them the scanner would pick up.
//!
//! Two sources, deliberately kept apart (§24): the framework's own convention
//! ([`super::NUXT_DEFAULT_AUTO_IMPORT_DIRS`] — no file in the tree states it) and the app's own
//! `nuxt.config.*` declaration ([`declared_dirs`] — nothing else in the tree states it either, which is
//! exactly why it has to be read rather than assumed). See the parent module's doc for the measured
//! split between them.

use std::collections::HashSet;
use std::path::Path;

use super::{nuxt_app_dir, AUTO_IMPORT_EXTS, NUXT_DEFAULT_AUTO_IMPORT_DIRS};

/// One auto-import directory of one app: where it sits relative to the app root, and whether files
/// BELOW its first level count. Nuxt's own default scan is `<dir>/*.{ts,js,mjs,mts}` plus
/// `<dir>/*/index.{ts,js,mjs,mts}` — one level, or a directory with an index; a `**` in an
/// `imports.dirs` entry is what widens that to every depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AutoImportDir {
    /// App-relative directory, no leading `./` and no trailing separator.
    pub(super) name: String,
    pub(super) recursive: bool,
}

/// The auto-import directories of ONE app: the framework's two defaults, plus whatever this app's own
/// `nuxt.config.*` declares. A declared entry that repeats a default is folded in rather than doubled,
/// and a declared `./composables/**` upgrades the default `composables` to recursive — the config is
/// additive to the convention, never a replacement for it.
pub(super) fn auto_import_dirs(
    root: &Path,
    ts_paths: &HashSet<String>,
    app_dir: &str,
) -> Vec<AutoImportDir> {
    let mut dirs = default_dirs();
    for declared in declared_dirs(root, ts_paths, app_dir) {
        match dirs.iter_mut().find(|d| d.name == declared.name) {
            Some(existing) => existing.recursive |= declared.recursive,
            None => dirs.push(declared),
        }
    }
    dirs
}

/// The convention half, on its own — what an app that declares nothing gets.
pub(super) fn default_dirs() -> Vec<AutoImportDir> {
    NUXT_DEFAULT_AUTO_IMPORT_DIRS
        .iter()
        .map(|d| AutoImportDir {
            name: (*d).to_string(),
            recursive: false,
        })
        .collect()
}

/// The `imports.dirs` entries this app's own `nuxt.config.*` declares, normalized. An entry that cannot
/// be anchored inside the app (absolute, or escaping through `..`) is dropped rather than guessed at;
/// so is a config file that will not read. Deterministic: the config paths are sorted, since two config
/// files in one directory would otherwise merge in hash order.
pub(super) fn declared_dirs(
    root: &Path,
    ts_paths: &HashSet<String>,
    app_dir: &str,
) -> Vec<AutoImportDir> {
    let mut configs: Vec<&String> = ts_paths
        .iter()
        .filter(|p| nuxt_app_dir(p) == Some(app_dir))
        .collect();
    configs.sort();
    let mut out: Vec<AutoImportDir> = Vec::new();
    for rel in configs {
        // Same reason as its sibling in `nuxt_auto_import.rs`: `ts_paths` carries refused files.
        let Some(text) = crate::analyze::read_for_parse(root, rel.as_str()) else {
            continue;
        };
        for entry in zzop_parser_typescript::parse_nuxt_imports_dirs(rel, &text) {
            if let Some(dir) = normalize_declared_dir(&entry) {
                if !out.contains(&dir) {
                    out.push(dir);
                }
            }
        }
    }
    out
}

/// `"./utils/**"` -> recursive `utils`; `"./lib"` -> non-recursive `lib`; `"helpers/*"` -> non-recursive
/// `helpers`. `None` for anything that cannot be anchored under the app root.
pub(super) fn normalize_declared_dir(entry: &str) -> Option<AutoImportDir> {
    let trimmed = entry.trim().trim_start_matches("./").trim_end_matches('/');
    let (name, recursive) = match trimmed.strip_suffix("/**") {
        Some(base) => (base, true),
        None => (trimmed.strip_suffix("/*").unwrap_or(trimmed), false),
    };
    let name = name.trim_end_matches('/');
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with("..")
        || name.contains("../")
        || name.contains('*')
    {
        return None;
    }
    Some(AutoImportDir {
        name: name.to_string(),
        recursive,
    })
}

/// Is `rel` an auto-import CANDIDATE of the app rooted at `app_dir`, given that app's directories?
pub(super) fn is_candidate(rel: &str, app_dir: &str, dirs: &[AutoImportDir]) -> bool {
    let Some(rest) = strip_app_dir(rel, app_dir) else {
        return false;
    };
    if !has_auto_import_ext(rest) {
        return false;
    }
    dirs.iter().any(|d| {
        let Some(tail) = rest
            .strip_prefix(d.name.as_str())
            .and_then(|t| t.strip_prefix('/'))
        else {
            return false;
        };
        match tail.split_once('/') {
            None => true,
            Some(_) if d.recursive => true,
            // Non-recursive: one extra level is admitted only as `<sub>/index.*`, Nuxt's own
            // directory-with-an-index shape.
            Some((_sub, leaf)) => {
                !leaf.contains('/') && leaf.rsplit_once('.').is_some_and(|(s, _)| s == "index")
            }
        }
    })
}

fn strip_app_dir<'a>(rel: &'a str, app_dir: &str) -> Option<&'a str> {
    if app_dir.is_empty() {
        return Some(rel);
    }
    rel.strip_prefix(app_dir)?.strip_prefix('/')
}

fn has_auto_import_ext(rel: &str) -> bool {
    rel.rsplit_once('.')
        .is_some_and(|(_, ext)| AUTO_IMPORT_EXTS.contains(&ext))
}
