//! Covers dead-island detection: a closed cycle unreachable from any entrypoint is flagged, a library's
//! public API entry keeps its helpers live, and files reachable from a test entrypoint are not flagged.
use super::*;
use std::collections::HashMap;

/// The no-extra-entries default every pre-existing case uses — the parameter only matters for the
/// caller-known-entry shapes (cargo-declared targets, overlay `is_entry`) covered by the dedicated
/// tests below.
fn no_extra() -> HashSet<String> {
    HashSet::new()
}

#[test]
fn extra_entry_with_fan_in_is_not_flagged_and_keeps_its_island_live() {
    // `packs/http.rs` is imported by a sibling (fan_in 1) yet nothing conventional reaches it — the
    // exact shape of a cargo `[[test]] path = "..."` target file (found by the first self-analysis
    // dogfood run). Passing it as an extra entry must both unflag it AND mark what it reaches live.
    let d = dep(&[
        ("packs/http.rs", &["packs/util.rs"]),
        ("packs/util.rs", &["packs/http.rs"]),
    ]);
    let nodes = vec![node("packs/http.rs", 1, 10), node("packs/util.rs", 1, 10)];
    assert_eq!(find_unreachable(&nodes, &d, 30, &no_extra()).len(), 2);
    let extra: HashSet<String> = std::iter::once("packs/http.rs".to_string()).collect();
    assert!(find_unreachable(&nodes, &d, 30, &extra).is_empty());
}

#[test]
fn asset_target_with_fan_in_but_no_dep_edge_needs_extra_entry() {
    // A `public/*.js` worklet gets fan_in from an asset-ref bump (`merge_asset_ref_fan_in`) but NO
    // incoming `dep` edge (the engine adds none — it mirrors the SFC fan-in bump), so it reads as a
    // false `unreachable` island unless seeded as an entry. This is the regression sentinel for the
    // mandatory `unreachable_entries.extend(asset_targets)` seed (assemble/rules.rs): without it, the
    // fix would trade a dead-candidates FP for an unreachable FP.
    let asset = "ai-hub-fe/public/noise-suppressor/rnnoiseWorklet.js";
    let d = DepGraph::new(); // no edge points at the asset
    let nodes = vec![node(asset, 1, 20)];
    assert_eq!(
        find_unreachable(&nodes, &d, 30, &no_extra()).len(),
        1,
        "without the seed, a fan_in>0 asset target with no dep edge is a false unreachable island"
    );
    let extra: HashSet<String> = std::iter::once(asset.to_string()).collect();
    assert!(
        find_unreachable(&nodes, &d, 30, &extra).is_empty(),
        "seeding the asset target as an extra entry must clear the false unreachable island"
    );
}

#[test]
fn e2e_infra_directories_are_test_paths() {
    assert!(is_test_file(
        "packages/testing/playwright/scripts/import-data.mjs"
    ));
    assert!(is_test_file("app/e2e/flows/login.ts"));
    assert!(is_test_file("cypress/scripts/setup.js"));
    // Whole-segment match only — names merely containing "testing" are not test paths.
    assert!(!is_test_file("src/app-testing-utils/service.ts"));
}

fn node(path: &str, fan_in: u32, loc: u32) -> FileNode {
    FileNode {
        id: path.into(),
        path: path.into(),
        change_count: 0,
        churn: 0,
        last_modified: None,
        author_count: 1,
        loc,
        tag_counts: HashMap::new(),
        fan_in,
        fan_out: 0,
        total_connections: 0,
        risk_score: 0.0,
        ..Default::default()
    }
}

fn dep(pairs: &[(&str, &[&str])]) -> DepGraph {
    pairs
        .iter()
        .map(|(k, vs)| (k.to_string(), vs.iter().map(|s| s.to_string()).collect()))
        .collect()
}

#[test]
fn flags_closed_dead_island() {
    let d = dep(&[
        ("index.ts", &["live.ts"]),
        ("live.ts", &[]),
        ("dead1.ts", &["dead2.ts"]),
        ("dead2.ts", &["dead1.ts"]),
    ]);
    let nodes = vec![
        node("index.ts", 0, 10),
        node("live.ts", 1, 10),
        node("dead1.ts", 1, 40),
        node("dead2.ts", 1, 20),
    ];
    let dead: Vec<String> = find_unreachable(&nodes, &d, 30, &no_extra())
        .into_iter()
        .map(|n| n.path)
        .collect();
    assert_eq!(dead, vec!["dead1.ts".to_string(), "dead2.ts".to_string()]); // ranked by loc desc
}

#[test]
fn does_not_flag_library_public_api() {
    let d = dep(&[("publicApi.ts", &["helper.ts"]), ("helper.ts", &[])]);
    let nodes = vec![node("publicApi.ts", 0, 10), node("helper.ts", 1, 10)];
    assert!(find_unreachable(&nodes, &d, 30, &no_extra()).is_empty());
}

#[test]
fn files_reachable_from_test_entry_not_flagged() {
    let d = dep(&[("x.test.ts", &["util.ts"]), ("util.ts", &[])]);
    let nodes = vec![node("x.test.ts", 0, 10), node("util.ts", 1, 10)];
    assert!(find_unreachable(&nodes, &d, 30, &no_extra()).is_empty());
}

/// Pins the exact rendered message — regression coverage for the `disable_hint` splice
/// `unreachable_findings` went through during the 2026-07-10 dialect-consolidation sweep.
#[test]
fn finding_message_is_byte_identical_to_the_pre_sweep_text() {
    let d = dep(&[("dead1.ts", &["dead2.ts"]), ("dead2.ts", &["dead1.ts"])]);
    let nodes = vec![node("dead1.ts", 1, 10), node("dead2.ts", 1, 5)];
    let out = unreachable_findings(&nodes, &d, &no_extra());
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].rule_id, "unreachable");
    assert_eq!(
            out[0].message,
            "file has 1 importer(s) in this tree but is unreachable from any entrypoint — its importers \
             form a closed island nothing outside it can reach, so it's effectively dead despite having \
             in-repo references. Delete the island, or wire it back to a real entrypoint if it should be \
             reachable. Disable via config `rules: { \"unreachable\": \"off\" }` (embedders: \
             `disabled_rules`) if this island is reached by a mechanism this graph doesn't see (e.g. \
             dynamic `require`, a plugin loader)."
        );
}

#[test]
fn tool_entry_file_positives() {
    for path in [
        "vite.config.ts",
        "vite.config.js",
        "vitest.config.mts",
        "jest.config.cjs",
        "playwright.config.ts",
        "tailwind.config.js",
        "postcss.config.cjs",
        "rollup.config.mjs",
        "webpack.config.js",
        "next.config.mjs",
        "nuxt.config.ts",
        "svelte.config.js",
        "astro.config.mts",
        "eslint.config.cts",
        ".eslintrc.cjs",
        ".eslintrc.js",
        ".prettierrc.cjs",
        ".babelrc.js",
        ".stylelintrc.js",
        "vite-env.d.ts",
        "foo.d.ts",
        "packages/app/src/vite-env.d.ts",
        // Test-runner setup entries (config-loaded via setupFiles/globalSetup, never imported).
        "src/setup-tests.ts",
        "vitest.setup.ts",
        "jest.setup.js",
        "setupTests.ts",
        "src/test-setup.ts",
        "e2e/global.setup.ts",
        "playwright/global.teardown.ts",
        // Tool-config entries loaded by their tool's own resolver, not imported.
        "jest.preset.js",
        "prisma/seed.ts",
        "server/prisma/seed.ts",
    ] {
        assert!(
            is_tool_entry_file(path),
            "expected tool-entry match: {path}"
        );
    }
}

#[test]
fn python_entry_pattern_positives() {
    // F1: `main.py`/`settings.py`/`conftest.py` join the existing `__main__`/`manage`/`wsgi`/`asgi`
    // Python entry-file group — all filename-convention-loaded, never imported.
    for path in [
        "__main__.py",
        "manage.py",
        "wsgi.py",
        "asgi.py",
        "main.py",
        "settings.py",
        "conftest.py",
        "app/main.py",
        "myproj/settings.py",
        "tests/conftest.py",
    ] {
        assert!(is_entry_file(path), "expected entry-file match: {path}");
    }
}

#[test]
fn python_entry_pattern_negatives() {
    // Anchored on the exact bare filename — a name merely containing one of these words, or a
    // similarly-named file with a different stem, does not match.
    for path in [
        "mainapp.py",
        "settings_local.py",
        "conftest_helpers.py",
        "main.pyc",
    ] {
        assert!(
            !is_entry_file(path),
            "expected NOT an entry-file match: {path}"
        );
    }
}

#[test]
fn rust_entry_pattern_positives() {
    // Crate/binary roots (`main.rs`/`lib.rs`/`build.rs`) plus anything under cargo's own
    // tests/examples/benches/src-bin convention directories — each compiled as its own separate target,
    // never `use`d from elsewhere, so zero in-repo importers is expected.
    for path in [
        "main.rs",
        "lib.rs",
        "build.rs",
        "src/main.rs",
        "crates/core/src/lib.rs",
        "tests/integration.rs",
        "crates/engine/tests/analyze_rust_self.rs",
        "examples/fastapi_overlay_adapter/main.rs",
        "benches/parse_bench.rs",
        "src/bin/cli.rs",
    ] {
        assert!(is_entry_file(path), "expected entry-file match: {path}");
    }
}

#[test]
fn rust_entry_pattern_negatives() {
    // An ordinary module file elsewhere in `src/` is not an entry file merely by being Rust.
    for path in [
        "src/handlers.rs",
        "src/lib/helpers.rs",
        "crates/core/src/util.rs",
    ] {
        assert!(
            !is_entry_file(path),
            "expected NOT an entry-file match: {path}"
        );
    }
}

#[test]
fn go_entry_pattern_positives() {
    // Package-main entry conventions: `main.go` anywhere in the tree.
    for path in ["main.go", "cmd/server/main.go", "internal/app/main.go"] {
        assert!(is_entry_file(path), "expected entry-file match: {path}");
    }
}

#[test]
fn go_entry_pattern_negatives() {
    // An ordinary package file elsewhere is not an entry file merely by being Go, and `mainx.go`/
    // `notmain.go` do not fuzzy-match the anchored `main.go` stem.
    for path in ["internal/db/db.go", "handlers.go", "mainx.go", "notmain.go"] {
        assert!(
            !is_entry_file(path),
            "expected NOT an entry-file match: {path}"
        );
    }
}

#[test]
fn go_test_file_is_recognized_as_a_test_path() {
    // `zzop_core::is_test_file` already carries the `_test.go` pattern — pinned here (rather than only
    // in `zzop-core`'s own suite) because `unreachable`'s own entry-set logic (`find_unreachable`
    // above) folds `is_test_file` results into the entry set, and a `_test.go` file reachable from
    // nothing else must still be treated as its own entry, not flagged as a dead island.
    assert!(is_test_file("handler_test.go"));
    assert!(is_test_file("internal/db/db_test.go"));
    // A `_test.go` file with zero fan-in and nothing importing it is its own entry (never flagged).
    let d = dep(&[("handler_test.go", &["handler.go"]), ("handler.go", &[])]);
    let nodes = vec![node("handler_test.go", 0, 10), node("handler.go", 1, 10)];
    assert!(find_unreachable(&nodes, &d, 30, &no_extra()).is_empty());
}

#[test]
fn tool_entry_file_negatives() {
    for path in [
        "config.ts",
        "card.ts",
        "features/x/Component.tsx",
        "index.ts",
        // Scoping guards: bare `seed.ts` (app code) is NOT the Prisma CLI entry; a `test-setup-*` stem
        // is not the exact `test-setup` setup file.
        "src/db/seed.ts",
        "src/test-setup-helpers.ts",
    ] {
        assert!(
            !is_tool_entry_file(path),
            "expected NOT a tool-entry match: {path}"
        );
    }
}
