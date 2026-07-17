//! `parse_calls` — Java `RawCall` extractor, matching `zzop_parser_typescript::lang::calls::parse_calls`'s
//! contract exactly (`crates/core/src/callgraph.rs`'s `RawCall` doc): each call site is attributed to its
//! enclosing method/constructor body (the smallest body span covering the call's line — the same
//! "innermost enclosing body wins" rule TS's `find_enclosing` uses), so this crate's output rides the
//! SAME whole-repo `SymbolGraph`/BFS the engine's call-graph-BFS native rules already build from
//! TypeScript (`crates/engine/src/analyze/native_rules/callgraph.rs`).
//!
//! ## Lambda bodies ARE covered — no special-casing needed
//! A lambda passed to `.map(...)`/`.filter(...)`/etc is not a symbol-bearing declaration in
//! `lang::symbols`'s scope (only types/methods/constructors get a `SourceSymbol`), so a call site INSIDE
//! a lambda body simply falls within its enclosing METHOD's own `body_start..=body_end` span — the exact
//! mechanism that already attributes a nested TS arrow-function call to its enclosing function without
//! any special-casing there. The full-tree walk below never stops at a lambda (or any other) boundary,
//! so a guard call written as `.map(x -> { AuthorizationService.canWriteX(...) })` is collected exactly
//! like a top-level statement in the method body would be.
//!
//! ## Receiver typing (`RawCall::receiver_type`)
//! A qualified call `recv.method(...)` records `recv`'s DECLARED type when `recv` is a tracked field/
//! local-variable/parameter name (`collect_var_types`, this module's own file-wide flat map — same
//! simplification `zzop_parser_typescript::calls::collect_class_var_types` makes, no per-scope shadowing
//! awareness), else `recv`'s own written text verbatim. The verbatim fallback is what makes a Java static
//! call resolve at all: `AuthorizationService.canWriteComment(...)` spells the class name directly at the
//! call site (unlike TS/JS, where a receiver is usually a variable needing type inference), so treating
//! an untracked identifier AS its own type name lets cross-file resolution
//! (`zzop_core::callgraph::resolve_calls_for_file`) match it against that class's import binding. A
//! receiver that resolves to neither an import nor a local symbol is simply dropped downstream (never
//! guessed) — same contract TS's resolver already enforces.
//!
//! A qualified call whose receiver is anything OTHER than a bare `identifier` (`this.x()`, chained
//! `a().b()`, `arr[i].x()`, `super.x()`) is out of v1 scope and not emitted at all — though the walk
//! still recurses into that receiver expression, so any call NESTED inside it is still collected on its
//! own.
//!
//! ## Out of v1 scope (documented, not attempted)
//! Method references (`Foo::bar`) and class-heritage (`extends`/`implements`) edges — TS's `parse_calls`
//! emits the latter as `is_heritage` `RawCall`s, but this crate's first cut is scoped to actual call
//! SITES (the auth-guard-reachability need this was built for), not type hierarchy. `new X()`
//! constructor calls are likewise not recorded as call edges.

use std::collections::HashMap;

use tree_sitter::{Node, TreeCursor};

use zzop_core::callgraph::RawCall;
use zzop_core::SourceSymbol;

use crate::util::{line_of, node_text, simple_type_name};

/// Extract this file's same-file call attributions — module doc. Empty on parse failure (never
/// panics); a partial in-file error skips just that subtree (`is_error`/`is_missing` nodes are never
/// visited).
pub fn parse_calls(rel: &str, text: &str) -> Vec<RawCall> {
    let Some(tree) = crate::parse_tree(text) else {
        return Vec::new();
    };
    let symbols = crate::lang::symbols::parse_symbols(rel, text);
    let bodies: Vec<&SourceSymbol> = symbols
        .iter()
        .filter(|s| s.body_start.is_some() && s.body_end.is_some())
        .collect();
    let var_types = collect_var_types(tree.root_node(), text);

    let mut out = Vec::new();
    let mut cursor = tree.walk();
    walk_calls(&mut cursor, text, &bodies, &var_types, &mut out);
    out
}

/// Innermost enclosing body-bearing symbol whose span covers `line`: the smallest `bodyEnd - bodyStart`
/// range wins when spans nest — same rule as `zzop_parser_typescript::calls::find_enclosing`.
fn find_enclosing<'a>(line: u32, bodies: &[&'a SourceSymbol]) -> Option<&'a SourceSymbol> {
    let mut best: Option<&SourceSymbol> = None;
    let mut best_range = u32::MAX;
    for s in bodies {
        let (Some(start), Some(end)) = (s.body_start, s.body_end) else {
            continue;
        };
        if line < start || line > end {
            continue;
        }
        let range = end - start;
        if range < best_range {
            best = Some(s);
            best_range = range;
        }
    }
    best
}

/// Full-tree walk collecting every `method_invocation` — descends unconditionally (unlike
/// `lang::used_names`'s scoped-identifier stop) since a call can be nested inside another call's
/// receiver/arguments and both must be collected independently.
fn walk_calls(
    cursor: &mut TreeCursor,
    src: &str,
    bodies: &[&SourceSymbol],
    var_types: &HashMap<String, String>,
    out: &mut Vec<RawCall>,
) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            if node.kind() == "method_invocation" {
                visit_call(node, src, bodies, var_types, out);
            }
            if cursor.goto_first_child() {
                walk_calls(cursor, src, bodies, var_types, out);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn visit_call(
    node: Node,
    src: &str,
    bodies: &[&SourceSymbol],
    var_types: &HashMap<String, String>,
    out: &mut Vec<RawCall>,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let callee_name = node_text(name_node, src).to_string();
    let receiver_type = match node.child_by_field_name("object") {
        None => None, // bare call — same-file/static-import resolution by name alone.
        Some(obj) if obj.kind() == "identifier" => {
            let recv = node_text(obj, src);
            Some(
                var_types
                    .get(recv)
                    .cloned()
                    .unwrap_or_else(|| recv.to_string()),
            )
        }
        // Qualified call on a non-identifier receiver (`this.x()`, chained, `super.x()`, ...) — out of
        // v1 scope, never guessed. The walk still recurses into `node`'s children via the caller, so a
        // call nested inside the receiver expression is still collected.
        Some(_) => return,
    };
    let line = line_of(node);
    let Some(enclosing) = find_enclosing(line, bodies) else {
        return; // call sits outside any tracked method/constructor body — dropped, same as TS.
    };
    out.push(RawCall {
        from_symbol: enclosing.id.clone(),
        callee_name,
        line,
        receiver_type,
        is_heritage: false,
    });
}

/// File-wide `varName -> DeclaredType` map from every `field_declaration`/`local_variable_declaration`
/// (via their shared `declarator`/`variable_declarator` shape) and `formal_parameter` — module doc's
/// "Receiver typing". Flat and file-wide, no per-scope shadowing awareness, same simplification
/// `zzop_parser_typescript::calls::collect_class_var_types` makes.
fn collect_var_types(root: Node, src: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut cursor = root.walk();
    walk_var_types(&mut cursor, src, &mut map);
    map
}

fn walk_var_types(cursor: &mut TreeCursor, src: &str, map: &mut HashMap<String, String>) {
    loop {
        let node = cursor.node();
        if !node.is_error() && !node.is_missing() {
            match node.kind() {
                "field_declaration" | "local_variable_declaration" => {
                    collect_declarators(node, src, map);
                }
                "formal_parameter" => {
                    collect_formal_parameter(node, src, map);
                }
                _ => {}
            }
            if cursor.goto_first_child() {
                walk_var_types(cursor, src, map);
                cursor.goto_parent();
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn collect_declarators(node: Node, src: &str, map: &mut HashMap<String, String>) {
    let Some(type_node) = node.child_by_field_name("type") else {
        return;
    };
    let Some(type_name) = simple_type_name(type_node, src) else {
        return;
    };
    let mut cursor = node.walk();
    for declarator in node.children_by_field_name("declarator", &mut cursor) {
        if declarator.is_error() || declarator.is_missing() {
            continue;
        }
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue; // an underscore-pattern declarator — never guessed.
        }
        map.insert(node_text(name_node, src).to_string(), type_name.clone());
    }
}

fn collect_formal_parameter(node: Node, src: &str, map: &mut HashMap<String, String>) {
    let Some(type_node) = node.child_by_field_name("type") else {
        return;
    };
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    if name_node.kind() != "identifier" {
        return;
    }
    let Some(type_name) = simple_type_name(type_node, src) else {
        return;
    };
    map.insert(node_text(name_node, src).to_string(), type_name);
}

#[cfg(test)]
mod tests;
