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
fn class_property_not_extracted() {
    let s = parse_symbols("x.ts", "class C {\n  field = 1;\n  method() {}\n}\n");
    assert_eq!(names(&s), vec!["C", "C.method"]);
}

#[test]
fn class_computed_and_string_names_skipped() {
    let s = parse_symbols(
        "x.ts",
        "class C {\n  [\"dyn\"]() {}\n  \"str\"() {}\n  ok() {}\n}\n",
    );
    assert_eq!(names(&s), vec!["C", "C.ok"]);
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
