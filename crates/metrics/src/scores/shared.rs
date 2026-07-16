//! Shared score utilities — path classification, external detection, and math helpers. Every function here
//! takes the config it needs explicitly (`&ScoresConfig`) instead of reading ambient module-level global state.

use super::config::ScoresConfig;

/// Result of `classify_path` — the FSD layer (1 = entry .. 4 = base/external) and, for an L2 path, its slice id
/// (e.g. "features/auth").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathClass {
    pub layer: u8,
    pub slice: Option<String>,
}

/// True when a basename is a module's barrel/index file — recognizes both ESM/TS and CommonJS/JS extensions
/// (`index.ts|tsx|js|jsx|mjs|cjs`). Used by public-API and hierarchy scoring so an `index.js` barrel in a JS/CJS
/// repo is not misread as a deep/upward import (a TS-only `index.ts` check would silently mis-score JS repos).
pub fn is_index_barrel(basename: &str) -> bool {
    static R: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    R.get_or_init(|| regex::Regex::new(r"^index\.(?:tsx?|jsx?|mjs|cjs)$").unwrap())
        .is_match(basename)
}

/// Classifies a path into Feature-Sliced Design layers (L1 entry -> L4 base/external).
pub fn classify_path(cfg: &ScoresConfig, p: &str) -> PathClass {
    if p.starts_with("../") || has_base_dir(cfg, p) {
        return PathClass {
            layer: 4,
            slice: None,
        };
    }
    if cfg.fsd.entry_re.is_match(p) {
        return PathClass {
            layer: 1,
            slice: None,
        };
    }
    if !p.contains('/') {
        return PathClass {
            layer: 1,
            slice: None,
        };
    }
    if let Some(caps) = cfg.fsd.slice_re.captures(p) {
        return PathClass {
            layer: 2,
            slice: Some(format!("{}/{}", &caps[1], &caps[2])),
        };
    }
    if cfg.fsd.shared_re.is_match(p) {
        return PathClass {
            layer: 3,
            slice: None,
        };
    }
    PathClass {
        layer: 4,
        slice: None,
    }
}

pub fn module_of(cfg: &ScoresConfig, p: &str) -> Option<String> {
    if is_external(p) {
        return None;
    }
    if let Some(caps) = cfg.fsd.slice_re.captures(p) {
        return Some(format!("{}/{}", &caps[1], &caps[2]));
    }
    if let Some(base) = base_module(cfg, p) {
        return Some(base);
    }
    let top = strip_leading_dotdot(p).split('/').next().unwrap_or("");
    if top.is_empty() || top.contains('.') {
        return None;
    }
    Some(top.to_string())
}

pub fn module_root(cfg: &ScoresConfig, p: &str) -> Option<String> {
    if is_external(p) {
        return None;
    }
    if let Some(caps) = cfg.fsd.slice_re.captures(p) {
        return Some(format!("{}/{}", &caps[1], &caps[2]));
    }
    base_module(cfg, p)
}

/// First path segment under `module_root_path`, or `None` when the tail is directly a file (contains a `.`) or
/// `module_root_path` is absent from `path`.
pub fn top_subdir(path: &str, module_root_path: &str) -> Option<String> {
    let stripped = strip_leading_dotdot(path);
    let needle = format!("{}/", module_root_path);
    let idx = stripped.find(needle.as_str())?;
    let tail = &stripped[idx + needle.len()..];
    let first = tail.split('/').next().unwrap_or("");
    if first.is_empty() || first.contains('.') {
        return None;
    }
    Some(first.to_string())
}

/// The directory portion of a path ("" when there is no slash).
pub fn dir_for(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) => &p[..i],
        None => "",
    }
}

pub fn is_upward_import(cfg: &ScoresConfig, from: &str, to: &str) -> bool {
    let from_dir = dir_for(from);
    let to_dir = dir_for(to);
    if from_dir == to_dir {
        return false;
    }
    if !format!("{}/", from_dir).starts_with(&format!("{}/", to_dir)) {
        return false;
    }
    let to_last = to_dir.rsplit('/').next().unwrap_or("");
    if cfg.hierarchy_shared_dirs.contains(to_last) {
        return false;
    }
    let to_base = to.rsplit('/').next().unwrap_or("");
    if is_index_barrel(to_base) {
        return false;
    }
    if let Some(fm) = module_of(cfg, from) {
        if top_subdir(to, &fm).is_none() {
            return false;
        }
    }
    true
}

pub fn is_external(p: &str) -> bool {
    p.starts_with('@') || (!p.starts_with('.') && !p.contains('/'))
}

/// Math.round semantics: rounds half away from zero. Scores are always non-negative, so this matches JS
/// `Math.round` (which rounds .5 toward +Infinity) exactly.
pub fn round(n: f64) -> f64 {
    n.round()
}

fn has_base_dir(cfg: &ScoresConfig, p: &str) -> bool {
    cfg.fsd
        .config
        .base_dirs
        .iter()
        .any(|d| p.contains(&format!("/{}/", d)))
}

/// `/{baseDir}/{name}/` -> `{baseDir}/{name}`, else `None`.
fn base_module(cfg: &ScoresConfig, p: &str) -> Option<String> {
    cfg.fsd
        .base_re
        .captures(p)
        .map(|c| format!("{}/{}", &c[1], &c[2]))
}

fn strip_leading_dotdot(p: &str) -> &str {
    let mut s = p;
    while let Some(rest) = s.strip_prefix("../") {
        s = rest;
    }
    s
}

#[cfg(test)]
mod tests;
