//! End-to-end proof that `.prisma` schema structural rules are actually wired into the engine: before this
//! wiring, `zzop_rules_schema::analyze_schema`/`apply_schema_rules` had unit-level tests
//! (`rules/native/rules-schema/src/structural.rs`) but `zzop_engine` never called either — a `.prisma` file's structural
//! anti-patterns (fk-no-index, float-money, stale-updated-at, ...) never became `Finding`s. `pipeline.rs`'s
//! `schema_findings` now runs `apply_schema_rules` in the same fused per-file pass that already parses the
//! schema for symbols, gated behind the pre-existing native id `"schema-structural"`
//! (`register_native_analyses`).
//!
//! The `Invoice` model fixture is the same shape `parser/parser-prisma/src/lib.rs`'s own
//! `prisma_schema_analysis_finds_...` test uses — `id`/`customerId`/`totalAmount`/`note`/`updatedAt`, no
//! `@@index`/`@relation`/`@updatedAt` — which fires `fk-no-index`, `float-money`, and `stale-updated-at`
//! (plus a couple of `info`-severity rules not asserted on here, since these three are the ones this test
//! is specifically pinning down).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RuleConfig;
use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

const INVOICE_SCHEMA: &str = "model Invoice {\n  id String @id\n  customerId String\n  totalAmount Float\n  note String\n  updatedAt DateTime\n}\n";

fn invoice_fixture() -> TempDir {
    let dir = TempDir::new("zzop-schema");
    dir.write("prisma/schema.prisma", INVOICE_SCHEMA);
    dir
}

fn schema_hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("schema/{rule}"))
        .collect()
}

#[test]
fn invoice_model_fires_fk_no_index_float_money_and_stale_updated_at() {
    let dir = invoice_fixture();
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    let fk = schema_hits(&out, "fk-no-index");
    assert_eq!(fk.len(), 1, "{:?}", out.findings);
    assert_eq!(fk[0].file, "prisma/schema.prisma");
    assert_eq!(fk[0].line, 1); // `model Invoice {` declaration line

    assert_eq!(
        schema_hits(&out, "float-money").len(),
        1,
        "{:?}",
        out.findings
    );
    assert_eq!(
        schema_hits(&out, "stale-updated-at").len(),
        1,
        "{:?}",
        out.findings
    );
}

#[test]
fn disabling_schema_structural_removes_every_schema_finding() {
    // `schema-usage` (a sibling native id with its own gate) also emits `schema/*` findings since the
    // usage wiring landed, so proving the structural toggle needs both ids off for the blanket
    // "no schema/* at all" assertion — the structural-off/usage-on split is asserted separately below.
    let dir = invoice_fixture();
    let config = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["schema-structural".to_string(), "schema-usage".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &config);
    assert!(
        !out.findings
            .iter()
            .any(|f| f.rule_id.starts_with("schema/")),
        "{:?}",
        out.findings
    );
}

#[test]
fn structural_toggle_leaves_usage_findings_and_vice_versa() {
    let dir = invoice_fixture();

    let structural_off = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["schema-structural".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &structural_off);
    assert!(
        schema_hits(&out, "fk-no-index").is_empty(),
        "{:?}",
        out.findings
    );
    assert_eq!(
        schema_hits(&out, "dead-model").len(),
        1,
        "{:?}",
        out.findings
    );

    let usage_off = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["schema-usage".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &usage_off);
    assert_eq!(
        schema_hits(&out, "fk-no-index").len(),
        1,
        "{:?}",
        out.findings
    );
    assert!(
        schema_hits(&out, "dead-model").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn warm_cached_run_still_reports_schema_findings() {
    let dir = invoice_fixture();
    let cache_dir = TempDir::new("zzop-schema-cache");
    let config = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        ..EngineConfig::default()
    };

    let cold = analyze_tree(dir.path(), &config);
    assert_eq!(
        schema_hits(&cold, "fk-no-index").len(),
        1,
        "{:?}",
        cold.findings
    );
    assert!(cold.cache.unwrap().misses > 0);

    // Warm run: full IR+findings cache hit for the unchanged schema file — schema findings must still be
    // present (they were part of the cached findings entry, not recomputed from scratch — see
    // `pipeline::schema_findings_eligible`'s doc for why the fresh-parse path and the cache-reuse path both
    // needed the wiring).
    let warm = analyze_tree(dir.path(), &config);
    assert_eq!(
        schema_hits(&warm, "fk-no-index").len(),
        1,
        "{:?}",
        warm.findings
    );
    assert_eq!(
        schema_hits(&warm, "float-money").len(),
        1,
        "{:?}",
        warm.findings
    );
    assert!(warm.cache.unwrap().hits > 0);
}

#[test]
fn ruleset_only_change_still_reflects_schema_structural_toggle_on_a_warm_cache() {
    // The cache's "IR hit, findings miss" partial-reuse path (a ruleset-only change: same content, same
    // parser fingerprint, different `disabled_rules`) must still run `schema_findings` — not just the
    // fresh-parse path — or a warm run re-enabling `schema-structural` after a run that disabled it would
    // silently keep serving zero schema findings from the disabled run's cached findings entry.
    let dir = invoice_fixture();
    let cache_dir = TempDir::new("zzop-schema-cache-toggle");

    let disabled_config = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        rule_config: RuleConfig {
            // `schema-usage` off as well — it emits `schema/*` findings outside the per-file cache this
            // test exercises, and the blanket assertion below is about the cached structural channel.
            disabled_rules: vec!["schema-structural".to_string(), "schema-usage".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let first = analyze_tree(dir.path(), &disabled_config);
    assert!(
        !first
            .findings
            .iter()
            .any(|f| f.rule_id.starts_with("schema/")),
        "{:?}",
        first.findings
    );

    let enabled_config = EngineConfig {
        cache_dir: Some(cache_dir.path().to_path_buf()),
        ..EngineConfig::default()
    };
    let second = analyze_tree(dir.path(), &enabled_config);
    assert_eq!(
        schema_hits(&second, "fk-no-index").len(),
        1,
        "{:?}",
        second.findings
    );
    // Same content + same parser fingerprint as the first run -> IR reused (a miss for findings only, not a
    // full reparse).
    assert!(second.cache.unwrap().misses > 0);
}

// --- schema-usage wiring (`zzop_core` implements the schema usage cross-checks —
// dead-model/dead-field/schema-churn — but the engine has to wire them in explicitly:
// `pipeline::schema_usage_findings` runs `cross_check_schema`/`apply_churn_rule` as a whole-tree
// global pass whenever code access exists, gated behind the pre-registered native id
// `"schema-usage"`. These tests prove that wiring end-to-end.) ---

/// Two models, no store bindings anywhere in the tree -> both are dead-model.
/// `cross_check_schema` short-circuits a dead model before its fields, so no dead-field noise
/// despite zero identifier usage.
#[test]
fn unbound_models_fire_dead_model_per_model() {
    let dir = TempDir::new("zzop-schema-usage");
    dir.write(
        "prisma/schema.prisma",
        "model Invoice {\n  id String @id\n  customerId String\n}\n\nmodel Customer {\n  id String @id\n}\n",
    );
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    let dead = schema_hits(&out, "dead-model");
    let models: Vec<&str> = dead
        .iter()
        .map(|f| f.data.as_ref().unwrap()["model"].as_str().unwrap())
        .collect();
    assert_eq!(models, ["Invoice", "Customer"], "{:?}", out.findings);
    assert_eq!(dead[0].file, "prisma/schema.prisma");
    assert_eq!(dead[0].line, 1); // `model Invoice {`
    assert_eq!(dead[1].line, 6); // `model Customer {` (line 5 is the blank separator)
}

/// A store binding (dsl-prisma default vocabulary: `createStore` + `getPrisma`) clears dead-model;
/// the bound model's fields are then usage-checked individually — `nickname` appears nowhere as an
/// identifier while `email` does, so exactly one dead-field fires.
#[test]
fn bound_model_skips_dead_model_but_flags_unused_field() {
    let dir = TempDir::new("zzop-schema-usage");
    dir.write(
        "prisma/schema.prisma",
        "model User {\n  id String @id\n  email String\n  nickname String\n}\n",
    );
    dir.write(
        "src/domains/user/STORES.ts",
        "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  userStore: createStore((f: UserFilters) => f, () => new PrismaStore(getPrisma().user)),\n};\n",
    );
    dir.write(
        "src/domains/user/service.ts",
        "import { STORES } from \"./STORES\";\nexport function contact(u: { email: string }) {\n  return u.email;\n}\n",
    );
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    assert!(
        schema_hits(&out, "dead-model").is_empty(),
        "{:?}",
        out.findings
    );
    let dead_fields = schema_hits(&out, "dead-field");
    assert_eq!(dead_fields.len(), 1, "{:?}", out.findings);
    assert_eq!(
        dead_fields[0].data.as_ref().unwrap()["field"].as_str(),
        Some("nickname")
    );
    assert_eq!(dead_fields[0].file, "prisma/schema.prisma");
    assert_eq!(dead_fields[0].line, 1); // anchors on the model declaration, same as structural rules
}
