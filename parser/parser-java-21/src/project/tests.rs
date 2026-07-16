//! Ported parity fixtures from `zzop_parser_java::project::tests`: cross-file literal `@RequestMapping`,
//! cross-file constant-reference resolution (the `Path.ASSET_PATH` shape), a `+`-concatenated constant
//! expression, class-scoped resolution surviving an unrelated same-named constant elsewhere in the
//! corpus, the ambiguous-qualifier-class skip, the CE-split class-hierarchy gate, and the full
//! `ResourceController`/`ResourceControllerCE`/`Path`/`PathCE` cross-file shape end to end.
use super::*;

fn keys(report: &ProjectProvidesReport) -> Vec<String> {
    let mut v: Vec<String> = report.provides.iter().map(|p| p.key.clone()).collect();
    v.sort();
    v
}

#[test]
fn cross_file_literal_prefix_still_resolves() {
    let files = vec![
        (
            "C.java".to_string(),
            "@RestController\n@RequestMapping(\"/x\")\nclass C {\n  @GetMapping(\"/y\")\n  void y() {}\n}\n"
                .to_string(),
        ),
        ("Other.java".to_string(), "class Unrelated {}\n".to_string()),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /x/y"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn cross_file_constant_reference_prefix_resolves() {
    let files = vec![
        (
            "Path.java".to_string(),
            "class Path {\n  public static final String ASSET_PATH = \"/asset\";\n}\n".to_string(),
        ),
        (
            "ResourceController.java".to_string(),
            "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /asset/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn concatenated_constant_expression_resolves_recursively() {
    let files = vec![
        (
            "Path.java".to_string(),
            "class Path {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n"
                .to_string(),
        ),
        (
            "ResourceController.java".to_string(),
            "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn class_scoped_resolution_survives_an_unrelated_same_named_constant_elsewhere() {
    let files = vec![
        (
            "Path.java".to_string(),
            "class Path {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n"
                .to_string(),
        ),
        (
            "unrelated/SomeService.java".to_string(),
            "class SomeService {\n    private static final String BASE_PATH = \"https://example.com/\";\n}\n"
                .to_string(),
        ),
        (
            "ResourceController.java".to_string(),
            "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn ambiguous_qualifier_class_name_is_skipped_not_guessed() {
    let files = vec![
        (
            "a/Path.java".to_string(),
            "class Path {\n  public static final String ASSET_PATH = \"/asset\";\n}\n".to_string(),
        ),
        (
            "b/Path.java".to_string(),
            "class Path {\n  public static final String ASSET_PATH = \"/other-asset\";\n}\n".to_string(),
        ),
        (
            "ResourceController.java".to_string(),
            "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert!(
        report.provides.is_empty(),
        "an ambiguous qualifier class must never guess a prefix, got: {:?}",
        report.provides
    );
    assert_eq!(report.skipped_unresolved_prefix, 1);
    assert_eq!(report.skipped_ambiguous_class_name, 2);
}

#[test]
fn ce_split_base_class_routes_are_reached_through_a_restcontroller_subclass_in_another_file() {
    let files = vec![
        (
            "ce/ResourceControllerCE.java".to_string(),
            "@RequestMapping(\"/asset\")\nclass ResourceControllerCE {\n  @GetMapping(\"/{id}\")\n  void getById() {}\n}\n"
                .to_string(),
        ),
        (
            "ResourceController.java".to_string(),
            "@RestController\n@RequestMapping(\"/asset\")\nclass ResourceController extends ResourceControllerCE {\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /asset/{}"]);
    let p = &report.provides[0];
    assert_eq!(p.file, "ce/ResourceControllerCE.java");
    assert_eq!(p.symbol.as_deref(), Some("getById"));
}

#[test]
fn a_base_class_with_no_restcontroller_descendant_anywhere_emits_nothing() {
    let files = vec![(
        "ce/OrphanCE.java".to_string(),
        "@RequestMapping(\"/orphan\")\nclass OrphanCE {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n"
            .to_string(),
    )];
    let report = extract_http_provides_project(&files);
    assert!(report.provides.is_empty());
}

#[test]
fn interface_constant_with_no_modifier_keywords_still_resolves() {
    let files = vec![
        (
            "Entity.java".to_string(),
            "public interface Entity {\n    String APPLICATIONS = \"applications\";\n}\n"
                .to_string(),
        ),
        (
            "Path.java".to_string(),
            "class Path {\n    static final String BASE_PATH = \"/api\";\n    public static final String APPLICATION_PATH = BASE_PATH + \"/\" + Entity.APPLICATIONS;\n}\n"
                .to_string(),
        ),
        (
            "ApplicationController.java".to_string(),
            "@RestController\n@RequestMapping(Path.APPLICATION_PATH)\nclass ApplicationController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/applications/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
}

#[test]
fn cross_file_base_class_and_constant_resolution_end_to_end() {
    let files = vec![
        (
            "constants/ce/PathCE.java".to_string(),
            "package com.example.app.constants.ce;\n\npublic class PathCE {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n".to_string(),
        ),
        (
            "constants/Path.java".to_string(),
            "package com.example.app.constants;\n\nimport com.example.app.constants.ce.PathCE;\n\npublic class Path extends PathCE {}\n".to_string(),
        ),
        (
            "controllers/ce/ResourceControllerCE.java".to_string(),
            "package com.example.app.controllers.ce;\n\nimport com.example.app.constants.Path;\nimport org.springframework.web.bind.annotation.GetMapping;\nimport org.springframework.web.bind.annotation.RequestMapping;\n\n@RequestMapping(Path.ASSET_PATH)\npublic class ResourceControllerCE {\n\n    @GetMapping(\"/{id}\")\n    public void getById() {}\n}\n".to_string(),
        ),
        (
            "controllers/ResourceController.java".to_string(),
            "package com.example.app.controllers;\n\nimport com.example.app.constants.Path;\nimport com.example.app.controllers.ce.ResourceControllerCE;\nimport org.springframework.web.bind.annotation.RequestMapping;\nimport org.springframework.web.bind.annotation.RestController;\n\n@RestController\n@RequestMapping(Path.ASSET_PATH)\npublic class ResourceController extends ResourceControllerCE {\n}\n".to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
    assert_eq!(report.skipped_unresolved_prefix, 0);
    assert_eq!(report.skipped_ambiguous_class_name, 0);
}

#[test]
fn a_nested_class_field_no_longer_leaks_into_the_enclosing_class_constant_scan() {
    // AST-native precision gain over the old lexical crate's documented limit (module doc): a nested
    // class's own `static final String` field must NOT resolve as if it belonged to the outer class.
    let files = vec![
        (
            "Path.java".to_string(),
            "class Path {\n    static final String OUTER = \"/outer\";\n    static class Inner {\n        static final String OUTER = \"/inner-shadow\";\n    }\n}\n"
                .to_string(),
        ),
        (
            "C.java".to_string(),
            "@RestController\n@RequestMapping(Path.OUTER)\nclass C {\n  @GetMapping(\"/y\")\n  void y() {}\n}\n"
                .to_string(),
        ),
    ];
    let report = extract_http_provides_project(&files);
    assert_eq!(keys(&report), vec!["GET /outer/y"]);
}
