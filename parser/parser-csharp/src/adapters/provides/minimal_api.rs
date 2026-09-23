//! Minimal-API route registrations — see the parent module doc (`mod.rs`) for scope. A FULL CST walk
//! (any receiver, any nesting depth reachable) finds every `invocation_expression` whose callee is
//! `.MapGet`/`.MapPost`/`.MapPut`/`.MapDelete`/`.MapPatch` with a LITERAL first (path) argument; a
//! receiver chain composes its `MapGroup` prefixes (`http_interface_key`'s slash-collapse
//! normalization makes the join exact). WHICH receivers carry a known prefix — a direct chain, a chain
//! with builder calls after the group, and a cross-statement group VARIABLE — is [`super::route_groups`]'s
//! question, and that module's doc carries the measurement that set its scope.
//!
//! A receiver whose prefix that module cannot resolve is SKIPPED (never-guess), not keyed prefix-less:
//! a bare identifier that is neither a DECLARED root route builder
//! (`vocabulary.csharpRootRouteBuilderVariableNames`) nor a group variable declared in this file might
//! carry any prefix, and a wrong key is worse than a missing one. Such a name is therefore
//! under-extracted, not mis-keyed — and WHICH names the root goes by is the project's to declare, so an
//! undeclared vocabulary under-extracts every root registration rather than guessing at `app`.
//!
//! A non-literal path argument (a variable, string concatenation, an interpolated string with a
//! non-trivial hole, ...) is SKIPPED entirely — never guessed. The second (handler) argument's own
//! `symbol` is only recorded when it is a bare identifier (a named method-group reference,
//! `g.MapGet("/y", Handler)`) — a lambda handler has no name to report (`symbol: None`).

use tree_sitter::Node;
use zzop_core::{http_interface_key, IoProvide};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

use super::route_groups::{self, CSharpRouteVocab, GroupMap};

/// Minimal-API registration method name -> the HTTP verb it implies.
const MAP_METHODS: &[(&str, &str)] = &[
    ("MapGet", "GET"),
    ("MapPost", "POST"),
    ("MapPut", "PUT"),
    ("MapDelete", "DELETE"),
    ("MapPatch", "PATCH"),
];

pub(crate) fn extract(
    rel: &str,
    root: Node,
    src: &str,
    vocab: &CSharpRouteVocab<'_>,
    out: &mut Vec<IoProvide>,
) {
    // ONE group-variable pass per file, before the registration walk: a `var api = app.MapGroup(...)`
    // may be declared after a sibling group that refers to it, so the map is resolved whole rather than
    // accumulated in walk order.
    let groups = route_groups::collect_group_variables(root, src, vocab);
    walk(rel, root, src, &groups, vocab, out);
}

fn walk(
    rel: &str,
    node: Node,
    src: &str,
    groups: &GroupMap,
    vocab: &CSharpRouteVocab<'_>,
    out: &mut Vec<IoProvide>,
) {
    if node.kind() == "invocation_expression" {
        if let Some(provide) = match_map_call(rel, node, src, groups, vocab) {
            out.push(provide);
        }
    }
    for child in valid_named_children(node) {
        walk(rel, child, src, groups, vocab, out);
    }
}

fn match_map_call(
    rel: &str,
    call: Node,
    src: &str,
    groups: &GroupMap,
    vocab: &CSharpRouteVocab<'_>,
) -> Option<IoProvide> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let name_node = func.child_by_field_name("name")?;
    let method_name = node_text(name_node, src);
    let (_, verb) = MAP_METHODS.iter().find(|(m, _)| *m == method_name)?;

    let args = call.child_by_field_name("arguments")?;
    let mut arguments = valid_named_children(args)
        .into_iter()
        .filter(|a| a.kind() == "argument");
    let path_arg = arguments.next()?;
    let path_expr = valid_named_children(path_arg).into_iter().next()?;
    let literal_path = string_literal_text(path_expr, src)?; // non-literal -> None -> skip, never guess

    let receiver = func.child_by_field_name("expression")?;
    // `None` means this build cannot know the receiver's prefix — skip rather than key it bare.
    let prefix = route_groups::receiver_prefix(receiver, src, groups, vocab)?;
    let full_path = format!("{prefix}/{literal_path}");

    let handler = arguments.next().and_then(|a| {
        let expr = valid_named_children(a).into_iter().next()?;
        (expr.kind() == "identifier").then(|| node_text(expr, src).to_string())
    });

    Some(IoProvide {
        route_version: None,
        response: None,
        kind: "http".to_string(),
        key: http_interface_key(verb, &full_path),
        file: rel.to_string(),
        line: line_of(call),
        symbol: handler,
        body: None,
    })
}

#[cfg(test)]
mod tests {
    /// Parity pin with every other parser's verb vocabulary: every verb `MAP_METHODS` emits must be a
    /// `zzop_core::HTTP_KEY_VERBS` join member. The minimal-API surface has no `MapHead`, so — unlike the
    /// attribute-provides `METHOD_ATTRIBUTES` — there is no HEAD carve-out to allow.
    #[test]
    fn map_methods_emit_only_core_key_verbs() {
        for (_, verb) in super::MAP_METHODS {
            assert!(
                zzop_core::HTTP_KEY_VERBS.contains(verb),
                "MAP_METHODS emits {verb}, which is not a core HTTP_KEY_VERBS member"
            );
        }
    }
}
