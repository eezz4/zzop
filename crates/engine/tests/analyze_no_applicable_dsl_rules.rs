//! D16: the "N rules loaded, 0 findings" ambiguity — is the tree clean, or does no loaded DSL rule even
//! apply to this tree's filetypes? `zzop_engine::analyze::diagnostics::no_applicable_dsl_rule_warning`
//! (wired into both `analyze::assemble` and `envelope::analyze_envelope`) distinguishes the two: packs
//! loaded > 0 but not one loaded rule's `file_pattern` matches any analyzed file in this tree pushes one
//! per-tree self-report warning. Native structural/whole-graph analyses are never `file_pattern`-gated, so
//! they still ran regardless — this warning is purely about DSL rule-pack applicability.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
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

/// Every real shipped pack under `rules/dsl/` — same resolution shape `analyze_minified.rs`'s
/// `all_shipped_packs` uses. None of the bundled packs carry a `.go`-matching `file_pattern` (verified by
/// inspection: every shipped `file_pattern` targets `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs`/`.java`/
/// `.jsp`/... — no Go extension anywhere), which is exactly the real-world gap this warning exists to
/// self-report.
fn all_shipped_packs() -> Vec<RulePackDef> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result.packs.into_iter().map(|(_, pack)| pack).collect()
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "no-applicable-dsl-fixture".to_string(),
        packs: all_shipped_packs(),
        ..EngineConfig::default()
    }
}

#[test]
fn go_only_tree_with_default_packs_gets_the_no_applicable_dsl_rule_warning() {
    let dir = TempDir::new("zzop-engine-go-only-fixture");
    dir.write(
        "main.go",
        "package main\n\nfunc main() {\n\tprintln(\"hi\")\n}\n",
    );
    dir.write(
        "internal/service.go",
        "package internal\n\nfunc Run() int {\n\treturn 1\n}\n",
    );

    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.packs_loaded.is_empty(),
        "expected the shipped packs to load, got: {:?}",
        out.packs_loaded
    );
    assert!(
        out.warnings.iter().any(|w| w.contains("DSL rule(s) loaded")
            && w.contains("file_pattern")
            && w.contains("no applicable rules")),
        "expected the no-applicable-DSL-rule self-report on a Go-only tree, got: {:?}",
        out.warnings
    );
}

#[test]
fn ts_fixture_with_default_packs_gets_no_no_applicable_dsl_rule_warning() {
    let dir = TempDir::new("zzop-engine-ts-fixture");
    dir.write("src/index.ts", "export function run() { return 1; }\n");

    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.packs_loaded.is_empty(),
        "expected the shipped packs to load, got: {:?}",
        out.packs_loaded
    );
    assert!(
        !out
            .warnings
            .iter()
            .any(|w| w.contains("DSL rule(s) loaded") && w.contains("no applicable rules")),
        "a .ts file matches multiple shipped packs' file_pattern — the warning must not fire, got: {:?}",
        out.warnings
    );
}
