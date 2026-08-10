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
//! - **The `db-table` half**: a `migrations/*.sql` tree's `CREATE TABLE` provides joined by a Rust
//!   service tree's raw-SQL consumes (`zzop_parser_rust::extract_rust_raw_sql_db_table_consumes`, wired
//!   into `pipeline::io_projection`'s `Language::Rust` arm). This is the channel `.rs` had NO producer
//!   for until 2026-08-02: the parser unit tests prove the facts exist, and only this file proves they
//!   reach the join — the chain that has broken before. Its negative twin pins that an interpolated
//!   table name produces no consume, so the same migration's provides stay unconsumed rather than
//!   joining to a fabricated key.

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

// --- The db-table half: a .sql migration tree x a Rust service tree's raw-SQL consumes ----------------

/// A migration tree: `CREATE TABLE` provides only, no application code at all.
fn migration_tree(prefix: &str) -> TempDir {
    let dir = TempDir::new(prefix);
    dir.write(
        "db/migrations/V1__init.sql",
        concat!(
            "CREATE TABLE users (\n",
            "  id BIGINT PRIMARY KEY,\n",
            "  email VARCHAR(255) NOT NULL\n",
            ");\n",
            "\n",
            "CREATE TABLE orders (\n",
            "  id BIGINT PRIMARY KEY,\n",
            "  user_id BIGINT NOT NULL\n",
            ");\n",
        ),
    );
    dir
}

#[test]
fn rust_raw_sql_consumes_join_a_sql_migration_trees_db_table_provides() {
    let db = migration_tree("zzop-engine-rust-db-migrations");
    let svc = TempDir::new("zzop-engine-rust-db-svc");
    // Both raw-SQL shapes in one file: the `sqlx::query!` MACRO (whose argument syn hands back as an
    // opaque token stream — the shape a literal-only walk would have missed) and an ordinary call
    // argument.
    svc.write(
        "src/db.rs",
        concat!(
            "pub async fn load_user(pool: &sqlx::PgPool, id: i64) {\n",
            "    sqlx::query!(\"SELECT id, email FROM users WHERE id = $1\", id);\n",
            "}\n",
            "\n",
            "pub async fn load_orders(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM orders\", &[]).await.ok();\n",
            "}\n",
        ),
    );

    let trees = vec![
        (db.path().to_path_buf(), config("db-migrations")),
        (svc.path().to_path_buf(), config("svc-rust")),
    ];
    let out = analyze_trees(&trees);

    let mut db_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "db-table")
        .collect();
    db_edges.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(
        db_edges.len(),
        2,
        "expected one db-table edge per touched table, got: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(db_edges[0].key, "table:orders");
    assert_eq!(db_edges[1].key, "table:users");
    for e in &db_edges {
        assert_eq!(e.from.source, "svc-rust", "the Rust file is the CONSUMER");
        assert_eq!(e.from.file, "src/db.rs");
        assert_eq!(e.to.source, "db-migrations");
        assert_eq!(e.to.file, "db/migrations/V1__init.sql");
        assert!(
            e.cross_source,
            "migrations and service are different sources"
        );
    }
    // The consume side is keyed by the parser itself (the statement names the physical table), so
    // nothing is left for the engine's ORM entity resolver to fill in.
    assert!(
        out.cross_layer.unresolved_consumes.is_empty(),
        "raw-SQL consumes are keyed at extraction time: {:?}",
        out.cross_layer.unresolved_consumes
    );
}

#[test]
fn an_interpolated_table_name_produces_no_consume_and_no_edge() {
    let db = migration_tree("zzop-engine-rust-db-neg-migrations");
    let svc = TempDir::new("zzop-engine-rust-db-neg-svc");
    // Three things that must all stay silent: a fully interpolated table, a prefix+hole table (the
    // shape that would key a non-existent `table:users_`), and English prose shaped like SQL.
    svc.write(
        "src/db.rs",
        concat!(
            "pub fn dynamic(table: &str) -> String {\n",
            "    format!(\"SELECT * FROM {table}\")\n",
            "}\n",
            "\n",
            "pub fn sharded(n: u8) -> String {\n",
            "    format!(\"SELECT * FROM users_{n}\")\n",
            "}\n",
            "\n",
            "pub const HELP: &str = \"Select a date from the list\";\n",
        ),
    );

    let trees = vec![
        (db.path().to_path_buf(), config("db-neg")),
        (svc.path().to_path_buf(), config("svc-rust-neg")),
    ];
    let out = analyze_trees(&trees);

    let db_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "db-table")
        .collect();
    assert!(
        db_edges.is_empty(),
        "a non-literal table name must never join: {db_edges:?}"
    );

    // Direct confirmation on the service tree alone: zero db-table consumes at all, not merely zero
    // edges — an edge count can be zero because the PROVIDE side was missing.
    let solo = analyze_tree(svc.path(), &config("svc-rust-neg-solo"));
    let db_consumes: Vec<_> = solo
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.consumes
                .iter()
                .filter(|c| c.kind == "db-table")
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        db_consumes.is_empty(),
        "expected zero db-table consumes for interpolated/prose strings, got: {db_consumes:?}"
    );

    // ... and the migration's provides are still there, unconsumed — which is what makes the zero above
    // a real silence rather than a tree with nothing in it.
    assert!(
        out.cross_layer
            .unconsumed_provides
            .iter()
            .any(|p| p.provide.key == "table:users"),
        "the CREATE TABLE provides must survive as unconsumed: {:?}",
        out.cross_layer.unconsumed_provides
    );
}
