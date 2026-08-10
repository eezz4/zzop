use super::*;

fn names(text: &str) -> Vec<String> {
    parse_symbols("f.cs", text)
        .into_iter()
        .map(|s| s.name)
        .collect()
}

#[test]
fn top_level_class_and_method() {
    let src = "public class C { public void M() {} }";
    let syms = parse_symbols("f.cs", src);
    let class = syms.iter().find(|s| s.name == "C").unwrap();
    assert_eq!(class.kind, SourceSymbolKind::Class);
    assert!(class.exported);
    let method = syms.iter().find(|s| s.name == "C.M").unwrap();
    assert_eq!(method.kind, SourceSymbolKind::Function);
    assert!(method.exported);
    assert!(method.body_start.is_some());
}

/// Same-defect-class audit pin (see `zzop_parser_go::lang::symbols`'s leading-comment `body_line_range`
/// bug this mirrors the check for): `body_start`/`body_end` here come from the `body` FIELD NODE's own
/// `line_of`/`end_line_of` (the method's own declaration line, and the `{...}` node's own closing line)
/// — never from that body's first/last named child — so a `comment` extra spliced in as a leading child
/// inside the body can't shift either boundary. This proves that rather than assuming it.
#[test]
fn method_body_opening_with_comment_is_unaffected() {
    let src =
        "public class C {\n  public void M() {\n    // leading comment\n    int x = 1;\n  }\n}\n";
    let syms = parse_symbols("f.cs", src);
    let method = syms.iter().find(|s| s.name == "C.M").unwrap();
    assert_eq!(method.body_start, Some(2));
    assert_eq!(method.body_end, Some(5));
}

#[test]
fn nested_type_dot_qualification() {
    let src = "class Outer { class Inner { void M() {} } }";
    let ns = names(src);
    assert!(ns.contains(&"Outer".to_string()));
    assert!(ns.contains(&"Outer.Inner".to_string()));
    assert!(ns.contains(&"Outer.Inner.M".to_string()));
}

#[test]
fn namespace_is_transparent_to_symbol_qualification() {
    let src = "namespace Foo.Bar { class C {} }";
    let ns = names(src);
    // Namespace never contributes to the qualified name — only type nesting does.
    assert_eq!(ns, vec!["C".to_string()]);
}

#[test]
fn file_scoped_namespace_leaves_following_types_top_level() {
    let src = "namespace Foo;\n\nclass C {}\ninterface I {}\n";
    let ns = names(src);
    assert!(ns.contains(&"C".to_string()));
    assert!(ns.contains(&"I".to_string()));
}

#[test]
fn exported_is_a_flat_public_modifier_gate() {
    let src = "class C { public void A() {} void B() {} }";
    let syms = parse_symbols("f.cs", src);
    let a = syms.iter().find(|s| s.name == "C.A").unwrap();
    let b = syms.iter().find(|s| s.name == "C.B").unwrap();
    assert!(a.exported);
    assert!(!b.exported);
    // Top-level type itself: no `public` -> not exported (C#'s internal default).
    let c = syms.iter().find(|s| s.name == "C").unwrap();
    assert!(!c.exported);
}

#[test]
fn const_and_static_readonly_fields_are_extracted_plain_field_is_not() {
    let src = "class C {
        public const int MaxCount = 10;
        public static readonly string Name = \"x\";
        private int count;
        public static int NotReadonly;
    }";
    let syms = parse_symbols("f.cs", src);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"C.MaxCount"));
    assert!(names.contains(&"C.Name"));
    assert!(!names.contains(&"C.count"));
    assert!(!names.contains(&"C.NotReadonly"));
    let max = syms.iter().find(|s| s.name == "C.MaxCount").unwrap();
    assert_eq!(max.kind, SourceSymbolKind::Const);
    assert!(max.exported);
}

#[test]
fn grouped_const_field_emits_one_symbol_per_name() {
    let src = "class C { public const int A = 1, B = 2; }";
    let ns = names(src);
    assert!(ns.contains(&"C.A".to_string()));
    assert!(ns.contains(&"C.B".to_string()));
}

#[test]
fn property_is_extracted_as_const_kind() {
    let src = "class C { public int Count { get; set; } }";
    let syms = parse_symbols("f.cs", src);
    let prop = syms.iter().find(|s| s.name == "C.Count").unwrap();
    assert_eq!(prop.kind, SourceSymbolKind::Const);
    assert!(prop.exported);
    assert!(prop.body_start.is_some());
}

#[test]
fn struct_record_interface_delegate_enum_kinds() {
    let src = "
        struct S {}
        interface I {}
        record R(int X);
        enum E { A, B }
        delegate void D(int x);
    ";
    let syms = parse_symbols("f.cs", src);
    assert_eq!(
        syms.iter().find(|s| s.name == "S").unwrap().kind,
        SourceSymbolKind::Class
    );
    assert_eq!(
        syms.iter().find(|s| s.name == "I").unwrap().kind,
        SourceSymbolKind::Interface
    );
    assert_eq!(
        syms.iter().find(|s| s.name == "R").unwrap().kind,
        SourceSymbolKind::Class
    );
    assert_eq!(
        syms.iter().find(|s| s.name == "E").unwrap().kind,
        SourceSymbolKind::Class
    );
    let d = syms.iter().find(|s| s.name == "D").unwrap();
    assert_eq!(d.kind, SourceSymbolKind::Type);
    assert_eq!(d.body_start, None);
}

#[test]
fn compact_record_has_no_body_span() {
    let src = "record Point(int X, int Y);";
    let syms = parse_symbols("f.cs", src);
    let r = syms.iter().find(|s| s.name == "Point").unwrap();
    assert_eq!(r.body_start, None);
    assert_eq!(r.body_end, None);
}

#[test]
fn constructor_is_extracted() {
    let src = "class C { public C(int x) {} }";
    let ns = names(src);
    assert!(ns.contains(&"C.C".to_string()));
}

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line, attribute lists included ---

/// `zzop_core::SourceSymbol`'s "Body span contract". A `[HttpGet]`/`[Authorize]`-anchored method-scan
/// concept needs the attribute list inside the span, and a wrapped signature must not push the span
/// past the declaration — neither holds under the old "`body` node's own line" reading, and neither is
/// visible in a one-line-header fixture.
#[test]
fn method_body_span_starts_at_the_first_attribute_not_at_the_opening_brace() {
    let src = concat!(
        "public class C {\n",
        "    [HttpGet]\n",
        "    public int M(\n",
        "        int a\n",
        "    ) {\n",
        "        return a;\n",
        "    }\n",
        "}\n",
    );
    let syms = parse_symbols("f.cs", src);
    let m = syms.iter().find(|s| s.name == "C.M").unwrap();
    assert_eq!(m.line, 2);
    assert_eq!(m.body_start, Some(2));
    assert_eq!(m.body_end, Some(7));
}

/// An expression-bodied PROPERTY (`int P => Compute();`) carried `None`/`None` — "kept simple, out of
/// v1 span scope", which under `drop_outer_spans` is not simplicity but invisibility: the enclosing
/// type's span is discarded as soon as any sibling member projects a leaf, so the property's own
/// expression became unreachable to every `.cs` method-scan rule. An expression-bodied METHOD was
/// never in this hole (`arrow_expression_clause` IS the `body` field there), which is exactly what made
/// the property gap easy to miss.
#[test]
fn expression_bodied_property_projects_a_span_like_every_other_member() {
    let src = concat!(
        "public class C {\n",
        "    public int P =>\n",
        "        Compute();\n",
        "\n",
        "    public void Sibling() {}\n",
        "}\n",
    );
    let syms = parse_symbols("f.cs", src);
    let p = syms.iter().find(|s| s.name == "C.P").unwrap();
    assert_eq!(p.body_start, Some(2));
    assert_eq!(p.body_end, Some(3));
}

/// A static constructor DOES project a symbol with a span — `tree-sitter-c-sharp` spells it
/// `constructor_declaration` like any other, so it never needed the special handling this module's doc
/// claimed for years that it lacked. Pinned so the doc and the behaviour cannot drift apart again.
#[test]
fn static_constructor_projects_a_span() {
    let src = "public class C {\n    static C() {\n        Boot();\n    }\n}\n";
    let syms = parse_symbols("f.cs", src);
    let ctor = syms.iter().find(|s| s.name == "C.C").unwrap();
    assert_eq!(ctor.body_start, Some(2));
    assert_eq!(ctor.body_end, Some(4));
}

#[test]
fn parse_symbols_empty_on_parse_failure() {
    assert!(parse_symbols("f.cs", "\u{0}\u{1}not csharp{{{{").is_empty());
}
