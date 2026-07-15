//! End-to-end proof that the three schema x usage JOIN native rules — `soft-delete-bypass`,
//! `orderby-unindexed`, and `enum-string-drift` (`zzop_rules_schema::join`, rule-pack catalog rows
//! #27/#28/#29) — are wired into `zzop_engine::analyze_tree`
//! end to end: `.prisma` schema on disk + `.ts` BE source on disk -> real `Finding`s, gated per-id via
//! `RuleConfig::disabled_rules` (`crates/engine/src/analyze.rs`'s `run_schema_join_rules`).
//!
//! Fixture shape: one `prisma/schema.prisma` with three models —
//! - `Item { id, ownerId, deletedAt }` — soft-delete-bypass fixture (a soft-delete marker field, no index
//!   needed).
//! - `Order { id, status, createdAt, total }` with `@@index([status, createdAt])` — orderby-unindexed
//!   fixture (`status` is the covered leading column, `total` is not covered by anything).
//! - `User { id, role: Role }` plus `enum Role { USER ADMIN }` — enum-string-drift fixture.
//!
//! and one `src/repo.ts` with `getPrisma().<model>.<method>(...)` call sites (the same
//! `getPrisma()`-accessor vocabulary `zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN`/`scan_store_map`
//! already use elsewhere in this crate's tests) — one positive + one negative per rule, so a single fixture
//! tree proves both the hit and the no-hit path for each rule without needing six separate directories.

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

const SCHEMA: &str = "model Item {\n  id        String   @id\n  ownerId   String\n  deletedAt DateTime?\n}\n\nmodel Order {\n  id        String   @id\n  status    String\n  createdAt DateTime\n  total     Float\n\n  @@index([status, createdAt])\n}\n\nmodel User {\n  id   String @id\n  role Role\n}\n\nenum Role {\n  USER\n  ADMIN\n}\n";

/// Lines (1-based) of each `getPrisma()...` call below, pinned as named constants so the assertions read as
/// "this call site fires" rather than a magic number recomputed by counting newlines by eye.
const SOFT_DELETE_POSITIVE_LINE: u32 = 2; // getPrisma().item.findMany  -- no deletedAt filter
const SOFT_DELETE_NEGATIVE_LINE: u32 = 6; // getPrisma().item.findFirst -- has deletedAt filter
const ORDERBY_POSITIVE_LINE: u32 = 10; // getPrisma().order.findMany -- orderBy: total (unindexed)
const ORDERBY_NEGATIVE_LINE: u32 = 14; // getPrisma().order.findMany -- orderBy: status (indexed leading column)
const ENUM_DRIFT_POSITIVE_LINE: u32 = 18; // getPrisma().user.findMany -- role: 'BOGUS' (not a Role member)
const ENUM_DRIFT_NEGATIVE_LINE: u32 = 22; // getPrisma().user.findMany -- role: 'ADMIN' (a Role member)

const REPO_TS: &str = "\
export function listItemsUnfiltered() {
  return getPrisma().item.findMany({ where: { ownerId: 'u1' } });
}

export function findItemExplicitlyIncludingDeleted() {
  return getPrisma().item.findFirst({ where: { deletedAt: null } });
}

export function listOrdersByTotal() {
  return getPrisma().order.findMany({ orderBy: { total: 'asc' } });
}

export function listOrdersByStatus() {
  return getPrisma().order.findMany({ orderBy: { status: 'asc' } });
}

export function listUsersByBadRole() {
  return getPrisma().user.findMany({ where: { role: 'BOGUS' } });
}

export function listUsersByValidRole() {
  return getPrisma().user.findMany({ where: { role: 'ADMIN' } });
}
";

fn fixture() -> TempDir {
    let dir = TempDir::new("zzop-schema-join");
    dir.write("prisma/schema.prisma", SCHEMA);
    dir.write("src/repo.ts", REPO_TS);
    dir
}

fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a zzop_core::Finding> {
    out.findings.iter().filter(|f| f.rule_id == rule).collect()
}

// --- soft-delete-bypass ---

#[test]
fn soft_delete_bypass_fires_on_unfiltered_call_and_not_on_filtered_call() {
    let dir = fixture();
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    let found = hits(&out, "soft-delete-bypass");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/repo.ts");
    assert_eq!(found[0].line, SOFT_DELETE_POSITIVE_LINE);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert_eq!(
        found[0].data.as_ref().unwrap()["model"].as_str(),
        Some("Item")
    );
    assert_eq!(
        found[0].data.as_ref().unwrap()["field"].as_str(),
        Some("deletedAt")
    );
    assert!(found[0].message.contains("deletedAt"));
    assert!(found[0].message.contains("disabled_rules"));
    // Never fires at the filtered call site's line.
    assert!(!found.iter().any(|f| f.line == SOFT_DELETE_NEGATIVE_LINE));
}

#[test]
fn soft_delete_bypass_disabled_via_config_removes_the_finding_but_leaves_orderby_unindexed() {
    let dir = fixture();
    let config = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["soft-delete-bypass".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &config);
    assert!(
        hits(&out, "soft-delete-bypass").is_empty(),
        "{:?}",
        out.findings
    );
    // Per-id gating, not bundled: the sibling rule still runs.
    assert_eq!(
        hits(&out, "orderby-unindexed").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- orderby-unindexed ---

#[test]
fn orderby_unindexed_fires_on_uncovered_field_and_not_on_indexed_leading_column() {
    let dir = fixture();
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    let found = hits(&out, "orderby-unindexed");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/repo.ts");
    assert_eq!(found[0].line, ORDERBY_POSITIVE_LINE);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert_eq!(
        found[0].data.as_ref().unwrap()["model"].as_str(),
        Some("Order")
    );
    assert_eq!(
        found[0].data.as_ref().unwrap()["field"].as_str(),
        Some("total")
    );
    assert!(found[0].message.contains("@@index"));
    assert!(found[0].message.contains("disabled_rules"));
    // The `status`-ordered call (leading column of `@@index([status, createdAt])`) never fires.
    assert!(!found.iter().any(|f| f.line == ORDERBY_NEGATIVE_LINE));
}

#[test]
fn orderby_unindexed_disabled_via_config_removes_the_finding_but_leaves_soft_delete_bypass() {
    let dir = fixture();
    let config = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["orderby-unindexed".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &config);
    assert!(
        hits(&out, "orderby-unindexed").is_empty(),
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "soft-delete-bypass").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- enum-string-drift ---

#[test]
fn enum_string_drift_fires_on_non_member_literal_and_not_on_valid_member() {
    let dir = fixture();
    let out = analyze_tree(dir.path(), &EngineConfig::default());

    let found = hits(&out, "enum-string-drift");
    assert_eq!(found.len(), 1, "{:?}", out.findings);
    assert_eq!(found[0].file, "src/repo.ts");
    assert_eq!(found[0].line, ENUM_DRIFT_POSITIVE_LINE);
    assert_eq!(found[0].severity, zzop_core::Severity::Warning);
    assert_eq!(
        found[0].data.as_ref().unwrap()["model"].as_str(),
        Some("User")
    );
    assert_eq!(
        found[0].data.as_ref().unwrap()["field"].as_str(),
        Some("role")
    );
    assert_eq!(
        found[0]
            .data
            .as_ref()
            .unwrap()
            .get("params")
            .and_then(|p| p.get("literal"))
            .and_then(|v| v.as_str()),
        Some("BOGUS")
    );
    assert!(found[0].message.contains("BOGUS"));
    assert!(found[0].message.contains("disabled_rules"));
    // Never fires at the valid-member call site's line.
    assert!(!found.iter().any(|f| f.line == ENUM_DRIFT_NEGATIVE_LINE));
}

#[test]
fn enum_string_drift_disabled_via_config_removes_the_finding_but_leaves_siblings() {
    let dir = fixture();
    let config = EngineConfig {
        rule_config: RuleConfig {
            disabled_rules: vec!["enum-string-drift".to_string()],
            ..RuleConfig::default()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &config);
    assert!(
        hits(&out, "enum-string-drift").is_empty(),
        "{:?}",
        out.findings
    );
    // Per-id gating, not bundled: the sibling rules still run.
    assert_eq!(
        hits(&out, "soft-delete-bypass").len(),
        1,
        "{:?}",
        out.findings
    );
    assert_eq!(
        hits(&out, "orderby-unindexed").len(),
        1,
        "{:?}",
        out.findings
    );
}

// --- all three are whole-tree, non-per-file-cached passes: unaffected by an unrelated file's per-file cache ---

#[test]
fn all_three_rules_absent_when_no_prisma_schema_present() {
    let dir = TempDir::new("zzop-schema-join-none");
    dir.write("src/repo.ts", REPO_TS);
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(
        hits(&out, "soft-delete-bypass").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "orderby-unindexed").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "enum-string-drift").is_empty(),
        "{:?}",
        out.findings
    );
}

#[test]
fn all_three_rules_absent_when_no_matching_call_sites_exist() {
    let dir = TempDir::new("zzop-schema-join-nocalls");
    dir.write("prisma/schema.prisma", SCHEMA);
    dir.write("src/repo.ts", "export function noop() { return 1; }\n");
    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(
        hits(&out, "soft-delete-bypass").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "orderby-unindexed").is_empty(),
        "{:?}",
        out.findings
    );
    assert!(
        hits(&out, "enum-string-drift").is_empty(),
        "{:?}",
        out.findings
    );
}
