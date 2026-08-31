//! Dead-file candidates — files likely unused (fan_in == 0 and not an entry-point pattern). Entry patterns:
//! index/Main/App/Page/Route/main.tsx/routes.ts/apiRoutes plus the Next.js App Router convention set
//! (page/layout/route/error/not-found/… and metadata routes like sitemap/robots — shared via
//! `unreachable::framework_route_patterns`). Test/Storybook files are excluded (they run
//! independently), as are tool-entry files (dev-tool config, ambient `.d.ts` — see `is_tool_entry_file`) and
//! `package.json`-referenced files (`main`/`module`/`bin`/`exports` entries, plus paths found in `scripts`
//! commands): all are loaded by a tool/runtime rather than imported, so `fan_in == 0` on them is expected,
//! not a dead-code signal. The same `extra_entries` channel carries a THIRD group the engine resolves:
//! every source path a tool CONFIG names (a bundler `input`/`entry`, including in a second config only a
//! `--config` flag reaches). Exempting the config file while discarding the entries it declares was
//! measured at 3 false positives on one tree — the config vocabulary that decides which files are read
//! is `is_tool_config_file`, and the reading itself is `zzop_engine`'s `assemble::rules::config_entries`,
//! which cannot be done here (this crate resolves no paths and reads no files).
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
//! **`.rs`/`.go`/`.java`/`.cs` are all excluded from eligibility entirely too**, for a DIFFERENT reason
//! than Python's: not a filename-loading-convention gap but a real-uses-without-an-import-edge gap. A
//! `pub`-equivalent symbol can be genuinely, heavily used with NO import binding ever pointing at its own
//! file, so "exported + fan_in == 0" is not dead-code evidence the way it is for TypeScript — the import
//! graph structurally cannot see these use shapes, not merely doesn't happen to. Each language's
//! import-free-visibility mechanism:
//! - `.rs`: trait impls (`impl Display for Foo`, reached through trait resolution), `#[derive(...)]`
//!   expansion, and fully-qualified calls — `crate::a::f()` and equally a cross-crate
//!   `some_crate::f()` — never bind a local `use`, and `lang::imports` reads only `use`/`mod` ITEMS.
//!   That scope, its measured size, and why closing it is not free are owned by
//!   `zzop_parser_rust::lang::imports`'s module doc (§"a dependency exercised WITHOUT a `use`"); this
//!   line only states the consequence THIS rule depends on. The exclusion below outlives any fix to
//!   it: trait-impl and `#[derive]` reach stay invisible either way.
//! - `.go`: files in the SAME package share every top-level symbol with zero `import` between them (a
//!   package is one compilation unit) — only cross-package `import`-bound edges are visible
//!   (`merge_go_dep_edges`, engine side).
//! - `.java`: package-private members, and even fully-qualified refs to a `public` type in the SAME package,
//!   are used by sibling files with no `import` at all — only cross-package `import`-bound edges are visible
//!   (`merge_java_dep_edges`, engine side).
//! - `.cs`: same-namespace types are visible to sibling files with no `using` (like Java's same-package
//!   case), and ASP.NET adds framework discovery with no import edge at all — controllers are found by
//!   attribute routing, MediatR/DI handlers by assembly scanning, and `Program.cs` is the runtime entry
//!   point. So a `.cs` file's `fan_in == 0` is never dead evidence (`merge_csharp_dep_edges`, engine side).
//!
//! ## Framework path conventions
//! A framework that turns a FILE into a route by its PATH leaves that file with zero in-repo importers by
//! design, so `fan_in == 0` on it is the convention working rather than dead code — and the prescription
//! this rule prints ("delete the file") takes a live endpoint off the air. Two of those sets are exempted
//! here and they are anchored differently, on purpose:
//! - **By filename shape alone** — the Next.js App Router / SvelteKit convention files, shared with
//!   `dead_exports` through `unreachable::framework_route_patterns` so the set cannot drift between the
//!   two rules. `page.tsx`, `+server.ts` and their siblings are spellings no ordinary module carries.
//! - **By a declaration the tree makes about a DIRECTORY** — [`framework_roots`], for conventions whose
//!   name is an ordinary word. `pages/` is a Next.js router only where a `next.config.*` sits beside it;
//!   `src/pages/` is an Astro route directory only where an `astro.config.*` does. That module's doc owns
//!   the rationale, the measured harvest of each shape, and the list of directions refused at harvest 0.
//!
//! The gate covers only frameworks we have measured, and it is structurally unable to cover the next one.
//! That residue is carried by the finding message rather than the matcher: the text names this class
//! BEFORE its imperative (`rule-quality.md` §27), so a reader meets "a framework may load a whole file
//! from its own path" before meeting "delete the file".
//!
//! `dead_candidate_findings` is the `"dead-candidates"` native-analysis Finding-shaping wrapper the engine
//! calls. One exemption lives engine-side rather than here, since it needs file text this crate stays free
//! of: a file carrying an author-declared `@generated`/auto-generated banner is dropped from the results
//! by the engine's `file_has_generated_banner` (in its `generated_banner` module), mirroring the same
//! exemption in `dead_exports` — a generated file is regenerated, not hand-edited, so flagging it dead is
//! non-actionable for both. Native (on-disk) analysis path only; the envelope/Mode-A path can't read file
//! heads, see that call site.
//!
//! **vs. [`crate::unreachable`]**: disjoint by construction, not merely similarly named. This module only
//! ever flags a file with ZERO importers (`fan_in == 0` — nothing in the tree points at it at all);
//! `unreachable` only ever flags a file with ONE OR MORE importers (`fan_in > 0`) that are themselves
//! unreachable from any entrypoint — a "closed island" of files that reference each other but that nothing
//! live reaches. A given file can therefore never be flagged by both.

mod findings;
mod framework_roots;

pub use findings::dead_candidate_findings;

use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{DepGraph, FileNode};

use crate::unreachable::{framework_route_patterns, is_tool_entry_file};
use framework_roots::FrameworkRoots;

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
    // Scanned once per run, then asked about every surviving candidate — the answer depends on what
    // OTHER files this tree holds (a framework config beside a directory), which no per-path regex can
    // see. See the module doc's "Framework path conventions".
    let framework_roots = FrameworkRoots::scan(nodes);
    let mut out: Vec<FileNode> = nodes
        .iter()
        .filter(|n| is_dead_candidate_eligible(&n.path, &participants))
        .filter(|n| n.fan_in == 0)
        .filter(|n| n.change_count <= max_changes)
        .filter(|n| !matches_any(&n.path, entry_patterns()))
        .filter(|n| !matches_any(&n.path, exclude_patterns()))
        // `zzop_core::is_test_file` is the SSOT for test surface — it adds the test-runner DIRECTORY
        // conventions (`e2e/`, `cypress/`, `playwright/`, `testing/`, `__tests__/`, `tests/`, `spec/`)
        // that `exclude_patterns()` intentionally does not duplicate. A file under one of those dirs
        // (e.g. `playwright/global.setup.ts`) is loaded by the runner, not imported, so `fan_in == 0` on
        // it is expected. Delegating here keeps this analysis in sync with `dead_exports::is_entry_or_test`.
        .filter(|n| !zzop_core::is_test_file(&n.path))
        .filter(|n| !is_tool_entry_file(&n.path))
        .filter(|n| !framework_roots.loads_by_path(&n.path))
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
/// importers" evidence); `.rs`, `.go`, `.java`, and `.cs` are excluded up front too, for the DIFFERENT
/// reason the module doc's "`.rs` is excluded from eligibility entirely too" / "`.go` is excluded from
/// eligibility entirely too" / "`.java` is excluded from eligibility entirely too" paragraphs explain
/// (trait impls/derive expansion/full-path calls for Rust, same-package symbol sharing with no import
/// statement for Go/Java/C# — all give a real use the import graph structurally cannot see). C# adds a
/// framework-discovery layer on top of Java's same-namespace visibility: ASP.NET controllers are found by
/// attribute routing, MediatR/DI handlers by assembly scanning, and `Program.cs` is the runtime entry —
/// none carry a `using`-import edge, so `fan_in == 0` is never dead evidence for a `.cs` file. Otherwise:
/// true if the path participates in the dep graph (branch a) OR its extension is in the TS-dispatch set
/// (branch b).
fn is_dead_candidate_eligible(path: &str, participants: &HashSet<&str>) -> bool {
    if is_python_source_ext(path)
        || is_rust_source_ext(path)
        || is_go_source_ext(path)
        || is_java_source_ext(path)
        || is_csharp_source_ext(path)
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

/// True for `.cs` (case-insensitive) — see `is_dead_candidate_eligible`'s doc for why C# is excluded from
/// candidacy entirely (same-namespace visibility with no `using`, plus ASP.NET attribute-routing /
/// MediatR-DI / `Program.cs`-entry framework discovery that carries no import edge).
fn is_csharp_source_ext(path: &str) -> bool {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?i)\.cs$").unwrap())
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
            r"(^|/)main\.(ts|tsx|js|jsx)$",
            r"(^|/)App\.(ts|tsx|js|jsx)$",
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
