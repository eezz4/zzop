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
