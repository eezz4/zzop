//! Coverage for `resolve_provide_body_refs`: successful ref resolution (fields/complete copied,
//! `dto_ref` cleared), a conflicting duplicate class name poisoning that name (with an aggregated
//! warning), a missing ref dropping the whole `body` (with an aggregated warning), and an identical
//! duplicate across 2 files resolving normally with no warning.
use super::*;
use zzop_core::{ClassShapeFragment, ProvideBodyField, ProvideBodyShape};

fn class(name: &str, fields: &[(&str, bool)], complete: bool) -> ClassShapeFragment {
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

fn provide_with_ref(file: &str, line: u32, dto_ref: &str) -> IoProvide {
    IoProvide {
        body: Some(ProvideBodyShape {
            sub_key: None,
            dto_ref: Some(dto_ref.to_string()),
            fields: Vec::new(),
            complete: false,
        }),
        kind: "http".to_string(),
        key: "POST /api/users".to_string(),
        file: file.to_string(),
        line,
        symbol: None,
    }
}

#[test]
fn resolved_ref_copies_fields_and_complete_and_clears_dto_ref() {
    let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
    let class_shapes = vec![(
        "dto.ts".to_string(),
        vec![class(
            "CreateUserDto",
            &[("name", false), ("nickname", true)],
            true,
        )],
    )];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
    assert!(warnings.is_empty());
    let body = provides[0].body.as_ref().unwrap();
    assert_eq!(body.dto_ref, None);
    assert!(body.complete);
    let names: Vec<&str> = body.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["name", "nickname"]);
    assert!(!body.fields[0].optional);
    assert!(body.fields[1].optional);
}

#[test]
fn conflicting_duplicate_class_shape_poisons_the_name_and_warns_on_both_sides() {
    // Two warnings are expected: one aggregated warning naming the class + conflicting files (the
    // MERGE step's honest-degrade), and one aggregated warning naming the dropped provide(s) (the
    // PROVIDE-resolution step's honest-degrade) — distinct concerns, both disclosed.
    let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
    let class_shapes = vec![
        (
            "a.ts".to_string(),
            vec![class("CreateUserDto", &[("name", false)], true)],
        ),
        (
            "b.ts".to_string(),
            vec![class(
                "CreateUserDto",
                &[("name", false), ("email", false)],
                true,
            )],
        ),
    ];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
    assert!(provides[0].body.is_none(), "poisoned ref drops the body");
    assert_eq!(warnings.len(), 2);
    let conflict_warning = warnings
        .iter()
        .find(|w| w.contains("conflicting"))
        .expect("a conflicting-shape warning");
    assert!(conflict_warning.contains("CreateUserDto"));
    assert!(conflict_warning.contains("a.ts"));
    assert!(conflict_warning.contains("b.ts"));
    let drop_warning = warnings
        .iter()
        .find(|w| w.contains("could not resolve"))
        .expect("a dropped-provide warning");
    assert!(drop_warning.contains("CreateUserDto"));
    assert!(drop_warning.contains("controller.ts"));
}

#[test]
fn unreferenced_conflicting_class_shape_stays_silent() {
    // Class-shape fragments cover EVERY class declaration, so same-name/different-shape
    // non-DTO classes (`Config`, `Options`, ...) are common and legitimate — a collision no
    // provide's `dto_ref` references must not warn (that would disclose a drop that never
    // happened: a phantom disclosure).
    let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
    let class_shapes = vec![
        (
            "a.ts".to_string(),
            vec![
                class("CreateUserDto", &[("name", false)], true),
                class("Options", &[("a", false)], true),
            ],
        ),
        (
            "b.ts".to_string(),
            vec![class("Options", &[("b", false)], true)],
        ),
    ];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
    assert!(
        warnings.is_empty(),
        "unreferenced collision must not warn: {warnings:?}"
    );
    let body = provides[0].body.as_ref().unwrap();
    assert_eq!(
        body.dto_ref, None,
        "the referenced ref still resolves normally"
    );
}

#[test]
fn identical_duplicate_class_shape_across_two_files_resolves_without_warning() {
    let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
    let class_shapes = vec![
        (
            "a.ts".to_string(),
            vec![class("CreateUserDto", &[("name", false)], true)],
        ),
        (
            "b.ts".to_string(),
            vec![class("CreateUserDto", &[("name", false)], true)],
        ),
    ];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
    assert!(warnings.is_empty());
    let body = provides[0].body.as_ref().unwrap();
    assert_eq!(body.dto_ref, None);
}

#[test]
fn missing_ref_drops_the_whole_body_and_warns_with_a_count() {
    let mut provides = vec![
        provide_with_ref("controller.ts", 10, "CreateUserDto"),
        provide_with_ref("controller.ts", 20, "CreateUserDto"),
    ];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, Vec::new(), &mut warnings);
    assert!(provides[0].body.is_none());
    assert!(provides[1].body.is_none());
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("CreateUserDto"));
    assert!(warnings[0].contains("controller.ts"));
    assert!(warnings[0].contains("2 provides"));
}

#[test]
fn provide_with_no_dto_ref_is_left_untouched() {
    let mut provides = vec![IoProvide {
        body: Some(ProvideBodyShape {
            sub_key: None,
            dto_ref: None,
            fields: vec![ProvideBodyField {
                name: "name".to_string(),
                optional: false,
            }],
            complete: true,
        }),
        kind: "http".to_string(),
        key: "POST /api/users".to_string(),
        file: "controller.ts".to_string(),
        line: 1,
        symbol: None,
    }];
    let mut warnings = Vec::new();
    resolve_provide_body_refs(&mut provides, Vec::new(), &mut warnings);
    assert!(warnings.is_empty());
    assert!(provides[0].body.is_some());
}
