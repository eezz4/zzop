//! End-to-end test for the new Java import-resolution wiring (task 4/5/8 of the parser-java ->
//! parser-java-21 swap): a mini two-package tree exercising the whole resolution pipeline
//! `analyze::assemble::dep_graph::merge_java_dep_edges` / `collect::census::drain_java_candidates` build
//! on top of `pipeline::scan_java_index`. Fixture shape:
//! - `com/example/a/Greeter.java` + `com/example/a/Helper.java` — two files in ONE package, the glob-import
//!   fanout target.
//! - `com/example/b/SingleImportCaller.java` — `import com.example.a.Greeter;` (single-type) +
//!   `import java.util.List;` (JDK std, must never enter the census).
//! - `com/example/b/GlobImportCaller.java` — `import com.example.a.*;` (glob, fans out to BOTH `a` files).
//! - `com/example/b/SpringController.java` — `import org.springframework.web.bind.annotation.GetMapping;`
//!   (unresolved, censused-at-two-segment-grain) + a real `@GetMapping` route, also exercised by the
//!   cross-layer FE-fetch join test below (extends `analyze_multi_tree_java.rs`'s own fixture shape).

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

fn write_fixture(dir: &TempDir) {
    dir.write(
        "com/example/a/Greeter.java",
        concat!(
            "package com.example.a;\n\n",
            "public class Greeter {\n",
            "    public String sayHi() { return \"hi\"; }\n",
            "}\n",
        ),
    );
    dir.write(
        "com/example/a/Helper.java",
        concat!("package com.example.a;\n\n", "public class Helper {}\n",),
    );
    dir.write(
        "com/example/b/SingleImportCaller.java",
        concat!(
            "package com.example.b;\n\n",
            "import com.example.a.Greeter;\n",
            "import java.util.List;\n\n",
            "public class SingleImportCaller {\n",
            "    public void run() {\n",
            "        Greeter g = new Greeter();\n",
            "    }\n",
            "}\n",
        ),
    );
    dir.write(
        "com/example/b/GlobImportCaller.java",
        concat!(
            "package com.example.b;\n\n",
            "import com.example.a.*;\n\n",
            "public class GlobImportCaller {}\n",
        ),
    );
    dir.write(
        "com/example/b/SpringController.java",
        concat!(
            "package com.example.b;\n\n",
            "import org.springframework.web.bind.annotation.GetMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "public class SpringController {\n",
            "    @GetMapping(\"/hello\")\n",
            "    public String hello() { return \"hi\"; }\n",
            "}\n",
        ),
    );
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-java".to_string(),
        ..EngineConfig::default()
    }
}

#[test]
fn single_type_import_resolves_to_a_real_dep_graph_edge() {
    let dir = TempDir::new("zzop-java-imports");
    write_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());
    let targets = out
        .ir
        .ir
        .dep
        .get("com/example/b/SingleImportCaller.java")
        .expect("SingleImportCaller.java should be a dep-graph node");
    assert_eq!(
        targets,
        &vec!["com/example/a/Greeter.java".to_string()],
        "a plain single-type import must resolve to exactly the declaring file, {targets:?}"
    );
}

#[test]
fn glob_import_fans_out_to_every_file_in_the_target_package() {
    let dir = TempDir::new("zzop-java-imports");
    write_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());
    let mut targets = out
        .ir
        .ir
        .dep
        .get("com/example/b/GlobImportCaller.java")
        .expect("GlobImportCaller.java should be a dep-graph node")
        .clone();
    targets.sort();
    assert_eq!(
        targets,
        vec![
            "com/example/a/Greeter.java".to_string(),
            "com/example/a/Helper.java".to_string(),
        ],
        "a glob import must fan out to EVERY file in the target package, got: {targets:?}"
    );
}

#[test]
fn jdk_std_import_is_excluded_from_the_package_census() {
    let dir = TempDir::new("zzop-java-imports");
    write_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());
    assert!(
        out.package_imports.iter().all(|p| p.specifier != "java"
            && p.specifier != "java.util"
            && !p.specifier.starts_with("java.")),
        "java.util.List must never enter the package census, got: {:?}",
        out.package_imports
    );
}

#[test]
fn unresolved_spring_import_enters_the_census_at_two_segment_grain() {
    let dir = TempDir::new("zzop-java-imports");
    write_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());
    let spring_entries: Vec<_> = out
        .package_imports
        .iter()
        .filter(|p| p.specifier.starts_with("org.springframework"))
        .collect();
    assert_eq!(
        spring_entries.len(),
        1,
        "expected exactly one Spring census entry (both GetMapping and RestController specifiers \
         collapse into the same two-segment grain), got: {:?}",
        out.package_imports
    );
    let entry = spring_entries[0];
    assert_eq!(
        entry.specifier, "org.springframework",
        "census grain must be the first TWO dotted segments, not the raw specifier verbatim"
    );
    assert_eq!(
        entry.example_file, "com/example/b/SpringController.java",
        "the only file importing an org.springframework.* symbol in this fixture"
    );
}

#[test]
fn java_files_are_absent_from_dead_candidates_findings() {
    let dir = TempDir::new("zzop-java-imports");
    write_fixture(&dir);
    let out = analyze_tree(dir.path(), &config());
    // `Helper.java` has fan_in == 0 from the import graph's own perspective directly (only reached via
    // the GlobImportCaller fanout target, which the rule itself never distinguishes from "genuinely
    // unused") — if `.java` were still eligible, this fixture would very likely produce at least one
    // `dead-candidates` finding among these small, mostly-unimported files. None should appear at all:
    // `.java` is excluded from dead-candidate eligibility entirely (same-package usage needs no import).
    let java_dead_candidates: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.rule_id == "dead-candidates" && f.file.ends_with(".java"))
        .collect();
    assert!(
        java_dead_candidates.is_empty(),
        "expected zero dead-candidates findings for any .java file, got: {java_dead_candidates:?}"
    );
}

#[test]
fn fe_fetch_still_joins_to_the_spring_get_mapping_route_across_trees() {
    // Cross-layer join parity check (task 10): the semantic-upgrade batch that gave Java real
    // imports/visibility must not regress the FE-fetch x Spring-route join
    // `analyze_multi_tree_java.rs` already proves — ported here against THIS fixture's own
    // `SpringController.java` (package-qualified, unlike that file's default-package original) to
    // confirm the join survives a real `package` declaration + real imports in the BE file too.
    let fe = TempDir::new("zzop-java-imports-fe");
    fe.write(
        "src/Ctx.tsx",
        "export function load() { return fetch(\"/hello\"); }\n",
    );
    let be = TempDir::new("zzop-java-imports-be");
    write_fixture(&be);

    let trees = vec![
        (fe.path().to_path_buf(), config_for("fe")),
        (be.path().to_path_buf(), config_for("be-java")),
    ];
    let out = analyze_trees(&trees);

    let http_edges: Vec<_> = out
        .cross_layer
        .edges
        .iter()
        .filter(|e| e.kind == "http" && e.key == "GET /hello")
        .collect();
    assert_eq!(
        http_edges.len(),
        1,
        "expected the FE fetch to join the Spring @GetMapping route, got edges: {:?}",
        out.cross_layer.edges
    );
    let edge = http_edges[0];
    assert_eq!(edge.to.source, "be-java");
    assert_eq!(edge.to.file, "com/example/b/SpringController.java");
    assert_eq!(edge.to.symbol.as_deref(), Some("hello"));
    assert!(edge.cross_source);
}

fn config_for(source_id: &str) -> EngineConfig {
    EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    }
}
