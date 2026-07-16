//! swc parse entry — file-extension-driven syntax selection (`tsx`/`decorators` — see
//! `parse_with_cm`'s inline rationale), shared `SourceMap` plumbing, and the parse-success signal.

use swc_core::common::{sync::Lrc, BytePos, FileName, Globals, SourceMap, GLOBALS};
use swc_core::ecma::ast::{EsVersion, Module};
use swc_core::ecma::parser::{parse_file_as_module, Syntax, TsSyntax};

pub(crate) fn line_of(cm: &SourceMap, pos: BytePos) -> u32 {
    cm.lookup_char_pos(pos).line as u32
}

pub(crate) fn parse_module(file: &str, source: &str) -> Option<Module> {
    parse_with_cm(file, source).map(|(_, m)| m)
}

pub(crate) fn parse_with_cm(file: &str, source: &str) -> Option<(Lrc<SourceMap>, Module)> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom(file.to_string())),
        source.to_string(),
    );
    let syntax = Syntax::Typescript(TsSyntax {
        // JSX is enabled for `.tsx` and for every JS-family extension (`.jsx`, and by React/CRA
        // convention plain `.js`/`.mjs`/`.cjs`). None of the JS-family extensions use TypeScript's
        // `<T>x` angle-bracket cast, so treating `<...>` as JSX is unambiguous there (a real comparison
        // `a < b > c` still parses, since JSX is only recognized in expression-start position, not after
        // an operand). The pure-TS extensions (`.ts`/`.mts`/`.cts`) keep `tsx` OFF so a type assertion
        // still parses. Without this, a React component in a `.js`/`.jsx` file fails to parse entirely
        // and the caller degrades the whole file to a lexical fallback (blinding structural analysis).
        tsx: file.ends_with(".tsx")
            || file.ends_with(".jsx")
            || file.ends_with(".js")
            || file.ends_with(".mjs")
            || file.ends_with(".cjs"),
        // Legacy/stage-2 decorator syntax (`@Component({...}) class Foo { @Input() x; }`) is
        // ubiquitous in real-world TS (Angular, NestJS, TypeORM, etc.), but swc_ecma_parser's
        // `TsSyntax::decorators` defaults to `false`. Without this, a decorated class fails to
        // parse at all (`parse_file_as_module` returns `Err`) and the caller degrades the whole file.
        decorators: true,
        ..Default::default()
    });
    let module = GLOBALS.set(&Globals::new(), || {
        let mut errors = Vec::new();
        parse_file_as_module(&fm, syntax, EsVersion::EsNext, None, &mut errors).ok()
    })?;
    Some((cm, module))
}

/// Distinguishes "parse failed" from "legitimately empty file" — a signal `parse_symbols`/`parse_imports`
/// can't give on their own, since both cases produce an empty `Vec`/`ImportMap`. Unlike TypeScript's own
/// error-tolerant parser, swc's `parse_file_as_module` returns `Err` (no `Module`) for malformed input
/// (unbalanced braces, a stray closing brace, plain syntax errors) while still parsing an empty file, a
/// comment-only file, or a merely *semantic* oddity like duplicate function declarations — see
/// `probe_parse_ok_signal` for the covered cases. Any produced `Module`, clean or not, counts as success.
/// `true` — a `Module` was produced, `parse_symbols`/`parse_imports`/etc. are meaningful for this text.
/// `false` — swc could not build one at all; the caller should treat this file as broken (degrade).
pub fn parse_ok(rel: &str, text: &str) -> bool {
    parse_with_cm(rel, text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::binding;
    use crate::{parse_imports, parse_symbols};
    use zzop_core::SourceSymbolKind as K;

    // --- parse_ok — see the fn doc for the empirical basis of every case below ---

    #[test]
    fn probe_parse_ok_signal() {
        assert!(!parse_ok("b.ts", "function f( {\n  x\n")); // unbalanced brace/paren
        assert!(!parse_ok("s.ts", "}\nfunction foo() {}\n")); // stray closing brace
        assert!(!parse_ok("t.ts", "const x: = 1;\n")); // plain syntax error, braces balanced
        assert!(parse_ok(
            "ok.ts",
            "export function foo(a: { x: number }) {\n  return [a.x, (a.x + 1)];\n}\n"
        ));
        assert!(parse_ok("empty.ts", "")); // legitimately empty file — not broken
        assert!(parse_ok("comment.ts", "// just a comment\n"));
        assert!(parse_ok("dupe.ts", "function f() {}\nfunction f() {}\n")); // semantic, not syntax
    }

    /// Regression: an Angular-style decorated class (class decorator with an object-literal arg, plus
    /// property/method/parameter decorators) used to fail `parse_file_as_module` entirely and degrade the
    /// whole file, because `TsSyntax::decorators` defaults to `false` in swc_ecma_parser.
    #[test]
    fn angular_style_decorators_parse_ok_and_yield_symbols() {
        let src = r#"
import { Component, Input, Output, EventEmitter, HostListener } from '@angular/core';

@Component({
  selector: 'pivot-table',
  template: '<div></div>',
})
export class PivotTableComponent {
  @Input() data: unknown[] = [];
  @Output() changed = new EventEmitter<void>();

  constructor(@Inject(TOKEN) private el: unknown) {}

  @HostListener('window:resize')
  onResize() {
    return this.data.length;
  }
}
"#;
        assert!(parse_ok("pivot-table.component.ts", src));

        let imports = parse_imports("pivot-table.component.ts", src);
        assert_eq!(
            imports["Component"],
            binding("@angular/core", "Component", false)
        );
        assert_eq!(imports["Input"], binding("@angular/core", "Input", false));

        let symbols = parse_symbols("pivot-table.component.ts", src);
        let class = symbols
            .iter()
            .find(|s| s.name == "PivotTableComponent")
            .expect("class symbol survives decorator parsing");
        assert_eq!(class.kind, K::Class);
        assert!(class.exported);
        assert!(symbols
            .iter()
            .any(|s| s.name == "PivotTableComponent.constructor"));
        assert!(symbols
            .iter()
            .any(|s| s.name == "PivotTableComponent.onResize"));
    }

    /// Regression: a React component containing JSX in a `.js` or `.jsx` file used to fail
    /// `parse_file_as_module` entirely and degrade the whole file, because `tsx` was enabled only for
    /// `.tsx`. Every JS-family extension now parses JSX (`.js`/`.jsx`/`.mjs`/`.cjs`).
    #[test]
    fn jsx_in_js_family_files_parse_ok_and_yield_symbols_and_imports() {
        let src = r#"
import React from 'react';
import { connect } from 'react-redux';

export function Header({ title }) {
  return (
    <nav className="navbar">
      <span>{title}</span>
      {title ? <a href="/new">New</a> : null}
    </nav>
  );
}

export default connect(null, null)(Header);
"#;
        for rel in [
            "src/Header.js",
            "src/Header.jsx",
            "src/Header.mjs",
            "src/Header.cjs",
        ] {
            assert!(parse_ok(rel, src), "{rel} should parse JSX, not degrade");
            let imports = parse_imports(rel, src);
            assert_eq!(
                imports["React"],
                binding("react", "default", false),
                "{rel}"
            );
            let symbols = parse_symbols(rel, src);
            assert!(
                symbols.iter().any(|s| s.name == "Header" && s.exported),
                "{rel}: Header symbol survives JSX parsing"
            );
        }
    }

    /// A pure-TS extension keeps `tsx` off, so an angle-bracket type assertion still parses (not treated
    /// as a JSX open tag) — the fix must not regress `.ts`/`.mts`/`.cts`.
    #[test]
    fn ts_extensions_keep_angle_bracket_type_assertions_parseable() {
        let src = "const x = <string>(y as unknown);\nexport const z = x;\n";
        for rel in ["a.ts", "a.mts", "a.cts"] {
            assert!(
                parse_ok(rel, src),
                "{rel}: `<string>` stays a type assertion"
            );
        }
    }
}
