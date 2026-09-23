//! Node-shape recognizers for the router-wrapper adapter — pure `Node` -> answer functions with no
//! collector state between them. Same seam `adapters::gin::shapes` draws: nothing here decides
//! anything about routes, it only reports what a node IS.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::util::{node_text, valid_named_children};

/// Node kinds that introduce a Go parameter scope. `func_literal` is in the list because the callback
/// form this adapter recognizes (`X.Group("/p", func(Y T) { ... })`) is one, and because a route
/// registered on a closure's own parameter is exactly the "mount point is elsewhere" case
/// [`super::Collector::resolve`] must refuse.
pub(super) const FUNCTION_KINDS: &[&str] =
    &["function_declaration", "method_declaration", "func_literal"];

/// Every parameter name declared by a function-ish node, including a method's receiver.
///
/// One `parameter_declaration` can declare SEVERAL names (`func f(a, b routing.RouteRegister)`), so
/// this walks every `identifier` child of each declaration rather than reading a single `name` field —
/// reading one name would leave `b` looking like a free identifier, which is the one mistake that turns
/// an unknown mount point into a confidently wrong route key.
pub(super) fn param_names(node: Node, src: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for field in ["parameters", "receiver"] {
        let Some(list) = node.child_by_field_name(field) else {
            continue;
        };
        for decl in valid_named_children(list) {
            if decl.kind() != "parameter_declaration" {
                continue;
            }
            for child in valid_named_children(decl) {
                if child.kind() == "identifier" {
                    out.insert(node_text(child, src).to_string());
                }
            }
        }
    }
    out
}

/// A `func_literal` argument declaring EXACTLY ONE parameter -> `(the func_literal, its parameter name)`.
///
/// Exactly one on purpose: the callback group idiom hands the sub-router and nothing else, and a
/// two-parameter callback is a shape this adapter has not been shown and therefore does not claim to
/// read. Refusing it costs the routes inside that callback, which then fall to the same never-guess
/// refusal as any other unresolvable receiver.
pub(super) fn single_param_closure<'t>(node: Node<'t>, src: &str) -> Option<(Node<'t>, String)> {
    if node.kind() != "func_literal" {
        return None;
    }
    let names = param_names(node, src);
    (names.len() == 1).then(|| (node, names.into_iter().next().unwrap_or_default()))
}

/// Joins a resolved router prefix with a route's own literal path.
///
/// `"/api"` + `"/"` is `"/api"`, not `"/api/"`: a group's root route is the group's own path, and the
/// trailing slash would key it as a different route from the one the server answers. `""` + `"/x"` is
/// `"/x"`, which is the ordinary un-nested case.
pub(super) fn join(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        return path.to_string();
    }
    if path == "/" {
        return prefix.to_string();
    }
    format!("{prefix}{path}")
}
