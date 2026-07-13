//! End-to-end pin for `axios-defaults-base-v1` (the CONSUME-side counterpart of
//! `analyze_controller_prefix_ref.rs`'s NestJS `setGlobalPrefix` e2e test): a real
//! `axios.defaults.baseURL = "/api"` bootstrap assignment in one file, and a real `axios.get(...)` call
//! site in a DIFFERENT file, composed through `zzop_engine::analyze_tree` end-to-end. Only assemble-time
//! composition (`compose::apply_client_base_prefixes`) can join these — the per-file egress extractor
//! never sees the bootstrap file's assignment.

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
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    }
}

#[test]
fn axios_defaults_base_url_prefixes_a_cross_file_axios_call_end_to_end() {
    let dir = TempDir::new("zzop-engine-axios-base-prefix");
    dir.write(
        "src/main.ts",
        "axios.defaults.baseURL = \"https://api.example.io/api\";\n",
    );
    dir.write(
        "src/user.service.ts",
        concat!(
            "export async function getUser() {\n",
            "  return axios.get('/users');\n",
            "}\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config());

    let consumes = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected merged IoFacts")
        .consumes
        .clone();

    let http_keys: Vec<&str> = consumes
        .iter()
        .filter(|c| c.kind == "http")
        .filter_map(|c| c.key.as_deref())
        .collect();
    assert!(
        http_keys.contains(&"GET /api/users"),
        "expected the axios.defaults.baseURL prefix to compose onto the cross-file call site, got: {http_keys:?}"
    );

    // The sentinel itself must never reach output.
    assert!(
        !consumes.iter().any(|c| c.kind == "client-base-prefix"),
        "the client-base-prefix sentinel must be stripped before output, got: {consumes:?}"
    );
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("axios.defaults.baseURL")),
        "a single resolvable base must not warn: {:?}",
        out.warnings
    );
}
