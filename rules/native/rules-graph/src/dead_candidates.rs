//! Dead-file candidates — files likely unused (fan_in == 0 and not an entry-point pattern). Entry patterns:
//! index/Main/App/Page/Route/main.tsx/routes.ts/apiRoutes plus the Next.js App Router convention set
//! (page/layout/route/error/not-found/… and metadata routes like sitemap/robots — shared via
//! `unreachable::framework_route_patterns`). Test/Storybook files are excluded (they run
//! independently), as are tool-entry files (dev-tool config, ambient `.d.ts` — see `is_tool_entry_file`) and
//! `package.json`-referenced files (`main`/`module`/`bin`/`exports` entries, plus paths found in `scripts`
//! commands): all are loaded by a tool/runtime rather than imported, so `fan_in == 0` on them is expected,
//! not a dead-code signal.
//!
//! ## Eligibility scope
//! "No importers" is only meaningful when the dep graph could, in principle, have pointed an edge at the
//! file. A file is eligible iff it participates in the `DepGraph` (a key or an edge target — every processed
//! file is inserted as a `dep` key even with zero outgoing edges) or its extension is in the TS-dispatch set
//! (`ts|tsx|js|jsx|mjs|cjs|mts|cts`) as a fallback for a TS-shaped file that never made it into `dep`.
//! Non-source extensions (`.json`/`.css`/`.md`/`.svg`/`.prisma`/`.java`/...) that never appear in the dep
//! graph fail both checks and are never eligible — otherwise they'd dominate the finding set with a false
//! "no importers" signal, since no edge was ever computed for them. Envelope-ingested non-TS source (e.g.
//! `.jsp`) that does participate in the graph stays eligible: `fan_in == 0` on it is real signal.
//!
//! `dead_candidate_findings` is the `"dead-candidates"` native-analysis Finding-shaping wrapper the engine
//! calls.

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{disable_hint, DepGraph, FileNode, Finding, Severity};

use crate::unreachable::{framework_route_patterns, is_tool_entry_file};

/// Default `max_changes` — a file changed more often than this is probably alive.
pub const DEAD_MAX_CHANGES: u32 = 3;

/// fan_in == 0, change_count <= max_changes, not an entry/test/storybook/tool-entry/package.json-referenced
/// file, and eligible per the scope above (see module doc). `extra_entries` is a set of concrete file paths
/// — package.json manifest entries resolved at runtime (`zzop_engine::pipeline::package_json_entries`), not
/// a naming-convention regex like `entry_patterns`. Ranked by change_count asc, then path.
pub fn find_dead_candidates(
    nodes: &[FileNode],
    dep: &DepGraph,
    max_changes: u32,
    extra_entries: &std::collections::HashSet<String>,
) -> Vec<FileNode> {
    let participants = dep_graph_participants(dep);
    let mut out: Vec<FileNode> = nodes
        .iter()
        .filter(|n| is_dead_candidate_eligible(&n.path, &participants))
        .filter(|n| n.fan_in == 0)
        .filter(|n| n.change_count <= max_changes)
        .filter(|n| !matches_any(&n.path, entry_patterns()))
        .filter(|n| !matches_any(&n.path, exclude_patterns()))
        .filter(|n| !is_tool_entry_file(&n.path))
        .filter(|n| !extra_entries.contains(&n.path))
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        a.change_count
            .cmp(&b.change_count)
            .then_with(|| a.path.cmp(&b.path))
    });
    out
}

/// One `Finding` per dead-candidate file (native analysis id `"dead-candidates"`, matching
/// `register_native_analyses`), gated at `DEAD_MAX_CHANGES`. See `find_dead_candidates`'s doc for
/// `extra_entries`.
pub fn dead_candidate_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &std::collections::HashSet<String>,
) -> Vec<Finding> {
    find_dead_candidates(nodes, dep, DEAD_MAX_CHANGES, extra_entries)
        .into_iter()
        .map(|n| Finding {
            rule_id: "dead-candidates".to_string(),
            severity: Severity::Info,
            file: n.path,
            line: 1,
            message: format!(
                "no importers found in this tree (candidate dead file — scoped to files that \
                 participate in the dep graph: a `dep`-map key or edge target, or a TS-dispatch \
                 extension ts/tsx/js/jsx/mjs/cjs/mts/cts as a fallback; dev-tool config files, \
                 ambient `.d.ts` declarations, and package.json-referenced entry files are \
                 excluded — they're loaded by a tool/runtime directly, not imported). Delete the \
                 file if it is genuinely unused, or wire it up if it should be reachable. {} if your \
                 build loads files this graph can't see (e.g. a custom bundler entry, a \
                 template-string dynamic import).",
                disable_hint("dead-candidates")
            ),
            data: None,
        })
        .collect()
}

fn matches_any(path: &str, patterns: &[Regex]) -> bool {
    patterns.iter().any(|re| re.is_match(path))
}

/// Every path that appears in `dep` as either a source key or an edge target — i.e. every file the dep
/// graph the caller's `fan_in` was computed from actually tracks as a node. See module doc branch (a).
fn dep_graph_participants(dep: &DepGraph) -> HashSet<&str> {
    dep.iter()
        .flat_map(|(src, targets)| {
            std::iter::once(src.as_str()).chain(targets.iter().map(String::as_str))
        })
        .collect()
}

/// Union discriminator — see module doc. True if the path participates in the dep graph (branch a) OR its
/// extension is in the TS-dispatch set (branch b).
fn is_dead_candidate_eligible(path: &str, participants: &HashSet<&str>) -> bool {
    participants.contains(path) || is_ts_dispatch_extension(path)
}

/// True for the extensions `dispatch_by_extension` routes to `Language::TypeScript` (case-insensitive).
/// Duplicated here rather than imported: this crate is deliberately `zzop-core`-only.
fn is_ts_dispatch_extension(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.(ts|tsx|js|jsx|mjs|cjs|mts|cts)$").unwrap())
        .is_match(path)
}

fn entry_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        let mut v: Vec<Regex> = [
            r"(^|/)index\.(ts|tsx|js|jsx)$",
            r"(^|/)main\.(ts|tsx)$",
            r"(^|/)App\.(ts|tsx)$",
            r"Page\.(ts|tsx)$",
            r"Route\.(ts|tsx)$",
            r"(^|/)routes?\.(ts|tsx)$",
            r"apiRoutes\.(ts|tsx)$",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect();
        // Next.js App Router convention files (`app/**/{page,layout,route,…}.tsx`, metadata routes) are
        // loaded by the framework via filename, so their fan_in == 0 is expected, not a dead signal.
        // Shared with `dead_exports` via one source so the exemption set can't drift between the rules.
        v.extend(framework_route_patterns().iter().cloned());
        v
    })
}

fn exclude_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"\.(test|spec)\.(ts|tsx|js|jsx)$",
            r"\.stories\.(ts|tsx|js|jsx)$",
            r"/__test__/",
            r"/__mocks__/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

#[cfg(test)]
mod tests {
    //! Exercises `find_dead_candidates`: zero-fan-in low-change-count files are flagged; files with
    //! incoming edges, high change counts, or entry/test patterns are excluded; non-source files never
    //! linked in the dep graph are excluded from candidacy entirely; a non-TS file that does participate in
    //! the dep graph (envelope-ingested source, e.g. `.jsp`) is still a candidate on zero fan-in.
    use super::*;
    use std::collections::HashMap;

    fn n(path: &str, fan_in: u32, change_count: u32) -> FileNode {
        FileNode {
            id: path.into(),
            path: path.into(),
            change_count,
            churn: 0,
            last_modified: Some("2026-01-01".into()),
            author_count: 1,
            loc: 50,
            tag_counts: HashMap::new(),
            fan_in,
            fan_out: 0,
            total_connections: fan_in,
            risk_score: 0.0,
            ..Default::default()
        }
    }

    fn empty_dep() -> DepGraph {
        DepGraph::new()
    }

    fn no_extra_entries() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn fan_in_zero_low_change_count_is_candidate() {
        let r = find_dead_candidates(
            &[n("features/x/Orphan.tsx", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["features/x/Orphan.tsx"]);
    }

    #[test]
    fn fan_in_positive_is_excluded() {
        let r = find_dead_candidates(
            &[n("x.tsx", 2, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn entry_patterns_are_excluded() {
        let r = find_dead_candidates(
            &[
                n("pages/HomePage.tsx", 0, 1),
                n("App.tsx", 0, 1),
                n("features/x/index.ts", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn nextjs_app_router_convention_files_are_not_dead_candidates() {
        // Framework-loaded by filename (never imported) so fan_in == 0 is expected — must not be flagged.
        // A genuinely orphaned sibling in the same set still fires.
        let r = find_dead_candidates(
            &[
                n("app/(lang)/[lang]/about/page.tsx", 0, 1),
                n("app/dashboard/layout.tsx", 0, 1),
                n("app/api/users/route.ts", 0, 1),
                n("app/not-found.tsx", 0, 1),
                n("app/sitemap.ts", 0, 1),
                n("app/robots.ts", 0, 1),
                n("features/x/old-helper.ts", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["features/x/old-helper.ts"], "{r:?}");
    }

    #[test]
    fn test_files_are_excluded() {
        let r = find_dead_candidates(
            &[n("features/x/__test__/x.test.ts", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn high_change_count_is_excluded() {
        let r = find_dead_candidates(
            &[n("features/x/Hot.ts", 0, 10)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn non_source_files_never_in_the_dep_graph_are_never_candidates() {
        // These extensions never participate in the dep graph and aren't TS-dispatch extensions, so
        // fan_in == 0 on them is not a "no importers" signal — it's just "untracked".
        let r = find_dead_candidates(
            &[
                n("data/config.json", 0, 1),
                n("styles/app.css", 0, 1),
                n("docs/README.md", 0, 1),
                n("assets/logo.svg", 0, 1),
                n("schema.prisma", 0, 1),
                n("Service.java", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty(), "expected no candidates, got: {r:?}");
    }

    #[test]
    fn source_file_dead_still_fires_alongside_excluded_non_source_files() {
        // Non-source files with equally zero fan-in don't suppress a genuine dead file in the same set.
        let r = find_dead_candidates(
            &[
                n("features/x/Orphan.tsx", 0, 1),
                n("data/config.json", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["features/x/Orphan.tsx"]);
    }

    #[test]
    fn finding_message_renders_single_braces_in_the_disable_hint() {
        // Regression: this message is a PLAIN string literal (not `format!`), so a `{{` written for
        // format-escaping renders literally as `{{` in user output — the 2026-07-10 dialect sweep
        // shipped exactly that. Pin the rendered form.
        let out = dead_candidate_findings(
            &[n("features/x/Orphan.tsx", 0, 1)],
            &empty_dep(),
            &no_extra_entries(),
        );
        assert_eq!(out.len(), 1);
        assert!(
            out[0]
                .message
                .contains("`rules: { \"dead-candidates\": \"off\" }`"),
            "{}",
            out[0].message
        );
        assert!(!out[0].message.contains("{{"), "{}", out[0].message);
    }

    #[test]
    fn all_import_eligible_extensions_are_candidates_when_dead() {
        let nodes: Vec<FileNode> = [
            "a.ts", "b.tsx", "c.js", "d.jsx", "e.mjs", "f.cjs", "g.mts", "h.cts",
        ]
        .iter()
        .map(|p| n(p, 0, 1))
        .collect();
        let r = find_dead_candidates(&nodes, &empty_dep(), DEAD_MAX_CHANGES, &no_extra_entries());
        assert_eq!(
            r.len(),
            8,
            "expected all 8 import-eligible extensions to be candidates, got: {r:?}"
        );
    }

    #[test]
    fn ts_extension_match_is_case_insensitive() {
        let r = find_dead_candidates(
            &[n("features/x/Orphan.TSX", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert_eq!(r.len(), 1, "{r:?}");
    }

    #[test]
    fn non_ts_file_present_as_a_dep_key_with_zero_fan_in_is_a_candidate() {
        // Envelope-ingested source inserted as a `dep` key is a real graph node, so fan_in == 0 here is
        // real "no importers" signal, not "untracked".
        let mut dep = empty_dep();
        dep.insert("legacy/UserController.jsp".to_string(), Vec::new());
        let r = find_dead_candidates(
            &[n("legacy/UserController.jsp", 0, 1)],
            &dep,
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert_eq!(r.len(), 1, "{r:?}");
    }

    #[test]
    fn non_ts_file_present_only_as_an_edge_target_with_zero_fan_in_is_still_evaluated() {
        // A file that appears only as a target in another file's edge list (never as its own `dep` key)
        // still participates in the graph — branch (a) checks both positions.
        let mut dep = empty_dep();
        dep.insert(
            "legacy/Controller.jsp".to_string(),
            vec!["legacy/util.jsp".to_string()],
        );
        // fan_in 1 means something imports it, so it correctly is NOT a candidate here.
        let r = find_dead_candidates(
            &[n("legacy/util.jsp", 1, 1)],
            &dep,
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn non_ts_file_absent_from_the_dep_graph_entirely_is_never_a_candidate() {
        // Never appears in `dep` at all, so it doesn't participate in the graph fan_in was computed from —
        // fan_in == 0 here is "untracked", not "no importers".
        let r = find_dead_candidates(
            &[n("legacy/Orphaned.jsp", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty());
    }

    #[test]
    fn ts_file_absent_from_the_dep_graph_is_still_a_candidate_via_extension_fallback() {
        // Never made it into `dep` at all, but still falls back to branch (b) — a `.ts` file missing from
        // the graph reads as an ingestion gap, not "outside the import graph".
        let r = find_dead_candidates(
            &[n("features/x/Isolated.ts", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert_eq!(r.len(), 1, "{r:?}");
    }

    #[test]
    fn tool_entry_files_are_never_dead_candidates() {
        // These are all zero-fan-in because they're loaded by a tool, not imported by app code.
        let r = find_dead_candidates(
            &[
                n(".eslintrc.cjs", 0, 1),
                n(".prettierrc.cjs", 0, 1),
                n("vite.config.ts", 0, 1),
                n("vite-env.d.ts", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        assert!(r.is_empty(), "expected no candidates, got: {r:?}");
    }

    #[test]
    fn genuinely_orphaned_source_file_still_fires_alongside_tool_entry_files() {
        let r = find_dead_candidates(
            &[
                n("vite.config.ts", 0, 1),
                n("features/x/old-helper.ts", 0, 1),
            ],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &no_extra_entries(),
        );
        let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["features/x/old-helper.ts"]);
    }

    #[test]
    fn package_json_referenced_file_is_never_a_dead_candidate() {
        // A path present in `extra_entries` (e.g. a package.json `main` target) is excluded, same as an
        // entry-pattern or tool-entry file; a genuinely orphaned file not in `extra_entries` still fires.
        let extra: HashSet<String> = ["src/cli.ts".to_string()].into_iter().collect();
        let r = find_dead_candidates(
            &[n("src/cli.ts", 0, 1), n("features/x/old-helper.ts", 0, 1)],
            &empty_dep(),
            DEAD_MAX_CHANGES,
            &extra,
        );
        let paths: Vec<&str> = r.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["features/x/old-helper.ts"]);
    }
}
