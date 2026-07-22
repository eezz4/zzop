//! Per-file loop-body line spans — every Go `for` statement's whole line span (1-based, inclusive),
//! feeding `MethodScan::trigger_in_loop`. Mirrors `zzop_parser_typescript::loop_spans::
//! extract_loop_spans`'s cross-language contract exactly (see `zzop_core::dsl::SourceFile::loop_spans`'s
//! doc for the shared definition every parser projects onto): a call sitting textually inside one of
//! these spans is proven to run once per iteration, not just "somewhere in the same function".
//!
//! ## Why one node kind covers every Go loop form
//! Go's grammar unifies all four loop shapes the task brief names — classic `for i := 0; i < n; i++`,
//! condition-only `for cond`, infinite `for {}`, and `for ... range ...` — into a single
//! `for_statement` node; the middle piece (`for_clause`, `range_clause`, a bare condition expression,
//! or nothing at all) is just an optional child, never a different STATEMENT kind (see
//! `tree-sitter-go`'s `grammar.js`: `for_statement: $ => seq('for', optional(choice($._expression,
//! $.for_clause, $.range_clause)), field('body', $.block))`). So unlike the TypeScript side (which has
//! to match `ForStmt`/`ForInStmt`/`ForOfStmt`/`WhileStmt`/`DoWhileStmt` as five distinct node types),
//! this module needs no per-form branching at all: every `for_statement` node's OWN whole span (header
//! line included — a call in the loop's condition/post-clause still runs once per iteration too,
//! matching the TS side's identical header-inclusive convention) is exactly one loop span.
//!
//! Go has no array-iteration-callback idiom the way TS's `.map`/`.forEach` do — a `range` over a slice
//! is already the `for_statement` handled above — so unlike `zzop_parser_typescript::loop_spans`, this
//! module emits only the one span source, never a second callback-argument-only kind.

use tree_sitter::TreeCursor;

use crate::util::{end_line_of, line_of};

/// Extract this file's loop-body line spans — see module doc. Empty on parse failure (never panics);
/// a partial in-file error skips just that subtree, the same "extract from the valid regions only"
/// discipline every walk in this crate follows. `_rel` is unused (tree-sitter parsing needs no
/// filename) — kept in the signature to match every other per-file `extract_*`/`parse_*` entry point in
/// this crate (and the engine's uniform `(rel, text)` call convention across every language).
pub fn extract_loop_spans(_rel: &str, text: &str) -> Vec<(u32, u32)> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, &mut out);
    out
}

/// Same error/missing-skipping recursive-descent shape as `lang::used_names::walk` — a preorder walk
/// (a node is recorded, THEN its children are visited), so nested loops emit outer-before-inner, the
/// same source-order convention `zzop_parser_typescript::loop_spans`'s own nested-loop test pins.
fn walk(cursor: &mut TreeCursor, out: &mut Vec<(u32, u32)>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            if node.kind() == "for_statement" {
                out.push((line_of(node), end_line_of(node)));
            }
            if cursor.goto_first_child() {
                walk(cursor, out);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
