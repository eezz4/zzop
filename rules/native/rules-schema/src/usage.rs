//! Prisma schema-usage analysis — usage-evidence collectors (source scan, migration churn, store bindings) plus the usage-aware cross-check layered on top of the structural analyzer in `structural.rs`.
//! `SchemaUsage` (the usage-evidence IR a producer assembles) lives in `zzop-core`; every function that consumes or produces it lives here. `analyze_schema_with_usage` wraps `structural::analyze_schema`
//! rather than modifying it, layering cross-check/churn issues and risk points on top. `structural::severity_points` is private to `structural.rs`, so it's duplicated here — keep the two in sync.
//!
//! The store/client-binding vocabulary (factory names like "createStore"/"getPrisma") isn't read from an ambient registry; `scan_store_map` takes `store_factory_fn` / `prisma_client_getter_fn` as explicit
//! parameters instead, with default values owned by the parser-prisma orchestrator that uses them.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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

// --- scanFieldUsage ---

/// Collects field-name occurrence counts across BE source files as evidence for dead-field detection. Matches
/// plain identifier tokens on comment/string-stripped text; common names (id/name) appear everywhere, so
/// they're effectively never flagged dead — this keeps false positives low at the cost of recall.
pub fn scan_field_usage(app_dir: &Path) -> HashMap<String, u32> {
    let mut counts = HashMap::new();
    let src_dir = app_dir.join("src");
    if !src_dir.is_dir() {
        return counts;
    }
    for file in walk_ts_files(&src_dir) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let stripped = strip_comments_and_strings(&text);
        for token in ident_re().find_iter(&stripped) {
            *counts.entry(token.as_str().to_string()).or_insert(0) += 1;
        }
    }
    counts
}

/// `pub(crate)`: shared with `crate::join`'s `scan_query_call_sites`, which uses the same `.ts`/`.tsx` walk.
pub(crate) fn walk_ts_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_ts_files_into(dir, &mut out);
    out
}

fn walk_ts_files_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "node_modules" || name == "dist" || name == "data" {
                continue;
            }
            walk_ts_files_into(&path, out);
        } else if is_scannable_ts_file(&name) {
            out.push(path);
        }
    }
}

/// `.ts`/`.tsx` only, excluding `.d.ts` declaration files.
fn is_scannable_ts_file(name: &str) -> bool {
    if name.ends_with(".d.ts") {
        return false;
    }
    name.ends_with(".ts") || name.ends_with(".tsx")
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

// --- scanStoreMap ---

/// Extracts `{storeName -> Prisma model name}` mappings from STORES files in a BE app. Pattern: `<storeName>: <storeFactoryFn>(..., () => new PrismaStore(<prismaClientGetterFn>().<client>))`, where
/// `client` is a camelCase accessor mapped to a model name by uppercasing its first letter. `store_factory_fn`/`prisma_client_getter_fn` are producer-supplied vocabulary (see module doc); an empty map
/// is returned if either is empty.
pub fn scan_store_map(
    app_dir: &Path,
    store_factory_fn: &str,
    prisma_client_getter_fn: &str,
) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if store_factory_fn.is_empty() || prisma_client_getter_fn.is_empty() {
        return result;
    }
    let domains_dir = app_dir.join("src").join("domains");
    let Ok(domains) = fs::read_dir(&domains_dir) else {
        return result;
    };
    let escaped_factory = regex::escape(store_factory_fn);
    let escaped_getter = regex::escape(prisma_client_getter_fn);
    // E.g. `itemStore: createStore(..., () => new PrismaStore(getPrisma().item))`.
    let factory_re = Regex::new(&format!(
        r"([A-Za-z0-9_]+):\s*{escaped_factory}[\s\S]*?{escaped_getter}\(\)\.([A-Za-z0-9_]+)"
    ))
    .unwrap();
    // Standalone variant: `export const userStore = new PrismaStore(getPrisma().user)`.
    let standalone_re = Regex::new(&format!(
        r"(?:const|let)\s+([A-Za-z0-9_]+Store)\s*=[\s\S]*?{escaped_getter}\(\)\.([A-Za-z0-9_]+)"
    ))
    .unwrap();

    let mut domains: Vec<_> = domains.filter_map(Result::ok).collect();
    domains.sort_by_key(|e| e.file_name());
    for domain in domains {
        if !domain.path().is_dir() {
            continue;
        }
        scan_domain_stores(&domain.path(), &mut result, &factory_re, &standalone_re);
    }
    result
}

lazy_re!(store_file_re, r"STORES?\.ts$|[Ss]tore\.ts$");

fn scan_domain_stores(
    domain_dir: &Path,
    out: &mut HashMap<String, String>,
    factory_re: &Regex,
    standalone_re: &Regex,
) {
    let Ok(entries) = fs::read_dir(domain_dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !store_file_re().is_match(&name) {
            continue;
        }
        parse_store_file(&path, out, factory_re, standalone_re);
    }
}

fn parse_store_file(
    file: &Path,
    out: &mut HashMap<String, String>,
    factory_re: &Regex,
    standalone_re: &Regex,
) {
    let Ok(text) = fs::read_to_string(file) else {
        return;
    };
    for c in factory_re.captures_iter(&text) {
        out.insert(c[1].to_string(), capitalize(&c[2]));
    }
    for c in standalone_re.captures_iter(&text) {
        out.entry(c[1].to_string())
            .or_insert_with(|| capitalize(&c[2]));
    }
}

/// `pub(crate)`: shared with `crate::join`'s `scan_query_call_sites` for the same camelCase-to-PascalCase mapping.
pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
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
    use std::collections::HashSet;
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

    // --- scanFieldUsage ---

    #[test]
    fn field_usage_empty_map_when_src_dir_missing() {
        let dir = TempDir::new("zzop-field-usage");
        let result = scan_field_usage(dir.path());
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn field_usage_collects_identifier_counts_from_ts_files_under_src() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function getPostTitle(post: any) {\n  return post.title;\n}\n",
        );
        let result = scan_field_usage(dir.path());
        assert!(result.get("title").copied().unwrap_or(0) > 0);
        assert!(result.get("post").copied().unwrap_or(0) > 0);
    }

    #[test]
    fn field_usage_merges_multiple_files_same_identifier_counts_accumulate() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function a(post: any) { return post.title; }\n",
        );
        dir.write(
            "src/domains/user/routes/createUserHandlers.ts",
            "export function b(post: any) { return post.title + \"!\"; }\n",
        );
        let result = scan_field_usage(dir.path());
        assert!(result.get("title").copied().unwrap_or(0) >= 2);
    }

    #[test]
    fn field_usage_dead_field_with_zero_usages_absent_or_zero() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function f(post: any) { return post.title; }\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("deadField").copied().unwrap_or(0), 0);
    }

    #[test]
    fn field_usage_multiple_fields_appear_at_correct_relative_frequencies() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/order/routes/createOrderHandlers.ts",
            "export function f(o: any) {\n  const a = o.status;\n  const b = o.status;\n  const c = o.amount;\n  return { a, b, c };\n}\n",
        );
        let result = scan_field_usage(dir.path());
        let status_count = result.get("status").copied().unwrap_or(0);
        let amount_count = result.get("amount").copied().unwrap_or(0);
        assert!(status_count >= 2);
        assert!(amount_count >= 1);
        assert!(status_count > amount_count);
    }

    #[test]
    fn field_usage_excludes_node_modules_directory() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/node_modules/some-lib/index.ts",
            "export const superRareFieldXYZ = 1;\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("superRareFieldXYZ").copied().unwrap_or(0), 0);
    }

    #[test]
    fn field_usage_excludes_dist_directory() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/dist/output.ts",
            "export const compiledOnlyFieldABC = 1;\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("compiledOnlyFieldABC").copied().unwrap_or(0), 0);
    }

    #[test]
    fn field_usage_excludes_d_ts_files() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/types/generated.d.ts",
            "export interface Generated { declarationOnlyFieldDEF: string; }\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(
            result.get("declarationOnlyFieldDEF").copied().unwrap_or(0),
            0
        );
    }

    #[test]
    fn field_usage_excludes_js_files() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/helper.js",
            "const jsOnlyFieldGHI = 1; module.exports = { jsOnlyFieldGHI };\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("jsOnlyFieldGHI").copied().unwrap_or(0), 0);
    }

    #[test]
    fn field_usage_excludes_identifiers_inside_comments() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "// commentOnlyFieldJKL: this is a comment\n/* also commentOnlyFieldJKL */\nexport function f() { return 1; }\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("commentOnlyFieldJKL").copied().unwrap_or(0), 0);
    }

    #[test]
    fn field_usage_excludes_identifiers_inside_string_literals() {
        let dir = TempDir::new("zzop-field-usage");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function f() {\n  const s = \"stringOnlyFieldMNO\";\n  const t = 'stringOnlyFieldMNO';\n  return s + t;\n}\n",
        );
        let result = scan_field_usage(dir.path());
        assert_eq!(result.get("stringOnlyFieldMNO").copied().unwrap_or(0), 0);
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

    // --- scanStoreMap ---

    const FACTORY: &str = "createStore";
    const GETTER: &str = "getPrisma";

    #[test]
    fn store_map_stores_ts_pattern() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/item/STORES.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  itemStore: createStore(\n    (filters: any) => filters,\n    () => new PrismaStore(getPrisma().item),\n  ),\n};\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(result.get("itemStore").map(String::as_str), Some("Item"));
    }

    #[test]
    fn store_map_compound_camel_case_client_to_pascal_case_model() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/item/STORES.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  itemUserLimitStore: createStore(\n    (f: any) => f,\n    () => new PrismaStore(getPrisma().itemUserLimit),\n  ),\n};\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(
            result.get("itemUserLimitStore").map(String::as_str),
            Some("ItemUserLimit")
        );
    }

    #[test]
    fn store_map_standalone_const_pattern() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/user/store.ts",
            "import { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const userStore = new PrismaStore(getPrisma().user);\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(result.get("userStore").map(String::as_str), Some("User"));
    }

    #[test]
    fn store_map_multiple_domain_files_collected_independently() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/post/STORES.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  postStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().post)),\n  postLikeStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().postLike)),\n};\n",
        );
        dir.write(
            "src/domains/comment/STORES.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  commentStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().comment)),\n};\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(result.get("postStore").map(String::as_str), Some("Post"));
        assert_eq!(
            result.get("postLikeStore").map(String::as_str),
            Some("PostLike")
        );
        assert_eq!(
            result.get("commentStore").map(String::as_str),
            Some("Comment")
        );
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn store_map_store_ts_mixed_case_variant_is_scanned() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/session/Store.ts",
            "import { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const sessionStore = new PrismaStore(getPrisma().session);\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(
            result.get("sessionStore").map(String::as_str),
            Some("Session")
        );
    }

    #[test]
    fn store_map_store_ts_singular_is_scanned() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/notification/STORE.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  notificationStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().notification)),\n};\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(
            result.get("notificationStore").map(String::as_str),
            Some("Notification")
        );
    }

    #[test]
    fn store_map_no_domains_folder_empty_map() {
        let dir = TempDir::new("zzop-store-map");
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn store_map_domain_without_stores_file_empty_map() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/post/routes/createPostHandlers.ts",
            "export function handler() { return null; }",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn store_map_file_without_get_prisma_pattern_not_mapped() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/post/STORES.ts",
            "import { JsonStore } from \"@app/json-store\";\nexport const STORES = {\n  postStore: new JsonStore(\"posts\"),\n};\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        assert!(!result.contains_key("postStore"));
    }

    #[test]
    fn store_map_same_store_name_twice_first_entry_wins() {
        let dir = TempDir::new("zzop-store-map");
        dir.write(
            "src/domains/post/STORES.ts",
            "import { createStore } from \"@app/store\";\nimport { PrismaStore, getPrisma } from \"@app/prisma\";\nexport const STORES = {\n  postStore: createStore((f: any) => f, () => new PrismaStore(getPrisma().post)),\n};\nexport const postStore = new PrismaStore(getPrisma().article);\n",
        );
        let result = scan_store_map(dir.path(), FACTORY, GETTER);
        // createStore pattern (post) is registered first; standalone entry is ignored since key already exists.
        assert_eq!(result.get("postStore").map(String::as_str), Some("Post"));
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
