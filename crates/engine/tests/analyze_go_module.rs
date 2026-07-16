//! Dogfooding-style smoke test: a mini `go.mod` module tree, driven end to end through `analyze_tree`,
//! pinning task 4/5/6's `go.mod` module resolution + package-directory-wide dep edges + census
//! discipline, alongside the `.go` dead-candidates/unreachable exemptions (rules-graph). Mirrors
//! `analyze_rust_self.rs`'s self-contained `TempDir`-harness style.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

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

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "go-module-self".to_string(),
        ..EngineConfig::default()
    }
}

/// One module (`example.com/app`), `main.go` at the module root importing: (a) `example.com/app/internal/db`
/// — a same-module package with TWO files, exercising task 5's "every file directly in the resolved
/// package dir gets an edge" rule; (b) `fmt` — Go standard library, excluded from the census by Go's own
/// dot-in-first-segment rule; (c) `github.com/some/dep` — genuinely external, must still be censused. A
/// `main_test.go` (fan_in 0, its own entry via `is_test_file`) pins the unreachable exemption.
fn module_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-go-module-self");
    dir.write("go.mod", "module example.com/app\n\ngo 1.22\n");
    dir.write(
        "main.go",
        concat!(
            "package main\n",
            "\n",
            "import (\n",
            "\t\"fmt\"\n",
            "\n",
            "\t\"example.com/app/internal/db\"\n",
            "\t\"github.com/some/dep\"\n",
            ")\n",
            "\n",
            "func main() {\n",
            "\tfmt.Println(db.Get())\n",
            "\tdep.Do()\n",
            "}\n",
        ),
    );
    dir.write(
        "main_test.go",
        concat!(
            "package main\n",
            "\n",
            "import \"testing\"\n",
            "\n",
            "func TestMain(t *testing.T) {}\n",
        ),
    );
    dir.write(
        "internal/db/db.go",
        concat!(
            "package db\n",
            "\n",
            "func Get() string {\n",
            "\treturn helper()\n",
            "}\n",
        ),
    );
    dir.write(
        "internal/db/helpers.go",
        concat!(
            "package db\n",
            "\n",
            "func helper() string {\n",
            "\treturn \"ok\"\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn import_resolves_to_edges_for_every_file_directly_in_the_package_dir() {
    let dir = module_tree();
    let out = analyze_tree(dir.path(), &config());
    let targets = out.ir.ir.dep.get("main.go");
    assert!(
        targets.is_some_and(|t| t.iter().any(|x| x == "internal/db/db.go")),
        "expected main.go -> internal/db/db.go package-dir edge, got: {targets:?}"
    );
    assert!(
        targets.is_some_and(|t| t.iter().any(|x| x == "internal/db/helpers.go")),
        "expected main.go -> internal/db/helpers.go package-dir edge (EVERY file in the package, not \
         just the one declaring `Get`), got: {targets:?}"
    );
}

#[test]
fn go_standard_library_import_is_never_censused() {
    let dir = module_tree();
    let out = analyze_tree(dir.path(), &config());
    let specifiers: Vec<&str> = out
        .package_imports
        .iter()
        .map(|p| p.specifier.as_str())
        .collect();
    assert!(
        !specifiers.contains(&"fmt"),
        "Go std library import must never be censused, got: {specifiers:?}"
    );
}

#[test]
fn external_import_is_censused_while_in_module_import_is_not() {
    let dir = module_tree();
    let out = analyze_tree(dir.path(), &config());
    let specifiers: Vec<&str> = out
        .package_imports
        .iter()
        .map(|p| p.specifier.as_str())
        .collect();
    assert!(
        specifiers.contains(&"github.com/some/dep"),
        "genuinely external import must still be censused, got: {specifiers:?}"
    );
    assert!(
        !specifiers.contains(&"example.com/app/internal/db"),
        "in-module-resolved import must NOT be censused, got: {specifiers:?}"
    );
}

#[test]
fn no_go_file_is_ever_a_dead_candidate() {
    let dir = module_tree();
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file.ends_with(".go")),
        "no .go file should ever be flagged dead-candidates, got: {:?}",
        out.findings
    );
}

#[test]
fn main_go_and_test_go_are_never_flagged_unreachable() {
    let dir = module_tree();
    let out = analyze_tree(dir.path(), &config());
    for file in [
        "main.go",
        "main_test.go",
        "internal/db/db.go",
        "internal/db/helpers.go",
    ] {
        assert!(
            !out.findings
                .iter()
                .any(|f| f.rule_id == "unreachable" && f.file == file),
            "{file} must never be flagged unreachable, got: {:?}",
            out.findings
        );
    }
}

// --- Task 14a's dogfood-class pre-check: does an all-.go cycle need the all-.rs cycle treatment? -------
//
// Answer (verified here, not just argued — see `dep_graph.rs`'s own doc at the Rust-cycle-exclusion call
// site for the full written-out reasoning): NO exclusion needed. `merge_go_dep_edges` never emits a
// same-package edge, so every `.go` edge is INTER-package; a cycle built entirely from `.go` edges is
// therefore always a REAL cross-package import cycle (which `go build` itself would reject), not an
// artifact of this pass's own file-fanout. This test constructs the shape most likely to produce a
// SPURIOUS cycle from fanout alone — package `a` has TWO files, only one of which imports package `b`;
// package `b` has one file importing package `a` back, fanning out to BOTH `a` files — and asserts the
// real 2-file cycle IS reported, while the fanout-only-reached file (`a2.go`, which itself imports
// nothing) is correctly NOT swept into any cycle.

fn cross_package_cycle_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-go-cycle-self");
    dir.write("go.mod", "module example.com/cycle\n");
    // package a: two files. Only a1.go imports package b.
    dir.write(
        "pkg/a/a1.go",
        concat!(
            "package a\n",
            "\n",
            "import \"example.com/cycle/pkg/b\"\n",
            "\n",
            "func FromA1() string {\n",
            "\treturn b.FromB()\n",
            "}\n",
        ),
    );
    dir.write(
        "pkg/a/a2.go",
        concat!(
            "package a\n",
            "\n",
            "func FromA2() string {\n",
            "\treturn \"a2\"\n",
            "}\n",
        ),
    );
    // package b: one file, imports package a back — a real cross-package cycle.
    dir.write(
        "pkg/b/b1.go",
        concat!(
            "package b\n",
            "\n",
            "import \"example.com/cycle/pkg/a\"\n",
            "\n",
            "func FromB() string {\n",
            "\treturn a.FromA1()\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn cross_package_mutual_import_cycle_is_reported_not_excluded() {
    let dir = cross_package_cycle_tree();
    let out = analyze_tree(dir.path(), &config());

    // Precondition: the fanout edges this test depends on really exist.
    let a1 = out.ir.ir.dep.get("pkg/a/a1.go");
    assert!(
        a1.is_some_and(|t| t.iter().any(|x| x == "pkg/b/b1.go")),
        "expected a1.go -> b1.go edge, got: {a1:?}"
    );
    let b1 = out.ir.ir.dep.get("pkg/b/b1.go");
    assert!(
        b1.is_some_and(|t| t.iter().any(|x| x == "pkg/a/a1.go"))
            && b1.is_some_and(|t| t.iter().any(|x| x == "pkg/a/a2.go")),
        "expected b1.go -> BOTH a1.go and a2.go (fan-out to every file in package a), got: {b1:?}"
    );

    let circular: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "circular")
        .collect();
    assert!(
        !circular.is_empty(),
        "a real cross-package Go import cycle must be reported, not excluded — got no circular findings"
    );

    // The real 2-file cycle (a1.go, b1.go) must be among the reported cycles.
    let has_real_cycle = circular.iter().any(|f| {
        f.data
            .as_ref()
            .and_then(|d| d.get("cycle"))
            .and_then(|c| c.as_array())
            .is_some_and(|arr| {
                let members: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
                members.len() == 2
                    && members.contains(&"pkg/a/a1.go")
                    && members.contains(&"pkg/b/b1.go")
            })
    });
    assert!(
        has_real_cycle,
        "expected the real (a1.go, b1.go) cycle among circular findings, got: {circular:?}"
    );

    // `a2.go` imports nothing, so it cannot be part of ANY cycle — the fan-out edge INTO it must not
    // spuriously sweep it into a reported cycle.
    let a2_in_any_cycle = circular.iter().any(|f| {
        f.data
            .as_ref()
            .and_then(|d| d.get("cycle"))
            .and_then(|c| c.as_array())
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some("pkg/a/a2.go")))
    });
    assert!(
        !a2_in_any_cycle,
        "a2.go imports nothing and must never appear inside a reported cycle, got: {circular:?}"
    );
}
