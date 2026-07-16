//! Tests for `trees: "auto"` workspace expansion — pure move of the former inline `mod tests` in
//! `workspaces.rs` (test files are exempt from the 300-line source cap).

use super::*;
use crate::test_support::TempDir;

fn pkg_json(name: Option<&str>) -> String {
    match name {
        Some(n) => format!(r#"{{"name": "{n}"}}"#),
        None => "{}".to_string(),
    }
}

fn auto_config() -> Value {
    json!({ "trees": "auto" })
}

// --- manifest precedence & minimal YAML forms -------------------------------------------

#[test]
fn pnpm_block_list_expands_to_matching_directories() {
    let dir = TempDir::new("zzop-ws-pnpm-block");
    dir.write(
        "pnpm-workspace.yaml",
        "packages:\n  - 'packages/*'\n  - 'apps/*'\n",
    );
    dir.write("packages/a/package.json", &pkg_json(Some("pkg-a")));
    dir.write("apps/x/package.json", &pkg_json(Some("app-x")));

    let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 2);
    let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
    assert_eq!(roots, vec!["apps/x", "packages/a"]); // sorted
    assert!(warnings
        .iter()
        .any(|w| w.contains("expanded to 2 tree(s) from pnpm-workspace.yaml")));
}

#[test]
fn pnpm_inline_flow_list_is_parsed() {
    let dir = TempDir::new("zzop-ws-pnpm-flow");
    dir.write("pnpm-workspace.yaml", "packages: ['pkgs/*']\n");
    dir.write("pkgs/one/package.json", &pkg_json(Some("one")));

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["root"].as_str().unwrap(), "pkgs/one");
    assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "one");
}

#[test]
fn pnpm_workspace_yaml_takes_precedence_over_package_json() {
    let dir = TempDir::new("zzop-ws-precedence");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'from-pnpm/*'\n");
    dir.write("package.json", r#"{"workspaces": ["from-npm/*"]}"#);
    dir.write("from-pnpm/a/package.json", &pkg_json(Some("a")));
    dir.write("from-npm/b/package.json", &pkg_json(Some("b")));

    let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["root"].as_str().unwrap(), "from-pnpm/a");
    assert!(warnings
        .iter()
        .any(|w| w.contains("from pnpm-workspace.yaml")));
}

#[test]
fn package_json_array_form_is_read_when_no_pnpm_manifest() {
    let dir = TempDir::new("zzop-ws-npm-array");
    dir.write("package.json", r#"{"workspaces": ["packages/*"]}"#);
    dir.write("packages/a/package.json", &pkg_json(Some("a")));

    let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert!(warnings
        .iter()
        .any(|w| w.contains("package.json \"workspaces\"")));
}

#[test]
fn package_json_object_form_with_packages_field_is_read() {
    let dir = TempDir::new("zzop-ws-npm-object");
    dir.write(
        "package.json",
        r#"{"workspaces": {"packages": ["packages/*"]}}"#,
    );
    dir.write("packages/a/package.json", &pkg_json(Some("a")));

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["root"].as_str().unwrap(), "packages/a");
}

// --- negation, recursion, skip-dirs -------------------------------------------------------

#[test]
fn negative_patterns_exclude_matching_directories() {
    let dir = TempDir::new("zzop-ws-negation");
    dir.write(
        "pnpm-workspace.yaml",
        "packages:\n  - 'packages/*'\n  - '!packages/legacy'\n",
    );
    dir.write("packages/a/package.json", &pkg_json(Some("a")));
    dir.write("packages/legacy/package.json", &pkg_json(Some("legacy")));

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
    assert_eq!(roots, vec!["packages/a"]);
}

#[test]
fn double_star_recurses_and_skips_node_modules_and_git() {
    let dir = TempDir::new("zzop-ws-recursion");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/**'\n");
    dir.write("packages/package.json", &pkg_json(Some("root-pkg")));
    dir.write(
        "packages/deep/nested/dir/package.json",
        &pkg_json(Some("deep-pkg")),
    );
    dir.write(
        "packages/node_modules/should-be-skipped/package.json",
        &pkg_json(Some("skip-me")),
    );
    dir.write(
        "packages/.git/fake/package.json",
        &pkg_json(Some("skip-me-too")),
    );

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
    assert_eq!(roots, vec!["packages", "packages/deep/nested/dir"]);
}

#[test]
fn a_matched_directory_without_package_json_is_excluded() {
    let dir = TempDir::new("zzop-ws-requires-pkg-json");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/has-pkg/package.json", &pkg_json(Some("has-pkg")));
    dir.mkdir("packages/no-pkg");

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
    assert_eq!(roots, vec!["packages/has-pkg"]);
}

#[test]
fn results_are_sorted_regardless_of_directory_creation_order() {
    let dir = TempDir::new("zzop-ws-sort");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    // Create out of alphabetical order on purpose.
    dir.write("packages/zeta/package.json", &pkg_json(Some("zeta")));
    dir.write("packages/alpha/package.json", &pkg_json(Some("alpha")));
    dir.write("packages/mid/package.json", &pkg_json(Some("mid")));

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    let roots: Vec<&str> = trees.iter().map(|t| t["root"].as_str().unwrap()).collect();
    assert_eq!(
        roots,
        vec!["packages/alpha", "packages/mid", "packages/zeta"]
    );
}

// --- errors --------------------------------------------------------------------------------

#[test]
fn no_workspace_manifest_is_a_config_error_with_the_exact_text() {
    let dir = TempDir::new("zzop-ws-no-manifest");
    let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
    let expected = format!(
        "trees: \"auto\" found no workspace manifest in {} — expected a pnpm-workspace.yaml with a \"packages:\" list, or a package.json with a \"workspaces\" field. Write an explicit \"trees\": [{{ \"root\": ..., \"sourceId\": ... }}] array instead, or run zzop from the workspace root.",
        dir.path().display()
    );
    assert_eq!(err.0, expected);
}

#[test]
fn no_matching_package_directories_is_a_config_error_with_the_exact_text() {
    let dir = TempDir::new("zzop-ws-no-match");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'nonexistent/*'\n");
    let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
    assert_eq!(
        err.0,
        "trees: \"auto\" matched no package directories from pnpm-workspace.yaml (patterns: nonexistent/*). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
    );
}

#[test]
fn no_matching_package_directories_falls_back_to_none_placeholder_with_empty_pattern_list() {
    let dir = TempDir::new("zzop-ws-no-match-empty");
    // A `packages:` key with no list items at all yields an empty pattern list.
    dir.write("pnpm-workspace.yaml", "packages:\n");
    let err = expand_auto_trees(auto_config(), dir.path()).unwrap_err();
    assert_eq!(
        err.0,
        "trees: \"auto\" matched no package directories from pnpm-workspace.yaml (patterns: (none)). Each pattern must resolve to directories containing a package.json. Write an explicit \"trees\" array instead."
    );
}

// --- sourceId derivation ---------------------------------------------------------------

#[test]
fn source_id_comes_from_package_json_name_when_present() {
    let dir = TempDir::new("zzop-ws-sourceid-name");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/a/package.json", &pkg_json(Some("my-pkg-name")));

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "my-pkg-name");
}

#[test]
fn source_id_falls_back_to_relative_dir_when_name_is_absent() {
    let dir = TempDir::new("zzop-ws-sourceid-fallback");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/nameless/package.json", "{}");

    let (config, _warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees[0]["sourceId"].as_str().unwrap(), "packages/nameless");
}

#[test]
fn duplicate_source_ids_produce_a_warning_not_an_error() {
    let dir = TempDir::new("zzop-ws-duplicate");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/a/package.json", &pkg_json(Some("dup")));
    dir.write("packages/b/package.json", &pkg_json(Some("dup")));

    let (config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let trees = config["trees"].as_array().unwrap();
    assert_eq!(trees.len(), 2);
    let expected_warning = "trees: \"auto\" derived a duplicate sourceId \"dup\" for both \"packages/a\" and \"packages/b\". Cross-source joins key on sourceId; give one package a distinct \"name\" or use an explicit \"trees\" array to disambiguate.";
    assert!(warnings.iter().any(|w| w == expected_warning));
}

#[test]
fn single_resolved_tree_gets_an_extra_warning() {
    let dir = TempDir::new("zzop-ws-single-tree");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/only/package.json", &pkg_json(Some("only")));

    let (_config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    assert!(warnings.iter().any(|w| w.contains("expanded to 1 tree(s)")));
    assert!(warnings.iter().any(|w| {
        w == "trees: \"auto\" resolved only one workspace package — the cross-layer join needs >= 2 trees with distinct sourceIds to fire, so this run behaves like a single-tree analysis."
    }));
}

#[test]
fn exact_expansion_warning_text_composition() {
    let dir = TempDir::new("zzop-ws-warning-text");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/a/package.json", &pkg_json(Some("name-a")));
    dir.write("packages/b/package.json", &pkg_json(Some("name-b")));

    let (_config, warnings) = expand_auto_trees(auto_config(), dir.path()).unwrap();
    let expected = "trees: \"auto\" expanded to 2 tree(s) from pnpm-workspace.yaml: name-a (packages/a), name-b (packages/b).";
    assert!(warnings.iter().any(|w| w == expected));
}

// --- pass-through for non-"auto" configs ------------------------------------------------

#[test]
fn explicit_trees_array_passes_through_untouched() {
    let config = json!({ "trees": [{ "root": ".", "sourceId": "x" }] });
    let (out, warnings) =
        expand_auto_trees(config.clone(), Path::new("/nonexistent/zzop-test-base")).unwrap();
    assert_eq!(out, config);
    assert!(warnings.is_empty());
}

#[test]
fn config_without_trees_key_passes_through_untouched() {
    let config = json!({ "roots": ["."] });
    let (out, warnings) =
        expand_auto_trees(config.clone(), Path::new("/nonexistent/zzop-test-base")).unwrap();
    assert_eq!(out, config);
    assert!(warnings.is_empty());
}

#[test]
fn non_object_config_passes_through_untouched() {
    let null_config = Value::Null;
    let (out, warnings) = expand_auto_trees(
        null_config.clone(),
        Path::new("/nonexistent/zzop-test-base"),
    )
    .unwrap();
    assert_eq!(out, null_config);
    assert!(warnings.is_empty());

    let arr_config = json!(["trees", "auto"]);
    let (out2, warnings2) =
        expand_auto_trees(arr_config.clone(), Path::new("/nonexistent/zzop-test-base")).unwrap();
    assert_eq!(out2, arr_config);
    assert!(warnings2.is_empty());
}
