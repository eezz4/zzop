//! Pin for D12: `RuleConfig::severity_overrides` must reach `MultiAnalyzeOutput::cross_layer_findings`,
//! not just per-tree `AnalyzeOutput::findings`. Before this fix, `analyze_trees` (`trees.rs`) returned
//! `compute_cross_layer_findings`'s output unmapped — a `rules: {"cross-layer/unconsumed-endpoint":
//! {"severity":"warning"}}` config entry was honored for tree findings (via `merge_findings` inside
//! `analyze_tree`) but silently did nothing for the shared cross-layer output. Mirrors the
//! `TempDir`-harness style of `analyze_go_cross_layer.rs` / `analyze_rule_config.rs`.
//!
//! Coverage:
//! - A single BE tree with one gin route nobody in the run consumes fires `cross-layer/unconsumed-endpoint`
//!   at its default severity (`info`) with no override.
//! - The same fixture with a `severity_overrides` entry on that tree remaps the cross-layer finding's
//!   severity to `critical`.
//! - Conflict case: two trees each override the SAME cross-layer rule id to a DIFFERENT severity — the
//!   FIRST-declaring tree (by `trees` input order) wins.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{RuleConfig, Severity};
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

fn config(source_id: &str, severity_override: Option<Severity>) -> EngineConfig {
    let mut severity_overrides = BTreeMap::new();
    if let Some(sev) = severity_override {
        severity_overrides.insert(RULE_ID.to_string(), sev);
    }
    EngineConfig {
        source_id: source_id.to_string(),
        rule_config: RuleConfig {
            severity_overrides,
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    }
}

/// One gin route nobody in the run consumes — fires `cross-layer/unconsumed-endpoint`.
fn orphan_route_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlayer-severity-be");
    dir.write(
        "handler.go",
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

/// An unrelated tree with no io facts at all — just a vehicle for its own `severity_overrides` entry in
/// the conflict test.
fn empty_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlayer-severity-empty");
    dir.write("README.md", "nothing here\n");
    dir
}

#[test]
fn baseline_unconsumed_endpoint_fires_at_default_info_severity() {
    let be = orphan_route_tree();
    let trees = vec![(be.path().to_path_buf(), config("be", None))];
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
    assert_eq!(finding.severity, Severity::Info);
}

#[test]
fn severity_override_remaps_a_cross_layer_finding() {
    let be = orphan_route_tree();
    let trees = vec![(
        be.path().to_path_buf(),
        config("be", Some(Severity::Critical)),
    )];
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
    assert_eq!(
        finding.severity,
        Severity::Critical,
        "severity_overrides must remap the cross-layer finding's severity, got: {:?}",
        out.cross_layer_findings
    );
}

#[test]
fn conflicting_severity_overrides_across_trees_the_first_declaring_tree_wins() {
    let be = orphan_route_tree();
    let other = empty_tree();
    // "be" (first in the trees slice) overrides to critical; "other" (second) overrides the SAME id to
    // warning. First-declared must win.
    let trees = vec![
        (
            be.path().to_path_buf(),
            config("be", Some(Severity::Critical)),
        ),
        (
            other.path().to_path_buf(),
            config("other", Some(Severity::Warning)),
        ),
    ];
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
    assert_eq!(
        finding.severity,
        Severity::Critical,
        "the FIRST-declaring tree's override must win on conflict, got: {:?}",
        out.cross_layer_findings
    );
}
