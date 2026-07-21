//! Coverage for the GORM `db-table` provide/consume adapters: model recognition (gorm.Model embed),
//! default naming, `TableName()` override, the import gate, and the composite-literal consume shapes.

use super::{extract_gorm_db_table_consumes, extract_gorm_db_table_provides};

const IMPORT: &str = "package m\n\nimport \"gorm.io/gorm\"\n\n";

#[test]
fn a_gorm_model_struct_provides_its_default_named_table() {
    let src = format!("{IMPORT}type ArticleModel struct {{\n\tgorm.Model\n\tSlug string\n}}\n");
    let out = extract_gorm_db_table_provides("articles/models.go", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(out[0].key, "table:article_models");
    assert_eq!(out[0].symbol.as_deref(), Some("ArticleModel"));
    assert_eq!(out[0].file, "articles/models.go");
}

#[test]
fn a_table_name_method_overrides_the_default_naming() {
    let src = format!(
        "{IMPORT}type ArticleModel struct {{\n\tgorm.Model\n}}\n\nfunc (ArticleModel) TableName() string {{\n\treturn \"articles\"\n}}\n"
    );
    let out = extract_gorm_db_table_provides("m.go", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(
        out[0].key, "table:articles",
        "TableName() literal wins over default naming"
    );
    assert_eq!(out[0].symbol.as_deref(), Some("ArticleModel"));
}

#[test]
fn a_struct_with_gorm_field_tags_is_a_model_even_without_the_embed() {
    // `UserModel` in be-gin defines its own `ID`/columns with `gorm:` tags and does NOT embed gorm.Model.
    let src = format!(
        "{IMPORT}type UserModel struct {{\n\tID uint `gorm:\"primaryKey\"`\n\tUsername string `gorm:\"column:username\"`\n}}\n"
    );
    let out = extract_gorm_db_table_provides("users/models.go", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:user_models");
    assert_eq!(out[0].symbol.as_deref(), Some("UserModel"));
}

#[test]
fn a_non_gorm_struct_provides_nothing() {
    // A plain struct with no embedded gorm.Model is not a model.
    let src = format!("{IMPORT}type Config struct {{\n\tName string\n}}\n");
    assert!(extract_gorm_db_table_provides("m.go", &src).is_empty());
}

#[test]
fn a_foreign_tag_whose_value_contains_gorm_is_not_a_model() {
    // `example:"gorm:foo"` mentions `gorm:` inside a VALUE, not as a tag key — must not mark a model.
    let src =
        format!("{IMPORT}type NotAModel struct {{\n\tName string `example:\"gorm:foo\"`\n}}\n");
    assert!(extract_gorm_db_table_provides("m.go", &src).is_empty());
}

#[test]
fn a_file_that_does_not_import_gorm_is_gated_out() {
    let src = "package m\n\ntype ArticleModel struct {\n\tgorm.Model\n}\n";
    assert!(extract_gorm_db_table_provides("m.go", src).is_empty());
    assert!(extract_gorm_db_table_consumes("m.go", src).is_empty());
}

#[test]
fn two_models_yield_two_provides() {
    let src = format!(
        "{IMPORT}type ArticleModel struct {{ gorm.Model }}\ntype TagModel struct {{ gorm.Model }}\n"
    );
    let out = extract_gorm_db_table_provides("m.go", &src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["table:article_models", "table:tag_models"]);
}

#[test]
fn db_model_pointer_literal_is_a_consume() {
    let src = format!(
        "{IMPORT}func f(db *gorm.DB) {{\n\tdb.Model(&FavoriteModel{{}}).Where(\"x\").Delete(&FavoriteModel{{}})\n}}\n"
    );
    let out = extract_gorm_db_table_consumes("m.go", &src);
    assert_eq!(out.len(), 1, "deduped per model: {out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("FavoriteModel"));
}

#[test]
fn db_where_value_literal_is_a_consume() {
    // `db.Where(ArticleUserModel{...})` — a bare (non-pointer) composite literal.
    let src = format!(
        "{IMPORT}func f(db *gorm.DB) {{\n\tdb.Where(ArticleUserModel{{Slug: \"x\"}}).First(nil)\n}}\n"
    );
    let out = extract_gorm_db_table_consumes("m.go", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].raw.as_deref(), Some("ArticleUserModel"));
}

#[test]
fn two_distinct_models_in_queries_yield_two_consumes() {
    let src = format!(
        "{IMPORT}func f(db *gorm.DB) {{\n\tdb.Find(&ArticleModel{{}})\n\tdb.Create(&TagModel{{}})\n}}\n"
    );
    let out = extract_gorm_db_table_consumes("m.go", &src);
    let mut raws: Vec<&str> = out.iter().filter_map(|c| c.raw.as_deref()).collect();
    raws.sort_unstable();
    assert_eq!(raws, vec!["ArticleModel", "TagModel"]);
}

#[test]
fn a_non_query_method_call_is_ignored() {
    // A composite literal passed to a non-GORM method must not mint a consume.
    let src = format!("{IMPORT}func f() {{\n\trender(ArticleModel{{}})\n}}\n");
    assert!(extract_gorm_db_table_consumes("m.go", &src).is_empty());
}
