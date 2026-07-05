//! Unreachable-code detector — closed "dead islands": files imported in-repo (fanIn > 0) yet not reachable from any
//! entrypoint. Pure over (nodes, dep), language-agnostic.
//!
//! `unreachable_findings` is the `"unreachable"` native-analysis Finding-shaping wrapper the engine calls
//! (moved here alongside the algorithm it shapes).

use std::collections::{HashSet, VecDeque};
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{DepGraph, FileNode, Finding, Severity};

#[derive(Debug, Clone, PartialEq)]
pub struct UnreachableFile {
    pub path: String,
    pub loc: u32,
    pub risk_score: f64,
    pub fan_in: u32,
}

/// Files with fanIn > 0 that no entrypoint reaches — closed dead islands. Ranked by loc desc, then risk, then path.
/// Entrypoints = conventional entry files + test files + every fanIn=0 file (false-positive-safe).
pub fn find_unreachable(nodes: &[FileNode], dep: &DepGraph, limit: usize) -> Vec<UnreachableFile> {
    let mut entries: HashSet<String> = HashSet::new();
    for n in nodes {
        if n.fan_in == 0 || is_entry_file(&n.path) || is_test_file(&n.path) {
            entries.insert(n.path.clone());
        }
    }
    let reachable = forward_closure(&entries, dep);

    let mut out: Vec<UnreachableFile> = nodes
        .iter()
        .filter(|n| n.fan_in > 0 && !reachable.contains(&n.path) && !is_test_file(&n.path))
        .map(|n| UnreachableFile {
            path: n.path.clone(),
            loc: n.loc,
            risk_score: n.risk_score,
            fan_in: n.fan_in,
        })
        .collect();
    out.sort_by(|a, b| {
        b.loc
            .cmp(&a.loc)
            .then(
                b.risk_score
                    .partial_cmp(&a.risk_score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(a.path.cmp(&b.path))
    });
    out.truncate(limit);
    out
}

/// One `Finding` per unreachable file (native analysis id `"unreachable"`, matching
/// `register_native_analyses`). No `limit` param — the engine's own call site passes `nodes.len()`.
pub fn unreachable_findings(nodes: &[FileNode], dep: &DepGraph) -> Vec<Finding> {
    find_unreachable(nodes, dep, nodes.len())
        .into_iter()
        .map(|u| Finding {
            rule_id: "unreachable".to_string(),
            severity: Severity::Info,
            file: u.path,
            line: 1,
            message: format!(
                "file has {} importer(s) in this tree but is unreachable from any entrypoint — its \
                 importers form a closed island nothing outside it can reach, so it's effectively dead \
                 despite having in-repo references. Delete the island, or wire it back to a real \
                 entrypoint if it should be reachable. Disable via rule config \
                 `disabled_rules: [\"unreachable\"]` if this island is reached by a mechanism this graph \
                 doesn't see (e.g. dynamic `require`, a plugin loader).",
                u.fan_in
            ),
            data: Some(serde_json::json!({ "loc": u.loc, "fan_in": u.fan_in })),
        })
        .collect()
}

/// All files reachable by following import edges forward from the entry set (BFS).
fn forward_closure(entries: &HashSet<String>, dep: &DepGraph) -> HashSet<String> {
    let mut seen: HashSet<String> = entries.clone();
    let mut queue: VecDeque<String> = entries.iter().cloned().collect();
    while let Some(cur) = queue.pop_front() {
        if let Some(nexts) = dep.get(&cur) {
            for next in nexts {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
    }
    seen
}

fn is_entry_file(path: &str) -> bool {
    entry_patterns().iter().any(|re| re.is_match(path))
}

/// Shared test-path predicate — also used by `mutating_route_no_auth` to skip route registrations in a
/// test/fixture file. Kept `pub(crate)`: same crate, one test-path convention.
pub(crate) fn is_test_file(path: &str) -> bool {
    test_patterns().iter().any(|re| re.is_match(path))
}

/// Files loaded directly by a dev tool or `tsc` rather than imported by app code — so `fan_in == 0` on them
/// is "not the kind of file the import graph would ever point at", not a "no importers" signal (e.g.
/// `.eslintrc.cjs`, `vite.config.ts`, `vite-env.d.ts`; see `dead_candidates.rs`). Shared here so
/// `dead_candidates`/`dead_exports` don't each duplicate the pattern list.
pub(crate) fn is_tool_entry_file(path: &str) -> bool {
    tool_entry_patterns().iter().any(|re| re.is_match(path))
}

/// Next.js App Router convention files — `app/**/{page,layout,route,error,not-found,…}.tsx` plus the
/// metadata routes (`sitemap`/`robots`/`manifest`/`opengraph-image`/…). The framework loads these by
/// filename, never through an import, so zero in-repo importers is expected — not a dead signal. Shared
/// here so `dead_candidates` and `dead_exports` reference ONE convention set and cannot drift (they did:
/// `dead_exports` carried this set while `dead_candidates` was missing it entirely).
pub(crate) fn framework_route_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(^|/)(page|layout|loading|error|global-error|not-found|template|default|route)\.(ts|tsx)$",
            r"(^|/)(sitemap|robots|manifest|opengraph-image|twitter-image|icon|apple-icon)\.(ts|tsx)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

fn tool_entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            // `<name>.config.*` — matches vite/jest/eslint/etc. config files by shape (requires a name
            // before `.config`, so bare `config.ts` does NOT match) rather than an enumerated tool list.
            r"(^|/)[^/]+\.config\.(js|ts|mjs|cjs|mts|cts)$",
            // Dotfile configs consumed directly by their tool's own resolver, never imported.
            r"(^|/)\.(eslintrc|prettierrc|babelrc|stylelintrc)(\.[^/]+)?$",
            // Ambient TypeScript declarations — type-only, consumed by tsc without an import edge.
            r"\.d\.ts$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

fn entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(^|/)index\.(t|j)sx?$",
            r"(^|/)main\.(t|j)sx?$",
            r"(^|/)main\.go$",
            r"(^|/)mod\.ts$",
            r"(^|/)App\.(t|j)sx?$",
            r"Page\.(t|j)sx?$",
            r"Route\.(t|j)sx?$",
            r"(^|/)routes?\.(t|j)sx?$",
            r"apiRoutes\.(t|j)sx?$",
            r"\.config\.(t|j)sx?$",
            r"(^|/)(server|app|bootstrap|worker|cli)\.(t|j)sx?$",
            r"(^|/)(cmd)/",
            r"Application\.java$",
            r"(^|/)Main\.java$",
            r"(^|/)(__main__|manage|wsgi|asgi)\.py$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

fn test_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"\.(test|spec)\.(t|j)sx?$",
            r"_test\.go$",
            r"(^|/)test_[^/]*\.py$",
            r"_test\.py$",
            r"Tests?\.java$",
            r"(^|/)Test[A-Z][^/]*\.java$",
            r"(^|/)(__tests__|__test__|tests?|spec)/",
            // Directories named for a test runner (or literally `testing`) are test surface by the same
            // "not deployed" reasoning as `__tests__`.
            r"(^|/)(e2e|cypress|playwright|testing)/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

#[cfg(test)]
mod tests {
    //! Covers dead-island detection: a closed cycle unreachable from any entrypoint is flagged, a library's
    //! public API entry keeps its helpers live, and files reachable from a test entrypoint are not flagged.
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn e2e_infra_directories_are_test_paths() {
        assert!(is_test_file(
            "packages/testing/playwright/scripts/import-data.mjs"
        ));
        assert!(is_test_file("app/e2e/flows/login.ts"));
        assert!(is_test_file("cypress/scripts/setup.js"));
        // Whole-segment match only — names merely containing "testing" are not test paths.
        assert!(!is_test_file("src/app-testing-utils/service.ts"));
    }

    fn node(path: &str, fan_in: u32, loc: u32) -> FileNode {
        FileNode {
            id: path.into(),
            path: path.into(),
            change_count: 0,
            churn: 0,
            last_modified: None,
            author_count: 1,
            loc,
            tag_counts: HashMap::new(),
            fan_in,
            fan_out: 0,
            total_connections: 0,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
        pairs
            .iter()
            .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn flags_closed_dead_island() {
        let d = dep(&[
            ("index.ts", &["live.ts"]),
            ("live.ts", &[]),
            ("dead1.ts", &["dead2.ts"]),
            ("dead2.ts", &["dead1.ts"]),
        ]);
        let nodes = vec![
            node("index.ts", 0, 10),
            node("live.ts", 1, 10),
            node("dead1.ts", 1, 40),
            node("dead2.ts", 1, 20),
        ];
        let dead: Vec<String> = find_unreachable(&nodes, &d, 30)
            .into_iter()
            .map(|n| n.path)
            .collect();
        assert_eq!(dead, vec!["dead1.ts".to_string(), "dead2.ts".to_string()]); // ranked by loc desc
    }

    #[test]
    fn does_not_flag_library_public_api() {
        let d = dep(&[("publicApi.ts", &["helper.ts"]), ("helper.ts", &[])]);
        let nodes = vec![node("publicApi.ts", 0, 10), node("helper.ts", 1, 10)];
        assert!(find_unreachable(&nodes, &d, 30).is_empty());
    }

    #[test]
    fn files_reachable_from_test_entry_not_flagged() {
        let d = dep(&[("x.test.ts", &["util.ts"]), ("util.ts", &[])]);
        let nodes = vec![node("x.test.ts", 0, 10), node("util.ts", 1, 10)];
        assert!(find_unreachable(&nodes, &d, 30).is_empty());
    }

    #[test]
    fn tool_entry_file_positives() {
        for path in [
            "vite.config.ts",
            "vite.config.js",
            "vitest.config.mts",
            "jest.config.cjs",
            "playwright.config.ts",
            "tailwind.config.js",
            "postcss.config.cjs",
            "rollup.config.mjs",
            "webpack.config.js",
            "next.config.mjs",
            "nuxt.config.ts",
            "svelte.config.js",
            "astro.config.mts",
            "eslint.config.cts",
            ".eslintrc.cjs",
            ".eslintrc.js",
            ".prettierrc.cjs",
            ".babelrc.js",
            ".stylelintrc.js",
            "vite-env.d.ts",
            "foo.d.ts",
            "packages/app/src/vite-env.d.ts",
        ] {
            assert!(
                is_tool_entry_file(path),
                "expected tool-entry match: {path}"
            );
        }
    }

    #[test]
    fn tool_entry_file_negatives() {
        for path in [
            "config.ts",
            "card.ts",
            "features/x/Component.tsx",
            "index.ts",
        ] {
            assert!(
                !is_tool_entry_file(path),
                "expected NOT a tool-entry match: {path}"
            );
        }
    }
}
