//! Per-file BOUND-STRING-LITERAL projection for C# ([`zzop_core::BoundStringLiteral`]) — the substrate
//! `zzop_core::dsl::Matcher::LiteralScan` reads. The channel's contract (hash + entropy, NEVER the
//! value; never-guess on the name) is `zzop_core::string_literals`'s to state; this doc owns which C#
//! shapes emit and which are deliberately silent.
//!
//! ## What is recognized
//! One arm, same as the Java sibling: a `variable_declarator` whose `name` field is an identifier and
//! whose initializer (first named child that is neither the name nor a `bracketed_argument_list` —
//! `project::collect::declarator_value`'s exact rule, restated because that helper is private to its
//! module tree) is a plain `string_literal`. That covers field declarations (`private string key =
//! "…"`), const/static fields, and local declarations. Anchored on the NAME's line.
//!
//! ## The value is the source spelling between the quotes — WHEN spelling == value, else silence
//! `util::string_literal_text` stays verbatim (its other callers need exactly that); THIS caller adds
//! an escape gate on top, and the Java producer's module doc owns the full argument.
//!
//! ## Deliberate silences
//! - **Escaped literals** (any `\` in an ordinary `string_literal`) — spelling ≠ value, so no value
//!   entropy/hash can honestly be produced; decoder weighed and rejected for v1 (Java twin's doc).
//! - **Interpolated (`$"…"`), verbatim (`@"…"`) and raw (`"""…"""`) strings** — each is a DIFFERENT
//!   grammar node kind, so they fall out of the `string_literal` gate rather than needing a special
//!   case (`util::string_literal_text` documents the same boundary).
//! - **Property initializers** (`public string Key { get; } = "…";`) and **expression-bodied
//!   properties** — not a `variable_declarator`; v1 keeps the one-arm declarator scope every sibling
//!   language mirrors. Additive later if a rule needs it.
//! - **Assignments** (`this.Key = "v"`), **concatenations**, **attribute arguments** — the same
//!   declaration-only line every sibling producer draws.

use tree_sitter::TreeCursor;

use zzop_core::{shannon_entropy_bits, value_hash_hex, BoundStringLiteral};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

/// Extract this file's bound string literals — see module doc. Empty on parse failure (never panics);
/// a partial in-file error skips just that subtree. `_rel` is unused (tree-sitter parsing needs no
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

/// Preorder walk, so entries come out in source order — same shape as `lang::loop_spans`'s.
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
    let Some(init) = valid_named_children(node)
        .into_iter()
        .find(|c| c.id() != name.id() && c.kind() != "bracketed_argument_list")
    else {
        return;
    };
    // `string_literal_text` is the kind gate AND the delimiter strip in one call — `None` for every
    // other literal shape (interpolated/verbatim/raw are different node kinds; module doc).
    let Some(inner) = string_literal_text(init, src) else {
        return;
    };
    // ESCAPE gate, caller-side (the helper stays verbatim for its other callers — its own doc says
    // so): an ordinary `string_literal` with a `\` inside spells escapes, so the spelling is not the
    // value and no value hash/entropy can honestly be produced (the Java twin's `inner_text` doc owns
    // the measured false-fire). Verbatim/raw literals never reach here (different node kinds).
    if inner.contains('\\') {
        return;
    }
    out.push(BoundStringLiteral {
        name: node_text(name, src).to_string(),
        line: line_of(name),
        value_hash: value_hash_hex(&inner),
        entropy: shannon_entropy_bits(&inner),
    });
}

#[cfg(test)]
mod tests;
