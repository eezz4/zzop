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
mod tests {
    //! Covers `classify_path`'s four FSD layers, `module_of`/`module_root`'s slice/base/top-level/external
    //! resolution, `top_subdir`, `dir_for`, `is_upward_import`'s exemptions, `is_external`, `round`, and
    //! `is_index_barrel` — all against `ScoresConfig::default()`.
    use super::*;

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn classify_path_l1_entry_prefix() {
        let c = cfg();
        assert_eq!(
            classify_path(&c, "pages/home/Index.tsx"),
            PathClass {
                layer: 1,
                slice: None
            }
        );
        assert_eq!(
            classify_path(&c, "api/users.ts"),
            PathClass {
                layer: 1,
                slice: None
            }
        );
    }

    #[test]
    fn classify_path_l1_top_level_no_slash() {
        let c = cfg();
        assert_eq!(
            classify_path(&c, "index.ts"),
            PathClass {
                layer: 1,
                slice: None
            }
        );
    }

    #[test]
    fn classify_path_l2_slice_carries_slice_id() {
        let c = cfg();
        assert_eq!(
            classify_path(&c, "features/auth/model.ts"),
            PathClass {
                layer: 2,
                slice: Some("features/auth".to_string())
            }
        );
        assert_eq!(
            classify_path(&c, "domains/billing/api.ts"),
            PathClass {
                layer: 2,
                slice: Some("domains/billing".to_string())
            }
        );
    }

    #[test]
    fn classify_path_l3_shared_prefix() {
        let c = cfg();
        assert_eq!(
            classify_path(&c, "core/util.ts"),
            PathClass {
                layer: 3,
                slice: None
            }
        );
        assert_eq!(
            classify_path(&c, "ui/Button.tsx"),
            PathClass {
                layer: 3,
                slice: None
            }
        );
    }

    #[test]
    fn classify_path_l4_base_relative_or_unrecognized() {
        let c = cfg();
        assert_eq!(
            classify_path(&c, "../sibling/x.ts"),
            PathClass {
                layer: 4,
                slice: None
            }
        );
        assert_eq!(
            classify_path(&c, "src/base/thing/x.ts"),
            PathClass {
                layer: 4,
                slice: None
            }
        );
        assert_eq!(
            classify_path(&c, "app/main.ts"),
            PathClass {
                layer: 4,
                slice: None
            }
        );
    }

    #[test]
    fn module_of_returns_slice_for_l2_path() {
        assert_eq!(
            module_of(&cfg(), "features/auth/model.ts"),
            Some("features/auth".to_string())
        );
    }

    #[test]
    fn module_of_returns_base_module() {
        assert_eq!(
            module_of(&cfg(), "src/base/thing/x.ts"),
            Some("base/thing".to_string())
        );
    }

    #[test]
    fn module_of_falls_back_to_top_level_dir() {
        assert_eq!(module_of(&cfg(), "app/main.ts"), Some("app".to_string()));
    }

    #[test]
    fn module_of_null_for_external_specifiers() {
        assert_eq!(module_of(&cfg(), "react"), None);
        assert_eq!(module_of(&cfg(), "@scope/pkg"), None);
    }

    #[test]
    fn module_root_returns_slice_for_l2_path() {
        assert_eq!(
            module_root(&cfg(), "features/auth/model.ts"),
            Some("features/auth".to_string())
        );
    }

    #[test]
    fn module_root_returns_base_module() {
        assert_eq!(
            module_root(&cfg(), "src/base/thing/x.ts"),
            Some("base/thing".to_string())
        );
    }

    #[test]
    fn module_root_no_top_level_fallback() {
        assert_eq!(module_root(&cfg(), "app/main.ts"), None);
    }

    #[test]
    fn module_root_null_for_external_specifiers() {
        assert_eq!(module_root(&cfg(), "react"), None);
    }

    #[test]
    fn top_subdir_returns_first_subdir_under_module_root() {
        assert_eq!(
            top_subdir("features/auth/widgets/Form.tsx", "features/auth"),
            Some("widgets".to_string())
        );
    }

    #[test]
    fn top_subdir_null_when_tail_is_a_file() {
        assert_eq!(top_subdir("features/auth/model.ts", "features/auth"), None);
    }

    #[test]
    fn top_subdir_null_when_module_root_absent() {
        assert_eq!(top_subdir("features/other/x.ts", "features/auth"), None);
    }

    #[test]
    fn dir_for_returns_directory_portion() {
        assert_eq!(dir_for("a/b/c.ts"), "a/b");
    }

    #[test]
    fn dir_for_empty_when_no_slash() {
        assert_eq!(dir_for("x.ts"), "");
    }

    #[test]
    fn is_upward_import_true_child_imports_ancestor_same_module() {
        // from dir "features/auth/widgets/inner" -> to dir "features/auth/widgets" (ancestor)
        // toLast "widgets" not shared, not an index barrel, topSubdir(to, "features/auth") = "widgets" != null
        assert!(is_upward_import(
            &cfg(),
            "features/auth/widgets/inner/Form.tsx",
            "features/auth/widgets/helper.ts"
        ));
    }

    #[test]
    fn is_upward_import_false_sibling_import() {
        assert!(!is_upward_import(
            &cfg(),
            "features/auth/ui/Form.tsx",
            "features/auth/utils/format.ts"
        ));
    }

    #[test]
    fn is_upward_import_false_same_directory() {
        assert!(!is_upward_import(
            &cfg(),
            "features/auth/ui/Form.tsx",
            "features/auth/ui/Button.tsx"
        ));
    }

    #[test]
    fn is_upward_import_false_ancestor_is_shared_dir() {
        assert!(!is_upward_import(
            &cfg(),
            "features/auth/utils/inner/x.ts",
            "features/auth/utils/helper.ts"
        ));
    }

    #[test]
    fn is_upward_import_false_importing_index_barrel() {
        assert!(!is_upward_import(
            &cfg(),
            "features/auth/widgets/inner/Form.tsx",
            "features/auth/widgets/index.ts"
        ));
    }

    #[test]
    fn is_external_true_for_scoped_and_bare_specifiers() {
        assert!(is_external("@scope/pkg"));
        assert!(is_external("react"));
    }

    #[test]
    fn is_external_false_for_relative_or_pathed_specifiers() {
        assert!(!is_external("./local"));
        assert!(!is_external("a/b"));
    }

    #[test]
    fn round_rounds_half_up() {
        assert_eq!(round(1.5), 2.0);
        assert_eq!(round(1.4), 1.0);
        assert_eq!(round(2.5), 3.0);
    }

    #[test]
    fn is_index_barrel_true_for_known_extensions() {
        assert!(is_index_barrel("index.ts"));
        assert!(is_index_barrel("index.tsx"));
        assert!(is_index_barrel("index.js"));
        assert!(is_index_barrel("index.mjs"));
        assert!(is_index_barrel("index.cjs"));
    }

    #[test]
    fn is_index_barrel_false_for_non_barrel_basenames() {
        assert!(!is_index_barrel("model.ts"));
        assert!(!is_index_barrel("indexs.ts"));
        assert!(!is_index_barrel("index.css"));
    }
}
