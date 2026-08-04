//! End-to-end coverage for the CONSUME half `zzop-parser-java-21` gained on 2026-08-02
//! (`http_clients` — RestTemplate/WebClient egress) and its JPA `db-table` provide arm (`jpa`), wired
//! through `crates/engine/src/io.rs::extract_java_file_io` into `pipeline::io_projection`'s
//! `Language::Java21` arm. Mirrors the `TempDir`-harness style of `analyze_rust_cross_layer.rs` —
//! self-contained, no shared test helper crate.
//!
//! Coverage:
//! - **The money shot**: a Java SERVICE tree that CALLS another Java Spring service
//!   (`restTemplate.getForObject("/api/users", …)` + `webClient.get().uri("/api/orders")`) joined
//!   against the sibling tree's Spring MVC provides — the exact half of the join Java lacked: it
//!   emitted routes while its outbound calls were invisible. The parser unit tests prove the consume
//!   facts exist; only this file proves they reach the join — the chain that has broken before.
//! - **The `db-table` half**: JPA `@Entity`/`@Table` provides from a Java tree joined by a Rust
//!   service tree's raw-SQL consumes (the same consumer `analyze_rust_cross_layer.rs`'s own db half
//!   uses), covering both the `@Table(name = "…")` literal and the snake-case class-name default.
//! - Negative twins: a variable URL stays `key: None` (witnessed, never guessed — it must NOT join),
//!   and the same egress/entity code on a `src/test/java/**` path projects NOTHING at all
//!   (`zzop_core::is_test_file` path gate, judged sufficient for Java — no inline test idiom exists).

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

// --- The money shot: Java egress consumes x a sibling Spring tree's provides ---------------------------

/// The CALLING service: one RestTemplate consume and one WebClient consume, both literal.
fn java_caller_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-java-cross-caller");
    dir.write(
        "src/main/java/com/acme/gateway/UserGateway.java",
        concat!(
            "package com.acme.gateway;\n",
            "\n",
            "import org.springframework.web.client.RestTemplate;\n",
            "\n",
            "public class UserGateway {\n",
            "    private final RestTemplate restTemplate = new RestTemplate();\n",
            "\n",
            "    public String loadUsers() {\n",
            "        return restTemplate.getForObject(\"/api/users\", String.class);\n",
            "    }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/main/java/com/acme/gateway/OrderGateway.java",
        concat!(
            "package com.acme.gateway;\n",
            "\n",
            "import org.springframework.web.reactive.function.client.WebClient;\n",
            "\n",
            "public class OrderGateway {\n",
            "    private final WebClient client = WebClient.create();\n",
            "\n",
            "    public void loadOrders() {\n",
            "        client.get().uri(\"/api/orders\").retrieve();\n",
            "    }\n",
            "}\n",
        ),
    );
    dir
}

/// The CALLED service: a Spring controller providing both routes the caller names.
fn java_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-java-cross-be");
    dir.write(
        "src/main/java/com/acme/api/ApiController.java",
        concat!(
            "package com.acme.api;\n",
            "\n",
            "import org.springframework.web.bind.annotation.GetMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n",
            "\n",
            "@RestController\n",
            "@RequestMapping(\"/api\")\n",
            "public class ApiController {\n",
            "    @GetMapping(\"/users\")\n",
            "    public String users() { return \"[]\"; }\n",
            "\n",
            "    @GetMapping(\"/orders\")\n",
            "    public String orders() { return \"[]\"; }\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn java_egress_consumes_join_a_sibling_spring_trees_provides_across_trees() {
    let caller = java_caller_tree();
    let be = java_be_tree();
    let trees = vec![
        (caller.path().to_path_buf(), config("svc-gateway")),
        (be.path().to_path_buf(), config("svc-api")),
    ];
    let out = analyze_trees(&trees);

    let mut http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    http_edges.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(
        http_edges.len(),
        2,
        "expected one edge per egress call site, got: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(http_edges[0].key, "GET /api/orders");
    assert_eq!(
        http_edges[0].from.file, "src/main/java/com/acme/gateway/OrderGateway.java",
        "the WebClient call site is the consumer"
    );
    assert_eq!(http_edges[1].key, "GET /api/users");
    assert_eq!(
        http_edges[1].from.file, "src/main/java/com/acme/gateway/UserGateway.java",
        "the RestTemplate call site is the consumer"
    );
    for e in &http_edges {
        assert_eq!(e.from.source, "svc-gateway");
        assert_eq!(e.to.source, "svc-api");
        assert_eq!(e.to.file, "src/main/java/com/acme/api/ApiController.java");
        assert!(e.cross_source, "caller and callee are different sources");
    }
    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}

// --- Negative: a variable URL is witnessed but never keyed, so it can never join ----------------------

#[test]
fn a_variable_url_stays_unresolved_and_forms_no_edge() {
    let caller = TempDir::new("zzop-engine-java-neg-caller");
    caller.write(
        "src/main/java/com/acme/DynGateway.java",
        concat!(
            "package com.acme;\n",
            "\n",
            "import org.springframework.web.client.RestTemplate;\n",
            "\n",
            "public class DynGateway {\n",
            "    public String load(RestTemplate rt, String url) {\n",
            "        return rt.getForObject(url, String.class);\n",
            "    }\n",
            "}\n",
        ),
    );
    let be = java_be_tree();
    let trees = vec![
        (caller.path().to_path_buf(), config("svc-dyn")),
        (be.path().to_path_buf(), config("svc-api-neg")),
    ];
    let out = analyze_trees(&trees);
    assert!(
        out.cross_layer.edges.iter().all(|e| e.kind != "http"),
        "a variable URL must never fabricate an http join: {:?}",
        out.cross_layer.edges
    );

    // Direct confirmation: the consume EXISTS (witnessed) with `key: None` (never guessed).
    let solo = analyze_tree(caller.path(), &config("svc-dyn-solo"));
    let io = solo.ir.ir.io.as_ref().expect("io facts present");
    let consumes: Vec<_> = io.consumes.iter().filter(|c| c.kind == "http").collect();
    assert_eq!(consumes.len(), 1, "the call site must be witnessed");
    assert_eq!(consumes[0].key, None);
    assert_eq!(consumes[0].raw.as_deref(), Some("url"));
    assert_eq!(consumes[0].method.as_deref(), Some("GET"));
}

// --- Negative: the test source root is silent on BOTH new channels ------------------------------------

#[test]
fn test_source_root_paths_project_no_egress_and_no_entities() {
    let dir = TempDir::new("zzop-engine-java-test-root");
    dir.write(
        "src/test/java/com/acme/GatewayIT.java",
        concat!(
            "package com.acme;\n",
            "\n",
            "import org.springframework.web.client.RestTemplate;\n",
            "\n",
            "public class GatewayIT {\n",
            "    void hit() { new RestTemplate().getForObject(\"/api/users\", String.class); }\n",
            "}\n",
        ),
    );
    dir.write(
        "src/test/java/com/acme/FixtureRow.java",
        concat!(
            "package com.acme;\n",
            "\n",
            "import jakarta.persistence.Entity;\n",
            "\n",
            "@Entity\n",
            "public class FixtureRow { long id; }\n",
        ),
    );
    let out = analyze_tree(dir.path(), &config("svc-test-only"));
    let (consumes, provides) = out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            (
                io.consumes.iter().filter(|c| c.kind == "http").count(),
                io.provides.iter().filter(|p| p.kind == "db-table").count(),
            )
        })
        .unwrap_or((0, 0));
    assert_eq!(consumes, 0, "test-path egress must be silent");
    assert_eq!(provides, 0, "test-path entities must be silent");
}

// --- The db-table half: JPA entity provides x a Rust service tree's raw-SQL consumes ------------------

#[test]
fn jpa_entity_provides_join_a_rust_trees_raw_sql_db_table_consumes() {
    let jpa = TempDir::new("zzop-engine-java-db-entities");
    jpa.write(
        "src/main/java/com/acme/model/UserAccount.java",
        concat!(
            "package com.acme.model;\n",
            "\n",
            "import jakarta.persistence.Entity;\n",
            "import jakarta.persistence.Table;\n",
            "\n",
            "@Entity\n",
            "@Table(name = \"users\")\n",
            "public class UserAccount { long id; }\n",
        ),
    );
    jpa.write(
        "src/main/java/com/acme/model/OrderItem.java",
        concat!(
            "package com.acme.model;\n",
            "\n",
            "import jakarta.persistence.Entity;\n",
            "\n",
            "@Entity\n",
            "public class OrderItem { long id; }\n",
        ),
    );
    let svc = TempDir::new("zzop-engine-java-db-rust-svc");
    // The same raw-SQL consumer shapes analyze_rust_cross_layer.rs's own db half pins: the statement
    // names the physical table, keyed at extraction time — `users` hits the @Table literal, and
    // `order_item` hits the snake-cased class-name default.
    svc.write(
        "src/db.rs",
        concat!(
            "pub async fn load_user(pool: &sqlx::PgPool, id: i64) {\n",
            "    sqlx::query!(\"SELECT id FROM users WHERE id = $1\", id);\n",
            "}\n",
            "\n",
            "pub async fn load_items(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM order_item\", &[]).await.ok();\n",
            "}\n",
        ),
    );

    let trees = vec![
        (jpa.path().to_path_buf(), config("svc-java")),
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
    assert_eq!(db_edges[0].key, "table:order_item");
    assert_eq!(
        db_edges[0].to.file,
        "src/main/java/com/acme/model/OrderItem.java"
    );
    assert_eq!(db_edges[0].to.symbol.as_deref(), Some("OrderItem"));
    assert_eq!(db_edges[1].key, "table:users");
    assert_eq!(
        db_edges[1].to.file,
        "src/main/java/com/acme/model/UserAccount.java"
    );
    for e in &db_edges {
        assert_eq!(e.from.source, "svc-rust", "the Rust file is the CONSUMER");
        assert_eq!(e.to.source, "svc-java", "the JPA entity is the PROVIDER");
        assert!(e.cross_source);
    }
}
