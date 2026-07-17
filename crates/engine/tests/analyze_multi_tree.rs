//! End-to-end test for the cross-layer multi-tree API: an FE tree with a `fetch` call and a BE tree with a
//! matching Hono route, analyzed via `zzop_engine::analyze_trees`, joined by `zzop_core::link_cross_layer_io`
//! into a `cross_source: true` edge. Mirrors the FE/BE fixture shapes used by `zzop_core::io`'s own
//! `link_cross_layer_io` unit tests and `zzop_parser_typescript::egress`/`routes`'s end-to-end tests.
//!
//! `overlay_injected_consume_joins_a_native_provide_across_trees` additionally locks the Mode-B injection
//! path's cross-tree behaviour — an adapter-overlay-supplied `IoConsume` on one tree joining another tree's
//! NATIVE provide into a `cross_source` edge (the openapi-sdk-adapter pipeline, immich's "web SDK consumes
//! 0 -> 349" lift in miniature). The single-tree overlay tests in `analyze_adapter_overlay.rs` never cross
//! a tree boundary, so this is the one slice of that shipped behaviour they leave unguarded.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{FileProjection, IoConsume, IoFacts, NormalizedEnvelope, NORMALIZED_AST_FORMAT};
use zzop_engine::{analyze_trees, EngineConfig, MIN_PARALLEL_IMPL_SIGNALS};

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

/// A minimal, all-empty `FileProjection` (same defaults `analyze_adapter_overlay.rs`'s own `projection`
/// helper uses — test files in this crate don't share a utils module). A test fills only the fields it
/// cares about.
fn projection(path: &str, loc: u32) -> FileProjection {
    FileProjection {
        class_shape_fragments: Vec::new(),
        path: path.to_string(),
        loc,
        symbols: Vec::new(),
        imports: zzop_core::ImportMap::new(),
        re_exports: Vec::new(),
        dynamic_imports: Vec::new(),
        used_names: Vec::new(),
        const_map_fragment: HashMap::new(),
        procedure_router_fragments: Vec::new(),
        router_mount_fragments: Vec::new(),
        io: IoFacts::default(),
        degraded: false,
        is_entry: false,
        attributes: Vec::new(),
        loop_spans: Vec::new(),
    }
}

fn overlay(parser: &str, files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: 1,
        parser: parser.to_string(),
        source: "adapter".to_string(),
        files,
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

/// The Mode-B injection path across a tree boundary: the FE talks to its backend only through a generated
/// SDK function (`getUserInfo()` is not `fetch`/`axios`/`ky`, so native egress keys NOTHING), an adapter
/// overlay projects that call site as a keyed `IoConsume` (what `openapi-sdk-adapter.mjs` emits from the
/// spec's `operationId -> "METHOD /path"`), and `analyze_trees` joins it to the BE's native Hono provide.
/// Asserts BOTH directions — the miniature of immich's "0 -> 349 cross-layer edges" lift — so a future
/// change that drops overlay consumes from the cross-tree join can't regress silently.
#[test]
fn overlay_injected_consume_joins_a_native_provide_across_trees() {
    let fe = TempDir::new("zzop-engine-multi-fe-sdk");
    fe.write(
        "src/page.ts",
        "import { getUserInfo } from '@my/sdk';\nexport function load() { return getUserInfo(); }\n",
    );
    let be = be_tree(); // native Hono `GET /authen/getUserInfo`

    // Baseline: with no overlay the SDK call is invisible to the native egress scan, so there is NO
    // cross-source http edge — the "0" the overlay lifts.
    let baseline = analyze_trees(&[
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ]);
    assert!(
        baseline.cross_layer.edges.iter().all(|e| e.kind != "http"),
        "native analysis alone must not see the SDK call: {:?}",
        baseline.cross_layer.edges
    );

    // Overlay: project the SDK call site as a keyed http consume, exactly as the reference adapter would.
    let mut sdk_call = projection("src/page.ts", 2);
    sdk_call.io.consumes.push(IoConsume {
        client: None,
        body: None,
        kind: "http".to_string(),
        key: Some("GET /authen/getUserInfo".to_string()),
        file: "src/page.ts".to_string(),
        line: 2,
        raw: None,
        method: None,
    });
    let mut fe_cfg = config("fe");
    fe_cfg.adapter_overlays = vec![overlay("openapi-sdk-adapter/1", vec![sdk_call])];

    let out = analyze_trees(&[
        (fe.path().to_path_buf(), fe_cfg),
        (be.path().to_path_buf(), config("be")),
    ]);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "the overlay-injected consume must join the BE's native provide: {:?}",
        out.cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.key, "GET /authen/getUserInfo");
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/page.ts");
    assert_eq!(edge.to.source, "be");
    assert_eq!(edge.to.file, "routes/apiRoutes.ts");
    assert!(
        edge.cross_source,
        "the overlay's FE consume and the native BE provide are different sources"
    );

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
}

// --- parallel-implementation tripwire (blind field test: trees:"auto" wiring 5 competing frontend
// reimplementations + 2 backends of the same API into one join produced 0 cross-source edges and 86
// pure duplicate-route/ambiguous-consume findings, presented with no run-level context) --------------

fn backend_with_routes(prefix: &str, handler_prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write(
        "routes/apiRoutes.ts",
        &format!(
            "const apiRoutes = new Hono();\n\
             apiRoutes.get(\"/api/a\", {handler_prefix}.a);\n\
             apiRoutes.get(\"/api/b\", {handler_prefix}.b);\n\
             apiRoutes.get(\"/api/c\", {handler_prefix}.c);\n\
             apiRoutes.get(\"/api/d\", {handler_prefix}.d);\n\
             apiRoutes.get(\"/api/e\", {handler_prefix}.e);\n"
        ),
    );
    dir
}

#[test]
fn parallel_implementation_tripwire_fires_on_zero_edges_plus_enough_duplicate_routes() {
    // Two trees registering the SAME 5 routes, consumed by nobody: 5 `cross-layer/duplicate-route`
    // findings (one per shared key), 0 cross-source edges (nothing ever consumes any of them) — exactly
    // the "parallel reimplementations of one API" shape, at the `MIN_PARALLEL_IMPL_SIGNALS` threshold.
    let svc_a = backend_with_routes("zzop-engine-parallel-impl-a", "api");
    let svc_b = backend_with_routes("zzop-engine-parallel-impl-b", "api2");
    let out = analyze_trees(&[
        (svc_a.path().to_path_buf(), config("svc-a")),
        (svc_b.path().to_path_buf(), config("svc-b")),
    ]);

    assert_eq!(
        out.cross_layer
            .edges
            .iter()
            .filter(|e| e.cross_source)
            .count(),
        0,
        "fixture must have zero cross-source edges: {:?}",
        out.cross_layer.edges
    );
    let duplicate_route_count = out
        .cross_layer_findings
        .iter()
        .filter(|f| f.rule_id == "cross-layer/duplicate-route")
        .count();
    assert_eq!(duplicate_route_count, MIN_PARALLEL_IMPL_SIGNALS);

    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("0 cross-source edges")
                && w.contains("parallel implementations of the same API surface")),
        "expected the parallel-implementation tripwire to fire, got: {:?}",
        out.warnings
    );
}

#[test]
fn parallel_implementation_tripwire_is_silent_on_a_healthy_joined_run() {
    // A real FE/BE pair with a clean cross-source edge must never trip the tripwire, even though (as
    // it happens) this fixture also has zero duplicate/ambiguous findings — the "healthy joined"
    // control case.
    let fe = fe_tree();
    let be = be_tree();
    let out = analyze_trees(&[
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be")),
    ]);
    assert!(
        out.cross_layer.edges.iter().any(|e| e.cross_source),
        "sanity: fixture must have a real cross-source edge"
    );
    assert!(
        out.warnings.is_empty(),
        "a healthy joined run must not trip the parallel-implementation tripwire, got: {:?}",
        out.warnings
    );
}

#[test]
fn parallel_implementation_tripwire_is_silent_when_edges_are_zero_but_no_duplicate_signals_exist() {
    // The "blind tree" control case: 0 cross-source edges (nobody consumes anything), but the two
    // trees' routes don't even overlap — so 0 duplicate-route/ambiguous-consume findings either. This
    // must stay silent: 0 edges alone is not the signal, only 0 edges PLUS a pile of duplicate/
    // ambiguity noise is.
    let svc_a = TempDir::new("zzop-engine-blind-a");
    svc_a.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/only-a\", api.a);\n",
    );
    let svc_b = TempDir::new("zzop-engine-blind-b");
    svc_b.write(
        "routes/apiRoutes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/api/only-b\", api.b);\n",
    );
    let out = analyze_trees(&[
        (svc_a.path().to_path_buf(), config("svc-a")),
        (svc_b.path().to_path_buf(), config("svc-b")),
    ]);
    assert_eq!(
        out.cross_layer
            .edges
            .iter()
            .filter(|e| e.cross_source)
            .count(),
        0
    );
    assert!(out
        .cross_layer_findings
        .iter()
        .all(|f| f.rule_id != "cross-layer/duplicate-route"
            && f.rule_id != "cross-layer/ambiguous-consume"));
    assert!(
        out.warnings.is_empty(),
        "0 edges with no duplicate/ambiguous signal must not trip the tripwire, got: {:?}",
        out.warnings
    );
}
