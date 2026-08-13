//! Class symbol emission — the class symbol itself plus its per-member "leaf" sub-symbols.
//!
//! Split out of `super` for the repo's 300-line file cap, along the seam that makes the split
//! honest: everything here is about ONE declaration form (`class`), and nothing else in
//! `symbol_shapes` reads it. The same cap split THIS file in turn on 2026-08-13: `members` answers
//! what each member node contributes, and what is left here answers what it is CALLED — the half
//! that needs the whole class body in view.

mod members;

use std::collections::{HashMap, HashSet};

use swc_core::common::SourceMap;
use swc_core::ecma::ast::Class;
use zzop_core::{SourceSymbol, SourceSymbolKind};

use crate::factory::{extract_object_methods, ObjectLitMap};
use crate::line_of;
use members::{collect_members, Leaf, Member};

/// The trailing segment a SETTER takes when a same-name, same-staticness getter also exists
/// (`set x(v)` next to `get x()` -> `C.x.set`, while the getter keeps the plain `C.x`).
///
/// WHY THE SETTER MOVES AND NOT THE GETTER, and why only when the pair is complete: same reasoning as
/// `Class.static.name` below — the plain id is left with the member a caller most likely means, and an
/// UNCONTESTED accessor keeps it outright, so a write-only `set x(v)` is still `C.x`. Reading is what
/// `C.x` resolves as everywhere else (a property read, a `c.x()` call through a getter-returned
/// function), so the getter holds the name.
///
/// WHY A TRAILING SEGMENT RATHER THAN THE MIDDLE ONE `Class.static.name` USES. Both spellings collide
/// with an existing, live shape — the object-literal leaf `Class.member.key` — but only one collision
/// is reachable in legal TypeScript, and that was measured rather than argued:
/// - `C.set.x` (middle) collides with `class C { set = { x: () => {} } }`, whose members are `set` and
///   `x`: two DIFFERENT names, so the class compiles and the duplicate id is real.
/// - `C.x.set` (trailing) collides with `class C { x = { set: () => {} } }`, which would need the class
///   to declare `x` TWICE — as an accessor pair and as a property. That is TS2300, so the collision
///   cannot be reached by code that type-checks.
///
/// The `-`-based collision-proofing `members::STATIC_BLOCK` uses is not available here: the segment has
/// to read as the accessor keyword to a human scanning a finding, and a `Str` key
/// (`class C { "x-set"() {} }`) makes no spelling literally unreachable anyway.
///
/// TWO CONSUMERS SEE THIS ID DIFFERENTLY, and both were checked rather than assumed:
/// - `rules_http`'s `symbol_index::build_name_index` keys on the LAST dotted segment, so this leaf
///   indexes under `set` — a phantom entry that can make a genuine route handler NAMED `set`
///   ambiguous, which resolves to `None` (do-not-guess) rather than to the wrong symbol. The rejected
///   `C.set.x` spelling would have indexed under the MEMBER name instead, so its phantom would collide
///   on arbitrary names rather than on one.
/// - `parse_calls` attributes a call in the setter body to this leaf instead of the class symbol, and
///   `zzop_core::callgraph::resolve_method` cannot mint a three-segment id, so the leaf takes no
///   in-edges. `lang::calls::tests::a_call_in_a_setter_body_attributes_to_the_setter_leaf_not_the_class`
///   carries the measurement and why the write-site channel those rules read is unaffected.
///
/// ONE OTHER FRONT END IS WAITING ON THIS DECISION, which is why it is written down here rather than
/// left implicit in the code: `zzop_parser_csharp`'s `lang::symbols::member::emit_property` discloses
/// that both accessors of a C# `accessor_list` share ONE span and says splitting them "needs a naming
/// decision (`C.P.get`) that changes call-graph ids". That is this decision, and the shape it takes
/// here — trailing segment, only when contested, plain name to the READING side — is the precedent.
/// The C# case is NOT the same defect and must be re-measured before it is taken: its shared span
/// COVERS both bodies, so the loss there is cross-accessor pattern pairing (a false-positive
/// direction), not the unscanned body this repair recovers.
const ACCESSOR_SET: &str = "set";

/// For each dedup key, the INDEX of the member that survives it — the one that has a body, never
/// simply the one that came first.
///
/// TypeScript lets one member name carry several declarations: `foo(a: string): void;` twice over an
/// implementation `foo(a: any) { ... }`. Unlike a get/set pair, all of them are ONE member, so no
/// naming decision arises (see [`ACCESSOR_SET`] for the case where one does) and one leaf is right —
/// the only question is which declaration that leaf comes from. Keeping the FIRST, which is what this
/// did until 2026-08-13, keeps an overload SIGNATURE. MEASURED before the repair, on
/// `class C { foo(a: string): void; foo(a: number): void; foo(a: any) { write(a); } ping = () => {} }`:
///
/// ```text
///   x.ts#C       Class     line 1  body 1..10
///   x.ts#C.foo   Function  line 2  body None..None   <- the implementation, lines 4..8, is in NO span
///   x.ts#C.ping  Function  line 9  body 9..9
/// ```
///
/// The `ping` leaf is enough to make `dsl::method_scan::gates::drop_outer_spans` discard the
/// class-wide span that used to cover those lines, so the body became unreachable to every
/// method-scan rule. That is strictly worse than the get/set defect this follows: there the surviving
/// symbol carried the WRONG body, here it carries none, and `zzop_core::SourceSymbol`'s span contract
/// reads `None` as the positive claim "this declaration encloses nothing scannable".
///
/// WHY "HAS A BODY" AND NOT "TAKE THE LAST". TypeScript requires the implementation to come last, so
/// the two rules agree on every overload set that type-checks — and that agreement is the trap.
/// `Class.static.name` and [`ACCESSOR_SET`] both exist because SOURCE ORDER had been silently deciding
/// which body survived; "take the last" would reinstate exactly that, differently spelled, and it
/// would take the wrong member the moment a language or a future TS relaxation puts the body first.
/// "The one with something to scan" is order-free, and it is the rule this workspace already names for
/// its other id-keyed consumer: `zzop_rules_http::http_scan::symbols_by_id` prefers the entry carrying
/// `write_sites` and otherwise keeps the first (`zzop_core::SourceSymbol`'s id doc mandates it for any
/// new id-keyed map). Same shape, different evidence field.
///
/// WHEN NOTHING HAS A BODY the first still wins and the leaf keeps `None`/`None` — an `abstract`
/// member, a `declare class`, an overload set with no implementation in this file. That `None` is the
/// span contract's positive claim, not a lost body.
///
/// THE `line` DECISION THAT COMES WITH THE TIE-BREAK: the surviving member is taken WHOLE, so `line`
/// moves to the implementation's declaration line (4 above, not 2). `line` and `body_start` are one
/// coordinate at the emission site — both are `line_of(cm, lo)` for the same member node — and keeping
/// them together is what makes "a symbol's `line` is the declaration line of the body it projects"
/// true of every TS symbol that has a body. REJECTED: freeze `line` at the FIRST signature's line so
/// the reported location does not move. It would make one symbol name two declarations at once, a
/// `line` pointing at a signature with no body and a span starting elsewhere; and the drift it avoids
/// was inventoried rather than assumed. `symbol.line` becomes a rule anchor in three places —
/// `dsl::ir_scan`'s symbol-scan (which matches NAMES, so the line it reports should be the declaration
/// it scanned), `engine::dead_exports` (leaves are `exported: false`, so never a leaf), and
/// `rules_graph`'s cache-lane anchor. `http_scan::scan_marker_window` reads `body_start` and falls back
/// to `line` only where there is no span, which after this repair is only the all-signatures case.
fn dedup_winners<'a>(leaves: &'a [Member<'_>]) -> HashMap<(bool, &'a str, bool), usize> {
    let mut winners: HashMap<(bool, &str, bool), usize> = HashMap::new();
    for (i, m) in leaves.iter().enumerate() {
        let held = winners
            .entry((m.is_static, m.name.as_str(), m.is_setter))
            .or_insert(i);
        if !leaves[*held].leaf.is_scannable() && m.leaf.is_scannable() {
            *held = i;
        }
    }
    winners
}

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

/// Class symbol + method sub-symbols (`Class.method`) — constructor/method/getter/setter/private-method,
/// function-VALUED properties (`m = () => {...}` / `m = function () {...}`, incl. `#private = ...`),
/// object-literal-valued properties (`routes = { m: () => {} }` -> `Class.routes.m`), and static
/// initialization blocks. A getter and a setter sharing a name are two distinct BODIES and BOTH emit,
/// the setter as `Class.name.set` (see [`ACCESSOR_SET`]); until 2026-08-13 the dedup key could not tell
/// them apart, so the second one's body landed in NO leaf while the class span that used to cover it
/// was discarded in favour of the first's — and which one survived was decided by SOURCE ORDER, so
/// `C.x` meant the getter's body in one class and the setter's in the next. A
/// static member
/// and an instance member sharing a name are two distinct members and BOTH emit — the dedup key is
/// `(is_static, name, is_setter)`, never the bare name, which used to let whichever came first in source
/// order swallow the other's leaf span. Several declarations that DO share a key are one member and
/// collapse onto one leaf — a TypeScript overload set — and [`dedup_winners`] states which of them the
/// leaf is taken from, and what that costs the symbol's reported `line`. In the static/instance collision case (only then) the static one is named
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
    // Pass 1: collected up front because neither naming decision below may depend on SOURCE ORDER —
    // whether a name is contested is a fact of the WHOLE class body.
    let leaves = collect_members(class);
    let contested = |n: &str| {
        leaves.iter().any(|m| m.name == n && m.is_static)
            && leaves.iter().any(|m| m.name == n && !m.is_static)
    };
    // Does a non-setter member hold the plain spelling this setter would otherwise take?
    let paired = |n: &str, is_static: bool| {
        leaves
            .iter()
            .any(|m| m.name == n && m.is_static == is_static && !m.is_setter)
    };
    // Pass 2: emit, deduping on (staticness, name, setter-ness) — a get/set pair and a static/instance
    // collision each emit BOTH members (see the fn doc and `ACCESSOR_SET` for the naming), while the
    // several declarations of ONE overloaded member collapse onto the one that has a body
    // (`dedup_winners`).
    let winners = dedup_winners(&leaves);
    for (
        i,
        Member {
            name: mname,
            is_static,
            is_setter,
            lo,
            leaf,
        },
    ) in leaves.iter().enumerate()
    {
        if winners[&(*is_static, mname.as_str(), *is_setter)] != i {
            continue;
        }
        let base = if *is_static && contested(mname) {
            format!("{name}.static.{mname}")
        } else {
            format!("{name}.{mname}")
        };
        let full = if *is_setter && paired(mname, *is_static) {
            format!("{base}.{ACCESSOR_SET}")
        } else {
            base
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
