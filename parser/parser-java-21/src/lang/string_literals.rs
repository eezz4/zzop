//! Per-file BOUND-STRING-LITERAL projection for Java ([`zzop_core::BoundStringLiteral`]) — the
//! substrate `zzop_core::dsl::Matcher::LiteralScan` reads. The channel's contract (hash + entropy,
//! NEVER the value; never-guess on the name) is `zzop_core::string_literals`'s to state; this doc owns
//! which Java shapes emit and which are deliberately silent.
//!
//! ## What is recognized
//! One arm covers every declared binding: a `variable_declarator` whose `name` is an identifier and
//! whose `value` is a `string_literal` — which is what a field declaration, a constant declaration
//! (interface `String X = "…"`), and a local variable declaration all wrap. Anchored on the NAME's
//! line.
//!
//! ## The value is the source spelling between the quotes — WHEN spelling == value, else silence
//! tree-sitter carries no cooked value, and decoding Java escapes here would be a re-implementation
//! this crate has avoided everywhere else. An escape-FREE literal's spelling IS its value, so those
//! emit verbatim; the swc/ruff/syn producers hash the cooked value because their ASTs hand it over.
//!
//! ## Deliberate silences
//! - **Escaped literals** (any `\` between the quotes) — spelling ≠ value, so no value entropy/hash
//!   can honestly be produced; a decoder was weighed and rejected for v1 (cost vs the rarity of
//!   escaped credentials). See [`inner_text`] for the measured false-fire this gate closes.
//! - **Text blocks** (`""" … """`) — a different literal shape with its own indentation-stripping
//!   semantics; hashing its raw span would hash the indentation. Skipped by the `"""` guard.
//! - **Concatenations** (`"a" + "b"`), **assignments** (`this.key = "v"`, `key = "v"` outside a
//!   declarator) — the same declaration-only line every sibling producer draws.
//! - **Annotation arguments** (`@Value("secret")`) — no binding name.

use tree_sitter::TreeCursor;

use zzop_core::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

use crate::util::{line_of, node_text};

/// Extract this file's bound string literals — see module doc. Empty on parse failure (never panics);
/// a partial in-file error skips just that subtree, the same "extract from the valid regions only"
/// discipline every walk in this crate follows. `_rel` is unused (tree-sitter parsing needs no
/// filename) — kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_string_literals(_rel: &str, text: &str) -> Vec<BoundStringLiteral> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, text, &mut out);
    out
}

/// Preorder walk, so entries come out in source order — same shape as `lang::loop_spans::walk`.
fn walk(cursor: &mut TreeCursor, src: &str, out: &mut Vec<BoundStringLiteral>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            if node.kind() == "variable_declarator" {
                record(node, src, out);
            }
            if cursor.goto_first_child() {
                walk(cursor, src, out);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn record(node: tree_sitter::Node, src: &str, out: &mut Vec<BoundStringLiteral>) {
    let Some(name) = node.child_by_field_name("name") else {
        return;
    };
    if name.kind() != "identifier" {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };
    if value.kind() != "string_literal" {
        return;
    }
    let Some(inner) = inner_text(node_text(value, src)) else {
        return;
    };
    out.push(BoundStringLiteral {
        name: node_text(name, src).to_string(),
        line: line_of(name),
        value_hash: value_hash_hex(inner),
        entropy: shannon_entropy_bits(inner),
    });
}

/// The source spelling between one pair of `"` delimiters, or `None` for a text block (`"""` opener —
/// module doc), a spelling that is not a single-quoted-pair literal, or an ESCAPE-carrying literal
/// (any `\` inside): with an escape present the spelling is not the value, so hashing it would break
/// the value-hash/value-entropy promise `zzop_core::BoundStringLiteral` makes — the 80-bit threshold
/// was calibrated on VALUES, and a spelling can clear it while the value does not (measured: an
/// escaped spelling of a 75.7-bit value read 120.9 bits). Never-guess ⇒ silence; a decoder was
/// weighed and rejected for v1 (cost vs the rarity of escaped credentials — module doc's list).
fn inner_text(raw: &str) -> Option<&str> {
    if raw.starts_with("\"\"\"") {
        return None;
    }
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    if inner.contains('\\') {
        return None;
    }
    Some(inner)
}

#[cfg(test)]
mod tests;
