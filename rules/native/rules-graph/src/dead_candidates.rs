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
//! **`.py`/`.pyi` are excluded from eligibility entirely** (F1), even though every `.py`/`.pyi` file now
//! participates in the `DepGraph` (a `dep`-map key or edge target, same as any other tracked file — see
//! `pipeline::FileArtifact::imports`'s doc). Unlike TypeScript's import-graph loading, Python's module
//! loading is substantially filename-convention-driven: `main.py`, `manage.py`, `wsgi.py`/`asgi.py`,
//! `settings.py`, `conftest.py`, a migration module, a `test_*.py` file — none of these are ever
//! `import`ed, so `fan_in == 0` on them is not "no importers" evidence, it's the loading convention
//! working as intended. Java is excluded too, for its own reason — see the `.java` paragraph below:
//! since the parser-java-21 upgrade `.java` DOES participate in the `DepGraph`, but same-package
//! visibility needs no import, so graph fan-in still is not liveness evidence there; Python's case
//! differs only in HOW graph participation fails to be evidence (loading-convention entry files vs
//! Java's import-free same-package visibility). Revisit if Python entry conventions are ever modeled the
//! way `entry_patterns`/`is_tool_entry_file` model TS/JS/Java conventions (a real per-file exemption list
//! instead of a blanket language exclusion) — until then, a blanket exclusion is the honest floor.
//!
//! **`.rs` is excluded from eligibility entirely too**, for a DIFFERENT reason than Python's: it is not a
//! filename-loading-convention gap, it is a real-uses-without-an-import-edge gap. A `pub` Rust item can be
//! genuinely, heavily used with NO import binding ever pointing at its own file: a trait implementation
//! (`impl Display for Foo`) is reached by the compiler through trait resolution, never a `use`; `#[derive(
//! ...)]` expansion generates calls into a type with no `use` of its own; and a fully-qualified call
//! (`crate::a::f()`) never binds a local name the way `use crate::a::f;` would, so it never surfaces in
//! the caller's own `ImportMap` at all — `lang::imports`' v1 scope is top-level `use`/`mod` items only.
//! So "exported + fan_in == 0" is not dead-code evidence for Rust the way it is for TypeScript: the import
//! graph structurally cannot see these use shapes, not merely doesn't happen to.
//!
//! **`.go` is excluded from eligibility entirely too**, same exclusion CLASS as `.rs` (a real-uses-
//! without-an-import-edge gap, not a filename-loading-convention gap like Python's): files in the SAME Go
//! package share every top-level symbol with NO import statement between them at all — Go's own
//! compilation-unit model (a package is compiled as one unit; a sibling file's `func`/`type`/`var` is
//! reachable with zero `import`s). So a `pub`-equivalent (exported, capitalized) Go symbol can be
//! genuinely, heavily used by a sibling file in its own package while the FILE that declares it shows
//! `fan_in == 0` — the import graph structurally cannot see intra-package usage, only cross-package
//! `import`-bound edges (`merge_go_dep_edges`'s own doc, engine side).
//!
//! **`.java` is excluded from eligibility entirely too**, same exclusion CLASS as `.rs`/`.go` (a
//! real-uses-without-an-import-edge gap): Java's own package-visibility model lets a type/member with NO
//! explicit access modifier (package-private) — and even a fully-qualified reference to a `public` type
//! in the SAME package — be used by a sibling file with no `import` statement pointing at it at all
//! (Java, like Go, never requires importing a type declared in your own package). So a `.java` file can
//! show `fan_in == 0` while genuinely, heavily used by same-package siblings — the import graph
//! structurally cannot see that usage, only cross-package `import`-bound edges
//! (`merge_java_dep_edges`'s own doc, engine side). `.java` previously never participated in `DepGraph`
//! at all (no import-graph-based dep resolution existed for it); now that it does
//! (`merge_java_dep_edges`), this exclusion keeps eligibility unaffected by that change rather than
//! silently gaining a whole language's worth of same-package false positives.
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

/// Union discriminator — see module doc. `.py`/`.pyi` are excluded up front regardless of graph
/// participation (F1: filename-convention loading makes `fan_in == 0` on them meaningless as "no
/// importers" evidence); `.rs`, `.go`, and `.java` are excluded up front too, for the DIFFERENT reason
/// the module doc's "`.rs` is excluded from eligibility entirely too" / "`.go` is excluded from
/// eligibility entirely too" / "`.java` is excluded from eligibility entirely too" paragraphs explain
/// (trait impls/derive expansion/full-path calls for Rust, same-package symbol sharing with no import
/// statement for Go and Java — all give a real use the import graph structurally cannot see). Otherwise:
/// true if the path participates in the dep graph (branch a) OR its extension is in the TS-dispatch set
/// (branch b).
fn is_dead_candidate_eligible(path: &str, participants: &HashSet<&str>) -> bool {
    if is_python_source_ext(path)
        || is_rust_source_ext(path)
        || is_go_source_ext(path)
        || is_java_source_ext(path)
    {
        return false;
    }
    participants.contains(path) || is_ts_dispatch_extension(path)
}

/// True for `.py`/`.pyi` (case-insensitive) — see `is_dead_candidate_eligible`'s doc and the module doc's
/// "Eligibility scope" section (F1) for why Python is excluded from candidacy entirely rather than
/// folded into the TS-dispatch fallback.
fn is_python_source_ext(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.pyi?$").unwrap())
        .is_match(path)
}

/// True for `.rs` (case-insensitive) — see `is_dead_candidate_eligible`'s doc and the module doc's "`.rs`
/// is excluded from eligibility entirely too" paragraph for why Rust is excluded from candidacy entirely.
fn is_rust_source_ext(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.rs$").unwrap())
        .is_match(path)
}

/// True for `.go` (case-insensitive) — see `is_dead_candidate_eligible`'s doc and the module doc's "`.go`
/// is excluded from eligibility entirely too" paragraph for why Go is excluded from candidacy entirely.
fn is_go_source_ext(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.go$").unwrap())
        .is_match(path)
}

/// True for `.java` (case-insensitive) — see `is_dead_candidate_eligible`'s doc and the module doc's
/// "`.java` is excluded from eligibility entirely too" paragraph for why Java is excluded from candidacy
/// entirely.
fn is_java_source_ext(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.java$").unwrap())
        .is_match(path)
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
mod tests;
