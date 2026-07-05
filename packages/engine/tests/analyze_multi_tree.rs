//! End-to-end test for the cross-layer multi-tree API: an FE tree with a `fetch` call and a BE tree with a
//! matching Hono route, analyzed via `zzop_engine::analyze_trees`, joined by `zzop_core::link_cross_layer_io`
//! into a `cross_source: true` edge. Mirrors the FE/BE fixture shapes used by `zzop_core::io`'s own
//! `link_cross_layer_io` unit tests and `zzop_parser_typescript::egress`/`routes`'s end-to-end tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-multi-fe");
    dir.write(
        "src/Ctx.tsx",
        "export function load() { return fetch(\"/authen/getUserInfo\"); }\n",
    );
    dir
}

fn be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-multi-be");
    dir.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/authen/getUserInfo\", api.getUserInfo);\n",
    );
    dir
}

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

#[test]
fn fe_fetch_joins_to_be_hono_route_across_trees() {
    let fe = fe_tree();
    let be = be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    assert_eq!(out.trees.len(), 2);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected exactly one cross-layer http edge, got: {:?}",
        out.cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.key, "GET /authen/getUserInfo");
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/Ctx.tsx");
    assert_eq!(edge.to.source, "be");
    assert_eq!(edge.to.file, "routes/apiRoutes.ts");
    assert_eq!(edge.to.symbol.as_deref(), Some("api.getUserInfo"));
    assert!(edge.cross_source, "FE and BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}

#[test]
fn per_tree_outputs_are_still_returned_alongside_the_join() {
    let fe = fe_tree();
    let be = be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ];
    let out = analyze_trees(&trees);

    let (_, fe_source_id, fe_output) = &out.trees[0];
    assert_eq!(fe_source_id, "fe");
    assert_eq!(fe_output.file_count, 1);

    let (_, be_source_id, be_output) = &out.trees[1];
    assert_eq!(be_source_id, "be");
    assert_eq!(be_output.file_count, 1);
}

#[test]
fn a_tree_with_no_io_projection_contributes_nothing_but_does_not_panic() {
    let empty = TempDir::new("zzop-engine-multi-empty");
    empty.write("plain.ts", "export const x = 1;\n");
    let trees = vec![(empty.path().to_path_buf(), config("empty"))];
    let out = analyze_trees(&trees);
    assert_eq!(out.trees.len(), 1);
    assert!(out.cross_layer.edges.is_empty());
}

/// End-to-end for the three join-integrity gates (the cross-layer join-integrity task), all through the
/// real `analyze_trees` pipeline (real TS parsing, real `link_cross_layer_io` call
/// site with the engine's injected `zzop_metrics::default_generic_interface_key_patterns()` table):
/// - `/health` is provided by TWO backend trees -> ambiguous, no edge.
/// - `/ping` is provided by ONE backend tree AND matches the default low-confidence table -> edge with
///   `lowConfidenceReason` set.
/// - an absolute-URL fetch -> `external`, never joined even though nothing internal provides it either.
#[test]
fn analyze_trees_surfaces_ambiguous_external_and_low_confidence_buckets() {
    let fe = TempDir::new("zzop-engine-multi-fe-gates");
    fe.write(
        "src/Ctx.tsx",
        "export function a() { return fetch(\"/health\"); }\n\
         export function b() { return fetch(\"/ping\"); }\n\
         export function c() { return fetch(\"https://vendor.com/api/users\"); }\n",
    );
    let be1 = TempDir::new("zzop-engine-multi-be1-gates");
    be1.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\n\
         apiRoutes.get(\"/health\", api.health);\n\
         apiRoutes.get(\"/ping\", api.ping);\n",
    );
    let be2 = TempDir::new("zzop-engine-multi-be2-gates");
    be2.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/health\", api.health2);\n",
    );

    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be1.path().to_path_buf(), config("be1")),
        (be2.path().to_path_buf(), config("be2")),
    ];
    let out = analyze_trees(&trees);

    // /health: ambiguous (two distinct provider trees), no edge for it.
    assert!(
        out.cross_layer.edges.iter().all(|e| e.key != "GET /health"),
        "ambiguous key must not produce an edge: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(out.cross_layer.ambiguous_consumes.len(), 1);
    assert_eq!(out.cross_layer.ambiguous_consumes[0].source, "fe");
    assert_eq!(
        out.cross_layer.ambiguous_consumes[0].consume.key.as_deref(),
        Some("GET /health")
    );
    assert_eq!(out.cross_layer.ambiguous_consumes[0].candidates.len(), 2);

    // /ping: single-provider edge, matches the default low-confidence table.
    let ping_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.key == "GET /ping")
        .collect();
    assert_eq!(ping_edges.len(), 1, "{:?}", out.cross_layer.edges);
    assert!(
        ping_edges[0].low_confidence_reason.is_some(),
        "expected /ping to match the default generic-path table"
    );

    // absolute-URL fetch: external, never joined.
    assert_eq!(out.cross_layer.external_consumes.len(), 1);
    assert_eq!(
        out.cross_layer.external_consumes[0].consume.key.as_deref(),
        Some("GET https://vendor.com/api/users")
    );
    assert!(out.cross_layer.unprovided_consumes.is_empty());
}
