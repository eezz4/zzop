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

/// A source language this engine has a parser frontend for. `TypeScript`/`Prisma`/`Python`/`Rust`/`Go`/
/// `Java21` are all real structural parsers (`Java21` — `zzop_parser_java_21`, tree-sitter-backed — see
/// module doc). JSP has no parser crate in this workspace at all: files that would route to it get no
/// `Language` match.
///
/// **Serialization invariant**: `Language` derives no `Serialize`/`Deserialize` and is never written into
/// `zzop_cache::FileIrSlice`, the cache envelope, or any wire-format enum, so renaming a variant is
/// cache-safe on its own. A change to a language's projected `FileIrSlice` *shape*, however, still
/// requires a `CACHE_SCHEMA_VERSION` bump (`cache.rs`).
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
/// package-manager bundle, which is not project source. `zzop-reports` and `.zzop-cache` are zzop's OWN
/// output dir names (the removed JS CLI wrote reports to `<root>/zzop-reports/` and defaulted `cacheDir`
/// to `.zzop-cache`, both inside the analyzed tree) — without this entry, such a dir sitting inside the
/// analyzed tree gets walked as source on the NEXT run (self-scan pollution: the reports dir's file count
/// grows every run, observed live in a blind field test).
const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".git",
    "target",
    ".yarn",
    "zzop-reports",
    ".zzop-cache",
];

/// Extensions this engine deliberately never names in the "bring an adapter" per-extension disclosure
/// (`analyze::diagnostics::unparsed_extension_warning`) — non-source file types where a dispatch-`None`
/// result is simply correct, not a coverage gap: there is no code in a `.png` or a `.lock` file to extract
/// io/symbol facts from. A mechanism list (like `DEFAULT_SKIP_DIRS` above), not rule vocabulary — nothing
/// here names a rule or pack id, it only gates which extensions are worth surfacing as an unparsed-language
/// signal. Grouped by kind, one comment line per group:
/// - docs/text: prose, never source.
/// - data/config: structured data a DSL `IoScan`/`SymbolScan` matcher has no symbols/io to key on.
/// - styles: presentation, not logic.
/// - markup-as-asset: plain `.html`/`.htm` are static assets in most trees this engine analyzes (SSR
///   template dialects are the exception — `.jsp`/`.erb`/`.vue`/`.svelte` are deliberately NOT listed
///   here, since those ARE plausible adapter targets and should still warn).
/// - images/fonts/media/binaries+archives: no text to parse at all.
/// - misc: certificates — data, not code.
const NON_SOURCE_EXTENSIONS: &[&str] = &[
    // docs/text
    "md",
    "mdx",
    "txt",
    "rst",
    "adoc",
    // data/config
    "json",
    "jsonc",
    "json5",
    "yaml",
    "yml",
    "toml",
    "xml",
    "csv",
    "tsv",
    "ini",
    "properties",
    "lock",
    // styles
    "css",
    "scss",
    "sass",
    "less",
    "styl",
    // markup-as-asset
    "html",
    "htm",
    // images
    "png",
    "jpg",
    "jpeg",
    "gif",
    "webp",
    "svg",
    "ico",
    "bmp",
    "avif",
    // fonts
    "woff",
    "woff2",
    "ttf",
    "otf",
    "eot",
    // media
    "mp3",
    "mp4",
    "webm",
    "wav",
    "ogg",
    "mov",
    // binaries/archives
    "zip",
    "gz",
    "tar",
    "pdf",
    "wasm",
    "exe",
    "dll",
    "so",
    "dylib",
    "node",
    "jar",
    "map",
    // misc
    "pem",
    "crt",
];

/// True if `ext` names a non-source file type (`NON_SOURCE_EXTENSIONS`) — the filter
/// `unparsed_extension_warning`'s collection step applies before naming a dispatch-`None` extension as a
/// coverage gap. Case-insensitive, mirroring `dispatch_by_extension`'s own `to_ascii_lowercase`
/// normalization (the caller is not required to pre-lowercase `ext`).
pub fn is_non_source_extension(ext: &str) -> bool {
    NON_SOURCE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
}

/// Configures the dispatcher: path-glob overrides (checked first, in list order — first match wins) and
/// which directory names to skip while walking a tree.
#[derive(Debug, Clone)]
pub struct DispatchConfig {
    /// `(glob, language)` — a path matching `glob` (see `matches_glob`) is dispatched to `language`
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
        if matches_glob(rel_path, glob) {
            return Some(*lang);
        }
    }
    dispatch_by_extension(rel_path)
}

fn dispatch_by_extension(rel_path: &str) -> Option<Language> {
    let ext = Path::new(rel_path)
        .extension()
        .and_then(|e| e.to_str())?
        .to_ascii_lowercase();
    match ext.as_str() {
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

/// True if `name` (a single path component — a directory's own name, not a full path) is one of
/// `config.skip_dirs`. Exact match against the directory's own name (not a glob).
pub fn is_skip_dir(name: &str, config: &DispatchConfig) -> bool {
    config.skip_dirs.iter().any(|d| d == name)
}

/// Minimal glob: "**" matches any characters (including "/"), "*" matches non-slash characters.
/// Reimplemented here rather than imported — `core::recommendations`'s equivalent is a private helper
/// with no public home.
fn matches_glob(path: &str, glob: &str) -> bool {
    let mut escaped = String::with_capacity(glob.len());
    for c in glob.chars() {
        if matches!(
            c,
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    const DOUBLE_STAR_PLACEHOLDER: char = '\u{0}';
    let rewritten = escaped
        .replace("**", &DOUBLE_STAR_PLACEHOLDER.to_string())
        .replace('*', "[^/]*")
        .replace(DOUBLE_STAR_PLACEHOLDER, ".*");
    let anchored = format!("^{rewritten}$");
    regex::Regex::new(&anchored)
        .map(|re| re.is_match(path))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests;
