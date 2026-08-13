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

/// The defect this module's "Inline `mod` bodies" section records, at its smallest. Before
/// qualification the inline `handler` and the top-level `handler` were both attributed to
/// `a.rs#handler`, so the top-level one's node carried `verify_token` — a call its own body never makes,
/// and downstream a `mutating-route-no-auth` clearance for a route that checks nothing. The inline call
/// is recorded now; what must never happen is it landing on the TOP-LEVEL id.
#[test]
fn inline_mod_calls_are_not_attributed_to_a_homonym_top_level_symbol() {
    let src = "fn handler() {\n\
               \x20   plain();\n\
               }\n\
               fn verify_token() {}\n\
               mod v1 {\n\
               \x20   use super::verify_token;\n\
               \x20   pub fn handler() {\n\
               \x20       verify_token();\n\
               \x20   }\n\
               }\n";
    let got = names("a.rs", src);
    assert_eq!(
        got,
        vec![
            ("a.rs#handler".to_string(), "plain".to_string()),
            ("a.rs#v1::handler".to_string(), "verify_token".to_string()),
        ],
        "an inline mod's call must carry its own qualified id, never the top-level homonym's"
    );
    assert!(
        !got.contains(&("a.rs#handler".to_string(), "verify_token".to_string())),
        "the top-level node must never acquire the inline mod's guard call: {got:?}"
    );
}

/// The callee side of the same agreement (module doc's "what the CALLEE side qualifies"). A bare call
/// to a name the SAME inline `mod` declares must name that mod's symbol, or the edge resolves to a
/// file-level homonym — the first failure mode again, one step later in the pipeline.
#[test]
fn a_bare_call_to_a_same_mod_sibling_is_qualified_but_an_outer_name_is_not() {
    let src = "fn shared() {}\n\
               mod v1 {\n\
               \x20   fn sibling() {}\n\
               \x20   pub fn entry() {\n\
               \x20       sibling();\n\
               \x20       shared();\n\
               \x20   }\n\
               }\n";
    assert_eq!(
        names("a.rs", src),
        vec![
            ("a.rs#v1::entry".to_string(), "v1::sibling".to_string()),
            ("a.rs#v1::entry".to_string(), "shared".to_string()),
        ]
    );
}

/// `v1::entry()` written OUTSIDE the mod: the qualifier is a module, not a type, so it must become part
/// of the callee NAME with no `receiver_type` — handing `v1` to `resolve_method` would look for a type
/// named `v1` and mint a candidate id nothing declares.
#[test]
fn a_path_call_through_an_inline_mod_names_the_module_rather_than_a_receiver_type() {
    let src = "mod v1 {\n\
               \x20   pub fn entry() {}\n\
               }\n\
               fn top() {\n\
               \x20   v1::entry();\n\
               }\n";
    let calls = parse_calls("a.rs", src);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].from_symbol, "a.rs#top");
    assert_eq!(calls[0].callee_name, "v1::entry");
    assert_eq!(calls[0].receiver_type, None);
}

/// Control for the rule above: a qualifier that is NOT an inline `mod` in this file keeps the
/// `Type::assoc` reading it has always had. Without this the new branch would swallow every associated
/// call in the workspace.
#[test]
fn a_type_qualified_call_keeps_its_receiver_type() {
    let calls = parse_calls("a.rs", "fn top() {\n    Session::open();\n}\n");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].callee_name, "open");
    assert_eq!(calls[0].receiver_type, Some("Session".to_string()));
}

/// The AGREEMENT pin between this module and `lang::symbols`, stated as the invariant rather than as a
/// list of shapes: every `from_symbol` this extractor mints must be an id `parse_symbols` actually
/// emits for the same source. Widening either module's scope alone breaks this.
#[test]
fn every_from_symbol_is_a_symbol_parse_symbols_emits() {
    let src = "fn top() {\n\
               \x20   a();\n\
               }\n\
               struct S;\n\
               impl S {\n\
               \x20   fn m(&self) {\n\
               \x20       b();\n\
               \x20   }\n\
               }\n\
               mod inner {\n\
               \x20   pub fn nested() {\n\
               \x20       c();\n\
               \x20   }\n\
               \x20   pub struct T;\n\
               \x20   impl T {\n\
               \x20       pub fn n(&self) {\n\
               \x20           d();\n\
               \x20       }\n\
               \x20   }\n\
               }\n\
               fn outer_with_nested_fn() {\n\
               \x20   fn helper() {\n\
               \x20       e();\n\
               \x20   }\n\
               \x20   helper();\n\
               }\n";
    let ids: Vec<String> = crate::lang::symbols::parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    let calls = parse_calls("a.rs", src);
    // Non-vacuity: an extractor that returned nothing would satisfy the invariant below trivially.
    let callees: Vec<&str> = calls.iter().map(|c| c.callee_name.as_str()).collect();
    assert_eq!(
        callees,
        vec!["a", "b", "c", "d", "helper"],
        "got {callees:?}"
    );
    for call in calls {
        assert!(
            ids.contains(&call.from_symbol),
            "`{}` (calling `{}`) is not a symbol `parse_symbols` emits — symbols: {ids:?}",
            call.from_symbol,
            call.callee_name
        );
    }
}
