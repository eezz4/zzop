//! Whole-corpus C# route constant-resolution fixtures — the C# parallel of
//! `zzop_parser_java_21::project::tests`: cross-file method-path + class-prefix constant resolution, the
//! out-of-corpus / ambiguous skips, and the per-file pass still dropping (unchanged).

use super::*;

fn keys(report: &CSharpProjectProvidesReport) -> Vec<String> {
    let mut v: Vec<String> = report.provides.iter().map(|p| p.key.clone()).collect();
    v.sort();
    v
}

#[test]
fn cross_file_method_path_constant_resolves() {
    let files = vec![
        (
            "Routes.cs".to_string(),
            "static class Routes { public const string List = \"/list\"; }".to_string(),
        ),
        (
            "UsersController.cs".to_string(),
            "[ApiController]\n[Route(\"api/[controller]\")]\npublic class UsersController {\n  [HttpGet(Routes.List)]\n  public string List() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/users/list"]);
    assert_eq!(report.skipped_unresolved_method_path, 0);
}

#[test]
fn cross_file_class_prefix_constant_resolves() {
    let files = vec![
        (
            "ApiRoutes.cs".to_string(),
            "static class ApiRoutes { public const string Base = \"api/things\"; }".to_string(),
        ),
        (
            "ThingsController.cs".to_string(),
            "[ApiController]\n[Route(ApiRoutes.Base)]\npublic class ThingsController {\n  [HttpGet(\"{id}\")]\n  public string Get(int id) { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/things/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn class_prefix_constant_with_controller_token_substitutes_after_resolution() {
    // A resolved prefix constant still gets `[controller]`-token substitution (lowercased class name minus
    // the `Controller` suffix), exactly as a literal `[Route("api/[controller]")]` would.
    let files = vec![
        (
            "R.cs".to_string(),
            "static class R { public const string Base = \"api/[controller]\"; }".to_string(),
        ),
        (
            "OrdersController.cs".to_string(),
            "[ApiController]\n[Route(R.Base)]\npublic class OrdersController {\n  [HttpGet]\n  public string All() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/orders"]);
}

#[test]
fn out_of_corpus_method_path_constant_is_skipped_and_counted() {
    // A method-path constant with no declaration anywhere in the corpus cannot resolve — skipped (never
    // keyed at the empty base) and counted. The controller's sibling literal route still emits.
    let files = vec![(
        "UsersController.cs".to_string(),
        "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(External.Unknown)]\n  public string List() { return \"\"; }\n  [HttpPost(\"create\")]\n  public string Create() { return \"\"; }\n}".to_string(),
    )];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["POST /api/create"]);
    assert_eq!(report.skipped_unresolved_method_path, 1);
}

#[test]
fn out_of_corpus_class_prefix_constant_blocks_the_controller_and_is_counted() {
    let files = vec![(
        "UsersController.cs".to_string(),
        "[ApiController]\n[Route(External.Base)]\npublic class UsersController {\n  [HttpGet(\"{id}\")]\n  public string Get(int id) { return \"\"; }\n}".to_string(),
    )];
    let report = extract_csharp_http_provides_project(&files);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
    assert_eq!(report.skipped_unresolved_prefix, 1);
}

#[test]
fn ambiguous_qualifier_class_name_is_skipped_not_guessed() {
    let files = vec![
        (
            "a/Routes.cs".to_string(),
            "static class Routes { public const string List = \"/list\"; }".to_string(),
        ),
        (
            "b/Routes.cs".to_string(),
            "static class Routes { public const string List = \"/other\"; }".to_string(),
        ),
        (
            "UsersController.cs".to_string(),
            "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(Routes.List)]\n  public string List() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
    assert_eq!(report.skipped_ambiguous_class_name, 2);
    assert_eq!(report.skipped_unresolved_method_path, 1);
}

#[test]
fn bare_const_reference_resolves_when_unique_but_is_ambiguous_across_two_classes() {
    // A bare `List` (no qualifier) resolves globally when declared in exactly one class...
    let unique = vec![
        (
            "Routes.cs".to_string(),
            "static class Routes { public const string List = \"/list\"; }".to_string(),
        ),
        (
            "UsersController.cs".to_string(),
            "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(List)]\n  public string L() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&unique);
    assert_eq!(keys(&report), vec!["GET /api/list"]);

    // ...but the SAME bare name declared in 2+ classes is ambiguous -> skipped, counted.
    let mut ambiguous = unique;
    ambiguous.push((
        "MoreRoutes.cs".to_string(),
        "static class MoreRoutes { public const string List = \"/other\"; }".to_string(),
    ));
    let report = extract_csharp_http_provides_project(&ambiguous);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
    assert_eq!(report.skipped_unresolved_method_path, 1);
}

#[test]
fn concatenated_const_stays_unresolved_v1_scope() {
    // v1 does NOT evaluate a `+`-concatenated constant — its route stays unresolved and is counted.
    let files = vec![
        (
            "Routes.cs".to_string(),
            "static class Routes { public const string List = \"/a\" + \"/b\"; }".to_string(),
        ),
        (
            "UsersController.cs".to_string(),
            "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(Routes.List)]\n  public string List() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
    assert_eq!(report.skipped_unresolved_method_path, 1);
}

#[test]
fn static_readonly_string_constant_resolves() {
    let files = vec![
        (
            "Routes.cs".to_string(),
            "class Routes { static readonly string List = \"/list\"; }".to_string(),
        ),
        (
            "UsersController.cs".to_string(),
            "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(Routes.List)]\n  public string List() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/list"]);
}

#[test]
fn partial_class_with_prefix_on_one_half_and_method_on_another_merges() {
    // The BLOCKING regression: a `partial class UsersController` carries `[ApiController][Route(...)]` (no
    // methods) in one file and a route method in another. Before the partial-aware merge the two same-name
    // rows were dropped as "ambiguous" -> `[]`; now they merge into one controller -> `GET /api/users/a`,
    // and the provide's `file` is the METHOD's file (the merged halves span files).
    let files = vec![
        (
            "UsersController.Api.cs".to_string(),
            "[ApiController]\n[Route(\"api/[controller]\")]\npublic partial class UsersController {\n}".to_string(),
        ),
        (
            "UsersController.Get.cs".to_string(),
            "public partial class UsersController {\n  [HttpGet(\"a\")]\n  public string A() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/users/a"]);
    assert_eq!(report.provides.len(), 1);
    assert_eq!(report.provides[0].file, "UsersController.Get.cs");
    assert_eq!(report.skipped_ambiguous_class_name, 0);
}

#[test]
fn partial_class_methods_split_across_both_halves_both_emit() {
    let files = vec![
        (
            "UsersController.A.cs".to_string(),
            "[ApiController]\n[Route(\"api/[controller]\")]\npublic partial class UsersController {\n  [HttpGet(\"a\")]\n  public string A() { return \"\"; }\n}".to_string(),
        ),
        (
            "UsersController.B.cs".to_string(),
            "public partial class UsersController {\n  [HttpPost(\"b\")]\n  public string B() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/users/a", "POST /api/users/b"]);
    // Each provide anchors on its own declaring file.
    let a = report
        .provides
        .iter()
        .find(|p| p.key == "GET /api/users/a")
        .unwrap();
    let b = report
        .provides
        .iter()
        .find(|p| p.key == "POST /api/users/b")
        .unwrap();
    assert_eq!(a.file, "UsersController.A.cs");
    assert_eq!(b.file, "UsersController.B.cs");
}

#[test]
fn partial_class_constant_declared_on_another_half_resolves_as_a_qualifier() {
    // A merged partial class is also a valid qualifier-resolution target: `UsersController.List` resolves
    // against the merged constants even though the const is declared on a different half than the reference.
    let files = vec![
        (
            "UsersController.Const.cs".to_string(),
            "public partial class UsersController {\n  const string List = \"/list\";\n}".to_string(),
        ),
        (
            "UsersController.Route.cs".to_string(),
            "[ApiController]\n[Route(\"api/[controller]\")]\npublic partial class UsersController {\n  [HttpGet(UsersController.List)]\n  public string L() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/users/list"]);
    assert_eq!(report.skipped_unresolved_method_path, 0);
}

#[test]
fn genuinely_distinct_non_partial_same_name_classes_still_drop_as_ambiguous() {
    // Two NON-partial classes that merely share a simple name are genuinely distinct -> still the (rare,
    // accepted) ambiguous drop, unchanged by the partial-aware merge.
    let files = vec![
        (
            "a/Widget.cs".to_string(),
            "[ApiController]\n[Route(\"a\")]\npublic class WidgetController {\n  [HttpGet(\"x\")]\n  public string X() { return \"\"; }\n}".to_string(),
        ),
        (
            "b/Widget.cs".to_string(),
            "[ApiController]\n[Route(\"b\")]\npublic class WidgetController {\n  [HttpGet(\"y\")]\n  public string Y() { return \"\"; }\n}".to_string(),
        ),
    ];
    let report = extract_csharp_http_provides_project(&files);
    assert!(report.provides.is_empty(), "{:?}", report.provides);
    assert_eq!(report.skipped_ambiguous_class_name, 2);
}

#[test]
fn per_file_pass_still_drops_the_non_literal_route_unchanged() {
    // The per-file pass has no corpus — its documented drop/block behavior is byte-identical after this change.
    let src = "[ApiController]\n[Route(\"api\")]\npublic class UsersController {\n  [HttpGet(Routes.List)]\n  public string List() { return \"\"; }\n  [HttpPost(\"create\")]\n  public string Create() { return \"\"; }\n}";
    let provides = crate::extract_csharp_http_provides("UsersController.cs", src);
    let mut got: Vec<&str> = provides.iter().map(|p| p.key.as_str()).collect();
    got.sort();
    assert_eq!(got, vec!["POST /api/create"]);
}

#[test]
fn minimal_api_routes_travel_through_the_project_pass_unchanged() {
    let files = vec![(
        "Program.cs".to_string(),
        "var app = builder.Build();\napp.MapGet(\"/health\", () => \"ok\");".to_string(),
    )];
    let report = extract_csharp_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /health"]);
}

#[test]
fn literal_routes_match_the_per_file_pass_so_replacement_is_behavior_neutral() {
    // A controller with only literal routes must produce the SAME keys the per-file pass does (the engine
    // REPLACES per-file C# http provides with this pass's output — literal routes must survive intact).
    let src = "[ApiController]\n[Route(\"api/[controller]\")]\npublic class UsersController {\n  [HttpGet(\"{id}\")]\n  public string Get(int id) { return \"\"; }\n}";
    let per_file = crate::extract_csharp_http_provides("UsersController.cs", src);
    let report = extract_csharp_http_provides_project(&[(
        "UsersController.cs".to_string(),
        src.to_string(),
    )]);
    let mut a: Vec<&str> = per_file.iter().map(|p| p.key.as_str()).collect();
    let mut b: Vec<&str> = report.provides.iter().map(|p| p.key.as_str()).collect();
    a.sort();
    b.sort();
    assert_eq!(a, b);
    assert_eq!(b, vec!["GET /api/users/{}"]);
}
