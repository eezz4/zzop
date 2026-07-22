//! Per-app-root bucketing for the S5/S7 egress-silence census.
//!
//! A monorepo tree (a config `{ root: "apps" }` holding many app packages) has ONE `package.json` per
//! app. The tree-wide census gates on tree-wide keyed http consumes, so a healthy sibling app's keyed
//! consumes lift the whole tree above the floor and MASK an app whose own FE<->BE contract is dark.
//! These helpers let the census gate per app-root instead, naming the dark app.
//!
//! All pure path/slice operations over data already in memory (the walked rel list and the extracted
//! `IoConsume` records) — no disk IO, no re-parse. The package.json filename predicate is deliberately
//! re-derived here rather than plumbed from `pipeline::manifest::is_package_json_path` (which is
//! `pub(super)` to `pipeline`); a filename regex is not a policy vocabulary, so it carries no
//! `check-policy-census.sh` weight.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;

fn package_json_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(^|/)package\.json$").unwrap())
}

/// The app-root directories in this tree = the parent dir of every `package.json` in `rels`, sorted +
/// deduped, PLUS the always-present `""` root bucket (the tree root / no-enclosing-package remainder).
/// A single-package tree yields `[""]` (a root `package.json`, or none at all), so the caller's gate
/// reduces to the exact pre-per-app tree-wide behavior. Deterministic via `BTreeSet`.
pub(crate) fn app_roots(rels: &[String]) -> Vec<String> {
    let mut roots: BTreeSet<String> = BTreeSet::new();
    roots.insert(String::new()); // the tree-root / remainder bucket always exists
    for rel in rels {
        if package_json_re().is_match(rel) {
            let dir = match rel.rfind('/') {
                Some(i) => rel[..i].to_string(),
                None => String::new(),
            };
            roots.insert(dir);
        }
    }
    roots.into_iter().collect()
}

/// The longest app-root in `roots` that contains `rel` — `r == ""` (matches everything), `rel == r`, or
/// `rel` under `r/`. Longest-prefix so a nested monorepo package wins over its parent; `""` is the
/// fallback owner for a file under no package. `roots` need not be sorted (longest wins regardless).
pub(crate) fn nearest_app_root<'a>(rel: &str, roots: &'a [String]) -> &'a str {
    let mut best: &'a str = "";
    for r in roots {
        let contains = r.is_empty()
            || rel == r
            || (rel.len() > r.len()
                && rel.as_bytes()[r.len()] == b'/'
                && rel.starts_with(r.as_str()));
        if contains && r.len() >= best.len() {
            best = r;
        }
    }
    best
}

/// Per-app-root count of KEYED `http` consumes (`kind == "http" && key.is_some()`), each attributed to
/// the [`nearest_app_root`] of its source `file`. Every root in `roots` appears in the map (0 if none),
/// so the caller can gate every bucket including empty ones.
pub(crate) fn keyed_http_by_root(
    io_consumes: &[zzop_core::IoConsume],
    roots: &[String],
) -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = roots.iter().map(|r| (r.clone(), 0)).collect();
    for c in io_consumes
        .iter()
        .filter(|c| c.kind == "http" && c.key.is_some())
    {
        let root = nearest_app_root(&c.file, roots);
        *counts.entry(root.to_string()).or_insert(0) += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::{app_roots, keyed_http_by_root, nearest_app_root};

    fn consume(file: &str, keyed: bool) -> zzop_core::IoConsume {
        zzop_core::IoConsume {
            kind: "http".to_string(),
            key: keyed.then(|| "GET /x".to_string()),
            file: file.to_string(),
            line: 1,
            raw: None,
            method: None,
            retry_configured: None,
            body: None,
            client: None,
        }
    }

    #[test]
    fn app_roots_are_package_json_parents_plus_root_sorted() {
        let rels = vec![
            "b-app/package.json".to_string(),
            "a-app/package.json".to_string(),
            "a-app/src/x.ts".to_string(), // not a package.json
            "a-app/nested/package.json".to_string(),
        ];
        // "" (root bucket) always present and sorts first.
        assert_eq!(app_roots(&rels), vec!["", "a-app", "a-app/nested", "b-app"]);
    }

    #[test]
    fn a_lone_root_package_json_collapses_to_the_single_root_bucket() {
        let rels = vec!["package.json".to_string(), "src/x.ts".to_string()];
        assert_eq!(app_roots(&rels), vec![""]);
    }

    #[test]
    fn nearest_app_root_picks_longest_prefix_with_a_boundary_guard() {
        let roots = vec![
            "".to_string(),
            "a-app".to_string(),
            "a-app/nested".to_string(),
            "b-app".to_string(),
        ];
        assert_eq!(nearest_app_root("a-app/src/x.ts", &roots), "a-app");
        assert_eq!(
            nearest_app_root("a-app/nested/y.ts", &roots),
            "a-app/nested"
        ); // longest wins
        assert_eq!(nearest_app_root("b-app/z.ts", &roots), "b-app");
        assert_eq!(nearest_app_root("shared/w.ts", &roots), ""); // no enclosing package
        assert_eq!(nearest_app_root("a-appX/x.ts", &roots), ""); // boundary: not under a-app
    }

    #[test]
    fn keyed_http_by_root_attributes_and_zero_fills() {
        let roots = vec!["".to_string(), "a-app".to_string(), "b-app".to_string()];
        let consumes = vec![
            consume("a-app/src/x.ts", true),
            consume("a-app/src/y.ts", true),
            consume("a-app/src/z.ts", false), // unkeyed -> not counted
            consume("shared/s.ts", true),     // -> "" bucket
        ];
        let by_root = keyed_http_by_root(&consumes, &roots);
        assert_eq!(by_root["a-app"], 2);
        assert_eq!(by_root["b-app"], 0);
        assert_eq!(by_root[""], 1);
    }
}
