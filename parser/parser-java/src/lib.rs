//! zzop-parser-java — Java parser — lexical method/class span projector (fusion contract: "spans can come
//! from ANY parser, even a lexical one" — see `docs/ARCHITECTURE.md`'s "Fused execution" section). No real
//! Java grammar here, just a comment/string-aware brace matcher that recovers class/method *body spans*
//! from raw text: good enough for `Matcher::MethodScan` (co-occurrence within a span), not a substitute for
//! a real parser (no type resolution, no overload disambiguation, no generics-aware anything).
//!
//! ## Algorithm
//! One left-to-right scan, comment/string/char-literal aware (a `{`/`}`/`;`/`(`/`)` inside one never
//! affects structure). Two pieces of state: `paren_depth` decides whether a `;` is a real top-level
//! statement terminator or one of a `for (init; cond; update)` header's internal semicolons (must NOT split
//! the header there); `stack` holds one frame per open `{` — every `{` classifies the header text
//! accumulated since the last top-level terminator and pushes a frame, every `}` pops one and, if its
//! header classified as a class/interface/enum/record or a method, emits a `SourceSymbol`.
//!
//! ## Header classification (`classify_header`)
//! - **Class-shaped**: the header contains `class`/`interface`/`enum`/`record NAME` anywhere — checked
//!   FIRST, so `record Point(int x, int y) {`'s parameter-list-shaped header is never mistaken for a method.
//! - **Method-shaped**: the header ends with `IDENT(args)` (optionally followed by `throws ...`), via a
//!   dot-all end-anchored regex with a lazy prefix. A captured name that is itself a control-flow keyword
//!   (`if`/`for`/`while`/`switch`/`catch`/`synchronized` — the only ones shaped like `KEYWORD (...)`) is
//!   rejected, without needing a separate anchored keyword pre-filter (which a label like
//!   `outer: for (...) {` would fool).
//! - Anything else (plain block, `try`/`else`/`finally`/`do`, a static/instance initializer, an
//!   array-literal `{...}`, a lambda body ending in `->` not `)`) classifies as neither — the frame is
//!   still pushed/popped for correct brace nesting, just emits no symbol.
//!
//! ## Line conventions (mirrors `zzop_parser_typescript::fn_symbol`/`class_symbol`)
//! A method/constructor's `line` is its declaration-start line (including any leading annotations) and
//! `body_start` is where its own opening `{` sits (may differ for a multi-line signature). A
//! class/interface/enum/record's `line` AND `body_start` are both the declaration-start line, not the
//! (possibly later) line its opening `{` sits on.
//!
//! ## Known lexical-approximation limits (documented, not fixed — v1 scope)
//! - Nested parens inside an annotation's own argument list that sits INSIDE a method's parameter list
//!   (`foo(@Size(min=1) String x)`) defeats the args-has-no-parens assumption; that header will not
//!   classify as a method (silently skipped, not mis-attributed).
//! - `new Foo(...) { ... }` (anonymous class body) classifies as a `Function` named `Foo`, not a `Class` —
//!   harmless for `Matcher::MethodScan`, just not the semantically precise kind.
//! - Java text blocks (`"""..."""`, Java 15+) are treated as a sequence of ordinary string literals — enough
//!   to keep a `{`/`}` inside one from being read as structural, but the triple-quote sequence itself is
//!   not specially recognized.

use zzop_core::{SourceSymbol, SourceSymbolKind};

pub mod project;
pub mod provides;

pub use project::{extract_http_provides_project, ProjectProvidesReport};
pub use provides::extract_http_provides;

/// Cache key ingredient for `zzop-cache`, mirroring `zzop_parser_prisma::PARSER_FINGERPRINT`'s scheme — bump
/// the trailing `/vN` counter whenever `parse_method_spans`'s projection logic changes its `SourceSymbol`
/// output for the same source text, or the per-file projection grows a new fact type (v2: added
/// `provides::extract_http_provides`), so a pre-existing cache entry from before that fact type existed is
/// never served stale for an unchanged `.java` file.
pub const PARSER_FINGERPRINT: &str = "java-lexical/v2";

/// Control-flow keywords that are syntactically shaped like a call (`KEYWORD (...)`) and so can fool the
/// method-name regex below into capturing the keyword itself as a "method name" — see module doc.
const CONTROL_KEYWORDS: &[&str] = &["if", "for", "while", "switch", "catch", "synchronized"];

/// One open, not-yet-closed `{` frame.
struct Frame {
    /// Declaration-start line (the header's own first non-blank line — see module doc).
    decl_line: u32,
    /// Line this frame's own opening `{` sits on.
    open_line: u32,
    /// `None` for a plain block (control-flow body, initializer, array literal, lambda, ...).
    kind: Option<(SourceSymbolKind, String)>,
}

/// Lexically recovers class/interface/enum/record and method/constructor spans from raw Java source — see
/// module doc for the full algorithm. Never panics on malformed input (an unterminated string/comment/brace
/// just stops contributing further frames; whatever was already closed by that point is still returned).
pub fn parse_method_spans(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let mut symbols = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();

    let mut line: u32 = 1;
    let mut paren_depth: i32 = 0;
    let mut header = String::new();
    let mut reset_line: u32 = 1;
    let mut stack: Vec<Frame> = Vec::new();

    let mut i = 0;
    while i < n {
        let c = chars[i];
        match c {
            '\n' => {
                line += 1;
                header.push(c);
                i += 1;
            }
            '/' if peek(&chars, i + 1) == Some('/') => {
                i += 2;
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
                header.push(' '); // comment content never contributes to header classification
            }
            '/' if peek(&chars, i + 1) == Some('*') => {
                i += 2;
                while i < n && !(chars[i] == '*' && peek(&chars, i + 1) == Some('/')) {
                    if chars[i] == '\n' {
                        line += 1;
                    }
                    i += 1;
                }
                i = (i + 2).min(n);
                header.push(' ');
            }
            '"' | '\'' => {
                let quote = c;
                header.push(c);
                i += 1;
                while i < n && chars[i] != quote && chars[i] != '\n' {
                    if chars[i] == '\\' && i + 1 < n {
                        header.push(chars[i]);
                        header.push(chars[i + 1]);
                        i += 2;
                        continue;
                    }
                    header.push(chars[i]);
                    i += 1;
                }
                if i < n && chars[i] == quote {
                    header.push(chars[i]);
                    i += 1;
                }
                // An unterminated literal (i hit '\n' or EOF first) just stops here — defensive, not a panic.
            }
            '(' => {
                paren_depth += 1;
                header.push(c);
                i += 1;
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                header.push(c);
                i += 1;
            }
            '{' => {
                let decl_line = reset_line + leading_newlines(&header);
                stack.push(Frame {
                    decl_line,
                    open_line: line,
                    kind: classify_header(header.trim()),
                });
                header.clear();
                reset_line = line;
                i += 1;
            }
            '}' => {
                if let Some(frame) = stack.pop() {
                    if let Some((kind, name)) = frame.kind {
                        let body_start = match kind {
                            SourceSymbolKind::Class => frame.decl_line,
                            _ => frame.open_line,
                        };
                        symbols.push(SourceSymbol {
                            id: format!("{rel}#{name}@{}", frame.open_line),
                            file: rel.to_string(),
                            name,
                            kind,
                            line: frame.decl_line,
                            exported: true,
                            is_default: false,
                            body_start: Some(body_start),
                            body_end: Some(line),
                            write_sites: Vec::new(),
                        });
                    }
                }
                header.clear();
                reset_line = line;
                i += 1;
            }
            ';' if paren_depth == 0 => {
                header.clear();
                reset_line = line;
                i += 1;
            }
            _ => {
                header.push(c);
                i += 1;
            }
        }
    }
    symbols
}

fn peek(chars: &[char], i: usize) -> Option<char> {
    chars.get(i).copied()
}

/// Number of newlines in `header`'s leading whitespace — added to the line the header started accumulating
/// on (`reset_line`) to get the line its actual (non-blank) content begins on.
fn leading_newlines(header: &str) -> u32 {
    let trimmed = header.trim_start();
    header[..header.len() - trimmed.len()].matches('\n').count() as u32
}

/// Classifies one brace's header text (already trimmed) — see module doc for the two shapes recognized.
fn classify_header(header: &str) -> Option<(SourceSymbolKind, String)> {
    if header.is_empty() {
        return None;
    }
    if let Some(c) = class_re().captures(header) {
        return Some((SourceSymbolKind::Class, c[1].to_string()));
    }
    if let Some(c) = method_re().captures(header) {
        let name = c[1].to_string();
        if CONTROL_KEYWORDS.contains(&name.as_str()) {
            return None;
        }
        return Some((SourceSymbolKind::Function, name));
    }
    None
}

fn class_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"\b(?:class|interface|enum|record)\s+([A-Za-z_$][\w$]*)").unwrap()
    })
}

/// End-anchored, dot-all: `IDENT(args)` optionally followed by a `throws ...` clause, with an arbitrary
/// (lazy) prefix — see module doc for why this correctly finds the LAST such shape in the header (the
/// argument list that actually reaches the end) even when annotations/earlier calls also contain parens.
fn method_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?s)^.*?\b([A-Za-z_$][\w$]*)\s*\([^()]*\)\s*(?:throws\s+[^{}]*)?$")
            .unwrap()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_and_kinds(symbols: &[SourceSymbol]) -> Vec<(&str, SourceSymbolKind)> {
        symbols.iter().map(|s| (s.name.as_str(), s.kind)).collect()
    }

    fn find<'a>(symbols: &'a [SourceSymbol], name: &str) -> &'a SourceSymbol {
        symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("expected a symbol named {name}, got: {symbols:?}"))
    }

    // --- the scanJavaCmdInjection.test.ts DVJA PingAction shape (Fix 1's engine e2e fixture) ---

    #[test]
    fn class_and_method_spans_match_the_dvja_pingaction_shape() {
        let src = "public class C {\n  private void run() {\n    String[] cmd = { \"/bin/bash\", \"-c\", \"ping \" + getAddress() };\n    Runtime.getRuntime().exec(cmd);\n  }\n}\n";
        let symbols = parse_method_spans("C.java", src);
        assert_eq!(
            names_and_kinds(&symbols),
            vec![
                ("run", SourceSymbolKind::Function),
                ("C", SourceSymbolKind::Class)
            ]
        );
        let run = find(&symbols, "run");
        assert_eq!(run.line, 2);
        assert_eq!(run.body_start, Some(2));
        assert_eq!(run.body_end, Some(5));
        let class = find(&symbols, "C");
        assert_eq!(class.line, 1);
        assert_eq!(class.body_start, Some(1));
        assert_eq!(class.body_end, Some(6));
    }

    // --- nested braces ---

    #[test]
    fn nested_control_flow_braces_do_not_break_the_enclosing_method_span() {
        let src = "class C {\n  void run() {\n    if (true) {\n      for (int i = 0; i < 3; i++) {\n        while (i > 0) { i--; }\n      }\n    }\n  }\n}\n";
        let symbols = parse_method_spans("N.java", src);
        let run = find(&symbols, "run");
        assert_eq!(run.body_start, Some(2));
        assert_eq!(run.body_end, Some(8));
        // Only the class and the method are real symbols — every control-flow block classifies as `None`.
        assert_eq!(
            names_and_kinds(&symbols),
            vec![
                ("run", SourceSymbolKind::Function),
                ("C", SourceSymbolKind::Class)
            ]
        );
    }

    #[test]
    fn nested_class_and_method_are_both_reported() {
        let src =
            "class Outer {\n  class Inner {\n    void go() {\n      int x = 1;\n    }\n  }\n}\n";
        let symbols = parse_method_spans("Nested.java", src);
        let inner = find(&symbols, "Inner");
        assert_eq!(inner.kind, SourceSymbolKind::Class);
        let go = find(&symbols, "go");
        assert_eq!(go.kind, SourceSymbolKind::Function);
        assert_eq!(go.body_start, Some(3));
        assert_eq!(go.body_end, Some(5));
        let outer = find(&symbols, "Outer");
        assert_eq!(outer.body_start, Some(1));
        assert_eq!(outer.body_end, Some(7));
    }

    // --- strings containing braces ---

    #[test]
    fn braces_inside_a_string_literal_are_not_structural() {
        let src = "class C {\n  String weird() {\n    return \"{ not a brace } either\";\n  }\n}\n";
        let symbols = parse_method_spans("S.java", src);
        let weird = find(&symbols, "weird");
        assert_eq!(weird.body_start, Some(2));
        assert_eq!(weird.body_end, Some(4));
    }

    #[test]
    fn brace_inside_a_char_literal_is_not_structural() {
        let src = "class C {\n  void f() {\n    if (c == '{') { return; }\n  }\n}\n";
        let symbols = parse_method_spans("Ch.java", src);
        let f = find(&symbols, "f");
        assert_eq!(f.body_start, Some(2));
        assert_eq!(f.body_end, Some(4));
        // The `if`'s own header must not be misread as a method named "if".
        assert!(symbols.iter().all(|s| s.name != "if"));
    }

    // --- comments ---

    #[test]
    fn a_class_like_shape_inside_a_line_comment_is_not_a_real_class() {
        let src = "class C {\n  // old: class Fake { void x() {} }\n  void real() {\n    int y = 1;\n  }\n}\n";
        let symbols = parse_method_spans("Cm.java", src);
        assert!(symbols.iter().all(|s| s.name != "Fake" && s.name != "x"));
        let real = find(&symbols, "real");
        assert_eq!(real.body_start, Some(3));
        assert_eq!(real.body_end, Some(5));
    }

    #[test]
    fn a_block_comment_spanning_lines_does_not_shift_line_numbers() {
        let src = "class C {\n  /* a comment\n     with a fake brace { in it\n     and a fake method foo() { */\n  void real() {\n    int y = 1;\n  }\n}\n";
        let symbols = parse_method_spans("Bc.java", src);
        assert!(symbols.iter().all(|s| s.name != "foo"));
        let real = find(&symbols, "real");
        assert_eq!(real.body_start, Some(5));
        assert_eq!(real.body_end, Some(7));
    }

    // --- interface methods without bodies ---

    #[test]
    fn interface_abstract_methods_produce_no_symbol_but_the_interface_itself_does() {
        let src = "interface Foo {\n  void bar();\n  int baz(int x);\n}\n";
        let symbols = parse_method_spans("I.java", src);
        assert_eq!(
            names_and_kinds(&symbols),
            vec![("Foo", SourceSymbolKind::Class)]
        );
        let foo = find(&symbols, "Foo");
        assert_eq!(foo.body_start, Some(1));
        assert_eq!(foo.body_end, Some(4));
    }

    // --- annotations ---

    #[test]
    fn an_annotated_method_is_still_recognized_by_name() {
        let src =
            "class C {\n  @Override\n  public String toString() {\n    return \"x\";\n  }\n}\n";
        let symbols = parse_method_spans("An.java", src);
        let to_string = find(&symbols, "toString");
        assert_eq!(to_string.kind, SourceSymbolKind::Function);
        assert_eq!(to_string.body_start, Some(3));
        assert_eq!(to_string.body_end, Some(5));
    }

    #[test]
    fn an_annotation_with_a_string_argument_before_a_method_does_not_confuse_the_name() {
        let src = "class C {\n  @SuppressWarnings(\"unchecked\")\n  public void run() {\n    int x = 1;\n  }\n}\n";
        let symbols = parse_method_spans("Ann2.java", src);
        let run = find(&symbols, "run");
        assert_eq!(run.kind, SourceSymbolKind::Function);
        assert_eq!(run.body_start, Some(3));
    }

    // --- negative-shape guards (scanJavaCmdInjection.test.ts's other fixtures) ---

    #[test]
    fn two_sibling_methods_get_independent_non_overlapping_spans() {
        let src = "public class C {\n  void a() { Runtime.getRuntime().exec(\"safe\"); }\n  String b(String x) { return \"msg \" + x; }\n}\n";
        let symbols = parse_method_spans("Sib.java", src);
        let a = find(&symbols, "a");
        let b = find(&symbols, "b");
        assert_eq!(a.body_start, Some(2));
        assert_eq!(a.body_end, Some(2));
        assert_eq!(b.body_start, Some(3));
        assert_eq!(b.body_end, Some(3));
    }

    #[test]
    fn process_builder_constructor_call_is_not_mistaken_for_a_declaration() {
        let src = "class C { void r(String h){ new ProcessBuilder(\"sh\",\"-c\",\"curl \" + h).start(); } }";
        let symbols = parse_method_spans("Pb.java", src);
        assert_eq!(
            names_and_kinds(&symbols),
            vec![
                ("r", SourceSymbolKind::Function),
                ("C", SourceSymbolKind::Class)
            ]
        );
    }

    // --- record (bonus: not required by the task brief, but a natural class_re extension) ---

    #[test]
    fn a_record_declaration_is_classified_as_a_class_not_a_method() {
        let src = "record Point(int x, int y) {\n  int sum() {\n    return x + y;\n  }\n}\n";
        let symbols = parse_method_spans("R.java", src);
        let point = find(&symbols, "Point");
        assert_eq!(point.kind, SourceSymbolKind::Class);
        let sum = find(&symbols, "sum");
        assert_eq!(sum.kind, SourceSymbolKind::Function);
    }

    #[test]
    fn empty_file_yields_no_symbols() {
        assert!(parse_method_spans("E.java", "").is_empty());
    }

    #[test]
    fn unterminated_brace_does_not_panic_and_yields_no_symbol_for_the_open_frame() {
        let src = "class C {\n  void run() {\n    int x = 1;\n";
        let symbols = parse_method_spans("U.java", src);
        assert!(symbols.is_empty());
    }
}
