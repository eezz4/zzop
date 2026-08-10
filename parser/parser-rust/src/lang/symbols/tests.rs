use super::*;

fn sym<'a>(out: &'a [SourceSymbol], name: &str) -> &'a SourceSymbol {
    out.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name:?} in {out:?}"))
}

#[test]
fn top_level_fn_is_extracted() {
    let out = parse_symbols("a.rs", "fn hello() {}\n");
    let s = sym(&out, "hello");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert_eq!(s.id, "a.rs#hello");
    assert_eq!(s.line, 1);
}

#[test]
fn struct_enum_union_map_to_class() {
    let src = "struct S {}\nenum E {}\nunion U { a: i32 }\n";
    let out = parse_symbols("a.rs", src);
    assert_eq!(sym(&out, "S").kind, SourceSymbolKind::Class);
    assert_eq!(sym(&out, "E").kind, SourceSymbolKind::Class);
    assert_eq!(sym(&out, "U").kind, SourceSymbolKind::Class);
}

#[test]
fn trait_maps_to_interface() {
    let out = parse_symbols("a.rs", "trait T {}\n");
    assert_eq!(sym(&out, "T").kind, SourceSymbolKind::Interface);
}

#[test]
fn type_alias_maps_to_type() {
    let out = parse_symbols("a.rs", "type Alias = i32;\n");
    assert_eq!(sym(&out, "Alias").kind, SourceSymbolKind::Type);
}

#[test]
fn const_and_static_map_to_const() {
    let out = parse_symbols("a.rs", "const X: i32 = 1;\nstatic Y: i32 = 2;\n");
    assert_eq!(sym(&out, "X").kind, SourceSymbolKind::Const);
    assert_eq!(sym(&out, "Y").kind, SourceSymbolKind::Const);
}

#[test]
fn visibility_matrix() {
    let src = concat!(
        "pub fn a() {}\n",
        "pub(crate) fn b() {}\n",
        "pub(super) fn c() {}\n",
        "fn d() {}\n",
    );
    let out = parse_symbols("a.rs", src);
    assert!(sym(&out, "a").exported);
    assert!(sym(&out, "b").exported);
    assert!(sym(&out, "c").exported);
    assert!(!sym(&out, "d").exported);
}

#[test]
fn pub_in_path_visibility_is_exported() {
    let out = parse_symbols("a.rs", "pub(in crate::foo) fn a() {}\n");
    assert!(sym(&out, "a").exported);
}

#[test]
fn line_numbers_are_one_based_and_track_declaration() {
    let src = "\n\nfn f() {}\n";
    let out = parse_symbols("a.rs", src);
    assert_eq!(sym(&out, "f").line, 3);
}

#[test]
fn function_body_span_covers_declaration_through_closing_brace() {
    let src = "fn f() {\n    let x = 1;\n    let y = 2;\n}\n";
    let out = parse_symbols("a.rs", src);
    let f = sym(&out, "f");
    assert_eq!(f.body_start, Some(1));
    assert_eq!(f.body_end, Some(4));
}

/// Regression pin, kept from the 2026-08-02 `body_end` fix: a MULTI-LINE last statement — most commonly
/// a loop sitting as the fn's final statement — must lie fully inside the scan region, or a
/// `trigger_in_loop` probe inside that loop can never fire (measured: this exact shape was the
/// engine-level failure that surfaced the bug the day `lang::loop_spans` landed). The original bug took
/// the last statement's START line; the span is now anchored on the brace, so the failure mode is
/// structurally gone rather than patched, and this test is what says so.
#[test]
fn function_body_end_covers_a_multi_line_last_statement() {
    let src = "fn f(xs: &[u32]) {\n    for x in xs {\n        use_it(*x);\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    let f = sym(&out, "f");
    assert_eq!(f.body_start, Some(1));
    assert_eq!(f.body_end, Some(5));
}

/// Same-defect-class audit pin (see `zzop_parser_go::lang::symbols`'s comment pins): `syn` discards
/// `//` comments during tokenization — a `syn::Block`'s `stmts: Vec<Stmt>` never contains one as an
/// item, unlike tree-sitter's `comment` "extra" node. Since the span no longer reads the block's
/// contents at all, a comment cannot move either boundary in any position. This proves that rather
/// than assuming it.
#[test]
fn function_body_opening_with_comment_is_unaffected() {
    let src = "fn f() {\n    // leading comment\n    let x = 1;\n    let y = 2;\n}\n";
    let out = parse_symbols("a.rs", src);
    let f = sym(&out, "f");
    assert_eq!(f.body_start, Some(1));
    assert_eq!(f.body_end, Some(5));
}

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line, ATTRIBUTES included ---

/// `zzop_core::SourceSymbol`'s "Body span contract". Rust is the one language where `line` is NOT the
/// declaration's first line — it is the `fn` token, a long-standing convention the call graph and the
/// census both read — so `body_start` here is deliberately allowed to sit ABOVE `line`, at the first
/// attribute. That is what makes an `#[get("/x")]`/`#[tokio::main]`-anchored method-scan concept
/// writable for `.rs` at all; under the first-statement reading the attribute was two lines outside
/// every span.
#[test]
fn function_body_span_starts_at_the_first_attribute_and_ends_at_the_closing_brace() {
    let src = "#[tokio::main]\n#[allow(dead_code)]\npub async fn handler(\n    a: u32,\n) -> u32 {\n    a\n}\n";
    let out = parse_symbols("a.rs", src);
    let f = sym(&out, "handler");
    assert_eq!(f.line, 3); // the `fn` token — unchanged, other consumers read it
    assert_eq!(f.body_start, Some(1));
    assert_eq!(f.body_end, Some(7));
}

#[test]
fn impl_method_body_span_starts_at_its_first_attribute() {
    let src =
        "struct S;\nimpl S {\n    #[inline]\n    pub fn m(&self) -> u32 {\n        1\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    let m = sym(&out, "S.m");
    assert_eq!(m.body_start, Some(3));
    assert_eq!(m.body_end, Some(6));
}

/// The contract's totality clause: an empty body still has a declaration line, so it reports a span.
/// Under the first-statement reading `fn f() {}` collapsed to `None`/`None` — indistinguishable in the
/// IR from a `struct`, which genuinely has no scannable region at all.
#[test]
fn empty_function_body_still_reports_the_declaration_span() {
    let out = parse_symbols("a.rs", "fn f() {}\n");
    let f = sym(&out, "f");
    assert_eq!(f.body_start, Some(1));
    assert_eq!(f.body_end, Some(1));
}

#[test]
fn struct_has_no_body_range() {
    let out = parse_symbols("a.rs", "struct S {\n    a: i32,\n}\n");
    let s = sym(&out, "S");
    assert_eq!(s.body_start, None);
    assert_eq!(s.body_end, None);
}

#[test]
fn inherent_impl_methods_are_dotted_type_member() {
    let src = "struct Foo;\nimpl Foo {\n    pub fn bar() {}\n    fn baz() {}\n}\n";
    let out = parse_symbols("a.rs", src);
    let bar = sym(&out, "Foo.bar");
    assert_eq!(bar.kind, SourceSymbolKind::Function);
    assert!(bar.exported);
    let baz = sym(&out, "Foo.baz");
    assert!(!baz.exported);
}

#[test]
fn trait_impl_methods_use_the_impl_type_not_the_trait_name() {
    let src = concat!(
        "struct Foo;\n",
        "trait Greet {\n    fn hello(&self);\n}\n",
        "impl Greet for Foo {\n    fn hello(&self) {}\n}\n",
    );
    let out = parse_symbols("a.rs", src);
    let hello = sym(&out, "Foo.hello");
    assert_eq!(hello.kind, SourceSymbolKind::Function);
}

#[test]
fn trait_impl_methods_carry_no_pub_keyword_so_are_not_exported() {
    // Rust's grammar forbids writing `pub` on a trait-impl item; this crate does not infer effective
    // visibility from the trait/type — see this module's doc.
    let src = "pub struct Foo;\npub trait Greet {\n    fn hello(&self);\n}\nimpl Greet for Foo {\n    fn hello(&self) {}\n}\n";
    let out = parse_symbols("a.rs", src);
    assert!(!sym(&out, "Foo.hello").exported);
}

#[test]
fn impl_associated_const_is_dotted_type_member() {
    let src = "struct Foo;\nimpl Foo {\n    pub const MAX: i32 = 10;\n}\n";
    let out = parse_symbols("a.rs", src);
    let c = sym(&out, "Foo.MAX");
    assert_eq!(c.kind, SourceSymbolKind::Const);
    assert!(c.exported);
}

#[test]
fn impl_self_type_with_generics_uses_the_leaf_name() {
    let src = "struct Foo<T> { _t: T }\nimpl<T> Foo<T> {\n    pub fn make() {}\n}\n";
    let out = parse_symbols("a.rs", src);
    assert!(sym(&out, "Foo.make").exported);
}

#[test]
fn macro_rules_is_not_extracted() {
    let out = parse_symbols("a.rs", "macro_rules! m {\n    () => {};\n}\n");
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn item_inside_inline_mod_is_out_of_v1_scope() {
    let out = parse_symbols("a.rs", "mod inner {\n    pub fn hidden() {}\n}\n");
    assert!(
        out.iter().all(|s| s.name != "hidden"),
        "inline mod body should not be walked in v1: {out:?}"
    );
}

#[test]
fn parse_failure_yields_empty_vec() {
    assert!(parse_symbols("bad.rs", "fn f(:\n").is_empty());
}

#[test]
fn empty_file_yields_empty_vec() {
    assert!(parse_symbols("e.rs", "").is_empty());
}

#[test]
fn declaration_order_is_preserved() {
    let src = "fn b() {}\nfn a() {}\nstruct Z;\n";
    let out = parse_symbols("a.rs", src);
    let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, vec!["b", "a", "Z"]);
}
