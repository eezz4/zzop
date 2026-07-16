//! End-to-end coverage for the `zzop-parser-rust` crate wired into the fused engine pipeline
//! (`crates/engine/src/pipeline/fresh.rs`'s `Language::Rust` arm) and the whole-graph assembly
//! (`analyze::assemble`'s `merge_rust_dep_edges` + the Rust branch of the router-mount compose
//! closure in `analyze::assemble::provides`). Mirrors the `TempDir`-harness style of
//! `analyze_python_cross_layer.rs` — self-contained, no shared test helper crate.
//!
//! Coverage:
//! - **The money shot**: a TS FE tree (`fetch('/api/users')`) and a Rust axum BE tree, split across TWO
//!   files (`src/main.rs` mounts a router imported from `src/routes.rs` via
//!   `Router::new().nest("/api", api_router)`, `api_router` bound through `use crate::routes::
//!   api_router;`) — pins cross-file mount composition through the Rust import resolver
//!   (`resolve_rust_import`), driven end to end via `analyze_trees` and asserted on
//!   `MultiAnalyzeOutput::cross_layer.edges` (the same surface `analyze_python_cross_layer.rs` asserts
//!   on for its own FE<->Python-BE join).
//! - A non-literal `.route()` path (`Router::new().route(path, get(handler))`, `path` a local variable)
//!   never becomes an `http` provide at all — `zzop_parser_rust::adapters::axum`'s "non-literal path
//!   skips the WHOLE `.route()` call" contract — so no cross-layer edge can form for it either.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, analyze_trees, EngineConfig};

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

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

// --- The money shot: cross-file axum `.nest()` mount x TS FE fetch, joined across two trees -----------

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-rust-cross-fe");
    dir.write(
        "src/api.ts",
        "export function loadUsers() { return fetch(\"/api/users\"); }\n",
    );
    dir
}

fn rust_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-rust-cross-be");
    // Two files: the router itself (routes.rs) and the mounting app (main.rs) — the cross-file half of
    // this test. `Router::new().nest("/api", api_router)` names `api_router` via `use crate::routes::
    // api_router;`; the engine's Rust resolver must resolve `crate::routes::api_router` (relative to
    // `src/main.rs`) to `src/routes.rs` for this mount to compose.
    dir.write(
        "src/routes.rs",
        concat!(
            "use axum::{routing::get, Router};\n",
            "\n",
            "fn list_users() -> &'static str {\n",
            "    \"[]\"\n",
            "}\n",
            "\n",
            "pub fn api_router() -> Router {\n",
            "    Router::new().route(\"/users\", get(list_users))\n",
            "}\n",
        ),
    );
    dir.write(
        "src/main.rs",
        concat!(
            "use axum::Router;\n",
            "use crate::routes::api_router;\n",
            "\n",
            "fn app() -> Router {\n",
            "    let app = Router::new().nest(\"/api\", api_router);\n",
            "    app\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn fe_fetch_call_joins_to_a_cross_file_axum_nest_mount_across_trees() {
    let fe = fe_tree();
    let be = rust_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-rust")),
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
    assert_eq!(edge.key, "GET /api/users");
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/api.ts");
    assert_eq!(edge.to.source, "be-rust");
    // The VERB registration's own file (routes.rs), not the mount site (main.rs) — same "leaf file, not
    // the mount site" anchoring convention `compose_router_mount_provides` documents.
    assert_eq!(edge.to.file, "src/routes.rs");
    assert_eq!(edge.to.symbol.as_deref(), Some("list_users"));
    assert!(edge.cross_source, "FE and Rust BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}

// --- Negative: a non-literal `.route()` path never becomes a provide, so no cross-layer edge forms -----

#[test]
fn non_literal_route_path_produces_no_http_provide_or_cross_layer_edge() {
    let fe_dir = TempDir::new("zzop-engine-rust-negative-fe");
    fe_dir.write(
        "src/api.ts",
        "export function loadItems() { return fetch(\"/items\"); }\n",
    );
    let be_dir = TempDir::new("zzop-engine-rust-negative-be");
    be_dir.write(
        "src/main.rs",
        concat!(
            "use axum::{routing::get, Router};\n",
            "\n",
            "fn list_items() -> &'static str {\n",
            "    \"[]\"\n",
            "}\n",
            "\n",
            "fn dynamic_path() -> &'static str {\n",
            "    \"/items\"\n",
            "}\n",
            "\n",
            "fn app() -> Router {\n",
            "    let path = dynamic_path();\n",
            "    Router::new().route(path, get(list_items))\n",
            "}\n",
        ),
    );

    let trees = vec![
        (fe_dir.path().to_path_buf(), config("fe-neg")),
        (be_dir.path().to_path_buf(), config("be-rust-neg")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert!(
        http_edges.is_empty(),
        "a non-literal .route() path must never become an http provide, got: {:?}",
        http_edges
    );

    // Direct confirmation on the BE tree alone: zero http provides at all, not merely zero edges.
    let be_out = analyze_tree(be_dir.path(), &config("be-rust-neg-solo"));
    let http_provides: Vec<_> = be_out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        http_provides.is_empty(),
        "expected zero http provides for the non-literal-path file, got: {:?}",
        http_provides
    );
}
