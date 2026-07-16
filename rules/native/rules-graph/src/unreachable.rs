//! Unreachable-code detector — closed "dead islands": files imported in-repo (fanIn > 0) yet not reachable from any
//! entrypoint. Pure over (nodes, dep), language-agnostic.
//!
//! `unreachable_findings` is the `"unreachable"` native-analysis Finding-shaping wrapper the engine calls
//! (moved here alongside the algorithm it shapes).

use std::collections::{HashSet, VecDeque};
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{disable_hint, DepGraph, FileNode, Finding, Severity};

#[derive(Debug, Clone, PartialEq)]
pub struct UnreachableFile {
    pub path: String,
    pub loc: u32,
    pub risk_score: f64,
    pub fan_in: u32,
}

/// Files with fanIn > 0 that no entrypoint reaches — closed dead islands. Ranked by loc desc, then risk, then path.
/// Entrypoints = conventional entry files + test files + every fanIn=0 file (false-positive-safe) +
/// `extra_entries` — paths the CALLER knows are loaded by a mechanism this graph can't see (the same
/// contract as `find_dead_candidates`' parameter of the same name): cargo-manifest-declared target
/// files (`[[bin]]`/`[[test]]`/... `path = "..."` — loaded by cargo, never imported) and Mode-B
/// adapter-overlay files marked `is_entry`.
pub fn find_unreachable(
    nodes: &[FileNode],
    dep: &DepGraph,
    limit: usize,
    extra_entries: &HashSet<String>,
) -> Vec<UnreachableFile> {
    let mut entries: HashSet<String> = HashSet::new();
    for n in nodes {
        if n.fan_in == 0
            || is_entry_file(&n.path)
            || is_test_file(&n.path)
            || extra_entries.contains(&n.path)
        {
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
pub fn unreachable_findings(
    nodes: &[FileNode],
    dep: &DepGraph,
    extra_entries: &HashSet<String>,
) -> Vec<Finding> {
    find_unreachable(nodes, dep, nodes.len(), extra_entries)
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
                 entrypoint if it should be reachable. {} if this island is reached by a mechanism this \
                 graph doesn't see (e.g. dynamic `require`, a plugin loader).",
                u.fan_in,
                disable_hint("unreachable")
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

/// Shared test-path predicate — also used by `zzop_rules_http::mutating_route_no_auth` and several
/// `zzop_rules_cross_layer` rules to skip route/consume registrations in a test/fixture file. Lives in
/// `zzop_core` (also needed by the TS parser's DB-table extractors); those other crates import it
/// directly as `zzop_core::is_test_file` rather than through this module.
use zzop_core::is_test_file;

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
            r"(^|/)(__main__|manage|wsgi|asgi|main|settings|conftest)\.py$",
            // Rust entry conventions: crate/binary roots (`main.rs`/`lib.rs`/`build.rs`) plus any file
            // under a `tests/`/`examples/`/`benches/`/`src/bin/` path component — cargo's own conventional
            // test-harness/example-binary/benchmark-binary/multi-binary directories, each compiled and run
            // as its own separate target rather than `use`d from elsewhere in the crate, so zero in-repo
            // importers is expected for files under them, not a dead/unreachable signal.
            r"(^|/)(main|lib|build)\.rs$",
            r"(^|/)(tests|examples|benches)/",
            r"(^|/)src/bin/",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    })
}

#[cfg(test)]
mod tests;
