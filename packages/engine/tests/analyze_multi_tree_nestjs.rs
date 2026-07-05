//! End-to-end test for the cross-layer multi-tree API with a NestJS-style TS backend: a TS/FE tree with a
//! `fetch` call and a TS/BE tree with a matching `@Controller`/`@Get` decorated route, analyzed via
//! `zpz_engine::analyze_trees`, joined by `zpz_core::link_cross_layer_io` into a `cross_source: true` edge.
//! Mirrors `analyze_multi_tree_java.rs`'s Spring-BE shape, but on the NestJS side — this is the FE↔NestJS-BE
//! join `zpz_parser_typescript::nest::extract_nest_provides` exists to unblock (before it, a NestJS BE tree
//! extracted zero `IoProvide`s for any decorator-routed controller).

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
    let dir = TempDir::new("zpz-engine-multi-fe-nest");
    dir.write(
        "src/api.ts",
        "export function loadUser(id: string) { return fetch(`/api/users/${id}`); }\n",
    );
    dir
}

fn nest_be_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-multi-be-nest");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get, Param } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get(':id')\n",
            "  findOne(@Param('id') id: string) {\n    return id;\n  }\n",
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
fn fe_fetch_call_joins_to_nestjs_controller_route_across_trees() {
    let fe = fe_tree();
    let be = nest_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-nest")),
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
    assert_eq!(edge.key, "GET /api/users/{}");
    assert_eq!(edge.from.source, "fe");
    assert_eq!(edge.from.file, "src/api.ts");
    assert_eq!(edge.to.source, "be-nest");
    assert_eq!(edge.to.file, "src/users/users.controller.ts");
    assert_eq!(edge.to.symbol.as_deref(), Some("findOne"));
    assert!(edge.cross_source, "FE and NestJS BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}
