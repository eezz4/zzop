//! Coverage for the Django `db-table` provide/consume adapters: field-driven model recognition (through a
//! project-local abstract base), abstract/manager exclusion, `db_table` override, app-label naming from the
//! file path, the models-module import gate, and the `.objects` manager-access consume shape.

use super::{extract_django_db_table_consumes, extract_django_db_table_provides};

const IMPORT: &str = "from django.db import models\n\n";

#[test]
fn a_field_bearing_model_through_an_abstract_base_provides_a_table() {
    // `Article(TimestampedModel)` does NOT inherit `models.Model` directly — recognition is field-driven.
    let src = format!(
        "{IMPORT}class Article(TimestampedModel):\n    slug = models.SlugField(unique=True)\n    title = models.CharField(max_length=255)\n"
    );
    let out = extract_django_db_table_provides("conduit/apps/articles/models.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(
        out[0].key, "table:articles_article",
        "default <app_label>_<model_lower> from the path"
    );
    assert_eq!(out[0].symbol.as_deref(), Some("Article"));
    assert_eq!(out[0].file, "conduit/apps/articles/models.py");
}

#[test]
fn an_abstract_base_provides_no_table() {
    // `TimestampedModel` has `models.DateTimeField` fields but `class Meta: abstract = True` -> no table.
    let src = format!(
        "{IMPORT}class TimestampedModel(models.Model):\n    created_at = models.DateTimeField(auto_now_add=True)\n\n    class Meta:\n        abstract = True\n"
    );
    assert!(extract_django_db_table_provides("core/models.py", &src).is_empty());
}

#[test]
fn an_annotated_field_assign_is_recognized() {
    // Modern typed Django: `age: int = models.IntegerField()` is a `Stmt::AnnAssign` — must still count
    // as a field (parity with the sqlalchemy sibling's annotated `__tablename__`).
    let src =
        format!("{IMPORT}class Widget(TimestampedModel):\n    age: int = models.IntegerField()\n");
    let out = extract_django_db_table_provides("apps/store/models.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].key, "table:store_widget");
    assert_eq!(out[0].symbol.as_deref(), Some("Widget"));
}

#[test]
fn an_annotated_abstract_flag_still_suppresses_the_table() {
    // `abstract: bool = True` (annotated) must suppress just as the plain `abstract = True` does.
    let src = format!(
        "{IMPORT}class Base(models.Model):\n    created_at = models.DateTimeField()\n\n    class Meta:\n        abstract: bool = True\n"
    );
    assert!(extract_django_db_table_provides("core/models.py", &src).is_empty());
}

#[test]
fn a_manager_class_with_no_model_field_provides_nothing() {
    // `UserManager(BaseUserManager)` defines methods, no `models.<Field>()` assign -> not a model.
    let src = format!(
        "{IMPORT}class UserManager(BaseUserManager):\n    def create_user(self, username):\n        return username\n"
    );
    assert!(extract_django_db_table_provides("auth/models.py", &src).is_empty());
}

#[test]
fn a_db_table_meta_literal_overrides_the_default_naming() {
    let src = format!(
        "{IMPORT}class Article(TimestampedModel):\n    slug = models.SlugField()\n\n    class Meta:\n        db_table = \"custom_articles\"\n"
    );
    let out = extract_django_db_table_provides("apps/articles/models.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(
        out[0].key, "table:custom_articles",
        "db_table literal wins over <app>_<model> default"
    );
    assert_eq!(out[0].symbol.as_deref(), Some("Article"));
}

#[test]
fn a_model_file_with_no_app_directory_falls_back_to_model_name_only() {
    let src = format!("{IMPORT}class Widget(models.Model):\n    name = models.CharField()\n");
    let out = extract_django_db_table_provides("models.py", &src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(
        out[0].key, "table:widget",
        "no parent dir -> bare model-name key"
    );
}

#[test]
fn a_file_that_does_not_import_the_models_module_is_gated_out() {
    // A serializer uses `serializers.CharField`, never `models.<Field>` — no django.db models import.
    let src = "from rest_framework import serializers\n\nclass ArticleSerializer(serializers.ModelSerializer):\n    title = serializers.CharField()\n";
    assert!(extract_django_db_table_provides("apps/articles/serializers.py", src).is_empty());
}

#[test]
fn two_models_in_one_file_yield_two_provides() {
    let src = format!(
        "{IMPORT}class Article(TimestampedModel):\n    slug = models.SlugField()\n\nclass Comment(TimestampedModel):\n    body = models.TextField()\n"
    );
    let out = extract_django_db_table_provides("apps/articles/models.py", &src);
    let mut keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["table:articles_article", "table:articles_comment"]
    );
}

#[test]
fn a_model_manager_access_is_a_consume() {
    let src =
        "from .models import Article\n\ndef list_articles():\n    return Article.objects.all()\n";
    let out = extract_django_db_table_consumes("apps/articles/views.py", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].kind, "db-table");
    assert_eq!(out[0].key, None);
    assert_eq!(out[0].raw.as_deref(), Some("Article"));
    assert_eq!(out[0].file, "apps/articles/views.py");
}

#[test]
fn distinct_models_queried_yield_distinct_consumes_deduped() {
    // Views import only rest_framework + `.models` (no literal "django") — consume side is NOT import-gated.
    let src = "from rest_framework import viewsets\nfrom .models import Article, Comment\n\nclass V:\n    a = Article.objects.select_related('author')\n    b = Comment.objects.all()\n    c = Article.objects.filter(slug='x')\n";
    let out = extract_django_db_table_consumes("apps/articles/views.py", src);
    let mut raws: Vec<&str> = out.iter().filter_map(|c| c.raw.as_deref()).collect();
    raws.sort_unstable();
    assert_eq!(
        raws,
        vec!["Article", "Comment"],
        "Article deduped across two query sites"
    );
}

#[test]
fn a_file_without_the_objects_token_is_pre_skipped() {
    let src = "from .models import Article\n\ndef f():\n    return Article\n";
    assert!(extract_django_db_table_consumes("v.py", src).is_empty());
}

#[test]
fn objects_on_a_non_name_base_is_not_a_consume() {
    // `self.objects` / `foo().objects` — the base is not a bare `Name` model reference.
    let src = "class V:\n    def f(self):\n        return self.objects.all()\n";
    assert!(extract_django_db_table_consumes("v.py", src).is_empty());
}
