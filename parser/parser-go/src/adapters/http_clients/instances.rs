//! Instance-receiver tracking for `adapters::http_clients` — a file-wide first pass binding a local name
//! to an `*http.Client`/`http.Client` VALUE, so a later `c.Get(url)`/`.Post(url, ...)`/`.Head(url)`/
//! `.PostForm(url, ...)` reads as egress the same as the package free function `http.Get(url)`. Bound
//! shapes: `c := &http.Client{}` (short var decl), `var c = http.Client{}` (var spec with initializer),
//! `c = &http.Client{}` (plain reassignment), `c := new(http.Client)`, and the zero-value declaration
//! `var c http.Client` (no initializer — the zero value is a usable client). The tree-sitter counterpart
//! of `zzop_parser_python_3::adapters::http_clients`'s instance pass — LOOSELY: the Python side moved to
//! last-write-wins line-ordered bindings with kill/`del` tracking (B14①, 2026-07-22) while this pass
//! stays a flat add-only name set (`.Get`/`.Post` don't collide with a common Go stdlib method, so the
//! rebind-FP pressure that motivated Python's upgrade is absent here); upgrade only if a live FP pulls it.
//!
//! Scope: only the URL-at-call-site convenience methods above. `client.Do(req)` — where the URL rides a
//! `*http.Request` value built elsewhere (`http.NewRequest("GET", url, ...)`) — stays a v1 roadmap item
//! (see the parent module doc), since resolving it means following the request value one indirection back.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

/// Every local name bound to an `http.Client` value across the tree. Empty when the file constructs none.
pub(super) fn client_instance_names(
    root: Node,
    net_http_names: &HashSet<String>,
    src: &str,
) -> HashSet<String> {
    let mut out = HashSet::new();
    collect(root, net_http_names, src, &mut out);
    out
}

fn collect(node: Node, net_http_names: &HashSet<String>, src: &str, out: &mut HashSet<String>) {
    let mut cursor = node.walk();
    match node.kind() {
        // `c := &http.Client{}` — `left`/`right` are each an `expression_list`.
        "short_var_declaration" => {
            let names = node
                .child_by_field_name("left")
                .map(exprs)
                .unwrap_or_default();
            let values = node
                .child_by_field_name("right")
                .map(exprs)
                .unwrap_or_default();
            bind_pairs(&names, &values, net_http_names, src, out);
        }
        // `c = &http.Client{}` — a plain reassignment of an already-declared name (tree-sitter-go uses the
        // same `left`/`right` expression_list fields as `short_var_declaration`). The Python instance
        // collector handles `Stmt::Assign` too; this brings Go to parity. A compound op (`c += …`) never
        // has an `http.Client` constructor on its right, so reusing `bind_pairs` is safe.
        "assignment_statement" => {
            let names = node
                .child_by_field_name("left")
                .map(exprs)
                .unwrap_or_default();
            let values = node
                .child_by_field_name("right")
                .map(exprs)
                .unwrap_or_default();
            bind_pairs(&names, &values, net_http_names, src, out);
        }
        // `var c = &http.Client{}` (with `value`) OR the zero-value `var c http.Client` (no `value`, the
        // TYPE itself is the http.Client signal — the zero value is a usable client). `name` is a repeated
        // identifier field; `value` is an expression_list (or a single expression for a one-name spec).
        "var_spec" => {
            let names: Vec<Node> = node.children_by_field_name("name", &mut cursor).collect();
            match node.child_by_field_name("value") {
                Some(value) => {
                    let values = exprs(value);
                    bind_pairs(&names, &values, net_http_names, src, out);
                }
                None => {
                    // No initializer — bind every name iff the declared type is `http.Client`.
                    if node
                        .child_by_field_name("type")
                        .is_some_and(|ty| is_http_client_type(ty, net_http_names, src))
                    {
                        for name in &names {
                            if name.kind() == "identifier" {
                                out.insert(node_text(*name, src).to_string());
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
    for child in valid_named_children(node) {
        collect(child, net_http_names, src, out);
    }
}

/// The comma-separated expressions of an `expression_list`, or the node itself when it is a single bare
/// expression (a `var_spec`'s one-name `value` is not wrapped in an `expression_list`).
fn exprs(node: Node) -> Vec<Node> {
    if node.kind() == "expression_list" {
        valid_named_children(node)
    } else {
        vec![node]
    }
}

/// Zip the left names with the right values positionally; bind a left identifier when its paired right
/// value constructs an `http.Client`.
fn bind_pairs(
    names: &[Node],
    values: &[Node],
    net_http_names: &HashSet<String>,
    src: &str,
    out: &mut HashSet<String>,
) {
    for (name, value) in names.iter().zip(values.iter()) {
        if name.kind() == "identifier" && constructs_http_client(*value, net_http_names, src) {
            out.insert(node_text(*name, src).to_string());
        }
    }
}

/// True when `node` is `&http.Client{}`, `http.Client{}`, or `new(http.Client)`.
fn constructs_http_client(node: Node, net_http_names: &HashSet<String>, src: &str) -> bool {
    match node.kind() {
        // `&<composite_literal>`
        "unary_expression" => valid_named_children(node)
            .into_iter()
            .next()
            .is_some_and(|inner| constructs_http_client(inner, net_http_names, src)),
        "composite_literal" => node
            .child_by_field_name("type")
            .is_some_and(|t| is_http_client_type(t, net_http_names, src)),
        // `new(http.Client)`
        "call_expression" => {
            let is_new = node
                .child_by_field_name("function")
                .is_some_and(|f| node_text(f, src) == "new");
            is_new
                && node
                    .child_by_field_name("arguments")
                    .and_then(|a| valid_named_children(a).into_iter().next())
                    .is_some_and(|arg| is_http_client_type(arg, net_http_names, src))
        }
        _ => false,
    }
}

/// A `qualified_type` `<http-name>.Client` whose package resolves to the file's `net/http` import.
fn is_http_client_type(ty: Node, net_http_names: &HashSet<String>, src: &str) -> bool {
    if ty.kind() != "qualified_type" {
        return false;
    }
    let package = ty.child_by_field_name("package").map(|n| node_text(n, src));
    let name = ty.child_by_field_name("name").map(|n| node_text(n, src));
    package.is_some_and(|p| net_http_names.contains(p)) && name == Some("Client")
}
