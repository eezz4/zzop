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

// --- `trait` ASSOCIATED ITEMS: the extraction-scope half of the span contract ---

/// The gap this arm closed: a default body is executable code, and until the walk descended into the
/// trait it sat inside NO symbol's span — a `Command::new` + `format!` pair written here was invisible
/// to the same method-scan rule that reads the identical pair in an `impl` method.
#[test]
fn a_trait_default_method_body_is_covered_by_its_own_span() {
    let src = concat!(
        "trait Health {\n",
        "    fn count(&self) -> usize;\n",
        "    fn ready(&self) -> bool {\n",
        "        self.count() > 0\n",
        "    }\n",
        "}\n",
    );
    let out = parse_symbols("a.rs", src);
    let ready = sym(&out, "Health.ready");
    assert_eq!(ready.kind, SourceSymbolKind::Function);
    assert_eq!(ready.id, "a.rs#Health.ready");
    assert_eq!(ready.line, 3);
    assert_eq!((ready.body_start, ready.body_end), (Some(3), Some(5)));
}

/// The other half, and the one that says what `None` MEANS: a bare signature encloses nothing, so it
/// reports no region rather than a zero-width or declaration-only one
/// (`zzop_core::SourceSymbol`'s span contract names this exact shape).
#[test]
fn a_body_less_trait_signature_reports_no_span() {
    let out = parse_symbols("a.rs", "trait Health {\n    fn count(&self) -> usize;\n}\n");
    let count = sym(&out, "Health.count");
    assert_eq!(count.kind, SourceSymbolKind::Function);
    assert_eq!((count.body_start, count.body_end), (None, None));
}

/// The trait ITSELF keeps `None` — its body is an associated-item list, not a statement list, and every
/// item in it now projects its own leaf. This is what makes the change pure addition: `drop_outer_spans`
/// has no container span here to discard.
#[test]
fn the_trait_itself_still_projects_an_interface_with_no_span() {
    let src = "trait Health {\n    fn ready(&self) -> bool {\n        true\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    let t = sym(&out, "Health");
    assert_eq!(t.kind, SourceSymbolKind::Interface);
    assert_eq!((t.body_start, t.body_end), (None, None));
}

/// `body_start` is the DECLARATION's line, attributes included — the same contract the top-level and
/// `impl` arms obey, verified separately here because a trait item's attributes live on a different
/// `syn` type (`TraitItemFn`) than either of those.
#[test]
fn a_trait_default_method_span_starts_at_its_first_attribute() {
    let src = "trait T {\n    #[inline]\n    fn m(&self) -> u32 {\n        1\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    let m = sym(&out, "T.m");
    assert_eq!(m.line, 3); // the `fn` token — unchanged
    assert_eq!((m.body_start, m.body_end), (Some(2), Some(5)));
}

/// A trait member and a trait-IMPL member of the same trait are two ids, because the impl side names
/// its members after the TYPE. Nothing has to be defended for them not to collide: Rust puts traits and
/// types in one namespace, so `trait Health` and `struct Health` cannot share a module.
#[test]
fn trait_members_and_impl_members_of_the_same_trait_are_distinct_ids() {
    let src = concat!(
        "struct Foo;\n",
        "trait Health {\n    fn ready(&self) -> bool {\n        true\n    }\n}\n",
        "impl Health for Foo {\n    fn ready(&self) -> bool {\n        false\n    }\n}\n",
    );
    let ids: Vec<String> = parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(
        ids,
        vec![
            "a.rs#Foo",
            "a.rs#Health",
            "a.rs#Health.ready",
            "a.rs#Foo.ready",
        ]
    );
}

/// `exported` is the TRAIT's visibility — a trait item has none of its own to read (`syn::TraitItemFn`
/// carries no `vis` field at all), so a blanket `false` would call a `pub trait`'s default body
/// unreachable from another file when it plainly is not. Deliberately UNLIKE the trait-impl arm, whose
/// trait may live in a file this frontend cannot follow.
#[test]
fn trait_member_exported_follows_the_traits_own_visibility() {
    let src = concat!(
        "pub trait Open {\n    fn a(&self) {}\n    const K: u8 = 1;\n}\n",
        "trait Closed {\n    fn b(&self) {}\n}\n",
    );
    let out = parse_symbols("a.rs", src);
    assert!(sym(&out, "Open.a").exported);
    assert!(sym(&out, "Open.K").exported);
    assert!(!sym(&out, "Closed.b").exported);
}

/// Associated `const`s are emitted for the same reason `emit_impl` emits them, and associated TYPES are
/// absent on both sides — the two walks answer identically about what a member is.
#[test]
fn a_trait_associated_const_is_a_const_and_an_associated_type_is_not_emitted() {
    let src = "trait T {\n    const MAX: u8 = 3;\n    type Item;\n}\n";
    let names: Vec<String> = parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(names, vec!["T", "T.MAX"]);
    assert_eq!(
        sym(&parse_symbols("a.rs", src), "T.MAX").kind,
        SourceSymbolKind::Const
    );
}

/// A trait inside an inline `mod` composes both separators, same as an `impl` there.
#[test]
fn a_trait_inside_an_inline_mod_composes_both_separators() {
    let src = "mod v1 {\n    pub trait T {\n        fn m(&self) {}\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    assert_eq!(sym(&out, "v1::T").kind, SourceSymbolKind::Interface);
    assert_eq!(sym(&out, "v1::T.m").id, "a.rs#v1::T.m");
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

/// The qualification contract, at its smallest: an inline `mod`'s item is THIS file's symbol, named
/// with its module chain. Before this, `parse_symbols` emitted nothing for a nested item, so
/// `lang::calls` had no honest id to attribute a nested call to.
#[test]
fn an_inline_mod_item_is_qualified_with_its_module_chain() {
    let src = "mod v1 {\n    pub fn handler() {}\n}\n";
    let out = parse_symbols("a.rs", src);
    let s = sym(&out, "v1::handler");
    assert_eq!(s.id, "a.rs#v1::handler");
    assert_eq!(s.kind, SourceSymbolKind::Function);
    assert!(s.exported, "`pub fn` inside a mod is still written `pub`");
}

/// The whole point of qualifying: a nested homonym and a top-level one are two ids, so neither can
/// carry the other's call-graph edges (the measured `mutating-route-no-auth` clearance).
#[test]
fn a_nested_homonym_never_shares_the_top_level_id() {
    let src = "fn handler() {}\nmod v1 {\n    pub fn handler() {}\n}\n";
    let ids: Vec<String> = parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec!["a.rs#handler", "a.rs#v1::handler"]);
}

/// The two separators compose and each keeps its own meaning — `::` for modules, `.` for the type a
/// member hangs off (module doc's `Type.member` section).
#[test]
fn an_impl_inside_an_inline_mod_composes_both_separators() {
    let src = "mod v1 {\n\
               \x20   pub struct T;\n\
               \x20   impl T {\n\
               \x20       pub fn run(&self) {}\n\
               \x20       pub const K: u8 = 1;\n\
               \x20   }\n\
               }\n";
    let out = parse_symbols("a.rs", src);
    assert_eq!(sym(&out, "v1::T").kind, SourceSymbolKind::Class);
    assert_eq!(sym(&out, "v1::T.run").kind, SourceSymbolKind::Function);
    assert_eq!(sym(&out, "v1::T.K").kind, SourceSymbolKind::Const);
}

#[test]
fn nesting_composes_to_arbitrary_depth() {
    let out = parse_symbols(
        "a.rs",
        "mod a {\n    mod b {\n        fn c() {}\n    }\n}\n",
    );
    assert_eq!(sym(&out, "a::b::c").id, "a.rs#a::b::c");
}

/// A `mod x;` DECLARATION names another FILE — it has no body here, so it contributes no symbol. That
/// fact belongs to `lang::imports`, which turns it into a `self::x` binding.
#[test]
fn a_mod_declaration_without_a_body_contributes_no_symbol() {
    assert!(parse_symbols("a.rs", "mod other;\n").is_empty());
}

/// A `#[cfg(test)] mod tests` block is walked like any other inline `mod` — safe precisely because its
/// items land in the `tests::` namespace, where no deployed symbol can collide with them.
#[test]
fn a_cfg_test_mod_is_walked_and_lands_in_its_own_namespace() {
    let src = "fn ship() {}\n#[cfg(test)]\nmod tests {\n    fn ship() {}\n}\n";
    let ids: Vec<String> = parse_symbols("a.rs", src)
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec!["a.rs#ship", "a.rs#tests::ship"]);
}

/// The span rule survives qualification: a nested fn still carries its own body span, so a method-scan
/// rule can reach statements that were previously inside no leaf at all.
#[test]
fn a_nested_fn_carries_its_own_body_span() {
    let src = "mod v1 {\n    pub fn handler() {\n        let x = 1;\n    }\n}\n";
    let out = parse_symbols("a.rs", src);
    let s = sym(&out, "v1::handler");
    assert_eq!((s.body_start, s.body_end), (Some(2), Some(4)));
}
