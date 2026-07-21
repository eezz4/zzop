//! Workspace (monorepo) package resolution and the workspace-/tsconfig-aware
//! `resolve_file_with_workspace` entry point.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::specifier::{resolve_file, try_ext};
use super::tsconfig::{governing_tsconfig, resolve_via_base_url, resolve_via_paths, TsconfigPaths};

/// A workspace (monorepo) package as seen by the import resolver, resolving both a bare `<name>` and a
/// `<name>/subpath` specifier. Built by `zzop-engine`'s `pipeline::package_json_entries` from `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePkg {
    /// The package's own directory (POSIX, relative to the analysis root; `""` for root-level) — resolves
    /// `<name>/subpath` specifiers via `try_ext` the same way a relative import resolves.
    pub dir: String,
    /// The package's resolved bare-specifier entry file (`main`/`module`/`exports["."]`, falling back to
    /// `index.ts`/`src/index.ts`). `None` if nothing resolves — `<name>/subpath` still resolves via `dir`.
    pub entry: Option<String>,
}

/// Checks whether an import specifier matches a workspace package name (exact or sub-path) against
/// `workspace_pkgs`' keys, generic over the value type. Examples:
///  - `@base/utils-fe`           -> `Some(("@base/utils-fe", None))`
///  - `@base/utils-fe/auth/hash` -> `Some(("@base/utils-fe", Some("auth/hash")))`
///  - `@/foo/bar`                -> `None` (`@/` is the path alias, handled separately)
///  - `react`                    -> `None`
pub fn match_workspace_pkg<'a, V>(
    specifier: &'a str,
    workspace_pkgs: &'a HashMap<String, V>,
) -> Option<(&'a str, Option<&'a str>)> {
    if let Some((k, _)) = workspace_pkgs.get_key_value(specifier) {
        return Some((k.as_str(), None));
    }
    // `@/foo` (no second '/') falls through below; unscoped packages (`lodash/fp`) use text after the first '/'.
    let (pkg_part, rest_start) = if !specifier.starts_with('@') {
        let slash = specifier.find('/')?;
        (&specifier[..slash], slash + 1)
    } else {
        let first_slash = specifier.find('/')?;
        let second_slash = specifier[first_slash + 1..]
            .find('/')
            .map(|i| i + first_slash + 1)?;
        (&specifier[..second_slash], second_slash + 1)
    };
    let (k, _) = workspace_pkgs.get_key_value(pkg_part)?;
    let rest = &specifier[rest_start..];
    Some((k.as_str(), (!rest.is_empty()).then_some(rest)))
}

/// `resolve_file`, aware of workspace packages and tsconfig `paths`/`baseUrl`. Non-relative specifiers
/// try, in order: the governing tsconfig's `paths`, then its `baseUrl`, then the `@/` convention, then
/// workspace packages; a relative specifier always resolves exactly as `resolve_file` (`paths` never
/// remaps one). A bare `<name>` resolves to `WorkspacePkg::entry`; `<name>/subpath` resolves to
/// `dir/subpath` via `try_ext`. Workspace packages win over a same-named npm dependency since
/// `all_paths` never contains `node_modules`.
pub fn resolve_file_with_workspace(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<String>,
    workspace_pkgs: &HashMap<String, WorkspacePkg>,
    tsconfigs: &BTreeMap<String, TsconfigPaths>,
) -> Option<String> {
    if specifier.starts_with('.') {
        return resolve_file(specifier, from_file, all_paths);
    }
    if let Some(cfg) = governing_tsconfig(from_file, tsconfigs) {
        if let Some(hit) = resolve_via_paths(specifier, cfg, all_paths) {
            return Some(hit);
        }
        if let Some(hit) = resolve_via_base_url(specifier, cfg, all_paths) {
            return Some(hit);
        }
    }
    if specifier.starts_with("@/") || specifier == "$lib" || specifier.starts_with("$lib/") {
        // `@/` and SvelteKit's `$lib` are built-in root aliases resolved by `resolve_file` — after the
        // governing tsconfig gets first say above (a project that explicitly remaps them wins).
        return resolve_file(specifier, from_file, all_paths);
    }
    let (pkg_name, subpath) = match_workspace_pkg(specifier, workspace_pkgs)?;
    let pkg = &workspace_pkgs[pkg_name];
    match subpath {
        None => pkg.entry.clone(),
        Some(sub) => {
            let base = if pkg.dir.is_empty() {
                sub.to_string()
            } else {
                format!("{}/{sub}", pkg.dir)
            };
            try_ext(&base, all_paths)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{match_workspace_pkg, resolve_file_with_workspace, WorkspacePkg};
    use crate::lang::resolve::test_util::{no_tsconfigs, paths, ws_pkgs};

    // --- matchWorkspacePkg ---

    /// `HashMap<String, ()>`: `match_workspace_pkg` is generic over the value type; these tests don't
    /// need `WorkspacePkg`'s `dir`/`entry` fields.
    fn pkgs() -> HashMap<String, ()> {
        ["@acme/utils-core", "@acme/utils-shared", "lodash"]
            .into_iter()
            .map(|s| (s.to_string(), ()))
            .collect()
    }

    #[test]
    fn workspace_pkg_exact_returns_as_is() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core", &pkgs()),
            Some(("@acme/utils-core", None))
        );
    }

    #[test]
    fn workspace_pkg_scoped_sub_path_matches_up_to_second_slash() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core/auth/hash", &pkgs()),
            Some(("@acme/utils-core", Some("auth/hash")))
        );
        assert_eq!(
            match_workspace_pkg("@acme/utils-shared/types", &pkgs()),
            Some(("@acme/utils-shared", Some("types")))
        );
    }

    #[test]
    fn workspace_pkg_scoped_trailing_slash_only_same_as_exact() {
        assert_eq!(
            match_workspace_pkg("@acme/utils-core/", &pkgs()),
            Some(("@acme/utils-core", None))
        );
    }

    #[test]
    fn workspace_pkg_non_scoped_sub_path_matches_up_to_first_slash() {
        assert_eq!(
            match_workspace_pkg("lodash/fp", &pkgs()),
            Some(("lodash", Some("fp")))
        );
    }

    #[test]
    fn workspace_pkg_at_alias_is_not_a_workspace_package() {
        assert_eq!(match_workspace_pkg("@/features/x", &pkgs()), None);
    }

    #[test]
    fn workspace_pkg_external_module_is_none() {
        assert_eq!(match_workspace_pkg("react", &pkgs()), None);
        assert_eq!(match_workspace_pkg("react/jsx-runtime", &pkgs()), None);
    }

    #[test]
    fn workspace_pkg_scoped_but_unregistered_is_none() {
        assert_eq!(match_workspace_pkg("@other/thing", &pkgs()), None);
        assert_eq!(match_workspace_pkg("@other/thing/sub", &pkgs()), None);
    }

    // --- resolve_file_with_workspace ---

    #[test]
    fn resolve_file_with_workspace_matches_relative_and_alias_like_resolve_file() {
        let all = paths(&["features/x/bar.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "./bar",
                "features/x/useFoo.ts",
                &all,
                &HashMap::new(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("features/x/bar.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_bare_specifier_resolves_to_package_entry() {
        let all = paths(&["packages/utils-core/src/index.ts"]);
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/utils-core",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("packages/utils-core/src/index.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_sub_path_specifier_resolves_via_dir_and_try_ext() {
        let all = paths(&[
            "packages/utils-core/src/index.ts",
            "packages/utils-core/auth/hash.ts",
        ]);
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/utils-core/auth/hash",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            )
            .as_deref(),
            Some("packages/utils-core/auth/hash.ts")
        );
    }

    #[test]
    fn resolve_file_with_workspace_bare_specifier_none_when_package_has_no_entry() {
        let all = paths(&["packages/no-entry/index.ts"]);
        // `@acme/no-entry`'s entry is `None` (no resolvable candidate) — a bare import has nowhere to
        // go, though the package directory is still reachable via an explicit sub-path.
        assert_eq!(
            resolve_file_with_workspace(
                "@acme/no-entry",
                "a.ts",
                &all,
                &ws_pkgs(),
                &no_tsconfigs()
            ),
            None
        );
    }

    #[test]
    fn resolve_file_with_workspace_external_still_none() {
        let all = paths(&["a.ts"]);
        assert_eq!(
            resolve_file_with_workspace("react", "a.ts", &all, &ws_pkgs(), &no_tsconfigs()),
            None
        );
    }

    #[test]
    fn resolve_file_with_workspace_wins_over_same_named_npm_dependency() {
        let mut pkgs = ws_pkgs();
        pkgs.insert(
            "left-pad".to_string(),
            WorkspacePkg {
                dir: "packages/left-pad".to_string(),
                entry: Some("packages/left-pad/index.ts".to_string()),
            },
        );
        let all = paths(&["packages/left-pad/index.ts"]);
        assert_eq!(
            resolve_file_with_workspace("left-pad", "a.ts", &all, &pkgs, &no_tsconfigs())
                .as_deref(),
            Some("packages/left-pad/index.ts")
        );
    }
}
