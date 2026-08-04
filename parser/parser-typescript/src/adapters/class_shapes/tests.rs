//! Coverage for `extract_class_shape_fragments`: field capture (Ident/Str keys), optionality
//! (`?` and `@IsOptional()`), each `complete: false` driver, the field-less-but-emitted case,
//! multi-class source order — and the interface arm (`response-shape-v1`'s common referent):
//! property signatures, `?` optionality, `extends`/index-signature incompleteness, and
//! method-signature skipping.
use super::*;

fn names(f: &ClassShapeFragment) -> Vec<&str> {
    f.fields.iter().map(|x| x.name.as_str()).collect()
}

#[test]
fn class_validator_dto_fields_are_required_by_default() {
    let src = concat!(
        "class CreateUserDto {\n",
        "  @IsNotEmpty() readonly email: string;\n",
        "  @IsNotEmpty() readonly name: string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1);
    let f = &out[0];
    assert_eq!(f.name, "CreateUserDto");
    assert_eq!(names(f), vec!["email", "name"]);
    assert!(f.fields.iter().all(|x| !x.optional));
    assert!(f.complete);
}

#[test]
fn question_mark_and_is_optional_decorator_both_mark_optional() {
    let src = concat!(
        "class UpdateUserDto {\n",
        "  name?: string;\n",
        "  @IsOptional() email: string;\n",
        "  required: string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    let f = &out[0];
    let optional_of = |n: &str| f.fields.iter().find(|x| x.name == n).unwrap().optional;
    assert!(optional_of("name"));
    assert!(optional_of("email"));
    assert!(!optional_of("required"));
}

#[test]
fn extends_clause_marks_incomplete() {
    let src = "class UpdateUserDto extends PartialType(CreateUserDto) {\n  extra: string;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", src);
    assert!(!out[0].complete);
    assert_eq!(names(&out[0]), vec!["extra"]);
}

#[test]
fn constructor_param_properties_mark_incomplete() {
    let src = concat!(
        "class CreateUserDto {\n",
        "  constructor(private readonly email: string) {}\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert!(!out[0].complete);
    assert!(out[0].fields.is_empty());
}

#[test]
fn index_signature_marks_incomplete() {
    let src = concat!(
        "class LooseDto {\n",
        "  known: string;\n",
        "  [key: string]: unknown;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert!(!out[0].complete);
    assert_eq!(names(&out[0]), vec!["known"]);
}

#[test]
fn computed_property_key_marks_incomplete_and_is_skipped_as_a_field() {
    let src = concat!(
        "const KEY = 'dynamic';\n",
        "class LooseDto {\n",
        "  known: string;\n",
        "  [KEY]: string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert!(!out[0].complete);
    assert_eq!(names(&out[0]), vec!["known"]);
}

#[test]
fn static_and_method_members_are_skipped() {
    let src = concat!(
        "class Service {\n",
        "  static VERSION = '1';\n",
        "  name: string;\n",
        "  greet() { return this.name; }\n",
        "  get upper() { return this.name; }\n",
        "  set upper(v: string) { this.name = v; }\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["name"]);
    assert!(out[0].complete);
}

#[test]
fn private_members_are_skipped() {
    let src = "class Service {\n  #secret = 'x';\n  name: string;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["name"]);
}

#[test]
fn field_less_extends_class_is_still_emitted_as_incomplete() {
    let src = "class UpdateUserDto extends PartialType(CreateUserDto) {}\n";
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(
        out.len(),
        1,
        "a field-less class must still be emitted: {out:?}"
    );
    assert_eq!(out[0].name, "UpdateUserDto");
    assert!(out[0].fields.is_empty());
    assert!(!out[0].complete);
}

#[test]
fn two_classes_in_one_file_both_emitted_in_source_order() {
    let src = concat!(
        "class First {\n  a: string;\n}\n\n",
        "export class Second {\n  b: string;\n}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].name, "First");
    assert_eq!(out[1].name, "Second");
}

#[test]
fn string_literal_key_is_captured_as_a_field() {
    let src = "class Dto {\n  'weird-name': string;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["weird-name"]);
}

#[test]
fn empty_file_yields_no_fragments() {
    assert!(extract_class_shape_fragments("e.ts", "").is_empty());
}

#[test]
fn class_expression_is_not_detected_v1_scope() {
    let src = "const Dto = class {\n  name: string;\n};\n";
    assert!(extract_class_shape_fragments("dto.ts", src).is_empty());
}

// ---- interface arm ----

#[test]
fn interface_property_signatures_are_captured_with_optionality() {
    let src = concat!(
        "export interface UserProfile {\n",
        "  id: string;\n",
        "  email?: string;\n",
        "  'weird-name': number;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1);
    let f = &out[0];
    assert_eq!(f.name, "UserProfile");
    assert_eq!(names(f), vec!["id", "email", "weird-name"]);
    let optional_of = |n: &str| f.fields.iter().find(|x| x.name == n).unwrap().optional;
    assert!(!optional_of("id"));
    assert!(optional_of("email"));
    assert!(f.complete);
}

#[test]
fn interface_extends_and_index_signature_mark_incomplete() {
    let ext = "interface A extends Base {\n  x: string;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", ext);
    assert!(!out[0].complete);
    assert_eq!(names(&out[0]), vec!["x"]);

    let idx = "interface B {\n  known: string;\n  [key: string]: unknown;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", idx);
    assert!(!out[0].complete);
    assert_eq!(names(&out[0]), vec!["known"]);
}

/// The class arm's computed-key contract, mirrored (module doc: "for an interface: an `extends`
/// clause, an index signature, or a computed key"): a computed key is NOT a field named after its
/// key EXPRESSION — `[SECRET]: string` declares whatever `SECRET` holds at runtime, never a field
/// named `SECRET`. Capturing the expression text would hand a phantom name to the sensitive-field
/// vocabulary AND let `complete: true` license body-field-drift's extra-key/missing-field checks.
/// The discriminator is `prop.computed` alone — a non-computed quoted key stays a legitimate capture
/// (pinned below).
#[test]
fn interface_computed_ident_key_marks_incomplete_and_is_not_a_field() {
    let src = concat!(
        "const SECRET = 'apiKey';\n",
        "interface TokenDto {\n",
        "  id: string;\n",
        "  [SECRET]: string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(names(&out[0]), vec!["id"], "{out:?}");
    assert!(!out[0].complete, "{out:?}");
}

#[test]
fn interface_computed_string_literal_key_marks_incomplete_and_is_not_a_field() {
    let src = concat!(
        "interface TokenDto {\n",
        "  id: string;\n",
        "  ['secretKey']: string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(names(&out[0]), vec!["id"], "{out:?}");
    assert!(!out[0].complete, "{out:?}");
}

/// The positive twin: a NON-computed quoted key (`'quoted-key': string`) is a statically-known
/// field name and must keep being captured — `computed`, not the literal-string key shape, is the
/// discriminator.
#[test]
fn interface_non_computed_quoted_key_is_still_captured() {
    let src = "interface Dto {\n  'quoted-key': string;\n}\n";
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["quoted-key"]);
    assert!(out[0].complete);
}

#[test]
fn interface_method_signatures_are_skipped_and_do_not_affect_completeness() {
    let src = concat!(
        "interface Svc {\n",
        "  name: string;\n",
        "  greet(): void;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["name"]);
    assert!(out[0].complete);
}

/// An interface GETTER signature is an own READABLE property of the type (`interface R { get
/// password(): string }` is structurally satisfied by `{ password: "…" }`) — so it is a field, with
/// the interface's usual computed-key contract (computed key -> incomplete, never a phantom name).
/// The CLASS arm deliberately keeps skipping getters (prototype accessor, not serialized by
/// `JSON.stringify` by default) — pinned by `static_and_method_members_are_skipped` above.
#[test]
fn interface_getter_signature_is_captured_as_a_readable_field() {
    let src = concat!(
        "interface SessionView {\n",
        "  id: string;\n",
        "  get password(): string;\n",
        "  get 'quoted-token'(): string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(
        names(&out[0]),
        vec!["id", "password", "quoted-token"],
        "{out:?}"
    );
    assert!(
        out[0].fields.iter().all(|f| !f.optional),
        "a getter member cannot be optional: {out:?}"
    );
    assert!(out[0].complete, "{out:?}");
}

#[test]
fn interface_computed_getter_key_marks_incomplete_and_is_not_a_field() {
    let src = concat!(
        "const SECRET = 'apiKey';\n",
        "interface TokenDto {\n",
        "  id: string;\n",
        "  get [SECRET](): string;\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(names(&out[0]), vec!["id"], "{out:?}");
    assert!(!out[0].complete, "{out:?}");
}

/// A SETTER signature is write-only — nothing a response serializes — so it is neither a field nor
/// an incompleteness driver (deliberate silence, module doc).
#[test]
fn interface_setter_signature_stays_skipped_and_does_not_affect_completeness() {
    let src = concat!(
        "interface SessionView {\n",
        "  id: string;\n",
        "  set password(v: string);\n",
        "}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(names(&out[0]), vec!["id"], "{out:?}");
    assert!(out[0].complete, "{out:?}");
}

#[test]
fn class_and_interface_in_one_file_both_emitted_in_source_order() {
    let src = concat!(
        "interface First {\n  a: string;\n}\n\n",
        "export class Second {\n  b: string;\n}\n"
    );
    let out = extract_class_shape_fragments("dto.ts", src);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].name, "First");
    assert_eq!(out[1].name, "Second");
}
