//! Public API — ratio of cross-module imports that bypass another module's index barrel (deep-path imports). A
//! module's index/barrel file is its public contract; reaching past it into internal files couples callers to
//! implementation details that are free to change.

use super::config::ScoresConfig;
use super::shared::{is_external, is_index_barrel, module_root, round};
use super::types::{DeepImport, PublicApiScore};
use zpz_core::DepGraph;

/// Caps the returned deep-import list (not the score).
const MAX_VIOLATIONS_LISTED: usize = 100;
/// `"src/".len()`.
const SRC_PREFIX_LEN: usize = 4;

pub fn compute_public_api(dep: &DepGraph, cfg: &ScoresConfig) -> PublicApiScore {
    let mut deep: Vec<DeepImport> = Vec::new();
    let mut total: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible order.
    let mut froms: Vec<&String> = dep.keys().collect();
    froms.sort();

    for from in froms {
        let fm = module_root(cfg, from);
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            let tm = match module_root(cfg, to) {
                Some(m) => m,
                None => continue,
            };
            if fm.as_deref() == Some(tm.as_str()) {
                continue;
            }
            total += 1;
            if !is_root_import(to, &tm) {
                deep.push(DeepImport {
                    from: from.clone(),
                    to: to.clone(),
                    to_module: tm,
                });
            }
        }
    }

    deep.sort_by(|a, b| a.to_module.cmp(&b.to_module));

    let score = if total == 0 {
        100.0
    } else {
        (100.0 - (deep.len() as f64 / total as f64) * 100.0).max(0.0)
    };

    deep.truncate(MAX_VIOLATIONS_LISTED);

    PublicApiScore {
        score: round(score),
        total_cross_module_imports: total,
        deep_imports: deep,
    }
}

/// True when `to` resolves to `module`'s barrel/index or a top-level file directly under it (its public surface),
/// false when it reaches into a subdirectory (a deep import).
fn is_root_import(to: &str, module: &str) -> bool {
    let stripped = strip_leading_dotdot(to);
    let prefix = format!("{}/", module);
    let mut after_root = stripped.strip_prefix(prefix.as_str()).unwrap_or("");
    if let Some(rest) = after_root.strip_prefix("src/") {
        debug_assert_eq!("src/".len(), SRC_PREFIX_LEN);
        after_root = rest;
    }
    is_index_barrel(after_root) || !after_root.contains('/')
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
    //! Covers the empty-graph baseline, same-module imports not counting as cross-module, barrel/root-file
    //! imports not being flagged as deep, a deep import bypassing the barrel being flagged, and a mixed
    //! case with one deep import among three cross-module imports.
    use super::*;

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn cfg() -> ScoresConfig {
        ScoresConfig::default()
    }

    #[test]
    fn empty_graph_score_100() {
        let r = compute_public_api(&DepGraph::new(), &cfg());
        assert_eq!(r.score, 100.0);
        assert_eq!(r.total_cross_module_imports, 0);
        assert!(r.deep_imports.is_empty());
    }

    #[test]
    fn same_module_imports_are_not_cross_module_score_100() {
        let d = dep(&[
            ("features/auth/login.ts", &["features/auth/util.ts"]),
            ("features/auth/util.ts", &[]),
        ]);
        let r = compute_public_api(&d, &cfg());
        assert_eq!(r.total_cross_module_imports, 0);
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn cross_module_import_via_barrel_or_root_file_is_not_deep() {
        // afterRoot = "index.ts" (barrel) and "login.ts" (no slash) -> both root imports
        let d = dep(&[(
            "features/cart/cart.ts",
            &["features/auth/index.ts", "features/auth/login.ts"],
        )]);
        let r = compute_public_api(&d, &cfg());
        assert_eq!(r.total_cross_module_imports, 2);
        assert!(r.deep_imports.is_empty());
        assert_eq!(r.score, 100.0);
    }

    #[test]
    fn deep_cross_module_import_bypasses_barrel_is_flagged() {
        // afterRoot = "ui/Btn.ts" -> has slash, not a barrel -> deep
        let d = dep(&[("features/cart/cart.ts", &["features/auth/ui/Btn.ts"])]);
        let r = compute_public_api(&d, &cfg());
        assert_eq!(r.total_cross_module_imports, 1);
        assert_eq!(r.deep_imports.len(), 1);
        assert_eq!(r.deep_imports[0].from, "features/cart/cart.ts");
        assert_eq!(r.deep_imports[0].to, "features/auth/ui/Btn.ts");
        assert_eq!(r.deep_imports[0].to_module, "features/auth");
        assert_eq!(r.score, 0.0);
    }

    #[test]
    fn one_deep_of_three_cross_module_imports_score_67() {
        let d = dep(&[(
            "features/cart/cart.ts",
            &[
                "features/auth/index.ts",  // root (barrel)
                "features/auth/ui/Btn.ts", // deep
                "features/auth/login.ts",  // root (no slash)
                "react",                   // external, skipped
            ],
        )]);
        let r = compute_public_api(&d, &cfg());
        assert_eq!(r.total_cross_module_imports, 3);
        assert_eq!(r.deep_imports.len(), 1);
        // 100 - (1/3)*100 = 66.67 -> 67
        assert_eq!(r.score, 67.0);
    }
}
