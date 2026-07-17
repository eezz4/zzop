//! Pin for D11: `analyze_trees`' cross-layer JOIN input must exclude test-classified io facts
//! (`zzop_core::is_test_file`), matching what the "classified-skip" disclosure (`disclosure.rs`) claims.
//! Before this fix, `trees.rs`'s `source_ios` was built straight from each tree's raw `output.ir.ir.io`
//! with no test-file filter, so e.g. a Go `_test.go` route registration became an ordinary production
//! "provide" that could join a real cross-tree edge. Mirrors the `TempDir`-harness style of
//! `analyze_go_cross_layer.rs`.
//!
//! Coverage:
//! - A gin route registered ONLY in a `_test.go` file does not appear in `cross_layer.edges` and its
//!   matching FE consume instead lands in `cross_layer.unprovided_consumes` (nothing provides it anymore,
//!   once the join input dropped it).
//! - A sibling production gin route (same file layout, no test suffix) still joins normally.
//! - The BE tree's own `AnalyzeOutput::warnings` carries exactly one join-input-drop warning naming the
//!   dropped provide count and the `_test.go` file path.
//! - The raw per-file facts are untouched: the BE tree's own `ir.ir.io.provides` still lists BOTH routes
//!   (the test-classified one included) — only the cross-layer JOIN input was narrowed, not `output.ir`.

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

fn config(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-testio-fe");
    dir.write(
        "src/api.ts",
        concat!(
            "export function loadWidgets() { return fetch(\"/api/widgets\"); }\n",
            "export function loadGadgets() { return fetch(\"/api/gadgets\"); }\n",
        ),
    );
    dir
}

/// A gin route registered ONLY in `handler_test.go` (`/api/widgets`) alongside a sibling production route
/// in plain `handler.go` (`/api/gadgets`) — the money-shot pair for the join-input filter.
fn go_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-testio-be");
    dir.write(
        "handler_test.go",
        concat!(
            "package main\n",
            "\n",
            "import \"github.com/gin-gonic/gin\"\n",
            "\n",
            "func listWidgets(c *gin.Context) {}\n",
            "\n",
            "func setupTestRoutes() {\n",
            "\tr := gin.Default()\n",
            "\tr.GET(\"/api/widgets\", listWidgets)\n",
            "}\n",
        ),
    );
    dir.write(
        "handler.go",
        concat!(
            "package main\n",
            "\n",
            "import \"github.com/gin-gonic/gin\"\n",
            "\n",
            "func listGadgets(c *gin.Context) {}\n",
            "\n",
            "func setupRoutes() {\n",
            "\tr := gin.Default()\n",
            "\tr.GET(\"/api/gadgets\", listGadgets)\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn test_classified_provide_is_dropped_from_the_join_while_a_sibling_production_provide_still_joins()
{
    let fe = fe_tree();
    let be = go_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-go")),
    ];
    let out = analyze_trees(&trees);

    // The production route joins normally.
    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected exactly one cross-layer http edge (the production route only), got: {:?}",
        out.cross_layer.edges
    );
    assert_eq!(http_edges[0].key, "GET /api/gadgets");
    assert_eq!(http_edges[0].to.file, "handler.go");

    // The test-classified route never joins — no edge keyed GET /api/widgets anywhere.
    assert!(
        !out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /api/widgets"),
        "the _test.go-only route must never appear in cross_layer.edges, got: {:?}",
        out.cross_layer.edges
    );
    // It also never appears among unconsumed_provides (dropped before the join, not merely unconsumed).
    assert!(
        !out.cross_layer
            .unconsumed_provides
            .iter()
            .any(|p| p.provide.key == "GET /api/widgets"),
        "the _test.go-only route must never appear in unconsumed_provides either, got: {:?}",
        out.cross_layer.unconsumed_provides
    );
    // Its matching FE consume is now unprovided (nothing provides that key post-filter).
    assert!(
        out.cross_layer
            .unprovided_consumes
            .iter()
            .any(|c| c.consume.key.as_deref() == Some("GET /api/widgets")),
        "the FE consume for the dropped test route must land in unprovided_consumes, got: {:?}",
        out.cross_layer.unprovided_consumes
    );

    // The BE tree's own warnings self-report exactly one join-input-drop, naming the count and the file.
    let be_output = &out
        .trees
        .iter()
        .find(|(_, source, _)| source == "be-go")
        .expect("be-go tree present")
        .2;
    let drop_warnings: Vec<&String> = be_output
        .warnings
        .iter()
        .filter(|w| w.contains("test-classified provide"))
        .collect();
    assert_eq!(
        drop_warnings.len(),
        1,
        "expected exactly one join-input-drop warning, got: {:?}",
        be_output.warnings
    );
    assert!(
        drop_warnings[0].contains("1 test-classified provide(s)"),
        "{}",
        drop_warnings[0]
    );
    assert!(
        drop_warnings[0].contains("0 test-classified consume(s)"),
        "{}",
        drop_warnings[0]
    );
    assert!(
        drop_warnings[0].contains("handler_test.go"),
        "{}",
        drop_warnings[0]
    );
    assert!(drop_warnings[0].contains("ir.io"), "{}", drop_warnings[0]);

    // Raw per-file facts are untouched: both routes still show up in the tree's own ir.ir.io.provides.
    let raw_provides: Vec<&str> = be_output
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| io.provides.iter().map(|p| p.key.as_str()).collect())
        .unwrap_or_default();
    assert!(
        raw_provides.contains(&"GET /api/widgets"),
        "raw per-file facts must still include the test-classified route, got: {raw_provides:?}"
    );
    assert!(
        raw_provides.contains(&"GET /api/gadgets"),
        "raw per-file facts must still include the production route, got: {raw_provides:?}"
    );
}
