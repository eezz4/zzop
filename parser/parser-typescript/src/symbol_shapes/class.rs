//! Class symbol emission — the class symbol itself plus its per-member "leaf" sub-symbols.
//!
//! Split out of `super` for the repo's 300-line file cap, along the seam that makes the split
//! honest: everything here is about ONE declaration form (`class`), and nothing else in
//! `symbol_shapes` reads it.

use std::collections::HashSet;

use swc_core::common::{BytePos, SourceMap, Span};
use swc_core::ecma::ast::{BlockStmtOrExpr, Class, ClassMember, Expr, ObjectLit, PropName};
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::factory::{extract_object_methods, ObjectLitMap};
use crate::line_of;

/// The member name a static initialization block takes. A class may legally hold SEVERAL, and they
/// have no key at all, so the 2nd and later ones take a 1-based ordinal suffix (`static-block-2`,
/// `static-block-3`, ...) — without it the `(is_static, name)` dedup below would keep the first
/// block's span and silently drop every later block's, which is exactly the coverage hole this name
/// exists to close. The `-` is what makes the spelling collision-proof against an identifier-keyed
/// member: `-` cannot appear in a JS identifier.
const STATIC_BLOCK: &str = "static-block";

fn class_symbol(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
) -> SourceSymbol {
    let line = line_of(cm, class.span.lo);
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.into(),
        name,
        kind: SourceSymbolKind::Class,
        line,
        exported,
        is_default,
        body_start: Some(line), // class bodyStart uses the node's own start line
        body_end: Some(line_of(cm, class.span.hi)),
        write_sites: Vec::new(),
    }
}

/// What one class member contributes to the scannable-span projection.
enum Leaf<'a> {
    /// One `Class.member` symbol spanning this body (`None` = declared with no body).
    Body(Option<Span>),
    /// An object-literal VALUE — the leaves are its own `key: value` members, emitted as
    /// `Class.member.key` by the same extractor object-literal factories use, so the three
    /// spellings of "a bag of handlers" (`return {...}` from a factory, a top-level
    /// `const o = {...}`, and a class property `o = {...}`) project the same shape.
    Object(&'a ObjectLit),
}

/// Class symbol + method sub-symbols (`Class.method`) — constructor/method/getter/setter/private-method,
/// function-VALUED properties (`m = () => {...}` / `m = function () {...}`, incl. `#private = ...`),
/// object-literal-valued properties (`routes = { m: () => {} }` -> `Class.routes.m`), and static
/// initialization blocks. Same-name SAME-STATICNESS pairs (e.g. get/set) emit once — which is a
/// DISCLOSED LEAF HOLE, not a clean simplification: the second accessor's body ends up in no leaf, and
/// the class span that used to cover it is discarded in favour of the first's
/// (`symbols_tests::class_member_shapes_that_still_project_no_leaf_are_disclosed_here` pins it). A
/// static member
/// and an instance member sharing a name are two distinct members and BOTH emit — the dedup key is
/// `(is_static, name)`, never the bare name, which used to let whichever came first in source order
/// swallow the other's leaf span. In that collision case (only then) the static one is named
/// `Class.static.name`, because `Class.name`/`{file}#Class.name` must stay unique and the id belongs
/// to the instance member — a documented approximation: call-graph resolution of `Class.name()` keeps
/// targeting `{file}#Class.name` regardless of which member the caller meant. An UNCONTESTED static
/// keeps the plain `Class.name` spelling, so ordinary static methods resolve exactly as before.
///
/// KEY SHAPES, following the crate-wide `PropName` convention (`adapters/class_shapes.rs` states it
/// for the same class body, and ~10 other extractors spell it the same way): `Ident` and `Str` keys
/// are statically-known names and DO emit (a string key contributes its literal text, so
/// `"run"() {}` is `Class.run`); `Computed` keys are unknowable — capturing the key EXPRESSION's
/// spelling would invent a phantom member name, so they emit nothing, and neither do `Num`/`BigInt`
/// keys, which no other extractor in this crate names either. A computed-key member is therefore
/// still unscannable when its class has any other leaf; that is a disclosed hole, not an oversight.
///
/// WHY EVERY NON-COMPUTED MEMBER MUST GET A LEAF. Function-valued properties were skipped until
/// 2026-08-09, and that skip was the method-scan span-boundary false-positive class: the three
/// same-meaning spellings (`class C { m() {} }`, `const o = { m: () => {} }`, `class C { m = () => {} }`)
/// all produced leaf spans except the third, so a property-only class (e.g. every
/// swagger-typescript-api client) projected ONE class-wide span and `method-scan` rules paired
/// patterns across unrelated members — 11 confirmed FPs and 2 confirmed FNs (one critical) in
/// `cases/trees/api-be/spans/`. Emitting those leaves let `dsl::method_scan::gates::drop_outer_spans`
/// discard the class-wide span — and that discard INVERTED THE SIGN for every member kind still
/// emitting nothing. Measured 2026-08-10 on `typescript/async-handler-no-try`: adding one unrelated
/// arrow property to a class whose only handler sat in an object-literal property (or a static block,
/// or a string-keyed method) silenced the finding on that OTHER member, because the class-wide span
/// that used to cover it was now dropped in favour of the new arrow's leaf.
///
/// The gap is closed HERE and not by making the discard conditional in `drop_outer_spans`, and the
/// two were measured against each other rather than argued. A retained class-wide span overlaps every
/// leaf inside it, so it re-reports what the leaf already reported and re-opens the cross-member
/// pairing: with `drop_outer_spans` patched to keep Class-kind spans, the 8-file reproduction went
/// 7 -> 14 findings (every leaf-covered defect duplicated at its class) and `detection-gate.sh` went
/// `TP 259 FN 0 FP 0` -> `FP 10`, precision 96.4%, the same span-boundary class v0.29.0 removed.
/// Emitting the missing leaves keeps the gate at `TP 259 FN 0 FP 0`. `drop_outer_spans` also could not
/// implement the narrow form ("keep the class span only while some member is unscannable") without a
/// new IR field: it sees projected symbols, never the class body, so it cannot know a member exists
/// that projected nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_class(
    cm: &SourceMap,
    file: &str,
    name: String,
    class: &Class,
    exported: bool,
    is_default: bool,
    object_lits_by_name: &ObjectLitMap,
    out: &mut Vec<SourceSymbol>,
) {
    out.push(class_symbol(
        cm,
        file,
        name.clone(),
        class,
        exported,
        is_default,
    ));
    // Pass 1: extractable members with their staticness, in source order. Collected up front because
    // the collision-only `Class.static.name` spelling below must not depend on source order — whether
    // a name is contested is a fact of the WHOLE class body.
    let mut static_blocks = 0usize;
    let leaves: Vec<(String, bool, BytePos, Leaf<'_>)> = class
        .body
        .iter()
        .filter_map(|member| match member {
            ClassMember::Constructor(c) => Some((
                "constructor".to_string(),
                false,
                c.span.lo,
                Leaf::Body(c.body.as_ref().map(|b| b.span)),
            )),
            ClassMember::Method(m) => {
                let n = prop_name(&m.key)?;
                Some((
                    n,
                    m.is_static,
                    m.span.lo,
                    Leaf::Body(m.function.body.as_ref().map(|b| b.span)),
                ))
            }
            ClassMember::PrivateMethod(m) => Some((
                format!("#{}", m.key.name),
                m.is_static,
                m.span.lo,
                Leaf::Body(m.function.body.as_ref().map(|b| b.span)),
            )),
            ClassMember::ClassProp(p) => {
                let n = prop_name(&p.key)?;
                // non-function, non-object field — no body to scan
                Some((n, p.is_static, p.span.lo, prop_leaf(p.value.as_deref())?))
            }
            ClassMember::PrivateProp(p) => Some((
                format!("#{}", p.key.name),
                p.is_static,
                p.span.lo,
                prop_leaf(p.value.as_deref())?,
            )),
            ClassMember::StaticBlock(b) => {
                static_blocks += 1;
                let n = match static_blocks {
                    1 => STATIC_BLOCK.to_string(),
                    n => format!("{STATIC_BLOCK}-{n}"),
                };
                Some((n, true, b.span.lo, Leaf::Body(Some(b.body.span))))
            }
            // index signatures / auto-accessors / empty statements — nothing scannable
            _ => None,
        })
        .collect();
    let contested = |n: &str| {
        leaves.iter().any(|(m, s, _, _)| m == n && *s)
            && leaves.iter().any(|(m, s, _, _)| m == n && !*s)
    };
    // Pass 2: emit, deduping on (staticness, name) — get/set pairs share both and emit once, while a
    // static/instance collision emits both members (see the fn doc for the naming).
    let mut seen = HashSet::new();
    for (mname, is_static, lo, leaf) in &leaves {
        if !seen.insert((*is_static, mname.clone())) {
            continue;
        }
        let full = if *is_static && contested(mname) {
            format!("{name}.static.{mname}")
        } else {
            format!("{name}.{mname}")
        };
        match leaf {
            // `body_start` is the MEMBER's own declaration line — decorators included, since `lo` is
            // the member node's start — never the body block's opening brace. See
            // `zzop_core::SourceSymbol`'s "Body span contract"; a member declared with no body at all
            // keeps `None`/`None`.
            Leaf::Body(body_span) => out.push(SourceSymbol {
                id: format!("{file}#{full}"),
                file: file.into(),
                name: full,
                kind: SourceSymbolKind::Function,
                line: line_of(cm, *lo),
                exported: false,
                is_default: false,
                body_start: body_span.map(|_| line_of(cm, *lo)),
                body_end: body_span.map(|s| line_of(cm, s.hi)),
                write_sites: Vec::new(),
            }),
            // The property itself gets NO symbol — it is a bag, not a body. Its members are the
            // leaves, and `visited` starts empty per property so two properties spreading the same
            // const both flatten it.
            Leaf::Object(obj) => extract_object_methods(
                cm,
                file,
                &full,
                obj,
                object_lits_by_name,
                &mut HashSet::new(),
                out,
            ),
        }
    }
}

/// What a property's initializer contributes. `None` for a value with nothing scannable in it.
///
/// The `Span` returned here supplies only `body_END` (and the Some/None decision) — `body_start` is
/// the MEMBER's declaration line at the call site above, per `zzop_core::SourceSymbol`'s "Body span
/// contract". Which end is taken still differs by shape:
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
/// are not — see `emit_class`'s KEY SHAPES paragraph for why they emit nothing.
fn prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}
