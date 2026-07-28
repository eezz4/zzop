//! The schema-IR -> Common IR bridge (`build_common_ir`): schema.prisma text this crate has already
//! parsed, projected into the symbol/io space the engine and cross-layer passes consume.
//!
//! There is no second, filesystem-walking entry point here. `prisma_schema_analysis` (a `find schema
//! files under app_dir -> analyze_schema` orchestrator, gated on a `target: &str` of `"be"`/`"all"`
//! left over from the JS CLI) and the `find_prisma_schemas` discovery walk it wrapped were removed
//! 2026-07-27 with zero callers workspace-wide: the engine reaches Prisma files through its own
//! dispatch + fused per-file pass, which calls `parse_schema`/`build_common_ir` per file and runs
//! `zzop_rules_schema` itself. Nothing routed through this crate's own walk.

use crate::parse::parse_schema;

/// The standard Prisma-client accessor name the `db-table` consume recognizer keys off
/// (`zzop_parser_typescript::adapters::db_table_consume`) — a common-Prisma idiom, shared as one literal.
pub const DEFAULT_PRISMA_CLIENT_GETTER_FN: &str = "getPrisma";

/// Project schema.prisma files into a `CommonIr` — the parser -> engine bridge (mirrors the
/// parser-typescript `build_common_ir` shape). Each model becomes an exported `SourceSymbol`
/// (kind = Class: a model is the closest thing PSL has to a data-shape declaration), so schema
/// entities join the same symbol space the engine and cross-layer passes consume, AND one or two
/// `kind="db-table"` io PROVIDEs at the model's declaration line — see [`db_table_provide_keys`] for
/// which keys and why (`accessor_casing`'s doc covers why the primary key is NOT the model's own
/// PascalCase name). PSL has no imports, so `dep` stays empty; `loc` counts non-blank/non-comment
/// lines per schema file. This bridge is a deliberate addition beyond schema analysis alone, so schema
/// entities can participate in cross-layer joins that key off Common IR symbols.
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
            for key in db_table_provide_keys(m) {
                provides.push(zzop_core::IoProvide {
                    kind: "db-table".into(),
                    key,
                    file: rel.clone(),
                    line,
                    symbol: None,
                    body: None,
                });
            }
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

/// The `db-table` PROVIDE key(s) ONE model declares, in emission order — ONE key for the ordinary
/// model, TWO when `@@map` renames the physical table to something the accessor key cannot spell.
///
/// The `db-table` channel key is cross-layer identity for "which table", but a `@@map`ed Prisma model
/// genuinely HAS TWO names that different layers see, and no single key can serve both:
/// - the Prisma-client ACCESSOR name (`model Article` -> `prisma.article`), which is what the CONSUME
///   side (`zzop_parser_typescript::adapters::db_table_consume`) reads off the call site verbatim;
/// - the physical SQL TABLE name (`@@map("articles")`), which is what the DDL PROVIDE side
///   (`zzop_parser_sql::extract_db_table_provides`) reads off `CREATE TABLE articles` and what a
///   non-Prisma ORM in another tree (SQLAlchemy/Django/GORM) keys on.
///
/// Without `@@map` the two coincide after [`zzop_core::db_table_channel_casing`] (Prisma's default
/// table name IS the model name), which is why one key sufficed until now; with `@@map` they diverge
/// and exactly one of the two joins would break whichever single key were chosen. So BOTH are emitted
/// rather than swapping: the model really does declare both identities, and claiming only the physical
/// one would silently drop every `prisma.<accessor>` consume edge for a renamed model (a REGRESSION
/// dressed as a fix), while claiming only the accessor one leaves the DDL/other-ORM side unjoinable
/// (the status quo this closes).
///
/// Emitting two provides for one model is safe by construction on the join side, verified against
/// `zzop_core::io::link`: multiple providers of one key WITHIN one source tree are legal and fan out to
/// one edge each (`link_cross_layer_io`'s own "Multiple providers for one key is legal" note); the
/// ambiguity gate keys on providers spanning 2+ DISTINCT SOURCE TREES, where a second tree also
/// declaring the same physical table IS a genuine ambiguity the linker is meant to report; and every
/// rule reading `unconsumed_provides` (`cross-layer/unconsumed-endpoint`, `-mutation-endpoint`,
/// `-procedure`, `cross-layer/duplicate-route`) filters to `kind == "http"`/`"trpc"`, so an
/// unconsumed extra `db-table` provide produces no finding. `cross-layer/db-table-name-in-multiple-sources` counts
/// CONSUMES only, so provide count cannot move it either.
///
/// The `@@map` key gets the SAME [`zzop_core::db_table_channel_casing`] the SQL side applies to a
/// quoted DDL name, so `@@map("Articles")` and `CREATE TABLE "Articles"` land on one key. A `@@map`
/// whose cased form equals the accessor key (`model Article { @@map("article") }`, or any model whose
/// map merely restates the default) emits ONE key, not a duplicate.
fn db_table_provide_keys(m: &zzop_core::SchemaModel) -> Vec<String> {
    let accessor_key = format!("table:{}", accessor_casing(&m.name));
    let mut keys = vec![accessor_key];
    if let Some(mapped) = &m.table_name {
        let physical_key = format!("table:{}", zzop_core::db_table_channel_casing(mapped));
        if physical_key != keys[0] {
            keys.push(physical_key);
        }
    }
    keys
}

/// PascalCase model name -> the Prisma-generated client accessor's casing (first character
/// lowercased, the rest unchanged — e.g. `Article` -> `article`, `UserProfile` -> `userProfile`).
/// CANONICAL `table:` KEY CASING, chosen to byte-match the CONSUME side:
/// `zzop_parser_typescript::adapters::db_table_consume` keys off the accessor exactly as written at
/// the call site (`prisma.article...` / `getPrisma().userProfile...`), which by Prisma convention is
/// already this same lower-first casing — so the provide side re-cases to meet it there rather than
/// the other way around. See that module's doc header ("CANONICAL KEY CASING") for the cross-reference
/// from the other side, and this module's [`db_table_provide_keys`] for the one call site (which also
/// documents the SECOND, `@@map`-derived physical-table key a renamed model additionally provides).
///
/// Delegates to [`zzop_core::db_table_channel_casing`] — the same shared transform
/// `zzop_parser_sql::extract::bare_table_name` calls for its own (independent) `db-table` PROVIDE side,
/// so the two extractors' casing cannot drift apart.
fn accessor_casing(model_name: &str) -> String {
    zzop_core::db_table_channel_casing(model_name)
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
