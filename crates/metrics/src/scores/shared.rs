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
/// (`index.ts|tsx|js|jsx|mjs|cjs`), so an `index.js` barrel in a JS/CJS repo is not misread as an upward
/// import (a TS-only `index.ts` check would silently mis-score JS repos).
///
/// ONE caller since 2026-08-12: [`is_upward_import`] (hierarchy scoring), which applies it to a path's
/// BASENAME and therefore honors a nested `pkg/sub/index.ts`. `public_api` used to name it too, on a
/// value that was the whole post-module remainder rather than a basename — a call that could never
/// change an answer, since this regex is fully anchored and the sibling `!contains('/')` test subsumed
/// every string it accepts. That dead call is gone; `public_api::is_root_import`'s doc carries what its
/// removal did and did not settle, including the still-open disagreement about nested barrels and the
/// fact that the pattern below knows no `__init__.py`/`mod.rs`.
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
    if cfg.feature_sliced_design.entry_re.is_match(p) {
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
    if let Some(caps) = cfg.feature_sliced_design.slice_re.captures(p) {
        return PathClass {
            layer: 2,
            slice: Some(format!("{}/{}", &caps[1], &caps[2])),
        };
    }
    if cfg.feature_sliced_design.shared_re.is_match(p) {
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
    if let Some(caps) = cfg.feature_sliced_design.slice_re.captures(p) {
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

/// The module a path's PUBLIC SURFACE belongs to — `module_of`'s sibling, used only by `public_api`.
///
/// Identical to [`module_of`] including the top-segment fallback. That fallback was missing here until
/// 2026-08-08, and its absence silently emptied the metric: without it a path only had a module when it
/// matched a declared FSD `sliceContainer` (`features/auth/...`) or a `baseDir`, so a perfectly ordinary
/// `src/api/user.ts` / `internal/service/x.go` / `crates/metrics/src/lib.rs` resolved to `None` and its
/// imports were skipped BEFORE `total_cross_module_imports` counted them. Any tree not laid out as
/// Feature-Sliced Design therefore reached the `total == 0 -> score 100` guard with an empty denominator
/// and published a perfect barrel-discipline score — including plain TypeScript repos, which is the
/// convention the metric was written for. Its five siblings (`hierarchy`, `mainSequence`, `modularity`,
/// `sdp`, `siblingCross`) all call `module_of` and always had the fallback, so `publicApi` was the one
/// metric of the six blind to non-FSD layouts.
///
/// This is now a thin alias rather than a copy: keeping two nearly-identical resolvers is what let them
/// drift apart in the first place, and `public_api` needs no different answer — it asks the same
/// question ("which module owns this path"), then decides separately whether the import reached that
/// module's barrel.
pub fn module_root(cfg: &ScoresConfig, p: &str) -> Option<String> {
    module_of(cfg, p)
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

/// The directory portion of a path ("" when there is no slash). Module-private: `is_upward_import` below
/// is the only caller, and every other score module reaches path structure through `module_of`/
/// `top_subdir`/`classify_path` instead.
fn dir_for(p: &str) -> &str {
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
    cfg.feature_sliced_design
        .config
        .base_dirs
        .iter()
        .any(|d| p.contains(&format!("/{}/", d)))
}

/// `/{baseDir}/{name}/` -> `{baseDir}/{name}`, else `None`.
fn base_module(cfg: &ScoresConfig, p: &str) -> Option<String> {
    cfg.feature_sliced_design
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
