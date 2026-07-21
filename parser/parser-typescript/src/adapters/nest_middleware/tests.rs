//! Coverage for `extract_nest_forroutes_guarded`: the be-nest `configure(consumer)` shape, the auth-name
//! gate (non-auth middleware ignored), object vs string forRoutes args, method/path normalization, and the
//! never-guess cases (controller ref, test file).

use super::extract_nest_forroutes_guarded;

const BE_NEST_SHAPE: &str = "\
export class ArticleModule implements NestModule {
  public configure(consumer: MiddlewareConsumer) {
    consumer
      .apply(AuthMiddleware)
      .forRoutes(
        {path: 'articles/feed', method: RequestMethod.GET},
        {path: 'articles', method: RequestMethod.POST},
        {path: 'articles/:slug', method: RequestMethod.DELETE},
        {path: 'articles/:slug/comments', method: RequestMethod.POST});
  }
}
";

#[test]
fn extracts_the_be_nest_forroutes_patterns_normalized() {
    let mut got = extract_nest_forroutes_guarded("article.module.ts", BE_NEST_SHAPE);
    got.sort();
    assert_eq!(
        got,
        vec![
            ("DELETE".to_string(), "articles/{}".to_string()),
            ("GET".to_string(), "articles/feed".to_string()),
            ("POST".to_string(), "articles".to_string()),
            ("POST".to_string(), "articles/{}/comments".to_string()),
        ],
        "`:slug` -> `{{}}`, verbs uppercased from RequestMethod.X"
    );
}

#[test]
fn a_non_auth_middleware_is_ignored() {
    // `LoggerMiddleware` is not auth — its forRoutes must NOT exempt anything (would be a false-clear).
    let src = "\
export class M implements NestModule {
  configure(consumer: MiddlewareConsumer) {
    consumer.apply(LoggerMiddleware).forRoutes({path: 'articles', method: RequestMethod.POST});
  }
}
";
    assert!(extract_nest_forroutes_guarded("m.ts", src).is_empty());
}

#[test]
fn a_bare_string_forroutes_arg_is_all_methods() {
    let src = "\
export class M implements NestModule {
  configure(c: MiddlewareConsumer) {
    c.apply(JwtMiddleware).forRoutes('profiles/:username');
  }
}
";
    assert_eq!(
        extract_nest_forroutes_guarded("m.ts", src),
        vec![("*".to_string(), "profiles/{}".to_string())]
    );
}

#[test]
fn an_omitted_method_is_all_methods() {
    let src = "\
export class M { configure(c) { c.apply(AuthMiddleware).forRoutes({path: 'articles'}); } }
";
    assert_eq!(
        extract_nest_forroutes_guarded("m.ts", src),
        vec![("*".to_string(), "articles".to_string())]
    );
}

#[test]
fn request_method_all_normalizes_to_star() {
    let src = "\
export class M { configure(c) { c.apply(AuthMiddleware).forRoutes({path: 'x', method: RequestMethod.ALL}); } }
";
    assert_eq!(
        extract_nest_forroutes_guarded("m.ts", src),
        vec![("*".to_string(), "x".to_string())]
    );
}

#[test]
fn a_controller_ref_forroutes_arg_is_not_guessed() {
    // `forRoutes(ArticleController)` — the covered routes aren't decidable from this call, so nothing.
    let src = "\
export class M { configure(c) { c.apply(AuthMiddleware).forRoutes(ArticleController); } }
";
    assert!(extract_nest_forroutes_guarded("m.ts", src).is_empty());
}

#[test]
fn a_file_without_forroutes_is_pre_skipped() {
    let src = "export class M { configure(c) { c.apply(AuthMiddleware); } }\n";
    assert!(extract_nest_forroutes_guarded("m.ts", src).is_empty());
}

#[test]
fn a_test_file_is_skipped() {
    assert!(extract_nest_forroutes_guarded("article.module.spec.ts", BE_NEST_SHAPE).is_empty());
}

#[test]
fn a_string_literal_method_is_accepted() {
    let src = "\
export class M { configure(c) { c.apply(AuthGuardMiddleware).forRoutes({path: 'x', method: 'PUT'}); } }
";
    assert_eq!(
        extract_nest_forroutes_guarded("m.ts", src),
        vec![("PUT".to_string(), "x".to_string())]
    );
}
