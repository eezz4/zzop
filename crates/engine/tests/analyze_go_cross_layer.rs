//! End-to-end coverage for the `zzop-parser-go` crate wired into the fused engine pipeline
//! (`crates/engine/src/pipeline/fresh.rs`'s `Language::Go` arm) and the whole-graph assembly's
//! router-mount composition (`analyze::compose_router_mount_provides` over
//! `zzop_parser_go::extract_go_router_fragments`'s combined gin + `net/http` fragments). Mirrors the
//! `TempDir`-harness style of `analyze_rust_cross_layer.rs`/`analyze_python_cross_layer.rs` —
//! self-contained, no shared test helper crate.
//!
//! Coverage:
//! - **The money shot**: a TS FE tree (`fetch("/api/users")`) and a Go gin BE tree
//!   (`api := r.Group("/api"); api.GET("/users", h)`) -> exactly one cross-source `http` edge keyed
//!   `GET /api/users`.
//! - A `net/http` `http.HandleFunc("GET /health", h)` (Go 1.22 method-in-pattern syntax) route joining
//!   an FE `fetch("/health")` -> a second, independent cross-source edge keyed `GET /health`.
//! - Negative: a non-literal `net/http` pattern (a local variable, not a string literal) never becomes
//!   an `http` provide at all — `adapters::net_http`'s "non-literal pattern skips the WHOLE call"
//!   contract — so no cross-layer edge can form for it either.

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

// --- The money shot: gin route group x TS FE fetch, plus a net/http Go-1.22-pattern route -------------

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-go-cross-fe");
    dir.write(
        "src/api.ts",
        concat!(
            "export function loadUsers() { return fetch(\"/api/users\"); }\n",
            "export function loadHealth() { return fetch(\"/health\"); }\n",
        ),
    );
    dir
}

fn go_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-go-cross-be");
    dir.write(
        "main.go",
        concat!(
            "package main\n",
            "\n",
            "import (\n",
            "\t\"net/http\"\n",
            "\n",
            "\t\"github.com/gin-gonic/gin\"\n",
            ")\n",
            "\n",
            "func listUsers(c *gin.Context) {}\n",
            "\n",
            "func healthCheck(w http.ResponseWriter, r *http.Request) {}\n",
            "\n",
            "func setup() {\n",
            "\tr := gin.Default()\n",
            "\tapi := r.Group(\"/api\")\n",
            "\tapi.GET(\"/users\", listUsers)\n",
            "\n",
            "\thttp.HandleFunc(\"GET /health\", healthCheck)\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn fe_fetch_call_joins_to_a_gin_route_group_across_trees() {
    let fe = fe_tree();
    let be = go_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-go")),
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
        2,
        "expected exactly two cross-layer http edges (gin + net/http), got: {:?}",
        out.cross_layer.edges
    );

    let gin_edges: Vec<_> = http_edges
        .iter()
        .filter(|e| e.key == "GET /api/users")
        .collect();
    assert_eq!(
        gin_edges.len(),
        1,
        "expected exactly one cross-source edge for GET /api/users, got: {:?}",
        http_edges
    );
    let gin_edge = gin_edges[0];
    assert_eq!(gin_edge.from.source, "fe");
    assert_eq!(gin_edge.from.file, "src/api.ts");
    assert_eq!(gin_edge.to.source, "be-go");
    assert_eq!(gin_edge.to.file, "main.go");
    assert_eq!(gin_edge.to.symbol.as_deref(), Some("listUsers"));
    assert!(gin_edge.cross_source, "FE and Go BE are different sources");

    let net_http_edges: Vec<_> = http_edges
        .iter()
        .filter(|e| e.key == "GET /health")
        .collect();
    assert_eq!(
        net_http_edges.len(),
        1,
        "expected exactly one cross-source edge for GET /health, got: {:?}",
        http_edges
    );
    let net_http_edge = net_http_edges[0];
    assert_eq!(net_http_edge.from.source, "fe");
    assert_eq!(net_http_edge.to.source, "be-go");
    assert_eq!(net_http_edge.to.symbol.as_deref(), Some("healthCheck"));

    assert!(out.cross_layer.unprovided_consumes.is_empty());
}

// --- Negative: a non-literal net/http pattern never becomes a provide, so no cross-layer edge forms -----

#[test]
fn non_literal_net_http_pattern_produces_no_http_provide_or_cross_layer_edge() {
    let fe_dir = TempDir::new("zzop-engine-go-negative-fe");
    fe_dir.write(
        "src/api.ts",
        "export function loadItems() { return fetch(\"/items\"); }\n",
    );
    let be_dir = TempDir::new("zzop-engine-go-negative-be");
    be_dir.write(
        "main.go",
        concat!(
            "package main\n",
            "\n",
            "import \"net/http\"\n",
            "\n",
            "func listItems(w http.ResponseWriter, r *http.Request) {}\n",
            "\n",
            "func setup() {\n",
            "\tpath := \"/items\"\n",
            "\thttp.HandleFunc(path, listItems)\n",
            "}\n",
        ),
    );

    let trees = vec![
        (fe_dir.path().to_path_buf(), config("fe-neg")),
        (be_dir.path().to_path_buf(), config("be-go-neg")),
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
        "a non-literal net/http pattern must never become an http provide, got: {:?}",
        http_edges
    );

    // Direct confirmation on the BE tree alone: zero http provides at all, not merely zero edges.
    let be_out = analyze_tree(be_dir.path(), &config("be-go-neg-solo"));
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
        "expected zero http provides for the non-literal-pattern file, got: {:?}",
        http_provides
    );
}
