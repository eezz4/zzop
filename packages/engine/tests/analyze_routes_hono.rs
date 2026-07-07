//! e2e coverage for structural Hono-router recognition — since the `+router-mounts-v1` batch this
//! flows through `zzop_parser_typescript::router_mounts::extract_router_mount_fragments` →
//! `analyze::compose_router_mount_provides` (originally `routes::extract_api_routes`, whose
//! per-file mapping the engine no longer uses). A BE app can register routes on an identifier
//! (`app`, a `Hono`-typed function parameter, a locally constructed router instance, ...) that is
//! never in the configured `router_names` allowlist (default: just `["apiRoutes"]`); `IoProvide`s
//! must still be extracted for it, or `duplicate-route` silently sees 0 findings not because the
//! routes are genuinely non-duplicated, but because they were never extracted at all.
//!
//! This file exercises the native `duplicate-route` analysis
//! (`zzop_engine::pipeline::duplicate_route_findings`, registered in
//! `zzop_rules_http::register_native_analyses`) end-to-end against exactly that registration style,
//! with NO `EngineConfig::io.router_names` override — proving the fix works on the default config.
//! `duplicate-route` is native, not a DSL rule, so its findings carry the plain rule id
//! `"duplicate-route"` (no pack prefix) and require no `packs` to be loaded (same convention as
//! `tests/pack_fullstack.rs`'s own `duplicate-route` coverage).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `pack_sql.rs`/
/// `pack_fullstack.rs`).
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
        source_id: "routes-hono-fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn duplicate_route_hits(out: &AnalyzeOutput) -> Vec<&zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "duplicate-route")
        .collect()
}

#[test]
fn same_route_on_two_differently_named_local_hono_instances_is_flagged() {
    // Neither `healthRoutes` nor `monitorRoutes` is in the default `router_names` allowlist
    // (`["apiRoutes"]`) — both are recognized purely from their own `= new Hono()` construction.
    let dir = TempDir::new("zzop-routes-hono");
    dir.write(
        "src/health/healthRoutes.ts",
        "import { Hono } from \"hono\";\nexport const healthRoutes = new Hono();\nhealthRoutes.get(\"/api/health\", (c) => c.json({ status: \"ok\" }));\n",
    );
    dir.write(
        "src/monitor/monitorRoutes.ts",
        "import { Hono } from \"hono\";\nexport const monitorRoutes = new Hono();\nmonitorRoutes.get(\"/api/health\", (c) => c.json({ status: \"up\" }));\n",
    );
    let out = scan(&dir);

    // Provides extraction itself worked (not just the duplicate-route finding) — both files'
    // `GET /api/health` should show up in the assembled tree-wide `IoFacts`.
    let provides = &out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected aggregated io facts")
        .provides;
    let health_provides: Vec<_> = provides
        .iter()
        .filter(|p| p.key == "GET /api/health")
        .collect();
    assert_eq!(health_provides.len(), 2, "{:?}", provides);

    let hits = duplicate_route_hits(&out);
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
    assert_eq!(hits[0].rule_id, "duplicate-route");
}

#[test]
fn same_route_registered_via_a_hono_typed_parameter_is_flagged() {
    // The router is never constructed with `new Hono()` in this file at all — it arrives as a
    // function parameter explicitly typed `Hono`, and routes are registered on it directly inside
    // the function body.
    let dir = TempDir::new("zzop-routes-hono");
    dir.write(
        "src/routes/registerA.ts",
        "import type { Hono } from \"hono\";\nexport function registerA(app: Hono): void {\n  app.post(\"/api/auth/register\", handlers.register);\n}\n",
    );
    dir.write(
        "src/routes/registerB.ts",
        "import type { Hono } from \"hono\";\nexport function registerB(app: Hono): void {\n  app.post(\"/api/auth/register\", handlers.registerAgain);\n}\n",
    );
    let out = scan(&dir);
    let hits = duplicate_route_hits(&out);
    assert_eq!(hits.len(), 1, "{:?}", out.findings);
}

#[test]
fn distinct_routes_on_local_hono_instances_are_not_flagged() {
    let dir = TempDir::new("zzop-routes-hono");
    dir.write(
        "src/health/healthRoutes.ts",
        "import { Hono } from \"hono\";\nexport const healthRoutes = new Hono();\nhealthRoutes.get(\"/api/health\", (c) => c.json({ status: \"ok\" }));\n",
    );
    dir.write(
        "src/monitor/monitorRoutes.ts",
        "import { Hono } from \"hono\";\nexport const monitorRoutes = new Hono();\nmonitorRoutes.get(\"/api/status\", (c) => c.json({ status: \"up\" }));\n",
    );
    let out = scan(&dir);

    let provides = &out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected aggregated io facts")
        .provides;
    assert_eq!(provides.len(), 2, "{:?}", provides);

    assert!(duplicate_route_hits(&out).is_empty(), "{:?}", out.findings);
}
