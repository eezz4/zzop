//! End-to-end tests for tsconfig `compilerOptions.paths`/`baseUrl` alias import resolution — a bare
//! specifier remapped by a governing `tsconfig.json` (`import ... from '@/features/x'` where
//! `"@/*": ["./src/*"]`) now resolves to the real target file instead of falling through to the guessed
//! `@/` -> root-then-`src/` convention (or, for a non-`@/` alias, straight to external/`None`). See
//! `crates/engine/src/pipeline.rs`'s `tsconfig_scan` (tsconfig collection + one-level local `extends`
//! merge) and `parser/parser-typescript/src/resolve.rs`'s `resolve_via_paths`/`resolve_via_base_url` (the
//! actual matching/resolution logic threaded through `resolve_file_with_workspace`/
//! `build_dep_with_workspace`'s new `tsconfigs` parameter).
//!
//! Before this fix, a tsconfig `paths` alias other than the hardcoded `@/` convention (or an `@/*` mapping
//! that didn't happen to match the hardcoded root/`src` fallback — e.g. `@/*` -> `./app/*`) produced no
//! dep-graph edge at all: `dead-candidates`/`unimported-export` both flagged the aliased-only file as orphaned,
//! a false-positive pattern that scales with how much of a monorepo's tsconfig diverges from that one
//! hardcoded convention.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

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

// Both tests below deliberately map `@/*` -> `./app/*` (NOT `./src/*`) — the hardcoded `@/` convention
// `resolve_file` already falls back to (root-relative, then `src/`-relative) would NOT resolve this on its
// own, so a passing test here proves the new tsconfig-`paths` resolution is doing the work, not a
// coincidental overlap with the pre-existing convention fallback.

#[test]
fn tsconfig_paths_alias_clears_dead_candidates_on_the_target_file() {
    let dir = TempDir::new("zzop-engine-tsconfig-paths-dead-candidates");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["./app/*"]}}}"#,
    );
    dir.write(
        "entry.ts",
        "import { helper } from '@/lib/helper';\nexport const x = helper();\n",
    );
    dir.write(
        "app/lib/helper.ts",
        "export function helper() { return 1; }\n",
    );
    dir.write(
        "app/lib/orphan.ts",
        "export function neverImported() { return 2; }\n",
    );
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "app/lib/helper.ts"),
        "helper.ts should not be dead-candidates once the tsconfig-paths edge exists, got: {:?}",
        out.findings
    );

    // Regression control: orphan.ts is never imported by anyone — it must still be flagged, proving the fix
    // doesn't blanket-suppress every finding under the aliased root.
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "app/lib/orphan.ts"),
        "orphan.ts (never imported) should still be flagged dead-candidates, got: {:?}",
        out.findings
    );
}

#[test]
fn tsconfig_paths_alias_clears_dead_exports_on_the_consumed_symbol() {
    let dir = TempDir::new("zzop-engine-tsconfig-paths-unimported-export");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["./app/*"]}}}"#,
    );
    dir.write(
        "entry.ts",
        "import { helper } from '@/lib/helper';\nexport const x = helper();\n",
    );
    dir.write(
        "app/lib/helper.ts",
        "export function helper() { return 1; }\nexport function neverImported() { return 2; }\n",
    );
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.findings.iter().any(|f| f.rule_id == "unimported-export"
            && f.file == "app/lib/helper.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "helper")),
        "helper should not be unimported-export once the tsconfig-paths resolver sees the import, got: {:?}",
        out.findings
    );

    assert!(
        out.findings.iter().any(|f| f.rule_id == "unimported-export"
            && f.file == "app/lib/helper.ts"
            && f.data
                .as_ref()
                .is_some_and(|d| d["name"] == "neverImported")),
        "neverImported (no consumer) should still be flagged unimported-export, got: {:?}",
        out.findings
    );
}

#[test]
fn bare_specifier_resolves_via_base_url_without_a_paths_entry() {
    // No `paths` pattern matches `lib/helper` at all — it resolves purely via `baseUrl`, the
    // "absolute-from-src" import convention.
    let dir = TempDir::new("zzop-engine-tsconfig-base-url-bare");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": "src"}}"#,
    );
    dir.write(
        "src/entry.ts",
        "import { helper } from 'lib/helper';\nexport const x = helper();\n",
    );
    dir.write(
        "src/lib/helper.ts",
        "export function helper() { return 1; }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "src/lib/helper.ts"),
        "helper.ts should resolve via bare baseUrl-relative import, got: {:?}",
        out.findings
    );
}

#[test]
fn control_without_tsconfig_the_same_import_still_looks_orphaned() {
    // Same fixture as the first test, minus the tsconfig.json — proves the passing assertions above are
    // actually exercising the new tsconfig-`paths` resolver: without it, `@/lib/helper` only has the
    // hardcoded `@/` -> root-then-`src/` convention to fall back to, and `app/lib/helper.ts` matches
    // neither (it's under `app/`, not root `lib/` or `src/lib/`), so the file still looks orphaned.
    let dir = TempDir::new("zzop-engine-tsconfig-paths-control");
    dir.write(
        "entry.ts",
        "import { helper } from '@/lib/helper';\nexport const x = helper();\n",
    );
    dir.write(
        "app/lib/helper.ts",
        "export function helper() { return 1; }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "app/lib/helper.ts"),
        "without a tsconfig.json, '@/lib/helper' should NOT resolve to app/lib/helper.ts — helper.ts should \
         still be flagged dead-candidates, got: {:?}",
        out.findings
    );
}

// --- The three-layout matrix ---------------------------------------------------------------
//
// `npm create vite@latest` has scaffolded a SPLIT tsconfig for TypeScript projects for roughly two
// years: the tree's `tsconfig.json` is a solution file (`"files": []`) that owns no compilerOptions and
// only names project `references`, while the real `baseUrl`/`paths` sit in `tsconfig.app.json`. zzop
// discovered a `tsconfig.json` at any depth and followed one local `extends` level, but never read a
// referenced config — so every `@/*` import in that (very common) layout resolved to nothing.
//
// The three layouts differ ONLY in where the identical `@/*` -> `./src/*` mapping is written; all three
// must resolve the same aliased import. The two inline layouts are the proof that following references
// EXTENDS resolution rather than replacing it. Every layout pairs the newly-resolved `Navbar.tsx`
// against `Ghost.tsx` — a file nothing imports — in the SAME assertion, because this change can only
// ever REMOVE `dead-candidates` findings and a fix that over-suppressed would otherwise look identical
// to a fix that worked.

/// Writes the shared four-file frontend under `<root>/<prefix>`: an aliased importer, a relative-path
/// importer, the alias target, and one file nothing imports at all (the control).
fn write_alias_fixture(dir: &TempDir, prefix: &str) {
    let p = |rel: &str| {
        if prefix.is_empty() {
            rel.to_string()
        } else {
            format!("{prefix}/{rel}")
        }
    };
    dir.write(
        &p("src/components/Navbar.tsx"),
        "export function Navbar() { return null; }\n",
    );
    // Control: imported by nobody, under the same aliased root as Navbar.tsx.
    dir.write(
        &p("src/components/Ghost.tsx"),
        "export function Ghost() { return null; }\n",
    );
    dir.write(
        &p("src/router.tsx"),
        "import { Navbar } from \"@/components/Navbar\";\nexport const routes = [Navbar];\n",
    );
    dir.write(
        &p("src/app.tsx"),
        "import { routes } from \"./router\";\nexport const App = routes;\n",
    );
}

/// Asserts the alias target escaped `dead-candidates` AND the never-imported control did not — one
/// assertion pair, so an over-suppressing fix cannot pass by clearing both.
fn assert_alias_resolved_and_control_still_flagged(
    out: &zzop_engine::AnalyzeOutput,
    prefix: &str,
    layout: &str,
) {
    let p = |rel: &str| {
        if prefix.is_empty() {
            rel.to_string()
        } else {
            format!("{prefix}/{rel}")
        }
    };
    let dead = |f: &str| {
        out.findings
            .iter()
            .any(|x| x.rule_id == "dead-candidates" && x.file == f)
    };
    assert!(
        !dead(&p("src/components/Navbar.tsx")),
        "[{layout}] Navbar.tsx is imported at src/router.tsx via the `@/*` alias — it must not be \
         dead-candidates, got: {:?}",
        out.findings
    );
    assert!(
        dead(&p("src/components/Ghost.tsx")),
        "[{layout}] Ghost.tsx is imported by nobody — it must STILL be dead-candidates, otherwise the \
         alias fix is blanket-suppressing rather than resolving, got: {:?}",
        out.findings
    );
}

#[test]
fn matrix_paths_inline_in_a_nested_tsconfig_resolves() {
    // Layout A — `web/tsconfig.json` carries the mapping itself. Already worked; here as the proof that
    // reference-following did not break the nearest-ancestor walk it sits on top of.
    let dir = TempDir::new("zzop-engine-tsconfig-matrix-a");
    write_alias_fixture(&dir, "web");
    dir.write(
        "web/tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
    );
    let out = analyze_tree(dir.path(), &config());
    assert_alias_resolved_and_control_still_flagged(&out, "web", "A: paths inline, nested");
}

#[test]
fn matrix_paths_reached_only_through_project_references_resolves() {
    // Layout B — the Vite scaffold. `web/tsconfig.json` owns no compilerOptions at all; the mapping is
    // reachable ONLY by following `references`. This is the defect: before the fix Navbar.tsx was
    // reported dead-candidates here while layouts A and C cleared it from identical source.
    let dir = TempDir::new("zzop-engine-tsconfig-matrix-b");
    write_alias_fixture(&dir, "web");
    dir.write(
        "web/tsconfig.json",
        r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}, {"path": "./tsconfig.node.json"}]}"#,
    );
    dir.write(
        "web/tsconfig.app.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}, "include": ["src"]}"#,
    );
    // The scaffold's second reference: real, parsed, and contributing nothing. It must not disturb the
    // first one's mapping.
    dir.write(
        "web/tsconfig.node.json",
        r#"{"compilerOptions": {"strict": true}, "include": ["vite.config.ts"]}"#,
    );
    let out = analyze_tree(dir.path(), &config());
    assert_alias_resolved_and_control_still_flagged(&out, "web", "B: paths via references");
}

#[test]
fn matrix_paths_inline_at_the_tree_root_resolves() {
    // Layout C — the mapping at the tree root. Second control for layout B.
    let dir = TempDir::new("zzop-engine-tsconfig-matrix-c");
    write_alias_fixture(&dir, "");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
    );
    let out = analyze_tree(dir.path(), &config());
    assert_alias_resolved_and_control_still_flagged(&out, "", "C: paths inline, root");
}

#[test]
fn a_reference_that_maps_nothing_leaves_the_tree_exactly_as_it_was() {
    // The suppression direction's own control: a solution-style tsconfig whose referenced configs carry
    // NO `paths` must not start resolving anything. Both files stay reported.
    let dir = TempDir::new("zzop-engine-tsconfig-ref-empty");
    write_alias_fixture(&dir, "web");
    dir.write(
        "web/tsconfig.json",
        r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}]}"#,
    );
    dir.write(
        "web/tsconfig.app.json",
        r#"{"compilerOptions": {"strict": true}, "include": ["src"]}"#,
    );
    let out = analyze_tree(dir.path(), &config());
    for f in [
        "web/src/components/Navbar.tsx",
        "web/src/components/Ghost.tsx",
    ] {
        assert!(
            out.findings
                .iter()
                .any(|x| x.rule_id == "dead-candidates" && x.file == f),
            "no referenced config declares `paths`, so `@/components/Navbar` must still resolve to \
             nothing and {f} must stay dead-candidates, got: {:?}",
            out.findings
        );
    }
}

#[test]
fn tsconfig_extends_merges_paths_from_a_local_base_config() {
    let dir = TempDir::new("zzop-engine-tsconfig-extends");
    dir.write(
        "tsconfig.base.json",
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["./shared/*"]}}}"#,
    );
    dir.write("tsconfig.json", r#"{"extends": "./tsconfig.base.json"}"#);
    dir.write(
        "entry.ts",
        "import { helper } from '@shared/helper';\nexport const x = helper();\n",
    );
    dir.write(
        "shared/helper.ts",
        "export function helper() { return 1; }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "shared/helper.ts"),
        "helper.ts should resolve via the extended base config's paths, got: {:?}",
        out.findings
    );
}
