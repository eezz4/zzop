//! End-to-end coverage for the `zzop-parser-python-3` crate wired into the fused engine pipeline
//! (`crates/engine/src/pipeline.rs`'s `Language::Python` arm) and the whole-graph assembly
//! (`analyze::assemble`'s `merge_python_dep_edges` + the Python branch of the router-mount compose
//! closure). Mirrors the `TempDir`-harness style of `analyze_native_middleware.rs`/
//! `analyze_attribute_injection.rs`/`analyze_multi_tree_java.rs` — self-contained, no shared test helper
//! crate (each `tests/*.rs` file is its own separate test binary).
//!
//! Coverage:
//! - **The money shot**: a TS FE tree (`fetch('/api/users')`) and a Python FastAPI BE tree, split across
//!   TWO files (`app/main.py` mounts a router imported from `app/routers.py` via
//!   `app.include_router(router, prefix="/api")`) — pins cross-file mount composition through the Python
//!   import resolver (`resolve_python_import` in `analyze/mod.rs`), driven end to end via `analyze_trees`
//!   and asserted on `MultiAnalyzeOutput::cross_layer.edges` (the same surface
//!   `analyze_multi_tree_java.rs` asserts on for its own FE↔Java-BE join).
//! - A Python dep-graph edge (`from .helpers import x`) clears `dead-candidates` on its target file —
//!   the same cheapest-reliable-assertion style `analyze_workspace_alias.rs` uses for its own
//!   cross-package dep-graph edge.
//! - A syntactically broken `.py` file degrades (no crash), with `loc` still counted lexically.
//! - `test_*.py` classification (`zzop_core::is_test_file`) is honored by `duplicate-route`: a route
//!   defined identically in a production file and a `test_*.py` file must not double-count as a
//!   duplicate, mirroring `zzop_rules_http::duplicate_route_findings`'s own test-path skip.
//! - `.py` files are never `dead-candidates` eligible (F1), even across a real dep-graph edge, while a
//!   `.ts` sibling still fires — Python's module loading is filename-convention-driven, not import-graph
//!   driven.
//! - A first-party absolute-dotted Python import (`app.services`, resolves in-tree) is excluded from
//!   `AnalyzeOutput::package_imports` (F5); a genuinely external one (`fastapi`) still enters it.

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

// --- The money shot: cross-file FastAPI mount x TS FE fetch, joined across two trees -------------------

fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-py-cross-fe");
    dir.write(
        "src/api.ts",
        "export function loadUsers() { return fetch(\"/api/users\"); }\n",
    );
    dir
}

fn python_be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-py-cross-be");
    // Two files: the router itself (routers.py) and the mounting app (main.py) — the cross-file half of
    // this test. `app.include_router(router, prefix="/api")` names `router` via `from .routers import
    // router`; the engine's Python resolver must resolve `./routers` (relative to `app/main.py`) to
    // `app/routers.py` for this mount to compose.
    dir.write(
        "app/routers.py",
        concat!(
            "from fastapi import APIRouter\n",
            "\n",
            "router = APIRouter()\n",
            "\n",
            "@router.get(\"/users\")\n",
            "def list_users():\n",
            "    return []\n",
        ),
    );
    dir.write(
        "app/main.py",
        concat!(
            "from fastapi import FastAPI\n",
            "from .routers import router\n",
            "\n",
            "app = FastAPI()\n",
            "app.include_router(router, prefix=\"/api\")\n",
        ),
    );
    dir
}

#[test]
fn fe_fetch_call_joins_to_a_cross_file_fastapi_include_router_mount_across_trees() {
    let fe = fe_tree();
    let be = python_be_tree();
    let trees = vec![
        (fe.path().to_path_buf(), config("fe")),
        (be.path().to_path_buf(), config("be-python")),
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
    assert_eq!(edge.to.source, "be-python");
    // The VERB registration's own file (routers.py), not the mount site (main.py) — same "leaf file, not
    // the mount site" anchoring convention `compose_router_mount_provides` documents.
    assert_eq!(edge.to.file, "app/routers.py");
    assert_eq!(edge.to.symbol.as_deref(), Some("list_users"));
    assert!(edge.cross_source, "FE and Python BE are different sources");

    assert!(out.cross_layer.unprovided_consumes.is_empty());
    assert!(out.cross_layer.unconsumed_provides.is_empty());
    assert!(out.cross_layer.unresolved_consumes.is_empty());
}

// --- Python dep-graph edge: `from .helpers import x` --------------------------------------------------

#[test]
fn relative_from_import_produces_a_dep_graph_edge_clearing_dead_candidates() {
    let dir = TempDir::new("zzop-engine-py-dep-edge");
    dir.write("main.py", "from .helpers import greet\n\ngreet()\n");
    dir.write("helpers.py", "def greet():\n    return \"hi\"\n");
    let out = analyze_tree(dir.path(), &config("py-dep-edge"));

    // The real substance of this test — `merge_python_dep_edges` resolved the relative import into a
    // real dep-graph edge `main.py -> helpers.py` (not just "no finding fired", which F1 below now makes
    // true for every `.py` file regardless of whether this edge exists).
    let main_targets = out.ir.ir.dep.get("main.py");
    assert!(
        main_targets.is_some_and(|targets| targets.iter().any(|t| t == "helpers.py")),
        "expected a main.py -> helpers.py dep-graph edge, got: {main_targets:?}"
    );

    // `.py`/`.pyi` are excluded from `dead-candidates` eligibility entirely (F1: Python's module loading
    // is substantially filename-convention-driven — `main.py`, `manage.py`, `settings.py`,
    // `conftest.py`, migrations, `test_*.py`, ... are never imported — so fan_in == 0 on a `.py` file is
    // never "no importers" evidence). `helpers.py` is never flagged, edge or no edge.
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "helpers.py"),
        "helpers.py should not be dead-candidates (never eligible per F1), got: {:?}",
        out.findings
    );

    // Regression control: dead-candidates must still fire for a genuinely-unimported NON-Python file in
    // the same tree — proves the assertion above isn't vacuously true because the rule never fires at
    // all. A same-tree `orphan.py` is no longer a meaningful control here (F1 excludes every `.py` file
    // from eligibility regardless of import status), so this uses a `.ts` sibling instead.
    dir.write("orphan.ts", "export const unused = 1;\n");
    let out2 = analyze_tree(dir.path(), &config("py-dep-edge-2"));
    assert!(
        out2.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "orphan.ts"),
        "orphan.ts (never imported) should still be flagged dead-candidates, got: {:?}",
        out2.findings
    );
}

#[test]
fn unimported_python_file_is_never_flagged_dead_candidates() {
    // F1 pin, at the e2e layer: a genuinely never-imported `.py` file (no dep-graph edge points at it at
    // all) still must not be flagged — Python's filename-convention loading (`main.py`, `manage.py`,
    // `wsgi.py`, `settings.py`, `conftest.py`, migrations, `test_*.py`, ...) means fan_in == 0 is not
    // evidence of dead code for this language, so eligibility is excluded up front, not just exempted
    // via an entry-pattern allowlist.
    let dir = TempDir::new("zzop-engine-py-never-flagged");
    dir.write("app/orphan_helper.py", "def unused():\n    pass\n");
    let out = analyze_tree(dir.path(), &config("py-never-flagged"));
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file == "app/orphan_helper.py"),
        "a Python file must never be flagged dead-candidates (F1), got: {:?}",
        out.findings
    );
}

// --- Broken .py syntax error: degraded, no crash, loc still counted -----------------------------------

#[test]
fn syntactically_broken_python_file_degrades_without_panicking_and_still_counts_loc() {
    let dir = TempDir::new("zzop-engine-py-broken");
    dir.write("bad.py", "def f(:\n    pass\n");
    let out = analyze_tree(dir.path(), &config("py-broken"));

    assert_eq!(out.degraded, vec!["bad.py".to_string()]);
    // Lexical fallback `loc` convention (`count_loc`/`lexical_loc`): a trailing newline adds 1, mirroring
    // `content.split("\n").length` — "def f(:\n    pass\n" splits into 3 elements.
    assert_eq!(*out.ir.ir.loc.get("bad.py").unwrap(), 3);
    // A degraded file still gets an (empty) dep-graph node, same convention as a degraded TS file.
    assert!(out.ir.ir.dep.contains_key("bad.py"));
}

// --- test_*.py classification: duplicate-route skips test-path providers -------------------------------

#[test]
fn duplicate_route_skips_a_provider_defined_only_in_a_test_prefixed_python_file() {
    let dir = TempDir::new("zzop-engine-py-test-classification");
    dir.write(
        "app.py",
        concat!(
            "from fastapi import FastAPI\n",
            "app = FastAPI()\n",
            "\n",
            "@app.get(\"/widgets\")\n",
            "def list_widgets():\n",
            "    return []\n",
        ),
    );
    // `test_app.py` — `zzop_core::is_test_file`'s Python convention (`test_foo.py`/`foo_test.py`) — hosts
    // an identically-keyed route. Without the test-path skip this would double-count as a duplicate.
    dir.write(
        "test_app.py",
        concat!(
            "from fastapi import FastAPI\n",
            "app = FastAPI()\n",
            "\n",
            "@app.get(\"/widgets\")\n",
            "def list_widgets_fixture():\n",
            "    return []\n",
        ),
    );
    let out = analyze_tree(dir.path(), &config("py-test-classification"));

    assert!(
        !out.findings.iter().any(|f| f.rule_id == "duplicate-route"),
        "a route re-declared only inside a test_*.py fixture must not count as a duplicate, got: {:?}",
        out.findings
    );

    // Both provides are still extracted (the skip is `duplicate-route`-scoped, not extraction-scoped).
    let http_provides: Vec<_> = out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| io.provides.iter().filter(|p| p.kind == "http").collect())
        .unwrap_or_default();
    assert_eq!(http_provides.len(), 2, "got: {http_provides:?}");
}

// --- F5: first-party dotted Python imports don't pollute the package-import census ----------------------

#[test]
fn in_tree_dotted_python_import_is_not_censused_as_a_package_import() {
    // `app.services` is a first-party absolute-dotted specifier that resolves in-tree (`app/services.py`
    // exists) — it must NOT show up in `package_imports` (that census exists for genuinely external
    // package-import tripwires, e.g. `framework_silence`'s S2/S4). `fastapi` never resolves in-tree, so it
    // still enters the census exactly as before.
    let dir = TempDir::new("zzop-engine-py-package-census");
    dir.write(
        "app/main.py",
        concat!(
            "from fastapi import FastAPI\n",
            "from app.services import get_users\n",
            "\n",
            "app = FastAPI()\n",
        ),
    );
    dir.write("app/services.py", "def get_users():\n    return []\n");
    let out = analyze_tree(dir.path(), &config("py-package-census"));

    let specifiers: Vec<&str> = out
        .package_imports
        .iter()
        .map(|p| p.specifier.as_str())
        .collect();
    assert!(
        !specifiers.contains(&"app.services"),
        "in-tree specifier app.services must not be censused as a package import, got: {specifiers:?}"
    );
    assert!(
        specifiers.contains(&"fastapi"),
        "genuinely external specifier fastapi must still be censused, got: {specifiers:?}"
    );
}
