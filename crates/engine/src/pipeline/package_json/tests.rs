//! `package_json_entries` unit tests — moved out of the parent file, unchanged in content, when the
//! deployment-role split pushed that file past the 300-line source cap (`tests.rs` is exempt by name).

use super::*;
use crate::pipeline::testutil::TempDir;
use std::collections::HashSet;

#[test]
fn package_json_entries_resolves_extensionless_or_js_main_via_try_ext() {
    let dir = TempDir::new("zzop-pkg-entries-main");
    dir.write("package.json", r#"{"main": "dist/index.js"}"#);
    let all_paths: HashSet<String> = ["dist/index.ts".to_string()].into_iter().collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert_eq!(scan.entry_paths, all_paths);
    assert!(scan.script_paths.is_empty());
}

#[test]
fn package_json_entries_resolves_bin_object_with_multiple_entries() {
    let dir = TempDir::new("zzop-pkg-entries-bin");
    dir.write(
        "package.json",
        r#"{"bin": {"foo-cli": "./bin/foo.ts", "bar-cli": "./bin/bar.ts"}}"#,
    );
    let all_paths: HashSet<String> = ["bin/foo.ts".to_string(), "bin/bar.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert_eq!(scan.entry_paths, all_paths);
    assert!(scan.script_paths.is_empty());
}

#[test]
fn package_json_entries_resolves_nested_exports_and_ignores_condition_keys() {
    let dir = TempDir::new("zzop-pkg-entries-exports");
    dir.write(
        "package.json",
        r#"{
                "exports": {
                    ".": { "import": "./src/index.mts", "require": "./src/index.cts" },
                    "./sub": "./src/sub.ts"
                }
            }"#,
    );
    let all_paths: HashSet<String> = [
        "src/index.mts".to_string(),
        "src/index.cts".to_string(),
        "src/sub.ts".to_string(),
    ]
    .into_iter()
    .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert_eq!(scan.entry_paths, all_paths);
    assert!(scan.script_paths.is_empty());
}

#[test]
fn package_json_entries_lexically_scans_scripts_for_path_tokens() {
    let dir = TempDir::new("zzop-pkg-entries-scripts");
    dir.write(
        "package.json",
        r#"{
                "scripts": {
                    "build": "tsc && node scripts/postbuild.js",
                    "test": "jest"
                }
            }"#,
    );
    let all_paths: HashSet<String> = ["scripts/postbuild.ts".to_string()].into_iter().collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    // "test": "jest" has no path-looking token — contributes nothing; "tsc" isn't a path either.
    // The token lands in `script_paths`, NOT `entry_paths`: the manifest said how it is built, not
    // what it ships. `all_entry_paths` still unions the two, which is what `dead-candidates` reads.
    assert_eq!(scan.script_paths, all_paths);
    assert!(scan.entry_paths.is_empty());
    assert_eq!(scan.all_entry_paths(), all_paths);
}

/// The split's load-bearing invariant, in the one shape that could break it: a file BOTH shipped and
/// invoked from a BUILD script (`build`, deliberately — a run-lifecycle key would make this pass through
/// the rescue path instead and stop testing the subtraction). `dead-candidates` must see it exactly once
/// (the union is unchanged), and the deployment-role classification must call it SHIPPED — otherwise the
/// summary layer would demote a real product file on the strength of a `scripts` mention.
#[test]
fn a_path_that_is_both_an_entry_field_and_a_script_token_stays_shipped() {
    let dir = TempDir::new("zzop-pkg-entries-both-roles");
    dir.write("packages/cli/package.json", r#"{"bin": "./run.ts"}"#);
    dir.write(
        "package.json",
        r#"{"scripts": {"build": "node packages/cli/run.ts"}}"#,
    );
    let all_paths: HashSet<String> = ["packages/cli/run.ts".to_string()].into_iter().collect();
    let scan = package_json_entries(
        dir.path(),
        // Scripts manifest FIRST, so a per-manifest subtraction would leave the collision standing.
        [
            "package.json".to_string(),
            "packages/cli/package.json".to_string(),
        ]
        .into_iter(),
        &all_paths,
    );
    assert_eq!(scan.entry_paths, all_paths);
    assert!(
        scan.script_paths.is_empty(),
        "shipped dominates: {:?}",
        scan.script_paths
    );
    assert_eq!(scan.all_entry_paths(), all_paths);
}

/// A manifest with NO `main`/`bin`/`exports` whose `start` names the source it runs. This is
/// `corpus/x/xai-cookbook/voice-examples/agent/telephony/xai/package.json`, reduced: an Express server
/// whose only declaration of itself is `npm start`. The first cut of the split called every `scripts`
/// token build surface and sorted three live servers last in that tree, below `unimported-export` noise.
///
/// `dev` names the SAME file, which is the shape that makes "stop demoting on `dev`" the wrong repair —
/// `dev` also names cal.com's genuinely-build `scripts/docker-start.ts`. The run key is what discriminates.
#[test]
fn a_start_script_target_is_a_run_entry_even_with_no_main_field() {
    let dir = TempDir::new("zzop-pkg-entries-start-no-main");
    dir.write(
        "package.json",
        // `dev` is spelled the way the sibling `web/xai/backend-nodejs` spells it (`tsx watch <file>`)
        // rather than the telephony package's `nodemon --exec '<file>'`: the quoted form puts a trailing
        // apostrophe on the token, which `looks_like_script_path_token` rejects, so that spelling would
        // contribute nothing and the fixture would not exercise the both-lists case at all.
        r#"{
                "scripts": {
                    "dev": "tsx watch src/index.ts",
                    "start": "ts-node src/index.ts",
                    "outbound": "ts-node src/outbound.ts"
                }
            }"#,
    );
    let all_paths: HashSet<String> = ["src/index.ts".to_string(), "src/outbound.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert_eq!(
        scan.entry_paths,
        ["src/index.ts".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
        "the server `npm start` runs is the product, not build surface"
    );
    assert_eq!(
        scan.script_paths,
        ["src/outbound.ts".to_string()]
            .into_iter()
            .collect::<HashSet<_>>(),
        "`outbound` is not a run-lifecycle key, so its target stays build surface"
    );
    // Detection is untouched either way: the union `dead-candidates` reads is both files, as before.
    assert_eq!(scan.all_entry_paths(), all_paths);
}

/// The other side of the same discriminator, and the reason it is the script KEY rather than a
/// stop-demoting-on-`dev` rule: cal.com's `apps/api/v2` names a genuine build script from `dev` alone.
/// Measured: that file and `apps/web/scripts/create-sentry-release.js` produce 5 of cal.com's 7 criticals.
#[test]
fn a_dev_only_script_target_stays_build_surface() {
    let dir = TempDir::new("zzop-pkg-entries-dev-only");
    dir.write(
        "package.json",
        r#"{"scripts": {"dev": "ts-node scripts/docker-start.ts"}}"#,
    );
    let all_paths: HashSet<String> = ["scripts/docker-start.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert!(scan.entry_paths.is_empty(), "{:?}", scan.entry_paths);
    assert_eq!(scan.script_paths, all_paths);
}

/// EXACT key match, measured rather than assumed: grafana spells `start:swagger` and `start:rspack`, and
/// both name real webpack/rspack build configs. A `start`-PREFIX rule would rescue those, so the key set
/// is exact and colon-suffixed project conventions stay build surface.
#[test]
fn colon_suffixed_start_conventions_are_not_run_lifecycle_keys() {
    let dir = TempDir::new("zzop-pkg-entries-start-suffixed");
    dir.write(
        "package.json",
        r#"{"scripts": {"start:swagger": "ts-node scripts/webpack/webpack.swagger.ts"}}"#,
    );
    let all_paths: HashSet<String> = ["scripts/webpack/webpack.swagger.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert!(scan.entry_paths.is_empty(), "{:?}", scan.entry_paths);
    assert_eq!(scan.script_paths, all_paths);
}

/// The union `dead-candidates` consumes carries BOTH roles — the pin that catches a future consumer
/// reading only `entry_paths` and silently re-flagging every build script as dead code.
#[test]
fn all_entry_paths_unions_both_deployment_roles() {
    let dir = TempDir::new("zzop-pkg-entries-union");
    dir.write(
        "package.json",
        r#"{"main": "./src/index.ts", "scripts": {"build": "node scripts/build.ts"}}"#,
    );
    let all_paths: HashSet<String> = ["src/index.ts".to_string(), "scripts/build.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("package.json".to_string()),
        &all_paths,
    );
    assert_eq!(
        scan.entry_paths,
        ["src/index.ts".to_string()]
            .into_iter()
            .collect::<HashSet<_>>()
    );
    assert_eq!(
        scan.script_paths,
        ["scripts/build.ts".to_string()]
            .into_iter()
            .collect::<HashSet<_>>()
    );
    assert_eq!(scan.all_entry_paths(), all_paths);
}

#[test]
fn package_json_entries_resolves_relative_to_own_directory_not_root() {
    let dir = TempDir::new("zzop-pkg-entries-nested");
    dir.write("packages/foo/package.json", r#"{"main": "./index.ts"}"#);
    let all_paths: HashSet<String> = ["packages/foo/index.ts".to_string()].into_iter().collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("packages/foo/package.json".to_string()),
        &all_paths,
    );
    assert_eq!(scan.entry_paths, all_paths);
}

// --- PackageJsonScan::workspace_pkgs ---

#[test]
fn package_json_entries_collects_workspace_pkg_name_to_main_entry() {
    let dir = TempDir::new("zzop-pkg-entries-ws-main");
    dir.write(
        "packages/prisma/package.json",
        r#"{"name": "@acme/prisma", "main": "index.ts"}"#,
    );
    let all_paths: HashSet<String> = ["packages/prisma/index.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("packages/prisma/package.json".to_string()),
        &all_paths,
    );
    let pkg = scan.workspace_pkgs.get("@acme/prisma").unwrap();
    assert_eq!(pkg.dir, "packages/prisma");
    assert_eq!(pkg.entry.as_deref(), Some("packages/prisma/index.ts"));
}

#[test]
fn package_json_entries_falls_back_to_index_ts_when_no_main_module_exports() {
    let dir = TempDir::new("zzop-pkg-entries-ws-index-fallback");
    dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
    dir.write("packages/lib/index.ts", "export {};\n");
    let all_paths: HashSet<String> = ["packages/lib/index.ts".to_string()].into_iter().collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("packages/lib/package.json".to_string()),
        &all_paths,
    );
    let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
    assert_eq!(pkg.entry.as_deref(), Some("packages/lib/index.ts"));
}

#[test]
fn package_json_entries_workspace_pkg_entry_none_when_nothing_resolves() {
    // A pure sub-path-only package with no entry point: no `main`/`module`/`exports`, no root
    // `index.ts` — every import of it names a sub-path. `entry` staying `None` (rather than some
    // guessed path) is the honest signal.
    let dir = TempDir::new("zzop-pkg-entries-ws-no-entry");
    dir.write("packages/lib/package.json", r#"{"name": "@acme/lib"}"#);
    dir.write("packages/lib/tracking.ts", "export {};\n");
    let all_paths: HashSet<String> = ["packages/lib/tracking.ts".to_string()]
        .into_iter()
        .collect();
    let scan = package_json_entries(
        dir.path(),
        std::iter::once("packages/lib/package.json".to_string()),
        &all_paths,
    );
    let pkg = scan.workspace_pkgs.get("@acme/lib").unwrap();
    assert_eq!(pkg.dir, "packages/lib");
    assert_eq!(pkg.entry, None);
}
