//! End-to-end test for the cross-layer multi-tree API with a REAL Java backend: a TS/FE tree with an
//! `axios.get` call and a Java/Spring BE tree with a matching `@GetMapping` route, analyzed via
//! `zpz_engine::analyze_trees`, joined by `zpz_core::link_cross_layer_io` into a `cross_source: true` edge.
//! Mirrors `analyze_multi_tree.rs`'s Hono-BE shape exactly, but on the Java side — this is the FE↔Java-BE
//! join the java-provides-extraction task exists to unblock (before it, every Java BE tree extracted zero
//! `IoProvide`s, so this pair always joined 0). Matches `zpz_core::io`'s own `link_cross_layer_io` unit
//! test fixture's route/path shape.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_trees, EngineConfig};

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
    let dir = TempDir::new("zpz-engine-multi-fe-java");
    dir.write(
        "src/Ctx.tsx",
        "export function load() { return axios.get(\"/authen/getUserInfo\"); }\n",
    );
    dir
}

fn java_be_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-multi-be-java");
    dir.write(
        "src/main/java/apps/controllers/SessionController.java",
        concat!(
            "@RequestMapping(\"/authen\")\n",
            "@RestController\n",
            "public class SessionController {\n",
            "    @GetMapping(\"/getUserInfo\")\n",
            "    public UserInfo getUserInfo() {\n        return null;\n    }\n",
            "}\n",
        ),
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
fn fe_axios_call_joins_to_java_spring_get_mapping_route_across_trees() {
    let fe = fe_tree();
    let be = java_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-java")),
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
    assert_eq!(edge.to.source, "be-java");
    assert_eq!(
        edge.to.file,
        "src/main/java/apps/controllers/SessionController.java"
    );
    assert_eq!(edge.to.symbol.as_deref(), Some("getUserInfo"));
    assert!(edge.cross_source, "FE and Java BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}
