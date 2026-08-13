use zzop_core::SourceSymbolKind;

use super::*;

fn find<'a>(symbols: &'a [zzop_core::SourceSymbol], name: &str) -> &'a zzop_core::SourceSymbol {
    symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("expected a symbol named {name}, got: {symbols:?}"))
}

// --- BODY SPANS: a class span covers its header through its closing brace; a method span covers
// only its own `{...}`. These are the coordinates every span-consuming rule reads. ---

#[test]
fn class_and_method_body_spans_cover_header_through_closing_brace() {
    let src = "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n";
    let symbols = parse_symbols("C.java", src);
    let class = find(&symbols, "C");
    assert_eq!(class.kind, SourceSymbolKind::Class);
    assert_eq!(class.line, 1);
    // Class: declaration line 1 .. closing `}` on line 6 (module doc's `body_start`/`body_end`).
    assert_eq!(class.body_start, Some(1));
    assert_eq!(class.body_end, Some(6));
    let run = find(&symbols, "C.run");
    assert_eq!(run.kind, SourceSymbolKind::Function);
    assert_eq!(run.line, 2);
    // Method: the `body` block's own lines only — 2 .. 5, NOT the enclosing class's 1 .. 6.
    assert_eq!(run.body_start, Some(2));
    assert_eq!(run.body_end, Some(5));
    // Names are qualified `Type.method` (module doc's "Qualified naming") — a bare "run" never appears.
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

// --- a record is a TYPE, not a method (kind mapping, module doc) ---

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

// --- THE SPAN CONTRACT: `body_start` is the DECLARATION's line, annotations included ---

/// `zzop_core::SourceSymbol`'s "Body span contract". Both halves of the pin are the halves that were
/// wrong before 2026-08-10, and neither is visible in a one-line-header fixture: the span must begin at
/// the FIRST ANNOTATION (not at `public`, not at the `{`), and a signature wrapped onto extra lines
/// must not push the span past it. `@Transactional`-, `@GetMapping`- and `async`-anchored method-scan
/// concepts are exactly the ones that need this, and under the old block-`{` reading every one of them
/// was a formatting coincidence away from being unwritable.
#[test]
fn method_body_span_starts_at_the_first_annotation_not_at_the_opening_brace() {
    let src = concat!(
        "class C {\n",
        "  @Override\n",
        "  @Deprecated\n",
        "  public void run(\n",
        "      int a\n",
        "  ) {\n",
        "    go(a);\n",
        "  }\n",
        "}\n",
    );
    let symbols = parse_symbols("C.java", src);
    let run = find(&symbols, "C.run");
    assert_eq!(run.line, 2);
    assert_eq!(run.body_start, Some(2));
    assert_eq!(run.body_end, Some(8));
    // Containment still holds, which is what keeps `drop_outer_spans` innermost-wins correct.
    let class = find(&symbols, "C");
    assert!(class.body_start.unwrap() <= run.body_start.unwrap());
    assert!(class.body_end.unwrap() >= run.body_end.unwrap());
}

// --- LEAF COMPLETENESS: initializer blocks are scannable regions and must project their own leaf ---

/// The canonical XXE-hardening shape lives in a `static { … }` block. Until 2026-08-10 that block
/// contributed no symbol, so in any class with one ordinary method `drop_outer_spans` discarded the
/// class-wide span in favour of that method's leaf and the whole static block became unreachable to
/// every `.java` method-scan rule — `security/xxe-no-guard` among them. Naming mirrors
/// `zzop_parser_typescript`'s `STATIC_BLOCK`: a `-` cannot appear in a Java identifier, and the 2nd and
/// later blocks take a 1-based ordinal so several blocks cannot collapse onto one name.
#[test]
fn static_initializer_blocks_each_project_their_own_leaf_span() {
    let src = concat!(
        "class C {\n",
        "  static {\n",
        "    FACTORY = DocumentBuilderFactory.newInstance();\n",
        "  }\n",
        "\n",
        "  static { second(); }\n",
        "\n",
        "  void ordinary() {\n",
        "    use();\n",
        "  }\n",
        "}\n",
    );
    let symbols = parse_symbols("C.java", src);
    let first = find(&symbols, "C.static-block");
    assert_eq!(first.kind, SourceSymbolKind::Function);
    assert!(!first.exported);
    assert_eq!(first.body_start, Some(2));
    assert_eq!(first.body_end, Some(4));
    let second = find(&symbols, "C.static-block-2");
    assert_eq!(second.body_start, Some(6));
    assert_eq!(second.body_end, Some(6));
}

/// An instance initializer (`{ … }` directly in a class body) is the same hole with the same cause —
/// real statements in a region no member declaration covers.
#[test]
fn instance_initializer_blocks_each_project_their_own_leaf_span() {
    let src = "class C {\n  {\n    init();\n  }\n\n  void ordinary() {}\n}\n";
    let symbols = parse_symbols("C.java", src);
    let block = find(&symbols, "C.instance-block");
    assert_eq!(block.kind, SourceSymbolKind::Function);
    assert_eq!(block.body_start, Some(2));
    assert_eq!(block.body_end, Some(4));
}

// --- annotation-type element declarations and record components are out of v1 scope ---

#[test]
fn annotation_type_elements_are_not_extracted_as_methods() {
    let src = "@interface Ann {\n  String value() default \"\";\n}\n";
    let symbols = parse_symbols("At.java", src);
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "Ann");
}

// --- INTERFACE / @interface: a SIGNATURE list is not a scannable region ---

/// `crates/core/src/ir.rs`'s span contract makes `None` the positive claim "this declaration encloses
/// nothing scannable", naming a TS `interface` and a Rust `trait` as exactly that. Java projected a span
/// anyway, so the same IR `kind` meant two different things depending on the producer — measured on the
/// reference corpus 2026-08-11: java/interface/span 24 and cs/interface/span 4 against ts 79 / tsx 17 /
/// rs 1 all correctly `None`.
///
/// The `default` body is the half that must NOT regress: dropping the container span is only safe
/// because such members keep their own leaves.
#[test]
fn an_interface_carries_no_body_span_while_its_default_method_keeps_one() {
    let src = "public interface N {\n  String getCursor();\n  default String both() {\n    return getCursor();\n  }\n}\n";
    let symbols = parse_symbols("N.java", src);

    let iface = find(&symbols, "N");
    assert_eq!(iface.kind, SourceSymbolKind::Interface);
    assert_eq!(
        (iface.body_start, iface.body_end),
        (None, None),
        "an interface body is a signature list, not a region: {iface:?}"
    );

    let abstract_method = find(&symbols, "N.getCursor");
    assert_eq!(
        (abstract_method.body_start, abstract_method.body_end),
        (None, None)
    );

    let default_method = find(&symbols, "N.both");
    assert!(
        default_method.body_start.is_some(),
        "a default method has a real body and must keep its leaf, or the interface's regions become \
         unreachable when the container span goes: {default_method:?}"
    );
}

/// An `@interface` body is the same shape and takes the same answer.
#[test]
fn an_annotation_type_carries_no_body_span() {
    let src = "public @interface Marker {\n  String value();\n}\n";
    let symbols = parse_symbols("Marker.java", src);
    let ann = find(&symbols, "Marker");
    assert_eq!((ann.body_start, ann.body_end), (None, None), "{ann:?}");
}

/// The CONTROL for both cases above, and the reason this is not "every type-ish declaration is None":
/// a Java `enum` body holds real method bodies, so it keeps its span. A Rust `enum` is `None` because it
/// has nothing to cover, not because the word matches.
#[test]
fn an_enum_keeps_its_body_span_because_a_java_enum_body_holds_real_bodies() {
    let src = "enum Color {\n  RED, GREEN;\n  String pretty() { return name(); }\n}\n";
    let symbols = parse_symbols("Color.java", src);
    let e = find(&symbols, "Color");
    assert_eq!(e.kind, SourceSymbolKind::Class);
    assert!(
        e.body_start.is_some(),
        "a java enum body is scannable: {e:?}"
    );
}
