//! Top-level `SourceSymbol` extraction — v1 scope: module-level `def`/`async def`, `class` (plus its own
//! top-level methods, emitted dotted as `Class.method`), and a top-level simple constant assignment
//! (`X = <literal>`, uppercase-by-convention). Nested/local declarations (a function defined inside
//! another function, a class defined inside a function, a method's own nested `def`) are out of scope —
//! only `module.body`'s direct children (and, for a class, that class's own direct `body` children) are
//! walked, mirroring `adapters::fastapi`'s identical "top-level only" v1 scope.
//!
//! `exported` (`all-dunder-v1`, F4): when the module declares a top-level `__all__ = [...]`/`(...)` whose
//! EVERY element is a plain string literal, membership in that set decides `exported` for every top-level
//! symbol — Python's own wildcard-import contract (`extract_all_set`). Any non-literal element anywhere
//! in `__all__` (a computed name, a spread, a call, ...) makes the whole list untrustworthy as a static
//! membership test, so the WHOLE module falls back to the underscore convention instead — never a
//! partial read of a half-static list. When no top-level `__all__` assignment is present at all, every
//! module falls back to the same convention Python tooling (linters, `pydoc`, wildcard-import) already
//! applies: a name NOT starting with `_` is `exported: true`.
//!
//! `Class.method` sub-symbols do NOT run either check against their own (possibly underscore-prefixed)
//! name — a method is never independently public/private in Python's own object model the way a
//! module-level name is, so each method INHERITS its enclosing class's `exported` value verbatim
//! (computed once, from the class's own bare name against the same `__all__`/underscore rule, then
//! reused for every one of that class's methods).

use std::collections::HashSet;

use ruff_python_ast::{Expr, ModModule, Stmt, StmtAssign, StmtClassDef, StmtFunctionDef};
use ruff_text_size::Ranged;
use zzop_core::{SourceSymbol, SourceSymbolKind};

/// Extract this file's top-level symbols — see module doc. Empty on parse failure (never panics).
/// Declaration order preserved.
pub fn parse_symbols(rel: &str, text: &str) -> Vec<SourceSymbol> {
    let Some(module) = crate::parse_module(text) else {
        return Vec::new();
    };
    let idx = crate::LineIndex::new(text);
    let all = extract_all_set(&module);
    let mut out = Vec::new();
    for stmt in &module.body {
        match stmt {
            Stmt::FunctionDef(f) => {
                let exported = is_exported(f.name.as_str(), all.as_ref());
                out.push(function_symbol(rel, f.name.as_str(), f, &idx, exported));
            }
            Stmt::ClassDef(c) => emit_class(rel, c, &idx, all.as_ref(), &mut out),
            Stmt::Assign(a) => {
                if let Some(sym) = const_symbol(rel, a, &idx, all.as_ref()) {
                    out.push(sym);
                }
            }
            _ => {}
        }
    }
    out
}

/// Module-level `__all__ = [...]`/`(...)` — the exact set of exported names, or `None` when there's no
/// top-level `__all__` assignment, or when one exists but contains at least one non-literal element (a
/// computed name, a `*expr` spread, a call, ...), in which case the caller falls back to the underscore
/// convention for the WHOLE module rather than trusting a partial static read (module doc). Only the
/// FIRST top-level `__all__ = ...` assignment is consulted; a multi-target assignment
/// (`__all__ = OTHER = [...]`) is not recognized as an `__all__` declaration at all (same "simple single
/// target only" discipline `const_symbol` applies).
fn extract_all_set(module: &ModModule) -> Option<HashSet<String>> {
    for stmt in &module.body {
        let Stmt::Assign(a) = stmt else { continue };
        if a.targets.len() != 1 {
            continue;
        }
        let Expr::Name(target) = &a.targets[0] else {
            continue;
        };
        if target.id.as_str() != "__all__" {
            continue;
        }
        return string_literal_list(&a.value);
    }
    None
}

/// Reads a `List`/`Tuple` expression whose every element is a plain string literal into a name set.
/// `None` for any other expression shape, OR a `List`/`Tuple` with even one non-string-literal element —
/// the "fall back to underscore rule for the whole module" case `extract_all_set`'s doc describes.
fn string_literal_list(e: &Expr) -> Option<HashSet<String>> {
    let elements: &[Expr] = match e {
        Expr::List(l) => &l.elts,
        Expr::Tuple(t) => &t.elts,
        _ => return None,
    };
    let mut out = HashSet::with_capacity(elements.len());
    for el in elements {
        let Expr::StringLiteral(s) = el else {
            return None;
        };
        out.insert(s.value.to_str().to_string());
    }
    Some(out)
}

/// `exported` for a module-level name: `__all__` membership when `all` is `Some` (a fully-static list was
/// found), else the "underscore-prefixed is private" convention this module's doc describes. Never called
/// for a `Class.method` sub-symbol — those inherit their class's `exported` value directly (module doc).
fn is_exported(name: &str, all: Option<&HashSet<String>>) -> bool {
    match all {
        Some(set) => set.contains(name),
        None => !name.starts_with('_'),
    }
}

fn function_symbol(
    rel: &str,
    name: &str,
    f: &StmtFunctionDef,
    idx: &crate::LineIndex,
    exported: bool,
) -> SourceSymbol {
    let line = idx.line_of(f.range.start());
    let body_start = f.body.first().map(|s| idx.line_of(s.start()));
    let body_end = f.body.last().map(|s| idx.line_of(s.end()));
    SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported,
        name: name.to_string(),
        kind: SourceSymbolKind::Function,
        line,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    }
}

/// `class Foo:` -> a `Class`-kind symbol for `Foo` itself, plus one `Function`-kind symbol per
/// top-level (direct-body) method, named `Foo.method` — the dotted convention `lib.rs`'s own module doc
/// pins ("class/`Class.method`"). A nested class or a method's own nested `def` is not walked further
/// (v1 scope, see this module's doc). Every method's `exported` is the CLASS's own `exported` value
/// (module doc's inheritance rule) — computed once here and reused verbatim, never re-derived from the
/// method's own name.
fn emit_class(
    rel: &str,
    c: &StmtClassDef,
    idx: &crate::LineIndex,
    all: Option<&HashSet<String>>,
    out: &mut Vec<SourceSymbol>,
) {
    let class_name = c.name.to_string();
    let class_line = idx.line_of(c.range.start());
    let body_start = c.body.first().map(|s| idx.line_of(s.start()));
    let body_end = c.body.last().map(|s| idx.line_of(s.end()));
    let class_exported = is_exported(&class_name, all);
    out.push(SourceSymbol {
        id: format!("{rel}#{class_name}"),
        file: rel.to_string(),
        exported: class_exported,
        name: class_name.clone(),
        kind: SourceSymbolKind::Class,
        line: class_line,
        is_default: false,
        body_start,
        body_end,
        write_sites: Vec::new(),
    });
    for stmt in &c.body {
        if let Stmt::FunctionDef(f) = stmt {
            let method_name = format!("{class_name}.{}", f.name.as_str());
            out.push(function_symbol(rel, &method_name, f, idx, class_exported));
        }
    }
}

/// A single bare-`Name` target whose value is a literal expression (string/number/boolean/`None`) — the
/// "simple" in "top-level simple const assignment". Every other assignment shape (tuple/attribute/
/// subscript target, multi-target, or a non-literal value like a call or a collection) is silently
/// skipped, never guessed at.
fn const_symbol(
    rel: &str,
    a: &StmtAssign,
    idx: &crate::LineIndex,
    all: Option<&HashSet<String>>,
) -> Option<SourceSymbol> {
    if a.targets.len() != 1 {
        return None;
    }
    let Expr::Name(target) = &a.targets[0] else {
        return None;
    };
    if !is_literal(&a.value) {
        return None;
    }
    let name = target.id.as_str().to_string();
    let line = idx.line_of(a.range.start());
    Some(SourceSymbol {
        id: format!("{rel}#{name}"),
        file: rel.to_string(),
        exported: is_exported(&name, all),
        name,
        kind: SourceSymbolKind::Const,
        line,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    })
}

fn is_literal(e: &Expr) -> bool {
    matches!(
        e,
        Expr::StringLiteral(_)
            | Expr::NumberLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
    )
}

#[cfg(test)]
mod tests;
