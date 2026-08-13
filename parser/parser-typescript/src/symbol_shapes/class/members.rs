//! Pass 1 of [`super::emit_class`]: what each `ClassMember` variant contributes, and nothing about
//! what it is CALLED.
//!
//! Split out of `super` for the repo's 300-line file cap along a seam the two halves already had:
//! everything here is a pure function of ONE member node (its key, its staticness, its body span),
//! while every naming decision in `super` — `Class.static.name`, `Class.name.set` — is a question
//! about the WHOLE class body that no single member can answer about itself. That is also why this
//! module returns the raw `name`/`is_setter` rather than a finished symbol name.

use swc_core::common::{BytePos, Span};
use swc_core::ecma::ast::{
    BlockStmtOrExpr, Class, ClassMember, Expr, MethodKind, ObjectLit, PropName,
};

/// The member name a static initialization block takes. A class may legally hold SEVERAL, and they
/// have no key at all, so the 2nd and later ones take a 1-based ordinal suffix (`static-block-2`,
/// `static-block-3`, ...) — without it `super`'s dedup would keep the first block's span and silently
/// drop every later block's, which is exactly the coverage hole this name exists to close. The `-` is
/// what makes the spelling collision-proof against an identifier-keyed member: `-` cannot appear in a
/// JS identifier.
const STATIC_BLOCK: &str = "static-block";

/// One extractable class member, in source order. `is_setter` is carried ALONGSIDE the name rather
/// than folded into it because `super` needs the raw name to ask whether the rest of the class body
/// contests it.
pub(super) struct Member<'a> {
    pub(super) name: String,
    pub(super) is_static: bool,
    pub(super) is_setter: bool,
    /// The MEMBER node's own start — decorators included, since a decorator is part of the member
    /// node. `super` turns it into `body_start`; see `zzop_core::SourceSymbol`'s "Body span contract".
    pub(super) lo: BytePos,
    pub(super) leaf: Leaf<'a>,
}

impl Leaf<'_> {
    /// Whether this member contributes a scannable region at all. `Leaf::Body(None)` — a TS overload
    /// SIGNATURE, an `abstract`/ambient member — is the only shape that contributes nothing, and
    /// `super`'s dedup uses that to decide which of several same-named declarations survives.
    pub(super) fn is_scannable(&self) -> bool {
        !matches!(self, Leaf::Body(None))
    }
}

/// What one class member contributes to the scannable-span projection.
pub(super) enum Leaf<'a> {
    /// One `Class.member` symbol spanning this body (`None` = declared with no body).
    Body(Option<Span>),
    /// An object-literal VALUE — the leaves are its own `key: value` members, emitted as
    /// `Class.member.key` by the same extractor object-literal factories use, so the three
    /// spellings of "a bag of handlers" (`return {...}` from a factory, a top-level
    /// `const o = {...}`, and a class property `o = {...}`) project the same shape.
    Object(&'a ObjectLit),
}

/// Every member of `class` that contributes something scannable, in source order.
///
/// KEY SHAPES, following the crate-wide `PropName` convention (`adapters/class_shapes.rs` states it
/// for the same class body, and ~10 other extractors spell it the same way): `Ident` and `Str` keys
/// are statically-known names and DO emit (a string key contributes its literal text, so
/// `"run"() {}` is `Class.run`); `Computed` keys are unknowable — capturing the key EXPRESSION's
/// spelling would invent a phantom member name, so they emit nothing, and neither do `Num`/`BigInt`
/// keys, which no other extractor in this crate names either. A computed-key member is therefore
/// still unscannable when its class has any other leaf; that is a disclosed hole, not an oversight,
/// and `symbols_tests::class_member_shapes_that_still_project_no_leaf_are_disclosed_here` keeps the
/// whole list of such holes countable.
pub(super) fn collect_members(class: &Class) -> Vec<Member<'_>> {
    let mut static_blocks = 0usize;
    class
        .body
        .iter()
        .filter_map(|member| match member {
            ClassMember::Constructor(c) => Some(Member {
                name: "constructor".to_string(),
                is_static: false,
                is_setter: false,
                lo: c.span.lo,
                leaf: Leaf::Body(c.body.as_ref().map(|b| b.span)),
            }),
            ClassMember::Method(m) => Some(Member {
                name: prop_name(&m.key)?,
                is_static: m.is_static,
                is_setter: m.kind == MethodKind::Setter,
                lo: m.span.lo,
                leaf: Leaf::Body(m.function.body.as_ref().map(|b| b.span)),
            }),
            ClassMember::PrivateMethod(m) => Some(Member {
                name: format!("#{}", m.key.name),
                is_static: m.is_static,
                is_setter: m.kind == MethodKind::Setter,
                lo: m.span.lo,
                leaf: Leaf::Body(m.function.body.as_ref().map(|b| b.span)),
            }),
            // a non-function, non-object field has no body to scan and drops out in `prop_leaf`
            ClassMember::ClassProp(p) => Some(Member {
                name: prop_name(&p.key)?,
                is_static: p.is_static,
                is_setter: false,
                lo: p.span.lo,
                leaf: prop_leaf(p.value.as_deref())?,
            }),
            ClassMember::PrivateProp(p) => Some(Member {
                name: format!("#{}", p.key.name),
                is_static: p.is_static,
                is_setter: false,
                lo: p.span.lo,
                leaf: prop_leaf(p.value.as_deref())?,
            }),
            ClassMember::StaticBlock(b) => {
                static_blocks += 1;
                let n = match static_blocks {
                    1 => STATIC_BLOCK.to_string(),
                    n => format!("{STATIC_BLOCK}-{n}"),
                };
                Some(Member {
                    name: n,
                    is_static: true,
                    is_setter: false,
                    lo: b.span.lo,
                    leaf: Leaf::Body(Some(b.body.span)),
                })
            }
            // index signatures / auto-accessors / empty statements — nothing scannable
            _ => None,
        })
        .collect()
}

/// What a property's initializer contributes. `None` for a value with nothing scannable in it.
///
/// The `Span` returned here supplies only `body_END` (and the Some/None decision) — `body_start` is
/// the MEMBER's declaration line at `super`'s emission site, per `zzop_core::SourceSymbol`'s "Body
/// span contract". Which end is taken still differs by shape:
/// - BLOCK-bodied (`m = () => { ... }` / `m = function () { ... }`): the body BLOCK's closing brace,
///   for parity with class methods.
/// - EXPRESSION-bodied arrow (`m = () => expr`): the arrow's own end, since there is no block.
///
/// Until the contract landed this asymmetry carried the START too, and had to: taking the expression's
/// own span left an `async` on the header line (`m = async () => expr`) outside the span, so a
/// `\basync\b`-anchored method-scan pattern could pair in every spelling of the same function EXCEPT
/// this one. The declaration-line start makes that failure mode structurally impossible instead of
/// patched per shape.
fn prop_leaf(value: Option<&Expr>) -> Option<Leaf<'_>> {
    match value? {
        Expr::Arrow(a) => Some(Leaf::Body(Some(match &*a.body {
            BlockStmtOrExpr::BlockStmt(b) => b.span,
            BlockStmtOrExpr::Expr(_) => a.span,
        }))),
        Expr::Fn(f) => Some(Leaf::Body(Some(f.function.body.as_ref()?.span))),
        Expr::Object(o) => Some(Leaf::Object(o)),
        _ => None,
    }
}

/// PropName -> static name. `Ident` and `Str` are statically known; `Computed` (and `Num`/`BigInt`)
/// are not — see [`collect_members`]'s KEY SHAPES paragraph for why they emit nothing.
fn prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}
