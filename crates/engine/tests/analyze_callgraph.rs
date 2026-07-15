//! End-to-end test for the call-graph-BFS native rules (`rule-pack-porting.md`'s "http GRAPH-classified" backlog):
//! `zzop-rules-http`'s `scan_unsafe_read_endpoint` / `scan_non_idempotent_write`, wired into
//! `zzop_engine::analyze::assemble` via the documented v1 "second pass" (`analyze.rs::run_callgraph_rules`'s
//! doc — re-reads TS file text off disk to extract `RawCall`s, since `FileArtifact` does not carry them).
//! Exercises the whole path: a Hono-style route file's per-file `IoProvide` -> reconstructed `ApiEndpoint` ->
//! handler-symbol resolution -> BFS over the whole-repo `SymbolGraph` -> a reachable write site -> a
//! `Finding` merged into `AnalyzeOutput::findings`.

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

/// - `routes/apiRoutes.ts`: a GET route (`/rates` -> `getRates`, reaches a write two hops away) and a PUT
///   route (`/things/:id` -> `putThing`, writes directly with a non-idempotent `create`).
/// - `routes/handlers.ts`: `getRates` -> `refresh` (same-file call edge) -> `prisma.rate.update(...)`;
///   `putThing` -> `prisma.thing.create(...)` directly.
fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-callgraph-fixture");
    dir.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/rates\", getRates);\napiRoutes.put(\"/things/:id\", putThing);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function getRates(c) { return refresh(c.env); }\nexport function refresh(env) { return prisma.rate.update({ where: { id: 1 }, data: { value: 2 } }); }\nexport function putThing(c) { return prisma.thing.create({ data: { id: c.id } }); }\n",
    );
    dir
}

#[test]
fn get_endpoint_reaching_a_write_two_hops_away_is_flagged_unsafe_read_endpoint() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "unsafe-read-endpoint")
        .unwrap_or_else(|| {
            panic!(
                "expected an unsafe-read-endpoint finding, got: {:?}",
                out.findings
            )
        });
    let data = hit.data.as_ref().unwrap();
    assert_eq!(data["method"], "GET");
    assert_eq!(data["path"], "/rates");
    assert_eq!(data["sink"], "prisma.rate.update");
    assert_eq!(data["depth"], 1);
    assert_eq!(hit.file, "routes/handlers.ts");
}

#[test]
fn put_endpoint_creating_directly_is_flagged_non_idempotent_write() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let hit = out
        .findings
        .iter()
        .find(|f| f.rule_id == "non-idempotent-write")
        .unwrap_or_else(|| {
            panic!(
                "expected a non-idempotent-write finding, got: {:?}",
                out.findings
            )
        });
    let data = hit.data.as_ref().unwrap();
    assert_eq!(data["method"], "PUT");
    assert_eq!(data["path"], "/things/{}");
    assert_eq!(data["kind"], "create");
    assert_eq!(data["depth"], 0);
}

#[test]
fn disabling_unsafe_read_endpoint_removes_only_that_finding() {
    let dir = fixture_tree();
    let mut config = EngineConfig::default();
    config
        .rule_config
        .disabled_rules
        .push("unsafe-read-endpoint".to_string());
    let out = analyze_tree(dir.path(), &config);
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id == "unsafe-read-endpoint"));
    assert!(out
        .findings
        .iter()
        .any(|f| f.rule_id == "non-idempotent-write"));
}

#[test]
fn a_tree_with_no_api_endpoints_produces_neither_finding() {
    let dir = TempDir::new("zzop-engine-callgraph-no-routes");
    dir.write(
        "lib/util.ts",
        "export function helper() { return prisma.user.create({ data: {} }); }\n",
    );
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(!out
        .findings
        .iter()
        .any(|f| f.rule_id == "unsafe-read-endpoint" || f.rule_id == "non-idempotent-write"));
}
