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
