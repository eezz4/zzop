//! Class field-shape extraction (`body-shape-v1`) — the per-file DTO half of
//! `cross-layer/body-field-drift`'s provide side.
//!
//! Emits one `zzop_core::ClassShapeFragment` per class DECLARATION (`ClassDecl` — top-level and
//! `export`ed alike; a `ClassExpr` assignment is out of scope for v1 and is not detected). A fragment
//! is emitted for EVERY class declaration, even one with no extractable fields at all — a field-less
//! `extends PartialType(CreateUserDto) {}` still resolves as "found but incomplete", which is a
//! different, more informative signal than "not found" (the assemble-time consumer treats a missing
//! name as unresolvable, but a present-and-incomplete one as "known partial shape"). Fields carry
//! their name plus per-field optionality (`?` or an `@IsOptional()` decorator) and the class overall
//! carries a `complete` flag; the tree-wide merge and `IoProvide::body.dto_ref` resolution happen at
//! assemble time (see `zzop_core::ClassShapeFragment`'s doc for the never-guess resolution contract).
//!
//! `complete: false` when the class's field list may be partial — an `extends` clause (mapped types
//! like `PartialType(CreateUserDto)` included), a constructor with a parameter property (`constructor
//! (private readonly x: string)` declares a field the property list alone would miss), an index
//! signature, or a computed property key (which may hide an arbitrary field name). Methods/getters/
//! setters/static members/private (`#x`) members are NOT JSON body fields and do not affect
//! completeness either way.

use swc_core::ecma::ast::{
    Callee, ClassDecl, ClassMember, Constructor, Decorator, Expr, ParamOrTsParamProp, PropName,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{ClassShapeFragment, ProvideBodyField};

/// Extracts every class declaration's field shape from one file. Returns an empty vec for files
/// that fail to parse or declare no class declarations — graceful degrade, mirroring the sibling
/// fragment extractors.
pub fn extract_class_shape_fragments(rel: &str, text: &str) -> Vec<ClassShapeFragment> {
    let Some((_cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let mut c = ClassShapeCollector { out: Vec::new() };
    module.visit_with(&mut c);
    c.out
}

struct ClassShapeCollector {
    out: Vec<ClassShapeFragment>,
}

impl Visit for ClassShapeCollector {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        let name = n.ident.sym.to_string();
        let mut fields = Vec::new();
        // An `extends` clause (incl. mapped-type shapes like `PartialType(CreateUserDto)`) may
        // declare fields this file can't see -- incomplete from the start.
        let mut complete = n.class.super_class.is_none();

        for member in &n.class.body {
            match member {
                ClassMember::ClassProp(prop) => {
                    if prop.is_static {
                        continue; // not an instance field
                    }
                    match &prop.key {
                        PropName::Ident(id) => {
                            let optional = prop.is_optional || has_is_optional(&prop.decorators);
                            fields.push(ProvideBodyField {
                                name: id.sym.to_string(),
                                optional,
                            });
                        }
                        PropName::Str(s) => {
                            let optional = prop.is_optional || has_is_optional(&prop.decorators);
                            fields.push(ProvideBodyField {
                                name: s.value.as_str().unwrap_or_default().to_string(),
                                optional,
                            });
                        }
                        PropName::Computed(_) => {
                            complete = false; // may hide an arbitrary field name
                        }
                        PropName::Num(_) | PropName::BigInt(_) => {
                            // Not a statically-known JSON-body field name; not a completeness
                            // driver either (unlike a truly dynamic computed key).
                        }
                    }
                }
                ClassMember::Constructor(ctor) => {
                    if has_param_props(ctor) {
                        complete = false; // a ctor parameter property declares a field
                    }
                }
                ClassMember::TsIndexSignature(_) => {
                    complete = false; // arbitrary extra keys may exist
                }
                // Methods/getters/setters/private props/static blocks/auto-accessors/empty stmts
                // are not JSON body fields and don't affect completeness.
                _ => {}
            }
        }

        self.out.push(ClassShapeFragment {
            name,
            fields,
            complete,
        });
        n.visit_children_with(self); // recurse -- covers any nested class declarations
    }
}

/// Whether a constructor declares any TypeScript parameter property (`constructor(private x:
/// string)`) — each one declares an instance field that the class body's own property list would
/// otherwise miss entirely.
fn has_param_props(ctor: &Constructor) -> bool {
    ctor.params
        .iter()
        .any(|p| matches!(p, ParamOrTsParamProp::TsParamProp(_)))
}

/// Whether any of a property's decorators is `@IsOptional()` (class-validator's optionality marker) —
/// matched by lexical name only, same tradeoff as `adapters::controller_decorators`'s own decorator
/// matching (import source is never verified).
fn has_is_optional(decorators: &[Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| decorator_name(&d.expr).as_deref() == Some("IsOptional"))
}

/// The decorator's callee/identifier name: `IsOptional` from both bare `@IsOptional` and called
/// `@IsOptional(...)`. `None` for any unrecognized shape (a member expression, a non-identifier
/// callee, ...). Deliberately a small local duplicate of
/// `adapters::controller_decorators::decorator_name` rather than a shared cross-module helper --
/// each adapter module in this crate is a self-contained framework-vocabulary recognizer.
fn decorator_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Call(call) => match &call.callee {
            Callee::Expr(callee) => match &**callee {
                Expr::Ident(id) => Some(id.sym.to_string()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_class_shape_fragments`: field capture (Ident/Str keys), optionality
    //! (`?` and `@IsOptional()`), each `complete: false` driver, the field-less-but-emitted case, and
    //! multi-class source order.
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
}
