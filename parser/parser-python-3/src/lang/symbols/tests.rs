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

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line, decorators included ---

/// `zzop_core::SourceSymbol`'s "Body span contract". `@app.route`/`@task`-anchored concepts, and the
/// `async def` header itself, are only writable as method-scan rules when the decorators and the `def`
/// line are inside the span. Under the first-statement reading none of them ever was — which is the
/// direct reason the shipped `async`-handler concept exists for TypeScript and has no `.py` twin.
#[test]
fn function_body_span_starts_at_the_first_decorator() {
    let src = "@app.get('/x')\n@audited\nasync def handler(\n    a: int,\n):\n    await go(a)\n";
    let out = parse_symbols("a.py", src);
    let s = sym(&out, "handler");
    assert_eq!(s.body_start, Some(1));
    assert_eq!(s.body_end, Some(6));
}

#[test]
fn class_body_span_starts_at_the_class_declaration() {
    let src = "@register\nclass Foo:\n    X = 1\n\n    def bar(self):\n        return 1\n";
    let out = parse_symbols("a.py", src);
    let c = sym(&out, "Foo");
    assert_eq!(c.body_start, Some(1));
    assert_eq!(c.body_end, Some(6));
    let bar = sym(&out, "Foo.bar");
    assert_eq!(bar.body_start, Some(5));
    // Containment: the class span must still cover every leaf, or `drop_outer_spans` stops nesting.
    assert!(c.body_start.unwrap() <= bar.body_start.unwrap());
    assert!(c.body_end.unwrap() >= bar.body_end.unwrap());
}

// --- LEAF COMPLETENESS: a class body is a STATEMENT list, and those statements need a leaf ---

/// A Python class body executes at class-creation time — it is the exact analogue of a Java
/// `static { … }` block, and it had the exact same hole: only `Stmt::FunctionDef` projected anything,
/// so the moment a class held one method `drop_outer_spans` discarded the class-wide span and every
/// class-body statement became unreachable to `.py` method-scan rules. Each maximal RUN of consecutive
/// non-declaration statements now projects one leaf, ordinal-suffixed from the second on, mirroring
/// `zzop_parser_typescript`'s `STATIC_BLOCK` naming (a `-` cannot appear in a Python identifier).
#[test]
fn class_body_statement_runs_each_project_their_own_leaf_span() {
    let src = concat!(
        "class Foo:\n",
        "    parser = make_parser()\n",
        "    parser.setFeature('x')\n",
        "\n",
        "    def bar(self):\n",
        "        return 1\n",
        "\n",
        "    LIMIT = 3\n",
    );
    let out = parse_symbols("a.py", src);
    let first = sym(&out, "Foo.class-body");
    assert_eq!(first.kind, SourceSymbolKind::Function);
    assert_eq!(first.body_start, Some(2));
    assert_eq!(first.body_end, Some(3));
    let second = sym(&out, "Foo.class-body-2");
    assert_eq!(second.body_start, Some(8));
    assert_eq!(second.body_end, Some(8));
}

/// A class whose body is only `def`s (or only a docstring-free `pass`) must not grow phantom leaves —
/// the run leaf exists to cover statements, so no statements means no leaf.
#[test]
fn a_class_of_only_methods_projects_no_class_body_leaf() {
    let src = "class Foo:\n    def bar(self):\n        return 1\n";
    let out = parse_symbols("a.py", src);
    assert!(out.iter().all(|s| !s.name.contains("class-body")));
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

/// Same-defect-class audit pin (see `zzop_parser_go::lang::symbols`'s comment pins): ruff's AST
/// (`StmtFunctionDef::body: Vec<Stmt>`) never represents a standalone `#` comment as a statement at all
/// — comments are discarded during tokenization, not retained as body nodes the way tree-sitter's
/// `comment` "extra" is. `body_start` no longer reads the body at all, and `body_end` reads only the
/// last statement, so a comment cannot move either boundary in any position. This proves that rather
/// than assuming it.
#[test]
fn function_body_opening_with_comment_is_unaffected() {
    let src = "def f():\n    # leading comment\n    x = 1\n    return x\n";
    let out = parse_symbols("a.py", src);
    let s = sym(&out, "f");
    assert_eq!(s.body_start, Some(1));
    assert_eq!(s.body_end, Some(4));
}
