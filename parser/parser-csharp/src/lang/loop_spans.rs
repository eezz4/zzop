//! Per-file loop-body line spans (1-based, inclusive), feeding `MethodScan::trigger_in_loop` —
//! mirrors `zzop_parser_go`/`zzop_parser_java_21::lang::loop_spans`'s cross-language contract
//! exactly (see `zzop_core::dsl::SourceFile::loop_spans`'s doc for the shared definition every
//! parser projects onto): a call sitting textually inside one of these spans is proven to run once
//! per iteration, not just "somewhere in the same function".
//!
//! ## What is a span here — the eager/lazy boundary
//! C#'s four STATEMENT loop forms, each its own grammar kind ([`LOOP_STATEMENT_KINDS`]): classic
//! `for_statement`, `foreach_statement` (covering `await foreach` too — the `await` token is a plain
//! child, not a different kind), `while_statement`, and `do_statement`. Every one is the node's OWN
//! whole span, header line included — a call in the loop's condition/increment clause runs once per
//! iteration too, the same header-inclusive convention TS/Go/Java pin. A `do`-statement's trailing
//! `while (cond);` line is part of its node span, and its condition likewise re-evaluates per
//! iteration.
//!
//! **LINQ lambdas are deliberately NOT spans**: `xs.Select(x => F(x))` is LAZY (deferred execution)
//! — the selector runs zero times unless the query is enumerated, so "proven to run once per
//! iteration" cannot be honored for the lambda's body. This module stays silent there, the same call
//! Java makes for Stream lambdas, Python for generator expressions, and Rust for `.iter().map(...)`
//! closures (and unlike TS's `ARRAY_ITERATION_METHODS` arm, whose `.map`/`.forEach` are EAGER). A
//! lambda inside a recorded statement loop is still covered textually by that loop's span, which is
//! the correct containment either way.

use tree_sitter::TreeCursor;

use crate::util::{end_line_of, line_of};

/// The four statement-loop node kinds — see module doc. All four are pinned in
/// `node_kinds::PINNED_NODE_KINDS` against the compiled grammar.
const LOOP_STATEMENT_KINDS: &[&str] = &[
    "for_statement",
    "foreach_statement",
    "while_statement",
    "do_statement",
];

/// Extract this file's loop-body line spans — see module doc. Empty on parse failure (never panics);
/// a partial in-file error skips just that subtree, the same "extract from the valid regions only"
/// discipline every walk in this crate follows. `_rel` is unused (tree-sitter parsing needs no
/// filename) — kept to match the engine's uniform `(rel, text)` call convention.
pub fn extract_loop_spans(_rel: &str, text: &str) -> Vec<(u32, u32)> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk(&mut cursor, &mut out);
    out
}

/// Same error/missing-skipping recursive-descent shape as `zzop_parser_go::lang::loop_spans::walk` —
/// a preorder walk (a node is recorded, THEN its children are visited), so nested loops emit
/// outer-before-inner, the source-order convention the sibling parsers' nested-loop tests pin.
fn walk(cursor: &mut TreeCursor, out: &mut Vec<(u32, u32)>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            if LOOP_STATEMENT_KINDS.contains(&node.kind()) {
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
