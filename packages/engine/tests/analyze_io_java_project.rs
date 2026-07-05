//! End-to-end test for the whole-project Java Spring provides pass (`zpz_parser_java::
//! extract_http_provides_project`, wired into `analyze::assemble` by `run_java_provides_project_pass`)
//! reaching `analyze_tree`/`analyze_trees` — a shape neither the fused per-file pass
//! (`crate::io::extract_java_file_io`) nor `analyze_io_java.rs`'s single-file fixture can see on their own:
//! a `FooController extends FooControllerCE` split (methods + class-level `@RequestMapping` live on the
//! un-annotated `CE` base class, in a DIFFERENT file from the `@RestController` subclass) whose prefix is
//! itself a cross-file, `+`-concatenated constant reference (`Paths.RESOURCE_URL` -> `BASE_URL + VERSION +
//! "/assets"`, each term a `static final String` declared in yet another file). Mirrors `project.rs`'s own
//! unit test for this same fixture shape, just driven through the public engine entry point instead of the
//! parser crate directly.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_tree, EngineConfig};

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

fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-io-java-project-fixture");
    dir.write(
        "constants/ce/PathsCE.java",
        concat!(
            "package com.example.server.constants.ce;\n\n",
            "public class PathsCE {\n",
            "    static final String BASE_URL = \"/api\";\n",
            "    static final String VERSION = \"/v1\";\n",
            "    public static final String RESOURCE_URL = BASE_URL + VERSION + \"/assets\";\n",
            "}\n",
        ),
    );
    dir.write(
        "constants/Paths.java",
        concat!(
            "package com.example.server.constants;\n\n",
            "import com.example.server.constants.ce.PathsCE;\n\n",
            "public class Paths extends PathsCE {}\n",
        ),
    );
    dir.write(
        "controllers/ce/ResourceControllerCE.java",
        concat!(
            "package com.example.server.controllers.ce;\n\n",
            "import com.example.server.constants.Paths;\n",
            "import org.springframework.web.bind.annotation.GetMapping;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n\n",
            "@RequestMapping(Paths.RESOURCE_URL)\n",
            "public class ResourceControllerCE {\n\n",
            "    @GetMapping(\"/{id}\")\n",
            "    public void getById() {}\n",
            "}\n",
        ),
    );
    dir.write(
        "controllers/ResourceController.java",
        concat!(
            "package com.example.server.controllers;\n\n",
            "import com.example.server.constants.Paths;\n",
            "import com.example.server.controllers.ce.ResourceControllerCE;\n",
            "import org.springframework.web.bind.annotation.RequestMapping;\n",
            "import org.springframework.web.bind.annotation.RestController;\n\n",
            "@RestController\n",
            "@RequestMapping(Paths.RESOURCE_URL)\n",
            "public class ResourceController extends ResourceControllerCE {\n",
            "}\n",
        ),
    );
    dir
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "be-java".to_string(),
        ..EngineConfig::default()
    }
}

#[test]
fn ce_split_controller_with_concatenated_constant_prefix_yields_provides_through_analyze_tree() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());
    let io = out.ir.ir.io.as_ref().expect(
        "expected merged IoFacts on the tree-wide CommonIr — per-file alone sees zero routes here",
    );

    let keys: Vec<&str> = io.provides.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(
        keys,
        vec!["GET /api/v1/assets/{}"],
        "expected exactly one route resolved via the CE-split + concatenated-constant project pass, got: {:?}",
        io.provides
    );

    let route = &io.provides[0];
    assert_eq!(route.kind, "http");
    assert_eq!(route.file, "controllers/ce/ResourceControllerCE.java");
    assert_eq!(route.symbol.as_deref(), Some("getById"));
}

#[test]
fn two_runs_over_the_same_ce_split_tree_produce_identical_merged_io() {
    let dir = fixture_tree();
    let out1 = analyze_tree(dir.path(), &config());
    let out2 = analyze_tree(dir.path(), &config());
    assert_eq!(
        serde_json::to_value(&out1.ir.ir.io).unwrap(),
        serde_json::to_value(&out2.ir.ir.io).unwrap()
    );
}
