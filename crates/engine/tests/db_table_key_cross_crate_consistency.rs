//! `db-table` join-key cross-crate equality pin. The `table:` key now has THREE producers/consumers:
//! parser-prisma's PROVIDE side (`accessor_casing`, lower-firsts the model name — private, driven here
//! through the public `build_common_ir` bridge), parser-sql's PROVIDE side (`bare_table_name`,
//! lower-firsts the DDL table name — private, driven through the public `extract_db_table_provides`),
//! and parser-typescript's CONSUME side (accessor as written at the call site, no re-casing). The
//! prisma- and sql-provide casing transforms are byte-identical local twins kept in two separate crates
//! on purpose (an engine already depends on both, so no parser-crate -> parser-crate edge is needed for
//! one 3-line transform — see each crate's own module doc: `parser-prisma/src/analysis.rs`'s
//! `accessor_casing` doc and `parser-sql/src/extract.rs`'s module doc "Casing"). Nothing besides this
//! test enforces the twins agree: a future edit to either one alone would silently break the `db-table`
//! join for any non-lowercase table/model name. Same T1-twin-pin pattern as
//! `crates/engine/src/analyze/native_rules/prisma_client_getter_consistency_tests.rs` (parser-typescript's
//! `PRISMA_CLIENT_GETTER` == parser-prisma's `DEFAULT_PRISMA_CLIENT_GETTER_FN`), just living in
//! `tests/` instead of `src/` since both producers here are public API of crates `zzop-engine` already
//! depends on (no dev-dependency needed).

fn sql_table_key(ddl: &str) -> String {
    let provides = zzop_parser_sql::extract_db_table_provides("schema.sql", ddl);
    assert_eq!(
        provides.len(),
        1,
        "fixture DDL must yield exactly one db-table provide, got: {provides:?}"
    );
    provides[0].key.clone()
}

fn prisma_table_key(schema: &str) -> String {
    let ir = zzop_parser_prisma::build_common_ir(
        "svc",
        &[("schema.prisma".to_string(), schema.to_string())],
    );
    let io = ir.ir.io.expect("non-empty schema must produce io provides");
    assert_eq!(
        io.provides.len(),
        1,
        "fixture schema must yield exactly one db-table provide, got: {:?}",
        io.provides
    );
    io.provides[0].key.clone()
}

#[test]
fn single_word_table_name_produces_the_identical_provide_key_on_both_sides() {
    let sql_key = sql_table_key("CREATE TABLE \"Article\" (id INT);\n");
    let prisma_key = prisma_table_key("model Article {\n  id String @id\n}\n");
    assert_eq!(sql_key, "table:article");
    assert_eq!(
        sql_key, prisma_key,
        "parser-sql's bare_table_name and parser-prisma's accessor_casing must land on the SAME \
         table: key for the same name, or the db-table join silently breaks"
    );
}

#[test]
fn multi_word_pascal_table_name_produces_the_identical_provide_key_on_both_sides() {
    let sql_key = sql_table_key("CREATE TABLE \"ArticleTag\" (id INT);\n");
    let prisma_key = prisma_table_key("model ArticleTag {\n  id String @id\n}\n");
    assert_eq!(sql_key, "table:articleTag");
    assert_eq!(
        sql_key, prisma_key,
        "a multi-word PascalCase name must lower-first identically on both sides (first char only), \
         or a Prisma-generated migration's CREATE TABLE would ride a different key than the schema's \
         own model provide"
    );
}
