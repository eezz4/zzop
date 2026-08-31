//! Workspace-package matching and the `resolve_file_with_workspace` entry point, including the pin that
//! the framework-reserved alias lane is actually REACHED from here.
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
        resolve_file_with_workspace("@acme/no-entry", "a.ts", &all, &ws_pkgs(), &no_tsconfigs()),
        None
    );
}

#[test]
fn resolve_file_with_workspace_strips_bundler_resource_query_on_a_pkg_subpath() {
    // The workspace/tsconfig branches match specifier TEXT, so the `?worker` strip has to happen
    // before them too — a cross-package worker entry (`@acme/utils-core/w?worker`) is imported only
    // this way, and an unresolved import means a `dead-candidates` FP on the worker file.
    let all = paths(&[
        "packages/utils-core/src/index.ts",
        "packages/utils-core/w.ts",
    ]);
    assert_eq!(
        resolve_file_with_workspace(
            "@acme/utils-core/w?worker",
            "a.ts",
            &all,
            &ws_pkgs(),
            &no_tsconfigs()
        )
        .as_deref(),
        Some("packages/utils-core/w.ts")
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
        resolve_file_with_workspace("left-pad", "a.ts", &all, &pkgs, &no_tsconfigs()).as_deref(),
        Some("packages/left-pad/index.ts")
    );
}

/// The pin that the Nuxt lane is REACHED, not merely correct. `framework_alias`'s own tests passed
/// while this caller still carried a hand-written alias list that did not include `~/`, so every
/// Nuxt specifier fell through to package matching and was recorded as an external dependency —
/// dead code with a green suite. Measured on nocodb: `dead-candidates` sat at 346 before and after
/// the module existed, and moved to 321 only once this entry point asked for it.
#[test]
fn a_nuxt_alias_resolves_through_the_workspace_entry_point() {
    let all = paths(&[
        "packages/nc-gui/nuxt.config.ts",
        "packages/nc-gui/components/monaco/json.ts",
        "packages/nc-gui/components/webhook/index.vue",
    ]);
    assert_eq!(
        resolve_file_with_workspace(
            "~/components/monaco/json",
            "packages/nc-gui/components/webhook/index.vue",
            &all,
            &ws_pkgs(),
            &no_tsconfigs(),
        ),
        Some("packages/nc-gui/components/monaco/json.ts".to_string())
    );
}

/// A tsconfig that explicitly remaps the spelling still wins — the reserved-alias branch sits AFTER
/// the governing-tsconfig branch, and adding Nuxt to it must not have reordered that.
#[test]
fn a_nuxt_alias_with_no_config_above_it_stays_external() {
    let all = paths(&["src/components/x.ts"]);
    assert_eq!(
        resolve_file_with_workspace(
            "~/components/x",
            "src/main.ts",
            &all,
            &ws_pkgs(),
            &no_tsconfigs(),
        ),
        None
    );
}
