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
//! dep-graph edge at all: `dead-candidates`/`dead-exports` both flagged the aliased-only file as orphaned,
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
    let dir = TempDir::new("zzop-engine-tsconfig-paths-dead-exports");
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
        !out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "app/lib/helper.ts"
            && f.data.as_ref().is_some_and(|d| d["name"] == "helper")),
        "helper should not be dead-exports once the tsconfig-paths resolver sees the import, got: {:?}",
        out.findings
    );

    assert!(
        out.findings.iter().any(|f| f.rule_id == "dead-exports"
            && f.file == "app/lib/helper.ts"
            && f.data
                .as_ref()
                .is_some_and(|d| d["name"] == "neverImported")),
        "neverImported (no consumer) should still be flagged dead-exports, got: {:?}",
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
