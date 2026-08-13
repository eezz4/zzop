//! Coverage for `parse_calls`: same-file call attribution plus class heritage edges.
use super::*;

#[test]
fn simple_call_from_symbol_is_enclosing_function() {
    let calls = parse_calls(
        "x.ts",
        "export function foo() { bar(); }\nfunction bar() {}\n",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "x.ts#foo");
    assert_eq!(calls[0].callee_name, "bar");
    assert_eq!(calls[0].line, 1);
}

#[test]
fn member_expr_method_from_external_symbol_not_collected() {
    let calls = parse_calls("x.ts", "export function foo() { window.alert(\"hi\"); }\n");
    assert!(calls.is_empty());
}

#[test]
fn member_expr_method_from_same_file_symbol_is_collected() {
    let calls = parse_calls(
        "x.ts",
        "export function foo() { helper.run(); }\nexport function run() {}\n",
    );
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "x.ts#foo");
    assert_eq!(calls[0].callee_name, "run");
}

#[test]
fn call_inside_const_arrow_function_is_attributed_to_it() {
    let calls = parse_calls(
        "x.ts",
        "export const run = () => {\n  helper();\n};\nfunction helper() {}\n",
    );
    assert_eq!(calls[0].from_symbol, "x.ts#run");
    assert_eq!(calls[0].callee_name, "helper");
}

#[test]
fn multiple_calls_inside_one_function() {
    let calls = parse_calls(
        "x.ts",
        "export function main() {\n  a();\n  b();\n  c();\n}\n",
    );
    let names: Vec<&str> = calls.iter().map(|c| c.callee_name.as_str()).collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    assert!(calls.iter().all(|c| c.from_symbol == "x.ts#main"));
}

#[test]
fn call_at_file_top_level_with_no_enclosing_symbol_is_dropped() {
    let calls = parse_calls("x.ts", "console.log(\"boot\");\nexport function fn() {}\n");
    assert!(calls.is_empty());
}

#[test]
fn line_is_one_based_call_site_line() {
    let calls = parse_calls(
        "x.ts",
        "export function fn() {\n\n  helper();\n}\nfunction helper() {}\n",
    );
    assert_eq!(calls[0].line, 3);
}

#[test]
fn cross_file_method_new_svc_then_svc_do_attaches_receiver_type() {
    let calls = parse_calls(
        "x.ts",
        "import { Svc } from \"./svc\";\nexport function fn() {\n  const svc = new Svc();\n  svc.do();\n}\n",
    );
    assert!(calls.contains(&RawCall {
        from_symbol: "x.ts#fn".to_string(),
        callee_name: "do".to_string(),
        line: 4,
        receiver_type: Some("Svc".to_string()),
        is_heritage: false,
    }));
}

#[test]
fn cross_file_method_param_type_annotation_also_attaches_receiver_type() {
    let calls = parse_calls(
        "x.ts",
        "import { Svc } from \"./svc\";\nexport function fn(svc: Svc) {\n  svc.run();\n}\n",
    );
    assert_eq!(calls[0].from_symbol, "x.ts#fn");
    assert_eq!(calls[0].callee_name, "run");
    assert_eq!(calls[0].receiver_type.as_deref(), Some("Svc"));
}

#[test]
fn class_extends_emits_heritage_raw_call() {
    let calls = parse_calls("x.ts", "export class Child extends Base {}\n");
    assert!(calls.contains(&RawCall {
        from_symbol: "x.ts#Child".to_string(),
        callee_name: "Base".to_string(),
        line: 1,
        receiver_type: None,
        is_heritage: true,
    }));
}

#[test]
fn class_implements_emits_heritage_raw_call_per_interface() {
    let calls = parse_calls("x.ts", "export class Impl implements IA, IB {}\n");
    let names: Vec<&str> = calls
        .iter()
        .filter(|c| c.is_heritage)
        .map(|c| c.callee_name.as_str())
        .collect();
    assert_eq!(names, vec!["IA", "IB"]);
}

/// The DOWNSIDE half of `symbol_shapes::class`'s accessor repair, pinned where it is observable.
///
/// `find_enclosing` takes the SMALLEST covering span, so giving a setter its own leaf moves every call
/// in that body off the class node (`x.ts#C`) and onto `x.ts#C.x.set`. That id is a graph ORPHAN:
/// `zzop_core::callgraph::resolve_method` mints only two-segment `<file>#<Class>.<method>` candidates,
/// and a setter is reached by ASSIGNMENT (`c.x = v`) rather than by a call, so no call site anywhere
/// can name it — while `x.ts#C` does receive edges (`new C()` resolves to it). A rule that walks
/// reachability from a route handler therefore stops crossing INTO a setter body.
///
/// It is a narrow loss and it was measured rather than assumed, on the one channel those rules
/// actually read: `write_sites` are computed per symbol from that symbol's OWN body span, and the class
/// symbol's span still covers the whole class, so `C` and `C.x.set` BOTH carry the setter's write sites
/// (`symbols_tests::a_same_name_getter_and_setter_are_two_bodies_and_both_get_a_leaf` pins the spans
/// that make that true). `http_scan`'s `symbols_by_id` lookup off a reachable `x.ts#C` is unchanged.
/// What moves is the outgoing CALL edge, and the pre-repair attribution it moves from — "some call
/// inside class C" — was itself the coarse consequence of the setter having no leaf at all.
#[test]
fn a_call_in_a_setter_body_attributes_to_the_setter_leaf_not_the_class() {
    let calls = parse_calls(
        "x.ts",
        "class C {\n  get x() {\n    return this._x;\n  }\n  set x(v) {\n    validate(v);\n  }\n}\nfunction validate(v) {}\n",
    );
    let attributed: Vec<(&str, &str)> = calls
        .iter()
        .filter(|c| !c.is_heritage)
        .map(|c| (c.from_symbol.as_str(), c.callee_name.as_str()))
        .collect();
    assert_eq!(attributed, vec![("x.ts#C.x.set", "validate")]);
}

/// The same seam for `symbol_shapes::class`'s OVERLOAD repair — and here it moves the other way.
///
/// Before the repair the overload set's leaf had no span, so `find_enclosing` attributed every call in
/// the implementation body to the class node `x.ts#C`. Giving that leaf the implementation's span moves
/// them to `x.ts#C.foo`, which — unlike the setter leaf above — is a name the call graph CAN reach:
/// `zzop_core::callgraph::resolve_method` mints exactly this two-segment `<file>#<Class>.<method>`
/// candidate for a `c.foo()` call site. So the overload arc has no orphan half; reachability from a
/// route handler into an overloaded method's body starts working rather than stopping.
#[test]
fn a_call_in_an_overload_implementation_attributes_to_the_resolvable_method_leaf() {
    let calls = parse_calls(
        "x.ts",
        "class C {\n  foo(a: string): void;\n  foo(a: any) {\n    validate(a);\n  }\n}\nfunction validate(v) {}\n",
    );
    let attributed: Vec<(&str, &str)> = calls
        .iter()
        .filter(|c| !c.is_heritage)
        .map(|c| (c.from_symbol.as_str(), c.callee_name.as_str()))
        .collect();
    assert_eq!(attributed, vec![("x.ts#C.foo", "validate")]);
}
