use super::*;

fn sym<'a>(out: &'a [SourceSymbol], name: &str) -> &'a SourceSymbol {
    out.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name:?} in {out:?}"))
}

#[test]
fn top_level_function_is_a_function_symbol() {
    let out = parse_symbols("a.py", "def foo():\n    return 1\n");
    let s = sym(&out, "foo");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert_eq!(s.line, 1);
    assert!(s.exported);
    assert_eq!(s.id, "a.py#foo");
}

#[test]
fn async_function_is_still_a_function_symbol() {
    let out = parse_symbols("a.py", "async def foo():\n    return 1\n");
    assert_eq!(sym(&out, "foo").kind, SourceSymbolKind::Function);
}

#[test]
fn underscore_prefixed_function_is_not_exported() {
    let out = parse_symbols("a.py", "def _helper():\n    pass\n");
    assert!(!sym(&out, "_helper").exported);
}

#[test]
fn class_and_its_top_level_methods_are_emitted_dotted() {
    let src = concat!(
        "class Foo:\n",
        "    def bar(self):\n",
        "        return 1\n",
        "    def _baz(self):\n",
        "        return 2\n",
    );
    let out = parse_symbols("a.py", src);
    assert_eq!(sym(&out, "Foo").kind, SourceSymbolKind::Class);
    let m = sym(&out, "Foo.bar");
    assert_eq!(m.kind, SourceSymbolKind::Function);
    assert!(m.exported);
    // F4: a method's `exported` is inherited from its CLASS, never re-derived from the method's own
    // (possibly underscore-prefixed) name — `Foo` is exported, so `Foo._baz` is too, even though
    // `_baz` alone would read as private under the bare underscore rule.
    assert!(sym(&out, "Foo._baz").exported);
}

#[test]
fn method_of_a_private_class_is_not_exported_regardless_of_its_own_name() {
    let src = concat!(
        "class _Internal:\n",
        "    def public_looking(self):\n",
        "        return 1\n",
    );
    let out = parse_symbols("a.py", src);
    assert!(!sym(&out, "_Internal").exported);
    assert!(!sym(&out, "_Internal.public_looking").exported);
}

#[test]
fn nested_function_inside_a_function_is_not_a_top_level_symbol() {
    let src = "def outer():\n    def inner():\n        return 1\n    return inner\n";
    let out = parse_symbols("a.py", src);
    assert!(out.iter().any(|s| s.name == "outer"));
    assert!(!out.iter().any(|s| s.name == "inner"));
}

#[test]
fn top_level_literal_constant_is_a_const_symbol() {
    let out = parse_symbols("a.py", "MAX_RETRIES = 3\n");
    let s = sym(&out, "MAX_RETRIES");
    assert_eq!(s.kind, SourceSymbolKind::Const);
}

#[test]
fn non_literal_top_level_assignment_is_not_a_symbol() {
    let out = parse_symbols("a.py", "router = APIRouter()\n");
    assert!(!out.iter().any(|s| s.name == "router"));
}

#[test]
fn multi_target_assignment_is_not_a_const_symbol() {
    let out = parse_symbols("a.py", "A = B = 1\n");
    assert!(!out.iter().any(|s| s.name == "A" || s.name == "B"));
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(parse_symbols("bad.py", "def f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(parse_symbols("e.py", "").is_empty());
}

#[test]
fn declaration_order_is_preserved() {
    let src = "def a():\n    pass\ndef b():\n    pass\n";
    let out = parse_symbols("a.py", src);
    let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
}

// --- F4: `__all__` literal-list membership ---

#[test]
fn all_dunder_overrides_underscore_convention_both_ways() {
    // `_private` is explicitly listed in `__all__` -> exported despite its leading underscore;
    // `public` is NOT listed -> not exported despite having no leading underscore.
    let src = concat!(
        "__all__ = [\"_private\"]\n",
        "def _private():\n",
        "    pass\n",
        "def public():\n",
        "    pass\n",
    );
    let out = parse_symbols("a.py", src);
    assert!(sym(&out, "_private").exported);
    assert!(!sym(&out, "public").exported);
}

#[test]
fn all_dunder_as_a_tuple_is_also_recognized() {
    let src = "__all__ = (\"a\",)\ndef a():\n    pass\ndef b():\n    pass\n";
    let out = parse_symbols("a.py", src);
    assert!(sym(&out, "a").exported);
    assert!(!sym(&out, "b").exported);
}

#[test]
fn all_dunder_with_a_non_literal_element_falls_back_to_underscore_convention() {
    // `compute_name()` is not a static string literal -> the WHOLE `__all__` is untrustworthy, so
    // the module falls back to the plain underscore rule instead of a partial read.
    let src = concat!(
        "__all__ = [\"a\", compute_name()]\n",
        "def a():\n",
        "    pass\n",
        "def _b():\n",
        "    pass\n",
    );
    let out = parse_symbols("a.py", src);
    assert!(sym(&out, "a").exported);
    assert!(!sym(&out, "_b").exported);
}

#[test]
fn computed_all_dunder_falls_back_to_underscore_convention() {
    // `__all__` assigned from a non-list/tuple expression (a call, a name, ...) is not a static list
    // at all -> falls back the same way.
    let src = "__all__ = build_all()\ndef a():\n    pass\ndef _b():\n    pass\n";
    let out = parse_symbols("a.py", src);
    assert!(sym(&out, "a").exported);
    assert!(!sym(&out, "_b").exported);
}

#[test]
fn all_dunder_governs_class_and_const_symbols_too() {
    let src = concat!(
        "__all__ = [\"Foo\", \"MAX\"]\n",
        "class Foo:\n",
        "    pass\n",
        "class _Bar:\n",
        "    pass\n",
        "MAX = 1\n",
        "MIN = 2\n",
    );
    let out = parse_symbols("a.py", src);
    assert!(sym(&out, "Foo").exported);
    assert!(!sym(&out, "_Bar").exported);
    assert!(sym(&out, "MAX").exported);
    assert!(!sym(&out, "MIN").exported);
}

#[test]
fn empty_all_dunder_exports_nothing() {
    let src = "__all__ = []\ndef a():\n    pass\n";
    let out = parse_symbols("a.py", src);
    assert!(!sym(&out, "a").exported);
}
