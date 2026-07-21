//! Base specifier resolution: relative/`@/`-alias resolution against the known file set, the
//! extension/index probing order, and the POSIX path helpers (`dirname`/`normalize`).

use std::collections::HashSet;

/// Extensions / index files tried in order when resolving a specifier base.
pub const RESOLVE_EXTS: &[&str] = &[
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".cjs",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
];

/// Resolve a specifier to an internal file path within `all_paths`, or `None`. Relative (`.`/`..`) is
/// joined against `from_file`'s dir; `@/` is a repo-root alias; everything else is external -> `None`.
pub fn resolve_file(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
) -> Option<String> {
    if specifier.starts_with('.') {
        let joined = normalize(&format!("{}/{}", dirname(from_file), specifier));
        return try_ext(&joined, all_paths);
    }
    if let Some(rest) = specifier.strip_prefix("@/") {
        // Root-relative first (tsconfig maps `@/*` to the analysis root), then `src/`-relative — the
        // dominant convention is `"@/*": ["./src/*"]`, so without this fallback every `@/` import
        // breaks and dead-exports/unreachable analysis misreports the whole `src/` tree as orphaned.
        // tsconfig `paths` isn't read here (yet); this covers the two conventional mappings, root first.
        return try_ext(rest, all_paths).or_else(|| try_ext(&format!("src/{rest}"), all_paths));
    }
    // SvelteKit reserves `$lib` for `src/lib` — a built-in, non-configurable alias every SvelteKit app
    // relies on. It is normally wired through the generated `.svelte-kit/tsconfig.json`, which is absent
    // from a fresh checkout (created by `svelte-kit sync` at build time), so resolve it directly here.
    // Without this, `$lib/*` imports from `.svelte` components and `+page.server.js` routes all fail to
    // resolve and dead-exports/dead-candidates misreport the whole `src/lib` tree as orphaned.
    if specifier == "$lib" {
        return try_ext("src/lib", all_paths);
    }
    if let Some(rest) = specifier.strip_prefix("$lib/") {
        return try_ext(&format!("src/lib/{rest}"), all_paths);
    }
    None
}

/// NodeNext-style literal extension -> real TypeScript source extension(s): `.js`/`.mjs`/`.cjs` imports
/// commonly name compiled output while the real source is `.ts`/`.tsx`, `.mts`, or `.cts`.
const EXTENSION_FALLBACKS: &[(&str, &[&str])] = &[
    (".js", &[".ts", ".tsx"]),
    (".mjs", &[".mts"]),
    (".cjs", &[".cts"]),
];

/// Try each extension/index suffix against `all_paths` (see `EXTENSION_FALLBACKS` for the NodeNext
/// `.js`/`.mjs`/`.cjs` -> real-source fallback).
pub fn try_ext(base: &str, all_paths: &HashSet<String>) -> Option<String> {
    for ext in RESOLVE_EXTS {
        let candidate = format!("{base}{ext}");
        if all_paths.contains(&candidate) {
            return Some(candidate);
        }
        if ext.is_empty() {
            for (literal, reals) in EXTENSION_FALLBACKS {
                let Some(stem) = base.strip_suffix(literal) else {
                    continue;
                };
                for real in *reals {
                    let c = format!("{stem}{real}");
                    if all_paths.contains(&c) {
                        return Some(c);
                    }
                }
            }
        }
    }
    None
}

/// POSIX dirname: text before the last '/', or "." when there is no '/'.
fn dirname(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => ".".to_string(),
    }
}

/// POSIX normalize: resolve "." and ".." segments (relative paths; leading ".." is preserved).
pub(super) fn normalize(p: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if matches!(stack.last(), Some(&s) if s != "..") {
                    stack.pop();
                } else {
                    stack.push("..");
                }
            }
            s => stack.push(s),
        }
    }
    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::resolve_file;
    use crate::lang::resolve::test_util::paths;

    #[test]
    fn resolves_relative_to_ts() {
        let all = paths(&["features/x/bar.ts"]);
        assert_eq!(
            resolve_file("./bar", "features/x/useFoo.ts", &all).as_deref(),
            Some("features/x/bar.ts")
        );
    }

    #[test]
    fn resolves_index_file() {
        let all = paths(&["a/shared/index.ts"]);
        assert_eq!(
            resolve_file("./shared", "a/b.ts", &all).as_deref(),
            Some("a/shared/index.ts")
        );
    }

    #[test]
    fn maps_js_specifier_to_ts_source() {
        let all = paths(&["a/bar.ts"]);
        assert_eq!(
            resolve_file("./bar.js", "a/b.ts", &all).as_deref(),
            Some("a/bar.ts")
        );
    }

    #[test]
    fn maps_mjs_specifier_to_mts_source() {
        let all = paths(&["a/bar.mts"]);
        assert_eq!(
            resolve_file("./bar.mjs", "a/b.ts", &all).as_deref(),
            Some("a/bar.mts")
        );
    }

    #[test]
    fn maps_cjs_specifier_to_cts_source() {
        let all = paths(&["a/bar.cts"]);
        assert_eq!(
            resolve_file("./bar.cjs", "a/b.ts", &all).as_deref(),
            Some("a/bar.cts")
        );
    }

    #[test]
    fn resolves_at_alias() {
        let all = paths(&["features/x.ts"]);
        assert_eq!(
            resolve_file("@/features/x", "anywhere/deep.ts", &all).as_deref(),
            Some("features/x.ts")
        );
    }

    #[test]
    fn resolves_at_alias_through_src_fallback() {
        let all = paths(&["src/core/blocklist.ts"]);
        assert_eq!(
            resolve_file("@/core/blocklist", "src/background/recording.ts", &all).as_deref(),
            Some("src/core/blocklist.ts")
        );
    }

    #[test]
    fn at_alias_prefers_root_match_over_src_fallback() {
        let all = paths(&["features/x.ts", "src/features/x.ts"]);
        assert_eq!(
            resolve_file("@/features/x", "a/b.ts", &all).as_deref(),
            Some("features/x.ts")
        );
    }

    #[test]
    fn resolves_sveltekit_lib_alias() {
        let all = paths(&["src/lib/api.ts", "src/lib/constants.js"]);
        // `$lib/api` -> src/lib/api.ts; `$lib/constants.js` -> src/lib/constants.js (literal ext kept).
        assert_eq!(
            resolve_file("$lib/api", "src/routes/+page.svelte", &all).as_deref(),
            Some("src/lib/api.ts")
        );
        assert_eq!(
            resolve_file(
                "$lib/constants.js",
                "src/routes/x/CommentInput.svelte",
                &all
            )
            .as_deref(),
            Some("src/lib/constants.js")
        );
    }

    #[test]
    fn resolves_bare_sveltekit_lib_to_index() {
        let all = paths(&["src/lib/index.ts"]);
        assert_eq!(
            resolve_file("$lib", "src/routes/+layout.svelte", &all).as_deref(),
            Some("src/lib/index.ts")
        );
    }

    #[test]
    fn normalizes_parent_segments() {
        let all = paths(&["features/shared/y.ts"]);
        assert_eq!(
            resolve_file("../shared/y", "features/x/useFoo.ts", &all).as_deref(),
            Some("features/shared/y.ts")
        );
    }

    #[test]
    fn external_specifier_is_none() {
        assert_eq!(resolve_file("react", "a/b.ts", &paths(&["a/b.ts"])), None);
    }

    #[test]
    fn unresolvable_relative_is_none() {
        assert_eq!(resolve_file("./missing", "a/b.ts", &paths(&[])), None);
    }
}
