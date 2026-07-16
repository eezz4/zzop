//! tsconfig `compilerOptions.paths`/`baseUrl` resolution: per-directory `TsconfigPaths` config,
//! nearest-ancestor governing lookup, and the `paths`-pattern / plain-`baseUrl` resolvers.

use std::collections::{BTreeMap, HashSet};

use super::specifier::{normalize, try_ext};

/// One directory's effective TypeScript path-mapping config: `compilerOptions.baseUrl` (POSIX dir
/// relative to the analysis root, `""` for root) and `compilerOptions.paths` (alias pattern -> ordered
/// target list, joined against `base_url` only at resolution time). Built from tsconfig.json (+ one
/// local `extends` level) by `zzop-engine`'s `pipeline::tsconfig_scan`; stays pure/filesystem-free.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TsconfigPaths {
    pub base_url: String,
    pub paths: BTreeMap<String, Vec<String>>,
}

/// POSIX dirname, `""`-for-root (matches `TsconfigPaths` keys) — distinct from `specifier`'s
/// `dirname`, which returns `"."` for a no-slash path.
fn dirname_posix(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

/// Nearest-ancestor lookup: walks up from `from_file`'s directory to the analysis root, returning the
/// first registered `TsconfigPaths` (mirrors how `tsc` finds the governing tsconfig).
pub(super) fn governing_tsconfig<'a>(
    from_file: &str,
    tsconfigs: &'a BTreeMap<String, TsconfigPaths>,
) -> Option<&'a TsconfigPaths> {
    let mut dir = dirname_posix(from_file);
    loop {
        if let Some(cfg) = tsconfigs.get(&dir) {
            return Some(cfg);
        }
        if dir.is_empty() {
            return None;
        }
        dir = dirname_posix(&dir);
    }
}

/// Resolves `specifier` against one tsconfig's `paths` map, TS semantics: longest-matching-prefix wins;
/// within the winning pattern, targets are tried in declared order and the first that resolves via
/// `try_ext` wins. Returns `None` if no pattern matches or all targets fail — mirrors `tsc`, which never
/// falls back to a shorter-prefix pattern.
pub(super) fn resolve_via_paths(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    all_paths: &HashSet<String>,
) -> Option<String> {
    let mut candidates: Vec<(&str, &str, &Vec<String>)> = Vec::new();
    for (pattern, targets) in &tsconfig.paths {
        match pattern.find('*') {
            None => {
                if specifier == pattern.as_str() {
                    candidates.push((pattern.as_str(), "", targets));
                }
            }
            Some(star) => {
                let prefix = &pattern[..star];
                let suffix = &pattern[star + 1..];
                if specifier.len() >= prefix.len() + suffix.len()
                    && specifier.starts_with(prefix)
                    && specifier.ends_with(suffix)
                {
                    candidates.push((prefix, suffix, targets));
                }
            }
        }
    }
    // Longest prefix first; ties broken by `paths`' BTreeMap-deterministic alphabetical order.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.0.len()));
    for (prefix, suffix, targets) in candidates {
        let captured = &specifier[prefix.len()..specifier.len() - suffix.len()];
        for target in targets {
            let filled = match target.find('*') {
                Some(i) => format!("{}{captured}{}", &target[..i], &target[i + 1..]),
                None => target.clone(),
            };
            let base = if tsconfig.base_url.is_empty() {
                filled
            } else {
                format!("{}/{filled}", tsconfig.base_url)
            };
            if let Some(hit) = try_ext(&normalize(&base), all_paths) {
                return Some(hit);
            }
        }
    }
    None
}

/// TS also resolves a non-relative specifier plainly against `baseUrl` (e.g. `'foo/bar'` ->
/// `<baseUrl>/foo/bar`), tried after any `paths` match. No separate "is baseUrl configured" guard
/// needed — an unconfigured directory is never registered.
pub(super) fn resolve_via_base_url(
    specifier: &str,
    tsconfig: &TsconfigPaths,
    all_paths: &HashSet<String>,
) -> Option<String> {
    let base = if tsconfig.base_url.is_empty() {
        specifier.to_string()
    } else {
        format!("{}/{specifier}", tsconfig.base_url)
    };
    try_ext(&normalize(&base), all_paths)
}

#[cfg(test)]
mod tests {
    //! `resolve_via_paths` / `resolve_via_base_url` / `governing_tsconfig`, exercised through the
    //! public `resolve_file_with_workspace` entry point.
    use std::collections::{BTreeMap, HashMap};

    use super::TsconfigPaths;
    use crate::lang::resolve::resolve_file_with_workspace;
    use crate::lang::resolve::test_util::{btree, paths, tsconfigs, ws_pkgs};

    #[test]
    fn tsconfig_star_pattern_resolves_via_base_url() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: btree(&[("@/*", &["*"])]),
            },
        )]);
        let all = paths(&["src/features/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "@/features/x",
                "anywhere/deep.ts",
                &all,
                &HashMap::new(),
                &cfgs
            )
            .as_deref(),
            Some("src/features/x.ts")
        );
    }

    #[test]
    fn tsconfig_exact_pattern_resolves() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("shims", &["src/shims.ts"])]),
            },
        )]);
        let all = paths(&["src/shims.ts"]);
        assert_eq!(
            resolve_file_with_workspace("shims", "a.ts", &all, &HashMap::new(), &cfgs).as_deref(),
            Some("src/shims.ts")
        );
    }

    #[test]
    fn tsconfig_longest_prefix_wins_over_shorter_wildcard() {
        // Both `@/*` and `@/utils/*` match `@/utils/format` — the longer, more specific prefix must win.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[
                    ("@/*", &["src/generic/*"]),
                    ("@/utils/*", &["src/special-utils/*"]),
                ]),
            },
        )]);
        let all = paths(&["src/generic/utils/format.ts", "src/special-utils/format.ts"]);
        assert_eq!(
            resolve_file_with_workspace("@/utils/format", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/special-utils/format.ts")
        );
    }

    #[test]
    fn tsconfig_bare_specifier_resolves_via_base_url_without_a_paths_entry() {
        // Resolves against `baseUrl` even when no `paths` pattern matches — the "absolute-from-src" convention.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: BTreeMap::new(),
            },
        )]);
        let all = paths(&["src/utils/format.ts"]);
        assert_eq!(
            resolve_file_with_workspace("utils/format", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/utils/format.ts")
        );
    }

    #[test]
    fn tsconfig_nearest_ancestor_governs_nested_file() {
        // `packages/app`'s own tsconfig governs files under it, not the root tsconfig, even when both are registered.
        let cfgs = tsconfigs(&[
            (
                "",
                TsconfigPaths {
                    base_url: "".to_string(),
                    paths: btree(&[("@/*", &["root-src/*"])]),
                },
            ),
            (
                "packages/app",
                TsconfigPaths {
                    base_url: "packages/app/src".to_string(),
                    paths: BTreeMap::new(),
                },
            ),
        ]);
        let all = paths(&["packages/app/src/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "x",
                "packages/app/deep/file.ts",
                &all,
                &HashMap::new(),
                &cfgs
            )
            .as_deref(),
            Some("packages/app/src/x.ts")
        );
    }

    #[test]
    fn tsconfig_relative_specifier_is_never_remapped_by_paths() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("./bar", &["somewhere/else.ts"])]),
            },
        )]);
        let all = paths(&["a/bar.ts"]);
        assert_eq!(
            resolve_file_with_workspace("./bar", "a/x.ts", &all, &HashMap::new(), &cfgs).as_deref(),
            Some("a/bar.ts")
        );
    }

    #[test]
    fn tsconfig_non_matching_specifier_falls_through_to_workspace_then_external() {
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "".to_string(),
                paths: btree(&[("@/*", &["src/*"])]),
            },
        )]);
        let all = paths(&["packages/utils-core/src/index.ts"]);
        // Doesn't match `@/*`, but does name a workspace package — still resolves via `ws_pkgs`.
        assert_eq!(
            resolve_file_with_workspace("@acme/utils-core", "a.ts", &all, &ws_pkgs(), &cfgs)
                .as_deref(),
            Some("packages/utils-core/src/index.ts")
        );
        // Doesn't match `@/*` or any workspace package -> external.
        assert_eq!(
            resolve_file_with_workspace("react", "a.ts", &all, &ws_pkgs(), &cfgs),
            None
        );
    }

    #[test]
    fn tsconfig_paths_pattern_match_with_unresolvable_target_falls_through_to_base_url() {
        // `features/*` matches, but its target `nowhere/*` doesn't resolve — falls through to
        // baseUrl-relative resolution of the original specifier, unlike real `tsc`.
        let cfgs = tsconfigs(&[(
            "",
            TsconfigPaths {
                base_url: "src".to_string(),
                paths: btree(&[("features/*", &["nowhere/*"])]),
            },
        )]);
        let all = paths(&["src/features/x.ts"]);
        assert_eq!(
            resolve_file_with_workspace("features/x", "a.ts", &all, &HashMap::new(), &cfgs)
                .as_deref(),
            Some("src/features/x.ts")
        );
    }
}
