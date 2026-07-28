use super::extract_django_view_guard_classes;

// NOTE ON FIXTURE SHAPE: Python is indentation-sensitive, so every fixture is a raw string that keeps
// its own leading whitespace (a `\`-continuation literal would strip it and fail to parse).

/// SHAPE FROM CORPUS: `corpus/oss/be-django/conduit/apps/articles/views.py` (its real import header plus
/// four of its view classes, in shape). Seals the whole verdict vocabulary at once:
/// `IsAuthenticated`/`IsAuthenticatedOrReadOnly` are auth evidence, `AllowAny` is NOT (reporting it as
/// guarded would silently clear the tree's genuinely open routes), and a class declaring no
/// `permission_classes` at all is ABSENT from the result rather than reported `false`.
#[test]
fn drf_permission_classes_verdicts_match_the_conduit_corpus() {
    let text = r#"from rest_framework import generics, viewsets
from rest_framework.permissions import (
    AllowAny, IsAuthenticated, IsAuthenticatedOrReadOnly
)

class ArticleViewSet(viewsets.GenericViewSet):
    permission_classes = (IsAuthenticatedOrReadOnly,)

class ArticlesFeedAPIView(generics.ListAPIView):
    permission_classes = (IsAuthenticated,)

class TagListAPIView(generics.ListAPIView):
    permission_classes = (AllowAny,)

class NoDeclarationAPIView(generics.ListAPIView):
    queryset = Tag.objects.all()
"#;
    assert_eq!(
        extract_django_view_guard_classes(text),
        vec![
            ("ArticleViewSet".to_string(), true),
            ("ArticlesFeedAPIView".to_string(), true),
            ("TagListAPIView".to_string(), false),
        ]
    );
}

/// Seals the dotted-member element form (`permissions.IsAuthenticated`) and the list-literal spelling —
/// both are ordinary DRF style and must judge identically to the bare-name tuple above.
#[test]
fn dotted_permission_element_and_list_literal_are_judged() {
    let text = r#"from rest_framework import permissions

class V(APIView):
    permission_classes = [permissions.IsAuthenticated]
"#;
    assert_eq!(
        extract_django_view_guard_classes(text),
        vec![("V".to_string(), true)]
    );
}

/// Seals the never-guess boundary: a `permission_classes` this scan cannot read as a literal collection
/// yields NO verdict for that class, rather than a fabricated one.
#[test]
fn non_literal_permission_classes_yields_no_verdict() {
    let text = r#"from rest_framework import permissions

class V(APIView):
    permission_classes = get_permissions()
"#;
    assert!(extract_django_view_guard_classes(text).is_empty());
}

/// Seals the import gate and the parse-failure contract every extractor in this crate upholds.
#[test]
fn import_gate_and_parse_failure_yield_nothing() {
    let no_drf = "class V:\n    permission_classes = (IsAuthenticated,)\n";
    assert!(extract_django_view_guard_classes(no_drf).is_empty());
    assert!(
        extract_django_view_guard_classes("from rest_framework import x\nclass V(:\n").is_empty()
    );
}
