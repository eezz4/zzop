//! End-to-end tests for workspace-alias import resolution — a bare or sub-path specifier that names a
//! sibling monorepo package's `package.json` `name` field (`import ... from '@scope/pkg-b'` /
//! `'@scope/pkg-b/util'`) now resolves to that package's real file instead of falling through to `None`
//! like any other external/npm specifier. See `packages/engine/src/pipeline.rs`'s `package_json_entries`
//! (workspace-name collection) and `parser/parser-typescript/src/resolve.rs`'s
//! `resolve_file_with_workspace`/`build_dep_with_workspace` (the actual resolution/dep-graph wiring).
//!
//! Before this fix, `analyze::assemble` called the plain `zpz_parser_typescript::build_dep` (no workspace
//! awareness) and `dead_exports`' resolver closure called the plain `resolve_file` — both treat ANY
//! non-`.`/non-`@/` specifier as external, so a cross-package import produced no dep-graph edge at all.
//! That made the imported package's file look unreferenced from outside: `dead-candidates` (file-level,
//! `fan_in == 0`) and `dead-exports` (symbol-level, "no importer") both fired on code a sibling package
//! genuinely uses.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// Two-package fixture: `@scope/pkg-a`'s `index.ts` imports `helper` from `@scope/pkg-b`'s `util.ts` via a
/// sub-path specifier (`@scope/pkg-b/util` — `pkg-b`'s manifest deliberately has no `main`/`module`, a
/// no-entry-field, sub-path-only-imports package shape). A sibling `orphan.ts` in `pkg-b`, exporting a
/// function nobody imports, is the regression control proving the fix doesn't just blanket-suppress every
/// finding in a package that happens to be imported by name elsewhere.
fn write_two_package_fixture(dir: &TempDir) {
    dir.write("packages/pkg-b/package.json", r#"{"name": "@scope/pkg-b"}"#);
    dir.write(
        "packages/pkg-b/util.ts",
        "export function helper() { return 1; }\n",
    );
    dir.write(
        "packages/pkg-b/orphan.ts",
        "export function neverImported() { return 2; }\n",
    );
    dir.write("packages/pkg-a/package.json", r#"{"name": "@scope/pkg-a"}"#);
    dir.write(
        "packages/pkg-a/index.ts",
        "import { helper } from '@scope/pkg-b/util';\nexport const x = helper();\n",
    );
}

#[test]
fn cross_package_workspace_import_clears_dead_candidates_on_the_target_file() {
    let dir = TempDir::new("zpz-engine-ws-alias-dead-candidates");
    write_two_package_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());

    // `util.ts` is reachable only via `pkg-a`'s workspace import — no in-package entry field names it
    // (`pkg-b`'s manifest has no `main`/`module`/`exports`), so `fan_in` depends entirely on the
    // cross-package dep-graph edge this fix adds.
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "packages/pkg-b/util.ts"),
        "util.ts should not be dead-candidates once the workspace-alias edge exists, got: {:?}",
        out.findings
    );

    // Regression control: `orphan.ts` is never imported by anyone (workspace or otherwise) — it must still
    // be flagged. This is a plain file-level orphan check ({@scope/pkg-b}'s manifest doesn't name it as an
    // entry either), independent of dead-candidates' own entry-point exemptions.
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "packages/pkg-b/orphan.ts"),
        "orphan.ts (never imported) should still be flagged dead-candidates, got: {:?}",
        out.findings
    );
}

#[test]
fn cross_package_workspace_import_clears_dead_exports_on_the_consumed_symbol() {
    let dir = TempDir::new("zpz-engine-ws-alias-dead-exports");
    write_two_package_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());

    // `helper` is imported by `pkg-a` through the workspace-alias specifier — the dead-exports resolver
    // closure must resolve that specifier back to `util.ts` for `helper` to count as used.
    assert!(
        !out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "packages/pkg-b/util.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "helper")),
        "helper should not be dead-exports once the workspace-alias resolver sees the import, got: {:?}",
        out.findings
    );

    // Regression control: `neverImported` has no consumer anywhere and must still be flagged.
    assert!(
        out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "packages/pkg-b/orphan.ts"
            && f.data
                .as_ref()
                .is_some_and(|d| d["name"] == "neverImported")),
        "neverImported (no consumer) should still be flagged dead-exports, got: {:?}",
        out.findings
    );
}

#[test]
fn bare_workspace_specifier_resolves_to_the_named_packages_main_entry() {
    // Companion to the sub-path case above: `pkg-c` DOES declare a `main`, and `pkg-a` imports it with a
    // bare specifier (no sub-path) — exercises `WorkspacePkg::entry` end to end, not just `dir`. The main
    // file is deliberately named `root.ts`, not `index.ts`/`main.ts` — both match dead-exports' own
    // `entry_patterns` (exempt unconditionally, real importer or not), which would make this test pass
    // even without the workspace-alias fix and prove nothing.
    let dir = TempDir::new("zpz-engine-ws-alias-bare-entry");
    dir.write(
        "packages/pkg-c/package.json",
        r#"{"name": "@scope/pkg-c", "main": "root.ts"}"#,
    );
    dir.write(
        "packages/pkg-c/root.ts",
        "export function fromC() { return 3; }\n",
    );
    dir.write("packages/pkg-a/package.json", r#"{"name": "@scope/pkg-a"}"#);
    dir.write(
        "packages/pkg-a/index.ts",
        "import { fromC } from '@scope/pkg-c';\nexport const y = fromC();\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "packages/pkg-c/root.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "fromC")),
        "fromC should not be dead-exports via the bare-specifier workspace entry resolution, got: {:?}",
        out.findings
    );
}
