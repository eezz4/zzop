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

// --- build_common_ir: db-table PROVIDES ---

#[test]
fn build_common_ir_emits_db_table_provide_per_model() {
    let schema =
        "\n// user table\nmodel User {\n  id String @id\n}\n\nmodel Post {\n  id String @id\n}\n";
    let ir = build_common_ir(
        "svc",
        &[("db/schema.prisma".to_string(), schema.to_string())],
    );
    let io = ir.ir.io.expect("provides expected for a non-empty schema");
    assert!(io.consumes.is_empty());
    let user_provide = io
        .provides
        .iter()
        .find(|p| p.key == "table:user")
        .expect("User model provides table:user");
    assert_eq!(user_provide.kind, "db-table");
    assert_eq!(user_provide.file, "db/schema.prisma");
    assert_eq!(user_provide.line, 3); // same declaration line as the symbol above
    assert!(io.provides.iter().any(|p| p.key == "table:post"));
}

#[test]
fn build_common_ir_lower_firsts_multi_word_pascal_model_names() {
    // `UserProfile` -> `userProfile`, matching Prisma's own client-accessor casing (first char only).
    let schema = "model UserProfile {\n  id String @id\n}\n";
    let ir = build_common_ir("svc", &[("schema.prisma".to_string(), schema.to_string())]);
    let io = ir.ir.io.expect("provides expected");
    assert_eq!(io.provides[0].key, "table:userProfile");
}

#[test]
fn build_common_ir_emits_no_io_for_schema_with_no_models() {
    // An (unrealistic but honest) schema with zero models must not emit an empty-but-`Some` IoFacts —
    // mirrors the parser-typescript `build_common_ir`'s empty-consumes -> `None` convention.
    let ir = build_common_ir("svc", &[("schema.prisma".to_string(), "".to_string())]);
    assert!(ir.ir.io.is_none());
}

#[test]
fn provide_key_matches_hand_built_bare_receiver_consume_key() {
    // Cross-side key-equality: the PROVIDE key `build_common_ir` emits for `model Article` must be
    // BYTE-IDENTICAL to the CONSUME key `zzop_parser_typescript::adapters::db_table_consume` would
    // produce for a call site like `prisma.article.findMany(...)` or `getPrisma().article.findMany(...)`
    // — that extractor keys off the accessor exactly as written at the call site
    // (`format!("table:{}", m.accessor)`, no re-casing), so for the join to work the schema side must
    // independently land on the same string. This crate does not depend on parser-typescript (no
    // cross-parser-crate edge for one string), so the expectation is a hand-built literal rather than a
    // call into that crate — kept honest by being exactly what a human reading `prisma.article.findMany`
    // would type as the accessor: the lower-camel Prisma client accessor for `model Article`.
    let schema = "model Article {\n  id String @id\n}\n";
    let ir = build_common_ir("svc", &[("schema.prisma".to_string(), schema.to_string())]);
    let provide_key = ir.ir.io.expect("provides expected").provides[0].key.clone();
    let hand_built_consume_key = format!("table:{}", "article"); // == literal accessor text in `prisma.article.findMany(...)`
    assert_eq!(provide_key, hand_built_consume_key);
}
