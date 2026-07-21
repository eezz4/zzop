//! Additional `partial class` merge fixtures kept in a sibling file so neither test module crosses the
//! 300-line guard — the core partial-merge cases live in `tests.rs`; this file holds the controller-ness
//! edge case (an `[ApiController]` on a different half than the route method).

use super::*;

fn keys(report: &CSharpProjectProvidesReport) -> Vec<String> {
    let mut v: Vec<String> = report.provides.iter().map(|p| p.key.clone()).collect();
    v.sort();
    v
}

#[test]
fn partial_controllerness_from_the_attribute_half_reaches_the_method_half() {
    // The class is named `Api` (does NOT end in `Controller`), so a half is a controller ONLY via its
    // `[ApiController]` attribute. That attribute sits on the half with no methods; the route method sits on
    // a half with no attribute. The merge's `is_controller = any` plus controller-INDEPENDENT method
    // collection is what lets the route still emit — a per-half `is_controller` gate would have dropped it.
    let files = vec![
        (
            "Api.Attr.cs".to_string(),
            "[ApiController]\n[Route(\"api/things\")]\npublic partial class Api {\n}".to_string(),
        ),
        (
            "Api.Method.cs".to_string(),
            "public partial class Api {\n  [HttpGet(\"x\")]\n  public string X() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/things/x"]);
    assert_eq!(report.provides[0].file, "Api.Method.cs");
}

#[test]
fn a_non_controller_partial_class_emits_nothing_even_though_methods_are_collected() {
    // Methods are collected regardless of controller-ness, but a class that is neither `[ApiController]`/
    // `[Controller]`-attributed nor `*Controller`-named on ANY half must still emit no routes.
    let files = vec![
        (
            "Helper.A.cs".to_string(),
            "public partial class Helper {\n  [HttpGet(\"x\")]\n  public string X() { return \"\"; }\n}".to_string(),
        ),
        (
            "Helper.B.cs".to_string(),
            "public partial class Helper {\n  const string K = \"/k\"; }".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
}

#[test]
fn merge_picks_a_nonliteral_prefix_half_over_a_literal_empty_half() {
    // The merge's first-non-empty-prefix rule must pick a NON-LITERAL `[Route(Consts.BASE)]` half over a
    // half with no `[Route]` at all (a `Literal("")`), independent of file-sort order — the empty-`Literal`
    // half is skipped, so the resolvable non-literal prefix wins and its constant resolves post-merge.
    let files = vec![
        // Method half declares NO class prefix (Literal("")) — must NOT fix the merged prefix.
        (
            "Api.Method.cs".to_string(),
            "[ApiController]\npublic partial class ThingsController {\n  [HttpGet(\"x\")]\n  public string X() { return \"\"; }\n}".to_string(),
        ),
        // Prefix half carries a non-literal `[Route(Consts.BASE)]` and declares the constant it references.
        (
            "Api.Route.cs".to_string(),
            "[Route(Consts.BASE)]\npublic partial class ThingsController {\n  const string BASE_UNUSED = \"\";\n}".to_string(),
        ),
        (
            "Consts.cs".to_string(),
            "public static class Consts {\n  public const string BASE = \"api/things\";\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/things/x"]);
    assert_eq!(report.provides[0].file, "Api.Method.cs");
}
