//! Tests for the crate-root loading entry points.

use super::*;
use crate::test_support::TempDir;

/// The reversal of the founding zero-config default (2026-07-27): a directory with no config is
/// refused outright rather than analyzed on assumed conventions. Pins the two properties the message
/// has to have beyond "it failed" — it names the missing file's full path, and it names an ARTIFACT
/// rather than either host's command, since the identical message reaches a terminal and an MCP client.
#[test]
fn load_for_root_absent_config_is_refused_and_names_the_template_document() {
    let dir = TempDir::new("zzop-config-lib-absent");
    let err = load_for_root(dir.path()).unwrap_err();
    assert!(
        err.0.contains(
            &dir.path()
                .join(DEFAULT_CONFIG_FILENAME)
                .display()
                .to_string()
        ),
        "the refusal must name the file it looked for: {}",
        err.0
    );
    assert!(
        err.0.contains("`config-template` contract document"),
        "the refusal must point at the artifact both hosts can serve: {}",
        err.0
    );
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
// The measured silent hole (22-package pnpm monorepo): `config = null, configWarnings = []` — the run
// degraded to one tree, so the cross-layer join never ran, and not one word said so. The trigger is the
// RESOLVED TREE COUNT: a config that exists but never declares `trees` falls into the trap (no `trees`
// means no expansion, hence no expansion report to speak in its place), while an author who explicitly
// declared `trees` is never second-guessed. These pin every silence condition and that the run itself is
// unchanged.
//
// There used to be a second, config-less variant of each of these. It went away with the config-less run
// (2026-07-27): a directory with no config is now refused, so "analyzed quietly as one tree" is no longer
// a state that can be reached without a config file.

/// A root with `pnpm-workspace.yaml` matching two package dirs. Callers add the config themselves —
/// which config (trees-less, `"auto"`, explicit array) is exactly what each test is about.
fn pnpm_monorepo(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write("pnpm-workspace.yaml", "packages:\n  - 'packages/*'\n");
    dir.write("packages/fe/package.json", r#"{"name": "fe"}"#);
    dir.write("packages/be/package.json", r#"{"name": "be"}"#);
    dir
}

/// The minimal config that reaches the disclosure: it exists (so the run is allowed) and declares no
/// `trees` (so exactly one tree results).
const TREES_LESS_CONFIG: &str = r#"{ "rules": { "toctou": "off" } }"#;

fn workspace_warnings(loaded: &LoadedRequest) -> Vec<&String> {
    loaded
        .warnings
        .iter()
        .filter(|w| w.contains("workspace packages"))
        .collect()
}

/// The manifest LABEL is read off the file that was really opened, so a repo using npm `workspaces`
/// is never told to look at a `pnpm-workspace.yaml` it does not have. (The pnpm spelling is pinned
/// byte-for-byte by `a_config_that_never_declares_trees_gets_the_same_disclosure_worded_for_its_own_remedy`;
/// this test exists for the other manifest.)
#[test]
fn a_trees_less_config_over_an_npm_workspaces_root_names_that_manifest_instead() {
    let dir = TempDir::new("zzop-config-ws-npm");
    dir.write("package.json", r#"{"workspaces": ["apps/*"]}"#);
    dir.write("apps/one/package.json", r#"{"name": "one"}"#);
    dir.write("apps/two/package.json", r#"{"name": "two"}"#);
    dir.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);
    let loaded = load_for_root(dir.path()).unwrap();
    let warnings = workspace_warnings(&loaded);
    assert_eq!(warnings.len(), 1, "got: {:?}", loaded.warnings);
    assert!(
        warnings[0].contains("package.json \"workspaces\" at ")
            && warnings[0].contains("resolves to 2 workspace packages"),
        "got: {}",
        warnings[0]
    );
}

/// The envelope lane takes ONLY the convention vocabulary and walks no tree, so the tree-walk advice
/// must not ride along — every clause of it ("this run analyzed a SINGLE tree", `add "trees": "auto"`,
/// "at the cost of the dependency graph, which is built per tree") is false there.
///
/// Observed 2026-08-08 in a real `zzop analyze-envelope` reply: the disclosure landed DIRECTLY AFTER
/// the envelope lane's own line saying the vocabulary block "is the ONLY thing an envelope run takes
/// from an adjacent config: every other key there configures a tree analysis … none of which an
/// envelope run does". Two adjacent sentences, the second telling the reader to set a key the first
/// had just called ignored.
///
/// The paired asserts are the point: the SAME fixture must keep producing it on `load_for_root`. A
/// one-sided test would pass just as well if the disclosure had been deleted outright.
#[test]
fn the_vocabulary_only_entry_omits_tree_walk_advice_while_the_tree_entry_keeps_it() {
    let dir = TempDir::new("zzop-config-ws-envelope");
    dir.write("package.json", r#"{"workspaces": ["apps/*"]}"#);
    dir.write("apps/one/package.json", r#"{"name": "one"}"#);
    dir.write("apps/two/package.json", r#"{"name": "two"}"#);
    dir.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);

    let tree_lane = load_for_root(dir.path()).unwrap();
    assert_eq!(
        workspace_warnings(&tree_lane).len(),
        1,
        "the tree lane must still disclose: {:?}",
        tree_lane.warnings
    );

    let envelope_lane = crate::load_for_root_vocabulary_only(dir.path()).unwrap();
    assert!(
        workspace_warnings(&envelope_lane).is_empty(),
        "a lane that walks no tree must not be told to add `trees`: {:?}",
        envelope_lane.warnings
    );
    // Everything else still comes through — the split is by what a warning ASSERTS ABOUT THE RUN, not
    // a blanket mute. Same request either way, so no caller loses anything but the false advice.
    assert_eq!(
        envelope_lane.request, tree_lane.request,
        "the mapped request must be identical; only the disclosure differs"
    );
}

#[test]
fn a_config_declaring_trees_auto_suppresses_the_disclosure() {
    // The author answered "which trees?" with `"auto"`, and the expansion prints its own positive
    // report — a second nag here would be duplicate, not honesty.
    let dir = pnpm_monorepo("zzop-config-ws-configured");
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
    let dir = pnpm_monorepo("zzop-config-ws-explicit-one");
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
    let dir = pnpm_monorepo("zzop-config-ws-trees-less");
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
                 \"trees\": \"auto\" to that config to analyze those 2 packages as separate trees \
                 — at the cost of the dependency graph, which is built per tree: an import \
                 crossing a package boundary then stops being a dep edge and is censused as an \
                 external package instead.",
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
    let dir = pnpm_monorepo("zzop-config-ws-explicit-entry");
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
    let dir = pnpm_monorepo("zzop-config-ws-multi-root");
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
    dir.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);
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
    dir.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);
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
    dir.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);
    let loaded = load_for_root(dir.path()).unwrap();
    assert!(
        workspace_warnings(&loaded).is_empty(),
        "got: {:?}",
        loaded.warnings
    );
}

// (The config-less twin of the determinism pin below is gone with the config-less run itself; the
// trees-less-config form is now the only way to reach this disclosure, so one test covers it.)

#[test]
fn the_trees_less_config_disclosure_is_deterministic_and_changes_nothing_about_the_run() {
    let dir = pnpm_monorepo("zzop-config-ws-determinism-cfg");
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

/// The mirror pin: each of the two tree-shape disclosures names the COST of the remedy it recommends —
/// the single-tree one that `trees: "auto"` splits the dependency graph (a cross-package import stops
/// being a dep edge), the cross-tree-import one that a single tree turns the cross-layer join off. This
/// is the mechanical stand-in for "a remedy must name its cost", which no lint can express; it asserts
/// only that each message names its own remedy AND its own cost, nothing about their wording beyond
/// those substrings. `zzop-config` is the only crate that sees both sides (it depends on `zzop-engine`;
/// there is no dependency the other way).
#[test]
fn both_tree_shape_disclosures_name_the_cost_of_the_remedy_they_recommend() {
    // Direction 1: one tree over a multi-package workspace -> "add trees: auto", and what that costs.
    let mono = pnpm_monorepo("zzop-config-ws-mirror");
    mono.write(DEFAULT_CONFIG_FILENAME, TREES_LESS_CONFIG);
    let loaded = load_for_root(mono.path()).unwrap();
    let single_tree = workspace_warnings(&loaded);
    assert_eq!(single_tree.len(), 1, "got: {:?}", loaded.warnings);
    let single_tree = single_tree[0];
    assert!(
        single_tree.contains("Add \"trees\": \"auto\" to that config"),
        "{single_tree}"
    );
    assert!(
        single_tree.contains("stops being a dep edge"),
        "the trees:\"auto\" remedy must name its cost: {single_tree}"
    );

    // Direction 2: separate trees whose imports cross a tree boundary -> "analyze as one tree", and
    // what THAT costs. Two real trees, run through the engine this crate already depends on.
    let fe = TempDir::new("zzop-config-ws-mirror-fe");
    fe.write("package.json", r#"{"name": "@apps/fe"}"#);
    fe.write(
        "src/app.ts",
        "import { fmt } from \"@base/utils\";\nexport const render = () => fmt();\n",
    );
    let utils = TempDir::new("zzop-config-ws-mirror-utils");
    utils.write("package.json", r#"{"name": "@base/utils"}"#);
    utils.write("src/index.ts", "export const fmt = () => \"x\";\n");
    let engine_config = |source_id: &str| zzop_engine::EngineConfig {
        source_id: source_id.to_string(),
        ..zzop_engine::EngineConfig::default()
    };
    let out = zzop_engine::analyze_trees(&[
        (fe.path().to_path_buf(), engine_config("@apps/fe")),
        (utils.path().to_path_buf(), engine_config("@base/utils")),
    ]);
    let fe_warnings = &out
        .trees
        .iter()
        .find(|(_, source, _)| source == "@apps/fe")
        .expect("fe tree present")
        .2
        .warnings;
    // PREFIX, not `contains`: the sibling zero-edge warning now POINTS AT this one by name (it used to
    // offer a closed two-way choice that excluded the tree-boundary cause), so a substring match counts
    // the pointer as a second occurrence. Identity of a message is where it starts.
    let cross_tree: Vec<&String> = fe_warnings
        .iter()
        .filter(|w| w.starts_with("cross-tree package imports:"))
        .collect();
    assert_eq!(cross_tree.len(), 1, "got: {fe_warnings:?}");
    let cross_tree = cross_tree[0];
    assert!(cross_tree.contains("\"roots\": [\".\"]"), "{cross_tree}");
    assert!(
        cross_tree.contains("at the cost of the cross-layer join"),
        "the one-tree remedy must name its cost: {cross_tree}"
    );
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
    // The remedy names the ARTIFACT both hosts can serve — the `config-template` contract document —
    // and, since 2026-07-27, also says WHY a config is required at all rather than just that one is
    // missing: the convention vocabulary has no built-in default behind it any more.
    assert!(err.0.contains("`config-template` contract document"));
    assert!(err.0.contains("has no built-in default"));
    // Both assertions now stand on ONE live reason, and it is no longer the "JS-CLI-only ghost, must
    // not survive the port" rationale they were written under: WIRE NEUTRALITY. This error is raised by
    // the SHARED library, and each host reaches this same line by a different spelling — so naming
    // either one here would be wrong for the other, and the remedy stays spelling-free ("Create a
    // zzop.config.jsonc there, or pass a directory that has one", pinned above) while each product's own
    // usage text owns its own words. The ghost reading died twice, on the same day (2026-07-26):
    //  - `--config` is a real flag on five `zzop` subcommands (`cross`/`endpoint`/`manifest`/`facts`/
    //    `graph`), while the `zzop-mcp` caller passes `configPath`, not a flag.
    //  - `zzop init` is a real subcommand too, as of the CLI-restoration batch — it writes the starter
    //    config document. That is exactly why the pin must NOT be deleted as "a ghost that came back":
    //    the MCP caller reaching this line cannot run a subcommand at all (it reads the same document as
    //    the `config-template` resource), so a library sentence telling it to run `zzop init` would be
    //    advice it cannot take. A real spelling in the wrong mouth is the same defect as a dead one.
    assert!(!err.0.contains("zzop init"));
    assert!(!err.0.contains("--config"));
    // WIDENED 2026-07-27. This assertion pair guarded ONE message while five siblings drifted the other
    // way and shipped — a multi-tree config telling a CLI user to "use the cross_repo tool with
    // configPath", a single-tree config saying "use analyze_repo for it", EVERY paths-mode run's
    // configWarnings saying "pass configPath to honor it", a capped edge list naming check_endpoint, and
    // a blank envelope FILE reporting "envelopeJson is empty". A pin on one point does not defend a
    // class, so the class now has its own machine contract:
    // `crates/engine/tests/rule_contracts/host_vocabulary.rs` scans every user-facing string literal in
    // `crates/summary/src` + `crates/config/src` for MCP-only vocabulary. This test stays because it is
    // the tighter, message-specific half (it also covers the CLI direction, which the class contract
    // does not).
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

/// `config-surface.json`'s `_docs.configPaths` claims the list is derived from `configKeys`. This is
/// what makes that claim true instead of aspirational: it REGENERATES the list here and compares.
///
/// The gap it closes is a false NEGATIVE, which no consumer could have noticed. `configPaths` is a
/// whitelist — a message may name a dotted config path only if it appears here — so a nested scope
/// present in `configKeys` but missing from `configPaths` does not make anything fail loudly; it just
/// makes a real, valid config path unsayable. `configKeys.mount`/`route` sat in exactly that state
/// (audited 2026-07-26) while the derivation claim was already written down.
///
/// The per-scope prefix table below is the derivation rule, and it is EXHAUSTIVE by construction: a
/// scope added to `configKeys` with no entry here fails this test with the question it needs answered
/// ("what does this scope look like when an author writes it in a sentence?"), rather than silently
/// contributing nothing.
#[test]
fn config_paths_are_derived_from_config_keys() {
    use std::collections::BTreeSet;

    let surface: serde_json::Value = serde_json::from_str(CONFIG_SURFACE_JSON).unwrap();
    let config_keys = surface["configKeys"].as_object().unwrap();

    // Scope -> the prefix its child keys carry when written out. `None` = this scope contributes no
    // dotted spelling at all, with the reason stated (see `_docs.configPaths`, which says the same two).
    let dotted_prefix = |scope: &str| -> Option<Option<&'static str>> {
        Some(match scope {
            // A top-level key IS its own spelling; it is checked against `configKeys.top` directly.
            "top" => None,
            // `rules.<id>.severity` — `<id>` is an open-keyed rule id, not enumerable.
            "ruleObject" => None,
            "packs" => Some("packs."),
            "git" => Some("git."),
            "vocabulary" => Some("vocabulary."),
            // The first NESTED scope under `vocabulary` (2026-07-27). Its four keys answer one question
            // ("what is your FSD layout?") and are meaningless apart, which is what the nesting says;
            // replacement granularity stays the LEAF, exactly as `packs.` and `git.` already work.
            "featureSlicedDesign" => Some("vocabulary.featureSlicedDesign."),
            "parsers" => Some("parsers."),
            // `globOverride` looks like dead vocabulary from the unknown-key walk's side and is not:
            // that walk deliberately stops at `parsers.` (a misspelled entry key fails the LOAD, see
            // `mapper/warnings.rs`), so THIS derivation is the scope's only consumer. Deleting the
            // scope therefore deletes `parsers.globOverrides[].glob`/`.language` from `configPaths` —
            // real spellings an author writes — which is exactly the `mount`/`route` false negative
            // this test was written to close. Weighed and rejected 2026-08-14; keep both.
            "globOverride" => Some("parsers.globOverrides[]."),
            "report" => Some("report."),
            "tree" => Some("trees[]."),
            "mount" => Some("trees[].topology.mounts[]."),
            "topology" => Some("trees[].topology."),
            "route" => Some("trees[].routes[]."),
            _ => return None,
        })
    };

    let mut expected: BTreeSet<String> = BTreeSet::new();
    for (scope, keys) in config_keys {
        let prefix = dotted_prefix(scope).unwrap_or_else(|| {
            panic!(
                "config-surface.json's configKeys gained the scope \"{scope}\", which this derivation \
                 does not know how to spell as a dotted path. Add it to the table in this test (with \
                 its `<parent>.`/`<parent>[].` prefix), or record it there as deliberately un-dotted \
                 with the reason — do not leave it silently contributing nothing."
            )
        });
        let Some(prefix) = prefix else { continue };
        for key in keys.as_array().unwrap() {
            expected.insert(format!("{prefix}{}", key.as_str().unwrap()));
        }
    }

    let actual: BTreeSet<String> = surface["configPaths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    assert_eq!(
        actual, expected,
        "config-surface.json's configPaths must be exactly the dotted spellings its configKeys imply"
    );
}

/// The on-disk two-way split is only real if git treats the two directories differently, and the two
/// spellings are ONE character apart (`.zzop/` derived and ignored, `zzop/` authored and committed).
/// A `zzop*` glob in an ignore file — the obvious-looking way to write the first rule — silently
/// swallows the second, taking every custom rule pack out of version control with no error anywhere.
/// That failure is invisible to reading: `**/.zzop/` and `zzop*` look equally reasonable in a diff.
/// So this asks git itself, against THIS repo's real `.gitignore`, replayed into a scratch repo so the
/// answer depends on that file alone (not on the developer's global excludes or the current index).
#[test]
fn the_repo_ignore_rules_hide_the_derived_zzop_dir_and_keep_the_authored_one_tracked() {
    use std::process::Command;

    let git_ok = Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !git_ok {
        return; // no git on PATH — the same tolerance `crates/facade`'s git-backed tests use.
    }

    let repo_gitignore = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.gitignore")
        .canonicalize()
        .expect("the repo's own .gitignore must exist");
    let rules = std::fs::read_to_string(&repo_gitignore).expect("readable .gitignore");

    let dir = TempDir::new("zzop-config-ignore-split");
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("git must spawn")
    };
    assert!(run(&["init"]).status.success());
    dir.write(".gitignore", &rules);
    dir.write(
        &format!("{}/my-pack.json", crate::DEFAULT_AUTHORED_PACKS_DIR),
        "{}",
    );
    dir.write("packages/api/zzop/rules/my-pack.json", "{}");
    dir.write(".zzop/cache/ir/entry.bin", "x");
    dir.write("packages/api/.zzop/cache/ir/entry.bin", "x");

    // `git check-ignore` exits 0 when a path IS ignored and 1 when it is not — the answer itself, and
    // the reason this asks git rather than re-implementing gitignore semantics and testing the copy.
    // Deliberately flagless: nothing is ever staged in this scratch repo, so there is no index for the
    // check to disagree with, and the reference-validation contract reads every `--flag`-shaped token
    // in this crate's source against the real CLI surface — a git flag spelled here would trip it.
    let ignored = |rel: &str| run(&["check-ignore", rel]).status.success();

    assert!(
        !ignored("zzop/rules/my-pack.json"),
        "an authored zzop/ pack must stay tracked — a zzop* glob would have swallowed it"
    );
    assert!(
        !ignored("packages/api/zzop/rules/my-pack.json"),
        "the authored directory stays tracked in a sub-tree too, not only at the repo root"
    );
    assert!(
        ignored(".zzop/cache/ir/entry.bin"),
        "the derived .zzop/ must be ignored at the root"
    );
    assert!(
        ignored("packages/api/.zzop/cache/ir/entry.bin"),
        "the derived .zzop/ must be ignored in every sub-tree — a per-path run creates one per base"
    );
}
