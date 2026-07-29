//! Pin: the top-level `exclude` config key (`RuleConfig::global_excludes`) must reach
//! `MultiAnalyzeOutput::cross_layer_findings`, not just per-tree `AnalyzeOutput::findings`.
//!
//! Before this fix, `cross_layer_findings::merge_config::union_configs` built its `merge_config` with
//! `..RuleConfig::default()`, which reset `global_excludes` to empty. Measured on `fe-axios` x `be-express`
//! via `zzop cross`: adding `exclude: ["src/app/routes/**", "src/services/**"]` took the per-tree
//! `findingCount` from 36 to 10 while `crossLayerFindings` stayed at 33, still naming
//! `src/app/routes/article/article.controller.ts`. `GlobalExclude`'s own contract
//! (`zzop_core::registry::config`) says a matching file has "findings from EVERY rule dropped, not just
//! one" — cross-layer rule ids live in the same id space as user `rules: {}` entries and what they emit
//! are findings, so that was a contract violation, not a judgement call.
//!
//! Coverage (mirrors the `TempDir` harness of `analyze_cross_layer_severity_override.rs`, the sibling pin
//! for `severity_overrides`):
//! - Baseline: one unconsumed gin route fires `cross-layer/unconsumed-endpoint`, anchored in `handler.go`.
//! - An `exclude` on the OWNING tree drops that cross-layer finding.
//! - An `exclude` declared on an unrelated tree ALSO drops it — sealing the union direction, which is
//!   deliberate and documented in `union_configs` (there is no expressible per-tree filter; see that doc).
//! - An `exclude` that matches no file leaves the finding alone (no over-suppression).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{GlobalExclude, RuleConfig};
use zzop_engine::{analyze_trees, EngineConfig};

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

const RULE_ID: &str = "cross-layer/unconsumed-endpoint";

fn config(source_id: &str, excludes: &[GlobalExclude]) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        rule_config: RuleConfig {
            global_excludes: excludes.to_vec(),
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

fn glob(pattern: &str) -> GlobalExclude {
    GlobalExclude {
        path: None,
        glob: Some(pattern.to_string()),
    }
}

/// One gin route nobody in the run consumes — fires `cross-layer/unconsumed-endpoint` anchored in
/// `internal/handler.go`.
fn orphan_route_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlayer-exclude-be");
    dir.write(
        "internal/handler.go",
        concat!(
            "package main\n",
            "\n",
            "import \"github.com/gin-gonic/gin\"\n",
            "\n",
            "func listOrphans(c *gin.Context) {}\n",
            "\n",
            "func setup() {\n",
            "\tr := gin.Default()\n",
            "\tr.GET(\"/api/orphans\", listOrphans)\n",
            "}\n",
        ),
    );
    dir
}

/// An unrelated tree with no io facts at all — a vehicle for its own `exclude` entry in the union test.
fn empty_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlayer-exclude-empty");
    dir.write("README.md", "nothing here\n");
    dir
}

#[test]
fn baseline_the_unconsumed_endpoint_finding_names_the_route_file() {
    let be = orphan_route_tree();
    let trees = vec![(be.path().to_path_buf(), config("be", &[]))];
    let out = analyze_trees(&trees);

    let finding = out
        .cross_layer_findings
        .iter()
        .find(|f| f.rule_id == RULE_ID)
        .unwrap_or_else(|| {
            panic!(
                "expected a {RULE_ID} finding, got: {:?}",
                out.cross_layer_findings
            )
        });
    assert!(
        finding.file.ends_with("handler.go"),
        "the finding must anchor in the route file (that path is what `exclude` filters on), got {:?}",
        finding.file
    );
}

#[test]
fn an_exclude_on_the_owning_tree_drops_the_cross_layer_finding() {
    let be = orphan_route_tree();
    let trees = vec![(
        be.path().to_path_buf(),
        config("be", &[glob("internal/**")]),
    )];
    let out = analyze_trees(&trees);

    assert!(
        !out.cross_layer_findings
            .iter()
            .any(|f| f.rule_id == RULE_ID),
        "`exclude` covers the only file this finding names, so the run must not report it; got: {:?}",
        out.cross_layer_findings
    );
}

/// The union direction, made explicit: an `exclude` declared by ANY tree filters the whole run's
/// cross-layer output. `union_configs`' doc argues why per-tree attribution is not expressible for a
/// joint-analysis output; this test is where that choice is visible, so a future change to per-tree
/// semantics has to come here and say so.
#[test]
fn an_exclude_declared_by_another_tree_also_drops_it_union_direction() {
    let be = orphan_route_tree();
    let other = empty_tree();
    let trees = vec![
        (be.path().to_path_buf(), config("be", &[])),
        (
            other.path().to_path_buf(),
            config("other", &[glob("internal/**")]),
        ),
    ];
    let out = analyze_trees(&trees);

    assert!(
        !out.cross_layer_findings
            .iter()
            .any(|f| f.rule_id == RULE_ID),
        "exclude-only union: any tree's `exclude` filters the shared cross-layer output; got: {:?}",
        out.cross_layer_findings
    );
}

/// The other half of the contract — the union must not become a blanket drop. An `exclude` matching no
/// file leaves every cross-layer finding standing.
#[test]
fn an_exclude_matching_nothing_leaves_the_finding_standing() {
    let be = orphan_route_tree();
    let trees = vec![(
        be.path().to_path_buf(),
        config("be", &[glob("does/not/exist/**")]),
    )];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer_findings
            .iter()
            .any(|f| f.rule_id == RULE_ID),
        "an `exclude` that matches nothing must suppress nothing, got: {:?}",
        out.cross_layer_findings
    );
}
