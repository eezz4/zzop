//! End-to-end coverage for the `zzop-parser-csharp` crate wired into the fused engine pipeline
//! (`crates/engine/src/pipeline/fresh.rs`'s `Language::CSharp` arm). Mirrors the `TempDir`-harness style
//! of `analyze_go_cross_layer.rs` — self-contained, no shared test helper crate.
//!
//! Coverage:
//! - **The money shot**: a TS FE tree (`` fetch(`/api/users/${id}`) ``) and a C# ASP.NET Core BE tree
//!   (`[ApiController] [Route("api/[controller]")] class UsersController { [HttpGet("{id}")] ... Get }`)
//!   -> exactly one cross-source `http` edge keyed `GET /api/users/{}` — C#'s route-param placeholder
//!   (`{id}`) and the TS template-literal interpolation (`${id}`) both normalize to the SAME `{}` slot
//!   (this engine's shared route-param convention — see `zzop_parser_typescript`'s
//!   `adapters::egress::keying` doc and `zzop_parser_csharp`'s own
//!   `attribute_controller_composes_class_and_method_route` pinned test, which already asserts
//!   `"GET /api/users/{}"` for this exact attribute shape), which is what lets the two sides join at all.
//! - Minimal API provides: `app.MapGet("/health", ...)` -> `GET /health`; grouped
//!   `app.MapGroup("/api").MapGet("/ping", ...)` -> `GET /api/ping`.
//! - `HttpClient` egress consume: `using System.Net.Http;` + `httpClient.GetAsync("/api/orders")` ->
//!   `GET /api/orders`.
//! - Dispatch: a `.cs` file is dispatched to `Language::CSharp` and parses (non-empty symbols, not
//!   degraded).

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

// --- The money shot: attribute-routed controller x TS FE template-literal fetch -------------------------

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-csharp-cross-fe");
    dir.write(
        "src/api.ts",
        "export function loadUser(id: string) { return fetch(`/api/users/${id}`); }\n",
    );
    dir
}

fn csharp_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-csharp-cross-be");
    dir.write(
        "Controllers/UsersController.cs",
        concat!(
            "[ApiController]\n",
            "[Route(\"api/[controller]\")]\n",
            "public class UsersController {\n",
            "    [HttpGet(\"{id}\")]\n",
            "    public string Get(int id) { return \"\"; }\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn fe_fetch_call_joins_to_a_csharp_attribute_controller_route_across_trees() {
    let fe = fe_tree();
    let be = csharp_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-csharp")),
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
    assert_eq!(
        edge.key, "GET /api/users/{}",
        "route-param placeholder must normalize to the shared `{{}}` slot on both sides"
    );
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/api.ts");
    assert_eq!(edge.to.source, "be-csharp");
    assert_eq!(edge.to.file, "Controllers/UsersController.cs");
    assert_eq!(edge.to.symbol.as_deref(), Some("Get"));
    assert!(edge.cross_source, "FE and C# BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
}

// --- Minimal API provides: MapGet + MapGroup-prefixed MapGet --------------------------------------------

#[test]
fn minimal_api_map_get_and_grouped_map_get_are_extracted_as_provides() {
    let dir = TempDir::new("zzop-engine-csharp-minimal-api");
    dir.write(
        "Program.cs",
        concat!(
            "var app = builder.Build();\n",
            "app.MapGet(\"/health\", () => \"ok\");\n",
            "app.MapGroup(\"/api\").MapGet(\"/ping\", () => \"pong\");\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config("minimal-api"));
    let provides: Vec<_> = out
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
        provides.iter().any(|p| p.key == "GET /health"),
        "expected a GET /health provide, got: {:?}",
        provides
    );
    assert!(
        provides.iter().any(|p| p.key == "GET /api/ping"),
        "expected a GET /api/ping provide (MapGroup prefix composed), got: {:?}",
        provides
    );
}

// --- HttpClient egress consume ---------------------------------------------------------------------------

#[test]
fn http_client_get_async_is_extracted_as_a_consume() {
    let dir = TempDir::new("zzop-engine-csharp-httpclient");
    dir.write(
        "Services/OrderClient.cs",
        concat!(
            "using System.Net.Http;\n",
            "class OrderClient {\n",
            "    async void Fetch() {\n",
            "        var client = new HttpClient();\n",
            "        var r = client.GetAsync(\"/api/orders\");\n",
            "    }\n",
            "}\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config("httpclient"));
    let consumes: Vec<_> = out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.consumes
                .iter()
                .filter(|c| c.kind == "http")
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    assert!(
        consumes
            .iter()
            .any(|c| c.key.as_deref() == Some("GET /api/orders")),
        "expected a GET /api/orders egress consume, got: {:?}",
        consumes
    );
}

// --- Dispatch: a .cs file is dispatched to Language::CSharp and parses ---------------------------------

#[test]
fn cs_file_dispatches_to_csharp_and_parses_with_non_empty_symbols() {
    let dir = TempDir::new("zzop-engine-csharp-dispatch");
    dir.write(
        "Program.cs",
        "public class Greeter { public void Hello() {} }\n",
    );

    let out = analyze_tree(dir.path(), &config("dispatch-cs"));
    assert!(
        !out.degraded.contains(&"Program.cs".to_string()),
        "expected Program.cs to parse cleanly (not degraded), got degraded: {:?}",
        out.degraded
    );
    let symbols: Vec<_> = out
        .ir
        .ir
        .symbols
        .iter()
        .filter(|s| s.file == "Program.cs")
        .collect();
    assert!(
        !symbols.is_empty(),
        "expected non-empty symbols for a well-formed .cs file"
    );
    assert!(symbols.iter().any(|s| s.name == "Greeter"));
    assert!(symbols.iter().any(|s| s.name == "Greeter.Hello"));
}

// --- Whole-corpus route-constant resolution (`run_csharp_provides_project_pass`) -----------------------

#[test]
fn cross_file_route_constant_resolves_through_the_engine_project_pass() {
    // `[HttpGet(Routes.List)]` references a `const string List` declared in a SEPARATE `static class Routes`
    // file — dropped by the per-file pass (no corpus), RESOLVED by the whole-corpus project pass wired into
    // `assemble`. The controller's class prefix + resolved method path compose to `GET /api/users/list`.
    let dir = TempDir::new("zzop-engine-csharp-route-const");
    dir.write(
        "Routing/Routes.cs",
        "namespace App.Routing { public static class Routes { public const string List = \"/list\"; } }\n",
    );
    dir.write(
        "Controllers/UsersController.cs",
        concat!(
            "[ApiController]\n",
            "[Route(\"api/[controller]\")]\n",
            "public class UsersController {\n",
            "    [HttpGet(App.Routing.Routes.List)]\n",
            "    public string List() { return \"\"; }\n",
            "    [HttpPost(\"create\")]\n",
            "    public string Create() { return \"\"; }\n",
            "}\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config("route-const"));
    let keys: Vec<String> = out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .map(|p| p.key.clone())
                .collect()
        })
        .unwrap_or_default();

    assert!(
        keys.iter().any(|k| k == "GET /api/users/list"),
        "cross-file route constant must resolve through the engine project pass, got: {keys:?}"
    );
    assert!(
        keys.iter().any(|k| k == "POST /api/users/create"),
        "the sibling literal route must survive too, got: {keys:?}"
    );
}

// --- The db-table half: EF Core provides x a Rust service tree's raw-SQL consumes ----------------------

/// Both EF Core provide shapes in one tree: a `DbSet<T>` convention name (`Users` -> `table:users`) and
/// a `[Table("legacy_orders")]` attribute rename. Joined by the same raw-SQL Rust consumer
/// `analyze_rust_cross_layer.rs`'s db half uses — this is the arm that gave `.cs` a db channel at all
/// (2026-08-02, `zzop_parser_csharp::adapters::ef_core` wired through `io::extract_csharp_file_io`).
#[test]
fn ef_core_provides_join_a_rust_trees_raw_sql_db_table_consumes() {
    let ef = TempDir::new("zzop-engine-csharp-db-ef");
    ef.write(
        "Data/AppDbContext.cs",
        concat!(
            "using Microsoft.EntityFrameworkCore;\n",
            "public class AppDbContext : DbContext {\n",
            "    public DbSet<User> Users { get; set; }\n",
            "}\n",
            "public class User { public long Id { get; set; } }\n",
        ),
    );
    ef.write(
        "Models/Order.cs",
        concat!(
            "using System.ComponentModel.DataAnnotations.Schema;\n",
            "[Table(\"legacy_orders\")]\n",
            "public class Order { public long Id { get; set; } }\n",
        ),
    );
    let svc = TempDir::new("zzop-engine-csharp-db-rust-svc");
    svc.write(
        "src/db.rs",
        concat!(
            "pub async fn load_user(pool: &sqlx::PgPool, id: i64) {\n",
            "    sqlx::query!(\"SELECT id FROM users WHERE id = $1\", id);\n",
            "}\n",
            "\n",
            "pub async fn load_orders(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM legacy_orders\", &[]).await.ok();\n",
            "}\n",
        ),
    );

    let trees = vec![
        (ef.path().to_path_buf(), config("svc-csharp")),
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
    assert_eq!(db_edges[0].key, "table:legacy_orders");
    assert_eq!(db_edges[0].to.file, "Models/Order.cs");
    assert_eq!(db_edges[0].to.symbol.as_deref(), Some("Order"));
    assert_eq!(db_edges[1].key, "table:users");
    assert_eq!(db_edges[1].to.file, "Data/AppDbContext.cs");
    assert_eq!(db_edges[1].to.symbol.as_deref(), Some("User"));
    for e in &db_edges {
        assert_eq!(e.from.source, "svc-rust", "the Rust file is the CONSUMER");
        assert_eq!(
            e.to.source, "svc-csharp",
            "the EF Core side is the PROVIDER"
        );
        assert!(e.cross_source);
    }
}

/// Invalidation guard for the C# arm of `zzop_core::is_test_file` (2026-08-03): a conventional C#
/// test project's DbSet fixture (`Api.Tests/UserServiceTests.cs` — `.Tests/` directory AND `*Tests.cs`
/// file name) must NOT reach the cross-layer join. Before the arm existed this fixture's
/// `table:fixtures` provide joined the Rust consume as ordinary production surface; the production
/// context's `table:users` edge pins that the filter is not vacuous.
#[test]
fn a_csharp_test_projects_dbset_fixture_is_filtered_from_the_cross_layer_join() {
    let ef = TempDir::new("zzop-engine-csharp-db-testproj-ef");
    ef.write(
        "Data/AppDbContext.cs",
        concat!(
            "using Microsoft.EntityFrameworkCore;\n",
            "public class AppDbContext : DbContext {\n",
            "    public DbSet<User> Users { get; set; }\n",
            "}\n",
            "public class User { public long Id { get; set; } }\n",
        ),
    );
    ef.write(
        "Api.Tests/UserServiceTests.cs",
        concat!(
            "using Microsoft.EntityFrameworkCore;\n",
            "public class FixtureDbContext : DbContext {\n",
            "    public DbSet<Fixture> Fixtures { get; set; }\n",
            "}\n",
            "public class Fixture { public long Id { get; set; } }\n",
        ),
    );
    let svc = TempDir::new("zzop-engine-csharp-db-testproj-rust");
    svc.write(
        "src/db.rs",
        concat!(
            "pub async fn load_user(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM users\", &[]).await.ok();\n",
            "}\n",
            "\n",
            "pub async fn load_fixtures(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM fixtures\", &[]).await.ok();\n",
            "}\n",
        ),
    );

    let trees = vec![
        (ef.path().to_path_buf(), config("svc-csharp-testproj")),
        (svc.path().to_path_buf(), config("svc-rust-testproj")),
    ];
    let out = analyze_trees(&trees);

    let db_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "db-table")
        .collect();
    assert!(
        db_edges.iter().any(|e| e.key == "table:users"),
        "the production DbSet must still join (filter must not be vacuous), got: {:?}",
        out.cross_layer.edges
    );
    assert!(
        db_edges.iter().all(|e| e.key != "table:fixtures"),
        "a `.Tests/` project's DbSet fixture must never join a cross-layer edge, got: {:?}",
        out.cross_layer.edges
    );
    assert!(
        db_edges
            .iter()
            .all(|e| !e.to.file.starts_with("Api.Tests/")),
        "no edge may anchor inside the test project, got: {:?}",
        out.cross_layer.edges
    );
}

/// Negative twin: a non-literal `[Table]` rename emits NOTHING (and suppresses the DbSet convention
/// name in the same file), so the Rust consume stays honestly unjoined instead of joining a table the
/// mapping renamed away.
#[test]
fn a_non_literal_table_rename_produces_no_provide_and_no_edge() {
    let ef = TempDir::new("zzop-engine-csharp-db-neg-ef");
    ef.write(
        "Data/AppDbContext.cs",
        concat!(
            "using Microsoft.EntityFrameworkCore;\n",
            "using System.ComponentModel.DataAnnotations.Schema;\n",
            "public class AppDbContext : DbContext {\n",
            "    public DbSet<Order> Orders { get; set; }\n",
            "}\n",
            "[Table(TableNames.Orders)]\n",
            "public class Order { public long Id { get; set; } }\n",
        ),
    );
    let svc = TempDir::new("zzop-engine-csharp-db-neg-rust");
    svc.write(
        "src/db.rs",
        concat!(
            "pub async fn load_orders(client: &tokio_postgres::Client) {\n",
            "    client.query(\"SELECT id FROM orders\", &[]).await.ok();\n",
            "}\n",
        ),
    );

    let trees = vec![
        (ef.path().to_path_buf(), config("svc-csharp-neg")),
        (svc.path().to_path_buf(), config("svc-rust-neg")),
    ];
    let out = analyze_trees(&trees);
    assert!(
        out.cross_layer.edges.iter().all(|e| e.kind != "db-table"),
        "a renamed-but-unreadable table must never join under its convention name: {:?}",
        out.cross_layer.edges
    );

    // Direct confirmation on the EF tree alone: zero db-table provides at all.
    let solo = analyze_tree(ef.path(), &config("svc-csharp-neg-solo"));
    let provides = solo
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| io.provides.iter().filter(|p| p.kind == "db-table").count())
        .unwrap_or(0);
    assert_eq!(provides, 0, "no provide may survive the non-literal rename");
}
