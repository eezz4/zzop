//! Prisma schema-usage analysis — usage-evidence collectors (per-file field-usage tokens, migration churn) plus the usage-aware cross-check layered on top of the structural analyzer in `structural.rs`.
//! `SchemaUsage` (the usage-evidence IR a producer assembles) lives in `zzop-core`; every function that consumes or produces it lives here. `analyze_schema_with_usage` wraps `structural::analyze_schema`
//! rather than modifying it, layering cross-check/churn issues and risk points on top. `structural::severity_points` is private to `structural.rs`, so it's duplicated here — keep the two in sync.
//!
//! `bound_models`/`identifier_counts` evidence used to come from this crate's own `<root>/src` filesystem re-walks (`scan_store_map`/`scan_field_usage`, both removed). Both are now sourced from per-file
//! facts carried through `zzop_engine`'s fused per-file pass instead: [`field_usage_tokens`] (this module) is the direct per-file substrate for `identifier_counts`, called once per file with the text
//! that pass already has in hand; the store-binding sibling (`extract_store_bound_models`) moved to `zzop_parser_typescript` since it needs the AST-based recognizer `db_table_consume.rs` already hosts.
//! Store/client-binding vocabulary (factory name "createStore" / getter name "getPrisma") is now a fixed literal at that call site rather than a caller-supplied parameter — this engine never had a second
//! vocabulary to plug in, so the parameterization the removed `scan_store_map` offered was unused generality.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use zzop_core::{SchemaModel, SchemaUsage, Severity};

use crate::structural::{analyze_schema, SchemaAnalysis, SchemaIssue};

macro_rules! lazy_re {
    ($f:ident, $p:expr) => {
        fn $f() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($p).unwrap())
        }
    };
}

// --- fieldUsageTokens (replaces the removed scanFieldUsage filesystem walk) ---

/// Comment/string-stripped identifier tokens referenced anywhere in one file's raw text — the direct
/// per-file substrate `zzop_engine`'s fused per-file pass now feeds into `SchemaUsage.identifier_counts`
/// (each file's set unioned tree-wide, then re-counted to presence — see that crate's `assemble`).
/// Replaces the removed `scan_field_usage`'s own `<root>/src` filesystem walk: same recognizer (plain
/// identifier tokens on comment/string-stripped text — common names like id/name appear everywhere, so
/// they're effectively never flagged dead, keeping false positives low at the cost of recall), just
/// invoked once per file instead of via a second full-tree walk. `rel` gates which files are worth
/// scanning at all (see [`is_field_usage_scan_file`]); an excluded file yields an empty set regardless of
/// `text`.
pub fn field_usage_tokens(rel: &str, text: &str) -> HashSet<String> {
    if !is_field_usage_scan_file(rel) {
        return HashSet::new();
    }
    let stripped = strip_comments_and_strings(text);
    ident_re()
        .find_iter(&stripped)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// `.ts`/`.tsx` only, excluding `.d.ts` declaration files — mirrors the removed `walk_ts_files`'s own
/// per-file filename filter. The old walk also hard-excluded `node_modules`/`dist`/`data` directories;
/// that exclusion isn't reproduced here since the fused per-file pass this now runs inside already skips
/// `node_modules`/`dist` under the DEFAULT `skip_dirs` (`EngineConfig`) — a subset of the old exclusions,
/// so under default config the fused pass covers every file the old `<root>/src` walk did plus more,
/// which only ADDS identifier evidence (the accepted tree-wide-widening deviation, see module doc) and
/// never adds a false dead-field positive. Caveat: a MORE-aggressive custom `skip_dirs` could exclude a
/// source dir the old walk scanned, dropping "used" tokens and potentially surfacing a false dead-field —
/// acceptable, since a user who scopes analysis away from a directory is opting out of its evidence.
fn is_field_usage_scan_file(rel: &str) -> bool {
    if rel.ends_with(".d.ts") {
        return false;
    }
    rel.ends_with(".ts") || rel.ends_with(".tsx")
}

fn strip_comments_and_strings(src: &str) -> String {
    let no_block = block_comment_re().replace_all(src, " ");
    let no_line = line_comment_re().replace_all(&no_block, "$1");
    let no_dq = double_quote_re().replace_all(&no_line, "\"\"");
    let no_sq = single_quote_re().replace_all(&no_dq, "''");
    template_re().replace_all(&no_sq, "``").into_owned()
}

lazy_re!(block_comment_re, r"(?s)/\*.*?\*/");
lazy_re!(line_comment_re, r"(?m)(^|[^:])//.*$");
lazy_re!(double_quote_re, r#""(?:\\.|[^"\\])*""#);
lazy_re!(single_quote_re, r"'(?:\\.|[^'\\])*'");
lazy_re!(template_re, r"`(?:\\.|[^`\\])*`");
// ASCII-only identifier token, mirroring JS `\b[a-zA-Z_$][\w$]*\b` (JS `\w` is ASCII-only).
lazy_re!(ident_re, r"[A-Za-z_$][A-Za-z0-9_$]*");

// --- scanMigrationChurn ---

/// Counts accumulated schema changes per model from `prisma/migrations/` history. Scans `{timestamp}_{name}/migration.sql` files under each domain's migrations directory for CREATE/ALTER/DROP TABLE
/// statements, tallies by table name, then maps to model names via `SchemaModel.table_name` or a snake_case fallback; returns an empty map when no migrations directory exists (e.g. a `db push` workflow).
pub fn scan_migration_churn(app_dir: &Path, models: &[SchemaModel]) -> HashMap<String, u32> {
    let table_counts = collect_table_counts(app_dir);
    map_tables_to_models(&table_counts, models)
}

fn collect_table_counts(app_dir: &Path) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    let domains_dir = app_dir.join("src").join("domains");
    let Ok(domains) = fs::read_dir(&domains_dir) else {
        return counts;
    };
    let mut domains: Vec<_> = domains.filter_map(Result::ok).collect();
    domains.sort_by_key(|e| e.file_name());
    for domain in domains {
        if !domain.path().is_dir() {
            continue;
        }
        let mig_dir = domain.path().join("prisma").join("migrations");
        let Ok(entries) = fs::read_dir(&mig_dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if !entry.path().is_dir() {
                continue;
            }
            let sql_path = entry.path().join("migration.sql");
            let Ok(sql) = fs::read_to_string(&sql_path) else {
                continue;
            };
            count_tables_in_sql(&sql, &mut counts);
        }
    }
    counts
}

// Matches CREATE/ALTER/DROP TABLE with optionally quoted names; covers Prisma-generated SQL, not full SQL DDL.
lazy_re!(
    table_re,
    r#"(?i)(?:CREATE|ALTER|DROP)\s+TABLE\s+(?:IF\s+(?:NOT\s+)?EXISTS\s+)?["`]?([A-Za-z0-9_.]+?)["`]?(?:\s|;|\()"#
);

fn count_tables_in_sql(sql: &str, counts: &mut HashMap<String, u32>) {
    for c in table_re().captures_iter(sql) {
        let table = strip_schema(&c[1]);
        *counts.entry(table).or_insert(0) += 1;
    }
}

fn map_tables_to_models(
    table_counts: &HashMap<String, u32>,
    models: &[SchemaModel],
) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for model in models {
        let key = model
            .table_name
            .clone()
            .unwrap_or_else(|| to_snake_case(&model.name));
        if let Some(&count) = table_counts.get(&key) {
            if count > 0 {
                out.insert(model.name.clone(), count);
            }
        }
    }
    out
}

fn strip_schema(raw: &str) -> String {
    match raw.rfind('.') {
        Some(idx) => raw[idx + 1..].to_string(),
        None => raw.to_string(),
    }
}

lazy_re!(snake_case_re, r"([a-z0-9])([A-Z])");

fn to_snake_case(pascal: &str) -> String {
    snake_case_re()
        .replace_all(pascal, "${1}_${2}")
        .to_lowercase()
}

// --- crossCheckSchema + applyChurnRule + analyzeSchema (usage branch) ---

const SKIP_FIELD_NAMES: [&str; 3] = ["id", "createdAt", "updatedAt"];
/// Very short field names appear everywhere in BE source; dead-field detection is meaningless -> exclude.
const MIN_FIELD_NAME_LEN: usize = 3;

/// Schema cross-check — compares the schema-IR against actual BE code usage. Surfaces dead-model (a model not bound to any store) and dead-field (a field never appearing as an identifier in BE source)
/// issues. id/createdAt/updatedAt are excluded by default since infrastructure fields are rarely referenced directly.
pub fn cross_check_schema(models: &[SchemaModel], usage: &SchemaUsage) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        if !usage.bound_models.contains(&model.name) {
            issues.push(SchemaIssue {
                rule: "dead-model".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: None,
                params: None,
            });
            continue;
        }
        for field in &model.fields {
            if SKIP_FIELD_NAMES.contains(&field.name.as_str()) {
                continue;
            }
            if field.name.len() < MIN_FIELD_NAME_LEN {
                continue;
            }
            if usage
                .identifier_counts
                .get(&field.name)
                .copied()
                .unwrap_or(0)
                > 0
            {
                continue;
            }
            issues.push(SchemaIssue {
                rule: "dead-field".to_string(),
                severity: Severity::Info,
                model: model.name.clone(),
                field: Some(field.name.clone()),
                params: None,
            });
        }
    }
    issues
}

const CHURN_WARNING_THRESHOLD: u32 = 5;
const CHURN_CRITICAL_THRESHOLD: u32 = 10;

/// schema-churn rule — detects design instability from accumulated migration churn on a model.
pub fn apply_churn_rule(models: &[SchemaModel], churn: &HashMap<String, u32>) -> Vec<SchemaIssue> {
    let mut issues = Vec::new();
    for model in models {
        let count = churn.get(&model.name).copied().unwrap_or(0);
        if count < CHURN_WARNING_THRESHOLD {
            continue;
        }
        let severity = if count >= CHURN_CRITICAL_THRESHOLD {
            Severity::Critical
        } else {
            Severity::Warning
        };
        issues.push(SchemaIssue {
            rule: "schema-churn".to_string(),
            severity,
            model: model.name.clone(),
            field: None,
            params: Some(serde_json::json!({ "count": count })),
        });
    }
    issues
}

/// Mirrors `structural::severity_points`, which is private to `structural.rs` (see module doc).
fn severity_points(s: Severity) -> i64 {
    match s {
        Severity::Critical => 5,
        Severity::Warning => 2,
        Severity::Info => 1,
    }
}

/// Usage-aware schema analysis: schema-IR (+ optional usage) -> `SchemaAnalysis` with a `model_risk` rollup. Always runs the structural rules; when `usage` is present, also runs `cross_check_schema` and
/// (if migration-churn data is available) `apply_churn_rule`, folding their risk points into `model_risk`.
pub fn analyze_schema_with_usage(
    models: Vec<SchemaModel>,
    usage: Option<SchemaUsage>,
) -> SchemaAnalysis {
    let mut analysis = analyze_schema(models);
    let Some(usage) = usage else {
        return analysis;
    };
    let mut extra = cross_check_schema(&analysis.models, &usage);
    if let Some(churn) = &usage.model_churn {
        extra.extend(apply_churn_rule(&analysis.models, churn));
    }
    for issue in &extra {
        *analysis.model_risk.entry(issue.model.clone()).or_insert(0) +=
            severity_points(issue.severity);
    }
    analysis.issues.extend(extra);
    analysis
}

#[cfg(test)]
mod tests {
    //! Unit tests for the usage-evidence collectors, the usage-aware cross-check, the churn rule, and
    //! `analyze_schema_with_usage`'s composition of structural + usage signals.
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use zzop_core::{FieldAttr, SchemaField};

    /// Self-cleaning temp directory (std-only, no `tempfile` dependency). Created fresh per test and removed
    /// on drop so tests don't leak directories between runs.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let dir =
                std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
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

    // --- fieldUsageTokens ---

    #[test]
    fn field_usage_tokens_collects_identifiers_from_one_file() {
        let result = field_usage_tokens(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function getPostTitle(post: any) {\n  return post.title;\n}\n",
        );
        assert!(result.contains("title"));
        assert!(result.contains("post"));
    }

    #[test]
    fn field_usage_tokens_dead_field_absent_when_never_referenced() {
        let result = field_usage_tokens(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function f(post: any) { return post.title; }\n",
        );
        assert!(!result.contains("deadField"));
    }

    #[test]
    fn field_usage_tokens_empty_for_a_d_ts_file() {
        let result = field_usage_tokens(
            "src/types/generated.d.ts",
            "export interface Generated { declarationOnlyFieldDEF: string; }\n",
        );
        assert!(result.is_empty());
    }

    #[test]
    fn field_usage_tokens_empty_for_a_js_file() {
        let result = field_usage_tokens(
            "src/domains/post/routes/helper.js",
            "const jsOnlyFieldGHI = 1; module.exports = { jsOnlyFieldGHI };\n",
        );
        assert!(result.is_empty());
    }

    #[test]
    fn field_usage_tokens_excludes_identifiers_inside_comments() {
        let result = field_usage_tokens(
            "src/domains/post/routes/createPostHandlers.ts",
            "// commentOnlyFieldJKL: this is a comment\n/* also commentOnlyFieldJKL */\nexport function f() { return 1; }\n",
        );
        assert!(!result.contains("commentOnlyFieldJKL"));
    }

    #[test]
    fn field_usage_tokens_excludes_identifiers_inside_string_literals() {
        let result = field_usage_tokens(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function f() {\n  const s = \"stringOnlyFieldMNO\";\n  const t = 'stringOnlyFieldMNO';\n  return s + t;\n}\n",
        );
        assert!(!result.contains("stringOnlyFieldMNO"));
    }

    #[test]
    fn field_usage_tokens_tsx_file_also_scanned() {
        let result = field_usage_tokens(
            "src/domains/post/PostCard.tsx",
            "export function PostCard(post: any) { return post.title; }\n",
        );
        assert!(result.contains("title"));
    }

    // --- scanMigrationChurn ---

    fn write_migration(dir: &TempDir, domain: &str, mig_name: &str, sql: &str) {
        dir.write(
            &format!("src/domains/{domain}/prisma/migrations/{mig_name}/migration.sql"),
            sql,
        );
    }

    fn churn_model(name: &str, table_name: Option<&str>) -> SchemaModel {
        SchemaModel {
            name: name.to_string(),
            table_name: table_name.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn churn_single_domain_create_table_once_maps_with_count_1() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "post",
            "20240101000000_init",
            "CREATE TABLE \"post\" (\n  \"id\" TEXT NOT NULL PRIMARY KEY,\n  \"title\" TEXT NOT NULL\n);",
        );
        let models = vec![churn_model("Post", Some("post"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("Post").copied(), Some(1));
    }

    #[test]
    fn churn_multiple_alter_table_on_same_table_accumulates() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "user",
            "20240101000000_init",
            "CREATE TABLE \"user\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        write_migration(
            &dir,
            "user",
            "20240201000000_add_email",
            "ALTER TABLE \"user\" ADD COLUMN \"email\" TEXT;",
        );
        write_migration(
            &dir,
            "user",
            "20240301000000_add_avatar",
            "ALTER TABLE \"user\" ADD COLUMN \"avatarUrl\" TEXT;",
        );
        let models = vec![churn_model("User", Some("user"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("User").copied(), Some(3));
    }

    #[test]
    fn churn_model_without_map_infers_snake_case_table_name() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "item",
            "20240101000000_init",
            "CREATE TABLE \"item_user_limit\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        let models = vec![churn_model("ItemUserLimit", None)];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("ItemUserLimit").copied(), Some(1));
    }

    #[test]
    fn churn_drop_table_also_counted() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "legacy",
            "20240101000000_create",
            "CREATE TABLE \"legacy_item\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        write_migration(
            &dir,
            "legacy",
            "20240201000000_drop",
            "DROP TABLE IF EXISTS \"legacy_item\";",
        );
        let models = vec![churn_model("LegacyItem", Some("legacy_item"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("LegacyItem").copied(), Some(2));
    }

    #[test]
    fn churn_multiple_domains_counted_independently() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "post",
            "20240101000000_init",
            "CREATE TABLE \"post\" (\"id\" TEXT NOT NULL PRIMARY KEY);\nALTER TABLE \"post\" ADD COLUMN \"slug\" TEXT;",
        );
        write_migration(
            &dir,
            "comment",
            "20240101000000_init",
            "CREATE TABLE \"comment\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        let models = vec![
            churn_model("Post", Some("post")),
            churn_model("Comment", Some("comment")),
        ];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("Post").copied(), Some(2));
        assert_eq!(result.get("Comment").copied(), Some(1));
    }

    #[test]
    fn churn_no_migrations_folder_empty_map() {
        let dir = TempDir::new("zzop-mig-churn");
        let models = vec![churn_model("Post", Some("post"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn churn_sql_does_not_reference_table_model_absent() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "post",
            "20240101000000_init",
            "CREATE TABLE \"unrelated_table\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        let models = vec![churn_model("Post", Some("post"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert!(!result.contains_key("Post"));
    }

    #[test]
    fn churn_empty_model_list_empty_map() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "post",
            "20240101000000_init",
            "CREATE TABLE \"post\" (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        let result = scan_migration_churn(dir.path(), &[]);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn churn_migration_folder_without_sql_skipped_without_error() {
        let dir = TempDir::new("zzop-mig-churn");
        dir.write(
            "src/domains/post/prisma/migrations/20240101000000_init/.gitkeep",
            "",
        );
        let models = vec![churn_model("Post", Some("post"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn churn_create_table_if_not_exists_also_counted() {
        let dir = TempDir::new("zzop-mig-churn");
        write_migration(
            &dir,
            "post",
            "20240101000000_init",
            "CREATE TABLE IF NOT EXISTS post (\"id\" TEXT NOT NULL PRIMARY KEY);",
        );
        let models = vec![churn_model("Post", Some("post"))];
        let result = scan_migration_churn(dir.path(), &models);
        assert_eq!(result.get("Post").copied(), Some(1));
    }

    // --- crossCheckSchema ---

    fn field(name: &str) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            r#type: "String".to_string(),
            optional: false,
            list: false,
            attrs: vec![],
        }
    }

    fn model(name: &str, field_names: &[&str]) -> SchemaModel {
        SchemaModel {
            name: name.to_string(),
            fields: field_names.iter().map(|n| field(n)).collect(),
            ..Default::default()
        }
    }

    fn usage(bound: &[&str], identifiers: &[(&str, u32)]) -> SchemaUsage {
        SchemaUsage {
            bound_models: bound.iter().map(|s| s.to_string()).collect(),
            identifier_counts: identifiers
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
            model_churn: None,
        }
    }

    #[test]
    fn cross_check_dead_model_no_store_binding_reported() {
        let issues = cross_check_schema(&[model("Orphan", &["id", "payload"])], &usage(&[], &[]));
        assert!(issues
            .iter()
            .any(|i| i.rule == "dead-model" && i.model == "Orphan"));
    }

    #[test]
    fn cross_check_dead_model_bound_model_not_reported() {
        let issues = cross_check_schema(
            &[model("User", &["id", "nickname"])],
            &usage(&["User"], &[("nickname", 5)]),
        );
        assert!(!issues.iter().any(|i| i.rule == "dead-model"));
    }

    #[test]
    fn cross_check_dead_field_zero_occurrences_reported() {
        let issues = cross_check_schema(
            &[model("User", &["id", "nickname", "ghostField"])],
            &usage(&["User"], &[("nickname", 3)]),
        );
        assert!(issues
            .iter()
            .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("ghostField")));
        assert!(!issues
            .iter()
            .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("nickname")));
    }

    #[test]
    fn cross_check_dead_field_excludes_id_created_updated_at() {
        let issues = cross_check_schema(
            &[model("X", &["id", "createdAt", "updatedAt", "name"])],
            &usage(&["X"], &[]),
        );
        let dead_fields: Vec<&str> = issues
            .iter()
            .filter(|i| i.rule == "dead-field")
            .map(|i| i.field.as_deref().unwrap())
            .collect();
        assert_eq!(dead_fields, vec!["name"]);
    }

    #[test]
    fn cross_check_dead_field_excludes_short_names() {
        let issues = cross_check_schema(&[model("Y", &["id", "ab", "name"])], &usage(&["Y"], &[]));
        assert!(!issues
            .iter()
            .any(|i| i.rule == "dead-field" && i.field.as_deref() == Some("ab")));
    }

    #[test]
    fn cross_check_dead_field_not_reported_when_parent_is_dead_model() {
        let issues =
            cross_check_schema(&[model("Q", &["id", "name", "payload"])], &usage(&[], &[]));
        assert_eq!(issues.iter().filter(|i| i.rule == "dead-field").count(), 0);
    }

    // --- applyChurnRule ---

    fn churn_map(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn churn_rule_at_least_5_is_warning() {
        let issues = apply_churn_rule(&[model("User", &["id"])], &churn_map(&[("User", 5)]));
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].severity, Severity::Warning);
    }

    #[test]
    fn churn_rule_at_least_10_is_critical() {
        let issues = apply_churn_rule(&[model("User", &["id"])], &churn_map(&[("User", 12)]));
        assert_eq!(issues[0].severity, Severity::Critical);
    }

    #[test]
    fn churn_rule_at_most_4_no_hit() {
        let issues = apply_churn_rule(&[model("User", &["id"])], &churn_map(&[("User", 4)]));
        assert_eq!(issues.len(), 0);
    }

    #[test]
    fn churn_rule_model_absent_from_churn_treated_as_zero() {
        let issues = apply_churn_rule(
            &[model("User", &["id"]), model("Item", &["id"])],
            &churn_map(&[("User", 6)]),
        );
        assert_eq!(
            issues.iter().map(|i| i.model.as_str()).collect::<Vec<_>>(),
            vec!["User"]
        );
    }

    #[test]
    fn churn_rule_empty_churn_map_no_issues() {
        let issues = apply_churn_rule(&[model("User", &["id"])], &HashMap::new());
        assert_eq!(issues.len(), 0);
    }

    // --- analyzeSchema (usage branch) ---

    fn risk_field(name: &str, optional: bool) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            r#type: "String".to_string(),
            optional,
            list: false,
            attrs: if name == "id" {
                vec![FieldAttr {
                    name: "id".to_string(),
                    args: None,
                }]
            } else {
                vec![]
            },
        }
    }

    fn risk_model(name: &str, field_names: &[&str]) -> SchemaModel {
        SchemaModel {
            name: name.to_string(),
            fields: field_names.iter().map(|n| risk_field(n, false)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn analyze_with_usage_structural_only_model_risk_matches_summed_points() {
        let analysis =
            analyze_schema_with_usage(vec![risk_model("P", &["id", "userId", "content"])], None);
        assert!(analysis.model_risk["P"] > 0);
        let expected: i64 = analysis
            .issues
            .iter()
            .filter(|i| i.model == "P")
            .map(|i| severity_points(i.severity))
            .sum();
        assert_eq!(analysis.model_risk["P"], expected);
    }

    #[test]
    fn analyze_with_usage_every_model_gets_model_risk_entry_even_zero_issues() {
        let analysis = analyze_schema_with_usage(vec![risk_model("Lookup", &["id", "code"])], None);
        assert_eq!(analysis.model_risk["Lookup"], 0);
    }

    #[test]
    fn analyze_with_usage_signals_add_dead_model_field_and_churn_issues() {
        let analysis = analyze_schema_with_usage(
            vec![risk_model("Ghost", &["id", "secretField"])],
            Some(SchemaUsage {
                bound_models: HashSet::new(),
                identifier_counts: HashMap::new(),
                model_churn: Some(churn_map(&[("Ghost", 12)])),
            }),
        );
        // Ghost is unbound -> dead-model; churn 12 -> schema-churn critical. dead-field is skipped under dead-model.
        assert!(analysis.issues.iter().any(|i| i.rule == "dead-model"));
        assert!(analysis
            .issues
            .iter()
            .any(|i| i.rule == "schema-churn" && i.severity == Severity::Critical));
    }

    #[test]
    fn analyze_with_usage_no_usage_runs_only_structural_rules() {
        let analysis =
            analyze_schema_with_usage(vec![risk_model("Orphan", &["id", "payload"])], None);
        assert!(!analysis.issues.iter().any(|i| i.rule == "dead-model"));
    }
}
