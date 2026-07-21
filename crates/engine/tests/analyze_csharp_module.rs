//! End-to-end test for C# namespace-based dep-graph resolution + import census (Stage 2 of wiring C#
//! into the engine — Stage 1 already gave `.cs` files dispatch/parse/fingerprint/io cross-layer join and
//! dep-graph NODE participation; this batch resolves their `using` imports into dep-graph EDGES and the
//! import census). Mirrors `analyze_java_imports.rs`'s own fixture/assertion shape, adapted for C#'s
//! simpler always-namespace-level `using` resolution (`analyze::assemble::dep_graph::
//! merge_csharp_dep_edges` / `collect::census::drain_csharp_candidates`, built on top of
//! `pipeline::scan_csharp_index`).

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
        source_id: "be-csharp".to_string(),
        ..EngineConfig::default()
    }
}

// --- Dep edge: a plain `using` resolves to the namespace's declaring file -------------------------------

#[test]
fn using_directive_resolves_to_a_real_dep_graph_edge() {
    let dir = TempDir::new("zzop-csharp-imports");
    dir.write(
        "services/User.cs",
        concat!(
            "namespace App.Services {\n",
            "    public class UserService {}\n",
            "}\n",
        ),
    );
    dir.write(
        "Program.cs",
        concat!(
            "using App.Services;\n",
            "class P {\n",
            "    void M() {\n",
            "        var s = new UserService();\n",
            "    }\n",
            "}\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let targets = out
        .ir
        .ir
        .dep
        .get("Program.cs")
        .expect("Program.cs should be a dep-graph node");
    assert_eq!(
        targets,
        &vec!["services/User.cs".to_string()],
        "a `using` on a single-file namespace must resolve to exactly the declaring file, {targets:?}"
    );
}

// --- Namespace fanout: a shared namespace edges to EVERY declaring file -----------------------------------

#[test]
fn using_directive_fans_out_to_every_file_declaring_the_namespace() {
    let dir = TempDir::new("zzop-csharp-imports");
    dir.write(
        "services/A.cs",
        "namespace App.Services { public class A {} }\n",
    );
    dir.write(
        "services/B.cs",
        "namespace App.Services { public class B {} }\n",
    );
    dir.write(
        "Program.cs",
        concat!("using App.Services;\n", "class P { void M() {} }\n",),
    );

    let out = analyze_tree(dir.path(), &config());
    let mut targets = out
        .ir
        .ir
        .dep
        .get("Program.cs")
        .expect("Program.cs should be a dep-graph node")
        .clone();
    targets.sort();
    assert_eq!(
        targets,
        vec!["services/A.cs".to_string(), "services/B.cs".to_string()],
        "a `using` on a multi-file namespace must fan out to EVERY declaring file, got: {targets:?}"
    );
}

// --- Std exclusion: System/Microsoft never create in-tree edges or census entries ------------------------

#[test]
fn std_and_framework_usings_are_excluded_from_edges_and_census() {
    let dir = TempDir::new("zzop-csharp-imports");
    dir.write(
        "services/A.cs",
        "namespace App.Services { public class A {} }\n",
    );
    dir.write(
        "Program.cs",
        concat!(
            "using System;\n",
            "using Microsoft.AspNetCore.Mvc;\n",
            "using App.Services;\n",
            "class P { void M() {} }\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    let targets = out
        .ir
        .ir
        .dep
        .get("Program.cs")
        .expect("Program.cs should be a dep-graph node");
    assert_eq!(
        targets,
        &vec!["services/A.cs".to_string()],
        "System/Microsoft usings must never create an in-tree dep edge, got: {targets:?}"
    );
    assert!(
        out.package_imports
            .iter()
            .all(|p| !p.specifier.starts_with("System") && !p.specifier.starts_with("Microsoft")),
        "System/Microsoft usings must never enter the package census, got: {:?}",
        out.package_imports
    );
}

// --- Census: in-tree resolves (drained), unresolved external stays a census entry ------------------------

#[test]
fn in_tree_using_is_drained_while_unresolved_external_stays_censused() {
    let dir = TempDir::new("zzop-csharp-imports");
    dir.write(
        "services/A.cs",
        "namespace App.Services { public class A {} }\n",
    );
    dir.write(
        "Program.cs",
        concat!(
            "using App.Services;\n",
            "using Some.ThirdParty.Lib;\n",
            "class P { void M() {} }\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config());
    assert!(
        out.package_imports
            .iter()
            .all(|p| p.specifier != "App.Services"),
        "an in-tree resolved namespace must never enter the package census, got: {:?}",
        out.package_imports
    );
    let external: Vec<_> = out
        .package_imports
        .iter()
        .filter(|p| p.specifier == "Some.ThirdParty.Lib")
        .collect();
    assert_eq!(
        external.len(),
        1,
        "an unresolved external using must enter the census exactly once, got: {:?}",
        out.package_imports
    );
    assert_eq!(external[0].example_file, "Program.cs");
}
