//! Lexical PSL parsing: `parse_schema` / `parse_schema_enums` and their line-scan helpers + regexes.

use std::sync::OnceLock;

use regex::Regex;
use zzop_core::{FieldAttr, SchemaEnum, SchemaField, SchemaModel};

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

/// `@@map` / `@@unique` / `@@index` on a model block.
///
/// The column list of `@@unique`/`@@index` is PSL's first POSITIONAL argument, and PSL accepts the same
/// list spelled as the NAMED argument `fields:` — `@@unique(fields: [a, b], name: "x")` declares exactly
/// the index `@@unique([a, b])` does. `re_unique`/`re_index` used to require `[` to follow `(`, so they
/// recognized only the positional half, and the half they missed did not degrade to "unsure": no group
/// was pushed, and the model projected as carrying NO such index — indistinguishable downstream from a
/// schema that really has none. `schema/fk-no-index` reads these two lists to ask whether a foreign-key
/// column LEADS some group; on calcom/cal.com `32919f9` it reported `SelectedSlots.userId` as unindexed
/// while `schema.prisma:1447` declares
/// `@@unique(fields: [userId, slotUtcStartDate, slotUtcEndDate, uid], name: "selectedSlotUnique")` with
/// `userId` leading it. One spelling, one false finding, and no policy question involved.
///
/// Only `fields:`, and only in first position. Every other block-attribute argument PSL takes here
/// (`name`, `map`, `type`, `clustered`, `sort`, `length`, `ops`) is a scalar, so the column list is the
/// only bracketed argument — but matching "the first `[...]` anywhere inside the parens" would buy an
/// argument ORDER nothing in the corpus writes, at the cost of a wildcard a quoted `name:` containing `]`
/// could walk through. Measured on the 9-tree corpus (2 `.prisma` files, both under cal.com):
/// `@@unique(fields:` **1**, `@@index(fields:` **0**, `@@id(fields:` **0**, `fields:` in any non-first
/// position **0**. TRAILING named arguments already parsed and still do, because the capture stops at the
/// closing bracket: `@@unique([a, b], name: "x")` **2**, `@@index([a], map: "x")` **0**. Both halves are
/// pinned by tests, so the fix cannot be bought by narrowing what already worked.
///
/// `@@id` is still not read at all, and that is a MEASURED silence rather than an oversight: a composite
/// `@@id([a, b])` is a primary key and therefore a real index, but all **4** occurrences in the corpus
/// (`schema.prisma:80`, `:1401`, `:1415`, `:1608`) sit on models that ALSO declare the same leading column
/// under an explicit `@@index`/`@@unique`, so reading them would harvest 0 findings while adding groups to
/// a channel `schema/redundant-index` also consumes. Widening here needs its own harvest measurement.
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

/// Field attributes, with their argument text kept WHOLE across one level of nesting.
///
/// The nesting is not an edge case: Prisma's own default generators are calls, so `@default(now())`,
/// `@default(uuid())`, `@default(cuid())` and `@default(autoincrement())` are the ordinary spellings.
/// A regex that stopped at the first `)` handed every one of them a truncated argument — `@default(now())`
/// projected as `now(` — and the consumer that asks "does this model carry a creation timestamp"
/// (`zzop_rules_schema`'s `has_default_now`) tests `contains("now()")`, so it answered NO for every model
/// whose creation timestamp is spelled anything other than the literal name `createdAt`. Measured on
/// calcom/cal.com `176037d`: 8 of the 37 `schema/missing-timestamps` findings sat on models that DO carry a
/// `DateTime @default(now())` — `User.createdDate`, `Feedback.date`, `UserFeatures.assignedAt` and five
/// more. `docs/rules/catalog.md` promised that spelling counted, the rule's own code tried to honour it,
/// and the projection under both of them made it unreachable.
///
/// One level, not arbitrary depth: `@default(dbgenerated("gen_random_uuid()"))` nests twice and is still
/// truncated. Stated rather than left silent — it is strictly better than before and the shapes above are
/// what the ecosystem writes. A real paren balancer belongs here only when a consumer needs depth 2.
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
lazy_re!(
    re_unique,
    r"^@@unique\s*\(\s*(?:fields\s*:\s*)?\[([^\]]+)\]"
);
lazy_re!(re_index, r"^@@index\s*\(\s*(?:fields\s*:\s*)?\[([^\]]+)\]");
lazy_re!(re_field, r"^(\w+)\s+(\w+)(\?|\[\])?\s*(.*)$");
lazy_re!(re_attr, r"@(\w+)(\(((?:[^()]|\([^()]*\))*)\))?");
lazy_re!(re_block_comment, r"(?s)/\*.*?\*/");
lazy_re!(re_line_comment, r"(?m)(^|[^:])//.*$");
