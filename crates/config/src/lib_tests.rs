//! Tests for the crate-root loading entry points.

use super::*;
use crate::test_support::TempDir;

#[test]
fn load_for_root_absent_config_produces_the_zero_config_request() {
    let dir = TempDir::new("zzop-config-lib-absent");
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(loaded.config_path.is_none());
    assert_eq!(loaded.method, Method::Analyze);
    let req = loaded.request.as_object().unwrap();
    assert_eq!(req["root"], dir.path().to_string_lossy().into_owned());
    assert_eq!(req["git"], serde_json::json!({}));
    assert_eq!(req["packDefs"].as_array().unwrap().len(), 12);
}

#[test]
fn load_for_root_present_config_is_discovered_and_mapped() {
    let dir = TempDir::new("zzop-config-lib-present");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "roots": ["."], "rules": { "toctou": "off" } }"#,
    );
    let loaded = load_for_root(dir.path()).unwrap();
    assert_eq!(
        loaded.config_path,
        Some(dir.path().join(DEFAULT_CONFIG_FILENAME))
    );
    let req = loaded.request.as_object().unwrap();
    assert!(req["disabledRules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "toctou"));
}

// --- single-tree-over-a-workspace disclosure ---------------------------------------------
//
// The measured silent hole (22-package pnpm monorepo, no zzop.config.jsonc): `config = null,
// configWarnings = []` — the run degraded to one tree, so the cross-layer join never ran, and not
// one word said so. The trigger is the RESOLVED TREE COUNT, not config absence, so a config that
// exists but never declares `trees` (a real second way into the same trap: no `trees` means no
// expansion, hence no expansion report to speak in its place) is disclosed too, while an author who
// explicitly declared `trees` is never second-guessed. These pin both variants, every silence
// condition, and that the run itself is unchanged (see the determinism test at the end).

/// A root with `pnpm-workspace.yaml` matching two package dirs, and no config file.
fn pnpm_monorepo_without_config(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/fe/package.json", r#"{"name": "fe"}"#);
    dir.write("packages/be/package.json", r#"{"name": "be"}"#);
    dir
}

fn workspace_warnings(loaded: &LoadedRequest) -> Vec<&String> {
    loaded
        .warnings
        .iter()
        .filter(|w| w.contains("workspace packages"))
        .collect()
}

#[test]
fn zero_config_over_a_pnpm_workspace_root_discloses_the_join_it_could_not_run() {
    let dir = pnpm_monorepo_without_config("zzop-config-ws-pnpm");
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(loaded.config_path.is_none());
    // The exact text, byte for byte — this string is the product's disclosure contract.
    assert_eq!(
        loaded.warnings.first().map(String::as_str),
        Some(
            format!(
                "no zzop.config.jsonc at {} — pnpm-workspace.yaml is present there and resolves to \
                 2 workspace packages, but this run analyzed the root as a SINGLE tree: the \
                 cross-layer join needs >= 2 trees with distinct sourceIds to fire, so it did not \
                 run. Create a zzop.config.jsonc at that root containing {{\"trees\": \"auto\"}} to \
                 analyze those 2 packages as separate trees.",
                dir.path().display()
            )
            .as_str()
        ),
        "got: {:?}",
        loaded.warnings
    );
    // The remedy must be copy-pasteable verbatim, and the manifest named must be the real one.
    let warning = &loaded.warnings[0];
    assert!(warning.contains("{\"trees\": \"auto\"}"));
    assert!(warning.contains("pnpm-workspace.yaml"));
    assert!(!warning.contains("package.json"));
}

#[test]
fn zero_config_over_an_npm_workspaces_root_names_that_manifest_instead() {
    let dir = TempDir::new("zzop-config-ws-npm");
    dir.write("package.json", r#"{"workspaces": ["apps/*"]}"#);
    dir.write("apps/one/package.json", r#"{"name": "one"}"#);
    dir.write("apps/two/package.json", r#"{"name": "two"}"#);
    let loaded = load_for_root(dir.path()).unwrap();
    let warnings = workspace_warnings(&loaded);
    assert_eq!(warnings.len(), 1, "got: {:?}", loaded.warnings);
    assert!(
        warnings[0].contains(
            "package.json \"workspaces\" is present there and resolves to 2 \
             workspace packages"
        ),
        "got: {}",
        warnings[0]
    );
}

#[test]
fn a_config_declaring_trees_auto_suppresses_the_disclosure() {
    // The author answered "which trees?" with `"auto"`, and the expansion prints its own positive
    // report — a second nag here would be duplicate, not honesty.
    let dir = pnpm_monorepo_without_config("zzop-config-ws-configured");
    dir.write(DEFAULT_CONFIG_FILENAME, r#"{ "trees": "auto" }"#);
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(loaded.config_path.is_some());
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
    // ...and the expansion's own positive disclosure is what speaks instead.
    assert!(loaded
        .warnings
        .iter()
        .any(|w| w.contains("expanded to 2 tree(s) from pnpm-workspace.yaml")));
}

#[test]
fn an_explicit_single_entry_trees_array_is_a_stated_choice_and_stays_silent() {
    // One tree results, and the manifest names two packages — but the author NAMED that tree, so
    // this is a decision, not an oversight. `Method::AnalyzeTrees` keeps it out of the disclosure.
    let dir = pnpm_monorepo_without_config("zzop-config-ws-explicit-one");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "trees": [{ "root": "packages/fe", "sourceId": "fe" }] }"#,
    );
    let loaded = load_for_root(dir.path()).unwrap();
    assert_eq!(loaded.method, Method::AnalyzeTrees);
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn a_config_that_never_declares_trees_gets_the_same_disclosure_worded_for_its_own_remedy() {
    // Backlog item S: `{"rules": {...}}` at a monorepo root is the identical trap — one tree, no
    // join — but its author reasonably believes having a config means the analysis was configured.
    let dir = pnpm_monorepo_without_config("zzop-config-ws-trees-less");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "rules": { "toctou": "off" } }"#,
    );
    let config_path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    let loaded = load_for_root(dir.path()).unwrap();
    assert_eq!(loaded.method, Method::Analyze);
    assert_eq!(
        loaded.warnings.first().map(String::as_str),
        Some(
            format!(
                "the config at {} declares no \"trees\" — pnpm-workspace.yaml at {} resolves to 2 \
                 workspace packages, but this run analyzed a SINGLE tree: the cross-layer join \
                 needs >= 2 trees with distinct sourceIds to fire, so it did not run. Add \
                 \"trees\": \"auto\" to that config to analyze those 2 packages as separate trees.",
                config_path.display(),
                dir.path().display()
            )
            .as_str()
        ),
        "got: {:?}",
        loaded.warnings
    );
    // The config's own mapping still happened — the disclosure rides ALONGSIDE it, never instead.
    assert!(loaded.request["disabledRules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "toctou"));
}

#[test]
fn the_trees_less_config_disclosure_reaches_the_explicit_load_config_file_entry_too() {
    // `load_config_file` is its own public entry (check_endpoint's `configPath` mode reaches it
    // directly, not through `load_for_root`) — the two loaders must not drift.
    let dir = pnpm_monorepo_without_config("zzop-config-ws-explicit-entry");
    dir.write(DEFAULT_CONFIG_FILENAME, "{}");
    let loaded = load_config_file(dir.path()).unwrap();
    let warnings = workspace_warnings(&loaded);
    assert_eq!(warnings.len(), 1, "got: {:?}", loaded.warnings);
    assert!(warnings[0].starts_with("the config at "));
    assert!(warnings[0].contains("Add \"trees\": \"auto\" to that config"));
}

#[test]
fn a_trees_less_config_over_a_non_workspace_repo_stays_silent() {
    // Having a config is not itself a reason to nag: with no workspace manifest there is no
    // observed fact to report.
    let dir = TempDir::new("zzop-config-ws-trees-less-plain");
    dir.write("package.json", r#"{"name": "just-an-app"}"#);
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "rules": { "toctou": "off" } }"#,
    );
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn a_multi_root_config_stays_silent_because_the_join_actually_ran() {
    // Two roots => two trees => `Method::AnalyzeTrees` => the join fired. Claiming it "did not run"
    // here would be the one thing worse than silence.
    let dir = pnpm_monorepo_without_config("zzop-config-ws-multi-root");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "roots": ["packages/fe", "packages/be"] }"#,
    );
    let loaded = load_for_root(dir.path()).unwrap();
    assert_eq!(loaded.method, Method::AnalyzeTrees);
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn an_ordinary_single_package_repo_is_never_nagged() {
    // No manifest at all: nothing was observed, so nothing is claimed.
    let dir = TempDir::new("zzop-config-ws-plain");
    dir.write("package.json", r#"{"name": "just-an-app"}"#);
    dir.write("src/index.ts", "export const x = 1;\n");
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn a_manifest_resolving_to_one_package_stays_silent_because_auto_would_not_help() {
    // `{"trees": "auto"}` over a one-package workspace still cannot reach 2 trees, so promising the
    // join here would be false advice.
    let dir = TempDir::new("zzop-config-ws-one-pkg");
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/only/package.json", r#"{"name": "only"}"#);
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn an_empty_manifest_package_list_stays_silent() {
    // A `packages:` key with no entries resolves to nothing; `trees: "auto"` would error, so there
    // is no honest remedy to offer.
    let dir = TempDir::new("zzop-config-ws-empty-list");
    dir.write("pnpm-workspace.yaml", "packages:\n");
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

#[test]
fn the_disclosure_is_deterministic_and_does_not_change_what_gets_analyzed() {
    let dir = pnpm_monorepo_without_config("zzop-config-ws-determinism");
    let first = load_for_root(dir.path()).unwrap();
    let second = load_for_root(dir.path()).unwrap();
    assert_eq!(first.warnings, second.warnings);
    // Same input, byte-identical request: the warning is DISCLOSURE only — same method, same single
    // root, same injected packs as the manifest-free zero-config run.
    assert_eq!(first.request, second.request);
    assert_eq!(first.method, Method::Analyze);
    assert_eq!(
        first.request["root"],
        dir.path().to_string_lossy().into_owned()
    );
    assert!(first.request.get("trees").is_none());
    assert_eq!(first.request["packDefs"].as_array().unwrap().len(), 12);
}

#[test]
fn the_trees_less_config_disclosure_is_deterministic_and_changes_nothing_about_the_run() {
    let dir = pnpm_monorepo_without_config("zzop-config-ws-determinism-cfg");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "rules": { "toctou": "off" } }"#,
    );
    let first = load_for_root(dir.path()).unwrap();
    let second = load_for_root(dir.path()).unwrap();
    assert_eq!(first.warnings, second.warnings);
    assert_eq!(first.request, second.request);
    // The request is EXACTLY the one this config produced before the disclosure existed: the same
    // single analyzed root, no `trees` invented, the rule override still applied, packs intact.
    let baseline = TempDir::new("zzop-config-ws-determinism-cfg-baseline");
    baseline.write(
        DEFAULT_CONFIG_FILENAME,
        r#"{ "rules": { "toctou": "off" } }"#,
    );
    let no_manifest = load_for_root(baseline.path()).unwrap();
    assert!(workspace_warnings(&no_manifest).is_empty());
    assert_eq!(first.method, no_manifest.method);
    assert_eq!(
        strip_root(&first.request, dir.path()),
        strip_root(&no_manifest.request, baseline.path()),
        "the manifest's presence must change the warnings and NOTHING else"
    );
}

/// A request with its root-derived absolute paths blanked, so two runs rooted at different temp dirs
/// can be compared field-for-field. Both are ASSERTED to be exactly what the root implies before being
/// blanked — blanking a field is how a comparison goes blind, so each one pays for itself first.
fn strip_root(request: &serde_json::Value, root: &std::path::Path) -> serde_json::Value {
    let mut cloned = request.clone();
    assert_eq!(cloned["root"], root.to_string_lossy().into_owned());
    cloned["root"] = serde_json::Value::Null;
    // Segment-by-segment join, not `join(DEFAULT_CACHE_DIR)`: the mapper resolves the constant through
    // `Path::components`, so the emitted value carries NATIVE separators (`.zzop\cache` on Windows)
    // while a single `join` of the slashed literal would not.
    let expected_cache = zzop_cache::DEFAULT_CACHE_DIR
        .split('/')
        .fold(root.to_path_buf(), |p, seg| p.join(seg));
    assert_eq!(
        cloned["cacheDir"],
        expected_cache.to_string_lossy().into_owned(),
        "the default cacheDir must be the analyzed root's own {}",
        zzop_cache::DEFAULT_CACHE_DIR
    );
    cloned["cacheDir"] = serde_json::Value::Null;
    cloned
}

#[test]
fn load_config_file_accepts_a_direct_file_path() {
    let dir = TempDir::new("zzop-config-lib-direct-file");
    let config_path = dir.path().join("custom.jsonc");
    std::fs::write(&config_path, r#"{ "roots": ["."] }"#).unwrap();
    let loaded = load_config_file(&config_path).unwrap();
    assert_eq!(loaded.config_path, Some(config_path));
}

#[test]
fn load_config_file_accepts_a_directory_and_finds_the_default_filename() {
    let dir = TempDir::new("zzop-config-lib-dir");
    dir.write(DEFAULT_CONFIG_FILENAME, r#"{ "roots": ["."] }"#);
    let loaded = load_config_file(dir.path()).unwrap();
    assert_eq!(
        loaded.config_path,
        Some(dir.path().join(DEFAULT_CONFIG_FILENAME))
    );
}

#[test]
fn load_config_file_missing_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-missing");
    let missing = dir.path().join("nope.jsonc");
    let err = load_config_file(&missing).unwrap_err();
    assert!(err
        .0
        .starts_with(&format!("No config file at {}.", missing.display())));
    assert!(err
        .0
        .contains("Create a zzop.config.jsonc there, or pass a directory that has one."));
    // The JS-CLI-only hint (`zzop init`, `--config`) must NOT survive the port.
    assert!(!err.0.contains("zzop init"));
    assert!(!err.0.contains("--config"));
}

#[test]
fn load_config_file_missing_in_a_directory_also_reports_the_expected_default_filename() {
    let dir = TempDir::new("zzop-config-lib-missing-dir");
    let err = load_config_file(dir.path()).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "No config file at {}.",
        dir.path().join(DEFAULT_CONFIG_FILENAME).display()
    )));
}

#[test]
fn load_config_file_invalid_jsonc_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-bad-jsonc");
    dir.write(DEFAULT_CONFIG_FILENAME, "{ not valid json");
    let err = load_config_file(dir.path()).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "Invalid JSONC in {}: ",
        dir.path().join(DEFAULT_CONFIG_FILENAME).display()
    )));
}

#[test]
fn load_config_file_non_object_top_level_reports_the_adapted_error_text() {
    let dir = TempDir::new("zzop-config-lib-non-object");
    dir.write(DEFAULT_CONFIG_FILENAME, "[1, 2, 3]");
    let err = load_config_file(dir.path()).unwrap_err();
    assert_eq!(
        err.0,
        format!(
            "Config in {} must be a JSON object.",
            dir.path().join(DEFAULT_CONFIG_FILENAME).display()
        )
    );
}

#[test]
fn load_config_file_unreadable_bytes_report_a_could_not_read_error() {
    // Invalid UTF-8 makes `read_to_string` fail with an I/O error, exercising the "file exists
    // but could not be read" branch (distinct from "missing") portably, without relying on
    // platform-specific file permission behavior.
    let dir = TempDir::new("zzop-config-lib-bad-utf8");
    let config_path = dir.path().join(DEFAULT_CONFIG_FILENAME);
    std::fs::write(&config_path, [0xFFu8, 0xFE, 0x00, 0x01]).unwrap();
    let err = load_config_file(&config_path).unwrap_err();
    assert!(err.0.starts_with(&format!(
        "Could not read config at {}: ",
        config_path.display()
    )));
}

#[test]
fn load_config_file_comments_and_trailing_commas_are_stripped_before_parsing() {
    let dir = TempDir::new("zzop-config-lib-jsonc-quirks");
    dir.write(
        DEFAULT_CONFIG_FILENAME,
        "{\n  // a comment\n  \"roots\": [\".\",],\n}",
    );
    let loaded = load_config_file(dir.path()).unwrap();
    let req = loaded.request.as_object().unwrap();
    assert_eq!(req["root"], dir.path().to_string_lossy().into_owned());
}
