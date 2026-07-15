//! End-to-end test for the IoFacts wiring: a BE controller file's NestJS-style decorator route
//! becomes a per-file `SourceFile.io` provide AND merges into the tree-wide `CommonIr.ir.io`; an
//! FE file's `fetch(...)` becomes a consume, merged the same way; an inline io-scan DSL pack (NOT
//! `rules/dsl/*.json` — inlined so the test controls the exact rule) fires on the unversioned
//! provide end to end through `zzop_engine::analyze_tree`.
//!
//! The BE fixture is deliberately Nest-style, NOT Hono-style: since the `+router-mounts-v1` batch,
//! code-registered router (Hono) provides compose whole-tree at assembly and are NOT visible to
//! the per-file `io-scan` matcher (a documented trade — `zzop_engine::io`'s module doc). Nest
//! decorator provides remain per-file, so they are what exercises the io-scan seam; the composed
//! router path has its own e2e coverage (`analyze_routes_hono.rs`, `analyze_multi_tree.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
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

/// A tiny inline io-scan pack — deliberately NOT one of `rules/dsl/*.json`, to prove `io-scan` rules can be
/// supplied entirely at runtime rather than only from a shipped pack file. Mirrors the shape of the
/// `endpoint-version-prefix` rule from the (test-only, not shipped) `http-conventions` fixture in
/// `zzop_core::dsl`'s own tests closely enough to prove the wiring, without depending on that fixture.
fn unversioned_provide_pack() -> RulePackDef {
    let json = r#"{
        "id": "test-io",
        "framework": "any",
        "rules": [
            {
                "id": "unversioned-provide",
                "severity": "warning",
                "message": "HTTP route is not under a versioned /api/v<N>/ prefix.",
                "matcher": {
                    "type": "io-scan",
                    "file_pattern": ".*",
                    "direction": "provides",
                    "kind": "http",
                    "key_pattern": "^(?:GET|POST|PUT|PATCH|DELETE) /api/v[0-9]+/",
                    "negate": true
                }
            }
        ]
    }"#;
    serde_json::from_str(json).expect("parse inline test-io pack")
}

fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-io-fixture");
    dir.write(
        "routes/apiRoutes.ts",
        "@Controller('authen')\nclass AuthenController {\n  @Get('getUserInfo')\n  getUserInfo() {}\n}\n",
    );
    dir.write(
        "src/api/client.ts",
        "export function loadOrders() { return fetch(\"/api/v1/orders\"); }\n",
    );
    dir
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        packs: vec![unversioned_provide_pack()],
        ..EngineConfig::default()
    }
}

#[test]
fn io_scan_rule_fires_on_the_unversioned_be_route_end_to_end() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "test-io/unversioned-provide");
    assert!(
        hit.is_some(),
        "expected an unversioned-provide finding, got: {:?}",
        out.findings
    );
    let hit = hit.unwrap();
    assert_eq!(hit.file, "routes/apiRoutes.ts");
    assert_eq!(hit.line, 3);
}

#[test]
fn versioned_fe_consume_does_not_trigger_the_provides_only_rule() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());
    assert!(!out
        .findings
        .iter()
        .any(|f| f.file == "src/api/client.ts" && f.rule_id == "test-io/unversioned-provide"));
}

#[test]
fn tree_wide_common_ir_merges_provides_and_consumes_across_files() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());
    let io = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected merged IoFacts on the tree-wide CommonIr");

    let provide = io
        .provides
        .iter()
        .find(|p| p.file == "routes/apiRoutes.ts")
        .expect("expected the BE route's provide to be merged into the tree IR");
    assert_eq!(provide.key, "GET /authen/getUserInfo");
    assert_eq!(provide.kind, "http");
    assert_eq!(provide.symbol.as_deref(), Some("getUserInfo"));

    let consume = io
        .consumes
        .iter()
        .find(|c| c.file == "src/api/client.ts")
        .expect("expected the FE fetch's consume to be merged into the tree IR");
    assert_eq!(consume.key.as_deref(), Some("GET /api/v1/orders"));
    assert_eq!(consume.kind, "http");
}

#[test]
fn two_runs_over_the_same_tree_produce_identical_merged_io() {
    let dir = fixture_tree();
    let out1 = analyze_tree(dir.path(), &config());
    let out2 = analyze_tree(dir.path(), &config());
    assert_eq!(
        serde_json::to_value(&out1.ir.ir.io).unwrap(),
        serde_json::to_value(&out2.ir.ir.io).unwrap()
    );
}
