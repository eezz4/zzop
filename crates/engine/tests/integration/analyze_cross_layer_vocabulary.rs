//! End-to-end coverage for the CROSS-LAYER half of `vocabulary.*` — the three keys
//! (`secretParamNames`, `apiVersionSegmentPattern`, `externallyFetchedPaths`) whose rules run over the
//! multi-tree `analyze_trees` join rather than a single tree, so their declaration has to survive the
//! per-key tree-union merge (`cross_layer_findings::merge_config::union_vocabulary`) before it reaches a
//! rule at all.
//!
//! Like `analyze_vocabulary_config.rs`, every assertion here is an INVALIDATION test: a declarable knob
//! whose declaration changes nothing is worse than no knob, so the declared run is compared against the
//! same fixture's built-in answer. Same `TempDir` fixture-tree pattern as `analyze_cross_layer_findings.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig, VocabularyConfig};

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

/// FE tree with one external egress call carrying a `?token=` query parameter — the
/// `cross-layer/external-secret-in-url` substrate, and the exact case `vocabulary.secretParamNames`
/// governs (`token` is a built-in secret-shaped name).
fn secret_url_fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlv-fe");
    dir.write(
        "src/Client.ts",
        "export function load() { return fetch(\"https://api.vendor.com/v1/users?token=abc123\"); }\n",
    );
    dir
}

/// A second tree that declares NO vocabulary at all — present so the run really goes through the per-key
/// tree-union merge instead of trivially reading one tree's config.
fn silent_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-xlv-be");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/local\", api.local);\n",
    );
    dir
}

fn config(source_id: &str, vocabulary: VocabularyConfig) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        vocabulary,
        ..EngineConfig::default()
    }
}

fn secret_findings(fe_vocabulary: VocabularyConfig) -> usize {
    let fe = secret_url_fe_tree();
    let be = silent_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe", fe_vocabulary)),
        (
            be.path().to_path_buf(),
            config("be", VocabularyConfig::default()),
        ),
    ];
    analyze_trees(&trees)
        .cross_layer_findings
        .iter()
        .filter(|f| f.rule_id == "cross-layer/external-secret-in-url")
        .count()
}

/// Seals that `vocabulary.secretParamNames` is WIRED, not merely recognized: declared on one tree of a
/// multi-tree run it survives the per-key union merge and replaces the list whole, so a project that
/// does not call `token` a secret stops seeing `cross-layer/external-secret-in-url` on a `?token=` URL.
/// The zzop-values run is the invalidation half — with the starter vocabulary declared it must fire, or
/// the two negative arms below would prove nothing.
#[test]
fn a_declared_secret_param_vocabulary_that_omits_token_silences_the_cross_layer_rule() {
    assert_eq!(
        secret_findings(VocabularyConfig::built_in()),
        1,
        "with zzop's own secret-param vocabulary declared, `?token=` must be flagged"
    );
    assert_eq!(
        secret_findings(VocabularyConfig {
            secret_param_names: vec!["sessionid".to_string()],
            ..VocabularyConfig::built_in()
        }),
        0,
        "a declared list that omits `token` must replace the list whole and silence the rule"
    );
    assert_eq!(
        secret_findings(VocabularyConfig::default()),
        0,
        "and with the key left undeclared entirely no parameter name is a secret — since 2026-07-27 \
         an absent vocabulary is a judgment not made, never a request for zzop's list"
    );
}
