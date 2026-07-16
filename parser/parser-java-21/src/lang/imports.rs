//! Import extraction -> `zzop_core::ImportMap`. v1 scope: every top-level `import_declaration` — Java's
//! grammar makes this inherently exhaustive the same way Go's does (`import` can only appear at file
//! scope, immediately after the `package` clause; no nested/function-local import).
//!
//! ## Grammar shape: `import_declaration` carries NO fields at all
//! `import_declaration: seq('import', optional('static'), $._name, optional(seq('.', $.asterisk)), ';')`
//! — unlike most other declarations in this grammar, none of `'import'`/`'static'`/`$._name`/the
//! trailing glob are wrapped in a `field(...)`. The imported name is found by kind among the
//! declaration's own named children (`identifier` for a single-segment name, `scoped_identifier` for a
//! dotted one) instead of `child_by_field_name`.
//!
//! ## Binding-name convention (task-pinned)
//! - **Plain** (`import a.b.C;`): local name = `C`, the dotted name's own rightmost segment
//!   (`scoped_identifier`'s `name` field, or the whole text for a single-segment `identifier` name) —
//!   Java has no `as`-aliasing, so this is always exact, unlike Go's last-`/`-segment approximation.
//!   `specifier` = the full dotted text (`a.b.C`); `original` = the same rightmost segment as `local`
//!   (Java's own binding name IS the imported symbol's real name — no separate "original" to track).
//! - **Static** (`import static a.b.C.m;`): IDENTICAL extraction (local = rightmost segment `m`,
//!   specifier = `a.b.C.m`) — the grammar's `optional('static')` token changes nothing about the
//!   `_name`/glob shape this module reads, so no `static`-specific branch exists. `ImportBinding` has no
//!   field for "this was a static import" (documented approximation, matches the task brief's "flag
//!   nothing special").
//! - **Glob** (`import a.b.*;` or `import static a.b.C.*;`): binds no single local name — mirrors
//!   `zzop_parser_go::lang::imports`'/`zzop_parser_rust`'s/`zzop_parser_python_3`'s identical synthetic,
//!   collision-free map key (`__glob_import_N__`) so the edge still enters the map instead of being
//!   silently dropped. `specifier` = the pre-glob dotted prefix with `.*` appended (`a.b.*`); `original`
//!   = `"*"`, reusing `ImportBinding::original`'s documented "namespace = `*`" convention.
//!
//! `deferred`/`type_only` are always `false` — Java has neither concept.

use tree_sitter::Node;
use zzop_core::{ImportBinding, ImportMap};

use crate::util::{node_text, valid_named_children};

/// Extract this file's import bindings — see module doc. Empty on parse failure (never panics).
pub fn parse_imports(text: &str) -> ImportMap {
    let mut map = ImportMap::new();
    let Some(tree) = crate::parse_tree(text) else {
        return map;
    };
    let mut glob_seq: u32 = 0;
    for child in valid_named_children(tree.root_node()) {
        if child.kind() == "import_declaration" {
            emit_import(child, text, &mut map, &mut glob_seq);
        }
    }
    map
}

fn emit_import(node: Node, src: &str, map: &mut ImportMap, glob_seq: &mut u32) {
    let children = valid_named_children(node);
    let Some(name_node) = children
        .iter()
        .find(|c| matches!(c.kind(), "identifier" | "scoped_identifier"))
    else {
        return; // no name at all — a bare `import *;` is not valid Java, never guessed.
    };
    let dotted = node_text(*name_node, src).to_string();
    if dotted.is_empty() {
        return;
    }
    let is_glob = children.iter().any(|c| c.kind() == "asterisk");

    if is_glob {
        let specifier = format!("{dotted}.*");
        insert(map, glob_key(glob_seq), specifier, "*".to_string());
        return;
    }

    let local = match name_node.kind() {
        "scoped_identifier" => match name_node.child_by_field_name("name") {
            Some(last) => node_text(last, src).to_string(),
            None => return,
        },
        _ => dotted.clone(),
    };
    let original = local.clone();
    insert(map, local, dotted, original);
}

fn insert(map: &mut ImportMap, local: String, specifier: String, original: String) {
    map.insert(
        local,
        ImportBinding {
            specifier,
            original,
            deferred: false,
            type_only: false,
        },
    );
}

fn glob_key(seq: &mut u32) -> String {
    let key = format!("__glob_import_{}__", *seq);
    *seq += 1;
    key
}

#[cfg(test)]
mod tests;
