//! Workspace (monorepo) package resolution and the workspace-/tsconfig-aware
//! `resolve_file_with_workspace` entry point.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::specifier::{resolve_file, strip_resource_query, try_ext};
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
    // A bundler resource query (`?worker`, `?url`, ...) is stripped here too, not only in `resolve_file`
    // below: the tsconfig-`paths`/`baseUrl`/workspace-package branches match the specifier TEXT, so an
    // aliased `@/workers/w?worker` would otherwise miss every one of them.
    let specifier = strip_resource_query(specifier);
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
    if specifier.starts_with("@/") || super::framework_alias::is_framework_alias(specifier) {
        // `@/` plus every framework-RESERVED alias (SvelteKit's `$lib`, Nuxt's `~`/`~~`/`@@`) are
        // resolved by `resolve_file` — after the governing tsconfig gets first say above, so a project
        // that explicitly remaps them wins. The reserved set is ASKED for rather than re-spelled here:
        // this line used to carry its own copy of it, which meant a `~/` specifier fell through to
        // package matching, matched nothing, and was recorded as an external dependency — so the whole
        // Nuxt lane was unreachable while its own unit tests passed.
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
mod tests;
