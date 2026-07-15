//! Language dispatcher — decides which parser frontend (if any) handles a file, purely from its path.
//! Extension map first, then a path-glob override list that can force a specific language regardless of
//! extension.
//!
//! A file matching neither an override nor a known extension still flows through the fused pipeline as
//! a lexical-only `SourceFile` — no symbols/imports/io, but still scanned by line-scan DSL rules. `None`
//! means "no structural parser", not "ignore this file".
//!
//! `.java` gets the same "no real parser, still worth spans" treatment via `Language::JavaLexical`,
//! routed to `zzop_parser_java::parse_method_spans` — a comment/string-aware brace matcher, not a real
//! grammar — so `Matcher::MethodScan` rules still get class/method spans. `.jsp`/`.jspx`/`.tag` stay on
//! the `None` path: JSP embeds Java inside HTML-like markup, a shape the brace matcher isn't built to
//! disentangle.

use std::path::Path;

/// A source language this engine has a parser frontend for. `TypeScript`/`Prisma` are real structural
/// parsers; `JavaLexical` is the lexical brace-matcher (`zzop_parser_java`, see module doc). JSP/Python
/// parser crates exist in the workspace but are out of scope: files routed to them by extension get no
/// `Language` match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    Prisma,
    JavaLexical,
}

/// Directory names skipped entirely during the tree walk: common Node-ecosystem build/dependency dirs,
/// plus this workspace's own build output dir (`target`). `.yarn` covers Yarn Berry's vendored
/// package-manager bundle, which is not project source.
const DEFAULT_SKIP_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".git",
    "target",
    ".yarn",
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
        "java" => Some(Language::JavaLexical),
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
mod tests {
    use super::*;

    fn cfg() -> DispatchConfig {
        DispatchConfig::default()
    }

    #[test]
    fn dispatches_known_typescript_extensions() {
        for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
            let path = format!("src/x.{ext}");
            assert_eq!(
                dispatch(&path, &cfg()),
                Some(Language::TypeScript),
                "{path}"
            );
        }
    }

    #[test]
    fn dispatches_prisma_extension() {
        assert_eq!(dispatch("db/schema.prisma", &cfg()), Some(Language::Prisma));
    }

    #[test]
    fn unknown_extension_dispatches_to_none() {
        assert_eq!(dispatch("README", &cfg()), None);
        // .jsp/.jspx/.tag stay lexical-fallback; only .java gets the lexical projector.
        assert_eq!(dispatch("src/Foo.jsp", &cfg()), None);
    }

    #[test]
    fn dispatches_java_extension_to_the_lexical_projector() {
        assert_eq!(
            dispatch("src/Foo.java", &cfg()),
            Some(Language::JavaLexical)
        );
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert_eq!(dispatch("src/Foo.TS", &cfg()), Some(Language::TypeScript));
    }

    #[test]
    fn glob_override_wins_over_extension_map() {
        let config = DispatchConfig {
            glob_overrides: vec![("legacy/**".to_string(), Language::Prisma)],
            ..cfg()
        };
        // `.ts` would normally be TypeScript, but the override forces the whole `legacy/` subtree to Prisma.
        assert_eq!(
            dispatch("legacy/schema.ts", &config),
            Some(Language::Prisma)
        );
        assert_eq!(
            dispatch("fresh/schema.ts", &config),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn glob_single_star_does_not_cross_slash() {
        let config = DispatchConfig {
            glob_overrides: vec![("src/*.ts".to_string(), Language::Prisma)],
            ..cfg()
        };
        assert_eq!(dispatch("src/Foo.ts", &config), Some(Language::Prisma));
        // `*` must not cross `/`, so the override doesn't apply here — falls through to the extension map.
        assert_eq!(
            dispatch("src/nested/Foo.ts", &config),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn default_skip_dirs_cover_common_build_and_vcs_output() {
        let config = cfg();
        for name in [
            "node_modules",
            "dist",
            "build",
            ".next",
            ".git",
            "target",
            ".yarn",
        ] {
            assert!(is_skip_dir(name, &config), "{name}");
        }
        assert!(!is_skip_dir("src", &config));
    }

    /// T2 policy pin: the exact `NON_SOURCE_EXTENSIONS` contents. Any edit to this list changes which
    /// extensions the "bring an adapter" per-extension disclosure stays silent about — pinned so a change
    /// is a conscious, reviewed decision, not an accidental drop/add.
    #[test]
    fn non_source_extensions_pin() {
        const EXPECTED: &[&str] = &[
            // docs/text
            "md",
            "mdx",
            "txt",
            "rst",
            "adoc", // data/config
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
            "lock", // styles
            "css",
            "scss",
            "sass",
            "less",
            "styl", // markup-as-asset
            "html",
            "htm", // images
            "png",
            "jpg",
            "jpeg",
            "gif",
            "webp",
            "svg",
            "ico",
            "bmp",
            "avif", // fonts
            "woff",
            "woff2",
            "ttf",
            "otf",
            "eot", // media
            "mp3",
            "mp4",
            "webm",
            "wav",
            "ogg",
            "mov", // binaries/archives
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
        assert_eq!(
            NON_SOURCE_EXTENSIONS, EXPECTED,
            "NON_SOURCE_EXTENSIONS drifted — update EXPECTED deliberately if this is an intended policy \
             change"
        );
    }

    #[test]
    fn is_non_source_extension_matches_every_pinned_entry() {
        for ext in NON_SOURCE_EXTENSIONS {
            assert!(is_non_source_extension(ext), "{ext}");
        }
    }

    #[test]
    fn is_non_source_extension_is_case_insensitive() {
        assert!(is_non_source_extension("MD"));
        assert!(is_non_source_extension("Png"));
    }

    #[test]
    fn is_non_source_extension_rejects_real_source_and_template_dialects() {
        for ext in ["ts", "py", "sql", "rb", "go"] {
            assert!(!is_non_source_extension(ext), "{ext}");
        }
        // Template dialects that embed source in markup — deliberately NOT in the non-source list, since
        // an adapter for these is a real, plausible gap worth naming.
        for ext in ["jsp", "erb", "vue", "svelte"] {
            assert!(!is_non_source_extension(ext), "{ext}");
        }
    }
}
