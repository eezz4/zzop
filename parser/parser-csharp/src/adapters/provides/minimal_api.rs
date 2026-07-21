//! Minimal-API route registrations — see the parent module doc (`mod.rs`) for scope. A FULL CST walk
//! (any receiver, any nesting depth reachable) finds every `invocation_expression` whose callee is
//! `.MapGet`/`.MapPost`/`.MapPut`/`.MapDelete`/`.MapPatch` with a LITERAL first (path) argument; a
//! chained `receiver.MapGroup("/api").MapGet("/x", ...)` composes `/api/x`
//! (`http_interface_key`'s slash-collapse normalization makes the join exact). Cross-statement group
//! variables (`var g = app.MapGroup("/api"); g.MapGet(...);`) are OUT of v1 scope — the group prefix is
//! only recognized when the `MapGroup(...)` call is the DIRECT receiver expression of the `MapX` call
//! itself (task brief: "roadmap; do not attempt").
//!
//! A bare-identifier receiver (`<name>.MapX("/x")`, no `MapGroup` chain) is emitted ONLY when the name
//! is the `app` WebApplication root (the near-universal `var app = builder.Build();` convention), whose
//! prefix is known to be empty. Any OTHER identifier (`api.MapGet(...)`, `group.MapGet(...)`) might be a
//! cross-statement `MapGroup` group variable carrying a prefix this v1 cannot see — emitting a bare
//! (prefix-less) key there would be a WRONG key, so it is SKIPPED (never-guess), not guessed. A non-`app`
//! root variable name is therefore under-extracted, not mis-keyed — roadmap.
//!
//! A non-literal path argument (a variable, string concatenation, an interpolated string with a
//! non-trivial hole, ...) is SKIPPED entirely — never guessed. The second (handler) argument's own
//! `symbol` is only recorded when it is a bare identifier (a named method-group reference,
//! `g.MapGet("/y", Handler)`) — a lambda handler has no name to report (`symbol: None`).

use tree_sitter::Node;
use zzop_core::{http_interface_key, IoProvide};

use crate::util::{line_of, node_text, string_literal_text, valid_named_children};

/// Minimal-API registration method name -> the HTTP verb it implies.
const MAP_METHODS: &[(&str, &str)] = &[
    ("MapGet", "GET"),
    ("MapPost", "POST"),
    ("MapPut", "PUT"),
    ("MapDelete", "DELETE"),
    ("MapPatch", "PATCH"),
];

pub(crate) fn extract(rel: &str, root: Node, src: &str, out: &mut Vec<IoProvide>) {
    walk(rel, root, src, out);
}

fn walk(rel: &str, node: Node, src: &str, out: &mut Vec<IoProvide>) {
    if node.kind() == "invocation_expression" {
        if let Some(provide) = match_map_call(rel, node, src) {
            out.push(provide);
        }
    }
    for child in valid_named_children(node) {
        walk(rel, child, src, out);
    }
}

fn match_map_call(rel: &str, call: Node, src: &str) -> Option<IoProvide> {
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
    let prefix = match group_prefix(receiver, src) {
        Some(p) => p, // direct `MapGroup("/p")` chain — prefix is statically known
        // Bare receiver: only the `app` root is known to carry no prefix. Any other identifier may be a
        // cross-statement `MapGroup` group variable (module doc), so a bare key would be WRONG — skip.
        None if is_app_root_receiver(receiver, src) => String::new(),
        None => return None,
    };
    let full_path = format!("{prefix}/{literal_path}");

    let handler = arguments.next().and_then(|a| {
        let expr = valid_named_children(a).into_iter().next()?;
        (expr.kind() == "identifier").then(|| node_text(expr, src).to_string())
    });

    Some(IoProvide {
        kind: "http".to_string(),
        key: http_interface_key(verb, &full_path),
        file: rel.to_string(),
        line: line_of(call),
        symbol: handler,
        body: None,
    })
}

/// The `app` WebApplication root receiver — the near-universal ASP.NET minimal-API convention
/// (`var app = builder.Build(); app.MapGet(...)`), the one bare-identifier receiver known to carry no
/// route-group prefix. Gating bare emission to this name turns an unseen group variable's prefix into
/// safe under-extraction rather than a wrong (prefix-less) key — module doc's never-guess rule.
fn is_app_root_receiver(receiver: Node, src: &str) -> bool {
    receiver.kind() == "identifier" && node_text(receiver, src) == "app"
}

/// `receiver.MapGroup("/prefix")`'s own literal prefix, when `receiver` is exactly that call shape —
/// module doc's "direct receiver only" v1 scope.
fn group_prefix(receiver: Node, src: &str) -> Option<String> {
    if receiver.kind() != "invocation_expression" {
        return None;
    }
    let func = receiver.child_by_field_name("function")?;
    if func.kind() != "member_access_expression" {
        return None;
    }
    let name_node = func.child_by_field_name("name")?;
    if node_text(name_node, src) != "MapGroup" {
        return None;
    }
    let args = receiver.child_by_field_name("arguments")?;
    let first = valid_named_children(args)
        .into_iter()
        .find(|a| a.kind() == "argument")?;
    let expr = valid_named_children(first).into_iter().next()?;
    string_literal_text(expr, src)
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
