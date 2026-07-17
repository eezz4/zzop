//! Exercises `rules/dsl/http/http.json`'s HTTP-route rules end-to-end via `zzop_engine::analyze_tree` against
//! real swc-parsed TypeScript fixtures. See `http.json` for each rule's exact matcher shape and message.
//!
//! Ordering-aware and graph-shaped route checks (auth-state-machine transitions, API churn, unsafe-read-endpoint,
//! non-idempotent-write, FE/BE spec drift) are out of scope for a per-file DSL matcher and stay on the native-analysis backlog.
//!
//! All three rules require file paths shaped like `HTTP_SCANNER_DEFAULTS.beHandlerPathPattern`; fixtures
//! below use `src/routes/apiRoutes.ts` so the `/routes/` alternative matches. `/routes/` and `/controllers/`
//! require a slash on both sides, so a route file at the tree root with no parent directory would not match.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

/// A self-cleaning temp directory (std-only mkdtemp equivalent — same pattern as `sql/sql.rs`/`typescript/typescript.rs`).
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

/// Loads the real `http.json` pack, filtered so this test is unaffected by sibling packs under concurrent development.
fn http_pack() -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl");
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors: {:?}",
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == "http")
        .expect("http pack present")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "http-fixture".to_string(),
        packs: vec![http_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("http/{rule}"))
        .collect()
}

// --- file_pattern language-scope regression ---

#[test]
fn java_file_under_an_api_directory_is_out_of_scope() {
    // Regression: the `(?:^|/)api/`/`/routes/`/`/controllers/` alternatives used to match ANY file
    // extension sitting under such a directory (a bare path-fragment match with no extension anchor),
    // so a Java Spring controller living at `.../io/spring/api/CommentsApi.java` (the be-spring corpus
    // shape) fell into this pack's scope even though every rule's matcher vocabulary
    // (`apiRoutes.get/post/put/patch/delete(...)`) is a JS/TS-only router-wrapper idiom no Java file can
    // ever contain. Each alternative now also requires a JS/TS extension, same as `[Hh]andler.ts$`/
    // `[Cc]ontroller.ts$` already did.
    let dir = TempDir::new("zzop-http");
    dir.write(
        "src/main/java/io/spring/api/CommentsApi.java",
        "apiRoutes.get(\"/api/admin/users\", api.userList);\napiRoutes.get(\"/api/dev/config\", api.devConfig);\n",
    );
    let out = scan(&dir);
    assert!(out.findings.is_empty(), "{:?}", out.findings);
}

mod auth_gates;
mod read_model;
mod route_exposure;
