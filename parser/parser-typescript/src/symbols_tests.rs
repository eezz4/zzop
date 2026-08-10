//! `parse_symbols` coverage — top-level declarations, class-method sub-symbols, and binding
//! patterns. Factory sub-symbol and CommonJS-export coverage live with their own modules
//! (`factory` / `cjs_exports`); this file exists because `symbols.rs` + these tests would exceed
//! the 300-line file budget.

use crate::parse_symbols;
use crate::test_util::names;
use zzop_core::SourceSymbolKind as K;

// --- parseSymbols (top-level; sub-symbols are follow-ups) ---

#[test]
fn export_function_extracted() {
    let s = parse_symbols("x.ts", "export function foo() { return 1; }\n");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].id, "x.ts#foo");
    assert_eq!(s[0].name, "foo");
    assert_eq!(s[0].kind, K::Function);
    assert!(s[0].exported);
    assert_eq!(s[0].line, 1);
    assert_eq!(s[0].body_start, Some(1));
    assert!(s[0].body_end.unwrap() >= 1);
}

#[test]
fn function_without_export() {
    let s = parse_symbols("x.ts", "function inner() {}\n");
    assert_eq!(s[0].name, "inner");
    assert!(!s[0].exported);
}

#[test]
fn const_arrow_is_function_kind() {
    let s = parse_symbols(
        "x.ts",
        "export const bar = () => 42;\nexport const BAZ = 7;\n",
    );
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].name, "bar");
    assert_eq!(s[0].kind, K::Function);
    assert!(s[0].exported);
    assert_eq!(s[1].name, "BAZ");
    assert_eq!(s[1].kind, K::Const);
    assert!(s[0].body_start.is_some());
    assert!(s[1].body_start.is_none());
}

#[test]
fn class_body_lines() {
    let s = parse_symbols("x.ts", "export class Foo {\n  bar() {}\n}\n");
    assert_eq!(s[0].name, "Foo");
    assert_eq!(s[0].kind, K::Class);
    assert!(s[0].exported);
    assert!(s[0].body_end.unwrap() > s[0].body_start.unwrap());
}

#[test]
fn interface_and_type_no_body() {
    let s = parse_symbols(
        "x.ts",
        "export interface Shape { size: number }\nexport type Id = string | number;\n",
    );
    assert_eq!(s.len(), 2);
    assert_eq!((s[0].name.as_str(), s[0].kind), ("Shape", K::Interface));
    assert_eq!((s[1].name.as_str(), s[1].kind), ("Id", K::Type));
    assert!(s[0].body_start.is_none());
    assert!(s[1].body_start.is_none());
}

#[test]
fn default_anonymous_function() {
    let s = parse_symbols("x.ts", "export default function() { return 1; }\n");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "default");
    assert_eq!(s[0].kind, K::Function);
    assert!(s[0].exported);
    assert!(s[0].is_default);
}

#[test]
fn default_named_function() {
    let s = parse_symbols("x.ts", "export default function Foo() { return 1; }\n");
    assert_eq!(s[0].name, "Foo");
    assert!(s[0].is_default);
}

#[test]
fn export_function_no_default() {
    let s = parse_symbols("x.ts", "export function Foo() {}\n");
    assert!(!s[0].is_default);
}

#[test]
fn line_number_is_1_based() {
    let s = parse_symbols("x.ts", "\n\nexport function multi() {}\n");
    assert_eq!(s[0].line, 3);
}

#[test]
fn multiple_declarations_preserve_order() {
    let s = parse_symbols(
        "x.ts",
        "export function a() {}\nfunction b() {}\nexport class C {}\n",
    );
    assert_eq!(names(&s), vec!["a", "b", "C"]);
}

#[test]
fn require_initializer_skipped() {
    // a CJS import alias is not a symbol
    let s = parse_symbols("x.js", "const X = require('./y');\n");
    assert!(s.is_empty());
}

// --- parseSymbols class-method sub-symbols ---

#[test]
fn class_method_sub_symbols() {
    let s = parse_symbols(
        "x.ts",
        "export class Svc {\n  foo() {}\n  async bar() {}\n}\n",
    );
    assert_eq!(names(&s), vec!["Svc", "Svc.foo", "Svc.bar"]);
    assert_eq!(s[1].kind, K::Function);
    assert!(!s[1].exported);
    assert!(s[1].body_start.unwrap() > 0);
}

#[test]
fn class_constructor_static_get_set_private() {
    let s = parse_symbols(
            "x.ts",
            "class C {\n  constructor() {}\n  static s() {}\n  get g() { return 1 }\n  set g(v) {}\n  #p() {}\n}\n",
        );
    // same name for get/set -> only the first
    assert_eq!(names(&s), vec!["C", "C.constructor", "C.s", "C.g", "C.#p"]);
}

#[test]
fn class_plain_field_not_extracted() {
    // Non-function fields carry no body to scan — they stay out. (Function-VALUED properties are
    // extracted; see the two tests below. Until 2026-08-09 this test pinned ALL properties as
    // unextracted, and that hole was the method-scan span-boundary FP class: a property-only class
    // projected one class-wide span, pairing patterns across unrelated members.)
    let s = parse_symbols("x.ts", "class C {\n  field = 1;\n  method() {}\n}\n");
    assert_eq!(names(&s), vec!["C", "C.method"]);
}

#[test]
fn class_property_functions_get_leaf_spans() {
    let s = parse_symbols(
        "x.ts",
        "class C {\n  m = async () => {\n    return 1;\n  };\n  n = function () {\n    return 2;\n  };\n  one = async () =>\n    fetch('/x');\n  method() {}\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.m", "C.n", "C.one", "C.method"]);
    for sym in &s[1..] {
        assert_eq!(sym.kind, K::Function);
        assert!(!sym.exported);
    }
    // Block-bodied members span their body BLOCK (parity with class methods)...
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(4));
    assert_eq!(s[2].body_start, Some(5));
    assert_eq!(s[2].body_end, Some(7));
    // ...and an expression-bodied arrow spans the WHOLE arrow, header line included (parity with the
    // sibling spellings in `factory`/`symbols`) — spanning only the expression left `async` on the
    // header outside the span, so `\basync\b`-anchored method-scan patterns could not pair in this
    // spelling alone. See `symbol_shapes::class::prop_leaf`'s doc for the block-vs-arrow choice.
    assert_eq!(s[3].body_start, Some(8));
    assert_eq!(s[3].body_end, Some(9));
}

#[test]
fn static_and_instance_members_sharing_a_name_both_emit_in_either_order() {
    // A static function-property and an instance method legally share a name. The dedup exists for
    // same-staticness get/set pairs only — keyed on the bare name it let whichever member came FIRST
    // in source order swallow the other's leaf span, so `C.m`'s facts pointed at the wrong member.
    // Both must emit; the colliding STATIC one is named `C.static.m` (see `emit_class`'s doc).
    let s = parse_symbols(
        "x.ts",
        "class C {\n  static m = () => {\n    return 1;\n  };\n  m() {\n    return 2;\n  }\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.static.m", "C.m"]);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(4));
    assert_eq!(s[2].body_start, Some(5));
    assert_eq!(s[2].body_end, Some(7));

    // The reverse source order must emit the same two members (order-dependence was the defect).
    let s = parse_symbols(
        "x.ts",
        "class C {\n  m() {\n    return 2;\n  }\n  static m = () => {\n    return 1;\n  };\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.m", "C.static.m"]);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(4));
    assert_eq!(s[2].body_start, Some(5));
    assert_eq!(s[2].body_end, Some(7));
}

#[test]
fn a_static_member_without_a_name_collision_keeps_its_plain_dotted_name() {
    // The `C.static.` spelling is COLLISION-ONLY: an uncontested static keeps `C.s` so existing
    // call-graph resolution (`C.s()` -> `<file>#C.s`) is untouched (also pinned by
    // `class_constructor_static_get_set_private` above).
    let s = parse_symbols(
        "x.ts",
        "class C {\n  static only = () => {\n    return 1;\n  };\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.only"]);
}

#[test]
fn class_private_property_arrow_extracted() {
    let s = parse_symbols(
        "x.ts",
        "class C {\n  #h = () => {\n    return 1;\n  };\n  plain = 1;\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.#h"]);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(4));
}

#[test]
fn class_computed_names_skipped_string_literal_names_kept() {
    // The crate-wide `PropName` convention (`adapters/class_shapes.rs` and ~10 siblings): a `Str` key
    // IS a statically-known name and contributes its literal text; a `Computed` key is unknowable and
    // contributes nothing, because the key EXPRESSION's spelling would be a phantom member name.
    // Before 2026-08-10 a string key was skipped too, which cost it its scannable span the moment any
    // sibling member emitted a leaf (see `class_members_without_a_leaf_keep_their_own_span`).
    let s = parse_symbols(
        "x.ts",
        "class C {\n  [\"dyn\"]() {}\n  \"str\"() {}\n  ok() {}\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.str", "C.ok"]);
    assert_eq!(s[1].body_start, Some(3));
    assert_eq!(s[1].body_end, Some(3));
}

#[test]
fn class_members_without_a_leaf_keep_their_own_span() {
    // The v0.29.0 sign inversion: once ANY member emits a leaf, `method_scan`'s
    // `gates::drop_outer_spans` discards the class-wide span — so every member kind that emitted
    // NOTHING went from "covered by the whole-class span" to uncovered. Measured 2026-08-10 on
    // `typescript/async-handler-no-try`: adding the unrelated `ping` arrow below silenced the
    // findings inside `routes`, the static block, and the string-keyed method. Each of those three
    // kinds must now project its own span.
    let src = "class C {\n  ping = async () => {\n    await beat();\n  };\n  routes = {\n    onSubmit: async () => {\n      await save();\n    },\n  };\n  static {\n    boot();\n  }\n  \"run\"() {\n    return 1;\n  }\n}\n";
    let s = parse_symbols("x.ts", src);
    assert_eq!(
        names(&s),
        vec![
            "C",
            "C.ping",
            "C.routes.onSubmit",
            "C.static-block",
            "C.run",
        ]
    );
    // Object-literal property: the property is a bag, not a body — its MEMBERS are the leaves, named
    // `Class.prop.key` by the same extractor a factory's `return {...}` goes through.
    assert_eq!(s[2].id, "x.ts#C.routes.onSubmit");
    assert_eq!(s[2].kind, K::Function);
    assert_eq!(s[2].body_start, Some(6));
    assert_eq!(s[2].body_end, Some(8));
    // Static block: spans its own braces, the same body-BLOCK choice class methods make.
    assert_eq!(s[3].body_start, Some(10));
    assert_eq!(s[3].body_end, Some(12));
    // String-keyed method.
    assert_eq!(s[4].body_start, Some(13));
    assert_eq!(s[4].body_end, Some(15));
    // Every leaf is strictly inside the class span, which is what makes the class span droppable.
    let (cs, ce) = (s[0].body_start.unwrap(), s[0].body_end.unwrap());
    for leaf in &s[1..] {
        assert!(
            cs <= leaf.body_start.unwrap() && leaf.body_end.unwrap() <= ce,
            "{leaf:?} escapes the class span {cs}..={ce}"
        );
    }
}

#[test]
fn class_object_literal_property_non_function_members_carry_no_span() {
    // Parity with `factory`'s object-literal extraction: a non-function value is still a member (it
    // gets a `Const` symbol) but has no body, so `method_scan` never scans it and it cannot make the
    // class span droppable on its own.
    let s = parse_symbols("x.ts", "class C {\n  cfg = {\n    url: \"u\",\n  };\n}\n");
    assert_eq!(names(&s), vec!["C", "C.cfg.url"]);
    assert_eq!(s[1].kind, K::Const);
    assert_eq!(s[1].body_start, None);
}

#[test]
fn several_static_blocks_in_one_class_each_get_a_distinct_leaf() {
    // A static block has no key, so all of them would share one name — and the `(is_static, name)`
    // dedup would keep the first block's span and drop the rest, re-opening the very hole the leaf
    // exists to close. The 2nd and later take a 1-based ordinal suffix.
    let s = parse_symbols(
        "x.ts",
        "class C {\n  static {\n    a();\n  }\n  static {\n    b();\n  }\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.static-block", "C.static-block-2"]);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[2].body_start, Some(5));
}

#[test]
fn class_member_shapes_that_still_project_no_leaf_are_disclosed_here() {
    // The DISCLOSURE half of `class_members_without_a_leaf_keep_their_own_span`. Each source below
    // holds a scannable body that this parser does NOT project, so once the sibling `ping` arrow gives
    // the class a leaf, `method_scan`'s `drop_outer_spans` discards the class span and that body goes
    // unscanned. This test does not argue any of them is right — it makes the list countable, so a
    // later decision changes an assertion here instead of discovering the hole from a missed finding.
    // Sweep basis: every `ClassMember` variant swc defines (Constructor / Method / PrivateMethod /
    // ClassProp / PrivateProp / TsIndexSignature / Empty / StaticBlock / AutoAccessor); the first five
    // and StaticBlock are covered above. `TsIndexSignature` and `Empty` are absent by CONSTRUCTION —
    // a type declaration and a stray `;` have no runtime body — so they are not in this list.
    let uncovered = [
        // Stage-3 auto-accessor. `emit_class` has no `AutoAccessor` arm; the value is a plain
        // initializer expression exactly like a `ClassProp`'s, so the arm is the whole cost.
        "class C {\n  accessor h = async () => {\n    await x();\n  };\n  ping = () => {};\n}\n",
        // TS type wrapper around the value — `prop_leaf` matches on the value expression itself, and
        // `x as const` / `x satisfies T` / `<T>x` are `TsAs`/`TsSatisfies`/`TsTypeAssertion` nodes
        // wrapping it. Same hole for a function value (`h = (async () => {}) as Handler`).
        "class C {\n  routes = {\n    onX: async () => {\n      await x();\n    },\n  } as const;\n  ping = () => {};\n}\n",
        // Method-shorthand/getter/setter INSIDE an object-literal property. `extract_object_methods`
        // documents that it takes `key: value` props only, and it is shared with factory extraction —
        // widening it changes what `return {...}` projects everywhere, so it is a separate decision.
        "class C {\n  routes = {\n    async onX() {\n      await x();\n    },\n  };\n  ping = () => {};\n}\n",
        // Function value wrapped in a call (`memo(...)`, `.bind(this)`, an IIFE). Never projected.
        "class C {\n  h = memo(async () => {\n    await x();\n  });\n  ping = () => {};\n}\n",
        // Numeric key — a `PropName::Num`, which no extractor in this crate names (see `prop_name`).
        "class C {\n  42() {\n    await x();\n  }\n  ping = () => {};\n}\n",
        // Computed key — unknowable by the crate-wide `PropName::Computed` convention. Note that a
        // computed key is computed even when it wraps a literal: `["run"]` and `"run"` are different
        // `PropName`s, and only the second is a statically-known name.
        "class C {\n  [\"ru\" + \"n\"]() {\n    await x();\n  }\n  ping = () => {};\n}\n",
    ];
    for src in uncovered {
        assert_eq!(
            names(&parse_symbols("x.ts", src)),
            vec!["C", "C.ping"],
            "expected ONLY the sibling arrow's leaf from:\n{src}"
        );
    }
    // The SECOND of a same-name get/set pair. Found 2026-08-10 while pinning the span contract, and
    // it is the only entry in this list that is not a missing ARM: `emit_class` reaches the setter,
    // recognizes it, and then drops it because the `(is_static, name)` dedup key cannot tell a getter
    // from a setter — so the setter's body (which is where validation logic actually lives) sits in no
    // leaf, and the class span that used to cover it is discarded in favour of the getter's. The fix
    // has a shape already in this file (`Class.static.name` for the static/instance collision) but it
    // is a NAMING decision that changes emitted symbol ids, so it is disclosed here rather than taken
    // in passing. Both bodies below are real; only the getter's is reachable.
    let s = parse_symbols(
        "x.ts",
        "class C {\n  get x() {\n    return this._x;\n  }\n  set x(v) {\n    validate(v);\n    this._x = v;\n  }\n  ping = () => {};\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.x", "C.ping"]);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(4)); // the getter's; lines 5..=8 are in no span at all

    // Nested object literals are a partial case: the outer key becomes a span-less `Const`, so the
    // inner handler is still unscanned. `extract_object_methods` flattens spreads, not nesting.
    let s = parse_symbols(
        "x.ts",
        "class C {\n  routes = {\n    v1: {\n      onX: async () => {\n        await x();\n      },\n    },\n  };\n  ping = () => {};\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.routes.v1", "C.ping"]);
    assert_eq!(s[1].body_start, None);
}

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line, decorators included ---

/// `zzop_core::SourceSymbol`'s "Body span contract". The shipped `typescript/async-handler-no-try`
/// rule triggers on `\bon[A-Z]\w*\s*[:=]\s*\{?\s*async\b` — a DECLARATION-line pattern — and worked
/// only because swc's block span starts on that line whenever the author put the `{` there. Wrap the
/// signature, or decorate the method, and the anchor fell outside the span with no diagnostic. Both
/// shapes are pinned here.
#[test]
fn function_and_method_spans_start_at_the_declaration_not_the_opening_brace() {
    let s = parse_symbols(
        "x.ts",
        "function onSubmit(\n  e: Event,\n) {\n  handle(e);\n}\n",
    );
    assert_eq!(s[0].body_start, Some(1));
    assert_eq!(s[0].body_end, Some(5));

    let s = parse_symbols(
        "x.ts",
        "class C {\n  @Post('/x')\n  async create(\n    dto: Dto,\n  ) {\n    await save(dto);\n  }\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.create"]);
    assert_eq!(s[1].line, 2);
    assert_eq!(s[1].body_start, Some(2));
    assert_eq!(s[1].body_end, Some(7));
    // Containment is what keeps `drop_outer_spans` able to discard the class span.
    assert!(s[0].body_start.unwrap() <= s[1].body_start.unwrap());
    assert!(s[0].body_end.unwrap() >= s[1].body_end.unwrap());
}

#[test]
fn anonymous_default_class_methods() {
    let s = parse_symbols("x.ts", "export default class { foo() {} bar() {} }\n");
    assert_eq!(names(&s), vec!["default", "default.foo", "default.bar"]);
}

// --- parseSymbols deferred exports (`export default foo;` / `export { foo }` as trailing statements) ---

#[test]
fn deferred_default_export_of_function() {
    let s = parse_symbols("x.ts", "function useX() {}\nexport default useX;\n");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "useX");
    assert!(s[0].exported);
    assert!(s[0].is_default);
}

#[test]
fn deferred_named_export() {
    let s = parse_symbols("x.ts", "const foo = 1;\nexport { foo };\n");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "foo");
    assert!(s[0].exported);
    assert!(!s[0].is_default);
}

#[test]
fn deferred_named_export_as_default() {
    let s = parse_symbols("x.ts", "function bar() {}\nexport { bar as default };\n");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "bar");
    assert!(s[0].exported);
    assert!(s[0].is_default);
}

#[test]
fn inline_default_export_still_works() {
    // regression: inline `export default function baz() {}` must not be affected by the deferred pass.
    let s = parse_symbols("x.ts", "export default function baz() {}\n");
    assert_eq!(s[0].name, "baz");
    assert!(s[0].exported);
    assert!(s[0].is_default);
}

#[test]
fn no_export_statement_stays_private() {
    // never-guess pin: a plain top-level decl with no export statement anywhere stays unexported.
    let s = parse_symbols("x.ts", "function priv() {}\n");
    assert_eq!(s[0].name, "priv");
    assert!(!s[0].exported);
}

#[test]
fn deferred_default_export_of_call_expr_fabricates_nothing() {
    // `export default makeThing()` has no ident to attribute to -> no symbol fabricated, no crash.
    let s = parse_symbols(
        "x.ts",
        "function makeThing() { return 1; }\nexport default makeThing();\n",
    );
    assert_eq!(names(&s), vec!["makeThing"]);
    assert!(!s[0].exported);
}

// --- parseSymbols binding patterns ---

#[test]
fn object_destructuring_each_binding_extracted() {
    let s = parse_symbols("x.ts", "export const { a, b } = obj;\n");
    assert_eq!(names(&s), vec!["a", "b"]);
    assert_eq!(s[0].kind, K::Const);
    assert!(s[0].exported);
    assert_eq!(s[1].kind, K::Const);
    assert!(s[1].exported);
}

#[test]
fn array_destructuring_skips_empty_slots() {
    let s = parse_symbols("x.ts", "export const [first, , third] = arr;\n");
    assert_eq!(names(&s), vec!["first", "third"]);
}

#[test]
fn nested_destructuring_flattened() {
    let s = parse_symbols("x.ts", "const { outer: { inner }, sibling } = obj;\n");
    let mut got = names(&s);
    got.sort_unstable();
    assert_eq!(got, vec!["inner", "sibling"]);
}
