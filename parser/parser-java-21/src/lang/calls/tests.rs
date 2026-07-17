//! Coverage for `parse_calls`: same-file call attribution, lambda-body traversal, and receiver typing.
use super::*;

#[test]
fn bare_call_from_symbol_is_enclosing_method() {
    let calls = parse_calls(
        "C.java",
        "class C {\n  void foo() { bar(); }\n  void bar() {}\n}\n",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "C.java#C.foo");
    assert_eq!(calls[0].callee_name, "bar");
    assert_eq!(calls[0].receiver_type, None);
}

#[test]
fn qualified_call_on_untracked_identifier_records_verbatim_receiver() {
    // Static-call idiom: `AuthorizationService.canWriteComment(...)` — the class name is spelled
    // directly at the call site, not tracked via any field/local/param declaration.
    let calls = parse_calls(
        "C.java",
        "class C {\n  void foo() {\n    AuthorizationService.canWriteComment(a, b);\n  }\n}\n",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "C.java#C.foo");
    assert_eq!(calls[0].callee_name, "canWriteComment");
    assert_eq!(
        calls[0].receiver_type.as_deref(),
        Some("AuthorizationService")
    );
}

#[test]
fn qualified_call_on_tracked_field_uses_declared_type() {
    let calls = parse_calls(
        "C.java",
        "class C {\n  private ArticleRepository articleRepository;\n  void foo() {\n    articleRepository.findBySlug(\"x\");\n  }\n}\n",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].callee_name, "findBySlug");
    assert_eq!(calls[0].receiver_type.as_deref(), Some("ArticleRepository"));
}

#[test]
fn call_inside_lambda_body_attributed_to_enclosing_method() {
    // The exact shape this extractor was built for: a guard call nested inside a `.map(x -> {...})`
    // lambda still resolves to the ENCLOSING method's body span, not lost.
    let src = "class C {\n  Object deleteComment() {\n    return repo.findById(\"x\")\n      .map(comment -> {\n        if (!AuthorizationService.canWriteComment(u, a, comment)) {\n          throw new RuntimeException();\n        }\n        repo.remove(comment);\n        return null;\n      })\n      .orElseThrow(RuntimeException::new);\n  }\n}\n";
    let calls = parse_calls("C.java", src);
    let guard = calls
        .iter()
        .find(|c| c.callee_name == "canWriteComment")
        .expect("expected the lambda-nested guard call to be collected");
    assert_eq!(guard.from_symbol, "C.java#C.deleteComment");
    assert_eq!(guard.receiver_type.as_deref(), Some("AuthorizationService"));
}

#[test]
fn call_in_a_field_initializer_attributes_to_the_enclosing_class_body() {
    // Unlike TS (a bare top-level call can sit outside every function/class), Java has no such position
    // — a field initializer's call still falls inside the enclosing CLASS's own body span (no smaller
    // method/constructor span exists here), so it attributes to the class symbol rather than being
    // dropped.
    let calls = parse_calls("C.java", "class C {\n  int x = compute();\n}\n");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "C.java#C");
    assert_eq!(calls[0].callee_name, "compute");
}

#[test]
fn line_is_one_based_call_site_line() {
    let calls = parse_calls(
        "C.java",
        "class C {\n  void foo() {\n\n    bar();\n  }\n  void bar() {}\n}\n",
    );
    assert_eq!(calls[0].line, 4);
}

#[test]
fn qualified_call_on_this_is_skipped_out_of_scope() {
    let calls = parse_calls(
        "C.java",
        "class C {\n  void foo() { this.helper(); }\n  void helper() {}\n}\n",
    );
    assert!(calls.is_empty());
}

#[test]
fn multiple_calls_inside_one_method_all_collected() {
    let calls = parse_calls(
        "C.java",
        "class C {\n  void main() {\n    a();\n    b();\n    c();\n  }\n}\n",
    );
    let names: Vec<&str> = calls.iter().map(|c| c.callee_name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    assert!(calls.iter().all(|c| c.from_symbol == "C.java#C.main"));
}

#[test]
fn parse_calls_empty_on_parse_failure() {
    assert!(parse_calls("bad.java", "\u{0}\u{1}\u{2}not java{{{{").is_empty());
}
