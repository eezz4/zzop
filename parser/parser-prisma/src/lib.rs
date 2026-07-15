//! zzop-parser-prisma — Prisma Schema Language (PSL) frontend. Line-based parser turning schema.prisma
//! into the core schema IR (`SchemaModel[]`) — a grammar the TypeScript parser does not handle. Extracts
//! model blocks, field declarations, field attributes, and @@map/@@unique/@@index. `parse_schema_enums` is
//! a separate top-level pass extracting `enum` blocks into `SchemaEnum[]`, kept out of `parse_schema`'s
//! return shape so existing call sites stay untouched; a caller that also needs enum data calls both.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Cache key ingredient for `zzop-cache` (see `zzop_parser_typescript::PARSER_FINGERPRINT`'s doc for the
/// scheme this mirrors). This crate has no external version pin to track (the parser is a local regex/line
/// scanner, not a wrapped third-party crate) — bump the trailing `/vN` counter whenever `parse_schema`'s
/// projection logic changes in a way that changes `SchemaModel`/`SourceSymbol` output for the same schema
/// text.
pub const PARSER_FINGERPRINT: &str = "prisma/v1";

use regex::Regex;
use zzop_core::{FieldAttr, SchemaEnum, SchemaField, SchemaModel};
use zzop_rules_schema::{analyze_schema, SchemaAnalysis};

/// Parse a schema.prisma string into models. `source_path`/`domain` tag each model (multi-file merge).
pub fn parse_schema(
    text: &str,
    source_path: Option<&str>,
    domain: Option<&str>,
) -> Vec<SchemaModel> {
    let normalized = normalize_braces(&strip_comments(text));
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut models = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(c) = re_model().captures(lines[i]) {
            let (model, next) = parse_model_block(&c[1], &lines, i + 1, source_path, domain);
            models.push(model);
            i = next;
        } else {
            i += 1;
        }
    }
    models
}

/// Parse a schema.prisma string into its declared `enum` blocks. Comment/whitespace tolerant (same
/// `strip_comments`/`normalize_braces` preprocessing `parse_schema` uses); `@@map(...)` block-attribute
/// lines are recognized and ignored (an enum's own DB-level rename has no bearing on its member list);
/// members are returned in declaration order, one or more per source line (tokenized on whitespace, so
/// both the common one-member-per-line style and a compact `enum Role { USER ADMIN }` single-line form
/// parse the same way).
pub fn parse_schema_enums(text: &str) -> Vec<SchemaEnum> {
    let normalized = normalize_braces(&strip_comments(text));
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut enums = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(c) = re_enum().captures(lines[i]) {
            let name = c[1].to_string();
            let (members, next) = parse_enum_block(&lines, i + 1);
            let line = enum_decl_line(text, &name);
            enums.push(SchemaEnum {
                name,
                members,
                line,
            });
            i = next;
        } else {
            i += 1;
        }
    }
    enums
}

fn parse_enum_block(lines: &[&str], start: usize) -> (Vec<String>, usize) {
    let mut members = Vec::new();
    let mut i = start;
    while i < lines.len() && !re_close().is_match(lines[i]) {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() || line.starts_with("@@") {
            continue;
        }
        for tok in line.split_whitespace() {
            if let Some(c) = re_enum_member().captures(tok) {
                members.push(c[1].to_string());
            }
        }
    }
    (members, i + 1)
}

/// 1-based line of `enum <name> {` in the (raw, un-normalized) schema text — the enum-block counterpart to
/// `model_decl_line` (same lexical-scan technique, over the same kind of source text).
fn enum_decl_line(text: &str, name: &str) -> u32 {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("enum ") {
            if rest.trim_start().starts_with(name)
                && rest.trim_start()[name.len()..]
                    .trim_start()
                    .starts_with('{')
            {
                return (i + 1) as u32;
            }
        }
    }
    1
}

fn parse_model_block(
    name: &str,
    lines: &[&str],
    start: usize,
    source_path: Option<&str>,
    domain: Option<&str>,
) -> (SchemaModel, usize) {
    let mut fields = Vec::new();
    let mut uniques = Vec::new();
    let mut indexes = Vec::new();
    let mut table_name = None;
    let mut i = start;
    while i < lines.len() && !re_close().is_match(lines[i]) {
        let line = lines[i].trim();
        i += 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with("@@") {
            apply_block_attr(line, &mut table_name, &mut uniques, &mut indexes);
            continue;
        }
        if let Some(field) = parse_field(line) {
            fields.push(field);
        }
    }
    let model = SchemaModel {
        name: name.to_string(),
        table_name,
        fields,
        uniques,
        indexes,
        domain: domain.map(str::to_string),
        source_path: source_path.map(str::to_string),
    };
    (model, i + 1)
}

fn apply_block_attr(
    line: &str,
    table_name: &mut Option<String>,
    uniques: &mut Vec<Vec<String>>,
    indexes: &mut Vec<Vec<String>>,
) {
    if let Some(c) = re_map().captures(line) {
        *table_name = Some(c[1].to_string());
    } else if let Some(c) = re_unique().captures(line) {
        uniques.push(split_list(&c[1]));
    } else if let Some(c) = re_index().captures(line) {
        indexes.push(split_list(&c[1]));
    }
}

fn parse_field(line: &str) -> Option<SchemaField> {
    let c = re_field().captures(line)?;
    let modifier = c.get(3).map(|m| m.as_str()).unwrap_or("");
    Some(SchemaField {
        name: c[1].to_string(),
        r#type: c[2].to_string(),
        optional: modifier == "?",
        list: modifier == "[]",
        attrs: parse_attrs(&c[4]),
    })
}

fn parse_attrs(rest: &str) -> Vec<FieldAttr> {
    re_attr()
        .captures_iter(rest)
        .map(|c| FieldAttr {
            name: c[1].to_string(),
            args: c.get(3).map(|m| m.as_str().to_string()),
        })
        .collect()
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn strip_comments(text: &str) -> String {
    let no_block = re_block_comment().replace_all(text, "");
    re_line_comment().replace_all(&no_block, "$1").to_string()
}

/// Insert newlines around `{` and `}` so they are always on their own lines.
fn normalize_braces(text: &str) -> String {
    text.replace('{', "{\n").replace('}', "\n}\n")
}

macro_rules! lazy_re {
    ($f:ident, $p:expr) => {
        fn $f() -> &'static Regex {
            static R: OnceLock<Regex> = OnceLock::new();
            R.get_or_init(|| Regex::new($p).unwrap())
        }
    };
}

lazy_re!(re_model, r"^\s*model\s+(\w+)\s*\{");
lazy_re!(re_enum, r"^\s*enum\s+(\w+)\s*\{");
lazy_re!(re_enum_member, r"^([A-Za-z_]\w*)");
lazy_re!(re_close, r"^\s*\}");
lazy_re!(re_map, r#"^@@map\s*\(\s*"([^"]+)"\s*\)"#);
lazy_re!(re_unique, r"^@@unique\s*\(\s*\[([^\]]+)\]");
lazy_re!(re_index, r"^@@index\s*\(\s*\[([^\]]+)\]");
lazy_re!(re_field, r"^(\w+)\s+(\w+)(\?|\[\])?\s*(.*)$");
lazy_re!(re_attr, r"@(\w+)(\(([^)]*)\))?");
lazy_re!(re_block_comment, r"(?s)/\*.*?\*/");
lazy_re!(re_line_comment, r"(?m)(^|[^:])//.*$");

// ---------------------------------------------------------------------------------------------
// find_prisma_schemas — generic schema.prisma discovery.
// ---------------------------------------------------------------------------------------------

const SKIP_DIRS: [&str; 4] = ["node_modules", "dist", "build", "coverage"];

/// Collect + parse every `schema.prisma` under `app_dir` into a flat `SchemaModel` list. `domain` is set to the
/// directory name above the schema file when it sits under a `prisma/` folder (best-effort grouping), else `None`.
/// Convention-based (not a specific repo layout): finds the conventional `prisma/schema.prisma` and any other
/// `schema.prisma` under the tree (multi-file / domain-split schemas), skipping node_modules/dist/dot-dirs.
pub fn find_prisma_schemas(app_dir: &Path) -> Vec<SchemaModel> {
    let mut models = Vec::new();
    for file in walk_schema_files(app_dir) {
        let Ok(text) = fs::read_to_string(&file) else {
            continue;
        };
        let rel = relative_slash_path(app_dir, &file);
        let domain = domain_of(&file);
        models.extend(parse_schema(&text, Some(&rel), domain.as_deref()));
    }
    models
}

fn walk_schema_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_schema_files_into(dir, &mut out);
    out
}

fn walk_schema_files_into(dir: &Path, out: &mut Vec<PathBuf>) {
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
            if SKIP_DIRS.contains(&name.as_ref()) || name.starts_with('.') {
                continue;
            }
            walk_schema_files_into(&path, out);
        } else if path.is_file() && name == "schema.prisma" {
            out.push(path);
        }
    }
}

/// Domain hint: the dir name enclosing the `prisma/` folder (e.g. `.../domains/billing/prisma/schema.prisma` ->
/// "billing"); `None` when the schema is not under a `prisma/` dir.
fn domain_of(file: &Path) -> Option<String> {
    let parent = file.parent()?.file_name()?.to_str()?;
    if parent != "prisma" {
        return None;
    }
    let grandparent = file.parent()?.parent()?.file_name()?.to_str()?;
    if grandparent.is_empty() {
        None
    } else {
        Some(grandparent.to_string())
    }
}

fn relative_slash_path(base: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(base).unwrap_or(file);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------------------------
// prismaSchemaAnalysis — thin orchestrator: schema.prisma on disk -> core structural/DB-pattern analysis.
// ---------------------------------------------------------------------------------------------

/// Public Prisma schema analysis — the bundled provider's `schemaAnalysis` capability. Discovers schema.prisma
/// files, parses them to the schema-IR, and runs `zzop_rules_schema::analyze_schema` for the STRUCTURAL +
/// DB-anti-pattern rules (god-model, fk-no-index, float-money, ...). No code-usage scan, so usage-based rules
/// (dead-model / dead-field / schema-churn) do NOT run here — that requires the richer
/// `prisma_schema_analysis_with_usage`.
/// Honest by omission: returns `None` when not a BE target or no schema is found.
///
/// Design note: a `phase(name, fn)` tracing callback wrapping each step was considered and dropped —
/// pure instrumentation with no effect on output (see `zzop_rules_schema::usage`'s module doc).
pub fn prisma_schema_analysis(app_dir: &Path, target: &str) -> Option<SchemaAnalysis> {
    if target != "be" && target != "all" {
        return None;
    }
    let models = find_prisma_schemas(app_dir);
    if models.is_empty() {
        return None;
    }
    Some(analyze_schema(models))
}

/// The standard Prisma-client accessor name the `db-table` consume recognizer keys off
/// (`zzop_parser_typescript::adapters::db_table_consume`) — a common-Prisma idiom, shared as one literal.
pub const DEFAULT_PRISMA_CLIENT_GETTER_FN: &str = "getPrisma";

/// Project schema.prisma files into a `CommonIr` — the parser -> engine bridge (mirrors the
/// parser-typescript `build_common_ir` shape). Each model becomes an exported `SourceSymbol`
/// (kind = Class: a model is the closest thing PSL has to a data-shape declaration), so schema
/// entities join the same symbol space the engine and cross-layer passes consume. PSL has no
/// imports, so `dep` stays empty; `loc` counts non-blank/non-comment lines per schema file. This
/// bridge is a deliberate addition beyond schema analysis alone, so schema entities can
/// participate in cross-layer joins that key off Common IR symbols.
pub fn build_common_ir(source_id: &str, files: &[(String, String)]) -> zzop_core::CommonIr {
    let mut symbols = Vec::new();
    let mut loc = std::collections::HashMap::new();
    for (rel, text) in files {
        let models = parse_schema(text, Some(rel), None);
        for m in &models {
            symbols.push(zzop_core::SourceSymbol {
                id: format!("{rel}#{}", m.name),
                file: rel.clone(),
                name: m.name.clone(),
                kind: zzop_core::SourceSymbolKind::Class,
                line: model_decl_line(text, &m.name),
                exported: true,
                is_default: false,
                body_start: None,
                body_end: None,
                write_sites: Vec::new(),
            });
        }
        loc.insert(rel.clone(), count_schema_loc(text));
    }
    zzop_core::CommonIr {
        source: source_id.to_string(),
        parser: "prisma".to_string(),
        ir: zzop_core::MinimalIr {
            dep: std::collections::HashMap::new(),
            symbols,
            loc,
            io: None,
        },
    }
}

/// 1-based line of `model <name> {` in the schema text (lexical; parse_schema does not record lines).
/// `pub`: `zzop_engine`'s per-file Prisma pass (`schema_issue_to_finding`) reuses this to place a
/// `SchemaIssue`-derived `Finding` at the issue's model's declaration line, rather than duplicating the
/// same lexical lookup.
pub fn model_decl_line(text: &str, name: &str) -> u32 {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("model ") {
            if rest.trim_start().starts_with(name)
                && rest.trim_start()[name.len()..]
                    .trim_start()
                    .starts_with('{')
            {
                return (i + 1) as u32;
            }
        }
    }
    1
}

/// Non-blank, non-`//`-comment schema lines.
fn count_schema_loc(text: &str) -> u32 {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .count() as u32
}

#[cfg(test)]
mod tests {
    //! Coverage for `parse_schema`: model blocks, field declarations/attributes, and
    //! @@map/@@unique/@@index block attributes.
    use super::*;

    const SAMPLE: &str = r#"
// user table
model User {
  id        String   @id @default(uuid(7))
  loginId   String   @unique @map("login_id")
  nickname  String
  createdAt DateTime @default(now()) @map("created_at")

  @@map("users")
}

model Item {
  id      String @id
  ownerId String @map("owner_id")
  name    String
  tags    String[]

  @@unique([ownerId, name])
  @@index([name])
  @@map("items")
}
"#;

    fn parse(text: &str) -> Vec<SchemaModel> {
        parse_schema(text, None, None)
    }

    #[test]
    fn extracts_model_names() {
        let models = parse(SAMPLE);
        assert_eq!(
            models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["User", "Item"]
        );
    }

    #[test]
    fn parses_field_type_and_list_flag() {
        let models = parse(SAMPLE);
        let item = models.iter().find(|m| m.name == "Item").unwrap();
        let tags = item.fields.iter().find(|f| f.name == "tags").unwrap();
        assert_eq!(tags.r#type, "String");
        assert!(tags.list);
    }

    #[test]
    fn parses_field_attributes() {
        let models = parse(SAMPLE);
        let user = models.iter().find(|m| m.name == "User").unwrap();
        let login = user.fields.iter().find(|f| f.name == "loginId").unwrap();
        assert!(login.attrs.iter().any(|a| a.name == "unique"));
        assert_eq!(
            login
                .attrs
                .iter()
                .find(|a| a.name == "map")
                .and_then(|a| a.args.as_deref()),
            Some(r#""login_id""#)
        );
    }

    #[test]
    fn parses_block_attributes() {
        let models = parse(SAMPLE);
        let item = models.iter().find(|m| m.name == "Item").unwrap();
        assert_eq!(item.table_name.as_deref(), Some("items"));
        assert_eq!(
            item.uniques,
            vec![vec!["ownerId".to_string(), "name".to_string()]]
        );
        assert_eq!(item.indexes, vec![vec!["name".to_string()]]);
    }

    #[test]
    fn ignores_comment_lines() {
        let models = parse("// comment\nmodel A { id String @id }\n");
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].fields.len(), 1);
    }

    #[test]
    fn parse_then_analyze_vertical_slice() {
        // Vertical slice: schema.prisma -> parse_schema -> analyze_schema -> issues.
        use zzop_rules_schema::analyze_schema;
        let analysis = analyze_schema(parse(SAMPLE));
        // Item.ownerId is an implicit FK (no @relation).
        assert!(analysis
            .issues
            .iter()
            .any(|i| i.rule == "implicit-fk" && i.field.as_deref() == Some("ownerId")));
        // User has createdAt but no updatedAt.
        assert!(analysis
            .issues
            .iter()
            .any(|i| i.rule == "missing-timestamps" && i.model == "User"));
    }

    // --- parseSchemaEnums ---

    const ENUM_SAMPLE: &str = r#"
enum Role {
  USER
  ADMIN
  // internal-only, not yet exposed
  SUPPORT
}

model User {
  id   String @id
  role Role   @default(USER)
}

enum Status {
  ACTIVE @map("active")
  ARCHIVED
}

// @@map on an enum only renames its DB-level type, not its members.
enum Priority {
  LOW MEDIUM HIGH

  @@map("priority_level")
}
"#;

    #[test]
    fn parse_schema_enums_extracts_enum_names_in_order() {
        let enums = parse_schema_enums(ENUM_SAMPLE);
        assert_eq!(
            enums.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["Role", "Status", "Priority"]
        );
    }

    #[test]
    fn parse_schema_enums_extracts_members_in_declaration_order() {
        let enums = parse_schema_enums(ENUM_SAMPLE);
        let role = enums.iter().find(|e| e.name == "Role").unwrap();
        assert_eq!(role.members, vec!["USER", "ADMIN", "SUPPORT"]);
    }

    #[test]
    fn parse_schema_enums_is_comment_tolerant() {
        // Role's SUPPORT member is preceded by a `//` comment line — must not be dropped or
        // mistaken for a member.
        let enums = parse_schema_enums(ENUM_SAMPLE);
        let role = enums.iter().find(|e| e.name == "Role").unwrap();
        assert!(role.members.contains(&"SUPPORT".to_string()));
        assert_eq!(role.members.len(), 3);
    }

    #[test]
    fn parse_schema_enums_ignores_member_level_map_attribute_text() {
        // "ACTIVE @map(\"active\")" must yield the member "ACTIVE" only, not a second bogus member
        // parsed out of the attribute text.
        let enums = parse_schema_enums(ENUM_SAMPLE);
        let status = enums.iter().find(|e| e.name == "Status").unwrap();
        assert_eq!(status.members, vec!["ACTIVE", "ARCHIVED"]);
    }

    #[test]
    fn parse_schema_enums_ignores_block_map_attribute_line() {
        // Priority's `@@map("priority_level")` line must not be read as members, and the
        // single-line multi-member form must still tokenize correctly.
        let enums = parse_schema_enums(ENUM_SAMPLE);
        let priority = enums.iter().find(|e| e.name == "Priority").unwrap();
        assert_eq!(priority.members, vec!["LOW", "MEDIUM", "HIGH"]);
    }

    #[test]
    fn parse_schema_enums_records_declaration_line() {
        let enums = parse_schema_enums(ENUM_SAMPLE);
        let role = enums.iter().find(|e| e.name == "Role").unwrap();
        // `enum Role {` sits on line 2 of ENUM_SAMPLE (leading newline is line 1).
        assert_eq!(role.line, 2);
    }

    #[test]
    fn parse_schema_enums_returns_empty_for_schema_with_no_enum() {
        assert!(parse_schema_enums(SAMPLE).is_empty());
    }

    #[test]
    fn parse_schema_enums_does_not_affect_model_parsing() {
        // A schema mixing model + enum blocks parses both independently: `parse_schema` still sees
        // only the model.
        let models = parse(ENUM_SAMPLE);
        assert_eq!(
            models.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["User"]
        );
    }
}

#[cfg(test)]
mod orchestrator_tests {
    //! End-to-end coverage: schema.prisma files on disk -> core analysis (both the schema-only
    //! and usage-combined paths).
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

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
        let schema = "\n// user table\nmodel User {\n  id String @id\n}\n\nmodel Post {\n  id String @id\n}\n";
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
}
