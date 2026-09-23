//! Language dispatcher — decides which parser frontend (if any) handles a file, purely from its path.
//! Extension map first, then a path-glob override list that can force a specific language regardless of
//! extension.
//!
//! A file matching neither an override nor a known extension still flows through the fused pipeline as
//! a lexical-only `SourceFile` — no symbols/imports/io, but still scanned by line-scan DSL rules. `None`
//! means "no structural parser", not "ignore this file".
//!
//! `.java` routes to `Language::Java21`, a real structural parser (`zzop_parser_java_21`,
//! tree-sitter-backed) at the same grade as `TypeScript`/`Python`/`Rust`/`Go` — see
//! `pipeline::parse_java21`'s own doc for the fused-pipeline wiring. `.jsp`/`.jspx`/`.tag` stay on the
//! `None` path: JSP embeds Java inside HTML-like markup, a shape this CST frontend isn't built to
//! disentangle.
//!
//! `.py`/`.pyi` route to `Language::Python`, a real structural parser (`zzop_parser_python_3`, ruff-backed)
//! at the same grade as `TypeScript` — see `pipeline::parse_python`'s own doc for the fused-pipeline wiring.
//!
//! `.rs` routes to `Language::Rust`, a real structural parser (`zzop_parser_rust`, syn-backed) at the
//! same grade as `TypeScript`/`Python` — see `pipeline::parse_rust`'s own doc for the fused-pipeline
//! wiring. Nothing else maps to `Language::Rust` (`.rs.in` and similar template-adjacent extensions stay
//! out of v1 scope, same as the general "no plausible mapping without guessing" discipline this table
//! upholds elsewhere).
//!
//! `.go` routes to `Language::Go`, a real structural parser (`zzop_parser_go`, tree-sitter-backed) at the
//! same grade as `TypeScript`/`Python`/`Rust` — see `pipeline::parse_go`'s own doc for the fused-pipeline
//! wiring. Nothing else maps to `Language::Go`.
//!
//! `.sql` routes to `Language::Sql`, a line/regex-level frontend (`zzop_parser_sql` — deliberately no
//! tree-sitter/`sqlparser` dependency, see that crate's own doc) extracting `CREATE TABLE` statements into
//! `db-table` io PROVIDEs only. No symbols/imports project for `.sql` (it never joins the shared dep
//! graph, same as `Prisma`) and no consumes (this engine has no SQL DML/egress extractor). Nothing else
//! maps to `Language::Sql`.

use std::path::Path;

/// A source language this engine has a parser frontend for — one variant per `parser/` crate, and the
/// variant list below IS that inventory. Deliberately not restated in prose here: the sentence that used
/// to sit on this line named six of the eight (it predated `Sql` and `CSharp`), which is how a
/// hand-listed inventory sitting next to a compiler-enforced one always ends. Each variant's precision
/// tier (full AST / full CST / lexical) and the crate behind it are in `docs/ARCHITECTURE.md`'s
/// "Language support" table; this module's own doc covers the dispatch route. JSP has no parser crate in
/// this workspace at all: files that would route to it get no `Language` match.
///
/// **Serialization invariant**: `Language` derives no `Serialize`/`Deserialize` and is never written into
/// `zzop_cache::FileIrSlice`, the cache envelope, or any wire-format enum, so renaming a variant is
/// cache-safe on its own. A change to a language's projected `FileIrSlice` *shape* is not free, but it
/// invalidates itself: the slice's shape lives in `zzop-cache`, which the derived
/// `CACHE_SCHEMA_VERSION` hashes by dependency closure, and the dispatch arm that fills it lives in this
/// crate, whose own sources `FP_ENGINE` hashes into every parser fingerprint (`cache.rs`). Nothing here
/// is raised by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    Prisma,
    Java21,
    Python,
    Rust,
    Go,
    Sql,
    CSharp,
}

/// Directory names skipped entirely during the tree walk: common Node-ecosystem build/dependency dirs,
/// plus this workspace's own build output dir (`target`). `.yarn` covers Yarn Berry's vendored
/// package-manager bundle, which is not project source.
///
/// The last three are zzop's OWN output dir names, and they are the load-bearing ones. The walker runs
/// `hidden(false)` (`pipeline::walking`), so a dot directory IS walked; `.gitignore` is honored but only
/// helps a user who has a git tree AND wrote the rule. Without these entries, output written inside the
/// analyzed tree gets walked as source on the NEXT run (self-scan pollution: the file count grows every
/// run, observed live in a blind field test).
/// - [`zzop_cache::TOOL_DIR`] (`.zzop`) — the CURRENT one, and the one that actually fires: the config
///   front-end defaults `cacheDir` to `zzop_cache::DEFAULT_CACHE_DIR` (`.zzop/cache`), so a
///   run whose config omits `cacheDir` writes there on its very first execution. Referenced as a symbol, not spelled as a
///   literal — the T1 single definition lives in `zzop-cache` next to the store that writes there.
/// - `zzop-reports` / `.zzop-cache` — the removed JS CLI's report dir and `cacheDir` template value.
///   Kept as legacy defense: those directories still sit in trees analyzed before v0.20.0.
///
/// The user-authored sibling `zzop/` (no dot — custom rule packs, adapter overlays) is deliberately NOT
/// here: that is source a human wrote and wants analyzed.
pub(crate) const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".git",
    "target",
    ".yarn",
    zzop_cache::TOOL_DIR,
    "zzop-reports",
    ".zzop-cache",
];

/// Configures the dispatcher: path-glob overrides (checked first, in list order — first match wins) and
/// which directory names to skip while walking a tree.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    /// `(glob, language)` — a path matching `glob` (shell-style, `zzop_core::glob_matches` — the same dialect `exclude`/`suppressions` use) is dispatched to `language`
    /// regardless of its extension. Checked in order; the first matching entry wins.
    pub glob_overrides: Vec<(String, Language)>,
    pub skip_dirs: Vec<String>,
}

impl Default for DispatchConfig {
    fn default() -> Self {
        Self {
            glob_overrides: Vec::new(),
            skip_dirs: DEFAULT_SKIP_DIRS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Dispatches a normalized (forward-slash) relative path to a `Language`, or `None` if no parser frontend
/// claims it (extension unknown / not a recognized source file). Glob overrides are consulted before the
/// extension map, so a project can force-route paths the extension map would otherwise miss or mis-tag.
pub fn dispatch(rel_path: &str, config: &DispatchConfig) -> Option<Language> {
    for (glob, lang) in &config.glob_overrides {
        // ONE glob dialect for the workspace (`zzop_core::glob_matches`), not two.
        //
        // This used to be a local `matches_glob` justified by a comment reading "reimplemented here
        // rather than imported — `core::recommendations`'s equivalent is a private helper with no public
        // home". That module does not exist (it is `zzop_metrics::recommendations`, and it holds no glob
        // helper at all — it delegates to core). The real sibling was
        // `core::registry::config::path_filter`, which was private, so the argument was true and
        // self-perpetuating; making it public was the whole fix (review ledger V98).
        //
        // The two dialects DISAGREED on 7 of 12 measured cases, and one disagreement was a defect, not a
        // dialect choice: the old translator did not escape `?`, so a user glob `file?.ts` compiled to
        // the regex `^file?\.ts$` — the `?` became a QUANTIFIER, matching `fil.ts` and not `file1.ts`,
        // exactly backwards. It also matched `{a,b}` literally instead of alternating, and read `**/x`
        // as requiring at least one directory. All three now behave the way `exclude`/`suppressions`
        // already did — the other user-writable glob key in the same config.
        if zzop_core::glob_matches(glob, rel_path) {
            return Some(*lang);
        }
    }
    dispatch_by_extension(rel_path)
}

/// The extension map alone, with no `glob_overrides` consulted. `pub(crate)` so
/// [`crate::dead_exports::is_ts_source_ext`] can BE this table rather than hand-copy its TypeScript
/// arm — the copy sat here unpinned until 2026-07-29, the widest of the extension-set duplicates and
/// the only one between a root and its own clone.
pub(crate) fn dispatch_by_extension(rel_path: &str) -> Option<Language> {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    language_for_extension(&ext)
}

/// The same table addressed by a bare extension instead of a path.
///
/// Split out so [`non_source::extension_content_kind`] can ask "does this build have a parser for this
/// extension?" from the one place the answer lives. A second list would be the exact duplicate this
/// module already names as the widest it ever carried.
pub(crate) fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "mts" | "cts" => Some(Language::TypeScript),
        "prisma" => Some(Language::Prisma),
        "java" => Some(Language::Java21),
        "py" | "pyi" => Some(Language::Python),
        "rs" => Some(Language::Rust),
        "go" => Some(Language::Go),
        "sql" => Some(Language::Sql),
        "cs" => Some(Language::CSharp),
        _ => None,
    }
}

/// True exactly for extensions dispatching to a DECLARATION-ONLY language (`Language::Prisma`,
/// `Language::Sql`) — the frontends whose files never join the shared dep graph and project no
/// symbols/imports (see this module's doc, and each arm's note above). A declaration-only language
/// cannot host client/handler code, so a silent-when-blind rule's evidence (call sites, write
/// sites, retry tags — all read off code that CALLS things) can never be witnessed there and its
/// absence in such files is not a blind spot. Derived from [`dispatch_by_extension`] itself, never
/// a second list, so a new dispatch arm cannot leave this predicate stale; the facade's coverage
/// cross reads it here rather than keeping a facade-side shadow table.
pub fn declaration_only_extension(ext: &str) -> bool {
    matches!(
        dispatch_by_extension(&format!("x.{ext}")),
        Some(Language::Prisma | Language::Sql)
    )
}

/// True if `name` (a single path component — a directory's own name, not a full path) is one of
/// `config.skip_dirs`. Exact match against the directory's own name (not a glob).
pub fn is_skip_dir(name: &str, config: &DispatchConfig) -> bool {
    config.skip_dirs.iter().any(|d| d == name)
}

mod non_source;
mod wire;

// The extension-CLASSIFICATION half, re-exported so `zzop_engine::dispatch::*` stays one surface for
// callers and `lib.rs`'s re-export list does not have to learn the internal split.
pub use non_source::{
    extension_content_kind, extraction_can_lose_facts, is_non_source_extension, non_source_kind,
    NonSourceKind,
};

#[cfg(test)]
mod tests;
