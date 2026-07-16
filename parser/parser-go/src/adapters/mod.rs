//! Framework-vocabulary producers emitting cross-layer IO facts — see crate root doc's "Layout"
//! section. `net_http`/`gin` each independently import-gate and produce `RouterMountFragment`s;
//! `extract_go_router_fragments` (this module) parses ONCE and hands the same tree to both (crate root
//! doc's "parse once per public fn call" discipline — two sibling PRODUCERS sharing one parse of the
//! same public call is not a second parse, unlike two different PUBLIC FNS each parsing independently).
//! `http_clients` is the CONSUME-side counterpart, exposed directly as its own public fn.

pub mod gin;
pub mod http_clients;
pub mod net_http;

use std::collections::HashMap;

use tree_sitter::Node;
use zzop_core::RouterMountEntry;
use zzop_core::RouterMountFragment;

use crate::util::valid_named_children;

/// Combined `net/http` + `gin` router-mount fragments for one file — see `adapters::net_http` and
/// `adapters::gin`'s own module docs for each producer's recognized shapes. Empty on parse failure, and
/// whenever the file imports NEITHER framework (never panics). A file importing both frameworks emits
/// both producers' fragments concatenated (net/http's first) with no cross-producer name reconciliation
/// — document rather than engineer around, the same "rare pattern" tradeoff
/// `zzop_parser_rust::adapters::axum`'s module doc accepts for its own file-global receiver-name model.
pub fn extract_go_router_fragments(rel: &str, text: &str) -> Vec<RouterMountFragment> {
    let _ = rel; // accepted for public-API parity with this crate's other extractors — unused, see
                 // `zzop_parser_python_3::adapters::fastapi::extract_fastapi_router_fragments`'s own doc
                 // for why a `RouterMountFragment` never needs its source path back.
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let imports = crate::lang::imports::parse_imports(text);
    let mut out = net_http::extract(&tree, &imports, text);
    out.extend(gin::extract(&tree, &imports, text));
    out
}

/// The first-appearance-order `(order, entries)` accumulation pattern
/// `zzop_parser_rust::adapters::axum::append` and `zzop_parser_python_3::adapters::fastapi` both use,
/// shared here between `net_http` and `gin`: a name enters `order` the first time it gets a
/// non-empty batch of new entries, and every later batch for the same name just extends its `Vec`.
pub(crate) fn append_entries(
    order: &mut Vec<String>,
    entries: &mut HashMap<String, Vec<RouterMountEntry>>,
    name: String,
    new_entries: Vec<RouterMountEntry>,
) {
    if new_entries.is_empty() {
        return;
    }
    if !entries.contains_key(&name) {
        order.push(name.clone());
    }
    entries.entry(name).or_default().extend(new_entries);
}

/// `node` as a bare identifier's text, `None` for any other expression shape — the Go-side analogue of
/// `zzop_parser_rust::adapters::axum::simple_expr_ident`, shared between `net_http` and `gin` for
/// resolving an optional handler argument.
pub(crate) fn bare_identifier(node: Node, src: &str) -> Option<String> {
    (node.kind() == "identifier").then(|| crate::util::node_text(node, src).to_string())
}

/// The `index`-th (0-based) NAMED, non-error argument of a `call_expression`'s `argument_list` —
/// shared positional-argument accessor between `net_http` and `gin`.
pub(crate) fn nth_arg(call: Node, index: usize) -> Option<Node> {
    let args = call.child_by_field_name("arguments")?;
    valid_named_children(args).into_iter().nth(index)
}

/// The sole target name of a single-target assignment/short-var-decl `expression_list` — `None` for
/// multi-target forms (out of v1 scope, both consumers' module docs). Shared between `net_http` and
/// `gin` (was byte-identical in each — opus review F5).
pub(crate) fn single_target_name(left: Option<Node>, src: &str) -> Option<String> {
    let list = left?;
    if list.kind() != "expression_list" {
        return None;
    }
    let mut children = valid_named_children(list).into_iter();
    let only = children.next()?;
    if children.next().is_some() {
        return None;
    }
    bare_identifier(only, src)
}

/// The sole `call_expression` of a single-value RHS `expression_list` — `None` for multi-value or
/// non-call shapes. Shared between `net_http` and `gin` (was byte-identical in each — opus review F5).
pub(crate) fn single_rhs_call(right: Option<Node<'_>>) -> Option<Node<'_>> {
    let list = right?;
    if list.kind() != "expression_list" {
        return None;
    }
    let mut children = valid_named_children(list).into_iter();
    let only = children.next()?;
    if children.next().is_some() || only.kind() != "call_expression" {
        return None;
    }
    Some(only)
}
