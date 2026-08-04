//! Per-file BOUND-STRING-LITERAL projection for Go ([`zzop_core::BoundStringLiteral`]) — the substrate
//! `zzop_core::dsl::Matcher::LiteralScan` reads. The channel's contract (hash + entropy, NEVER the
//! value; never-guess on the name) is `zzop_core::string_literals`'s to state; this doc owns which Go
//! shapes emit and which are deliberately silent.
//!
//! ## What is recognized
//! - **`const`/`var` specs** — `const apiKey = "…"`, `var token string = "…"`, grouped forms
//!   included. A spec's comma-separated names pair POSITIONALLY with its value list (`a, b = "x",
//!   "y"` emits both) — Go defines that pairing, so it is exact, not a guess; a spec whose name and
//!   value counts differ emits nothing. The name collection filters on `kind() == "identifier"`
//!   because tree-sitter-go 0.25 attaches the `name` FIELD to a `const_spec`'s COMMA tokens as well
//!   (`var_spec` does not): unfiltered, k names arrive as 2k-1 nodes, the count guard trips, and the
//!   whole multi-name const goes silent — the filter restores both the count and the alignment.
//! - **Short variable declarations** — `key := "…"`, same positional pairing on `left`/`right`.
//! - Both Go literal kinds emit: `interpreted_string_literal` ("…") when ESCAPE-FREE — with a `\`
//!   inside, spelling ≠ value and the entry is a deliberate SILENCE (no value entropy/hash can
//!   honestly be produced; decoder weighed and rejected for v1 — the Java twin's doc owns the
//!   measured false-fire) — and `raw_string_literal` (`` `…` ``) unconditionally, where the raw
//!   spelling IS the exact value by language definition, backslashes included.
//!
//! Anchored on the NAME's line.
//!
//! ## Deliberate silences
//! - **Assignments** (`x = "v"`, `s.field = "v"`) — the declaration-only line every sibling draws.
//! - **Struct literal fields** (`Config{Key: "v"}`) — a composite-literal VALUE, not a binding;
//!   only the TS producer includes object properties, by explicit A17 judgment about where JS config
//!   lives, and this doc's silence is the deliberate asymmetry.
//! - **Concatenations, function arguments, map literals** — no single binding name.

use tree_sitter::{Node, TreeCursor};

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
            match node.kind() {
                // `const_spec` and `var_spec` share one shape: `name` field (repeated) + `value`
                // field (one `expression_list`). The declaration-level wrappers (grouped specs,
                // `var_spec_list`) need no arms — the walk reaches every spec regardless of nesting.
                "const_spec" | "var_spec" => {
                    let mut names_cursor = node.walk();
                    // `kind() == "identifier"` is load-bearing, not defensive: a `const_spec`'s
                    // comma tokens carry the `name` field too (module doc's fielded-comma note), so
                    // without it the count guard reads 2k-1 "names" against k values and silences
                    // the entire spec.
                    let names: Vec<Node> = node
                        .children_by_field_name("name", &mut names_cursor)
                        .filter(|n| !n.is_error() && !n.is_missing() && n.kind() == "identifier")
                        .collect();
                    let values = node
                        .child_by_field_name("value")
                        .map(list_items)
                        .unwrap_or_default();
                    record_pairs(&names, &values, src, out);
                }
                "short_var_declaration" => {
                    let names = node
                        .child_by_field_name("left")
                        .map(list_items)
                        .unwrap_or_default();
                    let values = node
                        .child_by_field_name("right")
                        .map(list_items)
                        .unwrap_or_default();
                    record_pairs(&names, &values, src, out);
                }
                _ => {}
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

/// An `expression_list`'s valid named children (its comma-separated items).
fn list_items(list: Node) -> Vec<Node> {
    valid_named_children(list)
}

/// Emits one entry per position where the name is a plain identifier AND the value is a string
/// literal — but only when the two lists pair positionally at all (equal counts; module doc). A
/// non-literal value at one position silences that position only, not its neighbors.
fn record_pairs(names: &[Node], values: &[Node], src: &str, out: &mut Vec<BoundStringLiteral>) {
    if names.is_empty() || names.len() != values.len() {
        return;
    }
    for (name, value) in names.iter().zip(values) {
        if name.kind() != "identifier" {
            continue;
        }
        let Some(inner) = string_literal_text(*value, src) else {
            continue;
        };
        // ESCAPE gate, caller-side and INTERPRETED-only: a `\` in an interpreted literal spells an
        // escape, so the spelling is not the value and no value hash/entropy can honestly be
        // produced (the Java twin's `inner_text` doc owns the measured false-fire). A RAW literal's
        // backslash IS its value — Go defines backtick strings escape-free — so raw passes through.
        if value.kind() == "interpreted_string_literal" && inner.contains('\\') {
            continue;
        }
        out.push(BoundStringLiteral {
            name: node_text(*name, src).to_string(),
            line: line_of(*name),
            value_hash: value_hash_hex(&inner),
            entropy: shannon_entropy_bits(&inner),
        });
    }
}

#[cfg(test)]
mod tests;
