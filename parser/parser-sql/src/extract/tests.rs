use super::extract_db_table_provides;

fn keys(text: &str) -> Vec<String> {
    extract_db_table_provides("schema.sql", text)
        .into_iter()
        .map(|p| p.key)
        .collect()
}

#[test]
fn plain_create_table() {
    let src = "CREATE TABLE users (id INT);\n";
    let provides = extract_db_table_provides("schema.sql", src);
    assert_eq!(provides.len(), 1);
    assert_eq!(provides[0].kind, "db-table");
    assert_eq!(provides[0].key, "table:users");
    assert_eq!(provides[0].file, "schema.sql");
    assert_eq!(provides[0].line, 1);
    assert!(provides[0].symbol.is_none());
    assert!(provides[0].body.is_none());
}

#[test]
fn if_not_exists() {
    assert_eq!(
        keys("CREATE TABLE IF NOT EXISTS orders (id INT);\n"),
        vec!["table:orders"]
    );
}

#[test]
fn quoted_double() {
    assert_eq!(
        keys(r#"CREATE TABLE "users" (id INT);"#),
        vec!["table:users"]
    );
}

#[test]
fn quoted_backtick() {
    assert_eq!(keys("CREATE TABLE `users` (id INT);"), vec!["table:users"]);
}

#[test]
fn quoted_bracket() {
    assert_eq!(keys("CREATE TABLE [users] (id INT);"), vec!["table:users"]);
}

#[test]
fn schema_qualified_plain() {
    assert_eq!(
        keys("CREATE TABLE public.users (id INT);"),
        vec!["table:users"]
    );
}

#[test]
fn schema_qualified_quoted_double() {
    assert_eq!(
        keys(r#"CREATE TABLE "public"."users" (id INT);"#),
        vec!["table:users"]
    );
}

#[test]
fn schema_qualified_bracket() {
    assert_eq!(
        keys("CREATE TABLE [dbo].[users] (id INT);"),
        vec!["table:users"]
    );
}

#[test]
fn multiple_tables_one_file_deterministic_order() {
    let src = "CREATE TABLE users (id INT);\nCREATE TABLE orders (id INT);\n";
    let provides = extract_db_table_provides("schema.sql", src);
    assert_eq!(provides.len(), 2);
    assert_eq!(provides[0].key, "table:users");
    assert_eq!(provides[0].line, 1);
    assert_eq!(provides[1].key, "table:orders");
    assert_eq!(provides[1].line, 2);
}

#[test]
fn line_number_points_at_the_create_table_line_not_the_file_start() {
    let src = "-- migration header\n\nCREATE TABLE widgets (\n  id INT\n);\n";
    let provides = extract_db_table_provides("m.sql", src);
    assert_eq!(provides.len(), 1);
    assert_eq!(provides[0].line, 3);
}

#[test]
fn non_ddl_file_is_empty() {
    let src = "SELECT * FROM users;\nINSERT INTO users (id) VALUES (1);\n";
    assert!(extract_db_table_provides("seed.sql", src).is_empty());
}

#[test]
fn table_names_are_lower_firsted_to_the_channel_canonical() {
    // Lower-first (accessor casing) — the db-table channel canonical, see the module doc's "Casing"
    // section. A Prisma-generated migration's `CREATE TABLE "Article"` must produce the SAME key the
    // Prisma schema provide / client consume side produces (`table:article`), and hand-written
    // lowercase DDL is untouched.
    assert_eq!(
        keys("create table Widgets (id int);"),
        vec!["table:widgets"]
    );
    assert_eq!(
        keys("CREATE TABLE \"Article\" (id int);"),
        vec!["table:article"]
    );
    assert_eq!(
        keys("CREATE TABLE \"ArticleTags\" (id int);"),
        vec!["table:articleTags"] // matches zzop_parser_prisma::analysis::accessor_casing
    );
    assert_eq!(
        keys("CREATE TABLE article_tags (id int);"),
        vec!["table:article_tags"] // lower-first is a no-op on snake_case
    );
}

#[test]
fn alter_and_drop_are_ignored() {
    let src = "ALTER TABLE users ADD COLUMN age INT;\nDROP TABLE legacy;\n";
    assert!(extract_db_table_provides("mig.sql", src).is_empty());
}

#[test]
fn empty_file_is_empty() {
    assert!(extract_db_table_provides("empty.sql", "").is_empty());
}

#[test]
fn quoted_identifier_with_internal_dot_is_one_name_not_split() {
    // `"my.table"` is a single quoted identifier LITERALLY named `my.table` — the dot is inside the
    // quote pair, so it is not a schema-qualifier separator. Bare name: `my.table` lower-firsted (a
    // no-op here, first char is already lowercase), NOT `table` (which `rsplit('.')` before
    // quote-stripping used to produce as a bug).
    assert_eq!(
        keys(r#"CREATE TABLE "my.table" (id INT);"#),
        vec!["table:my.table"]
    );
}

#[test]
fn schema_qualified_still_correct_after_quote_aware_split() {
    // Regression pin: the quote-aware dot-split must not break the ordinary schema-qualified cases.
    assert_eq!(
        keys(r#"CREATE TABLE "public"."users" (id INT);"#),
        vec!["table:users"]
    );
    assert_eq!(
        keys("CREATE TABLE public.users (id INT);"),
        vec!["table:users"]
    );
}

#[test]
fn create_temporary_table_is_parsed_but_not_provided() {
    // A `TEMPORARY`/`TEMP` table is session-local, not a shared persistent schema object — it must NOT
    // mint a `db-table` provide (else a temp `t`/`results`/`staging` false-joins an ORM accessor).
    assert!(keys("CREATE TEMPORARY TABLE t (id INT);").is_empty());
}

#[test]
fn create_temp_table_is_not_provided() {
    assert!(keys("CREATE TEMP TABLE t (id INT);").is_empty());
}

#[test]
fn create_unlogged_table_is_still_recognized() {
    // UNLOGGED is persistent (just not crash-safe) — a real shared table, so it still provides.
    assert_eq!(keys("CREATE UNLOGGED TABLE t (id INT);"), vec!["table:t"]);
}

#[test]
fn create_global_temporary_table_is_not_provided() {
    assert!(keys("CREATE GLOBAL TEMPORARY TABLE t (id INT);").is_empty());
}

#[test]
fn create_temporary_table_if_not_exists_is_not_provided() {
    assert!(keys("CREATE TEMPORARY TABLE IF NOT EXISTS t (id INT);").is_empty());
}

#[test]
fn a_persistent_table_named_with_a_temp_prefix_still_provides() {
    // The skip keys off the MODIFIER slot, not the table name — `temp_config` is a normal table.
    assert_eq!(
        keys("CREATE TABLE temp_config (id INT);"),
        vec!["table:temp_config"]
    );
}
