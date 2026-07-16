//! Import specifier resolution + dep-graph projection: relative (`./` `../`) and `@/` alias specifiers
//! resolve to an internal file path; workspace-package specifiers (`WorkspacePkg`) resolve to that
//! package's entry/subpath file; specifiers matching a governing tsconfig's `paths`/`baseUrl`
//! (`TsconfigPaths`) resolve to the mapped target (tried before the `@/` and workspace fallbacks). Bare
//! npm packages and node builtins are external -> `None`, dropped from the dep graph.
//!
//! Split by concern: `specifier` (relative/`@/` resolution + extension probing + path helpers),
//! `tsconfig` (`paths`/`baseUrl` mapping), `workspace` (monorepo packages + the combined resolver),
//! `dep_graph` (the `build_dep`/`build_dep_with_workspace` projection).

mod dep_graph;
mod specifier;
mod tsconfig;
mod workspace;

pub use dep_graph::{build_dep, build_dep_with_workspace};
pub use specifier::{resolve_file, try_ext, RESOLVE_EXTS};
pub use tsconfig::TsconfigPaths;
pub use workspace::{match_workspace_pkg, resolve_file_with_workspace, WorkspacePkg};

/// Fixture builders shared by the submodules' test modules.
#[cfg(test)]
pub(crate) mod test_util {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use super::{TsconfigPaths, WorkspacePkg};

    pub fn paths(xs: &[&str]) -> HashSet<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    pub fn no_tsconfigs() -> BTreeMap<String, TsconfigPaths> {
        BTreeMap::new()
    }

    pub fn ws_pkgs() -> HashMap<String, WorkspacePkg> {
        [
            (
                "@acme/utils-core",
                WorkspacePkg {
                    dir: "packages/utils-core".to_string(),
                    entry: Some("packages/utils-core/src/index.ts".to_string()),
                },
            ),
            (
                "@acme/no-entry",
                WorkspacePkg {
                    dir: "packages/no-entry".to_string(),
                    entry: None,
                },
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    pub fn tsconfigs(entries: &[(&str, TsconfigPaths)]) -> BTreeMap<String, TsconfigPaths> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    pub fn btree(entries: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }
}
