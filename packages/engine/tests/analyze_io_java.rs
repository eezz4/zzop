//! End-to-end test for the Java/Spring `IoFacts` wiring: a `@RestController`-annotated Java class's
//! `@GetMapping` methods become per-file `SourceFile.io` provides AND merge into the tree-wide
//! `CommonIr.ir.io`, exactly the same wiring `analyze_io.rs` already proves for the TypeScript/Hono side.
//! Fixture shape: a class-level `@RequestMapping("/authen")` prefix plus three `@GetMapping` methods.

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

const SESSION_CONTROLLER_JAVA: &str = concat!(
    "package com.example.apps.controllers;\n\n",
    "import org.springframework.web.bind.annotation.GetMapping;\n",
    "import org.springframework.web.bind.annotation.RequestMapping;\n",
    "import org.springframework.web.bind.annotation.RestController;\n\n",
    "@RequestMapping(\"/authen\")\n",
    "@RestController\n",
    "public class SessionController {\n\n",
    "    @GetMapping(\"/getGoogleRedirect\")\n",
    "    public String getGoogleRedirect() {\n        return \"\";\n    }\n\n",
    "    @GetMapping(\"/getUserInfo\")\n",
    "    public UserInfo getUserInfo() {\n        return null;\n    }\n\n",
    "    @GetMapping(\"/getSignout\")\n",
    "    public boolean getSignout() {\n        return true;\n    }\n}\n",
);

fn fixture_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-io-java-fixture");
    dir.write(
        "src/main/java/com/example/apps/controllers/SessionController.java",
        SESSION_CONTROLLER_JAVA,
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
fn java_rest_controller_class_yields_http_provides_on_the_tree_wide_common_ir() {
    let dir = fixture_tree();
    let out = analyze_tree(dir.path(), &config());
    let io = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected merged IoFacts on the tree-wide CommonIr");

    assert!(
        io.consumes.is_empty(),
        "Java side has no egress extractor yet"
    );

    let mut keys: Vec<&str> = io.provides.iter().map(|p| p.key.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "GET /authen/getGoogleRedirect",
            "GET /authen/getSignout",
            "GET /authen/getUserInfo",
        ]
    );

    let user_info = io
        .provides
        .iter()
        .find(|p| p.key == "GET /authen/getUserInfo")
        .expect("expected the getUserInfo route");
    assert_eq!(user_info.kind, "http");
    assert_eq!(user_info.symbol.as_deref(), Some("getUserInfo"));
    assert_eq!(
        user_info.file,
        "src/main/java/com/example/apps/controllers/SessionController.java"
    );
}

#[test]
fn two_runs_over_the_same_java_tree_produce_identical_merged_io() {
    let dir = fixture_tree();
    let out1 = analyze_tree(dir.path(), &config());
    let out2 = analyze_tree(dir.path(), &config());
    assert_eq!(
        serde_json::to_value(&out1.ir.ir.io).unwrap(),
        serde_json::to_value(&out2.ir.ir.io).unwrap()
    );
}
