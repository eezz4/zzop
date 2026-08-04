//! Per-file BOUND-STRING-LITERAL projection ([`zzop_core::BoundStringLiteral`]) for the
//! TypeScript/JavaScript frontend. Sibling of [`crate::extract_call_sites`]: one parse, one recursive
//! walk, source-order emission, an AST gate rather than a text regex — and, specific to this channel,
//! the VALUE is reduced to hash + entropy at this boundary and never leaves it (see
//! `zzop_core::string_literals`'s module doc for the no-plaintext contract).
//!
//! # What is emitted, and with which name
//! - **Variable declarations** — `const/let/var NAME = "value"`. `name` is the identifier as written.
//! - **Object properties** — `{ key: "value" }` and `{ "key": "value" }` (TS includes properties by
//!   the A17 judgment: real secrets live in config objects; corpus ceiling measured negligible).
//!   `name` is the property key as written (string keys keep their inner text, not their quotes).
//! - **Class fields** — `class C { key = "value" }`, static or not. `name` is the key identifier.
//!
//! The line is the BINDING's own line (the declarator/property key), not the literal's — a value
//! wrapped onto the next line still anchors where the name a reader greps for is.
//!
//! # Deliberate silences — each a choice, not an oversight
//! - **Anything with no single resolvable name** (never-guess): destructuring patterns, array
//!   elements, positional arguments, computed keys (`[k]: "v"`), shorthand and spread properties.
//! - **Template literals**, even substitution-free ones, and **string concatenations** — v1 carries
//!   the language's plain string-literal node only, the same scope line every sibling producer draws,
//!   so a rule reading this channel sees the same population shape per language.
//! - **Assignment expressions** (`obj.key = "v"`) — not a declaration; the A17 shape lists
//!   declarations, field inits and TS properties. Adding assignments later is additive.
//!
//! `value` for hashing/entropy is the COOKED value (escapes decoded — `"a\x2db"` hashes as `a-b`),
//! which is the string the program actually compares/ships.

use swc_core::common::{SourceMap, Span};
use swc_core::ecma::ast::{ClassProp, Expr, KeyValueProp, Prop, PropName, VarDeclarator};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

use crate::{line_of, parse_with_cm};

/// Projects this file's bound string literals in SOURCE ORDER — the same determinism contract
/// `extract_call_sites` documents. Sorted by source offset rather than trusted from the walk, the
/// ruff producer's precedent: swc's visitor walks a node's FIELDS in AST-struct order, which is not
/// source order everywhere — a `ClassProp`'s `decorators` field sits after its key/value, so
/// `@ApiProperty({ example: '…' }) password = '…'` would otherwise emit line 3 before line 2
/// (measured; the decorator pin in this module's tests). Stable sort, so same-offset entries keep
/// walk order. A file swc cannot parse yields no entries at all; the engine additionally gates on
/// `!degraded`, and the double guard costs nothing.
pub fn extract_string_literals(file: &str, source: &str) -> Vec<BoundStringLiteral> {
    let Some((cm, module)) = parse_with_cm(file, source) else {
        return Vec::new();
    };
    let mut collector = LiteralCollector {
        cm: &cm,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out.sort_by_key(|(lo, _)| *lo);
    collector.out.into_iter().map(|(_, lit)| lit).collect()
}

struct LiteralCollector<'a> {
    cm: &'a SourceMap,
    /// `(name span lo, entry)` — the offset exists only to restore source order (fn doc).
    out: Vec<(u32, BoundStringLiteral)>,
}

impl LiteralCollector<'_> {
    fn push(&mut self, name: &str, name_span: Span, value: &str) {
        self.out.push((
            name_span.lo.0,
            BoundStringLiteral {
                name: name.to_string(),
                line: line_of(self.cm, name_span.lo),
                value_hash: value_hash_hex(value),
                entropy: shannon_entropy_bits(value),
            },
        ));
    }
}

/// The initializer's string value, iff the expression IS a plain string literal (module doc's scope
/// line: no templates, no concatenations, no parenthesized indirection — parens change nothing
/// semantically but v1 keeps the same "one literal node" rule every sibling language can mirror).
fn plain_str(expr: &Expr) -> Option<String> {
    match expr {
        // `as_str()` is `None` for a WTF-8 value that is not valid UTF-8 (lone surrogates). That is a
        // SILENCE, not a default: hashing an empty stand-in would claim an equality the source never
        // stated — the same never-guess line the name side draws.
        Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => s.value.as_str().map(str::to_string),
        _ => None,
    }
}

/// A property key's spelling, for the key shapes that carry one (module doc): an identifier as
/// written, or a string key's inner text. Computed/numeric keys resolve to nothing.
fn prop_key_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => s.value.as_str().map(str::to_string),
        PropName::Num(_) | PropName::Computed(_) | PropName::BigInt(_) => None,
    }
}

impl Visit for LiteralCollector<'_> {
    fn visit_var_declarator(&mut self, n: &VarDeclarator) {
        if let (swc_core::ecma::ast::Pat::Ident(ident), Some(init)) = (&n.name, &n.init) {
            if let Some(value) = plain_str(init) {
                self.push(ident.id.sym.as_str(), ident.id.span, &value);
            }
        }
        n.visit_children_with(self); // recurse: an object-literal init's properties emit their own.
    }

    fn visit_key_value_prop(&mut self, n: &KeyValueProp) {
        if let Some(name) = prop_key_name(&n.key) {
            if let Some(value) = plain_str(&n.value) {
                let span = match &n.key {
                    PropName::Ident(i) => i.span,
                    PropName::Str(s) => s.span,
                    _ => unreachable!("prop_key_name returned Some only for Ident/Str"),
                };
                self.push(&name, span, &value);
            }
        }
        n.visit_children_with(self);
    }

    /// Shorthand (`{ apiKey }`) and getter/setter props carry no literal; only the key-value form
    /// above does. This arm exists so a `Prop::KeyValue` nested in any object still recurses.
    fn visit_prop(&mut self, n: &Prop) {
        n.visit_children_with(self);
    }

    fn visit_class_prop(&mut self, n: &ClassProp) {
        if let Some(value) = n.value.as_deref().and_then(plain_str) {
            if let Some(name) = prop_key_name(&n.key) {
                let span = match &n.key {
                    PropName::Ident(i) => i.span,
                    PropName::Str(s) => s.span,
                    _ => unreachable!("prop_key_name returned Some only for Ident/Str"),
                };
                self.push(&name, span, &value);
            }
        }
        n.visit_children_with(self);
    }
}

#[cfg(test)]
mod tests;
