//! End-to-end coverage: schema.prisma files on disk -> core analysis (both the schema-only
//! and usage-combined paths).
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

/// A self-cleaning temp directory (std-only, no `tempfile` crate dependency) for isolated on-disk
/// fixtures.
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

fn invoice_fixture() -> TempDir {
    let dir = TempDir::new("zzop-parser-prisma");
    dir.write(
        "prisma/schema.prisma",
        "model Invoice {\n  id String @id\n  customerId String\n  totalAmount Float\n  note String\n  updatedAt DateTime\n}",
    );
    dir
}

// --- prismaSchemaAnalysis (structure-only) ---

#[test]
fn prisma_schema_analysis_finds_parses_and_analyzes_structural_and_db_pattern_rules() {
    let dir = invoice_fixture();
    let result = prisma_schema_analysis(dir.path(), "be");
    let result = result.expect("expected analysis");
    assert_eq!(
        result
            .models
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Invoice"]
    );
    let rules: Vec<&str> = result.issues.iter().map(|i| i.rule.as_str()).collect();
    assert!(rules.contains(&"fk-no-index"));
    assert!(rules.contains(&"float-money"));
    assert!(rules.contains(&"stale-updated-at"));
    // usage-based rules must NOT fire (no code scan in the structure-only path)
    assert!(!rules.contains(&"dead-model"));
    assert!(!rules.contains(&"dead-field"));
}

#[test]
fn prisma_schema_analysis_returns_none_for_non_be_targets() {
    let dir = TempDir::new("zzop-parser-prisma");
    dir.write("prisma/schema.prisma", "model X {\n  id String @id\n}");
    assert!(prisma_schema_analysis(dir.path(), "fe").is_none());
}

#[test]
fn prisma_schema_analysis_returns_none_when_no_schema_prisma_exists() {
    let dir = TempDir::new("zzop-parser-prisma");
    assert!(prisma_schema_analysis(dir.path(), "be").is_none());
}

#[test]
fn build_common_ir_projects_models_as_symbols() {
    let schema =
        "\n// user table\nmodel User {\n  id String @id\n}\n\nmodel Post {\n  id String @id\n}\n";
    let ir = build_common_ir(
        "svc",
        &[("db/schema.prisma".to_string(), schema.to_string())],
    );
    assert_eq!(ir.parser, "prisma");
    assert_eq!(ir.source, "svc");
    assert!(ir.ir.dep.is_empty()); // PSL has no imports
    let user = ir
        .ir
        .symbols
        .iter()
        .find(|s| s.name == "User")
        .expect("User model projected");
    assert_eq!(user.id, "db/schema.prisma#User");
    assert_eq!(user.file, "db/schema.prisma");
    assert!(user.exported);
    assert_eq!(user.line, 3); // `model User {` sits on line 3 of SAMPLE (leading newline + comment)
    assert!(ir.ir.loc["db/schema.prisma"] > 0);
}
