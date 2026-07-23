use zzop_core::SourceSymbolKind;

use super::*;

fn find<'a>(symbols: &'a [zzop_core::SourceSymbol], name: &str) -> &'a zzop_core::SourceSymbol {
    symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("expected a symbol named {name}, got: {symbols:?}"))
}

// --- METHOD-SCAN PARITY: the old lexical crate's `class_and_method_spans_match_the_dvja_pingaction_shape`
// fixture (`parser-java/src/scan/tests.rs`) — same source, direct body-span comparison. ---

#[test]
fn body_spans_match_the_old_lexical_crate_on_the_shared_dvja_pingaction_fixture() {
    let src = "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n";
    let symbols = parse_symbols("C.java", src);
    let class = find(&symbols, "C");
    assert_eq!(class.kind, SourceSymbolKind::Class);
    assert_eq!(class.line, 1);
    // Old crate: body_start = Some(1), body_end = Some(6) — exact match.
    assert_eq!(class.body_start, Some(1));
    assert_eq!(class.body_end, Some(6));
    let run = find(&symbols, "C.run");
    assert_eq!(run.kind, SourceSymbolKind::Function);
    assert_eq!(run.line, 2);
    // Old crate: body_start = Some(2), body_end = Some(5) — exact match.
    assert_eq!(run.body_start, Some(2));
    assert_eq!(run.body_end, Some(5));
    // Documented naming deviation: the old lexical crate names the method "run" (no qualifier); this
    // crate names it "C.run" (task-pinned `Type.method` convention).
    assert!(symbols.iter().all(|s| s.name != "run"));
}

/// Same-defect-class audit pin (see `zzop_parser_go::lang::symbols`'s leading-comment `body_line_range`
/// bug this mirrors the check for): unlike Go's walk, this crate's `body_start`/`body_end` come from the
/// `body` FIELD NODE's own `line_of`/`end_line_of` (the declaration's own line, and the `{...}` node's
/// own closing line) — never from that body's first/last named child. A `comment` extra spliced in as a
/// leading child inside the body therefore can't shift either boundary. This proves that rather than
/// assuming it.
#[test]
fn method_body_opening_with_comment_is_unaffected() {
    let src = "public class C {\n  void run() {\n    // leading comment\n    int x = 1;\n  }\n}\n";
    let symbols = parse_symbols("C.java", src);
    let run = find(&symbols, "C.run");
    assert_eq!(run.body_start, Some(2));
    assert_eq!(run.body_end, Some(5));
}

// --- all type kinds ---

#[test]
fn all_five_type_declaration_kinds_map_per_task_doc() {
    let src = "class A {}\ninterface B {}\nenum C { X }\nrecord D(int x) {}\n@interface E {}\n";
    let symbols = parse_symbols("K.java", src);
    assert_eq!(find(&symbols, "A").kind, SourceSymbolKind::Class);
    assert_eq!(find(&symbols, "B").kind, SourceSymbolKind::Interface);
    assert_eq!(find(&symbols, "C").kind, SourceSymbolKind::Class);
    assert_eq!(find(&symbols, "D").kind, SourceSymbolKind::Class);
    assert_eq!(find(&symbols, "E").kind, SourceSymbolKind::Interface);
}

// --- nested types: dot-qualified naming ---

#[test]
fn nested_type_and_its_method_are_dot_qualified() {
    let src = "class Outer {\n  class Inner {\n    void go() {\n      int x = 1;\n    }\n  }\n}\n";
    let symbols = parse_symbols("Nested.java", src);
    let inner = find(&symbols, "Outer.Inner");
    assert_eq!(inner.kind, SourceSymbolKind::Class);
    let go = find(&symbols, "Outer.Inner.go");
    assert_eq!(go.kind, SourceSymbolKind::Function);
    assert_eq!(go.body_start, Some(3));
    assert_eq!(go.body_end, Some(5));
}

// --- constructors, including a record's compact constructor ---

#[test]
fn a_constructor_is_a_function_symbol_named_type_dot_type() {
    let src = "class C {\n  C(int x) {\n    this.x = x;\n  }\n}\n";
    let symbols = parse_symbols("Ctor.java", src);
    let ctor = find(&symbols, "C.C");
    assert_eq!(ctor.kind, SourceSymbolKind::Function);
    assert_eq!(ctor.body_start, Some(2));
    assert_eq!(ctor.body_end, Some(4));
}

#[test]
fn a_records_compact_constructor_is_a_function_symbol() {
    let src = "record Point(int x, int y) {\n  public Point {\n    if (x < 0) throw new IllegalArgumentException();\n  }\n}\n";
    let symbols = parse_symbols("Compact.java", src);
    let ctor = find(&symbols, "Point.Point");
    assert_eq!(ctor.kind, SourceSymbolKind::Function);
    assert!(ctor.exported);
}

// --- static-final fields as Const; instance fields not symbol-surface ---

#[test]
fn only_static_final_fields_are_const_symbols() {
    let src = "class C {\n  static final int A = 1;\n  int instance = 2;\n  final int justFinal = 3;\n  static int justStatic = 4;\n}\n";
    let symbols = parse_symbols("F.java", src);
    assert_eq!(find(&symbols, "C.A").kind, SourceSymbolKind::Const);
    assert!(symbols.iter().all(|s| s.name != "C.instance"));
    assert!(symbols.iter().all(|s| s.name != "C.justFinal"));
    assert!(symbols.iter().all(|s| s.name != "C.justStatic"));
}

#[test]
fn a_grouped_const_declaration_emits_one_symbol_per_name() {
    let src = "class C {\n  static final int A = 1, B = 2;\n}\n";
    let symbols = parse_symbols("G.java", src);
    let a = find(&symbols, "C.A");
    let b = find(&symbols, "C.B");
    assert_eq!(a.line, 2);
    assert_eq!(b.line, 2);
}

#[test]
fn an_interface_constant_is_always_const_with_no_modifiers_written() {
    let src = "interface I {\n  String NAME = \"x\";\n}\n";
    let symbols = parse_symbols("IC.java", src);
    let c = find(&symbols, "I.NAME");
    assert_eq!(c.kind, SourceSymbolKind::Const);
    assert!(c.exported);
}

// --- visibility matrix ---

#[test]
fn public_and_protected_members_are_exported_private_and_package_private_are_not() {
    let src = concat!(
        "class C {\n",
        "  public void pub() {}\n",
        "  protected void prot() {}\n",
        "  private void priv() {}\n",
        "  void pkg() {}\n",
        "}\n",
    );
    let symbols = parse_symbols("V.java", src);
    assert!(find(&symbols, "C.pub").exported);
    assert!(find(&symbols, "C.prot").exported);
    assert!(!find(&symbols, "C.priv").exported);
    assert!(!find(&symbols, "C.pkg").exported);
}

#[test]
fn interface_members_are_implicitly_public_unless_explicitly_private() {
    let src = "interface I {\n  void a();\n  private void b() {}\n}\n";
    let symbols = parse_symbols("VI.java", src);
    assert!(find(&symbols, "I.a").exported);
    assert!(!find(&symbols, "I.b").exported);
}

#[test]
fn package_private_top_level_class_is_not_exported() {
    let symbols = parse_symbols("PP.java", "class C {}\n");
    assert!(!find(&symbols, "C").exported);
    let symbols = parse_symbols("PU.java", "public class C {}\n");
    assert!(find(&symbols, "C").exported);
}

// --- Java 21 syntax: records, sealed interfaces, pattern-matching switch ---

#[test]
fn a_sealed_interface_with_permits_still_extracts_symbols() {
    let src = "sealed interface Shape permits Circle, Square {}\nfinal class Circle implements Shape {}\nfinal class Square implements Shape {}\n";
    let symbols = parse_symbols("Sealed.java", src);
    assert_eq!(find(&symbols, "Shape").kind, SourceSymbolKind::Interface);
    assert_eq!(find(&symbols, "Circle").kind, SourceSymbolKind::Class);
    assert_eq!(find(&symbols, "Square").kind, SourceSymbolKind::Class);
}

#[test]
fn pattern_matching_switch_inside_a_method_body_does_not_break_extraction() {
    let src = concat!(
        "sealed interface Shape permits Circle, Square {}\n",
        "record Circle(double r) implements Shape {}\n",
        "record Square(double s) implements Shape {}\n",
        "class Describer {\n",
        "  String describe(Shape shape) {\n",
        "    return switch (shape) {\n",
        "      case Circle c -> \"circle\";\n",
        "      case Square sq -> \"square\";\n",
        "    };\n",
        "  }\n",
        "}\n",
    );
    let symbols = parse_symbols("Pm.java", src);
    let describe = find(&symbols, "Describer.describe");
    assert_eq!(describe.body_start, Some(5));
    assert_eq!(describe.body_end, Some(10));
}

// --- record component fixture from the old crate's own tests (bonus continuity) ---

#[test]
fn a_record_declaration_is_classified_as_a_class_not_a_method() {
    let src = "record Point(int x, int y) {\n  int sum() {\n    return x + y;\n  }\n}\n";
    let symbols = parse_symbols("R.java", src);
    assert_eq!(find(&symbols, "Point").kind, SourceSymbolKind::Class);
    assert_eq!(find(&symbols, "Point.sum").kind, SourceSymbolKind::Function);
}

// --- abstract/interface methods without a body carry no body span ---

#[test]
fn interface_abstract_methods_carry_no_body_span() {
    let src = "interface Foo {\n  void bar();\n  int baz(int x);\n}\n";
    let symbols = parse_symbols("I2.java", src);
    let bar = find(&symbols, "Foo.bar");
    assert_eq!(bar.body_start, None);
    assert_eq!(bar.body_end, None);
}

// --- partial-ERROR region extraction ---

#[test]
fn a_broken_member_amid_an_otherwise_valid_class_does_not_blank_the_whole_file() {
    let src = "class C {\n  void good() {\n    int x = 1;\n  }\n  void broken( {{{ this is not valid java\n  void alsoGood() {\n    int y = 2;\n  }\n}\n";
    let symbols = parse_symbols("Partial.java", src);
    assert!(symbols.iter().any(|s| s.name == "C.good"));
    assert!(symbols.iter().any(|s| s.name == "C"));
}

// --- empty file ---

#[test]
fn empty_file_yields_no_symbols() {
    assert!(parse_symbols("E.java", "").is_empty());
}

// --- annotation-type element declarations and record components are out of v1 scope ---

#[test]
fn annotation_type_elements_are_not_extracted_as_methods() {
    let src = "@interface Ann {\n  String value() default \"\";\n}\n";
    let symbols = parse_symbols("At.java", src);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Ann");
}
