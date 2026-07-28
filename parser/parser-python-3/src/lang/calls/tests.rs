use super::*;

fn calls(text: &str) -> Vec<RawCall> {
    parse_calls("app/x.py", text)
}

fn names(text: &str) -> Vec<(String, Option<String>, String)> {
    calls(text)
        .into_iter()
        .map(|c| (c.callee_name, c.receiver_type, c.from_symbol))
        .collect()
}

/// Seals the base contract: a bare call inside a `def` body is attributed to that def's symbol id.
#[test]
fn bare_call_in_function_body_attributes_to_the_function() {
    let out = names("def handler():\n    check_owner()\n");
    assert_eq!(
        out,
        vec![(
            "check_owner".to_string(),
            None,
            "app/x.py#handler".to_string()
        )]
    );
}

/// Seals the "innermost body wins" rule: a call in a method body attributes to `Class.method`, not the
/// class — the same nesting rule the TS/Java extractors use.
#[test]
fn method_body_call_attributes_to_the_dotted_method_symbol() {
    let out = names("class V:\n    def post(self):\n        do_write()\n");
    assert_eq!(out[0].2, "app/x.py#V.post");
}

/// Seals the `self`/`cls` receiver rewrite — Django's class-based views spell every intra-view hop as
/// `self.x()`, so dropping it would make every intra-class guard hop invisible.
/// SHAPE FROM CORPUS: `corpus/oss/be-django/conduit/apps/articles/views.py`'s `ArticlesFeedAPIView.list`
/// (`queryset = self.get_queryset()` / `page = self.paginate_queryset(queryset)`).
#[test]
fn self_receiver_is_rewritten_to_the_enclosing_class() {
    let out = names(
        "class ArticlesFeedAPIView:\n    def list(self, request):\n        queryset = self.get_queryset()\n        page = self.paginate_queryset(queryset)\n",
    );
    assert_eq!(
        out,
        vec![
            (
                "get_queryset".to_string(),
                Some("ArticlesFeedAPIView".to_string()),
                "app/x.py#ArticlesFeedAPIView.list".to_string()
            ),
            (
                "paginate_queryset".to_string(),
                Some("ArticlesFeedAPIView".to_string()),
                "app/x.py#ArticlesFeedAPIView.list".to_string()
            ),
        ]
    );
}

/// Seals the annotated-parameter receiver typing: the receiver's DECLARED type, not its variable name,
/// is what `resolve_calls_for_file` can place cross-file.
/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py`'s
/// `delete_article_by_slug` (`articles_repo: ArticlesRepository = ...` then
/// `await articles_repo.delete_article(article=article)`).
#[test]
fn annotated_parameter_receiver_records_its_declared_type() {
    let out = names(
        "async def delete_article_by_slug(articles_repo: ArticlesRepository = None) -> None:\n    await articles_repo.delete_article(article=article)\n",
    );
    assert_eq!(
        out,
        vec![(
            "delete_article".to_string(),
            Some("ArticlesRepository".to_string()),
            "app/x.py#delete_article_by_slug".to_string()
        )]
    );
}

/// Seals the untracked-receiver verbatim fallback — an imported module/class spelled at the call site
/// is its own receiver identity, which is what lets the resolver match it against an import binding.
#[test]
fn untracked_receiver_falls_back_to_its_written_text() {
    let out = names("def f():\n    jwt.create_access_token_for_user(u)\n");
    assert_eq!(out[0].1, Some("jwt".to_string()));
}

/// Seals the module-doc's load-bearing exclusion: a FastAPI `Depends(...)` PARAMETER DEFAULT sits
/// outside the function's body span, so it must never mint a call edge — otherwise the BFS would clear
/// a route for evidence the BFS cannot justify, and the `auth-guarded` producer's job would be done by
/// accident.
/// SHAPE FROM CORPUS: `corpus/oss/be-fastapi/app/api/routes/articles/articles_resource.py`'s
/// `create_new_article`.
#[test]
fn depends_parameter_default_is_not_a_call_edge() {
    let out = names(
        "async def create_new_article(\n    user: User = Depends(get_current_user_authorizer()),\n) -> ArticleInResponse:\n    slug = get_slug_for_article(t)\n",
    );
    assert_eq!(
        out,
        vec![(
            "get_slug_for_article".to_string(),
            None,
            "app/x.py#create_new_article".to_string()
        )]
    );
}

/// Seals the SAME exclusion at the nesting depths a body-span rule could not reach: a method's own
/// decorator and parameter default, and a nested `def`'s. The span rule made the exclusion hold only for
/// a top-level `def`, because a body span begins at the first STATEMENT line — so a method's signature
/// sat INSIDE its class's body span and every default became an edge of the CLASS.
#[test]
fn decorators_and_parameter_defaults_are_not_edges_at_any_depth() {
    let out = names(
        "class V:\n    @cache_result()\n    def post(self, x = make_default()):\n        real_body()\n",
    );
    assert_eq!(
        out,
        vec![("real_body".to_string(), None, "app/x.py#V.post".to_string())]
    );
}

/// Seals the class-body exclusion, which is what keeps a Django view CLASS node an honest leaf: a URLconf
/// provide's `symbol` is the view class and `build_symbol_graph` mints no class->method edge, so a class
/// initializer edge would be the ONLY thing the BFS could reach from that node — and
/// `authentication_classes = (JWTAuthentication(),)` is an auth-shaped NAME that says nothing about
/// whether the handler checks authorization.
/// SHAPE FROM CORPUS: `corpus/oss/be-django/conduit/apps/articles/views.py` (class-level
/// `permission_classes` / `renderer_classes` / `serializer_class` initializers on every view).
#[test]
fn a_class_body_initializer_is_not_a_call_edge() {
    assert!(calls(
        "class V:\n    authentication_classes = (JWTAuthentication(),)\n    serializer_class = build_serializer()\n"
    )
    .is_empty());
}

/// Seals the NESTED-class half of the same exclusion, which the doc previously illustrated only with
/// initializers: a nested `class` is one of the class body's non-`def` statements, so it is skipped in
/// full, methods included. This is not a preference — `lang::symbols` mints no `Outer.Inner.m` symbol, so
/// walking in would attribute that method's calls to `Outer`, the class node the exclusion exists to keep
/// an honest leaf. The outer class's OWN method is unaffected.
#[test]
fn a_nested_classs_methods_are_skipped_and_never_land_on_the_outer_class() {
    let out = names(
        "class Outer:\n    class Inner:\n        def m(self):\n            inner_call()\n\n    def om(self):\n        outer_call()\n",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "outer_call");
    assert_eq!(out[0].2, "app/x.py#Outer.om");
}

/// Seals the equal-span tie-break: a ONE-LINE method body makes the class's body span and the method's
/// identical, and `lang::symbols` emits the class first — so "strictly smaller wins" handed the method's
/// calls to its class. `def get_queryset(self): return ...` is a real Python idiom.
#[test]
fn a_one_line_method_body_attributes_to_the_method_not_the_class() {
    let out = names("class V:\n    def get_queryset(self): return build_queryset()\n");
    assert_eq!(out[0].2, "app/x.py#V.get_queryset");
}

/// Seals per-scope receiver typing: Python reuses parameter names across a module's functions, and a
/// file-wide flat map let the LAST annotation win for every body — typing `a`'s `session` as
/// `SessionValidator`, a real class that is simply not the one `a` was handed.
#[test]
fn same_parameter_name_in_two_functions_keeps_each_functions_own_type() {
    let out = names(
        "def a(session: Session):\n    session.query()\n\ndef b(session: SessionValidator):\n    session.check()\n",
    );
    assert_eq!(out[0].1, Some("Session".to_string()));
    assert_eq!(out[1].1, Some("SessionValidator".to_string()));
}

/// Seals the `self`-outside-a-class behavior the module doc now states: there is no class to name, so the
/// call is DROPPED. (The verbatim fallback must not apply — `self` is not an identity any resolver could
/// match, and emitting it would put a `self`-receiver edge into the graph.)
#[test]
fn self_outside_a_class_method_body_drops_the_call() {
    assert!(calls("def f(self):\n    self.check_permissions()\n").is_empty());
}

/// Seals that a nested call inside an out-of-scope (chained) receiver is still collected on its own —
/// the walk never stops at the boundary.
#[test]
fn call_nested_in_an_unsupported_receiver_is_still_collected() {
    let out = names("def f():\n    a().b(guard_check())\n");
    let collected: Vec<&String> = out.iter().map(|c| &c.0).collect();
    assert!(collected.contains(&&"guard_check".to_string()), "{out:?}");
    assert!(collected.contains(&&"a".to_string()), "{out:?}");
    // `a().b(...)` itself is not emitted (chained receiver, never guessed).
    assert!(!collected.contains(&&"b".to_string()), "{out:?}");
}

/// Seals the parse-failure contract every extractor in this crate upholds.
#[test]
fn parse_failure_yields_no_calls() {
    assert!(calls("def f(:\n").is_empty());
}

/// Seals that a module-level call (no enclosing body) is dropped rather than attributed to nothing.
#[test]
fn module_level_call_is_dropped() {
    assert!(calls("app = FastAPI()\n").is_empty());
}
