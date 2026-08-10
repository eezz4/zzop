//! D19 end-to-end: `cache-lane-file-read` through the real engine, on a Rust tree with NO HTTP routes.
//!
//! That last part is the point of the first test. Until this rule existed, `run_callgraph_rules` returned
//! immediately when a tree had no `http` provides, because every consumer it had was a route rule. A
//! library, a compiler, a build tool — exactly the trees whose caches this rule audits — would have been
//! skipped, and skipped SILENTLY. So "a route-free tree still gets this rule" is pinned here rather than
//! left to the reader of the gate.
//!
//! The rest is the invalidation discipline this repo applies to every guard: a check nobody has seen go
//! red is not a check. Each test below plants exactly one thing and removes exactly one thing.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, VocabularyConfig};

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

/// The engine's built-in vocabulary plus the one key that has no built-in — see
/// `VocabularyConfig::cache_lane_anchor_pattern`'s own doc for why it ships undeclared.
fn config_with_anchor(anchor: Option<&str>) -> EngineConfig {
    EngineConfig {
        vocabulary: VocabularyConfig {
            cache_lane_anchor_pattern: anchor.map(str::to_string),
            ..VocabularyConfig::built_in()
        },
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir, anchor: Option<&str>) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config_with_anchor(anchor))
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

const ANCHOR: &str = "^compute_fresh_artifact$";

/// Two files, no routes anywhere. The lane calls a helper in the OTHER file, and only that helper reads —
/// the shape a text scan of the lane's own file cannot see, which is the whole argument for making this a
/// rule instead of another `scripts/check-*.sh`.
fn tree_with_cross_file_read(dir: &TempDir, helper_reads: bool) {
    dir.write(
        "src/manifest.rs",
        &if helper_reads {
            concat!(
                "use std::fs;\n\n",
                "pub fn load_manifest(root: &str) -> String {\n",
                "    fs::read_to_string(root).unwrap_or_default()\n",
                "}\n"
            )
            .to_string()
        } else {
            // Same symbol, same call graph, no filesystem read — the ONE thing that differs.
            concat!(
                "pub fn load_manifest(root: &str) -> String {\n",
                "    root.to_string()\n",
                "}\n"
            )
            .to_string()
        },
    );
    dir.write(
        "src/fresh.rs",
        concat!(
            "use crate::manifest::load_manifest;\n\n",
            "pub fn compute_fresh_artifact(rel: &str) -> String {\n",
            "    let extra = load_manifest(rel);\n",
            "    format!(\"{rel}{extra}\")\n",
            "}\n"
        ),
    );
    dir.write("src/lib.rs", "pub mod fresh;\npub mod manifest;\n");
}

#[test]
fn a_read_one_hop_outside_the_cached_lane_is_found_on_a_route_free_tree() {
    let dir = TempDir::new("zzop-cache-lane-red");
    tree_with_cross_file_read(&dir, true);
    let out = scan(&dir, Some(ANCHOR));
    let found = hits(&out, "cache-lane-file-read");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/fresh.rs");
    let d = found[0].data.as_ref().unwrap();
    assert_eq!(d["anchor"], "compute_fresh_artifact");
    assert_eq!(d["callee"], "read_to_string");
    assert_eq!(
        d["reachedSymbol"], "src/manifest.rs#load_manifest",
        "the cross-file hop is the whole point — a same-file-only check would have missed this"
    );
}

/// The GREEN half of the same fixture: remove the read, keep the call graph. If this ever fires, the rule
/// is reporting the call rather than the read.
#[test]
fn the_same_lane_without_a_read_is_silent() {
    let dir = TempDir::new("zzop-cache-lane-green");
    tree_with_cross_file_read(&dir, false);
    let out = scan(&dir, Some(ANCHOR));
    assert!(
        hits(&out, "cache-lane-file-read").is_empty(),
        "{:?}",
        out.findings
    );
}

/// D14 end-to-end: with the anchor undeclared the rule judges nothing — and, because the pass's own gate
/// reads the same key, the tree is not even walked for it. Same fixture that fires above, so this proves
/// the vocabulary is what silenced it.
#[test]
fn an_undeclared_anchor_makes_no_judgment_on_the_very_tree_that_would_fire() {
    let dir = TempDir::new("zzop-cache-lane-undeclared");
    tree_with_cross_file_read(&dir, true);
    let out = scan(&dir, None);
    assert!(
        hits(&out, "cache-lane-file-read").is_empty(),
        "{:?}",
        out.findings
    );
}

/// A read that the lane cannot reach belongs to somebody else. The rule's claim is REACHABILITY, so an
/// unreachable read in the same tree must not be attributed to the lane.
#[test]
fn a_read_the_lane_cannot_reach_is_not_reported() {
    let dir = TempDir::new("zzop-cache-lane-unreachable");
    tree_with_cross_file_read(&dir, false);
    dir.write(
        "src/unrelated.rs",
        concat!(
            "use std::fs;\n\n",
            "pub fn dump(p: &str) -> String {\n",
            "    fs::read_to_string(p).unwrap_or_default()\n",
            "}\n"
        ),
    );
    dir.write(
        "src/lib.rs",
        "pub mod fresh;\npub mod manifest;\npub mod unrelated;\n",
    );
    let out = scan(&dir, Some(ANCHOR));
    assert!(
        hits(&out, "cache-lane-file-read").is_empty(),
        "{:?}",
        out.findings
    );
}

/// The rule is disableable like any other, and turning it off must not leave the pass half-running.
#[test]
fn disabling_the_rule_silences_it() {
    let dir = TempDir::new("zzop-cache-lane-disabled");
    tree_with_cross_file_read(&dir, true);
    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            rule_config: zzop_core::RuleConfig {
                disabled_rules: vec!["cache-lane-file-read".to_string()],
                ..zzop_core::RuleConfig::default()
            },
            ..config_with_anchor(Some(ANCHOR))
        },
    );
    assert!(
        hits(&out, "cache-lane-file-read").is_empty(),
        "{:?}",
        out.findings
    );
}
