//! prismaSchemaAnalysis — thin orchestrator: schema.prisma on disk -> core structural/DB-pattern analysis —
//! plus the schema-IR -> Common IR bridge (`build_common_ir`).

use std::path::Path;

use zzop_rules_schema::{analyze_schema, SchemaAnalysis};

use crate::discover::find_prisma_schemas;
use crate::parse::parse_schema;

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
/// entities join the same symbol space the engine and cross-layer passes consume, AND a
/// `(kind="db-table", key="table:<accessor-cased name>")` io PROVIDE at the model's declaration line —
/// see `accessor_casing`'s doc for why the key is NOT the model's own PascalCase name. PSL has no
/// imports, so `dep` stays empty; `loc` counts non-blank/non-comment lines per schema file. This
/// bridge is a deliberate addition beyond schema analysis alone, so schema entities can
/// participate in cross-layer joins that key off Common IR symbols.
pub fn build_common_ir(source_id: &str, files: &[(String, String)]) -> zzop_core::CommonIr {
    let mut symbols = Vec::new();
    let mut provides = Vec::new();
    let mut loc = std::collections::HashMap::new();
    for (rel, text) in files {
        let models = parse_schema(text, Some(rel), None);
        for m in &models {
            let line = model_decl_line(text, &m.name);
            symbols.push(zzop_core::SourceSymbol {
                id: format!("{rel}#{}", m.name),
                file: rel.clone(),
                name: m.name.clone(),
                kind: zzop_core::SourceSymbolKind::Class,
                line,
                exported: true,
                is_default: false,
                body_start: None,
                body_end: None,
                write_sites: Vec::new(),
            });
            provides.push(zzop_core::IoProvide {
                kind: "db-table".into(),
                key: format!("table:{}", accessor_casing(&m.name)),
                file: rel.clone(),
                line,
                symbol: None,
                body: None,
            });
        }
        loc.insert(rel.clone(), count_schema_loc(text));
    }
    let io = if provides.is_empty() {
        None
    } else {
        Some(zzop_core::IoFacts {
            provides,
            consumes: Vec::new(),
        })
    };
    zzop_core::CommonIr {
        source: source_id.to_string(),
        parser: "prisma".to_string(),
        ir: zzop_core::MinimalIr {
            dep: std::collections::HashMap::new(),
            symbols,
            loc,
            io,
        },
    }
}

/// PascalCase model name -> the Prisma-generated client accessor's casing (first character
/// lowercased, the rest unchanged — e.g. `Article` -> `article`, `UserProfile` -> `userProfile`).
/// CANONICAL `table:` KEY CASING, chosen to byte-match the CONSUME side:
/// `zzop_parser_typescript::adapters::db_table_consume` keys off the accessor exactly as written at
/// the call site (`prisma.article...` / `getPrisma().userProfile...`), which by Prisma convention is
/// already this same lower-first casing — so the provide side re-cases to meet it there rather than
/// the other way around. See that module's doc header ("CANONICAL KEY CASING") for the cross-reference
/// from the other side, and this module's `build_common_ir` for the one call site.
fn accessor_casing(model_name: &str) -> String {
    let mut chars = model_name.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        None => String::new(),
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
