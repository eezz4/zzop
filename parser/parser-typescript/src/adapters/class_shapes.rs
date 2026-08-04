//! Class/interface field-shape extraction (`body-shape-v1` + `response-shape-v1`) — the per-file DTO
//! half of `cross-layer/body-field-drift`'s provide side, and (since `response-shape-v1`) of the
//! declared-response-shape resolution the response-consuming rules read.
//!
//! Emits one `zzop_core::ClassShapeFragment` per class DECLARATION (`ClassDecl` — top-level and
//! `export`ed alike; a `ClassExpr` assignment is out of scope for v1 and is not detected) AND per
//! `interface` declaration (`TsInterfaceDecl` — a declared return type commonly names an interface,
//! and the projection contract's declaration-based response reading covers "the class/interface the
//! annotation points at"). A fragment is emitted for EVERY such declaration, even one with no
//! extractable fields at all — a field-less `extends PartialType(CreateUserDto) {}` still resolves as
//! "found but incomplete", which is a different, more informative signal than "not found" (the
//! assemble-time consumer treats a missing name as unresolvable, but a present-and-incomplete one as
//! "known partial shape"). Fields carry their name plus per-field optionality (`?` or an
//! `@IsOptional()` decorator; interfaces have no decorators, so `?` only) and the declaration overall
//! carries a `complete` flag; the tree-wide merge and `IoProvide::body.dto_ref` /
//! `IoProvide::response.dto_ref` resolution happen at assemble time (see
//! `zzop_core::ClassShapeFragment`'s doc for the never-guess resolution contract). A class and an
//! interface sharing one name, or TypeScript's legitimate interface declaration-merging across files,
//! surface as CONFLICTING shapes there and poison the name (dropped + warned, never guessed).
//!
//! `complete: false` when the declaration's field list may be partial — for a class: an `extends`
//! clause (mapped types like `PartialType(CreateUserDto)` included), a constructor with a parameter
//! property (`constructor(private readonly x: string)` declares a field the property list alone would
//! miss), an index signature, or a computed property key (which may hide an arbitrary field name);
//! for an interface: an `extends` clause, an index signature, or a computed key (getter keys
//! included). Methods/static members/private (`#x`) members/call+construct signatures are NOT JSON
//! body fields and do not affect completeness either way.
//!
//! ## Getters split by DIRECTION of declaration, not by accessor syntax (2026-08-03)
//!
//! An INTERFACE getter signature (`interface R { get password(): string }`) IS a field: an interface
//! member declares the type's readable surface regardless of how an implementation provides it —
//! `{ password: "…" }` satisfies it structurally, and the `class-transformer` `@Expose()`-getter
//! idiom makes it real wire surface on the response side. Dropping it was a silent FN plus a
//! `complete: true` over-claim. A CLASS getter stays skipped: it is a PROTOTYPE accessor, which
//! `JSON.stringify` does not serialize by default, so capturing it would claim wire surface the
//! default serialization never emits. A SETTER signature is write-only on either declaration form —
//! structurally satisfiable with no readable property at all — so it is dropped without touching
//! completeness (deliberate silence, not a gap).

use swc_core::ecma::ast::{
    Callee, ClassDecl, ClassMember, Constructor, Decorator, Expr, ParamOrTsParamProp, PropName,
    TsInterfaceDecl, TsTypeElement,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{ClassShapeFragment, ProvideBodyField};

/// Extracts every class and interface declaration's field shape from one file. Returns an empty vec
/// for files that fail to parse or declare neither — graceful degrade, mirroring the sibling
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
                    let optional = prop.is_optional || has_is_optional(&prop.decorators);
                    match &prop.key {
                        PropName::Ident(id) => {
                            fields.push(ProvideBodyField {
                                name: id.sym.to_string(),
                                optional,
                            });
                        }
                        PropName::Str(s) => {
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

    fn visit_ts_interface_decl(&mut self, n: &TsInterfaceDecl) {
        let name = n.id.sym.to_string();
        let mut fields = Vec::new();
        // `extends` may declare members this file can't see -- incomplete from the start, same
        // driver as a class's superclass.
        let mut complete = n.extends.is_empty();

        for member in &n.body.body {
            match member {
                TsTypeElement::TsPropertySignature(prop) => {
                    // `computed` is the ONE discriminator, checked before any key-shape match: a
                    // computed key (`[SECRET]: string`, `['x'+y]: string`) declares whatever the
                    // EXPRESSION evaluates to, so capturing the expression's own spelling would
                    // invent a phantom field name — same contract as the class arm's
                    // `PropName::Computed`. (Unlike `PropName`, `TsPropertySignature` keeps the key
                    // as a plain `Expr` plus this flag, so an `Expr::Ident`/`Expr::Lit` match alone
                    // cannot tell `[SECRET]` from `SECRET` or `['k']` from `'k'`.)
                    if prop.computed {
                        complete = false; // may hide an arbitrary field name
                        continue;
                    }
                    match &*prop.key {
                        Expr::Ident(id) => fields.push(ProvideBodyField {
                            name: id.sym.to_string(),
                            optional: prop.optional,
                        }),
                        Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => {
                            fields.push(ProvideBodyField {
                                name: s.value.as_str().unwrap_or_default().to_string(),
                                optional: prop.optional,
                            })
                        }
                        // Any other non-computed key shape is not a statically-known field name.
                        _ => complete = false,
                    }
                }
                // A getter signature is an own READABLE property of the type — `interface R { get
                // password(): string }` is structurally satisfied by `{ password: "…" }`, so it is
                // a field like any property signature (the `@Expose()`-getter idiom makes it real
                // wire surface). Same computed-key contract as above. This is deliberately an
                // INTERFACE-arm judgment only — see the module doc's direction note for why the
                // class arm keeps skipping getters.
                TsTypeElement::TsGetterSignature(getter) => {
                    if getter.computed {
                        complete = false; // may hide an arbitrary field name
                        continue;
                    }
                    match &*getter.key {
                        Expr::Ident(id) => fields.push(ProvideBodyField {
                            name: id.sym.to_string(),
                            optional: false, // a getter member cannot be `?`-optional
                        }),
                        Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => {
                            fields.push(ProvideBodyField {
                                name: s.value.as_str().unwrap_or_default().to_string(),
                                optional: false,
                            })
                        }
                        _ => complete = false,
                    }
                }
                TsTypeElement::TsIndexSignature(_) => {
                    complete = false; // arbitrary extra keys may exist
                }
                // Method/call/construct signatures are not JSON body fields; a SETTER signature is
                // write-only (nothing a response serializes, and structurally satisfiable without
                // any readable property), so it is dropped WITHOUT touching completeness — a
                // deliberate silence, not a gap.
                _ => {}
            }
        }

        self.out.push(ClassShapeFragment {
            name,
            fields,
            complete,
        });
        n.visit_children_with(self);
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
mod tests;
