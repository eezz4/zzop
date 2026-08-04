//! Coverage for `resolve_provide_response_refs`: successful ref resolution, missing/poisoned drops
//! (with aggregated warnings), the no-return-type sentinel strip + per-tree aggregated disclosure,
//! and the adapter-supplied pass-through.
use super::*;
use zzop_core::{ClassShapeFragment, IoProvide, ProvideBodyField, ProvideResponseShape};

fn shape(name: &str, fields: &[(&str, bool)], complete: bool) -> ClassShapeFragment {
    ClassShapeFragment {
        name: name.to_string(),
        fields: fields
            .iter()
            .map(|(n, optional)| ProvideBodyField {
                name: n.to_string(),
                optional: *optional,
            })
            .collect(),
        complete,
    }
}

fn provide(file: &str, line: u32, response: Option<ProvideResponseShape>) -> IoProvide {
    IoProvide {
        response,
        body: None,
        kind: "http".to_string(),
        key: "GET /api/users/{}".to_string(),
        file: file.to_string(),
        line,
        symbol: None,
    }
}

fn with_ref(file: &str, line: u32, dto_ref: &str) -> IoProvide {
    provide(
        file,
        line,
        Some(ProvideResponseShape {
            dto_ref: Some(dto_ref.to_string()),
            fields: Vec::new(),
            complete: false,
        }),
    )
}

fn sentinel(file: &str, line: u32) -> IoProvide {
    provide(
        file,
        line,
        Some(ProvideResponseShape {
            dto_ref: None,
            fields: Vec::new(),
            complete: false,
        }),
    )
}

fn merge_of(pairs: &[(&str, Vec<ClassShapeFragment>)]) -> ShapeMerge {
    let owned: Vec<(String, Vec<ClassShapeFragment>)> = pairs
        .iter()
        .map(|(f, v)| (f.to_string(), v.clone()))
        .collect();
    ShapeMerge::build(&owned)
}

#[test]
fn resolved_ref_copies_fields_and_complete_and_clears_dto_ref() {
    let mut provides = vec![with_ref("controller.ts", 10, "UserDto")];
    let merge = merge_of(&[(
        "dto.ts",
        vec![shape("UserDto", &[("id", false), ("email", true)], true)],
    )]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(warnings.is_empty());
    let resp = provides[0].response.as_ref().unwrap();
    assert_eq!(resp.dto_ref, None);
    assert!(resp.complete);
    let names: Vec<&str> = resp.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["id", "email"]);
}

#[test]
fn missing_ref_drops_the_whole_response_and_warns_with_a_count() {
    let mut provides = vec![
        with_ref("controller.ts", 10, "UserDto"),
        with_ref("controller.ts", 20, "UserDto"),
    ];
    let merge = merge_of(&[]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(provides[0].response.is_none());
    assert!(provides[1].response.is_none());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("UserDto"));
    assert!(warnings[0].contains("controller.ts"));
    assert!(warnings[0].contains("2 provides"));
}

#[test]
fn poisoned_ref_drops_the_response_and_warns_on_both_sides() {
    let mut provides = vec![with_ref("controller.ts", 10, "UserDto")];
    let merge = merge_of(&[
        ("a.ts", vec![shape("UserDto", &[("id", false)], true)]),
        (
            "b.ts",
            vec![shape("UserDto", &[("id", false), ("email", false)], true)],
        ),
    ]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(provides[0].response.is_none());
    assert_eq!(warnings.len(), 2);
    assert!(warnings.iter().any(|w| w.contains("conflicting")));
    assert!(warnings.iter().any(|w| w.contains("could not resolve")));
}

#[test]
fn sentinel_is_stripped_and_disclosed_as_one_aggregated_warning() {
    let mut provides = vec![
        sentinel("a.controller.ts", 5),
        sentinel("a.controller.ts", 9),
        sentinel("b.controller.ts", 3),
    ];
    let merge = merge_of(&[]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(
        provides.iter().all(|p| p.response.is_none()),
        "the sentinel must never survive assembly"
    );
    assert_eq!(warnings.len(), 1, "ONE aggregated disclosure: {warnings:?}");
    let w = &warnings[0];
    assert!(w.contains("3 route handlers"), "{w}");
    assert!(w.contains("2 files"), "{w}");
    assert!(w.contains("a.controller.ts"), "{w}");
    assert!(w.contains("declare a return type"), "{w}");
}

/// Seals the disclosure's counting unit: "N route handlers" must count HANDLERS, not sentinel
/// provides — an array-path decorator (`@Get(['a','b'])`) on one undeclared handler emits one
/// sentinel per path from the same `(file, line, symbol)`, and there is exactly one method for the
/// developer to annotate.
#[test]
fn array_path_sentinels_from_one_handler_are_disclosed_as_one_handler() {
    let mut a = sentinel("a.controller.ts", 5);
    a.key = "GET /users/me".to_string();
    let mut b = sentinel("a.controller.ts", 5);
    b.key = "GET /users/profile".to_string();
    let mut provides = vec![a, b];
    let merge = merge_of(&[]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("1 route handler across 1 file"),
        "one annotatable method must be counted once: {}",
        warnings[0]
    );
}

#[test]
fn adapter_supplied_resolved_fields_pass_through_untouched() {
    let resolved = ProvideResponseShape {
        dto_ref: None,
        fields: vec![ProvideBodyField {
            name: "id".to_string(),
            optional: false,
        }],
        complete: true,
    };
    let mut provides = vec![provide("overlay.ts", 1, Some(resolved.clone()))];
    let merge = merge_of(&[]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(warnings.is_empty());
    assert_eq!(provides[0].response.as_ref(), Some(&resolved));
}

/// `response: None` from a producer that never captures responses is NOT an undeclared handler
/// (only the parser's explicit sentinel is — its "declare a return type" advice would be wrong
/// here), but since 2026-08-03 it is no longer UNDISCLOSED either: the capture-less disclosure
/// (module doc §3) names it per tree, with its own wording — without it a 100% Express tree's
/// "0 response findings" was indistinguishable from a clean tree.
#[test]
fn capture_less_http_provide_is_left_untouched_and_disclosed_with_its_own_wording() {
    let mut provides = vec![provide("routes.go", 1, None)];
    let merge = merge_of(&[]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(provides[0].response.is_none(), "left untouched");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert!(w.contains("1 of 1 http route"), "{w}");
    assert!(w.contains("routes.go"), "{w}");
    assert!(w.contains("no response-shape evidence"), "{w}");
    assert!(
        w.contains("declaring a return type alone does not turn it on there"),
        "the capture-less advice is distinct from the sentinel's: {w}"
    );
    assert!(
        !w.contains("declare no return type"),
        "must not wear the sentinel disclosure's wording: {w}"
    );
}

/// The disclosure's scope pins: a non-`http` provide (db-table, trpc) has no response axis and joins
/// neither the numerator nor the denominator; a tree whose every http provide carries a response
/// (pure Nest, or overlay-resolved) gets NO capture-less warning.
#[test]
fn non_http_and_fully_captured_trees_get_no_capture_less_disclosure() {
    let mut db = provide("schema.prisma", 1, None);
    db.kind = "db-table".to_string();
    let nest = with_ref("nest.controller.ts", 3, "UserDto");
    let mut provides = vec![db, nest];
    let merge = merge_of(&[("dto.ts", vec![shape("UserDto", &[("id", false)], true)])]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert!(
        warnings.is_empty(),
        "no capture-less route exists here: {warnings:?}"
    );
}

/// Mixed tree: the counts must be exact — capture-less numerator over the http denominator, with
/// the sentinel (Some at entry) counted by ITS disclosure and not this one (entry-read disjointness,
/// module doc §3).
#[test]
fn mixed_tree_counts_capture_less_over_http_total_disjoint_from_the_sentinel() {
    let mut provides = vec![
        with_ref("nest.controller.ts", 3, "UserDto"), // captured — neither disclosure
        sentinel("nest.controller.ts", 9),            // undeclared — sentinel disclosure only
        provide("routes.ts", 1, None),                // capture-less
        provide("pages/api/x.ts", 2, None),           // capture-less
    ];
    let merge = merge_of(&[("dto.ts", vec![shape("UserDto", &[("id", false)], true)])]);
    let mut warnings = Vec::new();
    resolve_provide_response_refs(&mut provides, &merge, &mut warnings);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("1 route handler") && w.contains("declare no return type")),
        "the sentinel keeps its own disclosure: {warnings:?}"
    );
    let capture = warnings
        .iter()
        .find(|w| w.contains("no response-shape evidence"))
        .expect("capture-less disclosure present");
    assert!(capture.contains("2 of 4 http routes"), "{capture}");
    assert!(
        capture.contains("pages/api/x.ts") && capture.contains("routes.ts"),
        "{capture}"
    );
}
