//! Dogfooding smoke test: a mini two-crate Rust workspace, driven end to end through `analyze_tree`,
//! pinning task 6's "workspace-member manifest scan" payoff (`crate::pipeline::scan_rust_workspace` /
//! `analyze::assemble::helpers::resolve_rust_import`) alongside the rest of the Rust wiring —
//! intra-crate `mod`-tree resolution, cross-crate `use <workspace-member-name>::x` resolution, the
//! package-import census exclusion for a resolved workspace-member head, and the `.rs`
//! dead-candidates/unreachable exemptions (rules-graph). Mirrors `analyze_python_cross_layer.rs`'s
//! self-contained `TempDir`-harness style.

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
        source_id: "rust-workspace-self".to_string(),
        ..EngineConfig::default()
    }
}

/// Two-member Cargo workspace: `demo-core` (a library, `crates/core`) and `demo-app` (a binary,
/// `crates/app`, no `lib.rs` of its own — exercises task 6's "`src/main.rs` if `lib.rs` absent among
/// walked files" fallback). `demo-app`'s `src/main.rs` cross-crate-imports `demo-core` via its
/// UNDERSCORE `use`-path spelling (`demo_core`) even though the manifest's own `name` is hyphenated
/// (`demo-core`) — the exact normalization task 6 requires.
fn workspace_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-rust-self");
    dir.write(
        "crates/core/Cargo.toml",
        concat!(
            "[package]\n",
            "name = \"demo-core\"\n",
            "version = \"0.0.0\"\n",
            "\n",
            "[dependencies]\n",
            // A `name` key under [dependencies] must never be mistaken for the package's own name —
            // see `pipeline::rust_workspace::parse_package_name`'s own unit tests for the isolated
            // version of this same regression pin.
            "name = \"not-the-package\"\n",
        ),
    );
    dir.write(
        "crates/core/src/lib.rs",
        concat!(
            "pub mod util;\n",
            "\n",
            "use crate::util::helper;\n",
            "\n",
            "pub fn g() -> i32 {\n",
            "    helper()\n",
            "}\n",
        ),
    );
    dir.write(
        "crates/core/src/util.rs",
        concat!("pub fn helper() -> i32 {\n", "    42\n", "}\n",),
    );
    dir.write(
        "crates/app/Cargo.toml",
        concat!(
            "[package]\n",
            "name = \"demo-app\"\n",
            "version = \"0.0.0\"\n",
        ),
    );
    dir.write(
        "crates/app/src/main.rs",
        concat!(
            "use demo_core::g;\n",
            // A genuinely external head — never resolves in-tree, so it must still enter the package
            // census exactly as before.
            "use serde::Deserialize;\n",
            "\n",
            "fn main() {\n",
            "    let _ = g();\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn intra_crate_mod_declaration_resolves_to_a_real_dep_graph_edge() {
    let dir = workspace_tree();
    let out = analyze_tree(dir.path(), &config());
    let targets = out.ir.ir.dep.get("crates/core/src/lib.rs");
    assert!(
        targets.is_some_and(|t| t.iter().any(|x| x == "crates/core/src/util.rs")),
        "expected lib.rs -> util.rs mod-tree edge, got: {targets:?}"
    );
}

#[test]
fn cross_crate_use_resolves_to_the_other_workspace_members_root_file() {
    let dir = workspace_tree();
    let out = analyze_tree(dir.path(), &config());
    let targets = out.ir.ir.dep.get("crates/app/src/main.rs");
    assert!(
        targets.is_some_and(|t| t.iter().any(|x| x == "crates/core/src/lib.rs")),
        "expected main.rs -> (workspace member demo-core's) lib.rs cross-crate edge, got: {targets:?}"
    );
}

#[test]
fn external_head_is_censused_while_the_resolved_workspace_member_head_is_not() {
    let dir = workspace_tree();
    let out = analyze_tree(dir.path(), &config());
    let specifiers: Vec<&str> = out
        .package_imports
        .iter()
        .map(|p| p.specifier.as_str())
        .collect();
    assert!(
        specifiers.contains(&"serde"),
        "genuinely external head serde must still be censused, got: {specifiers:?}"
    );
    assert!(
        !specifiers.contains(&"demo_core") && !specifiers.contains(&"demo-core"),
        "workspace-member head demo_core must NOT be censused (resolves in-tree), got: {specifiers:?}"
    );
}

#[test]
fn no_rust_file_is_ever_a_dead_candidate() {
    let dir = workspace_tree();
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id == "dead-candidates" && f.file.ends_with(".rs")),
        "no .rs file should ever be flagged dead-candidates, got: {:?}",
        out.findings
    );
}

#[test]
fn main_rs_and_lib_rs_are_never_flagged_unreachable() {
    let dir = workspace_tree();
    let out = analyze_tree(dir.path(), &config());
    for file in [
        "crates/app/src/main.rs",
        "crates/core/src/lib.rs",
        "crates/core/src/util.rs",
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

/// The two dogfood-found FP classes from the first zzop-on-zzop self-analysis run, pinned e2e:
/// (1) an idiomatic parent<->child module cycle (`mod x;` down, `use crate::...` back up) must not be
/// reported by `circular` — cargo forbids cross-crate cycles and rustc compiles a crate as one unit;
/// (2) a cargo-manifest-declared `[[test]] path = "..."` target file with in-tree importers (fan_in > 0)
/// is loaded by cargo, not imported, so it and its island must not be flagged `unreachable`.
fn dogfood_regressions_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-rust-dogfood");
    dir.write(
        "crates/core/Cargo.toml",
        "[package]\nname = \"cycle-demo\"\nversion = \"0.0.0\"\n",
    );
    // lib.rs -> util.rs (mod decl) and util.rs -> lib.rs (use crate::) — a 2-cycle, idiomatic Rust.
    dir.write(
        "crates/core/src/lib.rs",
        concat!(
            "pub mod util;\n",
            "\n",
            "pub fn g() -> i32 {\n",
            "    1\n",
            "}\n",
        ),
    );
    dir.write(
        "crates/core/src/util.rs",
        concat!(
            "use crate::g;\n",
            "\n",
            "pub fn helper() -> i32 {\n",
            "    g()\n",
            "}\n",
        ),
    );
    // A DSL-pack-style declared test target OUTSIDE any src/ tree: http.rs is cargo-loaded via the
    // explicit [[test]] path, declares `mod util;` (child lives in http/util.rs — non-root module
    // anchoring), and the child's `use super::` gives http.rs a positive fan_in.
    dir.write(
        "rules/Cargo.toml",
        concat!(
            "[package]\n",
            "name = \"packs\"\n",
            "version = \"0.0.0\"\n",
            "\n",
            "[[test]]\n",
            "name = \"http\"\n",
            "path = \"dsl/http/http.rs\"\n",
        ),
    );
    dir.write(
        "rules/dsl/http/http.rs",
        concat!(
            "mod util;\n",
            "\n",
            "pub fn pack() -> i32 {\n",
            "    util::helper()\n",
            "}\n",
        ),
    );
    dir.write(
        "rules/dsl/http/http/util.rs",
        concat!(
            "use super::pack;\n",
            "\n",
            "pub fn helper() -> i32 {\n",
            "    let _ = pack;\n",
            "    2\n",
            "}\n",
        ),
    );
    dir
}

#[test]
fn idiomatic_rust_module_cycles_are_not_reported_as_circular() {
    let dir = dogfood_regressions_tree();
    let out = analyze_tree(dir.path(), &config());
    // Precondition: both cycle edges really exist in the dep graph (otherwise this test would pass
    // vacuously with no cycle to suppress).
    let down = out.ir.ir.dep.get("crates/core/src/lib.rs");
    let up = out.ir.ir.dep.get("crates/core/src/util.rs");
    assert!(
        down.is_some_and(|t| t.iter().any(|x| x == "crates/core/src/util.rs")),
        "expected the mod-decl down-edge, got: {down:?}"
    );
    assert!(
        up.is_some_and(|t| t.iter().any(|x| x == "crates/core/src/lib.rs")),
        "expected the use-crate up-edge, got: {up:?}"
    );
    assert!(
        !out.findings.iter().any(|f| f.rule_id == "circular"),
        "an all-.rs module cycle must not surface as a circular finding, got: {:?}",
        out.findings
    );
}

#[test]
fn cargo_declared_test_target_island_is_not_flagged_unreachable() {
    let dir = dogfood_regressions_tree();
    let out = analyze_tree(dir.path(), &config());
    for file in ["rules/dsl/http/http.rs", "rules/dsl/http/http/util.rs"] {
        assert!(
            !out.findings
                .iter()
                .any(|f| f.rule_id == "unreachable" && f.file == file),
            "{file} is cargo-loaded via the manifest's [[test] ] path — must not be unreachable, got: {:?}",
            out.findings
        );
    }
}
