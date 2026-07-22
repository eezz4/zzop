use super::*;

fn frag<'a>(out: &'a [RouterMountFragment], name: &str) -> &'a RouterMountFragment {
    out.iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fragment named {name:?} in {out:?}"))
}

fn verb(method: &str, path: &str, handler: &str, line: u32) -> RouterMountEntry {
    RouterMountEntry::Verb {
        method: method.into(),
        path: path.into(),
        handler: Some(handler.into()),
        line,
        attr_keys: vec![],
    }
}

#[test]
fn rel_maps_to_dotted_module_path_for_fragment_name_and_ident_matching() {
    // The fragment name = the file's dotted module path, so it equals an `include('<dotted>')` mount's
    // ident and the composer's root-exclusion-by-name matches the child even on a resolve miss.
    assert_eq!(
        rel_to_module_path("conduit/apps/articles/urls.py"),
        "conduit.apps.articles.urls"
    );
    assert_eq!(rel_to_module_path("app/urls.py"), "app.urls");
    // A package urlconf (`include('pkg')` resolving to `pkg/__init__.py`) drops the `__init__`.
    assert_eq!(rel_to_module_path("pkg/__init__.py"), "pkg");
    assert_eq!(rel_to_module_path("urls.py"), "urls");
}

#[test]
fn no_django_import_yields_nothing() {
    // No `django.urls`/`django.conf.urls` import -> empty, never a bare-name guess.
    let src = "urlpatterns = [\n    url(r'^user/?$', UserView.as_view()),\n]\n";
    assert!(extract_django_route_fragments("app/urls.py", src).is_empty());
}

#[test]
fn conf_urls_regex_route_with_trailing_optional_slash() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^user/?$', UserRetrieveUpdateAPIView.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("auth/urls.py", src);
    assert_eq!(
        frag(&out, "auth.urls").entries,
        vec![verb("?", "/user", "UserRetrieveUpdateAPIView", 3)],
    );
}

#[test]
fn named_group_regex_reduces_to_param_hole() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^articles/(?P<article_slug>[-\\w]+)/comments/?$', CommentsView.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("articles/urls.py", src);
    assert_eq!(
        frag(&out, "articles.urls").entries,
        vec![verb("?", "/articles/{}/comments", "CommentsView", 3)],
    );
}

#[test]
fn two_named_groups_reduce_independently() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^articles/(?P<slug>[-\\w]+)/comments/(?P<pk>[\\d]+)/?$', C.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("a/urls.py", src);
    assert_eq!(
        frag(&out, "a.urls").entries,
        vec![verb("?", "/articles/{}/comments/{}", "C", 3)],
    );
}

#[test]
fn re_path_is_recognized_like_url() {
    let src = concat!(
        "from django.urls import re_path\n",
        "urlpatterns = [\n",
        "    re_path(r'^tags/?$', TagListAPIView.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("tags/urls.py", src);
    assert_eq!(
        frag(&out, "tags.urls").entries,
        vec![verb("?", "/tags", "TagListAPIView", 3)],
    );
}

#[test]
fn modern_path_converters_reduce_to_param_holes() {
    let src = concat!(
        "from django.urls import path\n",
        "urlpatterns = [\n",
        "    path('users/<int:pk>/', UserDetail.as_view()),\n",
        "    path('profile/', Profile.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("u/urls.py", src);
    assert_eq!(
        frag(&out, "u.urls").entries,
        vec![
            verb("?", "/users/{}", "UserDetail", 3),
            verb("?", "/profile", "Profile", 4),
        ],
    );
}

#[test]
fn path_with_alias_binding_is_classified_by_original() {
    // `from django.urls import path as p` — classification resolves the alias to its original.
    let src = concat!(
        "from django.urls import path as p\n",
        "urlpatterns = [\n",
        "    p('health/', Health.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("h/urls.py", src);
    assert_eq!(
        frag(&out, "h.urls").entries,
        vec![verb("?", "/health", "Health", 3)],
    );
}

#[test]
fn include_string_module_becomes_a_mount() {
    let src = concat!(
        "from django.conf.urls import include, url\n",
        "urlpatterns = [\n",
        "    url(r'^api/', include('conduit.apps.articles.urls', namespace='articles')),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("conduit/urls.py", src);
    assert_eq!(
        frag(&out, "conduit.urls").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            // Full dotted path (not the bare `urls` segment): equals the child's fragment name so
            // root-exclusion-by-name matches even on a resolve miss, and is unique in the flat ident set.
            ident: "conduit.apps.articles.urls".into(),
            specifier: Some("conduit.apps.articles.urls".into()),
            attr_keys: vec![],
        }],
    );
}

#[test]
fn path_include_string_module_becomes_a_mount() {
    let src = concat!(
        "from django.urls import include, path\n",
        "urlpatterns = [\n",
        "    path('api/', include('app.api.urls')),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("root/urls.py", src);
    assert_eq!(
        frag(&out, "root.urls").entries,
        vec![RouterMountEntry::Mount {
            prefix: "/api".into(),
            ident: "app.api.urls".into(),
            specifier: Some("app.api.urls".into()),
            attr_keys: vec![],
        }],
    );
}

#[test]
fn drf_router_include_is_skipped_silently() {
    // `include(router.urls)` — a non-string include arg (DRF DefaultRouter mount) is out of scope v1.
    let src = concat!(
        "from django.conf.urls import include, url\n",
        "router = DefaultRouter()\n",
        "urlpatterns = [\n",
        "    url(r'^', include(router.urls)),\n",
        "    url(r'^tags/?$', TagListAPIView.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("a/urls.py", src);
    // Only the concrete route survives; the DRF mount is absent (not a guessed mount).
    assert_eq!(
        frag(&out, "a.urls").entries,
        vec![verb("?", "/tags", "TagListAPIView", 5)],
    );
}

#[test]
fn non_as_view_function_view_is_skipped() {
    // `url(r'^admin/', admin.site.urls)` — second arg is neither `as_view()` nor a string `include`.
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^admin/', admin.site.urls),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("root/urls.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn non_literal_regex_is_skipped() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "PATTERN = build_pattern()\n",
        "urlpatterns = [\n",
        "    url(PATTERN, View.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("a/urls.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn unreducible_regex_alternation_is_vetoed() {
    // An unnamed group / alternation cannot reduce to a clean literal key — skip, never guess.
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^(foo|bar)/?$', View.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("a/urls.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn bare_char_class_outside_param_position_is_vetoed() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^items/[0-9]+/?$', View.as_view()),\n",
        "]\n",
    );
    let out = extract_django_route_fragments("a/urls.py", src);
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn no_urlpatterns_yields_nothing() {
    let src = "from django.conf.urls import url\nx = 1\n";
    assert!(extract_django_route_fragments("a/urls.py", src).is_empty());
}

#[test]
fn regex_to_key_unit_cases() {
    assert_eq!(regex_to_key(r"^user/?$").as_deref(), Some("/user"));
    assert_eq!(
        regex_to_key(r"^users/login/?$").as_deref(),
        Some("/users/login")
    );
    assert_eq!(
        regex_to_key(r"^profiles/(?P<username>\w+)/follow/?$").as_deref(),
        Some("/profiles/{}/follow"),
    );
    assert_eq!(regex_to_key(r"^api/").as_deref(), Some("/api"));
    assert_eq!(regex_to_key(r"^$").as_deref(), Some("/"));
    // Vetoes:
    assert_eq!(regex_to_key(r"^(a|b)$"), None);
    assert_eq!(regex_to_key(r"^x/[0-9]+$"), None);
    assert_eq!(regex_to_key(r"^x/\d+$"), None);
}

#[test]
fn path_to_key_unit_cases() {
    assert_eq!(path_to_key("user/").as_deref(), Some("/user"));
    assert_eq!(
        path_to_key("articles/<slug:name>/").as_deref(),
        Some("/articles/{}")
    );
    assert_eq!(
        path_to_key("<int:pk>/detail/").as_deref(),
        Some("/{}/detail")
    );
    assert_eq!(path_to_key("sitemap.xml").as_deref(), Some("/sitemap.xml"));
    assert_eq!(path_to_key("a/<missing").as_deref(), None);
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(extract_django_route_fragments("bad.py", "def f(:\n").is_empty());
}

#[test]
fn deterministic_across_repeated_extractions() {
    let src = concat!(
        "from django.conf.urls import url\n",
        "urlpatterns = [\n",
        "    url(r'^user/?$', UserView.as_view()),\n",
        "]\n",
    );
    let a = extract_django_route_fragments("a/urls.py", src);
    let b = extract_django_route_fragments("a/urls.py", src);
    assert_eq!(a, b);
}
