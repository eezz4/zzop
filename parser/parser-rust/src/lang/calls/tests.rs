use super::*;

fn names(rel: &str, src: &str) -> Vec<(String, String)> {
    parse_calls(rel, src)
        .into_iter()
        .map(|c| (c.from_symbol, c.callee_name))
        .collect()
}

#[test]
fn a_free_function_call_is_attributed_to_its_enclosing_function() {
    let got = names("a.rs", "fn caller() {\n    callee();\n}\n");
    assert_eq!(got, vec![("a.rs#caller".to_string(), "callee".to_string())]);
}

/// The symbol id an impl method gets here MUST equal the one `symbols.rs` emits (`Type.method`), or
/// every edge out of a method dangles at resolution time. This is the pin for that agreement.
#[test]
fn an_impl_method_uses_the_same_symbol_id_shape_symbols_rs_emits() {
    let src = "struct S;\nimpl S {\n    fn run(&self) {\n        helper();\n    }\n}\n";
    assert_eq!(
        names("a.rs", src),
        vec![("a.rs#S.run".to_string(), "helper".to_string())]
    );
    // ...and the symbol side really does spell it that way.
    let ids: Vec<String> = crate::lang::symbols::parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(ids.contains(&"a.rs#S.run".to_string()), "got: {ids:?}");
}

#[test]
fn a_method_call_records_the_method_name_without_guessing_a_receiver_type() {
    let calls = parse_calls("a.rs", "fn f(x: &dyn T) {\n    x.run();\n}\n");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].callee_name, "run");
    assert_eq!(
        calls[0].receiver_type, None,
        "no type layer means no receiver type — an invented one would resolve to a wrong edge"
    );
}

/// `Type::assoc()` is the one shape that CAN carry a receiver type, and it is what makes a cross-file
/// `<file>#<Type>.<assoc>` edge resolvable.
#[test]
fn an_associated_function_call_carries_its_type_as_the_receiver() {
    let calls = parse_calls("a.rs", "fn f() {\n    Config::load();\n}\n");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].callee_name, "load");
    assert_eq!(calls[0].receiver_type.as_deref(), Some("Config"));
}

#[test]
fn calls_inside_closures_match_arms_and_loops_are_attributed_to_the_enclosing_symbol() {
    let src = "fn f(v: Vec<u32>) {\n\
               \x20   v.iter().map(|x| helper(x));\n\
               \x20   match v.len() {\n\
               \x20       0 => zero(),\n\
               \x20       _ => other(),\n\
               \x20   };\n\
               \x20   for i in v {\n\
               \x20       looped(i);\n\
               \x20   }\n\
               }\n";
    let got: Vec<String> = parse_calls("a.rs", src)
        .into_iter()
        .map(|c| c.callee_name)
        .collect();
    for expected in ["helper", "zero", "other", "looped"] {
        assert!(got.contains(&expected.to_string()), "{expected} in {got:?}");
    }
    assert!(
        parse_calls("a.rs", src)
            .iter()
            .all(|c| c.from_symbol == "a.rs#f"),
        "every call in one function body belongs to that function"
    );
}

#[test]
fn an_await_and_a_try_do_not_hide_the_call_underneath() {
    let got: Vec<String> = parse_calls("a.rs", "async fn f() {\n    fetch().await?;\n}\n")
        .into_iter()
        .map(|c| c.callee_name)
        .collect();
    assert_eq!(got, vec!["fetch".to_string()]);
}

/// The blindness this extractor declares in its own module doc, pinned so it is a KNOWN boundary rather
/// than a surprise: a call written inside a macro invocation is an opaque token stream to `syn`.
#[test]
fn a_call_inside_a_macro_is_not_seen_and_that_is_the_documented_boundary() {
    assert!(parse_calls("a.rs", "fn f() {\n    println!(\"{}\", helper());\n}\n").is_empty());
}

#[test]
fn an_unparseable_file_yields_no_calls_rather_than_panicking() {
    assert!(parse_calls("a.rs", "fn f(:\n").is_empty());
}
